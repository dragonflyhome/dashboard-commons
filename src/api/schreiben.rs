//! Gemeinsamer Schreibweg für die privaten Dateien des Kalenders, die der Server
//! selbst schreibt (Ort merken, Auswahl der Gebetszeiten): eine Sperre je Datei,
//! Prüfung des Projekts und des alten Stands, der neue Text geprüft, bevor er über
//! eine eindeutige Zwischendatei die alte ersetzt, Aufräumen im Fehlerfall und
//! Abgleich des Inhalts danach. Die Sätze bitten nie um Handarbeit an Dateien.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, OnceLock};

use crate::app::AppState;
use crate::fehler::AppError;
use crate::vertrag::daten::{Baustein, Datei, datei_lesen};
use crate::vertrag::lesen::LeseFehler;

/// Sätze für die Fälle, in denen nichts geschrieben wird.
pub struct Saetze {
    /// Das Projekt `kalender` fehlt.
    pub ohne_projekt: &'static str,
    /// Die alte Datei hat einen Fehler oder ist in Teile gegliedert.
    pub alt_fehlerhaft: &'static str,
    /// Der neue Text ließ sich nicht sauber bilden.
    pub neu_fehlerhaft: &'static str,
    /// Schreiben oder Umbenennen ging nicht.
    pub nicht_geschrieben: &'static str,
}

/// Eine Sperre je Datei (Stamm ohne `.toml`).
fn sperre(stamm: &str) -> Arc<tokio::sync::Mutex<()>> {
    static SPERREN: OnceLock<Mutex<BTreeMap<String, Arc<tokio::sync::Mutex<()>>>>> = OnceLock::new();
    let mut s = SPERREN.get_or_init(Default::default).lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    Arc::clone(s.entry(stamm.to_string()).or_default())
}

/// Schreibt `projects/kalender/daten/<stamm>.toml` neu. `neu` bekommt den alten
/// Inhalt (`None` ohne Datei) und liefert den neuen Text samt Ergebnis für den
/// Aufrufer; `pruefen` prüft den neuen Text über das Lesen hinaus (etwa eindeutige Kennungen).
pub async fn privat_schreiben<T: Baustein, R>(
    state: &AppState,
    stamm: &str,
    saetze: &Saetze,
    neu: impl FnOnce(Option<(&str, T)>) -> Result<(String, R), AppError>,
    pruefen: impl Fn(&Datei<T>, &str, &str) -> Result<(), LeseFehler>,
) -> Result<R, AppError> {
    let sperre = sperre(stamm);
    let _gesperrt = sperre.lock().await;
    let projekt = state.inhalt.ordner().join(crate::kalender::orte::PROJEKT);
    if !projekt.join("projekt.toml").is_file() {
        return Err(AppError::unlesbar(saetze.ohne_projekt));
    }
    let datei = projekt.join("daten").join(format!("{stamm}.toml"));
    let name = format!("projects/{}/daten/{stamm}.toml", crate::kalender::orte::PROJEKT);
    // Der Stand auf der Platte, nicht der Zwischenstand des Inhalts (er gleicht nur alle paar Sekunden ab).
    let alt = match std::fs::read_to_string(&datei) {
        Ok(t) => Some(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Err(AppError::intern(saetze.nicht_geschrieben)),
    };
    let (text, ergebnis) = match &alt {
        // Auch eine alte Datei, die zwar lesbar ist, aber die Zusatzprüfung nicht besteht
        // (etwa doppelte Kennungen), ist ein Fehler der Datei, kein Fehler des Dienstes.
        Some(t) => match datei_lesen::<T>(&name, t).and_then(|d| pruefen(&d, &name, t).map(|()| d)) {
            Ok(Datei::Ganz(g)) => neu(Some((t.as_str(), g)))?,
            Ok(Datei::Teile { .. }) | Err(_) => return Err(AppError::unlesbar(saetze.alt_fehlerhaft)),
        },
        None => neu(None)?,
    };
    let gut = datei_lesen::<T>(&name, &text).and_then(|d| pruefen(&d, &name, &text)).is_ok();
    if !gut {
        return Err(AppError::intern(saetze.neu_fehlerhaft));
    }
    let tmp = datei.with_file_name(format!("{stamm}.toml.{}-{}.neu", std::process::id(), chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()));
    let geschrieben =
        std::fs::create_dir_all(datei.parent().unwrap_or(&datei)).and_then(|()| std::fs::write(&tmp, text)).and_then(|()| std::fs::rename(&tmp, &datei));
    if geschrieben.is_err() {
        let _ = std::fs::remove_file(&tmp);
        return Err(AppError::intern(saetze.nicht_geschrieben));
    }
    // Gleich abgleichen, damit der nächste Aufruf den neuen Stand schon sieht.
    let inhalt = state.inhalt.clone();
    // Die Änderung geht auch in den Ereigniskanal: Sonst erfahren offene Seiten nichts davon,
    // weil der Beobachter danach keinen Unterschied mehr sieht.
    if let Ok(a) = tokio::task::spawn_blocking(move || inhalt.abgleichen()).await {
        super::events::abgeglichen_melden(state, &a);
    }
    Ok(ergebnis)
}
