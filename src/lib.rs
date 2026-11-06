//! Lokales Dashboard: PC-Status und Projektübersicht im Browser.
//!
//! Die Bibliothek hinter beiden Programmen: `dashboard.exe` (Server, Start in
//! `main.rs`) und `dashboard-bote.exe` (der Bote unter dem Konto des Inhabers, Start in
//! `bin/dashboard-bote.rs`). Beide teilen die Module hier.
//!
//! Zusammenbau in `app.rs`, Fehlertyp in `fehler.rs`, Inhalt (Projektseiten,
//! Bausteine) in `inhalt/`, JSON-API in `api/`, PC-Messung in `pc/`,
//! Oberfläche (web/build) in `oberflaeche.rs`, Befehle ohne Server
//! (`dashboard pruefe …`) in `werkzeug/`, Aufrufe, Einstellungsdatei,
//! Protokoll und Windows-Dienst in `dienst/`, Pipe, Aufgaben und Client des
//! Boten in `bote/`, die Rechnung für „Anstehend“ (Etappe A1) in `anstehend.rs`,
//! das Register der Backend-Module der Projekte (Etappe 4b) in `projekte/`,
//! die einzige Anmeldestelle der Module ([`starten`], Umzug des Inhalts) in `start.rs`,
//! Passkey und JWT für andere Windows-Konten (Etappe 7c) in `passkey/`,
//! mehrere Inhaltsquellen (`DASHBOARD_PROJECTS_DIR` als Liste, Etappe 4b) in `quellen.rs`,
//! Ereignisse mit Bereich (Datenspeicher, PC-Benutzerdaten, docs/; Live-Lücken) in `aenderungen.rs`,
//! das Verbergen von Profilpfad und Benutzername in allem, was hinausgeht, in `datenschutz.rs`.
//!
//! Für eine Zusammenbau-Kiste genügen [`starten`] und [`eintrag`]:
//! `std::process::exit(dashboard::starten(vec![dashboard::eintrag(projekt_x::X)]))`.

pub mod aenderungen;
pub mod anmeldung;
pub mod anstehend;
pub mod api;
pub mod app;
#[cfg(test)]
mod archiv_test;
pub mod backlog;
pub mod baumeister;
pub mod baustand;
pub mod bote;
pub mod datenschutz;
pub mod dienst;
pub mod fehler;
#[cfg(test)]
mod frei_test;
pub mod fristen;
pub mod git;
pub mod git_zwischenspeicher;
#[cfg(test)]
mod gruppen_test;
pub mod inhalt;
pub mod meldungen;
pub mod oberflaeche;
pub mod oeffnen;
pub mod passkey;
pub mod pc;
pub mod projekte;
pub mod quellen;
#[cfg(test)]
mod schleusen_test;
pub mod schluessel;
pub mod sicherheit;
pub mod speicher;
pub mod start;
pub mod vertrag;
pub mod werkzeug;

pub use projekte::{ModulEintrag, eintrag};
pub use start::starten;

/// Umgebungsvariable: gesetzt (etwa `1`), schlägt ein Test fehl, statt sich zu
/// überspringen, wenn ein Werkzeug fehlt (Node, npm, Module, Bau der Oberfläche).
pub const TESTS_VOLL: &str = "DASHBOARD_TESTS_VOLL";

/// Für Tests, die ein Werkzeug brauchen: meldet „ÜBERSPRUNGEN“ (sichtbar mit
/// `--nocapture`) und gibt `true` zurück. Mit [`TESTS_VOLL`] schlägt der
/// Test stattdessen fehl, damit ein übersprungener Test nie als bestanden
/// zählt (Abnahme 4c, Auflage). `true` heißt: jetzt `return`.
#[doc(hidden)]
pub fn ueberspringen(grund: &str) -> bool {
    if std::env::var_os(TESTS_VOLL).is_some() {
        panic!("{grund} ({TESTS_VOLL} ist gesetzt: kein Test wird übersprungen)");
    }
    eprintln!("ÜBERSPRUNGEN: {grund}");
    true
}

/// Zeitgrenze für Tests, die „schnell“ prüfen: `streng` im normalen Lauf,
/// `voll` mit [`TESTS_VOLL`]. Dort laufen alle Tests (auch Bauten mit npm und
/// PowerShell) gleichzeitig; einzelne Messungen streuen dann, ohne dass der
/// Code langsamer ist (Entscheidung, kleinkram_4).
#[doc(hidden)]
pub fn zeitgrenze(streng: std::time::Duration, voll: std::time::Duration) -> std::time::Duration {
    if std::env::var_os(TESTS_VOLL).is_some() { voll } else { streng }
}
