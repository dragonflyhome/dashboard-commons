//! Mischform für die Links der Aufgaben außerhalb von Windows: Verschlüsselt
//! wird im Programm mit XChaCha20-Poly1305 (RustCrypto), der Schlüssel von
//! 32 Bytes liegt als Datei im Schlüsselordner, neben dem Schlüssel der
//! Ausweise (`verschluesselung::LinksSchluessel`, `passkey::geheim`).
//!
//! Was das schützt: Kopien des Datenspeichers (die tägliche Sicherung, eine
//! kopierte Datei) und andere Konten am selben Rechner. Es schützt nicht gegen
//! Programme desselben Kontos und nicht gegen eine Kopie des Schlüsselordners.
//! Fehlt der Schlüssel (anderer Rechner, Sicherung ohne den Schlüsselordner),
//! sind gespeicherte Links unlesbar und laufen nur ab, wie bei DPAPI auf
//! einem anderen PC.
//!
//! Format eines Werts: [`KOPF`] (drei Bytes), 24 Bytes Nonce aus dem Zufall
//! des Systems, dann das Chiffrat mit Prüfsumme (16 Bytes). Gebunden sind
//! der Kopf und ein fester Wert der Anwendung ([`gebunden`]); beide sind
//! kein Geheimnis, sie trennen nur den Zusammenhang. Die Kennung der Aufgabe
//! wird bewusst nicht gebunden, wie bei DPAPI: Der Schutz gilt Kopien des
//! Datenspeichers, nicht jemandem, der die Datei schreiben kann, und
//! verschlüsselt wird vor der Vergabe der Kennung.
//!
//! Das Modul gibt es außerhalb von Windows und in Tests: Unter Windows bleibt
//! DPAPI, dort laufen nur die Tests für Format und Verfahren.

use chacha20poly1305::{AeadInOut, KeyInit, XChaCha20Poly1305, XNonce};
use zeroize::Zeroizing;

/// Kopf jedes Werts: die Fassung des Formats. Nie ändern, nur eine neue
/// Fassung daneben stellen.
pub const KOPF: &[u8; 3] = b"DL1";

/// Fester Wert der Anwendung, mit dem Kopf zusammen gebunden. Kein Geheimnis.
/// Nie ändern: sonst sind alle gespeicherten Links unlesbar.
pub const ZUSATZ: &[u8] = b"dashboard/aufgaben_links/xchacha/v1";

/// Länge der Nonce von XChaCha20.
pub const NONCE: usize = 24;

/// Länge der Prüfsumme von Poly1305.
pub const PRUEFSUMME: usize = 16;

/// Der Schlüssel der Links, in einem Puffer, der beim Freigeben überschrieben wird.
pub type Schluessel = Zeroizing<[u8; 32]>;

/// Warum ein Wert nicht lesbar ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatFehler {
    /// Kürzer als Kopf, Nonce und Prüfsumme zusammen.
    ZuKurz,
    /// Ein anderer Kopf als [`KOPF`].
    Kopf,
    /// Verändert, mit einem anderen Schlüssel geschrieben oder aus einem anderen Zusammenhang.
    Pruefsumme,
}

/// Der feste Satz zu einem unlesbaren Wert. Nie ein Teil des Werts.
pub fn format_satz(f: FormatFehler) -> &'static str {
    match f {
        FormatFehler::ZuKurz => "Der gespeicherte Link ist zu kurz für das Format DL1; er wird nicht gelesen.",
        FormatFehler::Kopf => "Der gespeicherte Link trägt nicht den Kopf DL1; er wird nicht gelesen.",
        FormatFehler::Pruefsumme => {
            "Der gespeicherte Link besteht die Prüfung nicht (verändert oder mit einem anderen Schlüssel geschrieben); er wird nicht gelesen."
        }
    }
}

/// Fester Satz, wenn das Verschlüsseln selbst scheitert.
pub const VERSIEGELN_SATZ: &str = "Der Link ließ sich nicht verschlüsseln.";

/// Fester Satz, wenn das System keinen Zufall liefert.
pub const ZUFALL_SATZ: &str = "Das System liefert keinen Zufall; der Link wird nicht verschlüsselt.";

/// Was mit dem Chiffrat zusammen geprüft wird: Kopf und fester Wert.
pub fn gebunden() -> Vec<u8> {
    let mut v = Vec::with_capacity(KOPF.len() + ZUSATZ.len());
    v.extend_from_slice(KOPF);
    v.extend_from_slice(ZUSATZ);
    v
}

/// 24 Bytes aus dem Zufall des Systems (derselbe Weg wie für den Schlüssel
/// der Ausweise). Ein Fehler wird ein fester Satz, keine Panik.
pub fn neue_nonce() -> Result<[u8; NONCE], String> {
    use rand_core::{OsRng, RngCore};
    let mut n = [0u8; NONCE];
    OsRng.try_fill_bytes(&mut n).map_err(|_| ZUFALL_SATZ.to_string())?;
    Ok(n)
}

/// Verschlüsselt `klartext` mit `schluessel` und `nonce` in das Format DL1.
/// Die Nonce darf sich je Schlüssel nie wiederholen; im Betrieb kommt sie aus
/// [`neue_nonce`].
pub fn versiegeln(schluessel: &[u8; 32], nonce: &[u8; NONCE], klartext: &[u8]) -> Result<Vec<u8>, String> {
    // Der Klartext liegt nur in diesem Puffer; er ist groß genug für die
    // Prüfsumme, damit er beim Anhängen nicht umzieht und eine Kopie hinterlässt.
    let mut puffer = Zeroizing::new(Vec::with_capacity(klartext.len() + PRUEFSUMME));
    puffer.extend_from_slice(klartext);
    let verfahren = XChaCha20Poly1305::new(schluessel.into());
    verfahren.encrypt_in_place(&XNonce::from(*nonce), &gebunden(), &mut *puffer).map_err(|_| VERSIEGELN_SATZ.to_string())?;
    let mut aus = Vec::with_capacity(KOPF.len() + NONCE + puffer.len());
    aus.extend_from_slice(KOPF);
    aus.extend_from_slice(nonce);
    aus.extend_from_slice(&puffer);
    Ok(aus)
}

/// Prüft und entschlüsselt einen Wert im Format DL1. Der Klartext entsteht
/// ohne Zwischenkopie in dem Puffer, der zurückgeht und beim Freigeben
/// überschrieben wird.
pub fn oeffnen(schluessel: &[u8; 32], geheim: &[u8]) -> Result<Zeroizing<Vec<u8>>, FormatFehler> {
    if geheim.len() < KOPF.len() + NONCE + PRUEFSUMME {
        return Err(FormatFehler::ZuKurz);
    }
    let (kopf, rest) = geheim.split_at(KOPF.len());
    if kopf != KOPF {
        return Err(FormatFehler::Kopf);
    }
    let (nonce, chiffrat) = rest.split_at(NONCE);
    let mut nonce_fest = [0u8; NONCE];
    nonce_fest.copy_from_slice(nonce);
    let mut puffer = Zeroizing::new(chiffrat.to_vec());
    let verfahren = XChaCha20Poly1305::new(schluessel.into());
    verfahren.decrypt_in_place(&XNonce::from(nonce_fest), &gebunden(), &mut *puffer).map_err(|_| FormatFehler::Pruefsumme)?;
    Ok(puffer)
}

/// Der Zwischenspeicher für den Schlüssel: Die Quelle wird erst beim ersten
/// Bedarf gefragt, nach einem Erfolg nie wieder. Ein Fehler wird nicht
/// gemerkt; der nächste Aufruf fragt wieder. Die Sperre gilt für die ganze
/// Frage an die Quelle: Zwei Läufe zugleich fragen nacheinander, und der
/// zweite findet den Schlüssel schon vor. Bricht die Quelle mit einer Panik
/// ab, bleibt der Speicher leer, und der nächste Aufruf läuft weiter.
pub fn zwischengespeichert(ablage: &std::sync::Mutex<Option<Schluessel>>, quelle: &dyn Fn() -> Result<Schluessel, String>) -> Result<Schluessel, String> {
    let mut platz = ablage.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(s) = platz.as_ref() {
        return Ok(s.clone());
    }
    let s = quelle()?;
    *platz = Some(s.clone());
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Erfundener Testwert, kein echter Link.
    const PROBE: &str = "https://beispiel.invalid/anmelden?token=erfunden-0815";
    /// Fester Testschlüssel und feste Nonce, beide erfunden.
    const SCHLUESSEL: [u8; 32] = [7u8; 32];
    const N: [u8; NONCE] = [3u8; NONCE];

    fn enthaelt(heu: &[u8], nadel: &[u8]) -> bool {
        heu.windows(nadel.len()).any(|w| w == nadel)
    }

    /// Kopf, Nonce, Chiffrat mit Prüfsumme; hin und zurück; kein Klartext im Wert.
    #[test]
    fn format_und_weg_zurueck() {
        let geheim = versiegeln(&SCHLUESSEL, &N, PROBE.as_bytes()).unwrap();
        assert_eq!(&geheim[..3], b"DL1");
        assert_eq!(&geheim[3..3 + NONCE], &N);
        assert_eq!(geheim.len(), 3 + NONCE + PROBE.len() + PRUEFSUMME);
        assert!(!enthaelt(&geheim, PROBE.as_bytes()) && !enthaelt(&geheim, b"erfunden-0815"), "Klartext im Wert");
        assert_eq!(oeffnen(&SCHLUESSEL, &geheim).unwrap().as_slice(), PROBE.as_bytes());
        // Der leere Klartext ist der kürzeste gültige Wert.
        let leer = versiegeln(&SCHLUESSEL, &N, b"").unwrap();
        assert_eq!(leer.len(), 3 + NONCE + PRUEFSUMME);
        assert!(oeffnen(&SCHLUESSEL, &leer).unwrap().is_empty());
    }

    /// Ein fester Vektor: Das Format ändert sich nicht unbemerkt, auf keiner Plattform.
    #[test]
    fn fester_vektor() {
        let geheim = versiegeln(&SCHLUESSEL, &N, b"abc").unwrap();
        let hex: String = geheim.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, FESTER_VEKTOR);
    }
    /// Nachgerechnet mit einer zweiten, unabhängigen Umsetzung nach RFC 8439
    /// und dem Entwurf zu XChaCha20 (HChaCha20, dann ChaCha20-Poly1305).
    const FESTER_VEKTOR: &str = "444c31030303030303030303030303030303030303030303030303e56428cb5d8a2d3d185397c7b913f4ad20e189";

    /// Kürzer als 3 + 24 + 16 Bytes: zu kurz, für jede Länge darunter.
    #[test]
    fn zu_kurz() {
        let geheim = versiegeln(&SCHLUESSEL, &N, b"").unwrap();
        for laenge in 0..geheim.len() {
            assert_eq!(oeffnen(&SCHLUESSEL, &geheim[..laenge]).unwrap_err(), FormatFehler::ZuKurz, "{laenge}");
        }
    }

    /// Ein anderer Kopf, auch der alte Testkopf und eine künftige Fassung.
    #[test]
    fn anderer_kopf() {
        let geheim = versiegeln(&SCHLUESSEL, &N, PROBE.as_bytes()).unwrap();
        for kopf in [b"DL2", b"dl1", b"TES", b"\0\0\0"] {
            let mut anders = geheim.clone();
            anders[..3].copy_from_slice(kopf);
            assert_eq!(oeffnen(&SCHLUESSEL, &anders).unwrap_err(), FormatFehler::Kopf);
        }
    }

    /// Jedes veränderte Byte hinter dem Kopf (Nonce, Chiffrat, Prüfsumme)
    /// ergibt den Fehler der Prüfung, nie Unsinn; ebenso ein angehängtes Byte.
    #[test]
    fn veraendertes_byte() {
        let geheim = versiegeln(&SCHLUESSEL, &N, PROBE.as_bytes()).unwrap();
        for stelle in 3..geheim.len() {
            let mut kaputt = geheim.clone();
            kaputt[stelle] ^= 0x01;
            assert_eq!(oeffnen(&SCHLUESSEL, &kaputt).unwrap_err(), FormatFehler::Pruefsumme, "Stelle {stelle}");
        }
        let mut laenger = geheim.clone();
        laenger.push(0);
        assert_eq!(oeffnen(&SCHLUESSEL, &laenger).unwrap_err(), FormatFehler::Pruefsumme);
    }

    /// Ein anderer Schlüssel liest nichts.
    #[test]
    fn anderer_schluessel() {
        let geheim = versiegeln(&SCHLUESSEL, &N, PROBE.as_bytes()).unwrap();
        assert_eq!(oeffnen(&[8u8; 32], &geheim).unwrap_err(), FormatFehler::Pruefsumme);
    }

    /// Kopf und fester Wert sind gebunden: Ein Wert, der mit demselben
    /// Schlüssel und derselben Nonce, aber ohne diese Angabe oder nur mit dem
    /// festen Wert geschrieben wurde, wird nicht gelesen.
    #[test]
    fn kopf_und_fester_wert_sind_gebunden() {
        assert_eq!(gebunden(), b"DL1dashboard/aufgaben_links/xchacha/v1");
        let verfahren = XChaCha20Poly1305::new((&SCHLUESSEL).into());
        for angabe in [&b""[..], ZUSATZ, b"DL1"] {
            let mut puffer = PROBE.as_bytes().to_vec();
            verfahren.encrypt_in_place(&XNonce::from(N), angabe, &mut puffer).unwrap();
            let mut wert = b"DL1".to_vec();
            wert.extend_from_slice(&N);
            wert.extend_from_slice(&puffer);
            assert_eq!(oeffnen(&SCHLUESSEL, &wert).unwrap_err(), FormatFehler::Pruefsumme);
        }
    }

    /// Zwei Nonces aus dem Zufall sind verschieden, und derselbe Klartext
    /// ergibt damit zwei verschiedene Werte.
    #[test]
    fn nonce_aus_dem_zufall() {
        let (a, b) = (neue_nonce().unwrap(), neue_nonce().unwrap());
        assert_ne!(a, b);
        assert_ne!(versiegeln(&SCHLUESSEL, &a, PROBE.as_bytes()).unwrap(), versiegeln(&SCHLUESSEL, &b, PROBE.as_bytes()).unwrap());
    }

    /// Der Zwischenspeicher: Ein Fehler wird nicht gemerkt, der zweite Aufruf
    /// fragt wieder; nach einem Erfolg fragt keiner mehr; die Quelle wird vor
    /// dem ersten Bedarf nie gefragt.
    #[test]
    fn zwischenspeicher_merkt_nur_den_erfolg() {
        use std::sync::Mutex;
        use std::sync::atomic::{AtomicU32, Ordering};
        let ablage: Mutex<Option<Schluessel>> = Mutex::new(None);
        let gefragt = AtomicU32::new(0);
        assert_eq!(gefragt.load(Ordering::SeqCst), 0, "ohne Bedarf keine Frage");
        let quelle = || {
            // Die ersten zwei Fragen scheitern, die dritte bringt den Schlüssel.
            match gefragt.fetch_add(1, Ordering::SeqCst) {
                0 | 1 => Err("erfundener Fehler".to_string()),
                n => Ok(Zeroizing::new([n as u8; 32])),
            }
        };
        assert_eq!(zwischengespeichert(&ablage, &quelle).unwrap_err(), "erfundener Fehler");
        assert!(ablage.lock().unwrap().is_none(), "ein Fehler wird nicht gespeichert");
        assert!(zwischengespeichert(&ablage, &quelle).is_err());
        assert_eq!(gefragt.load(Ordering::SeqCst), 2, "der zweite Aufruf fragt wieder");
        assert_eq!(*zwischengespeichert(&ablage, &quelle).unwrap(), [2u8; 32]);
        for _ in 0..5 {
            assert_eq!(*zwischengespeichert(&ablage, &quelle).unwrap(), [2u8; 32]);
        }
        assert_eq!(gefragt.load(Ordering::SeqCst), 3, "nach dem Erfolg fragt keiner mehr");
    }

    /// Eine Panik in der Quelle hält den nächsten Aufruf nicht auf, und der
    /// Speicher bleibt dabei leer.
    #[test]
    fn zwischenspeicher_laeuft_nach_einer_panik_weiter() {
        use std::sync::{Arc, Mutex};
        let ablage: Arc<Mutex<Option<Schluessel>>> = Arc::new(Mutex::new(None));
        let fremd = ablage.clone();
        let lauf = std::thread::spawn(move || zwischengespeichert(&fremd, &|| panic!("erfundene Panik in der Quelle")));
        assert!(lauf.join().is_err(), "die Panik kommt beim Aufrufer an");
        assert!(ablage.is_poisoned(), "die Sperre gilt als vergiftet");
        assert_eq!(*zwischengespeichert(&ablage, &|| Ok(Zeroizing::new([4u8; 32]))).unwrap(), [4u8; 32]);
        assert_eq!(*zwischengespeichert(&ablage, &|| Err("wird nicht gefragt".to_string())).unwrap(), [4u8; 32]);
    }

    /// Zwei Läufe zugleich fragen die Quelle nacheinander: Sie wird genau
    /// einmal gefragt, und beide bekommen denselben Schlüssel.
    #[test]
    fn zwischenspeicher_fragt_bei_gleichzeitigem_bedarf_einmal() {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::{Arc, Barrier, Mutex};
        let ablage: Arc<Mutex<Option<Schluessel>>> = Arc::new(Mutex::new(None));
        let gefragt = Arc::new(AtomicU32::new(0));
        let schranke = Arc::new(Barrier::new(8));
        let laeufe: Vec<_> = (0..8)
            .map(|_| {
                let (ablage, gefragt, schranke) = (ablage.clone(), gefragt.clone(), schranke.clone());
                std::thread::spawn(move || {
                    schranke.wait();
                    zwischengespeichert(&ablage, &|| {
                        let n = gefragt.fetch_add(1, Ordering::SeqCst);
                        std::thread::sleep(std::time::Duration::from_millis(20));
                        Ok(Zeroizing::new([10 + n as u8; 32]))
                    })
                })
            })
            .collect();
        for lauf in laeufe {
            assert_eq!(*lauf.join().unwrap().unwrap(), [10u8; 32]);
        }
        assert_eq!(gefragt.load(Ordering::SeqCst), 1);
    }

    /// Kein Satz nennt den Schlüssel, den Klartext oder einen Teil davon.
    #[test]
    fn saetze_verraten_nichts() {
        let geheim = versiegeln(&SCHLUESSEL, &N, PROBE.as_bytes()).unwrap();
        let mut saetze: Vec<String> =
            [FormatFehler::ZuKurz, FormatFehler::Kopf, FormatFehler::Pruefsumme].into_iter().map(|f| format_satz(f).to_string()).collect();
        saetze.extend([VERSIEGELN_SATZ, ZUFALL_SATZ].into_iter().map(str::to_string));
        // Der Satz, der beim echten Scheitern entsteht.
        let mut kaputt = geheim.clone();
        kaputt[40] ^= 1;
        saetze.push(format_satz(oeffnen(&SCHLUESSEL, &kaputt).unwrap_err()).to_string());
        let schluessel_hex: String = SCHLUESSEL.iter().map(|b| format!("{b:02x}")).collect();
        let wert_hex: String = geheim.iter().map(|b| format!("{b:02x}")).collect();
        for satz in &saetze {
            for teil in [PROBE, "erfunden", "0815", "beispiel.invalid", "token", &schluessel_hex[..16], &wert_hex[6..22], "07070707", "[7, 7"] {
                assert!(!satz.contains(teil), "{teil} in: {satz}");
            }
        }
    }
}
