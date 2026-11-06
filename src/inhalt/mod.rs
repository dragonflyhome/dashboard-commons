//! Inhalt: was die Chats schreiben, `projekt.toml` mit `daten/*.toml` und
//! `Seite.svelte` je Projekt. Die Typen dafür stehen in `src/vertrag/`;
//! `cargo test` erzeugt daraus `web/src/lib/vertrag/`.
//!
//! - `seite`: Projekte lesen
//! - `pruefung`: alle Regeln je Projekt, Befunde in einfachem Deutsch
//!   (für `dashboard pruefe` und `/api/pruefung`)
//! - `store`: der ContentStore (Etappe 2d): alles oben einmal gelesen und im
//!   Arbeitsspeicher, mit Versionsnummer und genauen Live-Ereignissen
//! - `assets`: Formate, Größen, SVG- und HTML-Regeln für `assets/`

pub mod assets;
pub mod pruefung;
pub mod seite;
pub mod store;
