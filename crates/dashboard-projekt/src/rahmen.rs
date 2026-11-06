//! Der Rahmen um den Router eines Moduls, im Dienst und im Prüfstand gleich:
//!
//! - ein Absturz im Endpunkt wird zu 500 mit Satz (nur diese Anfrage), dazu
//!   ein Fund in /api/pruefung;
//! - Körper höchstens 64 KiB;
//! - jede Fehlerantwort hat die Form `{ fehler, art }`;
//! - unbekannte Wege: 404 mit Satz;
//! - Protokoll mit `projekt=<slug>`.
//!
//! Anmeldepflicht (7b) und Herkunftsprüfung für schreibende Wege setzt der
//! Dienst außen herum, wie für alle Wege; das Präfix `/api/projekte/<slug>/x/`
//! setzt das Register, nicht das Modul.

use axum::Router;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tracing::Instrument;

use crate::Modul;
use crate::fehler::{Fehler, absturz_grund};
use crate::kontext::ProjektKontext;

/// Höchstgröße eines Körpers an einen Weg des Moduls.
pub const KOERPER_HOECHSTENS: usize = 64 * 1024;

/// Der Router des Moduls im Rahmen, mit seinem Kontext als Zustand.
pub fn einpacken<S: Clone + Send + Sync + 'static>(modul: &dyn Modul, kontext: ProjektKontext) -> Router<S> {
    let fuer_absturz = kontext.clone();
    let slug: std::sync::Arc<str> = std::sync::Arc::from(kontext.slug());
    let s2 = slug.clone();
    modul
        .router()
        .fallback(move || {
            let slug = s2.clone();
            async move { Fehler::nicht_gefunden(format!("Diesen Weg gibt es im Modul „{slug}“ nicht.")) }
        })
        .layer(middleware::from_fn(fehlerform))
        .layer(CatchPanicLayer::custom(move |e: Box<dyn std::any::Any + Send + 'static>| {
            let grund = absturz_grund(&*e);
            fuer_absturz.fehler_melden("endpunkt", &format!("abgestürzt: {grund}"));
            Fehler::intern(format!(
                "Das Modul „{}“ ist bei dieser Anfrage abgestürzt. Der Fehler steht im Protokoll und auf /api/pruefung.",
                fuer_absturz.slug()
            ))
            .into_response()
        }))
        .layer(middleware::from_fn(groesse))
        .layer(RequestBodyLimitLayer::new(KOERPER_HOECHSTENS))
        .layer(middleware::from_fn_with_state(slug, spanne))
        .with_state(kontext)
}

/// Protokoll: jede Anfrage im Span `projekt=<slug>`.
async fn spanne(State(slug): State<std::sync::Arc<str>>, req: Request, next: Next) -> Response {
    let span = tracing::info_span!("modul", projekt = %slug);
    next.run(req).instrument(span).await
}

/// Zu große Körper ablehnen: an der Längenangabe, sonst beim Lesen (der
/// Körper wird dafür hier gelesen, höchstens 64 KiB, und weitergereicht).
async fn groesse(req: Request, next: Next) -> Response {
    let zu_gross = || Fehler::zu_gross("Zu viele Daten auf einmal (höchstens 64 KiB).").into_response();
    let laenge = req.headers().get(header::CONTENT_LENGTH).and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<usize>().ok());
    if laenge.is_some_and(|n| n > KOERPER_HOECHSTENS) {
        return zu_gross();
    }
    let (teile, koerper) = req.into_parts();
    let Ok(bytes) = axum::body::to_bytes(koerper, KOERPER_HOECHSTENS).await else { return zu_gross() };
    next.run(Request::from_parts(teile, axum::body::Body::from(bytes))).await
}

/// Fehlerantworten ohne JSON (etwa 405 oder 413 aus axum) bekommen die Form `{ fehler, art }`.
async fn fehlerform(req: Request, next: Next) -> Response {
    let res = next.run(req).await;
    let status = res.status();
    let ist_json = res.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()).is_some_and(|v| v.starts_with("application/json"));
    if !(status.is_client_error() || status.is_server_error()) || ist_json {
        return res;
    }
    let (art, satz) = match status {
        StatusCode::NOT_FOUND => ("nicht-gefunden", "Diesen Weg gibt es nicht."),
        StatusCode::METHOD_NOT_ALLOWED => ("methode", "Diese Methode gibt es auf diesem Weg nicht."),
        StatusCode::PAYLOAD_TOO_LARGE => ("zu-gross", "Zu viele Daten auf einmal (höchstens 64 KiB)."),
        StatusCode::UNSUPPORTED_MEDIA_TYPE => ("unlesbar", "Bitte den Körper als JSON schicken (Content-Type: application/json)."),
        s if s.is_client_error() => ("unlesbar", "Die Anfrage passt nicht zu diesem Weg."),
        _ => ("intern", "Im Modul ist etwas schiefgegangen. Der Fehler steht im Protokoll."),
    };
    let mut neu = Fehler::neu(status, art, satz).into_response();
    if let Some(v) = res.headers().get(header::ALLOW) {
        neu.headers_mut().insert(header::ALLOW, v.clone());
    }
    neu.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    neu
}
