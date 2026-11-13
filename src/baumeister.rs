//! Der Baumeister des Dienstes (Etappe 4b, vorher die kleine Form aus 4v).
//! Er entscheidet, *wann* gebaut wird; prüfen und bauen tut der Bote
//! (`Anfrage::Bau`, siehe `bote::bau`). Der Dienst selbst schreibt nichts ins
//! Repo.
//!
//! Zwei Wege in **eine Warteschlange**:
//!
//! - **Der Beobachter** ([`beobachter_starten`]) schaut jede Sekunde auf die
//!   `.svelte`- und `.ts`-Dateien aller Projekte (aller Inhaltsquellen; TOML
//!   nie). Nach [`RUHE`] (5 s) ohne weitere Änderung meldet er die geänderten
//!   Projekte ([`Baumeister::aenderung`]).
//! - **Der Vorgang `bau`** eines Chats, wie seit 4v. Nach 4b nur noch nötig,
//!   wenn der Beobachter etwas verpasst hat.
//!
//! Die Schleife ([`starten`]) baut **höchstens einmal zugleich**. Was während
//! eines Baus dazukommt, wartet und wird danach mit **genau einem** weiteren
//! Bau abgedeckt. Jeder Bau hat eine Frist ([`Baumeister::frist`]); danach
//! gilt er als rot (den Prozessbaum beendet der Bote nach 120 s selbst).
//!
//! Nach einem Bau:
//!
//! - grün: der Zeiger im Speicher (`WebOrt::umstellen`) zeigt sofort auf den
//!   neuen Ordner (der Bote hat `web/builds/aktuell` schon getauscht), Ereignis
//!   `fassung` im Live-Kanal;
//! - grün, aber mit anderem Grundbau als die eingespielte Oberfläche
//!   (`WebOrt::wartender_bau`): gilt als rot mit [`crate::app::SATZ_BAU_WARTET`];
//!   ausgeliefert wird weiter der letzte gültige Bau, bis Dienst.ps1 -Neustart;
//! - je Projekt, das rot wurde: Ereignis `bau` mit Zustand `rot` und einem Satz
//!   in einfachem Deutsch; die Seite bleibt in ihrer letzten grünen Fassung;
//! - rot: die alte Fassung bleibt, und auf der Startseite steht eine Meldung
//!   (`meldungen::bau_ergebnis_4b`). Fehlt der Bote, gilt nur die Meldung
//!   „Bote fehlt“.
//! - Kleinkram 2: Die Vorgänge `bau` dieser Runde setzt der Baumeister selbst
//!   auf `erledigt`, mit dem Ergebnis (`bau`: grün oder rot und Meldung, der
//!   Satz in `notiz`, die Zeit in `geaendert`); das Warten des Chats liefert
//!   es trotzdem einmal (`Speicher::bau_ergebnisse`).
//!
//! Ein Neustart des Dienstes baut nicht: `WebOrt` liest `web/builds/aktuell`.
//! Liegen beim Start Vorgänge auf `laeuft` (Dienst während eines Baus
//! beendet), baut die erste Runde sie neu.

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::Notify;
use tracing::{info, warn};

use crate::api::events::{BauEreignis, FassungEreignis};
use crate::api::vertrag::{BauStand, BauZustand, Ereignis, Vorgang};
use crate::app::AppState;
use crate::bote::client::Bote;
use crate::quellen::Quellen;
use crate::speicher::{Speicher, SpeicherFehler};
use crate::vertrag::bote::{Anfrage, Antwort, BauErgebnis, BauFortschritt};

/// Wer im Journal als Herkunft der Bau-Schritte steht.
pub const VON: &str = crate::speicher::vorgaenge::BAU_ERLEDIGT_VON;
/// Auch ohne Anstoß schaut die Schleife so oft nach (etwa nach einem Fehler des Datenspeichers).
pub const NACHSEHEN: Duration = Duration::from_secs(60);
/// Höchstlänge der Meldung im Vorgang (Satz und Zeilen der Ausgabe).
pub const MELDUNG_HOECHSTENS: usize = 4000;
/// Höchstlänge der Notiz im Ereignis (wie beim Erledigen über die API).
pub const NOTIZ_HOECHSTENS: usize = 500;
/// So lange Ruhe nach der letzten Änderung, bevor gebaut wird.
pub const RUHE: Duration = Duration::from_secs(5);
/// So oft schaut der Beobachter auf die Dateien.
pub const BLICK: Duration = Duration::from_secs(1);
/// Umgebungsvariable: `direkt` baut ohne Boten im eigenen Prozess (nur ohne
/// Dienst: Testkopie oder lokaler Start; nie im Dienst).
pub const UMGEBUNG_DIREKT: &str = "DASHBOARD_BAUMEISTER";

/// Bau-Tempo, Zweig C: So oft fragt der Baumeister während eines Baus nach dem Fortschritt.
pub const FORTSCHRITT_TAKT: Duration = if cfg!(test) { Duration::from_millis(20) } else { Duration::from_millis(1500) };

/// Satz beim Annehmen.
pub const SATZ_LAEUFT: &str = "Der Bau läuft (der Bote prüft die geänderten Seiten und baut die Oberfläche).";

/// Etappe 4c: Satz für ein Projekt, das vor dem Bau schon rot war und es bleibt.
pub const SATZ_NOCH_ROT: &str = "Die Seite hat noch einen Fehler; es bleibt bei der letzten guten Fassung.";

/// Eine Bau-Funktion: in Betrieb über den Boten, in Tests eine Attrappe.
pub type Bauen = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = BauErgebnis> + Send>> + Send + Sync>;

/// Zustand des Baumeisters im `AppState`: die Warteschlange.
pub struct Baumeister {
    anstoss: Notify,
    laeufe: AtomicU64,
    /// Projekte, deren Änderung der Beobachter gemeldet hat, seit dem letzten Bau.
    bedarf: Mutex<BTreeSet<String>>,
    /// Etappe 4c: Projekte, für die gerade gebaut wird (für den Bau-Stand).
    laufend: Mutex<BTreeSet<String>>,
    /// Frist für einen ganzen Bau im Dienst (Prüfen und Bauen).
    frist: Duration,
}

impl Default for Baumeister {
    fn default() -> Self {
        Baumeister {
            anstoss: Notify::new(),
            laeufe: AtomicU64::new(0),
            bedarf: Mutex::new(BTreeSet::new()),
            laufend: Mutex::new(BTreeSet::new()),
            frist: crate::bote::client::FRIST_BAU,
        }
    }
}

impl Baumeister {
    pub fn neu() -> Baumeister {
        Baumeister::default()
    }

    /// Mit eigener Frist (Tests).
    pub fn mit_frist(frist: Duration) -> Baumeister {
        Baumeister { frist, ..Baumeister::default() }
    }

    pub fn frist(&self) -> Duration {
        self.frist
    }

    /// Ein neuer Bau-Vorgang ist da: die Schleife soll nachsehen. Läuft gerade
    /// ein Bau, schaut sie gleich danach nach.
    pub fn anstossen(&self) {
        self.anstoss.notify_one();
    }

    /// Der Beobachter meldet geänderte Projekte: ein Bau steht an. Mehrere
    /// Meldungen während eines Baus ergeben genau einen weiteren Bau.
    pub fn aenderung<I: IntoIterator<Item = String>>(&self, slugs: I) {
        self.bedarf.lock().unwrap_or_else(|e| e.into_inner()).extend(slugs);
        self.anstoss.notify_one();
    }

    /// Etappe 4c: Projekte, die bestellt sind oder gerade gebaut werden.
    pub fn im_bau(&self) -> BTreeSet<String> {
        let mut alle = self.bedarf.lock().unwrap_or_else(|e| e.into_inner()).clone();
        alle.extend(self.laufend.lock().unwrap_or_else(|e| e.into_inner()).iter().cloned());
        alle
    }

    /// Nimmt die gemeldeten Projekte (danach ist der Bedarf leer). Abnahme
    /// 4c: Sie gelten im selben Schritt als „im Bau“, damit der Bau-Stand
    /// nicht kurz zurückspringt (erst bedarf, dann laufend sperren, wie `im_bau`).
    fn nehmen(&self) -> BTreeSet<String> {
        let mut bedarf = self.bedarf.lock().unwrap_or_else(|e| e.into_inner());
        let anlass = std::mem::take(&mut *bedarf);
        self.laufend.lock().unwrap_or_else(|e| e.into_inner()).extend(anlass.iter().cloned());
        anlass
    }

    /// Etappe 4c: Der Bau ist fertig; nichts läuft mehr (der Bedarf bleibt).
    fn laufend_leeren(&self) {
        self.laufend.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    /// Etappe 4c: Diese Projekte werden gerade gebaut (dazu die der Vorgänge).
    fn laufend_dazu(&self, slugs: &BTreeSet<String>) {
        self.laufend.lock().unwrap_or_else(|e| e.into_inner()).extend(slugs.iter().cloned());
    }

    /// Wie viele Bauten seit dem Start (für Tests und Protokoll).
    pub fn laeufe(&self) -> u64 {
        self.laeufe.load(Ordering::Relaxed)
    }
}

/// Ein rotes Ergebnis ohne Bau, mit Satz.
pub fn rot(satz: impl Into<String>) -> BauErgebnis {
    BauErgebnis { satz: satz.into(), ..Default::default() }
}

/// Bauen über den Boten. Ohne Boten (Altweg ohne Einstellungen) wird nichts
/// gebaut; die Vorgänge werden mit einem Satz rot. Ist der Bote nicht
/// erreichbar, bleibt die letzte gute Fassung, und es gilt „Bote fehlt“.
pub fn ueber_boten(bote: Option<Arc<Bote>>) -> Bauen {
    Arc::new(move || {
        let bote = bote.clone();
        Box::pin(async move {
            let Some(b) = bote else {
                return rot("Ohne Boten baut das Dashboard nicht (es läuft ohne Einstellungsdatei). Gebaut wird dann mit Dienst.ps1 -Neustart.");
            };
            match b.anfragen(&Anfrage::Bau).await {
                Ok(Antwort::Bau(e)) => e,
                Ok(Antwort::Fehler { satz }) => rot(format!("Der Bote hat nicht gebaut: {satz}")),
                Ok(a) => rot(format!("Der Bote hat unerwartet geantwortet ({}).", a.art())),
                Err(satz) => BauErgebnis { bote_fehlt: true, ..rot(format!("Der Bote ist nicht erreichbar: {satz}. Die letzte gute Fassung bleibt.")) },
            }
        })
    })
}

/// Bauen im eigenen Prozess, ohne Boten (`DASHBOARD_BAUMEISTER=direkt`): nur
/// ohne Dienst (Testkopie oder lokaler Start), nie im Dienst. Gleicher Ablauf wie
/// im Boten (`bote::bau::bauen`).
pub fn direkt(ort: crate::bote::bau::BauOrt) -> Bauen {
    Arc::new(move || {
        let ort = ort.clone();
        Box::pin(async move { crate::bote::bau::bauen(&ort, crate::bote::bau::ZEITLIMIT).await })
    })
}

/// Ist der direkte Weg eingeschaltet?
pub fn direkt_gewollt() -> bool {
    std::env::var(UMGEBUNG_DIREKT).is_ok_and(|v| v.trim().eq_ignore_ascii_case("direkt"))
}

/// Startet die Schleife im Hintergrund (eine je Dienst).
pub fn starten(state: AppState, bauen: Bauen) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let abschaltung = state.abschaltung();
        tokio::pin!(abschaltung);
        loop {
            // Erst anmelden, dann nachsehen: ein Anstoß dazwischen geht nicht verloren.
            let angestossen = state.baumeister.anstoss.notified();
            tokio::pin!(angestossen);
            angestossen.as_mut().enable();
            match eine_runde(&state, &bauen).await {
                // Gleich noch einmal: was während des Baus kam, wartet.
                Ok(n) if n > 0 => continue,
                Ok(_) => {}
                Err(e) => warn!("Baumeister: {e} ({})", e.einzelheiten()),
            }
            tokio::select! {
                _ = &mut angestossen => {}
                _ = tokio::time::sleep(NACHSEHEN) => {}
                _ = &mut abschaltung => return,
            }
        }
    })
}

/// Schneidet `text` auf höchstens `max` Zeichen.
fn kuerzen(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut t: String = text.chars().take(max.saturating_sub(1)).collect();
    t.push('…');
    t
}

/// Die Meldung für den Vorgang: der Satz, bei rot dazu das Ende der Ausgabe.
pub fn meldung(e: &BauErgebnis) -> String {
    let mut m = e.satz.clone();
    if !e.gruen
        && let Some(ende) = e.ausgabe_ende.as_deref().map(str::trim).filter(|t| !t.is_empty())
    {
        m.push_str("\n\nEnde der Ausgabe:\n");
        m.push_str(ende);
    }
    kuerzen(&m, MELDUNG_HOECHSTENS)
}

/// Schreibt die Schritte und meldet sie: Live-Ereignis `vorgang` und Warter
/// je Vorgang.
fn vorgaenge_melden(state: &AppState, geaendert: &[(Vorgang, Ereignis)]) {
    for (_, e) in geaendert {
        crate::api::vorgaenge::melden(state, e);
    }
}

/// Live-Ereignis `bau` je Slug einmal. `gescheitert`: Der ganze Bau lief nicht durch.
fn bau_melden(state: &AppState, slugs: &BTreeSet<String>, zustand: BauZustand, satz: &str, gescheitert: bool) {
    for slug in slugs {
        let _ = state.bau_events.send(BauEreignis { slug: slug.clone(), zustand, meldung: satz.to_string(), gescheitert });
    }
}

/// Abnahme 4c: Satz im Ereignis `bau` für die Oberfläche, ohne Pfade und
/// ohne Befehle (den vollen Satz behalten Vorgang und Meldung).
pub fn ereignis_satz(e: &BauErgebnis, frist_um: bool) -> String {
    if e.gruen {
        return format!("Gebaut in {} s.", e.dauer_ms / 1000);
    }
    if frist_um {
        return SATZ_ZU_LANGE.to_string();
    }
    if e.bote_fehlt {
        return SATZ_BOTE_FEHLT.to_string();
    }
    SATZ_NICHT_GEBAUT.to_string()
}

/// Ereignis bei rot: Der Bau hat zu lange gedauert.
pub const SATZ_ZU_LANGE: &str = "Der Bau hat zu lange gedauert. Die letzte gute Fassung bleibt.";
/// Ereignis bei rot: Der Bote ist nicht erreichbar.
pub const SATZ_BOTE_FEHLT: &str = "Der Bote ist gerade nicht erreichbar. Die letzte gute Fassung bleibt.";
/// Ereignis, wenn ein grüner Bau auf den Neustart des Dienstes wartet.
pub const SATZ_WARTET: &str = "Die neue Fassung passt erst nach einem Neustart des Dienstes. Die letzte gute Fassung bleibt.";
/// Ereignis bei rot aus anderem Grund.
pub const SATZ_NICHT_GEBAUT: &str = "Die Oberfläche ließ sich nicht bauen. Die letzte gute Fassung bleibt; mehr steht in der Meldung auf der Startseite.";
/// Ereignis für ein Projekt, das in diesem Bau rot wurde.
pub const SATZ_WIRD_ROT: &str = "Die neue Fassung der Seite hat einen Fehler.";

/// Eine Runde: alle wartenden Bau-Vorgänge und alle gemeldeten Änderungen
/// mit einem Bau abdecken. Gibt die Zahl der Anlässe zurück (0: es gab keine).
pub async fn eine_runde(state: &AppState, bauen: &Bauen) -> Result<usize, SpeicherFehler> {
    let anlass = state.baumeister.nehmen();
    // Etappe 4c: „läuft“ gilt ab hier (nehmen) bis die Runde endet, auch bei Fehlern.
    let _laufend = Laufend(&state.baumeister);
    let offen = match state.speicher.clone() {
        Some(s) => match Speicher::im_hintergrund(s, |s| s.offene_bauten()).await {
            Ok(o) => o,
            Err(e) if !anlass.is_empty() => {
                warn!("Baumeister: Bau-Vorgänge nicht lesbar ({e}); gebaut wird für die Änderungen.");
                Vec::new()
            }
            Err(e) => return Err(e),
        },
        None => Vec::new(),
    };
    if offen.is_empty() && anlass.is_empty() {
        return Ok(0);
    }
    let ids: Vec<String> = offen.iter().map(|v| v.id.clone()).collect();
    let mut slugs: BTreeSet<String> = offen.iter().map(|v| v.slug.clone()).collect();
    slugs.extend(anlass.iter().cloned());
    info!(
        "Baumeister: Bau für {} Vorgang/Vorgänge ({}) und {} geänderte Projekte ({})",
        ids.len(),
        ids.join(", "),
        anlass.len(),
        anlass.iter().cloned().collect::<Vec<_>>().join(", ")
    );
    state.baumeister.laufend_dazu(&slugs);
    let speicher = state.speicher.clone();
    if let Some(s) = speicher.clone().filter(|_| !ids.is_empty()) {
        let laeuft = BauStand { zustand: BauZustand::Laeuft, meldung: SATZ_LAEUFT.to_string(), rote: Vec::new() };
        let i2 = ids.clone();
        let geaendert = Speicher::im_hintergrund(s, move |s| s.bau_setzen(&i2, &laeuft, VON, SATZ_LAEUFT)).await?;
        vorgaenge_melden(state, &geaendert);
    }
    bau_melden(state, &slugs, BauZustand::Laeuft, SATZ_LAEUFT, false);

    let frist = state.baumeister.frist;
    let (mut ergebnis, frist_um) = match tokio::time::timeout(frist, mit_fortschritt(state, bauen(), || fortschritt_fragen(state))).await {
        Ok(e) => (e, false),
        Err(_) => (rot(format!("Der Bau hat länger als {} s gedauert und gilt als rot. Die letzte gute Fassung bleibt.", frist.as_secs())), true),
    };
    state.baumeister.laeufe.fetch_add(1, Ordering::Relaxed);
    info!("Baumeister: {} ({})", ergebnis.satz, crate::bote::bau::schritte_text(&ergebnis.schritte));

    // Grün: Zeiger im Speicher sofort umstellen, Ereignis `fassung`.
    let mut wartet = false;
    if ergebnis.gruen
        && let Some(name) = ergebnis.ordner.clone()
    {
        match state.web.umstellen(&name) {
            Ok(ordner) => {
                info!("Baumeister: neue Fassung {}", ordner.display());
                let _ = state.fassung_events.send(FassungEreignis { ordner: name, dauer_ms: ergebnis.dauer_ms });
            }
            // Der Bote hat den Zeiger in der Datei schon umgestellt. Passt der
            // Grundbau nicht zur eingespielten Oberfläche, bleibt der letzte
            // gültige Bau (`WebOrt::ordner`), und der Bau gilt nicht als grün.
            Err(grund) if state.web.wartender_bau().is_some() => {
                warn!("Baumeister: Der neue Ordner wartet auf Dienst.ps1 -Neustart ({grund}).");
                wartet = true;
            }
            // Sonst gilt der Ordner hier nicht; `WebOrt` liefert die eingespielte Oberfläche.
            Err(grund) => warn!("Baumeister: Der neue Ordner gilt hier nicht ({grund}); ausgeliefert wird die eingespielte Oberfläche."),
        }
    }
    if wartet {
        ergebnis.gruen = false;
        ergebnis.satz = crate::app::SATZ_BAU_WARTET.to_string();
        ergebnis.ausgabe_ende = None;
    }
    let zustand = if ergebnis.gruen { BauZustand::Gruen } else { BauZustand::Rot };
    let satz = kuerzen(&ergebnis.satz, NOTIZ_HOECHSTENS);

    // Je Projekt, das rot wurde: `bau` mit Zustand rot (Abnahme 4c: fester
    // Satz ohne Pfade; den Fund nennen Vorgang und Meldung).
    let gescheitert = !ergebnis.gruen;
    let ereignis = if wartet { SATZ_WARTET.to_string() } else { ereignis_satz(&ergebnis, frist_um) };
    // Abnahme 4c (Nachbesserung 2): „läuft“ endet, bevor die Ergebnisse
    // hinausgehen. Lädt die Oberfläche danach die Liste (vor den Schritten
    // mit await unten), sagt sie nicht wieder „Baut …“. Was während des Baus
    // neu bestellt wurde, steht weiter im Bedarf und bleibt „läuft“.
    state.baumeister.laufend_leeren();
    let mut rot_gemeldet = BTreeSet::new();
    for r in &ergebnis.neu_rot {
        let meldung = if gescheitert { ereignis.clone() } else { SATZ_WIRD_ROT.to_string() };
        let _ = state.bau_events.send(BauEreignis { slug: r.slug.clone(), zustand: BauZustand::Rot, meldung, gescheitert });
        rot_gemeldet.insert(r.slug.clone());
    }
    // Etappe 4c: Projekte, die schon vorher rot waren und es bleiben, melden
    // sich auch bei grünem Bau rot (vorher kam für sie „grün“).
    let noch_rot = |s: &String| ergebnis.alte_fassung.contains(s) || ergebnis.rote.contains(s);
    let rest: BTreeSet<String> = slugs.iter().filter(|s| !rot_gemeldet.contains(*s) && !noch_rot(s)).cloned().collect();
    let weiter_rot: BTreeSet<String> = slugs.iter().filter(|s| !rot_gemeldet.contains(*s) && noch_rot(s)).cloned().collect();
    bau_melden(state, &rest, zustand, &ereignis, gescheitert);
    bau_melden(state, &weiter_rot, BauZustand::Rot, if gescheitert { &ereignis } else { SATZ_NOCH_ROT }, gescheitert);

    if let Some(s) = speicher.filter(|_| !ids.is_empty()) {
        let stand = BauStand { zustand, meldung: meldung(&ergebnis), rote: ergebnis.rote.clone() };
        let (i2, n2) = (ids.clone(), satz.clone());
        let geaendert = Speicher::im_hintergrund(s, move |s| s.bau_setzen(&i2, &stand, VON, &n2)).await?;
        vorgaenge_melden(state, &geaendert);
    }
    // Etappe A3/4b: Meldungen für den Inhaber (Startseite); fehlt der Bote, gilt „Bote fehlt“.
    if !ergebnis.bote_fehlt && state.speicher.is_some() {
        let slugs: Vec<String> = slugs.into_iter().collect();
        crate::meldungen::bau_ergebnis_4b(state, &slugs, &ergebnis, &satz, crate::speicher::aufgaben::jetzt()).await;
    }
    // Etappe 4b: Hook `nach_bau` der Backend-Module (eigene Aufgabe, wartet nicht).
    state.projekte.nach_bau(ergebnis.gruen);
    Ok(ids.len() + anlass.len())
}

/// Zweig C: Wie weit ist der Bau? Über den Boten (eigene Verbindung), ohne
/// Boten der Zähler im eigenen Prozess (direkter Weg der Testkopie).
async fn fortschritt_fragen(state: &AppState) -> Option<BauFortschritt> {
    match &state.bote {
        Some(b) => b.fortschritt().await,
        None => crate::bote::bau::FORTSCHRITT.jetzt(),
    }
}

/// Zweig C: Baut und fragt daneben alle [`FORTSCHRITT_TAKT`] nach dem
/// Fortschritt; jede Änderung geht als Ereignis `bau-fortschritt` (nur
/// Zahlen) in den geschützten Kanal. Kein Fortschritt (älterer Bote) heißt
/// nur: kein Ereignis; der Bau läuft weiter.
pub async fn mit_fortschritt<Q: Future<Output = Option<BauFortschritt>>>(
    state: &AppState,
    bau: impl Future<Output = BauErgebnis>,
    frage: impl Fn() -> Q,
) -> BauErgebnis {
    tokio::pin!(bau);
    let mut takt = tokio::time::interval(FORTSCHRITT_TAKT);
    takt.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut zuletzt = None;
    loop {
        tokio::select! {
            biased; // der Bau zuerst: seine Anfrage geht vor der ersten Frage raus
            e = &mut bau => return e,
            _ = takt.tick() => {
                let f = frage().await;
                if let Some(neu) = f.filter(|_| f != zuletzt) {
                    let _ = state.bau_fortschritt.send(neu);
                }
                zuletzt = f;
            }
        }
    }
}

/// Etappe 4c: leert `Baumeister::laufend`, wenn die Runde endet; Sicherheitsnetz
/// für Fehler (`?`), im Normalfall ist es vor den End-Ereignissen schon leer.
struct Laufend<'a>(&'a Baumeister);

impl Drop for Laufend<'_> {
    fn drop(&mut self) {
        self.0.laufend_leeren();
    }
}

// ----- Der Beobachter -----

/// Fingerabdrücke der `.svelte`- und `.ts`-Dateien je Projekt (TOML zählt nie).
pub fn abdruecke(q: &Quellen) -> BTreeMap<String, u64> {
    q.projekte().iter().map(|(slug, eltern)| (slug.clone(), crate::bote::bau::abdruck(eltern, slug))).collect()
}

/// Wartet die Ruhe ab: merkt sich je geändertem Projekt die letzte Änderung
/// und gibt die Projekte erst heraus, wenn [`RUHE`] lang nichts mehr kam.
#[derive(Debug, Default)]
pub struct Ruhe {
    alt: Option<BTreeMap<String, u64>>,
    offen: BTreeSet<String>,
    zuletzt: Option<Instant>,
}

impl Ruhe {
    /// Ein Blick: `neu` sind die Abdrücke jetzt. Rückgabe: die Projekte, für
    /// die jetzt gebaut wird (nach der Ruhe), sonst `None`. Der erste Blick
    /// merkt sich nur den Stand (ein Neustart baut nicht).
    pub fn blick(&mut self, neu: BTreeMap<String, u64>, jetzt: Instant, ruhe: Duration) -> Option<BTreeSet<String>> {
        if let Some(alt) = &self.alt {
            let mut geaendert = false;
            for (slug, a) in &neu {
                if alt.get(slug) != Some(a) {
                    self.offen.insert(slug.clone());
                    geaendert = true;
                }
            }
            for slug in alt.keys().filter(|s| !neu.contains_key(*s)) {
                self.offen.insert(slug.clone());
                geaendert = true;
            }
            if geaendert {
                self.zuletzt = Some(jetzt);
            }
        }
        self.alt = Some(neu);
        match self.zuletzt {
            Some(z) if !self.offen.is_empty() && jetzt.duration_since(z) >= ruhe => {
                self.zuletzt = None;
                Some(std::mem::take(&mut self.offen))
            }
            _ => None,
        }
    }
}

/// Startet den Beobachter: jede Sekunde ein Blick auf alle Projekte aller
/// Inhaltsquellen; nach 5 s Ruhe meldet er die geänderten an die Warteschlange.
pub fn beobachter_starten(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let abschaltung = state.abschaltung();
        tokio::pin!(abschaltung);
        let quellen: Vec<PathBuf> = state.quellen.to_vec();
        let mut ruhe = Ruhe::default();
        info!("Baumeister: Beobachter für .svelte und .ts in {} Quelle(n)", quellen.len());
        loop {
            let q = quellen.clone();
            let neu = match tokio::task::spawn_blocking(move || abdruecke(&Quellen::aus_liste(q))).await {
                Ok(n) => Some(n),
                Err(e) => {
                    warn!("Baumeister: Blick abgebrochen: {e}");
                    None
                }
            };
            if let Some(slugs) = neu.and_then(|n| ruhe.blick(n, Instant::now(), RUHE)) {
                info!("Baumeister: geändert und 5 s Ruhe: {}", slugs.iter().cloned().collect::<Vec<_>>().join(", "));
                state.baumeister.aenderung(slugs);
            }
            tokio::select! {
                _ = tokio::time::sleep(BLICK) => {}
                _ = &mut abschaltung => return,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::time::Instant;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;
    use crate::api::vertrag::Zustand;
    use crate::app::{router, router_angemeldet};

    fn zustand() -> AppState {
        AppState::fuer_tests(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel"))
    }

    fn post_bau(slug: &str, titel: &str) -> Request<Body> {
        Request::post("/api/vorgaenge")
            .header("host", "127.0.0.1:8787")
            .header("content-type", "application/json")
            .body(Body::from(format!(r#"{{"art": "bau", "slug": "{slug}", "titel": "{titel}"}}"#)))
            .unwrap()
    }

    fn get(pfad: &str) -> Request<Body> {
        Request::get(pfad).header("host", "127.0.0.1:8787").body(Body::empty()).unwrap()
    }

    async fn schicke(state: &AppState, req: Request<Body>) -> (StatusCode, serde_json::Value) {
        let res = router_angemeldet(state.clone()).oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
    }

    async fn anlegen(state: &AppState, slug: &str, titel: &str) -> String {
        let (status, v) = schicke(state, post_bau(slug, titel)).await;
        assert_eq!(status, StatusCode::CREATED, "{v}");
        v["vorgang"]["id"].as_str().unwrap().to_string()
    }

    fn gruen(satz: &str) -> BauErgebnis {
        BauErgebnis { gruen: true, dauer_ms: 5, ordner: Some("20261027-120000".into()), rote: vec!["kaputt".into()], satz: satz.into(), ..Default::default() }
    }

    /// Eine Attrappe, die zählt und sofort grün meldet.
    fn zaehlend(zaehler: Arc<AtomicU64>) -> Bauen {
        Arc::new(move || {
            let n = zaehler.fetch_add(1, Ordering::SeqCst) + 1;
            Box::pin(async move { gruen(&format!("Bau {n} grün.")) })
        })
    }

    /// Eine Attrappe, die erst nach einem Signal von `frei` fertig wird.
    fn wartend(zaehler: Arc<AtomicU64>, frei: Arc<Notify>) -> Bauen {
        Arc::new(move || {
            let n = zaehler.fetch_add(1, Ordering::SeqCst) + 1;
            let frei = frei.clone();
            Box::pin(async move {
                frei.notified().await;
                gruen(&format!("Bau {n} grün."))
            })
        })
    }

    fn vorgang(state: &AppState, id: &str) -> Vorgang {
        state.speicher().unwrap().vorgang_kopf(id).unwrap().unwrap()
    }

    async fn bis(was: &str, mut f: impl FnMut() -> bool) {
        let frist = Instant::now() + Duration::from_secs(5);
        while !f() {
            assert!(Instant::now() < frist, "nicht erreicht: {was}");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Hinter der Windows-Anmeldung: ohne sie 401, nichts angelegt.
    #[tokio::test]
    async fn bau_anlegen_ohne_anmeldung_ist_401() {
        let state = zustand();
        let res = router(state.clone()).oneshot(post_bau("bauplan", "Bitte bauen")).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(state.speicher().unwrap().cursor().unwrap(), 0, "nichts angelegt");
        assert!(state.speicher().unwrap().offene_bauten().unwrap().is_empty());
    }

    /// Zwei Bau-Vorgänge zugleich: ein Bau, beide mit demselben Ergebnis. Das
    /// Warten liefert sie erst danach aus, mit dem Bau-Stand. Kleinkram 2: Mit
    /// dem Ergebnis sind sie erledigt (vom Baumeister), und das Warten liefert
    /// jedes Ergebnis genau einmal.
    #[tokio::test]
    async fn zwei_vorgaenge_ein_bau() {
        let state = zustand();
        let mut live = state.bau_events.subscribe();
        let a = anlegen(&state, "bauplan", "Seite neu").await;
        let b = anlegen(&state, "dashboard", "Endpunkt fertig").await;
        // Vor dem Bau liefert das Warten nichts (der Chat bekäme sonst seine Bestellung zurück).
        let (_, l) = schicke(&state, get("/api/vorgaenge/warten?fuer=bauplan&timeout=0")).await;
        assert_eq!(l["vorgaenge"], serde_json::json!([]), "{l}");
        let zaehler = Arc::new(AtomicU64::new(0));
        assert_eq!(eine_runde(&state, &zaehlend(zaehler.clone())).await.unwrap(), 2);
        assert_eq!(zaehler.load(Ordering::SeqCst), 1, "ein Bau");
        assert_eq!(state.baumeister.laeufe(), 1);
        for id in [&a, &b] {
            let v = vorgang(&state, id);
            let bau = v.bau.unwrap();
            assert_eq!((bau.zustand, bau.meldung.as_str(), bau.rote), (BauZustand::Gruen, "Bau 1 grün.", vec!["kaputt".to_string()]));
            assert_eq!((v.zustand, v.notiz.as_deref(), v.von.as_str()), (Zustand::Erledigt, Some("Bau 1 grün."), "chat"));
            assert!(!v.geaendert.is_empty(), "die Zeit des Ergebnisses");
        }
        // Nichts mehr offen: die nächste Runde baut nicht.
        assert_eq!(eine_runde(&state, &zaehlend(zaehler.clone())).await.unwrap(), 0);
        assert_eq!(zaehler.load(Ordering::SeqCst), 1);
        // Jetzt liefert das Warten: erledigt, mit Ergebnis; nicht mehr unter den offenen.
        let (_, offen) = schicke(&state, get("/api/vorgaenge?fuer=bauplan&offen=1")).await;
        assert_eq!(offen["vorgaenge"], serde_json::json!([]), "{offen}");
        let (_, l) = schicke(&state, get("/api/vorgaenge/warten?fuer=bauplan&timeout=0")).await;
        assert_eq!(l["vorgaenge"].as_array().unwrap().len(), 1, "{l}");
        assert_eq!(l["vorgaenge"][0]["id"], a.as_str(), "{l}");
        assert_eq!(l["vorgaenge"][0]["zustand"], "erledigt");
        assert_eq!(l["vorgaenge"][0]["bau"]["zustand"], "gruen");
        assert_eq!(l["vorgaenge"][0]["notiz"], "Bau 1 grün.");
        // Genau einmal: das nächste Warten liefert es nicht wieder.
        let (_, l) = schicke(&state, get("/api/vorgaenge/warten?fuer=bauplan&timeout=0")).await;
        assert_eq!(l["vorgaenge"], serde_json::json!([]), "{l}");
        // Der andere Chat bekommt seins (ebenfalls einmal).
        let (_, l) = schicke(&state, get("/api/vorgaenge/warten?fuer=dashboard&timeout=0")).await;
        assert_eq!(l["vorgaenge"][0]["id"], b.as_str(), "{l}");
        // Journal: offen, laeuft, erledigt mit grün (kein Zustellen mehr).
        let (_, d) = schicke(&state, get(&format!("/api/vorgaenge/{a}"))).await;
        let schritte: Vec<(String, String)> = d["ereignisse"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| (e["zustand"].as_str().unwrap().to_string(), e["bau"].as_str().unwrap_or("-").to_string()))
            .collect();
        let erwartet = [("offen", "-"), ("offen", "laeuft"), ("erledigt", "gruen")];
        assert_eq!(schritte, erwartet.map(|(z, b)| (z.to_string(), b.to_string())), "{d}");
        assert_eq!(d["ereignisse"][2]["von"], VON);
        // Erledigt der Chat ihn trotzdem, ändert das nichts.
        let req = Request::post(format!("/api/vorgaenge/{a}/erledigt"))
            .header("host", "127.0.0.1:8787")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap();
        let (status, v) = schicke(&state, req).await;
        assert_eq!((status, v["geaendert"].as_bool()), (StatusCode::OK, Some(false)), "{v}");
        // Live: je Slug laeuft, dann gruen.
        let mut gesehen = Vec::new();
        while let Ok(e) = live.try_recv() {
            gesehen.push((e.slug, e.zustand));
        }
        let erwartet = [("bauplan", BauZustand::Laeuft), ("dashboard", BauZustand::Laeuft), ("bauplan", BauZustand::Gruen), ("dashboard", BauZustand::Gruen)];
        assert_eq!(gesehen, erwartet.map(|(s, z)| (s.to_string(), z)));
    }

    /// Während ein Bau läuft, warten neue Vorgänge; danach deckt ein einziger
    /// weiterer Bau alle wartenden ab. Das Warten des Chats wird erst vom
    /// Ergebnis geweckt, nicht vom Anlegen und nicht von `laeuft`.
    #[tokio::test]
    async fn waehrend_eines_baus_warten_die_naechsten() {
        let state = zustand();
        let zaehler = Arc::new(AtomicU64::new(0));
        let frei = Arc::new(Notify::new());
        let schleife = starten(state.clone(), wartend(zaehler.clone(), frei.clone()));
        let s = state.clone();
        let warter = tokio::spawn(async move {
            let res = router_angemeldet(s).oneshot(get("/api/vorgaenge/warten?fuer=bauplan&timeout=20")).await.unwrap();
            let bytes = res.into_body().collect().await.unwrap().to_bytes();
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()
        });
        let a = anlegen(&state, "bauplan", "erster").await;
        bis("A läuft", || vorgang(&state, &a).bau.is_some_and(|b| b.zustand == BauZustand::Laeuft)).await;
        let b = anlegen(&state, "bauplan", "zweiter").await;
        let c = anlegen(&state, "schaufenster", "dritter").await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(zaehler.load(Ordering::SeqCst), 1, "der zweite Bau wartet");
        assert!(vorgang(&state, &b).bau.is_none() && vorgang(&state, &c).bau.is_none());
        assert!(!warter.is_finished(), "Anlegen und laeuft wecken das Warten nicht auf");
        frei.notify_one();
        let l = tokio::time::timeout(Duration::from_secs(5), warter).await.unwrap().unwrap();
        assert_eq!(l["vorgaenge"][0]["id"], a.as_str(), "{l}");
        assert_eq!(l["vorgaenge"][0]["bau"]["meldung"], "Bau 1 grün.");
        bis("B läuft", || vorgang(&state, &b).bau.is_some_and(|x| x.zustand == BauZustand::Laeuft)).await;
        assert_eq!(vorgang(&state, &c).bau.unwrap().zustand, BauZustand::Laeuft);
        frei.notify_one();
        bis("C fertig", || vorgang(&state, &c).bau.is_some_and(|x| x.zustand == BauZustand::Gruen)).await;
        assert_eq!(vorgang(&state, &b).bau.unwrap().meldung, "Bau 2 grün.");
        assert_eq!(vorgang(&state, &c).bau.unwrap().meldung, "Bau 2 grün.");
        assert_eq!(vorgang(&state, &a).bau.unwrap().meldung, "Bau 1 grün.", "A bleibt beim ersten");
        assert_eq!(zaehler.load(Ordering::SeqCst), 2, "genau zwei Bauten für drei Vorgänge");
        schleife.abort();
    }

    /// Rot: Meldung mit Satz und Ende der Ausgabe, Notiz nur der Satz. Ohne
    /// Boten wird nicht gebaut, sondern mit Satz rot gemeldet.
    #[tokio::test]
    async fn rot_mit_ausgabe_und_ohne_boten() {
        let state = zustand();
        let a = anlegen(&state, "bauplan", "kaputt").await;
        let ausgabe = Arc::new(Mutex::new(Some("Fehler: Seite.svelte:3\nletzte Zeile".to_string())));
        let bauen: Bauen = Arc::new(move || {
            let ende = ausgabe.lock().unwrap().take();
            Box::pin(async move { BauErgebnis { ausgabe_ende: ende, ..rot("npm run build ist gescheitert (Rückgabe 1).") } })
        });
        eine_runde(&state, &bauen).await.unwrap();
        let v = vorgang(&state, &a);
        let bau = v.bau.unwrap();
        assert_eq!(bau.zustand, BauZustand::Rot);
        assert_eq!(bau.meldung, "npm run build ist gescheitert (Rückgabe 1).\n\nEnde der Ausgabe:\nFehler: Seite.svelte:3\nletzte Zeile");
        assert_eq!(v.notiz.as_deref(), Some("npm run build ist gescheitert (Rückgabe 1)."));
        let b = anlegen(&state, "bauplan", "ohne Boten").await;
        eine_runde(&state, &ueber_boten(None)).await.unwrap();
        let bau = vorgang(&state, &b).bau.unwrap();
        assert_eq!(bau.zustand, BauZustand::Rot);
        assert!(bau.meldung.starts_with("Ohne Boten baut das Dashboard nicht"), "{}", bau.meldung);
        // Lange Meldungen werden gekürzt.
        let lang = BauErgebnis { ausgabe_ende: Some("x".repeat(10_000)), ..rot("Satz.") };
        assert_eq!(meldung(&lang).chars().count(), MELDUNG_HOECHSTENS);
    }

    /// Ein erledigter Bau-Vorgang (der Chat hat ihn zurückgezogen) wird nicht gebaut.
    #[tokio::test]
    async fn erledigte_werden_nicht_gebaut() {
        let state = zustand();
        let a = anlegen(&state, "bauplan", "doch nicht").await;
        let req = Request::post(format!("/api/vorgaenge/{a}/erledigt"))
            .header("host", "127.0.0.1:8787")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap();
        assert_eq!(schicke(&state, req).await.0, StatusCode::OK);
        let zaehler = Arc::new(AtomicU64::new(0));
        assert_eq!(eine_runde(&state, &zaehlend(zaehler.clone())).await.unwrap(), 0);
        assert_eq!(zaehler.load(Ordering::SeqCst), 0);
        assert!(vorgang(&state, &a).bau.is_none());
    }

    // ----- Etappe 4b: Warteschlange, Frist, Beobachter -----

    /// Eine Attrappe, die zählt, wie viele zugleich laufen, und auf `frei` wartet.
    fn zaehlt_gleichzeitig(zaehler: Arc<AtomicU64>, aktiv: Arc<AtomicU64>, hoechstens: Arc<AtomicU64>, frei: Arc<Notify>) -> Bauen {
        Arc::new(move || {
            let n = zaehler.fetch_add(1, Ordering::SeqCst) + 1;
            let (aktiv, hoechstens, frei) = (aktiv.clone(), hoechstens.clone(), frei.clone());
            Box::pin(async move {
                let jetzt = aktiv.fetch_add(1, Ordering::SeqCst) + 1;
                hoechstens.fetch_max(jetzt, Ordering::SeqCst);
                frei.notified().await;
                aktiv.fetch_sub(1, Ordering::SeqCst);
                gruen(&format!("Bau {n} grün."))
            })
        })
    }

    /// Ein Bau zur Zeit; alles, was während des Baus kommt (drei Änderungen
    /// und zwei Vorgänge), ergibt genau einen weiteren Bau.
    #[tokio::test]
    async fn ein_bau_zur_zeit_und_genau_ein_weiterer() {
        let state = zustand();
        let (zaehler, aktiv, hoechstens, frei) =
            (Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)), Arc::new(AtomicU64::new(0)), Arc::new(Notify::new()));
        let schleife = starten(state.clone(), zaehlt_gleichzeitig(zaehler.clone(), aktiv.clone(), hoechstens.clone(), frei.clone()));
        state.baumeister.aenderung(["bauplan".to_string()]);
        bis("Bau 1 läuft", || aktiv.load(Ordering::SeqCst) == 1).await;
        for slug in ["dashboard", "schaufenster", "bauplan"] {
            state.baumeister.aenderung([slug.to_string()]);
        }
        anlegen(&state, "kontrolle", "bitte bauen").await;
        state.baumeister.anstossen();
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(zaehler.load(Ordering::SeqCst), 1, "der zweite Bau wartet");
        frei.notify_one();
        bis("Bau 2 läuft", || zaehler.load(Ordering::SeqCst) == 2 && aktiv.load(Ordering::SeqCst) == 1).await;
        frei.notify_one();
        bis("Bau 2 fertig", || aktiv.load(Ordering::SeqCst) == 0).await;
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(zaehler.load(Ordering::SeqCst), 2, "genau ein weiterer Bau");
        assert_eq!(hoechstens.load(Ordering::SeqCst), 1, "nie zwei zugleich");
        assert_eq!(state.baumeister.laeufe(), 2);
        schleife.abort();
    }

    /// Frist: Ein Bau, der hängt, gilt danach als rot (Ereignis `bau` rot mit
    /// Satz); die Warteschlange läuft weiter, der nächste Bau kommt dran.
    #[tokio::test]
    async fn frist_macht_rot_und_der_naechste_laeuft() {
        let mut state = zustand();
        state.baumeister = Arc::new(Baumeister::mit_frist(Duration::from_millis(200)));
        let mut live = state.bau_events.subscribe();
        let zaehler = Arc::new(AtomicU64::new(0));
        let z = zaehler.clone();
        let bauen: Bauen = Arc::new(move || {
            let n = z.fetch_add(1, Ordering::SeqCst) + 1;
            Box::pin(async move {
                if n == 1 {
                    std::future::pending::<()>().await;
                }
                gruen("Bau 2 grün.")
            })
        });
        state.baumeister.aenderung(["bauplan".to_string()]);
        let start = Instant::now();
        assert_eq!(eine_runde(&state, &bauen).await.unwrap(), 1);
        assert!(start.elapsed() < Duration::from_secs(3), "{:?}", start.elapsed());
        let mut gesehen = Vec::new();
        while let Ok(e) = live.try_recv() {
            gesehen.push((e.slug, e.zustand, e.meldung));
        }
        assert_eq!(gesehen.len(), 2, "{gesehen:?}");
        assert_eq!((gesehen[1].0.as_str(), gesehen[1].1), ("bauplan", BauZustand::Rot));
        // Abnahme 4c: im Ereignis ein Satz ohne Pfade; der volle Satz steht im Vorgang.
        assert_eq!(gesehen[1].2, SATZ_ZU_LANGE);
        state.baumeister.aenderung(["bauplan".to_string()]);
        eine_runde(&state, &bauen).await.unwrap();
        assert_eq!(zaehler.load(Ordering::SeqCst), 2);
        let e = live.try_recv().ok().and_then(|_| live.try_recv().ok()).unwrap();
        assert_eq!((e.zustand, e.meldung.as_str(), e.gescheitert), (BauZustand::Gruen, "Gebaut in 0 s.", false));
        // Ohne Anlass baut die Runde nicht.
        assert_eq!(eine_runde(&state, &bauen).await.unwrap(), 0);
    }

    /// Projekte, die rot wurden: je ein Ereignis `bau` rot mit ihrem Satz;
    /// ist der Bote nicht erreichbar, keine Bau-Meldung (es gilt „Bote fehlt“).
    #[tokio::test]
    async fn rote_projekte_und_bote_fehlt() {
        use crate::vertrag::bote::RotesProjekt;
        let state = zustand();
        let mut live = state.bau_events.subscribe();
        let bauen: Bauen = Arc::new(|| {
            Box::pin(async {
                BauErgebnis {
                    neu_rot: vec![RotesProjekt {
                        slug: "schaufenster".into(),
                        satz: "Die Seite von „schaufenster“ hat einen Fehler und bleibt in ihrer letzten guten Fassung.".into(),
                    }],
                    alte_fassung: vec!["schaufenster".into()],
                    ..gruen("Bau grün.")
                }
            })
        });
        state.baumeister.aenderung(["schaufenster".to_string(), "dashboard".to_string()]);
        eine_runde(&state, &bauen).await.unwrap();
        let mut gesehen = Vec::new();
        while let Ok(e) = live.try_recv() {
            gesehen.push((e.slug, e.zustand));
        }
        assert!(gesehen.contains(&("schaufenster".to_string(), BauZustand::Rot)), "{gesehen:?}");
        assert!(gesehen.contains(&("dashboard".to_string(), BauZustand::Gruen)), "{gesehen:?}");
        assert!(!gesehen.contains(&("schaufenster".to_string(), BauZustand::Gruen)), "{gesehen:?}");
        let ohne: Bauen = Arc::new(|| Box::pin(async { BauErgebnis { bote_fehlt: true, ..rot("Der Bote ist nicht erreichbar.") } }));
        state.baumeister.aenderung(["bauplan".to_string()]);
        eine_runde(&state, &ohne).await.unwrap();
        let s = state.speicher().unwrap();
        let meldungen = s.meldungen_lesen(Some("bauplan")).unwrap();
        let schaufenster = s.meldungen_lesen(Some("schaufenster")).unwrap();
        assert_eq!(schaufenster.len(), 1, "die rote Seite meldet sich: {schaufenster:?}");
        assert!(schaufenster[0].titel.contains("letzten guten Fassung"), "{:?}", schaufenster[0]);
        assert!(!meldungen.iter().any(|m| m.projekt == "bauplan"), "keine Bau-Meldung ohne Boten: {meldungen:?}");
    }

    /// Abnahme 4c (Nachbesserung 2): Wer das End-Ereignis `bau` sieht, sieht
    /// das Projekt nicht mehr „im Bau“, auch wenn die Runde danach noch auf
    /// den Datenspeicher wartet (Vorgänge, Meldungen). Der Test läuft auf
    /// einem Faden: Der Hörer kommt erst beim nächsten await der Runde dran,
    /// also nach den End-Ereignissen und vor ihrem Ende.
    #[tokio::test]
    async fn nach_dem_end_ereignis_nicht_mehr_im_bau() {
        let state = zustand();
        let mut live = state.bau_events.subscribe();
        type Gesehen = Option<(BauZustand, BTreeSet<String>)>;
        let gesehen: Arc<Mutex<Gesehen>> = Arc::new(Mutex::new(None));
        let (s2, g2) = (state.clone(), gesehen.clone());
        let hoerer = tokio::spawn(async move {
            while let Ok(e) = live.recv().await {
                if e.zustand != BauZustand::Laeuft {
                    *g2.lock().unwrap() = Some((e.zustand, s2.baumeister.im_bau()));
                    return;
                }
            }
        });
        let bauen: Bauen = Arc::new(|| Box::pin(async { gruen("Bau grün.") }));
        state.baumeister.aenderung(["bauplan".to_string()]);
        // Ein Vorgang dazu, damit die Runde nach den Ereignissen noch schreibt (await).
        anlegen(&state, "bauplan", "Bitte bauen").await;
        eine_runde(&state, &bauen).await.unwrap();
        hoerer.await.unwrap();
        let (zustand, im_bau) = gesehen.lock().unwrap().clone().expect("End-Ereignis gesehen");
        assert_eq!(zustand, BauZustand::Gruen);
        assert!(im_bau.is_empty(), "beim End-Ereignis noch im Bau: {im_bau:?}");
    }

    /// Etappe 4c: Ein Projekt, das schon vor dem Bau rot war und es bleibt
    /// (aus seiner Kopie gebaut), meldet `bau` rot, nicht grün. Während der
    /// Runde gilt es als „läuft“, danach nicht mehr.
    #[tokio::test]
    async fn weiter_rot_bleibt_rot_und_laeuft_nur_waehrend_der_runde() {
        let state = zustand();
        let mut live = state.bau_events.subscribe();
        let s2 = state.clone();
        let im_bau_waehrend = Arc::new(Mutex::new(BTreeSet::new()));
        let merk = im_bau_waehrend.clone();
        let bauen: Bauen = Arc::new(move || {
            *merk.lock().unwrap() = s2.baumeister.im_bau();
            Box::pin(async { BauErgebnis { alte_fassung: vec!["schaufenster".into()], ..gruen("Bau grün.") } })
        });
        // Zweimal gemeldet bleibt ein Bau.
        state.baumeister.aenderung(["schaufenster".to_string()]);
        state.baumeister.aenderung(["schaufenster".to_string(), "dashboard".to_string()]);
        assert_eq!(eine_runde(&state, &bauen).await.unwrap(), 2);
        assert_eq!(*im_bau_waehrend.lock().unwrap(), BTreeSet::from(["dashboard".to_string(), "schaufenster".to_string()]));
        assert!(state.baumeister.im_bau().is_empty());
        let mut ende = Vec::new();
        while let Ok(e) = live.try_recv() {
            if e.zustand != BauZustand::Laeuft {
                ende.push((e.slug, e.zustand, e.meldung));
            }
        }
        assert!(ende.contains(&("schaufenster".to_string(), BauZustand::Rot, SATZ_NOCH_ROT.to_string())), "{ende:?}");
        assert!(ende.contains(&("dashboard".to_string(), BauZustand::Gruen, "Gebaut in 0 s.".to_string())), "{ende:?}");
        assert_eq!(ende.len(), 2, "{ende:?}");
    }

    /// Abnahme 4c: Scheitert der ganze Bau, trägt jedes Ereignis `gescheitert`
    /// und einen Satz ohne Pfade und Befehle; ein Projekt, das rot wird, nur
    /// „Die neue Fassung der Seite hat einen Fehler.“ ohne den Fund.
    #[tokio::test]
    async fn ereignisse_ohne_pfade_und_gescheitert() {
        use crate::vertrag::bote::RotesProjekt;
        let state = zustand();
        let mut live = state.bau_events.subscribe();
        let ohne: Bauen = Arc::new(|| {
            Box::pin(async { rot("Ohne Boten baut das Dashboard nicht (es läuft ohne Einstellungsdatei). Gebaut wird dann mit Dienst.ps1 -Neustart.") })
        });
        state.baumeister.aenderung(["bauplan".to_string()]);
        eine_runde(&state, &ohne).await.unwrap();
        let ende: Vec<_> = std::iter::from_fn(|| live.try_recv().ok()).filter(|e| e.zustand != BauZustand::Laeuft).collect();
        assert_eq!(ende.len(), 1, "{ende:?}");
        assert_eq!((ende[0].zustand, ende[0].gescheitert, ende[0].meldung.as_str()), (BauZustand::Rot, true, SATZ_NICHT_GEBAUT));
        let fehlt: Bauen = Arc::new(|| Box::pin(async { BauErgebnis { bote_fehlt: true, ..rot(r"Der Bote ist nicht erreichbar: Pipe \\.\pipe\x.") } }));
        state.baumeister.aenderung(["bauplan".to_string()]);
        eine_runde(&state, &fehlt).await.unwrap();
        let e = std::iter::from_fn(|| live.try_recv().ok()).find(|e| e.zustand != BauZustand::Laeuft).unwrap();
        assert_eq!((e.gescheitert, e.meldung.as_str()), (true, SATZ_BOTE_FEHLT));
        let wird_rot: Bauen = Arc::new(|| {
            Box::pin(async {
                BauErgebnis {
                    neu_rot: vec![RotesProjekt { slug: "schaufenster".into(), satz: "Erster Fund: projects/schaufenster/Seite.svelte, Zeile 3".into() }],
                    ..gruen(r"Bau grün in 5 s: web\builds\x.")
                }
            })
        });
        state.baumeister.aenderung(["schaufenster".to_string()]);
        eine_runde(&state, &wird_rot).await.unwrap();
        let e = std::iter::from_fn(|| live.try_recv().ok()).find(|e| e.zustand != BauZustand::Laeuft).unwrap();
        assert_eq!((e.zustand, e.gescheitert, e.meldung.as_str()), (BauZustand::Rot, false, SATZ_WIRD_ROT));
        for s in [SATZ_ZU_LANGE, SATZ_BOTE_FEHLT, SATZ_NICHT_GEBAUT, SATZ_WIRD_ROT, SATZ_NOCH_ROT, SATZ_LAEUFT, SATZ_WARTET] {
            assert!(!s.contains('\\') && !s.contains('/') && !s.contains("Dienst.ps1"), "{s}");
        }
    }

    /// Grün gebaut, aber mit anderem Grundbau als die eingespielte Oberfläche
    /// (`WebOrt::wartender_bau`): Der Vorgang ist nicht grün, sondern rot mit
    /// dem Satz, dass der Bau auf Dienst.ps1 -Neustart wartet; das Ereignis
    /// sagt es ohne Befehl, und es kommt kein Ereignis `fassung`. Gegenprobe:
    /// Mit demselben Grundbau ist derselbe Bau grün.
    #[tokio::test]
    async fn anderer_grundbau_ist_nicht_gruen() {
        use crate::app::{CspCache, SATZ_BAU_WARTET, WebOrt};
        let w = std::env::temp_dir().join(format!("dashboard-baumeister-grundbau-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&w);
        let kopie = w.join("bin-web");
        let builds = w.join("web").join("builds");
        let bau = builds.join("20261027-120000");
        for (o, fassung) in [(&kopie, "abc-1"), (&bau, "def-2")] {
            std::fs::create_dir_all(o.join("_app")).unwrap();
            std::fs::write(o.join("index.html"), "<!doctype html><title>x</title>").unwrap();
            std::fs::write(o.join("_app").join("version.json"), format!("{{\"version\":\"{fassung}\"}}")).unwrap();
        }
        crate::bote::bau::zeiger_schreiben(&builds, "20261027-120000").unwrap();
        let web = Arc::new(WebOrt::neu(w.join("web").join("build")).mit_kopie(Some(kopie)));
        let mut state = zustand();
        state.csp = Arc::new(CspCache::neu(web.clone()));
        state.web = web;
        let mut live = state.bau_events.subscribe();
        let mut fassung = state.fassung_events.subscribe();
        let zaehler = Arc::new(AtomicU64::new(0));
        let id = anlegen(&state, "bauplan", "Seite neu").await;
        eine_runde(&state, &zaehlend(zaehler.clone())).await.unwrap();
        let stand = vorgang(&state, &id).bau.unwrap();
        assert_eq!((stand.zustand, stand.meldung.as_str()), (BauZustand::Rot, SATZ_BAU_WARTET));
        let e = std::iter::from_fn(|| live.try_recv().ok()).find(|e| e.zustand != BauZustand::Laeuft).unwrap();
        assert_eq!((e.zustand, e.gescheitert, e.meldung.as_str()), (BauZustand::Rot, true, SATZ_WARTET));
        assert!(fassung.try_recv().is_err(), "kein Ereignis fassung");
        // Gegenprobe: derselbe Grundbau.
        std::fs::write(bau.join("_app").join("version.json"), r#"{"version":"abc-2"}"#).unwrap();
        let id = anlegen(&state, "bauplan", "Seite neu").await;
        eine_runde(&state, &zaehlend(zaehler)).await.unwrap();
        assert_eq!(vorgang(&state, &id).bau.unwrap().zustand, BauZustand::Gruen);
        let e = std::iter::from_fn(|| live.try_recv().ok()).find(|e| e.zustand != BauZustand::Laeuft).unwrap();
        assert_eq!((e.zustand, e.gescheitert), (BauZustand::Gruen, false));
        assert_eq!(fassung.try_recv().unwrap().ordner, "20261027-120000");
        let _ = std::fs::remove_dir_all(&w);
    }

    /// Ruhe: erst nach 5 s ohne weitere Änderung; der erste Blick baut nie.
    #[test]
    fn ruhe_wartet_fuenf_sekunden() {
        let t0 = Instant::now();
        let s = |sek: u64| t0 + Duration::from_secs(sek);
        let stand = |a: u64, b: u64| BTreeMap::from([("a".to_string(), a), ("b".to_string(), b)]);
        let mut r = Ruhe::default();
        assert_eq!(r.blick(stand(1, 1), s(0), RUHE), None, "Neustart baut nicht");
        assert_eq!(r.blick(stand(1, 1), s(1), RUHE), None);
        assert_eq!(r.blick(stand(2, 1), s(2), RUHE), None, "a geändert");
        assert_eq!(r.blick(stand(2, 1), s(5), RUHE), None, "erst 3 s Ruhe");
        assert_eq!(r.blick(stand(2, 7), s(6), RUHE), None, "b geändert: Ruhe beginnt neu");
        assert_eq!(r.blick(stand(2, 7), s(10), RUHE), None);
        assert_eq!(r.blick(stand(2, 7), s(11), RUHE), Some(BTreeSet::from(["a".to_string(), "b".to_string()])));
        assert_eq!(r.blick(stand(2, 7), s(30), RUHE), None, "nur einmal");
        // Ein Projekt verschwindet: auch das ist eine Änderung.
        let nur_a = BTreeMap::from([("a".to_string(), 2)]);
        assert_eq!(r.blick(nur_a.clone(), s(31), RUHE), None);
        assert_eq!(r.blick(nur_a, s(36), RUHE), Some(BTreeSet::from(["b".to_string()])));
    }

    /// Bau-Tempo, Zweig C: Während des Baus fragt der Baumeister nach dem
    /// Fortschritt und meldet nur Änderungen (nur Zahlen, Kanal des
    /// geschützten Live-Kanals). Kein Fortschritt (älterer Bote, kein Bote)
    /// gibt kein Ereignis und hält den Bau nicht auf.
    #[tokio::test]
    async fn fortschritt_nur_bei_aenderung() {
        use crate::vertrag::bote::FortschrittSchritt;
        use std::collections::VecDeque;
        let state = zustand();
        let mut live = state.bau_fortschritt.subscribe();
        let f = |fertig| Some(BauFortschritt { schritt: FortschrittSchritt::Pruefung, fertig, gesamt: 3 });
        let antworten = Mutex::new(VecDeque::from([None, f(0), f(0), f(1), f(1), None, f(3)]));
        let fertig = Notify::new();
        let frage = || {
            let a = antworten.lock().unwrap().pop_front();
            if a.is_none() {
                fertig.notify_one();
            }
            async move { a.flatten() }
        };
        let e = mit_fortschritt(
            &state,
            async {
                fertig.notified().await;
                gruen("x")
            },
            frage,
        )
        .await;
        assert!(e.gruen);
        let mut gesendet = Vec::new();
        while let Ok(x) = live.try_recv() {
            gesendet.push(Some(x));
        }
        assert_eq!(gesendet, [f(0), f(1), f(3)], "nur bei Änderung");
        // Immer kein Fortschritt: kein Ereignis, der Bau endet trotzdem.
        let e = mit_fortschritt(
            &state,
            async {
                tokio::time::sleep(FORTSCHRITT_TAKT * 5).await;
                gruen("y")
            },
            || async { None },
        )
        .await;
        assert!(e.gruen && live.try_recv().is_err());
    }
}
