//! Integrierte Windows-Anmeldung (Negotiate über SSPI). In 7a1 als Probe
//! (`/api/anmeldung/probe`), seit Etappe 7b Pflicht auf den geschützten Wegen
//! (Schleuse `sicherheit::anmeldung_pflicht`, Liste in `sicherheit::geschuetzt`;
//! `autostart-dienst`, `[anmeldung]`).
//!
//! Die Anmeldung gehört zur TCP-Verbindung (`verbindung`), nicht zu einem
//! Cookie: keine Anmelde-Cookies, kein Token in der URL. Ins Protokoll kommt
//! nur die Verbindungsnummer und ob die SID passte, nie Name, SID oder Nachweis.

#[cfg(windows)]
pub mod aushandlung;
pub mod base64;
#[cfg(windows)]
pub mod sspi;
pub mod verbindung;

use serde::Serialize;

/// Erweiterung an der Anfrage, wenn ihre Verbindung als Inhaber angemeldet ist
/// (setzt `sicherheit::anmeldung_pflicht`, auf allen Wegen). Derzeit fragt
/// kein Handler danach: Benutzer-Daten liegen auf geschützten Wegen (etwa
/// `/api/pc/benutzer`), die die Schleuse ohne Anmeldung gar nicht durchlässt.
#[derive(Debug, Clone, Copy)]
pub struct Angemeldet;

/// Erweiterung an der Anfrage (Etappe 7c): wie sie als Inhaber gilt. Setzt die
/// Schleuse neben `Angemeldet`; die Wege für Passkeys fragen danach
/// (Freigeben nur im Hauptkonto oder mit frischer Passkey-Anmeldung).
#[derive(Debug, Clone)]
pub enum Inhaber {
    /// Windows-Anmeldung mit der SID des Inhabers (7b).
    Hauptkonto,
    /// Gültiger Ausweis (JWT) nach Anmeldung mit einem registrierten Passkey.
    Passkey(PasskeySitzung),
}

/// Eine Sitzung mit Passkey: der geprüfte Ausweis und der Name des Passkeys.
#[derive(Debug, Clone)]
pub struct PasskeySitzung {
    pub ausweis: crate::passkey::jwt::Ausweis,
    pub name: String,
}

/// Erweiterung an der Anfrage (Etappe 7c): das Windows-Konto der Verbindung
/// nach Negotiate, auch wenn es nicht der Inhaber ist. Nur die Anträge auf einen
/// Passkey brauchen es (der echte Kontoname statt eines eingetippten).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsKonto {
    pub sid: String,
    /// `PC\Name` (sonst die SID, wenn sich kein Name finden ließ).
    pub name: String,
    pub inhaber: bool,
}

impl WindowsKonto {
    /// Für Tests: der Inhaber (eigene SID, falls lesbar) oder ein fremdes Konto.
    #[cfg(test)]
    pub fn test(inhaber: bool) -> Self {
        if inhaber {
            #[cfg(windows)]
            let sid = crate::anmeldung::aushandlung::erwartete_sid(None).unwrap_or_default();
            #[cfg(not(windows))]
            let sid = String::from("S-1-5-21-1-2-3-1000");
            WindowsKonto { sid, name: r"PC\inhaber".into(), inhaber: true }
        } else {
            WindowsKonto { sid: "S-1-5-21-1111111111-2222222222-3333333333-1001".into(), name: r"PC\fremd".into(), inhaber: false }
        }
    }
}

/// Über welches Verfahren die Aushandlung lief.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mechanismus {
    Ntlm,
    Kerberos,
    Unbekannt,
}

impl Mechanismus {
    /// Aus dem Paketnamen, den SSPI nach der Aushandlung nennt.
    pub fn aus_paket(name: &str) -> Self {
        if name.eq_ignore_ascii_case("NTLM") {
            Mechanismus::Ntlm
        } else if name.eq_ignore_ascii_case("Kerberos") {
            Mechanismus::Kerberos
        } else {
            Mechanismus::Unbekannt
        }
    }
}

/// Ist die SID (als Text, `S-1-5-21-…`) die erwartete? Leere Angaben passen nie.
pub fn ist_inhaber(sid_text: &str, erwartet: &str) -> bool {
    let (sid, erwartet) = (sid_text.trim(), erwartet.trim());
    !sid.is_empty() && !erwartet.is_empty() && sid.eq_ignore_ascii_case(erwartet)
}

/// Beginnt der Nachweis eine neue Aushandlung (`true`) oder setzt er eine
/// offene fort (`false`)? SPNEGO: `0x60` ist der erste Nachweis (NegTokenInit,
/// auch roher Kerberos), `0xa1` eine Antwort (NegTokenResp). Rohes NTLM:
/// Typ 1 (NEGOTIATE_MESSAGE) beginnt, Typ 3 (AUTHENTICATE_MESSAGE) setzt fort.
/// Unbekanntes gilt als Beginn.
pub fn beginnt_neu(nachweis: &[u8]) -> bool {
    if nachweis.starts_with(b"NTLMSSP\0") {
        return nachweis.get(8) != Some(&3);
    }
    nachweis.first() != Some(&0xa1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nur_genau_die_erwartete_sid_ist_inhaber() {
        let inhaber = "S-1-5-21-1111111111-2222222222-3333333333-1001";
        assert!(ist_inhaber(inhaber, inhaber));
        assert!(ist_inhaber(&format!(" {} ", inhaber.to_lowercase()), inhaber));
        assert!(!ist_inhaber("S-1-5-21-1111111111-2222222222-3333333333-1002", inhaber), "fremdes Konto");
        assert!(!ist_inhaber("S-1-5-18", inhaber), "SYSTEM");
        assert!(!ist_inhaber("", inhaber));
        assert!(!ist_inhaber(inhaber, ""), "leere Erwartung passt nie");
        assert!(!ist_inhaber(&format!("{inhaber}0"), inhaber), "kein Präfix-Vergleich");
    }

    #[test]
    fn mechanismus_aus_dem_paketnamen() {
        assert_eq!(Mechanismus::aus_paket("NTLM"), Mechanismus::Ntlm);
        assert_eq!(Mechanismus::aus_paket("Kerberos"), Mechanismus::Kerberos);
        assert_eq!(Mechanismus::aus_paket("CloudAP"), Mechanismus::Unbekannt);
        assert_eq!(serde_json::to_string(&Mechanismus::Ntlm).unwrap(), "\"ntlm\"");
    }

    #[test]
    fn beginn_oder_fortsetzung() {
        assert!(beginnt_neu(b"NTLMSSP\0\x01\0\0\0"));
        assert!(!beginnt_neu(b"NTLMSSP\0\x03\0\0\0"));
        assert!(beginnt_neu(&[0x60, 0x28]));
        assert!(!beginnt_neu(&[0xa1, 0x28]));
    }
}
