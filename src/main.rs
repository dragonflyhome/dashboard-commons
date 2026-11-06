//! Lokales Dashboard: PC-Status und Projektübersicht im Browser.
//! Start: `cargo run`, dann http://localhost:8787 öffnen.
//!
//! Das allgemeine `dashboard.exe` des Code-Repos: ohne Backend-Module der
//! Projekte. Die Projektseiten laufen, die Rechenteile fehlen (ein Satz in der
//! Prüfung). Die Fassung mit Rechenteilen baut eine Zusammenbau-Kiste
//! (`vorlagen/dashboard-lokal/`), die `dashboard::starten(module)` aufruft.
//! Die Aufrufe zeigt `dashboard --hilfe`.

fn main() {
    std::process::exit(dashboard::starten(Vec::new()));
}
