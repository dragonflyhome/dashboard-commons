//! Antworten der Kalender-Schnittstelle (Stufe 3): Tageszeiten und Feste für
//! benannte und abgefragte Orte. Benannte Orte gehen nur mit Kennung und Titel
//! hinaus; Koordinaten stehen nur in der Antwort auf eine freie Abfrage (gerundet).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::kalender::zaehlung::Zaehlzeile;
use crate::kalender::{Mond, Name, Zeitraum};

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
    /// Die Gebetszeiten der ausgewählten Varianten (Stufe 4), je Variante eine Liste.
    pub gebete: Vec<Gebetsliste>,
    /// Die aktiven Zählkalender (Stufe 5) in der Reihenfolge der Vorlage, unabhängig vom Ort; ohne aktive leer.
    pub zaehlungen: Vec<Zaehlzeile>,
}

/// Die Gebetszeiten einer Variante an einem Tag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Gebetsliste {
    /// Kennung der Variante, etwa `fuenf-mwl`.
    pub kennung: String,
    pub titel: String,
    pub zeilen: Vec<Gebetszeile>,
    /// Ein Satz ohne Zahlen, wenn Zeiten fehlen (Polargebiet).
    pub hinweis: Option<String>,
}

/// Eine Gebetszeit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Gebetszeile {
    pub name: String,
    /// „HH:MM“ wie bei den Tageszeiten.
    pub zeit: Option<String>,
    /// Statt der Zeit: „keine Zeit“.
    pub text: Option<String>,
    /// Namen der zusammengelegten Gebete ohne eigene Zeit, etwa „Ischa“.
    pub umfasst: Vec<String>,
    /// Hochbreite, etwa „nach der Siebtel-Regel“.
    pub hinweis: Option<String>,
    /// Wie gerechnet, etwa „MWL 17° · Siebtel-Regel · Ort Zuhause · Europe/Berlin“.
    pub herkunft: String,
}

/// Eine Variante der Vorlage zur Auswahl.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GebeteVariante {
    pub kennung: String,
    pub titel: String,
}

/// Die gespeicherte Auswahl an einem Ort.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GebeteOrtAuswahl {
    /// Kennung des Orts.
    pub ort: String,
    pub varianten: Vec<String>,
    /// Den Ort gibt es nicht mehr (die Auswahl lässt sich dann nur leeren).
    pub fehlt: bool,
}

/// `GET /api/kalender/gebete` und Antwort auf `POST /api/kalender/gebete`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct KalenderGebete {
    pub varianten: Vec<GebeteVariante>,
    pub auswahl: Vec<GebeteOrtAuswahl>,
    /// Ein Satz, wenn die gespeicherte Auswahl nicht lesbar ist.
    pub hinweis: Option<String>,
}

/// Rumpf von `POST /api/kalender/gebete`: die Auswahl an einem benannten Ort;
/// eine leere Liste entfernt sie.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct KalenderGebeteSpeichern {
    pub ort: String,
    pub varianten: Vec<String>,
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
    /// Varianten der Gebetszeiten nur für diese Antwort (nichts wird gespeichert).
    #[serde(default)]
    pub gebete: Option<Vec<String>>,
    /// Auch das Festjahr des Jahreskreises (Stufe 6) für diesen Ort.
    #[serde(default)]
    pub jahr: Option<bool>,
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
    /// Nur mit `jahr = true` in der Abfrage.
    pub jahr: Option<KalenderJahr>,
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

/// Rumpf von `POST /api/kalender/staedte`: Suche in der Städteliste. Die Anfrage
/// wird weder protokolliert noch gespeichert.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct KalenderStaedteAnfrage {
    pub anfrage: String,
    /// Höchstens so viele Treffer, 1 bis 20 (Vorgabe 10).
    #[serde(default)]
    pub hoechstens: Option<u32>,
}

/// Eine öffentliche Stadt aus der Städteliste (GeoNames).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct KalenderStadt {
    pub name: String,
    /// Ländercode aus zwei Buchstaben.
    pub land: String,
    pub region: String,
    pub breite: f64,
    pub laenge: f64,
    /// IANA-Zeitzone, etwa „Europe/Berlin“.
    pub zeitzone: String,
}

/// Antwort auf `POST /api/kalender/staedte`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct KalenderStaedte {
    pub treffer: Vec<KalenderStadt>,
    /// Namensnennung der Liste, „GeoNames (geonames.org), CC BY 4.0“.
    pub quelle: String,
    /// Ein Satz, wenn die Liste fehlt oder nicht benutzt wird.
    pub hinweis: Option<String>,
}

/// `GET /api/kalender/kopf` (Stufe 6): der Himmel von heute am Heimatort für die
/// Kopfleiste, nur angemeldet. Ohne Koordinaten und ohne Zone als Zahl.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct KalenderKopf {
    /// Titel des Heimatorts.
    pub ort: String,
    /// Sonnenaufgang heute „HH:MM“ in der Zeitart des Orts; fehlt er, steht `text` da.
    pub aufgang: Option<String>,
    pub untergang: Option<String>,
    /// Ein Satz ohne Zahlen, wenn eine Zeit fehlt (Polarnacht, Polartag).
    pub text: Option<String>,
    /// Mondphase jetzt in ganzen Grad: 0 Neumond, 90 erstes Viertel, 180 Vollmond, 270 letztes Viertel.
    pub phase_grad: f64,
    /// Das Wort der Phase heute, wie beim laufenden Mond, etwa „abnehmend“.
    pub phase: String,
    /// Südhalbkugel: Das Bild des Monds steht gespiegelt.
    pub sued: bool,
    /// Tag des nächsten Vollmonds (JJJJ-MM-TT, Ortszeit), heute eingeschlossen.
    pub naechster_vollmond: String,
}

/// Rumpf von `POST /api/kalender/heimat`: welcher Ort Heimatort ist (ein eigener oder der Standardort).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct KalenderHeimat {
    pub ort: String,
}

/// Erster Tag eines Festjahrs und erster Tag des nächsten (JJJJ-MM-TT, Ortszeit).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct KalenderSpanne {
    pub anfang: String,
    pub ende: String,
}

/// Ein Mondmonat im Festjahr.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct KalenderJahrMonat {
    /// 1 = der Monat mit dem kürzesten Tag.
    pub nummer: u8,
    /// Sprechender Name wie beim laufenden Mond, etwa „2. Wintervollmond · Yule-Mond“.
    pub name: String,
    pub namen: Vec<Name>,
    /// Das Wort der Jahreszeit, etwa „Wintervollmond“.
    pub jahreszeit: Option<String>,
    pub zeitraum: Zeitraum,
}

/// Ein Himmelsereignis im Festjahr: ein Sonnentag oder eine der vier Mondphasen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct KalenderZeitpunkt {
    /// Rolle des Sonnentags am Ort (`wende-winter`, `gleiche-fruehling`, `wende-sommer`,
    /// `gleiche-herbst`) oder die Phase (`neumond`, `erstes-viertel`, `vollmond`, `letztes-viertel`).
    pub kennung: String,
    pub name: String,
    pub datum: String,
    /// „HH:MM“ in Ortszeit.
    pub zeit: String,
}

/// `GET /api/kalender/jahr` (Stufe 6): das Festjahr des Jahreskreises am Ort, das den Tag enthält.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct KalenderJahr {
    pub zeitraum: KalenderSpanne,
    /// 12 oder 13 Mondmonate; leer zwischen den Wendekreisen und außerhalb der Jahre, für die gerechnet wird.
    pub monate: Vec<KalenderJahrMonat>,
    /// Die vier Wenden und Gleichen in der Spanne, nach der Zeit.
    pub sonnentage: Vec<KalenderZeitpunkt>,
    /// Neumond, erstes Viertel, Vollmond, letztes Viertel in der Spanne, nach der Zeit.
    pub viertel: Vec<KalenderZeitpunkt>,
    /// Ein Satz ohne Zahlen, wenn Monate fehlen.
    pub hinweis: Option<String>,
}
