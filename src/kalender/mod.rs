//! Kalender, Stufe 1: Feste aus benannten Regelsätzen (Kalendern), mehrere
//! zugleich, mit Herkunft je Termin. Die Kalender liegen als Vorlagen unter
//! `vorlagen/kalender/` und sind eingebaut ([`Katalog::eingebaut`]).
//!
//! Eine Regel hat genau eine Art; welche, bestimmt ein Schlüssel:
//!
//! - festes Datum: `monat`, `tag` (30.4. in jedem Jahr; den 29.2. nur in Schaltjahren),
//! - Abstand: `nach` (ein Fest) und `tage` (mit Vorzeichen), etwa Pfingsten = Ostern + 49,
//! - Wochentag nach Bezug: `wochentag` und `nach`, der erste solche Tag nach dem
//!   Bezug (der Bezugstag selbst zählt nicht),
//! - Ostersonntag: `ostern = "westlich"` oder `"orthodox"` (julianische Rechnung,
//!   als heutiges Datum),
//! - chinesischer Kalender: `kalender = "chinesisch"`, `monat`, `tag`, wahlweise
//!   `schaltmonat = true` (Tage nach Pekinger Zeit),
//! - Verweis: `fest = "<kalender>.<fest>"`, nur an Ereignissen in `fristen.toml`.
//!
//! `nach` nennt in einer Kalenderdatei ein Fest desselben Kalenders (`ostern`)
//! oder eines anderen (`kirche.ostern`); an einem Ereignis immer mit Kalender.
//! Ein Fest gehört zu einem Jahr: Abstand und Wochentag rechnen vom Bezug
//! desselben Jahres. Gerechnet wird für die Jahre 1912 bis 2100 (für das
//! chinesische Neujahr gelten die heutigen Regeln seit 1912, die Tabellen des
//! Hong Kong Observatory reichen bis 2100); außerhalb gibt es kein Datum.
//!
//! Später (nicht in Stufe 1): Himmelsereignisse und Ostern nach dem Mond,
//! Tages- und Gebetszeiten, Zählkalender. Neue Arten kommen als weiterer Schlüssel dazu.

use std::collections::BTreeSet;
use std::sync::OnceLock;

use calendrical_calculations::chinese_based::{self, Chinese, YearBounds};
use calendrical_calculations::{gregorian, julian, rata_die::RataDie};
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::vertrag::daten::ist_kennung;
use crate::vertrag::fristen::Abstand;

/// Das erste und das letzte Jahr, für das gerechnet wird.
pub const ERSTES_JAHR: i32 = 1912;
pub const LETZTES_JAHR: i32 = 2100;

/// So weit darf ein Abstand in einer Regel höchstens reichen: ein Jahr. So liegt
/// das Fest eines Jahres nah bei diesem Jahr (Annahme von [`Katalog::naechstes`]);
/// weiter weg gehört es als Abstand an die Frist.
pub const TAGE_HOECHSTENS: i32 = 366;

/// Die eingebauten Kalender (Dateiname, Inhalt).
const EINGEBAUT: [(&str, &str); 3] = [
    ("vorlagen/kalender/kirche.toml", include_str!("../../vorlagen/kalender/kirche.toml")),
    ("vorlagen/kalender/orthodox.toml", include_str!("../../vorlagen/kalender/orthodox.toml")),
    ("vorlagen/kalender/chinesisch.toml", include_str!("../../vorlagen/kalender/chinesisch.toml")),
];

/// Eine Regel, wie sie in einer Kalenderdatei oder an einem Ereignis steht.
/// Welche Felder zusammen gelten, prüft [`Regel::art`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Regel {
    /// Verweis auf ein Fest: `<kalender>.<fest>`, etwa `kirche.ostern`.
    #[serde(default)]
    pub fest: Option<String>,
    /// Monat (1–12): festes Datum oder Monat im chinesischen Kalender.
    #[serde(default)]
    pub monat: Option<u8>,
    /// Tag im Monat (festes Datum 1–31, chinesisch 1–30).
    #[serde(default)]
    pub tag: Option<u8>,
    /// Bezug für Abstand und Wochentag: ein Fest.
    #[serde(default)]
    pub nach: Option<String>,
    /// Abstand zum Bezug in Tagen, mit Vorzeichen (negativ: davor).
    #[serde(default)]
    pub tage: Option<i32>,
    /// `montag` … `sonntag`: der erste solche Tag nach dem Bezug.
    #[serde(default)]
    pub wochentag: Option<String>,
    /// `westlich` oder `orthodox`.
    #[serde(default)]
    pub ostern: Option<String>,
    /// Ein anderer Kalender; heute nur `chinesisch`.
    #[serde(default)]
    pub kalender: Option<String>,
    /// Nur mit `kalender`: der Schaltmonat nach dem Monat `monat`.
    #[serde(default)]
    pub schaltmonat: Option<bool>,
}

/// Wie Ostern gerechnet wird.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsterRechnung {
    Westlich,
    Orthodox,
}

/// Die Art einer geprüften Regel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Art<'a> {
    Datum { monat: u32, tag: u32 },
    Abstand { nach: &'a str, tage: i32 },
    WochentagNach { wochentag: Weekday, nach: &'a str },
    Ostern(OsterRechnung),
    Chinesisch { monat: u8, tag: u8, schaltmonat: bool },
    Fest(&'a str),
}

const WOCHENTAGE: [(&str, Weekday); 7] = [
    ("montag", Weekday::Mon),
    ("dienstag", Weekday::Tue),
    ("mittwoch", Weekday::Wed),
    ("donnerstag", Weekday::Thu),
    ("freitag", Weekday::Fri),
    ("samstag", Weekday::Sat),
    ("sonntag", Weekday::Sun),
];

const ARTEN: &str = "`monat` und `tag`; `nach` und `tage`; `wochentag` und `nach`; `ostern`; `kalender` mit `monat` und `tag`; `fest`";

impl Regel {
    /// Die gesetzten Schlüssel, in fester Reihenfolge.
    fn gesetzt(&self) -> Vec<&'static str> {
        [
            ("fest", self.fest.is_some()),
            ("monat", self.monat.is_some()),
            ("tag", self.tag.is_some()),
            ("nach", self.nach.is_some()),
            ("tage", self.tage.is_some()),
            ("wochentag", self.wochentag.is_some()),
            ("ostern", self.ostern.is_some()),
            ("kalender", self.kalender.is_some()),
            ("schaltmonat", self.schaltmonat.is_some()),
        ]
        .into_iter()
        .filter_map(|(n, ja)| ja.then_some(n))
        .collect()
    }

    /// Prüft, ob die Felder zusammenpassen, und liefert die Art. Die Fehler
    /// sind ganze Sätze ohne Kennung des Fests (die setzt der Aufrufer davor).
    pub fn art(&self) -> Result<Art<'_>, String> {
        let gesetzt = self.gesetzt();
        let nur = |art: &str, felder: &[&str]| -> Result<(), String> {
            match gesetzt.iter().find(|g| !felder.contains(g)) {
                Some(g) => Err(format!("Eine Regel mit {art} hat kein `{g}`. Erlaubt ist je Regel genau eine Art: {ARTEN}.")),
                None => Ok(()),
            }
        };
        let pflicht = |name: &str, da: bool, art: &str| if da { Ok(()) } else { Err(format!("Eine Regel mit {art} braucht auch `{name}`.")) };
        if let Some(o) = &self.ostern {
            nur("`ostern`", &["ostern"])?;
            return match o.as_str() {
                "westlich" => Ok(Art::Ostern(OsterRechnung::Westlich)),
                "orthodox" => Ok(Art::Ostern(OsterRechnung::Orthodox)),
                // Die Sätze nennen den eingegebenen Wert nicht (welche Kalender jemand nutzt, ist persönlich).
                _ => Err("Diesen Wert für `ostern` gibt es nicht. Erlaubt sind `\"westlich\"` und `\"orthodox\"`.".into()),
            };
        }
        if let Some(k) = &self.kalender {
            nur("`kalender`", &["kalender", "monat", "tag", "schaltmonat"])?;
            if k != "chinesisch" {
                return Err("Diesen `kalender` gibt es nicht. Erlaubt ist `\"chinesisch\"`.".into());
            }
            let (monat, tag) = (self.monat.unwrap_or(0), self.tag.unwrap_or(0));
            pflicht("monat", self.monat.is_some(), "`kalender`")?;
            pflicht("tag", self.tag.is_some(), "`kalender`")?;
            if !(1..=12).contains(&monat) || !(1..=30).contains(&tag) {
                return Err(format!("Im chinesischen Kalender gibt es den Tag {tag} im Monat {monat} nicht. Erlaubt sind Monat 1–12 und Tag 1–30."));
            }
            return Ok(Art::Chinesisch { monat, tag, schaltmonat: self.schaltmonat.unwrap_or(false) });
        }
        if let Some(f) = &self.fest {
            nur("`fest`", &["fest"])?;
            return Ok(Art::Fest(f));
        }
        if let Some(w) = &self.wochentag {
            nur("`wochentag`", &["wochentag", "nach"])?;
            let Some(&(_, wochentag)) = WOCHENTAGE.iter().find(|(n, _)| n == w) else {
                return Err("Diesen `wochentag` gibt es nicht. Erlaubt sind `montag` bis `sonntag`, klein geschrieben.".into());
            };
            pflicht("nach", self.nach.is_some(), "`wochentag`")?;
            return Ok(Art::WochentagNach { wochentag, nach: self.nach.as_deref().unwrap_or_default() });
        }
        if let Some(t) = self.tage {
            nur("`tage`", &["tage", "nach"])?;
            pflicht("nach", self.nach.is_some(), "`tage`")?;
            if !(-TAGE_HOECHSTENS..=TAGE_HOECHSTENS).contains(&t) {
                return Err(format!("`tage = {t}` ist zu weit. Erlaubt sind höchstens {TAGE_HOECHSTENS} Tage davor oder danach."));
            }
            return Ok(Art::Abstand { nach: self.nach.as_deref().unwrap_or_default(), tage: t });
        }
        if self.monat.is_some() || self.tag.is_some() {
            nur("`monat` und `tag`", &["monat", "tag"])?;
            let (monat, tag) = (u32::from(self.monat.unwrap_or(0)), u32::from(self.tag.unwrap_or(0)));
            // 2000 ist ein Schaltjahr: Der 29.2. ist erlaubt und gilt dann nur in Schaltjahren.
            if self.monat.is_none() || self.tag.is_none() || NaiveDate::from_ymd_opt(2000, monat, tag).is_none() {
                return Err(format!(
                    "Ein festes Datum braucht `monat` und `tag` als echten Tag, etwa `monat = 4, tag = 30`; den {tag}.{monat}. gibt es nicht."
                ));
            }
            return Ok(Art::Datum { monat, tag });
        }
        if self.nach.is_some() {
            return Err("Eine Regel mit `nach` braucht auch `tage` oder `wochentag`.".into());
        }
        Err(format!("Die Regel ist leer. Erlaubt ist genau eine Art: {ARTEN}."))
    }

    /// Die Feste, auf die die Regel verweist (`nach`, `fest`).
    fn verweise(&self) -> impl Iterator<Item = &str> {
        self.nach.as_deref().into_iter().chain(self.fest.as_deref())
    }
}

/// Ein Fest eines Kalenders.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fest {
    pub kennung: String,
    pub titel: String,
    pub regel: Regel,
}

/// Ein Kalender: ein benannter Satz von Festen.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Kalender {
    pub kennung: String,
    pub titel: String,
    #[serde(default)]
    pub fest: Vec<Fest>,
}

/// Alle bekannten Kalender, geprüft: Kennungen einmalig, Regeln gültig,
/// Verweise bekannt, keine Kreise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Katalog {
    kalender: Vec<Kalender>,
}

/// Höchstens so tief folgt die Rechnung Verweisen (Kreise weist die Prüfung vorher ab).
const TIEFE_HOECHSTENS: usize = 16;

impl Katalog {
    /// Die eingebauten Kalender. Sie sind beim Bauen geprüft (Test
    /// `eingebaute_kalender_sind_gueltig`); schlägt das Lesen doch fehl, ist der Katalog leer.
    pub fn eingebaut() -> &'static Katalog {
        static K: OnceLock<Katalog> = OnceLock::new();
        K.get_or_init(|| Katalog::lesen(&EINGEBAUT).unwrap_or(Katalog { kalender: Vec::new() }))
    }

    /// Liest und prüft Kalenderdateien (Name, Inhalt). Fehler nennen Datei und Fest.
    pub fn lesen(dateien: &[(&str, &str)]) -> Result<Katalog, String> {
        let mut kalender = Vec::new();
        for (name, text) in dateien {
            kalender.push(toml::from_str::<Kalender>(text).map_err(|e| format!("{name}: {}", e.message()))?);
        }
        let k = Katalog { kalender };
        k.pruefen()?;
        Ok(k)
    }

    fn pruefen(&self) -> Result<(), String> {
        let mut namen = BTreeSet::new();
        for k in &self.kalender {
            if !ist_kennung(&k.kennung) || !namen.insert(k.kennung.as_str()) {
                return Err(format!("Der Kalender „{}“ hat keine einfache oder eine doppelte `kennung`.", k.kennung));
            }
            let mut feste = BTreeSet::new();
            for f in &k.fest {
                if !ist_kennung(&f.kennung) || !feste.insert(f.kennung.as_str()) || f.titel.trim().is_empty() {
                    return Err(format!(
                        "Im Kalender „{}“ hat das Fest „{}“ keine einfache oder eine doppelte `kennung` oder keinen `titel`.",
                        k.kennung, f.kennung
                    ));
                }
                let wo = format!("Im Kalender „{}“, Fest „{}“: ", k.kennung, f.kennung);
                f.regel.art().map_err(|e| format!("{wo}{e}"))?;
                for v in f.regel.verweise() {
                    if self.finden(Some(&k.kennung), v).is_none() {
                        return Err(format!("{wo}Das Fest „{v}“ gibt es nicht."));
                    }
                }
            }
        }
        for k in &self.kalender {
            for f in &k.fest {
                self.ohne_kreis(&k.kennung, f, &mut Vec::new())?;
            }
        }
        Ok(())
    }

    fn ohne_kreis<'a>(&'a self, kal: &'a str, f: &'a Fest, pfad: &mut Vec<(&'a str, &'a str)>) -> Result<(), String> {
        if pfad.contains(&(kal, f.kennung.as_str())) {
            let kette: Vec<String> = pfad.iter().map(|(k, f)| format!("{k}.{f}")).collect();
            return Err(format!("Die Regeln verweisen im Kreis: {} → {kal}.{}.", kette.join(" → "), f.kennung));
        }
        if pfad.len() >= TIEFE_HOECHSTENS {
            return Err(format!("Die Regeln verweisen über mehr als {TIEFE_HOECHSTENS} Stufen; so tief rechnet der Server nicht ({kal}.{}).", f.kennung));
        }
        pfad.push((kal, &f.kennung));
        for v in f.regel.verweise() {
            if let Some((k, g)) = self.finden(Some(kal), v) {
                self.ohne_kreis(&k.kennung, g, pfad)?;
            }
        }
        pfad.pop();
        Ok(())
    }

    /// Alle Kalender.
    pub fn kalender(&self) -> &[Kalender] {
        &self.kalender
    }

    /// Ein Fest zu einem Verweis: `kalender.fest`, oder ohne Punkt im Kalender `im`.
    pub fn finden(&self, im: Option<&str>, verweis: &str) -> Option<(&Kalender, &Fest)> {
        let (kal, fest) = match verweis.split_once('.') {
            Some((k, f)) => (k, f),
            None => (im?, verweis),
        };
        let k = self.kalender.iter().find(|k| k.kennung == kal)?;
        Some((k, k.fest.iter().find(|f| f.kennung == fest)?))
    }

    /// Prüft eine Regel an einem Ereignis: gültige Art, Verweise mit Kalender
    /// und bekannt. Welche Feste jemand nutzt, ist persönlich: Die Sätze nennen
    /// den Verweis nicht, nur die bekannten Kalender.
    pub fn ereignis_regel_pruefen(&self, regel: &Regel) -> Result<(), String> {
        regel.art()?;
        for v in regel.verweise() {
            if !v.contains('.') {
                return Err("Am Ereignis nennt ein Verweis den Kalender mit, etwa `kirche.ostern` statt `ostern`.".into());
            }
            if self.finden(None, v).is_none() {
                let bekannt: Vec<&str> = self.kalender.iter().map(|k| k.kennung.as_str()).collect();
                return Err(format!(
                    "Der Verweis nennt ein Fest, das es nicht gibt. Bekannte Kalender: {}; ihre Feste stehen unter vorlagen/kalender/.",
                    bekannt.join(", ")
                ));
            }
        }
        Ok(())
    }

    /// Das Datum einer Regel im Jahr `jahr` (`im`: Kalender für Verweise ohne Punkt).
    pub fn im_jahr(&self, im: Option<&str>, regel: &Regel, jahr: i32) -> Option<NaiveDate> {
        self.rechnen(im, regel, jahr, 0)
    }

    fn rechnen(&self, im: Option<&str>, regel: &Regel, jahr: i32, tiefe: usize) -> Option<NaiveDate> {
        if !(ERSTES_JAHR..=LETZTES_JAHR).contains(&jahr) || tiefe > TIEFE_HOECHSTENS {
            return None;
        }
        let bezug = |v: &str| {
            let (k, f) = self.finden(im, v)?;
            self.rechnen(Some(&k.kennung), &f.regel, jahr, tiefe + 1)
        };
        match regel.art().ok()? {
            Art::Datum { monat, tag } => NaiveDate::from_ymd_opt(jahr, monat, tag),
            Art::Abstand { nach, tage } => bezug(nach)?.checked_add_signed(Duration::days(i64::from(tage))),
            Art::WochentagNach { wochentag, nach } => {
                let b = bezug(nach)?;
                let schritt = (7 + wochentag.num_days_from_monday() - b.weekday().num_days_from_monday() - 1) % 7 + 1;
                b.checked_add_signed(Duration::days(i64::from(schritt)))
            }
            Art::Ostern(OsterRechnung::Westlich) => datum(gregorian::easter(jahr)),
            Art::Ostern(OsterRechnung::Orthodox) => datum(julian::easter(jahr)),
            Art::Chinesisch { monat, tag, schaltmonat } => chinesisch(jahr, monat, tag, schaltmonat),
            Art::Fest(v) => bezug(v),
        }
    }

    /// Das früheste Vorkommen einer Regel, dessen Frist (Vorkommen plus
    /// `abstand`) heute oder später liegt, mit der Herkunft als Satz. So
    /// rückt eine wiederkehrende Frist erst am Tag nach ihrem Ende weiter, und
    /// eine Frist nach dem Fest bleibt bis dahin beim vergangenen Vorkommen.
    pub fn naechstes(&self, im: Option<&str>, regel: &Regel, abstand: Abstand, heute: NaiveDate) -> Option<(NaiveDate, String)> {
        // Grob zurückrechnen und ab drei Jahren davor aufwärts suchen: Das Fest
        // des Jahres J liegt höchstens ein Jahr (TAGE_HOECHSTENS, chinesischer
        // 12. Monat im Januar J+1) neben J, und seine Tage steigen mit J. Das erste
        // passende ist also das früheste; manche Regeln fehlen in vielen Jahren
        // (29.2., 30. Tag eines kurzen Monats, ein bestimmter Schaltmonat).
        let mitte = Abstand { zahl: -abstand.zahl, einheit: abstand.einheit }.von(heute)?.year();
        let (v, j) = (mitte.saturating_sub(3).max(ERSTES_JAHR)..=LETZTES_JAHR)
            .filter_map(|j| Some((self.im_jahr(im, regel, j)?, j)))
            .find(|(v, _)| abstand.von(*v).is_some_and(|f| f >= heute))?;
        Some((v, self.herkunft(im, regel, j)))
    }

    /// Herkunft eines Vorkommens als Satz: Kalender · Fest · Regel (Bezug mit Datum).
    pub fn herkunft(&self, im: Option<&str>, regel: &Regel, jahr: i32) -> String {
        if let Ok(Art::Fest(v)) = regel.art()
            && let Some((k, f)) = self.finden(im, v)
        {
            return format!("{} · {} · {}", k.titel, f.titel, self.regel_satz(Some(&k.kennung), &f.regel, jahr));
        }
        format!("Eigene Regel · {}", self.regel_satz(im, regel, jahr))
    }

    fn regel_satz(&self, im: Option<&str>, regel: &Regel, jahr: i32) -> String {
        let bezug = |v: &str| match self.finden(im, v) {
            Some((k, f)) => {
                let tag = self.im_jahr(Some(&k.kennung), &f.regel, jahr).map(|d| format!(" ({})", d.format("%-d.%-m.%Y"))).unwrap_or_default();
                format!("{}{tag}", f.titel)
            }
            None => v.to_string(),
        };
        match regel.art() {
            Ok(Art::Datum { monat, tag }) => format!("jedes Jahr am {tag}.{monat}."),
            Ok(Art::Abstand { nach, tage }) => {
                let (n, wo) = if tage < 0 { (-tage, "vor") } else { (tage, "nach") };
                format!("{n} {} {wo} {}", if n == 1 { "Tag" } else { "Tage" }, bezug(nach))
            }
            Ok(Art::WochentagNach { wochentag, nach }) => {
                let name = WOCHENTAGE.iter().find(|(_, w)| *w == wochentag).map(|(n, _)| *n).unwrap_or_default();
                let gross: String = name.chars().take(1).flat_map(char::to_uppercase).chain(name.chars().skip(1)).collect();
                format!("erster {gross} nach {}", bezug(nach))
            }
            Ok(Art::Ostern(OsterRechnung::Westlich)) => "Ostersonntag nach westlicher Rechnung".into(),
            Ok(Art::Ostern(OsterRechnung::Orthodox)) => "Ostersonntag nach orthodoxer (julianischer) Rechnung".into(),
            Ok(Art::Chinesisch { monat, tag, schaltmonat }) => {
                let s = if schaltmonat { "Schaltmonats nach dem " } else { "" };
                format!("{tag}. Tag des {s}{monat}. Monats im chinesischen Kalender")
            }
            Ok(Art::Fest(v)) => bezug(v),
            Err(e) => e,
        }
    }
}

/// Ein Tag der Kiste als Datum.
fn datum(rd: RataDie) -> Option<NaiveDate> {
    // Tag 1 der Kiste ist der 1.1. des Jahres 1 (gregorianisch), wie bei chrono.
    NaiveDate::from_num_days_from_ce_opt(i32::try_from(rd.to_i64_date()).ok()?)
}

/// Ein Tag im chinesischen Kalender, im Jahr, das im gregorianischen Jahr
/// `jahr` beginnt. `None`, wenn es den Tag in diesem Jahr nicht gibt (Tag 30
/// in einem kurzen Monat, kein solcher Schaltmonat).
fn chinesisch(jahr: i32, monat: u8, tag: u8, schaltmonat: bool) -> Option<NaiveDate> {
    let grenzen = YearBounds::compute::<Chinese>(gregorian::fixed_from_gregorian(jahr, 7, 1));
    // Dieselbe Rechnung, aus der icu_calendar seine Tabelle erzeugt. Nicht
    // `get_leap_month_from_new_year` (und `chinese_based_date_from_fixed`, das es nutzt):
    // In calendrical_calculations 0.2.4 nennt es für 2033 den Schaltmonat an Stelle 8
    // statt 12 (HKO: zweiter 11. Monat ab 22.12.2033); gefunden mit `chinesisch_gegen_hko`.
    let (lang, schalt) = chinese_based::month_structure_for_year::<Chinese>(grenzen.new_year, grenzen.next_new_year);
    // Der Schaltmonat steht als Stelle in der Folge der Monate: Stelle s wiederholt Monat s − 1.
    let stelle = match (schalt, schaltmonat) {
        (Some(s), true) if s == monat + 1 => s,
        (_, true) => return None,
        (Some(s), false) if monat >= s => monat + 1,
        (_, false) => monat,
    };
    let laenge = |i: u8| -> u8 { if lang[usize::from(i)] { 30 } else { 29 } };
    if tag > laenge(stelle - 1) {
        return None;
    }
    let vorher: i64 = (0..stelle - 1).map(|i| i64::from(laenge(i))).sum();
    datum(grenzen.new_year + vorher + i64::from(tag - 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vertrag::fristen::Einheit;

    fn tag(t: &str) -> NaiveDate {
        NaiveDate::parse_from_str(t, "%Y-%m-%d").unwrap()
    }

    fn regel(toml: &str) -> Regel {
        toml::from_str(toml).unwrap()
    }

    fn fest(k: &Katalog, verweis: &str, jahr: i32) -> Option<String> {
        k.im_jahr(None, &Regel { fest: Some(verweis.into()), ..Default::default() }, jahr).map(|d| d.to_string())
    }

    #[test]
    fn eingebaute_kalender_sind_gueltig() {
        let k = Katalog::lesen(&EINGEBAUT).unwrap();
        assert_eq!(&k, Katalog::eingebaut());
        let namen: Vec<&str> = k.kalender().iter().map(|k| k.kennung.as_str()).collect();
        assert_eq!(namen, ["kirche", "orthodox", "chinesisch"]);
    }

    /// Ostern westlich und orthodox (Werte aus der Testliste der Kiste und
    /// veröffentlichten Ostertabellen), dazu die Feste, die daran hängen.
    #[test]
    fn ostern_und_kirchenjahr() {
        let k = Katalog::eingebaut();
        for (j, w, o) in [
            (2021, "2021-04-04", "2021-05-02"),
            (2022, "2022-04-17", "2022-04-24"),
            (2023, "2023-04-09", "2023-04-16"),
            (2024, "2024-03-31", "2024-05-05"),
            (2025, "2025-04-20", "2025-04-20"),
            (2026, "2026-04-05", "2026-04-12"),
            (2027, "2027-03-28", "2027-05-02"),
            (2038, "2038-04-25", "2038-04-25"),
        ] {
            assert_eq!(fest(k, "kirche.ostern", j).as_deref(), Some(w), "westlich {j}");
            assert_eq!(fest(k, "orthodox.ostern", j).as_deref(), Some(o), "orthodox {j}");
        }
        for (f, d) in [
            ("aschermittwoch", "2026-02-18"),
            ("karfreitag", "2026-04-03"),
            ("himmelfahrt", "2026-05-14"),
            ("pfingsten", "2026-05-24"),
            ("pfingstmontag", "2026-05-25"),
            ("dreifaltigkeit", "2026-05-31"),
            ("fronleichnam", "2026-06-04"),
            ("heiligabend", "2026-12-24"),
        ] {
            assert_eq!(fest(k, &format!("kirche.{f}"), 2026).as_deref(), Some(d), "{f}");
        }
        assert_eq!(fest(k, "kirche.ostern", 1911), None, "vor 1912 nicht");
        assert_eq!(fest(k, "kirche.ostern", 2101), None, "nach 2100 nicht");
        assert_eq!(fest(k, "kirche.ostern", 2100).as_deref(), Some("2100-03-28"));
    }

    /// Alle Monate 1912–2100 gegen die Tabelle des Hong Kong Observatory:
    /// Neujahr, Schaltmonat und der erste und letzte Tag jedes Monats.
    #[test]
    fn chinesisch_gegen_hko() {
        let mut jahre = 0;
        for zeile in include_str!("hko-1912-2100.txt").lines().filter(|z| !z.starts_with('#')) {
            let teile: Vec<&str> = zeile.split(' ').collect();
            let [j, neujahr, schalt, laengen] = teile[..] else { panic!("{zeile}") };
            let j: i32 = j.parse().unwrap();
            let schalt: Option<u8> = schalt.parse().ok();
            let mut anfang = tag(neujahr);
            for (i, l) in laengen.chars().enumerate() {
                let stelle = i as u8 + 1;
                let (monat, ist_schalt) = match schalt {
                    Some(s) if stelle == s => (s - 1, true),
                    Some(s) if stelle > s => (stelle - 1, false),
                    _ => (stelle, false),
                };
                let laenge = if l == 'L' { 30 } else { 29 };
                let am = |t| chinesisch(j, monat, t, ist_schalt);
                assert_eq!(am(1), Some(anfang), "{j} Monat {monat} Schalt {ist_schalt}");
                assert_eq!(am(laenge), anfang.checked_add_signed(Duration::days(i64::from(laenge) - 1)), "{j}/{monat} letzter Tag");
                if laenge == 29 {
                    assert_eq!(am(30), None, "{j}/{monat} hat keinen 30.");
                }
                anfang += Duration::days(i64::from(laenge));
            }
            if schalt.is_none() {
                assert_eq!(chinesisch(j, 6, 1, true), None, "{j} ohne Schaltmonat");
            }
            jahre += 1;
        }
        assert_eq!(jahre, 189);
        let k = Katalog::eingebaut();
        for (j, d) in [(2026, "2026-02-17"), (2027, "2027-02-06"), (2028, "2028-01-26"), (2033, "2033-01-31"), (2057, "2057-02-04"), (2097, "2097-02-12")] {
            assert_eq!(fest(k, "chinesisch.neujahr", j).as_deref(), Some(d), "{j}");
        }
        assert_eq!(chinesisch(2033, 11, 1, true), Some(tag("2033-12-22")), "Schaltmonat 11 im Jahr 2033");
    }

    #[test]
    fn regelarten_und_wochentag() {
        let k = Katalog::lesen(&[
            (
                "probe.toml",
                r#"
kennung = "probe"
titel = "Probe"
[[fest]]
kennung = "mai"
titel = "Erster Mai"
regel = { monat = 5, tag = 1 }
[[fest]]
kennung = "do-nach-mai"
titel = "Donnerstag danach"
regel = { wochentag = "donnerstag", nach = "mai" }
[[fest]]
kennung = "fr-nach-mai"
titel = "Freitag danach"
regel = { wochentag = "freitag", nach = "mai" }
[[fest]]
kennung = "schalttag"
titel = "Schalttag"
regel = { monat = 2, tag = 29 }
[[fest]]
kennung = "vor-ostern"
titel = "Drei Tage vor Ostern"
regel = { nach = "kirche.ostern", tage = -3 }
"#,
            ),
            EINGEBAUT[0],
        ])
        .unwrap();
        // Der 1.5.2026 ist ein Freitag: der Freitag danach ist der 8.5., nicht der Tag selbst.
        assert_eq!(fest(&k, "probe.do-nach-mai", 2026).as_deref(), Some("2026-05-07"));
        assert_eq!(fest(&k, "probe.fr-nach-mai", 2026).as_deref(), Some("2026-05-08"));
        assert_eq!(fest(&k, "probe.schalttag", 2026), None);
        assert_eq!(fest(&k, "probe.schalttag", 2028).as_deref(), Some("2028-02-29"));
        assert_eq!(fest(&k, "probe.vor-ostern", 2026).as_deref(), Some("2026-04-02"));
        let r = Regel { fest: Some("probe.do-nach-mai".into()), ..Default::default() };
        assert_eq!(k.herkunft(None, &r, 2026), "Probe · Donnerstag danach · erster Donnerstag nach Erster Mai (1.5.2026)");
    }

    /// Je Frist das früheste Vorkommen, dessen Frist heute oder später liegt;
    /// an einem Januartag mit dem Fest des laufenden Jahres, nach dem Fest das nächste.
    #[test]
    fn naechstes_vorkommen_je_frist() {
        let k = Katalog::eingebaut();
        let neujahr = Regel { fest: Some("chinesisch.neujahr".into()), ..Default::default() };
        let a = |t: &str| Abstand::lesen(t).unwrap();
        let n = |abstand: &str, heute: &str| k.naechstes(None, &neujahr, a(abstand), tag(heute)).map(|(d, _)| d.to_string());
        assert_eq!(n("+0 Tage", "2026-01-10").as_deref(), Some("2026-02-17"));
        assert_eq!(n("+0 Tage", "2026-02-17").as_deref(), Some("2026-02-17"), "am Tag selbst noch dieses");
        assert_eq!(n("+0 Tage", "2026-02-18").as_deref(), Some("2027-02-06"), "danach das nächste");
        // +3 Tage: bis zum 20.2. gilt das Neujahr vom 17.2. (sonst würde die Frist nie fällig).
        assert_eq!(n("+3 Tage", "2026-02-20").as_deref(), Some("2026-02-17"));
        assert_eq!(n("+3 Tage", "2026-02-21").as_deref(), Some("2027-02-06"));
        // -3 Tage (drei volle Tage vorher): Frist 13.2.2026; ab dem 14.2. zählt 2027.
        assert_eq!(n("-3 Tage", "2026-02-13").as_deref(), Some("2026-02-17"));
        assert_eq!(n("-3 Tage", "2026-02-14").as_deref(), Some("2027-02-06"));
        assert_eq!(n("+1 Jahr", "2026-01-20").as_deref(), Some("2025-01-29"), "ein Jahr nach dem Neujahr 2025");
        assert_eq!(n("+1 Jahr", "2026-03-01").as_deref(), Some("2026-02-17"));
        assert_eq!(n("+0 Tage", "2100-12-31"), None, "nach 2100 nichts mehr");
        // Regeln, die in vielen Jahren fehlen: weiter vorwärts suchen (Befund der Prüfung).
        let r = |t: &str, abstand: &str, heute: &str| k.naechstes(None, &regel(t), a(abstand), tag(heute)).map(|(d, _)| d.to_string());
        assert_eq!(r("monat = 2\ntag = 29", "+0 Tage", "2029-03-01").as_deref(), Some("2032-02-29"));
        assert_eq!(r("monat = 2\ntag = 29", "+1 Jahr", "2026-06-01").as_deref(), Some("2028-02-29"));
        assert_eq!(r("kalender = \"chinesisch\"\nmonat = 11\ntag = 1\nschaltmonat = true", "+0 Tage", "2026-11-20").as_deref(), Some("2033-12-22"));
        // Ein Jahr Abstand in der Regel: das richtige Vorkommen, auch rückwärts.
        assert_eq!(r("nach = \"kirche.ostern\"\ntage = 366", "+0 Tage", "2026-11-20").as_deref(), Some("2027-04-06"), "Ostern 2026 + 366");
        assert_eq!(r("nach = \"kirche.ostern\"\ntage = -366", "+0 Tage", "2026-11-20").as_deref(), Some("2027-04-16"), "Ostern 2028 (16.4.) − 366");
        let (_, h) = k.naechstes(None, &neujahr, a("+0 Tage"), tag("2026-01-10")).unwrap();
        assert_eq!(h, "Chinesischer Kalender · Chinesisches Neujahr · 1. Tag des 1. Monats im chinesischen Kalender");
        let pfingsten = Regel { fest: Some("kirche.pfingsten".into()), ..Default::default() };
        let (d, h) = k.naechstes(None, &pfingsten, Abstand { zahl: 0, einheit: Einheit::Tag }, tag("2026-11-20")).unwrap();
        assert_eq!((d.to_string().as_str(), h.as_str()), ("2027-05-16", "Kirchenjahr · Pfingstsonntag · 49 Tage nach Ostersonntag (28.3.2027)"));
        let eigene = regel("kalender = \"chinesisch\"\nmonat = 4\ntag = 4\nschaltmonat = true");
        assert_eq!(k.herkunft(None, &eigene, 2028), "Eigene Regel · 4. Tag des Schaltmonats nach dem 4. Monats im chinesischen Kalender");
    }

    #[test]
    fn pruefung_weist_ab() {
        let falsch = |t: &str| regel(t).art().unwrap_err();
        assert!(falsch("").contains("leer"));
        assert!(falsch("monat = 2\ntag = 30").contains("30.2."));
        assert!(falsch("monat = 5").contains("`tag`"));
        assert!(falsch("ostern = \"astronomisch\"").contains("westlich"));
        assert!(regel("nach = \"x\"\ntage = -366").art().is_ok(), "ein Jahr ist erlaubt");
        assert!(falsch("ostern = \"westlich\"\ntag = 1").contains("kein `tag`"));
        assert!(falsch("kalender = \"chinesisch\"\nmonat = 13\ntag = 1").contains("Monat 1–12"));
        let s = falsch("kalender = \"hebraeisch\"\nmonat = 1\ntag = 1");
        assert!(s.contains("chinesisch") && !s.contains("hebraeisch"), "nennt den Wert nicht: {s}");
        assert!(!falsch("ostern = \"geheim\"").contains("geheim"));
        assert!(!falsch("wochentag = \"geheim\"\nnach = \"x\"").contains("geheim"));
        assert!(falsch("nach = \"x\"\ntage = -2147483648").contains("zu weit"), "ohne Überlauf");
        assert!(falsch("nach = \"x\"\ntage = 367").contains("zu weit"));
        assert!(falsch("wochentag = \"Donnerstag\"\nnach = \"x\"").contains("klein"));
        assert!(falsch("wochentag = \"donnerstag\"").contains("`nach`"));
        assert!(falsch("nach = \"x\"").contains("`tage` oder `wochentag`"));
        assert!(falsch("nach = \"x\"\ntage = 4000").contains("zu weit"));
        assert!(falsch("fest = \"kirche.ostern\"\nmonat = 1").contains("kein `monat`"));
        assert!(toml::from_str::<Regel>("himmel = \"vollmond\"").is_err(), "unbekannte Schlüssel weist serde ab");
        let datei = |feste: &str| format!("kennung = \"k\"\ntitel = \"K\"\n{feste}");
        let kreis = datei(
            "[[fest]]\nkennung = \"a\"\ntitel = \"A\"\nregel = { nach = \"b\", tage = 1 }\n[[fest]]\nkennung = \"b\"\ntitel = \"B\"\nregel = { wochentag = \"montag\", nach = \"a\" }\n",
        );
        assert!(Katalog::lesen(&[("k.toml", &kreis)]).unwrap_err().contains("im Kreis"));
        let selbst = datei("[[fest]]\nkennung = \"a\"\ntitel = \"A\"\nregel = { fest = \"k.a\" }\n");
        assert!(Katalog::lesen(&[("k.toml", &selbst)]).unwrap_err().contains("im Kreis"));
        let unbekannt = datei("[[fest]]\nkennung = \"a\"\ntitel = \"A\"\nregel = { nach = \"b\", tage = 1 }\n");
        assert!(Katalog::lesen(&[("k.toml", &unbekannt)]).unwrap_err().contains("„b“ gibt es nicht"));
        let doppelt = datei(
            "[[fest]]\nkennung = \"a\"\ntitel = \"A\"\nregel = { monat = 1, tag = 1 }\n[[fest]]\nkennung = \"a\"\ntitel = \"A\"\nregel = { monat = 1, tag = 2 }\n",
        );
        assert!(Katalog::lesen(&[("k.toml", &doppelt)]).unwrap_err().contains("doppelte"));
        let k = Katalog::eingebaut();
        assert!(k.ereignis_regel_pruefen(&regel("fest = \"ostern\"")).unwrap_err().contains("kirche.ostern"));
        let s = k.ereignis_regel_pruefen(&regel("fest = \"kirche.ostara\"")).unwrap_err();
        assert!(s.contains("nicht gibt") && !s.contains("ostara"), "nennt den Verweis nicht: {s}");
        assert!(k.ereignis_regel_pruefen(&regel("nach = \"chinesisch.neujahr\"\ntage = -3")).is_ok());
        assert!(k.ereignis_regel_pruefen(&regel("ostern = \"orthodox\"")).is_ok());
    }
}
