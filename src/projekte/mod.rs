//! Register der Backend-Module der Projekte (Etappe 4b, `[projekt_backend]`).
//!
//! Ein Modul ist eine eigene Kiste in `projects/<slug>/server/`, die nur vom
//! SDK `dashboard-projekt` abhängt. Seit dem Umzug des Inhalts
//! ([zusammenbau], [code_nur_kern]) steht hier kein Projektname mehr: Die
//! Kisten liegen in der Inhaltsquelle, eine Zusammenbau-Kiste
//! (`vorlagen/dashboard-lokal/`) führt sie in ihrer Liste geprüfter Kisten
//! und meldet sie über [`crate::starten`] an ([`anmelden`]). Das allgemeine
//! `dashboard.exe` läuft ohne Module.
//!
//! Das Register
//! - hängt den Router jedes Moduls unter `/api/projekte/<slug>/x/` ein, im
//!   Rahmen des SDK (Absturz je Anfrage gefangen, Körper ≤ 64 KiB, Fehlerform
//!   `{ fehler, art }`). Lesen ist frei wie bei 4v; schreibende Wege brauchen
//!   die Windows-Anmeldung (`sicherheit::anmeldung_pflicht`) und die
//!   Herkunftsprüfung (`sicherheit::herkunft_pflicht`), wie überall;
//! - richtet beim Start die Tabellen der Module ein (`speicher::projekte`);
//! - ruft die Hooks: `nach_lesen` im Leseweg von 4v (`api::neue_form`),
//!   `nach_daten_geaendert` 2 s nach der letzten Änderung (Beobachter),
//!   `zeitplan` und `taeglich` aus dem Minutentakt, `bei_vorgang` aus dem
//!   Live-Kanal der Vorgänge (nur eigene), `nach_bau` aus dem Baumeister;
//! - führt jeden Hook in `spawn_blocking` aus, mit Zeitlimit, höchstens ein
//!   Lauf je Hook und Projekt (weitere Aufrufe warten, gleiche werden
//!   zusammengelegt);
//! - sammelt Fehler und Abstürze als Störungen: ins Protokoll und als Fund des
//!   Projekts nach /api/pruefung, mit Zahl der Wiederholungen.
//!
//! Die Wächter-Tests für `projects/*/server` stehen in `waechter`.

pub mod anbindung;
pub mod waechter;
pub mod zusammenbau;

use std::collections::{BTreeMap, HashMap, VecDeque};

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use axum::Router;
use dashboard_projekt::hooks::{self, Aufruf, BauErgebnis, HookArt, VorgangInfo};
use dashboard_projekt::{Herkunft, Hooks, Modul, ProjektKontext, rahmen};
use tracing::{error, info, warn};

use crate::app::AppState;
use crate::inhalt::pruefung::Befund;
use crate::inhalt::seite;
use crate::speicher::Speicher;

/// Ein Modul im Register mit dem Namen der Kiste, aus der sein Typ stammt.
#[derive(Clone)]
pub struct ModulEintrag {
    pub modul: Arc<dyn Modul>,
    /// Kistenname des Typs (`projekt_beispiel`), aus `std::any::type_name`.
    pub kiste: &'static str,
}

/// Ein Eintrag für [`crate::starten`]; die Kiste ergibt sich aus dem Typ.
pub fn eintrag<M: Modul>(modul: M) -> ModulEintrag {
    let name = std::any::type_name::<M>();
    ModulEintrag { modul: Arc::new(modul), kiste: name.split("::").next().unwrap_or(name) }
}

/// Die angemeldeten Module (einmal, beim Start).
static ANGEMELDET: std::sync::OnceLock<Vec<ModulEintrag>> = std::sync::OnceLock::new();

/// Meldet die Module an; nur [`crate::starten`] ruft das, einmal je Prozess.
/// Ein zweiter Aufruf ändert nichts (Satz im Protokoll).
pub fn anmelden(module: Vec<ModulEintrag>) {
    if ANGEMELDET.set(module).is_err() {
        warn!("Module sind schon angemeldet; der zweite Aufruf ändert nichts.");
    }
}

/// Alle Module, die im Dienst laufen: was [`anmelden`] bekam, sonst keins
/// (allgemeines `dashboard.exe`, Tests).
pub fn module() -> Vec<ModulEintrag> {
    ANGEMELDET.get().cloned().unwrap_or_default()
}

/// Der Satz für ein Projekt mit Rechenteil (`server/`), dessen Modul in
/// diesem Programm nicht angemeldet ist ([zusammenbau]: das allgemeine
/// `dashboard.exe` ohne die Zusammenbau-Kiste).
pub const SATZ_OHNE_ZUSAMMENBAU: &str = "Der Rechenteil dieses Projekts ist in diesem Programm nicht eingebaut: Es läuft das allgemeine dashboard.exe ohne die Zusammenbau-Kiste dashboard-lokal. Die Seite geht, die Wege unter /x/ fehlen.";

/// Passt ein Modul zu seinem Ordner? Geprüft wird nur, was zur Bauzeit in der
/// Modul-Kiste feststand (`herkunft!()`), nie Dateien zur Laufzeit: Kistenname
/// `projekt-<slug>`, Ordner der Kiste `projects/<slug>/server`, und der Typ
/// stammt aus genau dieser Kiste. So hängt das Ergebnis nicht vom
/// Projektordner des Laufs ab (`DASHBOARD_PROJECTS_DIR`, Testserver mit Kopie).
pub fn herkunft_pruefen(slug: &str, kiste_des_typs: &str, herkunft: Herkunft) -> Result<(), String> {
    let erwartet = format!("projekt-{slug}");
    if herkunft.kiste() != erwartet {
        return Err(format!("Das Modul „{slug}“ steht in der Kiste {}, erwartet ist {erwartet}.", herkunft.kiste()));
    }
    if kiste_des_typs != erwartet.replace('-', "_") {
        return Err(format!("Das Modul „{slug}“ stammt aus der Kiste {kiste_des_typs}, erwartet ist {erwartet}."));
    }
    let ordner = format!("projects/{slug}/server");
    if herkunft.ordner() != ordner {
        return Err(format!("Das Modul „{slug}“ liegt in {}, erwartet ist {ordner}.", herkunft.ordner()));
    }
    Ok(())
}

/// Ab so vielen Fehlern hintereinander steht ein Hook als „wiederholt“ im Protokoll.
pub const WIEDERHOLT_AB: u32 = 3;

fn sperren<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn jetzt_text() -> String {
    chrono::Local::now().format("%d.%m. %H:%M").to_string()
}

// ----- Störungen -----

/// Eine Störung an einer Stelle eines Moduls.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stoerung {
    /// Fehler hintereinander (ein guter Lauf hebt die Störung auf).
    pub folge: u32,
    /// Fehler seit dem Start des Dienstes.
    pub gesamt: u32,
    pub satz: String,
    /// Tag und Uhrzeit des letzten Fehlers.
    pub zuletzt: String,
}

/// Störungen aller Module, je (Slug, Stelle). Lebt nur im Arbeitsspeicher.
#[derive(Default)]
pub struct Stoerungen {
    karte: Mutex<BTreeMap<(String, String), Stoerung>>,
}

impl Stoerungen {
    pub fn melden(&self, slug: &str, wo: &str, satz: &str) {
        let mut karte = sperren(&self.karte);
        let s =
            karte.entry((slug.to_string(), wo.to_string())).or_insert_with(|| Stoerung { folge: 0, gesamt: 0, satz: String::new(), zuletzt: String::new() });
        s.folge += 1;
        s.gesamt += 1;
        s.satz = satz.chars().take(500).collect();
        s.zuletzt = jetzt_text();
        if s.folge >= WIEDERHOLT_AB {
            error!(projekt = %slug, "Modul {slug}, {wo}: wiederholt fehlgeschlagen ({}-mal hintereinander): {satz}", s.folge);
        } else {
            warn!(projekt = %slug, "Modul {slug}, {wo}: {satz}");
        }
    }

    /// Ein guter Lauf an dieser Stelle hebt die Störung auf.
    pub fn erfolg(&self, slug: &str, wo: &str) {
        if sperren(&self.karte).remove(&(slug.to_string(), wo.to_string())).is_some() {
            info!(projekt = %slug, "Modul {slug}, {wo}: wieder in Ordnung");
        }
    }

    pub fn alle(&self) -> Vec<((String, String), Stoerung)> {
        sperren(&self.karte).iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    /// Als Funde für /api/pruefung: (Slug, Befund).
    pub fn befunde(&self) -> Vec<(String, Befund)> {
        self.alle()
            .into_iter()
            .map(|((slug, wo), s)| {
                let wie_oft = if s.folge >= WIEDERHOLT_AB {
                    format!("wiederholt, {}-mal hintereinander", s.folge)
                } else if s.gesamt > 1 {
                    format!("{}-mal seit dem Start", s.gesamt)
                } else {
                    "einmal".to_string()
                };
                let text = format!("Backend-Modul, {wo}: {} ({wie_oft}, zuletzt {}).", s.satz.trim_end_matches('.'), s.zuletzt);
                (slug.clone(), Befund::fehler(format!("projects/{slug}/server/src/lib.rs"), None, text))
            })
            .collect()
    }
}

// ----- Register -----

/// Ein wartender Hook-Platz: höchstens ein Lauf, weitere Aufrufe warten.
#[derive(Default)]
struct Platz {
    riegel: Arc<tokio::sync::Mutex<()>>,
    wartend: Mutex<VecDeque<Aufruf>>,
}

struct Eintrag {
    modul: Arc<dyn Modul>,
    kontext: ProjektKontext,
    hooks: Hooks,
    plaetze: HashMap<HookArt, Arc<Platz>>,
    /// `nach_lesen`: höchstens ein Lauf zugleich.
    lesen: Arc<tokio::sync::Mutex<()>>,
    zeitplan_zuletzt: Mutex<Option<Instant>>,
    taeglich_zuletzt: Mutex<Option<String>>,
}

/// Alle eingehängten Module mit Kontext, Hooks und Störungen.
pub struct Register {
    eintraege: Vec<Eintrag>,
    stoerungen: Arc<Stoerungen>,
    /// Zeitlimit je Hook (30 s; Tests kürzer).
    pub zeitlimit: Duration,
    /// Zeitlimit für `nach_lesen` (5 s; eine Anfrage wartet).
    pub zeitlimit_lesen: Duration,
}

impl Register {
    /// Ohne Module (Tests, die keine brauchen).
    pub fn leer() -> Self {
        Self { eintraege: Vec::new(), stoerungen: Arc::default(), zeitlimit: hooks::ZEITLIMIT, zeitlimit_lesen: hooks::ZEITLIMIT_NACH_LESEN }
    }

    /// Register für den Zustand: Kontext über `DienstAnbindung` (Projektordner,
    /// Datenspeicher, Git-Zwischenspeicher des Zustands). Jedes Modul muss zu
    /// seinem Ordner passen (`herkunft_pruefen`), sonst bleibt es draußen.
    pub fn fuer_zustand(state: &AppState, module: Vec<ModulEintrag>) -> Self {
        Self::bauen(state, module, true)
    }

    /// Wie `fuer_zustand`, ohne die Herkunft zu prüfen: für Testmodule, die
    /// in der Kiste `dashboard` selbst stehen.
    #[cfg(test)]
    pub fn fuer_zustand_ungeprueft(state: &AppState, module: Vec<ModulEintrag>) -> Self {
        Self::bauen(state, module, false)
    }

    fn bauen(state: &AppState, module: Vec<ModulEintrag>, herkunft: bool) -> Self {
        let stoerungen = Arc::new(Stoerungen::default());
        let anbindung = Arc::new(anbindung::DienstAnbindung::neu(
            state.inhalt.clone(),
            state.speicher.clone(),
            state.speicher_satz(),
            state.git.clone(),
            stoerungen.clone(),
        ));

        Self::neu(module, anbindung, stoerungen, herkunft)
    }

    /// Register über einer beliebigen Anbindung. Module mit ungültigem oder
    /// doppeltem Slug bleiben draußen (Störung), mit `herkunft_pruefen_an` auch solche,
    /// deren Herkunft nicht passt.
    pub fn neu(module: Vec<ModulEintrag>, anbindung: Arc<dyn dashboard_projekt::Anbindung>, stoerungen: Arc<Stoerungen>, herkunft_pruefen_an: bool) -> Self {
        let mut eintraege: Vec<Eintrag> = Vec::new();
        for ModulEintrag { modul, kiste } in module {
            let slug = modul.slug();
            if !seite::is_valid_slug(slug) || slug.contains('_') {
                stoerungen.melden(slug, "register", "Der Slug des Moduls ist ungültig; das Modul ist nicht eingehängt.");
                continue;
            }
            if herkunft_pruefen_an && let Err(satz) = herkunft_pruefen(slug, kiste, modul.herkunft()) {
                stoerungen.melden(slug, "register", &format!("{satz} Das Modul ist nicht eingehängt."));
                continue;
            }
            if eintraege.iter().any(|e| e.modul.slug() == slug) {
                stoerungen.melden(slug, "register", "Das Modul steht doppelt im Register; nur das erste ist eingehängt.");
                continue;
            }
            let hooks = modul.hooks();
            let kontext = ProjektKontext::neu(slug, modul.tabellen(), anbindung.clone());
            let plaetze = [HookArt::NachDatenGeaendert, HookArt::Zeitplan, HookArt::Taeglich, HookArt::BeiVorgang, HookArt::NachBau]
                .into_iter()
                .filter(|a| hooks.hat(*a))
                .map(|a| (a, Arc::new(Platz::default())))
                .collect();
            eintraege.push(Eintrag {
                modul,
                kontext,
                hooks,
                plaetze,
                lesen: Arc::default(),
                zeitplan_zuletzt: Mutex::new(None),
                taeglich_zuletzt: Mutex::new(None),
            });
        }
        Self { eintraege, stoerungen, zeitlimit: hooks::ZEITLIMIT, zeitlimit_lesen: hooks::ZEITLIMIT_NACH_LESEN }
    }

    /// Mit anderen Zeitlimits (für Tests).
    pub fn mit_zeitlimits(mut self, hook: Duration, lesen: Duration) -> Self {
        self.zeitlimit = hook;
        self.zeitlimit_lesen = lesen;
        self
    }

    pub fn slugs(&self) -> Vec<&'static str> {
        self.eintraege.iter().map(|e| e.modul.slug()).collect()
    }

    pub fn module(&self) -> impl Iterator<Item = &Arc<dyn Modul>> {
        self.eintraege.iter().map(|e| &e.modul)
    }

    pub fn stoerungen(&self) -> &Stoerungen {
        &self.stoerungen
    }

    fn eintrag(&self, slug: &str) -> Option<(usize, &Eintrag)> {
        self.eintraege.iter().enumerate().find(|(_, e)| e.modul.slug() == slug)
    }

    /// Etappe B2: Darf GET diesen Weg (`/api/projekte/<slug>/x/…`) ohne
    /// Anmeldung lesen? Nur, wenn das Modul des Projekts ihn in
    /// `Modul::freie_wege` nennt; `{…}` steht für genau einen Abschnitt.
    pub fn frei_lesbar(&self, pfad: &str) -> bool {
        let Some(rest) = pfad.strip_prefix("/api/projekte/") else { return false };
        let Some((slug, nach)) = rest.split_once('/') else { return false };
        let weg = match nach.strip_prefix('x') {
            Some("") => "/",
            Some(w) if w.starts_with('/') => w,
            _ => return false,
        };
        let Some((_, e)) = self.eintrag(slug) else { return false };
        e.modul.freie_wege().iter().any(|muster| weg_passt(muster, weg))
    }

    /// Die Router aller Module unter `/api/projekte/<slug>/x`.
    pub fn router<S: Clone + Send + Sync + 'static>(&self) -> Router<S> {
        let mut r = Router::new();
        for e in &self.eintraege {
            let slug = e.modul.slug();
            // Hotfix Prozent, zweite Wand: Das Modul selbst verlangt die Anmeldung noch
            // einmal, am Weg nach dem Präfix, roh und dekodiert.
            let wand = axum::middleware::from_fn_with_state(e.modul.freie_wege(), modul_wand);
            r = r.nest(&format!("/api/projekte/{slug}/x"), rahmen::einpacken::<S>(&*e.modul, e.kontext.clone()).layer(wand));
        }
        r
    }

    /// Richtet die Tabellen aller Module ein (blockierend, beim Start).
    pub fn tabellen_einrichten(&self, speicher: &Speicher) {
        for e in &self.eintraege {
            let defs = e.modul.tabellen();
            if defs.is_empty() {
                continue;
            }
            let slug = e.modul.slug();
            match speicher.projekt_tabellen_einrichten(slug, &defs) {
                Ok(_) => self.stoerungen.erfolg(slug, "tabellen"),
                Err(err) => self.stoerungen.melden(slug, "tabellen", &format!("{err} {}", err.einzelheiten())),
            }
        }
    }

    // ----- Hooks -----

    /// Hat das Modul dieses Projekts einen Hook `nach_lesen`?
    pub fn hat_nach_lesen(&self, slug: &str) -> bool {
        self.eintrag(slug).is_some_and(|(_, e)| e.hooks.nach_lesen.is_some())
    }

    /// `nach_lesen` im Leseweg von 4v: gibt den verarbeiteten Inhalt zurück;
    /// bei Fehler, Absturz oder Zeitlimit den unveränderten (der Fehler steht
    /// dann in Protokoll und Prüfung, die Seite bekommt ihre Daten trotzdem).
    pub async fn nach_lesen(&self, slug: &str, name: &str, wert: serde_json::Value) -> serde_json::Value {
        let Some((_, e)) = self.eintrag(slug) else { return wert };
        if e.hooks.nach_lesen.is_none() {
            return wert;
        }
        let riegel = e.lesen.clone();
        let (k, h, n, w) = (e.kontext.clone(), e.hooks, name.to_string(), wert.clone());
        let lauf = async move {
            let halten = riegel.lock_owned().await;
            tokio::task::spawn_blocking(move || {
                let _halten = halten;
                let mut w = w;
                h.nach_lesen_ausfuehren(&k, &n, &mut w).map(|()| w)
            })
            .await
        };
        match tokio::time::timeout(self.zeitlimit_lesen, lauf).await {
            Ok(Ok(Ok(neu))) => {
                self.stoerungen.erfolg(slug, "nach_lesen (Zeitlimit)");
                neu
            }
            Ok(Ok(Err(_))) => wert,
            Ok(Err(join)) => {
                e.kontext.fehler_melden("nach_lesen", &format!("abgebrochen: {join}"));
                wert
            }
            Err(_) => {
                e.kontext.fehler_melden(
                    "nach_lesen (Zeitlimit)",
                    &format!("Zeitlimit von {} s überschritten; die Daten gingen unverarbeitet hinaus", self.zeitlimit_lesen.as_secs_f32()),
                );
                wert
            }
        }
    }

    /// Stößt einen Hook eines Moduls an (falls es ihn hat). Gleiche wartende
    /// Aufrufe werden zusammengelegt; läuft der Hook schon, wartet der Aufruf.
    pub fn ausloesen(self: &Arc<Self>, slug: &str, aufruf: Aufruf) {
        let Some((idx, e)) = self.eintrag(slug) else { return };
        let Some(platz) = e.plaetze.get(&aufruf.art()) else { return };
        {
            let mut w = sperren(&platz.wartend);
            if !w.contains(&aufruf) {
                w.push_back(aufruf.clone());
            }
        }
        self.arbeiter(idx, aufruf.art());
    }

    fn arbeiter(self: &Arc<Self>, idx: usize, art: HookArt) {
        let Some(platz) = self.eintraege[idx].plaetze.get(&art).cloned() else { return };
        let Ok(riegel) = platz.riegel.clone().try_lock_owned() else { return };
        let reg = self.clone();
        tokio::spawn(async move {
            loop {
                let naechster = sperren(&platz.wartend).pop_front();
                let Some(aufruf) = naechster else { break };
                reg.einmal(idx, aufruf).await;
            }
            drop(riegel);
            // Kam zwischen dem letzten Blick und der Freigabe etwas dazu?
            if !sperren(&platz.wartend).is_empty() {
                reg.arbeiter(idx, art);
            }
        });
    }

    /// Ein Lauf: `spawn_blocking` mit Zeitlimit. Nach dem Zeitlimit wird der
    /// Lauf zu Ende abgewartet (ein Thread lässt sich nicht abbrechen), damit
    /// nie zwei Läufe desselben Hooks zugleich laufen.
    async fn einmal(&self, idx: usize, aufruf: Aufruf) {
        let e = &self.eintraege[idx];
        let art = aufruf.art();
        let (k, h) = (e.kontext.clone(), e.hooks);
        let mut lauf = tokio::task::spawn_blocking(move || h.ausfuehren(&k, &aufruf));
        let zeitlimit_wo = format!("{} (Zeitlimit)", art.name());
        match tokio::time::timeout(self.zeitlimit, &mut lauf).await {
            Ok(Ok(_)) => self.stoerungen.erfolg(e.modul.slug(), &zeitlimit_wo),
            Ok(Err(join)) => e.kontext.fehler_melden(art.name(), &format!("abgebrochen: {join}")),
            Err(_) => {
                e.kontext
                    .fehler_melden(&zeitlimit_wo, &format!("Zeitlimit von {} s überschritten; der Lauf wird zu Ende abgewartet", self.zeitlimit.as_secs_f32()));
                let _ = lauf.await;
            }
        }
    }

    /// Minutentakt: `zeitplan` (je Modul nach seinem Abstand, mindestens
    /// 5 Minuten) und `taeglich` (einmal am Tag, sobald heute gesichert ist).
    pub fn takt(self: &Arc<Self>, jetzt: Instant, heute: &str, heute_gesichert: bool) {
        for e in &self.eintraege {
            let slug = e.modul.slug();
            if let Some(abstand) = e.hooks.zeitplan_abstand() {
                let faellig = {
                    let mut z = sperren(&e.zeitplan_zuletzt);
                    let faellig = z.is_none_or(|t| jetzt.duration_since(t) >= abstand);
                    if faellig {
                        *z = Some(jetzt);
                    }
                    faellig
                };
                if faellig {
                    self.ausloesen(slug, Aufruf::Zeitplan);
                }
            }
            if e.hooks.taeglich.is_some() && heute_gesichert {
                let faellig = {
                    let mut t = sperren(&e.taeglich_zuletzt);
                    let faellig = t.as_deref() != Some(heute);
                    if faellig {
                        *t = Some(heute.to_string());
                    }
                    faellig
                };
                if faellig {
                    self.ausloesen(slug, Aufruf::Taeglich);
                }
            }
        }
    }

    /// Nach jedem Bau der Oberfläche (aus dem Baumeister).
    pub fn nach_bau(self: &Arc<Self>, gruen: bool) {
        let ergebnis = if gruen { BauErgebnis::Gruen } else { BauErgebnis::Rot };
        for slug in self.slugs() {
            self.ausloesen(slug, Aufruf::NachBau(ergebnis));
        }
    }

    /// Eine Änderung aus dem Beobachter: `projects/<slug>/daten/<name>.toml`
    /// eines Moduls mit `nach_daten_geaendert` → (Slug, Name).
    fn daten_pfad(&self, pfad: &str) -> Option<(&'static str, String)> {
        let rest = pfad.strip_prefix("projects/")?;
        let (slug, rest) = rest.split_once('/')?;
        let name = rest.strip_prefix("daten/")?.strip_suffix(".toml")?;
        if name.contains('/') || name.is_empty() {
            return None;
        }
        let (_, e) = self.eintrag(slug)?;
        e.hooks.nach_daten_geaendert.is_some().then(|| (e.modul.slug(), name.to_string()))
    }

    /// Startet die Hintergrundaufgaben (Beobachter, Vorgänge, Minutentakt).
    /// Ohne Module tut es nichts.
    pub fn starten(self: &Arc<Self>, state: &AppState) {
        if self.eintraege.is_empty() {
            return;
        }
        info!("Backend-Module: {}", self.slugs().join(", "));
        if self.eintraege.iter().any(|e| e.hooks.nach_daten_geaendert.is_some()) {
            self.beobachter_starten(state);
        }
        if self.eintraege.iter().any(|e| e.hooks.bei_vorgang.is_some()) {
            self.vorgaenge_starten(state);
        }
        if self.eintraege.iter().any(|e| e.hooks.zeitplan.is_some() || e.hooks.taeglich.is_some()) {
            let reg = self.clone();
            let speicher = state.speicher.clone();
            let abschaltung = state.abschaltung();
            tokio::spawn(async move {
                tokio::pin!(abschaltung);
                let start = tokio::time::Instant::now() + TAKT;
                let mut takt = tokio::time::interval_at(start, TAKT);
                takt.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
                loop {
                    tokio::select! {
                        _ = takt.tick() => {}
                        _ = &mut abschaltung => return,
                    }
                    let heute = crate::speicher::sicherung::heute();
                    // Ohne Datenspeicher gibt es keine Sicherung; dann gilt der Tag als gesichert.
                    let gesichert = match &speicher {
                        Some(s) => {
                            let s = s.clone();
                            tokio::task::spawn_blocking(move || s.letzte_sicherung()).await.ok().flatten().as_deref() == Some(heute.as_str())
                        }
                        None => true,
                    };
                    reg.takt(Instant::now(), &heute, gesichert);
                }
            });
        }
    }

    fn beobachter_starten(self: &Arc<Self>, state: &AppState) {
        let reg = self.clone();
        let mut rx = state.events.subscribe();
        let abschaltung = state.abschaltung();
        tokio::spawn(async move {
            tokio::pin!(abschaltung);
            let mut faellig: BTreeMap<(&'static str, String), tokio::time::Instant> = BTreeMap::new();
            loop {
                let naechste = faellig.values().min().copied();
                let schlafen = async move {
                    match naechste {
                        Some(t) => tokio::time::sleep_until(t).await,
                        None => std::future::pending::<()>().await,
                    }
                };
                tokio::select! {
                    _ = &mut abschaltung => return,
                    _ = schlafen => {
                        let jetzt = tokio::time::Instant::now();
                        let reif: Vec<(&'static str, String)> = faellig.iter().filter(|(_, t)| **t <= jetzt).map(|(k, _)| k.clone()).collect();
                        for (slug, name) in reif {
                            faellig.remove(&(slug, name.clone()));
                            reg.ausloesen(slug, Aufruf::NachDatenGeaendert(name));
                        }
                    }
                    empfangen = rx.recv() => match empfangen {
                        Ok(c) => {
                            if let Some(k) = reg.daten_pfad(&c.path) {
                                faellig.insert(k, tokio::time::Instant::now() + hooks::ENTPRELLEN);
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            warn!("Backend-Module: {n} Dateiänderungen verpasst");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                    },
                }
            }
        });
    }

    fn vorgaenge_starten(self: &Arc<Self>, state: &AppState) {
        let reg = self.clone();
        let mut rx = state.vorgang_events.subscribe();
        let abschaltung = state.abschaltung();
        tokio::spawn(async move {
            tokio::pin!(abschaltung);
            loop {
                let empfangen = tokio::select! {
                    _ = &mut abschaltung => return,
                    e = rx.recv() => e,
                };
                match empfangen {
                    Ok(v) => {
                        let info = VorgangInfo { id: v.id.clone(), art: v.art.wort().to_string(), zustand: zustand_wort(v.zustand).to_string() };
                        reg.ausloesen(&v.slug, Aufruf::BeiVorgang(info));
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => warn!("Backend-Module: {n} Vorgangsschritte verpasst"),
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                }
            }
        });
    }
}

/// Abstand des Minutentakts.
pub const TAKT: Duration = Duration::from_secs(60);

fn zustand_wort(z: crate::api::vertrag::Zustand) -> &'static str {
    use crate::api::vertrag::Zustand;
    match z {
        Zustand::Offen => "offen",
        Zustand::Zugestellt => "zugestellt",
        Zustand::Uebernommen => "uebernommen",
        Zustand::Erledigt => "erledigt",
    }
}

/// Ist der Weg im Modul (nach `/api/projekte/<slug>/x`) frei lesbar? Nur, wenn
/// er roh und dekodiert zu einem freien Weg passt; ein Weg, der sich nicht
/// dekodieren lässt oder doppelt kodiert ist, ist nie frei.
pub fn modul_weg_frei(freie_wege: &[&str], weg: &str) -> bool {
    let weg = if weg.is_empty() { "/" } else { weg };
    let passt = |w: &str| freie_wege.iter().any(|m| weg_passt(m, w));
    passt(weg) && crate::sicherheit::pfad_dekodiert(weg).is_some_and(|d| passt(&d))
}

/// Hotfix Prozent, zweite Wand der Modul-Wege (die erste ist
/// `sicherheit::anmeldung_pflicht` mit `Register::frei_lesbar`): ohne
/// `Angemeldet` nur GET/HEAD/OPTIONS auf einen freien Weg, sonst 401 mit
/// demselben Satz wie die Schleuse (`bitte_anmelden`).
async fn modul_wand(
    axum::extract::State(freie_wege): axum::extract::State<&'static [&'static str]>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::http::Method;
    let lesend = matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    let angemeldet = req.extensions().get::<crate::anmeldung::Angemeldet>().is_some();
    if !angemeldet && !(lesend && modul_weg_frei(freie_wege, req.uri().path())) {
        return crate::api::anmeldung::bitte_anmelden(None);
    }
    next.run(req).await
}

/// Passt der Weg `weg` (ohne Abfrage) zum Muster eines freien Wegs? `{…}`
/// steht für genau einen nicht leeren Abschnitt; sonst muss alles gleich sein.
pub fn weg_passt(muster: &str, weg: &str) -> bool {
    let m: Vec<&str> = muster.trim_end_matches('/').split('/').collect();
    let w: Vec<&str> = weg.trim_end_matches('/').split('/').collect();
    m.len() == w.len() && m.iter().zip(&w).all(|(m, w)| if m.starts_with('{') && m.ends_with('}') { !w.is_empty() } else { m == w })
}

#[cfg(test)]
mod tests;
