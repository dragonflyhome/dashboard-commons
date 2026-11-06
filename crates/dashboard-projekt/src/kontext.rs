//! Der `ProjektKontext`: alles, was ein Modul vom Dienst bekommt, und nicht mehr.
//!
//! - eigene TOML-Dateien lesen (`projekt.toml`, `daten/<name>.toml`), vom
//!   Dienst mit dem Vertrag geprüft, als JSON oder als eigener Typ;
//! - eigene Tabellen im Datenspeicher (`projekt/<slug>/<name>`), nur die aus
//!   `Modul::tabellen()`;
//! - Git-Stand der eigenen Repos, nur lesen (aus dem Zwischenspeicher des Dienstes);
//! - `jetzt()` in lokaler Zeit;
//! - Protokoll mit dem Feld `projekt=<slug>`.
//!
//! Kein Tresor, keine Schlüssel, keine Vorgänge anderer Projekte, keine
//! fremden TOML: Der Slug steht fest im Kontext, das Modul kann ihn nicht ändern.
//!
//! Der Dienst füllt den Kontext über den Trait [`Anbindung`]. So hängt diese
//! Kiste nicht vom Dashboard ab; in Tests füllt ihn der Prüfstand.
//!
//! Alle Methoden hier können warten (Datei, Datenspeicher). Im Anfragepfad
//! darum über [`ProjektKontext::im_hintergrund`] aufrufen.

use std::sync::Arc;

use chrono::{DateTime, Local};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::fehler::{Fehler, absturz_grund};
use crate::tabellen::{self, TabellenDef};

/// Git-Stand eines Repos des Projekts (aus `[[repos]]` in projekt.toml).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct GitStand {
    /// Name aus `label`, sonst der Ordnername.
    pub name: String,
    pub zweig: Option<String>,
    pub commit: Option<String>,
    /// Datum des letzten Commits (wie git es liefert).
    pub datum: Option<String>,
    /// Betreff des letzten Commits; bei privaten Repos immer leer.
    pub betreff: Option<String>,
    /// Geänderte oder neue Dateien.
    pub geaendert: Option<u32>,
    /// Satz, wenn git nicht antwortete.
    pub fehler: Option<String>,
    /// Wann dieser Stand ermittelt wurde (RFC 3339); leer: noch nicht.
    pub stand: Option<String>,
}

/// Was der Dienst (oder der Prüfstand) für den Kontext bereitstellt. Die
/// Tabellen-Methoden bekommen die Fassung aus `tabellen()`: passt sie nicht zur
/// eingerichteten Fassung (Umbau abgelehnt oder gescheitert), ist die Tabelle
/// für das Modul gesperrt. Jede
/// Methode bekommt den Slug vom Kontext; ein Modul ruft sie nie selbst auf.
pub trait Anbindung: Send + Sync + 'static {
    /// `projekt.toml`, geprüft, als JSON.
    fn projekt_lesen(&self, slug: &str) -> Result<Value, Fehler>;
    /// `daten/<name>.toml`, geprüft, als JSON; `None`, wenn es sie nicht gibt.
    fn daten_lesen(&self, slug: &str, name: &str) -> Result<Option<Value>, Fehler>;
    /// Ein Eintrag (JSON-Bytes) einer eigenen Tabelle.
    fn eintrag_lesen(&self, slug: &str, tabelle: &str, fassung: u32, schluessel: &str) -> Result<Option<Vec<u8>>, Fehler>;
    /// Schreibt (oder ersetzt) einen Eintrag einer eigenen Tabelle.
    fn eintrag_schreiben(&self, slug: &str, tabelle: &str, fassung: u32, schluessel: &str, wert: &[u8]) -> Result<(), Fehler>;
    /// Höchstens `hoechstens` Einträge in Schlüsselreihenfolge; mit `rueckwaerts` die letzten zuerst.
    fn eintraege(&self, slug: &str, tabelle: &str, fassung: u32, hoechstens: usize, rueckwaerts: bool) -> Result<Vec<(String, Vec<u8>)>, Fehler>;
    /// Zahl der Einträge einer eigenen Tabelle.
    fn anzahl(&self, slug: &str, tabelle: &str, fassung: u32) -> Result<u64, Fehler>;
    /// Git-Stand der eigenen Repos (nur lesen, aus dem Zwischenspeicher).
    fn git(&self, slug: &str) -> Vec<GitStand>;
    /// Lokale Zeit.
    fn jetzt(&self) -> DateTime<Local> {
        Local::now()
    }
    /// Ein Fehler des Moduls für Protokoll und /api/pruefung (`wo`: Endpunkt
    /// oder Hook). Wiederholungen zählt der Dienst.
    fn fehler_melden(&self, slug: &str, wo: &str, satz: &str);
    /// Ein Lauf an dieser Stelle ging gut: ein früherer Fehler dort ist behoben.
    fn erfolg_melden(&self, _slug: &str, _wo: &str) {}
}

/// Der Kontext eines Moduls. Klein und billig zu klonen.
#[derive(Clone)]
pub struct ProjektKontext {
    slug: Arc<str>,
    tabellen: Arc<[TabellenDef]>,
    anbindung: Arc<dyn Anbindung>,
}

impl ProjektKontext {
    /// Nur für den Dienst und den Prüfstand.
    #[cfg(any(test, feature = "dienst", feature = "pruefstand"))]
    pub fn neu(slug: &str, tabellen: Vec<TabellenDef>, anbindung: Arc<dyn Anbindung>) -> Self {
        Self { slug: Arc::from(slug), tabellen: Arc::from(tabellen), anbindung }
    }

    /// Der eigene Slug.
    pub fn slug(&self) -> &str {
        &self.slug
    }

    /// `projekt.toml`, geprüft, als JSON.
    pub fn projekt(&self) -> Result<Value, Fehler> {
        self.anbindung.projekt_lesen(&self.slug)
    }

    /// `daten/<name>.toml` (Name ohne `.toml`), geprüft, als JSON; `None`, wenn es sie nicht gibt.
    pub fn daten(&self, name: &str) -> Result<Option<Value>, Fehler> {
        self.anbindung.daten_lesen(&self.slug, name)
    }

    /// Wie [`Self::daten`], gleich als eigener Typ. Passt die Form nicht: 422 mit Satz.
    pub fn daten_als<T: DeserializeOwned>(&self, name: &str) -> Result<Option<T>, Fehler> {
        let Some(wert) = self.daten(name)? else { return Ok(None) };
        serde_json::from_value(wert).map(Some).map_err(|e| Fehler::unlesbar(format!("daten/{name}.toml hat nicht die Form, die das Modul erwartet: {e}")))
    }

    /// Eine eigene Tabelle (nur die aus `tabellen()`).
    pub fn tabelle(&self, name: &str) -> Result<Tabelle<'_>, Fehler> {
        match self.tabellen.iter().find(|t| t.name == name) {
            Some(def) => Ok(Tabelle { kontext: self, name: def.name, fassung: def.fassung }),
            None => Err(Fehler::intern(format!("Die Tabelle „{name}“ steht nicht in tabellen() des Moduls „{}“.", self.slug))),
        }
    }

    /// Git-Stand der eigenen Repos, nur lesen.
    pub fn git(&self) -> Vec<GitStand> {
        self.anbindung.git(&self.slug)
    }

    /// Jetzt, lokale Zeit des PCs.
    pub fn jetzt(&self) -> DateTime<Local> {
        self.anbindung.jetzt()
    }

    /// Ins Protokoll (INFO), mit `projekt=<slug>`.
    pub fn info(&self, text: &str) {
        tracing::info!(projekt = %self.slug, "{text}");
    }

    /// Ins Protokoll (WARN), mit `projekt=<slug>`.
    pub fn warnung(&self, text: &str) {
        tracing::warn!(projekt = %self.slug, "{text}");
    }

    /// Meldet einen Fehler an Protokoll und /api/pruefung.
    pub fn fehler_melden(&self, wo: &str, satz: &str) {
        tracing::error!(projekt = %self.slug, "{wo}: {satz}");
        self.anbindung.fehler_melden(&self.slug, wo, satz);
    }

    /// Meldet, dass ein Lauf an dieser Stelle gut ging.
    pub fn erfolg_melden(&self, wo: &str) {
        self.anbindung.erfolg_melden(&self.slug, wo);
    }

    /// Führt Arbeit mit Dateien oder Datenspeicher in `spawn_blocking` aus
    /// (kein Blockieren im Anfragepfad). Ein Absturz darin wird zu 500 mit Satz.
    pub async fn im_hintergrund<T, F>(&self, f: F) -> Result<T, Fehler>
    where
        T: Send + 'static,
        F: FnOnce(&ProjektKontext) -> Result<T, Fehler> + Send + 'static,
    {
        let k = self.clone();
        match tokio::task::spawn_blocking(move || f(&k)).await {
            Ok(ergebnis) => ergebnis,
            Err(e) => {
                let grund = if e.is_panic() { absturz_grund(&*e.into_panic()) } else { e.to_string() };
                self.fehler_melden("im_hintergrund", &format!("abgestürzt: {grund}"));
                Err(Fehler::intern(format!(
                    "Das Modul „{}“ ist bei dieser Anfrage abgestürzt. Der Fehler steht im Protokoll und auf /api/pruefung.",
                    self.slug
                )))
            }
        }
    }

    /// Die Tabellen aus `tabellen()`.
    pub fn tabellen(&self) -> &[TabellenDef] {
        &self.tabellen
    }
}

/// Eine eigene Tabelle. Werte sind JSON; Einträge werden nie gelöscht
/// (Daten bleiben), nur ersetzt.
pub struct Tabelle<'a> {
    kontext: &'a ProjektKontext,
    name: &'static str,
    fassung: u32,
}

impl Tabelle<'_> {
    fn schluessel_pruefen(schluessel: &str) -> Result<(), Fehler> {
        if tabellen::ist_schluessel(schluessel) {
            Ok(())
        } else {
            Err(Fehler::unlesbar(format!("Der Schlüssel ist ungültig: 1 bis {} Zeichen, ohne Steuerzeichen.", tabellen::SCHLUESSEL_HOECHSTENS)))
        }
    }

    fn json<T: DeserializeOwned>(&self, schluessel: &str, bytes: &[u8]) -> Result<T, Fehler> {
        serde_json::from_slice(bytes).map_err(|e| Fehler::intern(format!("Der Eintrag „{schluessel}“ in der Tabelle „{}“ ist unlesbar: {e}", self.name)))
    }

    /// Ein Eintrag; `None`, wenn es ihn nicht gibt.
    pub fn lesen<T: DeserializeOwned>(&self, schluessel: &str) -> Result<Option<T>, Fehler> {
        Self::schluessel_pruefen(schluessel)?;
        let k = self.kontext;
        match k.anbindung.eintrag_lesen(&k.slug, self.name, self.fassung, schluessel)? {
            Some(b) => self.json(schluessel, &b).map(Some),
            None => Ok(None),
        }
    }

    /// Schreibt einen Eintrag (ersetzt einen gleichen Schlüssel).
    pub fn schreiben<T: Serialize>(&self, schluessel: &str, wert: &T) -> Result<(), Fehler> {
        Self::schluessel_pruefen(schluessel)?;
        let bytes = serde_json::to_vec(wert).map_err(|e| Fehler::intern(format!("Der Wert lässt sich nicht als JSON schreiben: {e}")))?;
        if bytes.len() > tabellen::WERT_HOECHSTENS {
            return Err(Fehler::zu_gross(format!("Ein Eintrag darf höchstens {} KiB groß sein.", tabellen::WERT_HOECHSTENS / 1024)));
        }
        let k = self.kontext;
        k.anbindung.eintrag_schreiben(&k.slug, self.name, self.fassung, schluessel, &bytes)
    }

    /// Die ersten `hoechstens` Einträge in Schlüsselreihenfolge.
    pub fn erste<T: DeserializeOwned>(&self, hoechstens: usize) -> Result<Vec<(String, T)>, Fehler> {
        self.liste(hoechstens, false)
    }

    /// Die letzten `hoechstens` Einträge, der letzte zuerst.
    pub fn letzte<T: DeserializeOwned>(&self, hoechstens: usize) -> Result<Vec<(String, T)>, Fehler> {
        self.liste(hoechstens, true)
    }

    fn liste<T: DeserializeOwned>(&self, hoechstens: usize, rueckwaerts: bool) -> Result<Vec<(String, T)>, Fehler> {
        let k = self.kontext;
        k.anbindung.eintraege(&k.slug, self.name, self.fassung, hoechstens, rueckwaerts)?.into_iter().map(|(s, b)| self.json(&s, &b).map(|w| (s, w))).collect()
    }

    /// Zahl der Einträge.
    pub fn anzahl(&self) -> Result<u64, Fehler> {
        let k = self.kontext;
        k.anbindung.anzahl(&k.slug, self.name, self.fassung)
    }
}
