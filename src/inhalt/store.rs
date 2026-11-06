//! ContentStore (Etappe 2d, Zielbild „Vom Speichern zum Bild“): der Inhalt
//! aller Projekte im Arbeitsspeicher, mit Versionsnummer.
//!
//! Was hier liegt, ist genau das, was die Lesewege bisher je Anfrage von der
//! Platte holten, fertig gelesen und mit dem Vertrag geprüft: je Projekt die
//! Projekt (`seite::load_one`), der Kopf aus
//! `projekt.toml` (`seite::kopf_lesen`), die Namen und Inhalte in `daten/`
//! (`seite::daten_lesen`, auch `fragen.toml`), dazu `data/schwerpunkte.toml`,
//! `data/gruppen.toml` (Projektgruppen, neben `schwerpunkte.toml`) und `data/pc.toml`. Gelesen wird mit denselben Funktionen wie vorher; die
//! Antworten der API ändern sich dadurch nicht, nur ihre Quelle.
//!
//! **Erneuern.** Der Beobachter (`api::events`, alle 2 s, ohne `notify`) ruft
//! [`ContentStore::abgleichen`]. Das vergleicht Zeitstempel und Größe aller
//! Dateien unter `projects/` und `data/` mit dem letzten Blick und liest nur
//! die Projekte neu, in denen sich `projekt.toml` oder etwas in
//! `daten/` geändert hat (und nur die geänderte Datei in `data/`). Danach
//! steigt die Versionsnummer, und der Live-Kanal bekommt genau ein Ereignis je
//! Projekt (`projekt { slug, version }`) oder Datei (`daten { name, version }`);
//! ändern sich mehr als [`SAMMEL_AB`] Projekte auf einmal, ein einziges
//! Sammelereignis `inhalt` mit allen Namen. Ein geänderter Git-Stand zählt wie
//! eine Änderung am Projekt ([`ContentStore::git_geaendert`]).
//!
//! **Lesefehler sind kein Verlust.** Ist eine Datei gerade gesperrt (ein
//! Programm schreibt, Windows meldet Freigabe- oder Sperrverletzung) oder
//! fehlen die Rechte, bleibt der letzte gute Stand des Projekts stehen, und
//! es gibt einen Fund für /api/pruefung ([`Bestand::lesefehler_befunde`]).
//! Ob das so ist, entscheidet [`ist_gesperrt`] am `io::Error` der
//! Leseoperation selbst (`seite::gesperrt_merken`, gesammelt in
//! `seite::mit_merkliste`), nicht am Satz und nicht an einem späteren
//! zweiten Blick; eine Verknüpfung zählt nie als Sperre.
//! Beim nächsten Blick wird es wieder versucht, auch ohne neuen Zeitstempel
//! und auch, wenn es noch nie einen guten Stand gab; solange es gesperrt
//! bleibt, ohne neue Version und ohne Ereignis. Eine fehlende Datei, Text,
//! der kein UTF-8 ist, und ein Bruch des Vertrags (falsches Feld, kaputtes
//! TOML) sind dagegen keine Lesefehler: Sie stehen wie bisher sofort in der
//! Antwort (422, rote Karte), damit der Chat sie sieht.
//!
//! **Tageswechsel.** Kennzahlen mit `berechnet` hängen vom Datum ab, ebenso
//! (Etappe 2e) die `art` im Zeitstrahl. Ist ein
//! neuer Tag, liest der nächste Abgleich des Beobachters (höchstens 2 s
//! später) die Projekte mit `kennzahlen.toml`, `zeitstrahl.toml` oder
//! `ampel.toml` neu; ein Zugriff selbst liest
//! im Dienst nie von der Platte.
//!
//! **Tests.** Mit [`Abgleich::BeiJedemZugriff`] gleicht jeder Zugriff erst ab
//! (es gibt dort keinen Beobachter). So sehen Tests, die Dateien schreiben und
//! dann fragen, ihren Stand, und der Weg ist derselbe wie im Dienst.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::{Instant, SystemTime};

use chrono::{DateTime, Local, NaiveDate};
use tokio::sync::broadcast;
use tracing::{debug, info, warn};

use crate::api::events::{DatenEreignis, InhaltEreignis, ProjektEreignis, SammelEreignis};
use crate::inhalt::pruefung::Befund;
use crate::inhalt::seite::{self, Broken, Project};
use crate::pc::pcdata::{self, PcData};
use crate::vertrag::abgeleitet;
use crate::vertrag::daten::{DatenInhalt, ist_datenname};
use crate::vertrag::gruppen::{self, GruppeAnzeige, Gruppen};
use crate::vertrag::lesen::LeseFehler;
use crate::vertrag::projekt::{self, Projekt};
use crate::vertrag::schwerpunkte::{self, Schwerpunkte};

/// Ändern sich mehr Projekte als so viele auf einmal, geht ein einziges
/// Sammelereignis `inhalt` hinaus statt eines Ereignisses je Projekt.
pub const SAMMEL_AB: usize = 3;

/// Zeitstempel und Größe einer Datei (wie der Beobachter sie vergleicht).
pub type Stempel = (SystemTime, u64);

/// Wann der Speicher mit der Platte abgleicht.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Abgleich {
    /// Im Dienst: der Beobachter alle 2 s (`api::events::spawn_watcher`).
    Beobachter,
    /// In Tests: vor jedem Zugriff.
    BeiJedemZugriff,
}

/// Ergebnis von `seite::kopf_lesen`.
pub type KopfErgebnis = Result<(Projekt, Option<DateTime<Local>>), LeseFehler>;
/// Ergebnis von `seite::daten_lesen` für eine Datei.
pub type DatenErgebnis = Result<Option<(DatenInhalt, toml::Table)>, LeseFehler>;

/// Ein Projekt, wie es zuletzt gelesen wurde.
#[derive(Debug, Clone)]
pub struct ProjektStand {
    pub slug: String,
    /// Steht in der Projektliste (`load_all`: ein Ordner, auch hinter einer Verknüpfung).
    pub in_liste: bool,
    /// Der Projektordner selbst ist eine Verknüpfung.
    pub verknuepfung: bool,
    /// Das Projekt (`seite::load_one`): `Ok(None)` heißt, keine projekt.toml.
    pub haupt: Result<Option<Project>, String>,
    /// Liegt `projekt.toml` da (gleich als was, wie `pruefung::ist_projekt`)?
    pub hat_projekt_toml: bool,
    /// `seite::kopf_lesen`, wenn `projekt.toml` da ist.
    pub kopf: Option<KopfErgebnis>,
    /// `seite::daten_namen`.
    pub namen: Result<Vec<String>, LeseFehler>,
    /// `seite::daten_lesen` für jede Datei `daten/<name>.toml` mit einem
    /// Namen aus dem Vertrag; `Err`, wenn schon der Ordner nicht ging.
    pub daten: Result<BTreeMap<String, DatenErgebnis>, LeseFehler>,
    /// Mit diesem Datum gerechnet (Kennzahlen mit `berechnet`).
    pub tag: NaiveDate,
    /// Wann gelesen.
    pub gelesen: DateTime<Local>,
    /// Lesefehler (gesperrte Dateien) beim letzten Versuch: Datei und Satz.
    /// Ist die Liste nicht leer, wird das Projekt alle 2 s wieder gelesen.
    pub lesefehler: Vec<LeseHinweis>,
    /// Ist dieser Stand ein guter (ohne Lesefehler gelesen)? `false`: Es gab
    /// noch nie einen guten Stand; die Fehler stehen so da, wie sie gelesen wurden.
    pub gut: bool,
    /// Gesperrte Dateien beim Lesen gerade dieses Stands (Datei, Satz).
    gesperrt: Vec<(String, String)>,
    /// Entscheidung archiv-status: Ist das Projekt archiviert? Aus
    /// dem Kopf; lässt der sich nicht lesen, bleibt der zuletzt bekannte Stand
    /// ([`uebernehmen`]), und ohne einen solchen zählt allein die Zeile
    /// `status = …` ([`status_locker`]). So hebt ein Tippfehler im Kopf das
    /// Archiv nicht auf.
    pub archiviert: bool,
    /// Ließ sich der Status streng lesen (Kopf in Ordnung)?
    archiv_streng: bool,
}

/// Eine Datei, die sich zuletzt nicht lesen ließ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeseHinweis {
    pub datei: String,
    pub satz: String,
    /// Seit wann (Tag und Uhrzeit).
    pub seit: String,
}

pub use crate::inhalt::seite::ist_gesperrt;

impl ProjektStand {
    /// Liest ein Projekt von der Platte, mit denselben Funktionen wie die
    /// Lesewege vorher. `None`: Den Ordner gibt es nicht (mehr). Blockiert.
    pub fn lesen(dir: &Path, slug: &str, heute: NaiveDate) -> Option<ProjektStand> {
        if !seite::is_valid_slug(slug) {
            return None;
        }
        let ordner = dir.join(slug);
        let verknuepfung = seite::ist_verknuepfung(&ordner);
        let in_liste = ordner.is_dir();
        if !in_liste && !verknuepfung {
            return None;
        }
        // Welche Leseoperation an einer Sperre scheiterte, merkt `seite` je
        // Faden in einer eigenen Merkliste (der Fehler der Operation selbst,
        // kein späterer Blick).
        let (mut p, gesperrt) = seite::mit_merkliste(|| {
            let haupt = seite::load_one(dir, slug).map_err(|e| format!("{e:#}"));
            let hat_projekt_toml = std::fs::symlink_metadata(ordner.join("projekt.toml")).is_ok();
            let kopf = hat_projekt_toml.then(|| seite::kopf_lesen(dir, slug));
            let daten = seite::daten_dateien(dir, slug).map(|liste| {
                liste
                    .into_iter()
                    .filter_map(|(name, _)| name.strip_suffix(".toml").filter(|s| ist_datenname(s)).map(str::to_string))
                    .map(|stamm| {
                        let inhalt = seite::daten_lesen(dir, slug, &stamm, heute);
                        (stamm, inhalt)
                    })
                    .collect()
            });
            ProjektStand {
                slug: slug.to_string(),
                in_liste,
                verknuepfung,
                haupt,
                hat_projekt_toml,
                kopf,
                namen: seite::daten_namen(dir, slug),
                daten,
                tag: heute,
                gelesen: Local::now(),
                lesefehler: Vec::new(),
                gut: true,
                gesperrt: Vec::new(),
                archiviert: false,
                archiv_streng: false,
            }
        });
        p.gesperrt = p.gesperrt_aus(gesperrt);
        p.gut = p.gesperrt.is_empty();
        let streng = match (&p.haupt, &p.kopf) {
            (Ok(Some(h)), _) => Some(projekt::ist_archiviert(&h.meta.status)),
            (_, Some(Ok((k, _)))) => Some(k.status == projekt::Status::Archiviert),
            _ => None,
        };
        p.archiv_streng = streng.is_some();
        p.archiviert = streng.unwrap_or_else(|| status_locker(&ordner).is_some_and(|s| projekt::ist_archiviert(&s)));
        Some(p)
    }

    /// Die beim Lesen gesperrten Dateien mit dem Satz des passenden Fehlers
    /// (Datei, Satz).
    fn gesperrt_aus(&self, dateien: Vec<String>) -> Vec<(String, String)> {
        let mut fehler: Vec<&LeseFehler> = Vec::new();
        if let Some(Err(e)) = &self.kopf {
            fehler.push(e);
        }
        if let Err(e) = &self.namen {
            fehler.push(e);
        }
        match &self.daten {
            Err(e) => fehler.push(e),
            Ok(karte) => fehler.extend(karte.values().filter_map(|r| r.as_ref().err())),
        }
        let mut aus: Vec<(String, String)> = Vec::new();
        for d in dateien {
            if aus.iter().any(|(x, _)| *x == d) {
                continue;
            }
            let satz = fehler.iter().find(|e| e.datei == d).map(|e| e.text.clone()).unwrap_or_else(|| "Die Datei lässt sich nicht lesen.".to_string());
            aus.push((d, satz));
        }
        aus
    }

    /// Hängt das Gelesene vom Datum ab? Kennzahlen (mit `berechnet`), seit
    /// Etappe 2e auch der Zeitstrahl (ein `geplant`, dessen Datum vorbei ist,
    /// wird `erledigt`), seit B1 die Ampel (eine Zeile mit `stand` kippt
    /// nach Mitternacht von frisch auf alt).
    fn haengt_vom_tag_ab(&self) -> bool {
        self.daten.as_ref().is_ok_and(|d| d.contains_key("kennzahlen") || d.contains_key("zeitstrahl") || d.contains_key("ampel"))
    }

    /// Wie `seite::daten_lesen` für dieses Projekt.
    pub fn daten(&self, name: &str) -> Result<Option<(DatenInhalt, toml::Table)>, LeseFehler> {
        if !ist_datenname(name) {
            return Ok(None);
        }
        match &self.daten {
            Err(e) => Err(e.clone()),
            Ok(karte) => karte.get(name).cloned().unwrap_or(Ok(None)),
        }
    }
}

/// Eine Datei aus `data/`: `Ok(None)` heißt, sie fehlt.
pub type DatenDatei<T> = Result<Option<T>, String>;

/// Der ganze Inhalt zu einem Zeitpunkt. Wer liest, bekommt ein `Arc` darauf
/// und sieht einen Stand, der zusammenpasst, auch wenn währenddessen neu
/// gelesen wird.
#[derive(Debug, Clone)]
pub struct Bestand {
    /// Steigt mit jeder Änderung.
    pub version: u64,
    /// Version je Projekt (die zuletzt geänderte).
    pub versionen: BTreeMap<String, u64>,
    /// Version je Datei aus `data/` (Name ohne `.toml`).
    pub daten_versionen: BTreeMap<String, u64>,
    pub tag: NaiveDate,
    /// Ließ sich der Projektordner beim letzten Blick lesen?
    pub ordner_lesbar: bool,
    pub projekte: BTreeMap<String, Arc<ProjektStand>>,
    /// `data/schwerpunkte.toml`; `Err` mit dem Fehler aus dem Vertrag.
    pub schwerpunkte: Result<Option<Schwerpunkte>, LeseFehler>,
    /// `data/gruppen.toml` (Entscheidung projektgruppe); `Ok(None)`:
    /// die Datei fehlt, es gibt keine Gruppen. `Err` mit dem Fehler aus dem Vertrag.
    pub gruppen: Result<Option<Gruppen>, LeseFehler>,
    /// `data/pc.toml`; `Err` mit dem Satz aus `pcdata` (Datei, Zeile, was falsch ist).
    pub pc: Result<PcData, String>,
    /// Lesefehler bei Dateien aus `data/`, deren letzter guter Stand gilt.
    pub daten_lesefehler: Vec<LeseHinweis>,
    /// Dauer des ersten vollen Lesens (für /api/betrieb und Messungen).
    pub geladen_in_ms: u64,
    /// Etappe 4b: je Slug, der in zwei Inhaltsquellen vorkommt, ein Satz
    /// (`quellen::satz_doppelt`; die erste Quelle bleibt sichtbar).
    pub quellen_doppelt: Vec<String>,
    /// Etappe 4b: Projekte weiterer Quellen mit Rechenteil (Slug, Satz).
    pub quellen_rechenteile: Vec<(String, String)>,
    /// Je unlesbarer Inhaltsquelle ein Satz ohne Pfad (`quellen::satz_unlesbar`).
    pub quellen_unlesbar: Vec<String>,
    /// Je Quelle mit abweichender oder kaputter `dashboard-version.toml` ein Satz.
    pub quellen_version: Vec<String>,
}

impl Bestand {
    fn leer(heute: NaiveDate) -> Self {
        Self {
            version: 0,
            versionen: BTreeMap::new(),
            daten_versionen: BTreeMap::new(),
            tag: heute,
            ordner_lesbar: true,
            projekte: BTreeMap::new(),
            schwerpunkte: Ok(None),
            gruppen: Ok(None),
            pc: Ok(PcData::default()),
            daten_lesefehler: Vec::new(),
            geladen_in_ms: 0,
            quellen_doppelt: Vec::new(),
            quellen_rechenteile: Vec::new(),
            quellen_unlesbar: Vec::new(),
            quellen_version: Vec::new(),
        }
    }

    pub fn projekt(&self, slug: &str) -> Option<&ProjektStand> {
        self.projekte.get(slug).map(|p| &**p)
    }

    /// Version eines Projekts (0: unbekannt).
    pub fn version_von(&self, slug: &str) -> u64 {
        self.versionen.get(slug).copied().unwrap_or(0)
    }

    /// Wie `seite::load_all`: alle Hauptseiten nach `order` und Titel, dazu
    /// die, die sich nicht laden ließen (nach Slug).
    pub fn liste(&self) -> (Vec<&Project>, Vec<Broken>) {
        let mut projekte = Vec::new();
        let mut kaputt = Vec::new();
        for p in self.projekte.values().filter(|p| p.in_liste) {
            match &p.haupt {
                Ok(Some(x)) => projekte.push(x),
                Ok(None) => {}
                Err(e) => kaputt.push(Broken { slug: p.slug.clone(), error: e.clone() }),
            }
        }
        projekte.sort_by(|a, b| a.meta.order.cmp(&b.meta.order).then_with(|| a.meta.title.cmp(&b.meta.title)));
        (projekte, kaputt)
    }

    /// Ist das Projekt archiviert (Entscheidung archiv-status)?
    /// Aus dem Kopf der Hauptseite (alte und neue Form) oder einem lesbaren
    /// `projekt.toml`; ist der Kopf kaputt, der zuletzt bekannte Stand, beim
    /// Start die Zeile `status = …` ([`ProjektStand::archiviert`]). Ein
    /// unbekanntes Projekt (etwa ein Eintrag im Datenspeicher, dessen Ordner
    /// fehlt) ist es nicht.
    pub fn archiviert(&self, slug: &str) -> bool {
        self.projekt(slug).is_some_and(|p| p.archiviert)
    }

    /// Die Slugs aller archivierten Projekte in der Liste.
    pub fn archivierte(&self) -> BTreeSet<String> {
        self.projekte.values().filter(|p| p.in_liste && self.archiviert(&p.slug)).map(|p| p.slug.clone()).collect()
    }

    /// Wie [`Self::liste`], aber ohne archivierte Projekte: das, was
    /// Seitenleiste, Schublade, Startseite und Cockpit zeigen.
    pub fn liste_sichtbar(&self) -> (Vec<&Project>, Vec<Broken>) {
        let (projekte, kaputt) = self.liste();
        (projekte.into_iter().filter(|p| !self.archiviert(&p.slug)).collect(), kaputt.into_iter().filter(|k| !self.archiviert(&k.slug)).collect())
    }

    /// Die Projektgruppen für die Oberfläche (Entscheidung
    /// projektgruppe): je Gruppe nur die Teile, die in
    /// [`Self::liste_sichtbar`] stehen (auch kaputte, nie archivierte), in der
    /// Reihenfolge aus `teile`; eine Gruppe ohne solche Teile fehlt. Ohne
    /// lesbare `data/gruppen.toml` keine Gruppen.
    pub fn gruppen_sichtbar(&self) -> Vec<GruppeAnzeige> {
        let Ok(Some(g)) = &self.gruppen else { return Vec::new() };
        let (projekte, kaputt) = self.liste_sichtbar();
        let da: BTreeSet<&str> = projekte.iter().map(|p| p.slug.as_str()).chain(kaputt.iter().map(|k| k.slug.as_str())).collect();
        g.sichtbar(|t| da.contains(t), |slug| self.projekte.contains_key(slug))
    }

    /// Sätze zu Gruppen, die [`Self::gruppen_sichtbar`] wegen ihres Slugs
    /// weglässt (ungültig, oder ein Projekt heißt so, auch ein archiviertes).
    pub fn gruppen_abgelehnt(&self) -> Vec<String> {
        let Ok(Some(g)) = &self.gruppen else { return Vec::new() };
        g.abgelehnt(|slug| self.projekte.contains_key(slug))
    }

    /// Wie `seite::load_one`.
    pub fn haupt(&self, slug: &str) -> Result<Option<&Project>, String> {
        match self.projekt(slug) {
            None => Ok(None),
            Some(p) => p.haupt.as_ref().map(Option::as_ref).map_err(Clone::clone),
        }
    }

    /// Wie `seite::kopf_lesen` (nur, wenn `projekt.toml` da ist).
    pub fn kopf(&self, slug: &str) -> Option<KopfErgebnis> {
        self.projekt(slug)?.kopf.clone()
    }

    /// Wie `seite::daten_lesen`; ein unbekanntes Projekt hat keine Daten.
    pub fn daten(&self, slug: &str, name: &str) -> Result<Option<(DatenInhalt, toml::Table)>, LeseFehler> {
        match self.projekt(slug) {
            Some(p) if seite::is_valid_slug(slug) => p.daten(name),
            _ => Ok(None),
        }
    }

    /// Gibt es den Projektordner (auch als Verknüpfung)?
    pub fn ordner_da(&self, slug: &str) -> bool {
        self.projekt(slug).is_some()
    }

    /// Alle Repos aller Projekte in der Liste (für den Git-Zwischenspeicher).
    pub fn repos(&self) -> Vec<(String, Vec<seite::RepoRef>)> {
        self.liste().0.into_iter().map(|p| (p.slug.clone(), p.meta.repos.clone())).collect()
    }

    /// Lesefehler, deren letzter guter Stand gerade gilt, als Funde je Projekt
    /// (für /api/pruefung); Dateien aus `data/` unter `dashboard`.
    pub fn lesefehler_befunde(&self) -> Vec<(String, Befund)> {
        let satz = |h: &LeseHinweis, gut: Option<&str>| {
            let anfang = format!("{}. Die Datei ist gesperrt oder ohne Leserechte (seit {}).", h.satz.trim_end_matches('.'), h.seit);
            match gut {
                Some(gelesen) => format!("{anfang} Das Dashboard zeigt den letzten guten Stand (gelesen {gelesen}) und versucht es alle 2 s wieder."),
                None => format!("{anfang} Einen guten Stand gibt es noch nicht; das Dashboard versucht es alle 2 s wieder."),
            }
        };
        let mut aus = Vec::new();
        for p in self.projekte.values() {
            let gelesen = p.gelesen.format("%d.%m. %H:%M").to_string();
            for h in &p.lesefehler {
                aus.push((p.slug.clone(), Befund::fehler(h.datei.clone(), None, satz(h, p.gut.then_some(gelesen.as_str())))));
            }
        }
        for h in &self.daten_lesefehler {
            aus.push(("dashboard".to_string(), Befund::fehler(h.datei.clone(), None, satz(h, Some("vorher")))));
        }
        aus
    }
}

/// Was ein Abgleich gefunden hat.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Aenderungen {
    /// Alle geänderten, neuen und verschwundenen Dateien (`projects/…`, `data/…`),
    /// für das Ereignis `change` wie bisher.
    pub pfade: Vec<String>,
    /// Neu gelesene Projekte mit ihrer neuen Version.
    pub projekte: Vec<(String, u64)>,
    /// Neu gelesene Dateien aus `data/` (Name ohne `.toml`) mit Version.
    pub daten: Vec<(String, u64)>,
}

/// Der letzte Blick auf die Platte. Die Sperre darauf hält auch zwei
/// Abgleiche auseinander.
#[derive(Default)]
struct Blick {
    geladen: bool,
    stempel: BTreeMap<String, Stempel>,
    /// Projekte und Dateien aus `data/` mit Lesefehler: beim nächsten Blick wieder lesen.
    nachlesen: BTreeSet<String>,
    daten_nachlesen: BTreeSet<String>,
    /// Mehrere Quellen: Slug → Elternordner beim letzten Blick (für Projekte einer Quelle, die unlesbar wird).
    eltern: BTreeMap<String, PathBuf>,
}

pub struct ContentStore {
    projekte: PathBuf,
    /// Etappe 4b: alle Inhaltsquellen (`quellen`), wenn es mehr als eine ist;
    /// sonst leer, und alles gilt wie vorher mit `projekte`.
    quellen: Vec<PathBuf>,
    daten_ordner: PathBuf,
    pc_datei: PathBuf,
    schwerpunkte_datei: PathBuf,
    /// `data/gruppen.toml`: im Ordner von `schwerpunkte_datei`.
    gruppen_datei: PathBuf,
    modus: Abgleich,
    bestand: RwLock<Arc<Bestand>>,
    blick: Mutex<Blick>,
    ereignisse: broadcast::Sender<InhaltEreignis>,
}

/// Sperre holen; eine vergiftete Sperre (ein Faden ist darin abgestürzt)
/// kommt ins Protokoll, und es geht mit dem Inhalt weiter.
fn sperren<'a, T>(m: &'a Mutex<T>, was: &str) -> MutexGuard<'a, T> {
    m.lock().unwrap_or_else(|e| {
        warn!("ContentStore: Sperre „{was}“ war vergiftet (ein Faden ist darin abgestürzt); es geht mit dem letzten Inhalt weiter.");
        e.into_inner()
    })
}

fn jetzt_text() -> String {
    Local::now().format("%d.%m. %H:%M").to_string()
}

/// Liegt `prefix` in `projects/<slug>/daten/` (oder tiefer)?
fn in_daten(prefix: &str) -> bool {
    prefix.strip_prefix("projects/").and_then(|r| r.split_once('/')).is_some_and(|(_, rest)| rest.starts_with("daten/"))
}

/// Sammelt Zeitstempel und Größe aller Dateien, die der Beobachter kennt:
/// `.md` (auch `.MD`, wie `pruefung::markdown_dateien`) und `.toml`, dazu
/// alles unter `assets/`. Unter `projects/<slug>/daten/` jeden Eintrag,
/// auch Ordner, Verknüpfungen und fremde Dateien (die Prüfung meldet sie),
/// dazu den Ordner `daten` selbst.
pub fn scan(dir: &Path, prefix: &str, out: &mut BTreeMap<String, Stempel>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let daten = in_daten(prefix);
    let projektordner = prefix.strip_prefix("projects/").is_some_and(|r| r.matches('/').count() == 1);
    for e in rd.flatten() {
        let p = e.path();
        let Ok(meta) = e.metadata() else { continue };
        let name = e.file_name().to_string_lossy().to_string();
        if daten || (projektordner && name == "daten") {
            // Ordner nur mit ihrem Dasein: Windows trägt ihre Zeit im
            // Elternordner verzögert nach, das gäbe falsche Änderungen. Was
            // in ihnen liegt, stempelt der Abstieg darunter.
            let stempel = if meta.is_dir() { (SystemTime::UNIX_EPOCH, 0) } else { (meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), meta.len()) };
            out.insert(format!("{prefix}{name}"), stempel);
        }
        if meta.is_dir() {
            scan(&p, &format!("{prefix}{name}/"), out);
        } else if name.to_ascii_lowercase().ends_with(".md") || name.ends_with(".toml") || prefix.contains("/assets/") {
            let stamp = (meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), meta.len());
            out.insert(format!("{prefix}{name}"), stamp);
        }
    }
}

/// Das Projekt, dessen Inhalt eine geänderte Datei betrifft: `projekt.toml`
/// und alles in `daten/`. Assets und Server-Teile nicht.
pub fn inhalt_slug(pfad: &str) -> Option<&str> {
    let (slug, rest) = pfad.strip_prefix("projects/")?.split_once('/')?;
    (seite::is_valid_slug(slug) && (rest == "projekt.toml" || rest == "daten" || rest.starts_with("daten/"))).then_some(slug)
}

/// Name einer Datei `data/<name>.toml`.
pub fn daten_name(pfad: &str) -> Option<&str> {
    let name = pfad.strip_prefix("data/")?.strip_suffix(".toml")?;
    (!name.is_empty() && !name.contains('/')).then_some(name)
}

/// `data/schwerpunkte.toml` und ob das Lesen an einer Sperre scheiterte.
fn schwerpunkte_lesen(datei: &Path) -> (Result<Option<Schwerpunkte>, LeseFehler>, bool) {
    let (ergebnis, gesperrt) = seite::mit_merkliste(|| match std::fs::symlink_metadata(datei) {
        Err(_) => Ok(None),
        Ok(_) => seite::neue_datei_lesen(datei, schwerpunkte::DATEI).and_then(|t| Schwerpunkte::lesen(&t)).map(Some),
    });
    (ergebnis, !gesperrt.is_empty())
}

/// `data/gruppen.toml` und ob das Lesen an einer Sperre scheiterte. Fehlt
/// die Datei: `Ok(None)` (keine Gruppen, kein Fehler).
fn gruppen_lesen(datei: &Path) -> (Result<Option<Gruppen>, LeseFehler>, bool) {
    let (ergebnis, gesperrt) = seite::mit_merkliste(|| match std::fs::symlink_metadata(datei) {
        Err(_) => Ok(None),
        Ok(_) => seite::neue_datei_lesen(datei, gruppen::DATEI).and_then(|t| Gruppen::lesen(&t)).map(Some),
    });
    (ergebnis, !gesperrt.is_empty())
}

/// `data/pc.toml` und ob das Lesen an einer Sperre scheiterte. Die Datei
/// wird genau einmal gelesen: Über die Sperre entscheidet der `io::Error`
/// dieses Lesens (nie bei einer Verknüpfung), der Satz kommt aus demselben
/// Lesen (`pcdata::lesefehler_satz`, sonst `pcdata::lesen`).
fn pc_lesen(datei: &Path) -> (Result<PcData, String>, bool) {
    match std::fs::read_to_string(datei) {
        Err(e) => {
            let gesperrt = ist_gesperrt(&e) && !seite::ist_verknuepfung(datei);
            (Err(pcdata::lesefehler_satz(&e)), gesperrt)
        }
        Ok(text) => (pcdata::lesen(&text).map_err(|e| e.to_string()), false),
    }
}

impl ContentStore {
    /// Ein Speicher für `projekte` mit `data/pc.toml` und
    /// `data/schwerpunkte.toml` (beobachtet wird der Ordner von `pc_datei`).
    /// Liest noch nichts; das tut der erste Zugriff oder [`Self::laden`].
    pub fn neu(projekte: PathBuf, pc_datei: PathBuf, schwerpunkte_datei: PathBuf, modus: Abgleich) -> Self {
        let daten_ordner = pc_datei.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("data"));
        let gruppen_datei = schwerpunkte_datei.with_file_name("gruppen.toml");
        Self {
            projekte,
            quellen: Vec::new(),
            daten_ordner,
            pc_datei,
            schwerpunkte_datei,
            gruppen_datei,
            modus,
            bestand: RwLock::new(Arc::new(Bestand::leer(abgeleitet::heute()))),
            blick: Mutex::new(Blick::default()),
            ereignisse: broadcast::channel(256).0,
        }
    }

    /// Wie `neu`, und liest gleich alles (blockiert; beim Start des Dienstes).
    pub fn laden(projekte: PathBuf, pc_datei: PathBuf, schwerpunkte_datei: PathBuf, modus: Abgleich) -> Self {
        let s = Self::neu(projekte, pc_datei, schwerpunkte_datei, modus);
        s.abgleichen();
        s
    }

    /// Wie `laden`, aus mehreren Inhaltsquellen (Etappe 4b). Mit einer Quelle
    /// genau wie `laden` mit ihrem Projektordner.
    pub fn laden_aus(quellen: &crate::quellen::Quellen, pc_datei: PathBuf, schwerpunkte_datei: PathBuf, modus: Abgleich) -> Self {
        let mut s = Self::neu(quellen.erste(), pc_datei, schwerpunkte_datei, modus);
        if !quellen.einzeln() {
            s.quellen = quellen.wurzeln();
        }
        s.abgleichen();
        s
    }

    /// Mehrere Quellen: ihre Projekte (Slug → Elternordner). `None` bei einer.
    fn orte(&self) -> Option<crate::quellen::Quellen> {
        (!self.quellen.is_empty()).then(|| crate::quellen::Quellen::aus_liste(self.quellen.clone()))
    }

    /// Sätze zu abweichenden Versionsdateien, auch bei nur einer Quelle (eine
    /// Datei lesen, meist fehlt sie). Unlesbar ist eine einzige Quelle schon
    /// über `ordner_lesbar` gemeldet, darum gibt es dafür hier keinen Satz.
    fn version_saetze(&self, orte: &Option<crate::quellen::Quellen>) -> Vec<String> {
        match orte {
            Some(q) => q.version.clone(),
            None => crate::quellen::version_ordner(&self.projekte),
        }
    }

    /// Der Elternordner eines Projekts (bei einer Quelle immer `projekte`).
    fn eltern_von(&self, orte: &Option<crate::quellen::Quellen>, slug: &str) -> PathBuf {
        orte.as_ref().and_then(|q| q.eltern(slug)).map(Path::to_path_buf).unwrap_or_else(|| self.projekte.clone())
    }

    pub fn ordner(&self) -> &Path {
        &self.projekte
    }

    pub fn modus(&self) -> Abgleich {
        self.modus
    }

    /// Für den Live-Kanal: die Ereignisse `projekt`, `daten` und `inhalt`.
    pub fn abonnieren(&self) -> broadcast::Receiver<InhaltEreignis> {
        self.ereignisse.subscribe()
    }

    /// Für Tests: ein Ereignis schicken, als hätte sich der Inhalt geändert.
    #[cfg(test)]
    pub fn senden_fuer_tests(&self, e: InhaltEreignis) {
        let _ = self.ereignisse.send(e);
    }

    /// Der aktuelle Stand. Im Testbetrieb gleicht jeder Zugriff erst ab. Im
    /// Dienst nie: Das tut der Beobachter alle 2 s (auch am Tageswechsel, dann
    /// mit den Projekten mit Kennzahlen); ein Zugriff liest nie von der Platte.
    pub fn bestand(&self) -> Arc<Bestand> {
        if self.modus == Abgleich::BeiJedemZugriff {
            self.abgleichen();
        }
        self.lesen_ohne_abgleich()
    }

    fn lesen_ohne_abgleich(&self) -> Arc<Bestand> {
        self.bestand
            .read()
            .unwrap_or_else(|e| {
                warn!("ContentStore: Lesesperre war vergiftet; es geht mit dem letzten Stand weiter.");
                e.into_inner()
            })
            .clone()
    }

    /// Ändert den Bestand unter der Schreibsperre (nur kurz: gelesen wird vorher).
    fn aendern(&self, f: impl FnOnce(&mut Bestand)) {
        let mut g = self.bestand.write().unwrap_or_else(|e| {
            warn!("ContentStore: Schreibsperre war vergiftet; es geht mit dem letzten Stand weiter.");
            e.into_inner()
        });
        let mut neu = (**g).clone();
        f(&mut neu);
        *g = Arc::new(neu);
    }

    /// Vergleicht die Platte mit dem letzten Blick und liest neu, was sich
    /// geändert hat (beim ersten Mal alles), dazu alles mit Lesefehler vom
    /// letzten Mal. Version und Ereignis nur, wenn sich der übernommene Stand
    /// wirklich geändert hat; ein Lesefehler, der bleibt, wird still wieder
    /// versucht. Gibt zurück, was sich geändert hat. Blockiert (std::fs): im
    /// Dienst nur aus `spawn_blocking`.
    pub fn abgleichen(&self) -> Aenderungen {
        let mut blick = sperren(&self.blick, "Blick");
        let heute = abgeleitet::heute();
        if !blick.geladen {
            self.alles_laden(&mut blick, heute);
            return Aenderungen::default();
        }
        if std::fs::read_dir(&self.projekte).is_err() {
            // Ein unlesbarer Projektordner hieße sonst „alle Projekte weg“.
            if self.lesen_ohne_abgleich().ordner_lesbar {
                warn!("ContentStore: {} lässt sich nicht lesen; der letzte gute Stand bleibt.", self.projekte.display());
                self.aendern(|b| b.ordner_lesbar = false);
            }
            return Aenderungen::default();
        }
        let orte = self.orte();
        // Projekte einer unlesbaren weiteren Quelle: nicht neu auflösen, der letzte gute Stand bleibt (wie beim ersten Ordner).
        let gesperrt = orte.as_ref().map(crate::quellen::Quellen::unlesbare_pfade).unwrap_or_default();
        let alte_eltern = std::mem::take(&mut blick.eltern);
        let in_gesperrter = |s: &str| alte_eltern.get(s).is_some_and(|e| gesperrt.iter().any(|g| *g == *e || *g == e.join(s)));
        blick.eltern = orte.as_ref().map(|q| q.projekte().clone()).unwrap_or_default();
        blick.eltern.extend(alte_eltern.iter().filter(|(s, _)| in_gesperrter(s)).map(|(s, e)| (s.clone(), e.clone())));
        let unlesbar = orte.as_ref().map(crate::quellen::Quellen::unlesbar_saetze).unwrap_or_default();
        let version = self.version_saetze(&orte);
        let stempel = self.stempel_lesen_aus(&orte);
        let mut pfade: Vec<String> = Vec::new();
        for (pfad, s) in &stempel {
            if blick.stempel.get(pfad) != Some(s) {
                pfade.push(pfad.clone());
            }
        }
        for pfad in blick.stempel.keys() {
            if !stempel.contains_key(pfad) {
                pfade.push(pfad.clone());
            }
        }
        blick.stempel = stempel;

        let alt = self.lesen_ohne_abgleich();
        let mut slugs: BTreeSet<String> = pfade.iter().filter_map(|p| inhalt_slug(p)).map(str::to_string).collect();
        slugs.extend(std::mem::take(&mut blick.nachlesen));
        if alt.tag != heute {
            slugs.extend(alt.projekte.values().filter(|p| p.haengt_vom_tag_ab()).map(|p| p.slug.clone()));
        }
        // Auch was beim Sperren verschwand oder danach aus einer anderen Quelle kommt: die Stempel tragen die Quelle nicht.
        slugs.extend(alte_eltern.iter().filter(|(s, e)| blick.eltern.get(*s) != Some(*e)).map(|(s, _)| s.clone()));
        slugs.retain(|s| !in_gesperrter(s));
        let mut namen: BTreeSet<String> = pfade.iter().filter_map(|p| daten_name(p)).map(str::to_string).collect();
        namen.extend(std::mem::take(&mut blick.daten_nachlesen));

        if slugs.is_empty() && namen.is_empty() && alt.tag == heute && alt.ordner_lesbar && alt.quellen_unlesbar == unlesbar && alt.quellen_version == version {
            return Aenderungen { pfade, ..Default::default() };
        }

        // Erst lesen (ohne Sperre auf dem Bestand), dann kurz übernehmen.
        let gelesen: Vec<(String, Option<ProjektStand>)> =
            slugs.iter().map(|s| (s.clone(), ProjektStand::lesen(&self.eltern_von(&orte, s), s, heute))).collect();
        let schwerpunkte_name = self.name_in_daten(&self.schwerpunkte_datei);
        let pc_name = self.name_in_daten(&self.pc_datei);
        let neue_schwerpunkte = namen.contains(&schwerpunkte_name).then(|| schwerpunkte_lesen(&self.schwerpunkte_datei));
        let gruppen_name = self.name_in_daten(&self.gruppen_datei);
        let neue_gruppen = namen.contains(&gruppen_name).then(|| gruppen_lesen(&self.gruppen_datei));
        let neues_pc = namen.contains(&pc_name).then(|| pc_lesen(&self.pc_datei));

        let mut aenderungen = Aenderungen { pfade, ..Default::default() };
        let mut nachlesen = BTreeSet::new();
        let mut daten_nachlesen = BTreeSet::new();
        self.aendern(|b| {
            let v = b.version + 1;
            b.tag = heute;
            b.ordner_lesbar = true;
            for (slug, neu) in gelesen {
                match neu {
                    None => {
                        if b.projekte.remove(&slug).is_some() {
                            debug!("ContentStore: Projekt {slug} ist weg");
                            b.versionen.insert(slug.clone(), v);
                            aenderungen.projekte.push((slug, v));
                        }
                    }
                    Some(neu) => {
                        let (stand, wieder, geaendert) = uebernehmen(b.projekte.get(&slug), neu);
                        if wieder {
                            nachlesen.insert(slug.clone());
                        }
                        b.projekte.insert(slug.clone(), stand);
                        if geaendert {
                            b.versionen.insert(slug.clone(), v);
                            aenderungen.projekte.push((slug, v));
                        }
                    }
                }
            }
            // Behalten (gesperrt): kein Ereignis, still wieder versuchen.
            let mut behalten = BTreeSet::new();
            if let Some((neu, zu)) = neue_schwerpunkte {
                match neu {
                    Err(e) if zu => {
                        daten_lesefehler(&mut b.daten_lesefehler, &e.datei, &e.text);
                        daten_nachlesen.insert(schwerpunkte_name.clone());
                        behalten.insert(schwerpunkte_name.clone());
                    }
                    neu => {
                        b.daten_lesefehler.retain(|h| h.datei != schwerpunkte::DATEI);
                        b.schwerpunkte = neu;
                    }
                }
            }
            if let Some((neu, zu)) = neue_gruppen {
                match neu {
                    Err(e) if zu => {
                        daten_lesefehler(&mut b.daten_lesefehler, &e.datei, &e.text);
                        daten_nachlesen.insert(gruppen_name.clone());
                        behalten.insert(gruppen_name.clone());
                    }
                    neu => {
                        b.daten_lesefehler.retain(|h| h.datei != gruppen::DATEI);
                        b.gruppen = neu;
                    }
                }
            }
            if let Some((neu, zu)) = neues_pc {
                let datei = format!("data/{pc_name}.toml");
                match neu {
                    Err(e) if zu => {
                        daten_lesefehler(&mut b.daten_lesefehler, &datei, &e);
                        daten_nachlesen.insert(pc_name.clone());
                        behalten.insert(pc_name.clone());
                    }
                    neu => {
                        b.daten_lesefehler.retain(|h| h.datei != datei);
                        b.pc = neu;
                    }
                }
            }
            for name in namen.iter().filter(|n| !behalten.contains(*n)) {
                b.daten_versionen.insert(name.clone(), v);
                aenderungen.daten.push((name.clone(), v));
            }
            b.quellen_doppelt = orte.as_ref().map(|q| q.doppelt.clone()).unwrap_or_default();
            b.quellen_rechenteile = orte.as_ref().map(|q| q.rechenteile.clone()).unwrap_or_default();
            b.quellen_unlesbar = unlesbar;
            b.quellen_version = version;
            if !aenderungen.projekte.is_empty() || !aenderungen.daten.is_empty() {
                b.version = v;
            }
        });
        blick.nachlesen = nachlesen;
        blick.daten_nachlesen = daten_nachlesen;
        drop(blick);
        self.melden(&aenderungen);
        aenderungen
    }

    /// Name ohne `.toml`, wenn die Datei im beobachteten Ordner `data/` liegt.
    fn name_in_daten(&self, datei: &Path) -> String {
        match (datei.parent(), datei.file_stem()) {
            (Some(o), Some(n)) if o == self.daten_ordner => n.to_string_lossy().into_owned(),
            _ => String::new(),
        }
    }

    fn stempel_lesen_aus(&self, orte: &Option<crate::quellen::Quellen>) -> BTreeMap<String, Stempel> {
        let mut m = BTreeMap::new();
        match orte {
            // Mehrere Quellen: je Projekt sein Ordner, unter demselben Schlüssel `projects/<slug>/…`.
            Some(q) => {
                for (slug, eltern) in q.projekte() {
                    scan(&eltern.join(slug), &format!("projects/{slug}/"), &mut m);
                }
            }
            None => scan(&self.projekte, "projects/", &mut m),
        }
        scan(&self.daten_ordner, "data/", &mut m);
        m
    }

    /// Das erste volle Lesen: alle Projektordner und beide Dateien aus `data/`.
    /// Was gesperrt war, wird beim nächsten Blick wieder gelesen.
    fn alles_laden(&self, blick: &mut Blick, heute: NaiveDate) {
        let start = Instant::now();
        let orte = self.orte();
        blick.stempel = self.stempel_lesen_aus(&orte);
        blick.eltern = orte.as_ref().map(|q| q.projekte().clone()).unwrap_or_default();
        let lesbar = std::fs::read_dir(&self.projekte).is_ok();
        let mut slugs: Vec<String> = match &orte {
            Some(q) => {
                for satz in &q.doppelt {
                    warn!("Inhaltsquellen: {satz}");
                }
                q.projekte().keys().cloned().collect()
            }
            None => std::fs::read_dir(&self.projekte)
                .map(|rd| rd.filter_map(|e| e.ok()).filter_map(|e| e.file_name().into_string().ok()).filter(|s| seite::is_valid_slug(s)).collect())
                .unwrap_or_default(),
        };
        slugs.sort();
        let mut projekte: BTreeMap<String, Arc<ProjektStand>> = BTreeMap::new();
        for p in slugs.iter().filter_map(|s| ProjektStand::lesen(&self.eltern_von(&orte, s), s, heute)) {
            let (stand, wieder, _) = uebernehmen(None, p);
            if wieder {
                blick.nachlesen.insert(stand.slug.clone());
            }
            projekte.insert(stand.slug.clone(), stand);
        }
        let (schwerpunkte, schwerpunkte_gesperrt) = schwerpunkte_lesen(&self.schwerpunkte_datei);
        let (gruppen, gruppen_gesperrt) = gruppen_lesen(&self.gruppen_datei);
        let (pc, pc_gesperrt) = pc_lesen(&self.pc_datei);
        let mut hinweise = Vec::new();
        if let Err(e) = &schwerpunkte
            && schwerpunkte_gesperrt
        {
            daten_lesefehler(&mut hinweise, &e.datei, &e.text);
            blick.daten_nachlesen.insert(self.name_in_daten(&self.schwerpunkte_datei));
        }
        if let Err(e) = &gruppen
            && gruppen_gesperrt
        {
            daten_lesefehler(&mut hinweise, &e.datei, &e.text);
            blick.daten_nachlesen.insert(self.name_in_daten(&self.gruppen_datei));
        }
        if let Err(e) = &pc
            && pc_gesperrt
        {
            daten_lesefehler(&mut hinweise, &format!("data/{}.toml", self.name_in_daten(&self.pc_datei)), e);
            blick.daten_nachlesen.insert(self.name_in_daten(&self.pc_datei));
        }
        let ms = start.elapsed().as_millis() as u64;
        let anzahl = projekte.len();
        self.aendern(|b| {
            b.version += 1;
            let v = b.version;
            b.versionen = projekte.keys().map(|s| (s.clone(), v)).collect();
            b.projekte = projekte;
            b.schwerpunkte = schwerpunkte;
            b.gruppen = gruppen;
            b.pc = pc;
            b.daten_lesefehler = hinweise;
            b.tag = heute;
            b.ordner_lesbar = lesbar;
            b.geladen_in_ms = ms;
            b.quellen_doppelt = orte.as_ref().map(|q| q.doppelt.clone()).unwrap_or_default();
            b.quellen_rechenteile = orte.as_ref().map(|q| q.rechenteile.clone()).unwrap_or_default();
            b.quellen_unlesbar = orte.as_ref().map(crate::quellen::Quellen::unlesbar_saetze).unwrap_or_default();
            b.quellen_version = self.version_saetze(&orte);
        });
        blick.geladen = true;
        if self.modus == Abgleich::Beobachter {
            info!("ContentStore: {anzahl} Projekte in {ms} ms gelesen ({})", self.projekte.display());
        }
    }

    /// Ein geänderter Git-Stand zählt wie eine Änderung an den Projekten,
    /// die das Repo nennen: neue Version und ein Ereignis (bei vielen eines).
    pub fn git_geaendert(&self, slugs: &[String]) {
        let mut aenderungen = Aenderungen::default();
        self.aendern(|b| {
            let slugs: Vec<&String> = slugs.iter().filter(|s| b.projekte.contains_key(*s)).collect();
            if slugs.is_empty() {
                return;
            }
            b.version += 1;
            let v = b.version;
            for s in slugs {
                b.versionen.insert(s.clone(), v);
                aenderungen.projekte.push((s.clone(), v));
            }
        });
        self.melden(&aenderungen);
    }

    /// Live-Lücken, Punkt 5: Eine Datei in `assets/` dieser Projekte hat sich
    /// geändert (Bild, Vorschau). Neue Version und je Projekt ein `projekt`
    /// mit `assets: true` (nie gesammelt: die Seite braucht den Slug, um die
    /// Adressen der Dateien zu erneuern), dazu die Namen der geänderten
    /// Dateien (bei mehr als `ASSET_DATEIEN_HOECHSTENS` ohne Namen: dann
    /// erneuert die Seite alle Adressen des Projekts).
    pub fn assets_geaendert(&self, geaendert: &[(String, Vec<String>)]) {
        let mut neu = Vec::new();
        self.aendern(|b| {
            let bekannt: Vec<&(String, Vec<String>)> = geaendert.iter().filter(|(s, _)| b.projekte.contains_key(s)).collect();
            if bekannt.is_empty() {
                return;
            }
            b.version += 1;
            let v = b.version;
            for (s, dateien) in bekannt {
                b.versionen.insert(s.clone(), v);
                neu.push((s.clone(), v, dateien.clone()));
            }
        });
        for (slug, version, dateien) in neu {
            debug!("ContentStore: assets/ von {slug} geändert ({} Dateien)", dateien.len());
            let dateien = (dateien.len() <= crate::api::events::ASSET_DATEIEN_HOECHSTENS).then_some(dateien);
            let _ = self.ereignisse.send(InhaltEreignis::Projekt(ProjektEreignis { slug, version, assets: Some(true), dateien }));
        }
    }

    /// Ereignisse senden: je Projekt und Datei eines, bei mehr als
    /// [`SAMMEL_AB`] Projekten ein Sammelereignis.
    fn melden(&self, a: &Aenderungen) {
        if a.projekte.is_empty() && a.daten.is_empty() {
            return;
        }
        if a.projekte.len() > SAMMEL_AB {
            let version = a.projekte.iter().chain(&a.daten).map(|(_, v)| *v).max().unwrap_or(0);
            debug!("ContentStore: Sammelereignis für {} Projekte", a.projekte.len());
            let _ = self.ereignisse.send(InhaltEreignis::Sammel(SammelEreignis {
                version,
                projekte: a.projekte.iter().map(|(s, _)| s.clone()).collect(),
                daten: a.daten.iter().map(|(n, _)| n.clone()).collect(),
            }));
            return;
        }
        for (slug, version) in &a.projekte {
            let _ = self.ereignisse.send(InhaltEreignis::Projekt(ProjektEreignis { slug: slug.clone(), version: *version, assets: None, dateien: None }));
        }
        for (name, version) in &a.daten {
            let _ = self.ereignisse.send(InhaltEreignis::Daten(DatenEreignis { name: name.clone(), version: *version }));
        }
    }
}

/// Trägt einen Lesefehler einer Datei aus `data/` ein (die Zeit des ersten bleibt).
fn daten_lesefehler(liste: &mut Vec<LeseHinweis>, datei: &str, satz: &str) {
    if !liste.iter().any(|h| h.datei == datei) {
        warn!("ContentStore: {datei}: {satz} Gesperrt; der letzte Stand bleibt, neuer Versuch alle 2 s.");
        liste.push(LeseHinweis { datei: datei.to_string(), satz: satz.to_string(), seit: jetzt_text() });
    }
}

/// Nimmt einen neu gelesenen Stand. Gibt (Stand, wieder lesen, geändert)
/// zurück.
///
/// - Ohne gesperrte Datei: der neue Stand, geändert.
/// - Gesperrt, und es gab einen Stand davor: der alte bleibt (die Lesefehler
///   vermerkt), nicht geändert, wieder lesen.
/// - Gesperrt ohne Stand davor: der neue, so wie er gelesen wurde (`gut`
///   ist dann `false`), geändert, wieder lesen.
fn uebernehmen(alt: Option<&Arc<ProjektStand>>, mut neu: ProjektStand) -> (Arc<ProjektStand>, bool, bool) {
    // Archiv-Status: Lässt sich der Kopf nicht lesen, gilt der zuletzt bekannte.
    if !neu.archiv_streng
        && let Some(a) = alt
    {
        neu.archiviert = a.archiviert;
    }
    if neu.gesperrt.is_empty() {
        if let Some(a) = alt.filter(|a| !a.lesefehler.is_empty()) {
            info!("ContentStore: {} lässt sich wieder lesen.", a.slug);
        }
        return (Arc::new(neu), false, true);
    }
    let seit = jetzt_text();
    let frueher = |datei: &str| alt.and_then(|a| a.lesefehler.iter().find(|h| h.datei == datei)).map(|h| h.seit.clone());
    let hinweise: Vec<LeseHinweis> = neu
        .gesperrt
        .iter()
        .map(|(datei, satz)| {
            let vorher = frueher(datei);
            if vorher.is_none() {
                warn!("ContentStore: {datei}: {satz} Gesperrt; neuer Versuch alle 2 s.");
            }
            LeseHinweis { datei: datei.clone(), satz: satz.clone(), seit: vorher.unwrap_or_else(|| seit.clone()) }
        })
        .collect();
    match alt {
        Some(alt) => {
            let mut behalten = (**alt).clone();
            behalten.lesefehler = hinweise;
            (Arc::new(behalten), true, false)
        }
        None => {
            neu.lesefehler = hinweise;
            (Arc::new(neu), true, true)
        }
    }
}

/// Das Wort aus der Zeile `status = "…"` in `projekt.toml`, ohne den Rest zu
/// prüfen: für ein Projekt, dessen Kopf sich nicht lesen lässt (Entscheidung archiv-status). Ohne
/// solche Zeile oder lesbare Datei `None`. Blockiert.
pub fn status_locker(ordner: &Path) -> Option<String> {
    let zeile = |text: &str| {
        text.lines().find_map(|z| {
            let (schluessel, wert) = z.trim().split_once('=')?;
            if schluessel.trim() != "status" {
                return None;
            }
            let wert = wert.split('#').next()?.trim();
            let wert = wert.strip_prefix(['"', '\''])?.strip_suffix(['"', '\''])?;
            Some(wert.trim().to_string())
        })
    };
    let pfad = ordner.join("projekt.toml");
    if seite::ist_verknuepfung(&pfad) || std::fs::metadata(&pfad).ok()?.len() > seite::TOML_HOECHSTENS {
        return None;
    }
    zeile(&std::fs::read_to_string(&pfad).ok()?)
}

#[cfg(test)]
mod tests;
