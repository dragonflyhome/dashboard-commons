//! Sitzung → Chat für den Herzschlag (`dashboard herzschlag`, Stop-Hook).
//!
//! Ein Chat schickt bei jedem Aufruf über `scripts\Api.ps1` die Kennung seiner
//! Sitzung in der Kopfzeile `X-Dashboard-Sitzung` mit. Gelingt ein schreibender
//! Aufruf (2xx) mit `von` = `chat:<name>` im Körper, merkt sich der Dienst
//! „diese Sitzung ist dieser Chat“ (der letzte Stand gilt). `GET
//! /api/sitzungen/<id>/offen` nennt dann den Chat und seine offenen Schritte:
//! ihm gehört ein laufender Schritt (oder eine laufende Arbeit ohne Schritte),
//! oder ihm ist einer angeboten. Wartende halten nicht auf; einen Schritt, den
//! er einem anderen angeboten hat, hält der andere.
//!
//! Die Zuordnung liegt nur im Arbeitsspeicher. Nach einem Neustart ist jede
//! Sitzung unbekannt, bis ihr Chat wieder schreibt; eine unbekannte Sitzung
//! hält nichts auf. Geschützt wie `/api/arbeiten` (`sicherheit::geschuetzt`).

use std::collections::HashMap;
use std::sync::Mutex;

use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::http::{Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::vertrag::{Arbeit, ArbeitsStand, OffenerSchritt, SitzungOffen};
use crate::app::AppState;
use crate::fehler::AppError;
use crate::speicher::Speicher;

/// Kopfzeile mit der Sitzungskennung des Aufrufers.
pub const KOPF: &str = "x-dashboard-sitzung";
/// Höchstens so viele Sitzungen; darüber fällt die am längsten nicht gesehene weg.
pub const HOECHSTENS: usize = 1000;
pub const SATZ_SITZUNG: &str = "Die Sitzungskennung hat 1 bis 128 Zeichen aus A–Z, a–z, 0–9, - und _.";

/// Sitzung → (Chat, Zeitpunkt in Aufrufen).
type Karte = HashMap<String, (String, u64)>;

/// Zähler der Aufrufe und die Karte.
#[derive(Default)]
pub struct Zuordnung(Mutex<(u64, Karte)>);

impl Zuordnung {
    pub fn merken(&self, sitzung: &str, von: &str) {
        let mut g = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let (zaehler, karte) = &mut *g;
        *zaehler += 1;
        if karte.len() >= HOECHSTENS
            && !karte.contains_key(sitzung)
            && let Some(alt) = karte.iter().min_by_key(|(_, (_, t))| *t).map(|(k, _)| k.clone())
        {
            karte.remove(&alt);
        }
        karte.insert(sitzung.to_string(), (von.to_string(), *zaehler));
    }

    pub fn chat(&self, sitzung: &str) -> Option<String> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).1.get(sitzung).map(|(von, _)| von.clone())
    }
}

/// Dieselbe Form wie im Herzschlag (`herzschlag::abfrage::sitzung_gueltig`).
pub fn sitzung_gueltig(s: &str) -> bool {
    crate::herzschlag::abfrage::sitzung_gueltig(s)
}

/// Schicht vor den Wegen der API: Nur ein POST mit gültiger Kopfzeile wird
/// gepuffert; nach einer Antwort 2xx zählt `von` aus dem Körper. Nur als
/// Inhaber angemeldet (Erweiterung aus `sicherheit::anmeldung_pflicht`), sonst
/// setzte jeder freie POST-Weg (etwa `/api/passkey/abmelden`) die Zuordnung.
pub async fn merken(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let sitzung = req.headers().get(KOPF).and_then(|v| v.to_str().ok()).filter(|s| sitzung_gueltig(s)).map(str::to_string);
    let inhaber = req.extensions().get::<crate::anmeldung::Inhaber>().is_some();
    let Some(sitzung) = sitzung.filter(|_| inhaber && req.method() == Method::POST) else {
        return next.run(req).await;
    };
    let (teile, koerper) = req.into_parts();
    // shortcut: jeder Fehler beim Puffern wird 413, auch ein Abbruch des Clients; nach Art des Fehlers trennen, wenn die Meldung stört.
    let Ok(bytes) = axum::body::to_bytes(koerper, crate::sicherheit::MAX_KOERPER).await else {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    };
    let von = serde_json::from_slice::<serde_json::Value>(&bytes).ok().and_then(|v| v.get("von").and_then(|v| v.as_str()).map(str::to_string));
    let res = next.run(Request::from_parts(teile, Body::from(bytes))).await;
    if let Some(von) = von.filter(|v| super::arbeiten::ist_chat(v))
        && res.status().is_success()
    {
        state.sitzungen.merken(&sitzung, &von);
    }
    res
}

/// Was `von` aufhält, in der Reihenfolge der Arbeiten.
pub fn offen_fuer(arbeiten: &[Arbeit], von: &str) -> Vec<OffenerSchritt> {
    let mut aus = Vec::new();
    for a in arbeiten {
        let eintrag = |nr, titel: &str, grund: &str| OffenerSchritt { arbeit: a.id.clone(), nr, titel: titel.to_string(), grund: grund.to_string() };
        if a.schritte.is_empty() {
            if a.stand == ArbeitsStand::Laeuft && a.besitzer.as_deref() == Some(von) {
                aus.push(eintrag(None, &a.titel, "laeuft"));
            }
            continue;
        }
        for s in &a.schritte {
            if s.angeboten_an.as_deref() == Some(von) {
                // Ein angebotener, aber wartender oder fertiger Schritt hält nicht auf.
                if matches!(s.stand, ArbeitsStand::Laeuft | ArbeitsStand::AlsNaechstes) {
                    aus.push(eintrag(Some(s.nr), &s.titel, "angeboten"));
                }
            } else if s.angeboten_an.is_none() && s.stand == ArbeitsStand::Laeuft && s.besitzer.as_deref() == Some(von) {
                aus.push(eintrag(Some(s.nr), &s.titel, "laeuft"));
            }
        }
    }
    aus
}

/// `GET /api/sitzungen/<id>/offen`.
pub async fn offen(State(state): State<AppState>, Path(sitzung): Path<String>) -> Result<Json<SitzungOffen>, AppError> {
    if !sitzung_gueltig(&sitzung) {
        return Err(AppError::unlesbar(SATZ_SITZUNG));
    }
    let Some(von) = state.sitzungen.chat(&sitzung) else {
        return Ok(Json(SitzungOffen { sitzung, von: None, offen: Vec::new() }));
    };
    let arbeiten = Speicher::im_hintergrund(state.speicher()?, |s| s.arbeiten_lesen(None)).await.map_err(|e| {
        tracing::error!("Sitzungen: Datenspeicher: {e} ({})", e.einzelheiten());
        AppError::intern(format!("{e} Einzelheiten stehen im Protokoll."))
    })?;
    let offen = offen_fuer(&arbeiten, &von);
    Ok(Json(SitzungOffen { sitzung, von: Some(von), offen }))
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;
    use crate::app::{beispiel_fuer_tests, router_angemeldet};

    fn get(pfad: &str) -> Request<Body> {
        Request::get(pfad).header("host", "127.0.0.1:8787").body(Body::empty()).unwrap()
    }

    /// Wie `scripts\Api.ps1` mit `-Json`: POST mit der Kopfzeile der Sitzung (oder ohne).
    fn post(pfad: &str, sitzung: Option<&str>, koerper: &str) -> Request<Body> {
        let mut r = Request::post(pfad).header("host", "127.0.0.1:8787").header("content-type", "application/json");
        if let Some(s) = sitzung {
            r = r.header("X-Dashboard-Sitzung", s);
        }
        r.body(Body::from(koerper.to_string())).unwrap()
    }

    async fn schicke(state: &AppState, req: Request<Body>) -> (StatusCode, Value) {
        let res = router_angemeldet(state.clone()).oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
    }

    async fn offen_lesen(state: &AppState, sitzung: &str) -> Value {
        let (status, v) = schicke(state, get(&format!("/api/sitzungen/{sitzung}/offen"))).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        v
    }

    async fn neue_arbeit(state: &AppState, sitzung: Option<&str>, von: &str, stand: &str) -> String {
        let koerper = format!(r#"{{"projekt": "dashboard", "von": "{von}", "titel": "Arbeit von {von}", "stand": "{stand}"}}"#);
        let (status, v) = schicke(state, post("/api/arbeiten", sitzung, &koerper)).await;
        assert_eq!(status, StatusCode::CREATED, "{v}");
        v["id"].as_str().unwrap().to_string()
    }

    /// Unbekannte Sitzung: `von` null, nichts offen (der Herzschlag lässt durch).
    /// Ein Schreibaufruf mit Kopfzeile und `von` ordnet zu; ohne Kopfzeile,
    /// mit falscher Kopfzeile oder bei einem Fehler (422) nicht.
    #[tokio::test]
    async fn zuordnung_nur_bei_gelungenem_schreiben_mit_kopfzeile() {
        let state = AppState::fuer_tests(&beispiel_fuer_tests());
        let v = offen_lesen(&state, "s-unbekannt").await;
        assert_eq!((v["sitzung"].as_str(), v["von"].clone(), v["offen"].as_array().map(Vec::len)), (Some("s-unbekannt"), Value::Null, Some(0)));

        neue_arbeit(&state, None, "chat:dashboard", "laeuft").await;
        assert_eq!(offen_lesen(&state, "s1").await["von"], Value::Null, "ohne Kopfzeile keine Zuordnung");
        let (status, _) = schicke(&state, post("/api/arbeiten", Some("s1"), r#"{"projekt": "dashboard", "von": "chat:dashboard"}"#)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(offen_lesen(&state, "s1").await["von"], Value::Null, "ein abgewiesener Aufruf ordnet nicht zu");
        neue_arbeit(&state, Some("C:\\x"), "chat:dashboard", "laeuft").await;
        assert!(state.sitzungen.chat("C:\\x").is_none(), "falsche Form der Kennung wird nicht gemerkt");

        neue_arbeit(&state, Some("s1"), "chat:dashboard", "als-naechstes").await;
        let v = offen_lesen(&state, "s1").await;
        assert_eq!(v["von"].as_str(), Some("chat:dashboard"), "{v}");
        // Zwei laufende Arbeiten ohne Schritte gehören chat:dashboard; die als-naechstes hält nicht auf.
        let offen = v["offen"].as_array().unwrap();
        assert_eq!(offen.len(), 2, "{v}");
        assert!(offen.iter().all(|s| s["grund"] == "laeuft" && s["nr"].is_null()), "{v}");

        // Der letzte Stand gilt: dieselbe Sitzung schreibt nun als anderer Chat.
        neue_arbeit(&state, Some("s1"), "chat:architektur", "als-naechstes").await;
        let v = offen_lesen(&state, "s1").await;
        assert_eq!((v["von"].as_str(), v["offen"].as_array().map(Vec::len)), (Some("chat:architektur"), Some(0)), "{v}");
    }

    /// Ein wartender Schritt hält nicht auf; ein angebotener hält den, dem er
    /// angeboten ist, nicht den, der ihn anbietet.
    #[tokio::test]
    async fn wartende_halten_nicht_auf_angebotene_schon() {
        let state = AppState::fuer_tests(&beispiel_fuer_tests());
        let id = neue_arbeit(&state, Some("s-dash"), "chat:dashboard", "laeuft").await;
        let a = format!("/api/arbeiten/{id}");
        let (status, v) = schicke(&state, post(&format!("{a}/schritte"), None, r#"{"von": "chat:dashboard", "titel": "Abnahme"}"#)).await;
        assert_eq!(status, StatusCode::CREATED, "{v}");
        neue_arbeit(&state, Some("s-arch"), "chat:architektur", "als-naechstes").await;
        assert_eq!(offen_lesen(&state, "s-arch").await["offen"].as_array().map(Vec::len), Some(0));

        // Schritt 1 („bisherige Arbeit“) läuft bei chat:dashboard.
        let v = offen_lesen(&state, "s-dash").await;
        assert_eq!((v["offen"][0]["nr"].as_u64(), v["offen"][0]["grund"].as_str()), (Some(1), Some("laeuft")), "{v}");

        // Schritt 1 wartet: hält nicht mehr auf.
        let (status, v) =
            schicke(&state, post(&format!("{a}/schritte/1"), None, r#"{"von": "chat:dashboard", "stand": "wartet", "wartet_auf": "extern"}"#)).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(offen_lesen(&state, "s-dash").await["offen"].as_array().map(Vec::len), Some(0), "wartender Schritt");

        // Schritt 2 an chat:architektur angeboten: hält architektur, nicht dashboard.
        let (status, v) = schicke(&state, post(&format!("{a}/schritte/2"), None, r#"{"von": "chat:dashboard", "anbieten": "chat:architektur"}"#)).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let v = offen_lesen(&state, "s-arch").await;
        assert_eq!((v["offen"][0]["nr"].as_u64(), v["offen"][0]["grund"].as_str()), (Some(2), Some("angeboten")), "{v}");
        assert_eq!(offen_lesen(&state, "s-dash").await["offen"].as_array().map(Vec::len), Some(0));
    }

    #[tokio::test]
    async fn falsche_kennung_ist_422_und_der_weg_ist_geschuetzt() {
        let state = AppState::fuer_tests(&beispiel_fuer_tests());
        let (status, _) = schicke(&state, get("/api/sitzungen/a.b/offen")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(crate::sicherheit::geschuetzt("/api/sitzungen/s1/offen"));
        assert!(crate::sicherheit::geschuetzt("/API/Sitzungen/s1/offen"));
        // Im echten Router: ohne Anmeldung 401, aus fremder Herkunft 403 (auch angemeldet).
        let res = crate::app::router(state.clone()).oneshot(get("/api/sitzungen/s1/offen")).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        let fremd = Request::get("/api/sitzungen/s1/offen")
            .header("host", "127.0.0.1:8787")
            .header("origin", "https://boese.example")
            .header("sec-fetch-site", "cross-site")
            .body(Body::empty())
            .unwrap();
        assert_eq!(schicke(&state, fremd).await.0, StatusCode::FORBIDDEN);
        // Gegenprobe: derselbe Weg angemeldet und ohne fremde Herkunft gibt 200.
        assert_eq!(schicke(&state, get("/api/sitzungen/s1/offen")).await.0, StatusCode::OK);
    }

    /// Ohne Anmeldung ordnet auch ein freier POST-Weg, der 2xx gibt, nicht zu;
    /// sonst ließe sich eine fremde Sitzung umbiegen oder die Karte fluten.
    #[tokio::test]
    async fn ohne_anmeldung_wird_nichts_zugeordnet() {
        let state = AppState::fuer_tests(&beispiel_fuer_tests());
        let abmelden = |sitzung: &str| {
            Request::post("/api/passkey/abmelden")
                .header("host", "127.0.0.1:8787")
                .header("origin", "http://127.0.0.1:8787")
                .header("content-type", "application/json")
                .header(KOPF, sitzung)
                .body(Body::from(r#"{"von": "chat:x"}"#))
                .unwrap()
        };
        let res = crate::app::router(state.clone()).oneshot(abmelden("s-fremd")).await.unwrap();
        assert!(res.status().is_success(), "der Weg ist frei und gibt 2xx: {}", res.status());
        assert!(state.sitzungen.chat("s-fremd").is_none(), "ohne Anmeldung keine Zuordnung");
        // Gegenprobe: derselbe Aufruf als Inhaber ordnet zu.
        let (status, _) = schicke(&state, abmelden("s-fremd")).await;
        assert!(status.is_success(), "{status}");
        assert_eq!(state.sitzungen.chat("s-fremd").as_deref(), Some("chat:x"));
    }

    #[test]
    fn zuordnung_hat_eine_grenze() {
        let z = Zuordnung::default();
        for i in 0..=HOECHSTENS {
            z.merken(&format!("s{i}"), "chat:a");
        }
        assert!(z.chat("s0").is_none(), "die älteste fällt weg");
        assert_eq!(z.chat(&format!("s{HOECHSTENS}")).as_deref(), Some("chat:a"));
    }

    /// Ende zu Ende mit dem echten Server auf einem freien Port (nie 8787) und
    /// der Windows-Anmeldung: die Abfrage des Herzschlags bekommt die offene
    /// Arbeit, eine unbekannte Sitzung ist `Unbekannt`.
    #[cfg(windows)]
    #[tokio::test(flavor = "multi_thread")]
    async fn herzschlag_fragt_den_echten_server_mit_anmeldung() {
        use crate::herzschlag::abfrage;
        use crate::herzschlag::graph::Arbeit as HArbeit;
        let port = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap().local_addr().unwrap().port();
        let (state, abschalter) = AppState::fuer_tests_mit_abschalter(&beispiel_fuer_tests(), port);
        let anlegen = Request::post("/api/arbeiten")
            .header("host", format!("127.0.0.1:{port}"))
            .header("content-type", "application/json")
            .header(KOPF, "s-e2e")
            .body(Body::from(r#"{"projekt": "dashboard", "von": "chat:dashboard", "titel": "Bauen", "stand": "laeuft"}"#))
            .unwrap();
        assert_eq!(schicke(&state, anlegen).await.0, StatusCode::CREATED);
        let (signal_tx, signal_rx) = tokio::sync::oneshot::channel::<()>();
        let (bereit_tx, mut bereit_rx) = tokio::sync::watch::channel(false);
        let bereit = move || {
            bereit_tx.send_replace(true);
        };
        let signal = async move {
            let _ = signal_rx.await;
            Duration::from_secs(2)
        };
        let server = crate::dienst::betreiben(port, move || Ok((state, abschalter)), &bereit, signal, "Test");
        let pruefung = async move {
            let _ = bereit_rx.wait_for(|&b| b).await;
            let ziel = format!("127.0.0.1:{port}");
            let (offen, unbekannt) =
                tokio::task::spawn_blocking(move || (abfrage::offen(&ziel, "s-e2e", abfrage::FRIST), abfrage::offen(&ziel, "s-fremd", abfrage::FRIST)))
                    .await
                    .unwrap();
            signal_tx.send(()).unwrap();
            (offen, unbekannt)
        };
        let (ergebnis, (offen, unbekannt)) =
            tokio::time::timeout(Duration::from_secs(30), async { tokio::join!(server, pruefung) }).await.expect("Server und Prüfung enden");
        assert!(ergebnis.is_ok(), "{ergebnis:?}");
        assert!(matches!(&offen, Ok(HArbeit::Offen(t)) if t.contains("laeuft")), "{offen:?}");
        assert_eq!(unbekannt, Ok(HArbeit::Unbekannt));
    }
}
