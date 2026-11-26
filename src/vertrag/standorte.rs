//! `daten/standorte.toml` (Kalender, Stufe 3): benannte Orte auf der Erde für
//! Kalender und Tageszeiten, im privaten Inhalt im Projekt `kalender`. Heimatort ist
//! der Ort mit `heimat = true`. (`daten/orte.toml` ist der Baustein zum Öffnen von
//! Dateien und Ordnern; darum heißt diese Datei anders.)
//!
//! Koordinaten sind persönlich: Sie gehen nie hinaus (nicht in JSON, nicht in
//! Fehlersätze, nicht ins Protokoll). Nach außen gibt es nur Kennung und Titel.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::daten::{Baustein, Datei, ist_kennung};
use super::lesen::{LeseFehler, Mangel};
use crate::kalender::mond::{Lage, Ort, Zeit};

/// Woher die Lage stammt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum StandortHerkunft {
    /// Von Hand eingegeben.
    Hand,
    /// Aus einem Stadtnamen (Ortsliste oder Nachschlagen durch den Chat).
    Stadt,
}

/// `daten/standorte.toml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Standorte {
    #[serde(default)]
    pub standort: Vec<Standort>,
}

/// Ein benannter Ort.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Standort {
    /// Kleinbuchstaben, Ziffern, Bindestriche; Ereignisse verweisen mit `ort` darauf.
    pub kennung: String,
    pub titel: String,
    /// Breite in Grad, Norden positiv. Geht nie hinaus.
    #[serde(skip_serializing)]
    #[ts(skip)]
    pub breite: f64,
    /// Länge in Grad, Osten positiv. Geht nie hinaus.
    #[serde(skip_serializing)]
    #[ts(skip)]
    pub laenge: f64,
    pub herkunft: StandortHerkunft,
    /// IANA-Zeitzone, etwa `Europe/Berlin`. Ohne sie gilt `versatz`, beim
    /// Heimatort ohne beides die Ortszeit von Windows. Geht nie hinaus.
    #[serde(default, skip_serializing)]
    #[ts(skip)]
    pub zeitzone: Option<String>,
    /// Fester Versatz zur Weltzeit, etwa `+08:00` (ohne Sommerzeit). Geht nie hinaus.
    #[serde(default, skip_serializing)]
    #[ts(skip)]
    pub versatz: Option<String>,
    /// Der Heimatort (höchstens einer); ohne ihn gilt der Standardort der Vorlage.
    #[serde(default)]
    pub heimat: bool,
}

/// Ein Versatz wie `+05:30` in Sekunden (−12:00 bis +14:00, Minuten 00/15/30/45).
pub fn versatz_lesen(text: &str) -> Option<i32> {
    let (vz, rest) = match text.as_bytes().first()? {
        b'+' => (1, &text[1..]),
        b'-' => (-1, &text[1..]),
        _ => return None,
    };
    let (h, m) = rest.split_once(':')?;
    if h.len() != 2 || m.len() != 2 {
        return None;
    }
    let (h, m): (i32, i32) = (h.parse().ok()?, m.parse().ok()?);
    let s = vz * (h * 3600 + m * 60);
    ((-12 * 3600..=14 * 3600).contains(&s) && [0, 15, 30, 45].contains(&m)).then_some(s)
}

impl Standort {
    /// Die Uhr des Orts; `windows`: ohne Zeitzone und Versatz die Ortszeit von
    /// Windows (Heimatort), sonst ein fester Versatz nach der Länge (Näherung).
    pub fn zeit(&self, windows: bool) -> Zeit {
        if let Some(z) = self.zeitzone.as_deref().and_then(crate::kalender::zone::zone) {
            return Zeit::Zone(z);
        }
        if let Some(s) = self.versatz.as_deref().and_then(versatz_lesen) {
            return Zeit::Fest { sekunden: s, naeherung: false };
        }
        if windows { Zeit::Windows } else { versatz_nach_laenge(self.laenge) }
    }

    /// Der Ort für die Rechnung.
    pub fn ort(&self, heimat: bool) -> Ort {
        Ort::an(Lage { breite: self.breite, laenge: self.laenge }, self.zeit(heimat))
    }
}

/// Ein fester Versatz aus der Länge (Länge/15 Stunden, auf die Stunde gerundet), als Näherung.
pub fn versatz_nach_laenge(laenge: f64) -> Zeit {
    Zeit::Fest { sekunden: ((laenge / 15.0).round() as i32).clamp(-12, 14) * 3600, naeherung: true }
}

impl Baustein for Standorte {
    const DATEI: &'static str = "standorte";

    /// Form je Ort. Die Sätze nennen die Kennung, nie die Zahlen.
    fn pruefen(&self) -> Result<(), Mangel> {
        for s in &self.standort {
            let k = &s.kennung;
            if !ist_kennung(k) || s.titel.trim().is_empty() {
                return Err(Mangel::bei(
                    "kennung",
                    k,
                    format!("Der Ort „{k}“ braucht eine einfache `kennung` (Kleinbuchstaben, Ziffern, Bindestriche) und einen `titel`."),
                ));
            }
            if !(s.breite.is_finite() && (-90.0..=90.0).contains(&s.breite) && s.laenge.is_finite() && (-180.0..=180.0).contains(&s.laenge)) {
                return Err(Mangel::bei(
                    "kennung",
                    k,
                    format!("Beim Ort „{k}“ liegt die Lage außerhalb der Erde. Erlaubt sind `breite` von −90 bis 90 und `laenge` von −180 bis 180 Grad."),
                ));
            }
            if s.zeitzone.as_deref().is_some_and(|z| crate::kalender::zone::zone(z).is_none()) {
                return Err(Mangel::bei(
                    "kennung",
                    k,
                    format!("Beim Ort „{k}“ kennt der Server die `zeitzone` nicht. Gemeint ist ein Name der IANA-Liste, etwa `zeitzone = \"Europe/Berlin\"`."),
                ));
            }
            if s.versatz.as_deref().is_some_and(|v| versatz_lesen(v).is_none()) {
                return Err(Mangel::bei(
                    "kennung",
                    k,
                    format!("Beim Ort „{k}“ passt `versatz` nicht. Erlaubt ist ±HH:MM von −12:00 bis +14:00, etwa `versatz = \"+08:00\"`."),
                ));
            }
        }
        Ok(())
    }

    /// Auf 0,1° runden (etwa 10 km): Feiner wird ein Ort nicht gespeichert und nicht gerechnet.
    fn ergaenzen(&mut self) {
        for s in &mut self.standort {
            (s.breite, s.laenge) = (runden(s.breite), runden(s.laenge));
        }
    }
}

/// Auf 0,1° gerundet.
pub fn runden(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// Anfänge der eigenen Sätze aus [`Standorte::pruefen`]; sie nennen nur Kennungen.
const EIGENE_SAETZE: [&str; 3] = ["Der Ort „", "Beim Ort „", "Den Ort „"];

/// Ein Lesefehler ohne Werte: Sätze von TOML zitieren die Zeile (eine Koordinate)
/// und werden durch einen allgemeinen Satz mit der Zeilennummer ersetzt.
pub fn ohne_werte(e: LeseFehler) -> LeseFehler {
    if EIGENE_SAETZE.iter().any(|a| e.text.starts_with(a)) {
        return e;
    }
    let text = "Diese Zeile passt nicht zur Form der Orte. Je Ort: `kennung`, `titel`, `breite` und `laenge` als Zahlen in Grad, `herkunft = \"hand\"` oder `\"stadt\"`, wahlweise `zeitzone` und `versatz`. (Der Satz zitiert die Zeile nicht, weil Orte persönlich sind.)".to_string();
    LeseFehler { text, ..e }
}

/// Jede `kennung` nur einmal und höchstens ein Heimatort, auch über Teile hinweg.
pub fn standorte_eindeutig(d: &Datei<Standorte>, datei: &str, text: &str) -> Result<(), LeseFehler> {
    let quelle = text.strip_prefix('\u{feff}').unwrap_or(text);
    if let Some(s) = d.alle().into_iter().filter(|s| s.heimat).nth(1) {
        return Err(Mangel::bei("kennung", &s.kennung, format!("Beim Ort „{}“ steht ein zweites `heimat = true`. Heimatort ist höchstens einer.", s.kennung))
            .in_datei(datei, quelle));
    }
    let mut gesehen = std::collections::BTreeSet::new();
    for k in d.inhalte().into_iter().flat_map(|t| &t.standort).map(|s| s.kennung.as_str()) {
        if !gesehen.insert(k) {
            let quelle = text.strip_prefix('\u{feff}').unwrap_or(text);
            return Err(Mangel::beim_zweiten("kennung", k, format!("Den Ort „{k}“ gibt es zweimal. Jede `kennung` nur einmal, auch über Teile hinweg."))
                .in_datei(datei, quelle));
        }
    }
    Ok(())
}

impl Datei<Standorte> {
    /// Alle Orte in der Reihenfolge der Datei.
    pub fn alle(&self) -> Vec<&Standort> {
        self.inhalte().into_iter().flat_map(|t| &t.standort).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lesen(text: &str) -> Result<Datei<Standorte>, LeseFehler> {
        let d = super::super::daten::datei_lesen::<Standorte>("projects/kalender/daten/standorte.toml", text).map_err(ohne_werte)?;
        standorte_eindeutig(&d, "projects/kalender/daten/standorte.toml", text)?;
        Ok(d)
    }

    const LONDON: &str =
        "[[standort]]\nkennung = \"london\"\ntitel = \"London\"\nbreite = 51.5\nlaenge = -0.13\nherkunft = \"stadt\"\nzeitzone = \"Europe/London\"\n";

    #[test]
    fn lesen_und_pruefen() {
        let d = lesen(LONDON).unwrap();
        let s = d.alle()[0];
        assert!(matches!(s.zeit(false), Zeit::Zone(_)));
        assert_eq!((s.breite, s.laenge), (51.5, -0.1), "auf 0,1° gerundet");
        assert_eq!(s.ort(true).halbkugel, crate::kalender::mond::Halbkugel::Nord);
        // JSON ohne Koordinaten.
        let json = serde_json::to_string(&d).unwrap();
        assert!(!json.contains("51.5") && !json.contains("breite") && !json.contains("0.1") && !json.contains("Europe/London"), "{json}");
        let zwei = format!("{LONDON}heimat = true\n\n{}heimat = true\n", LONDON.replace("london", "ort-2"));
        assert!(lesen(&zwei).unwrap_err().text.contains("zweites `heimat = true`"));
        // Fehler nennen die Kennung, nie die Zahlen.
        for (falsch, teil) in [
            ("breite = 51.5", "breite = 91.5"),
            ("zeitzone = \"Europe/London\"", "zeitzone = \"Mond/Mitte\""),
            ("zeitzone = \"Europe/London\"", "versatz = \"+5:30\""),
        ] {
            let e = lesen(&LONDON.replace(falsch, teil)).unwrap_err();
            assert!(e.text.contains("„london“") && !e.text.contains("91") && !e.text.contains("0.13"), "{}", e.text);
        }
        // Fehler von TOML selbst (falscher Typ, unbekanntes Feld) zitieren die Zeile nicht.
        for (alt, neu) in [("breite = 51.5", "breite = \"51.5\""), ("laenge = -0.13", "laenge = -0.13\nhoehe = 51.5")] {
            let e = lesen(&LONDON.replace(alt, neu)).unwrap_err();
            assert!(!e.text.contains("51") && e.zeile.is_some(), "{e:?}");
        }
        let doppelt = format!("{LONDON}\n{LONDON}");
        assert!(lesen(&doppelt).unwrap_err().text.contains("zweimal"));
    }

    #[test]
    fn versatz() {
        assert_eq!(versatz_lesen("+05:30"), Some(19800));
        assert_eq!(versatz_lesen("-03:00"), Some(-10800));
        for falsch in ["05:30", "+5:30", "+15:00", "+05:20", "", "+aa:bb"] {
            assert_eq!(versatz_lesen(falsch), None, "{falsch}");
        }
        assert!(matches!(versatz_nach_laenge(88.4), Zeit::Fest { sekunden: 21600, naeherung: true }));
    }
}
