//! Tests zur Entscheidung projektgruppe (`[anzeige].tests`, Teil
//! Server): Gruppenseite als Seite der Oberfläche, Gruppen in `/api/projects`
//! (mit Archiv-Status), Gruppentitel an den Einträgen in „Anstehend“, eine
//! Antwort im Teil weckt nur dessen Chat, `data/gruppen.toml` live gelesen und
//! wieder entfernt (alles wie vorher). Die Prüfung der Datei steht in
//! `inhalt::pruefung` (`gruppen_gueltig_unbekannt_doppelt_kollision`).
//!
//! Alle Projekte, Gruppen und Titel sind erfunden.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use crate::api::events::InhaltEreignis;
use crate::app::{AppState, router_angemeldet};

const GRUPPEN: &str = "[[gruppe]]\nslug = \"alpha-und-beta\"\ntitel = \"Alpha und Beta\"\nteile = [\"alpha\", \"beta\"]\n\n\
                       [[gruppe]]\nslug = \"nur-archiv\"\ntitel = \"Nur Archiv\"\nteile = [\"gamma\"]\n";

/// Ein Inhaltsordner mit `alpha` (Frage), `beta` (Frage), `gamma`
/// (archiviert) und `delta` (ohne Gruppe). Verschwindet mit dem Wächter.
struct Probe {
    wurzel: PathBuf,
}

impl Drop for Probe {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.wurzel);
    }
}

fn frage(id: &str) -> String {
    format!("[[frage]]\nid = \"{id}\"\ntext = \"Weiter so?\"\nart = \"ja-nein\"\ngestellt = \"2026-10-30\"\nprioritaet = \"jetzt\"\n")
}

impl Probe {
    fn neu(name: &str, mit_gruppen: bool) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let wurzel = std::env::temp_dir().join(format!("dashboard-test-{}-gruppen-{name}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&wurzel);
        for (slug, titel, status) in [
            ("alpha", "Alpha-Vorhaben", "aktiv"),
            ("beta", "Beta-Vorhaben", "pausiert"),
            ("gamma", "Gamma-Vorhaben", "archiviert"),
            ("delta", "Delta-Vorhaben", "aktiv"),
        ] {
            let p = wurzel.join("projects").join(slug);
            std::fs::create_dir_all(p.join("daten")).unwrap();
            std::fs::write(p.join("projekt.toml"), format!("schema = 3\ntitle = \"{titel}\"\nstatus = \"{status}\"\nsummary = \"S\"\n")).unwrap();
            if slug == "alpha" || slug == "beta" {
                std::fs::write(p.join("daten").join("fragen.toml"), frage(&format!("frage-{slug}"))).unwrap();
            }
        }
        std::fs::create_dir_all(wurzel.join("data")).unwrap();
        let p = Self { wurzel };
        if mit_gruppen {
            p.gruppen(Some(GRUPPEN));
        }
        p
    }

    fn gruppen(&self, text: Option<&str>) {
        let datei = self.wurzel.join("data").join("gruppen.toml");
        match text {
            Some(t) => std::fs::write(datei, t).unwrap(),
            None => std::fs::remove_file(datei).unwrap(),
        }
    }
}

fn get(pfad: &str) -> Request<Body> {
    Request::get(pfad).header("host", "127.0.0.1:8787").body(Body::empty()).unwrap()
}

fn post_browser(pfad: &str, koerper: &str) -> Request<Body> {
    Request::post(pfad)
        .header("host", "localhost:8787")
        .header("origin", "http://localhost:8787")
        .header("sec-fetch-site", "same-origin")
        .header("content-type", "application/json")
        .body(Body::from(koerper.to_string()))
        .unwrap()
}

async fn roh(state: &AppState, req: Request<Body>) -> (StatusCode, String) {
    let res = router_angemeldet(state.clone()).oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).into_owned())
}

async fn json(state: &AppState, pfad: &str) -> Value {
    let (status, text) = roh(state, get(pfad)).await;
    assert_eq!(status, StatusCode::OK, "{pfad}: {text}");
    serde_json::from_str(&text).unwrap()
}

/// Eine Seite der Oberfläche: 200 (oder 503 ohne Build), sonst 404.
async fn ist_seite(state: &AppState, pfad: &str) -> bool {
    let (status, _) = roh(state, get(pfad)).await;
    assert!(matches!(status, StatusCode::OK | StatusCode::SERVICE_UNAVAILABLE | StatusCode::NOT_FOUND), "{pfad}: {status}");
    status != StatusCode::NOT_FOUND
}

/// Die Gruppenseite ist eine Seite (Erlaubnisliste der Oberfläche) nur für
/// eine Gruppe mit sichtbaren Teilen; die alten Adressen der Teile bleiben.
#[tokio::test]
async fn gruppenseite_nur_fuer_gruppen_mit_teilen() {
    let p = Probe::neu("seite", true);
    let state = AppState::fuer_tests(&p.wurzel);
    assert!(ist_seite(&state, "/gruppen/alpha-und-beta").await);
    assert!(ist_seite(&state, "/gruppen/alpha-und-beta/").await);
    for nein in ["/gruppen", "/gruppen/nur-archiv", "/gruppen/gibt-es-nicht", "/gruppen/alpha-und-beta/x", "/gruppen/alpha"] {
        assert!(!ist_seite(&state, nein).await, "{nein}");
    }
    // Keine Umleitung: Die Teile bleiben unter ihren Adressen.
    assert!(ist_seite(&state, "/projects/alpha").await && ist_seite(&state, "/projects/beta").await);
}

/// `/api/projects` nennt die Gruppen mit ihren Teilen in der Reihenfolge aus
/// `teile`; ein archivierter Teil fehlt, eine Gruppe nur aus archivierten
/// Teilen auch. Bleibt ein Teil, zeigt die Gruppe diesen einen.
#[tokio::test]
async fn gruppen_in_der_projektliste_mit_archiv_status() {
    let p = Probe::neu("liste", true);
    let state = AppState::fuer_tests(&p.wurzel);
    let v = json(&state, "/api/projects").await;
    assert_eq!(v["gruppen"], serde_json::json!([{"slug": "alpha-und-beta", "titel": "Alpha und Beta", "teile": ["alpha", "beta"]}]));
    // Reihenfolge = teile, nicht nach Titel oder Slug.
    p.gruppen(Some("[[gruppe]]\nslug = \"alpha-und-beta\"\ntitel = \"Alpha und Beta\"\nteile = [\"beta\", \"alpha\"]\n"));
    assert_eq!(json(&state, "/api/projects").await["gruppen"][0]["teile"], serde_json::json!(["beta", "alpha"]));
    // alpha wird archiviert: die Gruppe zeigt nur noch beta; die Seite bleibt.
    std::fs::write(
        p.wurzel.join("projects").join("alpha").join("projekt.toml"),
        "schema = 3\ntitle = \"Alpha-Vorhaben\"\nstatus = \"archiviert\"\nsummary = \"S\"\n",
    )
    .unwrap();
    assert_eq!(json(&state, "/api/projects").await["gruppen"][0]["teile"], serde_json::json!(["beta"]));
    assert!(ist_seite(&state, "/gruppen/alpha-und-beta").await);
    // beta auch: Die Gruppe verschwindet, ihre Seite auch.
    std::fs::write(
        p.wurzel.join("projects").join("beta").join("projekt.toml"),
        "schema = 3\ntitle = \"Beta-Vorhaben\"\nstatus = \"archiviert\"\nsummary = \"S\"\n",
    )
    .unwrap();
    assert_eq!(json(&state, "/api/projects").await["gruppen"], serde_json::json!([]));
    assert!(!ist_seite(&state, "/gruppen/alpha-und-beta").await);
}

/// „Anstehend“: Die Einträge bleiben beim Projekt und tragen den
/// Gruppentitel; ein Projekt ohne Gruppe hat das Feld nicht.
#[tokio::test]
async fn eintraege_tragen_die_gruppe() {
    let p = Probe::neu("anstehend", true);
    let state = AppState::fuer_tests(&p.wurzel);
    let v = json(&state, "/api/anstehend").await;
    let e = v["eintraege"].as_array().unwrap();
    let von = |slug: &str| e.iter().find(|x| x["projekt"] == slug).unwrap_or_else(|| panic!("{slug}: {v}")).clone();
    assert_eq!(von("alpha")["gruppe"], serde_json::json!({"slug": "alpha-und-beta", "titel": "Alpha und Beta"}));
    assert_eq!((von("alpha")["projekt_titel"].as_str(), von("alpha")["ziel"].as_str()), (Some("Alpha-Vorhaben"), Some("/projects/alpha#frage-frage-alpha")));
    assert_eq!(von("beta")["gruppe"]["titel"], "Alpha und Beta");
    // Zwei Einträge, zwei Projekte: jeder Eintrag einmal, wie bisher.
    assert_eq!(e.len(), 2, "{v}");
}

/// Eine Antwort auf eine Frage im Teil alpha weckt nur den Chat von alpha;
/// der wartende Chat von beta schläft weiter.
#[tokio::test]
async fn antwort_im_teil_weckt_nur_dessen_chat() {
    let p = Probe::neu("antwort", true);
    let state = AppState::fuer_tests(&p.wurzel);
    let warten = |fuer: &'static str| {
        let s = state.clone();
        tokio::spawn(async move { roh(&s, get(&format!("/api/vorgaenge/warten?fuer={fuer}&timeout=10"))).await })
    };
    let alpha = warten("alpha");
    let beta = warten("beta");
    let frist = Instant::now() + Duration::from_secs(2);
    while state.warteraum.anzahl("alpha") != 1 || state.warteraum.anzahl("beta") != 1 {
        assert!(Instant::now() < frist, "Warter melden sich nicht an");
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let (status, text) = roh(&state, post_browser("/api/antworten/alpha/frage-alpha", r#"{"gestellt": "2026-10-30", "antwort": "ja"}"#)).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let (status, w) = alpha.await.unwrap();
    assert_eq!(status, StatusCode::OK, "{w}");
    let w: Value = serde_json::from_str(&w).unwrap();
    assert_eq!((w["vorgaenge"][0]["slug"].as_str(), w["vorgaenge"][0]["bezug"].as_str()), (Some("alpha"), Some("antwort:alpha/frage-alpha")));
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(!beta.is_finished(), "beta darf nicht aufwachen");
    assert_eq!(state.warteraum.anzahl("beta"), 1);
    beta.abort();
}

/// `data/gruppen.toml` wird wie `schwerpunkte.toml` live gelesen (Ereignis
/// `daten` mit Name `gruppen`). Entfernt: keine Gruppe mehr, alles andere
/// wie vorher (Projekte, Einträge, Seiten), nichts verloren.
#[tokio::test]
async fn gruppen_live_und_entfernt_wie_vorher() {
    let p = Probe::neu("live", false);
    let state = AppState::fuer_tests(&p.wurzel);
    let vorher_projekte = json(&state, "/api/projects").await;
    let vorher_anstehend = json(&state, "/api/anstehend").await;
    assert_eq!(vorher_projekte["gruppen"], serde_json::json!([]), "ohne Datei keine Gruppen, kein Fehler");
    assert!(!vorher_anstehend.to_string().contains("\"gruppe\""), "{vorher_anstehend}");

    let mut rx = state.inhalt.abonnieren();
    p.gruppen(Some(GRUPPEN));
    let mit = json(&state, "/api/projects").await;
    assert_eq!(mit["gruppen"][0]["slug"], "alpha-und-beta");
    let namen: Vec<String> = std::iter::from_fn(|| rx.try_recv().ok())
        .filter_map(|e| match e {
            InhaltEreignis::Daten(d) => Some(d.name),
            _ => None,
        })
        .collect();
    assert_eq!(namen, ["gruppen"]);
    // Die Projekte selbst ändern sich durch die Gruppe nicht.
    assert_eq!(mit["projects"], vorher_projekte["projects"]);

    p.gruppen(None);
    assert_eq!(json(&state, "/api/projects").await, vorher_projekte);
    assert_eq!(json(&state, "/api/anstehend").await["eintraege"], vorher_anstehend["eintraege"]);
    assert!(!ist_seite(&state, "/gruppen/alpha-und-beta").await);
    assert!(ist_seite(&state, "/projects/alpha").await);
}

/// Zur Laufzeit, nicht nur im Prüfbefehl: Gruppen mit leerem oder
/// ungültigem Slug und mit dem Slug eines Projekts (auch eines archivierten)
/// fehlen in `/api/projects` und als Seite; „Anstehend“ nennt sie als
/// Hinweis. Die gültige Gruppe daneben bleibt.
#[tokio::test]
async fn schlechte_gruppen_slugs_fehlen_zur_laufzeit() {
    let p = Probe::neu("slugs", false);
    p.gruppen(Some(
        "[[gruppe]]\nslug = \"\"\ntitel = \"Leer\"\nteile = [\"alpha\"]\n\n\
         [[gruppe]]\nslug = \"x/\"\ntitel = \"Schraeg\"\nteile = [\"alpha\"]\n\n\
         [[gruppe]]\nslug = \"delta\"\ntitel = \"Wie ein Projekt\"\nteile = [\"alpha\"]\n\n\
         [[gruppe]]\nslug = \"gamma\"\ntitel = \"Wie ein Archiv\"\nteile = [\"alpha\"]\n\n\
         [[gruppe]]\nslug = \"alpha-und-beta\"\ntitel = \"Alpha und Beta\"\nteile = [\"alpha\", \"beta\"]\n",
    ));
    let state = AppState::fuer_tests(&p.wurzel);
    let v = json(&state, "/api/projects").await;
    assert_eq!(v["gruppen"], serde_json::json!([{"slug": "alpha-und-beta", "titel": "Alpha und Beta", "teile": ["alpha", "beta"]}]));
    let a = json(&state, "/api/anstehend").await;
    let hinweise: Vec<String> = a["hinweise"].as_array().unwrap().iter().map(|h| h.as_str().unwrap().to_string()).collect();
    for titel in ["Leer", "Schraeg", "Wie ein Projekt", "Wie ein Archiv"] {
        assert!(hinweise.iter().any(|h| h.starts_with("data/gruppen.toml") && h.contains(&format!("„{titel}“"))), "{titel}: {hinweise:?}");
    }
    // Die Einträge von alpha tragen die gültige Gruppe.
    let alpha = a["eintraege"].as_array().unwrap().iter().find(|e| e["projekt"] == "alpha").unwrap().clone();
    assert_eq!(alpha["gruppe"]["slug"], "alpha-und-beta");
    // Die Projektseiten bleiben, die schlechten Gruppen haben keine Seite.
    assert!(ist_seite(&state, "/projects/delta").await && ist_seite(&state, "/gruppen/alpha-und-beta").await);
    assert!(!ist_seite(&state, "/gruppen/delta").await && !ist_seite(&state, "/gruppen/gamma").await);
}
