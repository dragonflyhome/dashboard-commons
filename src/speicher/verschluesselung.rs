//! Verschlüsselung der Links in `aufgaben_links` (Etappe 5a) mit DPAPI
//! (`CryptProtectData`), unter dem Windows-Konto, unter dem der Server läuft
//! (im Betrieb das Dienstkonto, in Tests das eigene Konto).
//!
//! Warum: Die tägliche Sicherung ist eine Kopie der Datei, bleibt 14 Tage
//! liegen und ist für das Konto des Inhabers lesbar. Ohne Verschlüsselung stünde ein
//! Anmeldelink dort noch lange nach Ablauf, lesbar für jedes Programm unter
//! ihrem Konto. Mit DPAPI kann nur dasselbe Konto auf demselben PC den Link
//! wieder lesen; eine kopierte Datei hilft niemandem sonst.
//!
//! Dazu ein fester Entropie-Wert der Anwendung (`ENTROPIE`). Er trennt die
//! Daten dieser Anwendung von anderen DPAPI-Daten desselben Kontos (ein
//! Blob aus einem anderen Zusammenhang lässt sich hier nicht unterschieben,
//! und ein beliebiger Aufruf ohne den Wert liefert nichts). Er ist aber kein
//! Geheimnis: Er steht im Quelltext und im Programm. Gegen ein Programm, das
//! unter demselben Konto läuft (im Betrieb dem Dienstkonto), schützt er nicht;
//! es kann den Wert lesen und DPAPI damit aufrufen. Den Schutz gibt allein
//! das Konto: Andere Konten und andere PCs lesen nichts. `CRYPTPROTECT_UI_FORBIDDEN`: nie ein Dialog
//! (der Dienst hat keinen Bildschirm). Schlägt etwas fehl, kommt ein Fehler
//! mit dem Windows-Fehlercode zurück, nie der Klartext.
//!
//! Außerhalb von Windows gibt es DPAPI nicht. Dort gilt die Mischform
//! (`mischform`): XChaCha20-Poly1305 im Programm, der Schlüssel von 32 Bytes
//! als Datei `aufgaben-links-schluessel` im Schlüsselordner, immer neben dem
//! Schlüssel der Ausweise und mit denselben Prüfungen (eigenes Konto, Rechte
//! 700 und 600, keine Verknüpfung, Anlegen ohne Halbstand). Unter macOS und
//! Linux derselbe Code. Jeder Speicher trägt die Quelle seines Schlüssels
//! (`LinksSchluessel`); ohne Ort werden Aufgaben mit Link abgelehnt.

/// Fester Zusatzwert der Anwendung für DPAPI: trennt diese Daten von
/// anderen DPAPI-Daten desselben Kontos, ist aber kein Geheimnis. Nie ändern: sonst sind alle
/// gespeicherten Links unlesbar (sie laufen dann nur ab).
pub const ENTROPIE: &[u8] = b"dashboard/aufgaben_links/dpapi/v1";

/// Dateiname des Schlüssels der Links. Er liegt immer neben dem Schlüssel
/// der Ausweise, im selben Ordner: ein Ort, eine Prüfung.
#[cfg(not(windows))]
pub const LINKS_SCHLUESSEL_DATEI: &str = "aufgaben-links-schluessel";

/// Satz, wenn es für den Schlüssel der Links keinen Ort gibt.
#[cfg(not(windows))]
pub const KEIN_ORT_SATZ: &str = "Für den Schlüssel der Aufgaben-Links gibt es keinen Ort; Aufgaben mit Link werden abgelehnt.";

/// Die Sätze und die Regel für die Schlüsseldatei der Links. Anders als beim
/// Schlüssel der Ausweise wird die Ausnahme für die Sicherung auch gesetzt,
/// wenn die Datei in einem schon vorhandenen Schlüsselordner neu entsteht.
#[cfg(unix)]
pub const RAHMEN_LINKS: crate::passkey::geheim::Rahmen = crate::passkey::geheim::Rahmen {
    folge: "Aufgaben mit Link werden abgelehnt, gespeicherte Links nicht gelesen",
    folge_im_satz: "Aufgaben mit Link werden abgelehnt, gespeicherte Links nicht gelesen",
    name: "Schlüssel für die Aufgaben-Links",
    nach_dem_entfernen: "dann entsteht mit dem nächsten Link ein neuer Schlüssel; gespeicherte Links laufen dann nur noch ab",
    fehlt: "Der Schlüssel der Aufgaben-Links fehlt; gespeicherte Links sind nicht lesbar und laufen nur ab.",
    ausnahme_bei_neuer_datei: true,
};

/// Der Ort des Schlüssels der Links, abgeleitet vom Ort des Schlüssels der
/// Ausweise: dieselbe Stelle, anderer Dateiname. `None`, wenn es für die
/// Ausweise keinen Ort gibt oder beides dieselbe Datei ergäbe.
#[cfg(not(windows))]
pub fn links_schluessel_neben(jwt_datei: Option<&std::path::Path>) -> Option<std::path::PathBuf> {
    let jwt = jwt_datei?;
    jwt.file_name()?;
    let links = jwt.with_file_name(LINKS_SCHLUESSEL_DATEI);
    // Ohne Rücksicht auf Groß- und Kleinschreibung: APFS unterscheidet sie
    // gewöhnlich nicht, „Aufgaben-Links-Schluessel“ wäre dort dieselbe Datei.
    let gleich = |a: &std::path::Path, b: &std::path::Path| a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase();
    if gleich(&links, jwt) { None } else { Some(links) }
}

/// Der Pfad, unter dem der Schlüssel der Aufgaben-Links im Schlüsselbund
/// des Mac-Helfers geführt wird (`helfer`). Jeder Schlüssel steht unter dem
/// Pfad der Nutzlast, die er öffnet.
#[cfg(unix)]
pub const LINKS_PFAD: &str = "speicher/aufgaben/links";

/// Satz, solange der Schlüssel noch nicht mit dem Helfer geöffnet ist.
#[cfg(unix)]
pub const VOM_HELFER_FEHLT_SATZ: &str =
    "Der Schlüssel der Aufgaben-Links wird mit dem Mac-Helfer geöffnet und liegt noch nicht vor; Aufgaben mit Link werden so lange abgelehnt.";

/// Woher ein Speicher den Schlüssel der Links nimmt.
#[cfg(not(windows))]
enum Quelle {
    /// Es gibt keinen Ort: Aufgaben mit Link werden abgelehnt.
    KeinOrt,
    /// Die Schlüsseldatei an diesem Ort.
    Datei(std::path::PathBuf),
    /// Der Schlüssel liegt im Schlüsselbund des Mac-Helfers unter dem Pfad
    /// dieser Nutzlast, nur im Arbeitsspeicher; bis er geöffnet ist, fehlt er.
    #[cfg(unix)]
    Helfer(crate::helfer::Bund),
    /// Nur im Testbau der Bibliothek: ein fester, erfundener Schlüssel. Das
    /// Programm und die Integrationstests kennen diese Variante nicht.
    #[cfg(test)]
    Test,
}

/// Fester, erfundener Schlüssel für die Tests der Bibliothek.
#[cfg(all(not(windows), test))]
const TEST_SCHLUESSEL: &[u8; 32] = b"dashboard-test-schluessel-012345";

/// Der Schlüssel der Links eines Speichers außerhalb von Windows: die Quelle
/// und der Zwischenspeicher. Geladen wird erst beim ersten Link, nie beim
/// Öffnen des Speichers; danach liegt der Schlüssel hier, in einem Puffer, der
/// beim Freigeben überschrieben wird. Ein Fehler wird nicht gemerkt, der
/// nächste Bedarf versucht es wieder (`mischform::zwischengespeichert`).
#[cfg(not(windows))]
pub struct LinksSchluessel {
    quelle: Quelle,
    ablage: std::sync::Mutex<Option<super::mischform::Schluessel>>,
}

#[cfg(not(windows))]
impl LinksSchluessel {
    /// Ohne Ort: Jede Aufgabe mit Link wird abgelehnt, Lesen liefert nichts.
    pub fn kein_ort() -> LinksSchluessel {
        LinksSchluessel { quelle: Quelle::KeinOrt, ablage: std::sync::Mutex::new(None) }
    }

    /// Mit der Schlüsseldatei an `ort`; `None` heißt kein Ort.
    pub fn an(ort: Option<std::path::PathBuf>) -> LinksSchluessel {
        match ort {
            Some(p) => LinksSchluessel { quelle: Quelle::Datei(p), ablage: std::sync::Mutex::new(None) },
            None => Self::kein_ort(),
        }
    }

    /// Der Schlüssel kommt aus dem Schlüsselbund des Mac-Helfers.
    #[cfg(unix)]
    pub fn vom_helfer(bund: crate::helfer::Bund) -> LinksSchluessel {
        LinksSchluessel { quelle: Quelle::Helfer(bund), ablage: std::sync::Mutex::new(None) }
    }

    /// Nur im Testbau: der feste Testschlüssel.
    #[cfg(test)]
    pub fn fuer_tests() -> LinksSchluessel {
        LinksSchluessel { quelle: Quelle::Test, ablage: std::sync::Mutex::new(None) }
    }

    /// Ob es einen Ort gibt (oder im Testbau den Testschlüssel).
    pub fn hat_ort(&self) -> bool {
        !matches!(self.quelle, Quelle::KeinOrt)
    }

    /// `anlegen`: Nur das Verschlüsseln darf einen fehlenden Schlüssel
    /// anlegen. Das Entschlüsseln holt nur; fehlt der Schlüssel zu einem
    /// gespeicherten Wert, kommt der feste Satz, und es entsteht kein neuer.
    fn schluessel(&self, anlegen: bool) -> Result<super::mischform::Schluessel, String> {
        match &self.quelle {
            Quelle::KeinOrt => Err(KEIN_ORT_SATZ.to_string()),
            #[cfg(test)]
            Quelle::Test => Ok(zeroize::Zeroizing::new(*TEST_SCHLUESSEL)),
            Quelle::Datei(pfad) => super::mischform::zwischengespeichert(&self.ablage, &|| aus_der_datei(pfad, anlegen)),
            // Nie zwischenspeichern und nie anlegen: Es gilt, was im Schlüsselbund gerade geöffnet ist.
            #[cfg(unix)]
            Quelle::Helfer(bund) => {
                bund.lock().unwrap_or_else(std::sync::PoisonError::into_inner).holen(LINKS_PFAD).ok_or_else(|| VOM_HELFER_FEHLT_SATZ.to_string())
            }
        }
    }

    /// Verschlüsselt `klartext` mit dem Schlüssel der Links (Mischform, Format DL1).
    pub fn verschliessen(&self, klartext: &[u8]) -> Result<Vec<u8>, String> {
        use super::mischform;
        let schluessel = self.schluessel(true)?;
        mischform::versiegeln(&schluessel, &mischform::neue_nonce()?, klartext)
    }

    /// Entschlüsselt, was `verschliessen` mit demselben Schlüssel geschrieben
    /// hat. Der Klartext liegt in einem Puffer, der beim Freigeben überschrieben wird.
    pub fn oeffnen(&self, geheim: &[u8]) -> Result<zeroize::Zeroizing<Vec<u8>>, String> {
        use super::mischform;
        #[cfg(test)]
        ENTSCHLUESSELT.with(|n| n.set(n.get() + 1));
        // Nur holen, nie anlegen: Zu einem gespeicherten Wert gibt es den Schlüssel, oder er fehlt.
        let schluessel = self.schluessel(false)?;
        mischform::oeffnen(&schluessel, geheim).map_err(|f| mischform::format_satz(f).to_string())
    }
}

/// Die Schlüsseldatei über den Kern, der auch den Schlüssel der Ausweise
/// prüft, lädt und anlegt (`passkey::geheim::schluessel_datei`).
#[cfg(unix)]
fn aus_der_datei(pfad: &std::path::Path, anlegen: bool) -> Result<super::mischform::Schluessel, String> {
    crate::passkey::geheim::schluessel_datei(pfad, &RAHMEN_LINKS, anlegen)
}

/// Auf anderen Systemen als Windows, macOS und Linux gibt es den Kern nicht.
#[cfg(all(not(windows), not(unix)))]
fn aus_der_datei(_pfad: &std::path::Path, _anlegen: bool) -> Result<super::mischform::Schluessel, String> {
    Err(KEIN_ORT_SATZ.to_string())
}

/// Nur im Testbau außerhalb von Windows: verschlüsselt mit dem festen
/// Testschlüssel, für Tests, die ohne Speicher direkt in die Tabelle schreiben.
#[cfg(all(not(windows), test))]
pub fn verschluesseln(klartext: &[u8]) -> Result<Vec<u8>, String> {
    LinksSchluessel::fuer_tests().verschliessen(klartext)
}

/// Nur im Testbau außerhalb von Windows: das Gegenstück mit dem festen Testschlüssel.
#[cfg(all(not(windows), test))]
pub fn entschluesseln(geheim: &[u8]) -> Result<zeroize::Zeroizing<Vec<u8>>, String> {
    LinksSchluessel::fuer_tests().oeffnen(geheim)
}

/// Verschlüsselt `klartext` für das eigene Konto.
#[cfg(windows)]
pub fn verschluesseln(klartext: &[u8]) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::Security::Cryptography::{CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData};
    let ein = blob(klartext);
    let entropie = blob(ENTROPIE);
    let mut aus = leerer_blob();
    // SAFETY: Ein- und Ausgabe zeigen auf gültigen Speicher, der während des
    // Aufrufs lebt; DPAPI liest `ein` und `entropie` nur. `aus` legt Windows
    // mit LocalAlloc an; `ausgabe_nehmen` gibt ihn frei.
    let ok = unsafe { CryptProtectData(&ein, std::ptr::null(), &entropie, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut aus) };
    if ok == 0 {
        return Err(format!("CryptProtectData: Windows-Fehler {}", letzter_fehler()));
    }
    // SAFETY: Nach Erfolg hat Windows `aus` gefüllt.
    Ok(unsafe { ausgabe_nehmen(aus, false) })
}

#[cfg(test)]
thread_local! {
    /// Nur in Tests: wie oft dieser Thread entschlüsselt hat (für den Nachweis,
    /// dass die Zählung der offenen Aufgaben keinen Link entschlüsselt).
    static ENTSCHLUESSELT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Nur in Tests: Zahl der Entschlüsselungen in diesem Thread.
#[cfg(test)]
pub fn entschluesselt_in_diesem_thread() -> u64 {
    ENTSCHLUESSELT.with(std::cell::Cell::get)
}

/// Entschlüsselt, was `verschluesseln` unter demselben Konto geschrieben hat.
/// Der Klartext liegt in einem Puffer, der beim Freigeben überschrieben wird.
#[cfg(windows)]
pub fn entschluesseln(geheim: &[u8]) -> Result<zeroize::Zeroizing<Vec<u8>>, String> {
    #[cfg(test)]
    ENTSCHLUESSELT.with(|n| n.set(n.get() + 1));
    use windows_sys::Win32::Security::Cryptography::{CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData};
    let ein = blob(geheim);
    let entropie = blob(ENTROPIE);
    let mut aus = leerer_blob();
    // SAFETY: wie bei `verschluesseln`; keine Beschreibung gewünscht (null).
    let ok = unsafe { CryptUnprotectData(&ein, std::ptr::null_mut(), &entropie, std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut aus) };
    if ok == 0 {
        return Err(format!("CryptUnprotectData: Windows-Fehler {}", letzter_fehler()));
    }
    // SAFETY: Nach Erfolg hat Windows `aus` gefüllt; der Klartext wird nach dem
    // Kopieren im Windows-Puffer überschrieben.
    Ok(zeroize::Zeroizing::new(unsafe { ausgabe_nehmen(aus, true) }))
}

#[cfg(windows)]
fn blob(daten: &[u8]) -> windows_sys::Win32::Security::Cryptography::CRYPT_INTEGER_BLOB {
    windows_sys::Win32::Security::Cryptography::CRYPT_INTEGER_BLOB {
        cbData: daten.len() as u32,
        // DPAPI liest den Eingang nur; der Typ verlangt trotzdem `*mut`.
        pbData: daten.as_ptr() as *mut u8,
    }
}

#[cfg(windows)]
fn leerer_blob() -> windows_sys::Win32::Security::Cryptography::CRYPT_INTEGER_BLOB {
    windows_sys::Win32::Security::Cryptography::CRYPT_INTEGER_BLOB { cbData: 0, pbData: std::ptr::null_mut() }
}

#[cfg(windows)]
fn letzter_fehler() -> u32 {
    // SAFETY: liest nur den Fehlercode des Threads.
    unsafe { windows_sys::Win32::Foundation::GetLastError() }
}

/// Kopiert die Ausgabe von DPAPI, überschreibt sie auf Wunsch und gibt sie frei.
///
/// # Safety
/// `aus` muss eine von DPAPI mit LocalAlloc gefüllte Ausgabe sein.
#[cfg(windows)]
unsafe fn ausgabe_nehmen(aus: windows_sys::Win32::Security::Cryptography::CRYPT_INTEGER_BLOB, ueberschreiben: bool) -> Vec<u8> {
    use zeroize::Zeroize;
    if aus.pbData.is_null() {
        return Vec::new();
    }
    // SAFETY: laut Aufrufer `cbData` gültige Bytes ab `pbData`.
    let teil = unsafe { std::slice::from_raw_parts_mut(aus.pbData, aus.cbData as usize) };
    let kopie = teil.to_vec();
    if ueberschreiben {
        teil.zeroize();
    }
    // SAFETY: Der Puffer stammt aus LocalAlloc (DPAPI) und wird genau einmal freigegeben.
    unsafe { windows_sys::Win32::Foundation::LocalFree(aus.pbData.cast()) };
    kopie
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Erfundener Testwert, kein echter Link.
    const PROBE: &str = "https://beispiel.invalid/anmelden?token=erfunden-0815";

    #[test]
    fn hin_und_zurueck() {
        let geheim = verschluesseln(PROBE.as_bytes()).unwrap();
        assert!(!geheim.windows(PROBE.len()).any(|w| w == PROBE.as_bytes()), "Klartext im Ergebnis");
        assert!(!geheim.windows(13).any(|w| w == b"erfunden-0815"), "Teil des Klartexts im Ergebnis");
        let klar = entschluesseln(&geheim).unwrap();
        assert_eq!(klar.as_slice(), PROBE.as_bytes());
    }

    #[cfg(windows)]
    #[test]
    fn ohne_entropie_oder_veraendert_nicht_lesbar() {
        use windows_sys::Win32::Security::Cryptography::{CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData};
        let geheim = verschluesseln(PROBE.as_bytes()).unwrap();
        // Ohne den Entropie-Wert der Anwendung: Windows lehnt ab.
        let ein = blob(&geheim);
        let mut aus = leerer_blob();
        let ok = unsafe {
            CryptUnprotectData(&ein, std::ptr::null_mut(), std::ptr::null(), std::ptr::null(), std::ptr::null(), CRYPTPROTECT_UI_FORBIDDEN, &mut aus)
        };
        assert_eq!(ok, 0, "ohne Entropie darf nichts herauskommen");
        // Ein verändertes Byte: Fehler statt Unsinn.
        let mut kaputt = geheim.clone();
        let mitte = kaputt.len() / 2;
        kaputt[mitte] ^= 0xff;
        let e = entschluesseln(&kaputt).unwrap_err();
        assert!(e.starts_with("CryptUnprotectData: Windows-Fehler"), "{e}");
        assert!(!e.contains("erfunden"), "{e}");
    }

    // ----- Schlüsseldatei der Links außerhalb von Windows (Schritt 4b) -----

    /// Ein eigener Temp-Ordner je Test; nie der echte Schlüsselordner.
    #[cfg(unix)]
    fn ordner(name: &str) -> std::path::PathBuf {
        let o = std::env::temp_dir().join(format!("dashboard-links-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&o);
        o
    }

    /// Der Ort des Schlüssels der Links: immer neben dem der Ausweise, mit
    /// eigenem Namen; ohne Ort der Ausweise oder bei derselben Datei keiner.
    #[cfg(not(windows))]
    #[test]
    fn links_schluessel_liegt_neben_dem_der_ausweise() {
        use std::path::{Path, PathBuf};
        let neben = |p: &str| links_schluessel_neben(Some(Path::new(p)));
        assert_eq!(neben("/x/dashboard-schluessel/jwt-schluessel"), Some(PathBuf::from("/x/dashboard-schluessel/aufgaben-links-schluessel")));
        assert_eq!(neben("/daten/geheim/irgendein-name"), Some(PathBuf::from("/daten/geheim/aufgaben-links-schluessel")));
        assert_eq!(neben("jwt-schluessel"), Some(PathBuf::from("aufgaben-links-schluessel")));
        // Dieselbe Datei wie der Schlüssel der Ausweise: kein Ort, auch in anderer Groß- und Kleinschreibung.
        assert_eq!(neben("/x/dashboard-schluessel/aufgaben-links-schluessel"), None);
        assert_eq!(neben("/x/dashboard-schluessel/Aufgaben-Links-Schluessel"), None);
        assert_eq!(neben("/x/dashboard-schluessel/AUFGABEN-LINKS-SCHLUESSEL"), None);
        // Kein Dateiname oder gar kein Ort für die Ausweise: kein Ort.
        assert_eq!(neben("/"), None);
        assert_eq!(neben(""), None);
        assert_eq!(links_schluessel_neben(None), None);
    }

    /// Ohne Ort wird nichts verschlüsselt und nichts gelesen, mit dem festen Satz.
    #[cfg(not(windows))]
    #[test]
    fn ohne_ort_wird_abgelehnt() {
        for quelle in [LinksSchluessel::kein_ort(), LinksSchluessel::an(None)] {
            assert!(!quelle.hat_ort());
            assert_eq!(quelle.verschliessen(PROBE.as_bytes()).unwrap_err(), KEIN_ORT_SATZ);
            let geheim = verschluesseln(PROBE.as_bytes()).unwrap();
            assert_eq!(quelle.oeffnen(&geheim).unwrap_err(), KEIN_ORT_SATZ);
        }
        assert_eq!(KEIN_ORT_SATZ, "Für den Schlüssel der Aufgaben-Links gibt es keinen Ort; Aufgaben mit Link werden abgelehnt.");
    }

    /// Die Schlüsseldatei entsteht erst beim ersten Verschlüsseln, mit engen
    /// Rechten; eine zweite Quelle am selben Ort liest, was die erste schrieb.
    #[cfg(unix)]
    #[test]
    fn unix_schluesseldatei_entsteht_beim_ersten_link() {
        use std::os::unix::fs::MetadataExt;
        let o = ordner("erster-link");
        let ort = o.join("dashboard-schluessel").join(LINKS_SCHLUESSEL_DATEI);
        let erste = LinksSchluessel::an(Some(ort.clone()));
        assert!(erste.hat_ort());
        assert!(!o.exists(), "das Setzen des Orts legt nichts an");
        let geheim = erste.verschliessen(PROBE.as_bytes()).unwrap();
        assert_eq!(&geheim[..3], b"DL1");
        let m = std::fs::metadata(&ort).unwrap();
        assert_eq!((m.len(), m.mode() & 0o777, m.uid()), (32, 0o600, crate::unix::eigene_uid()));
        assert_eq!(std::fs::metadata(ort.parent().unwrap()).unwrap().mode() & 0o777, 0o700);
        assert_eq!(std::fs::read_dir(ort.parent().unwrap()).unwrap().count(), 1, "keine Zwischendatei bleibt liegen");
        // Der Wert trägt weder den Klartext noch den Schlüssel.
        let schluessel = std::fs::read(&ort).unwrap();
        assert!(!geheim.windows(schluessel.len()).any(|w| w == schluessel.as_slice()));
        assert!(!geheim.windows(13).any(|w| w == b"erfunden-0815"));
        // Dieselbe Quelle und eine zweite am selben Ort lesen den Link; die Datei bleibt dieselbe.
        assert_eq!(erste.oeffnen(&geheim).unwrap().as_slice(), PROBE.as_bytes());
        let zweite = LinksSchluessel::an(Some(ort.clone()));
        assert_eq!(zweite.oeffnen(&geheim).unwrap().as_slice(), PROBE.as_bytes());
        assert_eq!(std::fs::read(&ort).unwrap(), schluessel);
        let _ = std::fs::remove_dir_all(&o);
    }

    /// Nur das Verschlüsseln legt an. Das Entschlüsseln an einem Ort ohne
    /// Schlüssel legt nichts an, weder Ordner noch Datei, und meldet den
    /// festen Satz; ein Fehler wird nicht gemerkt.
    #[cfg(unix)]
    #[test]
    fn unix_entschluesseln_legt_nie_an() {
        let o = ordner("nur-lesen");
        let ort = o.join("dashboard-schluessel").join(LINKS_SCHLUESSEL_DATEI);
        let geheim = LinksSchluessel::an(Some(ort.clone())).verschliessen(PROBE.as_bytes()).unwrap();
        // Ein anderer Ort, an dem es nichts gibt: kein Ordner entsteht.
        let fremd = o.join("anderer-ordner").join(LINKS_SCHLUESSEL_DATEI);
        let andere = LinksSchluessel::an(Some(fremd.clone()));
        assert_eq!(andere.oeffnen(&geheim).unwrap_err(), RAHMEN_LINKS.fehlt);
        assert!(!fremd.parent().unwrap().exists(), "beim Lesen entsteht kein Ordner");
        // Der Ordner ist da, die Datei fehlt: keine Datei entsteht.
        std::fs::DirBuilder::new().create(fremd.parent().unwrap()).unwrap();
        std::os::unix::fs::PermissionsExt::set_mode(&mut std::fs::metadata(fremd.parent().unwrap()).unwrap().permissions(), 0o700);
        std::fs::set_permissions(fremd.parent().unwrap(), std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
        assert_eq!(andere.oeffnen(&geheim).unwrap_err(), RAHMEN_LINKS.fehlt);
        assert_eq!(std::fs::read_dir(fremd.parent().unwrap()).unwrap().count(), 0, "beim Lesen entsteht keine Datei");
        // Derselbe Schlüssel am anderen Ort öffnet nichts: Dort entsteht mit dem ersten Link ein eigener.
        let eigener = andere.verschliessen(b"anderer Link").unwrap();
        assert!(fremd.is_file());
        assert_eq!(andere.oeffnen(&geheim).unwrap_err(), crate::speicher::mischform::format_satz(crate::speicher::mischform::FormatFehler::Pruefsumme));
        assert_eq!(andere.oeffnen(&eigener).unwrap().as_slice(), b"anderer Link");
        let _ = std::fs::remove_dir_all(&o);
    }

    /// Auch mit `anlegen = false` laufen die Prüfungen: Ein zu offener Ordner
    /// oder eine zu offene Datei wird abgelehnt, mit den Sätzen für die Links.
    #[cfg(unix)]
    #[test]
    fn unix_pruefungen_gelten_auch_beim_lesen() {
        use std::os::unix::fs::PermissionsExt;
        let o = ordner("pruefen");
        let ort = o.join("dashboard-schluessel").join(LINKS_SCHLUESSEL_DATEI);
        let quelle = LinksSchluessel::an(Some(ort.clone()));
        let geheim = quelle.verschliessen(PROBE.as_bytes()).unwrap();
        let vorher = std::fs::read(&ort).unwrap();
        // Zu offene Datei: abgelehnt, nicht nachgebessert, mit der Folge für gespeicherte Links im Rat.
        std::fs::set_permissions(&ort, std::fs::Permissions::from_mode(0o644)).unwrap();
        let neu = LinksSchluessel::an(Some(ort.clone()));
        let e = neu.oeffnen(&geheim).unwrap_err();
        assert!(e.contains("für andere Konten lesbar") && e.contains("gespeicherte Links laufen dann nur noch ab"), "{e}");
        assert!(e.contains("Aufgaben mit Link werden abgelehnt") && !e.contains("Passkey"), "{e}");
        assert!(neu.verschliessen(b"x").is_err(), "auch das Verschlüsseln nimmt den zu offenen Schlüssel nicht");
        assert_eq!(std::fs::metadata(&ort).unwrap().permissions().mode() & 0o777, 0o644, "nichts nachgebessert");
        assert_eq!(std::fs::read(&ort).unwrap(), vorher, "nichts überschrieben");
        // Wieder eng: geht; dann zu offener Ordner: abgelehnt.
        std::fs::set_permissions(&ort, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(neu.oeffnen(&geheim).unwrap().as_slice(), PROBE.as_bytes(), "ein Fehler wird nicht gemerkt");
        std::fs::set_permissions(ort.parent().unwrap(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let e = LinksSchluessel::an(Some(ort.clone())).oeffnen(&geheim).unwrap_err();
        assert!(e.contains("für andere Konten offen") && e.contains("Aufgaben mit Link werden abgelehnt"), "{e}");
        std::fs::set_permissions(ort.parent().unwrap(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let _ = std::fs::remove_dir_all(&o);
    }

    /// Die Ausnahme für die Sicherung beim Schlüssel der Links: genau einmal,
    /// auch wenn die Datei in einem schon vorhandenen Ordner neu entsteht,
    /// und erst nach dem gelungenen Anlegen. Beim Lesen und beim zweiten
    /// Laden nie. Gegenprobe: Beim Schlüssel der Ausweise bleibt es dabei,
    /// dass ein vorhandener Ordner keine Ausnahme auslöst.
    #[cfg(unix)]
    #[test]
    fn unix_ausnahme_auch_bei_neuer_datei_im_vorhandenen_ordner() {
        use crate::passkey::geheim::{RAHMEN_AUSWEISE, schluessel_datei_mit};
        use std::cell::RefCell;
        let gerufen: RefCell<Vec<std::path::PathBuf>> = RefCell::new(Vec::new());
        let merken = |p: &std::path::Path| {
            gerufen.borrow_mut().push(p.to_path_buf());
            Ok(())
        };
        let o = ordner("ausnahme");
        let schluesselordner = o.join("dashboard-schluessel");
        // Der Schlüssel der Ausweise legt den Ordner an: eine Ausnahme, für den Ordner.
        schluessel_datei_mit(&schluesselordner.join("jwt-schluessel"), &RAHMEN_AUSWEISE, true, &merken).unwrap();
        assert_eq!(gerufen.borrow().as_slice(), std::slice::from_ref(&schluesselordner));
        // Lesen der Links ohne Datei: nichts entsteht, keine Ausnahme.
        let links = schluesselordner.join(LINKS_SCHLUESSEL_DATEI);
        assert_eq!(schluessel_datei_mit(&links, &RAHMEN_LINKS, false, &merken).unwrap_err(), RAHMEN_LINKS.fehlt);
        assert_eq!(gerufen.borrow().len(), 1);
        // Die Links-Datei entsteht im vorhandenen Ordner: genau eine weitere Ausnahme.
        let erster = schluessel_datei_mit(&links, &RAHMEN_LINKS, true, &merken).unwrap();
        assert_eq!(gerufen.borrow().as_slice(), [schluesselordner.clone(), schluesselordner.clone()]);
        // Zweites Laden, mit und ohne Anlegen: derselbe Schlüssel, keine Ausnahme mehr.
        assert_eq!(*schluessel_datei_mit(&links, &RAHMEN_LINKS, true, &merken).unwrap(), *erster);
        assert_eq!(*schluessel_datei_mit(&links, &RAHMEN_LINKS, false, &merken).unwrap(), *erster);
        assert_eq!(gerufen.borrow().len(), 2);
        // Neuer Ordner und neue Datei in einem Zug: die Ausnahme nur einmal, nicht doppelt.
        gerufen.borrow_mut().clear();
        let frisch = o.join("zweiter-ordner");
        schluessel_datei_mit(&frisch.join(LINKS_SCHLUESSEL_DATEI), &RAHMEN_LINKS, true, &merken).unwrap();
        assert_eq!(gerufen.borrow().as_slice(), std::slice::from_ref(&frisch));
        // Gegenprobe Ausweise: neue Datei im vorhandenen Ordner löst nichts aus.
        gerufen.borrow_mut().clear();
        schluessel_datei_mit(&frisch.join("jwt-schluessel"), &RAHMEN_AUSWEISE, true, &merken).unwrap();
        assert!(gerufen.borrow().is_empty());
        // Scheitert die Ausnahme, ist der Schlüssel trotzdem da.
        let scheitert = |_: &std::path::Path| Err("erfunden".to_string());
        let dritter = o.join("dritter-ordner");
        std::fs::DirBuilder::new().create(&dritter).unwrap();
        std::fs::set_permissions(&dritter, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
        assert!(schluessel_datei_mit(&dritter.join(LINKS_SCHLUESSEL_DATEI), &RAHMEN_LINKS, true, &scheitert).is_ok());
        let _ = std::fs::remove_dir_all(&o);
    }

    /// Die zwei Schlüssel im selben Ordner sind getrennt: verschiedene Dateien
    /// und verschiedene Bytes, und das Anlegen des einen lässt den anderen,
    /// wie er ist.
    #[cfg(unix)]
    #[test]
    fn unix_getrennt_vom_schluessel_der_ausweise() {
        let o = ordner("getrennt");
        let jwt = o.join("dashboard-schluessel").join(crate::passkey::geheim::SCHLUESSEL_DATEI);
        crate::passkey::geheim::laden_oder_anlegen_mit(&jwt, &|_| Ok(())).unwrap();
        let jwt_bytes = std::fs::read(&jwt).unwrap();
        let ort = links_schluessel_neben(Some(&jwt)).unwrap();
        assert_eq!(ort.parent(), jwt.parent());
        assert_ne!(ort, jwt);
        LinksSchluessel::an(Some(ort.clone())).verschliessen(PROBE.as_bytes()).unwrap();
        let links_bytes = std::fs::read(&ort).unwrap();
        assert_eq!(links_bytes.len(), 32);
        assert_ne!(links_bytes, jwt_bytes, "zwei verschiedene Schlüssel");
        assert_eq!(std::fs::read(&jwt).unwrap(), jwt_bytes, "der Schlüssel der Ausweise bleibt, wie er ist");
        let mut namen: Vec<String> = std::fs::read_dir(jwt.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        namen.sort();
        assert_eq!(namen, [LINKS_SCHLUESSEL_DATEI, crate::passkey::geheim::SCHLUESSEL_DATEI], "genau die zwei Dateien, keine Zwischendatei");
        // Der Schlüssel der Ausweise lädt weiter denselben.
        crate::passkey::geheim::laden_oder_anlegen_mit(&jwt, &|_| Ok(())).unwrap();
        assert_eq!(std::fs::read(&jwt).unwrap(), jwt_bytes);
        let _ = std::fs::remove_dir_all(&o);
    }

    /// Ein Pfad ohne Ordner oder ohne Dateinamen ergibt einen festen Satz, und nichts entsteht.
    #[cfg(unix)]
    #[test]
    fn unix_pfad_ohne_ordner_oder_datei() {
        use crate::passkey::geheim::schluessel_datei_mit;
        let nie = |_: &std::path::Path| -> Result<(), String> { panic!("keine Ausnahme ohne Schlüssel") };
        let folge = "Aufgaben mit Link werden abgelehnt, gespeicherte Links nicht gelesen";
        // Kein Ordner: ein bloßer Name und die Wurzel.
        for pfad in ["aufgaben-links-schluessel", "/"] {
            let e = schluessel_datei_mit(std::path::Path::new(pfad), &RAHMEN_LINKS, true, &nie).unwrap_err();
            assert_eq!(e, format!("{pfad} nennt keinen Ordner; {folge}"));
        }
        // Ein Ordner, aber kein Dateiname: der andere Satz, für Anlegen und für Lesen.
        let o = ordner("ohne-dateinamen");
        let ohne_namen = o.join("x").join("..");
        assert!(ohne_namen.parent().is_some() && ohne_namen.file_name().is_none());
        let gezeigt = crate::datenschutz::verbergen(&ohne_namen.display().to_string());
        for anlegen in [true, false] {
            let e = schluessel_datei_mit(&ohne_namen, &RAHMEN_LINKS, anlegen, &nie).unwrap_err();
            assert_eq!(e, format!("{gezeigt} nennt keine Datei; {folge}"));
        }
        assert!(!o.exists(), "nichts entsteht");
    }

    /// Ein Dateiname, der kein UTF-8 ist, ist erlaubt: Der Schlüssel entsteht
    /// und wird wieder gelesen, und die Zwischendatei bleibt nicht liegen.
    #[cfg(unix)]
    #[test]
    fn unix_dateiname_ohne_utf8() {
        use crate::passkey::geheim::schluessel_datei_mit;
        use std::os::unix::ffi::OsStrExt;
        let o = ordner("ohne-utf8");
        let pfad = o.join("dashboard-schluessel").join(std::ffi::OsStr::from_bytes(b"schl\xfcssel"));
        assert!(pfad.file_name().unwrap().to_str().is_none());
        // Manche Dateisysteme (APFS) nehmen solche Namen nicht an; dann prüft der Test nichts weiter.
        match schluessel_datei_mit(&pfad, &RAHMEN_LINKS, true, &|_| Ok(())) {
            Ok(erster) => {
                assert_eq!(*schluessel_datei_mit(&pfad, &RAHMEN_LINKS, false, &|_| Ok(())).unwrap(), *erster);
                assert_eq!(std::fs::read_dir(pfad.parent().unwrap()).unwrap().count(), 1);
            }
            Err(e) => assert!(e.contains("ließ sich nicht anlegen"), "nur das Dateisystem darf ablehnen: {e}"),
        }
        let _ = std::fs::remove_dir_all(&o);
    }

    /// Die Ausnahme für die Sicherung bei neuer Datei, mit Zähler und ohne
    /// Zufall im Ablauf: Wer den Wettlauf verliert (die Datei entsteht zwischen
    /// Prüfung und Anlegen), ruft sie nicht und bekommt den Schlüssel des
    /// Gewinners. Der Gewinner ruft sie genau einmal. Neuer Ordner und neue
    /// Datei zusammen: genau einmal.
    #[cfg(unix)]
    #[test]
    fn unix_ausnahme_nur_beim_gewinner_des_wettlaufs() {
        use crate::passkey::geheim::{anlegen_wie_im_betrieb, schluessel_datei_mit_anleger};
        use std::cell::Cell;
        use std::path::Path;
        let gerufen = Cell::new(0u32);
        let zaehlen = |_: &Path| {
            gerufen.set(gerufen.get() + 1);
            Ok(())
        };
        let o = ordner("wettlauf");
        let schluesselordner = o.join("dashboard-schluessel");
        std::fs::create_dir_all(&schluesselordner).unwrap();
        std::fs::set_permissions(&schluesselordner, std::os::unix::fs::PermissionsExt::from_mode(0o700)).unwrap();
        let pfad = schluesselordner.join(LINKS_SCHLUESSEL_DATEI);

        // Der Verlierer: Zwischen seiner Prüfung (keine Datei) und seinem Anlegen
        // legt ein anderer Lauf die Datei an. Sein eigenes Anlegen meldet „gibt es schon“.
        let versuche = Cell::new(0u32);
        let ein_anderer_war_schneller = |ordner: &Path, ziel: &Path, eigene: &[u8; 32]| {
            versuche.set(versuche.get() + 1);
            anlegen_wie_im_betrieb(ordner, ziel, &[9u8; 32]).expect("der Gewinner legt an");
            anlegen_wie_im_betrieb(ordner, ziel, eigene)
        };
        let verlierer = schluessel_datei_mit_anleger(&pfad, &RAHMEN_LINKS, &zaehlen, &ein_anderer_war_schneller).unwrap();
        assert_eq!(*verlierer, [9u8; 32], "der Schlüssel des Gewinners gilt");
        assert_eq!(versuche.get(), 1, "genau ein Versuch anzulegen, danach nur noch laden");
        assert_eq!(gerufen.get(), 0, "wer den Wettlauf verliert, ruft die Ausnahme nicht");
        assert_eq!(std::fs::read(&pfad).unwrap(), [9u8; 32]);
        assert_eq!(std::fs::read_dir(&schluesselordner).unwrap().count(), 1, "keine Zwischendatei bleibt liegen");

        // Der Gewinner in einem vorhandenen Ordner: genau einmal.
        std::fs::remove_file(&pfad).unwrap();
        let gewinner = schluessel_datei_mit_anleger(&pfad, &RAHMEN_LINKS, &zaehlen, &anlegen_wie_im_betrieb).unwrap();
        assert_eq!(std::fs::read(&pfad).unwrap(), *gewinner);
        assert_eq!(gerufen.get(), 1, "der Gewinner ruft die Ausnahme genau einmal");
        // Danach lädt jeder nur noch: keine weitere Ausnahme.
        assert_eq!(*schluessel_datei_mit_anleger(&pfad, &RAHMEN_LINKS, &zaehlen, &anlegen_wie_im_betrieb).unwrap(), *gewinner);
        assert_eq!(gerufen.get(), 1);

        // Neuer Ordner und neue Datei zusammen: genau einmal, nicht zweimal.
        gerufen.set(0);
        let frisch = o.join("zweiter-ordner").join(LINKS_SCHLUESSEL_DATEI);
        schluessel_datei_mit_anleger(&frisch, &RAHMEN_LINKS, &zaehlen, &anlegen_wie_im_betrieb).unwrap();
        assert_eq!(gerufen.get(), 1);

        // Neuer Ordner, aber der Wettlauf um die Datei geht verloren: einmal für den
        // Ordner (den hat dieser Lauf angelegt), nicht noch einmal für die Datei.
        gerufen.set(0);
        versuche.set(0);
        let dritter = o.join("dritter-ordner").join(LINKS_SCHLUESSEL_DATEI);
        assert_eq!(*schluessel_datei_mit_anleger(&dritter, &RAHMEN_LINKS, &zaehlen, &ein_anderer_war_schneller).unwrap(), [9u8; 32]);
        assert_eq!((gerufen.get(), versuche.get()), (1, 1));

        // Scheitert das Anlegen aus einem anderen Grund, gibt es keinen Schlüssel und keine Ausnahme.
        gerufen.set(0);
        std::fs::remove_file(&pfad).unwrap();
        let scheitert = |_: &Path, _: &Path, _: &[u8; 32]| Err(std::io::Error::other("erfunden"));
        let e = schluessel_datei_mit_anleger(&pfad, &RAHMEN_LINKS, &zaehlen, &scheitert).unwrap_err();
        assert!(e.contains("ließ sich nicht anlegen") && !pfad.exists(), "{e}");
        assert_eq!(gerufen.get(), 0);
        let _ = std::fs::remove_dir_all(&o);
    }

    /// Viele Läufe zugleich am selben Ort: alle bekommen denselben Schlüssel.
    #[cfg(unix)]
    #[test]
    fn unix_gleichzeitig_derselbe_links_schluessel() {
        let o = ordner("gleichzeitig");
        let ort = o.join("dashboard-schluessel").join(LINKS_SCHLUESSEL_DATEI);
        let schranke = std::sync::Arc::new(std::sync::Barrier::new(12));
        let laeufe: Vec<_> = (0..12)
            .map(|_| {
                let (ort, schranke) = (ort.clone(), schranke.clone());
                std::thread::spawn(move || {
                    schranke.wait();
                    LinksSchluessel::an(Some(ort)).verschliessen(PROBE.as_bytes())
                })
            })
            .collect();
        let werte: Vec<Vec<u8>> = laeufe.into_iter().map(|l| l.join().unwrap().expect("jeder Lauf verschlüsselt")).collect();
        let leser = LinksSchluessel::an(Some(ort.clone()));
        for w in &werte {
            assert_eq!(leser.oeffnen(w).unwrap().as_slice(), PROBE.as_bytes());
        }
        assert_eq!(std::fs::read_dir(ort.parent().unwrap()).unwrap().count(), 1, "eine Datei, keine Zwischendatei");
        let _ = std::fs::remove_dir_all(&o);
    }

    /// Die Sätze der Schlüsseldatei verraten weder den Schlüssel noch den Klartext.
    #[cfg(unix)]
    #[test]
    fn unix_saetze_der_schluesseldatei_verraten_nichts() {
        use crate::passkey::geheim::{Mangel, mangel_satz_mit};
        let o = ordner("saetze");
        let ort = o.join("dashboard-schluessel").join(LINKS_SCHLUESSEL_DATEI);
        let quelle = LinksSchluessel::an(Some(ort.clone()));
        let geheim = quelle.verschliessen(PROBE.as_bytes()).unwrap();
        let schluessel_hex: String = std::fs::read(&ort).unwrap().iter().map(|b| format!("{b:02x}")).collect();
        let mut saetze: Vec<String> = [
            Mangel::OrdnerKeinOrdner,
            Mangel::OrdnerFremd,
            Mangel::OrdnerOffen,
            Mangel::DateiKeineDatei,
            Mangel::DateiFremd,
            Mangel::DateiOffen,
            Mangel::DateiGroesse,
        ]
        .into_iter()
        .map(|m| mangel_satz_mit(m, "<ordner>", "<datei>", &RAHMEN_LINKS))
        .collect();
        saetze.extend([KEIN_ORT_SATZ.to_string(), RAHMEN_LINKS.fehlt.to_string()]);
        let mut kaputt = geheim.clone();
        kaputt[40] ^= 1;
        saetze.push(quelle.oeffnen(&kaputt).unwrap_err());
        std::fs::write(&ort, [1u8; 31]).unwrap();
        saetze.push(LinksSchluessel::an(Some(ort.clone())).oeffnen(&geheim).unwrap_err());
        for satz in &saetze {
            for teil in [PROBE, "erfunden", "0815", "beispiel.invalid", "token", &schluessel_hex[..16], "Passkey"] {
                assert!(!satz.contains(teil), "{teil} in: {satz}");
            }
        }
        let _ = std::fs::remove_dir_all(&o);
    }
}
