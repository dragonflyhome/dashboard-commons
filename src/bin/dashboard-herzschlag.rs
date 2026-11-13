//! `dashboard-herzschlag.exe`: der Stop-Hook der Chats als eigenes Programm
//! ohne Fenster. Es tut dasselbe wie `dashboard.exe herzschlag` (Aufbau in
//! `src/herzschlag/`), gehört aber zum Subsystem „windows“: Startet ein
//! Programm ohne Konsole den Hook, öffnet Windows dafür kein Konsolenfenster.
//! stdin, stdout und stderr kommen weiter über die geerbten Handles, der
//! Exit-Code bleibt (0 durch, 2 aufhalten).

#![windows_subsystem = "windows"]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    std::process::exit(dashboard::herzschlag::ausfuehren(&args));
}
