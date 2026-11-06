//! Der Bau-Stand mit Satz (Etappe 4c).
//!
//! - `GET /api/bauen`: der Bau-Stand aller Projekte mit eigener Seite, bei rot
//!   mit Satz (geschützt: `/api/bauen` steht in `sicherheit::geschuetzt`).
//!   Zustand und Zeit stehen frei in `/api/projects`.
//!
//! Einen Bau bestellt niemand hier: Die Oberfläche baut von selbst nach dem
//! Speichern, Chats bestellen einen Bau mit dem Vorgang `bau`. Den Weg
//! `POST /api/bauen/<slug>` gibt es seit Etappe 6d nicht mehr (erst
//! ohne Knopf, danach ohne Aufrufer).

use axum::Json;
use axum::extract::State;

use crate::api::vertrag::BauListe;
use crate::app::AppState;
use crate::baustand;
use crate::fehler::AppError;

/// `GET /api/bauen` (nur der Inhaber).
pub async fn liste(State(state): State<AppState>) -> Result<Json<BauListe>, AppError> {
    let s = state.clone();
    let alle = tokio::task::spawn_blocking(move || baustand::alle(&s)).await.map_err(|e| {
        tracing::warn!("Bau-Stand: Lesen abgebrochen ({e})");
        AppError::intern("Der Bau-Stand ließ sich gerade nicht lesen.")
    })?;
    let projekte = alle.iter().map(|(slug, e)| e.mit_satz(slug)).collect();
    Ok(Json(BauListe { projekte }))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Method, Request, StatusCode, header};
    use axum::{Extension, Router};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use crate::anmeldung::verbindung::Verbindung;
    use crate::app::{AppState, beispiel_fuer_tests, router, router_angemeldet};

    /// Ein Projektordner mit einem Projekt mit eigener Seite (`probe`) und einem ohne (`nur-kopf`).
    fn projekte(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dashboard-bauen-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        for slug in ["probe", "nur-kopf"] {
            std::fs::create_dir_all(d.join(slug)).unwrap();
            std::fs::write(d.join(slug).join("projekt.toml"), "schema = 3\ntitle = \"Probe\"\nstatus = \"aktiv\"\nsummary = \"Für den Test.\"\n").unwrap();
        }
        std::fs::write(d.join("probe").join("Seite.svelte"), "<p>Probe</p>\n").unwrap();
        d
    }

    fn zustand(name: &str) -> AppState {
        AppState::fuer_tests(&beispiel_fuer_tests()).mit_projekten(projekte(name))
    }

    fn anfrage(methode: Method, pfad: &str) -> Request<Body> {
        Request::builder()
            .method(methode)
            .uri(pfad)
            .header(header::HOST, "localhost:8787")
            .header(header::ORIGIN, "http://localhost:8787")
            .body(Body::empty())
            .unwrap()
    }

    async fn schicke(app: Router, req: Request<Body>) -> (StatusCode, serde_json::Value) {
        let res = app.oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
    }

    /// Ohne Anmeldung (frische Verbindung, kein Ausweis).
    fn ohne(state: &AppState) -> Router {
        router(state.clone()).layer(Extension(ConnectInfo(Verbindung::neu())))
    }

    #[tokio::test]
    async fn ohne_anmeldung_401() {
        let state = zustand("ohne");
        let (status, _) = schicke(ohne(&state), anfrage(Method::GET, "/api/bauen")).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "der Satz bei rot ist geschützt");
    }

    /// Ein fremdes Windows-Konto ohne Passkey: wie auf allen geschützten Wegen
    /// 401 mit Art `passkey` (die Oberfläche bietet dann die Anmeldung mit
    /// Passkey an).
    #[cfg(windows)]
    #[tokio::test]
    async fn fremdes_konto_wird_abgewiesen() {
        let state = zustand("fremd");
        let v = Verbindung::neu();
        crate::anmeldung::verbindung::merken(v.nr, crate::anmeldung::WindowsKonto::test(false));
        let app = router(state.clone()).layer(Extension(ConnectInfo(v)));
        let (status, body) = schicke(app, anfrage(Method::GET, "/api/bauen")).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
        assert_eq!(body["art"], "passkey", "{body}");
    }

    /// Etappe 6d: Bestellen geht nur noch über Beobachter und Vorgang `bau`;
    /// auch der Inhaber bestellt über `POST /api/bauen/<slug>` nichts mehr.
    #[tokio::test]
    async fn post_bestellt_nichts() {
        let state = zustand("post");
        let (status, v) = schicke(router_angemeldet(state.clone()), anfrage(Method::POST, "/api/bauen/probe")).await;
        assert!(!status.is_success(), "{status} {v}");
        assert!(state.baumeister.im_bau().is_empty());
    }

    /// Der Bau-Stand sagt „läuft“, solange ein Bau ansteht: frei in
    /// /api/projects, geschützt mit /api/bauen; ohne eigene Seite kein Stand.
    #[tokio::test]
    async fn bau_stand_frei_und_geschuetzt() {
        let state = zustand("stand");
        state.baumeister.aenderung(["probe".to_string()]);
        let (_, l) = schicke(router_angemeldet(state.clone()), anfrage(Method::GET, "/api/projects")).await;
        let probe = l["projects"].as_array().unwrap().iter().find(|p| p["slug"] == "probe").unwrap().clone();
        assert_eq!(probe["bau"]["zustand"], "laeuft", "{probe}");
        let ohne_seite = l["projects"].as_array().unwrap().iter().find(|p| p["slug"] == "nur-kopf").unwrap().clone();
        assert!(ohne_seite.get("bau").is_none(), "ohne eigene Seite kein Bau-Stand: {ohne_seite}");
        let (status, b) = schicke(router_angemeldet(state.clone()), anfrage(Method::GET, "/api/bauen")).await;
        assert_eq!(status, StatusCode::OK, "{b}");
        assert_eq!(b["projekte"][0]["slug"], "probe");
        assert_eq!(b["projekte"][0]["zustand"], "laeuft");
    }

    /// Zweig nicht-geprueft-sichtbar: Lief ein Prüfwerkzeug nicht, heißt der
    /// Zustand `nicht_geprueft`, mit Kopie (`probe`, mit Zeit) und ohne
    /// (`ohne`, ohne Zeit). Frei in /api/projects nur der Zustand, der Satz
    /// nur angemeldet unter /api/bauen.
    #[tokio::test]
    async fn nicht_geprueft_frei_nur_zustand() {
        use crate::baustand::{SATZ_NICHT_GEPRUEFT_MIT_FASSUNG, SATZ_NICHT_GEPRUEFT_OHNE_FASSUNG};
        use crate::bote::bau::{GRUEN_ORDNER, ProjektStand, RotArt, STAND_DATEI, Stand};
        let p = projekte("nicht-geprueft");
        std::fs::create_dir_all(p.join("ohne")).unwrap();
        std::fs::write(p.join("ohne").join("projekt.toml"), "schema = 3\ntitle = \"Ohne\"\nstatus = \"aktiv\"\nsummary = \"x\"\n").unwrap();
        std::fs::write(p.join("ohne").join("Seite.svelte"), "<p>Ohne</p>\n").unwrap();
        let web = std::env::temp_dir().join(format!("dashboard-bauen-web-nicht-geprueft-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&web);
        let gruen = web.join(GRUEN_ORDNER);
        std::fs::create_dir_all(web.join("build")).unwrap();
        std::fs::create_dir_all(gruen.join("probe")).unwrap();
        std::fs::write(gruen.join("probe").join("Seite.svelte"), "<p>gut</p>\n").unwrap();
        let ng = |gruen_um| ProjektStand { gruen: false, gruen_um, rot_art: Some(RotArt::NichtGeprueft), satz: "SPUERHUND-BOTE".into(), ..Default::default() };
        let stand = Stand { projekte: [("probe".to_string(), ng(Some(1_793_450_700))), ("ohne".to_string(), ng(None))].into() };
        std::fs::write(gruen.join(STAND_DATEI), serde_json::to_string(&stand).unwrap()).unwrap();
        let state = AppState::fuer_tests(&beispiel_fuer_tests()).mit_projekten(p.clone()).mit_web(web.join("build"));

        let (status, l) = schicke(ohne(&state), anfrage(Method::GET, "/api/projects")).await;
        assert_eq!(status, StatusCode::OK, "{l}");
        let bau = |slug: &str| l["projects"].as_array().unwrap().iter().find(|x| x["slug"] == slug).unwrap()["bau"].clone();
        assert_eq!(bau("probe")["zustand"], "nicht_geprueft", "{l}");
        assert!(bau("probe")["letzte_gute"].is_string(), "mit Kopie die Zeit: {l}");
        assert_eq!((bau("ohne")["zustand"].as_str(), bau("ohne")["letzte_gute"].is_null()), (Some("nicht_geprueft"), true), "{l}");
        let t = l.to_string();
        for satz in [SATZ_NICHT_GEPRUEFT_MIT_FASSUNG, SATZ_NICHT_GEPRUEFT_OHNE_FASSUNG, "SPUERHUND-BOTE"] {
            assert!(!t.contains(satz), "ohne Anmeldung kein Satz: {satz}");
        }
        let (status, _) = schicke(ohne(&state), anfrage(Method::GET, "/api/bauen")).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);

        let (status, b) = schicke(router_angemeldet(state.clone()), anfrage(Method::GET, "/api/bauen")).await;
        assert_eq!(status, StatusCode::OK, "{b}");
        let satz = |slug: &str| b["projekte"].as_array().unwrap().iter().find(|x| x["slug"] == slug).unwrap().clone();
        assert_eq!((satz("probe")["zustand"].as_str(), satz("probe")["satz"].as_str()), (Some("nicht_geprueft"), Some(SATZ_NICHT_GEPRUEFT_MIT_FASSUNG)), "{b}");
        assert_eq!((satz("ohne")["zustand"].as_str(), satz("ohne")["satz"].as_str()), (Some("nicht_geprueft"), Some(SATZ_NICHT_GEPRUEFT_OHNE_FASSUNG)), "{b}");
        assert!(!b.to_string().contains("SPUERHUND-BOTE"), "der Satz des Boten (mit Werkzeugen) bleibt in stand.json");
        let _ = std::fs::remove_dir_all(&web);
        let _ = std::fs::remove_dir_all(&p);
    }
}
