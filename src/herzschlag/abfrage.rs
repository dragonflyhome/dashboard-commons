//! Die Frage an das Dashboard: `GET /api/sitzungen/<id>/offen` über
//! 127.0.0.1 mit der Windows-Anmeldung des aufrufenden Kontos (Negotiate,
//! derselbe SSPI-Client wie in den Tests der Anmeldung), alle Runden auf einer
//! Verbindung. Eine harte Frist gilt für das Netz zusammen (Verbinden, Senden,
//! Lesen), nicht für die Schritte der Anmeldung (SSPI, lokal); jeder Fehler ist
//! ein Satz, den der Herzschlag als „durchlassen“ nimmt.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::graph::Arbeit;

/// Höchstens so lange für die ganze Abfrage.
pub const FRIST: Duration = Duration::from_secs(5);
/// Standard: das Dashboard auf dem eigenen Rechner.
pub const SERVER: &str = "127.0.0.1:8787";
/// Höchstens so viele Bytes Antwort (Kopf und Körper).
const ANTWORT_HOECHSTENS: usize = 256 * 1024;

/// Form einer Sitzungskennung (wie der Dienst sie annimmt).
#[must_use]
pub fn sitzung_gueltig(s: &str) -> bool {
    (1..=128).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// `server` als Adresse, nur auf dem eigenen Rechner (die Anmeldung geht nie hinaus).
///
/// # Errors
/// Ein Satz, wenn die Adresse nicht lesbar ist oder nicht auf den eigenen Rechner zeigt.
pub fn adresse(server: &str) -> Result<SocketAddr, String> {
    // Nur IP:Port, ohne Namensauflösung: die liefe außerhalb der Frist.
    let a: SocketAddr = server.parse().map_err(|e| format!("Adresse {server}: {e}"))?;
    if a.ip().is_loopback() { Ok(a) } else { Err(format!("Adresse {server}: nicht der eigene Rechner")) }
}

/// Fragt das Dashboard nach der offenen Arbeit der Sitzung.
///
/// # Errors
/// Ein Satz bei jedem Fehler: keine Verbindung, Frist abgelaufen, 401 oder ein
/// anderer Code, unlesbare Antwort, falsche Sitzungskennung.
pub fn offen(server: &str, sitzung: &str, frist: Duration) -> Result<Arbeit, String> {
    if !sitzung_gueltig(sitzung) {
        return Err("Sitzungskennung hat nicht die Form [A-Za-z0-9_-]{1,128}".into());
    }
    let ende = Instant::now() + frist;
    let ziel = adresse(server)?;
    let koerper = holen(ziel, server, &format!("/api/sitzungen/{sitzung}/offen"), ende)?;
    arbeit_aus(&koerper)
}

/// Die Antwort des Dienstes als `Arbeit`.
///
/// # Errors
/// Ein Satz, wenn die Antwort nicht die erwartete Form hat.
pub fn arbeit_aus(koerper: &[u8]) -> Result<Arbeit, String> {
    let v: Value = serde_json::from_slice(koerper).map_err(|e| format!("Antwort kein JSON: {e}"))?;
    let offen = v.get("offen").and_then(Value::as_array).ok_or("Antwort ohne „offen“")?;
    match v.get("von") {
        Some(Value::Null) => return Ok(Arbeit::Unbekannt),
        Some(Value::String(_)) => {}
        _ => return Err("Antwort ohne „von“".into()),
    }
    if offen.is_empty() {
        return Ok(Arbeit::Frei);
    }
    // Die Antwort kann von einem fremden Programm auf dem Port kommen: jedes Feld
    // ohne Steuerzeichen und gekürzt, die ganze Liste höchstens OFFEN_HOECHSTENS.
    let teile: Vec<String> = offen
        .iter()
        .map(|s| {
            let text = |k: &str, max: usize| kuerzen(s.get(k).and_then(Value::as_str).unwrap_or_default(), max);
            let nr = s.get("nr").and_then(Value::as_u64).map(|n| format!(" Schritt {n}")).unwrap_or_default();
            format!("{}{nr} „{}“ ({})", text("arbeit", 64), text("titel", 160), text("grund", 64))
        })
        .collect();
    Ok(Arbeit::Offen(kuerzen(&teile.join("; "), OFFEN_HOECHSTENS)))
}

/// Höchstens so viele Bytes für die offenen Schritte im Grund eines Blocks
/// (der Grund bleibt so unter etwa 2 KB).
pub const OFFEN_HOECHSTENS: usize = 1536;

/// `s` ohne Steuerzeichen, höchstens `max` Bytes, an einer Zeichengrenze
/// gekürzt; gekürzt endet es mit „…“ (zählt mit).
#[must_use]
pub fn kuerzen(s: &str, max: usize) -> String {
    let rein: String = s.chars().filter(|c| !c.is_control()).collect();
    if rein.len() <= max {
        return rein;
    }
    let mut ende = max.saturating_sub('…'.len_utf8());
    while !rein.is_char_boundary(ende) {
        ende -= 1;
    }
    format!("{}…", &rein[..ende])
}

fn rest(ende: Instant) -> Result<Duration, String> {
    let r = ende.saturating_duration_since(Instant::now());
    if r.is_zero() { Err("Frist abgelaufen".into()) } else { Ok(r) }
}

/// Eine Antwort: Code, Kopfzeilen (Name klein), Körper.
struct Antwort {
    code: u16,
    kopf: Vec<(String, String)>,
    koerper: Vec<u8>,
}

impl Antwort {
    fn kopf(&self, name: &str) -> Option<&str> {
        self.kopf.iter().find(|(n, _)| n == name).map(|(_, w)| w.as_str())
    }
}

/// GET mit Negotiate, bis 200 (höchstens 5 Runden); der Körper bei 200.
#[cfg(windows)]
fn holen(ziel: SocketAddr, host: &str, pfad: &str, ende: Instant) -> Result<Vec<u8>, String> {
    use crate::anmeldung::base64;
    use crate::anmeldung::sspi::client::Client;

    let mut s = TcpStream::connect_timeout(&ziel, rest(ende)?).map_err(|e| format!("keine Verbindung: {e}"))?;
    let mut client = Client::neu("HTTP/localhost")?;
    let (mut nachweis, _) = client.schritt(None)?;
    for _ in 0..5 {
        let anfrage =
            format!("GET {pfad} HTTP/1.1\r\nHost: {host}\r\nAccept: application/json\r\nAuthorization: Negotiate {}\r\n\r\n", base64::kodieren(&nachweis));
        s.set_write_timeout(Some(rest(ende)?)).map_err(|e| e.to_string())?;
        s.write_all(anfrage.as_bytes()).map_err(|e| format!("senden: {e}"))?;
        let a = lesen(&mut s, ende)?;
        match a.code {
            200 => return Ok(a.koerper),
            401 => {
                let challenge = a.kopf("www-authenticate").and_then(|w| w.strip_prefix("Negotiate ")).ok_or("401 ohne Challenge")?;
                let token = base64::dekodieren(challenge.trim()).ok_or("Challenge nicht lesbar")?;
                nachweis = client.schritt(Some(&token))?.0;
            }
            code => return Err(format!("Antwort {code}")),
        }
    }
    Err("Anmeldung nach 5 Runden nicht fertig".into())
}

#[cfg(not(windows))]
fn holen(_ziel: SocketAddr, _host: &str, _pfad: &str, _ende: Instant) -> Result<Vec<u8>, String> {
    Err("Die Windows-Anmeldung gibt es nur unter Windows.".into())
}

/// Liest eine Antwort mit `Content-Length` bis zur Frist.
// shortcut: kein Transfer-Encoding chunked (der Dienst schickt JSON mit Länge); aufrüsten, wenn sich das ändert.
fn lesen(s: &mut TcpStream, ende: Instant) -> Result<Antwort, String> {
    let mut puffer = Vec::new();
    let mut stueck = [0u8; 8192];
    let kopf_ende = loop {
        if let Some(i) = puffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break i;
        }
        mehr(s, &mut puffer, &mut stueck, ende)?;
    };
    let kopf = String::from_utf8_lossy(&puffer[..kopf_ende]).into_owned();
    let mut zeilen = kopf.split("\r\n");
    let code = zeilen.next().and_then(|z| z.split(' ').nth(1)).and_then(|c| c.parse().ok()).ok_or("Statuszeile unlesbar")?;
    let kopf: Vec<(String, String)> = zeilen.filter_map(|z| z.split_once(':')).map(|(n, w)| (n.trim().to_ascii_lowercase(), w.trim().to_string())).collect();
    if kopf.iter().any(|(n, w)| n == "transfer-encoding" && w.eq_ignore_ascii_case("chunked")) {
        return Err("Antwort in Stücken (chunked) wird nicht gelesen".into());
    }
    let laenge: usize = kopf.iter().find(|(n, _)| n == "content-length").and_then(|(_, w)| w.parse().ok()).ok_or("Antwort ohne Content-Length")?;
    let start = kopf_ende + 4;
    while puffer.len() < start + laenge {
        mehr(s, &mut puffer, &mut stueck, ende)?;
    }
    let koerper = puffer[start..start + laenge].to_vec();
    Ok(Antwort { code, kopf, koerper })
}

fn mehr(s: &mut TcpStream, puffer: &mut Vec<u8>, stueck: &mut [u8], ende: Instant) -> Result<(), String> {
    s.set_read_timeout(Some(rest(ende)?)).map_err(|e| e.to_string())?;
    let n = s.read(stueck).map_err(|e| format!("lesen: {e}"))?;
    if n == 0 {
        return Err("Verbindung vorzeitig zu".into());
    }
    puffer.extend_from_slice(&stueck[..n]);
    if puffer.len() > ANTWORT_HOECHSTENS {
        return Err("Antwort zu groß".into());
    }
    Ok(())
}
