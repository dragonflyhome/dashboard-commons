//! Tabellen eines Moduls im Datenspeicher (Namensraum `projekt/<slug>/<name>`).
//!
//! Ein Modul nennt seine Tabellen in `Modul::tabellen()`. Nur diese gibt es;
//! der Dienst legt sie beim Start an. Jede Tabelle hat eine Fassung. Steigt
//! sie, baut der Dienst beim nächsten Start jeden Eintrag mit `umbau` um
//! (vorher eine Sicherung). Werte sind JSON; Schlüssel sind Text.

use serde_json::Value;

/// Höchstlänge eines Tabellennamens und eines Schlüssels (Zeichen).
pub const NAME_HOECHSTENS: usize = 40;
pub const SCHLUESSEL_HOECHSTENS: usize = 200;
/// Höchstgröße eines Werts (JSON, Bytes).
pub const WERT_HOECHSTENS: usize = 64 * 1024;

/// Ein Umbauschritt: hebt einen Eintrag von Fassung `von` auf `von + 1`.
/// Ein `Err` mit Satz bricht den Umbau ab; die Tabelle bleibt dann, wie sie war.
pub type Umbau = fn(von: u32, wert: Value) -> Result<Value, String>;

/// Eine Tabelle eines Moduls.
#[derive(Debug, Clone, Copy)]
pub struct TabellenDef {
    /// Kurzer Name: a–z, 0–9, Bindestrich, Unterstrich; höchstens 40 Zeichen.
    pub name: &'static str,
    /// Für die Seite /daten, in einfachem Deutsch.
    pub beschreibung: &'static str,
    /// Fassung der Einträge; beginnt bei 1.
    pub fassung: u32,
    /// Umbau von älteren Fassungen (siehe [`Umbau`]).
    pub umbau: Umbau,
}

/// Für Tabellen in Fassung 1: es gibt nichts umzubauen.
pub fn kein_umbau(von: u32, _wert: Value) -> Result<Value, String> {
    Err(format!("Für Fassung {von} gibt es keinen Umbauschritt."))
}

impl TabellenDef {
    /// Eine Tabelle in Fassung 1.
    pub const fn neu(name: &'static str, beschreibung: &'static str) -> Self {
        Self { name, beschreibung, fassung: 1, umbau: kein_umbau }
    }

    /// Dieselbe Tabelle in einer höheren Fassung, mit Umbauschritt.
    pub const fn mit_fassung(self, fassung: u32, umbau: Umbau) -> Self {
        Self { fassung, umbau, ..self }
    }

    /// Ganzer Name in der Datei: `projekt/<slug>/<name>`.
    pub fn voller_name(&self, slug: &str) -> String {
        voller_name(slug, self.name)
    }
}

/// `projekt/<slug>/<name>`.
pub fn voller_name(slug: &str, name: &str) -> String {
    format!("projekt/{slug}/{name}")
}

/// Ist das ein erlaubter Name (Tabelle) bzw. Slug?
pub fn ist_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= NAME_HOECHSTENS && name.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'_')
}

/// Ist das ein erlaubter Schlüssel (nicht leer, keine Steuerzeichen)?
pub fn ist_schluessel(schluessel: &str) -> bool {
    !schluessel.is_empty() && schluessel.chars().count() <= SCHLUESSEL_HOECHSTENS && !schluessel.chars().any(char::is_control)
}

/// Prüft die Tabellen eines Moduls: Namen gültig und nicht doppelt, Fassung ab 1.
pub fn pruefen(defs: &[TabellenDef]) -> Result<(), String> {
    for (i, d) in defs.iter().enumerate() {
        if !ist_name(d.name) {
            return Err(format!("Der Tabellenname „{}“ ist ungültig. Erlaubt sind 1 bis {NAME_HOECHSTENS} Zeichen aus a–z, 0–9, - und _.", d.name));
        }
        if d.fassung == 0 {
            return Err(format!("Die Tabelle „{}“ hat Fassung 0; Fassungen beginnen bei 1.", d.name));
        }
        if defs[..i].iter().any(|e| e.name == d.name) {
            return Err(format!("Die Tabelle „{}“ steht doppelt in tabellen().", d.name));
        }
    }
    Ok(())
}

/// Baut einen Eintrag Schritt für Schritt von Fassung `alt` auf `def.fassung` um.
pub fn eintrag_umbauen(def: &TabellenDef, alt: u32, mut wert: Value) -> Result<Value, String> {
    for von in alt..def.fassung {
        // Ein Absturz im Umbauschritt darf den Start des Dienstes nicht aufhalten.
        let schritt = std::panic::catch_unwind(|| (def.umbau)(von, wert)).unwrap_or_else(|e| Err(format!("abgestürzt: {}", crate::fehler::absturz_grund(&*e))));
        wert = schritt.map_err(|e| format!("Umbau von Fassung {von} auf {}: {e}", von + 1))?;
    }
    Ok(wert)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn umbau(von: u32, mut wert: Value) -> Result<Value, String> {
        match von {
            1 => {
                wert["zwei"] = Value::Bool(true);
                Ok(wert)
            }
            2 => {
                wert["drei"] = Value::Bool(true);
                Ok(wert)
            }
            _ => Err("unbekannt".into()),
        }
    }

    #[test]
    fn namen_und_umbau() {
        assert!(ist_name("verlauf") && ist_name("a_1-b") && !ist_name("Gross") && !ist_name("") && !ist_name("a/b"));
        assert!(ist_schluessel("2026-10-30") && !ist_schluessel("") && !ist_schluessel("a\nb"));
        let d = TabellenDef::neu("t", "Test").mit_fassung(3, umbau);
        assert_eq!(d.voller_name("bauplan"), "projekt/bauplan/t");
        let neu = eintrag_umbauen(&d, 1, serde_json::json!({})).unwrap();
        assert_eq!(neu, serde_json::json!({ "zwei": true, "drei": true }));
        assert_eq!(eintrag_umbauen(&d, 3, serde_json::json!(1)).unwrap(), serde_json::json!(1));
        let e = eintrag_umbauen(&TabellenDef::neu("t", "x").mit_fassung(2, kein_umbau), 1, Value::Null).unwrap_err();
        assert!(e.contains("Fassung 1 auf 2"), "{e}");
        assert!(pruefen(&[d, d]).unwrap_err().contains("doppelt"));
        assert!(pruefen(&[TabellenDef::neu("A", "x")]).is_err());
        assert!(pruefen(&[TabellenDef { fassung: 0, ..d }]).is_err());
    }
}
