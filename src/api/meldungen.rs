//! `/api/meldungen`: Meldungen für den Inhaber (Etappe A3, Konzept „Anstehendes“).
//!
//! - `POST /api/meldungen {projekt, art, titel, text?, verweis?, schluessel?, von?}`:
//!   Ein Chat meldet für sein Projekt „fertig“ oder „info“ (per curl; aus dem
//!   Browser 422). Mit `verweis` (Kennung einer Arbeit, Aufgabe oder Frage)
//!   ist der Schlüssel `<projekt>/<art>:<verweis>` und zählt nur einmal: Eine
//!   Arbeit auf „fertig“ hat dieselbe Meldung schon angelegt. Ohne Verweis
//!   fasst `schluessel` (sonst der Titel) Wiederholungen zusammen; eine
//!   Wiederholung zählt hoch. 201 bei einer neuen Meldung, 200 bei einer
//!   Wiederholung, je mit `id`, `neu` und `anzahl`. `fehler` und `frist`
//!   erzeugt nur der Server selbst (`crate::meldungen`).
//! - `GET /api/meldungen[?projekt=<slug>]`: alle außerhalb des Archivs,
//!   neueste zuerst (auch gelesene).
//! - `POST /api/meldungen/<id>/gelesen`: Der Inhaber klickt „gelesen“, nur aus dem
//!   Browser (Kopfzeilen `Origin` oder `Sec-Fetch-Site`; per curl 422). Das ist
//!   eine Prüfung von Kopfzeilen, keine Sicherheitsgrenze (wie beim Haken).
//!
//! Geschützt wie `/api/arbeiten` (Windows-Anmeldung, `sicherheit::geschuetzt`)
//! und ohne Herkunftskopf für curl erlaubt (`sicherheit::herkunft_pflicht`);
//! fremde Seiten bleiben 403. Schreiben nur mit `Content-Type: application/json`.
//! Jede Änderung sendet `change` mit Bereich `anstehend`.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use serde::Deserialize;

use super::arbeiten::NOTIZ_HOECHSTENS;
use super::vertrag::{MeldungAngelegt, MeldungGelesen, MeldungenListe, MeldungsArt};
use super::vorgaenge::{TITEL_HOECHSTENS, abfrage, angegeben, aus_browser, herkunft_pruefen, json_kopf_pruefen, slug_pruefen, text_pruefen};
use crate::app::AppState;
use crate::fehler::AppError;
use crate::speicher::aufgaben;
use crate::speicher::meldungen::{Gelesen, Melden, NeueMeldung, Wiederholung, schluessel};
use crate::speicher::{Speicher, SpeicherFehler};
use crate::vertrag::daten::ist_kennung;

pub const SATZ_NUR_CHATS: &str = "Meldungen schicken nur die Chats (per curl); der Inhaber klickt sie im Browser weg.";
pub const SATZ_GELESEN_NUR_BROWSER: &str = "„gelesen“ klickt nur der Inhaber im Browser.";
pub const SATZ_ART: &str = "„art“ ist fertig oder info. Fehler und Fristen meldet der Server selbst.";
const SATZ_NICHT_DA: &str = "Diese Meldung gibt es nicht.";
const MUSTER: &str = r#"{"projekt": "dashboard", "art": "fertig", "titel": "…", "text": "…", "verweis": "ar-20261030-0001", "von": "chat:dashboard"} (Pflicht: projekt, art, titel; art fertig oder info)"#;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NeueMeldungKoerper {
    projekt: String,
    art: String,
    titel: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    verweis: Option<String>,
    #[serde(default)]
    schluessel: Option<String>,
    #[serde(default)]
    von: Option<String>,
}

fn meldungsfehler(e: SpeicherFehler) -> AppError {
    match e {
        SpeicherFehler::NichtGefunden(_) => AppError::nicht_gefunden(SATZ_NICHT_DA),
        e => {
            tracing::error!("Meldungen: Datenspeicher: {e} ({})", e.einzelheiten());
            AppError::intern(format!("{e} Einzelheiten stehen im Protokoll."))
        }
    }
}

/// `POST /api/meldungen`: ein Chat meldet „fertig“ oder „info“.
pub async fn anlegen(State(state): State<AppState>, kopf: HeaderMap, koerper: Bytes) -> Result<(StatusCode, Json<MeldungAngelegt>), AppError> {
    json_kopf_pruefen(&kopf)?;
    let k: NeueMeldungKoerper = serde_json::from_slice(&koerper).map_err(|e| {
        tracing::debug!("Meldungen: Körper unlesbar ({:?}, Zeile {}, Spalte {})", e.classify(), e.line(), e.column());
        AppError::unlesbar(format!("Der Inhalt ist kein passendes JSON. Erwartet wird etwa {MUSTER}."))
    })?;
    if aus_browser(&kopf) {
        return Err(AppError::unlesbar(SATZ_NUR_CHATS));
    }
    slug_pruefen(&k.projekt)?;
    let art = match MeldungsArt::aus_wort(k.art.trim()) {
        Some(a @ (MeldungsArt::Fertig | MeldungsArt::Info)) => a,
        _ => return Err(AppError::unlesbar(SATZ_ART)),
    };
    let titel = k.titel.trim().to_string();
    if titel.is_empty() {
        return Err(AppError::unlesbar("Der Titel fehlt."));
    }
    text_pruefen("titel", &titel, TITEL_HOECHSTENS, false)?;
    let text = angegeben(k.text);
    if let Some(t) = &text {
        text_pruefen("text", t, NOTIZ_HOECHSTENS, true)?;
    }
    let verweis = angegeben(k.verweis);
    if verweis.as_deref().is_some_and(|v| !ist_kennung(v)) {
        return Err(AppError::unlesbar("„verweis“ ist die Kennung einer Arbeit, Aufgabe oder Frage, etwa ar-20261030-0001."));
    }
    let eigener = angegeben(k.schluessel);
    if eigener.as_deref().is_some_and(|s| !ist_kennung(s)) {
        return Err(AppError::unlesbar("„schluessel“ ist ein kurzes Wort aus a–z, 0–9 und Bindestrich, etwa bericht-fertig."));
    }
    let von = angegeben(k.von).unwrap_or_else(|| format!("chat:{}", k.projekt));
    herkunft_pruefen(&von, false)?;
    // Mit Verweis: dieselbe Meldung wie „Arbeit auf fertig“ (einmal). Sonst zählt eine Wiederholung hoch.
    let (was, wiederholung) = match (&verweis, &eigener) {
        (Some(v), _) => (format!("{}:{v}", art.wort()), Wiederholung::Einmal),
        (None, Some(s)) => (format!("{}:{s}", art.wort()), Wiederholung::Zaehlen),
        (None, None) => (format!("{}:{}", art.wort(), titel.to_lowercase()), Wiederholung::Zaehlen),
    };
    let neu = NeueMeldung { schluessel: schluessel(&k.projekt, &was), projekt: k.projekt, art, titel, text, verweis, system: false, geschuetzt: false, von };
    let jetzt = aufgaben::jetzt();
    let ergebnis = Speicher::im_hintergrund(state.speicher()?, move |s| s.meldung_melden(neu, wiederholung, &jetzt)).await.map_err(meldungsfehler)?;
    let m = ergebnis.meldung();
    tracing::info!("Meldung {} für {} vom Chat ({}, ×{})", m.id, m.projekt, m.art.wort(), m.anzahl);
    if ergebnis.geschrieben() {
        super::events::anstehend_geaendert(&state.events, Some(&m.projekt));
    }
    let status = if matches!(ergebnis, Melden::Neu(_)) { StatusCode::CREATED } else { StatusCode::OK };
    Ok((status, Json(MeldungAngelegt { id: m.id.clone(), neu: status == StatusCode::CREATED, anzahl: m.anzahl })))
}

/// `GET /api/meldungen[?projekt=<slug>]`.
pub async fn liste(State(state): State<AppState>, uri: Uri) -> Result<Json<MeldungenListe>, AppError> {
    let projekt = abfrage(&uri, "projekt");
    if let Some(p) = &projekt {
        slug_pruefen(p)?;
    }
    // Entscheidung archiv-status: ohne `projekt` fehlen archivierte Projekte (Glocke).
    let ohne = if projekt.is_none() { state.archivierte() } else { Default::default() };
    let mut meldungen = Speicher::im_hintergrund(state.speicher()?, move |s| s.meldungen_lesen(projekt.as_deref())).await.map_err(meldungsfehler)?;
    meldungen.retain(|m| !ohne.contains(&m.projekt));
    // Titel und Text ohne Profil und Benutzernamen (`datenschutz`; die PC-Seite
    // zeigt hier die Systemmeldungen, oft mit einem Pfad aus einem Lesefehler).
    for m in &mut meldungen {
        m.titel = crate::datenschutz::verbergen(&m.titel);
        m.text = m.text.as_deref().map(crate::datenschutz::verbergen);
    }
    Ok(Json(MeldungenListe { meldungen }))
}

/// `POST /api/meldungen/<id>/gelesen`: nur aus dem Browser.
pub async fn gelesen(State(state): State<AppState>, Path(id): Path<String>, kopf: HeaderMap, koerper: Bytes) -> Result<Json<MeldungGelesen>, AppError> {
    if !koerper.iter().all(u8::is_ascii_whitespace) {
        json_kopf_pruefen(&kopf)?;
    }
    if !aus_browser(&kopf) {
        return Err(AppError::unlesbar(SATZ_GELESEN_NUR_BROWSER));
    }
    if !ist_kennung(&id) {
        return Err(AppError::nicht_gefunden(SATZ_NICHT_DA));
    }
    let jetzt = aufgaben::jetzt();
    let ergebnis = Speicher::im_hintergrund(state.speicher()?, move |s| s.meldung_gelesen(&id, &jetzt)).await.map_err(meldungsfehler)?;
    Ok(Json(match ergebnis {
        Gelesen::Geaendert(meldung) => {
            tracing::info!("Meldung {} gelesen", meldung.id);
            super::events::anstehend_geaendert(&state.events, Some(&meldung.projekt));
            MeldungGelesen { meldung, geaendert: true }
        }
        Gelesen::Unveraendert(meldung) => MeldungGelesen { meldung, geaendert: false },
    }))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;
    use crate::app::router_angemeldet as router;

    fn zustand() -> AppState {
        AppState::fuer_tests(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel"))
    }

    fn get(pfad: &str) -> Request<Body> {
        Request::get(pfad).header("host", "127.0.0.1:8787").body(Body::empty()).unwrap()
    }

    fn post(pfad: &str, koerper: &str) -> Request<Body> {
        Request::post(pfad).header("host", "127.0.0.1:8787").header("content-type", "application/json").body(Body::from(koerper.to_string())).unwrap()
    }

    fn post_browser(pfad: &str) -> Request<Body> {
        Request::post(pfad)
            .header("host", "localhost:8787")
            .header("origin", "http://localhost:8787")
            .header("sec-fetch-site", "same-origin")
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap()
    }

    async fn schicke_mit(app: axum::Router, req: Request<Body>) -> (StatusCode, Value) {
        let res = app.oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        (status, serde_json::from_str(&text).unwrap_or_else(|e| panic!("kein JSON ({e}): {text}")))
    }

    async fn schicke(state: &AppState, req: Request<Body>) -> (StatusCode, Value) {
        schicke_mit(router(state.clone()), req).await
    }

    /// Ein Chat meldet fertig und info per curl; Wiederholung zählt hoch
    /// (200 statt 201); mit Verweis auf eine fertige Arbeit dieselbe Meldung.
    #[tokio::test]
    async fn chat_meldet_und_wiederholung_zaehlt() {
        let state = zustand();
        let mut live = state.events.subscribe();
        let (status, v) = schicke(&state, post("/api/meldungen", r#"{"projekt": "dashboard", "art": "info", "titel": "Bericht liegt bereit"}"#)).await;
        assert_eq!((status, v["neu"].as_bool(), v["anzahl"].as_u64()), (StatusCode::CREATED, Some(true), Some(1)), "{v}");
        let id = v["id"].as_str().unwrap().to_string();
        let (status, v) = schicke(&state, post("/api/meldungen", r#"{"projekt": "dashboard", "art": "info", "titel": "Bericht liegt bereit"}"#)).await;
        assert_eq!((status, v["id"].as_str(), v["neu"].as_bool(), v["anzahl"].as_u64()), (StatusCode::OK, Some(id.as_str()), Some(false), Some(2)));
        let pfade: Vec<String> = std::iter::from_fn(|| live.try_recv().ok()).filter(|c| c.area == "anstehend").map(|c| c.path).collect();
        assert_eq!(pfade, ["anstehend/dashboard", "anstehend/dashboard"]);
        // Arbeit auf fertig: Meldung fertig; der Chat meldet dazu fertig mit Verweis: dieselbe.
        let (_, a) =
            schicke(&state, post("/api/arbeiten", r#"{"projekt": "dashboard", "von": "chat:dashboard", "titel": "Server bauen", "stand": "laeuft"}"#)).await;
        let ar = a["id"].as_str().unwrap().to_string();
        let (status, _) = schicke(&state, post(&format!("/api/arbeiten/{ar}"), r#"{"von": "chat:dashboard", "stand": "fertig"}"#)).await;
        assert_eq!(status, StatusCode::OK);
        let (status, v) = schicke(
            &state,
            post("/api/meldungen", &format!(r#"{{"projekt": "dashboard", "art": "fertig", "titel": "Server ist gebaut", "verweis": "{ar}"}}"#)),
        )
        .await;
        assert_eq!((status, v["neu"].as_bool(), v["anzahl"].as_u64()), (StatusCode::OK, Some(false), Some(1)), "{v}");
        let (_, l) = schicke(&state, get("/api/meldungen?projekt=dashboard")).await;
        let liste = l["meldungen"].as_array().unwrap();
        assert_eq!(liste.len(), 2, "{l}");
        let fertig = liste.iter().find(|m| m["art"] == "fertig").unwrap();
        assert_eq!((fertig["schluessel"].as_str(), fertig["titel"].as_str()), (Some(format!("dashboard/fertig:{ar}").as_str()), Some("Fertig: Server bauen")));
        // In „Anstehend“: als Meldung mit fester Stufe (fertig Bald, info Irgendwann), Ziel mit Anker.
        let (_, an) = schicke(&state, get("/api/anstehend?art=meldung")).await;
        let e = an["eintraege"].as_array().unwrap();
        assert_eq!(e.len(), 2);
        let f = e.iter().find(|x| x["meldung"]["art"] == "fertig").unwrap();
        assert_eq!((f["stufe"]["wirksam"].as_str(), f["stufe"]["grund"].as_str()), (Some("bald"), Some("fest")));
        assert_eq!(f["ziel"], format!("/projects/dashboard#meldung-{}", f["id"].as_str().unwrap()));
        let i = e.iter().find(|x| x["meldung"]["art"] == "info").unwrap();
        assert_eq!((i["stufe"]["wirksam"].as_str(), i["meldung"]["anzahl"].as_u64()), (Some("irgendwann"), Some(2)));
        assert!(e.iter().all(|x| x["wartet_auf"].is_null()), "Meldungen warten nicht auf den Inhaber");
        let (_, nur) = schicke(&state, get("/api/anstehend?nur_fuer_mich=1")).await;
        assert!(nur["eintraege"].as_array().unwrap().iter().all(|x| x["art"] != "meldung"));
    }

    /// Falsches: Fehler und Frist nur vom Server, aus dem Browser 422, kein
    /// JSON 415, Pfad-Tricks im Projekt 422.
    #[tokio::test]
    async fn anlegen_prueft() {
        let state = zustand();
        for (koerper, soll) in [
            (r#"{"projekt": "dashboard", "art": "fehler", "titel": "x"}"#, StatusCode::UNPROCESSABLE_ENTITY),
            (r#"{"projekt": "dashboard", "art": "frist", "titel": "x"}"#, StatusCode::UNPROCESSABLE_ENTITY),
            (r#"{"projekt": "..\\x", "art": "info", "titel": "x"}"#, StatusCode::UNPROCESSABLE_ENTITY),
            (r#"{"projekt": "dashboard", "art": "info", "titel": " "}"#, StatusCode::UNPROCESSABLE_ENTITY),
            (r#"{"projekt": "dashboard", "art": "info", "titel": "x", "verweis": "a/b"}"#, StatusCode::UNPROCESSABLE_ENTITY),
            (r#"{"projekt": "dashboard", "art": "info", "titel": "x", "von": "browser"}"#, StatusCode::UNPROCESSABLE_ENTITY),
            (r#"{"projekt": "dashboard", "art": "info", "titel": "x", "unbekannt": 1}"#, StatusCode::UNPROCESSABLE_ENTITY),
        ] {
            assert_eq!(schicke(&state, post("/api/meldungen", koerper)).await.0, soll, "{koerper}");
        }
        let mut b = post_browser("/api/meldungen");
        *b.body_mut() = Body::from(r#"{"projekt": "dashboard", "art": "info", "titel": "x"}"#);
        let (status, v) = schicke(&state, b).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(SATZ_NUR_CHATS)));
        let ohne_json = Request::post("/api/meldungen").header("host", "127.0.0.1:8787").body(Body::from("x")).unwrap();
        assert_eq!(schicke(&state, ohne_json).await.0, StatusCode::UNSUPPORTED_MEDIA_TYPE);
        assert!(state.speicher().unwrap().meldungen_lesen(None).unwrap().is_empty(), "nichts gespeichert");
    }

    /// „gelesen“ nur aus dem Browser; per curl 422, nichts geändert; danach
    /// fällt die Meldung aus „Anstehend“, steht aber noch in der Liste.
    #[tokio::test]
    async fn gelesen_nur_im_browser() {
        let state = zustand();
        let (_, v) = schicke(&state, post("/api/meldungen", r#"{"projekt": "dashboard", "art": "info", "titel": "Zur Kenntnis"}"#)).await;
        let id = v["id"].as_str().unwrap().to_string();
        let (status, v) = schicke(&state, post(&format!("/api/meldungen/{id}/gelesen"), "")).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(SATZ_GELESEN_NUR_BROWSER)));
        assert!(state.speicher().unwrap().meldung(&id).unwrap().unwrap().gelesen.is_none());
        let (status, v) = schicke(&state, post_browser(&format!("/api/meldungen/{id}/gelesen"))).await;
        assert_eq!((status, v["geaendert"].as_bool()), (StatusCode::OK, Some(true)), "{v}");
        let (_, v) = schicke(&state, post_browser(&format!("/api/meldungen/{id}/gelesen"))).await;
        assert_eq!(v["geaendert"].as_bool(), Some(false));
        let (_, an) = schicke(&state, get("/api/anstehend")).await;
        assert!(an["eintraege"].as_array().unwrap().iter().all(|e| e["art"] != "meldung"));
        let (_, l) = schicke(&state, get("/api/meldungen")).await;
        assert!(l["meldungen"][0]["gelesen"].is_string());
        assert_eq!(schicke(&state, post_browser("/api/meldungen/m-19990101-0001/gelesen")).await.0, StatusCode::NOT_FOUND);
        // Hotfix Prozent: `%` im Pfad unter /api ist schon an der Schleuse 400.
        assert_eq!(schicke(&state, post_browser("/api/meldungen/..%5Cx/gelesen")).await.0, StatusCode::BAD_REQUEST);
        // Aktualisierungen: über /api/anstehend?projekt= (geschützt), gelesen vermerkt.
        let (_, an) = schicke(&state, get("/api/anstehend?projekt=dashboard")).await;
        let a = &an["aktualisierungen"][0];
        assert_eq!((a["quelle"].as_str(), a["id"].as_str(), a["gelesen"].as_bool()), (Some("meldung"), Some(id.as_str()), Some(true)));
    }

    /// Geschützt wie die Arbeiten (401 ohne Anmeldung); fremde Seite 403.
    #[tokio::test]
    async fn geschuetzt_und_fremde_seite() {
        let state = zustand();
        for req in [
            get("/api/meldungen"),
            post("/api/meldungen", r#"{"projekt": "dashboard", "art": "info", "titel": "x"}"#),
            post_browser("/api/meldungen/m-1/gelesen"),
        ] {
            let pfad = req.uri().to_string();
            let res = crate::app::router(state.clone()).oneshot(req).await.unwrap();
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{pfad}");
        }
        let fremd = Request::post("/api/meldungen/m-1/gelesen")
            .header("host", "localhost:8787")
            .header("origin", "https://fremd.invalid")
            .header("sec-fetch-site", "cross-site")
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap();
        assert_eq!(router(state.clone()).oneshot(fremd).await.unwrap().status(), StatusCode::FORBIDDEN);
    }

    /// Einstufen einer Meldung geht nicht (feste Stufe).
    #[tokio::test]
    async fn meldung_laesst_sich_nicht_umstufen() {
        let state = zustand();
        let (_, v) = schicke(&state, post("/api/meldungen", r#"{"projekt": "dashboard", "art": "info", "titel": "x"}"#)).await;
        let id = v["id"].as_str().unwrap();
        let mut b = post_browser("/api/einstufung");
        *b.body_mut() = Body::from(format!(r#"{{"projekt": "dashboard", "art": "meldung", "id": "{id}", "stufe": "jetzt"}}"#));
        let (status, v) = schicke(&state, b).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    }

    /// Grenze aus 7b: /stand ohne Anmeldung nennt Meldungen nur als Zahlen
    /// (neu, gesamt), ohne Titel, Texte und Dateipfade; mit Anmeldung alles.
    #[tokio::test]
    async fn stand_ohne_anmeldung_nur_zahlen() {
        let state = zustand();
        let jetzt = crate::speicher::aufgaben::jetzt();
        let (status, _) = schicke(
            &state,
            post("/api/meldungen", r#"{"projekt": "dashboard", "art": "info", "titel": "Vertraulicher Chat-Satz", "text": "Einzelheit aus dem Chat"}"#),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        let slugs = vec!["dashboard".to_string()];
        crate::meldungen::bau_ergebnis(&state, &slugs, false, "npm run build: Fehler in geheim.svelte", &[], jetzt).await;
        let befunde = vec![crate::inhalt::pruefung::Befund::fehler("projects/dashboard/daten/geheim.toml", Some(3), "Wert fehlt.")];
        let p = crate::meldungen::pruefung_meldung("dashboard", &befunde).unwrap();
        crate::meldungen::melden(&state, p, crate::speicher::meldungen::Wiederholung::Zustand, jetzt).await;
        let erste = state.speicher().unwrap().meldungen_lesen(Some("dashboard")).unwrap();
        assert_eq!(erste.len(), 3);
        state.speicher().unwrap().meldung_gelesen(&erste[0].id, &jetzt).unwrap();
        // Ohne Anmeldung: nur Zahlen.
        let res = crate::app::router(state.clone()).oneshot(get("/api/projekte/dashboard/stand")).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let text = String::from_utf8_lossy(&res.into_body().collect().await.unwrap().to_bytes()).into_owned();
        for geheim in ["Vertraulicher", "Einzelheit", "geheim", "Bau der Oberfläche", "Prüfung von", "meldung-m-"] {
            assert!(!text.contains(geheim), "„{geheim}“ steht ohne Anmeldung in /stand: {text}");
        }
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["aktualisierungen"], serde_json::json!([]));
        assert_eq!(v["zahlen"]["meldungen"], serde_json::json!({"neu": 2, "gesamt": 3}));
        assert_eq!(
            (v["gesamt"]["ampel"].as_str(), v["gesamt"]["grund"].as_str(), v["gesamt"]["bezug"].clone()),
            (Some("rot"), Some("fehler-offen"), Value::Null)
        );
        // Mit Anmeldung: Titel und Texte, keine Zahlen (die zählt die Oberfläche).
        let (_, v) = schicke(&state, get("/api/projekte/dashboard/stand")).await;
        assert_eq!(v["zahlen"], Value::Null);
        let meldungen: Vec<&Value> = v["aktualisierungen"].as_array().unwrap().iter().filter(|a| a["quelle"] == "meldung").collect();
        assert_eq!((meldungen.len(), meldungen.iter().filter(|a| a["gelesen"] == false).count()), (3, 2));
        let titel: Vec<&str> = v["aktualisierungen"].as_array().unwrap().iter().filter_map(|a| a["titel"].as_str()).collect();
        assert!(titel.contains(&"Vertraulicher Chat-Satz"), "{titel:?}");
    }

    /// Einmal ohne Wiederkehr: Ist die Meldung „fertig“ gelesen und im Archiv,
    /// bringt eine Notiz an der fertigen Arbeit sie nicht zurück; erst ein
    /// neuer Wechsel auf fertig (wieder geöffnet, wieder fertig) meldet sich.
    #[tokio::test]
    async fn fertig_kommt_nach_dem_archiv_nicht_wieder() {
        let state = zustand();
        let s = state.speicher().unwrap();
        let (_, a) =
            schicke(&state, post("/api/arbeiten", r#"{"projekt": "dashboard", "von": "chat:dashboard", "titel": "Server bauen", "stand": "laeuft"}"#)).await;
        let ar = a["id"].as_str().unwrap().to_string();
        schicke(&state, post(&format!("/api/arbeiten/{ar}"), r#"{"von": "chat:dashboard", "stand": "fertig"}"#)).await;
        let m = s.meldungen_lesen(Some("dashboard")).unwrap();
        assert_eq!(m.len(), 1);
        let spaeter = crate::speicher::aufgaben::jetzt() + chrono::Duration::days(8);
        s.meldung_gelesen(&m[0].id, &crate::speicher::aufgaben::jetzt()).unwrap();
        assert_eq!(s.meldungen_archivieren(&spaeter).unwrap().len(), 1);
        let (status, _) = schicke(&state, post(&format!("/api/arbeiten/{ar}"), r#"{"von": "chat:dashboard", "notiz": "Nachtrag"}"#)).await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) =
            schicke(&state, post(&format!("/api/arbeiten/{ar}"), r#"{"von": "chat:dashboard", "stand": "fertig", "notiz": "Noch ein Nachtrag"}"#)).await;
        assert_eq!(status, StatusCode::OK);
        assert!(s.meldungen_lesen(Some("dashboard")).unwrap().is_empty(), "keine neue Meldung „fertig“");
        schicke(&state, post(&format!("/api/arbeiten/{ar}"), r#"{"von": "chat:dashboard", "stand": "laeuft"}"#)).await;
        schicke(&state, post(&format!("/api/arbeiten/{ar}"), r#"{"von": "chat:dashboard", "stand": "fertig"}"#)).await;
        assert_eq!(s.meldungen_lesen(Some("dashboard")).unwrap().len(), 1, "neuer Wechsel auf fertig");
    }
}
