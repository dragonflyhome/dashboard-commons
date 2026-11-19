//! Schritt 7c der macOS-Folge: der Befehl `dashboard einmal-code`.
//!
//! Er zeigt im Terminal einen Einmal-Code zum Anlegen eines Passkeys. Er
//! meldet niemanden an und öffnet keinen Browser. Den Ort des Kanals findet
//! er mit denselben Funktionen wie der Server ([`crate::app::jwt_datei_fuer`],
//! dann [`super::ort_neben`]): ohne `--einstellungen` wie der Server ohne
//! Argumente, mit `--einstellungen <datei>` wie `--konsole` mit derselben Datei.
//!
//! Der Ablauf, je Schritt eine eigene Verbindung zum Kanal, jedes Mal mit
//! `peer_cred()` gleich der eigenen Benutzernummer:
//!
//! 1. den Schlüsselordner prüfen (`ordner_urteil`), dann `wer`;
//! 2. `beleg`: eine Frage und der Port;
//! 3. die Frage per `POST /api/passkey/code/beleg` einmal an 127.0.0.1 und
//!    einmal an [::1], je über eine eigene Verbindung mit Host
//!    `localhost:<port>` (ohne `localhost` aufzulösen); nur 204 gilt;
//! 4. `code` mit der Frage: Der Server gibt den Code nur, wenn die Frage auf
//!    beiden Adressen ankam.
//!
//! Jede Verbindung hat höchstens [`VERBINDUNG_FRIST`], der ganze Befehl
//! höchstens [`GANZE_FRIST`]; wiederholt wird nichts. Vor dem Erfolg steht
//! nichts auf stdout; der Code steht nur dort, nie auf stderr, in einer
//! Datei, im Protokoll oder in einer Adresse. Der Befehl startet keine
//! Mitschrift. Fehler sind ein bis zwei feste Sätze auf stderr. Ende: 0 gut,
//! 2 Aufruf oder Einstellungen falsch (unter Windows immer), 9 alles andere.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// Höchstens so lange je Verbindung (zum Kanal oder über TCP).
pub const VERBINDUNG_FRIST: Duration = Duration::from_secs(2);
/// Höchstens so lange für den ganzen Befehl.
pub const GANZE_FRIST: Duration = Duration::from_secs(10);

/// Kein Socket, oder die Verbindung wird abgewiesen.
pub const SATZ_ANTWORTET_NICHT: &str = "Das Dashboard antwortet nicht. Bitte das Dashboard starten und den Befehl noch einmal eingeben.";
/// Der Ordner ist zu offen, oder die Gegenseite gehört einem anderen Konto.
pub const SATZ_NICHT_SICHER: &str = "Das antwortende Programm ist nicht sicher das Dashboard dieses Kontos. Darum gibt es keinen Code. Bitte das Dashboard neu starten und den Befehl noch einmal eingeben.";
/// Der Beleg der zwei Adressen ist gescheitert.
pub const SATZ_BELEG_GESCHEITERT: &str =
    "Unter localhost antwortet nicht nur dieses Dashboard. Darum gibt es keinen Code. Bitte den Rechner neu starten und den Befehl noch einmal eingeben.";
/// Eine Frist ist abgelaufen.
pub const SATZ_FRIST: &str = "Das Dashboard hat nicht rechtzeitig geantwortet. Bitte den Befehl noch einmal eingeben.";

/// Höchstens so viele Bytes liest der Befehl als Antwortzeile des Kanals.
#[cfg(unix)]
const ZEILE_HOECHSTENS: usize = 4096;
/// Höchstens so viele Bytes liest er von einer HTTP-Antwort (nur die Statuszeile zählt).
#[cfg(unix)]
const STATUS_HOECHSTENS: usize = 1024;
/// Ein weitergegebener Satz des Kanals hat höchstens so viele Bytes.
const SATZ_HOECHSTENS: usize = 400;

/// Warum es keinen Code gibt: der Satz für stderr und das Ende.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Abbruch {
    pub satz: String,
    pub ende: i32,
}

impl Abbruch {
    /// Ende 9 mit einem festen Satz.
    pub fn mit(satz: &str) -> Self {
        Self { satz: satz.to_string(), ende: 9 }
    }
}

/// Der Ort des Kanals für diesen Aufruf. Ohne Einstellungsdatei wie der
/// Server ohne Argumente (Umgebung, sonst Standardort), mit Datei wie
/// `--konsole --einstellungen <datei>`. Eine Einstellungsdatei, die sich
/// nicht laden lässt, endet mit 2 und ihrem Satz, wie bei `--konsole`.
pub fn ort(einstellungen: Option<&Path>) -> Result<PathBuf, Abbruch> {
    let jwt = match einstellungen {
        None => crate::app::jwt_datei_fuer(None, &crate::app::daten_datei()),
        Some(datei) => {
            let e = crate::dienst::einstellungen::Einstellungen::laden(datei).map_err(|satz| Abbruch { satz: satz.0, ende: 2 })?;
            crate::app::jwt_datei_fuer(Some(&e), &e.pfade.daten)
        }
    };
    // Ohne Ort hat auch der Server keinen Kanal.
    crate::kanal::ort_neben(jwt.as_deref()).ok_or_else(|| Abbruch::mit(SATZ_ANTWORTET_NICHT))
}

/// „1 Minute“ oder „<n> Minuten“, aus den Sekunden der Antwort (abgerundet, mindestens 1).
fn dauer(sekunden: u64) -> String {
    match (sekunden / 60).max(1) {
        1 => "1 Minute".to_string(),
        n => format!("{n} Minuten"),
    }
}

/// Die vier Zeilen auf stdout nach dem Erfolg.
pub fn ausgabe(code: &str, gueltig_sekunden: u64, port: u16) -> String {
    format!(
        "Einmal-Code: {code}\nEr gilt {} und nur einmal.\nIm Browser auf diesem Rechner öffnen: http://localhost:{port}/anmelden\nDort unter „Passkey anlegen“ eingeben.\n",
        dauer(gueltig_sekunden)
    )
}

/// Die Anfrage des Belegs an eine der zwei Adressen, von Hand wie in
/// `herzschlag::abfrage`: ohne `Origin`, mit Host `localhost:<port>`.
pub fn beleg_anfrage(port: u16, frage: &str) -> String {
    let koerper = serde_json::json!({ "frage": frage }).to_string();
    format!(
        "POST {} HTTP/1.1\r\nHost: localhost:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{koerper}",
        crate::sicherheit::BELEG_WEG,
        koerper.len()
    )
}

/// Der Status aus der Statuszeile einer HTTP-Antwort (`HTTP/1.x <code> …`).
/// `None`, solange die Zeile nicht vollständig oder nicht lesbar ist.
pub fn status_aus(roh: &[u8]) -> Option<u16> {
    let ende = roh.windows(2).position(|w| w == b"\r\n")?;
    let zeile = std::str::from_utf8(&roh[..ende]).ok()?;
    let mut teile = zeile.split(' ');
    let fassung = teile.next()?;
    if !fassung.starts_with("HTTP/1.") {
        return None;
    }
    let code = teile.next()?;
    if code.len() != 3 {
        return None;
    }
    code.parse().ok()
}

/// Eine Antwortzeile des Kanals als Objekt. Ein Satz des Kanals (`fehler`)
/// geht unverändert weiter, nur ohne Steuerzeichen und gekürzt; alles, was
/// kein Objekt ist, heißt: Die Gegenseite ist nicht sicher das Dashboard.
pub fn kanal_antwort(zeile: &[u8]) -> Result<serde_json::Map<String, serde_json::Value>, Abbruch> {
    let Ok(serde_json::Value::Object(felder)) = serde_json::from_slice::<serde_json::Value>(zeile) else {
        return Err(Abbruch::mit(SATZ_NICHT_SICHER));
    };
    if let Some(satz) = felder.get("fehler") {
        let satz = satz.as_str().ok_or_else(|| Abbruch::mit(SATZ_NICHT_SICHER))?;
        return Err(Abbruch::mit(&crate::herzschlag::abfrage::kuerzen(satz, SATZ_HOECHSTENS)));
    }
    Ok(felder)
}

/// Schreibt das Ergebnis: den Erfolg nur auf `aus` (stdout), einen Abbruch
/// nur auf `fehler` (stderr). Gibt das Ende zurück.
pub fn melden(ergebnis: Result<String, Abbruch>, aus: &mut dyn std::io::Write, fehler: &mut dyn std::io::Write) -> i32 {
    match ergebnis {
        Ok(text) => {
            let _ = aus.write_all(text.as_bytes());
            let _ = aus.flush();
            0
        }
        Err(a) => {
            let _ = writeln!(fehler, "{}", a.satz);
            a.ende
        }
    }
}

#[cfg(unix)]
pub use lauf::{ablauf, ausfuehren};

/// Ein- und Ausgabe des Befehls.
#[cfg(unix)]
mod lauf {
    use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::{TcpStream, UnixStream};
    use tokio::time::{Instant, timeout_at};

    use super::{
        Abbruch, GANZE_FRIST, SATZ_ANTWORTET_NICHT, SATZ_BELEG_GESCHEITERT, SATZ_FRIST, SATZ_NICHT_SICHER, STATUS_HOECHSTENS, VERBINDUNG_FRIST,
        ZEILE_HOECHSTENS,
    };
    use crate::passkey::geheim::{self, Stand};

    type Felder = serde_json::Map<String, serde_json::Value>;

    fn frist() -> Abbruch {
        Abbruch::mit(SATZ_FRIST)
    }

    /// `dashboard einmal-code [--einstellungen <datei>]`: Ort, Ablauf, Ausgabe. Gibt das Ende zurück.
    pub fn ausfuehren(einstellungen: Option<&Path>) -> i32 {
        let ergebnis = super::ort(einstellungen).and_then(|ort| {
            let laufzeit = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|_| Abbruch::mit(SATZ_ANTWORTET_NICHT))?;
            laufzeit.block_on(ablauf(&ort, crate::unix::eigene_uid()))
        });
        super::melden(ergebnis, &mut std::io::stdout().lock(), &mut std::io::stderr().lock())
    }

    /// Der ganze Ablauf gegen den Kanal am Ort `ort`, höchstens
    /// [`GANZE_FRIST`]. Die Gegenseite muss die Benutzernummer `erwartete_uid`
    /// haben (im Betrieb die eigene). Gibt die vier Zeilen für stdout zurück.
    pub async fn ablauf(ort: &Path, erwartete_uid: u32) -> Result<String, Abbruch> {
        tokio::time::timeout(GANZE_FRIST, schritte(ort, erwartete_uid)).await.unwrap_or_else(|_| Err(frist()))
    }

    async fn schritte(ort: &Path, uid: u32) -> Result<String, Abbruch> {
        ordner_pruefen(ort)?;
        let wer = fragen(ort, uid, &serde_json::json!({ "befehl": "wer" })).await?;
        if !wer.contains_key("fassung") {
            return Err(Abbruch::mit(SATZ_NICHT_SICHER));
        }
        let beleg = fragen(ort, uid, &serde_json::json!({ "befehl": "beleg" })).await?;
        let frage =
            beleg.get("frage").and_then(|f| f.as_str()).filter(|f| crate::passkey::beleg::form_gilt(f)).ok_or_else(|| Abbruch::mit(SATZ_NICHT_SICHER))?;
        let port = port_aus(&beleg)?;
        for adresse in [SocketAddr::from((Ipv4Addr::LOCALHOST, port)), SocketAddr::from((Ipv6Addr::LOCALHOST, port))] {
            belegen(adresse, port, frage).await?;
        }
        let code = match fragen(ort, uid, &serde_json::json!({ "befehl": "code", "frage": frage })).await {
            Ok(c) => c,
            // Die Frage kam nicht auf beiden Adressen bei diesem Server an.
            Err(a) if a.satz == crate::api::passkey::SATZ_CODE_OHNE_BELEG => return Err(Abbruch::mit(SATZ_BELEG_GESCHEITERT)),
            Err(a) => return Err(a),
        };
        // Ab hier hat der Server den Code gesetzt: nicht mehr abbrechen, nur noch ausgeben.
        let text = code.get("code").and_then(|c| c.as_str()).ok_or_else(|| Abbruch::mit(SATZ_NICHT_SICHER))?;
        let gueltig = code.get("gueltig_sekunden").and_then(|g| g.as_u64()).ok_or_else(|| Abbruch::mit(SATZ_NICHT_SICHER))?;
        Ok(super::ausgabe(text, gueltig, port_aus(&code)?))
    }

    fn port_aus(felder: &Felder) -> Result<u16, Abbruch> {
        felder.get("port").and_then(|p| p.as_u64()).and_then(|p| u16::try_from(p).ok()).filter(|p| *p != 0).ok_or_else(|| Abbruch::mit(SATZ_NICHT_SICHER))
    }

    /// Der Schlüsselordner vor jeder Verbindung: ohne Ordner kein Dashboard,
    /// ein zu offener oder fremder Ordner ist nicht sicher.
    fn ordner_pruefen(ort: &Path) -> Result<(), Abbruch> {
        let ordner = ort.parent().filter(|p| !p.as_os_str().is_empty()).ok_or_else(|| Abbruch::mit(SATZ_ANTWORTET_NICHT))?;
        let m = std::fs::symlink_metadata(ordner).map_err(|_| Abbruch::mit(SATZ_ANTWORTET_NICHT))?;
        geheim::ordner_urteil(Stand { richtige_art: m.file_type().is_dir(), uid: m.uid(), modus: m.mode(), groesse: 0 }, crate::unix::eigene_uid())
            .map_err(|_| Abbruch::mit(SATZ_NICHT_SICHER))
    }

    /// Eine Anfrage an den Kanal über eine eigene Verbindung: verbinden,
    /// `peer_cred()` prüfen, eine Zeile schicken, eine Zeile lesen.
    async fn fragen(ort: &Path, uid: u32, anfrage: &serde_json::Value) -> Result<Felder, Abbruch> {
        let bis = Instant::now() + VERBINDUNG_FRIST;
        let mut v = match timeout_at(bis, UnixStream::connect(ort)).await {
            Err(_) => return Err(frist()),
            Ok(Err(_)) => return Err(Abbruch::mit(SATZ_ANTWORTET_NICHT)),
            Ok(Ok(v)) => v,
        };
        if v.peer_cred().ok().map(|c| c.uid()) != Some(uid) {
            return Err(Abbruch::mit(SATZ_NICHT_SICHER));
        }
        let zeile = timeout_at(bis, async {
            v.write_all(format!("{anfrage}\n").as_bytes()).await?;
            let mut roh = Vec::new();
            (&mut v).take(ZEILE_HOECHSTENS as u64).read_to_end(&mut roh).await?;
            Ok::<_, std::io::Error>(roh)
        })
        .await
        .map_err(|_| frist())?
        .map_err(|_| Abbruch::mit(SATZ_ANTWORTET_NICHT))?;
        let Some(zeile) = zeile.strip_suffix(b"\n") else {
            // Leer oder abgeschnitten: Der Kanal hat nicht geantwortet.
            return Err(Abbruch::mit(if zeile.is_empty() { SATZ_ANTWORTET_NICHT } else { SATZ_NICHT_SICHER }));
        };
        super::kanal_antwort(zeile)
    }

    /// Schickt die Frage an `adresse` und verlangt 204. Liest nur die
    /// Statuszeile (höchstens [`STATUS_HOECHSTENS`]), dann Schluss.
    async fn belegen(adresse: SocketAddr, port: u16, frage: &str) -> Result<(), Abbruch> {
        let bis = Instant::now() + VERBINDUNG_FRIST;
        let mut s = match timeout_at(bis, TcpStream::connect(adresse)).await {
            Err(_) => return Err(frist()),
            Ok(Err(_)) => return Err(Abbruch::mit(SATZ_BELEG_GESCHEITERT)),
            Ok(Ok(s)) => s,
        };
        let status = timeout_at(bis, async {
            s.write_all(super::beleg_anfrage(port, frage).as_bytes()).await?;
            let mut roh = Vec::new();
            let mut stueck = [0u8; 256];
            while roh.len() < STATUS_HOECHSTENS {
                if let Some(code) = super::status_aus(&roh) {
                    return Ok(Some(code));
                }
                let n = s.read(&mut stueck).await?;
                if n == 0 {
                    break;
                }
                roh.extend_from_slice(&stueck[..n]);
            }
            Ok::<_, std::io::Error>(super::status_aus(&roh))
        })
        .await
        .map_err(|_| frist())?;
        match status {
            Ok(Some(204)) => Ok(()),
            _ => Err(Abbruch::mit(SATZ_BELEG_GESCHEITERT)),
        }
    }
}

/// Der Befehl gegen echte Kanäle und Server; nur wo es Unix-Sockets gibt.
#[cfg(all(test, unix))]
mod unix_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ausgabe_vier_zeilen_mit_den_minuten_aus_der_antwort() {
        let text = ausgabe("ABCD-EFGH-JKMN-PQRS-TVWX-YZ01-23", 600, 8799);
        assert_eq!(
            text,
            "Einmal-Code: ABCD-EFGH-JKMN-PQRS-TVWX-YZ01-23\n\
             Er gilt 10 Minuten und nur einmal.\n\
             Im Browser auf diesem Rechner öffnen: http://localhost:8799/anmelden\n\
             Dort unter „Passkey anlegen“ eingeben.\n"
        );
        assert!(ausgabe("X", 120, 1).contains("Er gilt 2 Minuten und nur einmal."));
        assert!(ausgabe("X", 60, 1).contains("Er gilt 1 Minute und nur einmal."));
        assert!(ausgabe("X", 30, 1).contains("Er gilt 1 Minute und nur einmal."));
    }

    #[test]
    fn beleg_anfrage_ohne_origin_mit_host_localhost() {
        let frage = "0123456789abcdef0123456789abcdef";
        let a = beleg_anfrage(8799, frage);
        let (kopf, koerper) = a.split_once("\r\n\r\n").unwrap();
        assert_eq!(koerper, format!("{{\"frage\":\"{frage}\"}}"));
        let zeilen: Vec<&str> = kopf.split("\r\n").collect();
        assert_eq!(zeilen[0], "POST /api/passkey/code/beleg HTTP/1.1");
        assert!(zeilen.contains(&"Host: localhost:8799"), "{kopf}");
        assert!(zeilen.contains(&format!("Content-Length: {}", koerper.len()).as_str()), "{kopf}");
        assert!(!kopf.to_ascii_lowercase().contains("origin"), "{kopf}");
    }

    #[test]
    fn status_nur_aus_einer_vollstaendigen_statuszeile() {
        assert_eq!(status_aus(b"HTTP/1.1 204 No Content\r\n\r\n"), Some(204));
        assert_eq!(status_aus(b"HTTP/1.0 403 Forbidden\r\ncontent-length: 5\r\n"), Some(403));
        assert_eq!(status_aus(b"HTTP/1.1 204"), None, "ohne Zeilenende");
        for falsch in [&b"\r\n"[..], b"SSH-2.0-x\r\n", b"HTTP/2 204\r\n", b"HTTP/1.1 2040\r\n", b"HTTP/1.1 abc\r\n", b"\xff\r\n"] {
            assert_eq!(status_aus(falsch), None, "{falsch:?}");
        }
    }

    #[test]
    fn satz_des_kanals_geht_unveraendert_weiter_anderes_ist_nicht_sicher() {
        let satz = crate::passkey::einmalcode::SATZ_GILT_NICHT;
        assert_eq!(kanal_antwort(serde_json::json!({ "fehler": satz }).to_string().as_bytes()), Err(Abbruch::mit(satz)));
        // Steuerzeichen fallen weg, ein langer Satz wird gekürzt.
        let a = kanal_antwort(serde_json::json!({ "fehler": format!("a\u{1b}[31mb{}", "x".repeat(1000)) }).to_string().as_bytes()).unwrap_err();
        assert!(a.satz.starts_with("a[31mb") && a.satz.len() <= SATZ_HOECHSTENS && a.ende == 9, "{a:?}");
        for falsch in [&b"kein json"[..], b"[1]", b"\"x\"", b"{\"fehler\":7}"] {
            assert_eq!(kanal_antwort(falsch), Err(Abbruch::mit(SATZ_NICHT_SICHER)), "{falsch:?}");
        }
        assert_eq!(kanal_antwort(b"{\"fassung\":\"1\"}").unwrap()["fassung"], "1");
    }

    /// Der Erfolg steht nur auf stdout, ein Abbruch nur auf stderr; das Ende je Weg.
    #[test]
    fn code_nur_auf_stdout_fehler_nur_auf_stderr_und_ende_je_weg() {
        let (mut aus, mut fehler) = (Vec::new(), Vec::new());
        let text = ausgabe("ABCD-EFGH", 600, 8799);
        assert_eq!(melden(Ok(text.clone()), &mut aus, &mut fehler), 0);
        assert_eq!((String::from_utf8(aus).unwrap(), fehler.len()), (text, 0));
        for (abbruch, ende) in [(Abbruch::mit(SATZ_ANTWORTET_NICHT), 9), (Abbruch { satz: "Die Einstellungsdatei x fehlt.".into(), ende: 2 }, 2)] {
            let (mut aus, mut fehler) = (Vec::new(), Vec::new());
            assert_eq!(melden(Err(abbruch.clone()), &mut aus, &mut fehler), ende);
            assert!(aus.is_empty());
            assert_eq!(String::from_utf8(fehler).unwrap(), format!("{}\n", abbruch.satz));
        }
    }

    /// Ohne `--einstellungen` derselbe Kanalort wie beim Server ohne
    /// Argumente (`AppState::aus_umgebung`: `daten_datei()`, ohne Einstellungen).
    #[test]
    fn ohne_einstellungen_derselbe_kanalort_wie_der_server_ohne_argumente() {
        let server = crate::app::jwt_schluessel_datei(&crate::app::daten_datei());
        assert_eq!(ort(None).ok(), crate::kanal::ort_neben(server.as_deref()));
    }

    /// Mit `--einstellungen` derselbe Kanalort wie `--konsole` mit derselben
    /// Datei: neben `e.jwt_schluessel_datei()`. Eine fehlende Datei endet mit 2.
    #[test]
    fn mit_einstellungen_derselbe_kanalort_wie_konsole_mit_derselben_datei() {
        let ordner = std::env::temp_dir().join(format!("dk-{}-befehl-ort", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        for unter in ["repo", "daten", "sicherung", "protokoll", "web"] {
            std::fs::create_dir_all(ordner.join(unter)).unwrap();
        }
        let p = |n: &str| ordner.join(n).display().to_string().replace('\\', "/");
        let text = format!(
            "schema = 1\nrepo = \"{}\"\nport = 8799\n\n[pfade]\ndaten = \"{}\"\nsicherung = \"{}\"\nprotokoll = \"{}\"\nweb = \"{}\"\n\n[konten]\ninhaber_sid = \"S-1-5-21-1-2-3-1001\"\n",
            p("repo"),
            p("daten/dashboard.redb"),
            p("sicherung"),
            p("protokoll"),
            p("web")
        );
        let datei = ordner.join("dienst.toml");
        std::fs::write(&datei, text).unwrap();
        let e = crate::dienst::einstellungen::Einstellungen::laden(&datei).unwrap_or_else(|s| panic!("{s}"));
        assert_eq!(ort(Some(&datei)).unwrap(), crate::kanal::ort_neben(Some(&e.jwt_schluessel_datei())).unwrap());
        assert_eq!(ort(Some(&datei)).unwrap(), ordner.join("geheim").join(crate::kanal::KANAL_DATEI));
        // Eine fehlende Einstellungsdatei: ihr Satz, Ende 2 wie bei `--konsole`.
        let fehlt = ort(Some(&ordner.join("gibt-es-nicht.toml"))).unwrap_err();
        assert_eq!(fehlt.ende, 2);
        assert!(fehlt.satz.contains("fehlt"), "{fehlt:?}");
        let _ = std::fs::remove_dir_all(&ordner);
    }
}
