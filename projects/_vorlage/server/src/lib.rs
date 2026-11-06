#![forbid(unsafe_code)]
//! Vorlage für ein Backend-Modul (Etappe 4b). Hier in einfachen Worten
//! hinschreiben, was das Modul tut. Etwa so:
//!
//! - Endpunkt `GET /api/projekte/<slug>/x/fortschritt`: Durchschnitt der
//!   Zeilen aus `daten/fortschritt.toml`.
//! - Hook `zeitplan` (jede Stunde): schreibt den Durchschnitt in die eigene
//!   Tabelle `stand` (Schlüssel: Datum).
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

/// Die eigene Tabelle (`projekt/<slug>/stand` im Datenspeicher).
pub const STAND: &str = "stand";

#[derive(Debug, Clone, Default, Deserialize)]
struct Fortschritt {
    #[serde(default)]
    zeile: Vec<Zeile>,
}

#[derive(Debug, Clone, Deserialize)]
struct Zeile {
    #[serde(default)]
    prozent: Option<f64>,
    #[serde(default)]
    erreicht: Option<f64>,
    #[serde(default)]
    gesamt: Option<f64>,
}

impl Zeile {
    fn prozent(&self) -> f64 {
        match (self.prozent, self.erreicht, self.gesamt) {
            (Some(p), _, _) => p,
            (None, Some(e), Some(g)) if g > 0.0 => e / g * 100.0,
            _ => 0.0,
        }
    }
}

/// Antwort von `GET …/x/fortschritt` (Vertragstyp, geht nach TypeScript).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(rename = "VorlageFortschritt")]
pub struct FortschrittAntwort {
    pub zeilen: u32,
    /// Durchschnitt in Prozent, auf eine Stelle gerundet.
    pub durchschnitt: f64,
}

fn rechnen(k: &ProjektKontext) -> Result<FortschrittAntwort, Fehler> {
    let f: Fortschritt = k.daten_als("fortschritt")?.ok_or_else(|| Fehler::nicht_gefunden("Es gibt keine daten/fortschritt.toml."))?;
    let summe: f64 = f.zeile.iter().map(Zeile::prozent).sum();
    let durchschnitt = if f.zeile.is_empty() { 0.0 } else { (summe / f.zeile.len() as f64 * 10.0).round() / 10.0 };
    Ok(FortschrittAntwort { zeilen: f.zeile.len() as u32, durchschnitt })
}

async fn fortschritt(State(k): State<ProjektKontext>) -> Result<Json<FortschrittAntwort>, Fehler> {
    k.im_hintergrund(rechnen).await.map(Json)
}

fn zeitplan(k: &ProjektKontext) -> Result<(), Fehler> {
    let a = rechnen(k)?;
    let tag = k.jetzt().format("%Y-%m-%d").to_string();
    k.tabelle(STAND)?.schreiben(&tag, &a.durchschnitt)
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
        Router::new().route("/fortschritt", get(fortschritt))
    }

    /// Eine neue Fassung so: `TabellenDef::neu(…).mit_fassung(2, umbau)`, wobei
    /// `fn umbau(von: u32, wert: serde_json::Value) -> Result<Value, String>`
    /// einen Eintrag von `von` auf `von + 1` hebt. Dazu ein Migrationstest.
    fn tabellen(&self) -> Vec<TabellenDef> {
        vec![TabellenDef::neu(STAND, "Vorlage: Durchschnitt je Tag")]
    }

    /// Weitere Hooks: nach_lesen, nach_daten_geaendert, taeglich,
    /// bei_vorgang, nach_bau (siehe dashboard_projekt::hooks).
    fn hooks(&self) -> Hooks {
        Hooks { zeitplan: Some(Zeitplan { alle: Duration::from_secs(60 * 60), lauf: zeitplan }), ..Hooks::default() }
    }

    fn vertrag(&self, v: &mut Vertrag) {
        v.typ::<FortschrittAntwort>();
    }
}

#[cfg(test)]
mod tests {
    use dashboard_projekt::axum::http::StatusCode;
    use dashboard_projekt::pruefstand::Pruefstand;

    use super::*;

    const DATEN: &str = "[[zeile]]\ntitel = \"A\"\nprozent = 40\n\n[[zeile]]\ntitel = \"B\"\nerreicht = 3\ngesamt = 5\n";

    #[tokio::test]
    async fn endpunkt_gut() {
        let p = Pruefstand::neu(&Vorlage).datei("fortschritt", DATEN);
        let (status, v) = p.get("/fortschritt").await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!((v["zeilen"].as_u64(), v["durchschnitt"].as_f64()), (Some(2), Some(50.0)));
    }

    #[tokio::test]
    async fn endpunkt_fehler() {
        let (status, v) = Pruefstand::neu(&Vorlage).get("/fortschritt").await;
        assert_eq!((status, v["art"].as_str()), (StatusCode::NOT_FOUND, Some("nicht-gefunden")));
    }

    #[test]
    fn hook_zeitplan() {
        let p = Pruefstand::neu(&Vorlage).datei("fortschritt", DATEN);
        p.zeitplan().unwrap();
        assert_eq!(p.kontext().tabelle(STAND).unwrap().anzahl().unwrap(), 1);
    }

    #[test]
    fn tabelle_eingerichtet() {
        let p = Pruefstand::neu(&Vorlage);
        assert!(p.befunde().is_empty(), "{:?}", p.befunde());
        assert_eq!(p.kontext().tabelle(STAND).unwrap().anzahl().unwrap(), 0);
    }

    // Nach dem Kopieren einschalten: schreibt die TypeScript-Typen nach
    // web/src/lib/vertrag/projekte/<slug>/.
    // #[test]
    // fn vertrag_aktuell() {
    //     dashboard_projekt::vertrag::vertrag_pruefen(&Vorlage);
    // }
}
