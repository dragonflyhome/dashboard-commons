//! Git-Zwischenspeicher (Zielbild 2d): der Git-Stand aller Repos aus allen
//! Projekten, im Arbeitsspeicher gehalten. `/api/projects` und
//! `/api/projects/<slug>` lesen nur von hier und starten nie selbst git.
//!
//! Erneuert wird im Hintergrund: alle 60 s alle Repos (`TAKT`), der erste Lauf
//! sofort beim Start; dazu ein Repo einzeln, wenn der Datei-Beobachter
//! (`api::events`) dort eine Änderung meldet, entprellt um 2 s (`ENTPRELLEN`).
//! Über alle Wege zusammen (Takt, Start, Änderungsmeldung, `erneuern`) laufen
//! höchstens `GLEICHZEITIG` Abfragen zugleich (eine Semaphore im Speicher), je
//! Repo höchstens eine (Sperre je Pfad); jede hat wie bisher das Budget
//! `git::BUDGET` (über den Boten dazu den Spielraum für die Pipe).
//!
//! Mit Boten (Dienst und Konsole) geht jede Abfrage über ihn
//! (`git::ueber_boten_oder_weg`), sonst direkt (`git::inspect`). Antwortet der
//! Bote nicht, behält jedes Repo seinen letzten Stand mit dessen Zeit; das
//! Protokoll sagt es einmal je Zustandswechsel, nicht bei jedem Versuch. Ist
//! der Bote nur ausgelastet, ist das eine Verzögerung: Der Stand bleibt, der
//! nächste Versuch kommt mit dem nächsten Lauf oder der nächsten Meldung, und
//! der Bote gilt nicht als weg.
//!
//! Ändert sich der Stand eines Repos (Zweig, Commit, geänderte Dateien,
//! Fehler), bekommt der `Melder` (der ContentStore, `mit_melder`) die Slugs
//! der betroffenen Projekte, gesammelt über `MELDEN_NACH`; ein Lauf über alle
//! Repos (Start, Takt, Rückkehr des Boten) meldet einmal am Ende mit allen
//! Slugs. So kommt beim Start ein Sammelereignis statt eines je Projekt.
//!
//! Etappe 2d dazu: Die Repos stammen aus dem ContentStore (`mit_inhalt`),
//! nicht aus einem eigenen Lesen aller Seiten. Pfade gelten ohne Unterschied
//! in Groß/Klein und Schrägstrichen (`schluessel`); Sperren verschwundener
//! Repos werden aufgeräumt; „schon frisch genug“ vergleicht die monotone Uhr
//! (`Instant`); vergiftete Sperren stehen im Protokoll; alle
//! Hintergrundaufgaben enden am Abschaltsignal (`mit_abschaltung`).
//!
//! Die Schnittstelle für den ContentStore (2d): `stand_fuer`, `erneuern`,
//! `alle_erneuern`, `aenderung_gemeldet`. Kein axum hier.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use tokio::sync::{Semaphore, watch};
use tracing::{debug, info, warn};

use crate::bote::client::Bote;
use crate::git::{self, RepoStatus};
use crate::inhalt::store::ContentStore;
use crate::vertrag::bote::Anfrage;

/// Eine Prüfung „ist der Bote erreichbar?“ (für `bote_beobachten`).
type Pruefung = std::pin::Pin<Box<dyn std::future::Future<Output = bool> + Send>>;
use crate::inhalt::seite::{Project, RepoRef};

/// Abstand der Läufe über alle Repos.
pub const TAKT: Duration = Duration::from_secs(60);
/// So lange nach einer gemeldeten Änderung wird ein Repo erneuert; weitere
/// Meldungen in dieser Zeit fallen mit hinein.
pub const ENTPRELLEN: Duration = Duration::from_secs(2);
/// Höchstens so viele Repos werden zugleich abgefragt (über alle Wege).
pub const GLEICHZEITIG: usize = 2;
/// Abstand, in dem `bote_beobachten` nachsieht, ob der Bote erreichbar ist.
pub const BOTE_TAKT: Duration = Duration::from_secs(2);
/// So lange nach einem geänderten Stand geht die Meldung hinaus; weitere
/// Änderungen in dieser Zeit (in allen Projekten) fallen mit hinein.
pub const MELDEN_NACH: Duration = Duration::from_millis(500);

/// Bekommt die Slugs der Projekte, deren Git-Stand sich geändert hat.
pub type Melder = Arc<dyn Fn(&[String]) + Send + Sync>;

/// Ein Repo mit dem Zeitpunkt, zu dem seine Abfrage begann.
#[derive(Debug, Clone)]
pub struct Stand {
    pub status: RepoStatus,
    pub zeit: DateTime<Local>,
    /// Dasselbe auf der monotonen Uhr (für „schon frisch genug“; die Wanduhr
    /// kann springen).
    pub begonnen: Instant,
}

/// Woher der Git-Stand kommt.
enum Quelle {
    /// Altweg und Tests: git direkt (`git::inspect`).
    Direkt,
    /// Dienst und Konsole: über den Boten.
    Bote(Arc<Bote>),
    /// Nur Tests: vorgegebene Antworten nach einer festen Dauer.
    #[cfg(test)]
    Attrappe(Arc<Attrappe>),
}

/// Nur Tests: Antworten je Pfad (sonst ein leerer Stand), jede nach `dauer`.
#[cfg(test)]
pub(crate) struct Attrappe {
    pub antworten: Mutex<HashMap<String, Result<RepoStatus, git::BoteWeg>>>,
    pub dauer: Duration,
}

/// Was zuletzt über den Boten bekannt ist (für eine Warnung je Wechsel).
const BOTE_UNBEKANNT: u8 = 0;
const BOTE_DA: u8 = 1;
const BOTE_WEG: u8 = 2;

/// Sperre holen; eine vergiftete (ein Faden ist darin abgestürzt) kommt ins
/// Protokoll, und es geht mit dem Inhalt weiter.
fn sperren<'a, T>(m: &'a Mutex<T>, was: &str) -> MutexGuard<'a, T> {
    m.lock().unwrap_or_else(|e| {
        warn!("Git-Zwischenspeicher: Sperre „{was}“ war vergiftet; es geht mit dem letzten Inhalt weiter.");
        e.into_inner()
    })
}

fn lesen<'a, T>(m: &'a RwLock<T>, was: &str) -> RwLockReadGuard<'a, T> {
    m.read().unwrap_or_else(|e| {
        warn!("Git-Zwischenspeicher: Lesesperre „{was}“ war vergiftet; es geht mit dem letzten Inhalt weiter.");
        e.into_inner()
    })
}

fn schreiben<'a, T>(m: &'a RwLock<T>, was: &str) -> RwLockWriteGuard<'a, T> {
    m.write().unwrap_or_else(|e| {
        warn!("Git-Zwischenspeicher: Schreibsperre „{was}“ war vergiftet; es geht mit dem letzten Inhalt weiter.");
        e.into_inner()
    })
}

/// Schlüssel eines Repo-Pfads: ohne Unterschied zwischen `\` und `/`, ohne
/// Schrägstrich am Ende, unter Windows ohne Groß/Klein. Zwei Projekte, die
/// dasselbe Repo verschieden schreiben, teilen sich so Stand und Abfrage.
pub fn schluessel(pfad: &str) -> String {
    let p = pfad.trim().replace('\\', "/");
    let p = p.trim_end_matches('/');
    if cfg!(windows) { p.to_lowercase() } else { p.to_string() }
}

/// Ein Repo, das Projekte nennen: der Pfad so, wie das erste Projekt ihn
/// schreibt (damit wird gefragt), und ob es privat ist.
#[derive(Debug, Clone, PartialEq)]
struct Ziel {
    pfad: String,
    privat: bool,
}

/// Sammelt Projekte mit geändertem Stand und meldet sie gesammelt.
struct Meldestelle {
    melder: Option<Melder>,
    ausstehend: Mutex<BTreeSet<String>>,
}

impl Meldestelle {
    /// Die erste Meldung öffnet ein Fenster von `MELDEN_NACH`; alles, was bis
    /// dahin kommt (aus allen Projekten), geht in einem Aufruf hinaus.
    fn melden(self: &Arc<Self>, slugs: Vec<String>) {
        let Some(melder) = self.melder.clone() else { return };
        if slugs.is_empty() {
            return;
        }
        let oeffnen = {
            let mut a = sperren(&self.ausstehend, "Meldestelle");
            let war_leer = a.is_empty();
            a.extend(slugs);
            war_leer
        };
        if !oeffnen {
            return;
        }
        let ich = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(MELDEN_NACH).await;
            let alle: Vec<String> = std::mem::take(&mut *sperren(&ich.ausstehend, "Meldestelle")).into_iter().collect();
            if !alle.is_empty() {
                debug!("Git-Stand geändert in {alle:?}, an den ContentStore");
                melder(&alle);
            }
        });
    }
}

/// Zieht beim Ende einer Abfrage eins vom Zähler ab.
struct Abzug<'a>(&'a AtomicUsize);

impl Drop for Abzug<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Gleicher Stand für die Oberfläche: Zweig, Commit, geänderte Dateien und
/// Fehler (Name, „privat“ und Zeit zählen nicht).
fn gleicher_stand(a: &RepoStatus, b: &RepoStatus) -> bool {
    a.branch == b.branch
        && a.commit_hash == b.commit_hash
        && a.commit_date == b.commit_date
        && a.commit_subject == b.commit_subject
        && a.dirty_files == b.dirty_files
        && a.error == b.error
}

pub struct Zwischenspeicher {
    /// Die Inhaltsquellen (Etappe 4b, Umzug des Inhalts); nur ohne ContentStore gelesen.
    quellen: Vec<PathBuf>,
    /// Woher die Repos kommen (Etappe 2d); ohne ihn liest `ziele_lesen` die Seiten selbst.
    inhalt: Option<Arc<ContentStore>>,
    quelle: Quelle,
    /// Diese und die folgenden Karten sind nach `schluessel(pfad)` geordnet.
    staende: RwLock<HashMap<String, Stand>>,
    /// Alle Repos aus allen Projekten (privat, wenn es irgendein Projekt so
    /// führt; dann fragt niemand Commit-Texte ab).
    ziele: RwLock<BTreeMap<String, Ziel>>,
    /// Repo → Slugs der Projekte, die es nennen.
    slugs: RwLock<HashMap<String, Vec<String>>>,
    sperren: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    /// `GLEICHZEITIG` Plätze für alle Abfragen, gleich auf welchem Weg.
    plaetze: Semaphore,
    /// Repos mit einer gemeldeten Änderung, deren Erneuerung noch wartet.
    ausstehend: Mutex<HashSet<String>>,
    bote_lage: AtomicU8,
    meldung: Arc<Meldestelle>,
    /// Abschaltsignal: Hintergrundaufgaben enden, sobald es `true` ist.
    abschalten: Option<watch::Receiver<bool>>,
    /// Laufende Abfragen und das bisherige Höchste (Tests, Fehlersuche).
    laufend: AtomicUsize,
    hoechst_zugleich: AtomicUsize,
}

impl Zwischenspeicher {
    /// Leerer Speicher für die Projekte aller Inhaltsquellen `quellen`; mit
    /// Boten fragt er über ihn. Erneuert nichts, bevor jemand `starten` oder
    /// `erneuern` ruft.
    pub fn neu(quellen: Vec<PathBuf>, bote: Option<Arc<Bote>>) -> Self {
        Self::mit_quelle(quellen, bote.map(Quelle::Bote).unwrap_or(Quelle::Direkt))
    }

    fn mit_quelle(quellen: Vec<PathBuf>, quelle: Quelle) -> Self {
        Self {
            quellen,
            inhalt: None,
            quelle,
            staende: Default::default(),
            ziele: Default::default(),
            slugs: Default::default(),
            sperren: Default::default(),
            plaetze: Semaphore::new(GLEICHZEITIG),
            ausstehend: Default::default(),
            bote_lage: AtomicU8::new(BOTE_UNBEKANNT),
            meldung: Arc::new(Meldestelle { melder: None, ausstehend: Default::default() }),
            abschalten: None,
            laufend: AtomicUsize::new(0),
            hoechst_zugleich: AtomicUsize::new(0),
        }
    }

    /// Mit einem Melder für geänderte Stände (der ContentStore).
    pub fn mit_melder(mut self, melder: Melder) -> Self {
        self.meldung = Arc::new(Meldestelle { melder: Some(melder), ausstehend: Default::default() });
        self
    }

    /// Die Repos aus dem ContentStore statt aus einem eigenen Lesen.
    pub fn mit_inhalt(mut self, inhalt: Arc<ContentStore>) -> Self {
        self.inhalt = Some(inhalt);
        self
    }

    /// Mit Abschaltsignal: `starten`, `bote_beobachten` und die wartenden
    /// Erneuerungen enden, sobald es `true` ist.
    pub fn mit_abschaltung(mut self, rx: watch::Receiver<bool>) -> Self {
        self.abschalten = Some(rx);
        self
    }

    /// Wird fertig, sobald abgeschaltet wird (ohne Signal nie).
    fn abschaltung(&self) -> impl std::future::Future<Output = ()> + Send + 'static + use<> {
        let rx = self.abschalten.clone();
        async move {
            match rx {
                Some(mut rx) => {
                    if rx.wait_for(|&aus| aus).await.is_err() {
                        std::future::pending::<()>().await;
                    }
                }
                None => std::future::pending::<()>().await,
            }
        }
    }

    /// Hintergrundaufgabe: sofort alle Repos, danach alle `TAKT`; endet am Abschaltsignal.
    pub fn starten(self: Arc<Self>) {
        let aus = self.abschaltung();
        tokio::spawn(async move {
            tokio::pin!(aus);
            loop {
                let start = Instant::now();
                tokio::select! {
                    anzahl = self.alle_erneuern() => debug!("Git-Stand von {anzahl} Repos erneuert in {} ms", start.elapsed().as_millis()),
                    _ = &mut aus => return,
                }
                tokio::select! {
                    _ = tokio::time::sleep(TAKT) => {}
                    _ = &mut aus => return,
                }
            }
        });
    }

    /// Der letzte Stand eines Repos, `None` vor seiner ersten Abfrage.
    pub fn stand_fuer(&self, pfad: &str) -> Option<Stand> {
        lesen(&self.staende, "Stände").get(&schluessel(pfad)).cloned()
    }

    /// Die Zeile für ein Repo eines Projekts: Name, Pfad und „privat“ aus dem
    /// Projekt, der Rest aus dem Speicher. Noch nicht abgefragt: ohne Zweig,
    /// ohne Fehler, `stand` leer.
    pub fn zeile(&self, r: &RepoRef) -> RepoStatus {
        let leer = git::leer(&r.path, r.label.as_deref(), r.private);
        let Some(s) = self.stand_fuer(&r.path) else {
            return leer;
        };
        let mut z = s.status;
        z.label = leer.label;
        z.path = leer.path;
        z.private = r.private;
        if r.private {
            z.commit_subject = None;
        }
        z.stand = Some(s.zeit.to_rfc3339());
        z
    }

    /// Alle Zeilen eines Projekts in der Reihenfolge der `[[repos]]`.
    pub fn repos_fuer(&self, p: &Project) -> Vec<RepoStatus> {
        p.meta.repos.iter().map(|r| self.zeile(r)).collect()
    }

    /// Die bekannten Repo-Pfade (für den Datei-Beobachter), so geschrieben
    /// wie im ersten Projekt, das sie nennt.
    pub fn repo_pfade(&self) -> Vec<String> {
        lesen(&self.ziele, "Ziele").values().map(|z| z.pfad.clone()).collect()
    }

    /// Repos je Projekt: aus dem ContentStore, ohne ihn aus den Seiten selbst.
    fn projekt_repos(&self) -> Vec<(String, Vec<RepoRef>)> {
        match &self.inhalt {
            Some(i) => i.bestand().repos(),
            None => crate::quellen::Quellen::aus_liste(self.quellen.clone()).seiten().into_iter().map(|p| (p.slug, p.meta.repos)).collect(),
        }
    }

    /// Nimmt die Projekte neu und merkt sich ihre Repos; vergisst Stände und
    /// Sperren von Repos, die kein Projekt mehr nennt. Gibt die neu
    /// hinzugekommenen Pfade zurück.
    pub fn ziele_lesen(&self) -> Vec<String> {
        let mut neu_ziele: BTreeMap<String, Ziel> = BTreeMap::new();
        let mut neu_slugs: HashMap<String, Vec<String>> = HashMap::new();
        for (slug, repos) in self.projekt_repos() {
            for r in &repos {
                let k = schluessel(&r.path);
                neu_ziele.entry(k.clone()).or_insert_with(|| Ziel { pfad: r.path.clone(), privat: false }).privat |= r.private;
                let s = neu_slugs.entry(k).or_default();
                if !s.contains(&slug) {
                    s.push(slug.clone());
                }
            }
        }
        let neue = {
            let mut ziele = schreiben(&self.ziele, "Ziele");
            let neue = neu_ziele.iter().filter(|(k, _)| !ziele.contains_key(*k)).map(|(_, z)| z.pfad.clone()).collect();
            *ziele = neu_ziele;
            neue
        };
        *schreiben(&self.slugs, "Slugs") = neu_slugs;
        let ziele = lesen(&self.ziele, "Ziele");
        schreiben(&self.staende, "Stände").retain(|k, _| ziele.contains_key(k));
        sperren(&self.sperren, "Sperren je Repo").retain(|k, _| ziele.contains_key(k));
        neue
    }

    /// Erneuert alle Repos aus allen Projekten, höchstens `GLEICHZEITIG`
    /// zugleich, und meldet am Ende einmal alle geänderten Projekte. Gibt
    /// die Zahl der Repos zurück.
    pub async fn alle_erneuern(&self) -> usize {
        self.ziele_lesen();
        let pfade = self.repo_pfade();
        self.mehrere_erneuern(&pfade).await;
        pfade.len()
    }

    /// Mehrere Repos (die Grenze `GLEICHZEITIG` setzt `erneuern_still`
    /// selbst); geänderte Projekte gehen am Ende zusammen an den Melder.
    async fn mehrere_erneuern(&self, pfade: &[String]) {
        let geaendert = futures_util::future::join_all(pfade.iter().map(|p| self.erneuern_still(p))).await;
        let mut slugs: Vec<String> = geaendert.into_iter().flatten().collect();
        slugs.sort();
        slugs.dedup();
        self.meldung.melden(slugs);
    }

    /// Fragt ein Repo ab und legt das Ergebnis ab; ein geänderter Stand geht
    /// (gesammelt über `MELDEN_NACH`) an den Melder. Siehe `erneuern_still`.
    pub async fn erneuern(&self, pfad: &str) {
        let slugs = self.erneuern_still(pfad).await;
        self.meldung.melden(slugs);
    }

    /// Fragt ein Repo ab und legt das Ergebnis ab. Läuft für dieses Repo schon
    /// eine Abfrage, wartet sie darauf; hat die laufende erst nach diesem
    /// Aufruf begonnen (monotone Uhr), gilt deren Ergebnis, und es wird nicht
    /// noch einmal gefragt. Gefragt wird erst mit einem der `GLEICHZEITIG`
    /// Plätze. Gibt die Slugs zurück, deren Stand sich geändert hat.
    async fn erneuern_still(&self, pfad: &str) -> Vec<String> {
        let angefragt = Instant::now();
        let k = schluessel(pfad);
        let sperre = sperren(&self.sperren, "Sperren je Repo").entry(k.clone()).or_default().clone();
        let _gesperrt = sperre.lock().await;
        if self.stand_fuer(pfad).is_some_and(|s| s.begonnen >= angefragt) {
            return Vec::new();
        }
        let Ok(platz) = self.plaetze.acquire().await else { return Vec::new() };
        // Unbekannte Repos (nicht aus einem Projekt) vorsichtshalber als privat.
        let (frage_pfad, privat) = match lesen(&self.ziele, "Ziele").get(&k) {
            Some(z) => (z.pfad.clone(), z.privat),
            None => (pfad.to_string(), true),
        };
        let zeit = Local::now();
        let begonnen = Instant::now();
        let ergebnis = self.abfragen(&frage_pfad, privat).await;
        drop(platz);
        let status = match ergebnis {
            Ok(s) => {
                if !matches!(self.quelle, Quelle::Direkt) && self.bote_lage.swap(BOTE_DA, Ordering::Relaxed) == BOTE_WEG {
                    info!("Git-Stand: Der Bote antwortet wieder.");
                }
                s
            }
            Err(weg) => match self.bote_weg(pfad, privat, weg) {
                Some(s) => s,
                None => return Vec::new(),
            },
        };
        let geaendert = self.stand_fuer(pfad).is_none_or(|alt| !gleicher_stand(&alt.status, &status));
        schreiben(&self.staende, "Stände").insert(k.clone(), Stand { status, zeit, begonnen });
        if geaendert { lesen(&self.slugs, "Slugs").get(&k).cloned().unwrap_or_default() } else { Vec::new() }
    }

    /// Eine Abfrage an die Quelle (zählt die gleichzeitig laufenden).
    async fn abfragen(&self, pfad: &str, privat: bool) -> Result<RepoStatus, git::BoteWeg> {
        let jetzt = self.laufend.fetch_add(1, Ordering::SeqCst) + 1;
        let _abzug = Abzug(&self.laufend);
        self.hoechst_zugleich.fetch_max(jetzt, Ordering::SeqCst);
        match &self.quelle {
            Quelle::Direkt => Ok(git::inspect(pfad, None, privat).await),
            Quelle::Bote(bote) => git::ueber_boten_oder_weg(bote, pfad, None, privat).await,
            #[cfg(test)]
            Quelle::Attrappe(a) => {
                tokio::time::sleep(a.dauer).await;
                let antwort = a.antworten.lock().unwrap().get(pfad).cloned();
                antwort.unwrap_or_else(|| Ok(git::leer(pfad, None, privat)))
            }
        }
    }

    /// Der Bote hat keinen Stand geliefert. Ausgelastet: nur eine
    /// Verzögerung, der Stand bleibt (auch kein Stand), der Bote gilt nicht
    /// als weg. Sonst: einmal je Wechsel warnen; ein Repo ohne früheren Stand
    /// bekommt den Grund. `None`: nichts ablegen.
    fn bote_weg(&self, pfad: &str, privat: bool, weg: git::BoteWeg) -> Option<RepoStatus> {
        let satz = weg.satz;
        if weg.ausgelastet {
            debug!("Git-Stand {pfad}: Bote ausgelastet ({satz}); der Stand bleibt, neuer Versuch beim nächsten Lauf oder der nächsten Änderung");
            return None;
        }
        if self.bote_lage.swap(BOTE_WEG, Ordering::Relaxed) != BOTE_WEG {
            warn!("Git-Stand: Der Bote antwortet nicht ({satz}). Die Repos behalten ihren letzten Stand; neuer Versuch alle {} s.", TAKT.as_secs());
        }
        debug!("Git-Stand {pfad}: Bote antwortet nicht ({satz})");
        if self.stand_fuer(pfad).is_some() {
            return None;
        }
        // Noch nie ein Stand: lieber den Grund zeigen als ewig „wird ermittelt“.
        Some(RepoStatus { error: Some(format!("Git-Stand unbekannt: {satz}")), ..git::leer(pfad, None, privat) })
    }

    /// Meldung des Datei-Beobachters: In `pfad` hat sich etwas geändert. Das
    /// Repo wird nach `ENTPRELLEN` erneuert; weitere Meldungen bis dahin
    /// zählen mit. Unbekannte Pfade (kein Projekt nennt sie) zählen nicht.
    pub fn aenderung_gemeldet(self: &Arc<Self>, pfad: &str) {
        let k = schluessel(pfad);
        if !lesen(&self.ziele, "Ziele").contains_key(&k) {
            return;
        }
        if !sperren(&self.ausstehend, "Ausstehend").insert(k.clone()) {
            return;
        }
        let ich = self.clone();
        let pfad = pfad.to_string();
        let aus = self.abschaltung();
        tokio::spawn(async move {
            tokio::select! {
                _ = tokio::time::sleep(ENTPRELLEN) => {}
                _ = aus => return,
            }
            sperren(&ich.ausstehend, "Ausstehend").remove(&k);
            debug!("Git-Stand {pfad}: Änderung gemeldet, wird erneuert");
            ich.erneuern(&pfad).await;
        });
    }

    /// Nach einer Änderung unter `projects/`: neue Repos sofort abfragen
    /// (höchstens `GLEICHZEITIG` zugleich), nicht erst beim nächsten Takt.
    pub fn projekte_geaendert(self: &Arc<Self>) {
        let ich = self.clone();
        tokio::spawn(async move {
            let neue = ich.ziele_lesen();
            if !neue.is_empty() {
                debug!("Git-Stand: neue Repos {neue:?}");
                ich.mehrere_erneuern(&neue).await;
            }
        });
    }

    /// Sieht alle `BOTE_TAKT` nach, ob der Bote erreichbar ist. Solange er es
    /// nicht ist, schickt es dafür selbst ein Hallo (sonst ändert sich der
    /// Stand des Clients nur, wenn zufällig eine andere Anfrage läuft). Wird
    /// er nach „nicht erreichbar“ erreichbar, werden sofort alle Repos
    /// erneuert (mit der gemeinsamen Grenze) statt erst beim nächsten Takt:
    /// Beim Dienststart ist der Bote noch nicht da, und der erste Lauf bekäme
    /// sonst für alle Repos eine Minute lang „Git-Stand unbekannt“.
    pub fn bote_beobachten(self: Arc<Self>, bote: Arc<Bote>) {
        self.bote_beobachten_mit(BOTE_TAKT, move || {
            let bote = bote.clone();
            Box::pin(async move {
                if !bote.stand().verbunden {
                    let _ = bote.anfragen_mit_frist(&Anfrage::Hallo, BOTE_TAKT).await;
                }
                bote.stand().verbunden
            })
        });
    }

    fn bote_beobachten_mit(self: Arc<Self>, takt: Duration, erreichbar: impl Fn() -> Pruefung + Send + 'static) {
        let aus = self.abschaltung();
        tokio::spawn(async move {
            tokio::pin!(aus);
            let mut war = erreichbar().await;
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(takt) => {}
                    _ = &mut aus => return,
                }
                let ist = erreichbar().await;
                if ist && !war {
                    info!("Git-Stand: Der Bote ist erreichbar; alle Repos werden erneuert.");
                    let anzahl = self.alle_erneuern().await;
                    debug!("Git-Stand von {anzahl} Repos nach Rückkehr des Boten erneuert");
                }
                war = ist;
            }
        });
    }

    /// Nur für Tests: einen Stand vorgeben.
    #[cfg(test)]
    pub fn eintragen(&self, status: RepoStatus, zeit: DateTime<Local>) {
        let k = schluessel(&status.path);
        self.ziele.write().unwrap().entry(k.clone()).or_insert(Ziel { pfad: status.path.clone(), privat: status.private });
        let alter = (Local::now() - zeit).to_std().unwrap_or_default();
        let begonnen = Instant::now().checked_sub(alter).unwrap_or_else(Instant::now);
        self.staende.write().unwrap().insert(k, Stand { status, zeit, begonnen });
    }

    /// Nur für Tests: ein Repo bekannt machen, ohne Projektordner.
    #[cfg(test)]
    pub fn ziel_setzen(&self, pfad: &str, privat: bool) {
        self.ziele.write().unwrap().insert(schluessel(pfad), Ziel { pfad: pfad.to_string(), privat });
    }

    /// Nur für Tests: ein Repo eines Projekts bekannt machen.
    #[cfg(test)]
    pub fn ziel_fuer_projekt(&self, pfad: &str, slug: &str) {
        self.ziel_setzen(pfad, false);
        self.slugs.write().unwrap().entry(schluessel(pfad)).or_default().push(slug.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn hier() -> String {
        env!("CARGO_MANIFEST_DIR").to_string()
    }

    fn haengt() -> String {
        format!("{}/tests/beispiel/haengt", env!("CARGO_MANIFEST_DIR"))
    }

    fn leerer() -> Zwischenspeicher {
        Zwischenspeicher::neu(vec![PathBuf::from("C:/gibt-es-nicht/projects")], None)
    }

    #[test]
    fn vor_der_ersten_abfrage_ohne_stand_und_ohne_fehler() {
        let z = leerer();
        let r = RepoRef { path: hier(), label: Some("hier".into()), private: false };
        let s = z.zeile(&r);
        assert_eq!(s.label, "hier");
        assert!(s.stand.is_none() && s.error.is_none() && s.branch.is_none(), "{s:?}");
    }

    #[test]
    fn zeile_nimmt_name_und_privat_aus_dem_projekt() {
        let z = leerer();
        let zeit = Local::now();
        let status = RepoStatus { branch: Some("main".into()), commit_subject: Some("Geheim".into()), ..git::leer(&hier(), None, false) };
        z.eintragen(status, zeit);
        let offen = z.zeile(&RepoRef { path: hier(), label: Some("offen".into()), private: false });
        assert_eq!(offen.label, "offen");
        assert_eq!(offen.commit_subject.as_deref(), Some("Geheim"));
        assert_eq!(offen.stand, Some(zeit.to_rfc3339()));
        assert!(chrono::DateTime::parse_from_rfc3339(offen.stand.as_deref().unwrap()).is_ok());
        let privat = z.zeile(&RepoRef { path: hier(), label: None, private: true });
        assert!(privat.private && privat.commit_subject.is_none(), "{privat:?}");
        assert_eq!(privat.branch.as_deref(), Some("main"));
    }

    #[tokio::test]
    async fn erneuern_holt_den_stand_dieses_repos() {
        let z = leerer();
        z.ziel_setzen(&hier(), true);
        let vorher = Local::now();
        z.erneuern(&hier()).await;
        let s = z.stand_fuer(&hier()).expect("Stand fehlt");
        assert!(s.zeit >= vorher);
        assert!(s.status.error.is_none() && s.status.branch.is_some(), "{:?}", s.status);
        assert!(s.status.commit_subject.is_none(), "privat: keine Texte");
    }

    /// Je Repo höchstens eine Abfrage zugleich: Während A läuft, kommen B und
    /// C; sie warten. B fragt danach neu (A begann vor der Bitte), C nicht
    /// mehr (B begann nach ihr). Ein hängendes Repo kostet also zweimal das
    /// Budget, nicht einmal (parallel) und nicht dreimal.
    #[tokio::test]
    async fn hoechstens_eine_auffrischung_je_repo() {
        let z = leerer();
        let h = haengt();
        z.ziel_setzen(&h, false);
        let start = Instant::now();
        let a = z.erneuern(&h);
        let spaeter = async {
            tokio::time::sleep(Duration::from_millis(300)).await;
            tokio::join!(z.erneuern(&h), z.erneuern(&h));
        };
        tokio::join!(a, spaeter);
        let dauer = start.elapsed();
        assert!(dauer >= git::BUDGET * 2 && dauer < git::BUDGET * 3, "{dauer:?}");
        assert_eq!(z.stand_fuer(&h).unwrap().status.error.as_deref(), Some("git antwortet nicht (Zeitlimit)"));
    }

    /// Mehrere Meldungen in kurzer Folge: eine Erneuerung nach `ENTPRELLEN`.
    #[tokio::test]
    async fn meldungen_werden_entprellt() {
        let z = Arc::new(leerer());
        z.ziel_setzen(&hier(), false);
        for _ in 0..5 {
            z.aenderung_gemeldet(&hier());
        }
        z.aenderung_gemeldet("C:/kennt/niemand");
        assert_eq!(z.ausstehend.lock().unwrap().len(), 1);
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert!(z.stand_fuer(&hier()).is_none(), "zu früh erneuert");
        let ende = Instant::now() + Duration::from_secs(15);
        while z.stand_fuer(&hier()).is_none() && Instant::now() < ende {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        assert!(z.stand_fuer(&hier()).is_some_and(|s| s.status.branch.is_some()));
        assert!(z.ausstehend.lock().unwrap().is_empty());
        assert!(z.stand_fuer("C:/kennt/niemand").is_none());
    }

    fn mit_attrappe(dauer: Duration) -> (Zwischenspeicher, Arc<Attrappe>) {
        let a = Arc::new(Attrappe { antworten: Mutex::new(HashMap::new()), dauer });
        let z = Zwischenspeicher::mit_quelle(vec![PathBuf::from("C:/gibt-es-nicht/projects")], Quelle::Attrappe(a.clone()));
        (z, a)
    }

    /// Zehn Änderungsmeldungen auf zehn Repos zugleich, dazu ein Lauf über
    /// einige und ein einzelnes `erneuern`: nie mehr als `GLEICHZEITIG`
    /// Abfragen zugleich (die Grenze gilt für alle Wege gemeinsam).
    #[tokio::test]
    async fn zehn_meldungen_hoechstens_zwei_abfragen_zugleich() {
        let (z, _a) = mit_attrappe(Duration::from_millis(200));
        let z = Arc::new(z);
        let pfade: Vec<String> = (0..10).map(|i| format!("C:/attrappe/r{i}")).collect();
        for p in &pfade {
            z.ziel_setzen(p, false);
        }
        for p in &pfade {
            z.aenderung_gemeldet(p);
        }
        tokio::time::sleep(ENTPRELLEN).await;
        tokio::join!(z.mehrere_erneuern(&pfade[..4]), z.erneuern(&pfade[9]));
        let ende = Instant::now() + Duration::from_secs(20);
        while (pfade.iter().any(|p| z.stand_fuer(p).is_none()) || z.laufend.load(Ordering::SeqCst) > 0 || !z.ausstehend.lock().unwrap().is_empty())
            && Instant::now() < ende
        {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        assert!(pfade.iter().all(|p| z.stand_fuer(p).is_some()), "nicht alle erneuert");
        assert_eq!(z.hoechst_zugleich.load(Ordering::SeqCst), GLEICHZEITIG);
        assert_eq!(z.laufend.load(Ordering::SeqCst), 0);
    }

    /// Bote fehlt beim Start (alle Repos „Git-Stand unbekannt“), kommt dann:
    /// `bote_beobachten` erneuert sofort alle Repos, binnen weniger Sekunden
    /// hat jedes einen Stand ohne Fehler.
    #[tokio::test]
    async fn bote_kommt_nach_dem_start_alle_repos_sofort() {
        use std::sync::atomic::AtomicBool;
        // `alle_erneuern` liest die Projekte neu; darum ein echter Projektordner
        // mit einem Projekt, das die drei Repos nennt.
        let pfade: Vec<String> = (0..3).map(|i| format!("C:/attrappe/start{i}")).collect();
        let ordner = std::env::temp_dir().join(format!("dashboard-bote-wecker-{}", std::process::id()));
        let projekte = ordner.join("projects");
        std::fs::create_dir_all(projekte.join("probe")).unwrap();
        let mut seite = String::from("schema = 3\ntitle = \"Probe\"\nstatus = \"aktiv\"\nsummary = \"Bote-Wecker\"\n");
        for p in &pfade {
            seite.push_str(&format!("\n[[repos]]\npath = \"{p}\"\n"));
        }
        std::fs::write(projekte.join("probe").join("projekt.toml"), seite).unwrap();
        let a = Arc::new(Attrappe { antworten: Mutex::new(HashMap::new()), dauer: Duration::from_millis(20) });
        // Umzug des Inhalts: eine zweite Quelle mit einem Projekt, das ein
        // viertes Repo nennt, zählt mit (ohne ContentStore liest er alle Quellen).
        let zweite = ordner.join("zweite");
        std::fs::create_dir_all(zweite.join("beispiel-b")).unwrap();
        std::fs::write(
            zweite.join("beispiel-b").join("projekt.toml"),
            "schema = 3\ntitle = \"B\"\nstatus = \"aktiv\"\nsummary = \"Zweite Quelle\"\n\n[[repos]]\npath = \"C:/attrappe/zweite\"\n",
        )
        .unwrap();
        let beide = Zwischenspeicher::mit_quelle(vec![projekte.clone(), zweite.clone()], Quelle::Attrappe(a.clone()));
        assert_eq!(beide.ziele_lesen().len(), 4, "beide Quellen");
        let z = Arc::new(Zwischenspeicher::mit_quelle(vec![projekte], Quelle::Attrappe(a.clone())));
        assert_eq!(z.ziele_lesen().len(), 3, "Projekt nennt drei Repos");
        let weg = Err(git::BoteWeg { satz: "Der Bote läuft nicht (keine Pipe).".into(), ausgelastet: false });
        for p in &pfade {
            a.antworten.lock().unwrap().insert(p.clone(), weg.clone());
        }
        let erreichbar = Arc::new(AtomicBool::new(false));
        let e = erreichbar.clone();
        z.clone().bote_beobachten_mit(Duration::from_millis(50), move || {
            let ist = e.load(Ordering::SeqCst);
            Box::pin(async move { ist })
        });

        // Erster Lauf ohne Boten: jedes Repo kennt nur den Grund.
        z.mehrere_erneuern(&pfade).await;
        assert!(pfade.iter().all(|p| z.stand_fuer(p).is_some_and(|s| s.status.error.as_deref().is_some_and(|e| e.starts_with("Git-Stand unbekannt")))));
        assert_eq!(z.bote_lage.load(Ordering::Relaxed), BOTE_WEG);

        // Der Bote kommt: Antworten da, erreichbar.
        for p in &pfade {
            a.antworten.lock().unwrap().insert(p.clone(), Ok(RepoStatus { branch: Some("main".into()), ..git::leer(p, None, false) }));
        }
        erreichbar.store(true, Ordering::SeqCst);
        let ende = Instant::now() + Duration::from_secs(5);
        while pfade.iter().any(|p| z.stand_fuer(p).is_none_or(|s| s.status.error.is_some())) && Instant::now() < ende {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        for p in &pfade {
            let s = z.stand_fuer(p).expect("Stand");
            assert!(s.status.error.is_none(), "{p}: noch Fehler {:?}", s.status.error);
            assert_eq!(s.status.branch.as_deref(), Some("main"));
        }
        assert_eq!(z.bote_lage.load(Ordering::Relaxed), BOTE_DA);
        assert!(z.hoechst_zugleich.load(Ordering::SeqCst) <= GLEICHZEITIG);
        let _ = std::fs::remove_dir_all(&ordner);
    }

    /// „Ausgelastet“ ist nur eine Verzögerung: Der Stand bleibt (auch kein
    /// Stand), der Bote gilt nicht als weg. Ein echtes „weg“ dagegen schon.
    #[tokio::test]
    async fn ausgelastet_ist_kein_bote_weg() {
        let (z, a) = mit_attrappe(Duration::ZERO);
        let alt = Local::now() - chrono::Duration::minutes(10);
        let mit = "C:/attrappe/mit";
        let ohne = "C:/attrappe/ohne";
        z.eintragen(RepoStatus { branch: Some("main".into()), ..git::leer(mit, None, false) }, alt);
        z.ziel_setzen(ohne, false);
        let ausgelastet = Err(git::BoteWeg { satz: crate::bote::client::SATZ_AUSGELASTET.into(), ausgelastet: true });
        a.antworten.lock().unwrap().insert(mit.into(), ausgelastet.clone());
        a.antworten.lock().unwrap().insert(ohne.into(), ausgelastet);
        z.erneuern(mit).await;
        z.erneuern(ohne).await;
        let s = z.stand_fuer(mit).unwrap();
        assert_eq!(s.zeit, alt);
        assert_eq!(s.status.branch.as_deref(), Some("main"));
        assert!(z.stand_fuer(ohne).is_none(), "ausgelastet: kein Fehler-Stand");
        assert_eq!(z.bote_lage.load(Ordering::Relaxed), BOTE_UNBEKANNT);

        // Antwort da, dann ausgelastet: bleibt DA.
        a.antworten.lock().unwrap().insert(ohne.into(), Ok(git::leer(ohne, None, false)));
        z.erneuern(ohne).await;
        assert_eq!(z.bote_lage.load(Ordering::Relaxed), BOTE_DA);
        z.erneuern(mit).await;
        assert_eq!(z.bote_lage.load(Ordering::Relaxed), BOTE_DA);

        // Wirklich weg: WEG, der Stand bleibt.
        a.antworten.lock().unwrap().insert(mit.into(), Err(git::BoteWeg { satz: "keine Pipe".into(), ausgelastet: false }));
        z.erneuern(mit).await;
        assert_eq!(z.bote_lage.load(Ordering::Relaxed), BOTE_WEG);
        assert_eq!(z.stand_fuer(mit).unwrap().zeit, alt);
    }

    /// Ein Melder, der jeden Aufruf mitschreibt.
    fn mitschreiber() -> (Melder, Arc<Mutex<Vec<Vec<String>>>>) {
        let gemeldet: Arc<Mutex<Vec<Vec<String>>>> = Default::default();
        let g = gemeldet.clone();
        (Arc::new(move |s: &[String]| g.lock().unwrap().push(s.to_vec())), gemeldet)
    }

    /// Geänderter Stand: genau eine Meldung mit dem richtigen Slug (zwei
    /// Repos eines Projekts kurz nacheinander: eine). Zweimal derselbe Stand:
    /// keine.
    #[tokio::test]
    async fn geaenderter_stand_meldet_einmal_je_projekt() {
        let (z, a) = mit_attrappe(Duration::ZERO);
        let (melder, gemeldet) = mitschreiber();
        let z = z.mit_melder(melder);
        let neu = || std::mem::take(&mut *gemeldet.lock().unwrap());
        let (r1, r2, r3) = ("C:/attrappe/r1", "C:/attrappe/r2", "C:/attrappe/r3");
        z.ziel_fuer_projekt(r1, "alpha");
        z.ziel_fuer_projekt(r3, "alpha");
        z.ziel_fuer_projekt(r2, "beta");
        let stand = |branch: &str, pfad: &str| RepoStatus { branch: Some(branch.into()), commit_hash: Some("abc".into()), ..git::leer(pfad, None, false) };
        let alt = Local::now() - chrono::Duration::minutes(10);
        z.eintragen(stand("main", r1), alt);
        a.antworten.lock().unwrap().insert(r1.into(), Ok(stand("main", r1)));

        // Zweimal derselbe Stand: nichts.
        z.erneuern(r1).await;
        z.erneuern(r1).await;
        tokio::time::sleep(MELDEN_NACH * 2).await;
        assert!(neu().is_empty());

        // Anderer Zweig in r1 und gleich danach der erste Stand von r3 (selbes Projekt).
        a.antworten.lock().unwrap().insert(r1.into(), Ok(stand("neu", r1)));
        z.erneuern(r1).await;
        z.erneuern(r3).await;
        tokio::time::sleep(MELDEN_NACH * 2).await;
        assert_eq!(neu(), vec![vec!["alpha".to_string()]], "nur eine Meldung");

        // Fehler anders als vorher: wieder eines, jetzt für beta.
        z.eintragen(stand("main", r2), alt);
        a.antworten.lock().unwrap().insert(r2.into(), Ok(RepoStatus { error: Some("kaputt".into()), ..stand("main", r2) }));
        z.erneuern(r2).await;
        tokio::time::sleep(MELDEN_NACH * 2).await;
        assert_eq!(neu(), vec![vec!["beta".to_string()]]);
    }

    /// Ein Lauf über alle Repos meldet einmal am Ende, mit allen geänderten
    /// Projekten (Etappe 2d: ein Sammelereignis statt einer Meldung je Projekt).
    #[tokio::test]
    async fn lauf_ueber_alle_meldet_einmal() {
        let (z, _a) = mit_attrappe(Duration::from_millis(30));
        let (melder, gemeldet) = mitschreiber();
        let z = z.mit_melder(melder);
        let pfade: Vec<String> = (0..8).map(|i| format!("C:/attrappe/lauf{i}")).collect();
        for (i, p) in pfade.iter().enumerate() {
            z.ziel_fuer_projekt(p, &format!("p{i}"));
        }
        z.mehrere_erneuern(&pfade).await;
        tokio::time::sleep(MELDEN_NACH * 2).await;
        let m = gemeldet.lock().unwrap().clone();
        assert_eq!(m.len(), 1, "{m:?}");
        assert_eq!(m[0].len(), 8, "{m:?}");
    }

    /// Pfade gelten ohne Groß/Klein und Schrägstriche; Sperren und Stände
    /// verschwundener Repos werden aufgeräumt.
    #[test]
    fn pfade_normalisiert_und_aufgeraeumt() {
        assert_eq!(schluessel(r"C:\Users\X\repo\"), schluessel("c:/users/x/repo"));
        let z = leerer();
        let zeit = Local::now();
        z.eintragen(RepoStatus { branch: Some("main".into()), ..git::leer("C:/A/Repo", None, false) }, zeit);
        assert_eq!(z.stand_fuer(r"c:\a\repo").unwrap().status.branch.as_deref(), Some("main"));
        z.sperren.lock().unwrap().insert(schluessel("C:/A/Repo"), Default::default());
        // Kein Projekt nennt das Repo mehr (der Ordner gibt es nicht): alles weg.
        z.ziele_lesen();
        assert!(z.stand_fuer("C:/A/Repo").is_none());
        assert!(z.sperren.lock().unwrap().is_empty());
    }

    /// Am Abschaltsignal enden Takt und Wächter des Boten.
    #[tokio::test]
    async fn aufgaben_enden_am_abschaltsignal() {
        let (tx, rx) = watch::channel(false);
        let (z, _a) = mit_attrappe(Duration::ZERO);
        let z = Arc::new(z.mit_abschaltung(rx));
        z.clone().starten();
        z.clone().bote_beobachten_mit(Duration::from_millis(10), || Box::pin(async { true }));
        tokio::time::sleep(Duration::from_millis(50)).await;
        let vorher = Arc::strong_count(&z);
        assert!(vorher >= 3, "{vorher}");
        tx.send_replace(true);
        let ende = Instant::now() + Duration::from_secs(2);
        while Arc::strong_count(&z) > 1 && Instant::now() < ende {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert_eq!(Arc::strong_count(&z), 1, "Aufgaben halten den Speicher noch");
    }

    /// Ohne Boten bleibt der letzte Stand mit seiner Zeit; ein Repo ohne
    /// früheren Stand nennt den Grund.
    #[cfg(windows)]
    #[tokio::test]
    async fn ohne_boten_bleibt_der_letzte_stand() {
        use crate::dienst::einstellungen::Einstellungen;
        let pipe = format!("dashboard-bote-gibt-es-nicht-{}", std::process::id());
        let text = format!(
            "schema = 1\nrepo = \"{}\"\nport = 8792\n[pfade]\ndaten = \"C:/gibt-es-nicht/d.redb\"\nsicherung = \"C:/gibt-es-nicht/s\"\nprotokoll = \"C:/gibt-es-nicht/p\"\n[konten]\ninhaber_sid = \"\"\ndienst_sid = \"\"\n[bote]\npipe = \"{pipe}\"\nprogramm = \"C:/gibt-es-nicht/dashboard-bote.exe\"\n",
            hier().replace('\\', "/")
        );
        let e = Einstellungen::aus_text(std::path::Path::new("test.toml"), &text).unwrap();
        let z = Zwischenspeicher::neu(vec![PathBuf::from("C:/gibt-es-nicht/projects")], Some(Arc::new(Bote::neu(&e))));
        let alt = Local::now() - chrono::Duration::minutes(10);
        z.eintragen(RepoStatus { branch: Some("main".into()), ..git::leer(&hier(), None, false) }, alt);
        z.ziel_setzen(&haengt(), false);
        z.erneuern(&hier()).await;
        z.erneuern(&haengt()).await;
        let s = z.stand_fuer(&hier()).unwrap();
        assert_eq!(s.zeit, alt);
        assert_eq!(s.status.branch.as_deref(), Some("main"));
        let neu = z.stand_fuer(&haengt()).unwrap();
        assert!(neu.status.error.as_deref().is_some_and(|e| e.starts_with("Git-Stand unbekannt: ")), "{:?}", neu.status);
        assert_eq!(z.bote_lage.load(Ordering::Relaxed), BOTE_WEG);
    }
}
