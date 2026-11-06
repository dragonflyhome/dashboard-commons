//! Der Bote (Etappe 7a1): `dashboard-bote.exe` läuft ohne Fenster unter
//! das Konto des Inhabers und erledigt für den Dienst, was dessen Konto nicht darf oder
//! nicht sieht: Tresor (ranke/…), winget, Autostart, Ordnergrößen im Profil,
//! Werkzeug-Versionen aus dem PATH des Inhabers, Git-Stand der Repos.
//!
//! Der Bote ist der Server der benannten Pipe `\\.\pipe\<bote.pipe>`, der
//! Dienst der Client. Eine JSON-Zeile je Anfrage und Antwort (Typen in
//! `vertrag::bote`). Auflagen aus autostart-dienst:
//!
//! - Der Bote legt die Pipe mit FILE_FLAG_FIRST_PIPE_INSTANCE an, nur lokal,
//!   mit dem Inhaber als Besitzer und DACL: Inhaber voll, Dienstkonto nur Lesen und
//!   Schreiben, ohne Instanzen anzulegen (`sddl`, `DIENST_MASKE`).
//! - Der Bote erlaubt dem Dienstkonto, seinen Prozess und sein Token
//!   abzufragen (`win::pruefen_erlauben`), sonst könnte der Dienst ihn nicht prüfen.
//! - Der Dienst öffnet sie nur zur Identifikation (SECURITY_SQOS_PRESENT |
//!   SECURITY_IDENTIFICATION) und prüft vor dem ersten Senden auf zwei Wegen:
//!   Programm und Konto des Server-Prozesses, und den Besitzer der Pipe (`client`).
//!
//! - `win`: Win32-Aufrufe (Pipe, SIDs, Prozesse)
//! - `server`: die Schleife des Boten
//! - `aufgaben`: was der Bote auf eine Anfrage tut
//! - `bau`: die Oberfläche bauen (`npm run build` in einen neuen Ordner, Etappe 4v)
//! - `client`: die Seite des Dienstes, mit `BoteStand`
//! - `sammler`: die Sammler des Dienstes, die über den Boten holen
//! - `waechter`: startet den Boten neu, wenn er abstürzt (`--waechter`)
//!
//! Protokoll des Boten: `<pfade.protokoll>\bote\bote-JJJJ-MM-TT.log` (eigener
//! Unterordner, auf den der Inhaber Ändern hat; den Hauptordner darf der Inhaber nur
//! lesen). Geht das nicht, `%LOCALAPPDATA%\Dashboard\protokoll\` (`protokoll_ordner`).

pub mod aufgaben;
pub mod bau;
pub mod client;
pub mod oeffnen;
pub mod sammler;
#[cfg(windows)]
pub mod server;
pub mod waechter;
#[cfg(windows)]
pub mod win;

use std::path::{Path, PathBuf};
use std::time::Duration;

/// Eine Verbindung, auf der so lange keine Anfrage kam, trennt der Bote. Der
/// Dienst verwirft ruhende Verbindungen schon eine Minute früher (`client`)
/// und verbindet bei Bedarf neu.
pub const LEERLAUF: Duration = Duration::from_secs(10 * 60);

/// Unterordner des Protokollordners für den Boten.
pub const PROTOKOLL_UNTERORDNER: &str = "bote";

/// Wohin das Protokoll des Boten geht: `<protokoll>\bote`, wenn sich dort
/// schreiben lässt, sonst `ersatz` (`%LOCALAPPDATA%\Dashboard\protokoll`).
/// Der zweite Wert ist dann der Satz für die erste Zeile des Protokolls.
pub fn protokoll_ordner(protokoll: &Path, ersatz: Option<PathBuf>) -> (PathBuf, Option<String>) {
    let gewollt = protokoll.join(PROTOKOLL_UNTERORDNER);
    let grund = match schreibprobe(&gewollt) {
        Ok(()) => return (gewollt, None),
        Err(e) => e,
    };
    match ersatz {
        Some(e) if schreibprobe(&e).is_ok() => {
            let satz = format!("Protokoll des Boten hier statt in {} ({grund}). Das Einrichtungsskript gibt dir dort Ändern.", gewollt.display());
            (e, Some(satz))
        }
        // Auch der Ersatz geht nicht: beim gewollten Ordner bleiben;
        // `einrichten_mit` meldet es und schreibt in die Konsole.
        _ => (gewollt, None),
    }
}

/// `%LOCALAPPDATA%\Dashboard\protokoll`, wenn die Variable gesetzt ist.
pub fn ersatz_protokoll() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|l| PathBuf::from(l).join("Dashboard").join("protokoll"))
}

/// Legt den Ordner an und öffnet darin die heutige Protokolldatei zum
/// Anhängen (sie wird ohnehin gebraucht; so bleibt keine Probedatei liegen).
fn schreibprobe(ordner: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(ordner)?;
    let heute = crate::dienst::protokoll::dateiname("bote", chrono::Local::now().date_naive());
    std::fs::OpenOptions::new().create(true).append(true).open(ordner.join(heute))?;
    Ok(())
}

/// Zugriffsrechte des Dienstkontos an der Pipe; mit genau diesen Rechten
/// öffnet der Client sie auch (`win::pipe_oeffnen`). Herleitung:
///
/// ```text
///   FILE_READ_DATA         0x0000_0001   lesen
///   FILE_WRITE_DATA        0x0000_0002   schreiben
///   FILE_READ_ATTRIBUTES   0x0000_0080   Zustand der Pipe abfragen (GetNamedPipeServerProcessId)
///   FILE_WRITE_ATTRIBUTES  0x0000_0100   Zustand der Pipe setzen (SetNamedPipeHandleState)
///   READ_CONTROL           0x0002_0000   Sicherheitsbeschreibung lesen (Besitzer der Pipe)
///   SYNCHRONIZE            0x0010_0000   auf das Handle warten
///   -----------------------------------
///   zusammen               0x0012_0183
/// ```
///
/// Früher stand hier 0x12019F = FILE_GENERIC_READ (0x120089) | FILE_GENERIC_WRITE
/// (0x120116). Darin steckt FILE_APPEND_DATA = FILE_CREATE_PIPE_INSTANCE (0x4):
/// Wer das hat, darf weitere Instanzen der Pipe anlegen und so Clients
/// abfangen. Ebenfalls weg sind FILE_READ_EA (0x8) und FILE_WRITE_EA (0x10),
/// die eine Pipe nicht braucht. 0x12019F ohne 0x4 wäre 0x12019B; ohne die
/// beiden EA-Bits bleibt 0x120183.
pub const DIENST_MASKE: u32 = 0x0012_0183;
/// `DIENST_MASKE` als SDDL-Text.
pub const DIENST_RECHTE: &str = "0x120183";

/// Sicherheitsbeschreibung der Pipe als SDDL: Besitzer und voller Zugriff
/// das eigene Konto, das Dienstkonto (wenn bekannt) `DIENST_RECHTE`, sonst
/// niemand. Der Besitzer steht ausdrücklich drin, damit der Client ihn
/// prüfen kann (läuft der Bote erhöht, wäre sonst die Gruppe Administratoren
/// Besitzer).
pub fn sddl(eigene_sid: &str, dienst_sid: Option<&str>) -> String {
    let mut s = format!("O:{eigene_sid}D:(A;;GA;;;{eigene_sid})");
    if let Some(d) = dienst_sid.filter(|d| !d.is_empty()) {
        s.push_str(&format!("(A;;{DIENST_RECHTE};;;{d})"));
    }
    s
}

/// Pfad zum Vergleichen: kanonisch, wenn es die Datei gibt, ohne `\\?\`,
/// mit `\`, klein geschrieben.
pub fn pfad_zum_vergleich(p: &Path) -> String {
    let kanonisch = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let text = kanonisch.display().to_string().replace('/', "\\");
    let text = text.strip_prefix(r"\\?\UNC\").map(|r| format!(r"\\{r}")).unwrap_or(text);
    let text = text.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(text);
    text.to_lowercase()
}

/// Dieselbe Datei? Groß/Klein egal.
pub fn gleicher_pfad(a: &Path, b: &Path) -> bool {
    pfad_zum_vergleich(a) == pfad_zum_vergleich(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sddl_mit_und_ohne_dienst() {
        let ich = "S-1-5-21-1-2-3-1001";
        assert_eq!(sddl(ich, Some("S-1-5-80-9")), format!("O:{ich}D:(A;;GA;;;{ich})(A;;0x120183;;;S-1-5-80-9)"));
        assert_eq!(sddl(ich, None), format!("O:{ich}D:(A;;GA;;;{ich})"));
        assert_eq!(sddl(ich, Some("")), format!("O:{ich}D:(A;;GA;;;{ich})"));
    }

    #[test]
    fn dienst_maske_ohne_instanzen() {
        assert_eq!(DIENST_RECHTE, format!("{DIENST_MASKE:#X}").replace("0X", "0x"));
        // FILE_CREATE_PIPE_INSTANCE (= FILE_APPEND_DATA) ist nicht dabei.
        assert_eq!(DIENST_MASKE & 0x4, 0);
        // Auch keine EA-Rechte, kein WRITE_DAC, kein WRITE_OWNER, kein DELETE.
        assert_eq!(DIENST_MASKE & (0x8 | 0x10 | 0x4_0000 | 0x8_0000 | 0x1_0000), 0);
        assert_eq!(0x12019F & !0x4, 0x12019B);
        assert_eq!(0x12019B & !(0x8 | 0x10), DIENST_MASKE);
    }

    #[cfg(windows)]
    #[test]
    fn dienst_maske_aus_den_konstanten() {
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_READ_ATTRIBUTES, FILE_READ_DATA, FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, READ_CONTROL, SYNCHRONIZE,
        };
        assert_eq!(FILE_READ_DATA | FILE_WRITE_DATA | FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES | SYNCHRONIZE | READ_CONTROL, DIENST_MASKE);
    }

    #[cfg(windows)]
    #[test]
    fn sddl_ist_gueltig_fuer_windows() {
        let eigene = win::eigene_sid().unwrap();
        assert!(eigene.starts_with("S-1-5-"), "{eigene}");
        win::Beschreibung::aus_sddl(&sddl(&eigene, Some(&eigene))).unwrap();
        assert!(win::Beschreibung::aus_sddl("D:(A;;GA;;;keine-sid)").is_err());
    }

    #[test]
    fn pfade_vergleichen() {
        let hier = Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let anders = hier.display().to_string().to_uppercase().replace('\\', "/");
        assert!(gleicher_pfad(&hier, Path::new(&anders)));
        assert!(!gleicher_pfad(&hier, &hier.with_file_name("Cargo.lock")));
        assert_eq!(pfad_zum_vergleich(Path::new("C:/Gibt/Es/Nicht.exe")), r"c:\gibt\es\nicht.exe");
    }
}
