//! Ende-zu-Ende: `dashboard.exe herzschlag` und `dashboard-herzschlag.exe`
//! als Kindprozess, wie der Stop-Hook sie aufruft (Eingabe auf stdin).
//! `LOCALAPPDATA` zeigt für das Kind in einen eigenen Temp-Ordner; gefragt wird
//! nie Port 8787, sondern ein freier Port ohne Server oder ein Testserver in
//! diesem Prozess.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Output, Stdio};
use std::thread::JoinHandle;
use std::time::Duration;

use dashboard::dienst::programme::eigener_befehl;

const PROGRAMM: &str = env!("CARGO_BIN_EXE_dashboard");
const HOOK: &str = env!("CARGO_BIN_EXE_dashboard-herzschlag");

struct Ort(PathBuf);

impl Ort {
    fn neu(name: &str) -> Ort {
        let o = std::env::temp_dir().join(format!("dashboard-herzschlag-e2e-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&o);
        std::fs::create_dir_all(&o).unwrap();
        std::fs::write(o.join("transkript.jsonl"), "{\"type\":\"user\",\"message\":{\"content\":\"frage\"}}\n").unwrap();
        Ort(o)
    }

    fn eingabe(&self) -> Vec<u8> {
        serde_json::to_vec(&serde_json::json!({
            "hook_event_name": "Stop",
            "session_id": "s-e2e",
            "transcript_path": self.0.join("transkript.jsonl"),
            "stop_hook_active": false
        }))
        .unwrap()
    }

    fn befehl(&self, programm: &str, args: &[&str]) -> Command {
        let mut c = eigener_befehl(programm);
        c.args(args).env("LOCALAPPDATA", &self.0).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        c
    }

    fn aufrufen(&self, args: &[&str], eingabe: &[u8]) -> Output {
        let mut a = vec!["herzschlag"];
        a.extend_from_slice(args);
        let kind = self.befehl(PROGRAMM, &a).spawn().expect("dashboard.exe startet");
        fertig(kind, eingabe)
    }
}

impl Drop for Ort {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Eingabe schreiben, stdin schließen, auf das Ende warten.
fn fertig(mut kind: Child, eingabe: &[u8]) -> Output {
    // Ein Kind, das vor dem Lesen endet, darf die Leitung schließen.
    let _ = kind.stdin.take().unwrap().write_all(eingabe);
    kind.wait_with_output().unwrap()
}

fn freier_port() -> String {
    TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().to_string()
}

fn still_durch(o: &Output, fall: &str) {
    assert_eq!(o.status.code(), Some(0), "{fall}: {o:?}");
    assert!(o.stdout.is_empty() && o.stderr.is_empty(), "{fall}: {o:?}");
}

/// Ein Dashboard mit offener Arbeit für genau eine Anfrage: Adresse und die
/// gelesene Anfrage.
fn dashboard_mit_arbeit() -> (String, JoinHandle<String>) {
    let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let server = l.local_addr().unwrap().to_string();
    let h = std::thread::spawn(move || {
        let (mut s, _) = l.accept().unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut puffer = [0u8; 16384];
        let n = s.read(&mut puffer).unwrap();
        let anfrage = String::from_utf8_lossy(&puffer[..n]).into_owned();
        let koerper = r#"{"sitzung":"s-e2e","von":"chat:dashboard","offen":[{"arbeit":"ar-1","nr":null,"titel":"Bauen","grund":"laeuft"}]}"#;
        write!(s, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{koerper}", koerper.len()).unwrap();
        anfrage
    });
    (server, h)
}

fn haelt_auf(o: &Output) {
    assert_eq!(o.status.code(), Some(2), "{o:?}");
    let grund = String::from_utf8_lossy(&o.stderr);
    assert!(grund.starts_with("HERZSCHLAG:") && grund.contains("ar-1 „Bauen“ (laeuft)"), "{grund}");
}

/// Ohne Dashboard (freier Port), mit kaputter Eingabe oder falschem Ereignis:
/// Exit 0 ohne Ausgabe. Die Zustandsdateien liegen unter LOCALAPPDATA.
#[test]
fn ohne_dashboard_und_bei_kaputter_eingabe_geht_der_stop_durch() {
    let ort = Ort::neu("durch");
    let server = freier_port();
    still_durch(&ort.aufrufen(&["--server", &server], &ort.eingabe()), "keine Verbindung");
    still_durch(&ort.aufrufen(&["--server", &server], b"not json{{"), "kein JSON");
    still_durch(&ort.aufrufen(&["--server", &server], br#"{"hook_event_name":"PreToolUse"}"#), "anderes Ereignis");
    still_durch(&ort.aufrufen(&["--server", &server], b""), "leer");
    let log = std::fs::read_to_string(ort.0.join("Dashboard").join("herzschlag").join("herzschlag.log")).unwrap();
    assert!(log.contains("\"Tracker\"") && log.contains("\"Payload\""), "{log}");
}

/// Ein falscher Aufruf hält keinen Chat fest: Exit 0, ein Satz auf stderr.
#[test]
fn falscher_aufruf_laesst_durch() {
    let ort = Ort::neu("aufruf");
    let o = ort.aufrufen(&["--unbekannt"], &ort.eingabe());
    assert_eq!(o.status.code(), Some(0), "{o:?}");
    assert!(String::from_utf8_lossy(&o.stderr).contains("herzschlag"), "{o:?}");
}

/// Ist stderr geschlossen (die Leseseite der Leitung ist zu), scheitert der
/// Satz eines falschen Aufrufs; das Programm endet trotzdem mit 0 (mit
/// `eprintln!` bräche es mit 101 ab).
#[test]
fn geschlossenes_stderr_fuehrt_nicht_zu_101() {
    let ort = Ort::neu("stderr");
    for (programm, args) in [(PROGRAMM, &["herzschlag", "--unbekannt"][..]), (HOOK, &["--unbekannt"][..])] {
        let (lesen, schreiben) = std::io::pipe().unwrap();
        drop(lesen);
        let kind = ort.befehl(programm, args).stderr(schreiben).spawn().unwrap();
        let o = fertig(kind, &ort.eingabe());
        assert_eq!(o.status.code(), Some(0), "{programm}: {o:?}");
    }
}

/// Gegenprobe: Antwortet ein Dashboard mit offener Arbeit, hält derselbe
/// Aufruf den Stop auf (Exit 2, Grund auf stderr).
#[test]
fn mit_offener_arbeit_haelt_er_auf() {
    let ort = Ort::neu("auf");
    let (server, h) = dashboard_mit_arbeit();
    let o = ort.aufrufen(&["--server", &server], &ort.eingabe());
    let anfrage = h.join().unwrap();
    assert!(anfrage.starts_with("GET /api/sitzungen/s-e2e/offen HTTP/1.1\r\n") && anfrage.contains("Authorization: Negotiate "), "{anfrage}");
    haelt_auf(&o);
}

/// Top-level-Fenster, die zum Prozess `pid` gehören (auch unsichtbare; ein
/// Konsolenfenster zählt zu dem Prozess, dem die Konsole gehört).
#[cfg(windows)]
fn fenster_von(pid: u32) -> usize {
    use windows_sys::Win32::Foundation::{HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId};
    use windows_sys::core::BOOL;

    unsafe extern "system" fn je(h: HWND, l: LPARAM) -> BOOL {
        // SAFETY: l zeigt auf das Paar unten, das während EnumWindows lebt.
        let (pid, zahl) = unsafe { &mut *(l as *mut (u32, usize)) };
        let mut p = 0u32;
        // SAFETY: h kommt von EnumWindows, p ist beschreibbar.
        unsafe { GetWindowThreadProcessId(h, &mut p) };
        if p == *pid {
            *zahl += 1;
        }
        1
    }
    let mut paar = (pid, 0usize);
    // SAFETY: Rückruf und Zeiger leben bis zum Ende des Aufrufs.
    unsafe { EnumWindows(Some(je), &raw mut paar as LPARAM) };
    paar.1
}

/// Die Probe zur Auflage „kein Konsolenfenster“: Startet ein Programm ohne
/// Konsole (etwa ein Chat-Client mit Fenster) ein Konsolenprogramm, bekommt
/// dieses eine neue Konsole samt Fenster; das stellt `CREATE_NEW_CONSOLE` hier
/// nach (dazu `DETACHED_PROCESS`, ganz ohne Konsole).
/// `dashboard-herzschlag.exe` hat dabei kein einziges Fenster, liest stdin,
/// schreibt den Grund auf stderr und endet mit 2. Gegenprobe: `dashboard.exe
/// herzschlag` bekommt unter `CREATE_NEW_CONSOLE` ein Konsolenfenster (hier
/// verborgen gestartet, damit beim Test nichts aufgeht).
#[cfg(windows)]
#[test]
fn hook_ohne_konsole_hat_kein_fenster() {
    use std::os::windows::process::CommandExt;
    use std::time::Instant;
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const SW_HIDE: u16 = 0;

    let ort = Ort::neu("fenster");
    for (programm, args, schalter, fenster_erwartet) in
        [(HOOK, &[][..], CREATE_NEW_CONSOLE, false), (HOOK, &[][..], DETACHED_PROCESS, false), (PROGRAMM, &["herzschlag"][..], CREATE_NEW_CONSOLE, true)]
    {
        let (server, h) = dashboard_mit_arbeit();
        let mut a = args.to_vec();
        a.extend(["--server", &server]);
        let kind = ort.befehl(programm, &a).creation_flags(schalter).show_window(SW_HIDE).spawn().expect("Programm startet");
        // Das Kind wartet auf stdin; so lange wird nach Fenstern gesucht.
        let bis = Instant::now() + Duration::from_millis(if fenster_erwartet { 5000 } else { 1500 });
        let mut gesehen = 0;
        while Instant::now() < bis && gesehen == 0 {
            gesehen = fenster_von(kind.id());
            std::thread::sleep(Duration::from_millis(50));
        }
        let o = fertig(kind, &ort.eingabe());
        h.join().unwrap();
        assert_eq!(gesehen > 0, fenster_erwartet, "{programm} mit Schalter {schalter:#x}: {gesehen} Fenster");
        haelt_auf(&o);
        assert!(o.stdout.is_empty(), "{o:?}");
    }
}
