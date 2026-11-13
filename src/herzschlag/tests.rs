// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) Orchestration helpers authors
// Gegenüber der Quelle geändert (Repo und Commit in src/herzschlag/HERKUNFT.toml, was geändert ist
// dort unter geaendert); Zeitpunkt und Inhalt jeder Änderung stehen in der Git-Geschichte dieser Datei.

//! Tests des Herzschlags: Vertrag des Graphen, Zustand und Fehler, Matrix
//! über Antwort × Zähler × Nachlesen, die Szenarien ohne Wecker, die
//! Abweichungen von der Referenz (`fixtures/`) und die Fehler der Abfrage.
//! Jeder Fehlerfall hat eine Gegenprobe, in der derselbe Stop ohne den Fehler
//! aufgehalten wird; sonst bewiese „durch“ nichts.

use std::collections::{HashSet, VecDeque};
use std::fs;
use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::adapter::{self, HookResult};
use super::graph::{Arbeit, Effect, FailureClass, Guard, MAX_BLOCKS_PER_TURN, State, Timing, heartbeat_graph, schluessel};
use super::{abfrage, fragen};

static NEXT: AtomicUsize = AtomicUsize::new(0);

// ----- Hilfen (nach support/mod.rs) -----

/// Eigener Temp-Ordner je Fall; räumt beim Freigeben auf.
struct TestDir {
    path: PathBuf,
}

/// Was das Dashboard antwortet: der Reihe nach, das letzte wiederholt.
type Antworten = Vec<Result<Arbeit, FailureClass>>;

#[derive(Clone)]
struct RunOptions<'a> {
    antworten: Antworten,
    session: &'a str,
    raw_input: Option<&'a str>,
    missing_transcript: bool,
    stop_hook_active: bool,
    reload: Option<Vec<u8>>,
}

impl Default for RunOptions<'_> {
    fn default() -> Self {
        Self { antworten: vec![Ok(offen())], session: "reference-session", raw_input: None, missing_transcript: false, stop_hook_active: false, reload: None }
    }
}

fn offen() -> Arbeit {
    Arbeit::Offen("ar-1 Schritt 2 „Bauen“ (laeuft)".into())
}

impl TestDir {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!("dashboard-herzschlag-{label}-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(path.join("zustand")).unwrap();
        Self { path }
    }

    fn state_dir(&self) -> PathBuf {
        self.path.join("zustand")
    }

    fn state_path(&self, name: &str) -> PathBuf {
        self.state_dir().join(name)
    }

    fn write_state(&self, name: &str, bytes: impl AsRef<[u8]>) {
        fs::write(self.state_path(name), bytes).unwrap();
    }

    fn write_transcript(&self, entries: &[Value]) -> PathBuf {
        let path = self.path.join(format!("transcript-{}.jsonl", NEXT.fetch_add(1, Ordering::Relaxed)));
        fs::write(&path, json_lines(entries)).unwrap();
        path
    }

    fn run(&self, entries: &[Value], options: RunOptions<'_>) -> HookResult {
        self.run_in(&self.state_dir(), entries, options)
    }

    fn run_in(&self, state_dir: &Path, entries: &[Value], options: RunOptions<'_>) -> HookResult {
        let transcript = self.write_transcript(entries);
        if options.missing_transcript {
            fs::remove_file(&transcript).unwrap();
        }
        let input = options
            .raw_input
            .map_or_else(|| serde_json::to_vec(&payload(&transcript, options.session, options.stop_hook_active)).unwrap(), |raw| raw.as_bytes().to_vec());
        let timing = options.reload.as_deref().map_or_else(Timing::fixed, Timing::fixed_with_reload);
        let antworten = Mutex::new(options.antworten.into_iter().collect::<VecDeque<_>>());
        let abfrage = |_: &str| {
            let mut a = antworten.lock().unwrap();
            if a.len() > 1 { a.pop_front().unwrap() } else { a.front().cloned().unwrap() }
        };
        adapter::execute(&input, Some(state_dir), timing, &abfrage)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn payload(transcript: &Path, session: &str, stop_hook_active: bool) -> Value {
    json!({
        "transcript_path": transcript,
        "cwd": "C:\\projekt",
        "session_id": session,
        "stop_hook_active": stop_hook_active,
        "hook_event_name": "Stop"
    })
}

fn json_lines(entries: &[Value]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for entry in entries {
        bytes.extend(serde_json::to_vec(entry).unwrap());
        bytes.push(b'\n');
    }
    bytes
}

fn user(text: &str) -> Value {
    json!({"type": "user", "message": {"role": "user", "content": text}})
}

fn feedback() -> Value {
    user("Stop hook feedback:\nHERZSCHLAG: ...")
}

fn list_feedback() -> Value {
    json!({"type": "user", "message": {"role": "user", "content": [{"type": "text", "text": "Stop hook feedback:\nHERZSCHLAG: ..."}]}})
}

fn base() -> Vec<Value> {
    vec![user("question")]
}

fn append(mut left: Vec<Value>, right: impl IntoIterator<Item = Value>) -> Vec<Value> {
    left.extend(right);
    left
}

fn assert_silent_allow(result: &HookResult) {
    assert_eq!((result.status, result.stdout.as_slice(), result.stderr.as_slice()), (0, &[][..], &[][..]), "{result:?}");
}

fn assert_block(result: &HookResult) {
    assert_eq!((result.status, result.stdout.as_slice()), (2, &[][..]), "{result:?}");
    assert!(!result.stderr.is_empty());
}

fn system_message(result: &HookResult) -> String {
    assert_eq!((result.status, result.stderr.as_slice()), (0, &[][..]));
    serde_json::from_slice::<Value>(&result.stdout).unwrap()["systemMessage"].as_str().unwrap().to_owned()
}

fn with<'a>(antworten: Antworten) -> RunOptions<'a> {
    RunOptions { antworten, ..RunOptions::default() }
}

// ----- graph_contract -----

/// Mit std statt petgraph: alle von `Entry` aus erreichbaren Zustände.
fn reachable() -> HashSet<State> {
    let graph = heartbeat_graph();
    let mut seen = HashSet::from([State::Entry]);
    let mut queue = VecDeque::from([State::Entry]);
    while let Some(state) = queue.pop_front() {
        for (_, to, _) in graph.edges_from(state) {
            if seen.insert(*to) {
                queue.push_back(*to);
            }
        }
    }
    seen
}

#[test]
fn every_declared_state_occurs_once_and_is_reachable() {
    let graph = heartbeat_graph();
    let in_graph: HashSet<State> = graph.edges.iter().flat_map(|(from, to, _)| [*from, *to]).collect();
    assert_eq!(State::ALL.into_iter().collect::<HashSet<_>>().len(), State::ALL.len());
    assert_eq!(in_graph, State::ALL.into_iter().collect());
    assert_eq!(reachable(), State::ALL.into_iter().collect(), "unerreichbarer Zustand");
}

#[test]
fn all_three_verdict_paths_are_explicit_terminal_nodes() {
    let graph = heartbeat_graph();
    assert_eq!(State::VERDICTS.len(), 3);
    for verdict in State::VERDICTS {
        assert!(verdict.is_terminal());
        assert_eq!(graph.edges_from(verdict).count(), 0);
    }
    assert_eq!(State::ALL.into_iter().filter(|s| s.is_terminal()).collect::<HashSet<_>>(), State::VERDICTS.into_iter().collect());
}

#[test]
fn resolution_is_total_over_the_three_configuration_classes() {
    let guards: HashSet<Guard> = heartbeat_graph().edges_from(State::Resolve).map(|(_, _, t)| t.guard).collect();
    assert_eq!(guards, HashSet::from([Guard::InternalError, Guard::Idle, Guard::Prompt, Guard::Lapse]));
}

#[test]
fn every_blocking_verdict_records_the_count() {
    let graph = heartbeat_graph();
    let into_block: Vec<_> = graph.edges.iter().filter(|(_, to, _)| *to == State::BlockPrompt).collect();
    assert!(!into_block.is_empty());
    for (from, _, transition) in into_block {
        assert!(transition.effects.contains(&Effect::RecordCount), "Block aus {from:?} ohne Zähler");
    }
}

#[test]
fn every_nonterminal_has_an_outgoing_edge_and_every_failure_edge_opens() {
    let graph = heartbeat_graph();
    for state in State::ALL {
        if !state.is_terminal() {
            assert!(graph.edges_from(state).next().is_some(), "keine Kante aus {state:?}");
        }
    }
    // Jeder Fehler führt nach FailOpen, und FailOpen nur nach AllowSilent: kein Fehler blockiert.
    for (from, to, transition) in &graph.edges {
        if transition.guard == Guard::InternalError && *from != State::FailOpen {
            assert_eq!(*to, State::FailOpen, "Fehlerkante aus {from:?}");
        }
        if transition.guard == Guard::Malformed {
            assert_eq!(*to, State::FailOpen);
        }
    }
    assert!(graph.edges_from(State::FailOpen).all(|(_, to, _)| *to == State::AllowSilent));
    // Jeder Zustand mit Vorbereitung hat eine Fehlerkante.
    for state in [State::Tracker, State::Transcript, State::RetryOnce, State::Resolve, State::Prompt] {
        assert!(graph.edges_from(state).any(|(_, _, t)| t.guard == Guard::InternalError), "{state:?}");
    }
}

#[test]
fn the_tracker_is_asked_before_the_transcript() {
    let graph = heartbeat_graph();
    let from_entry: Vec<State> = graph.edges_from(State::Entry).map(|(_, to, _)| *to).collect();
    assert_eq!(from_entry, [State::FailOpen, State::Tracker]);
    let guards: HashSet<(Guard, State)> = graph.edges_from(State::Tracker).map(|(_, to, t)| (t.guard, *to)).collect();
    assert_eq!(guards, HashSet::from([(Guard::InternalError, State::FailOpen), (Guard::SessionUnknown, State::Off), (Guard::SessionKnown, State::Transcript)]));
    assert!(graph.edges_from(State::Off).all(|(_, to, _)| *to == State::AllowSilent));
}

// ----- state_and_faults -----

#[test]
fn absent_malformed_mismatched_and_negative_counts_recover_as_zero() {
    for (label, stored) in [("absent", None), ("malformed", Some("not a count")), ("mismatch", Some("9 2")), ("negative", Some("0 -4"))] {
        let directory = TestDir::new(label);
        if let Some(stored) = stored {
            directory.write_state("guard-count-count-session", stored);
        }
        let result = directory.run(&base(), RunOptions { session: "count-session", ..RunOptions::default() });
        assert_eq!(result.state, State::BlockPrompt, "case {label}");
        assert_eq!(fs::read(directory.state_path("guard-count-count-session")).unwrap(), b"0 1", "case {label}");
    }
}

#[test]
fn zero_and_two_counts_block_while_three_and_larger_lapse() {
    for (count, expected) in [(0, State::BlockPrompt), (2, State::BlockPrompt), (3, State::AllowLapseAlarm), (8, State::AllowLapseAlarm)] {
        let directory = TestDir::new(&format!("count-{count}"));
        directory.write_state("guard-count-count-session", format!("0 {count}"));
        let result = directory.run(&base(), RunOptions { session: "count-session", ..RunOptions::default() });
        assert_eq!(result.state, expected, "count {count}");
    }
}

#[test]
fn no_real_user_turn_uses_none_marker() {
    let directory = TestDir::new("no-user-turn");
    assert_block(&directory.run(&[], RunOptions::default()));
    assert_eq!(fs::read(directory.state_path("guard-count-reference-session")).unwrap(), b"None 1");
}

#[test]
fn string_and_list_feedback_are_not_user_turns() {
    for (label, entry) in [("string", feedback()), ("list", list_feedback())] {
        let directory = TestDir::new(label);
        directory.write_state("guard-count-reference-session", "0 3");
        let result = directory.run(&append(base(), [entry]), RunOptions::default());
        assert_eq!(result.state, State::AllowLapseAlarm, "case {label}: Rückmeldung zählt nicht als neuer Zug");
    }
}

#[test]
fn session_counters_are_isolated() {
    let directory = TestDir::new("session-isolation");
    assert_block(&directory.run(&base(), RunOptions { session: "one", ..RunOptions::default() }));
    assert_block(&directory.run(&base(), RunOptions { session: "two", ..RunOptions::default() }));
    assert_eq!(fs::read_to_string(directory.state_path("guard-count-one")).unwrap(), "0 1");
    assert_eq!(fs::read_to_string(directory.state_path("guard-count-two")).unwrap(), "0 1");
}

/// Nach project_flags_are_isolated_and_checked_before_transcript: Eine unbekannte
/// Sitzung lässt durch, ohne das Transkript zu lesen (es fehlt hier sogar).
#[test]
fn unknown_session_is_checked_before_the_transcript() {
    let directory = TestDir::new("unknown-session");
    let result = directory.run(&base(), RunOptions { missing_transcript: true, ..with(vec![Ok(Arbeit::Unbekannt)]) });
    assert_silent_allow(&result);
    assert_eq!(result.path, [State::Entry, State::Tracker, State::Off, State::AllowSilent]);
    // Gegenprobe: bekannte Sitzung mit offener Arbeit hält auf.
    assert_block(&directory.run(&base(), RunOptions::default()));
}

#[test]
fn unreadable_count_state_fails_open_without_a_block() {
    let directory = TestDir::new("unreadable-state");
    fs::create_dir_all(directory.state_path("guard-count-reference-session")).unwrap();
    let result = directory.run(&base(), RunOptions::default());
    assert_silent_allow(&result);
    assert!(result.path.contains(&State::FailOpen));
}

/// Ohne Unix-Rechte: Der Ordner der Zustandsdateien fehlt, lesen gibt 0, schreiben scheitert.
#[test]
fn count_write_failure_takes_the_anti_wedge_fail_open_edge() {
    let directory = TestDir::new("count-write-failure");
    let result = directory.run_in(&directory.path.join("fehlt"), &base(), RunOptions::default());
    assert_silent_allow(&result);
    assert!(result.path.contains(&State::Prompt) && result.path.contains(&State::FailOpen), "{:?}", result.path);
}

#[test]
fn malformed_transcript_lines_are_skipped_but_invalid_structure_fails_open() {
    let directory = TestDir::new("transcript-shape");
    let transcript = directory.path.join("mixed.jsonl");
    fs::write(&transcript, format!("not json\n[]\n{}\n", serde_json::to_string(&user("question")).unwrap())).unwrap();
    let input = serde_json::to_vec(&payload(&transcript, "mixed-lines", false)).unwrap();
    assert_block(&adapter::execute(&input, Some(&directory.state_dir()), Timing::fixed(), &|_| Ok(offen())));

    let result = directory.run(&[json!({"type": "user", "message": "not-an-object"})], RunOptions::default());
    assert_silent_allow(&result);
    assert!(result.path.contains(&State::FailOpen));
}

#[test]
fn lapse_names_the_open_work() {
    let directory = TestDir::new("lapse-message");
    directory.write_state("guard-count-reference-session", "0 3");
    let message = system_message(&directory.run(&base(), RunOptions::default()));
    assert!(message.contains("HERZSCHLAG (Ausnahme)") && message.contains("„Bauen“"), "{message}");
}

/// Nachlesen nach 1 s: Transkript und Dashboard werden neu gelesen; ist die
/// Arbeit inzwischen eingetragen, geht der Stop durch.
#[test]
fn active_stop_retries_once_and_asks_the_dashboard_again() {
    let directory = TestDir::new("retry-replacement");
    let result = directory.run(&base(), RunOptions { stop_hook_active: true, reload: Some(json_lines(&base())), ..with(vec![Ok(offen()), Ok(Arbeit::Frei)]) });
    assert_silent_allow(&result);
    assert_eq!(result.retry_count, 1);
    assert!(result.path.contains(&State::RetryOnce) && result.path.contains(&State::Idle), "{:?}", result.path);
    // Gegenprobe: bleibt die Arbeit offen, hält er nach dem Nachlesen auf.
    let result = directory.run(&base(), RunOptions { stop_hook_active: true, reload: Some(json_lines(&base())), ..RunOptions::default() });
    assert_block(&result);
    assert_eq!(result.retry_count, 1);
}

#[test]
fn tracker_failure_on_the_retry_fails_open() {
    let directory = TestDir::new("retry-tracker");
    let result = directory.run(&base(), RunOptions { stop_hook_active: true, ..with(vec![Ok(offen()), Err(FailureClass::Tracker)]) });
    assert_silent_allow(&result);
    assert!(result.path.contains(&State::RetryOnce) && result.path.contains(&State::FailOpen), "{:?}", result.path);
}

#[test]
fn unreadable_second_load_after_retry_fails_open() {
    let directory = TestDir::new("retry-unreadable");
    let result = directory.run(&base(), RunOptions { stop_hook_active: true, reload: Some(vec![0xff, 0xfe]), ..RunOptions::default() });
    assert_silent_allow(&result);
    assert_eq!(result.retry_count, 1);
    assert!(result.path.contains(&State::RetryOnce) && result.path.contains(&State::FailOpen));
}

#[test]
fn undecodable_first_transcript_load_fails_open() {
    let directory = TestDir::new("undecodable-transcript");
    let transcript = directory.path.join("undecodable.jsonl");
    fs::write(&transcript, [0xff, 0xfe]).unwrap();
    let input = serde_json::to_vec(&payload(&transcript, "undecodable", false)).unwrap();
    let result = adapter::execute(&input, Some(&directory.state_dir()), Timing::fixed(), &|_| Ok(offen()));
    assert_silent_allow(&result);
    assert!(result.path.contains(&State::FailOpen));
}

// ----- structural_matrix (Antwort × Zähler × Nachlesen) -----

#[test]
fn answer_count_and_retry_matrix_is_exact() {
    for antwort in [Arbeit::Unbekannt, Arbeit::Frei, offen()] {
        for count in [0, MAX_BLOCKS_PER_TURN - 1, MAX_BLOCKS_PER_TURN] {
            for retry in [false, true] {
                let directory = TestDir::new("matrix");
                directory.write_state("guard-count-reference-session", format!("0 {count}"));
                let result = directory.run(&base(), RunOptions { stop_hook_active: retry, ..with(vec![Ok(antwort.clone())]) });
                let expected = match (&antwort, count >= MAX_BLOCKS_PER_TURN) {
                    (Arbeit::Offen(_), false) => State::BlockPrompt,
                    (Arbeit::Offen(_), true) => State::AllowLapseAlarm,
                    _ => State::AllowSilent,
                };
                let case = format!("{antwort:?} count {count} retry {retry}");
                assert_eq!(result.state, expected, "{case}");
                // Nachgelesen wird nur bei aktivem Stop und offener Arbeit, und dann genau einmal.
                let retried = retry && matches!(antwort, Arbeit::Offen(_));
                assert_eq!(result.retry_count, u8::from(retried), "{case}");
                assert_eq!(result.path.contains(&State::Off), antwort == Arbeit::Unbekannt, "{case}");
                assert!(!result.path.contains(&State::FailOpen), "{case}");
            }
        }
    }
}

// ----- reference_scenarios (ohne Wecker) -----

#[test]
fn p01_stdin_not_json_fails_open() {
    let directory = TestDir::new("p01");
    assert_silent_allow(&directory.run(&[], RunOptions { raw_input: Some("not json{{"), ..RunOptions::default() }));
}

#[test]
fn p02_missing_transcript_with_open_work_fails_open() {
    let directory = TestDir::new("p02");
    assert_silent_allow(&directory.run(&base(), RunOptions { missing_transcript: true, ..RunOptions::default() }));
}

#[test]
fn p03_free_session_allows_silently() {
    let directory = TestDir::new("p03");
    let result = directory.run(&base(), with(vec![Ok(Arbeit::Frei)]));
    assert_silent_allow(&result);
    assert!(result.path.contains(&State::Idle));
}

#[test]
fn p06_open_work_blocks_with_the_steps_as_reason() {
    let directory = TestDir::new("p06");
    let result = directory.run(&base(), RunOptions::default());
    assert_block(&result);
    let reason = String::from_utf8_lossy(&result.stderr);
    assert!(reason.starts_with("HERZSCHLAG:") && reason.contains("ar-1 Schritt 2 „Bauen“ (laeuft)"), "{reason}");
}

#[test]
fn p17_prompt_cap_blocks_first_three_evaluations() {
    let directory = TestDir::new("p17");
    let entries = append(base(), [feedback(), feedback()]);
    for _ in 0..3 {
        assert_block(&directory.run(&entries, RunOptions { session: "cap-session", ..RunOptions::default() }));
    }
    assert_eq!(fs::read_to_string(directory.state_path("guard-count-cap-session")).unwrap(), "0 3");
}

#[test]
fn p19_grace_retry_is_scoped_and_controlled() {
    let directory = TestDir::new("p19");
    let first = directory.run(&base(), RunOptions::default());
    let retried = directory.run(&base(), RunOptions { stop_hook_active: true, ..RunOptions::default() });
    let idle = directory.run(&base(), RunOptions { stop_hook_active: true, ..with(vec![Ok(Arbeit::Frei)]) });
    assert_eq!((first.retry_count, retried.retry_count, idle.retry_count), (0, 1, 0));
}

#[test]
fn p25_pure_lapse_blocks_three_then_allows_with_alarm() {
    let directory = TestDir::new("p25");
    let entries = append(base(), [feedback(), feedback()]);
    let options = RunOptions { session: "pure-lapse-session", ..RunOptions::default() };
    for _ in 0..3 {
        assert_block(&directory.run(&entries, options.clone()));
    }
    assert!(system_message(&directory.run(&entries, options)).contains("geht jetzt durch"));
}

#[test]
fn p26_lapse_resolves_on_next_user_turn() {
    let directory = TestDir::new("p26");
    let entries = append(base(), [feedback(), feedback()]);
    let options = RunOptions { session: "lapse-recovery-session", ..RunOptions::default() };
    for _ in 0..4 {
        let _ = directory.run(&entries, options.clone());
    }
    assert_block(&directory.run(&append(entries, [user("next question")]), options));
}

/// Schreibt der Client die Rückmeldung eines Blocks so, dass sie wie ein
/// echter Nutzer-Zug aussieht, wandert der Marker bei jedem Stop. Die Grenze
/// hält trotzdem, weil ein Stop mit stop_hook_active den Zähler nicht
/// zurücksetzt; ein echter neuer Zug (ohne stop_hook_active) setzt ihn zurück.
#[test]
fn the_cap_holds_when_block_feedback_looks_like_a_user_turn() {
    let directory = TestDir::new("cap-misread");
    let mut entries = base();
    assert_block(&directory.run(&entries, RunOptions::default()));
    for _ in 0..2 {
        entries.push(user("HERZSCHLAG: ... (anders geschriebene Rückmeldung)"));
        assert_block(&directory.run(&entries, RunOptions { stop_hook_active: true, ..RunOptions::default() }));
    }
    entries.push(user("HERZSCHLAG: ... (anders geschriebene Rückmeldung)"));
    assert!(system_message(&directory.run(&entries, RunOptions { stop_hook_active: true, ..RunOptions::default() })).contains("geht jetzt durch"));
    // Gegenprobe: ein echter neuer Zug beginnt wieder bei 0 und hält auf.
    entries.push(user("next question"));
    assert_block(&directory.run(&entries, RunOptions::default()));
    assert_eq!(fs::read_to_string(directory.state_path("guard-count-reference-session")).unwrap(), format!("{} 1", entries.len() - 1));
}

// ----- oracle_fixtures (Abweichungen von der Referenz, hier ausgeführt) -----

const FIXTURES: [(&str, &str); 5] = [
    ("spec-block-stderr", include_str!("fixtures/spec-block-stderr.json")),
    ("spec-invalid-field-fail-open", include_str!("fixtures/spec-invalid-field-fail-open.json")),
    ("spec-list-feedback-turn-marker", include_str!("fixtures/spec-list-feedback-turn-marker.json")),
    ("spec-state-write-fail-open", include_str!("fixtures/spec-state-write-fail-open.json")),
    ("spec-unreadable-state-fail-open", include_str!("fixtures/spec-unreadable-state-fail-open.json")),
];

#[test]
fn every_specification_divergence_fixture_names_its_warrant_and_holds() {
    for (name, fixture) in FIXTURES {
        let value: Value = serde_json::from_str(fixture).unwrap();
        assert_eq!(value["fixture"].as_str(), Some(name));
        assert!(value["spec"].as_str().is_some() && value["python"].is_object() && value["specified"].is_object(), "{name}");
        let specified = &value["specified"];
        let directory = TestDir::new(name);
        let result = match name {
            "spec-block-stderr" => directory.run(&base(), RunOptions::default()),
            "spec-invalid-field-fail-open" => {
                let raw = r#"{"hook_event_name": "Stop", "session_id": 7, "transcript_path": "x"}"#;
                directory.run(&base(), RunOptions { raw_input: Some(raw), ..RunOptions::default() })
            }
            "spec-list-feedback-turn-marker" => {
                directory.write_state("guard-count-reference-session", "0 2");
                let result = directory.run(&append(base(), [list_feedback()]), RunOptions::default());
                assert_eq!(fs::read_to_string(directory.state_path("guard-count-reference-session")).unwrap(), "0 3", "Marker bleibt beim Nutzer-Zug 0");
                result
            }
            "spec-state-write-fail-open" => directory.run_in(&directory.path.join("fehlt"), &base(), RunOptions::default()),
            _ => {
                fs::create_dir_all(directory.state_path("guard-count-reference-session")).unwrap();
                directory.run(&base(), RunOptions::default())
            }
        };
        if let Some(status) = specified["status"].as_u64() {
            assert_eq!(u64::from(result.status), status, "{name}");
        }
        if specified["stdout"] == "" {
            assert!(result.stdout.is_empty(), "{name}");
        }
        if specified["stderr"] == "" {
            assert!(result.stderr.is_empty(), "{name}");
        }
        if specified["stream"] == "stderr" {
            assert!(!result.stderr.is_empty(), "{name}");
        }
        if specified["verdict"] == "FAIL_OPEN" {
            assert!(result.path.contains(&State::FailOpen), "{name}: {:?}", result.path);
        }
    }
}

// ----- neu: Eingabe, Pfade, Abfrage -----

/// Jede kaputte Eingabe und jedes falsche Ereignis endet mit Exit 0 ohne
/// Ausgabe, auch wo die Referenz mit Exit 2 ablehnt; ein Eintrag im Protokoll.
#[test]
fn broken_input_and_wrong_events_allow_silently() {
    let directory = TestDir::new("broken-input");
    let transcript = directory.write_transcript(&base());
    let gut = payload(&transcript, "s", false);
    let mut ohne_ereignis = gut.clone();
    ohne_ereignis.as_object_mut().unwrap().remove("hook_event_name");
    let mut anderes = gut.clone();
    anderes["hook_event_name"] = json!("PreToolUse");
    let mut kaputtes_feld = gut.clone();
    kaputtes_feld["stop_hook_active"] = json!("ja");
    let gross = vec![b' '; adapter::EINGABE_HOECHSTENS + 1];
    let faelle: Vec<(&str, Vec<u8>)> = vec![
        ("leer", Vec::new()),
        ("kein JSON", b"not json{{".to_vec()),
        ("Liste", b"[]".to_vec()),
        ("kein UTF-8", vec![0xff, 0xfe]),
        ("ohne Ereignis", serde_json::to_vec(&ohne_ereignis).unwrap()),
        ("anderes Ereignis", serde_json::to_vec(&anderes).unwrap()),
        ("Feld falsch", serde_json::to_vec(&kaputtes_feld).unwrap()),
        ("zu groß", gross),
    ];
    for (label, input) in faelle {
        let result = adapter::execute(&input, Some(&directory.state_dir()), Timing::fixed(), &|_| Ok(offen()));
        assert_silent_allow(&result);
        assert!(!result.path.contains(&State::BlockPrompt), "{label}");
    }
    assert!(fs::read_to_string(directory.state_path("herzschlag.log")).unwrap().contains("\"Payload\""));
    // Gegenprobe: dieselbe Eingabe, richtig, hält auf.
    assert_block(&adapter::execute(&serde_json::to_vec(&gut).unwrap(), Some(&directory.state_dir()), Timing::fixed(), &|_| Ok(offen())));
}

#[test]
fn without_a_state_directory_the_stop_passes() {
    let directory = TestDir::new("no-state-dir");
    let input = serde_json::to_vec(&payload(&directory.write_transcript(&base()), "s", false)).unwrap();
    assert_silent_allow(&adapter::execute(&input, None, Timing::fixed(), &|_| Ok(offen())));
}

#[test]
fn drive_paths_fold_to_a_safe_file_name() {
    assert_eq!(schluessel("C:\\Users\\a/b:c"), "C--Users-a-b-c");
    assert_eq!(schluessel("abc-DEF_123"), "abc-DEF_123");
    assert_eq!(schluessel("ä.."), "---");
    let directory = TestDir::new("drive-path");
    assert_block(&directory.run(&base(), RunOptions { session: "C:\\x/y:z", ..RunOptions::default() }));
    assert_eq!(fs::read_to_string(directory.state_path("guard-count-C--x-y-z")).unwrap(), "0 1");
}

/// Gegenprobe zu fail-open: Derselbe Stop hält bei offener Arbeit auf und
/// geht bei einem Fehler der Abfrage durch, mit Eintrag im Protokoll.
#[test]
fn tracker_failure_allows_the_same_stop_that_open_work_blocks() {
    let directory = TestDir::new("tracker-failure");
    assert_block(&directory.run(&base(), RunOptions { session: "a", ..RunOptions::default() }));
    let result = directory.run(&base(), RunOptions { session: "b", ..with(vec![Err(FailureClass::Tracker)]) });
    assert_silent_allow(&result);
    assert_eq!(result.path, [State::Entry, State::Tracker, State::FailOpen, State::AllowSilent]);
    let log = fs::read_to_string(directory.state_path("herzschlag.log")).unwrap();
    assert!(log.contains("\"Tracker\"") && log.contains("\"b\""), "{log}");
}

/// Ein Testserver auf einem freien Port (nie 8787), der jede Anfrage mit `antwort` beantwortet.
fn testserver(antwort: &'static [u8], warten: Duration) -> (String, std::thread::JoinHandle<()>) {
    let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let adresse = l.local_addr().unwrap().to_string();
    let h = std::thread::spawn(move || {
        if let Ok((mut s, _)) = l.accept() {
            let mut puffer = [0u8; 16384];
            let _ = s.set_read_timeout(Some(Duration::from_secs(5)));
            let _ = s.read(&mut puffer);
            std::thread::sleep(warten);
            let _ = s.write_all(antwort);
        }
    });
    (adresse, h)
}

/// Keine Verbindung, 401, Zeitüberschreitung, andere Codes, unlesbare und
/// gestückelte Antworten: jedes Mal ein Fehler, den `fragen` als
/// `FailureClass::Tracker` weitergibt (und der Graph als „durch“ nimmt).
#[test]
fn every_query_failure_is_a_tracker_failure() {
    let frei = TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().to_string();
    let e = abfrage::offen(&frei, "s1", Duration::from_secs(2)).unwrap_err();
    assert!(e.contains("keine Verbindung"), "{e}");
    assert_eq!(fragen(&frei, "s1"), Err(FailureClass::Tracker));

    for (label, antwort, teil) in [
        ("401", &b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\n\r\n"[..], "401"),
        ("403", &b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n"[..], "Antwort 403"),
        ("kein JSON", &b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nnein"[..], "kein JSON"),
        ("ohne offen", &b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}"[..], "offen"),
        ("chunked", &b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n"[..], "chunked"),
        ("zu", &b"HTTP/1.1 200 OK\r\nContent-Length: 50\r\n\r\n{"[..], "vorzeitig"),
    ] {
        let (adresse, h) = testserver(antwort, Duration::ZERO);
        let e = abfrage::offen(&adresse, "s1", Duration::from_secs(5)).unwrap_err();
        assert!(e.contains(teil), "{label}: {e}");
        h.join().unwrap();
    }
}

#[test]
fn a_hanging_dashboard_ends_at_the_deadline() {
    let (adresse, h) = testserver(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n", Duration::from_secs(3));
    let start = Instant::now();
    let e = abfrage::offen(&adresse, "s1", Duration::from_millis(500)).unwrap_err();
    assert!(start.elapsed() < Duration::from_secs(2), "{:?}", start.elapsed());
    assert!(e.contains("lesen") || e.contains("Frist"), "{e}");
    h.join().unwrap();
    assert_eq!(abfrage::FRIST, Duration::from_secs(5), "höchstens 5 s");
}

/// Gegenprobe zur Abfrage: Eine Antwort 200 in der Form des Dienstes wird gelesen.
#[test]
fn a_well_formed_answer_is_read() {
    let koerper = r#"{"sitzung":"s1","von":"chat:dashboard","offen":[{"arbeit":"ar-1","nr":2,"titel":"Bauen","grund":"laeuft"}]}"#;
    let antwort: &'static [u8] =
        format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{koerper}", koerper.len()).leak().as_bytes();
    let (adresse, h) = testserver(antwort, Duration::ZERO);
    assert_eq!(abfrage::offen(&adresse, "s1", Duration::from_secs(5)), Ok(offen()));
    h.join().unwrap();
    assert_eq!(abfrage::arbeit_aus(br#"{"sitzung":"s","von":null,"offen":[]}"#), Ok(Arbeit::Unbekannt));
    assert_eq!(abfrage::arbeit_aus(br#"{"sitzung":"s","von":"chat:a","offen":[]}"#), Ok(Arbeit::Frei));
}

#[test]
fn only_the_own_machine_and_well_formed_sessions_are_asked() {
    assert!(abfrage::adresse("10.0.0.1:80").is_err());
    assert!(abfrage::adresse("127.0.0.1:8788").is_ok());
    assert!(abfrage::adresse("localhost:8788").is_err(), "kein Name, keine Namensauflösung");
    for falsch in ["", "C:\\x", "a/b", "a b", &"x".repeat(129)] {
        assert!(!abfrage::sitzung_gueltig(falsch), "{falsch:?}");
        assert!(abfrage::offen("127.0.0.1:1", falsch, Duration::from_secs(1)).is_err());
    }
    assert!(abfrage::sitzung_gueltig("0b2c-4f_A"));
}

#[test]
fn the_state_folder_lies_under_local_app_data() {
    if let Some(o) = super::zustand_ordner() {
        assert!(o.ends_with(Path::new("Dashboard").join("herzschlag")), "{}", o.display());
    }
}
