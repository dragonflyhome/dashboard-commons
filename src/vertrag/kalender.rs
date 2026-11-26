//! Antworten der Kalender-Schnittstelle (Stufe 3): Tageszeiten und Feste für
//! benannte und abgefragte Orte. Benannte Orte gehen nur mit Kennung und Titel
//! hinaus; Koordinaten stehen nur in der Antwort auf eine freie Abfrage (gerundet).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::kalender::{Mond, Zeitraum};

/// Ein benannter Ort für die Auswahl.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct KalenderOrt {
    pub kennung: String,
    pub titel: String,
    /// Zeitart, etwa „Europe/Berlin“ oder „Ortszeit Windows“.
    pub zeitart: String,
    /// Der erste Ort: nach ihm rechnen Jahreskreis und Mondmonate.
    pub heimat: bool,
}

/// `GET /api/kalender/standorte`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct KalenderOrte {
    pub orte: Vec<KalenderOrt>,
}

/// Eine Tageszeit: Uhrzeit oder ein Satz, warum es keine gibt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Tageszeile {
    pub kennung: String,
    pub name: String,
    /// „HH:MM“ in der Zeitart des Orts, mit Datum dahinter, wenn es nicht der Tag selbst ist.
    pub zeit: Option<String>,
    /// Statt der Zeit, etwa „Sonne geht nicht unter“.
    pub text: Option<String>,
    /// Wie gerechnet, etwa „Refraktion 0,833° · Ort Zuhause · Europe/Berlin“.
    pub herkunft: String,
}

/// `GET /api/kalender/tag`: Tageszeiten und Mond eines Tags an einem Ort.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct KalenderTag {
    /// JJJJ-MM-TT.
    pub datum: String,
    /// Titel des Orts; ohne Ort `null`.
    pub ort: Option<String>,
    pub zeitart: String,
    /// Leer ohne Ort.
    pub zeiten: Vec<Tageszeile>,
    /// Taglänge als „16 h 47 min“.
    pub taglaenge: Option<String>,
    /// Am Tag der Sommersonnenwende: „Längster Tag · 16 h 47 min“.
    pub laengster_tag: Option<String>,
    /// Der laufende Mondmonat (aus dem ersten aktiven Kalender mit Jahreszeiten).
    pub mond: Option<Mond>,
    /// Ein Satz, etwa wenn noch kein Ort eingetragen ist.
    pub hinweis: Option<String>,
}

/// Ein Vorkommen eines Fests.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct KalenderFest {
    /// `<kalender>.<fest>`.
    pub kennung: String,
    pub datum: String,
    pub titel: String,
    /// Titel des Kalenders.
    pub kalender: String,
    pub herkunft: String,
    pub zeitraum: Option<Zeitraum>,
}

/// `GET /api/kalender/feste`: die Feste der aktiven Kalender in einem Zeitraum.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct KalenderFeste {
    pub von: String,
    pub bis: String,
    pub ort: Option<String>,
    pub zeitart: String,
    pub feste: Vec<KalenderFest>,
    pub hinweis: Option<String>,
}

/// Rumpf von `POST /api/kalender/abfrage`: ein Ort, der nicht gespeichert wird.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct KalenderAbfrage {
    /// Tag für die Tageszeiten (Vorgabe heute).
    #[serde(default)]
    pub datum: Option<String>,
    /// Zeitraum der Feste (Vorgabe: ab `datum` 30 Tage).
    #[serde(default)]
    pub von: Option<String>,
    #[serde(default)]
    pub bis: Option<String>,
    pub breite: f64,
    pub laenge: f64,
    /// IANA-Zeitzone; ohne sie ein fester Versatz nach der Länge (Näherung).
    #[serde(default)]
    pub zeitzone: Option<String>,
}

/// Antwort auf eine freie Abfrage, mit den gerundeten Koordinaten, für die gerechnet wurde.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct KalenderAbfrageAntwort {
    pub breite: f64,
    pub laenge: f64,
    /// Die Zeit ist aus der Länge geschätzt (keine Zeitzone angegeben).
    pub naeherung: bool,
    pub tag: KalenderTag,
    pub feste: KalenderFeste,
}

/// `GET /api/kalender/zonen`: die IANA-Zonen zur Auswahl und die Fassung der Datenbank.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct KalenderZonen {
    pub zonen: Vec<String>,
    /// Fassung der mitgelieferten Zeitzonen-Datenbank, etwa „2026a“.
    pub tzdb: String,
}

/// Rumpf von `POST /api/kalender/standorte`: einen abgefragten Ort merken.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct KalenderMerken {
    pub titel: String,
    pub breite: f64,
    pub laenge: f64,
    #[serde(default)]
    pub zeitzone: Option<String>,
}
