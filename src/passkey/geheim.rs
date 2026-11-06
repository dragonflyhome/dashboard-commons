//! Die Datei mit dem Schlüssel für die Ausweise (JWT, Ed25519): 32 zufällige
//! Bytes. Im Dienst liegt sie in `C:/ProgramData/Dashboard/geheim/jwt-schluessel`
//! (`Einstellungen::jwt_schluessel_datei`), sonst neben dem Datenspeicher
//! (`app::jwt_schluessel_datei`, für Tests über `DASHBOARD_JWT_SCHLUESSEL`).
//!
//! Beim ersten Start legt der Dienst sie selbst an, und zwar gleich mit enger
//! Freigabe: Ordner `geheim` und Datei bekommen eine geschützte DACL (keine
//! Vererbung von oben) mit Vollzugriff nur für SYSTEM, die Administratoren und
//! das Konto, unter dem der Dienst läuft (`NT SERVICE\Dashboard`). Ein
//! bestehender Ordner behält seine Rechte; die Datei selbst ist trotzdem eng.
//!
//! Eine vorhandene Datei wird nie überschrieben: Hat sie nicht genau 32
//! Bytes, gilt sie als kaputt, und die Anmeldung mit Passkey bleibt aus, bis
//! jemand nachsieht (sonst wären alle Ausweise auf einen Schlag ungültig, und
//! ein Fehler bliebe unbemerkt).

use std::io::Write;
use std::path::Path;

use super::jwt::JwtSchluessel;

/// Lädt den Schlüssel; fehlt die Datei, wird sie angelegt. Fehler als Satz fürs Protokoll.
pub fn laden_oder_anlegen(pfad: &Path) -> Result<JwtSchluessel, String> {
    match std::fs::read(pfad) {
        Ok(bytes) => {
            let b: [u8; 32] = bytes.try_into().map_err(|_| format!("{} hat nicht genau 32 Bytes; die Anmeldung mit Passkey bleibt aus", pfad.display()))?;
            Ok(JwtSchluessel::aus_bytes(&b))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut b = [0u8; 32];
            b.copy_from_slice(&super::zufall(32));
            match anlegen(pfad, &b) {
                Ok(()) => {
                    tracing::info!("Schlüssel für die Ausweise neu angelegt: {}", pfad.display());
                    Ok(JwtSchluessel::aus_bytes(&b))
                }
                // Ein anderer Lauf war schneller: dessen Datei gilt.
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => laden_oder_anlegen(pfad),
                Err(e) => Err(format!("{} ließ sich nicht anlegen: {e}", pfad.display())),
            }
        }
        Err(e) => Err(format!("{} ist nicht lesbar: {e}", pfad.display())),
    }
}

/// SDDL der Freigabe: geschützt, Vollzugriff für SYSTEM, Administratoren und `sid`.
/// `erben`: für den Ordner (gilt auch für Dateien darin).
pub fn sddl(sid: &str, erben: bool) -> String {
    let f = if erben { "OICI" } else { "" };
    format!("D:P(A;{f};FA;;;SY)(A;{f};FA;;;BA)(A;{f};FA;;;{sid})")
}

#[cfg(windows)]
fn anlegen(pfad: &Path, bytes: &[u8; 32]) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::Foundation::{GENERIC_WRITE, INVALID_HANDLE_VALUE, LocalFree};
    use windows_sys::Win32::Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
    use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
    use windows_sys::Win32::Storage::FileSystem::{CREATE_NEW, CreateDirectoryW, CreateFileW, FILE_ATTRIBUTE_NORMAL};

    fn breit(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    /// Sicherheitsbeschreibung aus SDDL, mit `LocalFree` freigegeben.
    struct Beschreibung(PSECURITY_DESCRIPTOR);
    impl Drop for Beschreibung {
        fn drop(&mut self) {
            // SAFETY: von ConvertStringSecurityDescriptorToSecurityDescriptorW mit LocalAlloc angelegt.
            unsafe { LocalFree(self.0) };
        }
    }
    fn beschreibung(sddl: &str) -> std::io::Result<Beschreibung> {
        let text = breit(std::ffi::OsStr::new(sddl));
        let mut psd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: nullterminierter Text; psd wird von der API gefüllt und unten freigegeben.
        let ok = unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(text.as_ptr(), SDDL_REVISION_1, &mut psd, std::ptr::null_mut()) };
        if ok == 0 || psd.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Beschreibung(psd))
    }
    let attribute =
        |b: &Beschreibung| SECURITY_ATTRIBUTES { nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: b.0, bInheritHandle: 0 };

    let sid = crate::anmeldung::sspi::eigene_sid().map_err(std::io::Error::other)?;
    if let Some(ordner) = pfad.parent().filter(|p| !p.as_os_str().is_empty())
        && !ordner.is_dir()
    {
        if let Some(darueber) = ordner.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(darueber)?;
        }
        let sd = beschreibung(&sddl(&sid, true))?;
        let sa = attribute(&sd);
        let name = breit(ordner.as_os_str());
        // SAFETY: nullterminierter Name, gültige Sicherheitsangaben für die Dauer des Aufrufs.
        if unsafe { CreateDirectoryW(name.as_ptr(), &sa) } == 0 {
            let e = std::io::Error::last_os_error();
            if !ordner.is_dir() {
                return Err(e);
            }
        }
    }
    let sd = beschreibung(&sddl(&sid, false))?;
    let sa = attribute(&sd);
    let name = breit(pfad.as_os_str());
    // SAFETY: nullterminierter Name, gültige Sicherheitsangaben; das Handle übernimmt File.
    let h = unsafe { CreateFileW(name.as_ptr(), GENERIC_WRITE, 0, &sa, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, std::ptr::null_mut()) };
    if h == INVALID_HANDLE_VALUE || h.is_null() {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: gültiges Handle aus CreateFileW, gehört ab jetzt der Datei.
    let mut datei = unsafe { std::fs::File::from_raw_handle(h as _) };
    datei.write_all(bytes)?;
    datei.sync_all()
}

#[cfg(not(windows))]
fn anlegen(pfad: &Path, bytes: &[u8; 32]) -> std::io::Result<()> {
    if let Some(ordner) = pfad.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(ordner)?;
    }
    let mut datei = std::fs::OpenOptions::new().write(true).create_new(true).open(pfad)?;
    datei.write_all(bytes)?;
    datei.sync_all()
}

/// Die DACL einer Datei als SDDL (nur für Tests und Prüfungen).
#[cfg(all(windows, test))]
pub fn dacl_text(pfad: &Path) -> Result<String, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetNamedSecurityInfoW, SDDL_REVISION_1, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::{DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR};
    let name: Vec<u16> = pfad.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: nullterminierter Name; sd wird mit LocalFree freigegeben.
    let r = unsafe {
        GetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut sd,
        )
    };
    if r != 0 {
        return Err(format!("GetNamedSecurityInfoW: {r}"));
    }
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: sd stammt von oben; text wird mit LocalFree freigegeben.
    unsafe {
        let ok = ConvertSecurityDescriptorToStringSecurityDescriptorW(sd, SDDL_REVISION_1, DACL_SECURITY_INFORMATION, &mut text, std::ptr::null_mut());
        LocalFree(sd);
        if ok == 0 {
            return Err("ConvertSecurityDescriptorToStringSecurityDescriptorW".into());
        }
        let mut n = 0;
        while *text.add(n) != 0 {
            n += 1;
        }
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(text, n));
        LocalFree(text as _);
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ordner(name: &str) -> std::path::PathBuf {
        let o = std::env::temp_dir().join(format!("dashboard-geheim-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&o);
        o
    }

    #[test]
    fn wird_angelegt_und_beim_zweiten_mal_gelesen() {
        let o = ordner("anlegen");
        let pfad = o.join("geheim").join("jwt-schluessel");
        let k = laden_oder_anlegen(&pfad).unwrap();
        assert_eq!(std::fs::read(&pfad).unwrap().len(), 32);
        let t = k.ausstellen(&crate::passkey::jwt::Ausweis::neu("c", 1_793_000_000));
        let zweiter = laden_oder_anlegen(&pfad).unwrap();
        assert!(zweiter.pruefen(&t, 1_793_000_001).is_ok(), "derselbe Schlüssel");
        #[cfg(windows)]
        {
            let sid = crate::anmeldung::sspi::eigene_sid().unwrap();
            for (was, erwartet) in [(pfad.clone(), sddl(&sid, false)), (pfad.parent().unwrap().to_path_buf(), sddl(&sid, true))] {
                let dacl = dacl_text(&was).unwrap();
                assert!(dacl.starts_with("D:P"), "geschützt, ohne Vererbung: {dacl}");
                assert!(dacl.contains(&sid) && dacl.contains(";SY)") && dacl.contains(";BA)"), "{dacl}");
                // Niemand sonst: keine Benutzer, keine Jeder-Gruppe, keine angemeldeten Benutzer.
                for fremd in [";BU)", ";WD)", ";AU)", ";IU)"] {
                    assert!(!dacl.contains(fremd), "{fremd} in {dacl}");
                }
                assert_eq!(dacl.matches("(A;").count(), erwartet.matches("(A;").count(), "{dacl}");
            }
        }
        let _ = std::fs::remove_dir_all(&o);
    }

    #[test]
    fn kaputte_datei_wird_nicht_ueberschrieben() {
        let o = ordner("kaputt");
        std::fs::create_dir_all(&o).unwrap();
        let pfad = o.join("jwt-schluessel");
        std::fs::write(&pfad, b"zu kurz").unwrap();
        assert!(laden_oder_anlegen(&pfad).is_err());
        assert_eq!(std::fs::read(&pfad).unwrap(), b"zu kurz");
        let _ = std::fs::remove_dir_all(&o);
    }
}
