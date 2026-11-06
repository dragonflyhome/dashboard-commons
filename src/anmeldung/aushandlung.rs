//! Ein Schritt der Negotiate-Aushandlung für eine Verbindung, gemeinsam für
//! die Schleuse `sicherheit::anmeldung_pflicht` und die Probe
//! `/api/anmeldung/probe`. Die offene Aushandlung liegt im Vorrat je
//! Verbindung (`verbindung`); ist sie fertig, merkt sich der Vorrat, ob es
//! der Inhaber war.
//!
//! Ins Protokoll kommt nur die Verbindungsnummer und ob die SID passte, nie
//! Name, SID oder Nachweis.

use std::sync::OnceLock;

use super::sspi::{self, Kontext, Schritt};
use super::{Mechanismus, WindowsKonto, beginnt_neu, ist_inhaber, verbindung};
use crate::dienst::einstellungen::Einstellungen;

/// Ergebnis eines Schritts.
pub enum Ausgang {
    /// Es braucht noch eine Runde: dieser Nachweis geht mit 401 zurück.
    Weiter(Vec<u8>),
    /// Fertig; `inhaber` sagt, ob die SID die des Inhabers war. `ausgabe` ist ein
    /// letzter Nachweis für den Client (kann leer sein).
    /// `konto`: SID und Name (Etappe 7c, für die Anträge auf einen Passkey).
    Fertig { ausgabe: Vec<u8>, inhaber: bool, mechanismus: Mechanismus, konto: WindowsKonto },
    /// Eine Fortsetzung, aber auf dieser Verbindung ist keine Aushandlung offen
    /// (verfallen, oder die Verbindung ist eine andere): neu beginnen.
    OhneKontext,
    /// SSPI hat den Nachweis nicht angenommen (technischer Satz fürs Protokoll).
    Abgewiesen(String),
    /// SSPI selbst geht gerade nicht.
    Intern(String),
}

/// Die SID, gegen die verglichen wird: `konten.inhaber_sid` aus den
/// Einstellungen; fehlt sie (Altweg, Tests), die des eigenen Prozesses.
pub fn erwartete_sid(einstellungen: Option<&Einstellungen>) -> Result<String, String> {
    if let Some(sid) = einstellungen.map(|e| e.konten.inhaber_sid.trim()).filter(|s| !s.is_empty()) {
        return Ok(sid.to_string());
    }
    static EIGENE: OnceLock<Result<String, String>> = OnceLock::new();
    EIGENE.get_or_init(sspi::eigene_sid).clone()
}

/// Vergleicht die SID mit der erwarteten und merkt das Ergebnis samt Konto
/// für die Verbindung `nr` (ohne Nummer: nur der Vergleich).
pub fn urteil(nr: Option<u64>, sid: &str, name: &str, erwartet: &str) -> bool {
    let passt = ist_inhaber(sid, erwartet);
    if let Some(nr) = nr {
        verbindung::merken(nr, WindowsKonto { sid: sid.to_string(), name: name.to_string(), inhaber: passt });
    }
    let nr = nr.map_or_else(|| "ohne Nummer".to_string(), |n| n.to_string());
    if passt {
        tracing::info!("Anmeldung, Verbindung {nr}: SID passt");
    } else {
        tracing::warn!("Anmeldung, Verbindung {nr}: SID passt nicht");
    }
    passt
}

/// Ein Schritt für die Verbindung `nr` (blockierend). Ohne Nummer geht nur ein
/// Verfahren mit einer Runde (Kerberos); NTLM kann dann nicht fortsetzen.
pub fn schritt(nr: Option<u64>, nachweis: &[u8], erwartet: &str) -> Ausgang {
    let kontext = if beginnt_neu(nachweis) {
        match Kontext::server() {
            Ok(k) => Some(k),
            Err(e) => return Ausgang::Intern(e),
        }
    } else {
        nr.and_then(verbindung::offen_nehmen)
    };
    let Some(mut kontext) = kontext else { return Ausgang::OhneKontext };
    match kontext.annehmen(nachweis) {
        Ok(Schritt::Weiter(antwort)) => {
            if let Some(nr) = nr {
                verbindung::offen_ablegen(nr, kontext);
            }
            Ausgang::Weiter(antwort)
        }
        Ok(Schritt::Fertig { ausgabe, sid, konto, mechanismus }) => {
            let inhaber = urteil(nr, &sid, &konto, erwartet);
            Ausgang::Fertig { ausgabe, inhaber, mechanismus, konto: WindowsKonto { sid, name: konto, inhaber } }
        }
        Err(e) => Ausgang::Abgewiesen(e),
    }
}

/// `schritt` in einem Faden für blockierende Arbeit.
pub async fn schritt_async(nr: Option<u64>, nachweis: Vec<u8>, erwartet: String) -> Ausgang {
    match tokio::task::spawn_blocking(move || schritt(nr, &nachweis, &erwartet)).await {
        Ok(a) => a,
        Err(e) => Ausgang::Intern(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anmeldung::verbindung::{Stand, Verbindung, stand};

    #[test]
    fn fremde_sid_wird_als_fremd_gemerkt() {
        let eigene = erwartete_sid(None).unwrap();
        let nr = Verbindung::neu().nr;
        assert!(!urteil(Some(nr), "S-1-5-21-1111111111-2222222222-3333333333-1001", r"PC\fremd", &eigene));
        assert_eq!(stand(nr), Stand::Fremd);
        let nr = Verbindung::neu().nr;
        assert!(urteil(Some(nr), &eigene, r"PC\inhaber", &eigene));
        assert_eq!(stand(nr), Stand::Inhaber);
    }

    #[test]
    fn fortsetzung_ohne_offene_aushandlung() {
        let nr = Verbindung::neu().nr;
        assert!(matches!(schritt(Some(nr), b"NTLMSSP\0\x03\0\0\0", "S-1-5-18"), Ausgang::OhneKontext));
        assert!(matches!(schritt(None, &[0xa1, 0x00], "S-1-5-18"), Ausgang::OhneKontext));
    }
}
