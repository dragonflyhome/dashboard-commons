//! `GET /api/fassungen` (frei, das Changelog ist öffentlich): die laufende
//! Version, Unreleased und alle Fassungen aus dem eingebetteten Changelog
//! (`crate::fassungen`), neueste zuerst; `?seit=<x.y.z>` nur die neueren.
//! Schreibt nichts.

use axum::Json;
use axum::http::Uri;

use super::vertrag::FassungenAntwort;
use super::vorgaenge::abfrage;
use crate::fehler::AppError;

pub async fn liste(uri: Uri) -> Result<Json<FassungenAntwort>, AppError> {
    let alle = crate::fassungen::alle();
    match abfrage(&uri, "seit") {
        None => Ok(Json(alle.clone())),
        Some(s) => crate::fassungen::seit(alle, &s).map(Json).map_err(AppError::unlesbar),
    }
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
}
