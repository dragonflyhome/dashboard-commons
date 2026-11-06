//! Tests zur Entscheidung archiv-status (`[form].tests`): ein
//! Projekt mit `status = "archiviert"` ist überall still und steht nur in der
//! Archivansicht und auf seiner eigenen Seite; nahe Fristen erscheinen
//! neutral in „Anstehend“; zurück auf `aktiv` ist alles wieder da. Dazu: Einträge
//! im Datenspeicher, deren Projektordner fehlt (Übergang per `git mv`), bringen
//! den Dienst nicht aus dem Tritt.
//!
//! Alle Projekte, Titel und Fristen sind erfunden.

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use crate::app::{AppState, router_angemeldet};
use crate::speicher::aufgaben::jetzt;
use crate::vertrag::abgeleitet::heute_von;

/// Titel des archivierten Projekts; darf nirgends außer auf seiner Seite und im Archiv stehen.
const TITEL: &str = "Umzug abgeschlossen";

/// Ein eigener Inhaltsordner mit zwei Projekten: `laufend` (aktiv) und
/// `ruhig` (archiviert, mit einer offenen Frage ohne Frist). Verschwindet mit dem Wächter.
struct Probe {
    wurzel: PathBuf,
}

impl Drop for Probe {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.wurzel);
    }
}

impl Probe {
    fn neu(name: &str) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let wurzel = std::env::temp_dir().join(format!("dashboard-test-{}-archiv-{name}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&wurzel);
        let laufend = wurzel.join("projects").join("laufend");
        std::fs::create_dir_all(&laufend).unwrap();
        std::fs::write(laufend.join("projekt.toml"), "schema = 3\ntitle = \"Laufendes Vorhaben\"\nstatus = \"aktiv\"\nsummary = \"S\"\nbackup = \"grün\"\n")
            .unwrap();
        let ruhig = wurzel.join("projects").join("ruhig");
        std::fs::create_dir_all(ruhig.join("daten")).unwrap();
        std::fs::write(
            ruhig.join("daten").join("fragen.toml"),
            "[[frage]]\nid = \"stille-frage\"\ntext = \"Soll das so bleiben?\"\nart = \"ja-nein\"\ngestellt = \"2026-10-30\"\nprioritaet = \"jetzt\"\n",
        )
        .unwrap();
        let p = Self { wurzel };
        p.status("archiviert");
        p
    }

    fn ruhig(&self) -> PathBuf {
        self.wurzel.join("projects").join("ruhig")
    }

    /// Setzt den Status von `ruhig` (wie der Projekt-Chat).
    fn status(&self, wort: &str) {
        std::fs::write(
            self.ruhig().join("projekt.toml"),
            format!("schema = 3\ntitle = \"{TITEL}\"\nstatus = \"{wort}\"\nsummary = \"S\"\nbackup = \"gelb\"\n"),
        )
        .unwrap();
    }

    fn zustand(&self) -> AppState {
        AppState::fuer_tests(&self.wurzel)
    }
}

fn get(pfad: &str) -> Request<Body> {
    Request::get(pfad).header("host", "127.0.0.1:8787").body(Body::empty()).unwrap()
}

fn post(pfad: &str, koerper: &str) -> Request<Body> {
    Request::post(pfad).header("host", "127.0.0.1:8787").header("content-type", "application/json").body(Body::from(koerper.to_string())).unwrap()
}

async fn roh(state: &AppState, req: Request<Body>) -> (StatusCode, String) {
    let res = router_angemeldet(state.clone()).oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

/// GET mit 200 als JSON, dazu der rohe Text (zum Suchen nach Titel und Slug).
async fn lies(state: &AppState, pfad: &str) -> (Value, String) {
    let (status, text) = roh(state, get(pfad)).await;
    assert_eq!(status, StatusCode::OK, "{pfad}: {text}");
    (serde_json::from_str(&text).unwrap_or_else(|e| panic!("{pfad}: kein JSON ({e}): {text}")), text)
}

async fn angelegt(state: &AppState, pfad: &str, koerper: &str) -> String {
    let (status, text) = roh(state, post(pfad, koerper)).await;
    assert_eq!(status, StatusCode::CREATED, "{pfad}: {text}");
    serde_json::from_str::<Value>(&text).unwrap()["id"].as_str().unwrap().to_string()
}

fn projekte_von(v: &Value, feld: &str) -> Vec<String> {
    v[feld].as_array().unwrap().iter().map(|e| e["projekt"].as_str().unwrap().to_string()).collect()
}

fn slugs(v: &Value) -> Vec<String> {
    v["projects"].as_array().unwrap().iter().map(|p| p["slug"].as_str().unwrap().to_string()).collect()
}

/// Ein Datum `tage` Tage nach heute (lokal), JJJJ-MM-TT.
fn in_tagen(tage: i64) -> String {
    (heute_von(&jetzt()) + chrono::Duration::days(tage)).format("%Y-%m-%d").to_string()
}

/// Legt für `projekt` je eine Aufgabe, eine Arbeit (ohne Frist) und eine Meldung an.
async fn eintraege_fuer(state: &AppState, projekt: &str) -> (String, String, String) {
    let a = angelegt(state, "/api/aufgaben", &format!(r#"{{"projekt": "{projekt}", "titel": "Umzugskisten zählen"}}"#)).await;
    let r = angelegt(
        state,
        "/api/arbeiten",
        &format!(r#"{{"projekt": "{projekt}", "von": "chat:{projekt}", "titel": "Kisten auspacken", "stand": "wartet", "wartet_auf": "inhaber"}}"#),
    )
    .await;
    let m = angelegt(state, "/api/meldungen", &format!(r#"{{"projekt": "{projekt}", "art": "info", "titel": "Nachsendeauftrag bestätigt"}}"#)).await;
    (a, r, m)
}

/// Test 1 der Entscheidung: Das archivierte Projekt fehlt in `/api/projects`
/// (Standardliste), Anstehend, Zählern, Glocke und Live-Kanal; es erscheint in
/// der Archivansicht.
#[tokio::test]
async fn archiviertes_projekt_fehlt_ueberall_und_steht_im_archiv() {
    let p = Probe::neu("fehlt");
    let state = p.zustand();
    let (a, r, m) = eintraege_fuer(&state, "ruhig").await;
    let (la, _, _) = eintraege_fuer(&state, "laufend").await;

    // Projektliste (Seitenleiste, Schublade, Startseite) und Cockpit.
    let (v, text) = lies(&state, "/api/projects").await;
    assert_eq!(slugs(&v), ["laufend"]);
    assert!(v["broken"].as_array().unwrap().is_empty(), "{v}");
    assert!(!text.contains(TITEL) && !text.contains("ruhig"), "{text}");
    let (status, text) = roh(&state, get("/api/pc")).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    assert!(text.contains("Laufendes Vorhaben") && !text.contains(TITEL), "Cockpit: Backup je Projekt ohne das Archiv: {text}");

    // Archivansicht: Titel und seit wann.
    let (v, _) = lies(&state, "/api/archiv").await;
    let archiv = v["projekte"].as_array().unwrap();
    assert_eq!(archiv.len(), 1, "{v}");
    assert_eq!((archiv[0]["slug"].as_str(), archiv[0]["titel"].as_str()), (Some("ruhig"), Some(TITEL)));
    assert_eq!(archiv[0]["seit"].as_str().map(str::len), Some(10), "JJJJ-MM-TT: {v}");

    // Anstehend: nichts von ruhig (Frage, Aufgabe, Arbeit, Meldung), auch kein Titel.
    let (v, text) = lies(&state, "/api/anstehend").await;
    assert!(!projekte_von(&v, "eintraege").contains(&"ruhig".to_string()), "{v}");
    assert!(projekte_von(&v, "eintraege").contains(&"laufend".to_string()), "{v}");
    assert!(!text.contains(TITEL) && !text.contains("Soll das so bleiben"), "{text}");
    assert_eq!(v["archiv_fristen"], serde_json::json!([]));

    // Zähler: nur die Aufgabe von laufend; Listen ohne Projekt ohne ruhig.
    let (v, _) = lies(&state, "/api/aufgaben/anzahl").await;
    assert_eq!(v["offen"], 1);
    let (v, _) = lies(&state, "/api/aufgaben?offen=1").await;
    let ids: Vec<&str> = v["aufgaben"].as_array().unwrap().iter().map(|x| x["id"].as_str().unwrap()).collect();
    assert_eq!(ids, [la.as_str()]);
    let (v, _) = lies(&state, "/api/arbeiten").await;
    assert_eq!(projekte_von(&v, "arbeiten"), ["laufend"]);
    // Glocke: Meldungen nur ohne ruhig.
    let (v, _) = lies(&state, "/api/meldungen").await;
    assert_eq!(projekte_von(&v, "meldungen"), ["laufend"]);

    // Auf der eigenen Seite (mit `projekt`) ist alles da.
    let (v, _) = lies(&state, "/api/aufgaben?offen=1&projekt=ruhig").await;
    assert_eq!(v["aufgaben"][0]["id"], a.as_str());
    let (v, _) = lies(&state, "/api/meldungen?projekt=ruhig").await;
    assert_eq!(v["meldungen"][0]["id"], m.as_str());
    let (v, _) = lies(&state, "/api/anstehend?projekt=ruhig").await;
    let ids: Vec<&str> = v["eintraege"].as_array().unwrap().iter().map(|e| e["id"].as_str().unwrap()).collect();
    for id in [a.as_str(), r.as_str(), m.as_str(), "stille-frage"] {
        assert!(ids.contains(&id), "{id} fehlt auf der Projektseite: {v}");
    }

    // Live-Kanal: Was ruhig ändert, kommt ohne Namen an (nur `anstehend`) oder gar nicht.
    let strom = crate::api::events::change_strom(&state);
    let n = {
        let anzahl = 6;
        for (area, path) in [
            ("aufgaben", "aufgaben/ruhig"),
            ("anstehend", "anstehend/ruhig"),
            ("projects", "projects/ruhig/daten/fragen.toml"),
            ("anstehend", "anstehend/laufend"),
            ("aufgaben", "aufgaben/laufend"),
            ("projects", "projects/laufend/projekt.toml"),
        ] {
            state.events.send(crate::api::events::Change { area, path: path.to_string(), at: chrono::Local::now() }).unwrap();
        }
        anzahl
    };
    let text = sse_text(tokio_stream::StreamExt::take(strom, n)).await;
    assert!(!text.contains("ruhig"), "{text}");
    let pfade: Vec<String> = text
        .lines()
        .filter_map(|z| z.strip_prefix("data: "))
        .map(|d| serde_json::from_str::<Value>(d).unwrap()["path"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(pfade, ["anstehend", "anstehend/laufend", "aufgaben/laufend", "projects/laufend/projekt.toml"]);
}

/// Der Text eines SSE-Stroms, bis er endet.
async fn sse_text(strom: impl tokio_stream::Stream<Item = Option<axum::response::sse::Event>> + Send + 'static) -> String {
    use axum::response::IntoResponse;
    let sse = axum::response::sse::Sse::new(tokio_stream::StreamExt::filter_map(strom, |e| e.map(Ok::<_, std::convert::Infallible>)));
    let koerper = tokio::time::timeout(std::time::Duration::from_secs(5), sse.into_response().into_body().collect()).await.unwrap().unwrap();
    String::from_utf8(koerper.to_bytes().to_vec()).unwrap()
}

/// Test 2 der Entscheidung: Eine Arbeit ohne Frist in einem archivierten
/// Projekt zählt nirgends mit und steht auf der Projektseite.
#[tokio::test]
async fn arbeit_ohne_frist_nirgends_gezaehlt_auf_der_projektseite_sichtbar() {
    let p = Probe::neu("ohne-frist");
    let state = p.zustand();
    let r = angelegt(
        &state,
        "/api/arbeiten",
        r#"{"projekt": "ruhig", "von": "chat:ruhig", "titel": "Übergabeprotokoll lesen", "stand": "wartet", "wartet_auf": "inhaber", "prioritaet": "jetzt"}"#,
    )
    .await;
    let (v, text) = lies(&state, "/api/anstehend").await;
    assert!(v["eintraege"].as_array().unwrap().iter().all(|e| e["projekt"] != "ruhig"), "{v}");
    assert!(!text.contains("Übergabeprotokoll lesen"), "{text}");
    assert_eq!(v["archiv_fristen"], serde_json::json!([]), "ohne Frist kein Satz");
    let (v, _) = lies(&state, "/api/arbeiten").await;
    assert!(v["arbeiten"].as_array().unwrap().is_empty(), "{v}");
    // Die Projektseite: Unter-Dashboard mit der Arbeit, Zahlen ohne Anmeldung mit ihr.
    let (v, _) = lies(&state, "/api/projekte/ruhig/stand/voll").await;
    assert!(v["eintraege"].as_array().unwrap().iter().any(|e| e["id"] == r.as_str()), "{v}");
    assert_eq!(v["gesamt"]["titel"], TITEL);
    assert_eq!((v["gesamt"]["status"].as_str(), v["gesamt"]["bedeutung"].as_str()), (Some("archiviert"), Some("neutral")));
    let (v, _) = lies(&state, "/api/anstehend?projekt=ruhig").await;
    assert!(v["eintraege"].as_array().unwrap().iter().any(|e| e["id"] == r.as_str()), "{v}");
    let (v, _) = lies(&state, "/api/projects/ruhig").await;
    assert_eq!((v["title"].as_str(), v["status"].as_str(), v["bedeutung"].as_str()), (Some(TITEL), Some("archiviert"), Some("neutral")));
}

/// Test 3 der Entscheidung: Eine Arbeit mit Frist in acht Tagen bleibt still;
/// in sieben Tagen steht sie in Anstehend mit dem neutralen Satz, ohne Titel.
/// Eine Frist-Meldung (Glocke) gibt es für archivierte Projekte nie.
#[tokio::test]
async fn frist_in_acht_tagen_still_in_sieben_tagen_neutral_in_anstehend() {
    let p = Probe::neu("frist");
    let state = p.zustand();
    let r = angelegt(
        &state,
        "/api/arbeiten",
        &format!(r#"{{"projekt": "ruhig", "von": "chat:ruhig", "titel": "Frist Wohnungsübergabe", "stand": "laeuft", "frist": "{}"}}"#, in_tagen(8)),
    )
    .await;
    let (v, text) = lies(&state, "/api/anstehend").await;
    assert_eq!(v["archiv_fristen"], serde_json::json!([]), "acht Tage: still");
    assert!(!text.contains("Frist Wohnungsübergabe") && !text.contains(TITEL), "{text}");

    // Die Frist rückt auf sieben Tage.
    let (status, text) = roh(&state, post(&format!("/api/arbeiten/{r}"), &format!(r#"{{"von": "chat:ruhig", "frist": "{}"}}"#, in_tagen(7)))).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let (v, text) = lies(&state, "/api/anstehend").await;
    let frist = chrono::NaiveDate::parse_from_str(&in_tagen(7), "%Y-%m-%d").unwrap();
    let satz = format!("Eine Frist in einem archivierten Projekt läuft am {} ab.", frist.format("%d.%m.%Y"));
    assert_eq!(v["archiv_fristen"], serde_json::json!([{ "satz": satz, "frist": in_tagen(7), "tage_bis": 7, "ziel": "/projects/ruhig" }]));
    assert!(v["eintraege"].as_array().unwrap().iter().all(|e| e["projekt"] != "ruhig"), "der Eintrag selbst bleibt still: {v}");
    assert!(!text.contains("Frist Wohnungsübergabe") && !text.contains(TITEL), "ohne Titel: {text}");
    // Auf der eigenen Seite steht der Eintrag wie sonst, ohne den Satz.
    let (v, _) = lies(&state, "/api/anstehend?projekt=ruhig").await;
    assert_eq!(v["archiv_fristen"], serde_json::json!([]));
    assert!(v["eintraege"].as_array().unwrap().iter().any(|e| e["id"] == r.as_str()));

    // Auch zwei Tage vorher keine Frist-Meldung (Glocke, Benachrichtigung).
    let (status, _) = roh(&state, post(&format!("/api/arbeiten/{r}"), &format!(r#"{{"von": "chat:ruhig", "frist": "{}"}}"#, in_tagen(1)))).await;
    assert_eq!(status, StatusCode::OK);
    let inhalt = crate::api::anstehend::inhalt_sammeln(&state).await.unwrap();
    crate::meldungen::fristen_lauf_mit(&state, &inhalt, jetzt()).await;
    let alle = state.speicher().unwrap().meldungen_lesen(None).unwrap();
    assert!(alle.iter().all(|m| m.projekt != "ruhig"), "keine Meldung für das Archiv: {alle:?}");
    let (v, _) = lies(&state, "/api/anstehend").await;
    assert_eq!(v["archiv_fristen"].as_array().unwrap().len(), 1, "{v}");
    assert_eq!(v["archiv_fristen"][0]["tage_bis"], 1);
    // Zum Vergleich: dasselbe bei einem aktiven Projekt meldet sich.
    angelegt(
        &state,
        "/api/arbeiten",
        &format!(r#"{{"projekt": "laufend", "von": "chat:laufend", "titel": "Frist Werkstatt", "stand": "laeuft", "frist": "{}"}}"#, in_tagen(1)),
    )
    .await;
    let inhalt = crate::api::anstehend::inhalt_sammeln(&state).await.unwrap();
    crate::meldungen::fristen_lauf_mit(&state, &inhalt, jetzt()).await;
    let alle = state.speicher().unwrap().meldungen_lesen(None).unwrap();
    assert!(alle.iter().any(|m| m.projekt == "laufend" && m.art == crate::vertrag::api::MeldungsArt::Frist), "{alle:?}");
}

/// Der neutrale Satz und die Grenze der Regel als Rechnung (ohne Server).
#[test]
fn archiv_frist_regel() {
    use crate::anstehend::{FRIST_BALD_TAGE, archiv_frist_satz};
    let tag = chrono::NaiveDate::from_ymd_opt(2026, 11, 8).unwrap();
    assert_eq!(archiv_frist_satz(tag, 7), "Eine Frist in einem archivierten Projekt läuft am 08.11.2026 ab.");
    assert_eq!(archiv_frist_satz(tag, 0), "Eine Frist in einem archivierten Projekt läuft am 08.11.2026 ab.");
    assert_eq!(archiv_frist_satz(tag, -2), "Eine Frist in einem archivierten Projekt ist am 08.11.2026 abgelaufen.");
    assert_eq!(FRIST_BALD_TAGE, 7, "die Entscheidung nennt sieben Tage");
}

/// Test 4 der Entscheidung: `status` zurück auf `aktiv`: alles wieder
/// sichtbar, nichts verloren.
#[tokio::test]
async fn zurueck_auf_aktiv_alles_wieder_sichtbar_nichts_verloren() {
    let p = Probe::neu("zurueck");
    let state = p.zustand();
    let (a, r, m) = eintraege_fuer(&state, "ruhig").await;
    let (v, _) = lies(&state, "/api/anstehend").await;
    assert!(v["eintraege"].as_array().unwrap().iter().all(|e| e["projekt"] != "ruhig"));

    p.status("aktiv");
    let (v, _) = lies(&state, "/api/projects").await;
    assert_eq!(slugs(&v), ["laufend", "ruhig"]);
    let (v, _) = lies(&state, "/api/archiv").await;
    assert!(v["projekte"].as_array().unwrap().is_empty(), "{v}");
    let (v, _) = lies(&state, "/api/anstehend").await;
    let ids: Vec<&str> = v["eintraege"].as_array().unwrap().iter().filter(|e| e["projekt"] == "ruhig").map(|e| e["id"].as_str().unwrap()).collect();
    for id in [a.as_str(), r.as_str(), m.as_str(), "stille-frage"] {
        assert!(ids.contains(&id), "{id} ist nach der Rückkehr wieder da: {v}");
    }
    assert_eq!(v["eintraege"].as_array().unwrap().iter().find(|e| e["id"] == a.as_str()).unwrap()["projekt_titel"], TITEL);
    let (v, _) = lies(&state, "/api/aufgaben/anzahl").await;
    assert_eq!(v["offen"], 1);
    let (v, _) = lies(&state, "/api/meldungen").await;
    assert_eq!(projekte_von(&v, "meldungen"), ["ruhig"]);
    let (v, _) = lies(&state, "/api/arbeiten").await;
    assert_eq!(projekte_von(&v, "arbeiten"), ["ruhig"]);
    // Der Live-Kanal nennt das Projekt wieder.
    let strom = crate::api::events::change_strom(&state);
    state.events.send(crate::api::events::Change { area: "anstehend", path: "anstehend/ruhig".into(), at: chrono::Local::now() }).unwrap();
    let text = sse_text(tokio_stream::StreamExt::take(strom, 1)).await;
    assert!(text.contains("\"path\":\"anstehend/ruhig\""), "{text}");
}

/// Übergang (`[uebergang]`): Ein Projektordner wird aus `projects/`
/// verschoben, seine Einträge im Datenspeicher bleiben. Das bringt den Dienst
/// nicht aus dem Tritt: Die Einträge stehen still als „nicht mehr da“ (ohne
/// Titel, ohne Link), keine Antwort ist ein Fehler, und der Fristen-Lauf geht.
#[tokio::test]
async fn eintraege_ohne_projektordner_stoeren_nicht() {
    let p = Probe::neu("ordner-weg");
    let state = p.zustand();
    let (a, r, m) = eintraege_fuer(&state, "laufend").await;
    angelegt(
        &state,
        "/api/arbeiten",
        &format!(r#"{{"projekt": "laufend", "von": "chat:laufend", "titel": "Mit Frist", "stand": "laeuft", "frist": "{}"}}"#, in_tagen(1)),
    )
    .await;
    // Der Ordner zieht um (wie `git mv projects/laufend archiviert/projects/laufend`).
    let ziel = p.wurzel.join("archiviert").join("projects");
    std::fs::create_dir_all(&ziel).unwrap();
    std::fs::rename(p.wurzel.join("projects").join("laufend"), ziel.join("laufend")).unwrap();

    let (v, _) = lies(&state, "/api/projects").await;
    assert!(slugs(&v).is_empty(), "{v}");
    let (v, _) = lies(&state, "/api/archiv").await;
    let archiv: Vec<&str> = v["projekte"].as_array().unwrap().iter().map(|x| x["slug"].as_str().unwrap()).collect();
    assert_eq!(archiv, ["ruhig"], "ein verschobener Ordner ist nicht archiviert, nur weg: {v}");
    let (v, _) = lies(&state, "/api/anstehend").await;
    for id in [&a, &r, &m] {
        let e = v["eintraege"].as_array().unwrap().iter().find(|e| e["id"] == id.as_str()).unwrap_or_else(|| panic!("{id}: {v}"));
        assert_eq!((e["projekt_titel"].clone(), e["ziel_da"].as_bool()), (Value::Null, Some(false)), "{e}");
    }
    for pfad in [
        "/api/aufgaben/anzahl",
        "/api/aufgaben?offen=1",
        "/api/arbeiten",
        "/api/meldungen",
        "/api/antworten",
        "/api/pc",
        "/api/pruefung",
        "/api/anstehend?projekt=laufend",
    ] {
        lies(&state, pfad).await;
    }
    let (status, text) = roh(&state, get("/api/projekte/laufend/stand")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{text}");
    let (status, text) = roh(&state, get("/api/projects/laufend")).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{text}");
    // Der Fristen-Lauf geht durch und meldet wie vorher (Arbeiten kommen aus dem Datenspeicher).
    let inhalt = crate::api::anstehend::inhalt_sammeln(&state).await.unwrap();
    crate::meldungen::fristen_lauf_mit(&state, &inhalt, jetzt()).await;
    // Der Live-Kanal hält auch Pfade zu unbekannten Projekten aus.
    let strom = crate::api::events::change_strom(&state);
    state.events.send(crate::api::events::Change { area: "anstehend", path: "anstehend/laufend".into(), at: chrono::Local::now() }).unwrap();
    let text = sse_text(tokio_stream::StreamExt::take(strom, 1)).await;
    assert!(text.contains("anstehend/laufend"), "{text}");
}

/// Prüfungsbefund: Ein Tippfehler im Kopf eines archivierten Projekts hebt das
/// Archiv nicht auf. Solange der Kopf kaputt ist, gilt der zuletzt bekannte
/// Status; startet der Dienst mit kaputtem Kopf, zählt die Zeile `status = …`.
/// Das Projekt steht dann weder unter „Seiten mit Fehlern“ noch in Anstehend,
/// Glocke oder Fristen-Lauf; seine Seite nennt den Fehler in einem Satz.
#[tokio::test]
async fn kaputter_kopf_hebt_das_archiv_nicht_auf() {
    let p = Probe::neu("kaputt");
    let state = p.zustand();
    let (a, _, _) = eintraege_fuer(&state, "ruhig").await;
    angelegt(
        &state,
        "/api/arbeiten",
        &format!(r#"{{"projekt": "ruhig", "von": "chat:ruhig", "titel": "Frist Wohnungsübergabe", "stand": "laeuft", "frist": "{}"}}"#, in_tagen(1)),
    )
    .await;
    lies(&state, "/api/projects").await;

    // Erst ein Tippfehler im Status selbst (der zuletzt bekannte Stand gilt),
    // dann ein unbekanntes Feld bei richtigem Status.
    for kopf in [
        format!("schema = 3\ntitle = \"{TITEL}\"\nstatus = \"archivert\"\nsummary = \"S\"\n"),
        format!("schema = 3\ntitle = \"{TITEL}\"\nstatus = \"archiviert\"\ntitel = \"Tippfehler\"\n"),
    ] {
        std::fs::write(p.ruhig().join("projekt.toml"), &kopf).unwrap();
        let (v, text) = lies(&state, "/api/projects").await;
        assert_eq!(slugs(&v), ["laufend"], "{kopf}");
        assert!(v["broken"].as_array().unwrap().is_empty() && !text.contains("ruhig"), "{kopf}: {text}");
        let (v, _) = lies(&state, "/api/archiv").await;
        assert_eq!(v["projekte"][0]["slug"], "ruhig", "{kopf}: {v}");
        // Prüfung rot und Fristen: nichts davon in Anstehend und Glocke.
        crate::meldungen::pruefung_lauf(&state, "ruhig", jetzt()).await;
        let inhalt = crate::api::anstehend::inhalt_sammeln(&state).await.unwrap();
        crate::meldungen::fristen_lauf_mit(&state, &inhalt, jetzt()).await;
        let (v, text) = lies(&state, "/api/anstehend").await;
        assert!(v["eintraege"].as_array().unwrap().iter().all(|e| e["projekt"] != "ruhig"), "{kopf}: {v}");
        assert!(!text.contains(TITEL) && !text.contains("ruhig/") && !text.contains("„ruhig“"), "{kopf}: {text}");
        assert_eq!(v["archiv_fristen"].as_array().unwrap().len(), 1, "die Sicherheitsregel gilt weiter: {v}");
        let (v, _) = lies(&state, "/api/meldungen").await;
        assert!(projekte_von(&v, "meldungen").iter().all(|p| p != "ruhig"), "{kopf}: {v}");
        let (v, _) = lies(&state, "/api/aufgaben/anzahl").await;
        assert_eq!(v["offen"], 0, "{kopf}");
        // Die eigene Seite nennt den Fehler (ein Satz), ihr Unter-Dashboard hat alles.
        let (status, text) = roh(&state, get("/api/projects/ruhig")).await;
        assert_ne!(status, StatusCode::OK, "{text}");
        assert!(text.contains("projekt.toml"), "{text}");
        let (v, _) = lies(&state, "/api/anstehend?projekt=ruhig").await;
        assert!(v["eintraege"].as_array().unwrap().iter().any(|e| e["id"] == a.as_str()), "{v}");
    }
    // Die Prüfung meldet sich nur auf der eigenen Seite.
    let (v, _) = lies(&state, "/api/meldungen?projekt=ruhig").await;
    assert!(v["meldungen"].as_array().unwrap().iter().any(|m| m["schluessel"] == "ruhig/pruefung-rot"), "{v}");

    // Neustart mit kaputtem Kopf (unbekanntes Feld): die Zeile `status` zählt.
    let (v, text) = lies(&p.zustand(), "/api/projects").await;
    assert!(v["broken"].as_array().unwrap().is_empty() && !text.contains("ruhig"), "{text}");
    // Groß und klein ist egal.
    std::fs::write(p.ruhig().join("projekt.toml"), format!("schema = 3\ntitle = \"{TITEL}\"\nstatus = \"Archiviert\"\n")).unwrap();
    let (_, text) = lies(&p.zustand(), "/api/projects").await;
    assert!(!text.contains("ruhig"), "{text}");
    // Ein kaputter Kopf mit anderem Status beim Start: nicht archiviert (als Fehler sichtbar).
    std::fs::write(p.ruhig().join("projekt.toml"), format!("schema = 3\ntitle = \"{TITEL}\"\nstatus = \"aktiv\"\ntitel = \"x\"\n")).unwrap();
    let (v, _) = lies(&p.zustand(), "/api/projects").await;
    assert_eq!(v["broken"][0]["slug"], "ruhig", "{v}");
}

/// `archiviert_seit` aus projekt.toml steht in der Archivansicht.
#[tokio::test]
async fn archiviert_seit_in_der_archivansicht() {
    let p = Probe::neu("seit");
    std::fs::write(
        p.ruhig().join("projekt.toml"),
        format!("schema = 3\ntitle = \"{TITEL}\"\nstatus = \"archiviert\"\narchiviert_seit = \"2026-10-31\"\nsummary = \"S\"\n"),
    )
    .unwrap();
    let (v, _) = lies(&p.zustand(), "/api/archiv").await;
    assert_eq!(v["projekte"][0]["seit"], "2026-10-31", "{v}");
}

/// Prüfungsbefund: Vorgänge eines archivierten Projekts (sein Chat, ein
/// Merge-Antrag für sein Modul) stehen nicht auf /vorgaenge, in den Zählern,
/// im Verlauf von Startseite und Cockpit und auf dem Live-Kanal; mit
/// `?fuer=<slug>` sieht sein Chat alles.
#[tokio::test]
async fn vorgaenge_eines_archivierten_projekts_sind_still() {
    let p = Probe::neu("vorgaenge");
    let state = p.zustand();
    let s = state.speicher().unwrap();
    let art = crate::vertrag::api::VorgangsArt::Abnahme;
    let (still, _) = s.vorgang_anlegen(art, "ruhig", "Abnahme Übergabeprotokoll", None, "chat:ruhig").unwrap();
    let (laut, _) = s.vorgang_anlegen(art, "laufend", "Abnahme Kachel", None, "chat:laufend").unwrap();
    let antrag = crate::vertrag::api::MergeAntrag {
        zweig: "projekt/ruhig-modul".into(),
        commit: "abcdef1".into(),
        projekt: "ruhig".into(),
        beschreibung: "Modul".into(),
        pruefbericht: "grün".into(),
    };
    let (merge, _) = s.merge_anlegen("dashboard", "Merge Modul Kistenliste", None, "chat:ruhig", antrag).unwrap();

    let (v, text) = lies(&state, "/api/vorgaenge/uebersicht").await;
    assert!(!text.contains("ruhig") && !text.contains("Kistenliste") && !text.contains("Übergabeprotokoll"), "{text}");
    let ids: Vec<&str> = v["vorgaenge"].as_array().unwrap().iter().map(|x| x["id"].as_str().unwrap()).collect();
    assert_eq!(ids, [laut.id.as_str()]);
    assert_eq!(v["zaehler"]["offen"], 1, "{v}");
    assert!(v["merge_offen"].as_array().unwrap().is_empty(), "{v}");
    // Mit `fuer`: alles.
    let (v, _) = lies(&state, "/api/vorgaenge?fuer=ruhig").await;
    assert_eq!(v["vorgaenge"][0]["id"], still.id.as_str());
    let (v, _) = lies(&state, "/api/vorgaenge?fuer=dashboard").await;
    assert_eq!(v["vorgaenge"][0]["id"], merge.id.as_str());
    // Verlauf (Startseite und PC-Seite): kein Merge-Antrag für das Archiv.
    let (_, text) = lies(&state, "/api/pc/verlauf/voll").await;
    assert!(!text.contains("Kistenliste") && !text.contains(&merge.id), "{text}");

    // Live-Kanal: Vorgang und Bau eines archivierten Projekts bleiben stumm.
    use crate::api::events::{Art, BauEreignis, VorgangEreignis, ohne_archiv_strom};
    let strom = ohne_archiv_strom(&state, state.vorgang_events.subscribe(), Art::Vorgang, |v: &VorgangEreignis| v.slug.as_str());
    for slug in ["ruhig", "laufend"] {
        state.vorgang_events.send(VorgangEreignis { id: "v-1".into(), slug: slug.into(), art, zustand: crate::vertrag::api::Zustand::Offen, nr: 1 }).unwrap();
    }
    let text = sse_text(tokio_stream::StreamExt::take(strom, 2)).await;
    assert!(!text.contains("ruhig") && text.contains("laufend"), "{text}");
    let strom = ohne_archiv_strom(&state, state.bau_events.subscribe(), Art::Bau, |b: &BauEreignis| b.slug.as_str());
    state
        .bau_events
        .send(BauEreignis { slug: "ruhig".into(), zustand: crate::vertrag::api::BauZustand::Laeuft, meldung: "Baut".into(), gescheitert: false })
        .unwrap();
    let text = sse_text(tokio_stream::StreamExt::take(strom, 1)).await;
    assert!(!text.contains("ruhig"), "{text}");
}
