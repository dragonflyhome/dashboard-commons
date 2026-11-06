//! `/api/anmeldung/probe` und `/api/anmeldung/wer`: Probe der integrierten
//! Windows-Anmeldung (aus Etappe 7a1). Beide Wege sind frei; die Pflicht auf
//! den geschützten Wegen prüft seit 7b die Schleuse
//! `sicherheit::anmeldung_pflicht`, mit denselben Antworten von hier
//! (`bitte_anmelden`, `fremdes_konto`, `nachweis_aus`).
//!
//! Ablauf auf `/probe`: ohne `Authorization` 401 mit `WWW-Authenticate:
//! Negotiate`; mit `Authorization: Negotiate <base64>` handelt der Server aus
//! (bei NTLM zwei Runden auf derselben Verbindung, Vorrat je Verbindung in
//! `anmeldung::verbindung`), am Ende 200 `{konto: "inhaber", mechanismus}` oder
//! 403 mit Satz. Verglichen wird mit `konten.inhaber_sid` aus der
//! Einstellungsdatei; fehlt sie (Altweg, Tests), mit der SID des eigenen
//! Prozesses. Ins Protokoll kommt nur „SID passt“ oder „SID passt nicht“.

use axum::Extension;
use axum::extract::State;
use axum::extract::connect_info::ConnectInfo;
use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::anmeldung::Mechanismus;
use crate::anmeldung::verbindung::Verbindung;
use crate::app::AppState;
use crate::fehler::AppError;

#[derive(Serialize)]
#[cfg_attr(not(windows), allow(dead_code))]
struct Probe {
    konto: &'static str,
    mechanismus: Mechanismus,
}

#[derive(Serialize)]
#[cfg_attr(not(windows), allow(dead_code))]
struct Wer {
    verfuegbar: bool,
    paket: &'static str,
}

/// 401 mit `WWW-Authenticate: Negotiate` (ohne oder mit Nachweis für die
/// nächste Runde) und dem Körper `{fehler, art: "anmeldung"}`.
pub fn bitte_anmelden(nachweis: Option<&[u8]>) -> Response {
    let mut res = AppError::anmeldung_noetig().into_response();
    let wert = match nachweis {
        Some(n) if !n.is_empty() => format!("Negotiate {}", crate::anmeldung::base64::kodieren(n)),
        _ => "Negotiate".to_string(),
    };
    if let Ok(v) = HeaderValue::from_str(&wert) {
        res.headers_mut().insert(header::WWW_AUTHENTICATE, v);
    }
    res
}

/// 403 mit Satz: angemeldet, aber nicht als Inhaber.
pub fn fremdes_konto() -> Response {
    AppError::fremdes_konto().into_response()
}

/// Hängt einen letzten Nachweis des Servers an (Kerberos mit gegenseitiger
/// Prüfung); ein leerer bleibt weg.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn mit_ausgabe(res: &mut Response, ausgabe: &[u8]) {
    if ausgabe.is_empty() {
        return;
    }
    let wert = format!("Negotiate {}", crate::anmeldung::base64::kodieren(ausgabe));
    if let Ok(v) = HeaderValue::from_str(&wert) {
        res.headers_mut().insert(header::WWW_AUTHENTICATE, v);
    }
}

/// Der Nachweis aus `Authorization: Negotiate <base64>`; `None` ohne Kopf oder
/// mit anderem Verfahren, `Some(Err)` wenn unlesbar.
pub fn nachweis_aus(headers: &HeaderMap) -> Option<Result<Vec<u8>, ()>> {
    let wert = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (verfahren, rest) = wert.split_once(' ')?;
    if !verfahren.eq_ignore_ascii_case("Negotiate") {
        return None;
    }
    Some(crate::anmeldung::base64::dekodieren(rest.trim()).filter(|n| !n.is_empty()).ok_or(()))
}

#[cfg(windows)]
mod windows {
    use super::*;
    use crate::anmeldung::aushandlung::{Ausgang, erwartete_sid, schritt_async};
    use crate::anmeldung::sspi;
    use axum::Json;

    pub async fn probe(state: AppState, nr: Option<u64>, headers: HeaderMap) -> Response {
        let nachweis = match nachweis_aus(&headers) {
            None => return bitte_anmelden(None),
            Some(Err(())) => {
                tracing::warn!("Anmeldung: Nachweis unlesbar");
                return AppError::abgelehnt("Der Windows-Nachweis ist unlesbar.").into_response();
            }
            Some(Ok(n)) => n,
        };
        let erwartet = match erwartete_sid(state.einstellungen.as_deref()) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("Anmeldung: eigene SID nicht lesbar ({e})");
                return AppError::intern("Die Windows-Anmeldung ist gerade nicht möglich.").into_response();
            }
        };
        match schritt_async(nr, nachweis, erwartet).await {
            Ausgang::Weiter(antwort) => bitte_anmelden(Some(&antwort)),
            Ausgang::OhneKontext => {
                tracing::info!("Anmeldung: Fortsetzung ohne offene Aushandlung, neu beginnen");
                bitte_anmelden(None)
            }
            Ausgang::Abgewiesen(e) => {
                tracing::warn!("Anmeldung: Nachweis nicht angenommen ({e})");
                AppError::abgelehnt("Der Windows-Nachweis wurde nicht angenommen.").into_response()
            }
            Ausgang::Intern(e) => {
                tracing::error!("Anmeldung: {e}");
                AppError::intern("Die Windows-Anmeldung ist gerade nicht möglich.").into_response()
            }
            Ausgang::Fertig { ausgabe, inhaber, mechanismus, .. } => {
                let mut res = if inhaber { Json(Probe { konto: "inhaber", mechanismus }).into_response() } else { fremdes_konto() };
                mit_ausgabe(&mut res, &ausgabe);
                res
            }
        }
    }

    pub async fn wer() -> Response {
        let verfuegbar = tokio::task::spawn_blocking(sspi::verfuegbar).await.unwrap_or(false);
        Json(Wer { verfuegbar, paket: sspi::PAKET }).into_response()
    }
}

#[cfg(not(windows))]
fn nur_windows() -> Response {
    AppError {
        status: axum::http::StatusCode::NOT_IMPLEMENTED,
        art: "nicht-verfuegbar",
        fehler: "Die Windows-Anmeldung gibt es nur unter Windows.".to_string(),
    }
    .into_response()
}

/// `GET /api/anmeldung/probe`.
pub async fn probe(State(state): State<AppState>, verbindung: Option<Extension<ConnectInfo<Verbindung>>>, headers: HeaderMap) -> Response {
    let nr = verbindung.map(|Extension(ConnectInfo(v))| v.nr);
    #[cfg(windows)]
    {
        windows::probe(state, nr, headers).await
    }
    #[cfg(not(windows))]
    {
        let _ = (state, nr, headers);
        nur_windows()
    }
}

/// `GET /api/anmeldung/wer`: gelingt AcquireCredentialsHandle für Negotiate?
pub async fn wer() -> Response {
    #[cfg(windows)]
    {
        windows::wer().await
    }
    #[cfg(not(windows))]
    {
        nur_windows()
    }
}
