//! Seite „Schlüssel“: Der Inhaber gibt Passwörter im Browser ein, sie landen im
//! Windows Credential Manager (Generic Credential, Ziel `ranke/<name>`).
//! Dieses Modul zeigt nie einen Wert an und gibt nie einen zurück: nach außen
//! gibt es nur „fehlt“ oder „gespeichert am …“. Sitzungen lesen den Wert selbst
//! (siehe Anleitung, „7. Geheimnisse: Passwörter nie im Terminal abfragen“).

use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

/// Präfix aller Ziele im Credential Manager.
pub const TARGET_PREFIX: &str = "ranke/";
/// Mehr nimmt der Credential Manager für einen Blob nicht an (2560 Bytes).
pub const MAX_SECRET_CHARS: usize = 1200;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyRequest {
    /// Kurzname, nur `a-z 0-9 . _ -`; Ziel wird `ranke/<name>`.
    pub name: String,
    /// Wofür der Wert ist, in einfachem Deutsch.
    pub zweck: String,
    /// Woher der Inhaber den Wert bekommt.
    #[serde(default)]
    pub hinweis: String,
    /// Welche Sitzung ihn braucht.
    #[serde(default)]
    pub angefordert_von: String,
    /// Benutzername, der mit dem Wert im Tresor steht (z. B. eine Kundennummer).
    /// Leer: der Zweck-Text.
    #[serde(default)]
    pub benutzer: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct KeyFile {
    #[serde(default)]
    pub key: Vec<KeyRequest>,
}

/// Ein Eintrag der Seite: Anforderung plus Zustand im Credential Manager.
#[derive(Debug, Clone)]
pub struct KeyView {
    pub req: KeyRequest,
    pub target: String,
    pub saved_at: Option<DateTime<Local>>,
    /// Ob der Eintrag gespeichert ist, ließ sich nicht sagen (Bote meldet
    /// einen Fehler für diesen Schlüssel oder antwortet nicht). Dann ist
    /// `saved_at` leer, aber der Schlüssel fehlt nicht (Etappe 3a+).
    pub unbekannt: bool,
}

impl KeyView {
    /// Der Stand dieses Schlüssels im Tresor.
    pub fn stand(&self) -> crate::vertrag::api::TresorStand {
        use crate::vertrag::api::TresorStand;
        match (self.unbekannt, self.saved_at) {
            (true, _) => TresorStand::Unbekannt,
            (false, Some(_)) => TresorStand::Gespeichert,
            (false, None) => TresorStand::Fehlt,
        }
    }
}

impl KeyRequest {
    /// Benutzername für den Tresor-Eintrag.
    pub fn username(&self) -> &str {
        if self.benutzer.is_empty() { &self.zweck } else { &self.benutzer }
    }
}

pub fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
        && !name.starts_with('.')
}

pub fn target_for(name: &str) -> String {
    format!("{TARGET_PREFIX}{name}")
}

pub fn load(path: &Path) -> Result<Vec<KeyRequest>> {
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("{} lesen", path.display()))?;
    let file: KeyFile = toml::from_str(&text).with_context(|| format!("{} ist kein gültiges TOML", path.display()))?;
    for k in &file.key {
        if !is_valid_name(&k.name) {
            anyhow::bail!("Schlüsselname „{}“ ist ungültig (erlaubt: a-z 0-9 . _ -)", k.name);
        }
    }
    Ok(file.key)
}

/// Anforderungen laden und für jede nachsehen, ob der Wert schon da ist.
pub fn views(path: &Path) -> Result<Vec<KeyView>> {
    Ok(load(path)?
        .into_iter()
        .map(|req| {
            let target = target_for(&req.name);
            let saved_at = cred_exists(&target);
            KeyView { req, target, saved_at, unbekannt: false }
        })
        .collect())
}

/// (gespeichert, fehlen) für Ampeln.
pub fn counts(path: &Path) -> (usize, usize) {
    match views(path) {
        Ok(v) => {
            let saved = v.iter().filter(|k| k.saved_at.is_some()).count();
            (saved, v.len() - saved)
        }
        Err(_) => (0, 0),
    }
}

/// Dekodiert `application/x-www-form-urlencoded` und liefert nur das gesuchte
/// Feld. Alles andere wird verworfen. Der Rückgabewert gehört dem Aufrufer,
/// der ihn nach Gebrauch überschreibt.
pub fn form_value(body: &[u8], field: &str) -> Option<String> {
    for pair in body.split(|&b| b == b'&') {
        let mut it = pair.splitn(2, |&b| b == b'=');
        let k = it.next().unwrap_or(&[]);
        let v = it.next().unwrap_or(&[]);
        if percent_decode(k) == field.as_bytes() {
            let mut bytes = percent_decode(v);
            let s = String::from_utf8(bytes.clone()).ok();
            bytes.zeroize();
            return s;
        }
    }
    None
}

fn percent_decode(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(input.len());
    let mut i = 0;
    while i < input.len() {
        match input[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < input.len() => {
                let hex = |b: u8| (b as char).to_digit(16);
                match (hex(input[i + 1]), hex(input[i + 2])) {
                    (Some(a), Some(b)) => {
                        out.push((a * 16 + b) as u8);
                        i += 2;
                    }
                    _ => out.push(b'%'),
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    out
}

// ----- Windows Credential Manager (advapi32), ohne Zusatz-Crate -----
// Absichtlich eigene Deklarationen: eine weitere windows-sys-Version würde
// über raw-dylib gelinkt, und das stürzt mit der GNU-Toolchain ab (siehe Anleitung, „Stolpersteine beim Bauen“).

#[cfg(windows)]
mod win {
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Clone, Copy)]
    // Name wie in der Win32-API, damit die Struktur dort nachzuschlagen ist.
    #[allow(clippy::upper_case_acronyms)]
    pub struct FILETIME {
        pub low: u32,
        pub high: u32,
    }

    #[repr(C)]
    // Name wie in der Win32-API, damit die Struktur dort nachzuschlagen ist.
    #[allow(clippy::upper_case_acronyms)]
    pub struct CREDENTIALW {
        pub flags: u32,
        pub kind: u32,
        pub target_name: *mut u16,
        pub comment: *mut u16,
        pub last_written: FILETIME,
        pub blob_size: u32,
        pub blob: *mut u8,
        pub persist: u32,
        pub attribute_count: u32,
        pub attributes: *mut c_void,
        pub target_alias: *mut u16,
        pub user_name: *mut u16,
    }

    pub const CRED_TYPE_GENERIC: u32 = 1;
    pub const CRED_PERSIST_LOCAL_MACHINE: u32 = 2;

    #[link(name = "advapi32")]
    unsafe extern "system" {
        pub fn CredReadW(target: *const u16, kind: u32, flags: u32, out: *mut *mut CREDENTIALW) -> i32;
        pub fn CredWriteW(cred: *const CREDENTIALW, flags: u32) -> i32;
        #[cfg_attr(not(test), allow(dead_code))]
        pub fn CredDeleteW(target: *const u16, kind: u32, flags: u32) -> i32;
        pub fn CredFree(buffer: *mut c_void);
    }

    pub fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// FILETIME (100 ns seit 1601) nach Unix-Sekunden.
    pub fn filetime_to_unix(ft: FILETIME) -> i64 {
        let t = ((ft.high as u64) << 32) | ft.low as u64;
        (t / 10_000_000) as i64 - 11_644_473_600
    }
}

/// Was über einen Tresor-Eintrag bekannt sein darf: wann er geschrieben wurde
/// und unter welchem Benutzernamen. Nie der Wert. Der Benutzername wird beim
/// Wegwerfen überschrieben und gehört weder in Antworten noch ins Protokoll.
pub struct CredInfo {
    pub saved_at: Option<DateTime<Local>>,
    pub username: Option<String>,
}

impl Drop for CredInfo {
    fn drop(&mut self) {
        if let Some(u) = self.username.as_mut() {
            u.zeroize();
        }
    }
}

/// Gibt es den Eintrag? Liefert den Zeitpunkt des letzten Schreibens.
pub fn cred_exists(target: &str) -> Option<DateTime<Local>> {
    cred_info(target).and_then(|i| i.saved_at)
}

/// Liest Zeitpunkt und Benutzernamen eines Eintrags. Der Wert selbst wird nicht kopiert.
#[cfg(windows)]
pub fn cred_info(target: &str) -> Option<CredInfo> {
    use chrono::TimeZone;
    let t = win::wide(target);
    let mut p: *mut win::CREDENTIALW = std::ptr::null_mut();
    // SAFETY: nullterminierter Zielname; der Ausgabezeiger wird geprüft und mit CredFree freigegeben.
    let ok = unsafe { win::CredReadW(t.as_ptr(), win::CRED_TYPE_GENERIC, 0, &mut p) };
    if ok == 0 || p.is_null() {
        return None;
    }
    // SAFETY: p zeigt auf eine von der API gefüllte Struktur, bis CredFree aufgerufen wird;
    // user_name ist dort null oder ein nullterminierter UTF-16-Text.
    let (secs, username) = unsafe {
        let secs = win::filetime_to_unix((*p).last_written);
        let u = (*p).user_name;
        let username = if u.is_null() {
            None
        } else {
            let mut len = 0usize;
            while *u.add(len) != 0 {
                len += 1;
            }
            String::from_utf16(std::slice::from_raw_parts(u, len)).ok().filter(|s| !s.is_empty())
        };
        (secs, username)
    };
    unsafe { win::CredFree(p as *mut _) };
    Some(CredInfo { saved_at: Local.timestamp_opt(secs, 0).single(), username })
}

/// Schreibt den Wert als Generic Credential (UTF-16, wie cmdkey und
/// PowerShell es erwarten). Die Puffer werden danach überschrieben.
#[cfg(windows)]
pub fn cred_write(target: &str, username: &str, secret: &str) -> Result<()> {
    if secret.chars().count() > MAX_SECRET_CHARS {
        anyhow::bail!("Wert ist zu lang (höchstens {MAX_SECRET_CHARS} Zeichen)");
    }
    let mut t = win::wide(target);
    let mut u = win::wide(username);
    let mut blob: Vec<u16> = secret.encode_utf16().collect();
    let blob_bytes = blob.len() * 2;
    let cred = win::CREDENTIALW {
        flags: 0,
        kind: win::CRED_TYPE_GENERIC,
        target_name: t.as_mut_ptr(),
        comment: std::ptr::null_mut(),
        last_written: win::FILETIME { low: 0, high: 0 },
        blob_size: blob_bytes as u32,
        blob: blob.as_mut_ptr() as *mut u8,
        persist: win::CRED_PERSIST_LOCAL_MACHINE,
        attribute_count: 0,
        attributes: std::ptr::null_mut(),
        target_alias: std::ptr::null_mut(),
        user_name: u.as_mut_ptr(),
    };
    // SAFETY: alle Zeiger zeigen auf lebende Puffer dieser Funktion; die API kopiert die Daten.
    let ok = unsafe { win::CredWriteW(&cred, 0) };
    let err = std::io::Error::last_os_error();
    blob.zeroize();
    t.zeroize();
    u.zeroize();
    if ok == 0 {
        anyhow::bail!("Credential Manager hat nicht gespeichert: {err}");
    }
    Ok(())
}

#[cfg(windows)]
#[cfg_attr(not(test), allow(dead_code))]
pub fn cred_delete(target: &str) -> Result<()> {
    let t = win::wide(target);
    // SAFETY: nullterminierter Zielname.
    let ok = unsafe { win::CredDeleteW(t.as_ptr(), win::CRED_TYPE_GENERIC, 0) };
    if ok == 0 {
        anyhow::bail!("nicht gelöscht: {}", std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn cred_info(_target: &str) -> Option<CredInfo> {
    None
}
#[cfg(not(windows))]
pub fn cred_write(_target: &str, _username: &str, _secret: &str) -> Result<()> {
    anyhow::bail!("Credential Manager gibt es nur unter Windows")
}
#[cfg(not(windows))]
pub fn cred_delete(_target: &str) -> Result<()> {
    Ok(())
}

/// Abnahme 4c: Tests, die in den Windows-Tresor schreiben, laufen nacheinander.
/// Gleichzeitiges Schreiben und Löschen verschiedener Einträge aus einem
/// Prozess verlor im vollen Testlauf einen Eintrag („Element nicht gefunden“).
#[cfg(test)]
pub static TRESOR_TESTS: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(is_valid_name("wetter-api"));
        assert!(is_valid_name("api.key_2"));
        assert!(!is_valid_name("zwei Worte"));
        assert!(!is_valid_name("../x"));
        assert!(!is_valid_name(""));
        assert_eq!(target_for("wetter-api"), "ranke/wetter-api");
    }

    #[test]
    fn form_decoding() {
        assert_eq!(form_value(b"wert=a%26b+c%3D", "wert").as_deref(), Some("a&b c="));
        assert_eq!(form_value(b"x=1&wert=%C3%A4%", "wert").as_deref(), Some("ä%"));
        assert_eq!(form_value(b"x=1", "wert"), None);
        assert_eq!(form_value(b"wert=", "wert").as_deref(), Some(""));
    }

    #[test]
    fn parses_file() {
        let text = "[[key]]\nname = \"wetter-api\"\nzweck = \"Wetterdienst\"\nhinweis = \"h\"\nangefordert_von = \"Gartenplaner\"\n";
        let f: KeyFile = toml::from_str(text).unwrap();
        assert_eq!(f.key.len(), 1);
        assert_eq!(f.key[0].name, "wetter-api");
    }

    #[cfg(windows)]
    #[test]
    fn write_read_delete_roundtrip() {
        let _nacheinander = super::TRESOR_TESTS.blocking_lock();
        let target = "ranke/test-dummy-dashboard";
        let _ = cred_delete(target);
        assert!(cred_exists(target).is_none());
        cred_write(target, "Test des Dashboards", "nur-ein-test").unwrap();
        let when = cred_exists(target).expect("nach dem Schreiben vorhanden");
        assert!((Local::now() - when).num_seconds().abs() < 120);
        let info = cred_info(target).expect("nach dem Schreiben vorhanden");
        assert_eq!(info.username.as_deref(), Some("Test des Dashboards"));
        cred_delete(target).unwrap();
        assert!(cred_exists(target).is_none());
    }
}
