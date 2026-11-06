//! Das Modul Backlog (Etappe B2, Entscheidung gemeinsame-bausteine):
//! Aufgaben, Arbeiten und Fragen eines Projekts als Spalten nach Stand und
//! Status (die Kanban-Sicht aus dem Datenspeicher, statt `daten/kanban.toml`
//! von Hand), dazu die wartenden Einträge je Verantwortlichem. Hier stehen
//! nur die Datenformen; gerechnet wird in `crate::backlog`.

use serde::Serialize;
use ts_rs::TS;

use super::anstehend::{EintragsArt, FristStufe};
use super::bedeutung::Bedeutung;

/// Die feste Folge der Spalten. Die Spalte ergibt sich allein aus Stand und
/// Status im Datenspeicher (und für Fragen aus der Warteschlange).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum BacklogSpaltenArt {
    /// Wartet auf den Inhaber: offene Aufgaben (nicht beim Chat), die eine
    /// sichtbare Frage, Arbeiten mit `wartet_auf = inhaber` ohne Verweis.
    FuerDich,
    /// Arbeiten mit Stand `laeuft`.
    Laeuft,
    /// Wartet auf jemand anderen: Arbeiten mit `wartet` (Chat, extern; auf
    /// den Inhaber nur mit Verweis auf eine offene Aufgabe oder Frage) und
    /// Aufgaben beim Chat (eigene Worte geschickt).
    Wartet,
    /// Arbeiten mit Stand `als-naechstes`.
    AlsNaechstes,
    /// Abgeschlossen und noch nicht im Archiv (höchstens sieben Tage):
    /// fertige Arbeiten, erledigte, abgelaufene und zurückgezogene Aufgaben.
    Fertig,
}

impl BacklogSpaltenArt {
    pub const ALLE: [BacklogSpaltenArt; 5] = [Self::FuerDich, Self::Laeuft, Self::Wartet, Self::AlsNaechstes, Self::Fertig];

    /// Die Überschrift der Spalte.
    pub fn titel(self) -> &'static str {
        match self {
            Self::FuerDich => "Für dich",
            Self::Laeuft => "Läuft",
            Self::Wartet => "Wartet",
            Self::AlsNaechstes => "Als Nächstes",
            Self::Fertig => "Fertig",
        }
    }

    /// Die Bedeutung (Punktfarbe) der Spalte.
    pub fn bedeutung(self) -> Bedeutung {
        match self {
            Self::FuerDich => Bedeutung::Wartet,
            Self::Laeuft => Bedeutung::Laeuft,
            Self::Wartet => Bedeutung::Info,
            Self::AlsNaechstes => Bedeutung::Neutral,
            Self::Fertig => Bedeutung::Ok,
        }
    }
}

/// Wer an der Reihe ist. Die Rollen sind die aus `wartet_auf` der Arbeiten:
/// `inhaber`, `chat:<name>` oder `extern`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Verantwortlich {
    Inhaber,
    Chat,
    Extern,
}

/// Eine Karte im Backlog: ein Eintrag mit Titel und Ziel im Dashboard.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct BacklogKarte {
    pub art: EintragsArt,
    /// Kennung: Aufgabe `a-…`, Arbeit `ar-…`, Frage ihre `id`.
    pub id: String,
    /// Titel der Aufgabe oder Arbeit, Text der Frage.
    pub titel: String,
    /// Der Zustand als Wort: Aufgabe `offen`, `beim-chat`, `erledigt`,
    /// `abgelaufen`, `zurueckgezogen`; Arbeit ihr Stand; Frage `offen`.
    pub zustand: String,
    /// Nur bei wartenden Einträgen: auf wen (`inhaber`, `chat:<name>`, `extern`).
    pub wartet_auf: Option<String>,
    /// JJJJ-MM-TT.
    pub frist: Option<String>,
    /// Wie nah die Frist ist (dieselbe Regel wie in Anstehend); `null` ohne Frist.
    pub frist_stufe: Option<FristStufe>,
    /// Ein kurzer Zusatz, etwa „noch 2 Fragen warten“ an der sichtbaren
    /// Frage oder „zurückgezogen“ in der Spalte „Fertig“.
    pub hinweis: Option<String>,
    /// Detailansicht im Dashboard: `/projects/<slug>#<art>-<id>`.
    pub ziel: String,
    /// Ortszeit RFC 3339 der letzten Änderung (bei Fragen das Datum `gestellt`).
    pub geaendert: String,
}

/// Eine Spalte mit ihren Karten (feste Ordnung je Spalte, siehe `crate::backlog`).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct BacklogSpalte {
    pub art: BacklogSpaltenArt,
    pub titel: String,
    pub bedeutung: Bedeutung,
    pub karten: Vec<BacklogKarte>,
}

/// Die wartenden Einträge eines Verantwortlichen (Etappe B2: „wartende
/// Arbeiten je Verantwortlichem“).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct WartendGruppe {
    pub rolle: Verantwortlich,
    /// Der Wert aus `wartet_auf`: `inhaber`, `chat:<name>` oder `extern`.
    pub wer: String,
    /// Für Menschen: „Wartet auf dich“, „Wartet auf den Chat „x““, „Wartet auf jemanden draußen“.
    pub titel: String,
    pub karten: Vec<BacklogKarte>,
}

/// `GET /api/projekte/<slug>/backlog` (geschützt wie `/api/anstehend`): das
/// Backlog eines Projekts.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct Backlog {
    pub slug: String,
    /// Das lokale Datum, mit dem gerechnet wurde (JJJJ-MM-TT).
    pub heute: String,
    /// Alle fünf Spalten in fester Folge, auch leere.
    pub spalten: Vec<BacklogSpalte>,
    /// Was wartet, je Verantwortlichem: Inhaber, dann Chats (nach Name), dann extern.
    /// Nur Gruppen mit Karten.
    pub wartend: Vec<WartendGruppe>,
    /// Die Warteschlange der Fragen: wie viele offene Fragen hinter der
    /// sichtbaren warten (ohne deren Text).
    pub fragen_weitere: u64,
    /// Das Projekt hat noch `daten/kanban.toml` (läuft aus; `pruefe` gibt einen Hinweis).
    pub kanban_datei: bool,
    /// Der Datenspeicher fehlt: Aufgaben und Arbeiten fehlen, die Fragen aus der Datei stehen da.
    pub speicher_fehlt: bool,
}
