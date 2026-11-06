//! Die Sammler des Dienstes, wenn es einen Boten gibt: Benutzer-Daten der
//! PC-Seite (winget, Autostart, Werkzeuge, Ordnergrößen) kommen über ihn,
//! der Rest (Windows, Dienste, Updates, Starts) wie bisher aus dem Dienst.
//!
//! Kommt der Bote nicht (der Inhaber nicht angemeldet), bleibt der letzte Stand
//! stehen; die PC-Seite sagt das mit der Zeit des letzten Stands.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use chrono::Local;
use tracing::{info, warn};

use super::client::Bote;
use crate::aenderungen::{Bereich, Melder};
use crate::pc::{inventory, sysapps};
use crate::speicher::{Speicher, pc_ereignisse};
use crate::vertrag::bote::{Anfrage, Antwort};

/// Gelingt die Abfrage beim Boten nicht, so bald wieder versuchen.
const NOCHMAL: Duration = Duration::from_secs(5 * 60);
/// Abstand der Lebenszeichen.
const LEBENSZEICHEN: Duration = Duration::from_secs(60);

/// Gestaffelter Start: so lange nach dem Start fragt der jeweilige Sammler
/// zum ersten Mal (Lebenszeichen sofort). So kommen beim Start nicht alle
/// Anfragen auf einmal; spätere Runden laufen nach ihrem eigenen Takt.
pub const START_AUTOSTART: Duration = Duration::from_secs(2);
pub const START_WERKZEUGE: Duration = Duration::from_secs(4);
pub const START_WINGET: Duration = Duration::from_secs(10);
pub const START_INVENTAR: Duration = Duration::from_secs(20);

/// Zeit für Sätze: `26.10.2026 14:03`.
fn zeit(t: Option<chrono::DateTime<Local>>) -> String {
    match t {
        Some(t) => format!("Letzter Stand: {}.", t.format("%d.%m.%Y %H:%M")),
        None => "Noch kein Stand.".to_string(),
    }
}

/// Fragt alle 60 s „Hallo“, damit `/api/betrieb` den Boten aktuell nennt.
pub fn spawn_lebenszeichen(bote: Arc<Bote>) {
    tokio::spawn(async move {
        loop {
            let _ = bote.anfragen(&Anfrage::Hallo).await;
            tokio::time::sleep(LEBENSZEICHEN).await;
        }
    });
}

/// Systemanwendungen: eigener Teil stündlich im Dienst (die Windows-Ereignisse
/// daraus in den Datenspeicher, wenn er offen ist), Benutzer-Teil über den
/// Boten (stündlich, nach einem Fehler alle 5 Minuten).
pub fn spawn_sysapps(shared: sysapps::SharedSysApps, bote: Arc<Bote>, speicher: Option<Arc<Speicher>>, melder: Arc<Melder>) {
    let lokal = shared.clone();
    let melder_lokal = melder.clone();
    tokio::spawn(async move {
        loop {
            let mut l = sysapps::collect_lokal().await;
            if !l.lokal_fehler.is_empty() {
                warn!("Systemanwendungen, Teilfehler: {:?}", l.lokal_fehler);
            }
            // Etappe 2e: Windows-Ereignisse in den Datenspeicher (für den Verlauf).
            pc_ereignisse::sammler_lauf(speicher.as_ref(), std::mem::take(&mut l.ereignisse)).await;
            let mut g = lokal.write().await;
            g.refreshed_at = l.refreshed_at;
            g.windows = l.windows;
            g.services_running = l.services_running;
            g.services_total = l.services_total;
            g.lokal_fehler = l.lokal_fehler;
            g.fehler_zusammenfassen();
            drop(g);
            // Live-Lücken: neuer Lauf als `pc-benutzer` mit Bereich `systemprogramme`.
            melder_lokal.melden(Bereich::Systemprogramme);
            tokio::time::sleep(Duration::from_secs(60 * 60)).await;
        }
    });
    tokio::spawn(async move {
        let mut erste = true;
        loop {
            let gut = benutzer_runde(&shared, &bote, &melder, erste).await;
            erste = false;
            tokio::time::sleep(if gut { Duration::from_secs(60 * 60) } else { NOCHMAL }).await;
        }
    });
}

/// Ein Lauf beim Boten für die Benutzer-Daten. Gemeldet (`pc-benutzer` mit
/// `systemprogramme`) wird nur, was die Seite ändert: ein gelungener Lauf
/// oder ein neuer Fehlersatz. Scheitert der Lauf wie der vorige (Bote weg,
/// alle 5 Minuten ein Versuch), bleibt es still. `true`, wenn er gelang.
async fn benutzer_runde(shared: &sysapps::SharedSysApps, bote: &Bote, melder: &Arc<Melder>, erste: bool) -> bool {
    let vorher = shared.read().await.benutzer_fehler.clone();
    let gut = benutzer_daten(shared, bote, erste).await;
    let nachher = shared.read().await.benutzer_fehler.clone();
    if nach_lauf_melden(gut, &vorher, &nachher) {
        melder.melden(Bereich::Systemprogramme);
    }
    gut
}

/// Ob nach einem Lauf `systemprogramme` gemeldet wird: nach einem gelungenen
/// immer, nach einem fehlgeschlagenen nur, wenn sich der Fehlersatz geändert hat.
fn nach_lauf_melden(gut: bool, vorher: &Option<String>, nachher: &Option<String>) -> bool {
    gut || vorher != nachher
}

/// `anfrage` nach `warten`.
async fn spaeter(bote: &Bote, warten: Duration, anfrage: Anfrage) -> Result<Antwort, crate::dienst::einstellungen::Satz> {
    tokio::time::sleep(warten).await;
    bote.anfragen(&anfrage).await
}

/// Holt winget, Autostart und Werkzeuge beim Boten. `true`, wenn er antwortete.
/// Beim ersten Mal gestaffelt (`START_…`), danach alle drei zugleich.
async fn benutzer_daten(shared: &sysapps::SharedSysApps, bote: &Bote, erste: bool) -> bool {
    let nach = |d: Duration| if erste { d } else { Duration::ZERO };
    let (autostart, winget, werkzeuge) = tokio::join!(
        spaeter(bote, nach(START_AUTOSTART), Anfrage::Autostart),
        spaeter(bote, nach(START_WINGET), Anfrage::Winget),
        spaeter(bote, nach(START_WERKZEUGE), Anfrage::Werkzeuge)
    );
    let mut g = shared.write().await;
    let mut fehlt = None;
    match autostart {
        Ok(Antwort::Autostart(l)) => g.autostart = l,
        Ok(a) => fehlt = Some(format!("Autostart: {}", satz_aus(a))),
        Err(s) => fehlt = Some(s.0),
    }
    match winget {
        Ok(Antwort::Winget(l)) => {
            g.winget = l;
            g.winget_error = None;
        }
        Ok(a) => g.winget_error = Some(satz_aus(a)),
        Err(s) => fehlt = fehlt.or(Some(s.0)),
    }
    match werkzeuge {
        Ok(Antwort::Werkzeuge(l)) => g.tools = l,
        Ok(a) => fehlt = fehlt.or(Some(format!("Werkzeuge: {}", satz_aus(a)))),
        Err(s) => fehlt = fehlt.or(Some(s.0)),
    }
    let gut = fehlt.is_none();
    match fehlt {
        None => {
            g.benutzer_stand = Some(Local::now());
            g.benutzer_fehler = None;
            info!("Benutzer-Daten vom Boten: {} Autostart, {} winget-Pakete, {} Werkzeuge", g.autostart.len(), g.winget.len(), g.tools.len());
        }
        Some(s) => {
            g.benutzer_fehler = Some(format!("Benutzer-Daten (winget, Autostart, Werkzeuge) über den Boten: {s} {}", zeit(g.benutzer_stand)));
        }
    }
    g.fehler_zusammenfassen();
    gut
}

fn satz_aus(a: Antwort) -> String {
    match a {
        Antwort::Fehler { satz } => satz,
        anders => format!("Der Bote hat unpassend geantwortet ({}).", anders.art()),
    }
}

/// Ordnergrößen über den Boten (Profil des Boten), Zwischenspeicher wie bisher.
/// Gezählt wird nur mit Ausschlussliste (der Bote liest sie selbst); den
/// Zwischenspeicher lädt der Dienst nur, wenn er die Liste `ausschluss`
/// auch sieht (geschlossen scheitern).
pub fn spawn_inventar(shared: inventory::SharedInventory, bote: Arc<Bote>, cache: PathBuf, ausschluss: PathBuf, melder: Arc<Melder>) {
    tokio::spawn(async move {
        if let Some(c) = inventory::load_cache(&cache).filter(|_| inventory::ausschluss_laden(Some(&ausschluss)).is_ok()) {
            info!("Ordnergrößen aus Cache ({})", cache.display());
            *shared.write().await = c;
        }
        tokio::time::sleep(START_INVENTAR).await;
        loop {
            let frisch = inventory::is_fresh(&*shared.read().await);
            let mut warten = Duration::from_secs(15 * 60);
            if !frisch {
                shared.write().await.computing = true;
                let antwort = bote.anfragen(&Anfrage::Inventar { wurzel: None }).await;
                let mut g = shared.write().await;
                match antwort {
                    Ok(Antwort::Inventar(inv)) => {
                        match &inv.hinweis {
                            None => {
                                info!("Ordnergrößen vom Boten in {} s", inv.duration_secs);
                                inventory::save_cache(&cache, &inv);
                            }
                            Some(satz) => warn!("Ordnergrößen über den Boten: {satz}"),
                        }
                        *g = inv;
                        // Live-Lücken: neue Ordnergrößen als `pc-benutzer` mit Bereich `inventar`.
                        melder.melden(Bereich::Inventar);
                    }
                    andere => {
                        let s = match andere {
                            Ok(a) => satz_aus(a),
                            Err(s) => s.0,
                        };
                        g.computing = false;
                        g.error = Some(format!("Ordnergrößen über den Boten: {s} {}", zeit(g.computed_at)));
                        warten = NOCHMAL;
                    }
                }
            }
            tokio::time::sleep(warten).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lebenszeichen sofort, dann Autostart, Werkzeuge, winget, Ordnergrößen.
    #[test]
    fn start_ist_gestaffelt() {
        let folge = [Duration::ZERO, START_AUTOSTART, START_WERKZEUGE, START_WINGET, START_INVENTAR];
        assert!(folge.windows(2).all(|p| p[0] < p[1]), "{folge:?}");
        assert_eq!(folge.map(|d| d.as_secs()), [0, 2, 4, 10, 20]);
    }

    #[test]
    fn melden_nur_nach_gutem_lauf_oder_neuem_fehler() {
        let f = |s: &str| Some(s.to_string());
        assert!(nach_lauf_melden(true, &None, &None));
        assert!(nach_lauf_melden(true, &f("alt"), &None));
        assert!(nach_lauf_melden(false, &None, &f("Bote weg")), "erster Fehler: die Seite zeigt ihn");
        assert!(!nach_lauf_melden(false, &f("Bote weg"), &f("Bote weg")), "derselbe Fehler noch einmal: still");
        assert!(nach_lauf_melden(false, &f("Bote weg"), &f("anderer Fehler")));
    }

    /// Abnahme-Auflage: Mit einem Boten, der nicht läuft (falscher Stand,
    /// keine Pipe), meldet der erste fehlgeschlagene Lauf einmal (neuer
    /// Fehlersatz), jeder weitere gleiche Lauf nichts.
    #[tokio::test]
    async fn fehlgeschlagener_lauf_meldet_systemprogramme_nicht_erneut() {
        let shared: sysapps::SharedSysApps = Default::default();
        let bote = Bote::fuer_tests();
        let melder = Melder::neu();
        let mut rx = melder.abonnieren();
        let warten = || tokio::time::sleep(crate::aenderungen::BUENDELN + Duration::from_millis(150));
        assert!(!benutzer_runde(&shared, &bote, &melder, false).await);
        warten().await;
        assert_eq!(rx.try_recv().unwrap().bereich, Bereich::Systemprogramme, "erster Fehler wird gezeigt");
        assert!(shared.read().await.benutzer_fehler.is_some());
        for _ in 0..2 {
            assert!(!benutzer_runde(&shared, &bote, &melder, false).await);
        }
        warten().await;
        assert!(rx.try_recv().is_err(), "kein Ereignis nach gleichen Fehlschlägen");
    }
}
