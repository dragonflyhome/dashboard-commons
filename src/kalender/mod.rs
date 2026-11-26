//! Kalender, Belegstufe 1: Feste aus benannten Regelsätzen (Kalendern), mehrere
//! zugleich, mit Herkunft je Termin. Die Kalender liegen als Vorlagen unter
//! `vorlagen/kalender/` und sind eingebaut ([`Katalog::eingebaut`]).
//!
//! Eine Regel hat genau eine Art; welche, bestimmt ein Schlüssel:
//!
//! - festes Datum: `monat`, `tag` (30.4. in jedem Jahr; den 29.2. nur in Schaltjahren),
//! - Abstand: `nach` (ein Fest) und `tage` (mit Vorzeichen), etwa Pfingsten = Ostern + 49,
//! - Wochentag nach Bezug: `wochentag` und `nach`, der erste solche Tag nach dem
//!   Bezug (der Bezugstag selbst zählt nicht),
//! - Ostersonntag: `ostern = "westlich"`, `"orthodox"` (julianische Rechnung,
//!   als heutiges Datum) oder `"astronomisch"` (erster Sonntag nach dem ersten
//!   Vollmond nach der Gleiche im März, alles nach dem tatsächlichen Himmel) oder
//!   `"gleiche-fruehling"` (ebenso, nach der Frühlingsgleiche des Orts: im Süden im September),
//! - chinesischer Kalender: `kalender = "chinesisch"`, `monat`, `tag`, wahlweise
//!   `schaltmonat = true` (Tage nach Pekinger Zeit),
//! - Sonnentag: `himmel = "sonnenwende-dezember"`, `"gleiche-maerz"`,
//!   `"sonnenwende-juni"` oder `"gleiche-september"` (der Tag des Ereignisses in Ortszeit),
//!   oder nach der Rolle am Ort `"wende-winter"`, `"gleiche-fruehling"`, `"wende-sommer"`,
//!   `"gleiche-herbst"` (auf der Südhalbkugel die andere Jahreshälfte),
//! - Mondphase: `himmel = "neumond"`, `"erstes-viertel"`, `"vollmond"` oder
//!   `"letztes-viertel"` mit `nach` und wahlweise `nummer` (1–13, Vorgabe 1):
//!   die n-te solche Phase nach dem Tag des Bezugs (am Tag selbst zählt nicht),
//! - Mondmonat (Neumond bis Neumond; der Tag ist sein Anfang):
//!   `mondmonat = { enthaelt = "<wende oder gleiche>" }`, der Monat, der das Ereignis
//!   enthält, oder `mondmonat = { vollmond = n }` (1–13), der Monat um den n-ten
//!   Vollmond nach dem kürzesten Tag (ein Vollmond am Tag der Wende zählt nicht),
//! - Verweis: `fest = "<kalender>.<fest>"`, nur an Ereignissen in `fristen.toml`.
//!
//! `nach` nennt in einer Kalenderdatei ein Fest desselben Kalenders (`ostern`)
//! oder eines anderen (`kirche.ostern`); an einem Ereignis immer mit Kalender.
//! Ein Fest gehört zu einem Jahr: Abstand und Wochentag rechnen vom Bezug
//! desselben Jahres; der Mondmonat um den n-ten Vollmond zählt ab der Wende im
//! Dezember davor (Festjahr von Yule zu Yule). Gerechnet wird für die Jahre
//! 1912 bis 2100 (für das chinesische Neujahr gelten die heutigen Regeln seit
//! 1912, die Tabellen des Hong Kong Observatory reichen bis 2100); außerhalb
//! gibt es kein Datum.
//!
//! Ein Kalender kann Jahreszeiten tragen ([`Jahreszeit`]): Dann benennt
//! [`Katalog::mond`] den laufenden Mondmonat. Feste und Monate tragen
//! überlieferte Namen mit Herkunft und Belegstufe ([`Name`]); die Namen sind Daten
//! der Vorlage, nicht Code.
//!
//! Später (nicht in Belegstufe 2): Orte und Südhalbkugel aus dem Ort, Tages- und
//! Gebetszeiten, Zählkalender. Neue Arten kommen als weiterer Schlüssel dazu.

pub mod gebete;
pub mod himmel;
pub mod mond;
pub mod orte;
pub mod ortsliste;
pub mod tag;
pub mod zone;

use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock};

use calendrical_calculations::chinese_based::{self, Chinese, YearBounds};
use calendrical_calculations::{gregorian, julian, rata_die::RataDie};
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::vertrag::daten::ist_kennung;
use crate::vertrag::fristen::Abstand;
use mond::{Himmelstag, Ort, Phase, Sonnenereignis};

/// Das erste und das letzte Jahr, für das gerechnet wird.
pub const ERSTES_JAHR: i32 = 1912;
pub const LETZTES_JAHR: i32 = 2100;

/// So weit darf ein Abstand in einer Regel höchstens reichen: ein Jahr. So liegt
/// das Fest eines Jahres nah bei diesem Jahr (Annahme von [`Katalog::naechstes`]);
/// weiter weg gehört es als Abstand an die Frist.
pub const TAGE_HOECHSTENS: i32 = 366;

/// Die eingebauten Kalender (Dateiname, Inhalt).
const EINGEBAUT: [(&str, &str); 4] = [
    ("vorlagen/kalender/kirche.toml", include_str!("../../vorlagen/kalender/kirche.toml")),
    ("vorlagen/kalender/orthodox.toml", include_str!("../../vorlagen/kalender/orthodox.toml")),
    ("vorlagen/kalender/chinesisch.toml", include_str!("../../vorlagen/kalender/chinesisch.toml")),
    ("vorlagen/kalender/jahreskreis.toml", include_str!("../../vorlagen/kalender/jahreskreis.toml")),
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
    /// `westlich`, `orthodox`, `astronomisch` oder `gleiche-fruehling`.
    #[serde(default)]
    pub ostern: Option<String>,
    /// Ein anderer Kalender; heute nur `chinesisch`.
    #[serde(default)]
    pub kalender: Option<String>,
    /// Nur mit `kalender`: der Schaltmonat nach dem Monat `monat`.
    #[serde(default)]
    pub schaltmonat: Option<bool>,
    /// Ein Sonnentag (`sonnenwende-dezember` …) oder eine Mondphase (`vollmond` …).
    #[serde(default)]
    pub himmel: Option<String>,
    /// Nur mit einer Mondphase: die wievielte nach dem Bezug (1–13).
    #[serde(default)]
    pub nummer: Option<u8>,
    /// Ein Mondmonat, Neumond bis Neumond.
    #[serde(default)]
    pub mondmonat: Option<MondmonatRegel>,
}

/// Welcher Mondmonat: genau eines von beiden.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct MondmonatRegel {
    /// Der Monat, der diese Wende oder Gleiche enthält (`sonnenwende-dezember` …).
    #[serde(default)]
    pub enthaelt: Option<String>,
    /// Der Monat um den n-ten Vollmond nach dem kürzesten Tag (1–13).
    #[serde(default)]
    pub vollmond: Option<u8>,
}

/// Wie Ostern gerechnet wird.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsterRechnung {
    Westlich,
    Orthodox,
    Astronomisch,
    /// Wie `Astronomisch`, aber nach der Frühlingsgleiche des Rechenorts (im Süden im September).
    GleicheFruehling,
}

/// Die Gleiche, nach der ein Ostern nach dem Mond rechnet.
fn ostern_gleiche(o: OsterRechnung) -> Himmelstag {
    match o {
        OsterRechnung::GleicheFruehling => Himmelstag::Rolle(Sonnenereignis::GleicheMaerz),
        _ => Himmelstag::Monat(Sonnenereignis::GleicheMaerz),
    }
}

/// Die Art einer geprüften Regel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Art<'a> {
    Datum { monat: u32, tag: u32 },
    Abstand { nach: &'a str, tage: i32 },
    WochentagNach { wochentag: Weekday, nach: &'a str },
    Ostern(OsterRechnung),
    Chinesisch { monat: u8, tag: u8, schaltmonat: bool },
    Sonne(Himmelstag),
    Phase { phase: Phase, nummer: u8, nach: &'a str },
    MondmonatMit(Himmelstag),
    MondmonatVollmond(u8),
    Fest(&'a str),
}

impl Art<'_> {
    /// Ein Mondmonat (sein Tag ist der Anfang, dazu gibt es einen [`Zeitraum`]).
    pub fn ist_mondmonat(&self) -> bool {
        matches!(self, Art::MondmonatMit(_) | Art::MondmonatVollmond(_))
    }
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

const ARTEN: &str = "`monat` und `tag`; `nach` und `tage`; `wochentag` und `nach`; `ostern`; `kalender` mit `monat` und `tag`; `himmel`; `mondmonat`; `fest`";

const SATZ_HIMMEL: &str = "Diesen Wert für `himmel` gibt es nicht. Erlaubt sind die Sonnentage `\"sonnenwende-dezember\"`, `\"gleiche-maerz\"`, `\"sonnenwende-juni\"`, `\"gleiche-september\"`, nach ihrer Rolle am Ort `\"wende-winter\"`, `\"gleiche-fruehling\"`, `\"wende-sommer\"`, `\"gleiche-herbst\"`, und die Mondphasen `\"neumond\"`, `\"erstes-viertel\"`, `\"vollmond\"`, `\"letztes-viertel\"`.";

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
            ("himmel", self.himmel.is_some()),
            ("nummer", self.nummer.is_some()),
            ("mondmonat", self.mondmonat.is_some()),
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
                "astronomisch" => Ok(Art::Ostern(OsterRechnung::Astronomisch)),
                "gleiche-fruehling" => Ok(Art::Ostern(OsterRechnung::GleicheFruehling)),
                // Die Sätze nennen den eingegebenen Wert nicht (welche Kalender jemand nutzt, ist persönlich).
                _ => Err("Diesen Wert für `ostern` gibt es nicht. Erlaubt sind `\"westlich\"`, `\"orthodox\"`, `\"astronomisch\"` (nach der Gleiche im März) und `\"gleiche-fruehling\"` (nach der Frühlingsgleiche des Orts).".into()),
            };
        }
        if let Some(h) = &self.himmel {
            if let Some(s) = Himmelstag::aus(h) {
                nur("einer Wende oder Gleiche in `himmel`", &["himmel"])?;
                return Ok(Art::Sonne(s));
            }
            let phase = Phase::aus(h).ok_or_else(|| SATZ_HIMMEL.to_string())?;
            nur("einer Mondphase in `himmel`", &["himmel", "nummer", "nach"])?;
            pflicht("nach", self.nach.is_some(), "einer Mondphase in `himmel`")?;
            let nummer = self.nummer.unwrap_or(1);
            if !(1..=13).contains(&nummer) {
                return Err("`nummer` zählt die Mondphasen nach dem Bezug; erlaubt sind 1 bis 13.".into());
            }
            return Ok(Art::Phase { phase, nummer, nach: self.nach.as_deref().unwrap_or_default() });
        }
        if let Some(m) = &self.mondmonat {
            nur("`mondmonat`", &["mondmonat"])?;
            return match (&m.enthaelt, m.vollmond) {
                (Some(e), None) => Himmelstag::aus(e)
                    .map(Art::MondmonatMit)
                    .ok_or_else(|| "In `mondmonat` nennt `enthaelt` eine Wende oder Gleiche: `\"sonnenwende-dezember\"`, `\"gleiche-maerz\"`, `\"sonnenwende-juni\"`, `\"gleiche-september\"` oder nach der Rolle am Ort `\"wende-winter\"`, `\"gleiche-fruehling\"`, `\"wende-sommer\"`, `\"gleiche-herbst\"`.".into()),
                (None, Some(n)) if (1..=13).contains(&n) => Ok(Art::MondmonatVollmond(n)),
                (None, Some(_)) => Err("`vollmond` in `mondmonat` zählt die Vollmonde nach dem kürzesten Tag; erlaubt sind 1 bis 13.".into()),
                _ => Err("`mondmonat` braucht genau eines: `enthaelt` (eine Wende oder Gleiche) oder `vollmond` (1 bis 13).".into()),
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
                return Err("Diesen Tag gibt es in diesem Kalender nicht. Erlaubt sind Monat 1–12 und Tag 1–30.".into());
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
                return Err(format!("Der Abstand in `tage` ist zu weit. Erlaubt sind höchstens {TAGE_HOECHSTENS} Tage davor oder danach."));
            }
            return Ok(Art::Abstand { nach: self.nach.as_deref().unwrap_or_default(), tage: t });
        }
        if self.monat.is_some() || self.tag.is_some() {
            nur("`monat` und `tag`", &["monat", "tag"])?;
            let (monat, tag) = (u32::from(self.monat.unwrap_or(0)), u32::from(self.tag.unwrap_or(0)));
            // 2000 ist ein Schaltjahr: Der 29.2. ist erlaubt und gilt dann nur in Schaltjahren.
            if self.monat.is_none() || self.tag.is_none() || NaiveDate::from_ymd_opt(2000, monat, tag).is_none() {
                return Err("Ein festes Datum braucht `monat` und `tag` als echten Tag, etwa `monat = 4, tag = 30`; diesen Tag gibt es nicht.".into());
            }
            return Ok(Art::Datum { monat, tag });
        }
        if self.nummer.is_some() {
            return Err("`nummer` gehört zu einer Mondphase in `himmel`, mit `nach`.".into());
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

/// Wie sicher ein überlieferter Name ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Belegstufe {
    /// In einer alten Quelle belegt.
    Belegt,
    /// Sprachwissenschaftlich rekonstruiert.
    Erschlossen,
    /// Neuzeitlich, etwa ein Festname aus dem 20. Jahrhundert.
    Neuzeitlich,
}

/// Ein überlieferter Name mit Herkunft, für ein Fest oder einen Mondmonat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Name {
    pub name: String,
    /// Sprache, etwa `altenglisch`.
    pub sprache: String,
    /// Quelle, etwa „Beda, De temporum ratione 15, um 725“; der Teil vor dem
    /// ersten Komma steht in der kurzen Form.
    pub quelle: String,
    /// Bedeutung oder Deutung, wenn bekannt.
    #[serde(default)]
    pub bedeutung: Option<String>,
    pub stufe: Belegstufe,
    /// Zuordnung oder Deutung nur aus allgemeinen Nachschlagewerken, nicht gegen eine Fachausgabe geprüft.
    #[serde(default)]
    pub ungeprueft: bool,
}

impl Name {
    /// Kurz, für eine Zeile: „Blōtmōnaþ (altenglisch, Beda)“; bei unsicherer Belegstufe mit ihr.
    pub fn kurz(&self) -> String {
        let quelle = self.quelle.split(',').next().unwrap_or_default().trim();
        let stufe = match self.stufe {
            Belegstufe::Belegt => "",
            Belegstufe::Erschlossen => ", erschlossen",
            Belegstufe::Neuzeitlich => ", neuzeitlich",
        };
        format!("{} ({}, {quelle}{stufe})", self.name, self.sprache)
    }
}

/// Ein Fest eines Kalenders.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fest {
    pub kennung: String,
    pub titel: String,
    pub regel: Regel,
    /// Überlieferte Namen, der erste steht in der Herkunft.
    #[serde(default)]
    pub namen: Vec<Name>,
}

/// Eine Jahreszeit eines Kalenders mit Mondmonaten: Sie beginnt mit dem
/// Neumond, der den Monat `nach` beendet, und zählt ihre Monate bis zum nächsten
/// solchen Monat durch („1. Wintervollmond“). Ihr Sonnenmonat (ein Fest mit
/// `mondmonat = { enthaelt = … }`) trägt seine eigenen Namen; die übrigen
/// Monate tragen die Namen nach ihrer Lage zu ihm.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Jahreszeit {
    /// Ein Fest desselben Kalenders mit `mondmonat = { vollmond = n }`.
    pub nach: String,
    /// Das Wort hinter der Nummer, etwa `Wintervollmond`.
    pub vollmond: String,
    /// Überlieferte Namen der Monate vor dem Sonnenmonat.
    #[serde(default)]
    pub davor: Vec<Name>,
    /// Überlieferte Namen der Monate nach dem Sonnenmonat.
    #[serde(default)]
    pub danach: Vec<Name>,
}

/// Ein Kalender: ein benannter Satz von Festen.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Kalender {
    pub kennung: String,
    pub titel: String,
    #[serde(default)]
    pub fest: Vec<Fest>,
    #[serde(default)]
    pub jahreszeit: Vec<Jahreszeit>,
}

/// Ein Mondmonat als Zeitraum: die Tage (JJJJ-MM-TT, Ortszeit) von Neumond,
/// Vollmond und dem Neumond, mit dem er endet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Zeitraum {
    pub anfang: String,
    pub vollmond: String,
    pub ende: String,
}

/// Der laufende Mondmonat eines Kalenders mit Jahreszeiten: kein Fest und
/// keine Frist, nur zum Zeigen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct Mond {
    /// Titel des Kalenders.
    pub kalender: String,
    /// Der wievielte Monat des Festjahrs (1 = der Monat mit dem kürzesten Tag);
    /// zwischen den Wendekreisen keiner.
    pub nummer: Option<u8>,
    /// Sprechender Name, etwa „2. Wintervollmond · Yule-Mond“ oder „Samhain“;
    /// zwischen den Wendekreisen nur „Mondmonat“.
    pub name: String,
    /// Überlieferte Namen, der erste gehört neben den Namen; zwischen den Wendekreisen keine.
    pub namen: Option<Vec<Name>>,
    pub zeitraum: Zeitraum,
    /// Die Phase am Tag: „Neumond“, „zunehmend“, „erstes Viertel“, „Vollmond“, „letztes Viertel“, „abnehmend“.
    pub phase: String,
    /// Warum Namen fehlen (zwischen den Wendekreisen).
    pub hinweis: Option<String>,
}

/// Wie ein Fest in Listen erscheint ([`Katalog::anzeige`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Anzeige {
    /// Mit Titel und Herkunft des Fests.
    Normal,
    /// Nur als Sonnentag, ohne Namen (zwischen den Wendekreisen).
    Neutral { titel: String, herkunft: String },
    /// Gar nicht (Monate und andere Feste nach den Jahreszeiten zwischen den Wendekreisen).
    Entfaellt,
}

/// Alle bekannten Kalender, geprüft: Kennungen einmalig, Regeln gültig,
/// Verweise bekannt, keine Kreise; dazu der Ort, für den gerechnet wird.
#[derive(Debug, Clone)]
pub struct Katalog {
    kalender: Arc<Vec<Kalender>>,
    ort: Ort,
}

fn tag_text(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn kurz_datum(d: NaiveDate) -> String {
    d.format("%-d.%-m.%Y").to_string()
}

/// Satz für eine Regel, die in keinem Jahr einen Tag ergibt.
const SATZ_KOMMT_NIE_VOR: &str = "Diese Regel ergibt in keinem Jahr von 1912 bis 2100 einen Tag, etwa ein Schaltmonat, den es nie gibt, oder der 30. Tag eines Monats, der immer 29 Tage hat.";

/// Satz für ein Fest nach den Jahreszeiten an einem Ort zwischen den Wendekreisen.
pub const SATZ_WENDEKREIS: &str = "Gilt an diesem Ort nicht: Er liegt zwischen den Wendekreisen; dort folgen die Jahreszeiten dem Regen, nicht der Sonne. Feste und Monate eines Kalenders nach den Jahreszeiten gibt es dort nicht, nur die Sonnentage und den Mond.";

/// Hinweis zum Mond an einem Ort zwischen den Wendekreisen (Laientext, ohne Zahlen).
pub const HINWEIS_WENDEKREIS: &str = "Dieser Ort liegt nahe am Äquator, zwischen den Wendekreisen. Dort sind die Tage das ganze Jahr fast gleich lang, und die Jahreszeiten richten sich nach Regen und Trockenheit, nicht nach der Sonne. Darum stehen hier nur die Sonnentage und die Mondphasen, ohne Fest- und Monatsnamen.";

/// Höchstens so tief folgt die Rechnung Verweisen (Kreise weist die Prüfung vorher ab).
const TIEFE_HOECHSTENS: usize = 16;

impl Katalog {
    /// Die eingebauten Kalender. Sie sind beim Bauen geprüft (Test
    /// `eingebaute_kalender_sind_gueltig`); schlägt das Lesen doch fehl, ist der Katalog leer.
    pub fn eingebaut() -> &'static Katalog {
        static K: OnceLock<Katalog> = OnceLock::new();
        K.get_or_init(|| Katalog::lesen(&EINGEBAUT).unwrap_or(Katalog { kalender: Arc::default(), ort: Ort::windows() }))
    }

    /// Dieselben Kalender, gerechnet für einen anderen Ort (geteilt, nicht kopiert).
    pub fn mit_ort(&self, ort: Ort) -> Katalog {
        Katalog { kalender: Arc::clone(&self.kalender), ort }
    }

    /// Der Ort, für den gerechnet wird.
    pub fn ort(&self) -> &Ort {
        &self.ort
    }

    /// Wie ein Fest in Listen erscheint. Zwischen den Wendekreisen stehen aus
    /// Kalendern nach den Jahreszeiten nur die Sonnentage, ohne Fest- und
    /// Monatsnamen (Frage 14); alle anderen Feste dort entfallen.
    pub fn anzeige(&self, k: &Kalender, f: &Fest, jahr: i32) -> Anzeige {
        if !(self.ort.tropen() && !k.jahreszeit.is_empty()) {
            return Anzeige::Normal;
        }
        match f.regel.art() {
            Ok(Art::Sonne(s)) => {
                let s = self.sonne(s);
                let titel = s.name().to_string();
                Anzeige::Neutral { herkunft: format!("{} · {} am {}", k.titel, s.name(), self.zeitpunkt_satz(s.zeitpunkt(jahr))), titel }
            }
            _ => Anzeige::Entfaellt,
        }
    }

    /// „Längster Tag · 16 h 47 min“, wenn `d` am Ort (mit Lage) der Tag der Sommersonnenwende ist.
    pub fn laengster_tag(&self, d: NaiveDate) -> Option<String> {
        self.ort.lage?;
        let sommer = match self.ort.halbkugel {
            mond::Halbkugel::Nord => Sonnenereignis::SonnenwendeJuni,
            mond::Halbkugel::Sued => Sonnenereignis::SonnenwendeDezember,
        };
        (self.ort.tag(sommer.zeitpunkt(d.year())?)? == d).then(|| {
            let z = tag::am_tag(d, &self.ort);
            let wert = match z.as_ref().map(|z| (z.aufgang, z.untergang)) {
                Some((tag::Sonnenzeit::BleibtDarueber, tag::Sonnenzeit::BleibtDarueber)) => "Sonne geht nicht unter".to_string(),
                _ => z.and_then(|z| z.taglaenge()).map_or("keine Angabe".into(), tag::dauer_text),
            };
            format!("Längster Tag · {wert}")
        })
    }

    /// Ein Satz, wenn der Ort zwischen den Wendekreisen liegt und die Regel
    /// (auch über Verweise) ein Fest eines Kalenders nach den Jahreszeiten nennt:
    /// Dort gibt es diese Feste und Monate nicht (Frage 14). Ohne Zahlen.
    pub fn wendekreis_pruefen(&self, regel: &Regel) -> Result<(), String> {
        if !self.ort.tropen() {
            return Ok(());
        }
        let mut offen: Vec<(Option<String>, &Regel)> = vec![(None, regel)];
        for _ in 0..=TIEFE_HOECHSTENS * 4 {
            let Some((im, r)) = offen.pop() else { return Ok(()) };
            for v in r.verweise() {
                if let Some((k, f)) = self.finden(im.as_deref(), v) {
                    if !k.jahreszeit.is_empty() {
                        // Ohne Namen des Kalenders: Welche Kalender jemand nutzt, ist persönlich.
                        return Err(SATZ_WENDEKREIS.into());
                    }
                    offen.push((Some(k.kennung.clone()), &f.regel));
                }
            }
        }
        Ok(())
    }

    /// Liest und prüft Kalenderdateien (Name, Inhalt), gerechnet für die
    /// Nordhalbkugel in der Ortszeit von Windows. Fehler nennen Datei und Fest.
    pub fn lesen(dateien: &[(&str, &str)]) -> Result<Katalog, String> {
        Self::lesen_fuer(dateien, Ort::windows())
    }

    /// Die eingebauten Kalender, gerechnet wie in Berlin (Mitteleuropa mit Sommerzeit), für Tests
    /// unabhängig von der Ortszeit des Rechners.
    #[cfg(test)]
    pub(crate) fn berlin() -> Katalog {
        Self::lesen_fuer(&EINGEBAUT, Ort { halbkugel: mond::Halbkugel::Nord, zeit: mond::Zeit::Test(mond::mitteleuropa), lage: None })
            .unwrap_or(Katalog { kalender: Arc::default(), ort: Ort::windows() })
    }

    /// Wie [`Katalog::lesen`], für einen anderen Ort (Tests; mit Belegstufe 3 der Ort des Inhabers).
    pub fn lesen_fuer(dateien: &[(&str, &str)], ort: Ort) -> Result<Katalog, String> {
        let mut kalender = Vec::new();
        for (name, text) in dateien {
            kalender.push(toml::from_str::<Kalender>(text).map_err(|e| format!("{name}: {}", e.message()))?);
        }
        let k = Katalog { kalender: Arc::new(kalender), ort };
        k.pruefen()?;
        Ok(k)
    }

    fn pruefen(&self) -> Result<(), String> {
        let mut namen = BTreeSet::new();
        for k in self.kalender.iter() {
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
            let mut nach = BTreeSet::new();
            for j in &k.jahreszeit {
                let ist_mondmonat = k.fest.iter().find(|f| f.kennung == j.nach).is_some_and(|f| matches!(f.regel.art(), Ok(Art::MondmonatVollmond(_))));
                if !ist_mondmonat || !nach.insert(j.nach.as_str()) || j.vollmond.trim().is_empty() {
                    return Err(format!(
                        "Im Kalender „{}“ nennt eine Jahreszeit in `nach` kein Fest dieses Kalenders mit `mondmonat = {{ vollmond = … }}`, eines doppelt oder hat kein Wort in `vollmond`.",
                        k.kennung
                    ));
                }
            }
        }
        for k in self.kalender.iter() {
            for f in &k.fest {
                self.ohne_kreis(&k.kennung, f, &mut Vec::new())?;
            }
        }
        for k in self.kalender.iter() {
            if let Some(f) = k.fest.iter().find(|f| !self.kommt_vor(Some(&k.kennung), &f.regel)) {
                return Err(format!("Im Kalender „{}“, Fest „{}“: {SATZ_KOMMT_NIE_VOR}", k.kennung, f.kennung));
            }
        }
        Ok(())
    }

    /// Ergibt die Regel in einem der Jahre, für die gerechnet wird, einen Tag?
    /// (Nicht bei unmöglichen Schaltmonaten oder dem 30. eines immer kurzen Monats.)
    fn kommt_vor(&self, im: Option<&str>, regel: &Regel) -> bool {
        (ERSTES_JAHR..=LETZTES_JAHR).any(|j| self.im_jahr(im, regel, j).is_some())
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
        if !self.kommt_vor(None, regel) {
            return Err(SATZ_KOMMT_NIE_VOR.into());
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
            Art::Ostern(o @ (OsterRechnung::Astronomisch | OsterRechnung::GleicheFruehling)) => self.ostern_astronomisch(o, jahr).map(|(o, _)| o),
            Art::Chinesisch { monat, tag, schaltmonat } => chinesisch(jahr, monat, tag, schaltmonat),
            Art::Sonne(s) => self.ort.tag(self.sonne(s).zeitpunkt(jahr)?),
            Art::Phase { phase, nummer, nach } => {
                // Ab dem Beginn des Tags nach dem Bezug: Eine Phase am Bezugstag zählt nicht.
                let mut t = himmel::phase_ab(phase.grad(), self.ort.beginn_ab(bezug(nach)?.succ_opt()?)?);
                for _ in 1..nummer {
                    t = himmel::phase_ab(phase.grad(), t + 1.0);
                }
                self.ort.tag(t)
            }
            Art::MondmonatMit(_) | Art::MondmonatVollmond(_) => self.ort.tag(mond::anfang(self.mondmonat(regel, jahr)?)),
            Art::Fest(v) => bezug(v),
        }
    }

    /// Das Sonnenereignis eines Sonnentags am Rechenort: Eine Rolle (`wende-winter` …)
    /// tauscht auf der Südhalbkugel die Jahreshälfte, ein Monat gilt wörtlich.
    fn sonne(&self, h: Himmelstag) -> Sonnenereignis {
        h.am(self.ort.halbkugel)
    }

    /// Die Nummer des Mondmonats einer Regel mit `mondmonat` im Jahr `jahr`.
    fn mondmonat(&self, regel: &Regel, jahr: i32) -> Option<i32> {
        match regel.art().ok()? {
            Art::MondmonatMit(s) => Some(mond::monat_bei(self.sonne(s).zeitpunkt(jahr)?)),
            Art::MondmonatVollmond(n) => self.ort.monat_um_vollmond(jahr, n),
            _ => None,
        }
    }

    /// Ostern nach dem Mond: der erste Sonntag nach dem Tag des ersten
    /// Vollmonds nach dem Zeitpunkt der Frühlings-Gleiche (im Jahreskreis der
    /// Südhalbkugel im September, sonst im März); dazu der Tag der Gleiche.
    fn ostern_astronomisch(&self, o: OsterRechnung, jahr: i32) -> Option<(NaiveDate, NaiveDate)> {
        let gleiche = self.sonne(ostern_gleiche(o)).zeitpunkt(jahr)?;
        let vollmond = self.ort.tag(himmel::phase_ab(180.0, gleiche))?;
        let bis_sonntag = 7 - i64::from(vollmond.weekday().num_days_from_sunday());
        Some((vollmond.checked_add_signed(Duration::days(bis_sonntag))?, self.ort.tag(gleiche)?))
    }

    /// Der Zeitraum des Mondmonats, der am Tag `anfang` beginnt (für eine Regel,
    /// die auf einen Mondmonat führt, auch über `fest`); sonst `None`.
    pub fn zeitraum(&self, im: Option<&str>, regel: &Regel, anfang: NaiveDate) -> Option<Zeitraum> {
        let mut r = regel;
        let mut im = im.map(str::to_string);
        for _ in 0..=TIEFE_HOECHSTENS {
            match r.art().ok()? {
                Art::Fest(v) => {
                    let (k, f) = self.finden(im.as_deref(), v)?;
                    im = Some(k.kennung.clone());
                    r = &f.regel;
                }
                a if a.ist_mondmonat() => return self.zeitraum_ab(anfang),
                _ => return None,
            }
        }
        None
    }

    fn zeitraum_ab(&self, anfang: NaiveDate) -> Option<Zeitraum> {
        let k = self.ort.monat_ab_tag(anfang);
        Some(Zeitraum {
            anfang: tag_text(self.ort.tag(mond::anfang(k))?),
            vollmond: tag_text(self.ort.tag(mond::vollmond(k))?),
            ende: tag_text(self.ort.tag(mond::anfang(k + 1))?),
        })
    }

    /// Der laufende Mondmonat am Tag `heute` in einem Kalender mit Jahreszeiten:
    /// Nummer im Festjahr, sprechender Name, überlieferte Namen und Zeitraum.
    pub fn mond(&self, kalender: &str, heute: NaiveDate) -> Option<Mond> {
        let kal = self.kalender.iter().find(|k| k.kennung == kalender && !k.jahreszeit.is_empty())?;
        let jahr = heute.year();
        if !(ERSTES_JAHR..=LETZTES_JAHR).contains(&jahr) {
            return None;
        }
        // Wie überall: Der Tag mit dem Neumond gehört schon zum neuen Monat.
        let m = self.ort.monat_ab_tag(heute);
        let zeitraum = Zeitraum {
            anfang: tag_text(self.ort.tag(mond::anfang(m))?),
            vollmond: tag_text(self.ort.tag(mond::vollmond(m))?),
            ende: tag_text(self.ort.tag(mond::anfang(m + 1))?),
        };
        if self.ort.tropen() {
            let hinweis = Some(HINWEIS_WENDEKREIS.to_string());
            return Some(Mond {
                kalender: kal.titel.clone(),
                nummer: None,
                name: "Mondmonat".into(),
                namen: None,
                zeitraum,
                phase: self.phase_am(m, heute),
                hinweis,
            });
        }
        // Die Monate, mit denen die Jahreszeiten beginnen, um das laufende Jahr.
        let mut anker: Vec<(i32, &Jahreszeit, &Fest)> = Vec::new();
        for j in &kal.jahreszeit {
            let f = kal.fest.iter().find(|f| f.kennung == j.nach)?;
            for y in jahr - 1..=jahr + 1 {
                anker.push((self.mondmonat(&f.regel, y)?, j, f));
            }
        }
        let (start, jz, fest) = anker.iter().filter(|(k, _, _)| *k <= m).max_by_key(|(k, _, _)| *k).copied()?;
        let ende = anker.iter().map(|(k, _, _)| *k).filter(|k| *k > start).min()?;
        // Der Sonnenmonat dieser Jahreszeit (der Monat mit ihrem Sonnentag).
        let mut sonnenmonat = None;
        for f in kal.fest.iter().filter(|f| matches!(f.regel.art(), Ok(Art::MondmonatMit(_)))) {
            for y in jahr - 1..=jahr + 1 {
                if let Some(k) = self.mondmonat(&f.regel, y).filter(|k| (start..ende).contains(k)) {
                    sonnenmonat = Some((k, f));
                }
            }
        }
        let (mut name, namen) = if start == m {
            (fest.titel.clone(), fest.namen.clone())
        } else {
            let n = m - start;
            let namen = match sonnenmonat {
                Some((s, f)) if s == m => f.namen.clone(),
                Some((s, _)) if m > s => jz.danach.clone(),
                _ => jz.davor.clone(),
            };
            (format!("{n}. {}", jz.vollmond), namen)
        };
        // Der Sonnenmonat trägt seinen Namen dazu („2. Wintervollmond · Yule-Mond“).
        if let Some((s, f)) = sonnenmonat
            && s == m
        {
            name = format!("{name} · {}", f.titel);
        }
        // Das Festjahr beginnt mit dem Monat, der den kürzesten Tag enthält.
        let anfang_jahr = |y: i32| self.ort.kuerzester_tag(y).map(mond::monat_bei);
        let erster = (jahr - 1..=jahr + 1).rev().filter_map(anfang_jahr).find(|a| *a <= m)?;
        Some(Mond {
            kalender: kal.titel.clone(),
            nummer: Some(u8::try_from(m - erster + 1).ok()?),
            name,
            namen: Some(namen),
            zeitraum,
            phase: self.phase_am(m, heute),
            hinweis: None,
        })
    }

    /// Die Phase des Mondmonats `m` am Tag `heute` in Ortszeit.
    fn phase_am(&self, m: i32, heute: NaiveDate) -> String {
        let neu = mond::anfang(m);
        let tag = |grad: f64| self.ort.tag(himmel::phase_ab(grad, neu));
        for (grad, name) in [(0.0, "Neumond"), (90.0, "erstes Viertel"), (180.0, "Vollmond"), (270.0, "letztes Viertel")] {
            if tag(grad) == Some(heute) {
                return name.into();
            }
        }
        if tag(180.0).is_some_and(|v| heute < v) { "zunehmend".into() } else { "abnehmend".into() }
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
            let name = f.namen.first().map(|n| format!(" · überliefert: {}", n.kurz())).unwrap_or_default();
            return format!("{} · {} · {}{name}", k.titel, f.titel, self.regel_satz(Some(&k.kennung), &f.regel, jahr));
        }
        format!("Eigene Regel · {}", self.regel_satz(im, regel, jahr))
    }

    /// „21.12.2026 um 21:50 Uhr Ortszeit“.
    fn zeitpunkt_satz(&self, t: Option<f64>) -> String {
        t.and_then(|t| self.ort.ortszeit(t)).map(|z| z.format("%-d.%-m.%Y um %H:%M Uhr Ortszeit").to_string()).unwrap_or_default()
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
            Ok(Art::Ostern(o @ (OsterRechnung::Astronomisch | OsterRechnung::GleicheFruehling))) => {
                let gleiche = self.ostern_astronomisch(o, jahr).map(|(_, g)| format!(" ({})", kurz_datum(g))).unwrap_or_default();
                let name = self.sonne(ostern_gleiche(o)).name();
                format!("Ostersonntag nach dem Mond: erster Sonntag nach dem ersten Vollmond nach der {name}{gleiche}")
            }
            Ok(Art::Sonne(s)) => {
                let s = self.sonne(s);
                format!("{} am {}", s.name(), self.zeitpunkt_satz(s.zeitpunkt(jahr)))
            }
            Ok(Art::Phase { phase, nummer, nach }) => format!("{nummer}. {} nach {}", phase.name(), bezug(nach)),
            Ok(Art::MondmonatMit(s)) => {
                let s = self.sonne(s);
                format!("Mondmonat (Neumond bis Neumond) mit der {} am {}", s.name(), self.zeitpunkt_satz(s.zeitpunkt(jahr)))
            }
            Ok(Art::MondmonatVollmond(n)) => {
                let wende = self.ort.kuerzester_tag(jahr).and_then(|t| self.ort.tag(t)).map(|d| format!(" ({})", kurz_datum(d))).unwrap_or_default();
                format!("Monat um den {n}. Vollmond nach der Wintersonnenwende{wende}")
            }
            Ok(Art::Chinesisch { monat, tag, schaltmonat }) => {
                if schaltmonat {
                    format!("{tag}. Tag des Schaltmonats nach dem {monat}. Monat im chinesischen Kalender")
                } else {
                    format!("{tag}. Tag des {monat}. Monats im chinesischen Kalender")
                }
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
        assert_eq!(k.kalender(), Katalog::eingebaut().kalender());
        let namen: Vec<&str> = k.kalender().iter().map(|k| k.kennung.as_str()).collect();
        assert_eq!(namen, ["kirche", "orthodox", "chinesisch", "jahreskreis"]);
    }

    fn berlin() -> Katalog {
        Katalog::berlin()
    }

    fn weltzeit_aus(tag_und_zeit: &str) -> f64 {
        let z = chrono::NaiveDateTime::parse_from_str(tag_und_zeit, "%Y-%m-%d %H:%M").unwrap();
        mond::tag_zahl(z.date()) + f64::from(chrono::Timelike::num_seconds_from_midnight(&z)) / 86400.0
    }

    /// Wenden und Gleichen 2001–2100 und die Mondphasen 2027 gegen die Tabellen
    /// von Fred Espenak (Weltzeit, auf die Minute). Gemessen über alle 4948
    /// Phasen und 400 Sonnenereignisse 2001–2100: siehe die Grenzen unten. Für den Tag in Ortszeit heißt das: Liegt ein Ereignis wenige Minuten vor oder nach Mitternacht, kann der Tag um eins abweichen.
    #[test]
    fn gegen_espenak() {
        let mut groesste: f64 = 0.0;
        for zeile in include_str!("espenak-gleichen-2001-2100.txt").lines().filter(|z| !z.starts_with('#')) {
            let t: Vec<&str> = zeile.split(' ').collect();
            let jahr: i32 = t[0].parse().unwrap();
            for (i, s) in [Sonnenereignis::GleicheMaerz, Sonnenereignis::SonnenwendeJuni, Sonnenereignis::GleicheSeptember, Sonnenereignis::SonnenwendeDezember]
                .into_iter()
                .enumerate()
            {
                let soll = weltzeit_aus(&format!("{jahr}-{} {}", t[1 + 2 * i], t[2 + 2 * i]));
                let ist = s.zeitpunkt(jahr).unwrap();
                groesste = groesste.max((ist - soll).abs() * 1440.0);
            }
        }
        assert!(groesste <= SONNE_MINUTEN, "Sonne: {groesste:.2} Minuten");
        assert_eq!(include_str!("espenak-gleichen-2001-2100.txt").lines().filter(|z| !z.starts_with('#')).count(), 100);
        let mut zahl = 0;
        groesste = 0.0;
        for zeile in include_str!("espenak-phasen-2027.txt").lines().filter(|z| !z.starts_with('#')) {
            let t: Vec<&str> = zeile.split(' ').collect();
            let soll = weltzeit_aus(&format!("{} {}", t[0], t[1]));
            let ist = himmel::phase_ab(Phase::aus(t[2]).unwrap().grad(), soll - 3.0);
            groesste = groesste.max((ist - soll).abs() * 1440.0);
            zahl += 1;
        }
        assert_eq!(zahl, 49);
        assert!(groesste <= MOND_MINUTEN, "Mond: {groesste:.2} Minuten");
    }

    /// Toleranzen in Minuten, gemessen gegen Espenak 2001–2100 (die Tabellen runden selbst auf
    /// die Minute): über alle 4948 Phasen höchstens 0,9 Minuten (im Mittel 0,3), über die 400
    /// Wenden und Gleichen höchstens 2,7 Minuten (im Mittel 0,7).
    const SONNE_MINUTEN: f64 = 2.7;
    const MOND_MINUTEN: f64 = 0.9;

    /// Das Beispieljahr 2027 aus dem Konzept (Abschnitt 3a und 3b), Ortszeit Berlin.
    #[test]
    fn beispieljahr_2027() {
        let k = berlin();
        let tag_von = |f: &str, j: i32| fest(&k, f, j).unwrap();
        for (f, j, d) in [
            ("jahreskreis.yule", 2026, "2026-12-21"),
            ("jahreskreis.ostara", 2027, "2027-03-20"),
            ("jahreskreis.litha", 2027, "2027-06-21"),
            ("jahreskreis.mabon", 2027, "2027-09-23"),
            ("jahreskreis.yule", 2027, "2027-12-22"),
            ("jahreskreis.yule-mond", 2026, "2026-12-09"),
            ("jahreskreis.imbolc", 2027, "2027-01-07"),
            ("jahreskreis.ostara-mond", 2027, "2027-03-08"),
            ("jahreskreis.dreimilch", 2027, "2027-04-07"),
            ("jahreskreis.litha-mond", 2027, "2027-06-04"),
            ("jahreskreis.lughnassad", 2027, "2027-07-04"),
            ("jahreskreis.mabon-mond", 2027, "2027-08-31"),
            ("jahreskreis.samhain", 2027, "2027-09-30"),
            ("jahreskreis.yule-mond", 2027, "2027-11-28"),
            ("jahreskreis.imbolc", 2028, "2028-01-26"),
        ] {
            assert_eq!(tag_von(f, j), d, "{f} {j}");
        }
        let zeitraum = |f: &str, j: i32| {
            let r = Regel { fest: Some(f.into()), ..Default::default() };
            let z = k.zeitraum(None, &r, tag(&tag_von(f, j))).unwrap();
            (z.anfang, z.vollmond, z.ende)
        };
        let z = |a: &str, v: &str, e: &str| (a.to_string(), v.to_string(), e.to_string());
        assert_eq!(zeitraum("jahreskreis.samhain", 2027), z("2027-09-30", "2027-10-15", "2027-10-29"));
        // Grenznah: Ende des Beltane-Monats am 6.5. mittags (Neumond 10:59 Weltzeit).
        assert_eq!(zeitraum("jahreskreis.dreimilch", 2027), z("2027-04-07", "2027-04-21", "2027-05-06"));
        assert!(
            k.zeitraum(None, &Regel { fest: Some("jahreskreis.yule".into()), ..Default::default() }, tag("2027-12-22")).is_none(),
            "ein Tag ist kein Monat"
        );
        // Herkunft mit Bezug und überliefertem Namen.
        let r = Regel { fest: Some("jahreskreis.samhain".into()), ..Default::default() };
        assert_eq!(
            k.herkunft(None, &r, 2027),
            "Jahreskreis · Samhain · Monat um den 11. Vollmond nach der Wintersonnenwende (21.12.2026) · überliefert: Winterfylleþ (altenglisch, Beda)"
        );
        let r = Regel { fest: Some("jahreskreis.ostara".into()), ..Default::default() };
        assert_eq!(
            k.herkunft(None, &r, 2027),
            "Jahreskreis · Frühlingsfest (Ostara) · Tag-und-Nacht-Gleiche im März am 20.3.2027 um 21:24 Uhr Ortszeit · überliefert: Ostara (neuheidnischer Festname, Göttinnenname bei Jacob Grimm, neuzeitlich)"
        );
        // Der laufende Mond durch das Jahr: die 17 Zeilen der Tabelle in Abschnitt 3b
        // (Samhain 2026 bis Imbolc 2028, mit dem Winter 2027/28 mit drei Monden), je am
        // Tag des Vollmonds: Anfang, Vollmond, Ende, Nummer im Festjahr, Name, erster alter Name.
        for (anfang, vollmond, ende, nummer, name, erster) in [
            ("2026-10-10", "2026-10-26", "2026-11-09", 11, "Samhain", "Winterfylleþ"),
            ("2026-11-09", "2026-11-24", "2026-12-09", 12, "1. Wintervollmond", "Blōtmōnaþ"),
            ("2026-12-09", "2026-12-24", "2027-01-07", 1, "2. Wintervollmond · Yule-Mond", "Giuli"),
            ("2027-01-07", "2027-01-22", "2027-02-06", 2, "Imbolc", "Sōlmōnaþ"),
            ("2027-02-06", "2027-02-21", "2027-03-08", 3, "1. Frühlingsvollmond", "Hrēþmōnaþ"),
            ("2027-03-08", "2027-03-22", "2027-04-07", 4, "2. Frühlingsvollmond · Ostara-Mond", "Ēosturmōnaþ"),
            ("2027-04-07", "2027-04-21", "2027-05-06", 5, "Beltane", "Þrimilce"),
            ("2027-05-06", "2027-05-20", "2027-06-04", 6, "1. Sommervollmond", "Līþa"),
            ("2027-06-04", "2027-06-19", "2027-07-04", 7, "2. Sommervollmond · Litha-Mond", "Līþa"),
            ("2027-07-04", "2027-07-18", "2027-08-02", 8, "Lughnassad (Lammas)", "Wēodmōnaþ"),
            ("2027-08-02", "2027-08-17", "2027-08-31", 9, "1. Herbstvollmond", "Hāligmōnaþ"),
            ("2027-08-31", "2027-09-16", "2027-09-30", 10, "2. Herbstvollmond · Mabon-Mond", "Hāligmōnaþ"),
            ("2027-09-30", "2027-10-15", "2027-10-29", 11, "Samhain", "Winterfylleþ"),
            ("2027-10-29", "2027-11-14", "2027-11-28", 12, "1. Wintervollmond", "Blōtmōnaþ"),
            ("2027-11-28", "2027-12-13", "2027-12-27", 1, "2. Wintervollmond · Yule-Mond", "Giuli"),
            ("2027-12-27", "2028-01-12", "2028-01-26", 2, "3. Wintervollmond", "Giuli"),
            ("2028-01-26", "2028-02-10", "2028-02-25", 3, "Imbolc", "Sōlmōnaþ"),
        ] {
            for heute in [anfang, vollmond] {
                let m = k.mond("jahreskreis", tag(heute)).unwrap();
                assert_eq!((m.nummer, m.name.as_str(), m.namen.as_ref().unwrap()[0].name.as_str()), (Some(nummer), name, erster), "{heute}");
                assert_eq!(m.zeitraum, Zeitraum { anfang: anfang.into(), vollmond: vollmond.into(), ende: ende.into() }, "{heute}");
                assert_eq!(m.kalender, "Jahreskreis");
            }
        }
        // Am Tag des Neumonds (4.6.2027, 21:40 Uhr) schon der neue Monat, wie in der Fristenliste;
        // ebenso am 6.5.2027 (Neumond mittags).
        let m = k.mond("jahreskreis", tag("2027-06-04")).unwrap();
        assert_eq!((m.nummer, m.name.as_str(), m.zeitraum.anfang.as_str()), (Some(7), "2. Sommervollmond · Litha-Mond", "2027-06-04"));
        assert_eq!(k.mond("jahreskreis", tag("2027-05-06")).unwrap().name, "1. Sommervollmond");
        assert_eq!(k.mond("jahreskreis", tag("2027-05-05")).unwrap().name, "Beltane");
        let m = k.mond("jahreskreis", tag("2027-02-20")).unwrap();
        // Vollmond 20.2. 23:24 Weltzeit: in Berlin schon der 21.2.
        assert_eq!(m.zeitraum, Zeitraum { anfang: "2027-02-06".into(), vollmond: "2027-02-21".into(), ende: "2027-03-08".into() });
        assert!(k.mond("kirche", tag("2027-02-20")).is_none(), "ohne Jahreszeiten kein laufender Mond");
    }

    /// Die alten Namen hängen an der Lage zum Sonnenmonat, nicht an der Nummer (Abnahme):
    /// Im Winter 2033/34 ist der Yule-Mond der 1. Wintervollmond und trägt Giuli, der Monat
    /// danach die Namen nach der Wende; der Ostara-Mond 2034 trägt Ēosturmōnaþ.
    #[test]
    fn alte_namen_nach_der_lage() {
        let k = berlin();
        let m = k.mond("jahreskreis", tag("2033-12-01")).unwrap();
        let namen: Vec<&str> = m.namen.iter().flatten().map(|n| n.name.as_str()).collect();
        assert_eq!((m.name.as_str(), m.zeitraum.anfang.as_str()), ("1. Wintervollmond · Yule-Mond", "2033-11-22"));
        assert_eq!(namen, ["Giuli", "heilagmanoth", "hrútmánuðr", "jiuleis"]);
        let m = k.mond("jahreskreis", tag("2034-01-01")).unwrap();
        let namen: Vec<&str> = m.namen.iter().flatten().map(|n| n.name.as_str()).collect();
        assert_eq!((m.name.as_str(), namen.as_slice()), ("2. Wintervollmond", ["Giuli", "wintarmanoth"].as_slice()));
        let m = k.mond("jahreskreis", tag("2034-03-25")).unwrap();
        assert!(m.name.ends_with("· Ostara-Mond"), "{}", m.name);
        assert_eq!(m.namen.as_ref().unwrap()[0].name, "Ēosturmōnaþ");
        assert!(m.namen.iter().flatten().any(|n| n.name == "Ostara-Mond" && n.stufe == Belegstufe::Neuzeitlich));
        // Über 1950–2099, ohne Bedingung: Am Tag jedes Sonnentags läuft dessen Monat mit seinem
        // Titel und ersten Namen; die Monate davor und danach in derselben Jahreszeit tragen die
        // Namen der Jahreszeit (Frühling und Herbst danach keine).
        let erster = |m: &Mond| m.namen.iter().flatten().next().map(|n| n.name.clone()).unwrap_or_default();
        let in_jahreszeit = |m: &Mond| m.name.starts_with(|c: char| c.is_ascii_digit());
        let mut litha_zuerst = Vec::new();
        for j in 1950..=2099 {
            for (s, titel, eigen, davor, danach) in [
                (Sonnenereignis::SonnenwendeDezember, "Yule-Mond", "Giuli", "Blōtmōnaþ", "Giuli"),
                (Sonnenereignis::GleicheMaerz, "Ostara-Mond", "Ēosturmōnaþ", "Hrēþmōnaþ", ""),
                (Sonnenereignis::SonnenwendeJuni, "Litha-Mond", "Līþa", "Līþa", "Līþa"),
                (Sonnenereignis::GleicheSeptember, "Mabon-Mond", "Hāligmōnaþ", "Hāligmōnaþ", ""),
            ] {
                // Der Monat, der den berechneten Sonnentag enthält, geprüft an seinem Vollmond (am
                // Sonnentag selbst kann schon der nächste laufen: Neumond am selben Tag nach dem
                // Ereignis, etwa 23.9.1957; der Tag mit dem Neumond gehört zum neuen Monat).
                let t = s.zeitpunkt(j).unwrap();
                let nr = mond::monat_bei(t);
                let tag_von = |n: i32| k.ort.tag(mond::vollmond(n)).unwrap();
                let m = k.mond("jahreskreis", tag_von(nr)).unwrap();
                assert!(m.name.ends_with(&format!("· {titel}")), "{j} {titel}: {}", m.name);
                assert_eq!(erster(&m), eigen, "{j} {titel}");
                let vor = k.mond("jahreskreis", tag_von(nr - 1)).unwrap();
                if in_jahreszeit(&vor) {
                    assert_eq!(erster(&vor), davor, "{j}: vor dem {titel}: {}", vor.name);
                }
                let nach = k.mond("jahreskreis", tag_von(nr + 1)).unwrap();
                if in_jahreszeit(&nach) {
                    assert_eq!(erster(&nach), danach, "{j}: nach dem {titel}: {}", nach.name);
                    if titel == "Litha-Mond" {
                        litha_zuerst.push(j);
                    }
                }
            }
        }
        // Der Litha-Mond als 1. Sommervollmond (etwa ein Drittel der Jahre), festes Beispiel 2028.
        assert!((40..=60).contains(&litha_zuerst.len()), "{litha_zuerst:?}");
        let m = k.mond("jahreskreis", tag("2028-06-21")).unwrap();
        assert_eq!(m.name, "1. Sommervollmond · Litha-Mond");
        let m = k.mond("jahreskreis", tag(&m.zeitraum.ende)).unwrap();
        assert_eq!((m.name.as_str(), erster(&m).as_str()), ("2. Sommervollmond", "Līþa"));
    }

    /// Grenzfälle aus den Tabellen 2001–2100: Neumond am Tag einer Wende oder
    /// Gleiche (Zeitpunkte entscheiden), Vollmond am Tag der Wintersonnenwende
    /// (zählt nicht), Mitternacht in der Sommerzeit, Südhalbkugel.
    #[test]
    fn grenzfaelle() {
        let k = berlin();
        // 21.12.2033: Wende 13:45, Neumond 18:47 Weltzeit. Die Wende liegt noch im Monat davor.
        assert_eq!(fest(&k, "jahreskreis.yule-mond", 2033).as_deref(), Some("2033-11-22"));
        // 20.3.2034: Neumond 10:14, Gleiche 13:18. Der neue Monat enthält die Gleiche.
        assert_eq!(fest(&k, "jahreskreis.ostara-mond", 2034).as_deref(), Some("2034-03-20"));
        // 21.12.2094: Wende 9:11, Vollmond 19:57 Weltzeit, in Berlin derselbe Tag: Er zählt
        // nicht; Imbolc ist der Monat um den Vollmond vom 19.2.2095, nicht den vom 20.1.
        assert_eq!(fest(&k, "jahreskreis.imbolc", 2095).as_deref(), Some("2095-02-04"));
        // Vollmond 29.6.2026 23:57 Weltzeit: in Berlin (Sommerzeit) der 30.6.
        let t = himmel::phase_ab(180.0, mond::tag_zahl(tag("2026-06-25")));
        assert_eq!(k.ort.tag(t), Some(tag("2026-06-30")));
        // Die Ortszeit von Windows rechnet: Mittag Weltzeit liegt in jeder Zone höchstens einen Tag daneben.
        let windows = mond::Ort::windows().tag(mond::tag_zahl(tag("2027-06-01")) + 0.5).unwrap();
        assert!((windows - tag("2027-06-01")).num_days().abs() <= 1, "{windows}");
        // Mondphase nach einem Bezug: der 2. Vollmond nach Yule 2027 (22.12.) ist der vom 10.2.2028.
        let probe = Katalog::lesen_fuer(
            &[
                ("probe.toml", "kennung = \"probe\"\ntitel = \"Probe\"\n[[fest]]\nkennung = \"v\"\ntitel = \"V\"\nregel = { himmel = \"vollmond\", nummer = 2, nach = \"jahreskreis.yule\" }\n"),
                EINGEBAUT[3],
            ],
            k.ort,
        )
        .unwrap();
        assert_eq!(fest(&probe, "probe.v", 2027).as_deref(), Some("2028-02-10"));
        // Südhalbkugel: Der kürzeste Tag ist die Wende im Juni (21.6.2027); der 2. Vollmond
        // danach ist der vom 17.8.2027, sein Monat beginnt am 2.8.
        let sued = Katalog::lesen_fuer(&EINGEBAUT, Ort { halbkugel: mond::Halbkugel::Sued, zeit: mond::Zeit::Test(mond::mitteleuropa), lage: None }).unwrap();
        assert_eq!(fest(&sued, "jahreskreis.imbolc", 2027).as_deref(), Some("2027-08-02"));
    }

    /// Eigenschaften über 1950–2099 (Konzept, Abschnitt 3a und 3b): Die acht
    /// benannten Monate kommen in jedem Festjahr in der richtigen Reihenfolge und
    /// nie doppelt; Frühling, Sommer und Herbst haben genau zwei Monde, der Winter
    /// zwei oder drei; das Festjahr hat 12 oder 13 Monate.
    #[test]
    fn eigenschaften_1950_bis_2099() {
        let k = berlin();
        let monat = |f: &str, j: i32| {
            let (_, fest) = k.finden(None, &format!("jahreskreis.{f}")).unwrap();
            k.mondmonat(&fest.regel, j).unwrap()
        };
        let (mut dreizehn, mut drei_winter) = (0, 0);
        for j in 1950..=2099 {
            let folge = [
                monat("yule-mond", j - 1),
                monat("imbolc", j),
                monat("ostara-mond", j),
                monat("dreimilch", j),
                monat("litha-mond", j),
                monat("lughnassad", j),
                monat("mabon-mond", j),
                monat("samhain", j),
                monat("yule-mond", j),
            ];
            assert!(folge.windows(2).all(|w| w[0] < w[1]), "{j}: {folge:?}");
            let anfang = |y: i32| mond::monat_bei(k.ort.kuerzester_tag(y).unwrap());
            assert_eq!(folge[0], anfang(j), "{j}: Festjahr beginnt mit dem Yule-Mond");
            let laenge = folge[8] - folge[0];
            assert!(laenge == 12 || laenge == 13, "{j}: {laenge} Monate");
            dreizehn += i32::from(laenge == 13);
            assert_eq!(folge[3] - folge[1] - 1, 2, "{j}: Frühling");
            assert_eq!(folge[5] - folge[3] - 1, 2, "{j}: Sommer");
            assert_eq!(folge[7] - folge[5] - 1, 2, "{j}: Herbst");
            let winter = monat("imbolc", j + 1) - folge[7] - 1;
            assert!(winter == 2 || winter == 3, "{j}: Winter {winter}");
            drei_winter += i32::from(winter == 3);
        }
        assert_eq!((dreizehn, drei_winter), (55, 55));
    }

    #[test]
    fn ostern_nach_dem_mond() {
        let k = berlin();
        // 2019 und 2038 weichen vom kirchlichen Ostern ab (21.4. und 25.4.); 2025 gleich.
        for (j, d) in [(2019, "2019-03-24"), (2038, "2038-03-28"), (2025, "2025-04-20"), (2027, "2027-03-28")] {
            assert_eq!(fest(&k, "kirche.ostern-astronomisch", j).as_deref(), Some(d), "{j}");
            assert_eq!(fest(&k, "jahreskreis.ostern", j).as_deref(), Some(d), "{j}");
        }
        let r = Regel { fest: Some("kirche.ostern-astronomisch".into()), ..Default::default() };
        assert_eq!(
            k.herkunft(None, &r, 2027),
            "Kirchenjahr · Ostersonntag nach dem Mond · Ostersonntag nach dem Mond: erster Sonntag nach dem ersten Vollmond nach der Tag-und-Nacht-Gleiche im März (20.3.2027)"
        );
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
        assert_eq!(k.herkunft(None, &eigene, 2028), "Eigene Regel · 4. Tag des Schaltmonats nach dem 4. Monat im chinesischen Kalender");
    }

    #[test]
    fn pruefung_weist_ab() {
        let falsch = |t: &str| regel(t).art().unwrap_err();
        assert!(falsch("").contains("leer"));
        assert!(falsch("monat = 2\ntag = 30").contains("diesen Tag gibt es nicht"));
        assert!(falsch("monat = 5").contains("`tag`"));
        assert!(falsch("ostern = \"gregorianisch\"").contains("astronomisch"));
        // Himmel und Mondmonat (Belegstufe 2).
        let s = falsch("himmel = \"sternschnuppe\"");
        assert!(s.contains("Sonnentage") && !s.contains("sternschnuppe"), "{s}");
        assert!(falsch("himmel = \"vollmond\"").contains("`nach`"));
        assert!(falsch("himmel = \"gleiche-maerz\"\nnach = \"x\"").contains("kein `nach`"));
        let s = falsch("himmel = \"vollmond\"\nnach = \"x\"\nnummer = 14");
        assert!(s.contains("1 bis 13") && !s.contains("14"), "{s}");
        assert!(falsch("nummer = 2").contains("gehört zu"));
        assert!(falsch("nach = \"x\"\nnummer = 2").contains("`himmel`"), "verweist auf himmel");
        assert!(falsch("mondmonat = {}").contains("genau eines"));
        assert!(falsch("mondmonat = { vollmond = 2, enthaelt = \"gleiche-maerz\" }").contains("genau eines"));
        let s = falsch("mondmonat = { vollmond = 14 }");
        assert!(s.contains("1 bis 13") && !s.contains("14"), "{s}");
        assert!(falsch("mondmonat = { enthaelt = \"vollmond\" }").contains("Wende oder Gleiche"));
        assert!(falsch("mondmonat = { vollmond = 2 }\nmonat = 1").contains("kein `monat`"));
        assert!(regel("himmel = \"neumond\"\nnach = \"x\"").art().is_ok(), "nummer ist wahlweise");
        let jahreszeit = |nach: &str| {
            format!(
                "kennung = \"k\"\ntitel = \"K\"\n[[fest]]\nkennung = \"m\"\ntitel = \"M\"\nregel = {{ mondmonat = {{ vollmond = 2 }} }}\n[[fest]]\nkennung = \"t\"\ntitel = \"T\"\nregel = {{ monat = 1, tag = 1 }}\n[[jahreszeit]]\nnach = \"{nach}\"\nvollmond = \"Wintervollmond\"\ndavor = []\n"
            )
        };
        assert!(Katalog::lesen(&[("k.toml", &jahreszeit("m"))]).is_ok());
        assert!(Katalog::lesen(&[("k.toml", &jahreszeit("t"))]).unwrap_err().contains("Jahreszeit"));
        assert!(Katalog::lesen(&[("k.toml", &jahreszeit("x"))]).unwrap_err().contains("Jahreszeit"));
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
        assert!(toml::from_str::<Regel>("stern = \"vollmond\"").is_err(), "unbekannte Schlüssel weist serde ab");
        assert!(toml::from_str::<Regel>("mondmonat = { stern = 1 }").is_err(), "auch im Mondmonat");
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
        // Regeln ohne ein einziges Vorkommen 1912–2100 (Hinweis der Abnahme).
        for nie in ["kalender = \"chinesisch\"\nmonat = 12\ntag = 1\nschaltmonat = true", "kalender = \"chinesisch\"\nmonat = 1\ntag = 1\nschaltmonat = true"] {
            assert!(k.ereignis_regel_pruefen(&regel(nie)).unwrap_err().contains("in keinem Jahr"), "{nie}");
        }
        assert!(k.ereignis_regel_pruefen(&regel("kalender = \"chinesisch\"\nmonat = 11\ntag = 1\nschaltmonat = true")).is_ok(), "2033");
        // Die Sätze nennen keine eingegebenen Zahlen.
        assert!(!falsch("kalender = \"chinesisch\"\nmonat = 13\ntag = 1").contains("13"));
        assert!(!falsch("monat = 2\ntag = 31").contains("31"));
        assert!(!falsch("nach = \"x\"\ntage = 400").contains("400"));
    }

    // ----- Stufe 3: Orte, Halbkugel, Wendekreise, Zeitzonen -----

    fn an(breite: f64, laenge: f64, name: &str) -> Katalog {
        Katalog::eingebaut().mit_ort(Ort::an(mond::Lage { breite, laenge }, mond::Zeit::Zone(zone::zone(name).unwrap())))
    }

    /// Der erste Donnerstag nach der Gleiche; fällt sie selbst auf einen Donnerstag (20.3.2025), zählt sie nicht.
    #[test]
    fn donnerstag_nach_ostara() {
        let k = berlin();
        assert_eq!(fest(&k, "jahreskreis.ostara", 2025).as_deref(), Some("2025-03-20"));
        assert_eq!(tag("2025-03-20").weekday(), Weekday::Thu);
        for (jahr, soll) in [(2025, "2025-03-27"), (2026, "2026-03-26"), (2027, "2027-03-25")] {
            assert_eq!(fest(&k, "jahreskreis.ostara-donnerstag", jahr).as_deref(), Some(soll));
        }
        // Himmelfahrt und Fronleichnam bleiben im Kirchenjahr.
        let jk = k.kalender().iter().find(|x| x.kennung == "jahreskreis").unwrap();
        assert!(jk.fest.iter().all(|f| !matches!(f.kennung.as_str(), "himmelfahrt" | "fronleichnam")));
    }

    /// Sydney: Yule im Juni, Ostara im September; Zählung ab der Junisonnenwende.
    /// Zeitpunkte 2027 (Espenak, Weltzeit): 20.3. 20:25, 21.6. 14:11, 23.9. 06:02, 22.12. 02:43.
    #[test]
    fn suedhalbkugel_sydney() {
        let s = an(-33.87, 151.21, "Australia/Sydney");
        assert_eq!(s.ort().halbkugel, mond::Halbkugel::Sued);
        for (f, soll) in [("yule", "2027-06-22"), ("ostara", "2027-09-23"), ("litha", "2027-12-22"), ("mabon", "2027-03-21")] {
            assert_eq!(fest(&s, &format!("jahreskreis.{f}"), 2027).as_deref(), Some(soll), "{f}");
        }
        // Derselbe Zeitpunkt (21.6. 14:11 Weltzeit) ist in Berlin Litha am 21.6., in Sydney Yule am 22.6.
        assert_eq!(fest(&berlin(), "jahreskreis.litha", 2027).as_deref(), Some("2027-06-21"));
        assert!(s.herkunft(None, &Regel { fest: Some("jahreskreis.yule".into()), ..Default::default() }, 2027).contains("Sonnenwende im Juni"));
        // Ostern nach dem Mond im Jahreskreis folgt der Gleiche im September, im Kirchenjahr der im März.
        let jk_ostern = s.im_jahr(None, &Regel { fest: Some("jahreskreis.ostern".into()), ..Default::default() }, 2027);
        let kirche_ostern = s.im_jahr(None, &Regel { fest: Some("kirche.ostern-astronomisch".into()), ..Default::default() }, 2027);
        assert!(jk_ostern.is_some_and(|d| d.month() >= 9), "{jk_ostern:?}");
        assert!(kirche_ostern.is_some_and(|d| d.month() <= 4), "{kirche_ostern:?}");
        // Ein Festjahr mit allen Monatsnamen: Winter im Juni/Juli, Sommer im Dezember/Januar.
        let mut namen: Vec<String> = Vec::new();
        let mut d = tag("2027-06-22");
        while d < tag("2028-06-22") {
            let m = s.mond("jahreskreis", d).unwrap();
            if namen.last() != Some(&m.name) {
                namen.push(m.name.clone());
            }
            d = d.succ_opt().unwrap();
        }
        for titel in ["Yule-Mond", "Ostara-Mond", "Litha-Mond", "Mabon-Mond", "Imbolc", "Beltane", "Lughnassad", "Samhain"] {
            assert!(namen.iter().any(|n| n.contains(titel)), "{titel}: {namen:?}");
        }
        assert!(s.mond("jahreskreis", tag("2027-07-15")).unwrap().name.contains("Winter"), "{namen:?}");
        assert!(s.mond("jahreskreis", tag("2027-12-30")).unwrap().name.contains("Sommer"), "{namen:?}");
        // Längster Tag im Süden: am Tag der Sonnenwende im Dezember.
        assert!(s.laengster_tag(tag("2027-12-22")).is_some_and(|l| l.starts_with("Längster Tag · 14 h")), "{:?}", s.laengster_tag(tag("2027-12-22")));
        assert_eq!(s.laengster_tag(tag("2027-06-22")), None);
    }

    /// Zwischen den Wendekreisen nur Sonnentage und Mondphasen, ohne Namen; knapp außerhalb mit Namen.
    #[test]
    fn wendekreise() {
        let sg = an(1.35, 103.82, "Asia/Singapore");
        let jk = sg.kalender().iter().find(|x| x.kennung == "jahreskreis").unwrap();
        let f = |k: &str| jk.fest.iter().find(|f| f.kennung == k).unwrap();
        assert!(
            matches!(sg.anzeige(jk, f("yule"), 2027), Anzeige::Neutral { ref titel, ref herkunft } if titel == "Sonnenwende im Dezember" && !herkunft.contains("Yule"))
        );
        for k in ["imbolc", "yule-mond", "ostern", "ostara-donnerstag"] {
            assert_eq!(sg.anzeige(jk, f(k), 2027), Anzeige::Entfaellt, "{k}");
        }
        let m = sg.mond("jahreskreis", tag("2027-07-15")).unwrap();
        assert!(m.nummer.is_none() && m.namen.is_none() && m.name == "Mondmonat" && m.hinweis.is_some(), "{m:?}");
        let samhain = Regel { fest: Some("jahreskreis.samhain".into()), ..Default::default() };
        assert_eq!(sg.wendekreis_pruefen(&samhain), Err(SATZ_WENDEKREIS.to_string()));
        let ueber_verweis = regel("himmel = \"vollmond\"\nnach = \"jahreskreis.yule\"");
        assert!(sg.wendekreis_pruefen(&ueber_verweis).is_err());
        assert_eq!(sg.wendekreis_pruefen(&regel("himmel = \"sonnenwende-dezember\"")), Ok(()));
        assert_eq!(sg.wendekreis_pruefen(&regel("fest = \"kirche.ostern\"")), Ok(()));
        // 24° Nord (Dhaka liegt knapp darüber): mit Namen.
        let nah = an(24.0, 90.4, "Asia/Dhaka");
        assert_eq!(nah.anzeige(jk, f("yule"), 2027), Anzeige::Normal);
        assert!(nah.mond("jahreskreis", tag("2027-07-15")).unwrap().nummer.is_some());
        assert_eq!(nah.wendekreis_pruefen(&samhain), Ok(()));
    }

    /// Zeitzonen: derselbe Zeitpunkt auf zwei Tagen, halbe Stunden, Sommerzeit über den Tagesanfang.
    #[test]
    fn zeitzonen() {
        // Wende im Dezember 2027 um 02:43 Weltzeit: in London am 22.12., auf Hawaii (24,5° Nord) am 21.12.
        assert_eq!(fest(&an(51.5, -0.1, "Europe/London"), "jahreskreis.yule", 2027).as_deref(), Some("2027-12-22"));
        assert_eq!(fest(&an(24.5, -157.9, "Pacific/Honolulu"), "jahreskreis.yule", 2027).as_deref(), Some("2027-12-21"));
        // Kolkata: +05:30, der Tag beginnt um 18:30 Weltzeit des Vortags.
        let kolkata = an(22.6, 88.4, "Asia/Kolkata");
        assert_eq!(kolkata.ort().zeit.art(), "Asia/Kolkata");
        assert!((kolkata.ort().beginn(tag("2027-06-21")).unwrap() - (mond::tag_zahl(tag("2027-06-21")) - 5.5 / 24.0)).abs() < 1e-9);
        // Sydney: Sommerzeit ab 3.10.2027 um 2 Uhr; der Tag beginnt noch mit +10, der 4.10. mit +11.
        let sydney = an(-33.87, 151.21, "Australia/Sydney");
        for (d, h) in [("2027-10-03", 10.0), ("2027-10-04", 11.0), ("2027-04-04", 11.0), ("2027-04-05", 10.0)] {
            assert!((sydney.ort().beginn(tag(d)).unwrap() - (mond::tag_zahl(tag(d)) - h / 24.0)).abs() < 1e-9, "{d}");
        }
        // Santiago stellt um Mitternacht um: Am 5.9.2027 gibt es kein 0 Uhr, der Tag beginnt um 1 Uhr (4 Uhr Weltzeit).
        let santiago = an(-33.4, -70.7, "America/Santiago");
        let b = santiago.ort().beginn(tag("2027-09-05")).unwrap();
        assert!((b - (mond::tag_zahl(tag("2027-09-05")) + 4.0 / 24.0)).abs() < 1e-9, "{}", (b - mond::tag_zahl(tag("2027-09-05"))) * 24.0);
        assert_eq!(santiago.ort().tag(b - 1e-3), Some(tag("2027-09-04")), "eine Minute davor ist Vortag");
    }

    /// Längster Tag in London am 21.6.2027 (Vergleichswert folgt mit der Quelle der Tageszeiten).
    #[test]
    fn laengster_tag_london() {
        let london = an(51.5074, -0.1278, "Europe/London");
        let l = london.laengster_tag(tag("2027-06-21")).unwrap();
        assert!(l.starts_with("Längster Tag · 16 h 3"), "{l}");
        assert_eq!(london.laengster_tag(tag("2027-06-20")), None);
        assert_eq!(berlin().laengster_tag(tag("2027-06-21")), None, "ohne Lage keine Taglänge");
    }

    /// S2: Im Polargebiet heißt der längste Tag „Sonne geht nicht unter“, nicht „24 h 0 min“.
    /// M1: Apia (Süden, Zone +13) rechnet den Tag der Wende über den Sonnentag mit Mittag an diesem Tag.
    #[test]
    fn laengster_tag_polar_und_datumsgrenze() {
        let tromsoe = an(69.6492, 18.9553, "Europe/Oslo");
        assert_eq!(tromsoe.laengster_tag(tag("2027-06-21")).as_deref(), Some("Längster Tag · Sonne geht nicht unter"));
        let apia = an(-13.8, -171.8, "Pacific/Apia");
        let tage: Vec<NaiveDate> = tag("2027-12-19").iter_days().take(6).filter(|d| apia.laengster_tag(*d).is_some()).collect();
        assert_eq!(tage.len(), 1, "{tage:?}");
        let l = apia.laengster_tag(tage[0]).unwrap();
        let z = tag::am_tag(tage[0], apia.ort()).unwrap();
        assert_eq!(l, format!("Längster Tag · {}", tag::dauer_text(z.taglaenge().unwrap())));
        assert!(l.starts_with("Längster Tag · 12 h") || l.starts_with("Längster Tag · 13 h"), "{l}");
    }
}
