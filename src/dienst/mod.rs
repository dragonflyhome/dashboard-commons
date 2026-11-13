//! Betrieb des Servers (Etappe 7a): die Aufrufe von `dashboard.exe`, der
//! Konsolen-Modus mit Einstellungsdatei, der Dienst-Modus für Windows und die
//! Lebenszeichen `/api/betrieb`.
//!
//! Einstellungen in `einstellungen`, Protokoll in Tagesdateien in `protokoll`,
//! der Handler für `/api/betrieb` in `betrieb`, die Anbindung an den
//! Dienststeuerungs-Manager (SCM) in `windows`, die festen Pfade externer
//! Programme (git, PowerShell, winget …) in `programme`. Die Zustandsfolge des Dienstes
//! (StartPending, Running, StopPending, Stopped) steht hier in `ablauf`, ohne
//! Windows, damit ein Test sie mit einer Attrappe prüfen kann.

pub mod betrieb;
pub mod einstellungen;
pub mod programme;
pub mod protokoll;
#[cfg(windows)]
pub mod windows;

use std::fmt;
use std::future::Future;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use tokio::sync::watch;
use tracing::{error, info, warn};

use crate::anmeldung::verbindung::Verbindung;
use crate::app::{self, AppState};
use einstellungen::Einstellungen;

/// Wie lange das geordnete Beenden höchstens auf offene Antworten wartet.
/// Abgeglichen mit der Frist von Windows (Auflage aus der Abnahme 7a1): Der
/// SCM wartet nach einem Stopp standardmäßig 20 s, danach braucht die Laufzeit
/// noch bis 2 s (`mit_laufzeit`), und Stopped muss vor Ablauf gemeldet sein.
/// 8 s lassen dafür reichlich Luft; lange offene Antworten (Warter, SSE) enden
/// ohnehin sofort mit dem Abschaltsignal. Die Wartezeit von StopPending in
/// `windows.rs` passt dazu.
///
/// Beim Herunterfahren von Windows gilt stattdessen `BEENDEN_BEIM_HERUNTERFAHREN`:
/// Dann wartet Windows nur `WaitToKillServiceTimeout` (auf diesem PC 5000 ms)
/// auf alle Dienste und beendet sie danach hart. 8 s + 2 s passten da nicht.
pub const BEENDEN_HOECHSTENS: Duration = Duration::from_secs(8);

/// Frist für offene Antworten beim Herunterfahren (`ServiceControl::Shutdown`):
/// 3 s, dazu bis 2 s für die Laufzeit (`mit_laufzeit`), bleibt knapp unter
/// den 5 s von `WaitToKillServiceTimeout`. Beim normalen Stopp bleiben 8 s.
pub const BEENDEN_BEIM_HERUNTERFAHREN: Duration = Duration::from_secs(3);

/// Warum der Dienst enden soll; bestimmt die Frist für offene Antworten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stoppart {
    /// Stopp über den SCM (`sc stop`, Dienste-Konsole): `BEENDEN_HOECHSTENS`.
    Stopp,
    /// Windows fährt herunter: `BEENDEN_BEIM_HERUNTERFAHREN`.
    Herunterfahren,
}

impl Stoppart {
    /// Wie lange höchstens auf offene Antworten gewartet wird.
    pub fn frist(self) -> Duration {
        match self {
            Stoppart::Stopp => BEENDEN_HOECHSTENS,
            Stoppart::Herunterfahren => BEENDEN_BEIM_HERUNTERFAHREN,
        }
    }
}

/// Wie der Server gestartet wurde.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modus {
    /// `--dienst --einstellungen <datei>`: Windows-Dienst.
    Dienst,
    /// `--konsole --einstellungen <datei>`: wie der Dienst, aber in der Konsole.
    Konsole,
    /// Ohne Argumente: Pfade aus Umgebungsvariablen und Repo (der Altweg).
    Umgebung,
}

impl Modus {
    pub fn wort(self) -> &'static str {
        match self {
            Modus::Dienst => "dienst",
            Modus::Konsole => "konsole",
            Modus::Umgebung => "umgebung",
        }
    }
}

/// Angaben zu diesem Lauf, für `/api/betrieb`.
#[derive(Debug, Clone)]
pub struct Lauf {
    pub modus: Modus,
    pub gestartet: chrono::DateTime<chrono::Local>,
}

impl Lauf {
    pub fn jetzt(modus: Modus) -> Lauf {
        Lauf { modus, gestartet: chrono::Local::now() }
    }
}

// ----- Aufrufe -----

/// Die Aufrufe, für `--hilfe`.
pub const HILFE: &str = "\
Aufrufe von dashboard.exe:

  dashboard.exe
      Server in der Konsole; Pfade aus Umgebungsvariablen (/anleitung, „Einstellungen“).
  dashboard.exe pruefe [<slug>|--alle]
      Projektseiten prüfen, ohne Server.
  dashboard.exe pruefe-zusammenbau <ordner> --code <code-repo> [--ohne-lock]
      Wächter der Zusammenbau-Kiste dashboard-lokal: liest nur, baut nichts.
      Läuft vor jedem cargo-Aufruf dort (Dienst.ps1 -Neustart). 0 grün, 1 rot.
  dashboard.exe herzschlag [--server 127.0.0.1:<port>]
      Stop-Hook eines Chats: Eingabe des Hooks auf stdin; 2 mit Grund auf stderr,
      solange die Sitzung im Dashboard offene Arbeit hat, sonst und bei jedem Fehler 0.
  dashboard.exe --konsole --einstellungen <datei>
      Server in der Konsole mit einer Einstellungsdatei (dienst.toml); Protokoll
      zusätzlich in den Protokollordner. Beenden mit Strg+C.
  dashboard.exe --dienst --einstellungen <datei> [--dienstname <name>]
      Nur für den Windows-Dienst (startet der Dienststeuerungs-Manager).
      Standard für --dienstname: Dashboard.
  dashboard.exe --hilfe
      Diese Hilfe.

Ende mit: 0 gut, 2 Einstellungen oder Aufruf falsch, 3 Port schon belegt, 9 anderer Fehler.
";

/// Was `dashboard.exe` tun soll.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Aufruf {
    Umgebung,
    Pruefe(Vec<String>),
    /// `dashboard pruefe-zusammenbau <ordner> --code <code-repo>`: der Wächter
    /// der Zusammenbau-Kiste (vor jedem cargo-Aufruf dort, Umzug des Inhalts).
    PruefeZusammenbau(Vec<String>),
    /// `dashboard herzschlag [--server …]`: Stop-Hook der Chats.
    Herzschlag(Vec<String>),
    Hilfe,
    Konsole {
        einstellungen: PathBuf,
    },
    Dienst {
        einstellungen: PathBuf,
        dienstname: String,
    },
    /// Falscher Aufruf, mit Satz.
    Falsch(String),
}

/// Liest die Argumente (ohne den Programmnamen).
pub fn aufruf_lesen(args: &[String]) -> Aufruf {
    match args.first().map(String::as_str) {
        None => return Aufruf::Umgebung,
        Some("pruefe") => return Aufruf::Pruefe(args[1..].to_vec()),
        Some("pruefe-zusammenbau") => return Aufruf::PruefeZusammenbau(args[1..].to_vec()),
        Some("herzschlag") => return Aufruf::Herzschlag(args[1..].to_vec()),
        Some("--hilfe" | "-h" | "--help" | "/?") => return Aufruf::Hilfe,
        _ => {}
    }
    let mut konsole = false;
    let mut dienst = false;
    let mut einstellungen = None;
    let mut dienstname = None;
    let mut i = 0;
    while i < args.len() {
        let wert = || args.get(i + 1).filter(|w| !w.starts_with("--")).cloned();
        match args[i].as_str() {
            "--konsole" => konsole = true,
            "--dienst" => dienst = true,
            "--einstellungen" => {
                let Some(w) = wert() else { return Aufruf::Falsch("Nach --einstellungen fehlt die Datei.".into()) };
                einstellungen = Some(PathBuf::from(w));
                i += 1;
            }
            "--dienstname" => {
                let Some(w) = wert() else { return Aufruf::Falsch("Nach --dienstname fehlt der Name.".into()) };
                dienstname = Some(w);
                i += 1;
            }
            anders => return Aufruf::Falsch(format!("Das Argument „{anders}“ kennt dashboard.exe nicht.")),
        }
        i += 1;
    }
    if konsole == dienst {
        return Aufruf::Falsch("Bitte genau eins angeben: --konsole oder --dienst.".into());
    }
    let Some(einstellungen) = einstellungen else {
        return Aufruf::Falsch("Es fehlt --einstellungen <datei>.".into());
    };
    if konsole {
        if dienstname.is_some() {
            return Aufruf::Falsch("--dienstname gilt nur zusammen mit --dienst.".into());
        }
        return Aufruf::Konsole { einstellungen };
    }
    Aufruf::Dienst { einstellungen, dienstname: dienstname.unwrap_or_else(|| "Dashboard".to_string()) }
}

// ----- Start und Ende -----

/// Warum der Server nicht starten konnte; `code` ist der Exit-Code
/// (im Dienst der dienstspezifische Code).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StartFehler {
    /// Einstellungsdatei fehlt oder ist falsch: 2.
    Einstellungen(String),
    /// Port belegt: 3.
    PortBelegt(u16),
    /// Alles andere: 9.
    Sonst(String),
}

impl StartFehler {
    pub fn code(&self) -> u32 {
        match self {
            StartFehler::Einstellungen(_) => 2,
            StartFehler::PortBelegt(_) => 3,
            StartFehler::Sonst(_) => 9,
        }
    }
}

impl fmt::Display for StartFehler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StartFehler::Einstellungen(s) | StartFehler::Sonst(s) => f.write_str(s),
            StartFehler::PortBelegt(port) => f.write_str(&port_satz(*port)),
        }
    }
}

/// Der Satz bei belegtem Port.
pub fn port_satz(port: u16) -> String {
    format!("Port {port} ist schon belegt. Läuft das Dashboard bereits (Dienst oder Start-Dashboard.ps1)?")
}

/// Die gebundenen Ports: 127.0.0.1 immer, [::1] wenn es geht. Windows löst
/// `localhost` zuerst als ::1 auf; lauscht dort niemand, wartet jeder Client
/// erst auf den Fehlschlag (curl 200 ms, .NET 2 s), bevor er 127.0.0.1 nimmt.
pub struct Lauscher {
    pub v4: tokio::net::TcpListener,
    pub v6: Option<tokio::net::TcpListener>,
}

impl Lauscher {
    /// Die Adressen als `http://…/`, zuerst IPv4.
    pub fn adressen(&self) -> Vec<String> {
        [Some(&self.v4), self.v6.as_ref()].into_iter().flatten().filter_map(|l| l.local_addr().ok()).map(|a| format!("http://{a}/")).collect()
    }
}

/// Warteschlange offener Verbindungen je Lauscher (wie bei `TcpListener::bind`).
const WARTESCHLANGE: u32 = 1024;

/// Bindet 127.0.0.1:<port> und [::1]:<port>, beide exklusiv. Belegt ist auf
/// beiden Adressen ein Startfehler (3): sonst landete `localhost` im Browser
/// (Windows nimmt zuerst ::1) bei einem fremden Programm. Geduldet wird nur
/// „kein IPv6 auf diesem Rechner“, dann mit Warnung nur 127.0.0.1.
pub async fn binden(port: u16) -> Result<Lauscher, StartFehler> {
    let v4 = lauscher_oeffnen(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).map_err(|e| match e.kind() {
        std::io::ErrorKind::AddrInUse => StartFehler::PortBelegt(port),
        _ => StartFehler::Sonst(format!("Port {port} lässt sich nicht öffnen ({e}).")),
    })?;
    // Derselbe Port wie bei IPv4, auch wenn `port` 0 war.
    let port = v4.local_addr().map(|a| a.port()).unwrap_or(port);
    let v6 = match lauscher_oeffnen(SocketAddr::from((Ipv6Addr::LOCALHOST, port))) {
        Ok(l) => Some(l),
        Err(e) if kein_ipv6(&e) => {
            warn!("[::1]:{port} gibt es hier nicht ({e}); das Dashboard hört nur auf 127.0.0.1, localhost kann langsam sein.");
            None
        }
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => return Err(StartFehler::PortBelegt(port)),
        Err(e) => return Err(StartFehler::Sonst(format!("Port {port} lässt sich auf [::1] nicht öffnen ({e})."))),
    };
    Ok(Lauscher { v4, v6 })
}

/// Fehler, die „kein IPv6 auf diesem Rechner“ bedeuten: Adresse nicht
/// vorhanden, nicht unterstützt, oder die Adressfamilie fehlt (WSAEAFNOSUPPORT
/// 10047, WSAEPFNOSUPPORT 10046).
fn kein_ipv6(e: &std::io::Error) -> bool {
    matches!(e.kind(), std::io::ErrorKind::AddrNotAvailable | std::io::ErrorKind::Unsupported)
        || (cfg!(windows) && matches!(e.raw_os_error(), Some(10046 | 10047)))
}

/// Öffnet einen Lauscher auf `addr`, unter Windows vor dem Binden mit
/// SO_EXCLUSIVEADDRUSE (kein anderes Programm kann sich auf dieselbe Adresse
/// legen). Gibt Windows die Option nicht her, nur eine Warnung.
fn lauscher_oeffnen(addr: SocketAddr) -> std::io::Result<tokio::net::TcpListener> {
    let socket = if addr.is_ipv4() { tokio::net::TcpSocket::new_v4()? } else { tokio::net::TcpSocket::new_v6()? };
    if let Err(e) = exklusiv(&socket) {
        warn!("{addr}: exklusives Binden geht nicht ({e}); weiter ohne.");
    }
    socket.bind(addr)?;
    socket.listen(WARTESCHLANGE)
}

/// Setzt SO_EXCLUSIVEADDRUSE.
#[cfg(windows)]
fn exklusiv(socket: &tokio::net::TcpSocket) -> std::io::Result<()> {
    use std::os::windows::io::AsRawSocket;
    use windows_sys::Win32::Networking::WinSock::{SO_EXCLUSIVEADDRUSE, SOCKET, SOL_SOCKET, setsockopt};
    let an: i32 = 1;
    // SAFETY: gültiger Socket (gehört `socket`), Zeiger und Länge auf ein i32,
    // das während des Aufrufs lebt.
    let r =
        unsafe { setsockopt(socket.as_raw_socket() as SOCKET, SOL_SOCKET, SO_EXCLUSIVEADDRUSE, (&an as *const i32).cast(), std::mem::size_of::<i32>() as i32) };
    if r == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
}

/// Außerhalb von Windows: nichts (dort bindet ohne SO_REUSEPORT ohnehin keiner doppelt).
#[cfg(not(windows))]
fn exklusiv(_socket: &tokio::net::TcpSocket) -> std::io::Result<()> {
    Ok(())
}

/// Bindet den Port, baut den Zustand, meldet `bereit` und bedient, bis
/// `signal` fertig ist. Dann erst das Abschaltsignal (Warter und SSE enden
/// sofort), danach höchstens die Frist, die `signal` liefert (beim Stopp
/// `BEENDEN_HOECHSTENS`, beim Herunterfahren kürzer), auf offene Antworten warten.
pub async fn betreiben(
    port: u16,
    zustand: impl FnOnce() -> anyhow::Result<(AppState, watch::Sender<bool>)>,
    bereit: &dyn Fn(),
    signal: impl Future<Output = Duration> + Send + 'static,
    hinweis: &str,
) -> Result<(), StartFehler> {
    // Erst binden: ist der Port belegt, wird weder der Datenspeicher geöffnet
    // noch ein Sammler gestartet.
    let lauscher = binden(port).await?;
    let (state, abschalter) = zustand().map_err(|e| StartFehler::Sonst(format!("Der Server lässt sich nicht aufbauen: {e:#}")))?;
    let adressen = lauscher.adressen().join(" und ");
    let port = lauscher.v4.local_addr().map_err(|e| StartFehler::Sonst(e.to_string()))?.port();
    let router = app::router(state);
    // Trägt nach dem Signal die Frist für offene Antworten.
    let (gesendet_tx, gesendet_rx) = watch::channel(None::<Duration>);
    // Jede angenommene Verbindung bekommt eine fortlaufende Nummer
    // (`ConnectInfo<Verbindung>`), für die Windows-Anmeldung je Verbindung (7b).
    let dienst = || router.clone().into_make_service_with_connect_info::<Verbindung>();
    // IPv4 wartet auf das Signal und gibt es weiter; IPv6 und der Deckel warten
    // auf die Weitergabe. So enden beide mit demselben Abschaltsignal.
    let v4 = axum::serve(lauscher.v4, dienst()).with_graceful_shutdown(async move {
        let frist = signal.await;
        abschalter.send_replace(true);
        gesendet_tx.send_replace(Some(frist));
        info!("Beende … (höchstens {} s für offene Antworten)", frist.as_secs());
    });
    // Wartet auf die Weitergabe und liefert die Frist. Ist der Sender ohne
    // Signal weg, ist der IPv4-Lauscher ohnehin zu Ende.
    let gesendet = stopp_abwarten;
    let v6 = lauscher.v6.map(|l| {
        let rx = gesendet_rx.clone();
        axum::serve(l, dienst()).with_graceful_shutdown(async move {
            gesendet(rx).await;
        })
    });
    let beide = async move {
        let v6 = async move {
            match v6 {
                Some(s) => s.await,
                None => Ok(()),
            }
        };
        let (a, b) = tokio::join!(async move { v4.await }, v6);
        a.and(b)
    };
    info!("Dashboard läuft: {adressen}  ({hinweis}). Im Browser: http://localhost:{port}/");
    bereit();
    let deckel = async move {
        let frist = gesendet(gesendet_rx).await;
        tokio::time::sleep(frist).await;
        frist
    };
    tokio::select! {
        r = beide => r.map_err(|e| StartFehler::Sonst(format!("Der Server ist mit einem Fehler beendet worden: {e}")))?,
        frist = deckel => warn!("Offene Antworten nach {} s abgebrochen.", frist.as_secs()),
    }
    info!("Dashboard beendet.");
    Ok(())
}

/// Führt `f` in einer eigenen Tokio-Laufzeit aus; Hintergrundarbeiten
/// (Ordnergrößen u. a.) bekommen danach höchstens 2 s.
pub fn mit_laufzeit<T>(f: impl Future<Output = Result<T, StartFehler>>) -> Result<T, StartFehler> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| StartFehler::Sonst(format!("Die Tokio-Laufzeit lässt sich nicht starten: {e}")))?;
    let ergebnis = rt.block_on(f);
    rt.shutdown_timeout(Duration::from_secs(2));
    ergebnis
}

/// Strg+C als Zukunft (bleibt offen, wenn es sich nicht abfangen lässt);
/// liefert die Frist für offene Antworten (`BEENDEN_HOECHSTENS`).
async fn strg_c() -> Duration {
    if tokio::signal::ctrl_c().await.is_err() {
        std::future::pending::<()>().await;
    }
    BEENDEN_HOECHSTENS
}

/// Exit-Code aus dem Ergebnis; den Satz bekommt die Konsole und das Protokoll.
fn ende(ergebnis: Result<(), StartFehler>) -> i32 {
    match ergebnis {
        Ok(()) => 0,
        Err(e) => {
            error!("{e}");
            eprintln!("{e}");
            e.code() as i32
        }
    }
}

/// `dashboard.exe` ohne Argumente: Umgebungsvariablen, Protokoll nur in der Konsole.
pub fn umgebung() -> i32 {
    protokoll::einrichten(None, true);
    let port: u16 = std::env::var("DASHBOARD_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(app::DEFAULT_PORT);
    ende(mit_laufzeit(betreiben(port, || AppState::aus_umgebung(port), &|| {}, strg_c(), "Beenden mit Strg+C")))
}

/// `dashboard.exe --konsole --einstellungen <datei>`.
pub fn konsole(datei: &Path) -> i32 {
    let e = match Einstellungen::laden(datei) {
        Ok(e) => e,
        Err(satz) => {
            eprintln!("{satz}");
            return 2;
        }
    };
    if let Some(p) = protokoll::einrichten(Some(&e.pfade.protokoll), true) {
        info!("Protokoll: {}", p.display());
    }
    info!("Einstellungen aus {}: {}", e.datei.display(), e.kurz());
    // Prüfung der Orte beim Bau (Profil und Ordner des Dienstes) und Verbergen von Profil und Namen: aus der SID, nie das Dienstkonto.
    crate::oeffnen::dienst_einrichten(&e);
    let zustand = || AppState::aus_einstellungen(&e).map(|(s, a)| (s.mit_modus(Modus::Konsole), a));
    ende(mit_laufzeit(betreiben(e.port, zustand, &|| {}, strg_c(), "Beenden mit Strg+C")))
}

// ----- Absturz -----

/// Text einer Panik (die Nutzlast ist meist `&str` oder `String`).
pub fn panik_text(nutzlast: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = nutzlast.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = nutzlast.downcast_ref::<String>() {
        s.clone()
    } else {
        "(ohne Text)".to_string()
    }
}

/// Schreibt jede Panik (auch in Hintergrundaufgaben, die Tokio sonst still
/// auffängt) mit Ort ins Protokoll; der bisherige Haken (stderr) läuft danach
/// weiter. Einmal je Prozess am Anfang von `main` setzen, für alle Modi.
pub fn panik_ins_protokoll() {
    let vorher = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let ort = info.location().map(|l| format!(" in {}:{}", l.file(), l.line())).unwrap_or_default();
        let faden = std::thread::current().name().map(|n| format!(", Faden {n}")).unwrap_or_default();
        error!("Absturz (Panik){ort}{faden}: {}", panik_text(info.payload()));
        vorher(info);
    }));
}

// ----- Dienst: Zustandsfolge -----

/// Zustände, die der Dienst dem SCM meldet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zustand {
    StartPending,
    Running,
    StopPending,
    /// Mit Exit-Code: 0 gut, sonst dienstspezifisch (2, 3, 9).
    Stopped(u32),
}

/// Wem der Dienst seinen Zustand meldet: dem SCM, im Test einer Attrappe.
pub trait Dienststatus: Send + Sync {
    fn melden(&self, zustand: Zustand);
}

/// Die Zustandsfolge: StartPending, dann `laufen` (das `bereit` ruft, sobald
/// der Port gebunden ist: Running), am Ende Stopped mit Code. Gibt den Code zurück.
/// Eine Panik in `laufen` endet als Stopped(9) mit einem Satz im Protokoll,
/// statt den Dienst ohne Meldung an den SCM verschwinden zu lassen.
pub fn ablauf(status: &dyn Dienststatus, laufen: impl FnOnce(&dyn Fn()) -> Result<(), StartFehler>) -> u32 {
    status.melden(Zustand::StartPending);
    let bereit = || status.melden(Zustand::Running);
    let ergebnis = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| laufen(&bereit)));
    let code = match ergebnis {
        Ok(Ok(())) => 0,
        Ok(Err(e)) => {
            error!("Der Dienst startet nicht (Code {}): {e}", e.code());
            e.code()
        }
        Err(nutzlast) => {
            let code = StartFehler::Sonst(String::new()).code();
            error!("Der Dienst ist abgestürzt und meldet Code {code}: {}", panik_text(&*nutzlast));
            code
        }
    };
    status.melden(Zustand::Stopped(code));
    code
}

/// Für den Steuer-Handler: Stopp oder Herunterfahren. Meldet StopPending und
/// löst das Ende aus; der Kanal trägt die Frist für offene Antworten
/// (`None`: läuft noch). Ein zweiter Aufruf (etwa Herunterfahren nach einem
/// Stopp) setzt die neue Frist, solange `betreiben` sie noch nicht gelesen hat.
pub fn stopp_anfordern(status: &dyn Dienststatus, stopp: &watch::Sender<Option<Duration>>, art: Stoppart) {
    status.melden(Zustand::StopPending);
    stopp.send_replace(Some(art.frist()));
}

/// Wartet, bis ein Stopp angefordert ist, und liefert dessen Frist (das
/// `signal` für `betreiben`). Ist der Sender weg, gilt `BEENDEN_HOECHSTENS`.
pub async fn stopp_abwarten(mut stopp: watch::Receiver<Option<Duration>>) -> Duration {
    match stopp.wait_for(Option::is_some).await {
        Ok(f) => f.unwrap_or(BEENDEN_HOECHSTENS),
        Err(_) => BEENDEN_HOECHSTENS,
    }
}

/// Was der Dienst zwischen StartPending und Stopped tut: Einstellungen,
/// Protokoll, Server bis zum Stopp.
pub fn dienst_laufen(datei: &Path, stopp: watch::Receiver<Option<Duration>>, bereit: &dyn Fn()) -> Result<(), StartFehler> {
    let e = match Einstellungen::laden(datei) {
        Ok(e) => e,
        Err(satz) => {
            // Ohne Einstellungen kein Protokollordner: neben der Datei versuchen
            // (C:/ProgramData/Dashboard/protokoll).
            let ersatz = datei.parent().map(|p| p.join("protokoll"));
            protokoll::einrichten(ersatz.as_deref(), false);
            return Err(StartFehler::Einstellungen(satz.0));
        }
    };
    protokoll::einrichten(Some(&e.pfade.protokoll), false);
    info!("Dienst startet, Einstellungen aus {}: {}", e.datei.display(), e.kurz());
    let signal = stopp_abwarten(stopp);
    // Prüfung der Orte beim Bau (Profil und Ordner des Dienstes) und Verbergen von Profil und Namen: aus der SID, nie das Dienstkonto.
    crate::oeffnen::dienst_einrichten(&e);
    let zustand = || AppState::aus_einstellungen(&e).map(|(s, a)| (s.mit_modus(Modus::Dienst), a));
    mit_laufzeit(betreiben(e.port, zustand, bereit, signal, "Windows-Dienst"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn args(s: &[&str]) -> Vec<String> {
        s.iter().map(|a| a.to_string()).collect()
    }

    #[test]
    fn aufrufe_lesen() {
        assert_eq!(aufruf_lesen(&[]), Aufruf::Umgebung);
        assert_eq!(aufruf_lesen(&args(&["pruefe", "--alle"])), Aufruf::Pruefe(args(&["--alle"])));
        assert_eq!(aufruf_lesen(&args(&["pruefe-zusammenbau", "o", "--code", "c"])), Aufruf::PruefeZusammenbau(args(&["o", "--code", "c"])));
        assert_eq!(aufruf_lesen(&args(&["--hilfe"])), Aufruf::Hilfe);
        assert_eq!(aufruf_lesen(&args(&["herzschlag", "--server", "127.0.0.1:8788"])), Aufruf::Herzschlag(args(&["--server", "127.0.0.1:8788"])));
        assert_eq!(
            aufruf_lesen(&args(&["--konsole", "--einstellungen", "C:/x/dienst.toml"])),
            Aufruf::Konsole { einstellungen: PathBuf::from("C:/x/dienst.toml") }
        );
        assert_eq!(
            aufruf_lesen(&args(&["--einstellungen", "d.toml", "--dienst"])),
            Aufruf::Dienst { einstellungen: PathBuf::from("d.toml"), dienstname: "Dashboard".into() }
        );
        assert_eq!(
            aufruf_lesen(&args(&["--dienst", "--einstellungen", "d.toml", "--dienstname", "Dashboard-Test"])),
            Aufruf::Dienst { einstellungen: PathBuf::from("d.toml"), dienstname: "Dashboard-Test".into() }
        );
        for falsch in [
            &["--konsole"][..],
            &["--konsole", "--dienst", "--einstellungen", "d.toml"],
            &["--konsole", "--einstellungen"],
            &["--konsole", "--einstellungen", "d.toml", "--dienstname", "X"],
            &["--irgendwas"],
        ] {
            assert!(matches!(aufruf_lesen(&args(falsch)), Aufruf::Falsch(_)), "{falsch:?}");
        }
    }

    #[test]
    fn codes_und_satz() {
        assert_eq!(StartFehler::Einstellungen("x".into()).code(), 2);
        assert_eq!(StartFehler::PortBelegt(8787).code(), 3);
        assert_eq!(StartFehler::Sonst("x".into()).code(), 9);
        assert_eq!(StartFehler::PortBelegt(8787).to_string(), "Port 8787 ist schon belegt. Läuft das Dashboard bereits (Dienst oder Start-Dashboard.ps1)?");
    }

    /// Merkt sich jede Meldung.
    #[derive(Default)]
    struct Attrappe(Mutex<Vec<Zustand>>);

    impl Dienststatus for Attrappe {
        fn melden(&self, z: Zustand) {
            self.0.lock().unwrap().push(z);
        }
    }

    #[test]
    fn dienst_meldet_start_lauf_stopp_und_ende() {
        let status = Arc::new(Attrappe::default());
        let (stopp_tx, stopp_rx) = watch::channel(None);
        // Der „Steuer-Handler“ läuft in einem anderen Faden, wie beim SCM.
        let handler = {
            let status = status.clone();
            std::thread::spawn(move || {
                while !status.0.lock().unwrap().contains(&Zustand::Running) {
                    std::thread::sleep(Duration::from_millis(5));
                }
                stopp_anfordern(&*status, &stopp_tx, Stoppart::Stopp);
            })
        };
        let code = ablauf(&*status, |bereit| {
            bereit();
            mit_laufzeit(async move {
                assert_eq!(stopp_abwarten(stopp_rx).await, BEENDEN_HOECHSTENS);
                Ok(())
            })
        });
        handler.join().unwrap();
        assert_eq!(code, 0);
        assert_eq!(*status.0.lock().unwrap(), [Zustand::StartPending, Zustand::Running, Zustand::StopPending, Zustand::Stopped(0)]);
    }

    /// Die Frist hängt an der Art des Stopps: beim Herunterfahren 3 s (Windows
    /// wartet dann nur 5 s), sonst 8 s. Das Signal für `betreiben` liefert sie.
    #[tokio::test]
    async fn frist_je_stoppart() {
        assert_eq!(Stoppart::Stopp.frist(), Duration::from_secs(8));
        assert_eq!(Stoppart::Herunterfahren.frist(), Duration::from_secs(3));
        // Frist und Laufzeit-Rest (2 s) bleiben unter WaitToKillServiceTimeout (5 s).
        assert!(BEENDEN_BEIM_HERUNTERFAHREN + Duration::from_secs(2) <= Duration::from_secs(5));
        for (art, frist) in [(Stoppart::Stopp, BEENDEN_HOECHSTENS), (Stoppart::Herunterfahren, BEENDEN_BEIM_HERUNTERFAHREN)] {
            let status = Attrappe::default();
            let (tx, rx) = watch::channel(None);
            let signal = tokio::spawn(stopp_abwarten(rx));
            tokio::task::yield_now().await;
            assert!(!signal.is_finished(), "ohne Stopp kein Signal");
            stopp_anfordern(&status, &tx, art);
            assert_eq!(signal.await.unwrap(), frist, "{art:?}");
            assert_eq!(*status.0.lock().unwrap(), [Zustand::StopPending]);
        }
        // Sender weg ohne Stopp: die normale Frist.
        let (tx, rx) = watch::channel(None);
        drop(tx);
        assert_eq!(stopp_abwarten(rx).await, BEENDEN_HOECHSTENS);
    }

    /// `betreiben` wartet nach dem Signal die gelieferte Frist (hier 500 ms)
    /// auf eine hängende Anfrage und endet dann, nicht erst nach 8 s.
    #[tokio::test(flavor = "multi_thread")]
    async fn betreiben_nimmt_die_frist_aus_dem_signal() {
        let port = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap().local_addr().unwrap().port();
        let (bereit_tx, mut bereit_rx) = watch::channel(false);
        let (signal_tx, signal_rx) = watch::channel(None);
        let bereit = move || {
            bereit_tx.send_replace(true);
        };
        let server = betreiben(port, || Ok(AppState::fuer_tests_mit_abschalter(&app::beispiel_fuer_tests(), port)), &bereit, stopp_abwarten(signal_rx), "Test");
        let pruefung = async move {
            let _ = bereit_rx.wait_for(|&b| b).await;
            // Eine halbe Anfrage hält eine Verbindung offen.
            use tokio::io::AsyncWriteExt;
            let mut halb = tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port)).await.unwrap();
            halb.write_all(b"GET /healthz HTTP/1.1\r\nHost: 127.0.0.1\r\n").await.unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;
            let start = std::time::Instant::now();
            signal_tx.send_replace(Some(Duration::from_millis(500)));
            (halb, start)
        };
        let (ergebnis, (halb, start)) =
            tokio::time::timeout(Duration::from_secs(20), async { tokio::join!(server, pruefung) }).await.expect("Server und Prüfung enden");
        assert!(ergebnis.is_ok(), "{ergebnis:?}");
        let dauer = start.elapsed();
        assert!(dauer >= Duration::from_millis(450) && dauer < Duration::from_secs(3), "Ende nach {dauer:?}");
        drop(halb);
    }

    #[test]
    fn dienst_endet_mit_eigenem_code_bei_startfehler() {
        for (fehler, code) in [(StartFehler::Einstellungen("kaputt".into()), 2), (StartFehler::PortBelegt(8787), 3), (StartFehler::Sonst("anders".into()), 9)] {
            let status = Attrappe::default();
            assert_eq!(ablauf(&status, |_| Err(fehler.clone())), code);
            assert_eq!(*status.0.lock().unwrap(), [Zustand::StartPending, Zustand::Stopped(code)]);
        }
    }

    #[test]
    fn panik_im_dienst_meldet_stopped_9() {
        let status = Attrappe::default();
        let code = ablauf(&status, |bereit| {
            bereit();
            panic!("absichtlich im Test");
        });
        assert_eq!(code, 9);
        assert_eq!(*status.0.lock().unwrap(), [Zustand::StartPending, Zustand::Running, Zustand::Stopped(9)]);
        assert_eq!(panik_text(&"x"), "x");
        assert_eq!(panik_text(&String::from("y")), "y");
        assert_eq!(panik_text(&5u8), "(ohne Text)");
    }

    /// Liest Kopf und Körper einer Antwort mit `Content-Length` (für 401) oder
    /// nur den Kopf (SSE); gibt den Kopf zurück. Die Verbindung bleibt offen.
    #[cfg(windows)]
    async fn antwort_lesen(s: &mut tokio::net::TcpStream) -> String {
        use tokio::io::AsyncReadExt;
        let mut puffer = Vec::new();
        let mut stueck = [0u8; 2048];
        let ende = loop {
            if let Some(i) = puffer.windows(4).position(|w| w == b"\r\n\r\n") {
                break i + 4;
            }
            let n = tokio::time::timeout(Duration::from_secs(5), s.read(&mut stueck)).await.unwrap().unwrap();
            assert!(n > 0, "Verbindung vorzeitig zu");
            puffer.extend_from_slice(&stueck[..n]);
        };
        let kopf = String::from_utf8_lossy(&puffer[..ende]).to_string();
        let laenge =
            kopf.lines().find_map(|z| z.to_ascii_lowercase().strip_prefix("content-length:").map(|l| l.trim().parse::<usize>().unwrap_or(0))).unwrap_or(0);
        let mut gelesen = puffer.len() - ende;
        while gelesen < laenge {
            let n = tokio::time::timeout(Duration::from_secs(5), s.read(&mut stueck)).await.unwrap().unwrap();
            assert!(n > 0, "Körper unvollständig");
            gelesen += n;
        }
        kopf
    }

    /// Beide Lauscher enden mit demselben Signal: ein offener SSE-Strom auf
    /// [::1] hält das Ende nicht auf (er endet mit dem Abschaltsignal).
    /// Seit 7b ist der Live-Kanal geschützt: Der Test meldet sich auf der
    /// Verbindung erst per Negotiate an (wie curl --negotiate), über die echte
    /// Verbindungskennung aus `betreiben`; eine zweite Verbindung bleibt 401.
    #[cfg(windows)]
    #[tokio::test(flavor = "multi_thread")]
    async fn beide_lauscher_enden_mit_dem_signal() {
        use crate::anmeldung::base64;
        use crate::anmeldung::sspi::client::Client;
        use tokio::io::AsyncWriteExt;
        // Freien Port suchen; betreiben bindet ihn gleich wieder.
        let port = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap().local_addr().unwrap().port();
        let (signal_tx, signal_rx) = tokio::sync::oneshot::channel::<()>();
        let (bereit_tx, mut bereit_rx) = watch::channel(false);
        let signal = async move {
            let _ = signal_rx.await;
            BEENDEN_HOECHSTENS
        };
        let bereit = move || {
            bereit_tx.send_replace(true);
        };
        let server = betreiben(port, || Ok(AppState::fuer_tests_mit_abschalter(&app::beispiel_fuer_tests(), port)), &bereit, signal, "Test");
        let pruefung = async move {
            let _ = bereit_rx.wait_for(|&b| b).await;
            // SSE über [::1], wenn es IPv6 gibt, sonst über 127.0.0.1.
            let (mut sse, host) = match tokio::net::TcpStream::connect((Ipv6Addr::LOCALHOST, port)).await {
                Ok(s) => (s, format!("[::1]:{port}")),
                Err(_) => (tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port)).await.unwrap(), format!("127.0.0.1:{port}")),
            };
            let anfrage = |nachweis: Option<&[u8]>| {
                let auth = nachweis.map(|n| format!("Authorization: Negotiate {}\r\n", base64::kodieren(n))).unwrap_or_default();
                format!("GET /api/events HTTP/1.1\r\nHost: {host}\r\nAccept: text/event-stream\r\n{auth}\r\n")
            };
            // Ohne Nachweis: 401.
            sse.write_all(anfrage(None).as_bytes()).await.unwrap();
            let kopf = antwort_lesen(&mut sse).await;
            assert!(kopf.starts_with("HTTP/1.1 401"), "{kopf}");
            // Aushandlung auf derselben Verbindung, bis 200.
            let mut client = Client::neu("HTTP/localhost").unwrap();
            let (mut nachweis, _) = client.schritt(None).unwrap();
            let kopf = loop {
                sse.write_all(anfrage(Some(&nachweis)).as_bytes()).await.unwrap();
                let kopf = antwort_lesen(&mut sse).await;
                if !kopf.starts_with("HTTP/1.1 401") {
                    break kopf;
                }
                let challenge =
                    kopf.lines().find_map(|z| z.strip_prefix("www-authenticate: Negotiate ")).unwrap_or_else(|| panic!("401 ohne Challenge: {kopf}"));
                nachweis = client.schritt(Some(&base64::dekodieren(challenge.trim()).unwrap())).unwrap().0;
            };
            assert!(kopf.starts_with("HTTP/1.1 200"), "{kopf}");
            assert!(kopf.contains("text/event-stream"), "{kopf}");
            // Eine andere Verbindung ist nicht angemeldet.
            let mut zweite = tokio::net::TcpStream::connect((Ipv4Addr::LOCALHOST, port)).await.unwrap();
            zweite.write_all(format!("GET /api/events HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n\r\n").as_bytes()).await.unwrap();
            let kopf = antwort_lesen(&mut zweite).await;
            assert!(kopf.starts_with("HTTP/1.1 401"), "{kopf}");
            let start = std::time::Instant::now();
            signal_tx.send(()).unwrap();
            // Der Strom bleibt offen, bis der Server endet.
            (sse, start)
        };
        let (ergebnis, (sse, start)) =
            tokio::time::timeout(Duration::from_secs(20), async { tokio::join!(server, pruefung) }).await.expect("Server und Prüfung enden");
        assert!(ergebnis.is_ok(), "{ergebnis:?}");
        assert!(start.elapsed() < Duration::from_secs(3), "Ende nach {:?}", start.elapsed());
        drop(sse);
    }

    #[tokio::test]
    async fn belegter_port_wird_erkannt() {
        let erster = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let port = erster.local_addr().unwrap().port();
        assert_eq!(binden(port).await.err(), Some(StartFehler::PortBelegt(port)));
    }

    #[tokio::test]
    async fn beide_loopback_adressen_und_nur_ipv6_belegt_ist_startfehler() {
        let l = binden(0).await.unwrap();
        let port = l.v4.local_addr().unwrap().port();
        let Some(v6) = &l.v6 else {
            // Rechner ohne IPv6: dann gibt es nur 127.0.0.1, das ist erlaubt.
            assert_eq!(l.adressen(), [format!("http://127.0.0.1:{port}/")]);
            return;
        };
        assert_eq!(v6.local_addr().unwrap(), SocketAddr::from((Ipv6Addr::LOCALHOST, port)));
        assert_eq!(l.adressen(), [format!("http://127.0.0.1:{port}/"), format!("http://[::1]:{port}/")]);
        // Nur [::1] belegt: Startfehler 3, 127.0.0.1 wird wieder frei.
        let v6 = l.v6.unwrap();
        drop(l.v4);
        assert_eq!(binden(port).await.err(), Some(StartFehler::PortBelegt(port)));
        drop(v6);
        let dritter = binden(port).await.unwrap();
        assert_eq!(dritter.v4.local_addr().unwrap().port(), port);
    }

    /// Ein fremdes Programm mit SO_REUSEADDR kommt nicht auf eine Adresse,
    /// die das Dashboard exklusiv hält.
    #[cfg(windows)]
    #[tokio::test]
    async fn exklusiv_gebunden() {
        let l = binden(0).await.unwrap();
        let addr = l.v4.local_addr().unwrap();
        let fremd = tokio::net::TcpSocket::new_v4().unwrap();
        fremd.set_reuseaddr(true).unwrap();
        assert!(fremd.bind(addr).is_err(), "SO_REUSEADDR darf die Adresse nicht übernehmen");
    }

    #[test]
    fn kein_ipv6_wird_erkannt() {
        use std::io::{Error, ErrorKind};
        assert!(kein_ipv6(&Error::from(ErrorKind::AddrNotAvailable)));
        assert!(kein_ipv6(&Error::from(ErrorKind::Unsupported)));
        assert!(!kein_ipv6(&Error::from(ErrorKind::AddrInUse)));
        assert!(!kein_ipv6(&Error::from(ErrorKind::PermissionDenied)));
        #[cfg(windows)]
        assert!(kein_ipv6(&Error::from_raw_os_error(10047)));
    }
}
