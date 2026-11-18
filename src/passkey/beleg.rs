//! Schritt 7c der macOS-Folge: der Beleg der zwei Adressen, reine Logik.
//!
//! Bevor der Befehl `dashboard einmal-code` einen Code holt, belegt er, dass
//! unter beiden Adressen für `localhost`, 127.0.0.1 und [::1], dieser Server
//! antwortet. Die Frage kommt vom Server über den lokalen Kanal (`beleg`) und
//! geht über TCP zurück (`POST /api/passkey/code/beleg`). So kann niemand eine
//! Frage vorher einspielen, und ein fremdes Programm an einer der Adressen
//! sähe sie zwar, könnte aber ihre Ankunft hier nicht bezeugen.
//!
//! - Die Frage hat 128 Bit aus `passkey::zufall`, gezeigt als 32 Zeichen hex.
//! - Im Arbeitsspeicher liegt nur ihr SHA-256.
//! - Sie gilt 30 Sekunden, gemessen mit zwei Uhren wie der Einmal-Code.
//! - Es gilt immer nur eine: Eine neue ersetzt die alte.
//! - Die Ankunft zählt je Familie der Gegenstelle: v4 für 127.0.0.1, v6 für
//!   [::1]. `binden` öffnet die zwei Lauscher getrennt, eine v6-Gegenstelle
//!   kommt also nur über den Lauscher an [::1].
//! - Der Kanalbefehl `code` löst sie ein: Er verbraucht sie in jedem Fall und
//!   gibt nur dann einen Code, wenn sie auf beiden Adressen ankam.
//!
//! Die Zeiten werden eingespeist; die Tests laufen auf jeder Plattform.

use std::net::IpAddr;
use std::time::Duration;

use sha2::{Digest, Sha256};

use super::einmalcode::Uhren;

/// So lange gilt eine Frage nach der Ausgabe.
pub const GUELTIG: Duration = Duration::from_secs(30);

/// So viele Bytes Zufall hat eine Frage (128 Bit).
pub const FRAGE_BYTES: usize = 16;

/// Die Familie der Gegenstelle, über die eine Frage ankam.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Familie {
    /// 127.0.0.1 (auch als IPv4 in IPv6 geschrieben).
    V4,
    /// [::1].
    V6,
}

/// Die Familie einer Gegenstelle, nach `to_canonical()`. Ohne bekannte
/// Gegenstelle keine: Dann zählt die Ankunft nicht.
pub fn familie(gegenstelle: Option<IpAddr>) -> Option<Familie> {
    match gegenstelle?.to_canonical() {
        IpAddr::V4(_) => Some(Familie::V4),
        IpAddr::V6(_) => Some(Familie::V6),
    }
}

/// Die gezeigte Form: Kleinbuchstaben hex, zwei Zeichen je Byte.
pub fn darstellen(zufall: &[u8]) -> String {
    zufall.iter().map(|b| format!("{b:02x}")).collect()
}

/// Hat `text` die Form einer Frage (32 Zeichen hex, klein)?
pub fn form_gilt(text: &str) -> bool {
    text.len() == FRAGE_BYTES * 2 && text.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn hash(frage: &str) -> [u8; 32] {
    Sha256::digest(frage.as_bytes()).into()
}

struct Offen {
    hash: [u8; 32],
    seit: Uhren,
    v4: bool,
    v6: bool,
}

impl Offen {
    fn abgelaufen(&self, jetzt: Uhren) -> bool {
        let stetig = jetzt.stetig.saturating_duration_since(self.seit.stetig) >= GUELTIG;
        // Steht die Wanduhr vor der Ausgabe, gilt die Frage als abgelaufen.
        let wand = jetzt.wand.duration_since(self.seit.wand).map_or(true, |d| d >= GUELTIG);
        stetig || wand
    }

    fn passt(&self, frage: &str) -> bool {
        super::einmalcode::gleich(&hash(frage), &self.hash)
    }
}

/// Der Stand: höchstens eine offene Frage.
#[derive(Default)]
pub struct BelegStand {
    offen: Option<Offen>,
}

impl std::fmt::Debug for BelegStand {
    /// Nie den Hash zeigen, nur ob eine Frage offen ist.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BelegStand").field("offen", &self.offen.is_some()).finish()
    }
}

impl BelegStand {
    /// Gibt eine neue Frage aus `zufall` aus ([`FRAGE_BYTES`] Bytes) und merkt
    /// nur ihren Hash. Eine schon offene ist damit ersetzt. Gibt die gezeigte Form zurück.
    pub fn ausgeben(&mut self, zufall: &[u8], jetzt: Uhren) -> String {
        let frage = darstellen(zufall);
        self.offen = Some(Offen { hash: hash(&frage), seit: jetzt, v4: false, v6: false });
        frage
    }

    /// Vermerkt, dass `frage` über eine Gegenstelle der Familie `familie`
    /// ankam. Eine unbekannte oder abgelaufene Frage ändert nichts; nach
    /// außen ist das nicht zu unterscheiden.
    pub fn ankunft(&mut self, frage: &str, familie: Familie, jetzt: Uhren) {
        let Some(offen) = self.offen.as_mut() else { return };
        if offen.abgelaufen(jetzt) || !offen.passt(frage) {
            return;
        }
        match familie {
            Familie::V4 => offen.v4 = true,
            Familie::V6 => offen.v6 = true,
        }
    }

    /// Löst die offene Frage ein: Sie ist danach verbraucht, was immer
    /// kommt. Wahr nur, wenn `frage` die offene ist, nicht abgelaufen, und
    /// auf beiden Adressen ankam. Ohne `frage` nie.
    pub fn einloesen(&mut self, frage: Option<&str>, jetzt: Uhren) -> bool {
        let Some(offen) = self.offen.take() else { return false };
        let Some(frage) = frage else { return false };
        offen.passt(frage) && !offen.abgelaufen(jetzt) && offen.v4 && offen.v6
    }

    /// Ob gerade eine Frage offen ist (für Tests; sagt nichts über ihren Wert).
    pub fn offen(&self) -> bool {
        self.offen.is_some()
    }
}

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr};

    use super::*;

    fn start() -> Uhren {
        Uhren::jetzt()
    }

    fn nach(u: Uhren, d: Duration) -> Uhren {
        Uhren { stetig: u.stetig + d, wand: u.wand + d }
    }

    fn zufall(saat: u8) -> Vec<u8> {
        (0..FRAGE_BYTES as u8).map(|i| i.wrapping_mul(29).wrapping_add(saat)).collect()
    }

    /// Eine ausgegebene Frage, die auf beiden Adressen ankam.
    fn belegt(stand: &mut BelegStand, t: Uhren) -> String {
        let frage = stand.ausgeben(&zufall(1), t);
        stand.ankunft(&frage, Familie::V4, t);
        stand.ankunft(&frage, Familie::V6, t);
        frage
    }

    #[test]
    fn frage_hat_32_zeichen_hex() {
        let frage = darstellen(&zufall(3));
        assert_eq!(frage.len(), 32);
        assert!(form_gilt(&frage), "{frage}");
        assert_eq!(darstellen(&[0, 15, 16, 255]), "000f10ff");
        for falsch in ["", "abc", &"g".repeat(32), &"A".repeat(32), &"a".repeat(33)] {
            assert!(!form_gilt(falsch), "{falsch}");
        }
    }

    #[test]
    fn familie_nach_der_gegenstelle() {
        assert_eq!(familie(Some(IpAddr::V4(Ipv4Addr::LOCALHOST))), Some(Familie::V4));
        assert_eq!(familie(Some(IpAddr::V6(Ipv6Addr::LOCALHOST))), Some(Familie::V6));
        // IPv4 in IPv6 geschrieben zählt als v4.
        assert_eq!(familie(Some("::ffff:127.0.0.1".parse().unwrap())), Some(Familie::V4));
        assert_eq!(familie(None), None);
    }

    #[test]
    fn auf_beiden_adressen_angekommen_gilt_der_beleg_genau_einmal() {
        let t = start();
        let mut stand = BelegStand::default();
        let frage = belegt(&mut stand, t);
        assert!(stand.einloesen(Some(&frage), nach(t, Duration::from_secs(1))));
        assert!(!stand.offen());
        // Schon verbraucht.
        assert!(!stand.einloesen(Some(&frage), nach(t, Duration::from_secs(2))));
    }

    #[test]
    fn nur_ueber_eine_adresse_auch_zweimal_kein_beleg() {
        let t = start();
        for familie in [Familie::V4, Familie::V6] {
            let mut stand = BelegStand::default();
            let frage = stand.ausgeben(&zufall(2), t);
            stand.ankunft(&frage, familie, t);
            stand.ankunft(&frage, familie, t);
            assert!(!stand.einloesen(Some(&frage), t), "{familie:?}");
        }
    }

    #[test]
    fn ohne_falsche_oder_abgelaufene_frage_kein_beleg_und_die_offene_ist_verbraucht() {
        let t = start();
        // Ohne Frage.
        let mut stand = BelegStand::default();
        let frage = belegt(&mut stand, t);
        assert!(!stand.einloesen(None, t));
        assert!(!stand.offen());
        assert!(!stand.einloesen(Some(&frage), t), "die offene war mit verbraucht");
        // Mit falscher Frage.
        let frage = belegt(&mut stand, t);
        assert!(!stand.einloesen(Some(&darstellen(&zufall(9))), t));
        assert!(!stand.einloesen(Some(&frage), t));
        // Abgelaufen, auf der stetigen Uhr und auf der Wanduhr.
        let frage = belegt(&mut stand, t);
        assert!(!stand.einloesen(Some(&frage), nach(t, GUELTIG)));
        let frage = belegt(&mut stand, t);
        let ruhe = Uhren { stetig: t.stetig + Duration::from_secs(1), wand: t.wand + GUELTIG + Duration::from_secs(1) };
        assert!(!stand.einloesen(Some(&frage), ruhe));
        let frage = belegt(&mut stand, t);
        let zurueck = Uhren { stetig: t.stetig + Duration::from_secs(1), wand: t.wand - Duration::from_secs(1) };
        assert!(!stand.einloesen(Some(&frage), zurueck));
        // Kurz vor Ablauf gilt er noch.
        let frage = belegt(&mut stand, t);
        assert!(stand.einloesen(Some(&frage), nach(t, GUELTIG - Duration::from_secs(1))));
    }

    #[test]
    fn eine_unbekannte_oder_abgelaufene_ankunft_zaehlt_nicht() {
        let t = start();
        let mut stand = BelegStand::default();
        let frage = stand.ausgeben(&zufall(4), t);
        // Eine andere Frage über v6 zählt nicht für die offene.
        stand.ankunft(&darstellen(&zufall(5)), Familie::V6, t);
        stand.ankunft(&frage, Familie::V4, t);
        assert!(!stand.einloesen(Some(&frage), t));
        // Die Ankunft nach Ablauf zählt nicht, auch wenn die erste rechtzeitig war.
        let frage = stand.ausgeben(&zufall(4), t);
        stand.ankunft(&frage, Familie::V4, t);
        stand.ankunft(&frage, Familie::V6, nach(t, GUELTIG));
        assert!(!stand.einloesen(Some(&frage), t));
        // Ohne offene Frage ändert eine Ankunft nichts.
        stand.ankunft(&frage, Familie::V4, t);
        assert!(!stand.offen());
    }

    #[test]
    fn eine_neue_frage_ersetzt_die_alte() {
        let t = start();
        let mut stand = BelegStand::default();
        let alt = belegt(&mut stand, t);
        let neu = stand.ausgeben(&zufall(6), t);
        assert_ne!(alt, neu);
        // Die Ankünfte der alten gelten nicht für die neue.
        assert!(!stand.einloesen(Some(&neu), t));
        let neu = stand.ausgeben(&zufall(6), t);
        stand.ankunft(&alt, Familie::V4, t);
        stand.ankunft(&alt, Familie::V6, t);
        assert!(!stand.einloesen(Some(&alt), t));
        assert!(!stand.einloesen(Some(&neu), t));
    }

    #[test]
    fn die_ausgabe_zeigt_den_hash_nicht() {
        let mut stand = BelegStand::default();
        let frage = stand.ausgeben(&zufall(7), start());
        let gezeigt = format!("{stand:?}");
        assert_eq!(gezeigt, "BelegStand { offen: true }");
        assert!(!gezeigt.contains(&frage));
    }
}
