//! Die Endpunkte des Mac-Helfers und ihre Leitung: TLS 1.3 auf dem eigenen Rechner.
//!
//! Wege ohne Schein: `POST /anmeldung`. Wege mit Schein: `POST /sperre`,
//! `POST /entsperrt`, `POST /lebenszeichen`, `POST /schluessel/angebot`,
//! `POST /schluessel/antwort`, `GET /schluessel`, `GET /stand`.
//!
//! Jeder Weg verlangt den belegten [`Aufrufer`], den nur diese Leitung
//! erzeugt: Am Web-Port oder an einem Lauscher ohne TLS läuft keiner.

use std::future::Future;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::{get, post};
use axum::serve::IncomingStream;
use axum::{Json, Router};
use rand_core::OsRng;
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::server::TlsStream;
use tracing::{error, info, warn};

use super::{Anfrage, BINDUNG, Beschreibung, Bund, Schein, SperrStand, Uebernahme};
use crate::passkey::geheim::Rahmen;
use crate::passkey::jwt::JwtSchluessel;

/// Die Liste der zugelassenen Helfer im Schlüsselordner: je Zeile ein
/// öffentlicher Schlüssel (Base64url) und ein Name.
pub const ZUGELASSEN_DATEI: &str = "helfer-zugelassen";
/// Das Geheimnis, aus dem Zertifikat und Schlüssel der Leitung entstehen.
pub const TLS_SCHLUESSEL_DATEI: &str = "helfer-tls-schluessel";
/// Der Fingerabdruck des Zertifikats, für den Helfer zum Vergleichen (Hexziffern).
pub const FINGERABDRUCK_DATEI: &str = "helfer-fingerabdruck";
/// So lange darf ein Handschlag dauern.
pub const HANDSCHLAG_FRIST: Duration = Duration::from_secs(5);
/// So viele Handschläge laufen höchstens zugleich; weitere Verbindungen werden geschlossen.
pub const HOECHSTENS_ZUGLEICH: usize = 8;
/// Längste Liste der zugelassenen Helfer, die gelesen wird.
const LISTE_HOECHSTENS: u64 = 64 * 1024;

/// Der Rahmen für das Geheimnis der Leitung im Schlüsselordner.
pub const RAHMEN_LEITUNG: Rahmen = Rahmen {
    folge: "Die Leitung des Mac-Helfers bleibt aus",
    folge_im_satz: "die Leitung des Mac-Helfers bleibt aus",
    name: "Schlüssel der Helfer-Leitung",
    nach_dem_entfernen: "dann entsteht ein neuer Schlüssel, und der Helfer braucht den neuen Fingerabdruck",
    fehlt: "Der Schlüssel der Helfer-Leitung fehlt; die Leitung des Mac-Helfers bleibt aus",
    ausnahme_bei_neuer_datei: true,
};

/// Der belegte Aufrufer: eine Verbindung über die TLS-Leitung, mit ihrem
/// Bindungswert (RFC 9266). Das Feld ist privat; den Wert erzeugt nur die Leitung.
#[derive(Debug, Clone)]
pub struct Aufrufer {
    bindung: [u8; BINDUNG],
}

/// Was die Endpunkte brauchen.
#[derive(Clone)]
pub struct HelferZustand {
    pub sperre: Arc<Mutex<SperrStand>>,
    /// Der Schlüsselbund; `None`, wenn der Server keine Schlüssel über den Helfer führt.
    pub schluessel: Option<Bund>,
    /// Der Schlüssel für die Scheine. Nur im Arbeitsspeicher: Nach einem
    /// Neustart gilt kein alter Schein mehr, und der Helfer meldet sich neu an.
    scheine: Arc<JwtSchluessel>,
    /// Der Schlüsselordner: Liste der zugelassenen Helfer und die Ablagen.
    ordner: PathBuf,
}

impl HelferZustand {
    pub fn neu(sperre: Arc<Mutex<SperrStand>>, schluessel: Option<Bund>, ordner: &Path) -> Self {
        Self { sperre, schluessel, scheine: Arc::new(JwtSchluessel::zufaellig()), ordner: ordner.to_path_buf() }
    }

    /// Der Name, unter dem `oeffentlich` zugelassen ist. Die Liste wird bei
    /// jeder Frage neu gelesen: Wer eine Zeile streicht, sperrt den Helfer sofort aus.
    async fn zugelassen(&self, oeffentlich: &[u8]) -> Option<String> {
        use tokio::io::AsyncReadExt;
        let datei = tokio::fs::File::open(self.ordner.join(ZUGELASSEN_DATEI)).await.ok()?;
        let mut liste = String::new();
        datei.take(LISTE_HOECHSTENS).read_to_string(&mut liste).await.ok()?;
        super::zugelassen(&liste, oeffentlich)
    }
}

/// Körper von `POST /anmeldung`.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Anmeldung {
    /// Öffentlicher Schlüssel des Helfers: P-256, 65 Bytes, Base64url.
    schluessel: String,
    /// ECDSA mit SHA-256 über Marke und Bindungswert dieser Verbindung, r und s, Base64url.
    signatur: String,
}

/// Antwort auf `POST /anmeldung`.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct AnmeldeAntwort {
    pub schein: String,
    /// Sekunden seit 1970.
    pub gilt_bis: i64,
}

/// Antwort auf `POST /lebenszeichen`.
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Lebenszeichen {
    /// Pfade ohne Ablage: Ein Helfer kann dafür den Schlüssel eines Chips anbieten.
    pub neu: Vec<String>,
    /// Offene Anfragen: Ablagen, die auf die Antwort ihres Chips warten.
    pub offen: Vec<Anfrage>,
}

/// Körper von `POST /schluessel/angebot`.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Angebot {
    pfad: String,
    name: String,
    etikett: String,
    /// Öffentlicher Schlüssel des Chips: P-256, 65 Bytes, Base64url.
    chip: String,
}

/// Körper von `POST /schluessel/antwort`.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Antwort {
    pfad: String,
    /// Was der Chip auf den Punkt der Anfrage geantwortet hat: 32 Bytes, Base64url.
    antwort: String,
}

/// Antwort auf Angebot und Antwort: was mit dem Pfad geschah.
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Ergebnis {
    pub pfad: String,
    pub stand: Uebernahme,
}

/// Antwort auf `GET /stand`.
#[derive(Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Stand {
    pub eingeschaltet: bool,
    pub gesperrt: bool,
}

/// Die Endpunkte des Helfers.
pub fn router(zustand: HelferZustand) -> Router {
    let mit_schein = Router::new()
        .route("/sperre", post(sperre))
        .route("/entsperrt", post(entsperrt))
        .route("/lebenszeichen", post(lebenszeichen))
        .route("/schluessel/angebot", post(angebot))
        .route("/schluessel/antwort", post(antwort))
        .route("/schluessel", get(schluessel_liste))
        .route("/stand", get(stand))
        .layer(axum::middleware::from_fn_with_state(zustand.clone(), schein_verlangen));
    Router::new().route("/anmeldung", post(anmeldung)).merge(mit_schein).with_state(zustand)
}

/// Die Schleuse vor allen Wegen außer der Anmeldung: ein gültiger Schein
/// eines Helfers, der noch zugelassen ist, auf einer Verbindung der Leitung.
async fn schein_verlangen(
    State(z): State<HelferZustand>,
    ConnectInfo(_wer): ConnectInfo<Aufrufer>,
    anfrage: Request,
    weiter: Next,
) -> Result<Response, StatusCode> {
    let token =
        anfrage.headers().get(header::AUTHORIZATION).and_then(|w| w.to_str().ok()).and_then(|w| w.strip_prefix("Bearer ")).ok_or(StatusCode::UNAUTHORIZED)?;
    let schein: Schein = z.scheine.lesen(token).map_err(|_| StatusCode::UNAUTHORIZED)?;
    let oeffentlich = crate::passkey::b64_lesen(&schein.kid).ok_or(StatusCode::UNAUTHORIZED)?;
    if !schein.gilt(super::wand_jetzt()) || z.zugelassen(&oeffentlich).await.is_none() {
        return Err(StatusCode::UNAUTHORIZED);
    }
    Ok(weiter.run(anfrage).await)
}

/// Der Helfer weist sich aus: mit einer Signatur über den Bindungswert
/// genau dieser Verbindung, von einem zugelassenen Schlüssel. Wer nicht
/// zugelassen ist und wer falsch signiert, bekommt dieselbe Antwort.
async fn anmeldung(
    State(z): State<HelferZustand>,
    ConnectInfo(wer): ConnectInfo<Aufrufer>,
    Json(k): Json<Anmeldung>,
) -> Result<Json<AnmeldeAntwort>, StatusCode> {
    let (Some(oeffentlich), Some(signatur)) = (crate::passkey::b64_lesen(&k.schluessel), crate::passkey::b64_lesen(&k.signatur)) else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let name = z.zugelassen(&oeffentlich).await;
    let (Some(name), true) = (name, super::signatur_gilt(&oeffentlich, &signatur, &wer.bindung)) else {
        warn!("Helfer-Leitung: Anmeldung abgewiesen (nicht zugelassen oder Signatur passt nicht zu dieser Verbindung).");
        return Err(StatusCode::FORBIDDEN);
    };
    let schein = Schein::neu(&oeffentlich, &crate::passkey::zufall(16), super::wand_jetzt());
    info!("Helfer-Leitung: Helfer angemeldet ({}).", if name.is_empty() { "ohne Namen" } else { "mit Namen in der Liste" });
    Ok(Json(AnmeldeAntwort { schein: z.scheine.signieren(&schein), gilt_bis: schein.exp }))
}

fn stand_von(z: &HelferZustand) -> std::sync::MutexGuard<'_, SperrStand> {
    z.sperre.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn bund_von(b: &Bund) -> std::sync::MutexGuard<'_, super::SchluesselBund> {
    b.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

async fn sperre(State(z): State<HelferZustand>) -> StatusCode {
    stand_von(&z).sperre(Instant::now(), super::wand_jetzt());
    info!("Helfer meldet: Der Rechner ist gesperrt. Alle Anmeldungen bis jetzt gelten nicht mehr.");
    StatusCode::NO_CONTENT
}

async fn entsperrt(State(z): State<HelferZustand>) -> StatusCode {
    stand_von(&z).entsperrt(Instant::now(), super::wand_jetzt());
    info!("Helfer meldet: Der Rechner ist entsperrt. Das Dashboard verlangt eine neue Anmeldung.");
    StatusCode::NO_CONTENT
}

async fn lebenszeichen(State(z): State<HelferZustand>) -> Json<Lebenszeichen> {
    stand_von(&z).zeichen(Instant::now());
    Json(match &z.schluessel {
        Some(bund) => {
            let mut bund = bund_von(bund);
            Lebenszeichen { neu: bund.neu(), offen: bund.anfragen(&mut OsRng) }
        }
        None => Lebenszeichen { neu: Vec::new(), offen: Vec::new() },
    })
}

/// Ein Helfer bietet für einen Pfad den öffentlichen Schlüssel eines Chips
/// an. Der Server würfelt den Schlüssel der Nutzlast, verschließt ihn daran
/// und legt die Ablage in den Schlüsselordner; erst dann gilt der Schlüssel.
async fn angebot(State(z): State<HelferZustand>, Json(k): Json<Angebot>) -> Result<Json<Ergebnis>, StatusCode> {
    let bund = z.schluessel.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    let chip = crate::passkey::b64_lesen(&k.chip).unwrap_or_default();
    let vorbereitet = bund_von(bund).vorbereiten(&k.pfad, &k.name, &k.etikett, &chip, &mut OsRng);
    let stand = match vorbereitet {
        Err(stand) => stand,
        Ok(v) => match ablage_schreiben(&z.ordner, v.ablage()) {
            Ok(()) => {
                let stand = bund_von(bund).einsetzen(v);
                info!("Helfer-Leitung: Schlüssel für eine Nutzlast angelegt und an einen Chip verschlossen.");
                stand
            }
            // Ein anderes Angebot war schneller: Seine Ablage bleibt, wie sie ist.
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Uebernahme::Vorhanden,
            Err(e) => {
                error!("Helfer-Leitung: Die Ablage ließ sich nicht schreiben ({e}); der Schlüssel gilt nicht.");
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        },
    };
    Ok(Json(Ergebnis { pfad: k.pfad, stand }))
}

/// Die Antwort des Chips auf eine offene Anfrage.
async fn antwort(State(z): State<HelferZustand>, Json(k): Json<Antwort>) -> Result<Json<Ergebnis>, StatusCode> {
    let bund = z.schluessel.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    let x = crate::passkey::b64_lesen(&k.antwort).unwrap_or_default();
    let stand = bund_von(bund).antworten(&k.pfad, &x);
    match stand {
        Uebernahme::Gesetzt => info!("Helfer-Leitung: Schlüssel einer Nutzlast mit dem Chip geöffnet."),
        Uebernahme::Falsch => warn!("Helfer-Leitung: Die Antwort öffnet die Ablage nicht (anderer Chip, falsche Antwort oder veränderte Ablage)."),
        _ => {}
    }
    Ok(Json(Ergebnis { pfad: k.pfad, stand }))
}

async fn schluessel_liste(State(z): State<HelferZustand>) -> Json<Vec<Beschreibung>> {
    Json(z.schluessel.as_ref().map(|b| bund_von(b).beschreiben()).unwrap_or_default())
}

async fn stand(State(z): State<HelferZustand>) -> Json<Stand> {
    let s = stand_von(&z);
    Json(Stand { eingeschaltet: s.eingeschaltet(), gesperrt: s.ist_gesperrt(Instant::now()) })
}

/// Schreibt eine Ablage in den Schlüsselordner: 0600, nie halb, nie über eine vorhandene hinweg.
fn ablage_schreiben(ordner: &Path, ablage: &super::Ablage) -> std::io::Result<()> {
    let text = serde_json::to_vec_pretty(ablage).map_err(std::io::Error::other)?;
    crate::passkey::geheim::ohne_halbstand_schreiben(ordner, &ordner.join(super::ablage_datei(&ablage.pfad)), &text)
}

/// Liest die Ablagen der erwarteten Pfade aus dem Schlüsselordner in den Bund.
fn ablagen_laden(ordner: &Path, bund: &Bund) {
    let mut bund = bund_von(bund);
    for pfad in bund.erwartet() {
        let datei = ordner.join(super::ablage_datei(&pfad));
        let gelesen = match std::fs::read(&datei) {
            Ok(bytes) => serde_json::from_slice::<super::Ablage>(&bytes).ok().filter(|a| a.pfad == pfad),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => None,
        };
        if !gelesen.is_some_and(|a| bund.ablage_laden(a)) {
            error!(
                "Helfer-Leitung: Die Ablage {} ist nicht lesbar. Der Schlüssel dieser Nutzlast bleibt aus, bis jemand nachsieht; ein neues Angebot wird nicht angenommen, solange die Datei da ist.",
                crate::datenschutz::verbergen(&datei.display().to_string())
            );
        }
    }
}

/// Die TLS-Leitung als Lauscher für axum: Die Handschläge laufen nebenher,
/// hier kommen nur fertige Verbindungen mit ihrem Bindungswert an.
struct TlsLauscher {
    fertig: tokio::sync::mpsc::Receiver<(TlsStream<TcpStream>, Aufrufer)>,
    /// Endet mit dem Lauscher.
    annahme: tokio::task::JoinHandle<()>,
}

impl Drop for TlsLauscher {
    fn drop(&mut self) {
        self.annahme.abort();
    }
}

impl axum::serve::Listener for TlsLauscher {
    type Io = TlsStream<TcpStream>;
    type Addr = Aufrufer;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        match self.fertig.recv().await {
            Some(verbindung) => verbindung,
            // Die Annahme ist beendet: Es kommt nichts mehr.
            None => std::future::pending().await,
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        // Einen Aufrufer gibt es nur aus einer echten Verbindung.
        Err(std::io::Error::other("die Helfer-Leitung nennt keinen eigenen Aufrufer"))
    }
}

impl axum::extract::connect_info::Connected<IncomingStream<'_, TlsLauscher>> for Aufrufer {
    fn connect_info(stream: IncomingStream<'_, TlsLauscher>) -> Self {
        stream.remote_addr().clone()
    }
}

/// Nimmt Verbindungen an und führt je Verbindung den Handschlag, mit Frist
/// und begrenzter Zahl. Nur Gegenseiten vom eigenen Rechner.
async fn annehmen(lauscher: TcpListener, tls: tokio_rustls::TlsAcceptor, fertig: tokio::sync::mpsc::Sender<(TlsStream<TcpStream>, Aufrufer)>) {
    let plaetze = Arc::new(tokio::sync::Semaphore::new(HOECHSTENS_ZUGLEICH));
    loop {
        let (tcp, von) = match lauscher.accept().await {
            Ok(v) => v,
            Err(e) => {
                warn!("Helfer-Leitung: Annehmen gescheitert ({e}).");
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
        };
        if !von.ip().is_loopback() {
            warn!("Helfer-Leitung: Verbindung abgewiesen, die Gegenseite ist nicht der eigene Rechner.");
            continue;
        }
        let Ok(platz) = plaetze.clone().try_acquire_owned() else {
            warn!("Helfer-Leitung: Verbindung abgewiesen, zu viele Handschläge zugleich.");
            continue;
        };
        let (tls, fertig) = (tls.clone(), fertig.clone());
        tokio::spawn(async move {
            let _platz = platz;
            match tokio::time::timeout(HANDSCHLAG_FRIST, tls.accept(tcp)).await {
                Ok(Ok(strom)) => {
                    let mut bindung = [0u8; BINDUNG];
                    // Ohne Bindungswert keine Verbindung: Die Anmeldung hinge sonst an nichts.
                    match strom.get_ref().1.export_keying_material(&mut bindung, super::BINDUNG_MARKE, Some(&[])) {
                        Ok(_) => {
                            let _ = fertig.send((strom, Aufrufer { bindung })).await;
                        }
                        Err(e) => warn!("Helfer-Leitung: kein Bindungswert ({e}); Verbindung geschlossen."),
                    }
                }
                Ok(Err(e)) => info!("Helfer-Leitung: Handschlag gescheitert ({e})."),
                Err(_) => info!("Helfer-Leitung: Handschlag nicht in der Frist; Verbindung geschlossen."),
            }
        });
    }
}

/// Die laufende Leitung. Beim Freigeben endet sie.
#[derive(Debug)]
pub struct Helfer {
    /// Wo die Leitung lauscht.
    pub adresse: SocketAddr,
    /// SHA-256 über das Zertifikat der Leitung.
    pub fingerabdruck: [u8; 32],
    aufgabe: tokio::task::JoinHandle<()>,
}

impl Drop for Helfer {
    fn drop(&mut self) {
        self.aufgabe.abort();
    }
}

/// Die Einstellungen für TLS: nur 1.3, nur reine Rust-Kryptografie, ohne
/// Zertifikat der Gegenseite (der Helfer weist sich danach aus, über die Bindung).
fn tls_einstellungen(paar: &super::zertifikat::Paar) -> Result<rustls::ServerConfig, String> {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
    let fehler = |e: rustls::Error| format!("TLS für die Helfer-Leitung ließ sich nicht einrichten ({e}). {}.", RAHMEN_LEITUNG.folge);
    rustls::ServerConfig::builder_with_provider(Arc::new(rustls_rustcrypto::provider()))
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(fehler)?
        .with_no_client_auth()
        .with_single_cert(vec![CertificateDer::from(paar.zertifikat.clone())], PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(paar.schluessel.to_vec())))
        .map_err(fehler)
}

/// Schreibt den Fingerabdruck in den Schlüsselordner (0600), damit der
/// Helfer ihn von dort nehmen kann. Öffentliches Wissen, aber nur für das eigene Konto nötig.
fn fingerabdruck_schreiben(ordner: &Path, fingerabdruck: &[u8; 32]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut datei = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(ordner.join(FINGERABDRUCK_DATEI))?;
    writeln!(datei, "{}", super::zertifikat::hex(fingerabdruck))
}

/// Startet die Leitung an `adresse`. Nur Adressen des eigenen Rechners. Das
/// Geheimnis der Leitung liegt im Schlüsselordner des Zustands und entsteht
/// beim ersten Start. Ist `ende` fertig, nimmt die Leitung nichts mehr an.
/// Muss in einer Tokio-Laufzeit aufgerufen werden.
pub fn starten(adresse: SocketAddr, ende: impl Future<Output = ()> + Send + 'static, zustand: HelferZustand) -> Result<Helfer, String> {
    if !adresse.ip().is_loopback() {
        return Err(format!("Die Helfer-Leitung lauscht nur auf dem eigenen Rechner, nicht auf {}. {}.", adresse.ip(), RAHMEN_LEITUNG.folge));
    }
    let geheim = crate::passkey::geheim::schluessel_datei(&zustand.ordner.join(TLS_SCHLUESSEL_DATEI), &RAHMEN_LEITUNG, true)?;
    let paar = super::zertifikat::ableiten(&geheim)?;
    let tls = tokio_rustls::TlsAcceptor::from(Arc::new(tls_einstellungen(&paar)?));
    let binden = |e: std::io::Error| format!("Die Helfer-Leitung ließ sich an {adresse} nicht binden ({e}). {}.", RAHMEN_LEITUNG.folge);
    let socket = if adresse.is_ipv4() { tokio::net::TcpSocket::new_v4() } else { tokio::net::TcpSocket::new_v6() }.map_err(binden)?;
    socket.set_reuseaddr(true).map_err(binden)?;
    socket.bind(adresse).map_err(binden)?;
    let lauscher = socket.listen(64).map_err(binden)?;
    let adresse = lauscher.local_addr().map_err(binden)?;
    fingerabdruck_schreiben(&zustand.ordner, &paar.fingerabdruck)
        .map_err(|e| format!("Der Fingerabdruck der Helfer-Leitung ließ sich nicht ablegen ({e}). {}.", RAHMEN_LEITUNG.folge))?;
    if let Some(bund) = &zustand.schluessel {
        ablagen_laden(&zustand.ordner, bund);
    }
    let (tx, rx) = tokio::sync::mpsc::channel(HOECHSTENS_ZUGLEICH);
    let annahme = tokio::spawn(annehmen(lauscher, tls, tx));
    let dienst = router(zustand).into_make_service_with_connect_info::<Aufrufer>();
    let aufgabe = tokio::spawn(async move {
        if let Err(e) = axum::serve(TlsLauscher { fertig: rx, annahme }, dienst).with_graceful_shutdown(ende).await {
            warn!("Helfer-Leitung beendet: {e}");
        }
    });
    info!("Helfer-Leitung: TLS 1.3 an {adresse}, Fingerabdruck {}.", super::zertifikat::hex(&paar.fingerabdruck));
    Ok(Helfer { adresse, fingerabdruck: paar.fingerabdruck, aufgabe })
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    use p256::NonZeroScalar;
    use p256::ecdsa::SigningKey;
    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
    use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
    use sha2::Digest;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;
    use crate::speicher::verschluesselung::{LINKS_PFAD, VOM_HELFER_FEHLT_SATZ};

    struct Ordner(PathBuf);

    impl Drop for Ordner {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn ordner(name: &str) -> Ordner {
        let o = std::env::temp_dir().join(format!("dh-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&o);
        std::fs::DirBuilder::new().mode(0o700).create(&o).unwrap();
        Ordner(o)
    }

    /// Prüft wie der Helfer: Es zählt nur der Fingerabdruck des Zertifikats,
    /// und dass die Gegenseite den Schlüssel dazu wirklich hat.
    #[derive(Debug)]
    struct Pin([u8; 32]);

    impl ServerCertVerifier for Pin {
        fn verify_server_cert(
            &self,
            blatt: &CertificateDer<'_>,
            _: &[CertificateDer<'_>],
            _: &ServerName<'_>,
            _: &[u8],
            _: UnixTime,
        ) -> Result<ServerCertVerified, rustls::Error> {
            if <[u8; 32]>::from(sha2::Sha256::digest(blatt.as_ref())) == self.0 {
                Ok(ServerCertVerified::assertion())
            } else {
                Err(rustls::Error::General("Fingerabdruck passt nicht".into()))
            }
        }

        fn verify_tls12_signature(
            &self,
            _: &[u8],
            _: &CertificateDer<'_>,
            _: &rustls::DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            Err(rustls::Error::General("nur TLS 1.3".into()))
        }

        fn verify_tls13_signature(
            &self,
            nachricht: &[u8],
            blatt: &CertificateDer<'_>,
            dss: &rustls::DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, rustls::Error> {
            rustls::crypto::verify_tls13_signature(nachricht, blatt, dss, &rustls_rustcrypto::provider().signature_verification_algorithms)
        }

        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            rustls_rustcrypto::provider().signature_verification_algorithms.supported_schemes()
        }
    }

    /// Eine Verbindung, wie der Helfer sie aufbaut, mit ihrem Bindungswert.
    struct Strom {
        tls: tokio_rustls::client::TlsStream<TcpStream>,
        bindung: [u8; BINDUNG],
    }

    async fn verbinden(adresse: SocketAddr, pin: [u8; 32]) -> std::io::Result<Strom> {
        let einstellungen = rustls::ClientConfig::builder_with_provider(Arc::new(rustls_rustcrypto::provider()))
            .with_protocol_versions(&[&rustls::version::TLS13])
            .unwrap()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(Pin(pin)))
            .with_no_client_auth();
        let tcp = TcpStream::connect(adresse).await?;
        let tls = tokio_rustls::TlsConnector::from(Arc::new(einstellungen)).connect(ServerName::try_from("localhost").unwrap(), tcp).await?;
        let mut bindung = [0u8; BINDUNG];
        tls.get_ref().1.export_keying_material(&mut bindung, crate::helfer::BINDUNG_MARKE, Some(&[])).unwrap();
        Ok(Strom { tls, bindung })
    }

    /// Eine HTTP-Anfrage auf einer bestehenden Verbindung; gibt Status und Körper zurück.
    async fn http(s: &mut Strom, methode: &str, pfad: &str, schein: Option<&str>, koerper: Option<&str>) -> (u16, String) {
        let mut anfrage = format!("{methode} {pfad} HTTP/1.1\r\nHost: localhost\r\n");
        if let Some(schein) = schein {
            anfrage.push_str(&format!("Authorization: Bearer {schein}\r\n"));
        }
        let koerper = koerper.unwrap_or_default();
        if !koerper.is_empty() {
            anfrage.push_str("Content-Type: application/json\r\n");
        }
        anfrage.push_str(&format!("Content-Length: {}\r\n\r\n{koerper}", koerper.len()));
        s.tls.write_all(anfrage.as_bytes()).await.unwrap();
        let lesen = async {
            let mut roh = Vec::new();
            let mut stueck = [0u8; 2048];
            loop {
                if let Some(ende) = roh.windows(4).position(|w| w == b"\r\n\r\n") {
                    let kopf = String::from_utf8_lossy(&roh[..ende]).to_string();
                    let laenge = kopf
                        .lines()
                        .find_map(|z| z.to_ascii_lowercase().strip_prefix("content-length:").map(|w| w.trim().parse::<usize>().unwrap()))
                        .unwrap_or(0);
                    if roh.len() >= ende + 4 + laenge {
                        let status = kopf.split(' ').nth(1).unwrap().parse().unwrap();
                        return (status, String::from_utf8_lossy(&roh[ende + 4..ende + 4 + laenge]).to_string());
                    }
                }
                let n = s.tls.read(&mut stueck).await.unwrap();
                assert!(n > 0, "die Verbindung endete mitten in der Antwort");
                roh.extend_from_slice(&stueck[..n]);
            }
        };
        tokio::time::timeout(Duration::from_secs(10), lesen).await.expect("die Antwort kommt")
    }

    /// Ein Helfer in Software: Schlüssel zum Anmelden und ein „Chip“.
    struct Probe {
        anmelden: SigningKey,
        oeffentlich: Vec<u8>,
        chip: NonZeroScalar,
    }

    impl Probe {
        fn neu() -> Self {
            let anmelden = SigningKey::random(&mut OsRng);
            let oeffentlich = anmelden.verifying_key().to_encoded_point(false).as_bytes().to_vec();
            Self { anmelden, oeffentlich, chip: NonZeroScalar::random(&mut OsRng) }
        }

        fn zeile(&self, name: &str) -> String {
            format!("{} {name}\n", crate::passkey::b64(&self.oeffentlich))
        }

        fn chip_oeffentlich(&self) -> String {
            crate::passkey::b64(&crate::verblendung::oeffentlich(&self.chip))
        }

        fn anmeldung(&self, bindung: &[u8; BINDUNG]) -> String {
            use p256::ecdsa::signature::Signer;
            let sig: p256::ecdsa::Signature = self.anmelden.sign(&crate::helfer::anmelde_nachricht(bindung));
            serde_json::json!({ "schluessel": crate::passkey::b64(&self.oeffentlich), "signatur": crate::passkey::b64(&sig.to_bytes()) }).to_string()
        }

        /// Meldet sich auf dieser Verbindung an und gibt den Schein zurück.
        async fn schein(&self, s: &mut Strom) -> String {
            let (status, k) = http(s, "POST", "/anmeldung", None, Some(&self.anmeldung(&s.bindung))).await;
            assert_eq!(status, 200, "{k}");
            serde_json::from_str::<AnmeldeAntwort>(&k).unwrap().schein
        }

        fn antwort(&self, a: &Anfrage) -> String {
            let x = crate::verblendung::chip_antwort(&self.chip, &crate::passkey::b64_lesen(&a.punkt).unwrap()).unwrap();
            serde_json::json!({ "pfad": a.pfad, "antwort": crate::passkey::b64(&x) }).to_string()
        }
    }

    fn zustand(o: &Ordner, bund: Option<Bund>) -> HelferZustand {
        HelferZustand::neu(Arc::new(Mutex::new(SperrStand::default())), bund, &o.0)
    }

    fn starte(z: HelferZustand) -> Helfer {
        starten("127.0.0.1:0".parse().unwrap(), std::future::pending(), z).unwrap()
    }

    fn zulassen(o: &Ordner, liste: &str) {
        std::fs::write(o.0.join(ZUGELASSEN_DATEI), liste).unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn anmeldung_ueber_die_bindung_gibt_einen_schein_und_die_wege_laufen() {
        let o = ordner("wege");
        let probe = Probe::neu();
        zulassen(&o, &probe.zeile("Schreibtisch"));
        let z = zustand(&o, None);
        let sperre = z.sperre.clone();
        sperre.lock().unwrap().einschalten(crate::helfer::wand_jetzt());
        let helfer = starte(z);
        let mut s = verbinden(helfer.adresse, helfer.fingerabdruck).await.unwrap();
        // Ohne Schein läuft nur die Anmeldung.
        for (methode, pfad) in [("GET", "/stand"), ("POST", "/sperre"), ("POST", "/entsperrt"), ("POST", "/lebenszeichen"), ("GET", "/schluessel")] {
            assert_eq!(http(&mut s, methode, pfad, None, None).await.0, 401, "{pfad}");
        }
        assert!(sperre.lock().unwrap().ist_gesperrt(Instant::now()), "ohne Lebenszeichen gesperrt");
        let schein = probe.schein(&mut s).await;
        let stand = |k: String| serde_json::from_str::<Stand>(&k).unwrap();
        assert_eq!(http(&mut s, "POST", "/lebenszeichen", Some(&schein), None).await, (200, r#"{"neu":[],"offen":[]}"#.to_string()));
        assert_eq!(stand(http(&mut s, "GET", "/stand", Some(&schein), None).await.1), Stand { eingeschaltet: true, gesperrt: false });
        assert_eq!(http(&mut s, "POST", "/sperre", Some(&schein), None).await.0, 204);
        assert_eq!(stand(http(&mut s, "GET", "/stand", Some(&schein), None).await.1), Stand { eingeschaltet: true, gesperrt: true });
        assert_eq!(http(&mut s, "POST", "/entsperrt", Some(&schein), None).await.0, 204);
        assert!(!sperre.lock().unwrap().ist_gesperrt(Instant::now()));
        // Ohne Schlüsselbund gibt es die Schlüsselwege nicht.
        assert_eq!(http(&mut s, "POST", "/schluessel/angebot", Some(&schein), Some(r#"{"pfad":"a","name":"n","etikett":"e","chip":"c"}"#)).await.0, 404);
        assert_eq!(http(&mut s, "GET", "/schluessel", Some(&schein), None).await, (200, "[]".to_string()));
        // Der Schein gilt auch auf einer neuen Verbindung zur selben Leitung …
        let mut zweite = verbinden(helfer.adresse, helfer.fingerabdruck).await.unwrap();
        assert_ne!(s.bindung, zweite.bindung, "je Verbindung ein eigener Bindungswert");
        assert_eq!(http(&mut zweite, "GET", "/stand", Some(&schein), None).await.0, 200);
        // … aber kein erfundener, kein verstümmelter, und keiner von einem anderen Schlüssel.
        let fremd = JwtSchluessel::zufaellig().signieren(&Schein::neu(&probe.oeffentlich, &[1u8; 16], crate::helfer::wand_jetzt()));
        for falsch in ["", "x.y.z", &schein[..schein.len() - 2], &fremd] {
            assert_eq!(http(&mut zweite, "GET", "/stand", Some(falsch), None).await.0, 401);
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn anmeldung_gilt_nur_fuer_ihre_verbindung_und_nur_fuer_zugelassene() {
        let o = ordner("anm");
        let (probe, fremd) = (Probe::neu(), Probe::neu());
        zulassen(&o, &format!("# zugelassene Helfer\n{}", probe.zeile("Schreibtisch")));
        let helfer = starte(zustand(&o, None));
        let mut a = verbinden(helfer.adresse, helfer.fingerabdruck).await.unwrap();
        let mut b = verbinden(helfer.adresse, helfer.fingerabdruck).await.unwrap();
        // Eine Anmeldung, die für Verbindung A signiert ist, taugt auf B nicht.
        let fuer_a = probe.anmeldung(&a.bindung);
        assert_eq!(http(&mut b, "POST", "/anmeldung", None, Some(&fuer_a)).await.0, 403);
        assert_eq!(http(&mut a, "POST", "/anmeldung", None, Some(&fuer_a)).await.0, 200);
        // Ein Schlüssel, der nicht in der Liste steht: dieselbe Antwort.
        let nicht_zugelassen = fremd.anmeldung(&b.bindung);
        assert_eq!(http(&mut b, "POST", "/anmeldung", None, Some(&nicht_zugelassen)).await.0, 403);
        // Zugelassener Schlüssel, Signatur von einem anderen.
        let gemischt = serde_json::json!({ "schluessel": crate::passkey::b64(&probe.oeffentlich), "signatur": serde_json::from_str::<serde_json::Value>(&nicht_zugelassen).unwrap()["signatur"] }).to_string();
        assert_eq!(http(&mut b, "POST", "/anmeldung", None, Some(&gemischt)).await.0, 403);
        // Unlesbares.
        assert_eq!(http(&mut b, "POST", "/anmeldung", None, Some(r#"{"schluessel":"kein base64!","signatur":"x"}"#)).await.0, 400);
        assert_eq!(http(&mut b, "POST", "/anmeldung", None, Some(r#"{"schluessel":"AA","signatur":"AA","mehr":1}"#)).await.0, 422);
        // Wer aus der Liste fällt, ist sofort draußen, auch mit gültigem Schein.
        let schein = probe.schein(&mut b).await;
        assert_eq!(http(&mut b, "GET", "/stand", Some(&schein), None).await.0, 200);
        zulassen(&o, "");
        assert_eq!(http(&mut b, "GET", "/stand", Some(&schein), None).await.0, 401);
        let erneut = probe.anmeldung(&b.bindung);
        assert_eq!(http(&mut b, "POST", "/anmeldung", None, Some(&erneut)).await.0, 403);
        // Ohne Liste ist niemand zugelassen.
        std::fs::remove_file(o.0.join(ZUGELASSEN_DATEI)).unwrap();
        assert_eq!(http(&mut b, "POST", "/anmeldung", None, Some(&erneut)).await.0, 403);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fingerabdruck_bleibt_ueber_neustarts_und_ein_falscher_bricht_ab() {
        let o = ordner("fp");
        let erster = starte(zustand(&o, None));
        let (adresse, fingerabdruck) = (erster.adresse, erster.fingerabdruck);
        let abgelegt = std::fs::read_to_string(o.0.join(FINGERABDRUCK_DATEI)).unwrap();
        assert_eq!(abgelegt.trim(), crate::helfer::zertifikat::hex(&fingerabdruck));
        for datei in [FINGERABDRUCK_DATEI, TLS_SCHLUESSEL_DATEI] {
            assert_eq!(std::fs::metadata(o.0.join(datei)).unwrap().permissions().mode() & 0o777, 0o600, "{datei}");
        }
        assert!(verbinden(adresse, [0u8; 32]).await.is_err(), "falscher Fingerabdruck: keine Verbindung");
        assert!(verbinden(adresse, fingerabdruck).await.is_ok());
        drop(erster);
        // Neustart mit demselben Schlüsselordner: derselbe Fingerabdruck.
        let zweiter = starte(zustand(&o, None));
        assert_eq!(zweiter.fingerabdruck, fingerabdruck);
        // Ein anderer Schlüsselordner: ein anderer.
        let o2 = ordner("fp2");
        assert_ne!(starte(zustand(&o2, None)).fingerabdruck, fingerabdruck);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn die_leitung_lauscht_nur_auf_dem_eigenen_rechner() {
        let o = ordner("nurlokal");
        let satz = starten("0.0.0.0:0".parse().unwrap(), std::future::pending(), zustand(&o, None)).unwrap_err();
        assert!(satz.contains("nur auf dem eigenen Rechner") && satz.contains(RAHMEN_LEITUNG.folge), "{satz}");
        assert!(!o.0.join(TLS_SCHLUESSEL_DATEI).exists(), "ohne Leitung entsteht nichts");
    }

    /// Der ganze Weg eines Schlüssels: Angebot, Ablage, Neustart, Anfrage,
    /// Antwort des Chips. Der Speicher kann die Links erst danach lesen, und
    /// über die Leitung geht nie der Schlüssel.
    #[tokio::test(flavor = "multi_thread")]
    async fn schluessel_der_links_ueber_angebot_ablage_und_verblendete_antwort() {
        let o = ordner("links");
        let probe = Probe::neu();
        zulassen(&o, &probe.zeile("Schreibtisch"));
        let bund = crate::helfer::bund_fuer(&[LINKS_PFAD]);
        let speicher = crate::speicher::Speicher::temporaer().mit_links_schluessel_vom_helfer(bund.clone());
        let helfer = starte(zustand(&o, Some(bund.clone())));
        let mut s = verbinden(helfer.adresse, helfer.fingerabdruck).await.unwrap();
        let schein = probe.schein(&mut s).await;
        let zeichen = |k: String| serde_json::from_str::<Lebenszeichen>(&k).unwrap();
        let ergebnis = |k: String| serde_json::from_str::<Ergebnis>(&k).unwrap().stand;
        // Vorher: Der Schlüssel fehlt, und das Lebenszeichen nennt den Pfad als neu.
        assert_eq!(speicher.link_verschliessen(b"https://beispiel.test/x").unwrap_err(), VOM_HELFER_FEHLT_SATZ);
        assert_eq!(
            zeichen(http(&mut s, "POST", "/lebenszeichen", Some(&schein), None).await.1),
            Lebenszeichen { neu: vec![LINKS_PFAD.to_string()], offen: vec![] }
        );
        // Falscher Körper: abgewiesen.
        for koerper in ["{}", "kein json", r#"{"pfad":"x"}"#] {
            assert_eq!(http(&mut s, "POST", "/schluessel/angebot", Some(&schein), Some(koerper)).await.0 / 100, 4, "{koerper}");
        }
        // Angebot: Der Server würfelt, verschließt an den Chip und legt die Ablage ab.
        let angebot = serde_json::json!({ "pfad": LINKS_PFAD, "name": "links", "etikett": "Aufgaben-Links", "chip": probe.chip_oeffentlich() }).to_string();
        let (status, k) = http(&mut s, "POST", "/schluessel/angebot", Some(&schein), Some(&angebot)).await;
        assert_eq!((status, ergebnis(k)), (200, Uebernahme::Gesetzt));
        let datei = o.0.join(crate::helfer::ablage_datei(LINKS_PFAD));
        assert_eq!(std::fs::metadata(&datei).unwrap().permissions().mode() & 0o777, 0o600);
        let erster = bund.lock().unwrap().holen(LINKS_PFAD).unwrap();
        let ablage = std::fs::read(&datei).unwrap();
        assert!(!ablage.windows(8).any(|w| w == &erster[..8]) && !String::from_utf8_lossy(&ablage).contains(&crate::passkey::b64(erster.as_slice())));
        let geheim = speicher.link_verschliessen(b"https://beispiel.test/x").unwrap();
        assert_eq!(speicher.link_oeffnen(&geheim).unwrap().as_slice(), b"https://beispiel.test/x");
        // Ein zweites Angebot tauscht nichts.
        assert_eq!(ergebnis(http(&mut s, "POST", "/schluessel/angebot", Some(&schein), Some(&angebot)).await.1), Uebernahme::Vorhanden);
        let liste: Vec<Beschreibung> = serde_json::from_str(&http(&mut s, "GET", "/schluessel", Some(&schein), None).await.1).unwrap();
        assert_eq!((liste[0].name.as_str(), liste[0].etikett.as_str(), liste[0].vorhanden), ("links", "Aufgaben-Links", true));
        drop(helfer);

        // Neustart des Servers: neuer Bund, neue Scheine; geblieben ist nur die Ablage.
        let bund = crate::helfer::bund_fuer(&[LINKS_PFAD]);
        let speicher = crate::speicher::Speicher::temporaer().mit_links_schluessel_vom_helfer(bund.clone());
        let helfer = starte(zustand(&o, Some(bund.clone())));
        let mut s = verbinden(helfer.adresse, helfer.fingerabdruck).await.unwrap();
        assert_eq!(http(&mut s, "POST", "/lebenszeichen", Some(&schein), None).await.0, 401, "der alte Schein gilt nach dem Neustart nicht");
        let schein = probe.schein(&mut s).await;
        assert_eq!(speicher.link_oeffnen(&geheim).unwrap_err(), VOM_HELFER_FEHLT_SATZ);
        let z = zeichen(http(&mut s, "POST", "/lebenszeichen", Some(&schein), None).await.1);
        assert!(z.neu.is_empty());
        assert_eq!(z.offen.len(), 1);
        assert_eq!((z.offen[0].pfad.as_str(), z.offen[0].chip.clone()), (LINKS_PFAD, probe.chip_oeffentlich()));
        // Ein anderer Chip öffnet nicht; die Anfrage ist danach verbraucht.
        let anderer = Probe::neu();
        assert_eq!(ergebnis(http(&mut s, "POST", "/schluessel/antwort", Some(&schein), Some(&anderer.antwort(&z.offen[0]))).await.1), Uebernahme::Falsch);
        assert_eq!(ergebnis(http(&mut s, "POST", "/schluessel/antwort", Some(&schein), Some(&probe.antwort(&z.offen[0]))).await.1), Uebernahme::Unerwartet);
        // Die nächste Anfrage ist eine neue; der richtige Chip öffnet, und der alte Link ist wieder lesbar.
        let z2 = zeichen(http(&mut s, "POST", "/lebenszeichen", Some(&schein), None).await.1);
        assert_ne!(z2.offen[0].punkt, z.offen[0].punkt);
        // Über die Leitung ging nur der verblendete Punkt, nie die Kapsel aus der Ablage.
        let kapsel = crate::passkey::b64_lesen(serde_json::from_slice::<crate::helfer::Ablage>(&ablage).unwrap().verschlossen.as_str()).unwrap()[..65].to_vec();
        assert_ne!(crate::passkey::b64_lesen(&z2.offen[0].punkt).unwrap(), kapsel);
        assert_eq!(ergebnis(http(&mut s, "POST", "/schluessel/antwort", Some(&schein), Some(&probe.antwort(&z2.offen[0]))).await.1), Uebernahme::Gesetzt);
        assert_eq!(*bund.lock().unwrap().holen(LINKS_PFAD).unwrap(), *erster);
        assert_eq!(speicher.link_oeffnen(&geheim).unwrap().as_slice(), b"https://beispiel.test/x");
        assert_eq!(zeichen(http(&mut s, "POST", "/lebenszeichen", Some(&schein), None).await.1), Lebenszeichen { neu: vec![], offen: vec![] });
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn unlesbare_ablage_bleibt_liegen_und_wird_nie_ueberschrieben() {
        let o = ordner("kaputt");
        let probe = Probe::neu();
        zulassen(&o, &probe.zeile(""));
        let datei = o.0.join(crate::helfer::ablage_datei(LINKS_PFAD));
        std::fs::write(&datei, b"kein json").unwrap();
        let bund = crate::helfer::bund_fuer(&[LINKS_PFAD]);
        let helfer = starte(zustand(&o, Some(bund.clone())));
        let mut s = verbinden(helfer.adresse, helfer.fingerabdruck).await.unwrap();
        let schein = probe.schein(&mut s).await;
        let angebot = serde_json::json!({ "pfad": LINKS_PFAD, "name": "links", "etikett": "Aufgaben-Links", "chip": probe.chip_oeffentlich() }).to_string();
        let (status, k) = http(&mut s, "POST", "/schluessel/angebot", Some(&schein), Some(&angebot)).await;
        assert_eq!((status, serde_json::from_str::<Ergebnis>(&k).unwrap().stand), (200, Uebernahme::Vorhanden));
        assert_eq!(std::fs::read(&datei).unwrap(), b"kein json");
        assert!(bund.lock().unwrap().holen(LINKS_PFAD).is_none());
    }

    #[tokio::test]
    async fn ohne_belegten_aufrufer_laeuft_kein_endpunkt() {
        use tower::ServiceExt;
        let o = ordner("ohne");
        let probe = Probe::neu();
        zulassen(&o, &probe.zeile(""));
        let z = zustand(&o, None);
        // Ein gültiger Schein hilft nicht, wenn die Anfrage nicht über die Leitung kam.
        let schein = z.scheine.signieren(&Schein::neu(&probe.oeffentlich, &[1u8; 16], crate::helfer::wand_jetzt()));
        for (methode, pfad) in [("POST", "/anmeldung"), ("GET", "/stand"), ("POST", "/sperre"), ("POST", "/lebenszeichen")] {
            let anfrage = axum::http::Request::builder()
                .method(methode)
                .uri(pfad)
                .header(header::AUTHORIZATION, format!("Bearer {schein}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(axum::body::Body::from(probe.anmeldung(&[0u8; BINDUNG])))
                .unwrap();
            let antwort = router(z.clone()).oneshot(anfrage).await.unwrap();
            assert_eq!(antwort.status(), StatusCode::INTERNAL_SERVER_ERROR, "{pfad}");
        }
        assert!(!z.sperre.lock().unwrap().ist_gesperrt(Instant::now()));
    }
}
