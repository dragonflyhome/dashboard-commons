//! Ende-zu-Ende: `dashboard.exe --konsole --einstellungen <datei>` als
//! Kindprozess (Etappe 7a). Alle Pfade liegen in einem eigenen Temp-Ordner;
//! `USERPROFILE` zeigt für das Kind ebenfalls dorthin, damit der Zwischenspeicher
//! der Ordnergrößen nicht den echten überschreibt. Ports 8789 bis 8794.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use dashboard::dienst::programme::eigener_befehl;

const PROGRAMM: &str = env!("CARGO_BIN_EXE_dashboard");

/// Temp-Ordner mit dienst.toml; räumt beim Freigeben auf.
struct Ort {
    ordner: PathBuf,
}

impl Ort {
    fn neu(name: &str, port: u16) -> Ort {
        let ordner = std::env::temp_dir().join(format!("dashboard-e2e-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        std::fs::create_dir_all(ordner.join("profil")).unwrap();
        let p = |rest: &str| ordner.join(rest).display().to_string().replace('\\', "/");
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel");
        let text = format!(
            "schema = 1\nrepo = \"{}\"\nport = {port}\n\n[pfade]\ndaten = \"{}\"\nsicherung = \"{}\"\nprotokoll = \"{}\"\n",
            repo.display().to_string().replace('\\', "/"),
            p("daten/dashboard.redb"),
            p("sicherung"),
            p("protokoll"),
        );
        std::fs::write(ordner.join("dienst.toml"), text).unwrap();
        Ort { ordner }
    }

    fn starten(&self, stderr: Stdio) -> Child {
        eigener_befehl(PROGRAMM)
            .arg("--konsole")
            .arg("--einstellungen")
            .arg(self.ordner.join("dienst.toml"))
            .env("USERPROFILE", self.ordner.join("profil"))
            .env_remove("RUST_LOG")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(stderr)
            .spawn()
            .expect("dashboard.exe startet")
    }
}

impl Drop for Ort {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.ordner);
    }
}

/// Beendet das Kind auch dann, wenn eine Prüfung fehlschlägt.
struct Kind(Child);

impl Drop for Kind {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Einfache GET-Anfrage an 127.0.0.1; gibt Statuszeile und Körper zurück.
fn hole(port: u16, pfad: &str) -> Option<(String, String)> {
    hole_ueber(&format!("127.0.0.1:{port}"), pfad)
}

/// Dasselbe über eine beliebige eigene Adresse (`localhost:…`, `[::1]:…`);
/// sie steht auch im Host-Kopf.
fn hole_ueber(adresse: &str, pfad: &str) -> Option<(String, String)> {
    let mut s = TcpStream::connect(adresse).ok()?;
    s.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    write!(s, "GET {pfad} HTTP/1.1\r\nHost: {adresse}\r\nConnection: close\r\n\r\n").ok()?;
    let mut antwort = String::new();
    s.read_to_string(&mut antwort).ok()?;
    let (kopf, koerper) = antwort.split_once("\r\n\r\n")?;
    Some((kopf.lines().next()?.to_string(), koerper.to_string()))
}

/// Wartet höchstens 15 s auf 200 von /healthz.
fn warten_bis_bereit(port: u16, kind: &mut Child) {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(15) {
        if let Some((zeile, _)) = hole(port, "/healthz")
            && zeile.contains(" 200 ")
        {
            return;
        }
        if let Ok(Some(status)) = kind.try_wait() {
            panic!("dashboard.exe hat sich vorzeitig beendet: {status}");
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    panic!("/healthz antwortet nach 15 s nicht mit 200");
}

/// Körper einer Antwort mit `Transfer-Encoding: chunked` zusammensetzen.
fn entstuecken(koerper: &str) -> String {
    let mut rest = koerper;
    let mut aus = String::new();
    while let Some((laenge, danach)) = rest.split_once("\r\n") {
        let Ok(n) = usize::from_str_radix(laenge.trim(), 16) else { return koerper.to_string() };
        if n == 0 {
            break;
        }
        aus.push_str(&danach[..n]);
        rest = &danach[n + 2..];
    }
    aus
}

#[test]
fn konsole_mit_einstellungen_laeuft_und_protokolliert() {
    let port = 8789;
    let ort = Ort::neu("laeuft", port);
    let mut kind = Kind(ort.starten(Stdio::null()));
    warten_bis_bereit(port, &mut kind.0);

    let (zeile, koerper) = hole(port, "/api/betrieb").expect("Antwort von /api/betrieb");
    assert!(zeile.contains(" 200 "), "{zeile}");
    let koerper = if koerper.trim_start().starts_with('{') { koerper } else { entstuecken(&koerper) };
    assert!(koerper.contains("\"modus\":\"konsole\""), "{koerper}");
    // Mit Einstellungen ist der Bote vorgesehen; im Test läuft keiner (oder
    // einer, der die Prüfung nicht besteht: inhaber_sid fehlt).
    // Seit der Abnahme der Kopfleiste 2 frei nur „vorgesehen“ (verbunden/getrennt in /api/betrieb/voll).
    assert!(koerper.contains("\"bote\":\"vorgesehen\""), "{koerper}");
    // Kopfleiste, Fassung 2: frei ohne Pfad und ohne Sitzung (nur in /api/betrieb/voll).
    assert!(!koerper.contains("dienst.toml"), "Pfad auf dem freien Weg: {koerper}");
    assert!(koerper.contains("\"einstellungen\":null") && koerper.contains("\"sitzung\":null"), "{koerper}");

    drop(kind);
    let protokoll = ort.ordner.join("protokoll");
    let dateien: Vec<PathBuf> = std::fs::read_dir(&protokoll)
        .expect("Protokollordner angelegt")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("dienst-") && n.ends_with(".log")))
        .collect();
    assert_eq!(dateien.len(), 1, "{dateien:?}");
    let text = std::fs::read_to_string(&dateien[0]).unwrap();
    assert!(text.contains("Dashboard läuft"), "{text}");
    assert!(!text.contains('\u{1b}'), "keine Farben in der Datei");
}

/// Windows löst `localhost` zuerst als ::1 auf. Hört der Server dort nicht,
/// wartet jeder Client auf den Fehlschlag (curl 200 ms, .NET 2 s).
#[test]
fn beide_loopback_adressen_antworten_und_localhost_ist_schnell() {
    let port = 8792;
    let ort = Ort::neu("loopback", port);
    let mut kind = Kind(ort.starten(Stdio::null()));
    warten_bis_bereit(port, &mut kind.0);

    for adresse in [format!("127.0.0.1:{port}"), format!("[::1]:{port}")] {
        let antwort = hole_ueber(&adresse, "/healthz");
        assert!(antwort.as_ref().is_some_and(|(z, _)| z.contains(" 200 ")), "{adresse}: {antwort:?}");
    }
    // Einmal aufwärmen, dann messen: Verbindung über den Namen bis zur Antwort.
    let _ = hole_ueber(&format!("localhost:{port}"), "/healthz");
    let start = Instant::now();
    let antwort = hole_ueber(&format!("localhost:{port}"), "/healthz");
    let dauer = start.elapsed();
    assert!(antwort.as_ref().is_some_and(|(z, _)| z.contains(" 200 ")), "localhost: {antwort:?}");
    assert!(dauer < Duration::from_millis(50), "localhost dauert {dauer:?}");

    drop(kind);
    let protokoll = ort.ordner.join("protokoll");
    let datei = std::fs::read_dir(&protokoll).unwrap().filter_map(|e| e.ok()).next().expect("Protokolldatei").path();
    let text = std::fs::read_to_string(datei).unwrap();
    assert!(text.contains(&format!("http://127.0.0.1:{port}/ und http://[::1]:{port}/")), "Startzeile ohne beide Adressen: {text}");
    assert!(text.contains(&format!("Im Browser: http://localhost:{port}/")), "{text}");
}

#[test]
fn zweiter_start_auf_demselben_port_endet_mit_3() {
    let port = 8790;
    let ort = Ort::neu("belegt", port);
    let mut erster = Kind(ort.starten(Stdio::null()));
    warten_bis_bereit(port, &mut erster.0);

    let mut zweiter = ort.starten(Stdio::piped());
    let start = Instant::now();
    let status = loop {
        if let Some(s) = zweiter.try_wait().unwrap() {
            break s;
        }
        if start.elapsed() > Duration::from_secs(15) {
            let _ = zweiter.kill();
            panic!("der zweite Start endet nicht");
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let mut fehler = String::new();
    zweiter.stderr.take().unwrap().read_to_string(&mut fehler).unwrap();
    assert_eq!(status.code(), Some(3), "{fehler}");
    assert!(fehler.contains("Port 8790 ist schon belegt. Läuft das Dashboard bereits (Dienst oder Start-Dashboard.ps1)?"), "{fehler}");
    // Der erste läuft weiter.
    assert!(hole(port, "/healthz").is_some_and(|(z, _)| z.contains(" 200 ")));
}

/// Ist nur [::1] belegt, startet das Dashboard nicht (sonst landete `localhost`
/// bei dem fremden Programm): Exit 3 mit demselben Satz wie bei 127.0.0.1.
#[test]
fn nur_ipv6_belegt_endet_mit_3() {
    let port = 8793;
    // Rechner ohne IPv6: nichts zu prüfen.
    let Ok(fremd) = std::net::TcpListener::bind(("::1", port)) else { return };
    let ort = Ort::neu("ipv6-belegt", port);
    let mut kind = ort.starten(Stdio::piped());
    let start = Instant::now();
    let status = loop {
        if let Some(s) = kind.try_wait().unwrap() {
            break s;
        }
        if start.elapsed() > Duration::from_secs(15) {
            let _ = kind.kill();
            panic!("der Start endet nicht, obwohl [::1]:{port} belegt ist");
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let mut fehler = String::new();
    kind.stderr.take().unwrap().read_to_string(&mut fehler).unwrap();
    assert_eq!(status.code(), Some(3), "{fehler}");
    assert!(fehler.contains("Port 8793 ist schon belegt. Läuft das Dashboard bereits (Dienst oder Start-Dashboard.ps1)?"), "{fehler}");
    // 127.0.0.1 ist wieder frei (der Lauscher dort ist mit dem Prozess weg).
    assert!(std::net::TcpListener::bind(("127.0.0.1", port)).is_ok());
    drop(fremd);
}

/// Windows PowerShell 5.1 aus System32, wie es Dienst-Einrichten.ps1 nutzt.
#[cfg(windows)]
const POWERSHELL: &str = r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe";

/// Eine Anfrage wie die Skripte der Chats: `WindowsAnfrage` aus
/// `scripts\Anfrage.ps1` (.NET mit der Windows-Anmeldung dieses Kontos; das
/// curl 8.22 von Windows kann kein NTLM mehr). Mit `json` ein
/// POST, ohne `anmelden` ohne Nachweis. Adresse und Körper gehen als
/// Umgebungsvariablen hinein, damit nichts gequotet werden muss. Gibt den
/// HTTP-Status ('000' ohne Verbindung) und den Körper zurück.
#[cfg(windows)]
fn anfrage(url: &str, anmelden: bool, json: Option<&str>) -> (String, String) {
    let skript = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts").join("Anfrage.ps1");
    let befehl = format!(
        "[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false); \
         . '{}'; \
         $p = @{{ Url = $env:ANFRAGE_URL; Sekunden = 40 }}; \
         if ($env:ANFRAGE_JSON) {{ $p.Methode = 'POST'; $p.Json = $env:ANFRAGE_JSON }}; \
         if ($env:ANFRAGE_OHNE) {{ $p.OhneAnmeldung = $true }}; \
         $r = WindowsAnfrage @p; \
         [Console]::Out.Write($r.Text + \"`n\" + $r.Code)",
        skript.display()
    );
    let mut cmd = dashboard::dienst::programme::eigener_befehl(POWERSHELL);
    cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", &befehl])
        .env("ANFRAGE_URL", url)
        .env_remove("ANFRAGE_JSON")
        .env_remove("ANFRAGE_OHNE");
    if let Some(j) = json {
        cmd.env("ANFRAGE_JSON", j);
    }
    if !anmelden {
        cmd.env("ANFRAGE_OHNE", "1");
    }
    let aus = cmd.output().expect("powershell.exe startet");
    let text = String::from_utf8_lossy(&aus.stdout).to_string();
    let (koerper, status) = text.rsplit_once('\n').unwrap_or(("", &text));
    (status.trim().to_string(), format!("{koerper}{}", String::from_utf8_lossy(&aus.stderr)))
}

/// Neutraler Name der Variable mit der Sitzungskennung, nur für diese Probe.
#[cfg(windows)]
const PROBE_VARIABLE: &str = "DASHBOARD_PROBE_SITZUNGSKENNUNG";

/// Ein POST wie ein Chat: `scripts\Api.ps1` mit `-Json` (Windows PowerShell 5.1).
/// Die Kennung `kennung` steht immer in [`PROBE_VARIABLE`]; nur mit `benannt`
/// zeigt DASHBOARD_SITZUNG_VARIABLE darauf. Gibt Exit-Code und Ausgabe zurück.
#[cfg(windows)]
fn wie_ein_chat(port: u16, weg: &str, json: &str, kennung: &str, benannt: bool) -> (Option<i32>, String) {
    let skript = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts").join("Api.ps1");
    let mut cmd = eigener_befehl(POWERSHELL);
    cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&skript)
        .args([weg, "-Json", json, "-Server", &format!("127.0.0.1:{port}"), "-Sekunden", "40"])
        .env(PROBE_VARIABLE, kennung)
        .env_remove("DASHBOARD_SITZUNG_VARIABLE");
    if benannt {
        cmd.env("DASHBOARD_SITZUNG_VARIABLE", PROBE_VARIABLE);
    }
    let aus = cmd.output().expect("powershell.exe startet");
    (aus.status.code(), format!("{}{}", String::from_utf8_lossy(&aus.stdout), String::from_utf8_lossy(&aus.stderr)))
}

/// Windows-Anmeldung als Pflicht (7b), Ende zu Ende mit der echten
/// Verbindungskennung: ohne Nachweis 401, mit 200; ein Warter mit Nachweis
/// wird von einem neuen Vorgang (ebenfalls mit Nachweis) binnen 2 s nach dem
/// Anlegen geweckt. Projekte bleiben frei. Gefragt wird wie in den Skripten
/// über `WindowsAnfrage` (Windows PowerShell 5.1).
#[cfg(windows)]
#[test]
fn anmeldung_ist_pflicht_und_warten_wird_geweckt() {
    let port = 8794;
    let ort = Ort::neu("anmeldung", port);
    let mut kind = Kind(ort.starten(Stdio::null()));
    warten_bis_bereit(port, &mut kind.0);
    let basis = format!("http://localhost:{port}");

    let (status, koerper) = anfrage(&format!("{basis}/api/vorgaenge/uebersicht"), false, None);
    assert_eq!(status, "401", "ohne Anmeldung: {koerper}");
    assert!(koerper.contains("\"art\":\"anmeldung\""), "{koerper}");
    let (status, koerper) = anfrage(&format!("{basis}/api/vorgaenge/uebersicht"), true, None);
    assert_eq!(status, "200", "mit Anmeldung: {koerper}");
    let (status, koerper) = anfrage(&format!("http://127.0.0.1:{port}/api/vorgaenge/uebersicht"), true, None);
    assert_eq!(status, "200", "auch über 127.0.0.1: {koerper}");
    let (status, koerper) = anfrage(&format!("{basis}/api/projects"), false, None);
    assert_eq!(status, "200", "Projekte frei: {koerper}");

    // Warten mit Anmeldung, dann ein neuer Vorgang mit Anmeldung. PowerShell
    // braucht zum Starten bis zu einigen Sekunden; darum hat der Warter 4 s
    // Vorsprung, und gemessen wird ab dem Ende des Anlegens.
    let warten_url = format!("{basis}/api/vorgaenge/warten?fuer=e2e-anmeldung&timeout=30");
    let warter = std::thread::spawn(move || {
        let (status, koerper) = anfrage(&warten_url, true, None);
        (status, koerper, Instant::now())
    });
    std::thread::sleep(Duration::from_secs(4));
    assert!(!warter.is_finished(), "der Warter wartet");
    let (status, koerper) = anfrage(&format!("{basis}/api/vorgaenge"), true, Some(r#"{"art": "abnahme", "slug": "e2e-anmeldung", "titel": "Probe 7b"}"#));
    let angelegt = Instant::now();
    assert_eq!(status, "201", "anlegen: {koerper}");
    let (status, koerper, geweckt) = warter.join().unwrap();
    assert_eq!(status, "200", "{koerper}");
    assert!(koerper.contains("Probe 7b"), "{koerper}");
    // Der Warter endet oft schon, bevor das PowerShell des Anlegens beendet ist (dann 0).
    let dauer = geweckt.saturating_duration_since(angelegt);
    assert!(dauer < Duration::from_secs(2), "geweckt {dauer:?} nach dem Anlegen");

    // Herzschlag: Api.ps1 schickt die Kennung aus der Variable, die
    // DASHBOARD_SITZUNG_VARIABLE nennt; ein POST damit ordnet die Sitzung dem Chat
    // zu, und dashboard.exe herzschlag hält ihren Stop dann auf (Exit 2).
    let arbeit = r#"{"projekt": "dashboard", "von": "chat:dashboard", "titel": "Probe Herzschlag", "stand": "laeuft"}"#;
    let zugeordnet = || {
        let (status, koerper) = anfrage(&format!("{basis}/api/sitzungen/s-konsole/offen"), true, None);
        assert_eq!(status, "200", "{koerper}");
        serde_json::from_str::<serde_json::Value>(&koerper).unwrap()["von"].clone()
    };
    // Gegenprobe: dieselbe Kennung in der Variable, aber DASHBOARD_SITZUNG_VARIABLE fehlt.
    let (code, aus) = wie_ein_chat(port, "api/arbeiten", arbeit, "s-konsole", false);
    assert_eq!(code, Some(0), "Arbeit ohne Kennung anlegen: {aus}");
    assert_eq!(zugeordnet(), serde_json::Value::Null, "ohne DASHBOARD_SITZUNG_VARIABLE keine Kopfzeile");
    let (code, aus) = wie_ein_chat(port, "api/arbeiten", arbeit, "s-konsole", true);
    assert_eq!(code, Some(0), "Arbeit mit Kennung anlegen: {aus}");
    assert_eq!(zugeordnet(), serde_json::json!("chat:dashboard"), "mit DASHBOARD_SITZUNG_VARIABLE geht die Kopfzeile mit");
    let transkript = ort.ordner.join("transkript.jsonl");
    std::fs::write(&transkript, "{\"type\":\"user\",\"message\":{\"content\":\"frage\"}}\n").unwrap();
    let eingabe = serde_json::json!({"hook_event_name": "Stop", "session_id": "s-konsole", "transcript_path": transkript, "stop_hook_active": false});
    let mut hook = eigener_befehl(PROGRAMM)
        .args(["herzschlag", "--server", &format!("127.0.0.1:{port}")])
        .env("LOCALAPPDATA", &ort.ordner)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    hook.stdin.take().unwrap().write_all(&serde_json::to_vec(&eingabe).unwrap()).unwrap();
    let aus = hook.wait_with_output().unwrap();
    let grund = String::from_utf8_lossy(&aus.stderr);
    assert_eq!(aus.status.code(), Some(2), "{grund}");
    assert!(grund.contains("Probe Herzschlag"), "{grund}");

    drop(kind);
    let protokoll = ort.ordner.join("protokoll");
    let datei = std::fs::read_dir(&protokoll).unwrap().filter_map(|e| e.ok()).next().expect("Protokolldatei").path();
    let text = std::fs::read_to_string(datei).unwrap();
    assert!(text.contains("SID passt"), "{text}");
    assert!(!text.contains("S-1-5-21"), "nie eine SID im Protokoll");
}

#[test]
fn falsche_einstellungen_enden_mit_2() {
    let ort = Ort::neu("falsch", 8791);
    let datei = ort.ordner.join("dienst.toml");
    let text = std::fs::read_to_string(&datei).unwrap().replace("port = 8791", "port = 0");
    std::fs::write(&datei, text).unwrap();
    let aus = eigener_befehl(PROGRAMM).arg("--konsole").arg("--einstellungen").arg(&datei).output().unwrap();
    assert_eq!(aus.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&aus.stderr).contains("Bei `port` steht 0."));
    let aus = eigener_befehl(PROGRAMM).arg("--hilfe").output().unwrap();
    assert_eq!(aus.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&aus.stdout).contains("--konsole --einstellungen <datei>"));
    let aus = eigener_befehl(PROGRAMM).arg("--quatsch").output().unwrap();
    assert_eq!(aus.status.code(), Some(2));
}
