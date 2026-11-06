//! Die Windows-Seite der Pipe (windows-sys): Pipe anlegen und öffnen, Zeilen
//! lesen und schreiben, SIDs und Programmpfade von Prozessen, die DACL aus
//! SDDL. Alles blockierend; Server und Client rufen es aus eigenen Fäden auf.

use std::ffi::c_void;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_BROKEN_PIPE, ERROR_FILE_NOT_FOUND, ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, HANDLE,
    INVALID_HANDLE_VALUE, LocalFree,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSecurityDescriptorToStringSecurityDescriptorW, ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, ConvertStringSidToSidW,
    EXPLICIT_ACCESS_W, GRANT_ACCESS, GetSecurityInfo, SDDL_REVISION_1, SE_KERNEL_OBJECT, SetEntriesInAclW, SetSecurityInfo, TRUSTEE_IS_SID, TRUSTEE_IS_UNKNOWN,
    TRUSTEE_W,
};
use windows_sys::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, GetTokenInformation, NO_INHERITANCE, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES,
    TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FlushFileBuffers, OPEN_EXISTING, PIPE_ACCESS_DUPLEX, READ_CONTROL, ReadFile, SECURITY_IDENTIFICATION,
    SECURITY_SQOS_PRESENT, WRITE_DAC, WriteFile,
};
use windows_sys::Win32::System::IO::CancelIoEx;
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeServerProcessId, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
    PIPE_WAIT, PeekNamedPipe, WaitNamedPipeW,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use zeroize::Zeroizing;

use crate::vertrag::bote::MAX_ZEILE;

/// Höchstens so viele Instanzen der Pipe gleichzeitig. Der Dienst hält
/// höchstens `client::PLAETZE` (8) Verbindungen offen; der Rest bleibt für
/// Tests und einen kurz überlappenden zweiten Dienst (Neustart).
pub const MAX_INSTANZEN: u32 = 16;
/// Puffergröße der Pipe je Richtung.
const PUFFER: u32 = 64 * 1024;

fn breit(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// `\\.\pipe\<name>`.
pub fn pipe_pfad(name: &str) -> String {
    format!(r"\\.\pipe\{name}")
}

/// Ein Handle, das beim Wegwerfen geschlossen wird.
pub struct Griff(HANDLE);

// SAFETY: Kernel-Handles dürfen zwischen Fäden wandern; Zugriffe laufen über
// die (threadsicheren) Win32-Aufrufe.
unsafe impl Send for Griff {}
unsafe impl Sync for Griff {}

impl Griff {
    fn gueltig(h: HANDLE) -> io::Result<Griff> {
        if h.is_null() || h == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        Ok(Griff(h))
    }

    /// Der rohe Wert, nur für `abbrechen`.
    pub fn roh(&self) -> usize {
        self.0 as usize
    }
}

impl Drop for Griff {
    fn drop(&mut self) {
        // SAFETY: eigenes, gültiges Handle; wird nur hier geschlossen.
        unsafe { CloseHandle(self.0) };
    }
}

/// Eine Seite der Pipe (Server oder Client).
pub struct Rohr(Griff);

impl Rohr {
    pub fn roh(&self) -> usize {
        self.0.roh()
    }

    /// Schreibt alles.
    pub fn schreiben(&self, mut daten: &[u8]) -> io::Result<()> {
        while !daten.is_empty() {
            let mut geschrieben = 0u32;
            let n = daten.len().min(PUFFER as usize) as u32;
            // SAFETY: Puffer lebt bis zum Ende des Aufrufs; synchrones Handle, kein OVERLAPPED.
            let ok = unsafe { WriteFile(self.0.0, daten.as_ptr(), n, &mut geschrieben, std::ptr::null_mut()) };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            daten = &daten[geschrieben as usize..];
        }
        Ok(())
    }

    /// Liest, was da ist; 0 heißt: die Gegenseite hat getrennt.
    pub fn lesen(&self, puffer: &mut [u8]) -> io::Result<usize> {
        let mut gelesen = 0u32;
        let n = puffer.len().min(u32::MAX as usize) as u32;
        // SAFETY: Puffer gehört dem Aufrufer und ist `n` Bytes lang.
        let ok = unsafe { ReadFile(self.0.0, puffer.as_mut_ptr(), n, &mut gelesen, std::ptr::null_mut()) };
        if ok == 0 {
            let e = io::Error::last_os_error();
            return match e.raw_os_error().map(|c| c as u32) {
                Some(ERROR_BROKEN_PIPE) | Some(ERROR_NO_DATA) => Ok(0),
                _ => Err(e),
            };
        }
        Ok(gelesen as usize)
    }

    /// Wie viele Bytes ohne Warten zu lesen sind; `None`: die Gegenseite hat getrennt.
    pub fn verfuegbar(&self) -> io::Result<Option<u32>> {
        let mut da = 0u32;
        // SAFETY: kein Puffer (nur Abfrage), Ausgabe in eine lokale Zahl.
        let ok = unsafe { PeekNamedPipe(self.0.0, std::ptr::null_mut(), 0, std::ptr::null_mut(), &mut da, std::ptr::null_mut()) };
        if ok == 0 {
            let e = io::Error::last_os_error();
            return match e.raw_os_error().map(|c| c as u32) {
                Some(ERROR_BROKEN_PIPE) | Some(ERROR_NO_DATA) => Ok(None),
                _ => Err(e),
            };
        }
        Ok(Some(da))
    }
}

/// Pause zwischen zwei Blicken in die Pipe beim Warten auf Daten, je nachdem,
/// wie lange schon nichts kam: kurz nach einer Antwort kurz (die nächste
/// Anfrage kommt oft gleich), später länger. Höchstens 50 ms Verzug.
pub fn warte_schritt(seit: Duration) -> Duration {
    if seit < Duration::from_secs(1) {
        Duration::from_millis(1)
    } else if seit < Duration::from_secs(10) {
        Duration::from_millis(10)
    } else {
        Duration::from_millis(50)
    }
}

/// Wartet, bis Daten da sind oder die Gegenseite getrennt hat (`true`), oder
/// bis `grenze` lang nichts kam (`false`). ReadFile auf einem synchronen
/// Handle ließe sich nicht mit Zeitlimit abbrechen; darum vorher schauen.
fn warten_auf_daten(rohr: &Rohr, grenze: Duration) -> io::Result<bool> {
    let start = Instant::now();
    loop {
        match rohr.verfuegbar()? {
            None => return Ok(true),
            Some(n) if n > 0 => return Ok(true),
            Some(_) => {}
        }
        let seit = start.elapsed();
        if seit >= grenze {
            return Ok(false);
        }
        std::thread::sleep(warte_schritt(seit).min(grenze - seit));
    }
}

/// Was `Zeilen::naechste` findet.
pub enum Gelesen {
    Zeile(Zeroizing<Vec<u8>>),
    /// Gegenseite hat getrennt.
    Ende,
    /// Mehr als `MAX_ZEILE` Bytes ohne `\n`.
    ZuLang,
    /// Länger als die Leerlaufgrenze kam nichts (nur `naechste_mit_leerlauf`).
    Leerlauf,
}

/// Zerlegt den Strom in Zeilen. Alle Puffer werden beim Wegwerfen überschrieben
/// (eine Anfrage kann einen Wert für den Tresor tragen). Der Puffer ist von
/// Anfang an groß genug für die längste erlaubte Zeile samt einem Stück, und
/// es wird nie mehr hineingelesen, als Platz ist: er wächst also nie, und es
/// bleiben keine Kopien in freigegebenem Speicher zurück.
pub struct Zeilen {
    rest: Zeroizing<Vec<u8>>,
}

/// Platz im Puffer von `Zeilen`.
const ZEILEN_PLATZ: usize = MAX_ZEILE + PUFFER as usize + 16;

impl Default for Zeilen {
    fn default() -> Self {
        Zeilen { rest: Zeroizing::new(Vec::with_capacity(ZEILEN_PLATZ)) }
    }
}

impl Zeilen {
    pub fn naechste(&mut self, rohr: &Rohr) -> io::Result<Gelesen> {
        self.naechste_mit_leerlauf(rohr, None)
    }

    /// Wie `naechste`; mit `grenze`: kommt so lange kein Byte, `Gelesen::Leerlauf`.
    pub fn naechste_mit_leerlauf(&mut self, rohr: &Rohr, grenze: Option<Duration>) -> io::Result<Gelesen> {
        let mut stueck = Zeroizing::new(vec![0u8; PUFFER as usize]);
        loop {
            if let Some(pos) = self.rest.iter().position(|&b| b == b'\n') {
                if pos > MAX_ZEILE {
                    return Ok(Gelesen::ZuLang);
                }
                let mut zeile = Zeroizing::new(Vec::with_capacity(pos));
                zeile.extend_from_slice(&self.rest[..pos]);
                if zeile.last() == Some(&b'\r') {
                    zeile.pop();
                }
                // Rückt den Rest an den Anfang, ohne neuen Speicher.
                self.rest.drain(..=pos);
                return Ok(Gelesen::Zeile(zeile));
            }
            if self.rest.len() > MAX_ZEILE {
                return Ok(Gelesen::ZuLang);
            }
            if let Some(g) = grenze
                && !warten_auf_daten(rohr, g)?
            {
                return Ok(Gelesen::Leerlauf);
            }
            let frei = (self.rest.capacity() - self.rest.len()).min(stueck.len());
            let n = rohr.lesen(&mut stueck[..frei])?;
            if n == 0 {
                return Ok(Gelesen::Ende);
            }
            self.rest.extend_from_slice(&stueck[..n]);
        }
    }
}

/// Sicherheitsbeschreibung aus SDDL; wird mit `LocalFree` freigegeben.
pub struct Beschreibung(PSECURITY_DESCRIPTOR);

// SAFETY: nach dem Anlegen nur noch gelesen (von CreateNamedPipeW).
unsafe impl Send for Beschreibung {}
unsafe impl Sync for Beschreibung {}

impl Beschreibung {
    pub fn aus_sddl(sddl: &str) -> io::Result<Beschreibung> {
        let text = breit(sddl);
        let mut psd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: nullterminierter Text; psd wird von der API gefüllt und mit LocalFree freigegeben.
        let ok = unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(text.as_ptr(), SDDL_REVISION_1, &mut psd, std::ptr::null_mut()) };
        if ok == 0 || psd.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Beschreibung(psd))
    }
}

impl Drop for Beschreibung {
    fn drop(&mut self) {
        // SAFETY: von ConvertStringSecurityDescriptorToSecurityDescriptorW mit LocalAlloc angelegt.
        unsafe { LocalFree(self.0) };
    }
}

/// Legt eine Instanz der Pipe an (Server). `erste`: mit
/// FILE_FLAG_FIRST_PIPE_INSTANCE, scheitert also, wenn es den Namen schon gibt.
pub fn pipe_anlegen(name: &str, sd: &Beschreibung, erste: bool) -> io::Result<Rohr> {
    let pfad = breit(&pipe_pfad(name));
    let sa = SECURITY_ATTRIBUTES { nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: sd.0, bInheritHandle: 0 };
    let modus = PIPE_ACCESS_DUPLEX | if erste { FILE_FLAG_FIRST_PIPE_INSTANCE } else { 0 };
    let art = PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS;
    // SAFETY: nullterminierter Name, gültige Sicherheitsangaben für die Dauer des Aufrufs.
    let h = unsafe { CreateNamedPipeW(pfad.as_ptr(), modus, art, MAX_INSTANZEN, PUFFER, PUFFER, 0, &sa) };
    Ok(Rohr(Griff::gueltig(h)?))
}

/// Ist der Fehler „Pipe-Instanzen alle belegt“?
pub fn ist_belegt(e: &io::Error) -> bool {
    e.raw_os_error() == Some(ERROR_PIPE_BUSY as i32)
}

/// Ist der Fehler „Pipe gibt es nicht“?
pub fn fehlt(e: &io::Error) -> bool {
    e.raw_os_error() == Some(ERROR_FILE_NOT_FOUND as i32)
}

/// Wartet blockierend auf einen Client.
pub fn warten_auf_client(rohr: &Rohr) -> io::Result<()> {
    // SAFETY: eigenes Pipe-Handle, synchron.
    let ok = unsafe { ConnectNamedPipe(rohr.0.0, std::ptr::null_mut()) };
    if ok == 0 {
        let e = io::Error::last_os_error();
        // Der Client war schneller als ConnectNamedPipe: auch verbunden.
        if e.raw_os_error() != Some(ERROR_PIPE_CONNECTED as i32) {
            return Err(e);
        }
    }
    Ok(())
}

/// Trennt den Client; die Instanz lässt sich danach wieder verwenden.
pub fn trennen(rohr: &Rohr) {
    // SAFETY: eigenes Pipe-Handle.
    unsafe {
        FlushFileBuffers(rohr.0.0);
        DisconnectNamedPipe(rohr.0.0);
    }
}

/// Öffnet die Pipe als Client: nur Identifikation (der Server kann das
/// Konto des Clients nicht annehmen) und nur mit `DIENST_MASKE` (lesen,
/// schreiben, Besitzer lesen; keine Instanzen anlegen). Sind alle Instanzen
/// belegt, wartet es insgesamt höchstens `warten_ms` auf eine freie (auch
/// über mehrere Runden, wenn ein anderer Client schneller war).
pub fn pipe_oeffnen(name: &str, warten_ms: u32) -> io::Result<Rohr> {
    let pfad = breit(&pipe_pfad(name));
    let ende = Instant::now() + Duration::from_millis(warten_ms as u64);
    loop {
        // SAFETY: nullterminierter Name, keine Sicherheitsangaben, keine Vorlage.
        let h = unsafe {
            CreateFileW(
                pfad.as_ptr(),
                super::DIENST_MASKE,
                0,
                std::ptr::null(),
                OPEN_EXISTING,
                SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                std::ptr::null_mut(),
            )
        };
        match Griff::gueltig(h) {
            Ok(g) => return Ok(Rohr(g)),
            Err(e) if ist_belegt(&e) => {
                let rest = ende.saturating_duration_since(Instant::now());
                if rest.is_zero() {
                    return Err(e);
                }
                // Mindestens 1 ms: 0 hieße „Standardwartezeit der Pipe“.
                let ms = (rest.as_millis() as u32).max(1);
                // SAFETY: nullterminierter Name.
                unsafe { WaitNamedPipeW(pfad.as_ptr(), ms) };
            }
            Err(e) => return Err(e),
        }
    }
}

/// Prozessnummer des Servers einer geöffneten Pipe.
pub fn server_pid(rohr: &Rohr) -> io::Result<u32> {
    let mut pid = 0u32;
    // SAFETY: eigenes Handle, Ausgabe in eine lokale Zahl.
    let ok = unsafe { GetNamedPipeServerProcessId(rohr.0.0, &mut pid) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(pid)
}

/// SID des Kontos am anderen Ende der Pipe (Client), über Impersonation auf
/// Identifikationsstufe: Der Dienst öffnet die Pipe mit
/// `SECURITY_IDENTIFICATION`, das reicht, um sein Token zu lesen, nicht, um
/// als er zu handeln. Geht nur, nachdem schon etwas gelesen wurde. Der Faden
/// kehrt danach immer zum eigenen Konto zurück (`RevertToSelf`).
pub fn client_sid(rohr: &Rohr) -> io::Result<String> {
    use windows_sys::Win32::Security::RevertToSelf;
    use windows_sys::Win32::System::Pipes::ImpersonateNamedPipeClient;
    use windows_sys::Win32::System::Threading::{GetCurrentThread, OpenThreadToken};
    // SAFETY: eigenes Pipe-Handle mit gelesenen Daten.
    if unsafe { ImpersonateNamedPipeClient(rohr.0.0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: Token des eigenen Fadens, nur lesen; OpenAsSelf, damit der Zugriff als Bote geprüft wird.
    let ok = unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut token) };
    let fehler = io::Error::last_os_error();
    // SAFETY: beendet die Impersonation dieses Fadens, immer.
    if unsafe { RevertToSelf() } == 0 {
        // Ohne Rückkehr darf der Faden nichts mehr tun.
        panic!("RevertToSelf ist gescheitert");
    }
    if ok == 0 {
        return Err(fehler);
    }
    token_sid(&Griff::gueltig(token)?)
}

/// Bricht laufendes Lesen oder Schreiben auf diesem Handle ab (aus einem
/// anderen Faden). Nur aufrufen, solange das Handle sicher offen ist.
pub fn abbrechen(roh: usize) {
    // SAFETY: der Aufrufer garantiert, dass das Handle noch offen ist.
    unsafe { CancelIoEx(roh as HANDLE, std::ptr::null()) };
}

/// SID des Kontos, unter dem ein Token läuft, als Text (`S-1-5-…`).
fn token_sid(token: &Griff) -> io::Result<String> {
    let mut laenge = 0u32;
    // SAFETY: erster Aufruf nur für die Größe.
    unsafe { GetTokenInformation(token.0, TokenUser, std::ptr::null_mut(), 0, &mut laenge) };
    if laenge == 0 {
        return Err(io::Error::last_os_error());
    }
    // u64-Puffer, damit TOKEN_USER richtig ausgerichtet ist.
    let mut puffer = vec![0u64; (laenge as usize).div_ceil(8)];
    // SAFETY: Puffer ist mindestens `laenge` Bytes groß.
    let ok = unsafe { GetTokenInformation(token.0, TokenUser, puffer.as_mut_ptr() as *mut c_void, laenge, &mut laenge) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: die API hat eine TOKEN_USER-Struktur an den Anfang geschrieben; die SID liegt im Puffer.
    let sid = unsafe { (*(puffer.as_ptr() as *const TOKEN_USER)).User.Sid };
    sid_als_text(sid)
}

/// Nimmt einen nullterminierten UTF-16-Text der API und gibt ihn mit LocalFree frei.
///
/// # Safety
/// `text` ist ein gültiger, mit LocalAlloc angelegter, nullterminierter Text.
unsafe fn api_text(text: *mut u16) -> String {
    let mut n = 0usize;
    // SAFETY: laut Aufrufer nullterminiert.
    while unsafe { *text.add(n) } != 0 {
        n += 1;
    }
    // SAFETY: `n` Zeichen ab `text` sind gültig; danach wird freigegeben.
    let s = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(text, n) });
    unsafe { LocalFree(text as *mut c_void) };
    s
}

/// Eine SID als Text (`S-1-5-…`).
fn sid_als_text(sid: PSID) -> io::Result<String> {
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: gültige SID; der Text wird in `api_text` mit LocalFree freigegeben.
    let ok = unsafe { ConvertSidToStringSidW(sid, &mut text) };
    if ok == 0 || text.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: von ConvertSidToStringSidW angelegt.
    Ok(unsafe { api_text(text) })
}

/// Eine SID aus Text; wird mit LocalFree freigegeben.
struct Sid(PSID);

impl Sid {
    fn aus_text(text: &str) -> io::Result<Sid> {
        let breit = breit(text);
        let mut sid: PSID = std::ptr::null_mut();
        // SAFETY: nullterminierter Text; die SID wird in Drop mit LocalFree freigegeben.
        let ok = unsafe { ConvertStringSidToSidW(breit.as_ptr(), &mut sid) };
        if ok == 0 || sid.is_null() {
            return Err(io::Error::last_os_error());
        }
        Ok(Sid(sid))
    }
}

impl Drop for Sid {
    fn drop(&mut self) {
        // SAFETY: von ConvertStringSidToSidW mit LocalAlloc angelegt.
        unsafe { LocalFree(self.0) };
    }
}

fn win32_ergebnis(code: u32) -> io::Result<()> {
    if code == 0 { Ok(()) } else { Err(io::Error::from_raw_os_error(code as i32)) }
}

/// Ist der Fehler „Zugriff verweigert“?
pub fn zugriff_verweigert(e: &io::Error) -> bool {
    e.raw_os_error() == Some(ERROR_ACCESS_DENIED as i32)
}

/// Erweitert die DACL eines Kernel-Objekts um einen Erlaubt-Eintrag für
/// `sid` mit `maske`. Besitzer und übrige Einträge bleiben, wie sie sind.
fn dacl_erweitern(objekt: HANDLE, sid: &str, maske: u32) -> io::Result<()> {
    let sid = Sid::aus_text(sid)?;
    let mut alt: *mut ACL = std::ptr::null_mut();
    let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: gültiges Handle mit READ_CONTROL; `alt` zeigt in `sd`, das mit LocalFree freigegeben wird.
    let code = unsafe {
        GetSecurityInfo(
            objekt,
            SE_KERNEL_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut alt,
            std::ptr::null_mut(),
            &mut sd,
        )
    };
    win32_ergebnis(code)?;
    let eintrag = EXPLICIT_ACCESS_W {
        grfAccessPermissions: maske,
        grfAccessMode: GRANT_ACCESS,
        grfInheritance: NO_INHERITANCE,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: std::ptr::null_mut(),
            MultipleTrusteeOperation: 0,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_UNKNOWN,
            ptstrName: sid.0 as *mut u16,
        },
    };
    let mut neu: *mut ACL = std::ptr::null_mut();
    // SAFETY: ein Eintrag; `alt` gültig (oder null); `neu` wird mit LocalFree freigegeben.
    let code = unsafe { SetEntriesInAclW(1, &eintrag, alt, &mut neu) };
    let ergebnis = win32_ergebnis(code).and_then(|()| {
        // SAFETY: nur die DACL wird gesetzt (Besitzer, Gruppe, SACL bleiben); Handle mit WRITE_DAC.
        let code =
            unsafe { SetSecurityInfo(objekt, SE_KERNEL_OBJECT, DACL_SECURITY_INFORMATION, std::ptr::null_mut(), std::ptr::null_mut(), neu, std::ptr::null()) };
        win32_ergebnis(code)
    });
    // SAFETY: beide mit LocalAlloc angelegt (oder null).
    unsafe {
        if !neu.is_null() {
            LocalFree(neu as *mut c_void);
        }
        LocalFree(sd);
    }
    ergebnis
}

/// Das eigene Token mit den Rechten `rechte`.
fn eigenes_token(rechte: u32) -> io::Result<Griff> {
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: Pseudo-Handle des eigenen Prozesses; das Token wird von `Griff` geschlossen.
    let ok = unsafe { OpenProcessToken(GetCurrentProcess(), rechte, &mut token) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Griff::gueltig(token)
}

/// Erlaubt dem Konto `sid` (dem Dienstkonto), den eigenen Prozess
/// abzufragen (PROCESS_QUERY_LIMITED_INFORMATION: Programmpfad) und das
/// eigene Token zu lesen (TOKEN_QUERY: Konto). Sonst scheitert die Prüfung
/// des Dienstes an OpenProcess/OpenProcessToken, weil die DACL eines Prozesses
/// unter dem Konto des Inhabers nur der Inhaber und SYSTEM kennt. Je ein zusätzlicher Eintrag;
/// Besitzer und alle übrigen Einträge bleiben.
pub fn pruefen_erlauben(sid: &str) -> io::Result<()> {
    // SAFETY: Pseudo-Handle des eigenen Prozesses (alle Rechte, muss nicht geschlossen werden).
    dacl_erweitern(unsafe { GetCurrentProcess() }, sid, PROCESS_QUERY_LIMITED_INFORMATION)?;
    let token = eigenes_token(READ_CONTROL | WRITE_DAC)?;
    dacl_erweitern(token.0, sid, TOKEN_QUERY)
}

/// Die DACL eines Kernel-Objekts als SDDL (zum Nachsehen und für Tests).
fn dacl_sddl(objekt: HANDLE) -> io::Result<String> {
    let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: gültiges Handle mit READ_CONTROL; `sd` wird mit LocalFree freigegeben.
    let code = unsafe {
        GetSecurityInfo(
            objekt,
            SE_KERNEL_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut sd,
        )
    };
    win32_ergebnis(code)?;
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: gültige Beschreibung; der Text wird in `api_text` freigegeben.
    let ok = unsafe { ConvertSecurityDescriptorToStringSecurityDescriptorW(sd, SDDL_REVISION_1, DACL_SECURITY_INFORMATION, &mut text, std::ptr::null_mut()) };
    let ergebnis = if ok == 0 || text.is_null() {
        Err(io::Error::last_os_error())
    } else {
        // SAFETY: von der API angelegt.
        Ok(unsafe { api_text(text) })
    };
    // SAFETY: von GetSecurityInfo mit LocalAlloc angelegt.
    unsafe { LocalFree(sd) };
    ergebnis
}

/// DACL des eigenen Prozesses und des eigenen Tokens als SDDL.
pub fn eigene_dacls() -> io::Result<(String, String)> {
    // SAFETY: Pseudo-Handle des eigenen Prozesses.
    let prozess = dacl_sddl(unsafe { GetCurrentProcess() })?;
    let token = dacl_sddl(eigenes_token(READ_CONTROL)?.0)?;
    Ok((prozess, token))
}

/// Besitzer der Pipe (die Sicherheitsbeschreibung des Pipe-Objekts, nicht
/// des Server-Prozesses) als SID-Text. Braucht READ_CONTROL am Handle.
pub fn pipe_besitzer(rohr: &Rohr) -> io::Result<String> {
    let mut besitzer: PSID = std::ptr::null_mut();
    let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: eigenes Handle; `besitzer` zeigt in `sd`, das danach mit LocalFree freigegeben wird.
    let code = unsafe {
        GetSecurityInfo(
            rohr.0.0,
            SE_KERNEL_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut besitzer,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut sd,
        )
    };
    win32_ergebnis(code)?;
    let ergebnis = if besitzer.is_null() { Err(io::Error::other("Die Pipe hat keinen Besitzer.")) } else { sid_als_text(besitzer) };
    // SAFETY: von GetSecurityInfo mit LocalAlloc angelegt.
    unsafe { LocalFree(sd) };
    ergebnis
}

/// SID des eigenen Kontos.
pub fn eigene_sid() -> io::Result<String> {
    token_sid(&eigenes_token(TOKEN_QUERY)?)
}

fn prozess_oeffnen(pid: u32) -> io::Result<Griff> {
    // SAFETY: nur Abfragerechte, kein Erben.
    let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    Griff::gueltig(h)
}

/// SID des Kontos, unter dem der Prozess `pid` läuft.
pub fn prozess_sid(pid: u32) -> io::Result<String> {
    let p = prozess_oeffnen(pid)?;
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: gültiges Prozess-Handle; das Token wird von `Griff` geschlossen.
    let ok = unsafe { OpenProcessToken(p.0, TOKEN_QUERY, &mut token) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    token_sid(&Griff::gueltig(token)?)
}

/// Vollständiger Pfad des Programms von Prozess `pid`.
pub fn prozess_programm(pid: u32) -> io::Result<PathBuf> {
    let p = prozess_oeffnen(pid)?;
    let mut puffer = vec![0u16; 32768];
    let mut n = puffer.len() as u32;
    // SAFETY: Puffer mit `n` Zeichen; die API setzt `n` auf die geschriebene Länge.
    let ok = unsafe { QueryFullProcessImageNameW(p.0, PROCESS_NAME_WIN32, puffer.as_mut_ptr(), &mut n) };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(PathBuf::from(String::from_utf16_lossy(&puffer[..n as usize])))
}
