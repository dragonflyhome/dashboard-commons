//! Die Fassung des Dashboards mit den Rechenteilen der Projekte
//! ([zusammenbau]): die Bibliothek des Code-Repos und die Module aus
//! `module()` (src/lib.rs). Aufrufe wie beim allgemeinen dashboard.exe
//! (`--hilfe`). Dienst.ps1 -Neustart spielt dieses Programm als dashboard.exe ein.

fn main() {
    std::process::exit(dashboard::starten(dashboard_lokal::module()));
}
