//! Die System-Seiten `/entscheidungen`, `/zielbild` (Etappe 4d) und
//! `/anleitung` (Etappe 6b): was der Server aus `docs/entscheidungen/*.toml`,
//! `docs/zielbild/*.toml` und `docs/anleitung/*.toml` liest.
//!
//! Die Dateien schreibt die Architektur-Rolle frei, ohne festes Schema. Der
//! Server liest sie darum duldsam: bekannte Felder kommen einzeln, alles
//! andere als Liste von Feldern mit einfachem Text. Eine Datei, die sich nicht
//! lesen lässt, wird ein Eintrag mit `fehler`, nie ein Fehler der ganzen
//! Liste. Pfade gehen nicht hinaus, nur der Dateiname. Wie es aussieht und
//! welche Wörter dastehen, entscheidet die Oberfläche.

use serde::Serialize;
use ts_rs::TS;

/// Ein Feld einer Datei mit seinem Wert, in der Reihenfolge der Datei.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct Feld {
    /// Der Schlüssel, wie er in der Datei steht (etwa `auflagen_spaeter`).
    pub name: String,
    pub wert: FeldWert,
}

/// Ein Wert als einfacher Text: Zahlen, Wahrheitswerte und Daten werden zu
/// Text, Listen bleiben Listen, Tabellen werden Gruppen von Feldern.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "art", content = "inhalt", rename_all = "lowercase")]
pub enum FeldWert {
    Text(String),
    Liste(Vec<FeldWert>),
    Gruppe(Vec<Feld>),
}

/// Eine Entscheidung aus `docs/entscheidungen/<datei>.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct Entscheidung {
    /// Nur der Dateiname, etwa `2026-10-31-plan-4.toml`.
    pub datei: String,
    /// `datum` aus der Datei; fehlt es, das Datum vorn im Dateinamen.
    pub datum: Option<String>,
    pub thema: Option<String>,
    pub status: Option<String>,
    pub von: Option<String>,
    /// Als Text, auch wenn in der Datei eine Zahl steht (`etappe = 4`).
    pub etappe: Option<String>,
    pub urteil: Option<String>,
    pub was: Option<String>,
    /// Alle übrigen Felder (und bekannte, die keine einfachen Werte sind).
    pub weitere: Vec<Feld>,
    /// Satz in einfachem Deutsch, wenn sich die Datei nicht lesen ließ;
    /// dann sind alle anderen Felder leer.
    pub fehler: Option<String>,
}

/// `GET /api/entscheidungen`: neueste zuerst (nach `datum`, bei gleichem
/// Datum nach Dateiname absteigend).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct EntscheidungsListe {
    pub eintraege: Vec<Entscheidung>,
    /// Satz in einfachem Deutsch (ohne Pfad), wenn der Server den Ordner
    /// nicht lesen darf oder das Lesen sonst scheitert (Hotfix 4d); dann ist
    /// die Liste leer, und die Seite zeigt den Satz als Problem statt „noch
    /// keine Entscheidungen“. Fehlt der Ordner nur, `None`.
    pub warnung: Option<String>,
}

/// Ein Abschnitt einer System-Seite: `/zielbild` aus
/// `docs/zielbild/<datei>.toml`, `/anleitung` (seit 6b) aus
/// `docs/anleitung/<datei>.toml`; ein Format, ein Baustein für beide Seiten.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct SystemAbschnitt {
    /// Nur der Dateiname.
    pub datei: String,
    pub titel: Option<String>,
    /// Kleinere zuerst; ohne Angabe ans Ende.
    pub reihenfolge: Option<i64>,
    /// Fließtext; eine Leerzeile beginnt einen neuen Absatz.
    pub text: Option<String>,
    /// Aufzählung unter dem Text.
    pub punkte: Vec<String>,
    /// `nummeriert = true`: die Punkte als nummerierte Liste (freiwillig).
    pub nummeriert: bool,
    /// Die `[[block]]` der Datei in ihrer Reihenfolge, unter Text und
    /// Punkten. Ein Block, der sich nicht zeigen lässt, fehlt hier und steht
    /// als Satz in `hinweise`.
    pub bloecke: Vec<SystemBlock>,
    /// Sätze „Block n: …“ zu Blöcken, die fehlen oder nur zum Teil gezeigt
    /// werden (kein oder mehr als ein Inhalt, falsche Zeilenlänge einer
    /// Tabelle, unbekanntes Feld). Der Rest des Abschnitts bleibt.
    pub hinweise: Vec<String>,
    /// Alle übrigen Felder als einfacher Text.
    pub weitere: Vec<Feld>,
    /// Satz in einfachem Deutsch, wenn sich die Datei nicht lesen ließ.
    pub fehler: Option<String>,
}

/// Ein Block eines Abschnitts (`[[block]]`): eine kleine Überschrift und
/// genau ein Inhalt. Der Name trägt `System` vorn, weil `Block` im Vertrag
/// schon die Anzeige-Bausteine der Projektseiten sind (`vertrag::bausteine`).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct SystemBlock {
    /// Kleine Überschrift über dem Block.
    pub titel: Option<String>,
    /// `nummeriert = true` bei `punkte`: eine nummerierte Liste.
    pub nummeriert: bool,
    pub inhalt: SystemBlockInhalt,
}

/// Der eine Inhalt eines Blocks. Alles bleibt reiner Text, keine Auszeichnung.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "art", content = "wert", rename_all = "lowercase")]
pub enum SystemBlockInhalt {
    /// Absätze wie `text` des Abschnitts.
    Text(String),
    /// Eine Aufzählung.
    Punkte(Vec<String>),
    /// Eine Tabelle; jede Zeile so lang wie `spalten`.
    Tabelle(SystemBlockTabelle),
    /// Vorformatiert: Monospace, die Zeilen bleiben, rollt seitlich.
    Beispiel(String),
}

/// `[block.tabelle]`: Spaltenköpfe und Zeilen als Text.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct SystemBlockTabelle {
    pub spalten: Vec<String>,
    /// Nur Zeilen mit so vielen Feldern wie `spalten`; andere fehlen und
    /// stehen als Hinweis im Abschnitt.
    pub zeilen: Vec<Vec<String>>,
}

/// `GET /api/zielbild`: leer, solange der Ordner fehlt oder nichts darin
/// liegt; dann zeigt die Seite den Platzhalter.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct ZielbildAntwort {
    pub abschnitte: Vec<SystemAbschnitt>,
    /// Wie bei [`EntscheidungsListe::warnung`]: Satz, wenn der Ordner sich
    /// nicht lesen lässt (kein Recht oder anderer Fehler); dann zeigt die
    /// Seite ihn statt des Platzhalters. Fehlt der Ordner nur, `None`.
    pub warnung: Option<String>,
}

/// `GET /api/anleitung` (Etappe 6b): die Anleitung aus
/// `docs/anleitung/*.toml` im Code-Repo, in derselben Form wie das Zielbild
/// (ein Abschnitt je Datei, nach `reihenfolge`). Leer, solange der Ordner
/// fehlt; `warnung` wie bei [`ZielbildAntwort::warnung`].
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct AnleitungAntwort {
    pub abschnitte: Vec<SystemAbschnitt>,
    pub warnung: Option<String>,
}
