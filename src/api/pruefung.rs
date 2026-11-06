//! `/api/pruefung` und `/api/pruefung/<slug>`: die Befunde der Prüfung
//! (`inhalt::pruefung`), dieselben wie bei `dashboard pruefe`. Die
//! Dateiarbeit läuft in `spawn_blocking`, damit sie den Server nicht aufhält.

use axum::extract::{Path, State};
use axum::{Extension, Json};

use crate::anmeldung::Angemeldet;

use super::vertrag::{PruefungGesamt, PruefungProjekt, PruefungsBericht};
use crate::app::AppState;
use crate::fehler::AppError;
use crate::inhalt::pruefung::{self, Befund};

/// Führt `f` im Hintergrund aus; ein Absturz darin wird zu 500 mit Satz.
async fn im_hintergrund<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, AppError> {
    tokio::task::spawn_blocking(f).await.map_err(|e| {
        tracing::error!("Prüfung: {e}");
        AppError::intern("Die Prüfung ist abgebrochen; Einzelheiten stehen im Protokoll.")
    })
}

/// Der Bericht über alle Projekte im Ordner `dir`.
pub fn bericht(dir: &std::path::Path) -> PruefungsBericht {
    bericht_aus(&[dir.to_path_buf()])
}

/// Der Bericht über alle Projekte aller Inhaltsquellen (Etappe 4b, Umzug des Inhalts).
pub fn bericht_aus(quellen: &[std::path::PathBuf]) -> PruefungsBericht {
    let alle = pruefung::pruefe_quellen(quellen);
    let mut gesamt = PruefungGesamt { projekte: alle.len() as u64, ..Default::default() };
    let mut projekte = Vec::new();
    for (slug, befunde) in alle {
        let (f, h) = pruefung::zaehlen(&befunde);
        gesamt.fehler += f as u64;
        gesamt.hinweise += h as u64;
        if befunde.is_empty() {
            continue;
        }
        projekte.push(PruefungProjekt::neu(slug, befunde));
    }
    PruefungsBericht { gesamt, projekte }
}

/// Etappe 4b: Störungen der Backend-Module (Fehler, Abstürze, Zeitlimits von
/// Endpunkten und Hooks) als Funde ihres Projekts dazu. Ein Projekt ohne
/// Ordner (etwa ein Testmodul) erscheint trotzdem, in neuer Form.
pub fn mit_modul_befunden(mut bericht: PruefungsBericht, modul: Vec<(String, Befund)>) -> PruefungsBericht {
    for (slug, befund) in modul {
        // Etappe 4b: Hinweise (Rechenteil aus fremder Quelle) zählen als Hinweis.
        if befund.ist_fehler() {
            bericht.gesamt.fehler += 1;
        } else {
            bericht.gesamt.hinweise += 1;
        }
        match bericht.projekte.iter_mut().find(|p| p.slug == slug) {
            Some(p) => {
                p.befunde.push(befund);
                p.zahlen_neu();
            }
            None => {
                bericht.projekte.push(PruefungProjekt::neu(slug, vec![befund]));
            }
        }
    }
    bericht.projekte.sort_by(|a, b| a.slug.cmp(&b.slug));
    bericht
}

/// Funde, die nicht aus der Prüfung selbst kommen: Störungen der
/// Backend-Module (4b) und Dateien, deren letzter guter Stand im
/// ContentStore gilt, weil sie sich gerade nicht lesen ließen (2d).
fn weitere_befunde(state: &AppState) -> Vec<(String, Befund)> {
    let mut b = state.projekte.stoerungen().befunde();
    let bestand = state.inhalt.bestand();
    b.extend(bestand.lesefehler_befunde());
    // Etappe 4b: ein Slug in zwei Inhaltsquellen (die erste bleibt sichtbar).
    for satz in &bestand.quellen_doppelt {
        let slug = satz.split('„').nth(1).and_then(|r| r.split('“').next()).unwrap_or("projects").to_string();
        b.push((slug, Befund::fehler("DASHBOARD_PROJECTS_DIR", None, satz.clone())));
    }
    // Rechenteil (server/) aus einer weiteren Quelle: wird nicht gebaut, ein Hinweis.
    for (slug, satz) in &bestand.quellen_rechenteile {
        b.push((slug.clone(), Befund::hinweis(format!("projects/{slug}/server"), None, satz.clone())));
    }
    // Unlesbare Quelle (Fehler) und abweichende dashboard-version.toml (Hinweis): Sätze ohne Pfad, beim System-Projekt wie die Lese-Meldungen.
    let system = crate::meldungen::SYSTEM_PROJEKT.to_string();
    b.extend(bestand.quellen_unlesbar.iter().map(|s| (system.clone(), Befund::fehler("DASHBOARD_PROJECTS_DIR", None, s.clone()))));
    b.extend(bestand.quellen_version.iter().map(|s| (system.clone(), Befund::hinweis(crate::quellen::VERSIONSDATEI, None, s.clone()))));
    b
}

/// [zusammenbau]: Rechenteile ohne angemeldetes Modul (allgemeines
/// `dashboard.exe`) als Hinweis ihres Projekts. Liest die Platte.
fn ohne_zusammenbau(state: &AppState) -> Vec<(String, Befund)> {
    crate::app::rechenteile_ohne_modul(state)
        .into_iter()
        .map(|slug| {
            let b = Befund::hinweis(format!("projects/{slug}/server"), None, crate::projekte::SATZ_OHNE_ZUSAMMENBAU);
            (slug, b)
        })
        .collect()
}

/// `GET /api/pruefung`. Etappe B2: ohne Windows-Anmeldung nur Zahlen (gesamt
/// und je Projekt), keine Befund-Sätze; sie können Inhalt zitieren.
pub async fn alle(State(state): State<AppState>, angemeldet: Option<Extension<Angemeldet>>) -> Result<Json<PruefungsBericht>, AppError> {
    let angemeldet = angemeldet.is_some();
    let quellen = state.quellen.to_vec();
    let mut modul = weitere_befunde(&state);
    let s = state.clone();
    modul.extend(im_hintergrund(move || ohne_zusammenbau(&s)).await?);
    let mut bericht = im_hintergrund(move || mit_modul_befunden(bericht_aus(&quellen), modul)).await?;
    bericht.projekte = bericht.projekte.into_iter().map(|p| p.ohne_saetze(angemeldet)).collect();
    Ok(Json(bericht))
}

/// `GET /api/pruefung/<slug>`: 404, wenn es das Projekt nicht gibt.
/// Etappe B2: ohne Windows-Anmeldung nur die Zahlen, keine Befund-Sätze.
pub async fn eines(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    angemeldet: Option<Extension<Angemeldet>>,
) -> Result<Json<PruefungProjekt>, AppError> {
    let angemeldet = angemeldet.is_some();
    let dir = state.projekt_eltern(&slug);
    let s = slug.clone();
    let mut alle_weiteren = weitere_befunde(&state);
    let st = state.clone();
    alle_weiteren.extend(im_hintergrund(move || ohne_zusammenbau(&st)).await?);
    let modul: Vec<Befund> = alle_weiteren.into_iter().filter(|(m, _)| *m == slug).map(|(_, b)| b).collect();
    let ergebnis = im_hintergrund(move || {
        let da = pruefung::wird_geprueft(&dir, &s);
        if !da && modul.is_empty() {
            return None;
        }
        let mut befunde = if da { pruefung::pruefe_projekt(&dir, &s) } else { Vec::new() };
        befunde.extend(modul);
        Some(PruefungProjekt::neu(s, befunde))
    })
    .await?;
    ergebnis.map(|p| Json(p.ohne_saetze(angemeldet))).ok_or_else(|| AppError::nicht_gefunden(format!("Kein Projekt „{slug}“")))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use crate::app::{AppState, router};

    /// Angemeldet (Etappe B2: die Befund-Sätze nur mit Anmeldung).
    async fn hole(pfad: &str) -> (StatusCode, serde_json::Value) {
        let beispiel = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel");
        hole_mit(crate::app::router_angemeldet(AppState::fuer_tests_ohne_speicher(&beispiel, "")), pfad).await
    }

    /// Ohne Anmeldung.
    async fn hole_frei(pfad: &str) -> (StatusCode, serde_json::Value) {
        let beispiel = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel");
        hole_mit(router(AppState::fuer_tests_ohne_speicher(&beispiel, "")), pfad).await
    }

    async fn hole_mit(app: axum::Router, pfad: &str) -> (StatusCode, serde_json::Value) {
        let req = Request::get(pfad).header("host", "127.0.0.1:8787").body(Body::empty()).unwrap();
        let res = app.oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn bericht_ueber_alle_projekte() {
        let (status, v) = hole("/api/pruefung").await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["gesamt"]["fehler"], 2, "{v}");
        assert_eq!(v["gesamt"]["projekte"], 7, "{v}");
        let slugs: Vec<&str> = v["projekte"].as_array().unwrap().iter().map(|p| p["slug"].as_str().unwrap()).collect();
        let mut sortiert = slugs.clone();
        sortiert.sort();
        assert_eq!(slugs, sortiert);
        let kaputt = v["projekte"].as_array().unwrap().iter().find(|p| p["slug"] == "neu-kaputt").unwrap();
        assert!(kaputt.get("form").is_none(), "Etappe 6d: kein Feld form");
        let b = &kaputt["befunde"][0];
        assert_eq!((b["datei"].as_str(), b["zeile"].as_u64(), b["schwere"].as_str()), (Some("projects/neu-kaputt/projekt.toml"), Some(4), Some("fehler")));
        assert!(b["text"].as_str().unwrap().contains("„aktief“"), "{b}");
        assert_eq!((kaputt["fehler"].as_u64(), kaputt["hinweise"].as_u64()), (Some(1), Some(0)), "{kaputt}");
    }

    /// Etappe B2: ohne Anmeldung nur Zahlen (gesamt und je Projekt), keine Befund-Sätze.
    #[tokio::test]
    async fn ohne_anmeldung_nur_zahlen() {
        let (status, v) = hole_frei("/api/pruefung").await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!((v["gesamt"]["fehler"].as_u64(), v["gesamt"]["projekte"].as_u64()), (Some(2), Some(7)), "{v}");
        let kaputt = v["projekte"].as_array().unwrap().iter().find(|p| p["slug"] == "neu-kaputt").cloned().unwrap();
        assert_eq!((kaputt["befunde"].clone(), kaputt["fehler"].as_u64()), (serde_json::json!([]), Some(1)), "{kaputt}");
        assert!(!v.to_string().contains("aktief"), "{v}");
        let (status, e) = hole_frei("/api/pruefung/neu-kaputt").await;
        assert_eq!((status, e["befunde"].clone(), e["fehler"].as_u64()), (StatusCode::OK, serde_json::json!([]), Some(1)), "{e}");
    }

    #[tokio::test]
    async fn ein_projekt_und_unbekannte() {
        let (status, v) = hole("/api/pruefung/neu-form").await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!((v["slug"].as_str(), v.get("form")), (Some("neu-form"), None));
        assert!(v["befunde"].as_array().unwrap().iter().all(|b| b["schwere"] == "hinweis"), "{v}");
        let (status, v) = hole("/api/pruefung/schaufenster").await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let (status, v) = hole("/api/pruefung/gibt-es-nicht").await;
        assert_eq!((status, v["art"].as_str()), (StatusCode::NOT_FOUND, Some("nicht-gefunden")));
        let (status, _) = hole("/api/pruefung/Gross").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    /// Umzug des Inhalts: Projekte aus zwei Quellen stehen im Bericht; ein
    /// Rechenteil ohne angemeldetes Modul (allgemeines dashboard.exe) ist ein
    /// Hinweis mit einem Satz, kein Fehler.
    #[tokio::test]
    async fn zwei_quellen_und_rechenteil_ohne_zusammenbau() {
        let beispiel = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel");
        let extra = std::env::temp_dir().join(format!("dashboard-pruefung-quellen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&extra);
        std::fs::create_dir_all(extra.join("beispiel-a/server/src")).unwrap();
        std::fs::write(extra.join("beispiel-a/projekt.toml"), "schema = 3\ntitle = \"Beispiel A\"\nstatus = \"aktiv\"\n").unwrap();
        let state = AppState::fuer_tests_ohne_speicher(&beispiel, "").mit_quellen(vec![extra.clone(), beispiel.join("projects")]);
        let app = crate::app::router_angemeldet(state);
        let hol = |pfad: &str| Request::get(pfad).header("host", "127.0.0.1:8787").body(Body::empty()).unwrap();
        let res = app.clone().oneshot(hol("/api/pruefung")).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert_eq!(v["gesamt"]["projekte"], 8, "{v}");
        assert_eq!(v["gesamt"]["fehler"], 2, "nur die zwei kaputten Beispiele: {v}");
        let res = app.oneshot(hol("/api/pruefung/beispiel-a")).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let v: serde_json::Value = serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        let satz = v["befunde"].as_array().unwrap().iter().find(|b| b["datei"] == "projects/beispiel-a/server").cloned().unwrap_or_default();
        assert_eq!((satz["schwere"].as_str(), satz["text"].as_str()), (Some("hinweis"), Some(crate::projekte::SATZ_OHNE_ZUSAMMENBAU)), "{v}");
        let _ = std::fs::remove_dir_all(&extra);
    }
}
