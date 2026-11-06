//! Die Anbindung des Dienstes an das SDK (`dashboard_projekt::Anbindung`):
//! Hier wird der `ProjektKontext` eines Moduls gefüllt, mit genau dem, was
//! `[projekt_backend]` erlaubt. Der Slug kommt immer vom Kontext, nie vom
//! Modul; darum liest ein Modul nur seine eigenen Dateien und Tabellen.
//!
//! - TOML: aus dem ContentStore (Etappe 2d), also mit dem Vertrag geprüft wie
//!   bei 4v; ein Vertragsbruch wird zu 422 `vertrag` mit Datei und Zeile.
//! - Datenspeicher: nur Tabellen aus dem Verzeichnis (`speicher::projekte`).
//! - Git: nur der Zwischenspeicher (kein git-Aufruf, nur lesen).

use std::sync::Arc;

use dashboard_projekt::{Anbindung, Fehler, GitStand};
use serde_json::Value;

use super::Stoerungen;
use crate::git;
use crate::inhalt::store::ContentStore;
use crate::speicher::{Speicher, SpeicherFehler};
use crate::vertrag::lesen::LeseFehler;

pub struct DienstAnbindung {
    inhalt: Arc<ContentStore>,
    speicher: Option<Arc<Speicher>>,
    /// Satz, wenn der Datenspeicher fehlt (`AppState::speicher_satz`).
    speicher_satz: String,
    git: Arc<git::Zwischenspeicher>,
    stoerungen: Arc<Stoerungen>,
}

impl DienstAnbindung {
    pub fn neu(
        inhalt: Arc<ContentStore>,
        speicher: Option<Arc<Speicher>>,
        speicher_satz: String,
        git: Arc<git::Zwischenspeicher>,
        stoerungen: Arc<Stoerungen>,
    ) -> Self {
        Self { inhalt, speicher, speicher_satz, git, stoerungen }
    }

    /// Wie `seite::kopf_lesen`: ohne projekt.toml der Satz der Datei, die fehlt.
    fn kopf(&self, slug: &str) -> Result<crate::vertrag::projekt::Projekt, LeseFehler> {
        match self.inhalt.bestand().kopf(slug) {
            Some(k) => k.map(|(p, _)| p),
            None => Err(LeseFehler { datei: format!("projects/{slug}/projekt.toml"), zeile: None, text: "Die Datei lässt sich nicht lesen.".to_string() }),
        }
    }

    fn speicher(&self) -> Result<&Speicher, Fehler> {
        self.speicher.as_deref().ok_or_else(|| Fehler::speicher_fehlt(self.speicher_satz.clone()))
    }
}

fn vertrag(e: LeseFehler) -> Fehler {
    Fehler::vertrag(e.to_string())
}

fn speicherfehler(e: SpeicherFehler) -> Fehler {
    tracing::error!("Modul-Tabelle: {e} ({})", e.einzelheiten());
    Fehler::intern(format!("Der Datenspeicher lässt sich gerade nicht lesen oder schreiben: {}", e.einzelheiten()))
}

impl Anbindung for DienstAnbindung {
    fn projekt_lesen(&self, slug: &str) -> Result<Value, Fehler> {
        let kopf = self.kopf(slug).map_err(vertrag)?;
        serde_json::to_value(kopf).map_err(|e| Fehler::intern(e.to_string()))
    }

    fn daten_lesen(&self, slug: &str, name: &str) -> Result<Option<Value>, Fehler> {
        match self.inhalt.bestand().daten(slug, name).map_err(vertrag)? {
            Some((inhalt, _)) => serde_json::to_value(inhalt).map(Some).map_err(|e| Fehler::intern(e.to_string())),
            None => Ok(None),
        }
    }

    fn eintrag_lesen(&self, slug: &str, tabelle: &str, fassung: u32, schluessel: &str) -> Result<Option<Vec<u8>>, Fehler> {
        self.speicher()?.projekt_eintrag_lesen(slug, tabelle, fassung, schluessel).map_err(speicherfehler)
    }

    fn eintrag_schreiben(&self, slug: &str, tabelle: &str, fassung: u32, schluessel: &str, wert: &[u8]) -> Result<(), Fehler> {
        self.speicher()?.projekt_eintrag_schreiben(slug, tabelle, fassung, schluessel, wert).map_err(speicherfehler)
    }

    fn eintraege(&self, slug: &str, tabelle: &str, fassung: u32, hoechstens: usize, rueckwaerts: bool) -> Result<Vec<(String, Vec<u8>)>, Fehler> {
        self.speicher()?.projekt_eintraege(slug, tabelle, fassung, hoechstens, rueckwaerts).map_err(speicherfehler)
    }

    fn anzahl(&self, slug: &str, tabelle: &str, fassung: u32) -> Result<u64, Fehler> {
        self.speicher()?.projekt_anzahl(slug, tabelle, fassung).map_err(speicherfehler)
    }

    fn git(&self, slug: &str) -> Vec<GitStand> {
        let Ok(kopf) = self.kopf(slug) else { return Vec::new() };
        kopf.repos
            .iter()
            .map(|r| {
                let z = self.git.zeile(r);
                GitStand {
                    name: z.label,
                    zweig: z.branch,
                    commit: z.commit_hash,
                    datum: z.commit_date,
                    betreff: if z.private { None } else { z.commit_subject },
                    geaendert: z.dirty_files.map(|n| u32::try_from(n).unwrap_or(u32::MAX)),
                    fehler: z.error,
                    stand: z.stand,
                }
            })
            .collect()
    }

    fn fehler_melden(&self, slug: &str, wo: &str, satz: &str) {
        self.stoerungen.melden(slug, wo, satz);
    }

    fn erfolg_melden(&self, slug: &str, wo: &str) {
        self.stoerungen.erfolg(slug, wo);
    }
}
