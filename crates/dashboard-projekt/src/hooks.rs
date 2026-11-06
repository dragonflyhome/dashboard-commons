//! Hooks: feste Stellen, an denen der Dienst ein Modul aufruft.
//!
//! - `nach_lesen(name, eintraege)`: verarbeitet eine `daten/<name>.toml` des
//!   eigenen Projekts, bevor `GET /api/projekte/<slug>/daten/<name>` sie
//!   ausliefert (die Middleware eines Projekts);
//! - `nach_daten_geaendert(name)`: eine eigene `daten/<name>.toml` wurde
//!   geändert (entprellt, 2 s nach der letzten Änderung);
//! - `zeitplan`: regelmäßig, höchstens alle 5 Minuten;
//! - `taeglich`: einmal am Tag, nach der Sicherung des Datenspeichers;
//! - `bei_vorgang(vorgang)`: ein Schritt eines eigenen Vorgangs;
//! - `nach_bau(ergebnis)`: der Bau der Oberfläche ist fertig, grün oder rot.
//!
//! Jeder Hook ist eine gewöhnliche (blockierende) Funktion. Der Dienst ruft
//! sie in `spawn_blocking` auf, mit Zeitlimit (30 s, `nach_lesen` 5 s, weil
//! eine Anfrage wartet), höchstens ein Lauf je Hook und Projekt. Ein Fehler
//! oder Absturz geht ins Protokoll und als Fund des Projekts nach /api/pruefung.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::fehler::{Fehler, absturz_grund};
use crate::kontext::ProjektKontext;

/// Der kürzeste erlaubte Abstand für `zeitplan`.
pub const ZEITPLAN_MINDESTENS: Duration = Duration::from_secs(5 * 60);
/// Zeitlimit je Lauf eines Hooks.
pub const ZEITLIMIT: Duration = Duration::from_secs(30);
/// Zeitlimit für `nach_lesen` (eine Anfrage wartet darauf).
pub const ZEITLIMIT_NACH_LESEN: Duration = Duration::from_secs(5);
/// Entprellung für `nach_daten_geaendert`.
pub const ENTPRELLEN: Duration = Duration::from_secs(2);

pub type NachLesen = fn(&ProjektKontext, name: &str, eintraege: &mut Value) -> Result<(), Fehler>;
pub type NachDatenGeaendert = fn(&ProjektKontext, name: &str) -> Result<(), Fehler>;
pub type Lauf = fn(&ProjektKontext) -> Result<(), Fehler>;
pub type BeiVorgang = fn(&ProjektKontext, vorgang: &VorgangInfo) -> Result<(), Fehler>;
pub type NachBau = fn(&ProjektKontext, ergebnis: BauErgebnis) -> Result<(), Fehler>;

/// Ein regelmäßiger Lauf.
#[derive(Debug, Clone, Copy)]
pub struct Zeitplan {
    /// Abstand; weniger als 5 Minuten zählt als 5 Minuten.
    pub alle: Duration,
    pub lauf: Lauf,
}

/// Die Hooks eines Moduls; was `None` ist, ruft der Dienst nie.
#[derive(Debug, Clone, Copy, Default)]
pub struct Hooks {
    pub nach_lesen: Option<NachLesen>,
    pub nach_daten_geaendert: Option<NachDatenGeaendert>,
    pub zeitplan: Option<Zeitplan>,
    pub taeglich: Option<Lauf>,
    pub bei_vorgang: Option<BeiVorgang>,
    pub nach_bau: Option<NachBau>,
}

/// Ein Schritt eines eigenen Vorgangs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VorgangInfo {
    /// Kennung `v-JJJJMMTT-NNNN`.
    pub id: String,
    /// Art: `antwort`, `frage`, `wunsch`, `bau`, `abnahme`, `merge`.
    pub art: String,
    /// Zustand nach diesem Schritt: `offen`, `zugestellt`, `uebernommen`, `erledigt`.
    pub zustand: String,
}

/// Ausgang eines Baus der Oberfläche.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BauErgebnis {
    Gruen,
    Rot,
}

/// Name eines Hooks (für Protokoll und Prüfung).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum HookArt {
    NachLesen,
    NachDatenGeaendert,
    Zeitplan,
    Taeglich,
    BeiVorgang,
    NachBau,
}

impl HookArt {
    pub fn name(self) -> &'static str {
        match self {
            Self::NachLesen => "nach_lesen",
            Self::NachDatenGeaendert => "nach_daten_geaendert",
            Self::Zeitplan => "zeitplan",
            Self::Taeglich => "taeglich",
            Self::BeiVorgang => "bei_vorgang",
            Self::NachBau => "nach_bau",
        }
    }
}

/// Ein Aufruf eines Hooks mit seinen Angaben.
#[derive(Debug, Clone, PartialEq)]
pub enum Aufruf {
    NachDatenGeaendert(String),
    Zeitplan,
    Taeglich,
    BeiVorgang(VorgangInfo),
    NachBau(BauErgebnis),
}

impl Aufruf {
    pub fn art(&self) -> HookArt {
        match self {
            Self::NachDatenGeaendert(_) => HookArt::NachDatenGeaendert,
            Self::Zeitplan => HookArt::Zeitplan,
            Self::Taeglich => HookArt::Taeglich,
            Self::BeiVorgang(_) => HookArt::BeiVorgang,
            Self::NachBau(_) => HookArt::NachBau,
        }
    }
}

impl Hooks {
    /// Hat das Modul diesen Hook?
    pub fn hat(&self, art: HookArt) -> bool {
        match art {
            HookArt::NachLesen => self.nach_lesen.is_some(),
            HookArt::NachDatenGeaendert => self.nach_daten_geaendert.is_some(),
            HookArt::Zeitplan => self.zeitplan.is_some(),
            HookArt::Taeglich => self.taeglich.is_some(),
            HookArt::BeiVorgang => self.bei_vorgang.is_some(),
            HookArt::NachBau => self.nach_bau.is_some(),
        }
    }

    /// Abstand des Zeitplans, mindestens 5 Minuten.
    pub fn zeitplan_abstand(&self) -> Option<Duration> {
        self.zeitplan.map(|z| z.alle.max(ZEITPLAN_MINDESTENS))
    }

    /// Führt einen Aufruf aus (blockierend). Absturz wird zu `Fehler`,
    /// Fehler und Erfolg gehen an den Kontext. Ohne Hook: `Ok`.
    pub fn ausfuehren(&self, kontext: &ProjektKontext, aufruf: &Aufruf) -> Result<(), Fehler> {
        let art = aufruf.art();
        let ergebnis = match aufruf {
            Aufruf::NachDatenGeaendert(name) => self.nach_daten_geaendert.map(|h| aufrufen(kontext, art, || h(kontext, name))),
            Aufruf::Zeitplan => self.zeitplan.map(|z| aufrufen(kontext, art, || (z.lauf)(kontext))),
            Aufruf::Taeglich => self.taeglich.map(|h| aufrufen(kontext, art, || h(kontext))),
            Aufruf::BeiVorgang(v) => self.bei_vorgang.map(|h| aufrufen(kontext, art, || h(kontext, v))),
            Aufruf::NachBau(e) => self.nach_bau.map(|h| aufrufen(kontext, art, || h(kontext, *e))),
        };
        ergebnis.unwrap_or(Ok(()))
    }

    /// `nach_lesen` (blockierend): verändert `eintraege` nur, wenn der Hook gut ausgeht.
    pub fn nach_lesen_ausfuehren(&self, kontext: &ProjektKontext, name: &str, eintraege: &mut Value) -> Result<(), Fehler> {
        let Some(h) = self.nach_lesen else { return Ok(()) };
        let mut kopie = eintraege.clone();
        aufrufen(kontext, HookArt::NachLesen, || h(kontext, name, &mut kopie))?;
        *eintraege = kopie;
        Ok(())
    }
}

/// Ruft `f` auf, fängt einen Absturz und meldet Fehler oder Erfolg.
pub fn aufrufen(kontext: &ProjektKontext, art: HookArt, f: impl FnOnce() -> Result<(), Fehler>) -> Result<(), Fehler> {
    let span = tracing::info_span!("hook", projekt = %kontext.slug(), hook = art.name());
    let _drin = span.enter();
    let ergebnis = match catch_unwind(AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(e) => Err(Fehler::intern(format!("abgestürzt: {}", absturz_grund(&*e)))),
    };
    match &ergebnis {
        Ok(()) => kontext.erfolg_melden(art.name()),
        Err(e) => kontext.fehler_melden(art.name(), &e.fehler),
    }
    ergebnis
}
