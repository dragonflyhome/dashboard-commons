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

/// Fester Zusatzwert der Anwendung für DPAPI: trennt diese Daten von
/// anderen DPAPI-Daten desselben Kontos, ist aber kein Geheimnis. Nie ändern: sonst sind alle
/// gespeicherten Links unlesbar (sie laufen dann nur ab).
pub const ENTROPIE: &[u8] = b"dashboard/aufgaben_links/dpapi/v1";

#[cfg(all(not(windows), not(test)))]
compile_error!("Die Links der Aufgaben werden mit DPAPI verschlüsselt; das gibt es nur unter Windows.");

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

/// Nur für Tests auf anderen Plattformen: keine echte Verschlüsselung, aber
/// der Klartext steht nie offen da (umgedreht und mit festem Kopf).
#[cfg(all(not(windows), test))]
pub fn verschluesseln(klartext: &[u8]) -> Result<Vec<u8>, String> {
    let mut v = b"TEST".to_vec();
    v.extend(klartext.iter().rev().map(|b| b ^ 0x5a));
    Ok(v)
}

#[cfg(all(not(windows), test))]
pub fn entschluesseln(geheim: &[u8]) -> Result<zeroize::Zeroizing<Vec<u8>>, String> {
    ENTSCHLUESSELT.with(|n| n.set(n.get() + 1));
    let rest = geheim.strip_prefix(b"TEST").ok_or("kein Testwert")?;
    Ok(zeroize::Zeroizing::new(rest.iter().rev().map(|b| b ^ 0x5a).collect()))
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
}
