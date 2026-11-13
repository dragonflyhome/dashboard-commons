//! Die einzige Stelle, an der Backend-Module der Projekte angemeldet werden
//! (Entscheidung `inhaltsquellen` [zusammenbau]).
//!
//! Das Code-Repo baut eine Bibliothek und ein allgemeines `dashboard.exe`
//! ohne Projektmodule (`main.rs` ruft `starten(Vec::new())`). Die
//! zusammengesetzte Fassung entsteht außerhalb, in einer Zusammenbau-Kiste
//! (Vorlage in `vorlagen/dashboard-lokal/`): Ihr `main.rs` ruft
//! `dashboard::starten(module)` mit den geprüften Projektkisten. Im Code-Repo
//! steht dafür kein Projektname.
//!
//! Ohne Module laufen alle Projektseiten; was ein Rechenteil unter
//! `/api/projekte/<slug>/x/…` liefern würde, fehlt, und die Prüfung sagt das
//! je Projekt mit einem Satz ([`crate::projekte::SATZ_OHNE_ZUSAMMENBAU`]).

use crate::dienst::{self, Aufruf};
use crate::projekte::ModulEintrag;
use crate::werkzeug;

/// Meldet die Module an und führt den Aufruf aus (Server, Dienst, `pruefe`,
/// `--hilfe`). Gibt den Rückgabewert für `std::process::exit`.
pub fn starten(module: Vec<ModulEintrag>) -> i32 {
    // Paniken auch ins Protokoll (alle Modi), nicht nur auf stderr.
    dienst::panik_ins_protokoll();
    crate::projekte::anmelden(module);
    let args: Vec<String> = std::env::args().skip(1).collect();
    match dienst::aufruf_lesen(&args) {
        Aufruf::Umgebung => dienst::umgebung(),
        // `dashboard pruefe [<slug>|--alle]`: prüfen und beenden, ohne Server.
        Aufruf::Pruefe(rest) => match dienst::mit_laufzeit(async move { Ok(werkzeug::pruefe::ausfuehren(&rest)) }) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("{e}");
                e.code() as i32
            }
        },
        // Wächter der Zusammenbau-Kiste: liest nur Dateien und fragt git, baut nichts.
        Aufruf::PruefeZusammenbau(rest) => crate::projekte::zusammenbau::ausfuehren(&rest),
        // Stop-Hook der Chats: ohne Server, jeder Fehler lässt den Stop durch.
        Aufruf::Herzschlag(rest) => crate::herzschlag::ausfuehren(&rest),
        Aufruf::Hilfe => {
            print!("{}", dienst::HILFE);
            0
        }
        Aufruf::Konsole { einstellungen } => dienst::konsole(&einstellungen),
        Aufruf::Dienst { einstellungen, dienstname } => dienst_starten(&dienstname, einstellungen),
        Aufruf::Falsch(satz) => {
            eprintln!("{satz}\n\n{}", dienst::HILFE);
            2
        }
    }
}

#[cfg(windows)]
fn dienst_starten(name: &str, einstellungen: std::path::PathBuf) -> i32 {
    match dienst::windows::starten(name, einstellungen) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e}");
            e.code() as i32
        }
    }
}

#[cfg(not(windows))]
fn dienst_starten(_name: &str, _einstellungen: std::path::PathBuf) -> i32 {
    eprintln!("--dienst gibt es nur unter Windows. Bitte --konsole --einstellungen <datei> nehmen.");
    2
}
