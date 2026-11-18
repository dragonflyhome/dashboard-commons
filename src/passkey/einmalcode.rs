//! Schritt 7b der macOS-Folge: der Einmal-Code, reine Logik.
//!
//! Außerhalb von Windows ersetzt er beim Anlegen eines Passkeys die
//! Aushandlung des Hauptkontos. Ausgegeben wird er nur über den lokalen Kanal
//! (`crate::kanal`), eingelöst nur im Körper eines POST. Er meldet niemanden an.
//!
//! - 26 Zeichen Base32 nach Crockford (0–9 und A–Z ohne I, L, O, U), je
//!   Zeichen 5 Bit Zufall, also 130 Bit; gezeigt in Gruppen zu vier.
//! - Im Arbeitsspeicher liegt nur der SHA-256 der normierten Form.
//! - Er gilt 10 Minuten, gemessen mit zwei Uhren: abgelaufen, sobald die
//!   stetige Uhr **oder** die Wanduhr 10 Minuten zeigt, oder wenn die Wanduhr
//!   vor der Ausgabe steht. Die stetige Uhr steht im Ruhezustand still, die
//!   Wanduhr lässt sich zurückstellen; zusammen halten sie die Grenze.
//! - Genau einmal: Der Vergleich entfernt ihn im selben Schritt.
//! - Nach 5 Eingaben, die nicht passen, ist er verfallen.
//! - Es gilt immer nur einer: Eine neue Ausgabe ersetzt den alten.
//! - Kein Orakel: Auch ohne offenen Code wird normiert, gehasht und in
//!   konstanter Zeit verglichen; das Urteil nach außen ist dasselbe.
//!
//! Die Zeiten werden eingespeist; die Tests laufen auf jeder Plattform.

use std::time::{Duration, Instant, SystemTime};

use sha2::{Digest, Sha256};

/// So lange gilt ein Code nach der Ausgabe.
pub const GUELTIG: Duration = Duration::from_secs(600);

/// Nach so vielen Eingaben, die nicht passen, ist der Code verfallen.
pub const FEHLVERSUCHE: u32 = 5;

/// So viele Zeichen hat ein Code (ohne Bindestriche), so viele Bytes Zufall braucht die Ausgabe.
pub const ZEICHEN: usize = 26;

/// Die eine Antwort für falsch, abgelaufen, verbraucht, verfallen und „kein Code offen“.
pub const SATZ_GILT_NICHT: &str = "Dieser Einmal-Code gilt nicht (mehr). Bitte einen neuen holen.";

/// Base32 nach Crockford: ohne I, L, O und U.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Vergleichswert, wenn kein Code offen ist: der Hash von nichts, das je
/// ausgegeben wird (die normierte Form hat immer 26 Zeichen aus dem Alphabet).
const KEIN_CODE: &str = "kein-code-offen";

/// Die zwei Uhren eines Augenblicks.
#[derive(Debug, Clone, Copy)]
pub struct Uhren {
    /// Die stetige Uhr; sie springt nie, steht aber im Ruhezustand.
    pub stetig: Instant,
    /// Die Wanduhr; sie läuft im Ruhezustand weiter, lässt sich aber stellen.
    pub wand: SystemTime,
}

impl Uhren {
    /// Jetzt.
    pub fn jetzt() -> Self {
        Self { stetig: Instant::now(), wand: SystemTime::now() }
    }
}

/// Die gezeigte Form: aus jedem Byte die unteren 5 Bit, in Gruppen zu vier mit Bindestrich.
pub fn darstellen(zufall: &[u8]) -> String {
    let mut text = String::with_capacity(zufall.len() + zufall.len() / 4);
    for (i, b) in zufall.iter().enumerate() {
        if i > 0 && i % 4 == 0 {
            text.push('-');
        }
        text.push(ALPHABET[usize::from(b & 31)] as char);
    }
    text
}

/// Die normierte Form einer Eingabe: groß, ohne Leerzeichen und Bindestriche,
/// O als 0, I und L als 1.
pub fn normieren(eingabe: &str) -> String {
    eingabe
        .chars()
        .filter(|z| !z.is_whitespace() && *z != '-')
        .map(|z| match z.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            gross => gross,
        })
        .collect()
}

fn hash(normiert: &str) -> [u8; 32] {
    Sha256::digest(normiert.as_bytes()).into()
}

/// Vergleich ohne frühes Ende: ODER über das XOR aller Bytes.
fn gleich(a: &[u8; 32], b: &[u8; 32]) -> bool {
    let mut unterschied = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        unterschied |= std::hint::black_box(x ^ y);
    }
    std::hint::black_box(unterschied) == 0
}

struct Offen {
    hash: [u8; 32],
    seit: Uhren,
    fehlversuche: u32,
}

impl Offen {
    fn abgelaufen(&self, jetzt: Uhren) -> bool {
        // Die stetige Uhr geht nie rückwärts; `saturating` nur zur Sicherheit.
        let stetig = jetzt.stetig.saturating_duration_since(self.seit.stetig) >= GUELTIG;
        // Steht die Wanduhr vor der Ausgabe, gilt der Code als abgelaufen.
        let wand = jetzt.wand.duration_since(self.seit.wand).map_or(true, |d| d >= GUELTIG);
        stetig || wand
    }
}

/// Was eine Eingabe ergibt. Nach außen sind `GiltNicht` und `Verfallen` dieselbe Antwort.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Urteil {
    /// Der Code passt, ist nicht abgelaufen und damit verbraucht.
    Gilt,
    /// Falsch, abgelaufen, schon verbraucht oder kein Code offen.
    GiltNicht,
    /// Diese Eingabe war der letzte Fehlversuch: Der offene Code ist jetzt verfallen.
    Verfallen,
}

/// Der Stand: höchstens ein offener Code.
#[derive(Default)]
pub struct CodeStand {
    offen: Option<Offen>,
}

impl std::fmt::Debug for CodeStand {
    /// Nie den Hash zeigen, nur ob ein Code offen ist.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodeStand").field("offen", &self.offen.is_some()).finish()
    }
}

impl CodeStand {
    /// Gibt einen neuen Code aus `zufall` aus ([`ZEICHEN`] Bytes) und merkt
    /// nur seinen Hash. Ein schon offener Code ist damit ersetzt, der Zähler
    /// der Fehlversuche beginnt neu. Gibt die gezeigte Form zurück.
    pub fn ausgeben(&mut self, zufall: &[u8], jetzt: Uhren) -> String {
        let gezeigt = darstellen(zufall);
        self.offen = Some(Offen { hash: hash(&normieren(&gezeigt)), seit: jetzt, fehlversuche: 0 });
        gezeigt
    }

    /// Prüft eine Eingabe und verbraucht den Code, wenn sie passt. Gerechnet
    /// wird immer dasselbe: normieren, hashen, vergleichen, auch ohne offenen
    /// Code. Jede Eingabe, die nicht passt, zählt als Fehlversuch.
    pub fn einloesen(&mut self, eingabe: &str, jetzt: Uhren) -> Urteil {
        let eingabe = hash(&normieren(eingabe));
        let kein = hash(KEIN_CODE);
        let passt = gleich(&eingabe, self.offen.as_ref().map_or(&kein, |o| &o.hash));
        let Some(offen) = self.offen.as_mut() else {
            return Urteil::GiltNicht;
        };
        if offen.abgelaufen(jetzt) {
            self.offen = None;
            return Urteil::GiltNicht;
        }
        if passt {
            self.offen = None;
            return Urteil::Gilt;
        }
        offen.fehlversuche += 1;
        if offen.fehlversuche >= FEHLVERSUCHE {
            self.offen = None;
            return Urteil::Verfallen;
        }
        Urteil::GiltNicht
    }

    /// Ob gerade ein Code offen ist (für Tests und Zustandsanzeigen; sagt nichts über seinen Wert).
    pub fn offen(&self) -> bool {
        self.offen.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start() -> Uhren {
        Uhren::jetzt()
    }

    /// Dieselben Uhren, beide um `d` weiter.
    fn nach(u: Uhren, d: Duration) -> Uhren {
        Uhren { stetig: u.stetig + d, wand: u.wand + d }
    }

    fn zufall(saat: u8) -> Vec<u8> {
        (0..ZEICHEN as u8).map(|i| i.wrapping_mul(37).wrapping_add(saat)).collect()
    }

    #[test]
    fn darstellung_26_zeichen_aus_dem_alphabet_in_gruppen_zu_vier() {
        let code = darstellen(&zufall(7));
        let gruppen: Vec<&str> = code.split('-').collect();
        assert_eq!(gruppen.len(), 7);
        assert!(gruppen[..6].iter().all(|g| g.len() == 4) && gruppen[6].len() == 2, "{code}");
        let zeichen: String = gruppen.concat();
        assert_eq!(zeichen.len(), ZEICHEN);
        assert!(zeichen.bytes().all(|b| ALPHABET.contains(&b)), "{code}");
        for verboten in ['I', 'L', 'O', 'U'] {
            assert!(!ALPHABET.contains(&(verboten as u8)));
        }
    }

    #[test]
    fn jedes_byte_gibt_nur_seine_unteren_fuenf_bit() {
        // Gleichverteilt: 256 Werte fallen zu je 8 auf die 32 Zeichen.
        let mut zahl = [0u32; 32];
        for b in 0..=255u8 {
            let z = darstellen(&[b]).bytes().next().unwrap();
            zahl[ALPHABET.iter().position(|a| *a == z).unwrap()] += 1;
        }
        assert!(zahl.iter().all(|n| *n == 8), "{zahl:?}");
        assert_eq!(darstellen(&[0, 31, 32, 255]), "0Z0Z");
    }

    #[test]
    fn eingabe_wird_normiert() {
        assert_eq!(normieren("ab12-cd34"), "AB12CD34");
        assert_eq!(normieren("  ab 12\tcd-34 \n"), "AB12CD34");
        // O wie 0, I und L wie 1, groß und klein.
        assert_eq!(normieren("oO-iI-lL"), "001111");
    }

    #[test]
    fn der_ausgegebene_code_gilt_genau_einmal() {
        let mut stand = CodeStand::default();
        let t = start();
        let code = stand.ausgeben(&zufall(1), t);
        assert!(stand.offen());
        assert_eq!(stand.einloesen(&code, nach(t, Duration::from_secs(5))), Urteil::Gilt);
        assert!(!stand.offen());
        // Schon verbraucht.
        assert_eq!(stand.einloesen(&code, nach(t, Duration::from_secs(6))), Urteil::GiltNicht);
    }

    #[test]
    fn eingabe_in_anderer_schreibung_gilt() {
        let mut stand = CodeStand::default();
        let t = start();
        let code = stand.ausgeben(&zufall(2), t);
        let anders = code.to_lowercase().replace('-', " ");
        assert_ne!(anders, code);
        assert_eq!(stand.einloesen(&anders, t), Urteil::Gilt);
    }

    #[test]
    fn ohne_offenen_code_gilt_nichts() {
        let mut stand = CodeStand::default();
        assert_eq!(stand.einloesen(&darstellen(&zufall(3)), start()), Urteil::GiltNicht);
        assert_eq!(stand.einloesen("", start()), Urteil::GiltNicht);
        // Auch nicht der Vergleichswert selbst.
        assert_eq!(stand.einloesen(KEIN_CODE, start()), Urteil::GiltNicht);
    }

    #[test]
    fn abgelaufen_nach_zehn_minuten_auf_der_stetigen_uhr() {
        let t = start();
        // Kurz vor Ablauf gilt er noch.
        let mut stand = CodeStand::default();
        let code = stand.ausgeben(&zufall(4), t);
        assert_eq!(stand.einloesen(&code, nach(t, GUELTIG - Duration::from_secs(1))), Urteil::Gilt);
        // 10 Minuten und 1 Sekunde: abgelaufen, und damit weg.
        let code = stand.ausgeben(&zufall(4), t);
        assert_eq!(stand.einloesen(&code, nach(t, GUELTIG + Duration::from_secs(1))), Urteil::GiltNicht);
        assert!(!stand.offen());
        // Genau 10 Minuten: abgelaufen.
        let code = stand.ausgeben(&zufall(4), t);
        assert_eq!(stand.einloesen(&code, nach(t, GUELTIG)), Urteil::GiltNicht);
    }

    /// Ruhezustand: Die stetige Uhr stand, die Wanduhr lief weiter.
    #[test]
    fn abgelaufen_wenn_nur_die_wanduhr_elf_minuten_zeigt() {
        let t = start();
        let mut stand = CodeStand::default();
        let code = stand.ausgeben(&zufall(5), t);
        let spaeter = Uhren { stetig: t.stetig + Duration::from_secs(1), wand: t.wand + Duration::from_secs(11 * 60) };
        assert_eq!(stand.einloesen(&code, spaeter), Urteil::GiltNicht);
        assert!(!stand.offen());
    }

    /// Die Wanduhr wurde zurückgestellt.
    #[test]
    fn abgelaufen_wenn_die_wanduhr_vor_der_ausgabe_steht() {
        let t = start();
        let mut stand = CodeStand::default();
        let code = stand.ausgeben(&zufall(6), t);
        let zurueck = Uhren { stetig: t.stetig + Duration::from_secs(1), wand: t.wand - Duration::from_secs(1) };
        assert_eq!(stand.einloesen(&code, zurueck), Urteil::GiltNicht);
        assert!(!stand.offen());
    }

    #[test]
    fn nach_fuenf_fehlversuchen_verfallen_auch_der_richtige_wird_abgewiesen() {
        let t = start();
        let mut stand = CodeStand::default();
        let code = stand.ausgeben(&zufall(7), t);
        let falsch = darstellen(&zufall(8));
        for _ in 0..FEHLVERSUCHE - 1 {
            assert_eq!(stand.einloesen(&falsch, t), Urteil::GiltNicht);
            assert!(stand.offen());
        }
        // Der fünfte: verfallen, genau einmal gemeldet.
        assert_eq!(stand.einloesen(&falsch, t), Urteil::Verfallen);
        assert!(!stand.offen());
        assert_eq!(stand.einloesen(&code, t), Urteil::GiltNicht);
        assert_eq!(stand.einloesen(&falsch, t), Urteil::GiltNicht);
    }

    #[test]
    fn auch_ein_falsches_format_zaehlt_als_fehlversuch() {
        let t = start();
        let mut stand = CodeStand::default();
        let code = stand.ausgeben(&zufall(9), t);
        for eingabe in ["", "zu kurz", "ÄÖÜ ß", &"A".repeat(500)] {
            assert_eq!(stand.einloesen(eingabe, t), Urteil::GiltNicht, "{eingabe:?}");
        }
        assert_eq!(stand.einloesen("?", t), Urteil::Verfallen);
        assert_eq!(stand.einloesen(&code, t), Urteil::GiltNicht);
    }

    #[test]
    fn vier_fehlversuche_und_dann_der_richtige_gilt() {
        let t = start();
        let mut stand = CodeStand::default();
        let code = stand.ausgeben(&zufall(10), t);
        for _ in 0..FEHLVERSUCHE - 1 {
            assert_eq!(stand.einloesen("falsch", t), Urteil::GiltNicht);
        }
        assert_eq!(stand.einloesen(&code, t), Urteil::Gilt);
    }

    #[test]
    fn ein_neuer_code_ersetzt_den_alten_und_den_zaehler() {
        let t = start();
        let mut stand = CodeStand::default();
        let alt = stand.ausgeben(&zufall(11), t);
        for _ in 0..FEHLVERSUCHE - 1 {
            assert_eq!(stand.einloesen("falsch", t), Urteil::GiltNicht);
        }
        let neu = stand.ausgeben(&zufall(12), t);
        assert_ne!(alt, neu);
        // Der alte gilt nicht mehr und zählt als erster Fehlversuch des neuen.
        assert_eq!(stand.einloesen(&alt, t), Urteil::GiltNicht);
        for _ in 0..FEHLVERSUCHE - 2 {
            assert_eq!(stand.einloesen("falsch", t), Urteil::GiltNicht);
        }
        assert_eq!(stand.einloesen(&neu, t), Urteil::Gilt);
    }

    #[test]
    fn abgelaufen_zaehlt_nicht_als_verfallen() {
        let t = start();
        let mut stand = CodeStand::default();
        stand.ausgeben(&zufall(13), t);
        let spaet = nach(t, GUELTIG + Duration::from_secs(1));
        // Ein falscher Code nach Ablauf: nur „gilt nicht“, der Code ist weg, nichts verfällt.
        assert_eq!(stand.einloesen("falsch", spaet), Urteil::GiltNicht);
        assert!(!stand.offen());
    }

    #[test]
    fn die_ausgabe_zeigt_den_hash_nicht() {
        let mut stand = CodeStand::default();
        let code = stand.ausgeben(&zufall(14), start());
        let gezeigt = format!("{stand:?}");
        assert_eq!(gezeigt, "CodeStand { offen: true }");
        assert!(!gezeigt.contains(&code));
    }

    #[test]
    fn vergleich_ohne_fruehes_ende_stimmt() {
        let a = hash("A");
        let mut b = a;
        assert!(gleich(&a, &b));
        b[31] ^= 1;
        assert!(!gleich(&a, &b));
        b = a;
        b[0] ^= 0x80;
        assert!(!gleich(&a, &b));
    }
}
