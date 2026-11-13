// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) The Orchestration helpers authors (coraxloop/superloop-tools)
// Geändert für dieses Repo; was und wie: src/herzschlag/HERKUNFT.toml.

//! Der Zustandsgraph des Herzschlags: entscheidet bei jedem Stop eines Chats,
//! ob er durchgeht, aufgehalten wird (höchstens dreimal je echtem Nutzer-Zug)
//! oder mit Warnung durchgeht (Lapse). Jeder Fehler endet in `FailOpen` und
//! lässt den Stop durch.
//!
//! Der Graph ist eine Liste von Kanten mit je einer Bedingung (`Guard`) und
//! geordneten Wirkungen (`Effect`); von jedem Zustand passt genau eine Kante,
//! sonst gilt das als Fehler im Graphen und endet ebenfalls in `FailOpen`.

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

use serde_json::{Value, json};

/// Höchstens so viele Blocks je echtem Nutzer-Zug, dann Lapse.
pub const MAX_BLOCKS_PER_TURN: i64 = 3;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// Ein Entscheidungs- oder Endzustand.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum State {
    Entry,
    /// Fragt das Dashboard, ob die Sitzung Arbeit hat.
    Tracker,
    /// Unbekannte Sitzung: nichts zu prüfen.
    Off,
    Transcript,
    RetryOnce,
    Resolve,
    /// Keine offene Arbeit.
    Idle,
    Prompt,
    ExceptionLapse,
    FailOpen,
    AllowSilent,
    BlockPrompt,
    AllowLapseAlarm,
}

impl State {
    /// Alle Zustände, für die Prüfung des Graphen.
    pub const ALL: [Self; 13] = [
        Self::Entry,
        Self::Tracker,
        Self::Off,
        Self::Transcript,
        Self::RetryOnce,
        Self::Resolve,
        Self::Idle,
        Self::Prompt,
        Self::ExceptionLapse,
        Self::FailOpen,
        Self::AllowSilent,
        Self::BlockPrompt,
        Self::AllowLapseAlarm,
    ];

    /// Die Endzustände (Urteile).
    pub const VERDICTS: [Self; 3] = [Self::AllowSilent, Self::BlockPrompt, Self::AllowLapseAlarm];

    #[must_use]
    pub fn is_terminal(self) -> bool {
        Self::VERDICTS.contains(&self)
    }
}

/// Bedingung einer Kante.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Guard {
    Malformed,
    Accepted,
    InternalError,
    SessionUnknown,
    SessionKnown,
    RetryNeeded,
    TranscriptReady,
    Idle,
    Prompt,
    Lapse,
    Always,
}

/// Wirkung einer Kante, in Reihenfolge ausgeführt.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Effect {
    /// 1 s warten, dann Dashboard und Transkript neu lesen.
    WaitAndReload,
    RecordCount,
    AppendInternalLog,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transition {
    pub guard: Guard,
    pub effects: &'static [Effect],
}

/// Gerichteter Graph als Kantenliste (von, nach, Übergang).
pub struct Graph {
    pub edges: Vec<(State, State, Transition)>,
}

impl Graph {
    /// Die Kanten, die von `state` ausgehen.
    pub fn edges_from(&self, state: State) -> impl Iterator<Item = &(State, State, Transition)> {
        self.edges.iter().filter(move |(from, _, _)| *from == state)
    }
}

/// Der Graph des Herzschlags.
#[must_use]
pub fn heartbeat_graph() -> Graph {
    let mut edges = Vec::new();
    let mut add = |from, to, guard, effects| edges.push((from, to, Transition { guard, effects }));
    add(State::Entry, State::FailOpen, Guard::Malformed, &[]);
    add(State::Entry, State::Tracker, Guard::Accepted, &[]);
    add(State::Tracker, State::FailOpen, Guard::InternalError, &[]);
    add(State::Tracker, State::Off, Guard::SessionUnknown, &[]);
    add(State::Tracker, State::Transcript, Guard::SessionKnown, &[]);
    add(State::Off, State::AllowSilent, Guard::Always, &[]);
    add(State::Transcript, State::FailOpen, Guard::InternalError, &[]);
    add(State::Transcript, State::RetryOnce, Guard::RetryNeeded, &[]);
    add(State::Transcript, State::Resolve, Guard::TranscriptReady, &[]);
    add(State::RetryOnce, State::FailOpen, Guard::InternalError, &[]);
    add(State::RetryOnce, State::Transcript, Guard::Always, &[Effect::WaitAndReload]);
    add(State::Resolve, State::FailOpen, Guard::InternalError, &[]);
    add(State::Resolve, State::Idle, Guard::Idle, &[]);
    add(State::Resolve, State::Prompt, Guard::Prompt, &[]);
    add(State::Resolve, State::ExceptionLapse, Guard::Lapse, &[]);
    add(State::Idle, State::AllowSilent, Guard::Always, &[]);
    add(State::Prompt, State::FailOpen, Guard::InternalError, &[]);
    add(State::Prompt, State::BlockPrompt, Guard::Always, &[Effect::RecordCount]);
    add(State::ExceptionLapse, State::AllowLapseAlarm, Guard::Always, &[]);
    add(State::FailOpen, State::AllowSilent, Guard::InternalError, &[Effect::AppendInternalLog]);
    add(State::FailOpen, State::AllowSilent, Guard::Always, &[]);
    Graph { edges }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureClass {
    Graph,
    Payload,
    StateRead,
    StateWrite,
    TranscriptRead,
    TranscriptStructure,
    /// Das Dashboard antwortete nicht brauchbar (keine Verbindung, 401, Frist, Unsinn).
    Tracker,
    Panic,
}

impl FailureClass {
    const fn name(self) -> &'static str {
        match self {
            Self::Graph => "Graph",
            Self::Payload => "Payload",
            Self::StateRead => "StateRead",
            Self::StateWrite => "StateWrite",
            Self::TranscriptRead => "TranscriptRead",
            Self::TranscriptStructure => "TranscriptStructure",
            Self::Tracker => "Tracker",
            Self::Panic => "Panic",
        }
    }
}

/// Antwort des Dashboards zu einer Sitzung.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Arbeit {
    /// Keinem Chat zugeordnet: durchlassen.
    Unbekannt,
    /// Zugeordnet, aber nichts offen.
    Frei,
    /// Offene Schritte, als ein Satz für den Grund des Blocks.
    Offen(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Resolution {
    Unknown,
    Idle,
    Prompt,
    Lapse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RetryPhase {
    Inactive,
    ActiveFresh,
    ActiveRetried,
}

/// Uhr des Nachlesens: im Betrieb 1 s schlafen, in Tests nicht.
pub struct Timing {
    system: bool,
    replacement_transcript: Option<Vec<u8>>,
}

impl Timing {
    #[must_use]
    pub fn system() -> Self {
        Self { system: true, replacement_transcript: None }
    }

    #[must_use]
    pub fn fixed() -> Self {
        Self { system: false, replacement_transcript: None }
    }

    /// Wie `fixed`, ersetzt beim Nachlesen aber das Transkript.
    #[must_use]
    pub fn fixed_with_reload(replacement: &[u8]) -> Self {
        Self { system: false, replacement_transcript: Some(replacement.to_vec()) }
    }

    fn wait(&mut self, transcript_path: &Path) -> Result<(), FailureClass> {
        if self.system {
            thread::sleep(Duration::from_secs(1));
        } else if let Some(replacement) = self.replacement_transcript.take() {
            fs::write(transcript_path, replacement).map_err(|_| FailureClass::TranscriptRead)?;
        }
        Ok(())
    }
}

/// Die geprüfte Eingabe eines Stops; ein Fehler in den Feldern bleibt bis zur Nutzung liegen.
pub struct StopInput {
    pub accepted: bool,
    pub fields: Result<StopFields, FailureClass>,
}

pub struct StopFields {
    pub transcript_path: PathBuf,
    pub session: Option<String>,
    pub retry: bool,
}

/// Eine Zeile des Transkripts, soweit der Graph sie braucht.
pub struct Entry {
    pub real_user: Result<bool, FailureClass>,
}

/// Die Grenze zur Umgebung: Transkript und Dashboard.
pub trait Observations {
    /// # Errors
    /// Die Fehlerklasse, wenn sich das Transkript nicht lesen lässt.
    fn load(&self, path: &Path) -> Result<Vec<Entry>, FailureClass>;
    /// # Errors
    /// `FailureClass::Tracker`, wenn das Dashboard nicht brauchbar antwortet.
    fn arbeit(&self, session: &str) -> Result<Arbeit, FailureClass>;
}

/// Ergebnis des Graphen vor der Ausgabe.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Evaluation {
    pub state: State,
    pub path: Vec<State>,
    pub retry_count: u8,
    /// Die offenen Schritte als Satz (leer ohne offene Arbeit).
    pub offen: String,
}

struct Context<'a> {
    state_dir: &'a Path,
    input: &'a StopInput,
    observations: &'a dyn Observations,
    transcript_path: PathBuf,
    session: Option<String>,
    retry_phase: RetryPhase,
    arbeit: Option<Arbeit>,
    entries: Vec<Entry>,
    idle: bool,
    resolution: Resolution,
    marker: String,
    count: i64,
    retry_count: u8,
    failure: Option<FailureClass>,
    timing: Timing,
}

impl<'a> Context<'a> {
    fn new(input: &'a StopInput, state_dir: &'a Path, timing: Timing, observations: &'a dyn Observations) -> Self {
        Self {
            state_dir,
            input,
            observations,
            transcript_path: PathBuf::new(),
            session: None,
            retry_phase: RetryPhase::Inactive,
            arbeit: None,
            entries: Vec::new(),
            idle: true,
            resolution: Resolution::Unknown,
            marker: String::new(),
            count: 0,
            retry_count: 0,
            failure: None,
            timing,
        }
    }

    /// Schlüssel der Sitzung für Dateinamen (nur nach `Tracker` mit bekannter Sitzung gebraucht).
    fn session_key(&self) -> String {
        schluessel(self.session.as_deref().unwrap_or_default())
    }

    fn prepare(&mut self, state: State) -> Result<(), FailureClass> {
        match state {
            State::Tracker if self.arbeit.is_none() => self.prepare_tracker(),
            State::Transcript => self.prepare_transcript(),
            State::Resolve if self.resolution == Resolution::Unknown => self.prepare_resolution(),
            _ => Ok(()),
        }
    }

    fn prepare_tracker(&mut self) -> Result<(), FailureClass> {
        let fields = self.input.fields.as_ref().map_err(|failure| *failure)?;
        self.transcript_path.clone_from(&fields.transcript_path);
        self.session.clone_from(&fields.session);
        self.retry_phase = if fields.retry { RetryPhase::ActiveFresh } else { RetryPhase::Inactive };
        self.arbeit = Some(match &self.session {
            None => Arbeit::Unbekannt,
            Some(session) => self.observations.arbeit(session)?,
        });
        Ok(())
    }

    fn prepare_transcript(&mut self) -> Result<(), FailureClass> {
        self.entries = self.observations.load(&self.transcript_path)?;
        self.idle = !matches!(self.arbeit, Some(Arbeit::Offen(_)));
        Ok(())
    }

    fn prepare_resolution(&mut self) -> Result<(), FailureClass> {
        let index = last_user_turn_index(&self.entries)?;
        self.marker = index.map_or_else(|| String::from("None"), |value| value.to_string());
        self.count = read_count(self.state_dir, &self.session_key(), &self.marker, self.retry_phase != RetryPhase::Inactive)?;
        self.resolution = match (self.idle, self.count >= MAX_BLOCKS_PER_TURN) {
            (true, _) => Resolution::Idle,
            (false, false) => Resolution::Prompt,
            (false, true) => Resolution::Lapse,
        };
        Ok(())
    }

    fn guard_matches(&self, guard: Guard) -> bool {
        let ok = self.failure.is_none();
        match guard {
            Guard::Malformed => !self.input.accepted,
            Guard::Accepted => self.input.accepted,
            Guard::InternalError => !ok,
            Guard::SessionUnknown => ok && self.arbeit == Some(Arbeit::Unbekannt),
            Guard::SessionKnown => ok && self.arbeit.as_ref().is_some_and(|a| *a != Arbeit::Unbekannt),
            Guard::RetryNeeded => ok && !self.idle && self.retry_phase == RetryPhase::ActiveFresh,
            Guard::TranscriptReady => ok && (self.idle || self.retry_phase != RetryPhase::ActiveFresh),
            Guard::Idle => ok && self.resolution == Resolution::Idle,
            Guard::Prompt => ok && self.resolution == Resolution::Prompt,
            Guard::Lapse => ok && self.resolution == Resolution::Lapse,
            Guard::Always => ok,
        }
    }

    fn apply(&mut self, effect: Effect) -> Result<(), FailureClass> {
        match effect {
            Effect::WaitAndReload => {
                self.timing.wait(&self.transcript_path)?;
                self.retry_phase = RetryPhase::ActiveRetried;
                self.retry_count = self.retry_count.saturating_add(1);
                self.entries.clear();
                if let Some(session) = self.session.clone() {
                    // Eine nun unbekannte Sitzung hat keine offene Arbeit mehr.
                    self.arbeit = Some(self.observations.arbeit(&session)?);
                }
                Ok(())
            }
            Effect::RecordCount => {
                // shortcut: Lesen und Schreiben ohne Sperre, gleichzeitige Stops derselben Sitzung blocken öfter als 3-mal; Sperrdatei (create_new, sonst durchlassen), wenn das vorkommt.
                atomic_write(&self.state_dir.join(format!("guard-count-{}", self.session_key())), format!("{} {}", self.marker, self.count + 1).as_bytes())
            }
            Effect::AppendInternalLog => {
                if let Some(failure) = self.failure {
                    append_log(self.state_dir, failure, self.session.as_deref());
                }
                Ok(())
            }
        }
    }
}

/// Wertet einen Stop mit dem Graphen aus.
#[must_use]
pub fn run(input: &StopInput, state_dir: &Path, timing: Timing, observations: &dyn Observations) -> Evaluation {
    let graph = heartbeat_graph();
    let mut current = State::Entry;
    let mut context = Context::new(input, state_dir, timing, observations);
    let mut path = vec![State::Entry];
    loop {
        if current.is_terminal() {
            let offen = match &context.arbeit {
                Some(Arbeit::Offen(text)) => text.clone(),
                _ => String::new(),
            };
            return Evaluation { state: current, path, retry_count: context.retry_count, offen };
        }
        if let Err(failure) = context.prepare(current) {
            context.failure = Some(failure);
        }
        let Some((_, destination, transition)) = selected_edge(&graph, current, &context) else {
            context.failure = Some(FailureClass::Graph);
            current = State::FailOpen;
            path.push(current);
            continue;
        };
        let (destination, transition) = (*destination, transition.clone());
        let mut effect_failed = false;
        for effect in transition.effects {
            if let Err(failure) = context.apply(*effect) {
                context.failure = Some(failure);
                effect_failed = true;
                break;
            }
        }
        if effect_failed {
            current = graph.edges_from(current).find(|(_, _, t)| t.guard == Guard::InternalError).map_or_else(
                || {
                    context.failure = Some(FailureClass::Graph);
                    State::FailOpen
                },
                |(_, to, _)| *to,
            );
            path.push(current);
            continue;
        }
        current = destination;
        path.push(current);
    }
}

/// Genau eine passende Kante, sonst keine.
fn selected_edge<'g>(graph: &'g Graph, current: State, context: &Context<'_>) -> Option<&'g (State, State, Transition)> {
    let mut matches = graph.edges_from(current).filter(|(_, _, t)| context.guard_matches(t.guard));
    let selected = matches.next()?;
    if matches.next().is_some() { None } else { Some(selected) }
}

fn last_user_turn_index(entries: &[Entry]) -> Result<Option<usize>, FailureClass> {
    let mut index = None;
    for (candidate, entry) in entries.iter().enumerate() {
        if entry.real_user? {
            index = Some(candidate);
        }
    }
    Ok(index)
}

/// Die Sitzungskennung als Teil eines Dateinamens: alles außer ASCII-Buchstaben,
/// Ziffern, `-` und `_` wird `-` (auch `/`, `\` und `:` eines Laufwerkspfads).
#[must_use]
pub fn schluessel(session: &str) -> String {
    session.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '-' }).collect()
}

/// `active` (stop_hook_active): Dieser Stop folgt auf einen Block, ein echter
/// Nutzer-Zug liegt dazwischen also nicht. Ein neuer Marker setzt den Zähler
/// dann nicht zurück, auch wenn das Transkript die Rückmeldung des Blocks
/// anders schreibt als erwartet; so bleibt die Grenze je Zug unabhängig davon.
fn read_count(state_dir: &Path, session: &str, marker: &str, active: bool) -> Result<i64, FailureClass> {
    let source = match fs::read_to_string(state_dir.join(format!("guard-count-{session}"))) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(_) => return Err(FailureClass::StateRead),
    };
    let mut fields = source.split_whitespace();
    let (Some(stored_marker), Some(stored_count)) = (fields.next(), fields.next()) else {
        return Ok(0);
    };
    if (stored_marker != marker && !active) || fields.next().is_some() {
        return Ok(0);
    }
    Ok(stored_count.parse::<i64>().ok().filter(|count| *count >= 0).unwrap_or(0))
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), FailureClass> {
    let Some(parent) = path.parent() else {
        return Err(FailureClass::StateWrite);
    };
    if !parent.is_dir() {
        return Err(FailureClass::StateWrite);
    }
    let temp = parent.join(format!(".herzschlag-{}-{}.tmp", std::process::id(), NEXT_TEMP.fetch_add(1, Ordering::Relaxed)));
    fs::write(&temp, bytes).map_err(|_| FailureClass::StateWrite)?;
    fs::rename(&temp, path).map_err(|_| {
        let _ = fs::remove_file(&temp);
        FailureClass::StateWrite
    })
}

fn append_log(state_dir: &Path, failure: FailureClass, session: Option<&str>) {
    if fs::create_dir_all(state_dir).is_err() {
        return;
    }
    let mut record = json!({"action": "internal_error", "error": failure.name()});
    if let Some(session) = session {
        record["session_id"] = Value::String(schluessel(session));
    }
    let Ok(mut bytes) = serde_json::to_vec(&record) else {
        return;
    };
    bytes.push(b'\n');
    if let Ok(mut log) = OpenOptions::new().create(true).append(true).open(state_dir.join("herzschlag.log")) {
        let _ = log.write_all(&bytes);
    }
}

pub fn record_payload_failure(state_dir: &Path) {
    append_log(state_dir, FailureClass::Payload, None);
}

pub fn record_panic(state_dir: &Path) {
    append_log(state_dir, FailureClass::Panic, None);
}
