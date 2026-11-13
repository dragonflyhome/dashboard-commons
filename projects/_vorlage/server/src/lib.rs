#![forbid(unsafe_code)]
//! Vorlage für ein Backend-Modul (Etappe 4b). Hier in einfachen Worten
//! hinschreiben, was das Modul tut. Etwa so:
//!
//! - Endpunkt `GET /api/projekte/<slug>/x/etappen`: wie viele Etappen
//!   `daten/plan.toml` vorsieht und welche einen Termin (`ab`) haben.
//! - Hook `zeitplan` (jede Stunde): schreibt die Zahl der Etappen in die
//!   eigene Tabelle `etappen` (Schlüssel: Datum).
//!
//! Das Modul liest nur Inhalt (Absicht). Stand einer Arbeit rechnet der Server
//! aus dem Tracker; ein Modul pflegt keinen eigenen.
//!
//! Regeln (prüft `cargo test` im Dashboard, Wächter-Tests; sie schützen vor
//! Versehen, vor Absicht schützt die Prüfung des Merge-Requests):
//! - jede Datei beginnt mit `#![forbid(unsafe_code)]`;
//! - kein `std::fs`, `std::net`, `std::env`, `std::process`, kein
//!   `std::thread::spawn`, kein `static mut`, kein `Command`;
//! - kein `tokio::fs`, `tokio::net`, `tokio::process`, `tokio::spawn`,
//!   `spawn_blocking`; tokio nur unter [dev-dependencies] für Tests;
//! - `include_str!` nur aus dem eigenen Ordner, kein `include!`;
//! - Abhängigkeiten nur aus Cargo.toml dieser Vorlage, nie die Kiste `dashboard`.
//! - Dateien und Datenspeicher im Endpunkt nur über `k.im_hintergrund(…)`.
//! - Eingaben nie mit `unwrap`; Fehler als `Fehler` mit Satz.

use std::time::Duration;

use dashboard_projekt::axum::extract::State;
use dashboard_projekt::axum::routing::get;
use dashboard_projekt::axum::{Json, Router};
use dashboard_projekt::{Fehler, Herkunft, Hooks, Modul, ProjektKontext, TabellenDef, Vertrag, Zeitplan};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Das Modul. Der Betreuer des Dashboards trägt es beim ersten Merge-Request ins
/// Register `src/projekte/mod.rs` ein.
pub struct Vorlage;

/// Die eigene Tabelle (`projekt/<slug>/etappen` im Datenspeicher).
pub const ETAPPEN: &str = "etappen";

#[derive(Debug, Clone, Default, Deserialize)]
struct Plan {
    #[serde(default)]
    etappe: Vec<Etappe>,
}

#[derive(Debug, Clone, Deserialize)]
struct Etappe {
    titel: String,
    #[serde(default)]
    ab: Option<String>,
}

/// Antwort von `GET …/x/etappen` (Vertragstyp, geht nach TypeScript).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(rename = "VorlageEtappen")]
pub struct EtappenAntwort {
    pub etappen: u32,
    /// Titel der Etappen mit Termin, in der Reihenfolge der Datei.
    pub mit_termin: Vec<String>,
}

fn rechnen(k: &ProjektKontext) -> Result<EtappenAntwort, Fehler> {
    let p: Plan = k.daten_als("plan")?.ok_or_else(|| Fehler::nicht_gefunden("Es gibt keine daten/plan.toml."))?;
    let mit_termin = p.etappe.iter().filter(|e| e.ab.is_some()).map(|e| e.titel.clone()).collect();
    Ok(EtappenAntwort { etappen: p.etappe.len() as u32, mit_termin })
}

async fn etappen(State(k): State<ProjektKontext>) -> Result<Json<EtappenAntwort>, Fehler> {
    k.im_hintergrund(rechnen).await.map(Json)
}

fn zeitplan(k: &ProjektKontext) -> Result<(), Fehler> {
    let a = rechnen(k)?;
    let tag = k.jetzt().format("%Y-%m-%d").to_string();
    k.tabelle(ETAPPEN)?.schreiben(&tag, &a.etappen)
}

impl Modul for Vorlage {
    /// Woher das Modul stammt (zur Bauzeit; prüft das Register beim Start).
    fn herkunft(&self) -> Herkunft {
        dashboard_projekt::herkunft!()
    }

    /// Hier den eigenen Slug eintragen.
    fn slug(&self) -> &'static str {
        "vorlage"
    }

    fn router(&self) -> Router<ProjektKontext> {
        Router::new().route("/etappen", get(etappen))
    }

    /// Eine neue Fassung so: `TabellenDef::neu(…).mit_fassung(2, umbau)`, wobei
    /// `fn umbau(von: u32, wert: serde_json::Value) -> Result<Value, String>`
    /// einen Eintrag von `von` auf `von + 1` hebt. Dazu ein Migrationstest.
    fn tabellen(&self) -> Vec<TabellenDef> {
        vec![TabellenDef::neu(ETAPPEN, "Vorlage: Zahl der Etappen je Tag")]
    }

    /// Weitere Hooks: nach_lesen, nach_daten_geaendert, taeglich,
    /// bei_vorgang, nach_bau (siehe dashboard_projekt::hooks).
    fn hooks(&self) -> Hooks {
        Hooks { zeitplan: Some(Zeitplan { alle: Duration::from_secs(60 * 60), lauf: zeitplan }), ..Hooks::default() }
    }

    fn vertrag(&self, v: &mut Vertrag) {
        v.typ::<EtappenAntwort>();
    }
}

#[cfg(test)]
mod tests {
    use dashboard_projekt::axum::http::StatusCode;
    use dashboard_projekt::pruefstand::Pruefstand;

    use super::*;

    const DATEN: &str = "[[etappe]]\nkennung = \"a\"\ntitel = \"Aussaat\"\nreihenfolge = 1\n\n\
                         [[etappe]]\nkennung = \"b\"\ntitel = \"Ernte\"\nreihenfolge = 2\nab = \"2027-06\"\n";

    #[tokio::test]
    async fn endpunkt_gut() {
        let p = Pruefstand::neu(&Vorlage).datei("plan", DATEN);
        let (status, v) = p.get("/etappen").await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!((v["etappen"].as_u64(), v["mit_termin"].clone()), (Some(2), serde_json::json!(["Ernte"])));
    }

    #[tokio::test]
    async fn endpunkt_fehler() {
        let (status, v) = Pruefstand::neu(&Vorlage).get("/etappen").await;
        assert_eq!((status, v["art"].as_str()), (StatusCode::NOT_FOUND, Some("nicht-gefunden")));
    }

    /// Die Vorlage passt zu ihrem eigenen Inhalt: daten/plan.toml liegt bei.
    #[tokio::test]
    async fn vorlage_mit_eigenem_inhalt() {
        let ordner = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let (status, v) = Pruefstand::neu(&Vorlage).ordner(&ordner).get("/etappen").await;
        assert_eq!((status, v["etappen"].as_u64()), (StatusCode::OK, Some(3)), "{v}");
    }

    #[test]
    fn hook_zeitplan() {
        let p = Pruefstand::neu(&Vorlage).datei("plan", DATEN);
        p.zeitplan().unwrap();
        assert_eq!(p.kontext().tabelle(ETAPPEN).unwrap().anzahl().unwrap(), 1);
    }

    #[test]
    fn tabelle_eingerichtet() {
        let p = Pruefstand::neu(&Vorlage);
        assert!(p.befunde().is_empty(), "{:?}", p.befunde());
        assert_eq!(p.kontext().tabelle(ETAPPEN).unwrap().anzahl().unwrap(), 0);
    }

    // Nach dem Kopieren einschalten: schreibt die TypeScript-Typen nach
    // web/src/lib/vertrag/projekte/<slug>/.
    // #[test]
    // fn vertrag_aktuell() {
    //     dashboard_projekt::vertrag::vertrag_pruefen(&Vorlage);
    // }
}
