//! Negotiate-Aushandlung mit SSPI (secur32.dll), Server-Seite. Auf 127.0.0.1
//! läuft sie in der Praxis als NTLM über Loopback (zwei Runden), mit Kerberos
//! wäre es eine Runde. Danach liefert der Kontext das Zugangs-Token des
//! anfragenden Kontos, daraus die SID als Text.
//!
//! Alle Handles werden beim Fallenlassen freigegeben: `Kontext` löscht den
//! Sicherheitskontext (DeleteSecurityContext) und gibt danach seine
//! Berechtigung frei (FreeCredentialsHandle); Tokens schließt `CloseHandle`.
//! Fehler sind kurze technische Sätze fürs Protokoll (Funktion und Code), nie
//! mit Name, SID oder Nachweis.

use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, LocalFree, SEC_E_OK, SEC_I_COMPLETE_AND_CONTINUE, SEC_I_COMPLETE_NEEDED, SEC_I_CONTINUE_NEEDED};
use windows_sys::Win32::Security::Authentication::Identity::{
    ASC_REQ_ALLOCATE_MEMORY, ASC_REQ_CONNECTION, AcceptSecurityContext, AcquireCredentialsHandleW, CompleteAuthToken, DeleteSecurityContext, FreeContextBuffer,
    FreeCredentialsHandle, QueryContextAttributesW, QuerySecurityContextToken, SECBUFFER_TOKEN, SECBUFFER_VERSION, SECPKG_ATTR_NEGOTIATION_INFO,
    SECPKG_ATTR_PACKAGE_INFO, SECPKG_CRED_INBOUND, SECURITY_NATIVE_DREP, SecBuffer, SecBufferDesc, SecPkgContext_NegotiationInfoW, SecPkgContext_PackageInfoW,
};
use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows_sys::Win32::Security::Credentials::SecHandle;
use windows_sys::Win32::Security::{GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use super::Mechanismus;

/// Das Sicherheitspaket: Negotiate wählt selbst zwischen Kerberos und NTLM.
pub const PAKET: &str = "Negotiate";

fn breit(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn fehler(funktion: &str, code: i32) -> String {
    format!("{funktion}: 0x{:08X}", code as u32)
}

/// Text aus einem nullterminierten UTF-16-Zeiger.
///
/// # Safety
/// `p` ist null oder zeigt auf einen nullterminierten UTF-16-Text.
unsafe fn text_aus(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut n = 0;
    // SAFETY: laut Vertrag nullterminiert; gelesen wird nur bis zur Null.
    unsafe {
        while *p.add(n) != 0 {
            n += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(p, n))
    }
}

// ----- Berechtigung (Credentials-Handle) -----

/// Ein Credentials-Handle des eigenen Kontos für das Paket Negotiate.
pub struct Berechtigung(SecHandle);

impl Berechtigung {
    /// `richtung` ist `SECPKG_CRED_INBOUND` (Server) oder `SECPKG_CRED_OUTBOUND` (Client).
    pub fn holen(richtung: u32) -> Result<Self, String> {
        let paket = breit(PAKET);
        let mut h = SecHandle::default();
        let mut ablauf = 0i64;
        // SAFETY: alle Zeiger zeigen auf lebende lokale Werte oder sind null (erlaubt).
        let r = unsafe { AcquireCredentialsHandleW(null(), paket.as_ptr(), richtung, null(), null(), None, null(), &mut h, &mut ablauf) };
        if r != SEC_E_OK {
            return Err(fehler("AcquireCredentialsHandleW", r));
        }
        Ok(Berechtigung(h))
    }
}

impl Drop for Berechtigung {
    fn drop(&mut self) {
        // SAFETY: das Handle stammt aus AcquireCredentialsHandleW und wird nur hier freigegeben.
        unsafe { FreeCredentialsHandle(&self.0) };
    }
}

/// Gelingt AcquireCredentialsHandle für eingehende Anmeldungen?
pub fn verfuegbar() -> bool {
    Berechtigung::holen(SECPKG_CRED_INBOUND).is_ok()
}

// ----- Kontext -----

/// Ergebnis eines Schritts der Aushandlung.
pub enum Schritt {
    /// Es braucht noch eine Runde: dieser Nachweis geht mit 401 zurück.
    Weiter(Vec<u8>),
    /// Fertig. `ausgabe` ist ein letzter Nachweis für den Client (kann leer sein).
    /// `konto` ist der Name des Kontos (`PC\Name`; Etappe 7c, für die Anträge).
    Fertig { ausgabe: Vec<u8>, sid: String, konto: String, mechanismus: Mechanismus },
}

/// Ein Sicherheitskontext samt seiner Berechtigung. Gehört zu genau einer
/// Aushandlung; `Drop` gibt beides frei.
pub struct Kontext {
    // Reihenfolge wichtig: erst der Kontext (in `drop`), dann die Berechtigung.
    handle: Option<SecHandle>,
    berechtigung: Berechtigung,
}

impl Drop for Kontext {
    fn drop(&mut self) {
        if let Some(h) = self.handle.take() {
            // SAFETY: das Handle stammt aus Accept-/InitializeSecurityContext und wird nur hier gelöscht.
            unsafe { DeleteSecurityContext(&h) };
        }
    }
}

/// Ausgabepuffer, den SSPI selbst angelegt hat (`*_REQ_ALLOCATE_MEMORY`), als Vec.
///
/// # Safety
/// `b.pvBuffer` ist null oder ein von SSPI angelegter Puffer mit `cbBuffer` Bytes.
unsafe fn ausgabe_nehmen(b: &SecBuffer) -> Vec<u8> {
    if b.pvBuffer.is_null() {
        return Vec::new();
    }
    // SAFETY: laut Vertrag ein SSPI-Puffer mit `cbBuffer` Bytes, danach nur hier freigegeben.
    unsafe {
        let v = std::slice::from_raw_parts(b.pvBuffer as *const u8, b.cbBuffer as usize).to_vec();
        FreeContextBuffer(b.pvBuffer);
        v
    }
}

impl Kontext {
    /// Neuer Server-Kontext (noch ohne Handle; das legt der erste Schritt an).
    pub fn server() -> Result<Self, String> {
        Ok(Kontext { handle: None, berechtigung: Berechtigung::holen(SECPKG_CRED_INBOUND)? })
    }

    /// Ein Schritt der Server-Seite: AcceptSecurityContext mit dem Nachweis des Clients.
    pub fn annehmen(&mut self, nachweis: &[u8]) -> Result<Schritt, String> {
        let mut ein = SecBuffer {
            cbBuffer: u32::try_from(nachweis.len()).map_err(|_| "Nachweis zu lang".to_string())?,
            BufferType: SECBUFFER_TOKEN,
            pvBuffer: nachweis.as_ptr() as *mut _,
        };
        let ein_desc = SecBufferDesc { ulVersion: SECBUFFER_VERSION, cBuffers: 1, pBuffers: &mut ein };
        let mut aus = SecBuffer { cbBuffer: 0, BufferType: SECBUFFER_TOKEN, pvBuffer: null_mut() };
        let mut aus_desc = SecBufferDesc { ulVersion: SECBUFFER_VERSION, cBuffers: 1, pBuffers: &mut aus };
        let mut neu = self.handle.unwrap_or_default();
        let neu_ptr: *mut SecHandle = &mut neu;
        let alt: *const SecHandle = if self.handle.is_some() { neu_ptr } else { null() };
        let mut attr = 0u32;
        let mut ablauf = 0i64;
        // SAFETY: alle Puffer leben bis nach dem Aufruf; `alt` ist null oder zeigt auf `neu`
        // (SSPI erlaubt denselben Kontext als Ein- und Ausgabe).
        let r = unsafe {
            AcceptSecurityContext(
                &self.berechtigung.0,
                alt,
                &ein_desc,
                ASC_REQ_CONNECTION | ASC_REQ_ALLOCATE_MEMORY,
                SECURITY_NATIVE_DREP,
                neu_ptr,
                &mut aus_desc,
                &mut attr,
                &mut ablauf,
            )
        };
        if r < 0 {
            // SAFETY: bei Fehler legt SSPI keinen Puffer an; falls doch, wird er freigegeben.
            unsafe { ausgabe_nehmen(&aus) };
            return Err(fehler("AcceptSecurityContext", r));
        }
        self.handle = Some(neu);
        if r == SEC_I_COMPLETE_NEEDED || r == SEC_I_COMPLETE_AND_CONTINUE {
            // SAFETY: Kontext und Ausgabepuffer stammen aus dem Aufruf eben.
            let c = unsafe { CompleteAuthToken(&neu, &aus_desc) };
            if c != SEC_E_OK {
                unsafe { ausgabe_nehmen(&aus) };
                return Err(fehler("CompleteAuthToken", c));
            }
        }
        // SAFETY: Puffer von SSPI (ASC_REQ_ALLOCATE_MEMORY).
        let ausgabe = unsafe { ausgabe_nehmen(&aus) };
        if r == SEC_I_CONTINUE_NEEDED || r == SEC_I_COMPLETE_AND_CONTINUE {
            return Ok(Schritt::Weiter(ausgabe));
        }
        let (sid, konto) = self.konto()?;
        Ok(Schritt::Fertig { ausgabe, sid, konto, mechanismus: self.mechanismus() })
    }

    /// SID und Name des angemeldeten Kontos (nach `Fertig`).
    fn konto(&self) -> Result<(String, String), String> {
        let h = self.handle.ok_or("Kein Kontext")?;
        let mut token: HANDLE = null_mut();
        // SAFETY: fertiger Kontext; das Token wird unten geschlossen.
        let r = unsafe { QuerySecurityContextToken(&h, &mut token) };
        if r != SEC_E_OK {
            return Err(fehler("QuerySecurityContextToken", r));
        }
        let konto = konto_aus_token(token);
        // SAFETY: Token aus QuerySecurityContextToken, nur hier geschlossen.
        unsafe { CloseHandle(token) };
        konto
    }

    /// Welches Paket Negotiate gewählt hat.
    fn mechanismus(&self) -> Mechanismus {
        let Some(h) = self.handle else { return Mechanismus::Unbekannt };
        let mut info = SecPkgContext_NegotiationInfoW::default();
        // SAFETY: fertiger Kontext; der Paket-Puffer wird mit FreeContextBuffer freigegeben.
        unsafe {
            if QueryContextAttributesW(&h, SECPKG_ATTR_NEGOTIATION_INFO, &mut info as *mut _ as *mut _) == SEC_E_OK && !info.PackageInfo.is_null() {
                let name = text_aus((*info.PackageInfo).Name);
                FreeContextBuffer(info.PackageInfo as *mut _);
                return Mechanismus::aus_paket(&name);
            }
            let mut paket = SecPkgContext_PackageInfoW::default();
            if QueryContextAttributesW(&h, SECPKG_ATTR_PACKAGE_INFO, &mut paket as *mut _ as *mut _) == SEC_E_OK && !paket.PackageInfo.is_null() {
                let name = text_aus((*paket.PackageInfo).Name);
                FreeContextBuffer(paket.PackageInfo as *mut _);
                return Mechanismus::aus_paket(&name);
            }
        }
        Mechanismus::Unbekannt
    }
}

// ----- SID aus einem Token -----

/// SID des Kontos eines Tokens als Text (`S-1-5-21-…`). Das Token bleibt offen.
fn sid_aus_token(token: HANDLE) -> Result<String, String> {
    konto_aus_token(token).map(|(sid, _)| sid)
}

/// Name eines Kontos (`PC\Name` oder `DOMÄNE\Name`) zu seiner SID; ohne Treffer `None`.
///
/// # Safety
/// `sid` zeigt auf eine gültige SID.
unsafe fn name_zur_sid(sid: windows_sys::Win32::Security::PSID) -> Option<String> {
    use windows_sys::Win32::Security::{LookupAccountSidW, SID_NAME_USE};
    let mut name = vec![0u16; 256];
    let mut domaene = vec![0u16; 256];
    let (mut n, mut d) = (name.len() as u32, domaene.len() as u32);
    let mut art: SID_NAME_USE = 0;
    // SAFETY: Puffer mit den angegebenen Längen; die SID ist laut Vertrag gültig.
    let ok = unsafe { LookupAccountSidW(null(), sid, name.as_mut_ptr(), &mut n, domaene.as_mut_ptr(), &mut d, &mut art) };
    if ok == 0 {
        return None;
    }
    let name = String::from_utf16_lossy(&name[..(n as usize).min(name.len())]);
    let domaene = String::from_utf16_lossy(&domaene[..(d as usize).min(domaene.len())]);
    Some(if domaene.is_empty() { name } else { format!("{domaene}\\{name}") })
}

/// SID als Text und Name des Kontos eines Tokens. Fehlt der Name (Konto
/// gelöscht, Domäne nicht erreichbar), steht dort die SID. Das Token bleibt offen.
fn konto_aus_token(token: HANDLE) -> Result<(String, String), String> {
    let mut laenge = 0u32;
    // SAFETY: erste Abfrage nur nach der Größe (Puffer null, Länge 0).
    unsafe { GetTokenInformation(token, TokenUser, null_mut(), 0, &mut laenge) };
    if laenge == 0 {
        return Err("GetTokenInformation: keine Größe".to_string());
    }
    // u64 für die Ausrichtung von TOKEN_USER.
    let mut puffer = vec![0u64; (laenge as usize).div_ceil(8)];
    // SAFETY: Puffer mit mindestens `laenge` Bytes.
    let ok = unsafe { GetTokenInformation(token, TokenUser, puffer.as_mut_ptr() as *mut _, laenge, &mut laenge) };
    if ok == 0 {
        return Err(fehler("GetTokenInformation", std::io::Error::last_os_error().raw_os_error().unwrap_or(0)));
    }
    // SAFETY: GetTokenInformation(TokenUser) hat ein TOKEN_USER an den Anfang geschrieben.
    let nutzer = unsafe { &*(puffer.as_ptr() as *const TOKEN_USER) };
    let mut text: *mut u16 = null_mut();
    // SAFETY: die SID liegt in `puffer`; der Text wird mit LocalFree freigegeben.
    unsafe {
        if ConvertSidToStringSidW(nutzer.User.Sid, &mut text) == 0 {
            return Err(fehler("ConvertSidToStringSidW", std::io::Error::last_os_error().raw_os_error().unwrap_or(0)));
        }
        let s = text_aus(text);
        LocalFree(text as *mut _);
        let name = name_zur_sid(nutzer.User.Sid).unwrap_or_else(|| s.clone());
        Ok((s, name))
    }
}

/// SID des Kontos, unter dem dieser Prozess läuft (Vergleich im Altweg und in
/// Tests, wenn `inhaber_sid` fehlt).
pub fn eigene_sid() -> Result<String, String> {
    let mut token: HANDLE = null_mut();
    // SAFETY: GetCurrentProcess ist ein Pseudo-Handle; das Token wird unten geschlossen.
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(fehler("OpenProcessToken", std::io::Error::last_os_error().raw_os_error().unwrap_or(0)));
    }
    let sid = sid_aus_token(token);
    // SAFETY: Token aus OpenProcessToken, nur hier geschlossen.
    unsafe { CloseHandle(token) };
    sid
}

// ----- Client-Seite: Aushandlung mit dem eigenen Konto (Tests und `dashboard herzschlag`) -----

pub mod client {
    use super::*;
    use windows_sys::Win32::Security::Authentication::Identity::{
        ISC_REQ_ALLOCATE_MEMORY, ISC_REQ_CONNECTION, InitializeSecurityContextW, SECPKG_CRED_OUTBOUND,
    };

    /// Client-Seite einer Aushandlung mit dem eigenen Konto (wie curl --negotiate).
    pub struct Client {
        kontext: Kontext,
        ziel: Vec<u16>,
    }

    impl Client {
        pub fn neu(ziel: &str) -> Result<Self, String> {
            Ok(Client { kontext: Kontext { handle: None, berechtigung: Berechtigung::holen(SECPKG_CRED_OUTBOUND)? }, ziel: breit(ziel) })
        }

        /// Ein Schritt: liefert den nächsten Nachweis und ob der Client fertig ist.
        pub fn schritt(&mut self, eingabe: Option<&[u8]>) -> Result<(Vec<u8>, bool), String> {
            let mut ein = SecBuffer {
                cbBuffer: eingabe.map_or(0, |e| e.len() as u32),
                BufferType: SECBUFFER_TOKEN,
                pvBuffer: eingabe.map_or(null_mut(), |e| e.as_ptr() as *mut _),
            };
            let ein_desc = SecBufferDesc { ulVersion: SECBUFFER_VERSION, cBuffers: 1, pBuffers: &mut ein };
            let ein_ptr: *const SecBufferDesc = if eingabe.is_some() { &ein_desc } else { null() };
            let mut aus = SecBuffer { cbBuffer: 0, BufferType: SECBUFFER_TOKEN, pvBuffer: null_mut() };
            let mut aus_desc = SecBufferDesc { ulVersion: SECBUFFER_VERSION, cBuffers: 1, pBuffers: &mut aus };
            let mut neu = self.kontext.handle.unwrap_or_default();
            let neu_ptr: *mut SecHandle = &mut neu;
            let alt: *const SecHandle = if self.kontext.handle.is_some() { neu_ptr } else { null() };
            let mut attr = 0u32;
            let mut ablauf = 0i64;
            // SAFETY: wie bei `annehmen`.
            let r = unsafe {
                InitializeSecurityContextW(
                    &self.kontext.berechtigung.0,
                    alt,
                    self.ziel.as_ptr(),
                    ISC_REQ_CONNECTION | ISC_REQ_ALLOCATE_MEMORY,
                    0,
                    SECURITY_NATIVE_DREP,
                    ein_ptr,
                    0,
                    neu_ptr,
                    &mut aus_desc,
                    &mut attr,
                    &mut ablauf,
                )
            };
            if r < 0 {
                // SAFETY: bei Fehler legt SSPI keinen Puffer an; falls doch, wird er freigegeben.
                unsafe { ausgabe_nehmen(&aus) };
                return Err(fehler("InitializeSecurityContextW", r));
            }
            self.kontext.handle = Some(neu);
            // SAFETY: Puffer von SSPI (ISC_REQ_ALLOCATE_MEMORY).
            let ausgabe = unsafe { ausgabe_nehmen(&aus) };
            Ok((ausgabe, r == SEC_E_OK))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::client::Client;
    use super::*;
    use crate::anmeldung::ist_inhaber;

    #[test]
    fn eigene_sid_ist_lesbar() {
        let sid = eigene_sid().unwrap();
        assert!(sid.starts_with("S-1-"), "keine SID-Form");
        assert!(verfuegbar(), "AcquireCredentialsHandle für Negotiate");
    }

    /// Client und Server im selben Prozess, bis SEC_E_OK; danach ist die SID die eigene.
    #[test]
    fn aushandlung_im_selben_prozess() {
        let mut client = Client::neu("HTTP/localhost").unwrap();
        let mut server = Kontext::server().unwrap();
        let (mut nachweis, _) = client.schritt(None).unwrap();
        for _ in 0..5 {
            match server.annehmen(&nachweis).unwrap() {
                Schritt::Weiter(antwort) => {
                    let (n, _) = client.schritt(Some(&antwort)).unwrap();
                    nachweis = n;
                }
                Schritt::Fertig { ausgabe, sid, konto, mechanismus } => {
                    if !ausgabe.is_empty() {
                        client.schritt(Some(&ausgabe)).unwrap();
                    }
                    assert!(ist_inhaber(&sid, &eigene_sid().unwrap()), "SID passt nicht");
                    assert!(konto.contains('\\') && !konto.starts_with("S-1-"), "Kontoname: {konto}");
                    assert_ne!(mechanismus, Mechanismus::Unbekannt);
                    return;
                }
            }
        }
        panic!("Aushandlung nach 5 Runden nicht fertig");
    }

    #[test]
    fn unsinn_als_nachweis_wird_abgelehnt() {
        let mut server = Kontext::server().unwrap();
        assert!(server.annehmen(b"kein echter Nachweis").is_err());
    }
}
