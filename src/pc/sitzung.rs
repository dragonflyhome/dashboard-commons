//! Ist der Inhaber an diesem PC angemeldet? (Etappe A3, Meldung „Bote fehlt“;
//! Hotfix Sitzungsabfrage)
//!
//! Der Bote läuft in der Sitzung des Inhabers (Aufgabenplanung bei der Anmeldung).
//! Fehlt er, ist das nur ein Fehler, wenn der Inhaber angemeldet ist; nachts oder
//! abgemeldet fehlt er erwartungsgemäß ([nach_stellungnahme_designteam]).
//!
//! ## Warum nicht mehr `WTSEnumerateSessionsW` (Fehler 259)
//!
//! Bis A3 zählte der Dienst alle Sitzungen auf (`WTSEnumerateSessionsW`,
//! `WTS_CURRENT_SERVER_HANDLE`, Reserved 0, Version 1, also genau nach
//! Dokumentation) und verglich deren Konten mit `konten.inhaber_sid`.
//! Belegt (live gemessen): Unter dem Dienstkonto `NT SERVICE\<Dienst>` in
//! Sitzung 0, mit nur SeChangeNotifyPrivilege, gibt der Aufruf FALSE zurück
//! und `GetLastError` 259 (ERROR_NO_MORE_ITEMS); derselbe Aufruf ging unter
//! dem Inhaber in seiner Sitzung (bis A3 Test `eigene_sitzung_und_fremde_sid`). Die
//! Parameter sind es also nicht. Vermutet, nicht belegt: Der Sitzungsmanager (LSM)
//! gibt einem Aufrufer nur die Sitzungen heraus, auf die er
//! WINSTATION_QUERY hat, und `winsta.dll` meldet eine leere Liste als
//! „keine Daten mehr“ (259) statt als Liste ohne Einträge; das
//! virtuelle Dienstkonto steht in keiner der Gruppen, denen die Sitzungen
//! Abfragen erlauben. Ebenso möglich: Die Funktion setzt bei diesem Fehler
//! keinen eigenen Fehlercode, und 259 ist ein Rest im Faden. Darum fragt der
//! Dienst jetzt nicht mehr alle Sitzungen ab, sondern nur eine, deren Nummer
//! er kennt, und hat einen Ersatz, falls auch das scheitert.
//!
//! ## Die Regel
//!
//! 1. Der Bote meldet bei jedem Hallo (Lebenszeichen alle 60 s) seine
//!    Sitzung: Nummer (`ProcessIdToSessionId`), ob es die Konsolensitzung
//!    ist (`WTSGetActiveConsoleSessionId`) und ihren Zustand
//!    (`WTSQuerySessionInformationW` auf die eigene Sitzung, das darf jeder
//!    in seiner Sitzung). Weder SID noch Name ([`eigene_sitzung`]). Ein
//!    älterer Bote meldet nichts: dann ist die Sitzung unbekannt.
//! 2. Der Dienst fragt Windows jede Minute ohne Aufzählen
//!    ([`SitzungsAbfrage`], in Tests ersetzbar): Ist die zuletzt gemeldete
//!    Sitzung noch da und aktiv oder getrennt (`WTSConnectState`), ist der Inhaber
//!    angemeldet (die Sitzung gehörte dem Boten, und dessen Konto prüft der
//!    Client an der Pipe). Sonst, falls der Inhaber sich neu angemeldet hat und der
//!    Bote dort nicht startete: die Konsolensitzung, wenn sie aktiv oder
//!    getrennt ist und ihr Konto `konten.inhaber_sid` ist. Quelle `windows`.
//! 3. Scheitert die Abfrage bei Windows (Fehler bleibt in `fehler` stehen,
//!    einmal je Wechsel eine Warnung), gilt der Bericht des Boten, aber nur
//!    so weit, dass eine Abmeldung nie wie ein Absturz des Boten aussieht
//!    (ein falscher Alarm ist schlimmer als ein fehlender). Ist er verbunden,
//!    ist der Inhaber angemeldet. Fehlt er und hatte er `konsole: true` gemeldet,
//!    zählt seine Sitzung nur, solange `WTSGetActiveConsoleSessionId` (braucht
//!    keine Rechte) dieselbe Nummer liefert: Nach einer Abmeldung bekommt der
//!    Anmeldebildschirm eine neue Sitzung (vermutet, nicht belegt: Windows
//!    vergibt die Nummern aufsteigend). Ohne Konsolenbezug (`konsole: false`,
//!    etwa eine Fernsitzung) gilt der Bericht höchstens
//!    [`BERICHT_OHNE_KONSOLE`] (5 Minuten) nach dem letzten Hallo; das ist
//!    nicht länger als die zehn Minuten bis „Bote fehlt“, also kommt ohne
//!    Konsolenbezug aus dem Rückfall nie ein Alarm (gewollt). Quelle `bote`.
//!    Sonst `unbekannt`, `inhaber_angemeldet` null, keine Meldung.
//! 5. Sagt Windows sicher „nicht angemeldet“ (Quelle `windows`), ist eine
//!    offene „Bote fehlt“ behoben (`meldungen::bote_takt`); bei `unbekannt`
//!    bleibt sie offen, bis der Bote wieder da ist.
//! 4. Hat Windows schon gesagt, dass die gemeldete Sitzung weg oder nicht
//!    angemeldet ist, und scheitert erst die Frage nach der Konsolensitzung
//!    (Zustand, Konto), gibt es keinen Rückfall auf den Bericht: `unbekannt`.
//!
//! [`Waechter`] merkt sich den Ausgang für `/api/betrieb` (ohne SID und
//! Namen) und sagt einmal je Wechsel, was ins Protokoll gehört, nicht jede
//! Minute. Es wird nichts geändert und nichts gespeichert.

use std::sync::{Arc, Mutex};

use crate::speicher::aufgaben::Zeit;
use crate::vertrag::api::SitzungsStand;
use crate::vertrag::bote::{BoteSitzung, SitzungsZustand};

/// So lange gilt ein Bericht des Boten ohne Konsolenbezug (`konsole: false`)
/// nach dem letzten Hallo, wenn Windows nicht antwortet. Höchstens so lang
/// wie `meldungen::BOTE_FEHLT_NACH` (Test `bericht_ohne_konsole_kuerzer_als_alarm`):
/// Sonst sähe eine Abmeldung ohne Konsolenbezug wie ein Absturz des Boten aus.
pub const BERICHT_OHNE_KONSOLE: chrono::Duration = chrono::Duration::minutes(5);

/// Warum die Abfrage keine Antwort gab. Die Texte nennen weder SID noch Namen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SitzungsFehler {
    /// `konten.inhaber_sid` ist leer (oder es gibt keine Einstellungen).
    SidFehlt,
    /// Ein WTS-Aufruf scheiterte (Funktion und Satz von Windows).
    Wts(String),
    /// Die Abfrage brach ab (Aufgabe im Hintergrund).
    Abgebrochen(String),
    /// Nicht unter Windows.
    NurWindows,
}

impl SitzungsFehler {
    /// Die Art als Wort für `/api/betrieb`.
    pub fn art(&self) -> &'static str {
        match self {
            Self::SidFehlt => "sid-fehlt",
            Self::Wts(_) => "wts",
            Self::Abgebrochen(_) => "abgebrochen",
            Self::NurWindows => "nur-windows",
        }
    }
}

impl std::fmt::Display for SitzungsFehler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SidFehlt => write!(f, "konten.inhaber_sid fehlt"),
            Self::Wts(e) => write!(f, "{e}"),
            Self::Abgebrochen(e) => write!(f, "abgebrochen: {e}"),
            Self::NurWindows => write!(f, "nur unter Windows"),
        }
    }
}

/// Art, wenn die Abfrage ging, der Inhaber aber nicht sah, obwohl der Bote verbunden ist.
pub const SIEHT_INHABER_NICHT: &str = "sieht-inhaber-nicht";

/// Woher „Inhaber angemeldet“ kommt (Feld `quelle` in `/api/betrieb`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quelle {
    /// Die Abfrage bei Windows ging.
    Windows,
    /// Sie scheiterte; es gilt der letzte Bericht des Boten.
    Bote,
    /// Sie scheiterte, und es gibt keinen Bericht.
    Unbekannt,
}

impl Quelle {
    pub fn wort(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Bote => "bote",
            Self::Unbekannt => "unbekannt",
        }
    }
}

/// Der letzte Bericht des Boten über seine Sitzung (aus `BoteStand`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bericht {
    /// `None`: ein älterer Bote, der keine Sitzung meldet.
    pub sitzung: Option<BoteSitzung>,
    /// Wann das Hallo kam.
    pub gemeldet: Zeit,
}

/// Ergebnis einer Prüfung: was Windows sagte, und was daraus folgt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitzungsBefund {
    /// Die Antwort von Windows (der Inhaber angemeldet?) oder warum es keine gab.
    pub abfrage: Result<bool, SitzungsFehler>,
    /// Was gilt: bei `Ok` die Antwort von Windows, sonst der Ersatz aus dem
    /// Bericht des Boten; `None`, wenn es keinen gibt.
    pub angemeldet: Option<bool>,
    pub quelle: Quelle,
}

impl SitzungsBefund {
    /// Ist der Inhaber angemeldet? Ohne Quelle: nein (dann keine Meldung).
    pub fn inhaber_angemeldet(&self) -> bool {
        self.angemeldet == Some(true)
    }

    /// Sagt Windows sicher „nicht angemeldet“ (nicht der Rückfall, nicht unbekannt)?
    pub fn windows_sagt_abgemeldet(&self) -> bool {
        self.quelle == Quelle::Windows && self.abfrage == Ok(false)
    }
}

/// Was der Dienst Windows fragt, ohne Sitzungen aufzuzählen. In Tests ersetzbar.
pub trait SitzungsAbfrage: Send + Sync {
    /// Die Sitzung an der Konsole (`WTSGetActiveConsoleSessionId`); `None`,
    /// wenn gerade keine angeschlossen ist.
    fn konsole(&self) -> Option<u32>;
    /// Zustand der Sitzung `id` (`WTSConnectState`); `Ok(None)`: Die Sitzung
    /// gibt es nicht (mehr).
    fn zustand(&self, id: u32) -> Result<Option<SitzungsZustand>, SitzungsFehler>;
    /// SID des Kontos der Sitzung `id` (Text); `Ok(None)`: niemand angemeldet.
    /// Bleibt in diesem Modul, geht nie nach außen.
    fn konto_sid(&self, id: u32) -> Result<Option<String>, SitzungsFehler>;
}

/// Die Regel (siehe oben). `bericht`: der letzte Bericht des Boten, auch wenn
/// er gerade fehlt; `bote_verbunden`: er antwortet gerade.
pub fn pruefen(q: &dyn SitzungsAbfrage, bericht: Option<&Bericht>, bote_verbunden: bool, inhaber_sid: &str, jetzt: Zeit) -> SitzungsBefund {
    let windows = |b: bool| SitzungsBefund { abfrage: Ok(b), angemeldet: Some(b), quelle: Quelle::Windows };
    let gemeldet = bericht.and_then(|b| b.sitzung).map(|s| s.id);
    // Hat Windows schon gesagt, dass die gemeldete Sitzung weg oder nicht
    // angemeldet ist? Dann kippt ein späterer Fehler nicht in den Rückfall.
    let mut gemeldet_weg = false;
    if let Some(id) = gemeldet {
        match q.zustand(id) {
            Ok(z) if z.is_some_and(SitzungsZustand::angemeldet) => return windows(true),
            Ok(_) => gemeldet_weg = true,
            Err(e) => return ersatz(e, bericht, bote_verbunden, q.konsole(), jetzt),
        }
    }
    match konsole_fragen(q, gemeldet, inhaber_sid) {
        Ok(b) => windows(b),
        Err(e) if gemeldet_weg => SitzungsBefund { abfrage: Err(e), angemeldet: None, quelle: Quelle::Unbekannt },
        Err(e) => ersatz(e, bericht, bote_verbunden, q.konsole(), jetzt),
    }
}

/// Windows hat nicht geantwortet (`fehler`): Es gilt der Bote, aber nie so,
/// dass eine Abmeldung wie sein Absturz aussieht. Antwortet er, läuft er in
/// der Sitzung des Inhabers (auch ein älterer, der seine Sitzung nicht nennt). Fehlt
/// er: Seine Sitzung zählt, wenn sie laut Bericht angemeldet war (oder ihr
/// Zustand unbekannt) und entweder (`konsole: true`) die Konsole jetzt noch
/// dieselbe Nummer hat (`konsole_jetzt`, aus `WTSGetActiveConsoleSessionId`)
/// oder (`konsole: false`) das Hallo höchstens `BERICHT_OHNE_KONSOLE` her
/// ist. Sonst unbekannt, keine Meldung.
pub fn ersatz(fehler: SitzungsFehler, bericht: Option<&Bericht>, bote_verbunden: bool, konsole_jetzt: Option<u32>, jetzt: Zeit) -> SitzungsBefund {
    let (angemeldet, quelle) = if bote_verbunden {
        (Some(true), Quelle::Bote)
    } else {
        match bericht.and_then(|b| b.sitzung.map(|s| (b.gemeldet, s))) {
            // Der Bote sah seine Sitzung selbst nicht angemeldet.
            Some((_, s)) if !s.zustand.is_none_or(SitzungsZustand::angemeldet) => (Some(false), Quelle::Bote),
            Some((_, s)) if s.konsole && konsole_jetzt == Some(s.id) => (Some(true), Quelle::Bote),
            Some((gemeldet, s)) if !s.konsole && jetzt - gemeldet <= BERICHT_OHNE_KONSOLE => (Some(true), Quelle::Bote),
            _ => (None, Quelle::Unbekannt),
        }
    };
    SitzungsBefund { abfrage: Err(fehler), angemeldet, quelle }
}

/// Fragt Windows nach der Konsolensitzung und ihrem Konto (der Inhaber hat sich
/// neu angemeldet, der Bote startete dort nicht). Die gemeldete Sitzung ist
/// schon geprüft.
fn konsole_fragen(q: &dyn SitzungsAbfrage, gemeldet: Option<u32>, inhaber_sid: &str) -> Result<bool, SitzungsFehler> {
    let Some(konsole) = q.konsole() else { return Ok(false) };
    if Some(konsole) == gemeldet {
        return Ok(false);
    }
    if !q.zustand(konsole)?.is_some_and(SitzungsZustand::angemeldet) {
        return Ok(false);
    }
    if inhaber_sid.trim().is_empty() {
        return Err(SitzungsFehler::SidFehlt);
    }
    Ok(q.konto_sid(konsole)?.is_some_and(|sid| sid.eq_ignore_ascii_case(inhaber_sid.trim())))
}

/// Was nach einer Abfrage ins Protokoll gehört (einmal je Wechsel).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Protokoll {
    /// Neu gescheitert, oder mit einer anderen Art als zuletzt: einmal `warn`.
    Warnung(String),
    /// Geht wieder: einmal `info`.
    WiederDa,
}

/// Merkt sich den Ausgang der Abfragen für `/api/betrieb` und entscheidet,
/// wann etwas ins Protokoll gehört.
#[derive(Debug, Default)]
pub struct Waechter {
    stand: SitzungsStand,
}

/// Der Wächter, geteilt zwischen der Meldungs-Schleife und `/api/betrieb`.
pub type GeteilterWaechter = Arc<Mutex<Waechter>>;

impl Waechter {
    pub fn stand(&self) -> SitzungsStand {
        self.stand.clone()
    }

    /// Trägt einen Befund ein. `bote_verbunden`: Der Bote läuft in der
    /// Sitzung des Inhabers; sieht Windows sie dann nicht, stimmt etwas mit der Abfrage
    /// nicht (Art `sieht-inhaber-nicht`). Gibt zurück, was ins Protokoll
    /// gehört: nur beim Wechsel der Fehlerart, nicht jede Minute.
    pub fn eintragen(&mut self, befund: &SitzungsBefund, bote_verbunden: bool, jetzt: &str) -> Option<Protokoll> {
        let vorher = self.stand.fehler.clone();
        let (geht, fehler, satz) = match &befund.abfrage {
            Ok(true) => (true, None, None),
            Ok(false) if bote_verbunden => (
                true,
                Some(SIEHT_INHABER_NICHT),
                Some("Sitzungsabfrage: Der Bote ist verbunden, aber Windows nennt seine Sitzung nicht angemeldet.".to_string()),
            ),
            Ok(false) => (true, None, None),
            Err(e) => {
                let folge = match befund.quelle {
                    Quelle::Unbekannt => "Ob du angemeldet bist, bleibt offen; die Meldung „Bote fehlt“ kommt so nicht.",
                    _ => "Ersatzweise gilt der Bericht des Boten über seine Sitzung (solange sie an der Konsole ist, sonst höchstens 5 Minuten).",
                };
                (false, Some(e.art()), Some(format!("Sitzungsabfrage scheitert ({e}). {folge}")))
            }
        };
        self.stand = SitzungsStand {
            geht: Some(geht),
            inhaber_angemeldet: befund.angemeldet,
            quelle: Some(befund.quelle.wort().to_string()),
            fehler: fehler.map(String::from),
            geprueft: Some(jetzt.to_string()),
        };
        match (vorher.as_deref(), fehler) {
            (alt, Some(neu)) if alt != Some(neu) => satz.map(Protokoll::Warnung),
            (Some(_), None) => Some(Protokoll::WiederDa),
            _ => None,
        }
    }
}

/// Die Abfrage bei Windows (WTS, ohne Aufzählen).
#[derive(Debug, Clone, Copy, Default)]
pub struct Wts;

#[cfg(windows)]
impl SitzungsAbfrage for Wts {
    fn konsole(&self) -> Option<u32> {
        // SAFETY: ohne Argumente; liest nur den Zustand des Systems.
        let id = unsafe { windows_sys::Win32::System::RemoteDesktop::WTSGetActiveConsoleSessionId() };
        (id != u32::MAX).then_some(id)
    }

    fn zustand(&self, id: u32) -> Result<Option<SitzungsZustand>, SitzungsFehler> {
        use windows_sys::Win32::System::RemoteDesktop::{WTSActive, WTSConnectState, WTSDisconnected};
        let Some(roh) = sitzungs_info(id, WTSConnectState)? else { return Ok(None) };
        if roh.len() < 4 {
            return Err(SitzungsFehler::Wts(format!("WTSQuerySessionInformationW(WTSConnectState): {} Bytes", roh.len())));
        }
        let z = i32::from_ne_bytes([roh[0], roh[1], roh[2], roh[3]]);
        Ok(Some(match z {
            x if x == WTSActive => SitzungsZustand::Aktiv,
            x if x == WTSDisconnected => SitzungsZustand::Getrennt,
            _ => SitzungsZustand::Anderer,
        }))
    }

    fn konto_sid(&self, id: u32) -> Result<Option<String>, SitzungsFehler> {
        use windows_sys::Win32::System::RemoteDesktop::{WTSDomainName, WTSUserName};
        let text = |was| -> Result<String, SitzungsFehler> { Ok(sitzungs_info(id, was)?.map(|b| utf16_text(&b)).unwrap_or_default()) };
        let nutzer = text(WTSUserName)?;
        if nutzer.is_empty() {
            return Ok(None);
        }
        let domaene = text(WTSDomainName)?;
        let name = if domaene.is_empty() { nutzer } else { format!("{domaene}\\{nutzer}") };
        // Den Namen nicht in den Fehler schreiben.
        sid_von_konto(&name).map(Some).ok_or_else(|| SitzungsFehler::Wts("LookupAccountNameW: Konto der Sitzung nicht auflösbar".to_string()))
    }
}

#[cfg(not(windows))]
impl SitzungsAbfrage for Wts {
    fn konsole(&self) -> Option<u32> {
        None
    }

    fn zustand(&self, _id: u32) -> Result<Option<SitzungsZustand>, SitzungsFehler> {
        Err(SitzungsFehler::NurWindows)
    }

    fn konto_sid(&self, _id: u32) -> Result<Option<String>, SitzungsFehler> {
        Err(SitzungsFehler::NurWindows)
    }
}

/// Die Sitzung des eigenen Prozesses (für den Boten): Nummer, Konsole,
/// Zustand. `None`, wenn schon die Nummer nicht lesbar ist (oder nicht Windows).
#[cfg(windows)]
pub fn eigene_sitzung() -> Option<BoteSitzung> {
    use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
    use windows_sys::Win32::System::Threading::GetCurrentProcessId;
    let mut id = 0u32;
    // SAFETY: Ausgabezeiger gültig.
    if unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut id) } == 0 {
        return None;
    }
    Some(BoteSitzung { id, konsole: Wts.konsole() == Some(id), zustand: Wts.zustand(id).ok().flatten() })
}

#[cfg(not(windows))]
pub fn eigene_sitzung() -> Option<BoteSitzung> {
    None
}

/// Rohe Antwort von `WTSQuerySessionInformationW`; `Ok(None)`, wenn es die
/// Sitzung nicht gibt. Gemessen (unter dem Konto des Inhabers): Eine Nummer, die
/// es nicht gibt, ergibt ERROR_FILE_NOT_FOUND (2); ERROR_CTX_WINSTATION_NOT_FOUND
/// (7022) zählt vorsichtshalber ebenso.
#[cfg(windows)]
fn sitzungs_info(sitzung: u32, was: windows_sys::Win32::System::RemoteDesktop::WTS_INFO_CLASS) -> Result<Option<Vec<u8>>, SitzungsFehler> {
    use windows_sys::Win32::Foundation::{ERROR_CTX_WINSTATION_NOT_FOUND, ERROR_FILE_NOT_FOUND, SetLastError};
    use windows_sys::Win32::System::RemoteDesktop::{WTS_CURRENT_SERVER_HANDLE, WTSFreeMemory, WTSQuerySessionInformationW};
    let mut puffer: *mut u16 = std::ptr::null_mut();
    let mut bytes = 0u32;
    // SAFETY: SetLastError setzt nur den Fehlercode dieses Fadens (vorher 0:
    // so ist ein Fehler ohne eigenen Code erkennbar, kein Rest eines früheren
    // Aufrufs, vgl. 259 oben). Ausgabezeiger gültig; der Puffer wird unten
    // freigegeben.
    let ok = unsafe {
        SetLastError(0);
        WTSQuerySessionInformationW(WTS_CURRENT_SERVER_HANDLE, sitzung, was, &mut puffer, &mut bytes)
    };
    if ok == 0 || puffer.is_null() {
        let e = std::io::Error::last_os_error();
        return match e.raw_os_error() {
            Some(c) if c as u32 == ERROR_FILE_NOT_FOUND || c as u32 == ERROR_CTX_WINSTATION_NOT_FOUND => Ok(None),
            Some(0) => Err(SitzungsFehler::Wts("WTSQuerySessionInformationW: ohne Fehlercode gescheitert".to_string())),
            _ => Err(SitzungsFehler::Wts(format!("WTSQuerySessionInformationW: {e}"))),
        };
    }
    // SAFETY: `bytes` Bytes ab `puffer`, von Windows angelegt.
    let roh = unsafe { std::slice::from_raw_parts(puffer as *const u8, bytes as usize).to_vec() };
    // SAFETY: von WTSQuerySessionInformationW angelegt.
    unsafe { WTSFreeMemory(puffer as *mut _) };
    Ok(Some(roh))
}

/// Nullterminierter UTF-16-Text aus Bytes.
#[cfg(windows)]
fn utf16_text(roh: &[u8]) -> String {
    let einheiten: Vec<u16> = roh.as_chunks::<2>().0.iter().map(|c| u16::from_ne_bytes([c[0], c[1]])).take_while(|&u| u != 0).collect();
    String::from_utf16_lossy(&einheiten)
}

/// Die SID eines Kontos (`DOMÄNE\Name`) als Text; `None`, wenn es sie nicht gibt.
#[cfg(windows)]
fn sid_von_konto(name: &str) -> Option<String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows_sys::Win32::Security::{LookupAccountNameW, SID_NAME_USE};
    let breit: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut sid_laenge = 0u32;
    let mut dom_laenge = 0u32;
    let mut art: SID_NAME_USE = 0;
    // SAFETY: erste Abfrage nur nach den Größen.
    unsafe { LookupAccountNameW(std::ptr::null(), breit.as_ptr(), std::ptr::null_mut(), &mut sid_laenge, std::ptr::null_mut(), &mut dom_laenge, &mut art) };
    if sid_laenge == 0 {
        return None;
    }
    let mut sid = vec![0u64; (sid_laenge as usize).div_ceil(8)];
    let mut dom = vec![0u16; dom_laenge.max(1) as usize];
    // SAFETY: Puffer in den erfragten Größen.
    let ok = unsafe {
        LookupAccountNameW(std::ptr::null(), breit.as_ptr(), sid.as_mut_ptr() as *mut _, &mut sid_laenge, dom.as_mut_ptr(), &mut dom_laenge, &mut art)
    };
    if ok == 0 {
        return None;
    }
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: die SID liegt in `sid`; der Text wird mit LocalFree freigegeben.
    unsafe {
        if ConvertSidToStringSidW(sid.as_mut_ptr() as *mut _, &mut text) == 0 || text.is_null() {
            return None;
        }
        let mut n = 0usize;
        while *text.add(n) != 0 {
            n += 1;
        }
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(text, n));
        LocalFree(text as *mut _);
        Some(s)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::collections::HashMap;

    use chrono::{FixedOffset, TimeZone};

    use super::*;

    pub(crate) const INHABER: &str = "S-1-5-21-1-2-3-1001";

    /// Eine erfundene Windows-Seite: Konsole, Sitzungen mit Zustand und
    /// Konto, oder ein Fehler bei jeder Frage (wie 259 unter dem Dienstkonto).
    #[derive(Debug, Default, Clone)]
    pub(crate) struct FalscheAbfrage {
        pub konsole: Option<u32>,
        pub sitzungen: HashMap<u32, (SitzungsZustand, Option<String>)>,
        pub kaputt: Option<SitzungsFehler>,
        /// Nur die Frage nach dem Konto scheitert (Teilfehler).
        pub konto_kaputt: Option<SitzungsFehler>,
    }

    impl FalscheAbfrage {
        pub fn mit(mut self, id: u32, z: SitzungsZustand, sid: Option<&str>) -> Self {
            self.sitzungen.insert(id, (z, sid.map(String::from)));
            self
        }
    }

    impl SitzungsAbfrage for FalscheAbfrage {
        fn konsole(&self) -> Option<u32> {
            self.konsole
        }

        fn zustand(&self, id: u32) -> Result<Option<SitzungsZustand>, SitzungsFehler> {
            match &self.kaputt {
                Some(e) => Err(e.clone()),
                None => Ok(self.sitzungen.get(&id).map(|(z, _)| *z)),
            }
        }

        fn konto_sid(&self, id: u32) -> Result<Option<String>, SitzungsFehler> {
            match self.kaputt.as_ref().or(self.konto_kaputt.as_ref()) {
                Some(e) => Err(e.clone()),
                None => Ok(self.sitzungen.get(&id).and_then(|(_, s)| s.clone())),
            }
        }
    }

    fn zeit(tag: u32, stunde: u32) -> Zeit {
        FixedOffset::east_opt(3600).unwrap().with_ymd_and_hms(2026, 10, tag, stunde, 0, 0).unwrap()
    }

    pub(crate) fn bericht(id: u32, z: Option<SitzungsZustand>, gemeldet: Zeit) -> Bericht {
        Bericht { sitzung: Some(BoteSitzung { id, konsole: true, zustand: z }), gemeldet }
    }

    fn wts_259() -> SitzungsFehler {
        SitzungsFehler::Wts("WTSQuerySessionInformationW: Es sind keine Daten mehr verfügbar. (os error 259)".into())
    }

    /// Windows geht: Die gemeldete Sitzung zählt, solange sie aktiv oder
    /// getrennt ist; ist sie weg, nicht (auch wenn der Bericht frisch ist).
    #[test]
    fn gemeldete_sitzung_laut_windows() {
        let jetzt = zeit(30, 9);
        let b = bericht(2, Some(SitzungsZustand::Aktiv), zeit(30, 8));
        for z in [SitzungsZustand::Aktiv, SitzungsZustand::Getrennt] {
            let q = FalscheAbfrage { konsole: Some(2), ..Default::default() }.mit(2, z, Some(INHABER));
            let f = pruefen(&q, Some(&b), false, INHABER, jetzt);
            assert_eq!((f.abfrage, f.angemeldet, f.quelle), (Ok(true), Some(true), Quelle::Windows), "{z:?}");
        }
        // Abgemeldet: Sitzung 2 weg, an der Konsole der Anmeldebildschirm.
        let q = FalscheAbfrage { konsole: Some(3), ..Default::default() }.mit(3, SitzungsZustand::Anderer, None);
        let f = pruefen(&q, Some(&b), false, INHABER, jetzt);
        assert_eq!((f.abfrage, f.angemeldet, f.quelle), (Ok(false), Some(false), Quelle::Windows));
        assert!(!f_angemeldet(&q, Some(&b)));
        // Ohne Konsole, ohne Bericht: niemand.
        assert!(!f_angemeldet(&FalscheAbfrage::default(), None));
    }

    fn f_angemeldet(q: &dyn SitzungsAbfrage, b: Option<&Bericht>) -> bool {
        pruefen(q, b, false, INHABER, zeit(30, 9)).inhaber_angemeldet()
    }

    /// Der Inhaber hat sich neu angemeldet (neue Nummer), der Bote startete dort
    /// nicht: Die Konsolensitzung zählt, wenn ihr Konto das des Inhabers ist; ein
    /// anderes Konto an der Konsole nicht. Ohne SID: Fehler `sid-fehlt`.
    #[test]
    fn konsolensitzung_mit_konto() {
        let alt = bericht(2, Some(SitzungsZustand::Aktiv), zeit(29, 18));
        let q = FalscheAbfrage { konsole: Some(4), ..Default::default() }.mit(4, SitzungsZustand::Aktiv, Some(INHABER));
        assert!(f_angemeldet(&q, Some(&alt)));
        assert!(f_angemeldet(&q, None), "auch ganz ohne Bericht (Bote nie gestartet)");
        let fremd = FalscheAbfrage { konsole: Some(4), ..Default::default() }.mit(4, SitzungsZustand::Aktiv, Some("S-1-5-21-9-9-9-500"));
        assert!(!f_angemeldet(&fremd, Some(&alt)));
        let f = pruefen(&q, None, false, "", zeit(30, 9));
        assert_eq!((f.abfrage, f.quelle), (Err(SitzungsFehler::SidFehlt), Quelle::Unbekannt));
    }

    /// Windows scheitert (259): Es gilt der Bericht des Boten, aber nur, wo
    /// eine Abmeldung nicht wie ein Absturz aussehen kann. Verbunden:
    /// angemeldet. Fehlt er: an der Konsole nur bei derselben
    /// Konsolennummer, ohne Konsolenbezug höchstens 5 Minuten.
    #[test]
    fn abfrage_scheitert_bericht_des_boten() {
        let q = FalscheAbfrage { konsole: Some(2), kaputt: Some(wts_259()), ..Default::default() };
        let b = bericht(2, Some(SitzungsZustand::Aktiv), zeit(30, 8));
        let f = pruefen(&q, Some(&b), false, INHABER, zeit(30, 9));
        assert_eq!((f.abfrage.is_err(), f.angemeldet, f.quelle), (true, Some(true), Quelle::Bote));
        // An der Konsole gilt er ohne Altersgrenze, solange die Nummer bleibt.
        assert_eq!(pruefen(&q, Some(&b), false, INHABER, zeit(31, 9)).angemeldet, Some(true));
        // Getrennt gemeldet (Benutzer gewechselt) zählt ebenso; Zustand unbekannt auch.
        assert!(pruefen(&q, Some(&bericht(2, Some(SitzungsZustand::Getrennt), zeit(30, 8))), false, INHABER, zeit(30, 9)).inhaber_angemeldet());
        assert!(pruefen(&q, Some(&bericht(2, None, zeit(30, 8))), false, INHABER, zeit(30, 9)).inhaber_angemeldet());
        assert!(!pruefen(&q, Some(&bericht(2, Some(SitzungsZustand::Anderer), zeit(30, 8))), false, INHABER, zeit(30, 9)).inhaber_angemeldet());
        // Abmeldung: Die Konsole hat eine neue Nummer. Unbekannt, kein Alarm.
        let neu = FalscheAbfrage { konsole: Some(3), ..q.clone() };
        let f = pruefen(&neu, Some(&b), false, INHABER, zeit(30, 8));
        assert_eq!((f.angemeldet, f.quelle, f.inhaber_angemeldet()), (None, Quelle::Unbekannt, false));
        // Keine Konsole angeschlossen: ebenso.
        let ohne = FalscheAbfrage { konsole: None, ..q.clone() };
        assert_eq!(pruefen(&ohne, Some(&b), false, INHABER, zeit(30, 8)).quelle, Quelle::Unbekannt);
        // Ohne Konsolenbezug (Fernsitzung): höchstens 5 Minuten nach dem Hallo.
        let fern = Bericht { sitzung: Some(BoteSitzung { id: 5, konsole: false, zustand: Some(SitzungsZustand::Getrennt) }), gemeldet: zeit(30, 8) };
        let um = |minute| FixedOffset::east_opt(3600).unwrap().with_ymd_and_hms(2026, 10, 30, 8, minute, 0).unwrap();
        let f = pruefen(&q, Some(&fern), false, INHABER, um(5));
        assert_eq!((f.angemeldet, f.quelle), (Some(true), Quelle::Bote), "genau 5 Minuten: gilt noch");
        let f = pruefen(&q, Some(&fern), false, INHABER, um(6));
        assert_eq!((f.angemeldet, f.quelle), (None, Quelle::Unbekannt), "älter: unbekannt");
        // Verbunden: angemeldet, auch ein älterer Bote ohne Sitzung.
        let alt = Bericht { sitzung: None, gemeldet: zeit(30, 8) };
        let f = pruefen(&q, Some(&alt), true, INHABER, zeit(30, 9));
        assert_eq!((f.angemeldet, f.quelle), (Some(true), Quelle::Bote));
        // Älterer Bote, nicht verbunden, oder gar kein Bericht: unbekannt.
        for b in [Some(&alt), None] {
            let f = pruefen(&q, b, false, INHABER, zeit(30, 9));
            assert_eq!((f.angemeldet, f.quelle, f.inhaber_angemeldet()), (None, Quelle::Unbekannt, false));
        }
    }

    /// Teilfehler: Windows sagt, die gemeldete Sitzung ist weg, dann scheitert
    /// erst das Konto der Konsolensitzung. Das kippt nicht in den Rückfall
    /// (der Bericht sähe sonst nach angemeldet aus): unbekannt, mit Fehler.
    #[test]
    fn teilfehler_kippt_nicht_in_den_rueckfall() {
        let q = FalscheAbfrage { konsole: Some(2), konto_kaputt: Some(wts_259()), ..Default::default() }.mit(2, SitzungsZustand::Aktiv, None);
        // Gemeldet war Sitzung 1 (frisch, ohne Konsolenbezug), jetzt weg.
        let b = Bericht { sitzung: Some(BoteSitzung { id: 1, konsole: false, zustand: Some(SitzungsZustand::Aktiv) }), gemeldet: zeit(30, 9) };
        let f = pruefen(&q, Some(&b), false, INHABER, zeit(30, 9));
        assert_eq!((f.abfrage, f.angemeldet, f.quelle), (Err(wts_259()), None, Quelle::Unbekannt));
        // Ohne Bericht scheitert dasselbe Konto: Rückfall, aber ohne Bericht unbekannt.
        assert_eq!(pruefen(&q, None, false, INHABER, zeit(30, 9)).quelle, Quelle::Unbekannt);
        // Geht das Konto, sagt Windows: angemeldet (Konsole, Konto des Inhabers).
        let gut = FalscheAbfrage { konsole: Some(2), ..Default::default() }.mit(2, SitzungsZustand::Aktiv, Some(INHABER));
        assert_eq!(pruefen(&gut, Some(&b), false, INHABER, zeit(30, 9)).quelle, Quelle::Windows);
    }

    /// Der Wächter: Stand für /api/betrieb nach jeder Prüfung, mit Quelle;
    /// ins Protokoll nur beim Wechsel (einmal je Fehlerart, nicht jede
    /// Minute), und einmal, wenn es wieder geht.
    #[test]
    fn waechter_meldet_einmal_je_wechsel() {
        let befund = |abfrage: Result<bool, SitzungsFehler>, angemeldet: Option<bool>, quelle| SitzungsBefund { abfrage, angemeldet, quelle };
        let mut w = Waechter::default();
        assert_eq!(w.stand(), SitzungsStand::default(), "vor der ersten Abfrage alles null");
        assert_eq!(w.eintragen(&befund(Ok(true), Some(true), Quelle::Windows), true, "2026-10-30T08:00:00+01:00"), None);
        assert_eq!(
            w.stand(),
            SitzungsStand {
                geht: Some(true),
                inhaber_angemeldet: Some(true),
                quelle: Some("windows".into()),
                fehler: None,
                geprueft: Some("2026-10-30T08:00:00+01:00".into())
            }
        );
        // Scheitert: einmal warn, die nächsten Minuten nicht mehr.
        let wts = befund(Err(wts_259()), Some(true), Quelle::Bote);
        assert!(
            matches!(w.eintragen(&wts, false, "2026-10-30T08:01:00+01:00"), Some(Protokoll::Warnung(s)) if s.contains("259") && s.contains("Bericht des Boten"))
        );
        for minute in 2..10 {
            assert_eq!(w.eintragen(&wts, false, &format!("2026-10-30T08:0{minute}:00+01:00")), None, "Minute {minute}");
        }
        let s = w.stand();
        assert_eq!((s.geht, s.inhaber_angemeldet, s.quelle.as_deref(), s.fehler.as_deref()), (Some(false), Some(true), Some("bote"), Some("wts")));
        assert_eq!(s.geprueft.as_deref(), Some("2026-10-30T08:09:00+01:00"));
        // Andere Art: wieder einmal.
        let sid = befund(Err(SitzungsFehler::SidFehlt), None, Quelle::Unbekannt);
        assert!(matches!(w.eintragen(&sid, false, "t"), Some(Protokoll::Warnung(s)) if s.contains("kommt so nicht")));
        assert_eq!(w.eintragen(&sid, false, "t"), None);
        assert_eq!(w.stand().quelle.as_deref(), Some("unbekannt"));
        // Geht wieder: einmal info.
        assert_eq!(w.eintragen(&befund(Ok(false), Some(false), Quelle::Windows), false, "t"), Some(Protokoll::WiederDa));
        assert_eq!(w.eintragen(&befund(Ok(false), Some(false), Quelle::Windows), false, "t"), None);
        assert_eq!(w.stand().fehler, None);
        // Geht, sieht den Inhaber aber nicht, obwohl der Bote verbunden ist: einmal warn.
        assert!(matches!(w.eintragen(&befund(Ok(false), Some(false), Quelle::Windows), true, "t"), Some(Protokoll::Warnung(_))));
        assert_eq!(w.eintragen(&befund(Ok(false), Some(false), Quelle::Windows), true, "t"), None);
        let s = w.stand();
        assert_eq!((s.geht, s.inhaber_angemeldet, s.fehler.as_deref()), (Some(true), Some(false), Some(SIEHT_INHABER_NICHT)));
        assert_eq!(w.eintragen(&befund(Ok(true), Some(true), Quelle::Windows), true, "t"), Some(Protokoll::WiederDa));
    }

    /// Die echte Abfrage in der eigenen Sitzung (wer die Tests ausführt, ist
    /// angemeldet): Die eigene Sitzung ist aktiv oder getrennt und gehört dem
    /// eigenen Konto; eine Nummer, die es nicht gibt, ist `Ok(None)`.
    #[cfg(windows)]
    #[test]
    fn eigene_sitzung_ist_aktiv() {
        let eigene = eigene_sitzung().expect("eigene Sitzung");
        let z = Wts.zustand(eigene.id).unwrap();
        assert!(z.is_some_and(SitzungsZustand::angemeldet), "{z:?}");
        assert_eq!(eigene.zustand, z);
        assert_eq!(Wts.zustand(65_000).unwrap(), None, "Sitzung, die es nicht gibt");
        let sid = crate::anmeldung::sspi::eigene_sid().unwrap();
        assert_eq!(Wts.konto_sid(eigene.id).unwrap().as_deref(), Some(sid.as_str()));
        let b = Bericht { sitzung: Some(eigene), gemeldet: crate::speicher::aufgaben::jetzt() };
        let f = pruefen(&Wts, Some(&b), false, &sid, crate::speicher::aufgaben::jetzt());
        assert_eq!((f.abfrage, f.quelle), (Ok(true), Quelle::Windows));
    }
}
