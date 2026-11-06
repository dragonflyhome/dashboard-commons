//! Der Bote des Dashboards (Etappe 7a1): läuft ohne Fenster unter dem Konto des
//! Konto, legt die Pipe an und erledigt Anfragen des Dienstes. Aufbau in
//! `src/bote/`. Die Aufrufe zeigt `dashboard-bote --hilfe`.
//!
//! Ohne Fenster heißt auch: ohne eigene Konsole. Für `--hilfe`, `--einmal`
//! und Fehlermeldungen gilt darum (`zeigen`): Ist die Ausgabe umgeleitet
//! (etwa `| Out-Host` oder aus einem Test), geht der Text dorthin. Sonst hängt
//! sich der Bote an die Konsole des Aufrufers (AttachConsole mit
//! ATTACH_PARENT_PROCESS), etwa das Terminal, aus dem er gestartet wurde.
//! Gibt es keine (Start aus der Aufgabenplanung oder per Doppelklick), steht
//! der Text in `%TEMP%\dashboard-bote\bote-JJJJ-MM-TT.log`.

#![windows_subsystem = "windows"]

const HILFE: &str = "\
Aufrufe von dashboard-bote.exe:

  dashboard-bote.exe --einstellungen <datei>
      Der Bote: liest dienst.toml (Pipe-Name, Protokollordner, SIDs), schreibt
      bote-JJJJ-MM-TT.log in den Unterordner bote des Protokollordners (geht
      das nicht: %LOCALAPPDATA%\\Dashboard\\protokoll) und bedient die Pipe,
      bis er beendet wird.
  dashboard-bote.exe --waechter --einstellungen <datei>
      Der Wächter: startet den Boten als Kind und nach einem Absturz neu
      (nach 5 s, 30 s, 2 min, dann alle 2 min; bei mehr als 5 Abstürzen je
      Stunde alle 10 min). Endet der Bote mit 0, endet auch der Wächter; endet
      er mit 2 (Einstellungen falsch), endet der Wächter ohne Neustart mit 2.
      Startet bei deiner Anmeldung über die Aufgabenplanung.
  dashboard-bote.exe --einmal <anfrage-json> [--einstellungen <datei>]
      Eine Anfrage ohne Pipe ausführen und die Antwort als JSON-Zeile ausgeben,
      etwa --einmal {\"art\":\"hallo\"}. Für Tests.
  dashboard-bote.exe --hilfe
      Diese Hilfe.

Ende mit: 0 gut, 2 Einstellungen oder Aufruf falsch, 4 Pipe schon belegt
(läuft schon ein Bote?), 9 anderer Fehler.
";

/// Was der Bote tun soll.
enum Aufruf {
    Hilfe,
    Bote(std::path::PathBuf),
    Waechter(std::path::PathBuf),
    Einmal { anfrage: String, einstellungen: Option<std::path::PathBuf> },
    Falsch(String),
}

fn aufruf_lesen(args: &[String]) -> Aufruf {
    let mut einstellungen = None;
    let mut einmal = None;
    let mut waechter = false;
    let mut i = 0;
    while i < args.len() {
        let wert = args.get(i + 1).cloned();
        match args[i].as_str() {
            "--hilfe" | "-h" | "--help" | "/?" => return Aufruf::Hilfe,
            "--waechter" => waechter = true,
            "--einstellungen" => {
                let Some(w) = wert.filter(|w| !w.starts_with("--")) else {
                    return Aufruf::Falsch("Nach --einstellungen fehlt die Datei.".into());
                };
                einstellungen = Some(std::path::PathBuf::from(w));
                i += 1;
            }
            "--einmal" => {
                let Some(w) = wert else { return Aufruf::Falsch("Nach --einmal fehlt die Anfrage (JSON).".into()) };
                einmal = Some(w);
                i += 1;
            }
            anders => return Aufruf::Falsch(format!("Das Argument „{anders}“ kennt dashboard-bote.exe nicht.")),
        }
        i += 1;
    }
    match (einmal, einstellungen) {
        (Some(_), _) if waechter => Aufruf::Falsch("--waechter und --einmal gehen nicht zusammen.".into()),
        (Some(anfrage), einstellungen) => Aufruf::Einmal { anfrage, einstellungen },
        (None, Some(e)) if waechter => Aufruf::Waechter(e),
        (None, Some(e)) => Aufruf::Bote(e),
        (None, None) => Aufruf::Falsch("Es fehlt --einstellungen <datei>.".into()),
    }
}

/// Wohin ein Text gehört.
#[derive(Clone, Copy, PartialEq)]
enum Strom {
    Aus,
    Fehler,
}

/// Macht den Strom sichtbar, wenn es geht: umgeleitet ist er es schon, sonst
/// an die Konsole des Aufrufers hängen und die leeren Standard-Handles auf
/// sie setzen. `false`: keine Konsole, der Text ginge verloren.
#[cfg(windows)]
fn konsole(strom: Strom) -> bool {
    use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING};
    use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle};
    let gueltig = |h: HANDLE| !h.is_null() && h != INVALID_HANDLE_VALUE;
    let welcher = if strom == Strom::Aus { STD_OUTPUT_HANDLE } else { STD_ERROR_HANDLE };
    // SAFETY: GetStdHandle liest nur.
    if gueltig(unsafe { GetStdHandle(welcher) }) {
        return true;
    }
    // Scheitert es, weil schon eine Konsole da ist, geht es trotzdem weiter.
    // SAFETY: einfacher Aufruf ohne Zeiger.
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
    let name: Vec<u16> = "CONOUT$".encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: nullterminierter Name; das Handle bleibt bis zum Prozessende offen (Standard-Handle).
    let h = unsafe {
        CreateFileW(name.as_ptr(), GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE, std::ptr::null(), OPEN_EXISTING, 0, std::ptr::null_mut())
    };
    if !gueltig(h) {
        return false;
    }
    for s in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        // SAFETY: nur leere Standard-Handles werden gesetzt; std liest sie bei jedem Schreiben neu.
        unsafe {
            if !gueltig(GetStdHandle(s)) {
                SetStdHandle(s, h);
            }
        }
    }
    // Eine Leerzeile, damit der Text nicht hinter der Eingabeaufforderung klebt.
    eprintln!();
    true
}

#[cfg(not(windows))]
fn konsole(_strom: Strom) -> bool {
    true
}

/// Zeigt einen Text: auf stdout/stderr (umgeleitet oder Konsole des
/// Aufrufers), sonst mit `ersatzdatei` in `%TEMP%\dashboard-bote\bote-JJJJ-MM-TT.log`.
fn zeigen(strom: Strom, text: &str, ersatzdatei: bool) {
    if konsole(strom) {
        match strom {
            Strom::Aus => print!("{text}"),
            Strom::Fehler => eprint!("{text}"),
        }
        return;
    }
    if ersatzdatei {
        use std::io::Write;
        let ordner = std::env::temp_dir().join("dashboard-bote");
        let jetzt = chrono::Local::now();
        let datei = ordner.join(format!("bote-{}.log", jetzt.format("%Y-%m-%d")));
        let _ = std::fs::create_dir_all(&ordner);
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(datei) {
            let _ = writeln!(f, "{} {}", jetzt.format("%Y-%m-%d %H:%M:%S"), text.trim_end());
        }
    }
}

fn main() {
    dashboard::dienst::panik_ins_protokoll();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match aufruf_lesen(&args) {
        Aufruf::Hilfe => {
            zeigen(Strom::Aus, HILFE, true);
            0
        }
        Aufruf::Falsch(satz) => {
            zeigen(Strom::Fehler, &format!("{satz}\n\n{HILFE}"), true);
            2
        }
        Aufruf::Einmal { anfrage, einstellungen } => einmal(&anfrage, einstellungen.as_deref()),
        Aufruf::Bote(datei) => bote(&datei),
        Aufruf::Waechter(datei) => waechter(&datei),
    };
    std::process::exit(code);
}

/// `--einmal`: eine Anfrage, Antwort auf stdout.
fn einmal(anfrage: &str, einstellungen: Option<&std::path::Path>) -> i32 {
    use dashboard::bote::aufgaben::{self, Kontext};
    use dashboard::vertrag::bote::{Anfrage, Antwort};
    let mut kontext = Kontext::jetzt();
    if let Some(datei) = einstellungen {
        match dashboard::dienst::einstellungen::Einstellungen::laden(datei) {
            // Mit Einstellungen gilt die Projektliste für den Git-Stand wie im Boten.
            Ok(e) => kontext = kontext.mit_einstellungen(&e),
            Err(satz) => {
                zeigen(Strom::Fehler, &format!("{satz}\n"), true);
                return 2;
            }
        }
    }
    let (antwort, code) = match serde_json::from_str::<Anfrage>(anfrage) {
        Err(e) => (Antwort::fehler(format!("Die Anfrage ist unlesbar ({e}).")), 2),
        Ok(a) => match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
            Ok(rt) => (rt.block_on(aufgaben::ausfuehren(a, &kontext)), 0),
            Err(e) => (Antwort::fehler(format!("Die Tokio-Laufzeit lässt sich nicht starten: {e}")), 9),
        },
    };
    match serde_json::to_string(&antwort) {
        Ok(zeile) => zeigen(Strom::Aus, &format!("{zeile}\n"), true),
        Err(e) => {
            zeigen(Strom::Fehler, &format!("Antwort nicht schreibbar: {e}\n"), true);
            return 9;
        }
    }
    code
}

/// Einstellungen laden und das Protokoll einrichten (Bote und Wächter):
/// `<protokoll>\bote\`, sonst `%LOCALAPPDATA%\Dashboard\protokoll\` mit
/// einem Satz dazu in der ersten Zeile. Gibt die Einstellungen und den
/// Protokollordner zurück, oder den Exit-Code.
fn vorbereiten(datei: &std::path::Path) -> Result<(dashboard::dienst::einstellungen::Einstellungen, std::path::PathBuf), i32> {
    use dashboard::bote::{ersatz_protokoll, protokoll_ordner};
    use dashboard::dienst::{einstellungen::Einstellungen, protokoll};
    let e = match Einstellungen::laden(datei) {
        Ok(e) => e,
        Err(satz) => {
            // Ohne Einstellungen kein Protokollordner: neben der Datei versuchen.
            let neben = datei.parent().map(|p| p.join("protokoll")).unwrap_or_default();
            let (ordner, hinweis) = protokoll_ordner(&neben, ersatz_protokoll());
            protokoll::einrichten_mit_hinweis("bote", Some(&ordner), false, hinweis.as_deref());
            tracing::error!("{satz}");
            // Steht schon im Protokoll; hier nur noch in die Konsole, falls es eine gibt.
            zeigen(Strom::Fehler, &format!("{satz}\n"), false);
            return Err(2);
        }
    };
    let (ordner, hinweis) = protokoll_ordner(&e.pfade.protokoll, ersatz_protokoll());
    protokoll::einrichten_mit_hinweis("bote", Some(&ordner), false, hinweis.as_deref());
    Ok((e, ordner))
}

/// Der Bote mit Einstellungen.
#[cfg(windows)]
fn bote(datei: &std::path::Path) -> i32 {
    let (e, ordner) = match vorbereiten(datei) {
        Ok(x) => x,
        Err(code) => return code,
    };
    tracing::info!("Bote startet, Einstellungen aus {}: {}", e.datei.display(), e.kurz());
    // Nur in Tests gesetzt: ein Absturz gleich nach dem Start (für den Wächter).
    if let Some(code) = std::env::var(dashboard::bote::waechter::TEST_EXIT).ok().and_then(|c| c.parse::<i32>().ok()) {
        tracing::warn!("Bote endet absichtlich mit Code {code} ({} ist gesetzt).", dashboard::bote::waechter::TEST_EXIT);
        return code;
    }
    let code = dashboard::bote::server::laufen(&e);
    if code != 0 {
        let satz = format!("Der Bote ist mit Code {code} beendet; Einzelheiten im Protokoll {}.\n", ordner.display());
        zeigen(Strom::Fehler, &satz, false);
    }
    code
}

#[cfg(not(windows))]
fn bote(_datei: &std::path::Path) -> i32 {
    zeigen(Strom::Fehler, "Den Boten gibt es nur unter Windows.\n", false);
    2
}

/// Der Wächter: startet dieses Programm als Boten und bei Absturz neu.
fn waechter(datei: &std::path::Path) -> i32 {
    if let Err(code) = vorbereiten(datei) {
        return code;
    }
    let programm = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("Wächter: Das eigene Programm lässt sich nicht finden ({e}).");
            return 9;
        }
    };
    dashboard::bote::waechter::laufen(&programm, datei)
}
