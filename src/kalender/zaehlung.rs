//! Zählkalender (Kalender, Stufe 5): Maya, Hochland Guatemalas, Azteken (Caso, Tena)
//! und der 13-Monde-Kalender (Dreamspell). Alles geht vom bürgerlichen Tag aus
//! (kein Wechsel um Sonnenuntergang, kein Ort) und rechnet über die Julianische
//! Tageszahl (JDN) ohne Jahresgrenze; Reste und Divisionen immer euklidisch.
//! Namen und Bezugszahlen sind Daten in `vorlagen/kalender/zaehlungen.toml`.

use std::collections::BTreeSet;

use calendrical_calculations::{gregorian, julian, rata_die::RataDie};
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{Belegstufe, Fest};

/// Abstand der Tageszählung der Kiste (Tag 1 = 1.1.1 gregorianisch) zur JDN.
const JDN_RD: i64 = 1_721_425;

/// Eine Art, ihre Pflichtlisten mit Länge und der sinnvolle Bereich von `bezug`.
type ArtDef = (&'static str, &'static [(&'static str, usize)], std::ops::RangeInclusive<i64>);

const ARTEN: [ArtDef; 5] = [
    ("maya", &[("zeichen", 20), ("monate", 19)], 400_000..=800_000),
    ("hochland", &[("zeichen", 20)], 400_000..=800_000),
    ("azteken-caso", &[("zeichen", 20), ("monate", 19)], 2_200_000..=2_400_000),
    ("azteken-tena", &[("zeichen", 20), ("monate", 19)], 2_200_000..=2_400_000),
    ("dreamspell", &[("zeichen", 20), ("toene", 13), ("wochentage", 7), ("farben", 4)], 2_440_000..=2_460_000),
];

/// Die Jahresträger der Azteken als Index in `zeichen`: Calli, Tochtli, Acatl, Tecpatl.
const TRAEGER: [usize; 4] = [2, 7, 12, 17];

/// Eine Zählung der Vorlage.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Zaehlvariante {
    pub kennung: String,
    pub titel: String,
    /// `maya`, `hochland`, `azteken-caso`, `azteken-tena` oder `dreamspell`.
    pub art: String,
    /// Julianische Tageszahl des Bezugstags (Korrelation bei den Maya).
    pub bezug: i64,
    pub stufe: Belegstufe,
    /// Wie gerechnet, ein Satz für die Anzeige.
    pub herkunft: String,
    /// Fester Satz für die Einzelansicht.
    #[serde(default)]
    pub hinweis: Option<String>,
    /// Woher die Namen stammen und in welcher Schreibung.
    pub quelle: String,
    /// Je Eintrag zuerst der Name, dahinter Zweitnamen.
    pub zeichen: Vec<Vec<String>>,
    #[serde(default)]
    pub monate: Vec<Vec<String>>,
    #[serde(default)]
    pub toene: Vec<String>,
    #[serde(default)]
    pub wochentage: Vec<String>,
    #[serde(default)]
    pub farben: Vec<String>,
    /// Feste nach der Zählung (`[[variante.fest]]`).
    #[serde(default)]
    pub fest: Vec<Fest>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Datei {
    variante: Vec<Zaehlvariante>,
}

/// Eine Zählung an einem Tag (`GET /api/kalender/tag`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Zaehlzeile {
    pub kennung: String,
    pub titel: String,
    /// Die Teile mit „ · “ verbunden, etwa „13.0.14.2.8 · 9 Lamat · 1 Mak · Kalenderrunde 17048“.
    pub stellung: String,
    pub teile: Vec<String>,
    pub herkunft: String,
    pub stufe: Belegstufe,
    /// Der feste Satz der Variante oder der Satz zum Zählbeginn; „Tag außerhalb der Zeit“ und „0.0 Hunab Ku“ stehen nur in `stellung`.
    pub hinweis: Option<String>,
}

/// Satz am Zählbeginn der Langen Zählung.
pub const SATZ_ZAEHLBEGINN: &str = "Zählbeginn: Rechnerisch 0.0.0.0.0; in den Maya-Inschriften steht dieser Tag als 13.0.0.0.0.";
pub const AUSSERHALB: &str = "Tag außerhalb der Zeit";
pub const HUNAB_KU: &str = "0.0 Hunab Ku";

/// Liest und prüft die Vorlage; die Sätze nennen Kennungen und Listen, keine Werte.
pub fn lesen(name: &str, text: &str) -> Result<Vec<Zaehlvariante>, String> {
    let d: Datei = toml::from_str(text).map_err(|e| format!("{name}: {}", e.message()))?;
    let mut kennungen = BTreeSet::new();
    for v in &d.variante {
        let k = &v.kennung;
        if !crate::vertrag::daten::ist_kennung(k) || !kennungen.insert(k.as_str()) || v.titel.trim().is_empty() {
            return Err(format!("{name}: Die Zählung „{k}“ hat keine einfache oder eine doppelte Kennung oder keinen Titel."));
        }
        let Some((_, listen, bereich)) = ARTEN.iter().find(|(a, _, _)| *a == v.art) else {
            let erlaubt: Vec<&str> = ARTEN.iter().map(|(a, _, _)| *a).collect();
            return Err(format!("{name}: Die Zählung „{k}“ hat eine unbekannte `art`. Erlaubt sind {}.", erlaubt.join(", ")));
        };
        for (liste, laenge) in *listen {
            let namen: Vec<&str> = match *liste {
                "zeichen" => v.zeichen.iter().map(|n| n.first().map_or("", String::as_str)).collect(),
                "monate" => v.monate.iter().map(|n| n.first().map_or("", String::as_str)).collect(),
                "toene" => v.toene.iter().map(String::as_str).collect(),
                "wochentage" => v.wochentage.iter().map(String::as_str).collect(),
                _ => v.farben.iter().map(String::as_str).collect(),
            };
            let einmal: BTreeSet<&str> = namen.iter().copied().collect();
            if namen.len() != *laenge || einmal.len() != namen.len() || einmal.iter().any(|n| n.trim().is_empty()) {
                return Err(format!("{name}: Bei der Zählung „{k}“ hat `{liste}` nicht genau {laenge} verschiedene Namen."));
            }
        }
        if !bereich.contains(&v.bezug) {
            return Err(format!("{name}: Bei der Zählung „{k}“ liegt `bezug` außerhalb des sinnvollen Bereichs für diese Art."));
        }
    }
    Ok(d.variante)
}

/// Julianische Tageszahl eines gregorianischen Tags.
pub fn jdn(d: NaiveDate) -> i64 {
    // Monat und Tag passen immer in u8.
    gregorian::fixed_from_gregorian(d.year(), d.month() as u8, d.day() as u8).to_i64_date() + JDN_RD
}

/// Julianische Tageszahl eines Tags im julianischen Kalender.
pub fn jdn_julianisch(jahr: i32, monat: u8, tag: u8) -> i64 {
    julian::fixed_from_julian(jahr, monat, tag).to_i64_date() + JDN_RD
}

fn jdn_gregorianisch(jahr: i32, monat: u8, tag: u8) -> i64 {
    gregorian::fixed_from_gregorian(jahr, monat, tag).to_i64_date() + JDN_RD
}

/// Der julianische Tag (Jahr, Monat, Tag) einer JDN.
fn julianisch(j: i64) -> (i32, u8, u8) {
    julian::julian_from_fixed(RataDie::new(j - JDN_RD)).unwrap_or((i32::MAX, 1, 1))
}

/// Der gregorianische Tag (Jahr, Monat, Tag) einer JDN.
fn gregorianisch(j: i64) -> (i32, u8, u8) {
    gregorian::gregorian_from_fixed(RataDie::new(j - JDN_RD)).unwrap_or((i32::MAX, 1, 1))
}

/// Maya-Zählungen an einem Tag (d = JDN − Korrelation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Maya {
    /// Baktun, Katun, Tun, Uinal, Kin; vor dem Zählbeginn keine.
    pub lange: Option<[i64; 5]>,
    pub zahl: i64,
    pub zeichen: usize,
    /// 0 = Pop … 17 = Kumk'u, 18 = Wayeb.
    pub monat: usize,
    /// 0 („Sitzung“) bis 19.
    pub tag: i64,
    /// Stellung in der Kalenderrunde (0 bis 18979).
    pub runde: i64,
    pub zaehlbeginn: bool,
}

pub fn maya(j: i64, korrelation: i64) -> Maya {
    let d = j - korrelation;
    let h = (d + 348).rem_euclid(365);
    Maya {
        lange: (d >= 0).then(|| {
            [d.div_euclid(144_000), d.div_euclid(7200).rem_euclid(20), d.div_euclid(360).rem_euclid(20), d.div_euclid(20).rem_euclid(18), d.rem_euclid(20)]
        }),
        zahl: (d + 3).rem_euclid(13) + 1,
        zeichen: index(d + 19, 20),
        monat: index(h.div_euclid(20), 19),
        tag: h.rem_euclid(20),
        runde: d.rem_euclid(18_980),
        zaehlbeginn: d == 0,
    }
}

/// Ein Rest als Index (immer kleiner als `n`).
fn index(x: i64, n: i64) -> usize {
    usize::try_from(x.rem_euclid(n)).unwrap_or(0)
}

/// Aztekische Zählungen an einem Tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Azteke {
    pub zahl: i64,
    pub zeichen: usize,
    /// Index in `monate` der Variante; 18 = Nemontemi.
    pub monat: usize,
    /// Tag der Veintena (1–20) oder der Nemontemi (1–5, bei Tena bis 6).
    pub tag: i64,
    pub traeger_zahl: i64,
    pub traeger_zeichen: usize,
}

/// Veintena und Tag aus dem Tag im Jahr (ab 0).
fn veintena(p: i64) -> (usize, i64) {
    if p < 360 { (index(p.div_euclid(20), 18), p.rem_euclid(20) + 1) } else { (18, p - 359) }
}

/// Caso 1967: fortlaufend ohne Schalttag. Das Jahr beginnt 201 Tage vor dem Bezugstag
/// (2 Xocotl Huetzi ist der 202. Tag des Jahres mit der Reihe ab Izcalli).
pub fn caso(j: i64, bezug: i64) -> Azteke {
    let d = j - bezug;
    let t = j - (bezug - 201);
    let (monat, tag) = veintena(t.rem_euclid(365));
    let y = t.div_euclid(365);
    Azteke {
        zahl: d.rem_euclid(13) + 1,
        zeichen: index(d + 4, 20),
        monat,
        tag,
        traeger_zahl: (2 + y).rem_euclid(13) + 1,
        traeger_zeichen: TRAEGER[index(y, 4)],
    }
}

/// Biquinto-Tage nach Tena (12.2. julianisch der Jahre J mit J mod 4 = 1) mit `von < b ≤ bis`,
/// schrittweise über die julianischen Jahre gezählt.
fn biquinto(von: i64, bis: i64) -> i64 {
    let (j0, j1) = (julianisch(von).0, julianisch(bis).0);
    let zahl = (j0..=j1).filter(|j| j.rem_euclid(4) == 1).map(|j| jdn_julianisch(j, 2, 12)).filter(|b| von < *b && *b <= bis).count();
    i64::try_from(zahl).unwrap_or(0)
}

/// Tena 1987/2008: Jahr fest am julianischen Kalender, 1 Atlcahualo am 13.2., in Jahren mit
/// julianischem 29.2. sechs Nemontemi; am sechsten rückt die 260er-Zählung nicht weiter.
pub fn tena(j: i64, bezug: i64) -> Azteke {
    let (jj, m, t) = julianisch(j);
    let y = if (m, t) >= (2, 13) { jj } else { jj - 1 };
    let (monat, tag) = veintena(j - jdn_julianisch(y, 2, 13));
    let b = if j >= bezug { biquinto(bezug, j) } else { -biquinto(j, bezug) };
    let dt = j - bezug - b;
    let n = i64::from(y) - 1521;
    Azteke {
        zahl: dt.rem_euclid(13) + 1,
        zeichen: index(dt + 4, 20),
        monat,
        tag,
        traeger_zahl: (n + 2).rem_euclid(13) + 1,
        traeger_zeichen: TRAEGER[index(n, 4)],
    }
}

/// 29. Februare (gregorianisch) mit `von < x ≤ bis`.
fn schalttage(von: i64, bis: i64) -> i64 {
    let (j0, j1) = (gregorianisch(von).0, gregorianisch(bis).0);
    let zahl = (j0..=j1).filter(|j| gregorian::is_leap_year(*j)).map(|j| jdn_gregorianisch(j, 2, 29)).filter(|x| von < *x && *x <= bis).count();
    i64::try_from(zahl).unwrap_or(0)
}

/// Der 13-Monde-Kalender an einem Tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dreamspell {
    /// 1–260; am 29.2. keiner.
    pub kin: Option<i64>,
    /// Mond (1–13) und Tag (1–28); am 25.7. und 29.2. keiner.
    pub mond: Option<(i64, i64)>,
    /// Kin des 26.7., mit dem das Jahr beginnt (Jahresname).
    pub jahr_kin: i64,
    pub ausserhalb: bool,
    pub hunab_ku: bool,
}

/// Kin eines Tags, der kein 29.2. ist (`bezug`: JDN des 26.7.1987 = Kin 34).
fn kin(j: i64, bezug: i64) -> i64 {
    let s = if j >= bezug { schalttage(bezug, j) } else { -schalttage(j, bezug) };
    (33 + (j - bezug) - s).rem_euclid(260) + 1
}

pub fn dreamspell(j: i64, bezug: i64) -> Dreamspell {
    let (jahr, m, t) = gregorianisch(j);
    let y = if (m, t) >= (7, 26) { jahr } else { jahr - 1 };
    let anfang = jdn_gregorianisch(y, 7, 26);
    let hunab_ku = (m, t) == (2, 29);
    let ausserhalb = (m, t) == (7, 25);
    let i = j - anfang - schalttage(anfang, j);
    Dreamspell {
        kin: (!hunab_ku).then(|| kin(j, bezug)),
        mond: (!hunab_ku && !ausserhalb).then(|| (i.div_euclid(28) + 1, i.rem_euclid(28) + 1)),
        jahr_kin: kin(anfang, bezug),
        ausserhalb,
        hunab_ku,
    }
}

/// Erster Name eines Eintrags.
fn erster(liste: &[Vec<String>], i: usize) -> &str {
    liste.get(i).and_then(|n| n.first()).map_or("", String::as_str)
}

fn eintrag(liste: &[String], i: i64) -> &str {
    usize::try_from(i).ok().and_then(|i| liste.get(i)).map_or("", String::as_str)
}

impl Zaehlvariante {
    /// Die Stellung im 260-Tage-Zyklus: Zahl (1–13) und Index des Zeichens; am 29.2. im
    /// 13-Monde-Kalender keine.
    pub fn stellung260(&self, j: i64) -> Option<(i64, usize)> {
        match self.art.as_str() {
            "maya" | "hochland" => {
                let m = maya(j, self.bezug);
                Some((m.zahl, m.zeichen))
            }
            "azteken-caso" => {
                let a = caso(j, self.bezug);
                Some((a.zahl, a.zeichen))
            }
            "azteken-tena" => {
                let a = tena(j, self.bezug);
                Some((a.zahl, a.zeichen))
            }
            "dreamspell" => dreamspell(j, self.bezug).kin.map(|k| ((k - 1).rem_euclid(13) + 1, index(k - 1, 20))),
            _ => None,
        }
    }

    /// Index eines Zeichens nach seinem ersten Namen.
    pub fn zeichen_index(&self, name: &str) -> Option<usize> {
        self.zeichen.iter().position(|n| n.first().is_some_and(|n| n == name))
    }

    /// „Yellow Magnetic Human“: Farbe, Ton und Siegel eines Kin (auch der Name des Jahres).
    fn siegel_name(&self, k: i64) -> String {
        let siegel = index(k - 1, 20);
        let farbe = eintrag(&self.farben, i64::try_from(siegel % 4).unwrap_or(0));
        format!("{farbe} {} {}", eintrag(&self.toene, (k - 1).rem_euclid(13)), erster(&self.zeichen, siegel))
    }

    /// Die Zeile für den Tag mit der JDN `j`.
    pub fn zeile(&self, j: i64) -> Zaehlzeile {
        let mut hinweis = self.hinweis.clone();
        let teile: Vec<String> = match self.art.as_str() {
            "maya" => {
                let m = maya(j, self.bezug);
                if m.zaehlbeginn {
                    hinweis = Some(SATZ_ZAEHLBEGINN.into());
                }
                let lange = m.lange.map(|l| l.map(|x| x.to_string()).join("."));
                lange
                    .into_iter()
                    .chain([
                        format!("{} {}", m.zahl, erster(&self.zeichen, m.zeichen)),
                        format!("{} {}", m.tag, erster(&self.monate, m.monat)),
                        format!("Kalenderrunde {}", m.runde),
                    ])
                    .collect()
            }
            "hochland" => {
                let m = maya(j, self.bezug);
                vec![format!("{} {}", m.zahl, erster(&self.zeichen, m.zeichen))]
            }
            "azteken-caso" | "azteken-tena" => {
                let a = if self.art == "azteken-caso" { caso(j, self.bezug) } else { tena(j, self.bezug) };
                let monat = erster(&self.monate, a.monat);
                let tag = if a.monat == 18 { format!("{monat} {}", a.tag) } else { format!("{} {monat}", a.tag) };
                vec![
                    format!("{} {}", a.zahl, erster(&self.zeichen, a.zeichen)),
                    tag,
                    format!("Jahr {} {}", a.traeger_zahl, erster(&self.zeichen, a.traeger_zeichen)),
                ]
            }
            "dreamspell" => {
                let d = dreamspell(j, self.bezug);
                let besonders = if d.hunab_ku {
                    Some(HUNAB_KU)
                } else if d.ausserhalb {
                    Some(AUSSERHALB)
                } else {
                    None
                };
                // Der Sondername steht nur in `stellung`; `hinweis` behält die Einordnung der Variante.
                besonders
                    .map(str::to_string)
                    .into_iter()
                    .chain(d.mond.map(|(m, t)| format!("Mond {m} Tag {t}")))
                    .chain(d.mond.map(|(_, t)| eintrag(&self.wochentage, (t - 1).rem_euclid(7)).to_string()))
                    .chain(d.kin.map(|k| format!("Kin {k} {}", self.siegel_name(k))))
                    .chain([format!("Jahr {}", self.siegel_name(d.jahr_kin))])
                    .collect()
            }
            _ => Vec::new(),
        };
        Zaehlzeile {
            kennung: self.kennung.clone(),
            titel: self.titel.clone(),
            stellung: teile.join(" · "),
            teile,
            herkunft: self.herkunft.clone(),
            stufe: self.stufe,
            hinweis,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VORLAGE: &str = include_str!("../../vorlagen/kalender/zaehlungen.toml");

    fn varianten() -> Vec<Zaehlvariante> {
        lesen("zaehlungen.toml", VORLAGE).unwrap()
    }

    fn v(kennung: &str) -> Zaehlvariante {
        varianten().into_iter().find(|v| v.kennung == kennung).unwrap()
    }

    fn g(t: &str) -> i64 {
        jdn(NaiveDate::parse_from_str(t, "%Y-%m-%d").unwrap())
    }

    fn stellung(kennung: &str, j: i64) -> String {
        v(kennung).zeile(j).stellung
    }

    #[test]
    fn julianische_tageszahl() {
        assert_eq!(g("2012-12-21"), 2_456_283);
        assert_eq!(g("2026-11-26"), 2_461_371);
        assert_eq!(g("2025-01-18"), 2_460_694);
        assert_eq!(g("0683-08-29"), 1_970_761);
        assert_eq!(g("1987-07-26"), 2_447_003);
        assert_eq!(jdn_julianisch(1500, 1, 1), 2_268_933);
        assert_eq!(jdn_julianisch(1521, 8, 13), 2_276_828);
        assert_eq!(julianisch(2_276_828), (1521, 8, 13));
    }

    #[test]
    fn vorlage_in_reihenfolge_und_pruefung() {
        let namen: Vec<String> = varianten().into_iter().map(|v| v.kennung).collect();
        assert_eq!(namen, ["maya-584283", "maya-584285", "hochland", "azteken-caso", "azteken-tena", "dreamspell"]);
        let falsch = |alt: &str, neu: &str| {
            assert!(VORLAGE.contains(alt), "{alt}");
            lesen("z.toml", &VORLAGE.replacen(alt, neu, 1)).unwrap_err()
        };
        // Sätze ohne den falschen Wert.
        for (alt, neu, wort, nicht) in [
            ("[\"Imix\", \"Imix\"], ", "", "`zeichen` nicht genau 20", "Imix"),
            ("[\"Wayeb\", \"Uayeb\"],", "", "`monate` nicht genau 19", "Wayeb"),
            ("\"Magnetic\", ", "", "`toene` nicht genau 13", "Magnetic"),
            ("\"Dali\", ", "", "`wochentage` nicht genau 7", "Dali"),
            ("[\"Ik'\", \"Ik\"]", "[\"Imix\"]", "verschiedene", "Imix"),
            ("kennung = \"maya-584285\"", "kennung = \"maya-584283\"", "doppelte", "584285"),
            ("art = \"hochland\"", "art = \"spuerhund-art\"", "unbekannte `art`", "spuerhund"),
            ("bezug = 584285", "bezug = 99", "außerhalb", "99"),
        ] {
            let satz = falsch(alt, neu);
            assert!(satz.contains(wort) && !satz.contains(nicht), "{neu}: {satz}");
        }
    }

    /// Ankernamen je Variante (Index 0, 4, 19), damit vertauschte Einträge der Vorlage auffallen.
    #[test]
    fn ankernamen() {
        for (kennung, namen) in [
            ("maya-584283", ["Imix", "Chikchan", "Ajaw"]),
            ("maya-584285", ["Imix", "Chikchan", "Ajaw"]),
            ("hochland", ["Imox", "Kan", "Ajpu"]),
            ("azteken-caso", ["Cipactli", "Coatl", "Xochitl"]),
            ("azteken-tena", ["Cipactli", "Coatl", "Xochitl"]),
            ("dreamspell", ["Dragon", "Serpent", "Sun"]),
        ] {
            let z = v(kennung).zeichen;
            assert_eq!([erster(&z, 0), erster(&z, 4), erster(&z, 19)], namen, "{kennung}");
        }
    }

    #[test]
    fn maya_testwerte() {
        let z = v("maya-584283").zeile(584_283);
        assert_eq!(z.stellung, "0.0.0.0.0 · 4 Ajaw · 8 Kumk'u · Kalenderrunde 0");
        assert_eq!(z.hinweis.as_deref(), Some(SATZ_ZAEHLBEGINN));
        assert_eq!(z.teile.len(), 4);
        let w = |k: &str, j: i64| stellung(k, j);
        assert_eq!(w("maya-584283", 2_456_283), "13.0.0.0.0 · 4 Ajaw · 3 K'ank'in · Kalenderrunde 11960");
        assert_eq!(w("maya-584283", 2_461_371), "13.0.14.2.8 · 9 Lamat · 1 Mak · Kalenderrunde 17048");
        assert_eq!(w("maya-584283", 1_970_761), "9.12.11.5.18 · 6 Etz'nab' · 11 Yax · Kalenderrunde 938");
        assert_eq!(w("maya-584283", 2_268_933), "11.13.19.10.10 · 10 Ok · 18 Mol · Kalenderrunde 14410");
        assert_eq!(v("maya-584283").zeile(2_461_371).hinweis, None);
        assert_eq!(w("maya-584285", 2_456_283), "12.19.19.17.18 · 2 Etz'nab' · 1 K'ank'in · Kalenderrunde 11958");
        assert_eq!(w("maya-584285", 2_456_285), "13.0.0.0.0 · 4 Ajaw · 3 K'ank'in · Kalenderrunde 11960");
        assert_eq!(w("maya-584285", 2_461_371), "13.0.14.2.6 · 7 Kimi · 19 Keh · Kalenderrunde 17046");
        // Vor dem Zählbeginn keine Lange Zählung.
        assert_eq!(w("maya-584285", 584_283), "2 Etz'nab' · 6 Kumk'u · Kalenderrunde 18978");
        assert_eq!(maya(2_456_283, 584_283).runde, 11_960);
    }

    #[test]
    fn hochland_testwerte() {
        let h = v("hochland");
        assert_eq!(h.zeile(2_456_283).stellung, "4 Ajpu");
        let z = h.zeile(2_460_694);
        assert_eq!(z.stellung, "8 B'atz'");
        assert!(z.hinweis.unwrap().contains("Sonnenuntergang"));
    }

    #[test]
    fn azteken_testwerte() {
        let c = |j: i64| stellung("azteken-caso", j);
        let t = |j: i64| stellung("azteken-tena", j);
        let jul = jdn_julianisch;
        assert_eq!(c(jul(1521, 8, 13)), "1 Coatl · 2 Xocotl Huetzi · Jahr 3 Calli");
        assert!(c(jul(1519, 11, 8)).starts_with("7 Cipactli · ") && c(jul(1519, 11, 8)).ends_with(" · Jahr 1 Acatl"));
        assert_eq!(c(2_456_283), "4 Xochitl · 17 Tozoztontli · Jahr 1 Calli");
        assert_eq!(c(2_461_371), "9 Tochtli · 15 Tlacaxipehualiztli · Jahr 2 Acatl");
        assert_eq!(t(jul(1521, 8, 13)), "1 Coatl · 2 Xocotl Huetzi · Jahr 3 Calli");
        assert!(t(jul(1521, 2, 11)).contains(" · Nemontemi 5 · "));
        assert!(t(jul(1521, 2, 12)).starts_with("1 Calli · Nemontemi 6 · "));
        assert!(t(jul(1521, 2, 13)).starts_with("2 Cuetzpalin · 1 Atlcahualo · "));
        assert!(t(jul(1519, 11, 8)).starts_with("8 Ehecatl · ") && t(jul(1519, 11, 8)).ends_with(" · Jahr 1 Acatl"));
        assert!(t(jul(1525, 2, 13)).starts_with("6 Cuetzpalin · "));
        assert!(c(jul(1525, 2, 13)).starts_with("7 Coatl · "));
        assert_eq!(t(2_456_283), "12 Tecpatl · 20 Panquetzaliztli · Jahr 13 Tecpatl");
        assert_eq!(t(2_461_371), "13 Ehecatl · 14 Quecholli · Jahr 1 Tochtli");
        // Der sechste Nemontemi trägt den Namen des Vortags (die 260er-Zählung rückt nicht weiter).
        assert_eq!(tena(jul(1521, 2, 12), 2_276_828).zeichen, tena(jul(1521, 2, 11), 2_276_828).zeichen);
    }

    #[test]
    fn dreamspell_testwerte() {
        let d = v("dreamspell");
        let s = |t: &str| d.zeile(g(t));
        assert!(s("1987-07-26").stellung.contains("Kin 34 "));
        assert!(s("1987-01-01").stellung.contains("Kin 88 "));
        assert!(s("1980-01-01").stellung.contains("Kin 133 Red Electric Skywalker"));
        assert!(s("2012-12-21").stellung.starts_with("Mond 6 Tag 9 · Seli · Kin 207 "));
        assert!(s("1996-02-28").stellung.contains("Kin 51 "));
        let hunab = s("1996-02-29");
        assert!(hunab.stellung.starts_with("0.0 Hunab Ku · Jahr ") && !hunab.stellung.contains("Kin"), "{}", hunab.stellung);
        // An den Sondertagen steht der Name nur in `stellung`; `hinweis` behält die Einordnung.
        assert_eq!(hunab.hinweis, d.hinweis);
        assert!(s("1996-03-01").stellung.starts_with("Mond 8 Tag 23 · ") && s("1996-03-01").stellung.contains("Kin 52 "));
        let aus = s("1988-07-25");
        assert!(aus.stellung.starts_with("Tag außerhalb der Zeit · Kin 138 "), "{}", aus.stellung);
        assert_eq!(aus.hinweis, d.hinweis);
        assert!(aus.hinweis.as_deref().is_some_and(|h| h.contains("nicht als Maya-Zählung an")), "{:?}", aus.hinweis);
        assert_eq!(s("2025-07-26").stellung, "Mond 1 Tag 1 · Dali · Kin 124 Yellow Resonant Seed · Jahr Yellow Resonant Seed");
        assert_eq!(s("2026-11-26").stellung, "Mond 5 Tag 12 · Alpha · Kin 92 Yellow Magnetic Human · Jahr Red Galactic Moon");
        assert!(s("2030-07-24").stellung.starts_with("Mond 13 Tag 28 · Silio · Kin 127 "));
        assert!(s("2026-11-26").hinweis.unwrap().starts_with("Neuzeitliche Umdeutung"));
    }

    /// Über 1800–2150: Caso-Tonalpohualli = Tzolkin 584283, Kalenderrunde stimmig, Kin rückt
    /// an jedem Tag außer dem 29.2. um eins weiter; Tena wiederholt einen Namen nur am Biquinto.
    #[test]
    fn eigenschaften_1800_bis_2150() {
        let ds = v("dreamspell");
        let (von, bis) = (g("1800-01-01"), g("2150-12-31"));
        let mut vorher = kin(von - 1, ds.bezug);
        let mut tena_vorher = tena(von - 1, 2_276_828);
        for j in von..=bis {
            let m = maya(j, 584_283);
            let c = caso(j, 2_276_828);
            assert_eq!((c.zahl, c.zeichen), (m.zahl, m.zeichen), "{j}");
            assert_eq!((i64::try_from(m.zeichen).unwrap() - m.tag).rem_euclid(5), 1, "{j}");
            let d = dreamspell(j, ds.bezug);
            if let Some(k) = d.kin {
                assert_eq!(k, vorher % 260 + 1, "{j}");
                vorher = k;
            }
            let t = tena(j, 2_276_828);
            let (jahr, monat, tag) = julianisch(j);
            if (monat, tag) == (2, 12) && jahr.rem_euclid(4) == 1 {
                assert_eq!((t.zahl, t.zeichen), (tena_vorher.zahl, tena_vorher.zeichen), "{j}");
            } else {
                assert_eq!(t.zeichen, (tena_vorher.zeichen + 1) % 20, "{j}");
            }
            tena_vorher = t;
        }
    }
}
