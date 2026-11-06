//! TOML lesen und Fehler in einfachem Deutsch melden (Zielbild, Abschnitt b,
//! „Fehlermeldungen“): jede Meldung nennt Datei, Zeile, was falsch ist und
//! wie es richtig geht, etwa
//!
//! > `projects/beispiel-a/projekt.toml`, Zeile 3: Bei `status` steht „aktief“.
//! > Erlaubt sind: aktiv, pausiert, geplant, fertig, problem, archiviert.

use std::fmt;

use serde::de::DeserializeOwned;

/// Ein Fehler in einer Datei eines Projekts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeseFehler {
    /// Pfad wie `projects/<slug>/projekt.toml`.
    pub datei: String,
    /// Zeile, ab 1; `None`, wenn sie sich nicht bestimmen lässt.
    pub zeile: Option<usize>,
    /// Der Satz: was falsch ist und wie es richtig geht.
    pub text: String,
}

impl fmt::Display for LeseFehler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.zeile {
            Some(z) => write!(f, "{}, Zeile {z}: {}", self.datei, self.text),
            None => write!(f, "{}: {}", self.datei, self.text),
        }
    }
}

impl std::error::Error for LeseFehler {}

/// Ein Mangel, den die Prüfung nach dem Lesen findet (etwa ein Pfeil zu einem
/// Kasten, den es nicht gibt). `suche` hilft, die Zeile zu finden: Schlüssel
/// und Wert, etwa `("id", "ernte")`. Bei Doppelten zeigt die Zeile auf das
/// zweite Vorkommen (`zweites`), denn das ist das überzählige.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mangel {
    pub suche: Option<(String, String)>,
    pub zweites: bool,
    pub text: String,
}

impl Mangel {
    pub fn bei(schluessel: &str, wert: &str, text: impl Into<String>) -> Self {
        Self { suche: Some((schluessel.to_string(), wert.to_string())), zweites: false, text: text.into() }
    }

    /// Wie [`Mangel::bei`], aber die Zeile des zweiten Vorkommens (für „steht doppelt da“).
    pub fn beim_zweiten(schluessel: &str, wert: &str, text: impl Into<String>) -> Self {
        Self { zweites: true, ..Self::bei(schluessel, wert, text) }
    }

    /// Zum Lesefehler in `datei` mit dem Text `quelle` machen.
    pub fn in_datei(self, datei: &str, quelle: &str) -> LeseFehler {
        let zeile = self.suche.as_ref().and_then(|(k, w)| {
            let erste = zeile_mit(quelle, k, w)?;
            if !self.zweites {
                return Some(erste);
            }
            // Ab der Zeile nach dem ersten Vorkommen weitersuchen; sonst bleibt die erste.
            Some(zeile_mit_ab(quelle, k, w, erste).unwrap_or(erste))
        });
        LeseFehler { datei: datei.to_string(), zeile, text: self.text }
    }
}

/// Liest `text` (Inhalt von `datei`) als TOML in den Typ `T`.
pub fn toml_lesen<T: DeserializeOwned>(datei: &str, text: &str) -> Result<T, LeseFehler> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    toml::from_str(text).map_err(|e| aus_toml_fehler(datei, text, &e))
}

/// Macht aus einem Fehler von `toml` einen Satz mit Zeile.
pub fn aus_toml_fehler(datei: &str, text: &str, e: &toml::de::Error) -> LeseFehler {
    let span = e.span();
    let zeile = span.as_ref().map(|s| zeile_bei(text, s.start));
    // Der Ausschnitt reicht bis zur nächsten Tabellenzeile (`[…]`), damit bei
    // einem fehlenden Feld der Name des Eintrags darin steht.
    let ausschnitt = span
        .as_ref()
        .map(|s| {
            let von = s.start.min(text.len());
            let ab = s.end.clamp(von, text.len());
            let bis = text[ab..].find("\n[").map(|i| ab + i).unwrap_or(text.len());
            &text[von..bis]
        })
        .unwrap_or("");
    let zeilentext = zeile.and_then(|z| text.lines().nth(z - 1)).unwrap_or("");
    LeseFehler { datei: datei.to_string(), zeile, text: satz(e.message(), zeilentext, ausschnitt) }
}

/// Zeile (ab 1) zur Byte-Stelle `pos`.
pub fn zeile_bei(text: &str, pos: usize) -> usize {
    let pos = pos.min(text.len());
    text.as_bytes()[..pos].iter().filter(|&&b| b == b'\n').count() + 1
}

/// Erste Zeile (ab 1) der Form `schluessel = "wert"` (auch ohne Anführungszeichen).
pub fn zeile_mit(text: &str, schluessel: &str, wert: &str) -> Option<usize> {
    zeile_mit_ab(text, schluessel, wert, 0)
}

/// Wie [`zeile_mit`], aber erst nach den ersten `nach` Zeilen gesucht.
pub fn zeile_mit_ab(text: &str, schluessel: &str, wert: &str, nach: usize) -> Option<usize> {
    text.lines()
        .enumerate()
        .skip(nach)
        .find(|(_, l)| {
            let Some((k, v)) = l.split_once('=') else { return false };
            let v = v.split('#').next().unwrap_or("").trim().trim_matches(['"', '\'']);
            k.trim() == schluessel && v == wert
        })
        .map(|(i, _)| i + 1)
}

/// Erste Zeile (ab 1), die mit `schluessel = …` beginnt, gleich welcher Wert.
pub fn zeile_mit_schluessel(text: &str, schluessel: &str) -> Option<usize> {
    text.lines().position(|l| schluessel_der_zeile(l) == Some(schluessel)).map(|i| i + 1)
}

/// Erste Zeile (ab 1), in der `stueck` vorkommt.
pub fn zeile_mit_text(text: &str, stueck: &str) -> Option<usize> {
    if stueck.is_empty() {
        return None;
    }
    text.lines().position(|l| l.contains(stueck)).map(|i| i + 1)
}

/// Schlüssel am Anfang einer Zeile `schluessel = …`.
fn schluessel_der_zeile(zeile: &str) -> Option<&str> {
    let (k, _) = zeile.split_once('=')?;
    let k = k.trim();
    let einfach = !k.is_empty() && k.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    einfach.then_some(k)
}

/// Alle Wörter in Backticks nach `nach`.
fn woerter_in_backticks(s: &str) -> Vec<&str> {
    s.split('`').skip(1).step_by(2).collect()
}

/// Wie ein fehlendes Feld richtig aussieht.
fn beispiel(feld: &str) -> Option<&'static str> {
    Some(match feld {
        "schema" => "schema = 3",
        "title" => "title = \"Beispiel A\"",
        "status" => "status = \"aktiv\"",
        "name" => "name = \"Phase 1\"",
        "titel" => "titel = \"Kran geliefert\"",
        "text" => "text = \"Aussaat\"",
        "wert" => "wert = \"45 kg\"",
        "datum" => "datum = \"2025-03-17\"",
        "von" => "von = \"2026-11\"",
        "bis" => "bis = \"2027-01\"",
        "erreicht" => "erreicht = 3",
        "gesamt" => "gesamt = 5",
        "nach" => "nach = \"ernte\"",
        "id" => "id = \"schlusssatz\"",
        "art" => "art = \"wahl\"",
        "gestellt" => "gestellt = \"2026-10-25\"",
        "farbe" => "farbe = \"grün\"",
        "url" => "url = \"https://example.org\"",
        "path" => "path = \"D:/Ablage/projekt\"",
        _ => return None,
    })
}

/// Name eines Eintrags im Ausschnitt (`name`, `titel` oder `id`), für „Dem Eintrag … fehlt“.
fn eintrag_name(ausschnitt: &str) -> Option<String> {
    ausschnitt.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        matches!(k.trim(), "name" | "titel" | "title" | "id").then(|| v.trim().trim_matches(['"', '\'']).to_string()).filter(|v| !v.is_empty())
    })
}

/// Art eines Werts in einfachem Deutsch (aus den Wörtern von serde, etwa
/// „a string“, „floating point `1.5`“, „u32“). Der Artikel fällt weg, ein
/// Wert in Backticks dahinter bleibt.
fn art_deutsch(s: &str) -> String {
    let s = s.trim();
    let s = s.strip_prefix("a ").or_else(|| s.strip_prefix("an ")).unwrap_or(s).trim();
    let (art, rest) = match s.strip_prefix("floating point") {
        Some(rest) => ("float", rest.trim()),
        None => s.split_once(' ').map(|(a, r)| (a, r.trim())).unwrap_or((s, "")),
    };
    let wort = match art {
        "integer" | "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "usize" | "isize" => "eine ganze Zahl",
        "float" | "f32" | "f64" => "eine Kommazahl",
        "string" | "str" | "char" => "ein Text",
        "boolean" | "bool" => "ja/nein",
        "sequence" | "array" | "tuple" => return "eine Liste".to_string(),
        "map" | "table" | "struct" => return "eine Tabelle".to_string(),
        "datetime" | "date" => "ein Datum",
        _ => return s.to_string(),
    };
    format!("{wort} {rest}").trim().to_string()
}

/// Wert einer Zeile `schluessel = wert # Kommentar`, ohne Kommentar.
fn wert_der_zeile(zeile: &str) -> &str {
    zeile.split_once('=').map(|(_, v)| v.split('#').next().unwrap_or("").trim()).unwrap_or("")
}

/// Ein TOML-Datum ohne Anführungszeichen (`2026-10-25`, auch mit Uhrzeit)?
fn ist_nacktes_datum(wert: &str) -> bool {
    let b = wert.as_bytes();
    b.len() >= 7 && b[..4].iter().all(u8::is_ascii_digit) && b[4] == b'-' && b[5..7].iter().all(u8::is_ascii_digit)
}

/// Häufige Meldungen des TOML-Lesers als Satz; `None`, wenn keiner passt.
fn toml_satz(meldung: &str) -> Option<&'static str> {
    Some(if meldung.contains("string values must be quoted") {
        "Texte brauchen Anführungszeichen, etwa `titel = \"Phase 1\"`."
    } else if meldung.contains("expected `=`") {
        "Nach dem Feldnamen fehlt `=`, etwa `titel = \"Phase 1\"`."
    } else if meldung.contains("unclosed table") || meldung.contains("unclosed array of tables") {
        "Bei einer Tabellenzeile fehlt die schließende Klammer, etwa `[[balken]]`."
    } else if meldung.contains("invalid basic string") || meldung.contains("invalid literal string") {
        "Ein Text ist nicht abgeschlossen: das Anführungszeichen am Ende fehlt."
    } else if meldung.contains("unquoted keys cannot be empty") {
        "Vor dem `=` fehlt der Feldname."
    } else {
        return None;
    })
}

/// Der Satz zu einer Meldung von toml/serde.
fn satz(meldung: &str, zeilentext: &str, ausschnitt: &str) -> String {
    let meldung = meldung.trim();
    let schluessel = schluessel_der_zeile(zeilentext);
    let bei = |s: Option<&str>| match s {
        Some(k) => format!("Bei `{k}`"),
        None => "Hier".to_string(),
    };

    if let Some(rest) = meldung.strip_prefix("unknown variant ") {
        let (wert, erwartet) = rest.split_once(", expected").unwrap_or((rest, ""));
        let wert = wert.trim().trim_matches('`');
        let erlaubt = woerter_in_backticks(erwartet).join(", ");
        return format!("{} steht „{wert}“. Erlaubt sind: {erlaubt}.", bei(schluessel));
    }
    if let Some(rest) = meldung.strip_prefix("unknown field ") {
        let (feld, erwartet) = rest.split_once(',').unwrap_or((rest, ""));
        let feld = feld.trim().trim_matches('`');
        let erlaubt = woerter_in_backticks(erwartet);
        if erlaubt.is_empty() {
            return format!("Das Feld `{feld}` gibt es hier nicht; hier sind keine Felder erlaubt.");
        }
        return format!("Das Feld `{feld}` gibt es hier nicht. Erlaubt sind: {}.", erlaubt.join(", "));
    }
    if let Some(rest) = meldung.strip_prefix("missing field ") {
        let feld = rest.trim().trim_matches('`');
        let wem = match eintrag_name(ausschnitt) {
            Some(n) => format!("Dem Eintrag „{n}“ fehlt"),
            None => "Hier fehlt".to_string(),
        };
        return match beispiel(feld) {
            Some(b) => format!("{wem} `{feld}`. Beispiel: `{b}`"),
            None => format!("{wem} `{feld}`."),
        };
    }
    if let Some(rest) = meldung.strip_prefix("invalid type: ") {
        let (ist, soll) = rest.split_once(", expected").unwrap_or((rest, ""));
        // Ein Datum ohne Anführungszeichen ist in TOML ein eigener Typ; serde nennt es „map“.
        let wert = wert_der_zeile(zeilentext);
        if ist.trim() == "map" && ist_nacktes_datum(wert) {
            let k = schluessel.unwrap_or("datum");
            return format!("{} steht ein Datum ohne Anführungszeichen. Bitte so schreiben: `{k} = \"{wert}\"`.", bei(schluessel));
        }
        let mut s = format!("{} steht {}, erwartet ist {}.", bei(schluessel), art_deutsch(ist), art_deutsch(soll));
        if soll.contains("string")
            && (ist.starts_with("integer") || ist.starts_with("float"))
            && let Some(k) = schluessel
        {
            let zahl = ist.split('`').nth(1).unwrap_or("1");
            s.push_str(&format!(" Bitte in Anführungszeichen: `{k} = \"{zahl}\"`"));
        }
        return s;
    }
    if meldung.contains("did not match any variant of untagged enum Zeitpunkt") {
        return format!("{} passt der Wert nicht. Erlaubt ist ein Datum wie \"2026-11\" oder \"2026-11-03\" oder eine Monatszahl wie 3.", bei(schluessel));
    }
    if meldung.contains("escape") || zeilentext.contains('\\') {
        return "Ein Rückwärtsstrich `\\` in einem Text ist in TOML ein Steuerzeichen. Bitte Schrägstrich `/` nehmen (etwa \"D:/Ablage\") oder den Text in einfache Anführungszeichen setzen ('D:\\Ablage').".to_string();
    }
    if meldung.contains("duplicate key") {
        return format!("{} steht der Schlüssel doppelt. Jeder Schlüssel darf nur einmal vorkommen.", bei(schluessel));
    }
    // Die englische Einzelheit nur ins Protokoll, nicht in den Satz.
    tracing::debug!("TOML-Lesefehler: {meldung}");
    match toml_satz(meldung) {
        Some(zusatz) => format!("Die Datei ist kein gültiges TOML. {zusatz}"),
        None => "Die Datei ist kein gültiges TOML. Bitte die genannte Zeile prüfen.".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    #[allow(dead_code)]
    struct Probe {
        name: String,
        #[serde(default)]
        zahl: u32,
        #[serde(default)]
        art: Option<Art>,
        #[serde(default)]
        balken: Vec<Balken>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "lowercase")]
    enum Art {
        Eins,
        Zwei,
        Drei,
    }

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    #[allow(dead_code)]
    struct Balken {
        name: String,
        bis: String,
    }

    fn fehler(text: &str) -> LeseFehler {
        toml_lesen::<Probe>("projects/x/probe.toml", text).unwrap_err()
    }

    #[test]
    fn falsches_wort_nennt_zeile_und_erlaubtes() {
        let e = fehler("name = \"a\"\n\nart = \"vier\"\n");
        assert_eq!(e.zeile, Some(3));
        assert_eq!(e.to_string(), "projects/x/probe.toml, Zeile 3: Bei `art` steht „vier“. Erlaubt sind: eins, zwei, drei.");
    }

    #[test]
    fn unbekanntes_und_fehlendes_feld() {
        let e = fehler("name = \"a\"\nnmae = \"b\"\n");
        assert_eq!(e.zeile, Some(2));
        assert!(e.text.starts_with("Das Feld `nmae` gibt es hier nicht. Erlaubt sind: name, zahl"), "{e}");
        let e = fehler("name = \"a\"\n[[balken]]\nname = \"Phase 1\"\n");
        assert!(e.text.contains("Dem Eintrag „Phase 1“ fehlt `bis`. Beispiel: `bis = \"2027-01\"`"), "{e}");
        assert!(e.zeile.is_some(), "{e:?}");
    }

    #[test]
    fn falsche_art_und_rueckwaertsstrich() {
        let e = fehler("name = 5\n");
        assert!(e.text.starts_with("Bei `name` steht eine ganze Zahl"), "{e}");
        assert!(e.text.contains("`name = \"5\"`"), "{e}");
        let e = fehler("name = \"C:\\Users\"\n");
        assert_eq!(e.zeile, Some(1));
        assert!(e.text.contains("Schrägstrich `/`"), "{e}");
    }

    #[test]
    fn arten_ganz_auf_deutsch() {
        assert_eq!(art_deutsch("a string"), "ein Text");
        assert_eq!(art_deutsch("string \"x\""), "ein Text \"x\"");
        assert_eq!(art_deutsch("a sequence"), "eine Liste");
        assert_eq!(art_deutsch("a map"), "eine Tabelle");
        assert_eq!(art_deutsch("floating point `1.5`"), "eine Kommazahl `1.5`");
        assert_eq!(art_deutsch("integer `5`"), "eine ganze Zahl `5`");
        assert_eq!(art_deutsch("u32"), "eine ganze Zahl");
        assert_eq!(art_deutsch("a boolean"), "ja/nein");
        assert_eq!(art_deutsch("boolean `true`"), "ja/nein `true`");
        assert_eq!(art_deutsch("a datetime"), "ein Datum");
        let e = fehler("name = 1.5\n");
        assert_eq!(e.text, "Bei `name` steht eine Kommazahl `1.5`, erwartet ist ein Text. Bitte in Anführungszeichen: `name = \"1.5\"`");
        let e = fehler("name = \"a\"\nbalken = \"x\"\n");
        assert_eq!(e.text, "Bei `balken` steht ein Text \"x\", erwartet ist eine Liste.");
        let e = fehler("name = [\"a\"]\n");
        assert_eq!(e.text, "Bei `name` steht eine Liste, erwartet ist ein Text.");
        for t in [&e.text, &fehler("name = true\n").text, &fehler("name = \"a\"\nzahl = \"3\"\n").text] {
            for englisch in ["a string", "a sequence", "a map", "floating point", "integer", "boolean", "u32"] {
                assert!(!t.contains(englisch), "{englisch} in: {t}");
            }
        }
    }

    #[test]
    fn datum_ohne_anfuehrungszeichen() {
        let e = toml_lesen::<Probe>("p", "name = 2026-10-25\n").unwrap_err();
        assert_eq!(e.zeile, Some(1));
        assert_eq!(e.text, "Bei `name` steht ein Datum ohne Anführungszeichen. Bitte so schreiben: `name = \"2026-10-25\"`.");
        #[derive(Debug, Deserialize)]
        #[allow(dead_code)]
        struct Frage {
            gestellt: String,
        }
        let e = toml_lesen::<Frage>("p", "gestellt = 2026-10-25 # heute\n").unwrap_err();
        assert_eq!(e.text, "Bei `gestellt` steht ein Datum ohne Anführungszeichen. Bitte so schreiben: `gestellt = \"2026-10-25\"`.");
        // Eine echte Tabelle bleibt eine Tabelle.
        let e = fehler("name = { a = 1 }\n");
        assert_eq!(e.text, "Bei `name` steht eine Tabelle, erwartet ist ein Text.");
    }

    #[test]
    fn kaputtes_toml_ohne_englisch() {
        let e = fehler("name = abc\n");
        assert_eq!(e.text, "Die Datei ist kein gültiges TOML. Texte brauchen Anführungszeichen, etwa `titel = \"Phase 1\"`.");
        assert_eq!(e.zeile, Some(1));
        let e = fehler("name \"a\"\n");
        assert!(e.text.starts_with("Die Datei ist kein gültiges TOML. Nach dem Feldnamen fehlt `=`"), "{e}");
        let e = fehler("name = \"a\"\n[[balken\n");
        assert!(e.text.starts_with("Die Datei ist kein gültiges TOML. "), "{e}");
        assert!(!e.text.contains("expected") && !e.text.contains('('), "{e}");
        let e = fehler("name = \"\\q\"\n");
        assert!(e.text.contains("Rückwärtsstrich"), "{e}");
    }

    #[test]
    fn doppelte_zeigen_auf_das_zweite() {
        let t = "[[a]]\nid = \"x\"\n[[a]]\nid = \"y\"\n[[a]]\nid = \"x\"\n";
        assert_eq!(zeile_mit_ab(t, "id", "x", 2), Some(6));
        assert_eq!(Mangel::bei("id", "x", "t").in_datei("d", t).zeile, Some(2));
        assert_eq!(Mangel::beim_zweiten("id", "x", "t").in_datei("d", t).zeile, Some(6));
        // Gibt es kein zweites, bleibt das erste.
        assert_eq!(Mangel::beim_zweiten("id", "y", "t").in_datei("d", t).zeile, Some(4));
    }

    #[test]
    fn zeilen_finden() {
        let t = "a = 1\nid = \"ernte\" # Kommentar\n";
        assert_eq!(zeile_mit(t, "id", "ernte"), Some(2));
        assert_eq!(zeile_mit(t, "a", "1"), Some(1));
        assert_eq!(zeile_mit(t, "id", "x"), None);
        assert_eq!(zeile_mit_schluessel(t, "id"), Some(2));
        assert_eq!(zeile_mit_schluessel(t, "b"), None);
        assert_eq!(zeile_mit_text(t, "Kommentar"), Some(2));
        assert_eq!(zeile_mit_text(t, ""), None);
        assert_eq!(zeile_bei(t, 0), 1);
        assert_eq!(zeile_bei(t, 6), 2);
    }
}
