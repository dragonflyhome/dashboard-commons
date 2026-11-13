//! `dashboard.exe herzschlag`: der Stop-Hook eines Chats. Er liest die Eingabe
//! des Hooks von stdin, fragt das Dashboard, ob der Chat dieser Sitzung noch
//! offene Arbeit hat (`GET /api/sitzungen/<id>/offen`), und hält den Stop auf,
//! solange das so ist: höchstens dreimal je echtem Nutzer-Zug, danach geht er
//! mit Warnung durch. Jeder Fehler lässt den Stop durch (Exit 0).
//!
//! Zustandsgraph (`graph`) und Hook-Seite (`adapter`) sind aus einem anderen
//! Projekt übernommen und stehen unter GPL-3.0-or-later (Kopf der Dateien,
//! Herkunft in `HERKUNFT.toml`). Die Abfrage (`abfrage`) und dieser Teil
//! gehören zu diesem Repo.

pub mod abfrage;
pub mod adapter;
pub mod graph;
#[cfg(test)]
mod tests;

use std::io::{Read, Write};
use std::path::PathBuf;

/// Ordner der Zustandsdateien: `%LOCALAPPDATA%\Dashboard\herzschlag`; ohne die Variable `None`.
#[must_use]
pub fn zustand_ordner() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty()).map(|v| PathBuf::from(v).join("Dashboard").join("herzschlag"))
}

/// Die Arbeit einer Sitzung beim Dashboard unter `server`, mit Frist.
fn fragen(server: &str, sitzung: &str) -> Result<graph::Arbeit, graph::FailureClass> {
    abfrage::offen(server, sitzung, abfrage::FRIST).map_err(|_| graph::FailureClass::Tracker)
}

/// `dashboard.exe herzschlag [--server 127.0.0.1:<port>]`. Gibt den Exit-Code
/// zurück: 0 durch, 2 aufhalten (Grund auf stderr). Ein falscher Aufruf lässt
/// ebenfalls durch, damit ein Tippfehler im Hook keinen Chat festhält.
#[must_use]
pub fn ausfuehren(args: &[String]) -> i32 {
    // Eine Panik schreibt nichts auf stderr: bei Exit 0 sähe der Chat sonst Rauschen.
    std::panic::set_hook(Box::new(|_| {}));
    let server = match args {
        [] => abfrage::SERVER.to_string(),
        [schalter, wert] if schalter == "--server" => wert.clone(),
        _ => {
            eprintln!("herzschlag: Aufruf ohne Wirkung, erwartet: dashboard.exe herzschlag [--server 127.0.0.1:<port>]");
            return 0;
        }
    };
    let mut eingabe = Vec::new();
    if std::io::stdin().lock().take(adapter::EINGABE_HOECHSTENS as u64 + 1).read_to_end(&mut eingabe).is_err() {
        return 0;
    }
    let ordner = zustand_ordner();
    if let Some(o) = &ordner {
        let _ = std::fs::create_dir_all(o);
    }
    let r = adapter::execute(&eingabe, ordner.as_deref(), graph::Timing::system(), &|sitzung| fragen(&server, sitzung));
    let geschrieben = std::io::stdout().write_all(&r.stdout).and_then(|()| std::io::stdout().flush()).is_ok()
        && std::io::stderr().write_all(&r.stderr).and_then(|()| std::io::stderr().flush()).is_ok();
    // Lässt sich der Grund nicht schreiben, kein Block ohne Grund (Upstream gab hier 2).
    if geschrieben { i32::from(r.status) } else { 0 }
}
