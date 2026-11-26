//! `daten/gebete.toml` (Kalender, Stufe 4): welche Varianten der Gebetszeiten
//! an welchem benannten Ort erscheinen, im privaten Inhalt im Projekt `kalender`.
//! Die Datei schreibt nur der Server (`POST /api/kalender/gebete`).
//!
//! Die Auswahl ist persönlich: Sie geht nie hinaus (nicht im Daten-Weg, nicht in
//! Fehlersätzen, nicht ins Protokoll). Sätze nennen höchstens die Ortskennung, nie
//! eine Variante.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::daten::{Baustein, Datei, ist_kennung};
use super::lesen::{LeseFehler, Mangel};
use crate::kalender::gebete::Vorlage;

/// `daten/gebete.toml`. Nach außen leer: Die Auswahl geht nie hinaus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Gebete {
    #[serde(default, skip_serializing)]
    #[ts(skip)]
    pub auswahl: Vec<GebeteAuswahl>,
}

/// Die Varianten an einem Ort.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GebeteAuswahl {
    /// Kennung eines benannten Orts (auch `mitte` aus der Vorlage).
    pub ort: String,
    /// Kennungen von Varianten aus `vorlagen/kalender/gebete.toml`.
    pub varianten: Vec<String>,
}

/// Die Vorlage der Gebetszeiten. In Tests mit der Probevariante `spuerhund-gebet`
/// (eine Zeile „SPUERHUND-GEBET“ nach Dhuhr), damit die Datenschutz-Tests eine
/// Auswahl verfolgen können, deren Namen sonst nirgends vorkommen.
pub fn vorlage() -> &'static Vorlage {
    #[cfg(test)]
    {
        static PROBE: std::sync::OnceLock<Vorlage> = std::sync::OnceLock::new();
        PROBE.get_or_init(|| {
            let mut v = crate::kalender::gebete::vorlage().clone();
            if let Some(mut p) = v.variante.first().cloned() {
                p.kennung = "spuerhund-gebet".into();
                p.titel = "Spürhund".into();
                p.zeiten = vec![crate::kalender::gebete::ZeitDef { name: "SPUERHUND-GEBET".into(), regel: "dhuhr".into(), umfasst: Vec::new() }];
                v.variante.push(p);
            }
            v
        })
    }
    #[cfg(not(test))]
    crate::kalender::gebete::vorlage()
}

impl Baustein for Gebete {
    const DATEI: &'static str = "gebete";

    /// Form je Auswahl. Die Sätze nennen höchstens die Ortskennung.
    fn pruefen(&self) -> Result<(), Mangel> {
        let bekannt = &vorlage().variante;
        for a in &self.auswahl {
            if !ist_kennung(&a.ort) {
                return Err(Mangel::bei(
                    "ort",
                    &a.ort,
                    "Eine Auswahl der Gebetszeiten hat keine einfache Ortskennung (Kleinbuchstaben, Ziffern, Bindestriche).",
                ));
            }
            let o = &a.ort;
            if a.varianten.is_empty() {
                return Err(Mangel::bei("ort", o, format!("Die Auswahl der Gebetszeiten für den Ort „{o}“ nennt keine Variante.")));
            }
            if !a.varianten.iter().all(|v| bekannt.iter().any(|b| b.kennung == *v)) {
                return Err(Mangel::bei("ort", o, format!("Die Auswahl der Gebetszeiten für den Ort „{o}“ nennt eine Variante, die die Vorlage nicht kennt.")));
            }
        }
        Ok(())
    }
}

/// Anfänge der eigenen Sätze; sie nennen höchstens die Ortskennung.
const EIGENE_SAETZE: [&str; 3] = ["Eine Auswahl der Gebetszeiten", "Die Auswahl der Gebetszeiten", "Den Ort „"];

/// Ein Lesefehler ohne Werte: Sätze von TOML zitieren die Zeile (eine Variante)
/// und werden durch einen allgemeinen Satz mit der Zeilennummer ersetzt.
pub fn ohne_werte(e: LeseFehler) -> LeseFehler {
    if EIGENE_SAETZE.iter().any(|a| e.text.starts_with(a)) {
        return e;
    }
    let text = "Diese Zeile passt nicht zur Form der Auswahl der Gebetszeiten. Je Ort: `ort` als Kennung und `varianten` als Liste von Kennungen. (Der Satz zitiert die Zeile nicht, weil die Auswahl persönlich ist.)".to_string();
    LeseFehler { text, ..e }
}

/// Jeder Ort höchstens einmal, auch über Teile hinweg.
pub fn gebete_eindeutig(d: &Datei<Gebete>, datei: &str, text: &str) -> Result<(), LeseFehler> {
    let mut gesehen = std::collections::BTreeSet::new();
    for o in d.inhalte().into_iter().flat_map(|t| &t.auswahl).map(|a| a.ort.as_str()) {
        if !gesehen.insert(o) {
            let quelle = text.strip_prefix('\u{feff}').unwrap_or(text);
            return Err(Mangel::beim_zweiten(
                "ort",
                o,
                format!("Den Ort „{o}“ gibt es in der Auswahl der Gebetszeiten zweimal. Jeder Ort nur einmal, auch über Teile hinweg."),
            )
            .in_datei(datei, quelle));
        }
    }
    Ok(())
}

impl Datei<Gebete> {
    /// Alle Einträge in der Reihenfolge der Datei.
    pub fn alle(&self) -> Vec<&GebeteAuswahl> {
        self.inhalte().into_iter().flat_map(|t| &t.auswahl).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAME: &str = "projects/kalender/daten/gebete.toml";

    fn lesen(text: &str) -> Result<Datei<Gebete>, LeseFehler> {
        let d = super::super::daten::datei_lesen::<Gebete>(NAME, text).map_err(ohne_werte)?;
        gebete_eindeutig(&d, NAME, text)?;
        Ok(d)
    }

    const PROBE: &str = "[[auswahl]]\nort = \"zuhause\"\nvarianten = [\"spuerhund-gebet\", \"fuenf-mwl\"]\n";

    #[test]
    fn lesen_und_pruefen() {
        let d = lesen(PROBE).unwrap();
        assert_eq!(d.alle()[0].varianten, ["spuerhund-gebet", "fuenf-mwl"]);
        // Nach außen nichts von der Auswahl.
        let json = serde_json::to_string(&d).unwrap();
        assert!(!json.contains("spuerhund") && !json.contains("zuhause") && !json.contains("fuenf"), "{json}");
        assert!(lesen("").unwrap().alle().is_empty(), "eine leere Datei ist eine leere Auswahl");
        // Fehler nennen höchstens den Ort, nie die Variante.
        for (alt, neu, wort) in [
            ("\"spuerhund-gebet\", ", "\"spuerhund-unbekannt\", ", "nicht kennt"),
            ("[\"spuerhund-gebet\", \"fuenf-mwl\"]", "[]", "keine Variante"),
            ("ort = \"zuhause\"", "ort = \"Zu Hause spuerhund\"", "keine einfache"),
            ("varianten = [", "varianten = 3\nx = [", "Zeile passt nicht"),
            ("varianten = [", "variante = [", "Zeile passt nicht"),
            ("]\n", "\n", "Zeile passt nicht"),
        ] {
            let e = lesen(&PROBE.replace(alt, neu)).unwrap_err();
            assert!(e.text.contains(wort), "{neu}: {}", e.text);
            assert!(!e.text.contains("spuerhund") && !e.text.contains("fuenf"), "{neu}: {}", e.text);
        }
        let doppelt = format!("{PROBE}\n{PROBE}");
        let e = lesen(&doppelt).unwrap_err();
        assert!(e.text.contains("„zuhause“") && e.text.contains("zweimal") && !e.text.contains("spuerhund") && e.zeile == Some(6), "{e:?}");
        let teile = format!("[[teil.a.auswahl]]\n{}\n[[teil.b.auswahl]]\n{}", &PROBE[12..], &PROBE[12..]);
        assert!(lesen(&teile).unwrap_err().text.contains("zweimal"));
    }
}
