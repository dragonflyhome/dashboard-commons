//! `/api/daten`: Stand des Datenspeichers für die Seite /daten, der PC-Verlauf,
//! der Export (TOML zum Ansehen, JSON zum Herunterladen) und die Sicherung
//! auf Knopfdruck. Alle Zugriffe auf die Datei laufen in `spawn_blocking`.
//! Ließ sich der Datenspeicher beim Start nicht öffnen, antwortet `/api/daten`
//! trotzdem (mit `fehler`), alle anderen Wege mit 503 `speicher-fehlt`.

use axum::Json;
use axum::extract::State;
use axum::http::{HeaderValue, Uri, header};
use axum::response::{IntoResponse, Response};
use chrono::Utc;

use super::vertrag::{DatenBericht, PcVerlaufAntwort, PcVerlaufEckdaten, SicherungAntwort, TabelleInfo};
use crate::app::AppState;
use crate::fehler::AppError;
use crate::speicher::{self, Speicher, SpeicherFehler, pc_verlauf, sicherung};

/// Standard und Obergrenze für `?stunden=`.
pub const STUNDEN_STANDARD: u32 = 24;
pub const STUNDEN_HOECHSTENS: u32 = pc_verlauf::AUFBEWAHREN_TAGE as u32 * 24;

/// Wert eines Parameters aus der Abfrage (`?name=wert`), ohne Dekodierung:
/// unsere Werte sind Zahlen und feste Wörter.
fn abfrage<'a>(uri: &'a Uri, name: &str) -> Option<&'a str> {
    uri.query()?.split('&').find_map(|teil| match teil.split_once('=') {
        Some((k, v)) if k == name => Some(v),
        None if teil == name => Some(""),
        _ => None,
    })
}

/// Lesefehler: Satz für den Inhaber, Einzelheiten ins Protokoll.
fn lesefehler(e: SpeicherFehler) -> AppError {
    tracing::error!("Datenspeicher nicht lesbar: {e} ({})", e.einzelheiten());
    AppError::intern("Der Datenspeicher lässt sich gerade nicht lesen; Einzelheiten stehen im Protokoll.")
}

/// Bericht, wenn der Datenspeicher beim Start nicht aufging: 200 mit Ort,
/// Sicherungsordner und dem Satz in `fehler`, damit /daten rot zeigen kann.
fn bericht_ohne_speicher(state: &AppState) -> DatenBericht {
    let datei = state.daten_datei.as_path();
    DatenBericht {
        pfad: speicher::anzeige_pfad(datei),
        groesse_bytes: std::fs::metadata(datei).map(|m| m.len()).unwrap_or(0),
        schema: 0,
        letzte_sicherung: None,
        sicherung_klasse: "bad",
        sicherungs_ordner: speicher::anzeige_pfad(&state.sicherungs_ordner()),
        tabellen: Vec::new(),
        pc_verlauf: PcVerlaufEckdaten { erster: None, letzter: None, eintraege: 0 },
        fehler: Some(state.speicher_satz()),
    }
}

pub async fn bericht(State(state): State<AppState>) -> Result<Json<DatenBericht>, AppError> {
    let Some(offen) = state.speicher.clone() else {
        return Ok(Json(bericht_ohne_speicher(&state)));
    };
    let bericht = Speicher::im_hintergrund(offen, |s| {
        let letzte_sicherung = s.letzte_sicherung();
        let heute = chrono::Local::now().date_naive();
        let (erster, letzter, eintraege) = s.pc_verlauf_eckdaten()?;
        Ok(DatenBericht {
            pfad: speicher::anzeige_pfad(s.pfad()),
            groesse_bytes: s.groesse_bytes(),
            schema: s.schema()?,
            sicherung_klasse: sicherung::klasse(letzte_sicherung.as_deref(), heute),
            letzte_sicherung,
            sicherungs_ordner: speicher::anzeige_pfad(s.sicherungs_ordner()),
            tabellen: s.tabellen_info()?.into_iter().map(|(name, beschreibung, eintraege)| TabelleInfo { name, beschreibung, eintraege }).collect(),
            pc_verlauf: PcVerlaufEckdaten { erster, letzter, eintraege },
            fehler: None,
        })
    })
    .await
    .map_err(lesefehler)?;
    Ok(Json(bericht))
}

/// `?stunden=` prüfen: ganze Zahl ab 1, höchstens 90 Tage.
fn stunden(uri: &Uri) -> Result<u32, AppError> {
    let Some(text) = abfrage(uri, "stunden") else { return Ok(STUNDEN_STANDARD) };
    match text.parse::<u32>() {
        Ok(n) if n >= 1 => Ok(n.min(STUNDEN_HOECHSTENS)),
        _ => Err(AppError::unlesbar(format!("Die Zahl der Stunden ist ungültig. Erlaubt ist eine ganze Zahl von 1 bis {STUNDEN_HOECHSTENS}."))),
    }
}

pub async fn pc_verlauf(State(state): State<AppState>, uri: Uri) -> Result<Json<PcVerlaufAntwort>, AppError> {
    let stunden = stunden(&uri)?;
    let seit = Utc::now() - chrono::Duration::hours(i64::from(stunden));
    let werte = Speicher::im_hintergrund(state.speicher()?, move |s| s.pc_verlauf_lesen(seit)).await.map_err(lesefehler)?;
    Ok(Json(PcVerlaufAntwort { werte }))
}

pub async fn export(State(state): State<AppState>, uri: Uri) -> Result<Response, AppError> {
    let json = match abfrage(&uri, "format") {
        None | Some("toml") => false,
        Some("json") => true,
        Some(_) => {
            return Err(AppError::unlesbar("Dieses Format gibt es nicht. Möglich sind „toml“ und „json“."));
        }
    };
    let export = Speicher::im_hintergrund(state.speicher()?, |s| s.export()).await.map_err(lesefehler)?;
    if !json {
        return Ok(([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], export.als_toml()).into_response());
    }
    let mut res = ([(header::CONTENT_TYPE, "application/json")], export.als_json()).into_response();
    let datei = format!("attachment; filename=\"dashboard-export-{}.json\"", sicherung::heute());
    if let Ok(v) = HeaderValue::from_str(&datei) {
        res.headers_mut().insert(header::CONTENT_DISPOSITION, v);
    }
    Ok(res)
}

pub async fn sichern(State(state): State<AppState>) -> Result<Json<SicherungAntwort>, AppError> {
    let s = Speicher::im_hintergrund(state.speicher()?, |s| s.sichern()).await.map_err(|e| {
        tracing::error!("Sicherung auf Knopfdruck fehlgeschlagen: {e} ({})", e.einzelheiten());
        AppError::intern("Die Sicherung ist fehlgeschlagen; Einzelheiten stehen im Protokoll.")
    })?;
    // Nur die Dateinamen: den Ordner zeigt der Bericht (`sicherungs_ordner`).
    let name = |p: &std::path::Path| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    Ok(Json(SicherungAntwort { datei: name(&s.datei), export: name(&s.export), datum: s.datum }))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;
    use crate::app::{AppState, router_angemeldet as router};

    fn zustand() -> AppState {
        AppState::fuer_tests(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel"))
    }

    fn get(pfad: &str) -> Request<Body> {
        Request::get(pfad).header("host", "127.0.0.1:8787").body(Body::empty()).unwrap()
    }

    async fn lesen(res: Response) -> (StatusCode, String) {
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8_lossy(&bytes).to_string())
    }

    #[test]
    fn abfrage_findet_werte() {
        let uri: Uri = "/x?a=1&stunden=48&leer".parse().unwrap();
        assert_eq!(abfrage(&uri, "stunden"), Some("48"));
        assert_eq!(abfrage(&uri, "leer"), Some(""));
        assert_eq!(abfrage(&uri, "fehlt"), None);
        assert_eq!(stunden(&"/x?stunden=99999".parse().unwrap()).unwrap(), STUNDEN_HOECHSTENS);
        assert_eq!(stunden(&"/x".parse().unwrap()).unwrap(), 24);
        assert!(stunden(&"/x?stunden=0".parse().unwrap()).is_err());
    }

    #[tokio::test]
    async fn bericht_zeigt_schema_und_tabellen() {
        let (status, body) = lesen(router(zustand()).oneshot(get("/api/daten")).await.unwrap()).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["schema"], crate::speicher::migration::SCHEMA_AKTUELL);
        assert!(v["tabellen"].as_array().unwrap().iter().any(|t| t["name"] == "pc_verlauf"), "{body}");
        assert_eq!(v["letzte_sicherung"], serde_json::Value::Null);
        assert_eq!(v["sicherung_klasse"], "bad");
        assert_eq!(v["pc_verlauf"]["eintraege"], 0);
        assert!(v["pfad"].as_str().unwrap().contains("dashboard-test-"), "nie die echte Datei: {body}");
        assert_eq!(v["fehler"], serde_json::Value::Null);
        // Liegt der Temp-Ordner im Benutzerordner, steht dort kein Benutzername.
        if let Some(heim) = std::env::var_os("USERPROFILE")
            && std::env::temp_dir().starts_with(&heim)
        {
            for feld in ["pfad", "sicherungs_ordner"] {
                assert!(v[feld].as_str().unwrap().starts_with("%USERPROFILE%\\"), "{feld}: {body}");
            }
            assert!(!body.contains(&*heim.to_string_lossy().replace('\\', "\\\\")), "{body}");
        }
    }

    const SATZ: &str = "Ein Eintrag im Datenspeicher ist beschädigt.";

    fn ohne_speicher() -> AppState {
        AppState::fuer_tests_ohne_speicher(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel"), SATZ)
    }

    /// Auflage aus der Abnahme 2a: ohne Datenspeicher zeigt /api/daten rot mit
    /// einem Satz, der den Sicherungsordner nennt.
    #[tokio::test]
    async fn bericht_ohne_speicher_nennt_den_sicherungsordner() {
        let state = ohne_speicher();
        let (status, body) = lesen(router(state.clone()).oneshot(get("/api/daten")).await.unwrap()).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["sicherung_klasse"], "bad");
        assert_eq!(v["schema"], 0);
        assert_eq!(v["groesse_bytes"], 0, "die Datei gibt es im Test nicht");
        assert!(v["tabellen"].as_array().unwrap().is_empty());
        let ordner = speicher::anzeige_pfad(&state.sicherungs_ordner());
        assert_eq!(v["sicherungs_ordner"], ordner.as_str());
        assert_eq!(v["fehler"].as_str().unwrap(), format!("Der Datenspeicher lässt sich nicht öffnen. Die letzten Sicherungen liegen in {ordner}. {SATZ}"));
    }

    /// Ohne Datenspeicher: 503 als JSON auf allen anderen Daten-Wegen.
    #[tokio::test]
    async fn ohne_speicher_antworten_die_anderen_wege_mit_503() {
        let app = router(ohne_speicher());
        let post = Request::post("/api/daten/sichern").header("host", "127.0.0.1:8787").header("origin", "http://127.0.0.1:8787").body(Body::empty()).unwrap();
        let antworten = [
            app.clone().oneshot(get("/api/daten/export")).await.unwrap(),
            app.clone().oneshot(get("/api/daten/export?format=json")).await.unwrap(),
            app.clone().oneshot(get("/api/daten/pc-verlauf")).await.unwrap(),
            app.oneshot(post).await.unwrap(),
        ];
        for res in antworten {
            assert!(res.headers()[header::CONTENT_TYPE].to_str().unwrap().starts_with("application/json"));
            let (status, body) = lesen(res).await;
            assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{body}");
            let v: serde_json::Value = serde_json::from_str(&body).unwrap();
            assert_eq!(v["art"], "speicher-fehlt");
            let satz = v["fehler"].as_str().unwrap();
            assert!(satz.contains("Die letzten Sicherungen liegen in") && satz.ends_with(SATZ), "{body}");
        }
    }

    #[tokio::test]
    async fn pc_verlauf_prueft_die_stunden() {
        let app = router(zustand());
        let (status, body) = lesen(app.clone().oneshot(get("/api/daten/pc-verlauf?stunden=abc")).await.unwrap()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["art"], "unlesbar");
        assert!(v["fehler"].as_str().unwrap().contains("Stunden"), "{body}");
        let (status, body) = lesen(app.oneshot(get("/api/daten/pc-verlauf")).await.unwrap()).await;
        assert_eq!(status, StatusCode::OK);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(v["werte"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn export_als_toml_oder_json() {
        let app = router(zustand());
        let res = app.clone().oneshot(get("/api/daten/export?format=json")).await.unwrap();
        assert_eq!(res.headers()[header::CONTENT_TYPE], "application/json");
        let anhang = res.headers()[header::CONTENT_DISPOSITION].to_str().unwrap().to_string();
        assert!(anhang.starts_with("attachment; filename=\"dashboard-export-"), "{anhang}");
        let (_, body) = lesen(res).await;
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["schema"], crate::speicher::migration::SCHEMA_AKTUELL);
        let res = app.clone().oneshot(get("/api/daten/export")).await.unwrap();
        assert_eq!(res.headers()[header::CONTENT_TYPE], "text/plain; charset=utf-8");
        assert!(res.headers().get(header::CONTENT_DISPOSITION).is_none());
        assert!(lesen(res).await.1.contains(&format!("schema = {}", crate::speicher::migration::SCHEMA_AKTUELL)));
        let (status, _) = lesen(app.oneshot(get("/api/daten/export?format=xml")).await.unwrap()).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn sichern_legt_dateien_an() {
        let state = zustand();
        let app = router(state.clone());
        let post = Request::post("/api/daten/sichern").header("host", "127.0.0.1:8787").header("origin", "http://127.0.0.1:8787").body(Body::empty()).unwrap();
        let (status, body) = lesen(app.clone().oneshot(post).await.unwrap()).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        let heute = sicherung::heute();
        assert_eq!(v["datum"], heute.as_str());
        assert_eq!(v["datei"], format!("dashboard-{heute}.redb"), "nur der Dateiname: {body}");
        assert_eq!(v["export"], format!("dashboard-{heute}.toml"), "nur der Dateiname: {body}");
        let offen = state.speicher().unwrap();
        let ordner = offen.sicherungs_ordner();
        assert!(ordner.join(v["datei"].as_str().unwrap()).is_file());
        assert!(ordner.join(v["export"].as_str().unwrap()).is_file());
        let (_, body) = lesen(app.clone().oneshot(get("/api/daten")).await.unwrap()).await;
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["letzte_sicherung"], heute.as_str());
        assert_eq!(v["sicherung_klasse"], "ok");
        // Ohne Herkunft kein Schreiben.
        let ohne = Request::post("/api/daten/sichern").header("host", "127.0.0.1:8787").body(Body::empty()).unwrap();
        assert_eq!(app.oneshot(ohne).await.unwrap().status(), StatusCode::FORBIDDEN);
    }
}
