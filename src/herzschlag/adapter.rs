// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) The Orchestration helpers authors (coraxloop/superloop-tools)
// Geändert für dieses Repo; was und wie: src/herzschlag/HERKUNFT.toml.

//! Die Seite zum Hook: liest die Eingabe eines Stops (JSON auf stdin), das
//! Transkript (JSON Lines) und gibt das Urteil in der Form des Hooks zurück:
//! Exit 0 ohne Ausgabe (durch), Exit 2 mit dem Grund auf stderr (aufhalten),
//! Exit 0 mit `{"systemMessage": …}` auf stdout (Lapse). Jede kaputte Eingabe
//! und jeder Fehler endet mit Exit 0.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use super::graph::{self, Arbeit, Entry, FailureClass, Observations, State, StopFields, StopInput, Timing};

/// So beginnt die Nachricht, mit der der Grund eines Blocks im Transkript steht;
/// sie zählt nicht als echter Nutzer-Zug.
const FEEDBACK_PREFIX: &str = "Stop hook feedback:";

/// Höchstens so viele Bytes Eingabe.
pub const EINGABE_HOECHSTENS: usize = 1_048_576;

fn block_reason(offen: &str) -> String {
    format!(
        "HERZSCHLAG: Diese Sitzung hat im Dashboard noch offene Arbeit: {offen}. Arbeite daran weiter. Ist der Schritt fertig, setze ihn auf fertig; \
         wartet er auf etwas, trage stand wartet mit wartet_auf ein; gibst du ihn ab, biete ihn an oder gib ihn frei. Danach endet der Zug."
    )
}

fn lapse_message(offen: &str) -> String {
    format!(
        "HERZSCHLAG (Ausnahme): Der Stop wurde in diesem Zug {} Mal aufgehalten, ohne dass die offene Arbeit eingetragen wurde ({offen}). \
         Er geht jetzt durch; die Arbeit bleibt im Dashboard offen.",
        graph::MAX_BLOCKS_PER_TURN
    )
}

/// Ergebnis einer Auswertung, samt Weg durch den Graphen.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HookResult {
    /// 0 durch, 2 aufhalten.
    pub status: u8,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub state: State,
    pub path: Vec<State>,
    pub retry_count: u8,
}

impl HookResult {
    /// Durch, ohne Ausgabe.
    #[must_use]
    pub fn silent(state: State, path: Vec<State>, retry_count: u8) -> Self {
        Self { status: 0, stdout: Vec::new(), stderr: Vec::new(), state, path, retry_count }
    }
}

/// Transkript aus der Datei, Arbeit aus `abfrage`.
struct HookObservations<'a> {
    abfrage: &'a dyn Fn(&str) -> Result<Arbeit, FailureClass>,
}

impl Observations for HookObservations<'_> {
    fn load(&self, path: &Path) -> Result<Vec<Entry>, FailureClass> {
        Ok(load_entries(path)?.iter().map(|entry| Entry { real_user: is_real_user_turn(entry) }).collect())
    }

    fn arbeit(&self, session: &str) -> Result<Arbeit, FailureClass> {
        (self.abfrage)(session)
    }
}

/// Ein Stop von Anfang bis Ende, ohne Prozess: Eingabe prüfen, Graph,
/// Ausgabe. `state_dir` fehlt, wenn der Ort der Zustandsdateien unbekannt ist
/// (dann durch). Eine Panik im Graphen endet ebenfalls mit „durch“.
#[must_use]
pub fn execute(input: &[u8], state_dir: Option<&Path>, timing: Timing, abfrage: &dyn Fn(&str) -> Result<Arbeit, FailureClass>) -> HookResult {
    let leer = || HookResult::silent(State::AllowSilent, Vec::new(), 0);
    let Some(state_dir) = state_dir else { return leer() };
    let event = serde_json::from_slice::<Value>(input).ok().and_then(|value| value.get("hook_event_name").and_then(Value::as_str).map(str::to_owned));
    if input.len() > EINGABE_HOECHSTENS || event.as_deref() != Some("Stop") {
        // Anders als bei Upstream nie eine Ablehnung: bei Stop hieße Exit 2 aufhalten.
        graph::record_payload_failure(state_dir);
        return leer();
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(input, state_dir, timing, abfrage)));
    result.unwrap_or_else(|_| {
        graph::record_panic(state_dir);
        leer()
    })
}

/// Der Graph mit dekodierter Eingabe, ohne Prüfung des Ereignisses.
#[must_use]
pub fn run(input: &[u8], state_dir: &Path, timing: Timing, abfrage: &dyn Fn(&str) -> Result<Arbeit, FailureClass>) -> HookResult {
    let result = graph::run(&decode(input), state_dir, timing, &HookObservations { abfrage });
    terminal_result(result.state, result.path, result.retry_count, &result.offen)
}

fn decode(input: &[u8]) -> StopInput {
    let payload = serde_json::from_slice::<Value>(input).ok().and_then(|value| value.as_object().cloned());
    let accepted = payload.as_ref().is_some_and(|object| match object.get("hook_event_name") {
        None => true,
        Some(Value::String(event)) => event == "Stop",
        Some(_) => false,
    });
    let fields = (|| {
        let payload = payload.as_ref().ok_or(FailureClass::Payload)?;
        let transcript_path = match payload.get("transcript_path") {
            None | Some(Value::Null) => PathBuf::new(),
            Some(Value::String(value)) => PathBuf::from(value),
            Some(_) => return Err(FailureClass::Payload),
        };
        let session = match payload.get("session_id") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) if value.is_empty() => None,
            Some(Value::String(value)) => Some(value.clone()),
            Some(_) => return Err(FailureClass::Payload),
        };
        let retry = match payload.get("stop_hook_active") {
            None | Some(Value::Null | Value::Bool(false)) => false,
            Some(Value::Bool(true)) => true,
            Some(_) => return Err(FailureClass::Payload),
        };
        Ok(StopFields { transcript_path, session, retry })
    })();
    StopInput { accepted, fields }
}

fn terminal_result(state: State, path: Vec<State>, retry_count: u8, offen: &str) -> HookResult {
    match state {
        State::BlockPrompt => HookResult { status: 2, stdout: Vec::new(), stderr: block_reason(offen).into_bytes(), state, path, retry_count },
        State::AllowLapseAlarm => {
            let stdout = serde_json::to_vec(&json!({"systemMessage": lapse_message(offen)})).unwrap_or_default();
            HookResult { status: 0, stdout, stderr: Vec::new(), state, path, retry_count }
        }
        _ => HookResult::silent(State::AllowSilent, path, retry_count),
    }
}

fn load_entries(path: &Path) -> Result<Vec<Map<String, Value>>, FailureClass> {
    let source = fs::read_to_string(path).map_err(|_| FailureClass::TranscriptRead)?;
    let mut entries = Vec::new();
    for line in source.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if let Ok(Value::Object(entry)) = serde_json::from_str::<Value>(line) {
            entries.push(entry);
        }
    }
    Ok(entries)
}

fn is_real_user_turn(entry: &Map<String, Value>) -> Result<bool, FailureClass> {
    if entry.get("type").and_then(Value::as_str) != Some("user") || json_truthy(entry.get("isSidechain")) {
        return Ok(false);
    }
    if is_guard_feedback(entry)? {
        return Ok(false);
    }
    let Some(message) = message_object(entry)? else {
        return Ok(false);
    };
    match message.get("content") {
        Some(Value::Array(content)) => {
            Ok(!content.iter().any(|block| block.as_object().is_some_and(|object| object.get("type").and_then(Value::as_str) == Some("tool_result"))))
        }
        Some(Value::String(_)) => Ok(true),
        _ => Ok(false),
    }
}

fn is_guard_feedback(entry: &Map<String, Value>) -> Result<bool, FailureClass> {
    let Some(message) = message_object(entry)? else {
        return Ok(false);
    };
    Ok(match message.get("content") {
        Some(Value::String(content)) => content.starts_with(FEEDBACK_PREFIX),
        Some(Value::Array(content)) => content.iter().any(|block| {
            block.as_object().is_some_and(|object| {
                object.get("type").and_then(Value::as_str) == Some("text")
                    && object.get("text").and_then(Value::as_str).is_some_and(|text| text.starts_with(FEEDBACK_PREFIX))
            })
        }),
        _ => false,
    })
}

fn message_object(entry: &Map<String, Value>) -> Result<Option<&Map<String, Value>>, FailureClass> {
    match entry.get("message") {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Object(message)) => Ok(Some(message)),
        Some(value) if !json_truthy(Some(value)) => Ok(None),
        Some(_) => Err(FailureClass::TranscriptStructure),
    }
}

fn json_truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(value)) => *value,
        Some(Value::Number(value)) => value.as_f64().is_none_or(|number| number != 0.0),
        Some(Value::String(value)) => !value.is_empty(),
        Some(Value::Array(value)) => !value.is_empty(),
        Some(Value::Object(value)) => !value.is_empty(),
    }
}
