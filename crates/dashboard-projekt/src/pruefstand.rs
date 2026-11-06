//! Der Prüfstand für Modul-Autoren: ein Kontext ohne Dienst, mit Dateien und
//! Tabellen im Arbeitsspeicher. So läuft `cargo test` in
//! `projects/<slug>/server/` ohne den Bau des ganzen Servers.
//!
//! ```ignore
//! let p = Pruefstand::neu(&MeinModul).datei("fortschritt", "[[zeile]]\ntitel = \"A\"\nprozent = 100\n");
//! let (status, json) = p.get("/stufen").await;
//! p.zeitplan().unwrap();
//! ```
//!
//! Unterschied zum Dienst: Der Prüfstand liest TOML ohne den Vertrag des
//! Dashboards (der liegt in der Kiste `dashboard`). Ob eine Datei den Vertrag
//! erfüllt, sagt `dashboard.exe pruefe <slug>` bzw. `cargo test` im ganzen Repo.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use chrono::{DateTime, Local};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use crate::fehler::Fehler;
use crate::hooks::{Aufruf, BauErgebnis, VorgangInfo};
use crate::kontext::{Anbindung, GitStand, ProjektKontext};
use crate::tabellen::{self, TabellenDef};
use crate::{Modul, rahmen};

fn sperren<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Eine Tabelle im Arbeitsspeicher: Fassung und Einträge.
#[derive(Debug, Clone, Default)]
pub struct TestTabelle {
    pub fassung: u32,
    pub eintraege: BTreeMap<String, Vec<u8>>,
}

/// Anbindung im Arbeitsspeicher (für den Prüfstand und Tests im Dashboard).
#[derive(Default)]
pub struct TestAnbindung {
    /// `projekt` für projekt.toml, sonst der Name in `daten/` (ohne `.toml`).
    dateien: Mutex<BTreeMap<String, String>>,
    /// Ganzer Name `projekt/<slug>/<name>` → Tabelle.
    tabellen: Mutex<BTreeMap<String, TestTabelle>>,
    /// Feste Zeit; sonst die echte.
    jetzt: Mutex<Option<DateTime<Local>>>,
    git: Mutex<Vec<GitStand>>,
    /// Gemeldete Fehler: (wo, Satz).
    befunde: Mutex<Vec<(String, String)>>,
}

impl TestAnbindung {
    fn tabelle_da(&self, slug: &str, tabelle: &str, fassung: u32) -> Result<String, Fehler> {
        let voll = tabellen::voller_name(slug, tabelle);
        if sperren(&self.tabellen).get(&voll).is_some_and(|t| t.fassung == fassung) {
            Ok(voll)
        } else {
            Err(Fehler::intern(format!("Die Tabelle {voll} ist nicht eingerichtet oder hat nicht die Fassung {fassung}; sie ist gesperrt.")))
        }
    }

    /// Richtet Tabellen ein wie der Dienst: neu anlegen, ältere Fassung umbauen,
    /// zu neue ablehnen.
    pub fn einrichten(&self, slug: &str, defs: &[TabellenDef]) -> Result<(), String> {
        tabellen::pruefen(defs)?;
        let mut alle = sperren(&self.tabellen);
        for d in defs {
            let t = alle.entry(d.voller_name(slug)).or_insert_with(|| TestTabelle { fassung: d.fassung, eintraege: BTreeMap::new() });
            if t.fassung > d.fassung {
                return Err(format!("Die Tabelle {} hat Fassung {}, das Modul kennt {}.", d.voller_name(slug), t.fassung, d.fassung));
            }
            if t.fassung < d.fassung {
                let mut neu = BTreeMap::new();
                for (k, v) in &t.eintraege {
                    let wert: Value = serde_json::from_slice(v).map_err(|e| format!("{k}: {e}"))?;
                    let wert = tabellen::eintrag_umbauen(d, t.fassung, wert).map_err(|e| format!("{k}: {e}"))?;
                    neu.insert(k.clone(), serde_json::to_vec(&wert).map_err(|e| e.to_string())?);
                }
                t.eintraege = neu;
                t.fassung = d.fassung;
            }
        }
        Ok(())
    }
}

impl Anbindung for TestAnbindung {
    fn projekt_lesen(&self, slug: &str) -> Result<Value, Fehler> {
        match sperren(&self.dateien).get("projekt") {
            Some(text) => toml_als_json(&format!("projects/{slug}/projekt.toml"), text),
            None => Err(Fehler::nicht_gefunden(format!("Das Projekt „{slug}“ hat keine projekt.toml."))),
        }
    }

    fn daten_lesen(&self, slug: &str, name: &str) -> Result<Option<Value>, Fehler> {
        match sperren(&self.dateien).get(name) {
            Some(text) => toml_als_json(&format!("projects/{slug}/daten/{name}.toml"), text).map(Some),
            None => Ok(None),
        }
    }

    fn eintrag_lesen(&self, slug: &str, tabelle: &str, fassung: u32, schluessel: &str) -> Result<Option<Vec<u8>>, Fehler> {
        let voll = self.tabelle_da(slug, tabelle, fassung)?;
        Ok(sperren(&self.tabellen).get(&voll).and_then(|t| t.eintraege.get(schluessel).cloned()))
    }

    fn eintrag_schreiben(&self, slug: &str, tabelle: &str, fassung: u32, schluessel: &str, wert: &[u8]) -> Result<(), Fehler> {
        let voll = self.tabelle_da(slug, tabelle, fassung)?;
        if let Some(t) = sperren(&self.tabellen).get_mut(&voll) {
            t.eintraege.insert(schluessel.to_string(), wert.to_vec());
        }
        Ok(())
    }

    fn eintraege(&self, slug: &str, tabelle: &str, fassung: u32, hoechstens: usize, rueckwaerts: bool) -> Result<Vec<(String, Vec<u8>)>, Fehler> {
        let voll = self.tabelle_da(slug, tabelle, fassung)?;
        let alle = sperren(&self.tabellen);
        let Some(t) = alle.get(&voll) else { return Ok(Vec::new()) };
        let paare = t.eintraege.iter().map(|(k, v)| (k.clone(), v.clone()));
        Ok(if rueckwaerts { paare.rev().take(hoechstens).collect() } else { paare.take(hoechstens).collect() })
    }

    fn anzahl(&self, slug: &str, tabelle: &str, fassung: u32) -> Result<u64, Fehler> {
        let voll = self.tabelle_da(slug, tabelle, fassung)?;
        Ok(sperren(&self.tabellen).get(&voll).map_or(0, |t| t.eintraege.len() as u64))
    }

    fn git(&self, _slug: &str) -> Vec<GitStand> {
        sperren(&self.git).clone()
    }

    fn jetzt(&self) -> DateTime<Local> {
        sperren(&self.jetzt).unwrap_or_else(Local::now)
    }

    fn fehler_melden(&self, _slug: &str, wo: &str, satz: &str) {
        sperren(&self.befunde).push((wo.to_string(), satz.to_string()));
    }
}

/// TOML-Text als JSON (Datumswerte als Text, wie im Dienst).
fn toml_als_json(datei: &str, text: &str) -> Result<Value, Fehler> {
    let tabelle: toml::Table = toml::from_str(text).map_err(|e| Fehler::vertrag(format!("{datei}: {}", e.message())))?;
    serde_json::to_value(tabelle).map_err(|e| Fehler::vertrag(format!("{datei}: {e}")))
}

/// Ein Modul mit Kontext im Arbeitsspeicher.
pub struct Pruefstand {
    anbindung: Arc<TestAnbindung>,
    kontext: ProjektKontext,
    hooks: crate::Hooks,
    router: Router,
    defs: Vec<TabellenDef>,
}

impl Pruefstand {
    /// Prüfstand mit leeren Tabellen aus `tabellen()` und ohne Dateien.
    pub fn neu(modul: &dyn Modul) -> Self {
        Self::mit_anbindung(modul, Arc::new(TestAnbindung::default()))
    }

    /// Prüfstand über einer vorbereiteten Anbindung (etwa mit Tabellen in alter Fassung).
    pub fn mit_anbindung(modul: &dyn Modul, anbindung: Arc<TestAnbindung>) -> Self {
        let defs = modul.tabellen();
        if let Err(e) = anbindung.einrichten(modul.slug(), &defs) {
            anbindung.fehler_melden(modul.slug(), "tabellen", &e);
        }
        let kontext = ProjektKontext::neu(modul.slug(), defs.clone(), anbindung.clone());
        let router = rahmen::einpacken(modul, kontext.clone());
        Self { anbindung, kontext, hooks: modul.hooks(), router, defs }
    }

    /// Legt eine Datei `daten/<name>.toml` an (`projekt` für projekt.toml).
    pub fn datei(self, name: &str, toml: &str) -> Self {
        sperren(&self.anbindung.dateien).insert(name.to_string(), toml.to_string());
        self
    }

    /// Liest projekt.toml und alle `daten/*.toml` aus einem Projektordner.
    pub fn ordner(self, projekt_ordner: &Path) -> Self {
        let mut d = sperren(&self.anbindung.dateien);
        if let Ok(t) = std::fs::read_to_string(projekt_ordner.join("projekt.toml")) {
            d.insert("projekt".into(), t);
        }
        if let Ok(rd) = std::fs::read_dir(projekt_ordner.join("daten")) {
            for e in rd.filter_map(|e| e.ok()) {
                let name = e.file_name().to_string_lossy().to_string();
                if let (Some(stamm), Ok(t)) = (name.strip_suffix(".toml"), std::fs::read_to_string(e.path())) {
                    d.insert(stamm.to_string(), t);
                }
            }
        }
        drop(d);
        self
    }

    /// Feste Zeit für `jetzt()`.
    pub fn zeit(self, jetzt: DateTime<Local>) -> Self {
        *sperren(&self.anbindung.jetzt) = Some(jetzt);
        self
    }

    /// Git-Stand, den `git()` liefert.
    pub fn git(self, stand: Vec<GitStand>) -> Self {
        *sperren(&self.anbindung.git) = stand;
        self
    }

    pub fn kontext(&self) -> &ProjektKontext {
        &self.kontext
    }

    pub fn anbindung(&self) -> &Arc<TestAnbindung> {
        &self.anbindung
    }

    /// Die Tabellen aus `tabellen()`.
    pub fn tabellen(&self) -> &[TabellenDef] {
        &self.defs
    }

    /// Gemeldete Fehler (wo, Satz), etwa aus Hooks und Abstürzen.
    pub fn befunde(&self) -> Vec<(String, String)> {
        sperren(&self.anbindung.befunde).clone()
    }

    /// Eine Anfrage an den Router des Moduls (Pfad ohne `/api/projekte/<slug>/x`).
    pub async fn anfrage(&self, methode: Method, pfad: &str, koerper: Option<Value>) -> (StatusCode, Value) {
        let mut req = Request::builder().method(methode).uri(pfad);
        let body = match koerper {
            Some(v) => {
                req = req.header(header::CONTENT_TYPE, "application/json");
                Body::from(serde_json::to_vec(&v).unwrap_or_default())
            }
            None => Body::empty(),
        };
        let Ok(req) = req.body(body) else { return (StatusCode::BAD_REQUEST, Value::Null) };
        let Ok(res) = self.router.clone().oneshot(req).await;
        let status = res.status();
        let bytes = res.into_body().collect().await.map(|b| b.to_bytes()).unwrap_or_default();
        let json = serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into()));
        (status, json)
    }

    pub async fn get(&self, pfad: &str) -> (StatusCode, Value) {
        self.anfrage(Method::GET, pfad, None).await
    }

    pub async fn post(&self, pfad: &str, koerper: Value) -> (StatusCode, Value) {
        self.anfrage(Method::POST, pfad, Some(koerper)).await
    }

    /// `nach_lesen` auf einer Datei wie im Dienst: erst lesen, dann der Hook.
    pub fn nach_lesen(&self, name: &str) -> Result<Value, Fehler> {
        let mut wert = self.kontext.daten(name)?.ok_or_else(|| Fehler::nicht_gefunden(format!("daten/{name}.toml fehlt")))?;
        self.hooks.nach_lesen_ausfuehren(&self.kontext, name, &mut wert)?;
        Ok(wert)
    }

    pub fn nach_daten_geaendert(&self, name: &str) -> Result<(), Fehler> {
        self.hooks.ausfuehren(&self.kontext, &Aufruf::NachDatenGeaendert(name.to_string()))
    }

    pub fn zeitplan(&self) -> Result<(), Fehler> {
        self.hooks.ausfuehren(&self.kontext, &Aufruf::Zeitplan)
    }

    pub fn taeglich(&self) -> Result<(), Fehler> {
        self.hooks.ausfuehren(&self.kontext, &Aufruf::Taeglich)
    }

    pub fn bei_vorgang(&self, vorgang: VorgangInfo) -> Result<(), Fehler> {
        self.hooks.ausfuehren(&self.kontext, &Aufruf::BeiVorgang(vorgang))
    }

    pub fn nach_bau(&self, ergebnis: BauErgebnis) -> Result<(), Fehler> {
        self.hooks.ausfuehren(&self.kontext, &Aufruf::NachBau(ergebnis))
    }
}
