//! `GET /api/fassungen`: die Fassungen des Dienstes aus seinem Changelog
//! (`CHANGELOG.txt`, beim Bau eingebettet). Gelesen von `crate::fassungen`.

use serde::Serialize;
use ts_rs::TS;

/// Die Zeilen eines Abschnitts im Changelog, je Unterabschnitt in der
/// Reihenfolge der Datei; eine Zeile ist ein Eintrag („- …“) samt seiner
/// eingerückten Folgezeilen, ohne den Strich.
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
pub struct FassungsZeilen {
    pub hinzugefuegt: Vec<String>,
    pub geaendert: Vec<String>,
    pub entfernt: Vec<String>,
    pub behoben: Vec<String>,
    /// Die Einträge „Bruch: …“ unter Geändert und Entfernt noch einmal (sie
    /// stehen auch dort).
    pub brueche: Vec<String>,
}

impl FassungsZeilen {
    /// Ohne jeden Eintrag.
    pub fn leer(&self) -> bool {
        self.hinzugefuegt.is_empty() && self.geaendert.is_empty() && self.entfernt.is_empty() && self.behoben.is_empty()
    }
}

/// Eine herausgegebene Fassung („x.y.z - JJJJ-MM-TT“).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct FassungsEintrag {
    /// `x.y.z`.
    pub version: String,
    /// Der Tag dazu: `v` und die Version (jede Fassung hat genau einen).
    pub tag: String,
    /// `JJJJ-MM-TT`.
    pub datum: String,
    #[serde(flatten)]
    pub zeilen: FassungsZeilen,
}

/// `GET /api/fassungen` (frei: das Changelog ist öffentlich).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct FassungenAntwort {
    /// Die Version des laufenden Dienstes (aus Cargo.toml).
    pub laufend: String,
    /// Der Abschnitt Unreleased: was seit der letzten Fassung dazukam. Hat
    /// er Einträge, läuft ein Bau zwischen zwei Fassungen.
    pub unveroeffentlicht: FassungsZeilen,
    /// Neueste zuerst; mit `?seit=<version>` nur die neueren.
    pub fassungen: Vec<FassungsEintrag>,
    /// Was sich im Changelog nicht lesen ließ, je ein Satz (leer: alles gelesen).
    pub hinweise: Vec<String>,
}
