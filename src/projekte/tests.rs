//! Tests des Registers (Etappe 4b): Router im Dienst, Absturz je Anfrage und
//! je Hook, Anmeldung und Größe für schreibende Wege, Hooks aus Beobachter,
//! Vorgängen, Bau und Takt, Zeitlimit und höchstens ein Lauf, Namensraum in
//! /daten, Sicherung und Export, Vertrag der Module.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::State;
use axum::http::{Request, StatusCode, header};
use axum::routing::{get, post};
use axum::{Json, Router};
use dashboard_projekt::hooks::{Aufruf, BauErgebnis, VorgangInfo};
use dashboard_projekt::{Fehler, Hooks, Modul, ProjektKontext, TabellenDef, Zeitplan};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use super::*;
use crate::app::{AppState, beispiel_fuer_tests, router, router_angemeldet};

/// Ein Testmodul für das Beispielprojekt `neu-daten` (hat daten/fortschritt.toml).
struct Probe;

fn zaehlen(k: &ProjektKontext, schluessel: &str) -> Result<(), Fehler> {
    let t = k.tabelle("protokoll")?;
    let n: u64 = t.lesen(schluessel)?.unwrap_or(0);
    t.schreiben(schluessel, &(n + 1))
}

fn nach_lesen(_: &ProjektKontext, name: &str, w: &mut Value) -> Result<(), Fehler> {
    if name == "gantt" {
        panic!("Absicht in nach_lesen");
    }
    w["vom_modul"] = json!(true);
    Ok(())
}

fn zeitplan_panik(_: &ProjektKontext) -> Result<(), Fehler> {
    panic!("Absicht im Zeitplan")
}

fn taeglich(k: &ProjektKontext) -> Result<(), Fehler> {
    zaehlen(k, "taeglich")
}

fn geaendert(k: &ProjektKontext, name: &str) -> Result<(), Fehler> {
    zaehlen(k, &format!("geaendert/{name}"))
}

fn bei_vorgang(k: &ProjektKontext, v: &VorgangInfo) -> Result<(), Fehler> {
    k.tabelle("protokoll")?.schreiben(&format!("vorgang/{}/{}", v.id, v.zustand), &v.art)
}

fn nach_bau(k: &ProjektKontext, e: BauErgebnis) -> Result<(), Fehler> {
    k.tabelle("protokoll")?.schreiben("bau", &e)
}

impl Modul for Probe {
    fn herkunft(&self) -> dashboard_projekt::Herkunft {
        dashboard_projekt::herkunft!()
    }
    fn slug(&self) -> &'static str {
        "neu-daten"
    }
    fn router(&self) -> Router<ProjektKontext> {
        Router::new()
            .route("/gut", get(|State(k): State<ProjektKontext>| async move { Json(json!({ "slug": k.slug() })) }))
            .route(
                "/panik",
                get(|| async {
                    if true {
                        panic!("Absicht im Endpunkt");
                    }
                    "nie"
                }),
            )
            .route("/schreiben", post(|Json(v): Json<Value>| async move { Json(json!({ "laenge": v.to_string().len() })) }))
    }
    fn tabellen(&self) -> Vec<TabellenDef> {
        vec![TabellenDef::neu("protokoll", "Probe: was die Hooks taten")]
    }
    fn hooks(&self) -> Hooks {
        Hooks {
            nach_lesen: Some(nach_lesen),
            nach_daten_geaendert: Some(geaendert),
            zeitplan: Some(Zeitplan { alle: Duration::from_secs(1), lauf: zeitplan_panik }),
            taeglich: Some(taeglich),
            bei_vorgang: Some(bei_vorgang),
            nach_bau: Some(nach_bau),
        }
    }
}

fn zustand_mit(module: Vec<ModulEintrag>) -> AppState {
    let mut s = AppState::fuer_tests(&beispiel_fuer_tests());
    let reg = Register::fuer_zustand_ungeprueft(&s, module).mit_zeitlimits(Duration::from_millis(300), Duration::from_millis(300));
    reg.tabellen_einrichten(s.speicher.as_ref().unwrap());
    s.projekte = Arc::new(reg);
    s
}

fn get_req(pfad: &str) -> Request<Body> {
    Request::get(pfad).header("host", "127.0.0.1:8787").body(Body::empty()).unwrap()
}

fn post_req(pfad: &str, koerper: String, origin: bool) -> Request<Body> {
    let mut r = Request::post(pfad).header("host", "127.0.0.1:8787").header(header::CONTENT_TYPE, "application/json");
    if origin {
        r = r.header(header::ORIGIN, "http://127.0.0.1:8787");
    }
    r.body(Body::from(koerper)).unwrap()
}

async fn schicke(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into())))
}

fn protokoll(state: &AppState, schluessel: &str) -> Option<Value> {
    let s = state.speicher().unwrap();
    s.projekt_eintrag_lesen("neu-daten", "protokoll", 1, schluessel).unwrap().map(|b| serde_json::from_slice(&b).unwrap())
}

async fn bis(was: &str, frist: Duration, f: impl Fn() -> bool) {
    let ende = Instant::now() + frist;
    while !f() {
        assert!(Instant::now() < ende, "{was}: nicht eingetreten");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn hat_stoerung(state: &AppState, wo: &str) -> bool {
    state.projekte.stoerungen().alle().iter().any(|((s, w), _)| s == "neu-daten" && w == wo)
}

/// Abnahme: Panic im Endpunkt → diese Anfrage 500 mit Satz und ein Fund in
/// /api/pruefung; alles andere läuft (auch die allgemeinen Wege desselben Projekts).
#[tokio::test]
async fn panik_im_endpunkt_trifft_nur_diese_anfrage() {
    let state = zustand_mit(vec![eintrag(Probe)]);
    // Seit Etappe B2 sind auch lesende Modul-Wege und die Befunde geschützt: angemeldet.
    let app = router_angemeldet(state.clone());
    let (status, v) = schicke(&app, get_req("/api/projekte/neu-daten/x/panik")).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "{v}");
    assert_eq!(v["art"], "intern");
    assert!(v["fehler"].as_str().unwrap().contains("Das Modul „neu-daten“ ist bei dieser Anfrage abgestürzt"), "{v}");
    assert_eq!(schicke(&app, get_req("/api/projekte/neu-daten/x/gut")).await, (StatusCode::OK, json!({ "slug": "neu-daten" })));
    for pfad in ["/healthz", "/api/projects", "/api/projekte/neu-daten", "/api/projekte/neu-daten/daten/fortschritt", "/api/projekte/neu-form"] {
        assert_eq!(schicke(&app, get_req(pfad)).await.0, StatusCode::OK, "{pfad}");
    }
    let (status, v) = schicke(&app, get_req("/api/projekte/neu-daten/x/gibt-es-nicht")).await;
    assert_eq!((status, v["art"].as_str()), (StatusCode::NOT_FOUND, Some("nicht-gefunden")), "{v}");
    let (_, p) = schicke(&app, get_req("/api/pruefung")).await;
    let projekt = p["projekte"].as_array().unwrap().iter().find(|x| x["slug"] == "neu-daten").cloned().unwrap();
    let befund = projekt["befunde"].as_array().unwrap().iter().find(|b| b["datei"] == "projects/neu-daten/server/src/lib.rs").cloned();
    let befund = befund.unwrap_or_else(|| panic!("kein Fund des Moduls: {projekt}"));
    assert_eq!(befund["schwere"], "fehler");
    assert!(befund["text"].as_str().unwrap().contains("endpunkt: abgestürzt: Absicht im Endpunkt"), "{befund}");
    let (_, e) = schicke(&app, get_req("/api/pruefung/neu-daten")).await;
    assert!(e["befunde"].to_string().contains("Absicht im Endpunkt"), "{e}");
}

/// Abnahme: Panic im Hook → Fund in /api/pruefung (wiederholt sichtbar), der Dienst läuft weiter.
#[tokio::test]
async fn panik_im_hook_wird_fund() {
    let state = zustand_mit(vec![eintrag(Probe)]);
    for n in 1..=3 {
        state.projekte.ausloesen("neu-daten", Aufruf::Zeitplan);
        bis("Störung zeitplan", Duration::from_secs(5), || {
            state.projekte.stoerungen().alle().iter().any(|((s, w), st)| s == "neu-daten" && w == "zeitplan" && st.folge == n)
        })
        .await;
    }
    let app = router_angemeldet(state.clone());
    let (_, e) = schicke(&app, get_req("/api/pruefung/neu-daten")).await;
    let text = e["befunde"].to_string();
    assert!(text.contains("zeitplan: abgestürzt: Absicht im Zeitplan") && text.contains("wiederholt, 3-mal hintereinander"), "{e}");
    assert_eq!(schicke(&app, get_req("/healthz")).await.0, StatusCode::OK);
    // nach_lesen mit Absturz: die Daten kommen unverarbeitet, dazu ein Fund.
    let (status, v) = schicke(&app, get_req("/api/projekte/neu-daten/daten/gantt")).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert!(v["inhalt"].get("vom_modul").is_none(), "{v}");
    assert!(hat_stoerung(&state, "nach_lesen"));
    // Ein guter nach_lesen-Lauf verarbeitet die Daten (Middleware des Projekts).
    let (status, v) = schicke(&app, get_req("/api/projekte/neu-daten/daten/fortschritt")).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["inhalt"]["vom_modul"], true, "{v}");
    assert!(v["inhalt"]["zeile"].is_array() && v["abgeleitet"].is_object(), "{v}");
}

#[tokio::test]
async fn schreibende_wege_nur_mit_anmeldung_herkunft_und_64_kib() {
    let state = zustand_mit(vec![eintrag(Probe)]);
    let klein = json!({ "text": "x".repeat(20 * 1024) }).to_string();
    // Ohne Anmeldung: 401 (seit Etappe B2 auch lesend, siehe unten).
    #[cfg(windows)]
    {
        let (status, v) = schicke(&router(state.clone()), post_req("/api/projekte/neu-daten/x/schreiben", klein.clone(), true)).await;
        assert_eq!((status, v["art"].as_str()), (StatusCode::UNAUTHORIZED, Some("anmeldung")), "{v}");
    }
    let app = router_angemeldet(state.clone());
    // Ohne Herkunft (wie curl): 403.
    let (status, _) = schicke(&app, post_req("/api/projekte/neu-daten/x/schreiben", "{}".into(), false)).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    // 20 KiB gehen (mehr als die 16 KB der anderen Wege), 70 KiB nicht.
    let (status, v) = schicke(&app, post_req("/api/projekte/neu-daten/x/schreiben", klein.clone(), true)).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let gross = json!({ "text": "x".repeat(70 * 1024) }).to_string();
    let (status, v) = schicke(&app, post_req("/api/projekte/neu-daten/x/schreiben", gross, true)).await;
    assert_eq!((status, v["art"].as_str()), (StatusCode::PAYLOAD_TOO_LARGE, Some("zu-gross")), "{v}");
    // Die anderen Wege behalten ihre 16 KB.
    let (status, _) = schicke(&app, post_req("/api/vorgaenge", klein, true)).await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE);
    assert!(crate::sicherheit::modul_weg("/api/projekte/a/x") && crate::sicherheit::modul_weg("/api/projekte/a/x/b"));
    assert!(!crate::sicherheit::modul_weg("/api/projekte/a/daten/x") && !crate::sicherheit::modul_weg("/api/projekte//x"));
}

/// Ein Modul mit einem erklärten freien Weg (Etappe B2).
struct MitFreiemWeg;

impl Modul for MitFreiemWeg {
    fn herkunft(&self) -> dashboard_projekt::Herkunft {
        dashboard_projekt::herkunft!()
    }
    fn slug(&self) -> &'static str {
        "neu-form"
    }
    fn router(&self) -> Router<ProjektKontext> {
        Router::new()
            .route("/zahlen", get(|| async { Json(json!({ "offen": 3 })) }))
            .route("/stufe/{nr}", get(|| async { Json(json!({ "stufe": 1 })) }))
            .route("/geheim", get(|| async { Json(json!({ "titel": "nur mit Anmeldung" })) }))
    }
    fn freie_wege(&self) -> &'static [&'static str] {
        &["/zahlen", "/stufe/{nr}"]
    }
}

/// Etappe B2: Modul-Wege sind standardmäßig geschützt, auch GET. Frei ist nur,
/// was das Modul in `freie_wege` erklärt; ein Modul ohne Erklärung hat keinen freien Weg.
#[tokio::test]
async fn modul_wege_geschuetzt_ausser_erklaerten() {
    let state = zustand_mit(vec![eintrag(Probe), eintrag(MitFreiemWeg)]);
    let frei = router(state.clone());
    let an = router_angemeldet(state.clone());
    for pfad in ["/api/projekte/neu-form/x/zahlen", "/api/projekte/neu-form/x/stufe/2"] {
        assert_eq!(schicke(&frei, get_req(pfad)).await.0, StatusCode::OK, "{pfad} ist erklärt frei");
    }
    #[cfg(windows)]
    for pfad in [
        "/api/projekte/neu-form/x/geheim",
        "/api/projekte/neu-form/x/stufe",
        "/api/projekte/neu-form/x/stufe/2/mehr",
        "/api/projekte/neu-form/x/gibt-es-nicht",
        "/api/projekte/neu-daten/x/gut",
    ] {
        let (status, v) = schicke(&frei, get_req(pfad)).await;
        assert_eq!((status, v["art"].as_str()), (StatusCode::UNAUTHORIZED, Some("anmeldung")), "{pfad}: {v}");
    }
    assert_eq!(schicke(&an, get_req("/api/projekte/neu-form/x/geheim")).await, (StatusCode::OK, json!({ "titel": "nur mit Anmeldung" })));
    assert_eq!(schicke(&an, get_req("/api/projekte/neu-daten/x/gut")).await, (StatusCode::OK, json!({ "slug": "neu-daten" })));
    // Das Muster: `{…}` ist genau ein nicht leerer Abschnitt.
    assert!(crate::projekte::weg_passt("/stufe/{nr}", "/stufe/7") && !crate::projekte::weg_passt("/stufe/{nr}", "/stufe/"));
    assert!(!crate::projekte::weg_passt("/zahlen", "/zahlenx") && crate::projekte::weg_passt("/zahlen", "/zahlen/"));
    assert!(state.projekte.frei_lesbar("/api/projekte/neu-form/x/zahlen") && !state.projekte.frei_lesbar("/api/projekte/neu-form/xzahlen"));
    assert!(!state.projekte.frei_lesbar("/api/projekte/neu-daten/x/gut"), "ohne Erklärung nichts frei");
}

/// Hotfix Prozent, zweite Wand der Modul-Wege: Auch ohne die Schleusen davor
/// (nur der Router der Module) geht ohne `Angemeldet` nur ein erklärt freier
/// Weg; kodierte, doppelt kodierte und schreibende Wege bekommen 401.
#[tokio::test]
async fn modul_wand_ohne_schleusen() {
    let state = zustand_mit(vec![eintrag(Probe), eintrag(MitFreiemWeg)]);
    let nackt: Router = state.projekte.router::<()>();
    let hole = |pfad: &str| Request::get(pfad).body(Body::empty()).unwrap();
    for pfad in ["/api/projekte/neu-form/x/zahlen", "/api/projekte/neu-form/x/stufe/2"] {
        let res = nackt.clone().oneshot(hole(pfad)).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK, "{pfad}");
    }
    for pfad in [
        "/api/projekte/neu-form/x/geheim",
        "/api/projekte/neu-form/x/%67eheim",
        "/api/projekte/neu-form/x/%7Aahlen",
        "/api/projekte/neu-form/x/stufe/%252",
        "/api/projekte/neu-form/x/stufe/%zz",
        "/api/projekte/neu-daten/x/gut",
    ] {
        let res = nackt.clone().oneshot(hole(pfad)).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{pfad}");
        let v: Value = serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert_eq!(v["art"], "anmeldung", "{pfad}: {v}");
    }
    let schreiben = Request::post("/api/projekte/neu-form/x/zahlen").body(Body::empty()).unwrap();
    assert_eq!(nackt.clone().oneshot(schreiben).await.unwrap().status(), StatusCode::UNAUTHORIZED);
    let mut angemeldet = hole("/api/projekte/neu-form/x/geheim");
    angemeldet.extensions_mut().insert(crate::anmeldung::Angemeldet);
    assert_eq!(nackt.clone().oneshot(angemeldet).await.unwrap().status(), StatusCode::OK);
    assert!(modul_weg_frei(&["/zahlen", "/stufe/{nr}"], "/stufe/%32") && !modul_weg_frei(&["/zahlen"], "/%7Aahlen"));
    assert!(!modul_weg_frei(&["/stufe/{nr}"], "/stufe/%2532") && !modul_weg_frei(&["/stufe/{nr}"], "/stufe/%zz"));
}

/// Hooks aus Beobachter (entprellt), Vorgängen (nur eigene), Bau und Takt.
#[tokio::test]
async fn hooks_aus_beobachter_vorgaengen_bau_und_takt() {
    let state = zustand_mit(vec![eintrag(Probe)]);
    state.projekte.starten(&state);
    // Drei Änderungen kurz hintereinander: ein Lauf, 2 s nach der letzten.
    let start = Instant::now();
    let aenderung = |pfad: &str| crate::api::events::Change { area: "projects", path: pfad.into(), at: chrono::Local::now() };
    for _ in 0..3 {
        let _ = state.events.send(aenderung("projects/neu-daten/daten/fortschritt.toml"));
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // Andere Projekte und andere Dateien zählen nicht.
    let _ = state.events.send(aenderung("projects/neu-form/daten/gantt.toml"));
    let _ = state.events.send(aenderung("projects/neu-daten/projekt.toml"));
    bis("nach_daten_geaendert", Duration::from_secs(6), || protokoll(&state, "geaendert/fortschritt").is_some()).await;
    assert!(start.elapsed() >= Duration::from_secs(2), "entprellt: {:?}", start.elapsed());
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert_eq!(protokoll(&state, "geaendert/fortschritt"), Some(json!(1)), "nur ein Lauf");
    // Vorgang für dieses Projekt: bei_vorgang; für ein anderes nicht.
    let app = router_angemeldet(state.clone());
    let neu = |slug: &str| post_req("/api/vorgaenge", format!(r#"{{"art": "abnahme", "slug": "{slug}", "titel": "t"}}"#), false);
    let (status, v) = schicke(&app, neu("neu-daten")).await;
    assert_eq!(status, StatusCode::CREATED, "{v}");
    let id = v["vorgang"]["id"].as_str().unwrap().to_string();
    let (_, w) = schicke(&app, neu("anderes")).await;
    let fremd = w["vorgang"]["id"].as_str().unwrap().to_string();
    bis("bei_vorgang", Duration::from_secs(5), || protokoll(&state, &format!("vorgang/{id}/offen")).is_some()).await;
    assert_eq!(protokoll(&state, &format!("vorgang/{id}/offen")), Some(json!("abnahme")));
    assert_eq!(protokoll(&state, &format!("vorgang/{fremd}/offen")), None, "fremde Vorgänge nicht");
    // nach_bau.
    state.projekte.nach_bau(false);
    bis("nach_bau", Duration::from_secs(5), || protokoll(&state, "bau").is_some()).await;
    assert_eq!(protokoll(&state, "bau"), Some(json!("rot")));
    // taeglich erst nach der Sicherung, dann einmal am Tag.
    let jetzt = Instant::now();
    state.projekte.takt(jetzt, "2026-10-30", false);
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(protokoll(&state, "taeglich"), None, "vor der Sicherung nicht");
    state.projekte.takt(jetzt, "2026-10-30", true);
    state.projekte.takt(jetzt, "2026-10-30", true);
    bis("taeglich", Duration::from_secs(5), || protokoll(&state, "taeglich").is_some()).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(protokoll(&state, "taeglich"), Some(json!(1)));
    state.projekte.takt(jetzt, "2026-10-31", true);
    bis("taeglich am nächsten Tag", Duration::from_secs(5), || protokoll(&state, "taeglich") == Some(json!(2))).await;
    state.abschalten_ausloesen();
}

/// Der Namensraum steht auf /daten, im Export und in der Sicherung.
#[tokio::test]
async fn namensraum_in_daten_export_und_sicherung() {
    let state = zustand_mit(vec![eintrag(Probe)]);
    state.projekte.takt(Instant::now(), "2026-10-30", true);
    bis("taeglich", Duration::from_secs(5), || protokoll(&state, "taeglich").is_some()).await;
    let app = router_angemeldet(state.clone());
    let (status, v) = schicke(&app, get_req("/api/daten")).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let t = v["tabellen"].as_array().unwrap().iter().find(|t| t["name"] == "projekt/neu-daten/protokoll").cloned();
    assert_eq!(t.map(|t| t["eintraege"].clone()), Some(json!(1)), "{v}");
    let (status, export) = schicke(&app, get_req("/api/daten/export?format=json")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(export["projekt/neu-daten/protokoll"]["taeglich"], 1, "{export}");
    let s = state.speicher().unwrap();
    let sicherung = s.sichern().unwrap();
    let db = redb::ReadOnlyDatabase::open(&sicherung.datei).unwrap();
    use redb::{ReadableDatabase, ReadableTableMetadata};
    let def = crate::speicher::projekte::definition("projekt/neu-daten/protokoll");
    assert_eq!(db.begin_read().unwrap().open_table(def).unwrap().len().unwrap(), 1);
}

static LAEUFT: AtomicUsize = AtomicUsize::new(0);
static HOECHSTENS: AtomicUsize = AtomicUsize::new(0);
static LAEUFE: AtomicUsize = AtomicUsize::new(0);

struct Langsam;

fn langsam(_: &ProjektKontext) -> Result<(), Fehler> {
    let jetzt = LAEUFT.fetch_add(1, Ordering::SeqCst) + 1;
    HOECHSTENS.fetch_max(jetzt, Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(500));
    LAEUFT.fetch_sub(1, Ordering::SeqCst);
    LAEUFE.fetch_add(1, Ordering::SeqCst);
    Ok(())
}

impl Modul for Langsam {
    fn herkunft(&self) -> dashboard_projekt::Herkunft {
        dashboard_projekt::herkunft!()
    }
    fn slug(&self) -> &'static str {
        "neu-form"
    }
    fn router(&self) -> Router<ProjektKontext> {
        Router::new()
    }
    fn hooks(&self) -> Hooks {
        Hooks { zeitplan: Some(Zeitplan { alle: Duration::from_secs(300), lauf: langsam }), ..Hooks::default() }
    }
}

/// Zeitlimit: Fund „Zeitlimit“; höchstens ein Lauf zugleich, wartende gleiche
/// Aufrufe zusammengelegt; der Takt löst den Zeitplan höchstens alle 5 Minuten aus.
#[tokio::test]
async fn zeitlimit_und_hoechstens_ein_lauf() {
    let state = zustand_mit(vec![eintrag(Langsam)]);
    state.projekte.ausloesen("neu-form", Aufruf::Zeitplan);
    bis("erster Lauf", Duration::from_secs(5), || LAEUFT.load(Ordering::SeqCst) == 1).await;
    for _ in 0..3 {
        state.projekte.ausloesen("neu-form", Aufruf::Zeitplan);
    }
    bis("zwei Läufe", Duration::from_secs(5), || LAEUFE.load(Ordering::SeqCst) >= 2).await;
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(LAEUFE.load(Ordering::SeqCst), 2, "einer läuft, einer wartet, gleiche werden zusammengelegt");
    assert_eq!(HOECHSTENS.load(Ordering::SeqCst), 1, "nie zwei zugleich");
    let st = state.projekte.stoerungen().alle();
    assert!(st.iter().any(|((s, w), _)| s == "neu-form" && w == "zeitplan (Zeitlimit)"), "{st:?}");
    let t0 = Instant::now();
    state.projekte.takt(t0, "2026-10-30", true);
    state.projekte.takt(t0 + Duration::from_secs(60), "2026-10-30", true);
    bis("dritter Lauf", Duration::from_secs(5), || LAEUFE.load(Ordering::SeqCst) >= 3).await;
    tokio::time::sleep(Duration::from_millis(700)).await;
    assert_eq!(LAEUFE.load(Ordering::SeqCst), 3);
}

#[test]
fn register_laesst_ungueltige_und_doppelte_aus() {
    struct Falsch;
    impl Modul for Falsch {
        fn herkunft(&self) -> dashboard_projekt::Herkunft {
            dashboard_projekt::herkunft!()
        }
        fn slug(&self) -> &'static str {
            "Gross"
        }
        fn router(&self) -> Router<ProjektKontext> {
            Router::new()
        }
    }
    let s = AppState::fuer_tests(&beispiel_fuer_tests());
    let reg = Register::fuer_zustand_ungeprueft(&s, vec![eintrag(Probe), eintrag(Probe), eintrag(Falsch)]);
    assert_eq!(reg.slugs(), ["neu-daten"]);
    let st = reg.stoerungen().alle();
    assert!(st.iter().any(|((s, w), _)| s == "neu-daten" && w == "register") && st.iter().any(|((s, _), _)| s == "Gross"), "{st:?}");
    // Das allgemeine dashboard.exe (und jeder Test) läuft ohne angemeldete
    // Module; angemeldet wird nur über `starten` ([zusammenbau]).
    assert!(module().is_empty());
}

/// Die Tabellen des Vorlagenmoduls lassen sich im echten Datenspeicher einrichten.
#[test]
fn vorlagenmodul_tabellen_im_datenspeicher() {
    let s = AppState::fuer_tests(&beispiel_fuer_tests());
    let reg = Register::fuer_zustand_ungeprueft(&s, vec![eintrag(projekt_vorlage::Vorlage)]);
    reg.tabellen_einrichten(s.speicher.as_ref().unwrap());
    assert!(reg.stoerungen().alle().is_empty(), "{:?}", reg.stoerungen().alle());
    let info = s.speicher().unwrap().tabellen_info().unwrap();
    assert!(info.iter().any(|(n, _, _)| n == "projekt/vorlage/etappen"), "{info:?}");
}

/// Ein neutrales Beispielmodul, als stamme es aus der Kiste
/// `projekt-beispiel` in `projects/beispiel/server` (wie eine Kiste der
/// Inhaltsquelle, die eine Zusammenbau-Kiste anmeldet).
struct Beispiel;

impl Modul for Beispiel {
    fn herkunft(&self) -> dashboard_projekt::Herkunft {
        dashboard_projekt::Herkunft::__aus_bau("projekt-beispiel", "C:/inhalt/projects/beispiel/server")
    }
    fn slug(&self) -> &'static str {
        "beispiel"
    }
    fn router(&self) -> Router<ProjektKontext> {
        Router::new()
    }
}

/// Herkunft: slug() passt zum Ordner projects/<slug>/server und zur Kiste
/// projekt-<slug>, beides zur Bauzeit festgehalten; sonst wird das Modul nicht
/// eingehängt (Fund im Register). Ein anderer Projektordner (Testserver,
/// DASHBOARD_PROJECTS_DIR) ändert daran nichts.
#[test]
fn modul_passt_zu_ordner_und_kiste() {
    // Die echte Kiste der Vorlage: Kiste und Ordner zur Bauzeit.
    let echt = projekt_vorlage::Vorlage.herkunft();
    assert_eq!((echt.kiste(), echt.ordner().as_str()), ("projekt-vorlage", "projects/_vorlage/server"));
    // Der Ordner _vorlage passt zu keinem Slug: die Vorlage wird nie eingehängt.
    assert!(herkunft_pruefen("vorlage", "projekt_vorlage", echt).unwrap_err().contains("liegt in projects/_vorlage/server"));
    // Fremder slug() mit der Herkunft der Kiste der Vorlage: abgelehnt.
    assert!(herkunft_pruefen("probe", "projekt_vorlage", echt).unwrap_err().contains("steht in der Kiste projekt-vorlage"));
    // Das neutrale Beispiel passt; ein Typ aus einer anderen Kiste nicht.
    let beispiel = Beispiel.herkunft();
    assert_eq!(herkunft_pruefen("beispiel", "projekt_beispiel", beispiel), Ok(()));
    assert!(herkunft_pruefen("beispiel", "projekt_andere", beispiel).unwrap_err().contains("stammt aus der Kiste projekt_andere"));
    // Kiste und Ordner passen nicht zusammen (Testmodul der Kiste dashboard).
    let fremd = Probe.herkunft();
    assert!(herkunft_pruefen("neu-daten", "projekt_neu_daten", fremd).is_err());
    let mut s = AppState::fuer_tests(&beispiel_fuer_tests());
    let reg = Register::fuer_zustand(&s, vec![eintrag(Probe)]);
    assert!(reg.slugs().is_empty());
    assert!(reg.stoerungen().alle().iter().any(|((sl, w), st)| sl == "neu-daten" && w == "register" && st.satz.contains("Kiste dashboard")));
    // Ein ganz anderer Projektordner (ohne projects/beispiel): das Modul bleibt eingehängt.
    let ordner = std::env::temp_dir().join(format!("dashboard-herkunft-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&ordner);
    s = s.mit_projekten(ordner.clone());
    let reg = Register::fuer_zustand(&s, vec![ModulEintrag { modul: Arc::new(Beispiel), kiste: "projekt_beispiel" }]);
    assert_eq!(reg.slugs(), ["beispiel"], "{:?}", reg.stoerungen().alle());
    let _ = std::fs::remove_dir_all(&ordner);
}
