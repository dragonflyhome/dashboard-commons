//! `data/schwerpunkte.toml` (Konzept „Anstehendes“, [schwerpunkt]): die
//! Schwerpunkte für die Übersicht, Eingabe von Hand (gepflegt nach
//! der Antwort des Inhabers). Jedes Projekt gehört zu genau einem Schwerpunkt; ein
//! Projekt ohne Zuordnung steht unter „Sonstiges“. Der Server findet die Datei
//! wie `schluessel.toml` neben dem Projektordner (`<repo>/data/`).
//!
//! Die Regeln für den Prüfbefehl (unbekannte Projekte, ohne Zuordnung,
//! doppelt zugeordnet) stehen in `inhalt::pruefung::pruefe_schwerpunkte`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::lesen::{LeseFehler, toml_lesen};

/// Dateiname, relativ zum Repo (für Meldungen).
pub const DATEI: &str = "data/schwerpunkte.toml";
/// Der Schwerpunkt für Projekte ohne Zuordnung.
pub const SONSTIGES: &str = "Sonstiges";

/// Die ganze Datei.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Schwerpunkte {
    #[serde(default)]
    pub schwerpunkt: Vec<Schwerpunkt>,
}

/// `[[schwerpunkt]]`: Name, Platz in der Übersicht (kleiner = weiter vorn) und Projekte (Slugs).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Schwerpunkt {
    pub name: String,
    pub reihenfolge: i64,
    #[serde(default)]
    pub projekte: Vec<String>,
}

impl Schwerpunkte {
    /// Liest den Text der Datei mit dem Vertrag.
    pub fn lesen(text: &str) -> Result<Self, LeseFehler> {
        toml_lesen(DATEI, text)
    }

    /// Die Schwerpunkte nach `reihenfolge` (bei Gleichstand nach Name).
    pub fn geordnet(&self) -> Vec<&Schwerpunkt> {
        let mut v: Vec<&Schwerpunkt> = self.schwerpunkt.iter().collect();
        v.sort_by(|a, b| a.reihenfolge.cmp(&b.reihenfolge).then_with(|| a.name.cmp(&b.name)));
        v
    }

    /// Projekt → Name des Schwerpunkts. Steht ein Projekt doppelt (ein
    /// Fehler für den Prüfbefehl), gilt der Schwerpunkt, der in der
    /// Übersicht zuerst kommt.
    pub fn zuordnung(&self) -> BTreeMap<String, String> {
        let mut karte = BTreeMap::new();
        for s in self.geordnet() {
            for p in &s.projekte {
                karte.entry(p.clone()).or_insert_with(|| s.name.clone());
            }
        }
        karte
    }

    /// `reihenfolge` von „Sonstiges“: eins mehr als der letzte Schwerpunkt
    /// (ohne Schwerpunkte 1), damit es immer zuletzt kommt.
    pub fn sonstiges_reihenfolge(&self) -> i64 {
        self.schwerpunkt.iter().map(|s| s.reihenfolge).max().unwrap_or(0).saturating_add(1)
    }

    /// Projekt → Name und `reihenfolge` des Schwerpunkts (Etappe 3a+: als
    /// Fakt je Eintrag, die Oberfläche gruppiert danach). Steht ein Name
    /// doppelt, gilt die `reihenfolge` des vorderen. Projekte ohne Eintrag
    /// fehlen; sie gehören zu [`SONSTIGES`] mit [`Self::sonstiges_reihenfolge`].
    pub fn zuordnung_mit_reihenfolge(&self) -> BTreeMap<String, (String, i64)> {
        let mut reihenfolge: BTreeMap<&str, i64> = BTreeMap::new();
        for s in self.geordnet() {
            reihenfolge.entry(s.name.as_str()).or_insert(s.reihenfolge);
        }
        self.zuordnung()
            .into_iter()
            .map(|(p, name)| {
                let r = reihenfolge.get(name.as_str()).copied().unwrap_or_else(|| self.sonstiges_reihenfolge());
                (p, (name, r))
            })
            .collect()
    }

    /// Schwerpunkt eines Projekts mit `reihenfolge`; ohne Zuordnung „Sonstiges“.
    pub fn von(&self, projekt: &str) -> (String, i64) {
        self.zuordnung_mit_reihenfolge().remove(projekt).unwrap_or_else(|| (SONSTIGES.to_string(), self.sonstiges_reihenfolge()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "[[schwerpunkt]]\nname = \"Bienen und Honig\"\nreihenfolge = 5\nprojekte = [\"dashboard\"]\n\n\
                        [[schwerpunkt]]\nname = \"Arbeit\"\nreihenfolge = 2\nprojekte = [\"schaufenster\", \"dashboard\"]\n";

    #[test]
    fn lesen_ordnen_zuordnen() {
        let s = Schwerpunkte::lesen(TEXT).unwrap();
        assert_eq!(s.geordnet().iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["Arbeit", "Bienen und Honig"]);
        // dashboard steht doppelt: es gilt der vordere Schwerpunkt.
        assert_eq!(s.zuordnung()["dashboard"], "Arbeit");
        // Etappe 3a+: Name und reihenfolge je Projekt; Sonstiges hinter dem letzten.
        let k = s.zuordnung_mit_reihenfolge();
        assert_eq!(k["dashboard"], ("Arbeit".to_string(), 2));
        assert_eq!(k["schaufenster"], ("Arbeit".to_string(), 2));
        assert_eq!(k.len(), 2);
        assert_eq!(s.sonstiges_reihenfolge(), 6);
        assert_eq!(s.von("neu"), (SONSTIGES.to_string(), 6));
        assert_eq!(s.von("schaufenster"), ("Arbeit".to_string(), 2));
    }

    #[test]
    fn unbekanntes_feld_ist_ein_fehler() {
        let e = Schwerpunkte::lesen("[[schwerpunkt]]\nname = \"A\"\nreihenfolge = 1\nfarbe = \"rot\"\n").unwrap_err();
        assert_eq!(e.datei, DATEI);
        assert_eq!(e.zeile, Some(4), "{e}");
        let e = Schwerpunkte::lesen("[[schwerpunkt]]\nname = \"A\"\n").unwrap_err();
        assert!(e.text.contains("reihenfolge"), "{e}");
    }

    #[test]
    fn leere_datei_alles_sonstiges() {
        let s = Schwerpunkte::lesen("").unwrap();
        assert_eq!(s.von("a"), (SONSTIGES.to_string(), 1));
    }
}
