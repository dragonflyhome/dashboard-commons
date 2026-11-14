//! Mischform für die Links der Aufgaben außerhalb von Windows: Verschlüsselt
//! wird im Programm mit XChaCha20-Poly1305 (RustCrypto), der Schlüssel von
//! 32 Bytes liegt unter macOS im Anmelde-Schlüsselbund.
//!
//! Was das schützt: Kopien des Datenspeichers (die tägliche Sicherung, eine
//! kopierte Datei) und andere Konten am selben Rechner. Es schützt nicht gegen
//! Programme desselben Kontos und nicht gegen jemanden, der eine Sicherung des
//! ganzen Benutzerordners samt Anmeldekennwort hat. Fehlt der Schlüssel
//! (anderer Rechner, Sicherung ohne Schlüsselbund), sind gespeicherte Links
//! unlesbar und laufen nur ab, wie bei DPAPI auf einem anderen PC.
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

/// Fehler aus dem Schlüsselbund, den der Server nicht übergeht (`errSecItemNotFound`).
pub const NICHT_GEFUNDEN: i32 = -25300;
/// `errSecDuplicateItem`.
pub const GIBT_ES_SCHON: i32 = -25299;
/// `errSecInteractionNotAllowed`.
pub const GESPERRT: i32 = -25308;
/// `errSecAuthFailed`. Am Gerät gemessen: Das meldet der Anmelde-Schlüsselbund
/// bei gesperrten Dialogen, wenn ein anderes Programm als das anlegende den
/// Eintrag lesen will, also auch dasselbe Programm nach einem Neubau ohne
/// feste Signatur.
pub const VERWEIGERT: i32 = -25293;

/// Was eine Nummer aus dem Schlüsselbund bedeutet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fehlerart {
    /// Es gibt den Eintrag nicht. Nur hier wird angelegt.
    NichtGefunden,
    /// Beim Anlegen: Es gibt den Eintrag schon. Einmal neu lesen, nie schreiben.
    GibtEsSchon,
    /// Der Schlüsselbund ist gesperrt, oder die Sitzung hat keine Anmeldung (etwa über SSH).
    Gesperrt,
    /// Der Schlüsselbund gibt den Eintrag diesem Programm nicht.
    Verweigert,
    /// Alles andere, mit der Nummer.
    Anderer(i32),
}

/// Ordnet die Nummer des Schlüsselbunds ein. Reine Funktion.
pub fn fehlerart(nummer: i32) -> Fehlerart {
    match nummer {
        NICHT_GEFUNDEN => Fehlerart::NichtGefunden,
        GIBT_ES_SCHON => Fehlerart::GibtEsSchon,
        GESPERRT => Fehlerart::Gesperrt,
        VERWEIGERT => Fehlerart::Verweigert,
        n => Fehlerart::Anderer(n),
    }
}

/// Der feste Satz zu einem Fehler des Schlüsselbunds: die Art und die Nummer,
/// nie ein Inhalt.
pub fn schluesselbund_satz(nummer: i32) -> String {
    match fehlerart(nummer) {
        Fehlerart::NichtGefunden => {
            format!("Der Schlüssel der Aufgaben-Links fehlt im Schlüsselbund ({nummer}); gespeicherte Links sind nicht lesbar und laufen nur ab.")
        }
        Fehlerart::GibtEsSchon => format!("Der Schlüssel der Aufgaben-Links ist schon im Schlüsselbund und wird nie überschrieben ({nummer})."),
        Fehlerart::Gesperrt => format!("Schlüsselbund gesperrt oder eine Sitzung ohne Anmeldung ({nummer})."),
        Fehlerart::Verweigert => {
            format!("Der Schlüsselbund gibt den Schlüssel der Aufgaben-Links diesem Programm nicht (etwa nach einem Neubau ohne feste Signatur) ({nummer}).")
        }
        Fehlerart::Anderer(n) => format!("Der Schlüsselbund hat den Schlüssel der Aufgaben-Links nicht herausgegeben (Fehler {n})."),
    }
}

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

/// 32 Bytes für einen neuen Schlüssel, aus demselben Zufall.
pub fn neuer_schluessel() -> Result<Schluessel, String> {
    use rand_core::{OsRng, RngCore};
    let mut s = Zeroizing::new([0u8; 32]);
    OsRng.try_fill_bytes(s.as_mut_slice()).map_err(|_| ZUFALL_SATZ.to_string())?;
    Ok(s)
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

/// Holt den Schlüssel aus einer Ablage oder legt ihn dort an. Die Ablage ist
/// eingespeist (`lesen`, `anlegen`, je mit der Nummer des Schlüsselbunds als
/// Fehler), damit die Regeln auf jeder Plattform prüfbar sind, ohne einen
/// Schlüsselbund zu berühren.
///
/// Die Regeln: Angelegt wird nur, wenn das Lesen ausdrücklich „nicht
/// gefunden“ meldet. Bei jedem anderen Fehler entsteht kein Schlüssel. Meldet
/// das Anlegen „gibt es schon“ (ein anderer Lauf war schneller), wird genau
/// einmal neu gelesen und nie geschrieben. Ein Eintrag mit anderer Länge als
/// 32 Bytes ist ein Fehler und bleibt, wie er ist.
pub fn schluessel_holen(
    lesen: &dyn Fn() -> Result<Zeroizing<Vec<u8>>, i32>,
    anlegen: &dyn Fn(&[u8; 32]) -> Result<(), i32>,
    neu: &dyn Fn() -> Result<Schluessel, String>,
) -> Result<Schluessel, String> {
    match lesen() {
        Ok(bytes) => aus_eintrag(&bytes),
        Err(nummer) if fehlerart(nummer) == Fehlerart::NichtGefunden => {
            let frisch = neu()?;
            match anlegen(&frisch) {
                Ok(()) => Ok(frisch),
                Err(n) if fehlerart(n) == Fehlerart::GibtEsSchon => match lesen() {
                    Ok(bytes) => aus_eintrag(&bytes),
                    Err(n) => Err(schluesselbund_satz(n)),
                },
                Err(n) => Err(schluesselbund_satz(n)),
            }
        }
        Err(nummer) => Err(schluesselbund_satz(nummer)),
    }
}

/// Holt den Schlüssel nur, ohne je anzulegen: für das Entschlüsseln. Dort
/// gibt es schon einen gespeicherten Wert; fehlt der Schlüssel dazu (Eintrag
/// entfernt, anderer Rechner, Sicherung ohne Schlüsselbund), würde ein neuer
/// Schlüssel ihn nicht öffnen, und kehrte der alte Eintrag zurück, stünde er
/// neben einem neuen. Darum hier bei „nicht gefunden“ nur der feste Satz.
pub fn schluessel_nur_lesen(lesen: &dyn Fn() -> Result<Zeroizing<Vec<u8>>, i32>) -> Result<Schluessel, String> {
    match lesen() {
        Ok(bytes) => aus_eintrag(&bytes),
        Err(nummer) => Err(schluesselbund_satz(nummer)),
    }
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

/// Fester Satz für einen Eintrag mit falscher Länge.
pub const LAENGE_SATZ: &str = "Der Eintrag für den Schlüssel der Aufgaben-Links hat nicht genau 32 Bytes; er wird nicht benutzt und nicht überschrieben.";

fn aus_eintrag(bytes: &[u8]) -> Result<Schluessel, String> {
    let mut s = Zeroizing::new([0u8; 32]);
    if bytes.len() != s.len() {
        return Err(LAENGE_SATZ.to_string());
    }
    s.copy_from_slice(bytes);
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

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
        assert_ne!(*neuer_schluessel().unwrap(), *neuer_schluessel().unwrap());
    }

    /// Die Nummern des Schlüsselbunds, als reine Funktion.
    #[test]
    fn fehlerart_aus_der_nummer() {
        assert_eq!(fehlerart(-25300), Fehlerart::NichtGefunden);
        assert_eq!(fehlerart(-25299), Fehlerart::GibtEsSchon);
        assert_eq!(fehlerart(-25308), Fehlerart::Gesperrt);
        assert_eq!(fehlerart(-25293), Fehlerart::Verweigert);
        for n in [-25294, -25307, -128, -34018, 0, 1, i32::MIN, i32::MAX] {
            assert_eq!(fehlerart(n), Fehlerart::Anderer(n), "{n}");
        }
    }

    /// Eine Ablage für die Tests: zählt Lesen und Anlegen und antwortet der Reihe nach.
    struct Ablage {
        lesen: RefCell<Vec<Result<Vec<u8>, i32>>>,
        anlegen: RefCell<Vec<Result<(), i32>>>,
        gelesen: Cell<u32>,
        angelegt: RefCell<Vec<[u8; 32]>>,
    }

    impl Ablage {
        fn neu(lesen: Vec<Result<Vec<u8>, i32>>, anlegen: Vec<Result<(), i32>>) -> Ablage {
            Ablage { lesen: RefCell::new(lesen), anlegen: RefCell::new(anlegen), gelesen: Cell::new(0), angelegt: RefCell::new(Vec::new()) }
        }
        fn holen(&self) -> Result<Schluessel, String> {
            schluessel_holen(
                &|| {
                    self.gelesen.set(self.gelesen.get() + 1);
                    self.lesen.borrow_mut().remove(0).map(Zeroizing::new)
                },
                &|b| {
                    self.angelegt.borrow_mut().push(*b);
                    self.anlegen.borrow_mut().remove(0)
                },
                &|| Ok(Zeroizing::new([5u8; 32])),
            )
        }
        fn nur_lesen(&self) -> Result<Schluessel, String> {
            schluessel_nur_lesen(&|| {
                self.gelesen.set(self.gelesen.get() + 1);
                self.lesen.borrow_mut().remove(0).map(Zeroizing::new)
            })
        }
    }

    /// Der Eintrag ist da: einmal lesen, nie anlegen.
    #[test]
    fn vorhandener_schluessel_wird_gelesen() {
        let a = Ablage::neu(vec![Ok(vec![9u8; 32])], vec![]);
        assert_eq!(*a.holen().unwrap(), [9u8; 32]);
        assert_eq!((a.gelesen.get(), a.angelegt.borrow().len()), (1, 0));
    }

    /// Nur „nicht gefunden“ legt an, und zwar genau einmal den neuen Schlüssel.
    #[test]
    fn nur_nicht_gefunden_legt_an() {
        let a = Ablage::neu(vec![Err(NICHT_GEFUNDEN)], vec![Ok(())]);
        assert_eq!(*a.holen().unwrap(), [5u8; 32]);
        assert_eq!((a.gelesen.get(), a.angelegt.borrow().as_slice()), (1, &[[5u8; 32]][..]));
    }

    /// Jeder andere Fehler beim Lesen: kein Schlüssel, kein Anlegen, der Satz mit der Nummer.
    #[test]
    fn anderer_fehler_legt_nie_an() {
        for nummer in [GESPERRT, VERWEIGERT, GIBT_ES_SCHON, -25294, -25307, -128, 1] {
            let a = Ablage::neu(vec![Err(nummer)], vec![]);
            let satz = a.holen().unwrap_err();
            assert!(satz.contains(&nummer.to_string()), "{satz}");
            assert_eq!((a.gelesen.get(), a.angelegt.borrow().len()), (1, 0), "{nummer}");
        }
        let a = Ablage::neu(vec![Err(GESPERRT)], vec![]);
        assert_eq!(a.holen().unwrap_err(), "Schlüsselbund gesperrt oder eine Sitzung ohne Anmeldung (-25308).");
        // Am Gerät gemessen: Nach einem Neubau ohne feste Signatur kommt -25293. Auch dann entsteht kein zweiter Schlüssel.
        let a = Ablage::neu(vec![Err(VERWEIGERT)], vec![]);
        assert_eq!(
            a.holen().unwrap_err(),
            "Der Schlüsselbund gibt den Schlüssel der Aufgaben-Links diesem Programm nicht (etwa nach einem Neubau ohne feste Signatur) (-25293)."
        );
        assert_eq!(a.angelegt.borrow().len(), 0);
    }

    /// „Gibt es schon“ beim Anlegen: genau ein weiterer Leseversuch, der
    /// vorhandene Schlüssel gilt, und es wird kein zweites Mal geschrieben.
    #[test]
    fn auf_gibt_es_schon_wird_nie_geschrieben() {
        let a = Ablage::neu(vec![Err(NICHT_GEFUNDEN), Ok(vec![9u8; 32])], vec![Err(GIBT_ES_SCHON)]);
        assert_eq!(*a.holen().unwrap(), [9u8; 32], "der Schlüssel des anderen Laufs gilt, nicht der eigene");
        assert_eq!((a.gelesen.get(), a.angelegt.borrow().len()), (2, 1));
        // Auch das zweite Lesen findet nichts: Fehler, kein weiterer Versuch, kein weiteres Anlegen.
        let a = Ablage::neu(vec![Err(NICHT_GEFUNDEN), Err(NICHT_GEFUNDEN)], vec![Err(GIBT_ES_SCHON)]);
        assert!(a.holen().is_err());
        assert_eq!((a.gelesen.get(), a.angelegt.borrow().len()), (2, 1));
    }

    /// Scheitert das Anlegen aus einem anderen Grund, gibt es keinen Schlüssel:
    /// Ein Schlüssel, der nicht gespeichert ist, würde Links unlesbar machen.
    #[test]
    fn gescheitertes_anlegen_gibt_keinen_schluessel() {
        for nummer in [GESPERRT, VERWEIGERT, -128] {
            let a = Ablage::neu(vec![Err(NICHT_GEFUNDEN)], vec![Err(nummer)]);
            assert!(a.holen().unwrap_err().contains(&nummer.to_string()));
            assert_eq!(a.gelesen.get(), 1);
        }
    }

    /// Ein Eintrag mit falscher Länge wird nicht benutzt und nicht ersetzt.
    #[test]
    fn falsche_laenge_wird_nicht_ersetzt() {
        for laenge in [0, 31, 33, 64] {
            let a = Ablage::neu(vec![Ok(vec![9u8; laenge])], vec![]);
            assert_eq!(a.holen().unwrap_err(), LAENGE_SATZ);
            assert_eq!(a.angelegt.borrow().len(), 0);
        }
    }

    /// Das Entschlüsseln holt nur: Bei „nicht gefunden“ entsteht kein
    /// Schlüssel, und es kommt der feste Satz. Auch sonst wird nie angelegt
    /// und genau einmal gelesen.
    #[test]
    fn lesen_allein_legt_nie_an() {
        let a = Ablage::neu(vec![Err(NICHT_GEFUNDEN)], vec![]);
        assert_eq!(
            a.nur_lesen().unwrap_err(),
            "Der Schlüssel der Aufgaben-Links fehlt im Schlüsselbund (-25300); gespeicherte Links sind nicht lesbar und laufen nur ab."
        );
        assert_eq!((a.gelesen.get(), a.angelegt.borrow().len()), (1, 0));
        let a = Ablage::neu(vec![Ok(vec![9u8; 32])], vec![]);
        assert_eq!(*a.nur_lesen().unwrap(), [9u8; 32]);
        assert_eq!((a.gelesen.get(), a.angelegt.borrow().len()), (1, 0));
        for nummer in [GESPERRT, VERWEIGERT, GIBT_ES_SCHON, -128] {
            let a = Ablage::neu(vec![Err(nummer)], vec![]);
            assert!(a.nur_lesen().unwrap_err().contains(&nummer.to_string()));
            assert_eq!((a.gelesen.get(), a.angelegt.borrow().len()), (1, 0));
        }
        let a = Ablage::neu(vec![Ok(vec![9u8; 31])], vec![]);
        assert_eq!(a.nur_lesen().unwrap_err(), LAENGE_SATZ);
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
        saetze.extend([NICHT_GEFUNDEN, GIBT_ES_SCHON, GESPERRT, VERWEIGERT, -128].into_iter().map(schluesselbund_satz));
        saetze.extend([VERSIEGELN_SATZ, ZUFALL_SATZ, LAENGE_SATZ].into_iter().map(str::to_string));
        // Die Sätze, die beim echten Scheitern entstehen.
        let mut kaputt = geheim.clone();
        kaputt[40] ^= 1;
        saetze.push(format_satz(oeffnen(&SCHLUESSEL, &kaputt).unwrap_err()).to_string());
        saetze.push(Ablage::neu(vec![Ok(SCHLUESSEL[..31].to_vec())], vec![]).holen().unwrap_err());
        let schluessel_hex: String = SCHLUESSEL.iter().map(|b| format!("{b:02x}")).collect();
        let wert_hex: String = geheim.iter().map(|b| format!("{b:02x}")).collect();
        for satz in &saetze {
            for teil in [PROBE, "erfunden", "0815", "beispiel.invalid", "token", &schluessel_hex[..16], &wert_hex[6..22], "07070707", "[7, 7"] {
                assert!(!satz.contains(teil), "{teil} in: {satz}");
            }
        }
    }
}
