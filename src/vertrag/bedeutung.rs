//! Farben sind Bedeutungen (Zielbild, Abschnitt b): Im JSON stehen nur die
//! sechs Wörter `ok`, `laeuft`, `wartet`, `problem`, `info` und `neutral`,
//! keine CSS-Klassen. Die deutschen Farb- und Statuswörter aus TOML
//! (`grün`, `fertig`, `rot` …) übersetzt der Server hierher.
//!
//! Welche Klasse und welcher Ton zu einer Bedeutung gehören, entscheidet nur
//! die Oberfläche (`web/src/lib/bedeutung.ts`, `klasse()`: eine Klasse je
//! Bedeutung, die den Ton setzt). Der Server kennt keine Klassennamen.
//!
//! Welche Wörter zu welcher Bedeutung gehören:
//!
//! | Wörter | Bedeutung |
//! |---|---|
//! | fertig, erledigt, grün, gesichert, belegt; Fortschritt 100 % | `ok` |
//! | aktiv, läuft; Fortschritt 50 bis 99 % | `laeuft` |
//! | wartet, offen, pausiert, gelb, unsicher, vermutet; Fortschritt unter 50 % | `wartet` |
//! | problem, rot, blockiert, widerlegt | `problem` |
//! | blau, in Arbeit, aktuell | `info` |
//! | geplant, archiviert, grau (auch alles Unbekannte) | `neutral` |
//!
//! Zwei Wörter hatten früher zwei Farben und haben jetzt eine: `geplant`
//! ist `neutral`; `aktiv` und `läuft` sind `laeuft`, wie beim Status.

use serde::{Deserialize, Deserializer, Serialize};
use ts_rs::TS;

/// Eine der sechs Bedeutungen. Gelesen werden auch die deutschen Wörter
/// (siehe [`Bedeutung::aus_wort`]); geschrieben wird immer das Bedeutungswort.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Bedeutung {
    /// Erledigt, in Ordnung (grün).
    Ok,
    /// Ist in Arbeit, läuft gerade.
    Laeuft,
    /// Wartet, pausiert, halb fertig (gelb).
    Wartet,
    /// Kaputt, blockiert, fehlt (rot).
    Problem,
    /// Hinweis ohne Wertung (blau).
    Info,
    /// Geplant, unbekannt, ohne Farbe (grau).
    Neutral,
}

/// Alle Wörter, die der Server versteht, und ihre Bedeutung. Groß und klein
/// ist egal, Leerzeichen am Rand auch.
const WOERTER: &[(&str, Bedeutung)] = &[
    // ok
    ("ok", Bedeutung::Ok),
    ("grün", Bedeutung::Ok),
    ("gruen", Bedeutung::Ok),
    ("green", Bedeutung::Ok),
    ("fertig", Bedeutung::Ok),
    ("erledigt", Bedeutung::Ok),
    ("erreicht", Bedeutung::Ok),
    // Belegung einer Angabe (Tabellen mit Quelle und Sicherheit)
    ("gesichert", Bedeutung::Ok),
    ("belegt", Bedeutung::Ok),
    // laeuft
    ("laeuft", Bedeutung::Laeuft),
    ("läuft", Bedeutung::Laeuft),
    ("aktiv", Bedeutung::Laeuft),
    // wartet
    ("wartet", Bedeutung::Wartet),
    ("gelb", Bedeutung::Wartet),
    ("yellow", Bedeutung::Wartet),
    ("offen", Bedeutung::Wartet),
    ("pausiert", Bedeutung::Wartet),
    ("vertagt", Bedeutung::Wartet),
    ("läuft bald", Bedeutung::Wartet),
    ("laeuft bald", Bedeutung::Wartet),
    ("teils", Bedeutung::Wartet),
    ("warn", Bedeutung::Wartet),
    // Belegung: noch nicht gesichert, Vorsicht (gelb, nicht blau: blau ist
    // „Hinweis ohne Wertung“, unsicher wertet aber).
    ("unsicher", Bedeutung::Wartet),
    ("vermutet", Bedeutung::Wartet),
    // problem
    ("problem", Bedeutung::Problem),
    ("rot", Bedeutung::Problem),
    ("red", Bedeutung::Problem),
    ("blockiert", Bedeutung::Problem),
    ("fehlt", Bedeutung::Problem),
    ("kritisch", Bedeutung::Problem),
    ("kaputt", Bedeutung::Problem),
    ("fehler", Bedeutung::Problem),
    ("keins", Bedeutung::Problem),
    ("bad", Bedeutung::Problem),
    // Belegung: das Gegenteil ist belegt
    ("widerlegt", Bedeutung::Problem),
    // info
    ("info", Bedeutung::Info),
    ("blau", Bedeutung::Info),
    ("blue", Bedeutung::Info),
    ("in arbeit", Bedeutung::Info),
    ("aktuell", Bedeutung::Info),
    // neutral
    ("neutral", Bedeutung::Neutral),
    ("geplant", Bedeutung::Neutral),
    ("idee", Bedeutung::Neutral),
    ("grau", Bedeutung::Neutral),
    ("plan", Bedeutung::Neutral),
    // Entscheidung archiv-status: archiviert ist ruhig (keine Farbe).
    ("archiviert", Bedeutung::Neutral),
];

impl Bedeutung {
    #[cfg_attr(not(test), allow(dead_code))]
    pub const ALLE: [Bedeutung; 6] = [Self::Ok, Self::Laeuft, Self::Wartet, Self::Problem, Self::Info, Self::Neutral];

    /// Was eine Fehlermeldung als erlaubt nennt: die sechs Bedeutungen und die
    /// üblichen Farbwörter. Verstanden werden mehr (siehe `WOERTER`).
    pub const ERLAUBT: &'static [&'static str] =
        &["ok", "laeuft", "wartet", "problem", "info", "neutral", "grün", "gelb", "rot", "blau", "grau", "fertig", "aktiv"];

    /// Das Wort im JSON.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn wort(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Laeuft => "laeuft",
            Self::Wartet => "wartet",
            Self::Problem => "problem",
            Self::Info => "info",
            Self::Neutral => "neutral",
        }
    }

    /// Bedeutung eines deutschen (oder englischen) Farb- oder Statusworts.
    /// `None`, wenn das Wort unbekannt ist.
    pub fn aus_wort(wort: &str) -> Option<Self> {
        let wort = wort.trim().to_lowercase();
        WOERTER.iter().find(|(w, _)| *w == wort).map(|(_, b)| *b)
    }

    /// Wie [`Bedeutung::aus_wort`], aber Unbekanntes wird `neutral` (so wie
    /// heute unbekannte Wörter grau sind). Für den Status als Wort.
    pub fn aus_wort_oder_neutral(wort: &str) -> Self {
        Self::aus_wort(wort).unwrap_or(Self::Neutral)
    }
}

impl<'de> Deserialize<'de> for Bedeutung {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let wort = String::deserialize(d)?;
        Self::aus_wort(&wort).ok_or_else(|| serde::de::Error::unknown_variant(&wort, Self::ERLAUBT))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deutsche_woerter_werden_bedeutungen() {
        assert_eq!(Bedeutung::aus_wort("grün"), Some(Bedeutung::Ok));
        assert_eq!(Bedeutung::aus_wort(" Grün "), Some(Bedeutung::Ok));
        assert_eq!(Bedeutung::aus_wort("fertig"), Some(Bedeutung::Ok));
        assert_eq!(Bedeutung::aus_wort("aktiv"), Some(Bedeutung::Laeuft));
        assert_eq!(Bedeutung::aus_wort("Läuft"), Some(Bedeutung::Laeuft));
        assert_eq!(Bedeutung::aus_wort("pausiert"), Some(Bedeutung::Wartet));
        assert_eq!(Bedeutung::aus_wort("archiviert"), Some(Bedeutung::Neutral));
        assert_eq!(Bedeutung::aus_wort("teils"), Some(Bedeutung::Wartet));
        assert_eq!(Bedeutung::aus_wort("rot"), Some(Bedeutung::Problem));
        assert_eq!(Bedeutung::aus_wort("keins"), Some(Bedeutung::Problem));
        assert_eq!(Bedeutung::aus_wort("in Arbeit"), Some(Bedeutung::Info));
        assert_eq!(Bedeutung::aus_wort("geplant"), Some(Bedeutung::Neutral));
        assert_eq!(Bedeutung::aus_wort("lila"), None);
        assert_eq!(Bedeutung::aus_wort_oder_neutral("lila"), Bedeutung::Neutral);
    }

    /// Wörter zur Belegung einer Angabe: groß und klein egal, nur die
    /// Grundform; Abstufungen wie „wahrscheinlich“ bleiben ohne Farbe.
    #[test]
    fn woerter_zur_belegung() {
        assert_eq!(Bedeutung::aus_wort("gesichert"), Some(Bedeutung::Ok));
        assert_eq!(Bedeutung::aus_wort("Gesichert"), Some(Bedeutung::Ok));
        assert_eq!(Bedeutung::aus_wort(" belegt "), Some(Bedeutung::Ok));
        assert_eq!(Bedeutung::aus_wort("unsicher"), Some(Bedeutung::Wartet));
        assert_eq!(Bedeutung::aus_wort("Vermutet"), Some(Bedeutung::Wartet));
        assert_eq!(Bedeutung::aus_wort("WIDERLEGT"), Some(Bedeutung::Problem));
        assert_eq!(Bedeutung::aus_wort("wahrscheinlich"), None);
        assert_eq!(Bedeutung::aus_wort("nur eine Quelle"), None);
        assert_eq!(Bedeutung::aus_wort("gesicherte"), None, "keine Beugungen, wie bei den übrigen Wörtern");
    }

    #[test]
    fn jedes_bedeutungswort_steht_fuer_sich_selbst() {
        for b in Bedeutung::ALLE {
            assert_eq!(Bedeutung::aus_wort(b.wort()), Some(b));
            assert_eq!(serde_json::to_string(&b).unwrap(), format!("\"{}\"", b.wort()));
        }
    }

    #[test]
    fn alles_erlaubte_wird_verstanden() {
        for w in Bedeutung::ERLAUBT {
            assert!(Bedeutung::aus_wort(w).is_some(), "{w}");
        }
        for (w, _) in WOERTER {
            assert_eq!(w.trim().to_lowercase(), *w, "Tabelle nur klein und ohne Rand: {w}");
        }
    }

    #[test]
    fn lesen_nimmt_woerter_schreiben_gibt_bedeutung() {
        #[derive(Debug, Deserialize)]
        struct T {
            farbe: Bedeutung,
        }
        let t: T = toml::from_str("farbe = \"grün\"").unwrap();
        assert_eq!(t.farbe, Bedeutung::Ok);
        assert_eq!(serde_json::to_string(&t.farbe).unwrap(), "\"ok\"");
        let e = toml::from_str::<T>("farbe = \"lila\"").unwrap_err();
        assert!(e.message().contains("lila"), "{e}");
    }
}
