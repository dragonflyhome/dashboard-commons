//! `GET /api/fassungen` (frei, das Changelog ist öffentlich): die laufende
//! Version, Unreleased und alle Fassungen aus dem eingebetteten Changelog
//! (`crate::fassungen`), neueste zuerst; `?seit=<x.y.z>` nur die neueren.
//! Schreibt nichts.
//!
//! `/api/fassungen/gezeigt` (geschützt): „Neu in …“ für die Oberfläche;
//! `GET` nennt die laufende Fassung, solange sie nicht gezeigt wurde, `POST`
//! merkt sie im Datenspeicher als gezeigt.

use axum::Json;
use axum::extract::State;
use axum::http::Uri;

use super::vertrag::{FassungenAntwort, NeuIn};
use super::vorgaenge::abfrage;
use crate::app::AppState;
use crate::fehler::AppError;
use crate::speicher::{Speicher, SpeicherFehler, tabellen};

/// Fehler des Datenspeichers: 500 mit Satz, Einzelheiten ins Protokoll.
fn speicherfehler(e: SpeicherFehler) -> AppError {
    tracing::error!("Fassungen: Datenspeicher: {e} ({})", e.einzelheiten());
    AppError::intern(format!("{e} Einzelheiten stehen im Protokoll."))
}

pub async fn liste(uri: Uri) -> Result<Json<FassungenAntwort>, AppError> {
    let alle = crate::fassungen::alle();
    match abfrage(&uri, "seit") {
        None => Ok(Json(alle.clone())),
        Some(s) => crate::fassungen::seit(alle, &s).map(Json).map_err(AppError::unlesbar),
    }
}

/// `GET /api/fassungen/gezeigt`: die laufende Fassung, solange ihr „Neu in …“ nicht gezeigt wurde.
pub async fn neu_in(State(state): State<AppState>) -> Result<Json<NeuIn>, AppError> {
    let gezeigt = Speicher::im_hintergrund(state.speicher()?, |s| s.meta_lesen(tabellen::META_NEU_IN_GEZEIGT)).await.map_err(speicherfehler)?;
    Ok(Json(NeuIn { neu_in: crate::fassungen::neu_in(crate::fassungen::alle(), gezeigt.as_deref()) }))
}

/// `POST /api/fassungen/gezeigt` (ohne Inhalt): Das „Neu in …“ der laufenden Fassung ist gezeigt.
pub async fn gezeigt(State(state): State<AppState>) -> Result<Json<NeuIn>, AppError> {
    Speicher::im_hintergrund(state.speicher()?, |s| s.neu_in_gezeigt(crate::fassungen::LAUFEND)).await.map_err(speicherfehler)?;
    Ok(Json(NeuIn { neu_in: None }))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use crate::app::AppState;

    async fn hole(pfad: &str) -> (StatusCode, serde_json::Value) {
        let state = AppState::fuer_tests(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel"));
        let req = Request::get(pfad).header("host", "127.0.0.1:8787").body(Body::empty()).unwrap();
        let res = crate::app::router(state).oneshot(req).await.unwrap();
        let status = res.status();
        (status, serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap())
    }

    /// Ohne Anmeldung lesbar, in der Form der Entscheidung.
    #[tokio::test]
    async fn frei_mit_allen_fassungen() {
        let (status, v) = hole("/api/fassungen").await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["laufend"], env!("CARGO_PKG_VERSION"));
        for feld in ["hinzugefuegt", "geaendert", "entfernt", "behoben", "brueche"] {
            assert!(v["unveroeffentlicht"][feld].is_array(), "{feld}: {v}");
        }
        let f = v["fassungen"].as_array().unwrap();
        assert!(f.len() >= 2, "{v}");
        let erste = &f[0];
        for feld in ["version", "tag", "datum", "hinzugefuegt", "geaendert", "entfernt", "behoben", "brueche"] {
            assert!(!erste[feld].is_null(), "{feld}: {erste}");
        }
        assert_eq!(erste["tag"], format!("v{}", erste["version"].as_str().unwrap()));
        assert_eq!(v["hinweise"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn seit_kuerzt_und_falsches_seit_ist_422() {
        let (_, alle) = hole("/api/fassungen").await;
        let zweite = alle["fassungen"][1]["version"].as_str().unwrap().to_string();
        let (status, v) = hole(&format!("/api/fassungen?seit={zweite}")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(v["fassungen"].as_array().map(Vec::len), Some(1), "{v}");
        assert_eq!(v["fassungen"][0], alle["fassungen"][0]);
        let (status, v) = hole("/api/fassungen?seit=neu").await;
        assert_eq!((status, v["art"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("unlesbar")), "{v}");
    }

    async fn json(res: axum::response::Response) -> serde_json::Value {
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap()
    }

    /// „Neu in …“: ohne Anmeldung 401; angemeldet die laufende Fassung, nach dem POST nichts mehr.
    #[tokio::test]
    async fn neu_in_bis_zum_post() {
        let state = AppState::fuer_tests(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel"));
        let hole = || Request::get("/api/fassungen/gezeigt").header("host", "localhost:8787").body(Body::empty()).unwrap();
        assert_eq!(crate::app::router(state.clone()).oneshot(hole()).await.unwrap().status(), StatusCode::UNAUTHORIZED);
        let app = crate::app::router_angemeldet(state);
        let v = json(app.clone().oneshot(hole()).await.unwrap()).await;
        assert_eq!(v["neu_in"]["version"], env!("CARGO_PKG_VERSION"), "{v}");
        assert!(v["neu_in"]["datum"].is_string(), "{v}");
        let post = Request::post("/api/fassungen/gezeigt")
            .header("host", "localhost:8787")
            .header("origin", "http://localhost:8787")
            .header("sec-fetch-site", "same-origin")
            .body(Body::empty())
            .unwrap();
        let res = app.clone().oneshot(post).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(json(res).await, serde_json::json!({ "neu_in": null }));
        assert_eq!(json(app.oneshot(hole()).await.unwrap()).await, serde_json::json!({ "neu_in": null }), "gezeigt bleibt gemerkt");
    }
}
