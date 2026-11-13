//! Daten für die Bausteine: `projects/<slug>/daten/*.toml` (Zielbild,
//! Abschnitt b, „Daten für Bausteine“). Jeder Baustein hat genau einen Typ;
//! alle Zeilen stehen als Tabellen-Arrays mit Feldnamen, nie als Text mit
//! Trennzeichen. Unbekannte Felder sind ein Fehler.
//!
//! Eine Datei kann statt eines Ganzen auch benannte Teile haben
//! (`[teil.vergangenheit]`, `[teil.plan]`), siehe [`Datei`].

use std::collections::{BTreeMap, BTreeSet};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::anstehend::Stufe;
use super::bedeutung::Bedeutung;
use super::lesen::{LeseFehler, Mangel, toml_lesen};
use super::verlauf::Art;

// ----- Kennzahlen -----

/// `daten/kennzahlen.toml`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Kennzahlen {
    #[serde(default)]
    pub kennzahl: Vec<Kennzahl>,
}

/// Eine Kennzahl hat genau eines von `wert` (eingetippt) oder `berechnet`
/// (der Server rechnet, siehe `vertrag::abgeleitet`). In der API ist `wert`
/// immer gesetzt: bei `berechnet` steht dort das fertige Ergebnis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Kennzahl {
    pub titel: String,
    /// Als Text, damit „40–60“ oder „3 von 5“ gehen. In der TOML-Datei
    /// fehlt es bei `berechnet`; in der API steht immer ein Text da.
    #[serde(default)]
    #[ts(type = "string")]
    pub wert: Option<String>,
    /// Wie der Server den Wert rechnet; bleibt in der API zur Erklärung drin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub berechnet: Option<Berechnet>,
    #[serde(default)]
    pub einheit: Option<String>,
    #[serde(default)]
    pub hinweis: Option<String>,
    #[serde(default)]
    pub farbe: Option<Bedeutung>,
    /// Etappe B1: von wann die Momentaufnahme ist (JJJJ-MM-TT). Die
    /// Oberfläche zeigt „Stand vom …“; wie alt das ist, rechnet der Server
    /// (`stand_alter`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub stand: Option<String>,
    /// Ab wie vielen Tagen der Stand als alt gilt (gelb); ohne Angabe
    /// [`STAND_ALT_AB`]. Nur zusammen mit `stand`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub alt_ab: Option<u32>,
    /// Vom Server gerechnet, nie in der Datei (siehe [`StandAlter`]).
    #[serde(default, skip_deserializing, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub stand_alter: Option<StandAlter>,
}

// ----- Stand-Hinweis (Etappe B1) -----

/// Ohne `alt_ab` gilt ein Stand ab so vielen Tagen als alt.
pub const STAND_ALT_AB: u32 = 7;

/// Wie alt ein Stand ist (Etappe B1, eine Regel des Servers): `alt` ab
/// `alt_ab` Tagen (ohne Angabe [`STAND_ALT_AB`]), sonst `frisch`. Die
/// Farbe macht die Oberfläche daraus (alt: wartet, gelb).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum StandStufe {
    Frisch,
    Alt,
}

/// Alter eines Stands am Tag der Anfrage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StandAlter {
    /// Heute minus Stand in Tagen (0 ist heute; negativ, wenn der Stand in
    /// der Zukunft liegt, dann ist er `frisch`).
    pub tage: i64,
    pub stufe: StandStufe,
}

impl StandAlter {
    /// Aus dem Text `stand` (JJJJ-MM-TT); `None`, wenn es kein Datum ist.
    pub fn rechnen(stand: &str, alt_ab: Option<u32>, heute: chrono::NaiveDate) -> Option<Self> {
        let tag = chrono::NaiveDate::parse_from_str(stand.trim(), "%Y-%m-%d").ok()?;
        let tage = (heute - tag).num_days();
        let grenze = i64::from(alt_ab.unwrap_or(STAND_ALT_AB));
        Some(Self { tage, stufe: if tage >= grenze { StandStufe::Alt } else { StandStufe::Frisch } })
    }
}

/// Prüft `stand` und `alt_ab` eines Eintrags mit dem Titel `titel`.
fn stand_pruefen(titel: &str, stand: Option<&str>, alt_ab: Option<u32>) -> Result<(), Mangel> {
    match (stand, alt_ab) {
        (Some(s), _) if !ist_datum(s.trim(), false) => {
            Err(Mangel::bei("stand", s, format!("Bei „{titel}“ steht `stand = \"{s}\"`. Erlaubt ist ein Tag JJJJ-MM-TT, etwa `stand = \"2026-10-30\"`.")))
        }
        (None, Some(_)) => Err(Mangel::bei(
            "titel",
            titel,
            format!(
                "Bei „{titel}“ steht `alt_ab`, aber kein `stand`. `alt_ab` sagt, ab wie vielen Tagen der Stand alt ist; dazu gehört `stand = \"2026-10-30\"`."
            ),
        )),
        (Some(_), Some(0)) => {
            Err(Mangel::bei("titel", titel, format!("Bei „{titel}“ steht `alt_ab = 0`. Erlaubt ist 1 oder mehr (Tage), etwa `alt_ab = 14`.")))
        }
        _ => Ok(()),
    }
}

/// Der Wert, den `anzahl` und `anteil` mit `feld` zählen (Etappe B1):
/// Text (groß und klein egal), wahr/falsch oder eine Zahl.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum Treffer {
    Wahr(bool),
    Zahl(f64),
    Text(String),
}

impl Treffer {
    /// Passt der Wert eines Eintrags?
    pub fn passt(&self, v: &toml::Value) -> bool {
        match (self, v) {
            (Self::Wahr(a), toml::Value::Boolean(b)) => a == b,
            (Self::Zahl(a), toml::Value::Integer(b)) => *a == *b as f64,
            (Self::Zahl(a), toml::Value::Float(b)) => a == b,
            (Self::Text(a), toml::Value::String(b)) => a.trim().to_lowercase() == b.trim().to_lowercase(),
            _ => false,
        }
    }

    /// So, wie es in TOML stünde (für Meldungen).
    pub fn als_toml(&self) -> String {
        match self {
            Self::Wahr(b) => b.to_string(),
            Self::Zahl(z) => z.to_string(),
            Self::Text(t) => format!("\"{t}\""),
        }
    }
}

/// `berechnet = { quelle = "schritte", liste = "schritt", art = "anteil" }`:
/// der Server rechnet die Kennzahl aus einer anderen Datei in `daten/`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Berechnet {
    /// Dateiname ohne `.toml`, bei Teilen mit Punkt: `gantt` oder `zeitstrahl.plan`.
    pub quelle: String,
    /// Pfad der Liste in der Datei (bei Teilen innerhalb des Teils), etwa
    /// `schritt` oder `spalte.karte`. Darf fehlen, wenn es nur eine Liste gibt.
    #[serde(default)]
    pub liste: Option<String>,
    pub art: BerechnetArt,
    /// Die Spalte; nötig bei `summe`, `tage_seit` und `tage_bis`. Bei
    /// `anzahl` und `anteil` (Etappe B1) zusammen mit `wert`: gezählt wird,
    /// wo `feld` diesen Wert hat, statt nach dem Status-Feld.
    #[serde(default)]
    pub feld: Option<String>,
    /// Etappe B1: der Wert, der bei `anzahl` und `anteil` zählt, etwa
    /// `feld = "freigabe", wert = "freigegeben"` („2 von 5“, „40 %“).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub wert: Option<Treffer>,
}

/// Was gerechnet wird.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum BerechnetArt {
    /// Zahl der Einträge; mit Status-Feld „3 von 45“ (erledigt von allen).
    Anzahl,
    /// Anteil erledigter Einträge in Prozent („7 %“).
    Anteil,
    /// Summe einer Zahlenspalte.
    Summe,
    /// Tage seit dem letzten vergangenen Datum einer Datumsspalte.
    TageSeit,
    /// Tage bis zum nächsten Datum (heute oder später) einer Datumsspalte.
    TageBis,
}

impl BerechnetArt {
    pub fn wort(self) -> &'static str {
        match self {
            Self::Anzahl => "anzahl",
            Self::Anteil => "anteil",
            Self::Summe => "summe",
            Self::TageSeit => "tage_seit",
            Self::TageBis => "tage_bis",
        }
    }

    /// Braucht diese Art ein `feld`?
    pub fn braucht_feld(self) -> bool {
        matches!(self, Self::Summe | Self::TageSeit | Self::TageBis)
    }
}

// ----- Ampel -----

/// `daten/ampel.toml`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Ampel {
    #[serde(default)]
    pub zeile: Vec<AmpelZeile>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AmpelZeile {
    pub titel: String,
    pub farbe: Bedeutung,
    #[serde(default)]
    pub hinweis: Option<String>,
    /// Etappe B1: wann die Einschätzung zuletzt geprüft wurde (JJJJ-MM-TT),
    /// wie bei [`Kennzahl::stand`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub stand: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub alt_ab: Option<u32>,
    /// Vom Server gerechnet, nie in der Datei.
    #[serde(default, skip_deserializing, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub stand_alter: Option<StandAlter>,
}

// ----- Fortschritt -----

/// `daten/fortschritt.toml`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Fortschritt {
    #[serde(default)]
    pub zeile: Vec<FortschrittZeile>,
}

/// Wie weit ein Fortschritt ist (Etappe 3a+, eine Regel des Servers):
/// ab 100 % erreicht, ab 50 % ab der Hälfte, sonst unter der Hälfte. Die
/// Farbe macht die Oberfläche daraus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FortschrittStufe {
    Erreicht,
    AbHaelfte,
    UnterHaelfte,
}

impl FortschrittStufe {
    /// Aus Prozent (0 bis 100).
    pub fn aus_prozent(p: f64) -> Self {
        if p >= 100.0 {
            Self::Erreicht
        } else if p >= 50.0 {
            Self::AbHaelfte
        } else {
            Self::UnterHaelfte
        }
    }
}

/// Entweder `prozent` (0 bis 100) oder `erreicht` und `gesamt` („3 von 5“:
/// `erreicht = 3`, `gesamt = 5`). Seit Etappe B1 auch mit Kommazahl
/// (`erreicht = 0.2`, `gesamt = 1.5`, `einheit = "GB"`: „0,2 von 1,5 GB“).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FortschrittZeile {
    pub titel: String,
    #[serde(default)]
    pub prozent: Option<f32>,
    /// Wie viel erreicht ist; ganze Zahl oder Kommazahl, nicht negativ.
    #[serde(default)]
    pub erreicht: Option<f64>,
    /// Wie viel es insgesamt sind; größer als 0.
    #[serde(default)]
    pub gesamt: Option<f64>,
    /// Etappe B1: Einheit hinter „erreicht von gesamt“, etwa „GB“.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub einheit: Option<String>,
    #[serde(default)]
    pub hinweis: Option<String>,
    /// Vom Server beim Lesen gerechnet (Etappe 3a+), nie in der Datei: steht
    /// das Feld in der TOML, ist das ein Fehler (unbekanntes Feld).
    #[serde(default, skip_deserializing, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub stufe: Option<FortschrittStufe>,
}

impl FortschrittZeile {
    /// Der Anteil in Prozent (0 bis 100), aus `prozent` oder `erreicht`/`gesamt`.
    pub fn prozent_wert(&self) -> f64 {
        let p = match (self.prozent, self.erreicht, self.gesamt) {
            (Some(p), _, _) => f64::from(p),
            (None, Some(e), Some(g)) if g > 0.0 => e / g * 100.0,
            _ => 0.0,
        };
        if p.is_nan() {
            return 0.0;
        }
        p.clamp(0.0, 100.0)
    }
}

// ----- Zeitstrahl -----

/// `daten/zeitstrahl.toml`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Zeitstrahl {
    #[serde(default)]
    pub eintrag: Vec<ZeitstrahlEintrag>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ZeitstrahlEintrag {
    /// Stabile Kennung (Etappe 2e): Kleinbuchstaben, Ziffern, Bindestriche,
    /// je Datei nur einmal. Fehlt sie (alte Form, bis Schritt 6 erlaubt),
    /// bildet der Server sie aus Datum und Titel, und die Prüfung gibt einen Hinweis.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub kennung: Option<String>,
    /// JJJJ-MM-TT oder JJJJ-MM.
    pub datum: String,
    pub titel: String,
    #[serde(default)]
    pub text: Option<String>,
    /// Was der Eintrag fachlich ist (Etappe 2e): erledigt, begonnen, geplant,
    /// abgebrochen, routine, problem. Ersetzt `farbe`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub art: Option<Art>,
    /// Veraltet (bis Schritt 6 erlaubt, die Prüfung gibt einen Hinweis): wird
    /// zu `art` (problem zu problem, plan zu geplant, wartet zu begonnen, sonst erledigt).
    #[serde(default)]
    pub farbe: Option<Bedeutung>,
}

// ----- Gantt -----

/// `daten/gantt.toml`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Gantt {
    /// Optional, JJJJ-MM oder JJJJ-MM-TT: Plan-Monate (Zahlen bei `von`/`bis`) in echte Daten umrechnen.
    #[serde(default)]
    pub start: Option<String>,
    #[serde(default)]
    pub balken: Vec<GanttBalken>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct GanttBalken {
    pub name: String,
    pub von: Zeitpunkt,
    pub bis: Zeitpunkt,
    #[serde(default)]
    pub farbe: Option<Bedeutung>,
    /// Zeilen mit derselben Gruppe stehen zusammen.
    #[serde(default)]
    pub gruppe: Option<String>,
}

/// Ein Datum (`"2026-11"`, `"2026-11-03"`) oder ein Plan-Monat (`3`, `2.5`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum Zeitpunkt {
    Monat(f64),
    Datum(String),
}

// ----- Kanban -----

/// `daten/kanban.toml`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Kanban {
    #[serde(default)]
    pub spalte: Vec<KanbanSpalte>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct KanbanSpalte {
    pub titel: String,
    /// Farbe der Spalte; ohne Angabe entscheidet die Seite.
    #[serde(default)]
    pub farbe: Option<Bedeutung>,
    #[serde(default)]
    pub karte: Vec<KanbanKarte>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct KanbanKarte {
    pub titel: String,
    #[serde(default)]
    pub hinweis: Option<String>,
    #[serde(default)]
    pub farbe: Option<Bedeutung>,
}

// ----- Fluss -----

/// `daten/fluss.toml`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Fluss {
    #[serde(default)]
    pub kasten: Vec<FlussKasten>,
    #[serde(default)]
    pub pfeil: Vec<FlussPfeil>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlussKasten {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub farbe: Option<Bedeutung>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FlussPfeil {
    /// `id` eines Kastens.
    pub von: String,
    /// `id` eines Kastens.
    pub nach: String,
    #[serde(default)]
    pub text: Option<String>,
}

// ----- Fragen -----

/// `daten/fragen.toml`: Fragen an den Inhaber. Die Antworten liegen im Datenspeicher.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Fragen {
    #[serde(default)]
    pub frage: Vec<Frage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Frage {
    /// Bleibt für immer gleich; Kleinbuchstaben, Ziffern, Bindestriche.
    pub id: String,
    pub text: String,
    pub art: FrageArt,
    /// Nur bei `art = "wahl"`, mindestens zwei.
    #[serde(default)]
    pub optionen: Vec<String>,
    #[serde(default)]
    pub empfehlung: Option<String>,
    /// JJJJ-MM-TT.
    pub gestellt: String,
    /// Vorschlag des Chats für die Stufe (Etappe A1); ohne Angabe `bald`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub prioritaet: Option<Stufe>,
    /// Bis wann der Inhaber antworten sollte (JJJJ-MM-TT); hebt die Stufe an.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub frist: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum FrageArt {
    #[serde(rename = "wahl")]
    Wahl,
    #[serde(rename = "ja-nein")]
    JaNein,
    #[serde(rename = "text")]
    Text,
    /// Der Knopf „Konzept bestätigen“.
    #[serde(rename = "bestaetigen")]
    Bestaetigen,
}

// ----- Orte -----

/// `daten/orte.toml`: Dateien und Ordner auf diesem PC, die der Inhaber mit
/// „Öffnen“ aufmachen kann (Entscheidung lokal-oeffnen). Die
/// Oberfläche bekommt nur `kennung`, `titel` und `hinweis`; den Pfad kennt nur
/// der Server (er geht nie hinaus, auch nicht an Backend-Module).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Orte {
    #[serde(default)]
    pub ort: Vec<Ort>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Ort {
    /// Bleibt gleich; Kleinbuchstaben, Ziffern, Bindestriche. Die Oberfläche
    /// schickt nur sie (mit dem Projekt) an `POST /api/oeffnen`.
    pub kennung: String,
    pub titel: String,
    /// Vollständiger Pfad mit Laufwerk. Wird nie ausgeliefert.
    #[serde(skip_serializing)]
    #[ts(skip)]
    pub pfad: String,
    #[serde(default)]
    pub hinweis: Option<String>,
}

impl Ort {
    /// Für Tests und Beispiele.
    pub fn neu(kennung: &str, titel: &str, pfad: &str) -> Ort {
        Ort { kennung: kennung.into(), titel: titel.into(), pfad: pfad.into(), hinweis: None }
    }
}

// ----- Ganz oder in Teilen -----

/// Inhalt einer Datei: entweder ein Ganzes oder benannte Teile
/// (`[teil.plan]` → `teil.plan`). Im JSON: das Ganze selbst oder `{ "teil": { … } }`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(untagged)]
pub enum Datei<T> {
    Ganz(T),
    Teile {
        #[ts(as = "BTreeMap<String, T>")]
        teil: Teile<T>,
    },
}

impl<T> Datei<T> {
    /// Das Ganze oder alle Teile, in Reihenfolge der Datei.
    pub fn inhalte(&self) -> Vec<&T> {
        match self {
            Datei::Ganz(t) => vec![t],
            Datei::Teile { teil } => teil.iter().map(|(_, t)| t).collect(),
        }
    }

    /// Wie [`Datei::inhalte`], zum Ändern (etwa um berechnete Werte einzusetzen).
    pub fn inhalte_mut(&mut self) -> Vec<&mut T> {
        match self {
            Datei::Ganz(t) => vec![t],
            Datei::Teile { teil } => teil.0.iter_mut().map(|(_, t)| t).collect(),
        }
    }
}

/// Benannte Teile in der Reihenfolge der Datei (nicht nach dem Alphabet).
/// Im JSON ein Objekt, dessen Schlüssel in dieser Reihenfolge stehen; in
/// TypeScript `{ [key in string]: T }`.
#[derive(Debug, Clone, PartialEq)]
pub struct Teile<T>(pub Vec<(String, T)>);

impl<T> Teile<T> {
    /// Name und Inhalt, in Reihenfolge der Datei.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &T)> {
        self.0.iter().map(|(n, t)| (n.as_str(), t))
    }

    /// Der Teil mit dem Namen `name` (bisher nur in Tests gebraucht).
    #[cfg(test)]
    pub fn get(&self, name: &str) -> Option<&T> {
        self.iter().find(|(n, _)| *n == name).map(|(_, t)| t)
    }
}

impl<T: Serialize> Serialize for Teile<T> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut m = s.serialize_map(Some(self.0.len()))?;
        for (n, t) in &self.0 {
            m.serialize_entry(n, t)?;
        }
        m.end()
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Teile<T> {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Besucher<T>(std::marker::PhantomData<T>);
        impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Besucher<T> {
            type Value = Teile<T>;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a table")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut teile = Vec::new();
                while let Some((n, t)) = map.next_entry::<String, T>()? {
                    teile.push((n, t));
                }
                Ok(Teile(teile))
            }
        }
        d.deserialize_map(Besucher(std::marker::PhantomData))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NurTeile<T> {
    teil: Teile<T>,
}

/// Ein Datentyp eines Bausteins: Dateiname und Prüfung nach dem Lesen.
pub trait Baustein: DeserializeOwned {
    /// Dateiname ohne `.toml`.
    const DATEI: &'static str;

    fn pruefen(&self) -> Result<(), Mangel> {
        Ok(())
    }

    /// Nach dem Prüfen: was der Server aus den Eingaben rechnet und mitliefert
    /// (Etappe 3a+, etwa die Stufe eines Fortschritts).
    fn ergaenzen(&mut self) {}
}

/// Liest eine Baustein-Datei, ganz oder in Teilen, und prüft sie.
pub fn datei_lesen<T: Baustein>(datei: &str, text: &str) -> Result<Datei<T>, LeseFehler> {
    let tabelle: toml::Table = toml_lesen(datei, text)?;
    let quelle = text.strip_prefix('\u{feff}').unwrap_or(text);
    let inhalt = if tabelle.contains_key("teil") {
        let NurTeile { teil } = toml_lesen::<NurTeile<T>>(datei, text)?;
        if let Some((n, _)) = teil.iter().find(|(n, _)| !ist_kennung(n)) {
            return Err(LeseFehler {
                datei: datei.to_string(),
                zeile: quelle.lines().position(|l| l.contains(&format!("teil.{n}"))).map(|i| i + 1),
                text: format!("Der Teil „{n}“ hat keinen einfachen Namen. Erlaubt sind Kleinbuchstaben, Ziffern und Bindestriche, etwa `[teil.plan]`."),
            });
        }
        for (_, t) in teil.iter() {
            t.pruefen().map_err(|m| m.in_datei(datei, quelle))?;
        }
        let mut teil = teil;
        for (_, t) in teil.0.iter_mut() {
            t.ergaenzen();
        }
        Datei::Teile { teil }
    } else {
        let mut t: T = toml_lesen(datei, text)?;
        t.pruefen().map_err(|m| m.in_datei(datei, quelle))?;
        t.ergaenzen();
        Datei::Ganz(t)
    };
    Ok(inhalt)
}

/// Die Feldnamen, die ein Eintrag vom Typ `T` haben darf (Etappe B1, für
/// `feld` bei einer leeren Liste). Gelesen aus der Meldung von serde für ein
/// unbekanntes Feld (alle Typen hier haben `deny_unknown_fields`), damit die
/// Liste nie von den Typen abweicht.
pub fn feldnamen<T: DeserializeOwned>() -> Vec<String> {
    let Err(e) = toml::from_str::<T>("zz_kein_feld = 0") else { return Vec::new() };
    // „unknown field `zz_kein_feld`, expected one of `a`, `b`“ oder „expected `a`“
    e.message().split('`').skip(3).step_by(2).map(str::to_string).collect()
}

/// Kleinbuchstaben, Ziffern, Bindestriche; nicht leer, nicht mit `-` vorn.
pub fn ist_kennung(s: &str) -> bool {
    !s.is_empty() && s.len() <= 64 && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') && !s.starts_with('-')
}

/// Der feste Wert in `antwort`, wenn der Inhaber statt eines Klicks „Etwas
/// anderes …“ wählt und eigene Worte schreibt. Den
/// Knopf setzt die Oberfläche selbst; der Text steht im Feld `eigene`.
pub const ETWAS_ANDERES: &str = "etwas-anderes";

/// Heißt eine Option (einer Frage oder ein Knopf einer Aufgabe) wie
/// „Etwas anderes“? Die Oberfläche setzt diesen Knopf immer selbst; eine
/// solche Option stünde doppelt da (und `etwas-anderes` ist der feste Wert).
/// Groß und klein, Bindestriche, Punkte und „…“ am Ende zählen nicht.
pub fn heisst_wie_etwas_anderes(option: &str) -> bool {
    let klein = option.to_lowercase().replace(['-', '_'], " ");
    let ohne_ende = klein.trim().trim_end_matches(['…', '.', ' ', ':']);
    ohne_ende.split_whitespace().collect::<Vec<_>>().join(" ") == "etwas anderes"
}

/// JJJJ-MM-TT oder (mit `monat_reicht`) JJJJ-MM, als echtes Datum.
pub fn ist_datum(s: &str, monat_reicht: bool) -> bool {
    let teile: Vec<&str> = s.split('-').collect();
    let zahl = |t: &str, n: usize| t.len() == n && t.chars().all(|c| c.is_ascii_digit());
    match teile.as_slice() {
        [j, m] if monat_reicht => zahl(j, 4) && zahl(m, 2) && (1..=12).contains(&m.parse::<u32>().unwrap_or(0)),
        [j, m, t] => {
            zahl(j, 4)
                && zahl(m, 2)
                && zahl(t, 2)
                && chrono::NaiveDate::from_ymd_opt(j.parse().unwrap_or(0), m.parse().unwrap_or(0), t.parse().unwrap_or(0)).is_some()
        }
        _ => false,
    }
}

impl Baustein for Kennzahlen {
    const DATEI: &'static str = "kennzahlen";

    /// Genau eines von `wert` und `berechnet`; `feld`, wo die Art es braucht.
    /// Ob es Quelle, Liste und Spalte gibt, prüft `abgeleitet::kennzahlen_rechnen`
    /// (dafür braucht es die anderen Dateien).
    fn pruefen(&self) -> Result<(), Mangel> {
        for k in &self.kennzahl {
            let t = &k.titel;
            match (&k.wert, &k.berechnet) {
                (Some(_), Some(_)) => {
                    return Err(Mangel::bei(
                        "titel",
                        t,
                        format!(
                            "Bei der Kennzahl „{t}“ stehen `wert` und `berechnet`. Bitte genau eines: `wert` für einen eingetippten Wert oder `berechnet`, wenn der Server rechnen soll."
                        ),
                    ));
                }
                (None, None) => {
                    return Err(Mangel::bei(
                        "titel",
                        t,
                        format!(
                            "Der Kennzahl „{t}“ fehlt `wert` oder `berechnet`. Beispiel: `wert = \"45 kg\"` oder `berechnet = {{ quelle = \"eigen-schritte\", art = \"anzahl\" }}`"
                        ),
                    ));
                }
                (None, Some(b)) => {
                    if b.art.braucht_feld() && b.feld.as_deref().is_none_or(|f| f.trim().is_empty()) {
                        return Err(Mangel::bei(
                            "titel",
                            t,
                            format!(
                                "Bei der Kennzahl „{t}“ braucht `art = \"{}\"` auch `feld`, die Spalte, aus der gerechnet wird. Beispiel: `feld = \"kosten\"`",
                                b.art.wort()
                            ),
                        ));
                    }
                    let feld = b.feld.as_deref().filter(|f| !f.trim().is_empty());
                    match (b.art.braucht_feld(), feld, &b.wert) {
                        (true, _, Some(w)) => {
                            return Err(Mangel::bei(
                                "titel",
                                t,
                                format!(
                                    "Bei der Kennzahl „{t}“ steht `wert = {}` in `berechnet`. Einen Wert zum Zählen gibt es nur bei `art = \"anzahl\"` und `art = \"anteil\"`.",
                                    w.als_toml()
                                ),
                            ));
                        }
                        (false, None, Some(w)) => {
                            return Err(Mangel::bei(
                                "titel",
                                t,
                                format!(
                                    "Bei der Kennzahl „{t}“ steht `wert = {}` in `berechnet`, aber kein `feld`. Gezählt wird, wo die Spalte `feld` diesen Wert hat, etwa `feld = \"freigabe\", wert = \"freigegeben\"`.",
                                    w.als_toml()
                                ),
                            ));
                        }
                        (false, Some(f), None) => {
                            return Err(Mangel::bei(
                                "titel",
                                t,
                                format!(
                                    "Bei der Kennzahl „{t}“ steht `feld = \"{f}\"`, aber kein `wert`. Bei `art = \"{}\"` zählt der Server, wo `{f}` einen Wert hat, etwa `wert = \"freigegeben\"`; ohne `feld` zählt er nach dem Status-Feld.",
                                    b.art.wort()
                                ),
                            ));
                        }
                        _ => {}
                    }
                }
                (Some(_), None) => {}
            }
            stand_pruefen(t, k.stand.as_deref(), k.alt_ab)?;
        }
        Ok(())
    }
}

impl Baustein for Ampel {
    const DATEI: &'static str = "ampel";

    fn pruefen(&self) -> Result<(), Mangel> {
        for z in &self.zeile {
            stand_pruefen(&z.titel, z.stand.as_deref(), z.alt_ab)?;
        }
        Ok(())
    }
}

impl Baustein for Fortschritt {
    const DATEI: &'static str = "fortschritt";

    fn ergaenzen(&mut self) {
        for z in &mut self.zeile {
            z.stufe = Some(FortschrittStufe::aus_prozent(z.prozent_wert()));
        }
    }

    fn pruefen(&self) -> Result<(), Mangel> {
        for z in &self.zeile {
            if z.prozent.is_some() && z.einheit.is_some() {
                return Err(Mangel::bei(
                    "titel",
                    &z.titel,
                    format!(
                        "Bei „{}“ steht `einheit` neben `prozent`. Die Einheit gehört zu `erreicht` und `gesamt`, etwa `erreicht = 0.2`, `gesamt = 1.5`, `einheit = \"GB\"`.",
                        z.titel
                    ),
                ));
            }
            match (z.prozent, z.erreicht, z.gesamt) {
                (Some(p), None, None) if (0.0..=100.0).contains(&p) => {}
                (Some(p), None, None) => {
                    return Err(Mangel::bei("titel", &z.titel, format!("Bei „{}“ steht `prozent = {p}`. Erlaubt ist 0 bis 100.", z.titel)));
                }
                (None, Some(v), Some(b)) if b.is_finite() && v.is_finite() && b > 0.0 && (0.0..=b).contains(&v) => {}
                (None, Some(_), Some(_)) => {
                    return Err(Mangel::bei(
                        "titel",
                        &z.titel,
                        format!(
                            "Bei „{}“ passen `erreicht` und `gesamt` nicht: `gesamt` muss größer als 0 sein und `erreicht` zwischen 0 und `gesamt`. Beispiel: `erreicht = 3` und `gesamt = 5` oder mit Komma `erreicht = 0.2` und `gesamt = 1.5`",
                            z.titel
                        ),
                    ));
                }
                _ => {
                    return Err(Mangel::bei(
                        "titel",
                        &z.titel,
                        format!(
                            "Bei „{}“ steht entweder `prozent` oder `erreicht` und `gesamt`. Beispiel: `prozent = 70` oder `erreicht = 3` und `gesamt = 5`",
                            z.titel
                        ),
                    ));
                }
            }
        }
        Ok(())
    }
}

impl Baustein for Zeitstrahl {
    const DATEI: &'static str = "zeitstrahl";

    fn pruefen(&self) -> Result<(), Mangel> {
        for e in &self.eintrag {
            if !ist_datum(&e.datum, true) {
                return Err(Mangel::bei(
                    "datum",
                    &e.datum,
                    format!("Bei `datum` steht „{}“. Erlaubt ist JJJJ-MM-TT oder JJJJ-MM, etwa `datum = \"2026-10-24\"`.", e.datum),
                ));
            }
            if let Some(k) = e.kennung.as_deref().filter(|k| !ist_kennung(k)) {
                return Err(Mangel::bei(
                    "kennung",
                    k,
                    format!(
                        "Der Eintrag „{}“ hat keine einfache `kennung`. Erlaubt sind Kleinbuchstaben, Ziffern und Bindestriche, etwa `kennung = \"pilot-start\"`.",
                        e.titel
                    ),
                ));
            }
        }
        Ok(())
    }
}

impl Baustein for Orte {
    const DATEI: &'static str = "orte";

    /// Nur die Form der Einträge; ob ein Pfad erlaubt ist, meldet `pruefe`
    /// (`inhalt::pruefung`), und Server und Bote prüfen es bei jedem Öffnen.
    fn pruefen(&self) -> Result<(), Mangel> {
        for o in &self.ort {
            if !ist_kennung(&o.kennung) {
                return Err(Mangel::bei(
                    "kennung",
                    &o.kennung,
                    format!(
                        "Der Ort „{}“ hat keine einfache `kennung`. Erlaubt sind Kleinbuchstaben, Ziffern und Bindestriche, etwa `kennung = \"bauplan\"`.",
                        o.titel
                    ),
                ));
            }
            if o.titel.trim().is_empty() {
                return Err(Mangel::bei("kennung", &o.kennung, format!("Der Ort „{}“ hat keinen `titel`.", o.kennung)));
            }
            if o.pfad.trim().is_empty() {
                return Err(Mangel::bei("kennung", &o.kennung, format!("Beim Ort „{}“ fehlt `pfad`.", o.kennung)));
            }
        }
        Ok(())
    }
}

/// Orte: jede `kennung` nur einmal in der ganzen Datei, auch über Teile hinweg
/// (sie ist der Schlüssel fürs Öffnen). Die Zeile zeigt auf das zweite Vorkommen.
fn orte_kennungen_eindeutig(o: &Datei<Orte>, datei: &str, text: &str) -> Result<(), LeseFehler> {
    let mut gesehen = BTreeSet::new();
    for k in o.inhalte().into_iter().flat_map(|t| &t.ort).map(|o| o.kennung.as_str()) {
        if !gesehen.insert(k) {
            let quelle = text.strip_prefix('\u{feff}').unwrap_or(text);
            return Err(Mangel::beim_zweiten("kennung", k, format!("Den Ort „{k}“ gibt es zweimal. Jede `kennung` nur einmal, auch über Teile hinweg."))
                .in_datei(datei, quelle));
        }
    }
    Ok(())
}

impl Datei<Orte> {
    /// Der Ort mit dieser Kennung (in allen Teilen).
    pub fn ort(&self, kennung: &str) -> Option<&Ort> {
        self.inhalte().into_iter().flat_map(|t| &t.ort).find(|o| o.kennung == kennung)
    }
}

impl Baustein for Gantt {
    const DATEI: &'static str = "gantt";

    fn pruefen(&self) -> Result<(), Mangel> {
        if let Some(s) = &self.start
            && !ist_datum(s, true)
        {
            return Err(Mangel::bei("start", s, format!("Bei `start` steht „{s}“. Erlaubt ist JJJJ-MM oder JJJJ-MM-TT, etwa `start = \"2026-11\"`.")));
        }
        for b in &self.balken {
            for (feld, z) in [("von", &b.von), ("bis", &b.bis)] {
                if let Zeitpunkt::Datum(d) = z
                    && !ist_datum(d, true)
                {
                    return Err(Mangel::bei(
                        feld,
                        d,
                        format!("Beim Balken „{}“ steht `{feld} = \"{d}\"`. Erlaubt ist JJJJ-MM, JJJJ-MM-TT oder eine Monatszahl wie 3.", b.name),
                    ));
                }
            }
            let vorher = match (&b.von, &b.bis) {
                (Zeitpunkt::Monat(v), Zeitpunkt::Monat(e)) => e < v,
                // JJJJ-MM und JJJJ-MM-TT lassen sich als Text vergleichen.
                (Zeitpunkt::Datum(v), Zeitpunkt::Datum(e)) => e < v,
                _ => {
                    return Err(Mangel::bei(
                        "name",
                        &b.name,
                        format!("Beim Balken „{}“ sind `von` und `bis` verschieden: bitte beide als Datum oder beide als Monatszahl.", b.name),
                    ));
                }
            };
            if vorher {
                return Err(Mangel::bei("name", &b.name, format!("Beim Balken „{}“ liegt `bis` vor `von`.", b.name)));
            }
        }
        // Über alle Balken: entweder nur Daten oder nur Plan-Monate, auch mit `start`.
        let monate = self.balken.iter().position(|b| matches!(b.von, Zeitpunkt::Monat(_)));
        let daten = self.balken.iter().position(|b| matches!(b.von, Zeitpunkt::Datum(_)));
        if let (Some(mi), Some(di)) = (monate, daten) {
            // Die Zeile zeigt auf den ersten Balken, der die andere Art hat.
            let spaeter = &self.balken[mi.max(di)].name;
            let (m, d) = (&self.balken[mi], &self.balken[di]);
            return Err(Mangel::bei(
                "name",
                spaeter,
                format!(
                    "Der Balkenplan mischt Plan-Monate und Daten: „{}“ hat Monatszahlen, „{}“ hat Daten. Bitte alle Balken gleich, entweder `von = 0` oder `von = \"2026-11\"`.",
                    m.name, d.name
                ),
            ));
        }
        Ok(())
    }
}

impl Baustein for Kanban {
    const DATEI: &'static str = "kanban";
}

impl Baustein for Fluss {
    const DATEI: &'static str = "fluss";

    fn pruefen(&self) -> Result<(), Mangel> {
        let mut ids = BTreeSet::new();
        for k in &self.kasten {
            if !ids.insert(k.id.as_str()) {
                return Err(Mangel::beim_zweiten("id", &k.id, format!("Den Kasten „{}“ gibt es zweimal. Jede `id` nur einmal.", k.id)));
            }
        }
        for p in &self.pfeil {
            for (feld, id) in [("von", &p.von), ("nach", &p.nach)] {
                if !ids.contains(id.as_str()) {
                    return Err(Mangel::bei(
                        feld,
                        id,
                        format!("Ein Pfeil zeigt bei `{feld}` auf „{id}“, aber diesen Kasten gibt es nicht. Erst `[[kasten]]` mit `id = \"{id}\"` anlegen."),
                    ));
                }
            }
        }
        Ok(())
    }
}

impl Baustein for Fragen {
    const DATEI: &'static str = "fragen";

    fn pruefen(&self) -> Result<(), Mangel> {
        let mut ids = BTreeSet::new();
        for f in &self.frage {
            if !ist_kennung(&f.id) {
                return Err(Mangel::bei(
                    "id",
                    &f.id,
                    format!(
                        "Die Frage „{}“ hat keine einfache `id`. Erlaubt sind Kleinbuchstaben, Ziffern und Bindestriche, etwa `id = \"schlusssatz\"`.",
                        f.id
                    ),
                ));
            }
            if !ids.insert(f.id.as_str()) {
                return Err(Mangel::beim_zweiten("id", &f.id, format!("Die Frage „{}“ gibt es zweimal. Jede `id` nur einmal.", f.id)));
            }
            if f.art == FrageArt::Wahl && f.optionen.len() < 2 {
                return Err(Mangel::bei(
                    "id",
                    &f.id,
                    format!("Die Frage „{}“ ist eine Wahl und braucht mindestens zwei `optionen`. Beispiel: `optionen = [\"A\", \"B\"]`", f.id),
                ));
            }
            if f.art != FrageArt::Wahl && !f.optionen.is_empty() {
                return Err(Mangel::bei("id", &f.id, format!("Die Frage „{}“ hat `optionen`, die gibt es nur bei `art = \"wahl\"`.", f.id)));
            }
            if let Some(o) = f.optionen.iter().find(|o| heisst_wie_etwas_anderes(o)) {
                return Err(Mangel::bei(
                    "id",
                    &f.id,
                    format!(
                        "Die Frage „{}“ hat die Option „{o}“. „Etwas anderes …“ setzt das Dashboard bei jeder Klick-Antwort selbst dazu; bitte die Option weglassen.",
                        f.id
                    ),
                ));
            }
            if !ist_datum(&f.gestellt, false) {
                return Err(Mangel::bei(
                    "gestellt",
                    &f.gestellt,
                    format!("Bei `gestellt` steht „{}“. Erlaubt ist JJJJ-MM-TT, etwa `gestellt = \"2026-10-25\"`.", f.gestellt),
                ));
            }
            if let Some(frist) = f.frist.as_deref().filter(|d| !ist_datum(d, false)) {
                return Err(Mangel::bei(
                    "frist",
                    frist,
                    format!("Bei der Frage „{}“ steht `frist = \"{frist}\"`. Erlaubt ist JJJJ-MM-TT, etwa `frist = \"2026-11-05\"`.", f.id),
                ));
            }
        }
        Ok(())
    }
}

/// Fragen in Teilen: jede `id` nur einmal in der ganzen Datei, nicht nur je
/// Teil (sie ist der Schlüssel für Einstufung, Antwort und Ziel). Die Zeile
/// zeigt auf das zweite Vorkommen.
fn fragen_ids_eindeutig(fragen: &Datei<Fragen>, datei: &str, text: &str) -> Result<(), LeseFehler> {
    let mut ids = BTreeSet::new();
    for f in fragen.inhalte().into_iter().flat_map(|t| &t.frage) {
        if !ids.insert(f.id.as_str()) {
            let quelle = text.strip_prefix('\u{feff}').unwrap_or(text);
            return Err(Mangel::beim_zweiten("id", &f.id, format!("Die Frage „{}“ gibt es zweimal. Jede `id` nur einmal, auch über Teile hinweg.", f.id))
                .in_datei(datei, quelle));
        }
    }
    Ok(())
}

/// Zeitstrahl (Etappe 2e): jede `kennung` nur einmal in der ganzen Datei,
/// auch über Teile hinweg (sie ist die Kennung des Ereignisses im Verlauf).
/// Die Zeile zeigt auf das zweite Vorkommen.
fn zeitstrahl_kennungen_eindeutig(z: &Datei<Zeitstrahl>, datei: &str, text: &str) -> Result<(), LeseFehler> {
    let mut gesehen = BTreeSet::new();
    for k in z.inhalte().into_iter().flat_map(|t| &t.eintrag).filter_map(|e| e.kennung.as_deref()) {
        if !gesehen.insert(k) {
            let quelle = text.strip_prefix('\u{feff}').unwrap_or(text);
            return Err(Mangel::beim_zweiten("kennung", k, format!("Die Kennung „{k}“ gibt es zweimal. Jede `kennung` nur einmal, auch über Teile hinweg."))
                .in_datei(datei, quelle));
        }
    }
    Ok(())
}

// ----- Alles zusammen -----

/// Alle Daten eines Projekts (`daten/*.toml`), so wie die Seite sie bekommt.
/// Fehlt eine Datei, steht hier `null`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
pub struct ProjektDaten {
    pub kennzahlen: Option<Datei<Kennzahlen>>,
    pub ampel: Option<Datei<Ampel>>,
    pub fortschritt: Option<Datei<Fortschritt>>,
    pub zeitstrahl: Option<Datei<Zeitstrahl>>,
    pub gantt: Option<Datei<Gantt>>,
    pub kanban: Option<Datei<Kanban>>,
    pub fluss: Option<Datei<Fluss>>,
    pub fragen: Option<Datei<Fragen>>,
    /// `daten/orte.toml` (Entscheidung lokal-oeffnen), ohne Pfade.
    pub orte: Option<Datei<Orte>>,
    /// `daten/fristen.toml` (Etappe B3): Ereignisse und Fristen. Nur mit
    /// Windows-Anmeldung (`/api/projects/<slug>` lässt sie sonst weg).
    pub fristen: Option<Datei<super::fristen::Fristen>>,
    /// `daten/plan.toml` (Etappe B8): die Etappen, nur Absicht.
    pub plan: Option<Datei<super::plan::Plan>>,
    /// `daten/eigen-<name>.toml` unter dem Schlüssel `<name>`, als JSON durchgereicht.
    #[ts(type = "Record<string, unknown>")]
    pub eigen: BTreeMap<String, serde_json::Value>,
}

/// Der Inhalt einer einzelnen Datei aus `daten/`
/// (`GET /api/projekte/<slug>/daten/<name>`): genau das, was in
/// [`ProjektDaten`] für diese Datei steht. Welcher Typ es ist, sagt der Name.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(untagged)]
pub enum DatenInhalt {
    Kennzahlen(Datei<Kennzahlen>),
    Ampel(Datei<Ampel>),
    Fortschritt(Datei<Fortschritt>),
    Zeitstrahl(Datei<Zeitstrahl>),
    Gantt(Datei<Gantt>),
    Kanban(Datei<Kanban>),
    Fluss(Datei<Fluss>),
    Fragen(Datei<Fragen>),
    Orte(Datei<Orte>),
    Fristen(Datei<super::fristen::Fristen>),
    Plan(Datei<super::plan::Plan>),
    /// `eigen-<name>.toml`, als JSON durchgereicht.
    Eigen(#[ts(type = "Record<string, unknown>")] serde_json::Value),
}

/// Ist `name` (ohne `.toml`) ein Name, den es in `daten/` geben darf?
/// Die festen Namen aus [`DATEIEN`] und `eigen-<name>`.
pub fn ist_datenname(name: &str) -> bool {
    DATEIEN.contains(&name) || name.strip_prefix("eigen-").is_some_and(ist_kennung)
}

impl ProjektDaten {
    /// Nimmt den Inhalt der Datei `name` (ohne `.toml`) heraus; `None`, wenn
    /// sie nicht gelesen wurde.
    pub fn nimm(&mut self, name: &str) -> Option<DatenInhalt> {
        Some(match name {
            "kennzahlen" => DatenInhalt::Kennzahlen(self.kennzahlen.take()?),
            "ampel" => DatenInhalt::Ampel(self.ampel.take()?),
            "fortschritt" => DatenInhalt::Fortschritt(self.fortschritt.take()?),
            "zeitstrahl" => DatenInhalt::Zeitstrahl(self.zeitstrahl.take()?),
            "gantt" => DatenInhalt::Gantt(self.gantt.take()?),
            "kanban" => DatenInhalt::Kanban(self.kanban.take()?),
            "fluss" => DatenInhalt::Fluss(self.fluss.take()?),
            "fragen" => DatenInhalt::Fragen(self.fragen.take()?),
            "orte" => DatenInhalt::Orte(self.orte.take()?),
            "fristen" => DatenInhalt::Fristen(self.fristen.take()?),
            "plan" => DatenInhalt::Plan(self.plan.take()?),
            _ => DatenInhalt::Eigen(self.eigen.remove(name.strip_prefix("eigen-")?)?),
        })
    }
}

impl ProjektDaten {
    /// Etappe B1: setzt bei Kennzahlen und Ampel-Zeilen mit `stand` das
    /// Alter am Tag `heute` (Stufe `alt` ab `alt_ab` Tagen).
    pub fn stand_rechnen(&mut self, heute: chrono::NaiveDate) {
        for k in self.kennzahlen.iter_mut().flat_map(Datei::inhalte_mut).flat_map(|t| t.kennzahl.iter_mut()) {
            k.stand_alter = k.stand.as_deref().and_then(|s| StandAlter::rechnen(s, k.alt_ab, heute));
        }
        for z in self.ampel.iter_mut().flat_map(Datei::inhalte_mut).flat_map(|t| t.zeile.iter_mut()) {
            z.stand_alter = z.stand.as_deref().and_then(|s| StandAlter::rechnen(s, z.alt_ab, heute));
        }
    }
}

/// Die Dateinamen, die in `daten/` stehen dürfen (ohne `eigen-<name>.toml`).
pub const DATEIEN: [&str; 11] = [
    Kennzahlen::DATEI,
    Ampel::DATEI,
    Fortschritt::DATEI,
    Zeitstrahl::DATEI,
    Gantt::DATEI,
    Kanban::DATEI,
    Fluss::DATEI,
    Fragen::DATEI,
    Orte::DATEI,
    super::fristen::Fristen::DATEI,
    super::plan::Plan::DATEI,
];

fn einsetzen<T: Baustein>(feld: &mut Option<Datei<T>>, datei: &str, text: &str) -> Result<(), LeseFehler> {
    *feld = Some(datei_lesen::<T>(datei, text)?);
    Ok(())
}

/// TOML-Wert als JSON; Datum und Zeit werden Text.
fn toml_zu_json(v: toml::Value) -> serde_json::Value {
    use serde_json::Value as J;
    match v {
        toml::Value::String(s) => J::String(s),
        toml::Value::Integer(i) => J::from(i),
        toml::Value::Float(f) => serde_json::Number::from_f64(f).map(J::Number).unwrap_or(J::Null),
        toml::Value::Boolean(b) => J::Bool(b),
        toml::Value::Datetime(d) => J::String(d.to_string()),
        toml::Value::Array(a) => J::Array(a.into_iter().map(toml_zu_json).collect()),
        toml::Value::Table(t) => J::Object(t.into_iter().map(|(k, v)| (k, toml_zu_json(v))).collect()),
    }
}

impl ProjektDaten {
    /// Liest eine Datei aus `daten/`. `name` ist der Dateiname (`gantt.toml`),
    /// `datei` der Pfad für Meldungen (`projects/<slug>/daten/gantt.toml`).
    pub fn datei_einlesen(&mut self, name: &str, datei: &str, text: &str) -> Result<(), LeseFehler> {
        let fehler = |t: String| LeseFehler { datei: datei.to_string(), zeile: None, text: t };
        let Some(stamm) = name.strip_suffix(".toml") else {
            return Err(fehler(format!(
                "In `daten/` liegen nur TOML-Dateien. Bilder und andere Dateien gehören nach `assets/`.{}",
                if name.ends_with(".md") { " Text steht in Svelte oder als `text = \"\"\"…\"\"\"` in TOML." } else { "" }
            )));
        };
        match stamm {
            "kennzahlen" => einsetzen(&mut self.kennzahlen, datei, text),
            "ampel" => einsetzen(&mut self.ampel, datei, text),
            "fortschritt" => einsetzen(&mut self.fortschritt, datei, text),
            "zeitstrahl" => {
                einsetzen(&mut self.zeitstrahl, datei, text)?;
                match &self.zeitstrahl {
                    Some(z) => zeitstrahl_kennungen_eindeutig(z, datei, text),
                    None => Ok(()),
                }
            }
            "gantt" => einsetzen(&mut self.gantt, datei, text),
            "kanban" => einsetzen(&mut self.kanban, datei, text),
            "fluss" => einsetzen(&mut self.fluss, datei, text),
            "fragen" => {
                einsetzen(&mut self.fragen, datei, text)?;
                match &self.fragen {
                    Some(f) => fragen_ids_eindeutig(f, datei, text),
                    None => Ok(()),
                }
            }
            "orte" => {
                einsetzen(&mut self.orte, datei, text)?;
                match &self.orte {
                    Some(o) => orte_kennungen_eindeutig(o, datei, text),
                    None => Ok(()),
                }
            }
            "fristen" => {
                einsetzen(&mut self.fristen, datei, text)?;
                match &self.fristen {
                    Some(f) => super::fristen::fristen_ueber_die_datei(f, datei, text),
                    None => Ok(()),
                }
            }
            "plan" => {
                einsetzen(&mut self.plan, datei, text)?;
                match &self.plan {
                    Some(p) => super::plan::plan_ueber_die_datei(p, datei, text),
                    None => Ok(()),
                }
            }
            _ => match stamm.strip_prefix("eigen-") {
                Some(n) if ist_kennung(n) => {
                    let t: toml::Table = toml_lesen(datei, text)?;
                    self.eigen.insert(n.to_string(), toml_zu_json(toml::Value::Table(t)));
                    Ok(())
                }
                Some(_) => Err(fehler(
                    "Der Name nach `eigen-` ist nicht einfach. Erlaubt sind Kleinbuchstaben, Ziffern und Bindestriche, etwa `eigen-notizen.toml`.".to_string(),
                )),
                None => Err(fehler(format!(
                    "Die Datei `{name}` kennt der Server nicht. Erlaubt sind: {}.toml und eigen-<name>.toml für eigene Daten.",
                    DATEIEN.join(".toml, ")
                ))),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lesen(name: &str, text: &str) -> Result<ProjektDaten, LeseFehler> {
        let mut d = ProjektDaten::default();
        d.datei_einlesen(name, &format!("projects/x/daten/{name}"), text)?;
        Ok(d)
    }

    #[test]
    fn kennzahlen_mit_farbwort() {
        let d = lesen("kennzahlen.toml", "[[kennzahl]]\ntitel = \"Regenwasser\"\nwert = \"40–60\"\neinheit = \"Liter\"\nfarbe = \"grün\"\n").unwrap();
        let Some(Datei::Ganz(k)) = d.kennzahlen else { panic!() };
        assert_eq!(k.kennzahl[0].farbe, Some(Bedeutung::Ok));
        let json = serde_json::to_value(&k).unwrap();
        assert_eq!(json["kennzahl"][0]["farbe"], "ok");
        let e = lesen("kennzahlen.toml", "[[kennzahl]]\ntitel = \"A\"\nwert = 42\n").unwrap_err();
        assert_eq!(e.zeile, Some(3));
        assert!(e.text.contains("`wert = \"42\"`"), "{e}");
    }

    #[test]
    fn ampel_braucht_farbe() {
        let d = lesen("ampel.toml", "[[zeile]]\ntitel = \"Backup\"\nfarbe = \"rot\"\nhinweis = \"nur eine Kopie\"\n").unwrap();
        let Some(Datei::Ganz(a)) = d.ampel else { panic!() };
        assert_eq!(a.zeile[0].farbe, Bedeutung::Problem);
        let e = lesen("ampel.toml", "[[zeile]]\ntitel = \"Backup\"\nfarbe = \"lila\"\n").unwrap_err();
        assert_eq!(e.zeile, Some(3));
        assert!(e.text.starts_with("Bei `farbe` steht „lila“. Erlaubt sind: ok, laeuft"), "{e}");
    }

    #[test]
    fn fortschritt_prozent_oder_von_bis() {
        assert!(lesen("fortschritt.toml", "[[zeile]]\ntitel = \"A\"\nprozent = 70\n").is_ok());
        assert!(lesen("fortschritt.toml", "[[zeile]]\ntitel = \"A\"\nerreicht = 3\ngesamt = 5\n").is_ok());
        let e = lesen("fortschritt.toml", "[[zeile]]\ntitel = \"A\"\nvon = 3\nbis = 5\n").unwrap_err();
        assert!(e.text.contains("gibt es hier nicht. Erlaubt sind: titel, prozent, erreicht, gesamt"), "{e}");
        let e = lesen("fortschritt.toml", "[[zeile]]\ntitel = \"A\"\nerreicht = 6\ngesamt = 5\n").unwrap_err();
        assert!(e.text.contains("`erreicht` zwischen 0 und `gesamt`"), "{e}");
        let e = lesen("fortschritt.toml", "[[zeile]]\ntitel = \"A\"\n\n[[zeile]]\ntitel = \"B\"\n").unwrap_err();
        assert_eq!(e.zeile, Some(2));
        assert!(e.text.contains("entweder `prozent`"), "{e}");
        assert!(lesen("fortschritt.toml", "[[zeile]]\ntitel = \"A\"\nprozent = 170\n").is_err());
        // Etappe 3a+: die Stufe rechnet der Server beim Lesen, auch in Teilen; in der Datei ist sie ein Fehler.
        let d = lesen("fortschritt.toml", "[[zeile]]\ntitel = \"A\"\nprozent = 100\n\n[[zeile]]\ntitel = \"B\"\nerreicht = 1\ngesamt = 2\n\n[[zeile]]\ntitel = \"C\"\nerreicht = 2\ngesamt = 5\n").unwrap();
        let json = serde_json::to_value(&d).unwrap();
        let stufen: Vec<&str> = json["fortschritt"]["zeile"].as_array().unwrap().iter().map(|z| z["stufe"].as_str().unwrap()).collect();
        assert_eq!(stufen, ["erreicht", "ab-haelfte", "unter-haelfte"]);
        let d = lesen("fortschritt.toml", "[teil.a]\n[[teil.a.zeile]]\ntitel = \"A\"\nprozent = 49.9\n").unwrap();
        assert_eq!(serde_json::to_value(&d).unwrap()["fortschritt"]["teil"]["a"]["zeile"][0]["stufe"], "unter-haelfte");
        assert!(lesen("fortschritt.toml", "[[zeile]]\ntitel = \"A\"\nprozent = 70\nstufe = \"erreicht\"\n").is_err());
    }

    /// Etappe B1: Fortschritt mit Kommazahl und Einheit („0,2 von 1,5 GB“).
    #[test]
    fn fortschritt_mit_kommazahl() {
        let d = lesen("fortschritt.toml", "[[zeile]]\ntitel = \"Ablage\"\nerreicht = 0.2\ngesamt = 1.5\neinheit = \"GB\"\n").unwrap();
        let Some(Datei::Ganz(f)) = &d.fortschritt else { panic!() };
        let z = &f.zeile[0];
        assert_eq!((z.erreicht, z.gesamt, z.einheit.as_deref()), (Some(0.2), Some(1.5), Some("GB")));
        assert!((z.prozent_wert() - 13.333).abs() < 0.01);
        assert_eq!(z.stufe, Some(FortschrittStufe::UnterHaelfte));
        let json = serde_json::to_value(&d).unwrap();
        assert_eq!(json["fortschritt"]["zeile"][0]["erreicht"], 0.2);
        assert_eq!(json["fortschritt"]["zeile"][0]["einheit"], "GB");
        // Ganze Zahlen gehen weiter; ohne Einheit fehlt das Feld im JSON.
        let d = lesen("fortschritt.toml", "[[zeile]]\ntitel = \"A\"\nerreicht = 3\ngesamt = 5\n").unwrap();
        let json = serde_json::to_value(&d).unwrap();
        assert_eq!((json["fortschritt"]["zeile"][0]["erreicht"].as_f64(), json["fortschritt"]["zeile"][0].get("einheit")), (Some(3.0), None));
        let e = lesen("fortschritt.toml", "[[zeile]]\ntitel = \"A\"\nerreicht = -0.5\ngesamt = 2\n").unwrap_err();
        assert!(e.text.contains("`erreicht` zwischen 0 und `gesamt`"), "{e}");
        let e = lesen("fortschritt.toml", "[[zeile]]\ntitel = \"A\"\nerreicht = 1.6\ngesamt = 1.5\n").unwrap_err();
        assert!(e.text.contains("zwischen 0 und `gesamt`"), "{e}");
        let e = lesen("fortschritt.toml", "[[zeile]]\ntitel = \"A\"\nerreicht = 0.0\ngesamt = 0.0\n").unwrap_err();
        assert!(e.text.contains("größer als 0"), "{e}");
        let e = lesen("fortschritt.toml", "[[zeile]]\ntitel = \"A\"\nprozent = 40\neinheit = \"GB\"\n").unwrap_err();
        assert_eq!(e.zeile, Some(2));
        assert!(e.text.contains("`einheit` neben `prozent`"), "{e}");
    }

    /// Etappe B1: „Stand vom …“ an Kennzahl und Ampel-Zeile; das Alter und
    /// die Stufe rechnet der Server am Tag der Anfrage.
    #[test]
    fn stand_mit_alter_vom_server() {
        let heute = chrono::NaiveDate::from_ymd_opt(2026, 11, 3).unwrap();
        let alter = |s: &str, ab: Option<u32>| StandAlter::rechnen(s, ab, heute).map(|a| (a.tage, a.stufe));
        assert_eq!(alter("2026-11-03", None), Some((0, StandStufe::Frisch)));
        assert_eq!(alter("2026-10-28", None), Some((6, StandStufe::Frisch)));
        assert_eq!(alter("2026-10-27", None), Some((7, StandStufe::Alt)), "ab 7 Tagen alt");
        assert_eq!(alter("2026-10-27", Some(14)), Some((7, StandStufe::Frisch)));
        assert_eq!(alter("2026-10-20", Some(14)), Some((14, StandStufe::Alt)));
        assert_eq!(alter("2026-11-05", Some(1)), Some((-2, StandStufe::Frisch)), "Zukunft ist frisch");
        assert_eq!(alter("2026-11", None), None);

        let mut d = lesen(
            "kennzahlen.toml",
            "[[kennzahl]]\ntitel = \"Ungelesen\"\nwert = \"12\"\nstand = \"2026-10-30\"\n\n[[kennzahl]]\ntitel = \"Ohne\"\nwert = \"1\"\n",
        )
        .unwrap();
        d.einlesen_ampel("[teil.a]\n[[teil.a.zeile]]\ntitel = \"Postfach\"\nfarbe = \"gelb\"\nstand = \"2026-10-01\"\nalt_ab = 14\n");
        d.stand_rechnen(heute);
        let json = serde_json::to_value(&d).unwrap();
        assert_eq!(json["kennzahlen"]["kennzahl"][0]["stand"], "2026-10-30");
        assert_eq!(json["kennzahlen"]["kennzahl"][0]["stand_alter"], serde_json::json!({ "tage": 4, "stufe": "frisch" }));
        assert!(json["kennzahlen"]["kennzahl"][1].get("stand_alter").is_none());
        assert_eq!(json["ampel"]["teil"]["a"]["zeile"][0]["stand_alter"], serde_json::json!({ "tage": 33, "stufe": "alt" }));
        // Fehler mit Zeile.
        let e = lesen("ampel.toml", "[[zeile]]\ntitel = \"A\"\nfarbe = \"ok\"\nstand = \"30.10.2026\"\n").unwrap_err();
        assert_eq!(e.zeile, Some(4));
        assert!(e.text.starts_with("Bei „A“ steht `stand = \"30.10.2026\"`. Erlaubt ist ein Tag JJJJ-MM-TT"), "{e}");
    }

    impl ProjektDaten {
        fn einlesen_ampel(&mut self, text: &str) {
            self.datei_einlesen("ampel.toml", "projects/x/daten/ampel.toml", text).unwrap();
        }
    }

    /// Abnahme B1: die Feldnamen eines Eintrags kommen aus dem Typ selbst.
    #[test]
    fn feldnamen_aus_dem_typ() {
        assert_eq!(feldnamen::<AmpelZeile>(), ["titel", "farbe", "hinweis", "stand", "alt_ab"]);
        assert_eq!(feldnamen::<Frage>(), ["id", "text", "art", "optionen", "empfehlung", "gestellt", "prioritaet", "frist"]);
        assert!(!feldnamen::<Kennzahl>().contains(&"stand_alter".to_string()), "rechnet nur der Server");
    }

    /// Etappe B1: der Wert für `anzahl`/`anteil` als Text, wahr/falsch oder Zahl.
    #[test]
    fn treffer_lesen() {
        let b = |w: &str| toml_lesen::<Berechnet>("t", &format!("quelle = \"q\"\nart = \"anzahl\"\nfeld = \"f\"\nwert = {w}")).unwrap().wert.unwrap();
        assert_eq!(b("\"Ja\""), Treffer::Text("Ja".into()));
        assert_eq!(b("true"), Treffer::Wahr(true));
        assert_eq!(b("3"), Treffer::Zahl(3.0));
        assert_eq!(b("0.5"), Treffer::Zahl(0.5));
        assert!(Treffer::Text("ja".into()).passt(&toml::Value::String(" JA ".into())));
        assert!(!Treffer::Zahl(1.0).passt(&toml::Value::String("1".into())));
    }

    #[test]
    fn zeitstrahl_in_teilen() {
        let text = "[teil.vergangenheit]\n[[teil.vergangenheit.eintrag]]\ndatum = \"2025-03-17\"\ntitel = \"Kran geliefert\"\nfarbe = \"fertig\"\n\n[teil.plan]\n[[teil.plan.eintrag]]\ndatum = \"2026-12\"\ntitel = \"Pilot\"\n";
        let d = lesen("zeitstrahl.toml", text).unwrap();
        let Some(Datei::Teile { teil }) = &d.zeitstrahl else { panic!("{d:?}") };
        assert_eq!(teil.get("plan").unwrap().eintrag[0].titel, "Pilot");
        let json = serde_json::to_value(&d).unwrap();
        assert_eq!(json["zeitstrahl"]["teil"]["vergangenheit"]["eintrag"][0]["farbe"], "ok");
        let e = lesen("zeitstrahl.toml", "[[eintrag]]\ndatum = \"24.10.2026\"\ntitel = \"A\"\n").unwrap_err();
        assert_eq!(e.zeile, Some(2));
        assert!(e.text.contains("JJJJ-MM-TT"), "{e}");
    }

    #[test]
    fn gantt_datum_oder_monat() {
        let d = lesen("gantt.toml", "start = \"2026-11\"\n[[balken]]\nname = \"Phase 0\"\nvon = 0\nbis = 2\nfarbe = \"aktiv\"\n").unwrap();
        let Some(Datei::Ganz(g)) = d.gantt else { panic!() };
        assert_eq!(g.balken[0].von, Zeitpunkt::Monat(0.0));
        assert_eq!(g.balken[0].farbe, Some(Bedeutung::Laeuft));
        let e = lesen("gantt.toml", "[[balken]]\nname = \"Phase 1\"\nvon = \"2026-11\"\n").unwrap_err();
        assert!(e.text.contains("Dem Eintrag „Phase 1“ fehlt `bis`. Beispiel: `bis = \"2027-01\"`"), "{e}");
        let e = lesen("gantt.toml", "[[balken]]\nname = \"P\"\nvon = true\nbis = \"2027-01\"\n").unwrap_err();
        assert_eq!(e.zeile, Some(3), "{e}");
        assert!(e.text.contains("Monatszahl"), "{e}");
    }

    #[test]
    fn gantt_nicht_mischen_und_bis_nach_von() {
        let zwei = |start: &str, a: (&str, &str), b: (&str, &str)| {
            format!("{start}[[balken]]\nname = \"A\"\nvon = {}\nbis = {}\n\n[[balken]]\nname = \"B\"\nvon = {}\nbis = {}\n", a.0, a.1, b.0, b.1)
        };
        // Gemischt über die Balken: ohne und mit `start` ein Fehler, Zeile beim zweiten Balken.
        for start in ["", "start = \"2026-11\"\n"] {
            let e = lesen("gantt.toml", &zwei(start, ("0", "2"), ("\"2026-11\"", "\"2027-01\""))).unwrap_err();
            assert!(e.text.starts_with("Der Balkenplan mischt Plan-Monate und Daten: „A“ hat Monatszahlen, „B“ hat Daten."), "{e}");
            assert_eq!(e.zeile, Some(if start.is_empty() { 7 } else { 8 }), "{e}");
        }
        // Gleiche Art geht, auch mit gleichem Anfang und Ende.
        assert!(lesen("gantt.toml", &zwei("", ("0", "2"), ("2", "2"))).is_ok());
        assert!(lesen("gantt.toml", &zwei("", ("\"2026-11\"", "\"2026-11-03\""), ("\"2026-12-01\"", "\"2027-01\""))).is_ok());
        // `bis` vor `von`, als Zahl und als Datum.
        let e = lesen("gantt.toml", &zwei("", ("0", "2"), ("5", "4.5"))).unwrap_err();
        assert_eq!(e.text, "Beim Balken „B“ liegt `bis` vor `von`.");
        assert_eq!(e.zeile, Some(7));
        let e = lesen("gantt.toml", &zwei("", ("\"2027-01\"", "\"2026-11-30\""), ("\"2026-11\"", "\"2027-01\""))).unwrap_err();
        assert_eq!(e.to_string(), "projects/x/daten/gantt.toml, Zeile 2: Beim Balken „A“ liegt `bis` vor `von`.");
    }

    #[test]
    fn teile_in_reihenfolge_der_datei() {
        // „zukunft“ vor „anfang“: umgekehrt zum Alphabet.
        let text = "[teil.zukunft]\n[[teil.zukunft.eintrag]]\ndatum = \"2027-01\"\ntitel = \"Erntefest\"\n\n[teil.anfang]\n[[teil.anfang.eintrag]]\ndatum = \"2026-10\"\ntitel = \"Start\"\n";
        let d = lesen("zeitstrahl.toml", text).unwrap();
        let Some(Datei::Teile { teil }) = &d.zeitstrahl else { panic!("{d:?}") };
        assert_eq!(teil.iter().map(|(n, _)| n).collect::<Vec<_>>(), ["zukunft", "anfang"]);
        // Auch im JSON-Text (so geht es über die API) und als serde_json::Value.
        let json = serde_json::to_string(&d).unwrap();
        let (z, a) = (json.find("\"zukunft\"").unwrap(), json.find("\"anfang\"").unwrap());
        assert!(z < a, "{json}");
        let v = serde_json::to_value(&d).unwrap();
        let namen: Vec<&String> = v["zeitstrahl"]["teil"].as_object().unwrap().keys().collect();
        assert_eq!(namen, ["zukunft", "anfang"]);
        // Eigene Daten behalten ihre Reihenfolge auch.
        let d = lesen("eigen-notizen.toml", "zuletzt = 1\nanfang = 2\n").unwrap();
        let v = serde_json::to_value(&d).unwrap();
        let namen: Vec<&String> = v["eigen"]["notizen"].as_object().unwrap().keys().collect();
        assert_eq!(namen, ["zuletzt", "anfang"]);
    }

    #[test]
    fn doppelte_ids_zeigen_auf_das_zweite() {
        let k = "[[kasten]]\nid = \"a\"\ntext = \"A\"\n[[kasten]]\nid = \"a\"\ntext = \"B\"\n";
        let e = lesen("fluss.toml", k).unwrap_err();
        assert_eq!(e.zeile, Some(5), "{e}");
        assert!(e.text.contains("gibt es zweimal"), "{e}");
        let f = "[[frage]]\nid = \"x\"\ntext = \"?\"\nart = \"text\"\ngestellt = \"2026-10-25\"\n\n[[frage]]\nid = \"x\"\ntext = \"?\"\nart = \"text\"\ngestellt = \"2026-10-25\"\n";
        let e = lesen("fragen.toml", f).unwrap_err();
        assert_eq!(e.zeile, Some(8), "{e}");
    }

    #[test]
    fn kanban_und_fluss() {
        let d = lesen("kanban.toml", "[[spalte]]\ntitel = \"Offen\"\n[[spalte.karte]]\ntitel = \"eins\"\n").unwrap();
        let Some(Datei::Ganz(k)) = d.kanban else { panic!() };
        assert_eq!(k.spalte[0].karte[0].titel, "eins");
        let gut = "[[kasten]]\nid = \"aussaat\"\ntext = \"Aussaat\"\n[[kasten]]\nid = \"ernte\"\ntext = \"Erntefest\"\n[[pfeil]]\nvon = \"aussaat\"\nnach = \"ernte\"\n";
        assert!(lesen("fluss.toml", gut).is_ok());
        let e = lesen("fluss.toml", &gut.replace("nach = \"ernte\"", "nach = \"ertne\"")).unwrap_err();
        assert_eq!(e.zeile, Some(9));
        assert!(e.text.contains("„ertne“"), "{e}");
    }

    #[test]
    fn fragen_pruefen() {
        let gut = "[[frage]]\nid = \"schlusssatz\"\ntext = \"Welcher Schlusssatz?\"\nart = \"wahl\"\noptionen = [\"Kurz\", \"Lang\"]\nempfehlung = \"Lang\"\ngestellt = \"2026-10-25\"\n";
        let d = lesen("fragen.toml", gut).unwrap();
        let Some(Datei::Ganz(f)) = d.fragen else { panic!() };
        assert_eq!(f.frage[0].art, FrageArt::Wahl);
        let e = lesen("fragen.toml", &gut.replace("\"wahl\"", "\"jein\"")).unwrap_err();
        assert!(e.text.contains("Erlaubt sind: wahl, ja-nein, text, bestaetigen."), "{e}");
        let e = lesen("fragen.toml", &gut.replace("optionen = [\"Kurz\", \"Lang\"]\n", "")).unwrap_err();
        assert!(e.text.contains("mindestens zwei"), "{e}");
        let ja = "[[frage]]\nid = \"ok\"\ntext = \"Passt?\"\nart = \"ja-nein\"\ngestellt = \"2026-10-25\"\n";
        let d = lesen("fragen.toml", ja).unwrap();
        assert_eq!(serde_json::to_value(&d).unwrap()["fragen"]["frage"][0]["art"], "ja-nein");
        // Ohne prioritaet und frist bleibt das JSON wie bisher (kompatibel).
        assert!(serde_json::to_value(&d).unwrap()["fragen"]["frage"][0].get("prioritaet").is_none());
    }

    /// Eine Option, die wie „Etwas anderes“ heißt,
    /// ist ein Fehler (die Oberfläche setzt den Knopf selbst).
    #[test]
    fn option_etwas_anderes_ist_ein_fehler() {
        let gut =
            "[[frage]]\nid = \"schlusssatz\"\ntext = \"Welcher Schlusssatz?\"\nart = \"wahl\"\noptionen = [\"Kurz\", \"Lang\"]\ngestellt = \"2026-10-25\"\n";
        for name in ["Etwas anderes", "etwas anderes …", "Etwas Anderes...", "etwas-anderes", "  ETWAS   anderes: "] {
            let e = lesen("fragen.toml", &gut.replace("\"Lang\"", &format!("\"{name}\""))).unwrap_err();
            assert!(e.text.contains("setzt das Dashboard bei jeder Klick-Antwort selbst dazu"), "{name}: {e}");
            assert_eq!(e.zeile, Some(2), "{name}: {e}");
        }
        for name in ["Etwas anderes machen", "Anderes", "Lang"] {
            assert!(!heisst_wie_etwas_anderes(name), "{name}");
        }
        assert!(lesen("fragen.toml", gut).is_ok());
    }

    /// Etappe A1: `prioritaet` und `frist` sind freiwillig; falsche Werte und
    /// doppelte ids (auch über Teile hinweg) sind Fehler mit Zeile.
    #[test]
    fn fragen_mit_prioritaet_und_frist() {
        let gut = "[[frage]]\nid = \"a\"\ntext = \"?\"\nart = \"text\"\ngestellt = \"2026-10-25\"\nprioritaet = \"jetzt\"\nfrist = \"2026-11-01\"\n";
        let d = lesen("fragen.toml", gut).unwrap();
        let Some(Datei::Ganz(f)) = &d.fragen else { panic!() };
        assert_eq!((f.frage[0].prioritaet, f.frage[0].frist.as_deref()), (Some(Stufe::Jetzt), Some("2026-11-01")));
        assert_eq!(serde_json::to_value(&d).unwrap()["fragen"]["frage"][0]["prioritaet"], "jetzt");
        let e = lesen("fragen.toml", &gut.replace("\"jetzt\"", "\"sofort\"")).unwrap_err();
        assert_eq!(e.zeile, Some(6), "{e}");
        assert!(e.text.contains("Erlaubt sind: jetzt, bald, irgendwann."), "{e}");
        let e = lesen("fragen.toml", &gut.replace("2026-11-01", "1.11.2026")).unwrap_err();
        assert_eq!(e.zeile, Some(7), "{e}");
        assert!(e.text.contains("`frist = \"1.11.2026\"`"), "{e}");
        let teile = "[teil.a]\n[[teil.a.frage]]\nid = \"x\"\ntext = \"?\"\nart = \"text\"\ngestellt = \"2026-10-25\"\n\n\
                     [teil.b]\n[[teil.b.frage]]\nid = \"x\"\ntext = \"!\"\nart = \"text\"\ngestellt = \"2026-10-26\"\n";
        let e = lesen("fragen.toml", teile).unwrap_err();
        assert_eq!(e.zeile, Some(10), "{e}");
        assert!(e.text.contains("„x“ gibt es zweimal"), "{e}");
    }

    #[test]
    fn eigen_und_unbekannte_dateien() {
        let d = lesen("eigen-notizen.toml", "stand = 2026-10-24\n[[notiz]]\ntext = \"a\"\n").unwrap();
        assert_eq!(d.eigen["notizen"]["notiz"][0]["text"], "a");
        assert_eq!(d.eigen["notizen"]["stand"], "2026-10-24");
        let e = lesen("zeitleiste.toml", "").unwrap_err();
        assert!(e.text.contains("kennt der Server nicht"), "{e}");
        assert!(lesen("eigen-Gross.toml", "").is_err());
        assert!(lesen("notizen.md", "").unwrap_err().text.contains("nur TOML-Dateien"));
    }

    #[test]
    fn daten_als_json() {
        let d = ProjektDaten::default();
        let v = serde_json::to_value(&d).unwrap();
        assert!(v["kennzahlen"].is_null());
        assert!(v["eigen"].as_object().unwrap().is_empty());
        assert!(ist_datum("2026-02-28", false));
        assert!(!ist_datum("2026-02-30", false));
        assert!(!ist_datum("2026-02", false));
        assert!(ist_datum("2026-02", true));
    }

    #[test]
    fn kennzahl_mit_wert_oder_berechnet() {
        // Wie bisher: nur `wert`; im JSON ohne `berechnet`.
        let d = lesen("kennzahlen.toml", "[[kennzahl]]\ntitel = \"A\"\nwert = \"3\"\n").unwrap();
        let v = serde_json::to_value(&d).unwrap();
        assert_eq!(v["kennzahlen"]["kennzahl"][0]["wert"], "3");
        assert!(v["kennzahlen"]["kennzahl"][0].get("berechnet").is_none(), "{v}");
        let d = lesen(
            "kennzahlen.toml",
            "[[kennzahl]]\ntitel = \"A\"\nberechnet = { quelle = \"gantt\", liste = \"balken\", art = \"tage_bis\", feld = \"bis\" }\n",
        )
        .unwrap();
        let Some(Datei::Ganz(k)) = &d.kennzahlen else { panic!() };
        let b = k.kennzahl[0].berechnet.as_ref().unwrap();
        assert_eq!((b.quelle.as_str(), b.liste.as_deref(), b.art, b.feld.as_deref()), ("gantt", Some("balken"), BerechnetArt::TageBis, Some("bis")));
        let e = lesen("kennzahlen.toml", "[[kennzahl]]\ntitel = \"A\"\nberechnet = { quelle = \"gantt\", art = \"anzahl\", spalte = \"x\" }\n").unwrap_err();
        assert!(e.text.contains("Das Feld `spalte` gibt es hier nicht. Erlaubt sind: quelle, liste, art, feld, wert."), "{e}");
    }

    /// Orte (Entscheidung lokal-oeffnen): kennung, titel, pfad;
    /// der Pfad geht nie ins JSON, doppelte Kennung und fehlende Felder sind Fehler.
    #[test]
    fn orte_ohne_pfad_im_json() {
        let text = "[[ort]]\nkennung = \"kiste\"\ntitel = \"Rechnungen\"\npfad = 'C:\\Users\\beispiel\\Documents\\rechnungen.pdf'\nhinweis = \"PDF\"\n";
        let d = lesen("orte.toml", text).unwrap();
        let Some(o) = &d.orte else { panic!() };
        assert_eq!(o.ort("kiste").unwrap().pfad, r"C:\Users\beispiel\Documents\rechnungen.pdf");
        let json = serde_json::to_string(&d).unwrap();
        assert!(!json.contains("pfad") && !json.contains("rechnungen.pdf") && !json.contains("beispiel"), "{json}");
        assert!(json.contains("\"kennung\":\"kiste\""), "{json}");
        let e = lesen("orte.toml", "[[ort]]\nkennung = \"a\"\ntitel = \"A\"\npfad = 'C:\\x'\n[[ort]]\nkennung = \"a\"\ntitel = \"B\"\npfad = 'C:\\y'\n")
            .unwrap_err();
        assert_eq!(e.zeile, Some(6), "{e}");
        assert!(lesen("orte.toml", "[[ort]]\nkennung = \"A b\"\ntitel = \"A\"\npfad = 'C:\\x'\n").is_err());
        assert!(lesen("orte.toml", "[[ort]]\nkennung = \"a\"\ntitel = \"A\"\n").is_err(), "pfad fehlt");
        assert!(lesen("orte.toml", "[[ort]]\nkennung = \"a\"\ntitel = \"A\"\npfad = \"\"\n").is_err(), "pfad leer");
        assert!(lesen("orte.toml", "[[ort]]\nkennung = \"a\"\ntitel = \"A\"\npfad = 'C:\\x'\nargumente = \"/e\"\n").is_err(), "keine Schalter");
        let mut d = lesen("orte.toml", text).unwrap();
        assert!(matches!(d.nimm("orte"), Some(DatenInhalt::Orte(_))));
        assert!(ist_datenname("orte"));
    }

    #[test]
    fn eine_datei_herausnehmen() {
        let mut d = lesen("eigen-notizen.toml", "a = 1\n").unwrap();
        d.datei_einlesen("gantt.toml", "g", "[[balken]]\nname = \"A\"\nvon = 0\nbis = 1\n").unwrap();
        assert!(matches!(d.nimm("gantt"), Some(DatenInhalt::Gantt(Datei::Ganz(_)))));
        assert!(d.nimm("gantt").is_none(), "schon herausgenommen");
        let Some(DatenInhalt::Eigen(v)) = d.nimm("eigen-notizen") else { panic!() };
        assert_eq!(v["a"], 1);
        assert!(d.nimm("ampel").is_none());
        assert!(ist_datenname("eigen-notizen") && ist_datenname("fragen"));
        assert!(!ist_datenname("eigen-") && !ist_datenname("notizen") && !ist_datenname("../gantt"));
    }
}
