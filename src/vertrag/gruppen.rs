//! `data/gruppen.toml` (Entscheidung projektgruppe): Projekte, die
//! als eine Karte und eine Seite erscheinen. Die Projekte selbst bleiben
//! unverändert (eigener Ordner, eigener Chat, eigene Daten); die Gruppe steht
//! nur hier, gepflegt von der Koordination. Der Server findet die Datei wie
//! `schwerpunkte.toml` neben dem Projektordner (`<repo>/data/`). Fehlt sie,
//! gibt es keine Gruppen (kein Fehler).
//!
//! ```toml
//! [[gruppe]]
//! slug = "alpha-und-beta"
//! titel = "Alpha und Beta"
//! teile = ["alpha", "beta"]   # Reihenfolge = Reihenfolge auf der Seite
//! ```
//!
//! Die Regeln für den Prüfbefehl (unbekannte oder doppelte Teile, ein Slug,
//! den es als Projekt schon gibt) stehen in `inhalt::pruefung::pruefe_gruppen`.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::lesen::{LeseFehler, toml_lesen};

/// Dateiname, relativ zum Repo (für Meldungen).
pub const DATEI: &str = "data/gruppen.toml";

/// Die ganze Datei.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gruppen {
    #[serde(default)]
    pub gruppe: Vec<Gruppe>,
}

/// `[[gruppe]]`: Adresse (`/gruppen/<slug>`), Titel und die Projekte (Slugs)
/// in der Reihenfolge der Seite.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gruppe {
    pub slug: String,
    pub titel: String,
    #[serde(default)]
    pub teile: Vec<String>,
}

/// Eine Gruppe, wie die Oberfläche sie zeigt (`/api/projects`, Feld
/// `gruppen`): nur sichtbare Teile (es gibt sie, nicht archiviert), in der
/// Reihenfolge aus `teile`. Eine Gruppe ohne sichtbaren Teil fehlt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct GruppeAnzeige {
    pub slug: String,
    pub titel: String,
    /// Slugs der Projekte, in der Reihenfolge der Seite.
    pub teile: Vec<String>,
}

/// Zu welcher Gruppe ein Projekt gehört (etwa an jedem Eintrag in „Anstehend“).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct GruppenBezug {
    pub slug: String,
    pub titel: String,
}

impl Gruppen {
    /// Liest den Text der Datei mit dem Vertrag.
    pub fn lesen(text: &str) -> Result<Self, LeseFehler> {
        toml_lesen(DATEI, text)
    }

    /// Die Gruppen für die Oberfläche: je Teil nur, wenn `sichtbar(slug)`
    /// (es gibt das Projekt, es ist nicht archiviert). Ein Projekt gehört
    /// höchstens einer Gruppe an: steht es doppelt (ein Fehler für den
    /// Prüfbefehl), gilt das erste Vorkommen. Ein doppelter Gruppen-Slug gilt
    /// nur beim ersten Mal. Gruppen ohne sichtbaren Teil fehlen.
    ///
    /// Auch zur Laufzeit, nicht nur im Prüfbefehl: Eine Gruppe mit
    /// ungültigem Slug oder mit dem Slug eines Projekts (`projekt_da`, auch
    /// archivierte) fehlt ganz ([`Self::abgelehnt`] nennt sie). Sonst
    /// stünden in Seitenleiste und Adressen zwei gleiche Schlüssel oder eine
    /// Adresse, die sich nicht bauen lässt.
    pub fn sichtbar(&self, sichtbar: impl Fn(&str) -> bool, projekt_da: impl Fn(&str) -> bool) -> Vec<GruppeAnzeige> {
        let mut vergeben: BTreeSet<&str> = BTreeSet::new();
        let mut slugs: BTreeSet<&str> = BTreeSet::new();
        let mut aus = Vec::new();
        for g in &self.gruppe {
            if slug_abgelehnt(&g.slug, &projekt_da).is_some() || !slugs.insert(g.slug.as_str()) {
                continue;
            }
            let mut teile = Vec::new();
            for t in &g.teile {
                if vergeben.insert(t.as_str()) && sichtbar(t) {
                    teile.push(t.clone());
                }
            }
            if !teile.is_empty() {
                aus.push(GruppeAnzeige { slug: g.slug.clone(), titel: g.titel.clone(), teile });
            }
        }
        aus
    }
}

impl Gruppen {
    /// Die Gruppen, die [`Self::sichtbar`] wegen ihres Slugs weglässt, je mit
    /// einem Satz (für die Hinweise in „Anstehend“).
    pub fn abgelehnt(&self, projekt_da: impl Fn(&str) -> bool) -> Vec<String> {
        self.gruppe
            .iter()
            .filter_map(|g| slug_abgelehnt(&g.slug, &projekt_da).map(|grund| format!("{DATEI}: Die Gruppe „{}“ fehlt, {grund}", g.titel)))
            .collect()
    }
}

/// Warum ein Gruppen-Slug nicht geht, sonst `None`.
fn slug_abgelehnt(slug: &str, projekt_da: &impl Fn(&str) -> bool) -> Option<String> {
    if !crate::inhalt::seite::is_valid_slug(slug) {
        Some(format!("weil ihr Slug „{slug}“ nicht geht (nur a-z, 0-9 und Bindestrich)."))
    } else if projekt_da(slug) {
        Some(format!("weil es den Slug „{slug}“ schon als Projekt gibt."))
    } else {
        None
    }
}

/// Die Gruppe eines Projekts aus der Liste von [`Gruppen::sichtbar`].
pub fn bezug(gruppen: &[GruppeAnzeige], projekt: &str) -> Option<GruppenBezug> {
    gruppen.iter().find(|g| g.teile.iter().any(|t| t == projekt)).map(|g| GruppenBezug { slug: g.slug.clone(), titel: g.titel.clone() })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "[[gruppe]]\nslug = \"alpha-und-beta\"\ntitel = \"Alpha und Beta\"\nteile = [\"alpha\", \"beta\"]\n\n\
                        [[gruppe]]\nslug = \"gamma-gruppe\"\ntitel = \"Gamma\"\nteile = [\"gamma\", \"beta\"]\n";

    #[test]
    fn lesen_und_sichtbar() {
        let g = Gruppen::lesen(TEXT).unwrap();
        assert_eq!(g.gruppe.len(), 2);
        let alle = g.sichtbar(|_| true, |_| false);
        // beta steht doppelt: es gilt das erste Vorkommen; Reihenfolge wie in teile.
        assert_eq!(alle[0].teile, ["alpha", "beta"]);
        assert_eq!(alle[1].teile, ["gamma"]);
        assert_eq!(bezug(&alle, "beta"), Some(GruppenBezug { slug: "alpha-und-beta".into(), titel: "Alpha und Beta".into() }));
        assert_eq!(bezug(&alle, "delta"), None);
    }

    /// Archiv-Status: Ein archivierter Teil fehlt; sind alle Teile weg, fehlt die Gruppe.
    #[test]
    fn archivierte_teile_fehlen() {
        let g = Gruppen::lesen(TEXT).unwrap();
        let s = g.sichtbar(|t| t != "alpha", |_| false);
        assert_eq!(s[0].teile, ["beta"]);
        let s = g.sichtbar(|t| t == "alpha" || t == "beta", |_| false);
        assert_eq!(s.iter().map(|g| g.slug.as_str()).collect::<Vec<_>>(), ["alpha-und-beta"]);
        assert!(g.sichtbar(|_| false, |_| false).is_empty());
    }

    /// Zur Laufzeit: ungültige Slugs und Projekt-Slugs fehlen, mit Satz.
    #[test]
    fn schlechte_slugs_fehlen() {
        let text = "[[gruppe]]
slug = \"\"
titel = \"Leer\"
teile = [\"a\"]

                    [[gruppe]]
slug = \"x/\"
titel = \"Schraeg\"
teile = [\"b\"]

                    [[gruppe]]
slug = \"website\"
titel = \"Kollision\"
teile = [\"c\"]

                    [[gruppe]]
slug = \"gut\"
titel = \"Gut\"
teile = [\"a\", \"d\"]
";
        let g = Gruppen::lesen(text).unwrap();
        let s = g.sichtbar(|_| true, |p| p == "website");
        assert_eq!(s.iter().map(|g| (g.slug.as_str(), g.teile.clone())).collect::<Vec<_>>(), [("gut", vec!["a".to_string(), "d".to_string()])]);
        let saetze = g.abgelehnt(|p| p == "website");
        assert_eq!(saetze.len(), 3, "{saetze:?}");
        assert!(saetze[2].contains("„website“ schon als Projekt"), "{saetze:?}");
    }

    #[test]
    fn unbekanntes_feld_und_leere_datei() {
        let e = Gruppen::lesen("[[gruppe]]\nslug = \"a\"\ntitel = \"A\"\nfarbe = \"rot\"\n").unwrap_err();
        assert_eq!((e.datei.as_str(), e.zeile), (DATEI, Some(4)), "{e}");
        let e = Gruppen::lesen("[[gruppe]]\nslug = \"a\"\n").unwrap_err();
        assert!(e.text.contains("titel"), "{e}");
        assert!(Gruppen::lesen("").unwrap().sichtbar(|_| true, |_| false).is_empty());
    }
}
