//! Ende-zu-Ende: `dashboard.exe herzschlag` als Kindprozess, wie der Stop-Hook
//! ihn aufruft (Eingabe auf stdin). `LOCALAPPDATA` zeigt für das Kind in einen
//! eigenen Temp-Ordner; gefragt wird nie Port 8787, sondern ein freier Port
//! ohne Server oder ein Testserver in diesem Prozess.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Output, Stdio};
use std::time::Duration;

use dashboard::dienst::programme::eigener_befehl;

const PROGRAMM: &str = env!("CARGO_BIN_EXE_dashboard");

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

    fn aufrufen(&self, args: &[&str], eingabe: &[u8]) -> Output {
        let mut kind = eigener_befehl(PROGRAMM)
            .arg("herzschlag")
            .args(args)
            .env("LOCALAPPDATA", &self.0)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("dashboard.exe startet");
        // Ein Kind, das vor dem Lesen endet, darf die Leitung schließen.
        let _ = kind.stdin.take().unwrap().write_all(eingabe);
        kind.wait_with_output().unwrap()
    }
}

impl Drop for Ort {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn freier_port() -> String {
    TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().to_string()
}

fn still_durch(o: &Output, fall: &str) {
    assert_eq!(o.status.code(), Some(0), "{fall}: {o:?}");
    assert!(o.stdout.is_empty() && o.stderr.is_empty(), "{fall}: {o:?}");
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

/// Gegenprobe: Antwortet ein Dashboard mit offener Arbeit, hält derselbe
/// Aufruf den Stop auf (Exit 2, Grund auf stderr).
#[test]
fn mit_offener_arbeit_haelt_er_auf() {
    let ort = Ort::neu("auf");
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
    let o = ort.aufrufen(&["--server", &server], &ort.eingabe());
    let anfrage = h.join().unwrap();
    assert!(anfrage.starts_with("GET /api/sitzungen/s-e2e/offen HTTP/1.1\r\n") && anfrage.contains("Authorization: Negotiate "), "{anfrage}");
    assert_eq!(o.status.code(), Some(2), "{o:?}");
    let grund = String::from_utf8_lossy(&o.stderr);
    assert!(grund.starts_with("HERZSCHLAG:") && grund.contains("ar-1 „Bauen“ (laeuft)"), "{grund}");
}
