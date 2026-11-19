//! Schritt 7c der macOS-Folge: der Befehl `dashboard einmal-code` gegen
//! echte Kanäle, einen echten Server (`dienst::betreiben` mit Kanal) und
//! falsche Gegenseiten.

use std::future::Future;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::Extension;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use tokio::sync::watch;
use tower::ServiceExt;

use super::*;
use crate::anmeldung::verbindung::Verbindung;
use crate::api::passkey::{CodeAusgabe, SATZ_BELEG_OHNE_V6};
use crate::app::{AppState, beispiel_fuer_tests};
use crate::kanal::{CodeQuelle, KANAL_DATEI};

/// Ein Testordner mit 0700, der am Ende verschwindet. Kurzer Name: Der Pfad
/// eines Sockets hat auf dem Mac höchstens 104 Zeichen.
struct Ordner(PathBuf);

impl std::ops::Deref for Ordner {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Ordner {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o700));
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn ordner(name: &str) -> Ordner {
    let o = std::env::temp_dir().join(format!("dk-{}-ec-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&o);
    std::fs::DirBuilder::new().mode(0o700).create(&o).unwrap();
    Ordner(o)
}

fn eigene() -> u32 {
    crate::unix::eigene_uid()
}

fn freier_port() -> u16 {
    std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap().local_addr().unwrap().port()
}

/// Hat dieser Rechner [::1]?
fn mit_ipv6() -> bool {
    std::net::TcpListener::bind((Ipv6Addr::LOCALHOST, 0)).is_ok()
}

/// Nur der Kanal mit der Quelle des Dienstes, ohne TCP.
fn kanal(ort: &Path, state: &AppState) -> crate::kanal::Kanal {
    let quelle: Arc<dyn CodeQuelle> = Arc::new(CodeAusgabe(state.clone()));
    crate::kanal::starten(ort, eigene(), std::future::pending(), Some(quelle)).unwrap()
}

/// Ein echter Server (`betreiben`, beide Lauscher, Kanal im Ordner `o`);
/// `pruefung` bekommt Ort, Zustand und Port, sobald er bereit ist. Danach
/// endet der Server geordnet.
async fn mit_server<T, F: Future<Output = T>>(o: &Path, pruefung: impl FnOnce(PathBuf, AppState, u16) -> F) -> T {
    let ort = o.join(KANAL_DATEI);
    let port = freier_port();
    let (state, abschalter) = AppState::fuer_tests_mit_abschalter(&beispiel_fuer_tests(), port);
    let fuer_server = state.clone().mit_kanal_ort(ort.clone());
    let (bereit_tx, mut bereit_rx) = watch::channel(false);
    let (signal_tx, signal_rx) = watch::channel(None);
    let bereit = move || {
        bereit_tx.send_replace(true);
    };
    let server = crate::dienst::betreiben(port, move || Ok((fuer_server, abschalter)), &bereit, crate::dienst::stopp_abwarten(signal_rx), "Test");
    let pruefen = async move {
        let _ = bereit_rx.wait_for(|&b| b).await;
        let t = pruefung(ort, state, port).await;
        signal_tx.send_replace(Some(Duration::from_millis(200)));
        t
    };
    let (ergebnis, t) = tokio::time::timeout(Duration::from_secs(30), async { tokio::join!(server, pruefen) }).await.expect("Server und Prüfung enden");
    assert!(ergebnis.is_ok(), "{ergebnis:?}");
    t
}

/// `code/beginn` über den Router mit dem Code, wie ihn der Browser schickt.
async fn code_am_weg(state: &AppState, port: u16, code: &str) -> StatusCode {
    let app = crate::app::router(state.clone()).layer(Extension(ConnectInfo(Verbindung::neu().mit_gegenstelle(Ipv4Addr::LOCALHOST.into()))));
    let req = Request::post("/api/passkey/code/beginn")
        .header(header::HOST, format!("localhost:{port}"))
        .header(header::ORIGIN, format!("http://localhost:{port}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::json!({ "code": code, "name": "Laptop" }).to_string()))
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    let status = res.status();
    let _ = res.into_body().collect().await;
    status
}

/// Der Code aus den vier Zeilen.
fn code_aus(text: &str) -> String {
    text.lines().next().and_then(|z| z.strip_prefix("Einmal-Code: ")).unwrap_or_else(|| panic!("{text}")).to_string()
}

fn vorgaenge(state: &AppState) -> usize {
    state.speicher.as_ref().unwrap().vorgaenge_alle().unwrap().len()
}

fn namen_im(o: &Path) -> Vec<String> {
    let mut n: Vec<String> = std::fs::read_dir(o).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    n.sort();
    n
}

/// Der gute Weg gegen einen echten Server bis zur Ausgabe; der Code gilt am
/// Weg. Der Befehl schreibt keine Datei: Im Schlüsselordner liegt danach
/// nur der Socket.
#[tokio::test(flavor = "multi_thread")]
async fn guter_weg_gegen_echten_server_bis_zur_ausgabe_und_der_code_gilt_am_weg() {
    let o = ordner("gut");
    let ohne_v6 = !mit_ipv6();
    let (ergebnis, gilt, namen) = mit_server(&o, |ort, state, port| async move {
        let ergebnis = ablauf(&ort, eigene()).await;
        let gilt = match &ergebnis {
            Ok(text) => Some(code_am_weg(&state, port, &code_aus(text)).await),
            Err(_) => None,
        };
        let text = ergebnis.map(|t| (t, port));
        (text, gilt, namen_im(ort.parent().unwrap()))
    })
    .await;
    if ohne_v6 {
        // Rechner ohne [::1]: kein Code, der Satz des Kanals.
        assert_eq!(ergebnis.unwrap_err().satz, SATZ_BELEG_OHNE_V6);
        return;
    }
    let (text, port) = ergebnis.unwrap_or_else(|a| panic!("{a:?}"));
    let code = code_aus(&text);
    assert_eq!(text, ausgabe(&code, 600, port));
    assert_eq!(crate::passkey::einmalcode::normieren(&code).len(), 26);
    assert_eq!(gilt, Some(StatusCode::OK));
    assert_eq!(namen, [KANAL_DATEI]);
}

#[tokio::test(flavor = "multi_thread")]
async fn ohne_socket_oder_ordner_antwortet_nicht_sofort() {
    let o = ordner("ohne");
    let start = std::time::Instant::now();
    assert_eq!(ablauf(&o.join(KANAL_DATEI), eigene()).await, Err(Abbruch::mit(SATZ_ANTWORTET_NICHT)));
    assert_eq!(ablauf(&o.join("fehlt").join(KANAL_DATEI), eigene()).await, Err(Abbruch::mit(SATZ_ANTWORTET_NICHT)));
    assert!(start.elapsed() < Duration::from_secs(1), "nicht sofort: {:?}", start.elapsed());
}

#[tokio::test(flavor = "multi_thread")]
async fn abgewiesene_verbindung_an_einem_toten_socket_antwortet_nicht() {
    let o = ordner("tot");
    let ort = o.join(KANAL_DATEI);
    drop(std::os::unix::net::UnixListener::bind(&ort).unwrap());
    assert_eq!(ablauf(&ort, eigene()).await, Err(Abbruch::mit(SATZ_ANTWORTET_NICHT)));
}

/// Ein zu offener Ordner bricht vor jeder Verbindung ab: keine Frage, kein Code.
#[tokio::test(flavor = "multi_thread")]
async fn ordner_zu_offen_bricht_vor_jeder_anfrage_ab() {
    let o = ordner("offen");
    let ort = o.join(KANAL_DATEI);
    let state = AppState::fuer_tests(&beispiel_fuer_tests());
    let _kanal = kanal(&ort, &state);
    std::fs::set_permissions(&*o, std::fs::Permissions::from_mode(0o750)).unwrap();
    assert_eq!(ablauf(&ort, eigene()).await, Err(Abbruch::mit(SATZ_NICHT_SICHER)));
    assert!(!state.beleg.lock().unwrap().offen());
    assert!(!state.einmal_code.lock().unwrap().offen());
    assert_eq!(vorgaenge(&state), 0);
}

/// Gehört die Gegenseite einem anderen Konto (hier: der Befehl erwartet
/// eine andere Nummer), bricht er vor jeder Anfrage ab.
#[tokio::test(flavor = "multi_thread")]
async fn fremde_gegenseite_bricht_vor_jeder_anfrage_ab() {
    let o = ordner("fremd");
    let ort = o.join(KANAL_DATEI);
    let state = AppState::fuer_tests(&beispiel_fuer_tests());
    let _kanal = kanal(&ort, &state);
    assert_eq!(ablauf(&ort, eigene().wrapping_add(1)).await, Err(Abbruch::mit(SATZ_NICHT_SICHER)));
    assert!(!state.beleg.lock().unwrap().offen());
    assert_eq!(vorgaenge(&state), 0);
}

/// Hält der Server [::1] nicht, gibt der Befehl den Satz des Kanals unverändert weiter.
#[tokio::test(flavor = "multi_thread")]
async fn server_ohne_v6_der_satz_des_kanals_unveraendert_ende_9() {
    let o = ordner("ohne6");
    let ort = o.join(KANAL_DATEI);
    let state = AppState::fuer_tests(&beispiel_fuer_tests()).mit_v6(false);
    let _kanal = kanal(&ort, &state);
    let a = ablauf(&ort, eigene()).await.unwrap_err();
    assert_eq!((a.satz.as_str(), a.ende), (SATZ_BELEG_OHNE_V6, 9));
    assert_eq!(vorgaenge(&state), 0);
}

/// Antwortet unter [::1] ein anderes Programm (ein zweiter Lauscher, der
/// immer 204 sagt), scheitert der Beleg: kein Code, kein Vorgang „ausgegeben“.
#[tokio::test(flavor = "multi_thread")]
async fn zweiter_lauscher_unter_einer_adresse_beleg_gescheitert_kein_code() {
    if !mit_ipv6() {
        return;
    }
    let o = ordner("zweit");
    let ort = o.join(KANAL_DATEI);
    let v4 = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
    let port = v4.local_addr().unwrap().port();
    let fremd = tokio::net::TcpListener::bind((Ipv6Addr::LOCALHOST, port)).await.unwrap();
    let state = AppState::fuer_tests_mit_abschalter(&beispiel_fuer_tests(), port).0;
    // 127.0.0.1: dieses Dashboard.
    let dienst = crate::app::router(state.clone()).into_make_service_with_connect_info::<Verbindung>();
    let echt = tokio::spawn(async move { axum::serve(v4, dienst).await });
    // [::1]: ein anderes Programm, das jede Anfrage mit 204 beantwortet.
    let falsch = tokio::spawn(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        while let Ok((mut s, _)) = fremd.accept().await {
            let mut puffer = [0u8; 2048];
            let _ = s.read(&mut puffer).await;
            let _ = s.write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n").await;
        }
    });
    let _kanal = kanal(&ort, &state);
    assert_eq!(ablauf(&ort, eigene()).await, Err(Abbruch::mit(SATZ_BELEG_GESCHEITERT)));
    assert!(!state.einmal_code.lock().unwrap().offen());
    assert_eq!(vorgaenge(&state), 0);
    echt.abort();
    falsch.abort();
}

/// Antwortet unter einer Adresse niemand, scheitert der Beleg ebenso.
#[tokio::test(flavor = "multi_thread")]
async fn niemand_unter_dem_port_beleg_gescheitert() {
    let o = ordner("niemand");
    let ort = o.join(KANAL_DATEI);
    let state = AppState::fuer_tests_mit_abschalter(&beispiel_fuer_tests(), freier_port()).0;
    let _kanal = kanal(&ort, &state);
    assert_eq!(ablauf(&ort, eigene()).await, Err(Abbruch::mit(SATZ_BELEG_GESCHEITERT)));
    assert_eq!(vorgaenge(&state), 0);
}

/// Ein Kanal, der annimmt und schweigt: nach der Frist je Verbindung der Satz zur Frist.
#[tokio::test(flavor = "multi_thread")]
async fn stiller_kanal_nach_der_frist_der_satz_zur_frist() {
    let o = ordner("still");
    let ort = o.join(KANAL_DATEI);
    let lauscher = tokio::net::UnixListener::bind(&ort).unwrap();
    let halten = tokio::spawn(async move {
        let mut offen = Vec::new();
        while let Ok((v, _)) = lauscher.accept().await {
            offen.push(v);
        }
    });
    let start = std::time::Instant::now();
    assert_eq!(ablauf(&ort, eigene()).await, Err(Abbruch::mit(SATZ_FRIST)));
    let dauer = start.elapsed();
    assert!(dauer >= VERBINDUNG_FRIST - Duration::from_millis(200) && dauer < VERBINDUNG_FRIST + Duration::from_secs(2), "{dauer:?}");
    halten.abort();
}

/// Schreibt das Protokoll dieses Threads mit.
#[derive(Clone, Default)]
struct Mitschrift(Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Mitschrift {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Die längste Folge von Zeichen aus 0–9 und a–f im Text (eine Frage hätte 32).
fn laengste_hex_folge(text: &str) -> usize {
    let (mut laengste, mut jetzt) = (0, 0);
    for b in text.bytes() {
        jetzt = if b.is_ascii_digit() || (b'a'..=b'f').contains(&b) { jetzt + 1 } else { 0 };
        laengste = laengste.max(jetzt);
    }
    laengste
}

/// Im Protokoll des Servers steht keine Frage und kein Code, auch nicht
/// nach einem abgewiesenen Beleg. Läuft allein in einem eigenen Prozess
/// (`crate::test_allein`): Neben anderen Tests verlöre die Mitschrift Zeilen.
/// Dass sie mitliest, sichern die Zeilen des Belegs zu.
#[tokio::test]
async fn im_protokoll_des_servers_steht_keine_frage_und_kein_code() {
    if !crate::test_allein::im_eigenen_prozess("kanal::befehl::unix_tests::im_protokoll_des_servers_steht_keine_frage_und_kein_code") {
        return;
    }
    if !mit_ipv6() {
        return;
    }
    let mitschrift = Mitschrift::default();
    let schreiber = mitschrift.clone();
    let abonnent = tracing_subscriber::fmt().with_max_level(tracing::Level::TRACE).with_ansi(false).with_writer(move || schreiber.clone()).finish();
    let _wache = tracing::subscriber::set_default(abonnent);
    let o = ordner("protokoll");
    let (text, frage) = mit_server(&o, |ort, state, port| async move {
        let text = ablauf(&ort, eigene()).await.unwrap_or_else(|a| panic!("{a:?}"));
        // Dazu eine Frage, deren Beleg mit `Origin` abgewiesen wird.
        let zeile = crate::api::passkey::beleg_ausgeben(&state, &crate::kanal::konto_fuer_tests());
        let frage = serde_json::from_str::<serde_json::Value>(&zeile).unwrap()["frage"].as_str().unwrap().to_string();
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut s = tokio::net::TcpStream::connect((Ipv6Addr::LOCALHOST, port)).await.unwrap();
        let anfrage = beleg_anfrage(port, &frage).replacen("\r\n", &format!("\r\nOrigin: http://localhost:{port}\r\n"), 1);
        s.write_all(anfrage.as_bytes()).await.unwrap();
        let mut antwort = Vec::new();
        let _ = s.read_to_end(&mut antwort).await;
        assert_eq!(status_aus(&antwort), Some(403));
        (text, frage)
    })
    .await;
    let protokoll = String::from_utf8(mitschrift.0.lock().unwrap().clone()).unwrap();
    for zeile in
        ["Beleg der Adressen: Frage ausgegeben", "Beleg der Adressen: Anfrage angenommen", "Beleg der Adressen: Anfrage abgewiesen", "Einmal-Code ausgegeben"]
    {
        assert!(protokoll.contains(zeile), "die Mitschrift erfasst „{zeile}“ nicht: {protokoll}");
    }
    let code = code_aus(&text);
    assert!(!protokoll.contains(&frage) && !protokoll.contains(&code), "{protokoll}");
    assert!(!protokoll.to_uppercase().contains(&crate::passkey::einmalcode::normieren(&code)));
    assert!(laengste_hex_folge(&protokoll) < 32, "eine Frage im Protokoll? {protokoll}");
}
