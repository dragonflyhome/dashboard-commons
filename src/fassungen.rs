//! Die Fassungen des Dienstes aus seinem Changelog (Entscheidung „Freigabe in
//! einem Schritt“, `GET /api/fassungen`, dazu `version_datum` und
//! `unveroeffentlicht` in `/api/betrieb`).
//!
//! `CHANGELOG.txt` wird beim Bau eingebettet (`include_str!`): Das laufende
//! Programm trägt genau das Changelog des Stands, aus dem es gebaut ist, so
//! wie Cargo.toml seine Version. Der Server liest dafür weder Git noch eine
//! Datei neben dem Programm, die zu einer anderen Fassung gehören könnte.
//!
//! Form (Keep a Changelog in Klartext): Eine Überschrift ist eine Zeile, unter
//! der nur Striche stehen (mindestens drei). Die erste heißt `Unreleased`,
//! jede weitere `x.y.z - JJJJ-MM-TT`. Darunter die Unterabschnitte
//! Hinzugefügt, Geändert, Entfernt und Behoben, je mit Einträgen „- …“;
//! eingerückte Zeilen setzen einen Eintrag fort. Ein BOM am Anfang und
//! CRLF stören nicht. Was sich nicht lesen lässt (andere Überschrift,
//! unbekannter Unterabschnitt, lose Zeile), wird übersprungen und steht als
//! Satz in `hinweise`; der Leser bricht nie ab.

use std::sync::LazyLock;

use crate::vertrag::api::{FassungenAntwort, FassungsEintrag, FassungsZeilen};

/// Das Changelog dieses Baus.
pub const CHANGELOG: &str = include_str!("../CHANGELOG.txt");

/// Die Version dieses Baus.
pub const LAUFEND: &str = env!("CARGO_PKG_VERSION");

/// Das eingebettete Changelog, einmal gelesen.
static GELESEN: LazyLock<FassungenAntwort> = LazyLock::new(|| lesen(CHANGELOG, LAUFEND));

/// Das eingebettete Changelog mit allen Fassungen.
pub fn alle() -> &'static FassungenAntwort {
    &GELESEN
}

/// Für `/api/betrieb`: das Datum der laufenden Fassung (`null`, wenn das
/// Changelog sie nicht nennt) und ob Unreleased Einträge hat.
pub fn betrieb_angaben() -> (Option<String>, bool) {
    let a = alle();
    let datum = a.fassungen.iter().find(|f| f.version == a.laufend).map(|f| f.datum.clone());
    (datum, !a.unveroeffentlicht.leer())
}

/// `x.y.z` als Zahlen, sonst `None`.
pub fn version_zahlen(v: &str) -> Option<[u64; 3]> {
    let mut teile = v.split('.');
    let mut z = [0u64; 3];
    for stelle in &mut z {
        let t = teile.next()?;
        if t.is_empty() || !t.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        *stelle = t.parse().ok()?;
    }
    teile.next().is_none().then_some(z)
}

/// `x.y.z - JJJJ-MM-TT` → (Version, Datum).
fn fassungs_titel(t: &str) -> Option<(&str, &str)> {
    let (v, d) = t.split_once(" - ")?;
    let datum = d.len() == 10 && chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok();
    (datum && version_zahlen(v).is_some()).then_some((v, d))
}

fn ist_strichzeile(z: &str) -> bool {
    let z = z.trim_end();
    z.len() >= 3 && z.bytes().all(|b| b == b'-')
}

/// Wohin die Zeilen gerade gehen.
enum Ziel {
    /// Vor der ersten Überschrift (Kopf der Datei) oder unter einer unlesbaren.
    Nichts,
    Unreleased,
    Fassung(usize),
}

/// Liest ein Changelog; `laufend` ist die Version des Dienstes.
pub fn lesen(text: &str, laufend: &str) -> FassungenAntwort {
    let zeilen: Vec<&str> = text.strip_prefix('\u{feff}').unwrap_or(text).lines().collect();
    let mut a = FassungenAntwort { laufend: laufend.to_string(), unveroeffentlicht: FassungsZeilen::default(), fassungen: Vec::new(), hinweise: Vec::new() };
    let mut ziel = Ziel::Nichts;
    let mut unreleased_gesehen = false;
    let mut unter: Option<&str> = None;
    // Der offene Eintrag: (Unterabschnitt, Text).
    let mut offen: Option<(&str, String)> = None;
    let mut i = 0;
    while i < zeilen.len() {
        let z = zeilen[i].trim_end();
        let nr = i + 1;
        // Überschrift: nicht leer, nicht eingerückt, darunter nur Striche.
        if !z.is_empty() && !z.starts_with([' ', '\t']) && zeilen.get(i + 1).is_some_and(|n| ist_strichzeile(n)) {
            abschliessen(&mut a, &ziel, offen.take());
            unter = None;
            i += 2;
            if z == "Unreleased" {
                if unreleased_gesehen || !a.fassungen.is_empty() {
                    a.hinweise.push(format!("Zeile {nr}: Unreleased steht nicht als erster Abschnitt oder doppelt; diese Zeilen fehlen hier."));
                    ziel = Ziel::Nichts;
                } else {
                    ziel = Ziel::Unreleased;
                }
                unreleased_gesehen = true;
                continue;
            }
            match fassungs_titel(z) {
                Some((v, d)) => {
                    a.fassungen.push(FassungsEintrag { version: v.to_string(), tag: format!("v{v}"), datum: d.to_string(), zeilen: FassungsZeilen::default() });
                    ziel = Ziel::Fassung(a.fassungen.len() - 1);
                }
                None => {
                    a.hinweise.push(format!("Zeile {nr}: Die Überschrift „{z}“ hat nicht die Form x.y.z - JJJJ-MM-TT; ihre Einträge fehlen hier."));
                    ziel = Ziel::Nichts;
                }
            }
            continue;
        }
        i += 1;
        if matches!(ziel, Ziel::Nichts) {
            continue;
        }
        if z.trim().is_empty() {
            continue;
        }
        if z.starts_with([' ', '\t']) {
            match &mut offen {
                Some((_, text)) => {
                    text.push(' ');
                    text.push_str(z.trim());
                }
                None => a.hinweise.push(format!("Zeile {nr}: eingerückte Zeile ohne Eintrag davor.")),
            }
            continue;
        }
        if let Some(rest) = z.strip_prefix("- ") {
            abschliessen(&mut a, &ziel, offen.take());
            match unter {
                Some(u) => offen = Some((u, rest.trim().to_string())),
                None => a.hinweise.push(format!("Zeile {nr}: Eintrag ohne Unterabschnitt (Hinzugefügt, Geändert, Entfernt, Behoben).")),
            }
            continue;
        }
        abschliessen(&mut a, &ziel, offen.take());
        match z {
            "Hinzugefügt" | "Geändert" | "Entfernt" | "Behoben" => unter = Some(z),
            _ => {
                a.hinweise.push(format!(
                    "Zeile {nr}: „{z}“ ist kein Unterabschnitt (Hinzugefügt, Geändert, Entfernt, Behoben) und kein Eintrag; was darunter steht, fehlt hier."
                ));
                unter = None;
            }
        }
    }
    abschliessen(&mut a, &ziel, offen.take());
    if !unreleased_gesehen {
        a.hinweise.push("Der Abschnitt Unreleased fehlt.".to_string());
    }
    if !a.fassungen.iter().any(|f| f.version == laufend) {
        a.hinweise.push(format!("Die laufende Version {laufend} hat keinen Abschnitt im Changelog."));
    }
    a
}

/// Legt einen fertigen Eintrag in seinen Unterabschnitt.
fn abschliessen(a: &mut FassungenAntwort, ziel: &Ziel, eintrag: Option<(&str, String)>) {
    let Some((unter, text)) = eintrag else { return };
    let zeilen = match ziel {
        Ziel::Nichts => return,
        Ziel::Unreleased => &mut a.unveroeffentlicht,
        Ziel::Fassung(n) => &mut a.fassungen[*n].zeilen,
    };
    if matches!(unter, "Geändert" | "Entfernt") && text.starts_with("Bruch: ") {
        zeilen.brueche.push(text.clone());
    }
    match unter {
        "Hinzugefügt" => zeilen.hinzugefuegt.push(text),
        "Geändert" => zeilen.geaendert.push(text),
        "Entfernt" => zeilen.entfernt.push(text),
        _ => zeilen.behoben.push(text),
    }
}

/// Nur die Fassungen nach `seit` (`x.y.z`), sonst ein Satz.
pub fn seit(a: &FassungenAntwort, seit: &str) -> Result<FassungenAntwort, String> {
    let grenze = version_zahlen(seit).ok_or_else(|| format!("„seit“ muss eine Version der Form x.y.z sein, nicht „{seit}“."))?;
    let mut aus = a.clone();
    aus.fassungen.retain(|f| version_zahlen(&f.version).is_some_and(|v| v > grenze));
    Ok(aus)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KOPF: &str = "CHANGELOG\n=========\n\nKopf mit - Strich.\n\n";

    fn beispiel() -> String {
        format!(
            "{KOPF}Unreleased\n----------\n\nHinzugefügt\n- Neu A,\n  weiter.\n\n0.2.0 - 2026-11-13\n------------------\n\nGeändert\n- Bruch: Alte Form weg.\n- Anders.\n\nEntfernt\n- Bruch: Schalter -x.\n\nBehoben\n- Fehler B.\n\n0.1.0 - 2026-11-06\n------------------\n\nHinzugefügt\n- Erste.\n"
        )
    }

    #[test]
    fn liest_unreleased_fassungen_und_brueche() {
        let a = lesen(&beispiel(), "0.2.0");
        assert_eq!(a.hinweise, Vec::<String>::new());
        assert_eq!(a.laufend, "0.2.0");
        assert_eq!(a.unveroeffentlicht.hinzugefuegt, ["Neu A, weiter."]);
        assert!(!a.unveroeffentlicht.leer());
        let v: Vec<_> = a.fassungen.iter().map(|f| (f.version.as_str(), f.tag.as_str(), f.datum.as_str())).collect();
        assert_eq!(v, [("0.2.0", "v0.2.0", "2026-11-13"), ("0.1.0", "v0.1.0", "2026-11-06")]);
        let z = &a.fassungen[0].zeilen;
        assert_eq!(z.geaendert, ["Bruch: Alte Form weg.", "Anders."]);
        assert_eq!(z.entfernt, ["Bruch: Schalter -x."]);
        assert_eq!(z.behoben, ["Fehler B."]);
        assert_eq!(z.brueche, ["Bruch: Alte Form weg.", "Bruch: Schalter -x."]);
        assert_eq!(a.fassungen[1].zeilen.hinzugefuegt, ["Erste."]);
    }

    /// BOM am Anfang, CRLF und LF ergeben dasselbe.
    #[test]
    fn bom_crlf_und_lf_gleich() {
        let lf = beispiel();
        let soll = lesen(&lf, "0.2.0");
        let crlf = lf.replace('\n', "\r\n");
        for text in [format!("\u{feff}{lf}"), crlf.clone(), format!("\u{feff}{crlf}")] {
            assert_eq!(lesen(&text, "0.2.0"), soll, "{text:?}");
        }
        // Vor dem Kopf fiele ein BOM nicht auf; direkt vor Unreleased darf er die Überschrift nicht verstecken.
        let ohne_kopf = "\u{feff}Unreleased\r\n----------\r\n\r\n0.2.0 - 2026-11-13\r\n------------------\r\n";
        let a = lesen(ohne_kopf, "0.2.0");
        assert_eq!((a.hinweise.len(), a.fassungen.len()), (0, 1), "{:?}", a.hinweise);
    }

    #[test]
    fn leeres_unreleased_und_fassung_ohne_eintraege() {
        let text = format!(
            "{KOPF}Unreleased\n----------\n\n0.2.0 - 2026-11-13\n------------------\n\n0.1.0 - 2026-11-06\n------------------\n\nHinzugefügt\n- Erste.\n"
        );
        let a = lesen(&text, "0.2.0");
        assert_eq!(a.hinweise, Vec::<String>::new());
        assert!(a.unveroeffentlicht.leer());
        assert_eq!(a.unveroeffentlicht, FassungsZeilen::default());
        assert_eq!(a.fassungen.len(), 2);
        assert_eq!(a.fassungen[0].zeilen, FassungsZeilen::default());
        assert_eq!(a.fassungen[1].zeilen.hinzugefuegt, ["Erste."]);
    }

    /// Eine kaputte Überschrift gibt einen Hinweis; die anderen Fassungen bleiben.
    #[test]
    fn kaputte_ueberschrift_gibt_hinweis() {
        let text = beispiel().replace("0.2.0 - 2026-11-13", "0.2 - 13.11.2026");
        let a = lesen(&text, "0.2.0");
        assert_eq!(a.fassungen.iter().map(|f| f.version.as_str()).collect::<Vec<_>>(), ["0.1.0"]);
        assert_eq!(a.fassungen[0].zeilen.hinzugefuegt, ["Erste."], "Zeilen der kaputten gehen nicht in die nächste");
        assert!(a.hinweise.iter().any(|h| h.contains("„0.2 - 13.11.2026“") && h.starts_with("Zeile ")), "{:?}", a.hinweise);
        assert!(a.hinweise.iter().any(|h| h.contains("0.2.0 hat keinen Abschnitt")), "{:?}", a.hinweise);
        // Ungültiges Datum, fehlendes Unreleased, unbekannter Unterabschnitt, Eintrag ohne Unterabschnitt.
        let a = lesen("0.1.0 - 2026-02-30\n---\nx\n\n0.0.1 - 2026-01-01\n---\n- lose\nSicherheit\n- s\n", "0.0.1");
        assert!(a.fassungen.len() == 1 && a.fassungen[0].zeilen.leer(), "{a:?}");
        for teil in ["„0.1.0 - 2026-02-30“", "Unreleased fehlt", "ohne Unterabschnitt", "„Sicherheit“ ist kein Unterabschnitt"] {
            assert!(a.hinweise.iter().any(|h| h.contains(teil)), "{teil}: {:?}", a.hinweise);
        }
        // Leer oder nur Müll: kein Absturz.
        for t in ["", "\u{feff}", "---\n---\n", "- x\n  y\n"] {
            let a = lesen(t, "0.1.0");
            assert!(a.fassungen.is_empty() && a.unveroeffentlicht.leer(), "{t:?}");
        }
    }

    #[test]
    fn seit_kuerzt_und_prueft() {
        let a = lesen(&beispiel(), "0.2.0");
        let nur = |s: &str| seit(&a, s).unwrap().fassungen.into_iter().map(|f| f.version).collect::<Vec<_>>();
        assert_eq!(nur("0.1.0"), ["0.2.0"]);
        assert_eq!(nur("0.0.9"), ["0.2.0", "0.1.0"]);
        assert_eq!(nur("0.2.0"), Vec::<String>::new());
        assert_eq!(nur("0.1.10"), ["0.2.0"], "Zahlen, nicht Text");
        for falsch in ["", "0.1", "v0.1.0", "0.1.0.0", "a.b.c", "0.-1.0"] {
            assert!(seit(&a, falsch).is_err(), "{falsch}");
        }
    }

    /// Das eingebettete Changelog dieses Baus liest sich ohne Hinweis und nennt die laufende Version.
    #[test]
    fn eingebettetes_changelog_ohne_hinweis() {
        let a = alle();
        assert_eq!(a.hinweise, Vec::<String>::new());
        assert_eq!(a.laufend, env!("CARGO_PKG_VERSION"));
        assert!(a.fassungen.iter().any(|f| f.version == a.laufend));
        assert!(a.fassungen.iter().all(|f| !f.zeilen.leer()), "jede Fassung hat Einträge");
        let (datum, _) = betrieb_angaben();
        assert!(datum.is_some_and(|d| d.len() == 10));
    }
}
