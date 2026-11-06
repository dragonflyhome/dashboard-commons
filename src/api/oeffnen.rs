//! `POST /api/oeffnen` (Entscheidung lokal-oeffnen): eine Datei
//! oder einen Ordner aus `daten/orte.toml` eines Projekts in der Sitzung des
//! Inhabers öffnen. Die Oberfläche schickt `{"projekt": "…", "kennung": "…"}`,
//! nie einen Pfad.
//!
//! Die Kette, Schritt für Schritt:
//!
//! 1. Schleusen wie bei allen schreibenden Wegen: nur 127.0.0.1/localhost,
//!    Herkunft und CSRF (Origin/Sec-Fetch-Site), Anmeldung als Inhaber
//!    (`/api/oeffnen` steht in `sicherheit::geschuetzt`), Zeitlimit, Größe.
//!    Nur POST (GET: 405).
//! 2. Nur im eigenen Windows-Konto: Mit Passkey aus einem anderen Konto 403
//!    (das Fenster ginge sonst in einer anderen Sitzung auf).
//! 3. Nur aus dem Browser (Origin oder Sec-Fetch-Site), nur JSON (415).
//! 4. Genau `projekt` und `kennung` (einfache Namen); alles andere, etwa ein
//!    `pfad`: 422.
//! 5. Höchstens 10 je Minute (429).
//! 6. Projekt und Kennung nachschlagen (404).
//! 7. Ohne bekanntes Profil des Inhabers (leere `inhaber_sid` im Dienst,
//!    fehlendes `USERPROFILE`): 503, nie das Profil des Dienstkontos.
//! 8. Pfad als Text prüfen (`oeffnen::vorpruefen`: Form, Tabu, Lage). Auflösen
//!    kann der Dienst ihn nicht: sein Konto darf das Profil des Inhabers nicht
//!    lesen, ein Auflösen dort scheiterte als „unlesbar“.
//! 9. Der Bote prüft alles selbst (auflösen, Endung, Programm) und öffnet
//!    (`bote::oeffnen`); nur vom Dienstkonto, höchstens 10 je Minute.
//! 10. Journal: Vorgang `oeffnen`, gleich erledigt, mit Projekt und Kennung;
//!     keine Meldung, keine Glocke, kein Chat wird geweckt.

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::{Extension, Json};
use serde::Deserialize;

use crate::anmeldung::Inhaber;
use crate::api::vorgaenge::{aus_browser, json_kopf_pruefen};
use crate::app::AppState;
use crate::fehler::AppError;
use crate::inhalt::seite;
use crate::oeffnen::{self, Abweisung, Regeln};
use crate::pc::inventory;
use crate::vertrag::api::OeffnenAntwort;
use crate::vertrag::bote::{Anfrage, Antwort};
use crate::vertrag::daten::{DatenInhalt, ist_kennung};

/// 403 mit Passkey aus einem anderen Windows-Konto (Wortlaut der Entscheidung).
pub const SATZ_EIGENES_KONTO: &str = "Öffnen geht nur in deinem eigenen Windows-Konto, dort erscheint das Fenster.";
pub const SATZ_NUR_BROWSER: &str = "Öffnen geht nur über den Knopf im Dashboard.";
pub const SATZ_KOERPER: &str =
    "Öffnen nimmt nur `projekt` und `kennung` aus daten/orte.toml, nie einen Pfad. Etwa {\"projekt\": \"beispiel\", \"kennung\": \"bauplan\"}.";
pub const SATZ_GRENZE: &str = "Höchstens 10 Öffnungen je Minute. Bitte kurz warten.";
pub const SATZ_GEOEFFNET: &str = "Geöffnet.";
pub const SATZ_PROFIL_FEHLT: &str = "Öffnen geht gerade nicht: Das Profil des Inhabers ist unbekannt (in dienst.toml fehlt konten.inhaber_sid).";
pub const SATZ_BOTE_FEHLT: &str = "Öffnen geht gerade nicht: Der Bote (das Hilfsprogramm in deiner Windows-Sitzung) antwortet nicht.";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Koerper {
    projekt: String,
    kennung: String,
}

fn fehler(status: StatusCode, art: &'static str, satz: impl Into<String>) -> AppError {
    AppError { status, art, fehler: satz.into() }
}

/// Eine Abweisung als Antwort: fehlt der Ort 404, fehlt die Tabu-Liste 503, sonst 422.
fn abweisung(a: &Abweisung) -> AppError {
    match a {
        Abweisung::KeineListe => fehler(StatusCode::SERVICE_UNAVAILABLE, "tabu-liste", a.satz()),
        _ => fehler(StatusCode::UNPROCESSABLE_ENTITY, "ort-abgelehnt", a.satz()),
    }
}

/// Die Regeln des Servers: Profil des Inhabers (über seine SID), die
/// Inhaltsquellen, die Ordner des Dienstes und die Tabu-Liste aus dem Inhalt
/// (je Anfrage frisch gelesen; fehlt sie, öffnet nichts).
pub fn regeln(state: &AppState) -> Regeln {
    #[cfg(test)]
    if let Some(r) = state.oeffnen.regeln.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        return r;
    }
    let e = state.einstellungen.as_deref();
    let profil = oeffnen::inhaber_profil(e);
    let liste = e.map(|e| e.ausschluss_datei()).unwrap_or_else(|| state.daten_ordner().join(crate::dienst::einstellungen::AUSSCHLUSS_DATEI));
    let tabu = inventory::ausschluss_laden(Some(&liste)).ok();
    let mut verboten = oeffnen::ordner_des_dienstes(e);
    verboten.extend(state.daten_datei.parent().map(std::path::Path::to_path_buf));
    verboten.push(state.sicherungs_ordner());
    verboten.push(state.web.build().to_path_buf());
    Regeln::des_servers(profil, state.quellen.to_vec(), verboten, tabu)
}

/// Die Anfrage an den Boten (in Tests an den Ersatz).
async fn an_bote(state: &AppState, a: Anfrage) -> Result<Antwort, AppError> {
    #[cfg(test)]
    {
        let ersatz = state.oeffnen.ersatz.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(f) = ersatz {
            return Ok(f(a).await);
        }
    }
    let Some(bote) = state.bote.clone() else {
        return Err(fehler(StatusCode::SERVICE_UNAVAILABLE, "bote-fehlt", SATZ_BOTE_FEHLT));
    };
    bote.anfragen(&a).await.map_err(|s| {
        tracing::warn!("Öffnen: Bote nicht erreicht ({s})");
        fehler(StatusCode::SERVICE_UNAVAILABLE, "bote-fehlt", SATZ_BOTE_FEHLT)
    })
}

/// `POST /api/oeffnen`.
pub async fn oeffnen(
    State(state): State<AppState>,
    inhaber: Option<Extension<Inhaber>>,
    kopf: HeaderMap,
    koerper: Bytes,
) -> Result<Json<OeffnenAntwort>, AppError> {
    match inhaber {
        Some(Extension(Inhaber::Hauptkonto)) => {}
        Some(Extension(Inhaber::Passkey(_))) => {
            tracing::info!("Öffnen abgelehnt: Passkey aus einem anderen Windows-Konto.");
            return Err(fehler(StatusCode::FORBIDDEN, "fremdes-konto", SATZ_EIGENES_KONTO));
        }
        None => return Err(AppError::anmeldung_noetig()),
    }
    if !aus_browser(&kopf) {
        return Err(AppError::abgelehnt(SATZ_NUR_BROWSER));
    }
    json_kopf_pruefen(&kopf)?;
    let k: Koerper = serde_json::from_slice(&koerper).map_err(|_| AppError::unlesbar(SATZ_KOERPER))?;
    if !seite::is_valid_slug(&k.projekt) || !ist_kennung(&k.kennung) {
        return Err(AppError::unlesbar(SATZ_KOERPER));
    }
    if !state.oeffnen.grenze.nehmen(std::time::Instant::now()) {
        tracing::warn!("Öffnen: mehr als {} je Minute, abgelehnt.", oeffnen::JE_MINUTE);
        return Err(AppError::zu_viele(SATZ_GRENZE));
    }
    // Ohne Datenspeicher kein Eintrag im Journal: dann wird auch nicht geöffnet.
    let speicher = state.speicher()?;
    let pfad = {
        let b = state.inhalt.bestand();
        if !b.ordner_da(&k.projekt) {
            return Err(AppError::nicht_gefunden(format!("Ein Projekt „{}“ gibt es nicht.", k.projekt)));
        }
        match b.daten(&k.projekt, "orte") {
            Ok(Some((DatenInhalt::Orte(o), _))) => o.ort(&k.kennung).map(|o| o.pfad.clone()),
            Ok(_) => None,
            Err(_) => {
                return Err(AppError::unlesbar("daten/orte.toml hat einen Fehler; `dashboard pruefe` nennt ihn."));
            }
        }
    };
    let Some(pfad) = pfad else {
        return Err(AppError::nicht_gefunden(format!("Im Projekt „{}“ gibt es keinen Ort „{}“.", k.projekt, k.kennung)));
    };
    let r = regeln(&state);
    if r.profil.is_none() {
        // Nie das Profil des Dienstkontos und nie „.“ als Wurzel (Abnahme).
        tracing::warn!("Öffnen: das Profil des Inhabers ist unbekannt (konten.inhaber_sid leer oder USERPROFILE fehlt).");
        return Err(fehler(StatusCode::SERVICE_UNAVAILABLE, "profil-fehlt", SATZ_PROFIL_FEHLT));
    }
    let p = pfad.clone();
    let geprueft =
        tokio::task::spawn_blocking(move || oeffnen::vorpruefen(&p, &r)).await.map_err(|_| AppError::intern("Die Prüfung des Orts wurde abgebrochen."))?;
    geprueft.map_err(|a| {
        tracing::warn!("Öffnen {}/{}: vom Server abgelehnt ({a:?}).", k.projekt, k.kennung);
        abweisung(&a)
    })?;
    match an_bote(&state, Anfrage::Oeffnen { pfad }).await? {
        Antwort::Geoeffnet => {}
        Antwort::Fehler { satz } => {
            tracing::warn!("Öffnen {}/{}: vom Boten abgelehnt.", k.projekt, k.kennung);
            return Err(fehler(StatusCode::UNPROCESSABLE_ENTITY, "ort-abgelehnt", satz));
        }
        andere => {
            tracing::warn!("Öffnen: unerwartete Antwort des Boten ({}).", andere.art());
            return Err(fehler(StatusCode::SERVICE_UNAVAILABLE, "bote-fehlt", SATZ_BOTE_FEHLT));
        }
    }
    let (p, kn) = (k.projekt.clone(), k.kennung.clone());
    match crate::speicher::Speicher::im_hintergrund(speicher, move |s| s.oeffnung_vermerken(&p, &kn)).await {
        // Nur der Live-Kanal der Oberfläche; kein Chat wird geweckt, keine Meldung.
        Ok((_, e)) => {
            let _ = state.vorgang_events.send((&e).into());
        }
        Err(e) => tracing::error!("Öffnen {}/{}: nicht im Journal ({e}).", k.projekt, k.kennung),
    }
    tracing::info!("Öffnen {}/{}: geöffnet.", k.projekt, k.kennung);
    Ok(Json(OeffnenAntwort { projekt: k.projekt, kennung: k.kennung, satz: SATZ_GEOEFFNET.to_string() }))
}

#[cfg(test)]
mod tests;
