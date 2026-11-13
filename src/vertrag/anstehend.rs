//! „Anstehendes“ (Etappe A1, Fassung 3): Aufgaben,
//! Fragen und Arbeiten der Chats als Einträge einer gemeinsamen Liste, mit
//! Priorität (Stufe), Schwerpunkt und Ziel. Hier stehen nur die Datenformen;
//! gerechnet wird in `crate::anstehend`, gespeichert in `speicher::arbeiten`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::bedeutung::Bedeutung;
use super::daten::FrageArt;

// ----- Stufen -----

/// Priorität eines Eintrags: Jetzt, Bald, Irgendwann. Die Reihenfolge von
/// `Ord` ist die der Liste (Jetzt zuerst).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Stufe {
    /// Heute, oder es hält etwas auf.
    Jetzt,
    /// In den nächsten Tagen (Standard, wenn der Chat nichts angibt).
    #[default]
    Bald,
    /// Kein Druck.
    Irgendwann,
}

impl Stufe {
    pub const ALLE: [Stufe; 3] = [Self::Jetzt, Self::Bald, Self::Irgendwann];

    pub fn wort(self) -> &'static str {
        match self {
            Self::Jetzt => "jetzt",
            Self::Bald => "bald",
            Self::Irgendwann => "irgendwann",
        }
    }

    pub fn aus_wort(wort: &str) -> Option<Self> {
        Self::ALLE.into_iter().find(|s| s.wort() == wort)
    }
}

/// Warum die wirksame Stufe so ist: Vorschlag des Chats, die Einstufung des Inhabers
/// oder die Frist (heute, vorbei oder in höchstens sieben Tagen); bei
/// Meldungen die feste Stufe nach Art (Etappe A3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum StufenGrund {
    Vorschlag,
    Inhaber,
    Frist,
    Fest,
}

/// Wie nah eine Frist ist (Etappe 3a-4), am lokalen Datum `heute` gerechnet:
/// vorbei, heute, in höchstens `frist_bald_tage` Tagen oder später. Aus
/// derselben Stufe kommt die Hebung der wirksamen Stufe (`crate::anstehend::
/// wirksame_stufe`): überfällig und heute heben auf jetzt, bald hebt
/// irgendwann auf bald. Sie hängt vom Tag ab; die Oberfläche lädt darum um
/// Mitternacht neu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum FristStufe {
    Ueberfaellig,
    Heute,
    /// In 1 bis `frist_bald_tage` Tagen.
    Bald,
    /// Später als `frist_bald_tage` Tage.
    Spaeter,
}

/// Die Stufe eines Eintrags mit allem, woraus sie gerechnet ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StufenAngabe {
    /// Was der Chat vorgeschlagen hat (ohne Angabe: bald).
    pub vorschlag: Stufe,
    /// Die Einstufung des Inhabers; `null`: es gilt der Vorschlag.
    pub inhaber: Option<Stufe>,
    /// Die Stufe, nach der sortiert und gezählt wird.
    pub wirksam: Stufe,
    pub grund: StufenGrund,
}

/// Welche Art von Eintrag. Meldungen (Etappe A3) stehen in der Liste, solange
/// sie ungelesen und nicht behoben sind; ihre Stufe ist fest (nach Art), sie
/// lassen sich nicht umstufen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum EintragsArt {
    Aufgabe,
    Frage,
    Arbeit,
    Meldung,
}

impl EintragsArt {
    pub const ALLE: [EintragsArt; 4] = [Self::Aufgabe, Self::Frage, Self::Arbeit, Self::Meldung];

    pub fn wort(self) -> &'static str {
        match self {
            Self::Aufgabe => "aufgabe",
            Self::Frage => "frage",
            Self::Arbeit => "arbeit",
            Self::Meldung => "meldung",
        }
    }

    pub fn aus_wort(wort: &str) -> Option<Self> {
        Self::ALLE.into_iter().find(|a| a.wort() == wort)
    }
}

// ----- Meldungen (Etappe A3) -----

/// Welche Art von Meldung. Die Stufe ist fest ([prioritaet] regel):
/// `fehler` und `frist` Jetzt, `fertig` Bald, `info` Irgendwann.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum MeldungsArt {
    /// Etwas ist fertig (eine Arbeit auf „fertig“ oder vom Chat gemeldet).
    Fertig,
    /// Eine Frist ist nah (höchstens zwei Tage) oder vorbei.
    Frist,
    /// Etwas ist kaputt (Bau rot, Prüfung rot, Bote fehlt); bleibt, bis es behoben ist.
    Fehler,
    /// Zur Kenntnis.
    Info,
}

impl MeldungsArt {
    pub const ALLE: [MeldungsArt; 4] = [Self::Fertig, Self::Frist, Self::Fehler, Self::Info];

    pub fn wort(self) -> &'static str {
        match self {
            Self::Fertig => "fertig",
            Self::Frist => "frist",
            Self::Fehler => "fehler",
            Self::Info => "info",
        }
    }

    pub fn aus_wort(wort: &str) -> Option<Self> {
        Self::ALLE.into_iter().find(|a| a.wort() == wort)
    }

    /// Die feste Stufe nach Art.
    pub fn stufe(self) -> Stufe {
        match self {
            Self::Fehler | Self::Frist => Stufe::Jetzt,
            Self::Fertig => Stufe::Bald,
            Self::Info => Stufe::Irgendwann,
        }
    }
}

/// Eine Meldung (Tabelle `meldungen`, Etappe A3): Etwas ist passiert, das
/// der Inhaber wissen sollte. Meist erzeugt sie der Server selbst (Bau rot,
/// Prüfung rot, Frist nah oder vorbei, Bote fehlt, Arbeit fertig), sonst
/// meldet der Chat über `POST /api/meldungen` „fertig“ oder „info“.
///
/// `schluessel` fasst Wiederholungen zusammen und enthält immer das Projekt
/// (etwa `architektur/bau-rot` oder `dashboard/frist:aufgabe-a-20261030-0001`):
/// Eine Wiederholung zählt `anzahl` hoch, statt eine zweite Meldung anzulegen.
/// Der Inhaber klickt „gelesen“; gelesene und behobene Meldungen ziehen nach sieben
/// Tagen unverändert nach `meldungen_archiv` (nichts wird gelöscht). Eine
/// Fehler-Meldung bleibt, solange der Fehler besteht (`behoben` leer).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Meldung {
    /// Kennung `m-JJJJMMTT-NNNN`, vom Server vergeben.
    pub id: String,
    /// Slug des zuständigen Projekts (Systemmeldungen: `dashboard`).
    pub projekt: String,
    pub art: MeldungsArt,
    /// Fasst Wiederholungen zusammen, immer mit dem Projekt vorn: `<projekt>/<was>`.
    pub schluessel: String,
    /// Ein Satz, höchstens 200 Zeichen.
    pub titel: String,
    /// Einzelheiten, etwa der Satz aus dem Prüfbericht (höchstens 1000 Zeichen).
    pub text: Option<String>,
    /// Worauf sie sich bezieht (Kennung einer Arbeit, Aufgabe oder Frage).
    pub verweis: Option<String>,
    /// Systemmeldung (Bote, Dienst, Datenspeicher): gehört zur PC-Seite, im Band steht „PC“.
    pub system: bool,
    /// Der Titel nennt eine Aufgabe; ohne Windows-Anmeldung steht er nicht da.
    pub geschuetzt: bool,
    /// `server` oder der Chat (`chat:<slug>`, eine Angabe, kein Nachweis).
    pub von: String,
    /// Ortszeit, RFC 3339: das erste Mal.
    pub erstellt: String,
    /// Ortszeit, RFC 3339: das letzte Mal (bei Wiederholung neu).
    pub zuletzt: String,
    /// Wie oft es passiert ist (ab 1).
    pub anzahl: u64,
    /// Wann der Inhaber „gelesen“ geklickt hat; `null`: ungelesen. Eine Wiederholung macht sie wieder ungelesen.
    pub gelesen: Option<String>,
    /// Nur bei Fehlern und Fristen: wann der Grund weg war (Bau wieder grün, Frist erledigt).
    pub behoben: Option<String>,
    /// Wann sie ins Archiv gezogen ist (nur in `meldungen_archiv`).
    pub archiviert: Option<String>,
    /// Vom Server als gelesen abgelegt, weil mehr als 20 ungelesen waren.
    #[serde(default)]
    pub zusammengefasst: bool,
}

/// Was eine Meldung in „Anstehend“ zusätzlich mitbringt.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct MeldungAngabe {
    pub art: MeldungsArt,
    pub anzahl: u64,
    /// Ortszeit, RFC 3339: das letzte Mal.
    pub zuletzt: String,
    pub text: Option<String>,
    /// Systemmeldung: im Band steht „PC“, das Ziel ist die PC-Seite.
    pub system: bool,
    /// Kopfleiste, Fassung 3: der Schlüssel der Meldung (`<projekt>/<was>`,
    /// etwa `dashboard/lesen:docs`), als Fakt. Die Oberfläche erkennt daran
    /// Rechte- und Zugriffsmeldungen und zeigt sie als Hinweis statt als
    /// Fehler. Steht immer im JSON; in TypeScript optional (ältere Beispiele).
    #[ts(as = "Option<String>", optional)]
    pub schluessel: String,
}

/// `POST /api/meldungen`: die Kennung der Meldung und ob sie neu ist
/// (`false`: Wiederholung, nur hochgezählt oder schon da).
#[derive(Debug, Serialize, TS)]
pub struct MeldungAngelegt {
    pub id: String,
    pub neu: bool,
    pub anzahl: u64,
}

/// `GET /api/meldungen[?projekt=<slug>]`: alle außerhalb des Archivs, neueste zuerst.
#[derive(Debug, Serialize, TS)]
pub struct MeldungenListe {
    pub meldungen: Vec<Meldung>,
}

/// `POST /api/meldungen/<id>/gelesen`: die Meldung danach und ob sich etwas geändert hat.
#[derive(Debug, Serialize, TS)]
pub struct MeldungGelesen {
    pub meldung: Meldung,
    pub geaendert: bool,
}

// ----- Arbeiten der Chats -----

/// Wie eine Arbeit steht.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum ArbeitsStand {
    Laeuft,
    /// Wartet auf jemanden (`wartet_auf`).
    Wartet,
    AlsNaechstes,
    Fertig,
}

impl ArbeitsStand {
    pub const ALLE: [ArbeitsStand; 4] = [Self::Laeuft, Self::Wartet, Self::AlsNaechstes, Self::Fertig];

    pub fn wort(self) -> &'static str {
        match self {
            Self::Laeuft => "laeuft",
            Self::Wartet => "wartet",
            Self::AlsNaechstes => "als-naechstes",
            Self::Fertig => "fertig",
        }
    }

    pub fn aus_wort(wort: &str) -> Option<Self> {
        Self::ALLE.into_iter().find(|a| a.wort() == wort)
    }
}

/// Was ein Chat tut oder vorhat (Tabelle `arbeiten`). Der Chat legt sie über
/// `POST /api/arbeiten` an und schreibt sie über `POST /api/arbeiten/<id>`
/// fort; der Inhaber liest nur. Fertige ziehen sieben Tage nach ihrer letzten
/// Änderung nach `arbeiten_archiv` um.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Arbeit {
    /// Kennung `ar-JJJJMMTT-NNNN`, vom Server vergeben.
    pub id: String,
    /// Slug des Projekts.
    pub projekt: String,
    /// Welcher Chat sie angelegt hat (eine Angabe, kein Nachweis).
    pub von: String,
    /// Wer dran ist (`chat:<name>`, Schema 10): ohne Schritte bei `laeuft` und
    /// `wartet` Pflicht, nur er ändert sie; mit Schritten nur verantwortlich
    /// für Titel und Notiz. Fehlt bei älteren und freien Arbeiten.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub besitzer: Option<String>,
    /// Ein Satz, höchstens 200 Zeichen.
    pub titel: String,
    /// Hat die Arbeit Schritte, rechnet der Server ihn daraus aus.
    pub stand: ArbeitsStand,
    /// Nur bei `wartet`: `inhaber`, `chat:<name>` oder `extern`.
    pub wartet_auf: Option<String>,
    /// Kennung der Frage oder Aufgabe, auf die die Arbeit wartet.
    pub verweis: Option<String>,
    /// Vorschlag des Chats.
    pub prioritaet: Stufe,
    /// Bis wann (JJJJ-MM-TT).
    pub frist: Option<String>,
    /// Kurze Notiz, höchstens 1000 Zeichen.
    pub notiz: Option<String>,
    /// Ortszeit, RFC 3339.
    pub erstellt: String,
    pub geaendert: String,
    /// Verlauf der Arbeit, ältester Eintrag zuerst (Titel, Besitzer,
    /// Schritte). Fehlt, solange er leer ist (auch bei älteren Arbeiten).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<ArbeitVerlauf>>", optional)]
    pub verlauf: Vec<ArbeitVerlauf>,
    /// Teilschritte, nach `nr` (Schema 10). Fehlt, solange es keine gibt.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<Schritt>>", optional)]
    pub schritte: Vec<Schritt>,
    /// Etappe aus `daten/plan.toml` (Schema 11): `<projekt>/<kennung>`, auch
    /// eines anderen Projekts. Der Server prüft beim Schreiben, dass es sie
    /// gibt; setzen und ändern darf nur, wer die Arbeit verantwortet. Fehlt,
    /// solange keine gesetzt ist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub etappe: Option<String>,
}

/// Ein Teilschritt einer Arbeit (Zweig teilschritte). Jeder Schritt, an dem
/// gearbeitet wird, hat genau einen Chat als Besitzer; nur er ändert ihn.
/// Schritte werden nie gelöscht.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Schritt {
    /// Ab 1, vom Server vergeben, nie neu vergeben.
    pub nr: u32,
    /// Höchstens 200 Zeichen.
    pub titel: String,
    pub stand: ArbeitsStand,
    /// `chat:<name>`: Pflicht bei `laeuft` und `wartet`, bleibt bei `fertig`
    /// als Angabe stehen; nie der Inhaber.
    pub besitzer: Option<String>,
    /// Nur bei `wartet`: `schritt:<nr>` (derselben Arbeit), `inhaber` oder `extern`.
    pub wartet_auf: Option<String>,
    /// Bei `wartet_auf = inhaber` Pflicht: die Frage oder Aufgabe.
    pub verweis: Option<String>,
    /// `chat:<name>`, dem der Schritt übergeben werden soll; übergeben ist er
    /// erst, wenn dieser Chat übernimmt.
    pub angeboten_an: Option<String>,
    /// Nur dieser Chat darf den Schritt bekommen (etwa für eine Abnahme).
    pub fuer: Option<String>,
    /// Ortszeit, RFC 3339: wann zuletzt am Schritt geschrieben wurde (auch
    /// eine Notiz zählt); daran misst sich „verwaist“ nach 24 Stunden.
    pub seit: String,
    /// Höchstens 1000 Zeichen.
    pub notiz: Option<String>,
    /// Kennung des offenen Weck-Vorgangs.
    pub vorgang: Option<String>,
}

/// Was ein Eintrag im Verlauf einer Arbeit festhält.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum ArbeitVerlaufArt {
    /// Der Chat hat den Titel geändert (`alt` → `neu`).
    Titel,
    /// Die Arbeit hat einen Besitzer bekommen (`alt` → `neu`).
    Besitzer,
    /// Ein Schritt wurde angelegt (`neu`: Titel).
    SchrittAngelegt,
    /// Ein Schritt hat den Besitzer gewechselt (auch verwaist, mit `statt`).
    SchrittUebernommen,
    /// Ein Schritt wurde geändert (Stand, Warten, Notiz, Angebot, frei).
    SchrittGeaendert,
    /// Der Server hat einen wartenden Schritt zurück auf `laeuft` gesetzt.
    SchrittGeweckt,
    /// Der Stand der Arbeit hat gewechselt (`alt` → `neu` als Stand-Wort),
    /// von Hand gesetzt oder aus den Schritten abgeleitet (Schema 11).
    Stand,
    /// Die Etappe der Arbeit wurde gesetzt oder geändert (`alt` → `neu`, leer: keine).
    Etappe,
}

/// Ein Eintrag im Verlauf einer Arbeit. Er erzeugt keine Meldung.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ArbeitVerlauf {
    /// Ortszeit, RFC 3339.
    pub zeit: String,
    /// Wer geändert hat (`chat:<name>`, beim Zurückwecken `server`).
    pub von: String,
    pub art: ArbeitVerlaufArt,
    /// Bei Einträgen zu einem Schritt dessen Nummer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub schritt: Option<u32>,
    /// Der Wert vorher.
    pub alt: String,
    /// Der Wert danach.
    pub neu: String,
    /// Ein Satz für Menschen, etwa `Titel geändert: „alt“ → „neu“`.
    pub text: String,
}

/// `POST /api/arbeiten`: die Kennung der neuen Arbeit.
#[derive(Debug, Serialize, TS)]
pub struct ArbeitAngelegt {
    pub id: String,
}

/// `POST /api/arbeiten/<id>`: die Arbeit danach und ob sich etwas geändert hat.
#[derive(Debug, Serialize, TS)]
pub struct ArbeitGeaendert {
    pub arbeit: Arbeit,
    pub geaendert: bool,
}

/// `POST /api/arbeiten/<id>/schritte`: die Nummer des neuen Schritts (201).
/// Übernehmen und Ändern eines Schritts antworten mit `ArbeitGeaendert`.
#[derive(Debug, Serialize, TS)]
pub struct SchrittAngelegt {
    pub nr: u32,
}

/// 409 auf den schreibenden Arbeiten- und Schritt-Wegen: ein anderer Chat ist
/// dran. Wie jede Fehlerantwort mit `fehler` und `art` (`konflikt`), dazu wer
/// dran ist und seit wann (Ortszeit, RFC 3339).
#[derive(Debug, Serialize, TS)]
pub struct Besetzt {
    pub fehler: String,
    pub art: String,
    pub besitzer: String,
    pub seit: String,
}

/// `GET /api/arbeiten[?projekt=<slug>]`: alle nicht archivierten, nicht
/// fertige zuerst, dann nach Stufe des Chats, Frist und Alter.
#[derive(Debug, Serialize, TS)]
pub struct ArbeitenListe {
    pub arbeiten: Vec<Arbeit>,
}

// ----- Einstufung durch den Inhaber -----

/// Die Einstufung eines Eintrags durch den Inhaber (Tabelle `einstufungen`). Schlüssel:
/// Projekt, Art, Kennung, bei Fragen dazu `gestellt` (damit eine später neu
/// vergebene Frage-Kennung keine alte Einstufung erbt). `stufe = null` heißt:
/// zurück zum Vorschlag des Chats; der Eintrag bleibt dabei stehen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Einstufung {
    pub projekt: String,
    pub art: EintragsArt,
    pub id: String,
    /// Nur bei Fragen: JJJJ-MM-TT aus `fragen.toml`.
    pub gestellt: Option<String>,
    pub stufe: Option<Stufe>,
    /// Wann der Inhaber zuletzt eingestuft hat (Ortszeit, RFC 3339).
    pub zeit: String,
}

/// `POST /api/einstufung`: die gespeicherte Einstufung und ob sie sich geändert hat.
#[derive(Debug, Serialize, TS)]
pub struct EinstufungAntwort {
    pub einstufung: Einstufung,
    pub geaendert: bool,
}

// ----- Antworten auf Fragen (Etappe A2) -----

/// Die Antwort des Inhabers auf eine Frage aus `daten/fragen.toml` (Tabelle
/// `antworten`). Schlüssel: Projekt, Kennung der Frage und `gestellt`, wie
/// bei den Einstufungen: Eine später neu gestellte Frage mit derselben
/// Kennung erbt so keine alte Antwort. Antworten entstehen nur im Browser
/// (`von = "browser"`), zusammen mit einem Vorgang `antwort` für den Chat.
/// Nach sieben Tagen ziehen sie unverändert nach `antworten_archiv`; gelöscht
/// wird nichts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Antwort {
    /// Slug des Projekts.
    pub projekt: String,
    /// Kennung der Frage aus fragen.toml.
    pub id: String,
    /// JJJJ-MM-TT aus fragen.toml, gehört zum Schlüssel.
    pub gestellt: String,
    pub art: FrageArt,
    /// Text der Frage, wie er beim Antworten dastand (der Chat nimmt die
    /// Frage danach aus fragen.toml heraus).
    pub frage: String,
    /// Bei `wahl` die gewählte Option, bei `ja-nein` „ja“ oder „nein“, bei
    /// `text` der Text, bei `bestaetigen` „bestaetigt“. Hat der Inhaber statt
    /// eines Klicks „Etwas anderes …“ gewählt (nicht bei `text`): der feste
    /// Wert `etwas-anderes`, seine Worte stehen dann in `eigene`.
    pub antwort: String,
    /// Bei `antwort = "etwas-anderes"` die eigenen Worte des Inhabers (nicht
    /// leer, höchstens 20 000 Zeichen, Zeilenumbrüche erlaubt); bei einem
    /// Klick `null`. Antworten von vor diesem Feld lesen sich als `null`.
    #[serde(default)]
    pub eigene: Option<String>,
    /// Die freiwillige Anmerkung des Inhabers zur Antwort (höchstens 1000 Zeichen,
    /// Zeilenumbrüche erlaubt); `null`, wenn sie keine geschrieben hat. Gibt
    /// es bei `wahl`, `ja-nein` und `bestaetigen`, nie bei `text`. Antworten
    /// von vor der Anmerkung haben das Feld nicht gespeichert und lesen sich
    /// als `null`.
    #[serde(default)]
    pub notiz: Option<String>,
    /// Wann der Inhaber geantwortet hat (Ortszeit, RFC 3339).
    pub zeit: String,
    /// Herkunft, immer `browser`.
    pub von: String,
    /// Kennung des Vorgangs `antwort`, der den Chat weckt.
    pub vorgang: String,
}

/// Art, Optionen und Empfehlung einer Frage, damit die Seite sie direkt
/// beantworten kann (nur bei Einträgen der Art `frage`).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct FrageAngabe {
    pub art: FrageArt,
    /// Nur bei `wahl`: die Optionen in der Reihenfolge aus fragen.toml.
    pub optionen: Vec<String>,
    /// Was der Chat empfiehlt (bei `wahl` eine der Optionen).
    pub empfehlung: Option<String>,
}

/// `POST /api/antworten/<slug>/<id>`: die gespeicherte Antwort und ob sie
/// neu ist (`false`: dieselbe Antwort stand schon da, nichts geschrieben).
#[derive(Debug, Serialize, TS)]
pub struct AntwortGespeichert {
    pub antwort: Antwort,
    pub geaendert: bool,
}

/// `GET /api/antworten?projekt=<slug>`: Antworten außerhalb des Archivs, neueste zuerst.
#[derive(Debug, Serialize, TS)]
pub struct AntwortenListe {
    pub antworten: Vec<Antwort>,
}

// ----- Die gemeinsame Liste -----

/// Ein Eintrag in „Anstehend“ (`GET /api/anstehend`). Klick-Links von
/// Aufgaben stehen hier nie, nur `hat_link`; den Link liefert allein
/// `/api/aufgaben`. `ziel` ist immer eine Adresse im Dashboard.
///
/// Etappe 3a+: Der Server liefert alle offenen Einträge (auch alle offenen
/// Fragen eines Projekts) mit ihren Fakten und Regel-Ergebnissen; die
/// Reihenfolge, Gruppen und Zahlen rechnet die Oberfläche
/// (`web/src/lib/anstehend.ts`). Welche Frage je Projekt sichtbar ist, sagt
/// seit Etappe B2 der Server (`warteschlange`).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct AnstehendEintrag {
    pub art: EintragsArt,
    /// Kennung: Aufgabe `a-…`, Arbeit `ar-…`, Frage ihre `id` aus fragen.toml.
    pub id: String,
    /// Slug des Projekts.
    pub projekt: String,
    /// Titel des Projekts; `null`, wenn es das Projekt nicht (mehr) gibt.
    pub projekt_titel: Option<String>,
    /// Die Projektgruppe des Projekts aus `data/gruppen.toml` (Entscheidung
    /// projektgruppe): Die Oberfläche zeigt ihren Titel vor dem
    /// Projekt („Gruppe › Projekt“). Ohne Gruppe fehlt das Feld.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub gruppe: Option<super::gruppen::GruppenBezug>,
    /// Name des Schwerpunkts aus `data/schwerpunkte.toml`, sonst „Sonstiges“.
    pub schwerpunkt: String,
    /// `reihenfolge` dieses Schwerpunkts (kleiner steht weiter vorn);
    /// „Sonstiges“ hat eins mehr als der letzte Schwerpunkt.
    pub schwerpunkt_reihenfolge: i64,
    /// Regel-Ergebnis: Fragen immer; Aufgaben außer „beim Chat“ (Zustand
    /// `beim-chat`, eigene Worte geschickt); Arbeiten mit `wartet_auf =
    /// inhaber`, außer sie verweisen auf eine offene Frage oder Aufgabe (dann
    /// zählt jene, nicht doppelt); Meldungen nie.
    pub wartet_auf_dich: bool,
    /// Titel der Aufgabe oder Arbeit, Text der Frage.
    pub titel: String,
    pub stufe: StufenAngabe,
    /// JJJJ-MM-TT.
    pub frist: Option<String>,
    /// Tage von heute bis zur Frist (0 heute, negativ: vorbei).
    pub tage_bis: Option<i64>,
    /// Wie nah die Frist ist; `null` ohne Frist.
    pub frist_stufe: Option<FristStufe>,
    /// Aufgabe und Frage: `offen` (eine Aufgabe mit eigenen Worten: `beim-chat`);
    /// Arbeit: `laeuft`, `wartet` oder `als-naechstes`.
    pub zustand: String,
    /// Auf wen der Eintrag wartet: bei Aufgaben und Fragen `inhaber`, bei
    /// Arbeiten ihr Feld `wartet_auf`.
    pub wartet_auf: Option<String>,
    /// Nur Arbeiten: Kennung der Frage oder Aufgabe, auf die sie warten.
    pub verweis: Option<String>,
    /// Nur Arbeiten mit Verweis auf eine offene Frage oder Aufgabe (bei
    /// Schritten: irgendein auf dich wartender Schritt): „wartet auf deine
    /// Antwort“. Den Vermerk zu weiteren Fragen setzt die Oberfläche.
    pub vermerk: Option<String>,
    /// Detailansicht im Dashboard: `/projects/<slug>#<art>-<id>`.
    pub ziel: String,
    /// `false`, wenn es das Projekt nicht (mehr) gibt: dann „nicht mehr da“ statt Link.
    pub ziel_da: bool,
    /// Woher der Eintrag kommt: `datenspeicher:aufgaben`, `datenspeicher:arbeiten`
    /// oder die Datei `projects/<slug>/daten/fragen.toml`.
    pub quelle: String,
    /// Nur Aufgaben: ob sie einen Klick-Link hat (der Link selbst nur über `/api/aufgaben`).
    pub hat_link: bool,
    /// Nur Aufgaben: die Antwort-Knöpfe.
    pub knopf: Option<Vec<String>>,
    /// Nur Fragen: JJJJ-MM-TT, gehört zum Schlüssel der Einstufung und der Antwort.
    pub gestellt: Option<String>,
    /// Nur Fragen: Art, Optionen und Empfehlung zum Beantworten (Etappe A2).
    pub frage: Option<FrageAngabe>,
    /// Nur Meldungen: Art, Anzahl, letztes Mal, Einzelheiten (Etappe A3).
    pub meldung: Option<MeldungAngabe>,
    /// Welcher Chat den Eintrag angelegt hat (bei Fragen unbekannt).
    pub von: Option<String>,
    /// Ortszeit RFC 3339, bei Fragen das Datum `gestellt`.
    pub erstellt: String,
    /// Nur Aufgaben „beim Chat“ (Zustand `beim-chat`): die eigenen Worte, die der
    /// Inhaber mit „Etwas anderes …“ geschickt hat. Sonst fehlt das Feld.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub eigene: Option<String>,
    /// Nur Fragen (Etappe B2): ihr Platz in der Warteschlange des Projekts
    /// (`crate::anstehend::fragen_warteschlange`). Platz 1 ist die eine
    /// sichtbare Frage; die übrigen warten. Bei anderen Arten fehlt das Feld.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub warteschlange: Option<FragenPlatz>,
    /// Nur Aufgaben mit beweglicher Frist (Etappe B3): Ereignis und Abstand.
    /// `frist` ist das aufgelöste Datum oder `null`, solange das Ereignis kein
    /// Datum hat. Sonst fehlt das Feld.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub frist_bezug: Option<super::fristen::FristBezug>,
    /// Nur mit `frist_bezug`: das Ereignis mit Titel und Datum.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ereignis: Option<super::fristen::EreignisStand>,
    /// Nur Arbeiten (Zweig teilschritte, T4): ihre Schritte nach `nr`, für
    /// die Liste im Kasten Arbeiten. Wie alle Einträge nur mit Anmeldung.
    /// Fehlt, solange es keine gibt.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<Schritt>>", optional)]
    pub schritte: Vec<Schritt>,
    /// Nur Arbeiten mit Schritten (T5, Auflage aus T4): die Verweise der auf
    /// den Inhaber wartenden Schritte, die eine offene Frage oder Aufgabe
    /// sind, je einmal. Je Schritt mit einem davon führt ein Link hin. Wie
    /// die Schritte nur mit Anmeldung; fehlt, solange keiner offen ist.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<String>>", optional)]
    pub offene_verweise: Vec<String>,
}

/// Der Platz einer offenen Frage in der Warteschlange ihres Projekts (Etappe
/// B2, Entscheidung gemeinsame-bausteine: „Je Projekt ist eine
/// Frage sichtbar; weitere warten in der Warteschlange“). Reihenfolge:
/// wirksame Stufe (Vorschlag des Chats, Einstufung des Inhabers, Frist),
/// dann die frühere Frist (ohne Frist danach), dann die Reihenfolge in `fragen.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub struct FragenPlatz {
    /// 1 für die sichtbare Frage, dann 2, 3 …
    pub platz: u64,
    /// Wie viele offene Fragen das Projekt hat (mit dieser).
    pub offen: u64,
}

/// Die Warteschlange der Fragen eines Projekts in Kürze (Etappe B2): die
/// sichtbare Frage (Kennung und `gestellt`) und wie viele dahinter warten.
/// Steht im Abgeleiteten von `daten/fragen.toml`, damit der Baustein
/// `Fragen` dieselbe eine Frage zeigt wie Anstehend und das Unter-Dashboard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct FragenAuswahl {
    /// Kennung der sichtbaren Frage; `null`: keine offene Frage.
    pub sichtbar: Option<String>,
    /// Ihr `gestellt` (JJJJ-MM-TT).
    pub gestellt: Option<String>,
    /// Wie viele offene Fragen dahinter warten (ohne deren Text).
    pub weitere: u64,
}

/// `GET /api/anstehend`: alle offenen Einträge in einer festen, fachlich
/// neutralen Ordnung (Projekt, Art, Kennung), ohne Auswahl und ohne Zahlen
/// (Etappe 3a+). Ordnen, Auswählen, Gruppieren und Zählen macht die Oberfläche.
#[derive(Debug, Clone, Serialize, TS)]
pub struct AnstehendListe {
    /// Das lokale Datum, mit dem gerechnet wurde (JJJJ-MM-TT).
    pub heute: String,
    /// Bis zu so vielen Tagen ist eine Frist `bald` (Fakt zu `frist_stufe`).
    pub frist_bald_tage: i64,
    pub eintraege: Vec<AnstehendEintrag>,
    /// Sätze zu Dateien, die sich nicht lesen ließen (fragen.toml, schwerpunkte.toml).
    pub hinweise: Vec<String>,
    /// Nur mit dem Filter `projekt`: alle Meldungen des Projekts (auch
    /// gelesene) und seine Vorgänge der letzten sieben Tage, neutral nach
    /// Quelle und Kennung; sonst leer.
    pub aktualisierungen: Vec<Aktualisierung>,
    /// Entscheidung archiv-status, Sicherheitsregel: Fristen in
    /// archivierten Projekten, die in höchstens `frist_bald_tage` Tagen
    /// ablaufen (oder schon abgelaufen sind), als neutraler Satz ohne
    /// Projekttitel. Nur ohne Filter `projekt`; Einträge archivierter
    /// Projekte stehen sonst nie in `eintraege` (außer mit `projekt` auf
    /// ihrer eigenen Seite). Zählen nirgends mit. Steht immer im JSON; in
    /// TypeScript optional, damit ältere Beispieldaten der Oberfläche passen.
    #[ts(as = "Option<Vec<ArchivFrist>>", optional)]
    pub archiv_fristen: Vec<ArchivFrist>,
}

/// Eine Frist in einem archivierten Projekt (Sicherheitsregel, siehe
/// `AnstehendListe::archiv_fristen`): nur Satz, Datum und Link zur Projektseite.
/// Etappe B3: Eine bewegliche Frist ohne aufgelöstes Datum steht ohne Datum
/// da („Eine Frist in einem archivierten Projekt hängt an einem Ereignis.“),
/// bis ihr Ereignis ein Datum hat; dann gilt die Regel der sieben Tage.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct ArchivFrist {
    /// „Eine Frist in einem archivierten Projekt läuft am 08.11.2026 ab.“
    pub satz: String,
    /// JJJJ-MM-TT; `null` bei einer Frist, die an einem Ereignis ohne Datum hängt.
    pub frist: Option<String>,
    /// Tage von heute bis zur Frist (0 heute, negativ: vorbei); `null` ohne Datum.
    pub tage_bis: Option<i64>,
    /// Die Projektseite: `/projects/<slug>`.
    pub ziel: String,
}

// ----- Unter-Dashboard einer Projektseite -----

/// Ampel des Gesamtstatus: rot bei einer offenen Fehler-Meldung oder einer
/// Frist, die vorbei ist, gelb, wenn etwas auf den Inhaber wartet, sonst grün.
/// Fehlt der Datenspeicher und ist nichts rot: unbekannt (Aufgaben, Fristen
/// und Meldungen fehlen dann).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum StandAmpel {
    Gruen,
    Gelb,
    Rot,
    Unbekannt,
}

/// Warum die Ampel so steht (Etappe 3a+: eine Kennung statt eines Satzes;
/// den Satz macht die Oberfläche).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum AmpelGrund {
    /// Eine Fehler-Meldung des Projekts ist offen (nicht behoben, auch gelesen).
    FehlerOffen,
    /// Die Frist eines offenen Eintrags ist vorbei.
    FristVorbei,
    /// Mindestens ein Eintrag wartet auf den Inhaber.
    Wartet,
    /// Nichts wartet auf den Inhaber.
    NichtsWartet,
    /// Der Datenspeicher fehlt; Aufgaben, Fristen und Meldungen sind unbekannt.
    SpeicherFehlt,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct GesamtStand {
    pub titel: String,
    /// Das Wort aus dem Kopf (aktiv, pausiert, geplant, fertig, problem, archiviert).
    pub status: String,
    pub bedeutung: Bedeutung,
    /// Regel-Ergebnis (siehe [`StandAmpel`]).
    pub ampel: StandAmpel,
    pub grund: AmpelGrund,
    /// Bei `fehler-offen` und `frist-vorbei`: die Art dessen, was die Ampel rot macht
    /// (`meldung` oder die Art des Eintrags); sonst `null`.
    pub bezug_art: Option<EintragsArt>,
    /// Sein Titel, nur mit Windows-Anmeldung; sonst `null`.
    pub bezug: Option<String>,
    /// Letzte Änderung der Seite.
    pub zuletzt: String,
}

/// Die Zahlen des Unter-Dashboards als Fakt, nur auf dem freien Weg ohne
/// Anmeldung (Ausnahme aus dem Zielbild: dort gehen keine Einträge hinaus,
/// also kann die Oberfläche nicht selbst zählen). Mit Anmeldung zählt die
/// Oberfläche aus `eintraege` und `aktualisierungen` dasselbe; ein Test hält
/// beide Zählungen gleich.
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
pub struct StandZahlen {
    /// Was auf den Inhaber wartet; je Projekt zählt höchstens eine Frage.
    pub wartet_auf_dich: u64,
    pub aufgaben: u64,
    /// Alle offenen Fragen.
    pub fragen: u64,
    /// Arbeiten je Zustand.
    pub laeuft: u64,
    pub wartet: u64,
    pub als_naechstes: u64,
    pub meldungen: MeldungsZahlen,
    /// Die nächste Frist (JJJJ-MM-TT) eines offenen Eintrags (bei Fragen nur
    /// die der sichtbaren Frage; Aufgaben mit beweglicher Frist aufgelöst)
    /// oder einer offenen Frist aus `daten/fristen.toml` (Etappe B3). Nie aus
    /// `zeitstrahl.toml`.
    pub naechste_frist: Option<String>,
    pub tage_bis: Option<i64>,
    /// Wie nah die nächste Frist ist; `null` ohne Frist.
    pub frist_stufe: Option<FristStufe>,
}

/// Eine Aktualisierung (Etappe A3): eine Meldung oder etwas, das zuletzt
/// passiert ist (ein Vorgang des Projekts: Antwort, Haken, Bau, Abnahme).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct Aktualisierung {
    /// `meldung` oder `vorgang`.
    pub quelle: String,
    /// Kennung der Meldung oder des Vorgangs.
    pub id: String,
    /// Bei Meldungen ihre Art (`fertig`, `frist`, `fehler`, `info`), bei
    /// Vorgängen deren Art (`antwort`, `bau`, …).
    pub art: String,
    pub titel: String,
    /// Ortszeit, RFC 3339.
    pub zeit: String,
    /// Adresse im Dashboard: Meldungen `/projects/<slug>#meldung-<id>` (Systemmeldungen
    /// `/pc#meldung-<id>`), Vorgänge `/vorgaenge#vorgang-<id>`.
    pub ziel: String,
    /// Vorgänge zählen als gelesen; Meldungen, bis der Inhaber „gelesen“ klickt, nicht.
    pub gelesen: bool,
    /// Nur Meldungen: wie oft (ab 1); Vorgänge 1.
    pub anzahl: u64,
    /// Nur Fehler- und Frist-Meldungen: der Grund ist weg.
    pub behoben: bool,
    /// Nur Meldungen: Einzelheiten (etwa der Satz aus dem Prüfbericht).
    pub text: Option<String>,
}

/// Wie viele Meldungen ein Projekt hat (außerhalb des Archivs), ohne Titel
/// und Texte: Das steht auch ohne Windows-Anmeldung im Stand.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, TS)]
pub struct MeldungsZahlen {
    /// Ungelesen (der Inhaber hat noch nicht „gelesen“ geklickt).
    pub neu: u64,
    /// Alle, gelesen oder nicht.
    pub gesamt: u64,
}

/// `GET /api/projekte/<slug>/stand` und `/stand/voll`: der Zustand des
/// Projekts für das Unter-Dashboard, aus derselben Rechnung wie
/// `/api/anstehend` (Etappe 3a+). Mit Windows-Anmeldung (`sichtbar`) alle
/// offenen Einträge und Aktualisierungen in neutraler Ordnung, ohne Zahlen;
/// ohne Anmeldung keine Einträge, nur `zahlen`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct ProjektStand {
    pub slug: String,
    pub schwerpunkt: String,
    pub gesamt: GesamtStand,
    /// Bis zu so vielen Tagen ist eine Frist `bald` (Fakt zu `frist_stufe`).
    pub frist_bald_tage: i64,
    /// Mit Windows-Anmeldung: `eintraege` und `aktualisierungen` sind da.
    pub sichtbar: bool,
    /// Alle offenen Einträge des Projekts (Projekt, Art, Kennung); ohne Anmeldung leer.
    pub eintraege: Vec<AnstehendEintrag>,
    /// Alle Meldungen des Projekts und seine Vorgänge der letzten sieben
    /// Tage (Quelle, Kennung); ohne Anmeldung leer (Grenze aus 7b).
    pub aktualisierungen: Vec<Aktualisierung>,
    /// Nur ohne Anmeldung: die Zahlen als Fakt; mit Anmeldung `null`.
    pub zahlen: Option<StandZahlen>,
    /// `open = [...]` aus dem Kopf.
    /// Etappe B2: nur mit Windows-Anmeldung (`sichtbar`); ohne leer, dann
    /// steht nur `notizen_zahl` da.
    pub notizen: Vec<String>,
    /// Etappe B2: wie viele Notizen es gibt, auch ohne Anmeldung. In
    /// TypeScript optional (ältere Beispiele).
    #[ts(as = "Option<u64>", optional)]
    pub notizen_zahl: u64,
    /// Der Datenspeicher fehlt gerade: Aufgaben und Arbeiten sind leer, die
    /// Notizen und Fragen aus den Dateien stehen trotzdem da.
    pub speicher_fehlt: bool,
    /// Etappe B3: die offenen Fristen aus `daten/fristen.toml` mit Datum
    /// (fest oder aufgelöst), nach Datum; nur mit Windows-Anmeldung, sonst
    /// leer (ohne Anmeldung steckt die nächste in `zahlen.naechste_frist`).
    /// Fehlt im JSON, solange es keine gibt.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[ts(as = "Option<Vec<super::fristen::FristZeile>>", optional)]
    pub fristen: Vec<super::fristen::FristZeile>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn woerter_hin_und_zurueck() {
        for s in Stufe::ALLE {
            assert_eq!(Stufe::aus_wort(s.wort()), Some(s));
            assert_eq!(serde_json::to_value(s).unwrap(), s.wort());
        }
        for a in ArbeitsStand::ALLE {
            assert_eq!(ArbeitsStand::aus_wort(a.wort()), Some(a));
            assert_eq!(serde_json::to_value(a).unwrap(), a.wort());
        }
        for a in EintragsArt::ALLE {
            assert_eq!(EintragsArt::aus_wort(a.wort()), Some(a));
            assert_eq!(serde_json::to_value(a).unwrap(), a.wort());
        }
        for m in MeldungsArt::ALLE {
            assert_eq!(MeldungsArt::aus_wort(m.wort()), Some(m));
            assert_eq!(serde_json::to_value(m).unwrap(), m.wort());
        }
        let stufen: Vec<Stufe> = MeldungsArt::ALLE.iter().map(|m| m.stufe()).collect();
        assert_eq!(stufen, [Stufe::Bald, Stufe::Jetzt, Stufe::Jetzt, Stufe::Irgendwann], "fertig, frist, fehler, info");
        assert!(Stufe::Jetzt < Stufe::Bald && Stufe::Bald < Stufe::Irgendwann);
        assert_eq!(Stufe::default(), Stufe::Bald);
    }
}
