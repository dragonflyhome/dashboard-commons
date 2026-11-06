//! Export des ganzen Datenspeichers als TOML (zum Ansehen) oder JSON (zum
//! Herunterladen): je Tabelle aus `tabellen::ALLE` ein Feld mit ihrem Namen.
//! Der Export enthält nie Passwörter: Passwörter liegen nur im Windows-Tresor,
//! nie im Datenspeicher. Kommt eine Tabelle mit heiklen Werten dazu, bekommt
//! sie in `ALLE` bewusst `exportieren: None`.

use std::collections::BTreeMap;

use redb::{ReadTransaction, ReadableDatabase};
use serde::Serialize;
use tracing::warn;

use super::tabellen;
use super::{Speicher, SpeicherFehler};

#[derive(Debug, Clone, Serialize)]
pub struct Export {
    /// Zeitpunkt des Exports (Ortszeit, RFC 3339).
    pub erstellt: String,
    pub schema: u32,
    /// Je Tabelle ein Feld mit ihrem Namen (`meta`, `pc_verlauf`, …), seit
    /// Etappe 4b auch je Tabelle eines Backend-Moduls (`projekt/<slug>/<name>`,
    /// Schlüssel → Wert).
    #[serde(flatten)]
    pub tabellen: BTreeMap<String, serde_json::Value>,
}

impl Speicher {
    /// Alles aus einer Lese-Transaktion, also in sich stimmig.
    pub fn export(&self) -> Result<Export, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        aus_transaktion(&txn)
    }
}

/// Export aus einer bestehenden Lese-Transaktion (auch für die Sicherung).
pub(super) fn aus_transaktion(txn: &ReadTransaction) -> Result<Export, SpeicherFehler> {
    let mut felder = BTreeMap::new();
    for t in tabellen::ALLE {
        if let Some(exportieren) = t.exportieren {
            felder.insert(t.name.to_string(), exportieren(txn)?);
        }
    }
    for t in super::projekte::verzeichnis(txn)? {
        let voll = t.voller_name();
        felder.insert(voll.clone(), super::projekte::exportieren(txn, &voll)?);
    }
    let schema = super::meta_aus(txn, tabellen::META_SCHEMA)?.and_then(|s| s.parse().ok()).unwrap_or(0);
    Ok(Export { erstellt: chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false), schema, tabellen: felder })
}

/// TOML kennt kein `null`: leere Felder fallen dort weg (im JSON bleiben sie).
fn ohne_null(wert: &mut serde_json::Value) {
    match wert {
        serde_json::Value::Object(map) => {
            map.retain(|_, v| !v.is_null());
            map.values_mut().for_each(ohne_null);
        }
        serde_json::Value::Array(liste) => liste.iter_mut().for_each(ohne_null),
        _ => {}
    }
}

impl Export {
    pub fn als_toml(&self) -> String {
        let mut fuer_toml = self.clone();
        fuer_toml.tabellen.values_mut().for_each(ohne_null);
        toml::to_string(&fuer_toml).unwrap_or_else(|e| {
            warn!("Export als TOML fehlgeschlagen: {e}");
            "# Der Export ließ sich nicht als TOML schreiben; Einzelheiten stehen im Protokoll.\n".to_string()
        })
    }

    pub fn als_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|e| {
            warn!("Export als JSON fehlgeschlagen: {e}");
            "{}".to_string()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speicher::tabellen::{PcMesswert, zeit_schluessel};

    #[tokio::test]
    async fn export_enthaelt_meta_und_verlauf() {
        let s = Speicher::temporaer();
        let mut m = PcMesswert { zeit: zeit_schluessel(chrono::Utc::now()), cpu: Some(7.5), mem: 12.3, platte_belegt: None, platte_gesamt: None };
        s.pc_messwert_anhaengen(m.clone()).await.unwrap();
        m.zeit = zeit_schluessel(chrono::Utc::now() - chrono::Duration::minutes(5));
        m.platte_belegt = Some(10);
        m.platte_gesamt = Some(20);
        s.pc_messwert_anhaengen(m).await.unwrap();
        let e = s.export().unwrap();
        assert_eq!(e.schema, crate::speicher::migration::SCHEMA_AKTUELL);
        assert_eq!(e.tabellen["pc_verlauf"].as_array().unwrap().len(), 2);
        let toml = e.als_toml();
        assert!(toml.contains(&format!("schema = {}", e.schema)), "{toml}");
        assert!(toml.contains("[[pc_verlauf]]"), "{toml}");
        assert!(toml.contains("platte_gesamt = 20"), "{toml}");
        assert!(toml.contains("mem = 12.3\n"), "f32 ohne Rundungsrest: {toml}");
        let wieder: toml::Value = toml::from_str(&toml).unwrap();
        assert_eq!(wieder["meta"]["fassung.pc_verlauf"].as_str(), Some("1"));
        let json: serde_json::Value = serde_json::from_str(&e.als_json()).unwrap();
        assert_eq!(json["pc_verlauf"][1]["cpu"], 7.5);
        assert_eq!(json["pc_verlauf"][1]["platte_belegt"], serde_json::Value::Null);
        assert_eq!(json["pc_verlauf"][0]["platte_belegt"], 10);
    }
}
