//! Der Vertrag aus einer Quelle (Zielbild, „API-Vertrag aus einer Quelle“).
//!
//! Ein Typ, drei Nutzungen: Dieselben Rust-Typen lesen die TOML-Dateien der
//! Chats (`projekt.toml`, `daten/*.toml`, mit `serde`), gehen als JSON über
//! die API und werden zu TypeScript-Typen für die Oberfläche (`ts-rs`).
//! `cargo test` schreibt die TypeScript-Dateien nach `web/src/lib/vertrag/`
//! (siehe `export.rs`); sie sind eingecheckt und werden nie von Hand geändert.
//!
//! - `bedeutung`: die sechs Bedeutungen statt Farbklassen
//! - `projekt`: der Kopf `projekt.toml`
//! - `daten`: je Baustein ein Datentyp (`daten/<baustein>.toml`)
//! - `api`: alle Antworten der API
//! - `bausteine`: die Anzeige-Bausteine der Oberfläche (`Block`)
//! - `abgeleitet`: was der Server aus einer Datei rechnet (Anzahl, Summe, Tage)
//! - `lesen`: TOML lesen, Fehler in einfachem Deutsch mit Datei und Zeile
//! - `anstehend`: Aufgaben, Fragen und Arbeiten als eine Liste (Etappe A1)
//! - `backlog`: Spalten nach Stand und Status, Wartende je Verantwortlichem (Etappe B2)
//! - `fristen`: Ereignisse, feste und bewegliche Fristen, Fristenliste (Etappe B3)
//! - `plan`: die Etappen eines Projekts aus `daten/plan.toml` (Etappe B8)
//! - `schwerpunkte`: `data/schwerpunkte.toml`
//! - `gruppen`: `data/gruppen.toml` (Projektgruppen)
//! - `fassungen`: die Fassungen aus dem Changelog (`/api/fassungen`)
//! - `passkey`: Passkeys und ihre Wege (Etappe 7c)
//! - `system_seiten`: Entscheidungen und Zielbild aus `docs/` (Etappe 4d)
//! - `verlauf`: Ereignisse für den Verlauf (PC und Zeitstrahl, Etappe 2e)
//! - `export`: TypeScript schreiben und prüfen (nur in Tests)

pub mod abgeleitet;
pub mod anstehend;
pub mod api;
pub mod backlog;
pub mod bausteine;
pub mod bedeutung;
pub mod bote;
pub mod daten;
pub mod fassungen;
pub mod fristen;
pub mod gruppen;
pub mod lesen;
pub mod passkey;
pub mod plan;
pub mod projekt;
pub mod schwerpunkte;
pub mod system_seiten;
pub mod verlauf;

#[cfg(test)]
mod export;
