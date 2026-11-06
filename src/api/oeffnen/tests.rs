//! Tests von `POST /api/oeffnen` (Entscheidung lokal-oeffnen,
//! `[form].tests`): der ganze Weg durch die Schleusen, mit dem echten Code
//! des Boten (`bote::aufgaben::ausfuehren`) und einer Aufnahme statt
//! ShellExecute und Explorer. Nichts wird geöffnet.

use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use axum::{Extension, Router};
use http_body_util::BodyExt;
use tower::ServiceExt;

use super::*;
use crate::anmeldung::verbindung::Verbindung;
use crate::app::{beispiel_fuer_tests, router, router_angemeldet};
use crate::bote::oeffnen::{Aufnahme, Aufruf};
use crate::oeffnen::VERBOTENE_ENDUNGEN;
use crate::oeffnen::tests::Platz;
use crate::vertrag::api::VorgangsArt;

const HERKUNFT: &str = "http://localhost:8787";

/// Spielplatz, Projekt `probe` mit `daten/orte.toml` in einer eigenen
/// Inhaltsquelle, Regeln des Servers und Bote mit Aufnahme.
struct Aufbau {
    platz: Platz,
    state: AppState,
    aufnahme: Arc<Aufnahme>,
    _projekte: Projekte,
}

/// Der Projektordner wird am Ende gelöscht.
struct Projekte(PathBuf);
impl Drop for Projekte {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn ort(kennung: &str, pfad: &str) -> String {
    format!("[[ort]]\nkennung = \"{kennung}\"\ntitel = \"Ort {kennung}\"\npfad = '{pfad}'\n\n")
}

fn aufbau(name: &str) -> Aufbau {
    let platz = Platz::neu(&format!("api-{name}"));
    let projekte = platz.quelle.join("projekte");
    let probe = projekte.join("probe");
    std::fs::create_dir_all(probe.join("daten")).unwrap();
    std::fs::write(probe.join("projekt.toml"), "schema = 3\ntitle = \"Probe\"\nstatus = \"aktiv\"\nsummary = \"Für den Test.\"\n").unwrap();
    let mut orte = String::new();
    orte += &ort("bauplan", &platz.dok("bauplan.pdf"));
    orte += &ort("kiste", &platz.dok("Werkzeugkiste"));
    orte += &ort("archiv", &platz.dok("archiv.zip"));
    orte += &ort("notiz", &platz.text(&platz.quelle.join("notiz.md")));
    for e in VERBOTENE_ENDUNGEN {
        orte += &ort(&format!("endung-{}", e.replace('-', "")), &platz.dok(&format!("datei.{e}")));
        orte += &ort(&format!("gross-{}", e.replace('-', "")), &platz.dok(&format!("GROSS.{}", e.to_uppercase())));
    }
    orte += &ort("doppelt", &platz.dok("x.pdf.exe"));
    orte += &ort("ohne-endung", &platz.dok("ohne_endung"));
    orte += &ort("eltern", &format!("{}\\..\\Dokumente\\bauplan.pdf", platz.text(&platz.profil.join("Dokumente"))));
    orte += &ort("unc", r"\\server\freigabe\bauplan.pdf");
    orte += &ort("geraet", &format!(r"\\?\{}", platz.dok("bauplan.pdf")));
    orte += &ort("geraet-punkt", &format!(r"\\.\{}", platz.dok("bauplan.pdf")));
    orte += &ort("strom", &format!("{}:versteckt", platz.dok("bauplan.pdf")));
    orte += &ort("tabu", &platz.text(&platz.profil.join("Geheim Ablage").join("brief.pdf")));
    orte += &ort("tabu-anders", &platz.text(&platz.profil.join("GEHEIM_ABLAGE").join("brief.pdf")));
    orte += &ort("appdata", &platz.text(&platz.profil.join("AppData").join("Local").join("x.pdf")));
    orte += &ort("draussen", &platz.text(&platz.draussen.join("fremd.pdf")));
    orte += &ort("junction", &platz.text(&platz.profil.join("sprung").join("fremd.pdf")));
    orte += &ort("junction-tabu", &platz.text(&platz.profil.join("harmlos").join("brief.pdf")));
    orte += &ort("fehlt", &platz.dok("gibt-es-nicht.pdf"));
    orte += &ort("svg", &platz.dok("bild.svg"));
    std::fs::write(probe.join("daten").join("orte.toml"), orte).unwrap();
    let state = AppState::fuer_tests(&beispiel_fuer_tests()).mit_projekten(projekte.clone());
    *state.oeffnen.regeln.lock().unwrap() = Some(crate::oeffnen::Regeln { quellen: vec![platz.quelle.clone()], ..platz.regeln() });
    let aufnahme = Arc::new(Aufnahme::default());
    let k = crate::bote::oeffnen::tests::kontext(&platz, aufnahme.clone());
    let ersatz: crate::oeffnen::Ersatz = Arc::new(move |a| {
        let k = k.clone();
        Box::pin(async move { crate::bote::aufgaben::ausfuehren_von(a, &k, Some(crate::bote::oeffnen::tests::DIENST.into())).await })
    });
    *state.oeffnen.ersatz.lock().unwrap() = Some(ersatz);
    Aufbau { platz, state, aufnahme, _projekte: Projekte(projekte) }
}

fn post(koerper: &str) -> Request<Body> {
    Request::post("/api/oeffnen")
        .header(header::HOST, "localhost:8787")
        .header(header::ORIGIN, HERKUNFT)
        .header("sec-fetch-site", "same-origin")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(koerper.to_string()))
        .unwrap()
}

fn oeffne(kennung: &str) -> Request<Body> {
    post(&format!(r#"{{"projekt": "probe", "kennung": "{kennung}"}}"#))
}

async fn schicke(app: Router, req: Request<Body>) -> (StatusCode, serde_json::Value) {
    let res = app.oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
}

/// Erlaubte Datei und erlaubter Ordner: geöffnet (mit dem Ersatz), je ein
/// Eintrag im Journal mit Projekt und Kennung, ohne Pfad; kein Chat geweckt.
#[tokio::test]
async fn erlaubte_datei_und_ordner_geoeffnet() {
    let a = aufbau("erlaubt");
    for kennung in ["bauplan", "kiste", "archiv", "notiz"] {
        let (s, v) = schicke(router_angemeldet(a.state.clone()), oeffne(kennung)).await;
        assert_eq!(s, StatusCode::OK, "{kennung}: {v}");
        assert_eq!(v["satz"], SATZ_GEOEFFNET);
        let text = v.to_string();
        assert!(!text.contains(r"\\") && !text.contains("Dokumente"), "kein Pfad in der Antwort: {text}");
    }
    let aufrufe = a.aufnahme.alle();
    assert_eq!(aufrufe.len(), 4, "{aufrufe:?}");
    assert!(matches!(&aufrufe[0], Aufruf::Datei(s) if s.verb == "open" && s.parameter.is_none()));
    assert!(matches!(&aufrufe[1], Aufruf::Ordner { argument, .. } if argument.ends_with(r#"Werkzeugkiste\.""#)));
    assert!(matches!(&aufrufe[2], Aufruf::Ordner { .. }), "zip im Explorer");
    assert!(matches!(&aufrufe[3], Aufruf::Datei(_)), "Datei in der Inhaltsquelle");
    // Journal: Vorgang `oeffnen`, erledigt, Bezug ort:probe/<kennung>, kein Pfad.
    let sp = a.state.speicher.clone().unwrap();
    let v = sp.vorgaenge_fuer("probe", false).unwrap();
    assert_eq!(v.len(), 4, "{v:?}");
    for x in &v {
        assert_eq!((x.art, x.zustand, x.von.as_str()), (VorgangsArt::Oeffnen, crate::vertrag::api::Zustand::Erledigt, "browser"));
        assert!(x.bezug.as_deref().is_some_and(|b| b.starts_with("ort:probe/")), "{x:?}");
        let json = serde_json::to_string(x).unwrap();
        assert!(!json.contains("Dokumente") && !json.contains(r"\\"), "{json}");
    }
    // Kein Chat wartet darauf (nie offen), und keine Meldung.
    assert!(sp.vorgaenge_fuer("probe", true).unwrap().is_empty());
    let (_, m) =
        schicke(router_angemeldet(a.state.clone()), Request::get("/api/meldungen").header(header::HOST, "localhost:8787").body(Body::empty()).unwrap()).await;
    assert!(!m.to_string().contains("probe"), "keine Meldung: {m}");
    drop(a.platz);
}

/// Pfad aus dem Körper statt Kennung: 422; unbekannte Kennung oder Projekt: 404.
#[tokio::test]
async fn pfad_im_koerper_422_unbekannt_404() {
    let a = aufbau("koerper");
    let pfad = a.platz.dok("bauplan.pdf").replace('\\', "\\\\");
    for k in [
        format!(r#"{{"projekt": "probe", "pfad": "{pfad}"}}"#),
        format!(r#"{{"projekt": "probe", "kennung": "bauplan", "pfad": "{pfad}"}}"#),
        format!(r#"{{"projekt": "probe", "kennung": "{pfad}"}}"#),
        r#"{"projekt": "probe", "kennung": "../bauplan"}"#.to_string(),
        r#"{"projekt": "../probe", "kennung": "bauplan"}"#.to_string(),
        r#"{"projekt": "probe"}"#.to_string(),
        "kein json".to_string(),
    ] {
        let (s, v) = schicke(router_angemeldet(a.state.clone()), post(&k)).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{k}: {v}");
    }
    for (projekt, kennung) in [("probe", "gibt-es-nicht"), ("gibt-es-nicht", "bauplan")] {
        let (s, v) = schicke(router_angemeldet(a.state.clone()), post(&format!(r#"{{"projekt": "{projekt}", "kennung": "{kennung}"}}"#))).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{projekt}/{kennung}: {v}");
    }
    // Den Ort gibt es nicht (mehr): ein Satz.
    let (s, v) = schicke(router_angemeldet(a.state.clone()), oeffne("fehlt")).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    assert_eq!(v["fehler"].as_str().unwrap(), crate::oeffnen::SATZ_NICHT_DA, "{v}");
    assert!(a.aufnahme.alle().is_empty());
}

/// Jede verbotene Endung einzeln, groß, doppelt, ohne Endung; Junction hinaus,
/// .., UNC, Gerätepfad, Datenstrom, Tabu (auch anders geschrieben), AppData:
/// abgewiesen (was am Text liegt, schon beim Server; Endung, Junction und
/// „gibt es nicht“ beim Boten), nichts geöffnet, kein Pfad im Satz.
#[tokio::test]
async fn server_weist_ab() {
    let a = aufbau("server-ab");
    let mut kennungen: Vec<String> = Vec::new();
    for e in VERBOTENE_ENDUNGEN {
        kennungen.push(format!("endung-{}", e.replace('-', "")));
        kennungen.push(format!("gross-{}", e.replace('-', "")));
    }
    kennungen
        .extend(["doppelt", "ohne-endung", "eltern", "unc", "geraet", "geraet-punkt", "strom", "tabu", "tabu-anders", "appdata", "draussen"].map(String::from));
    if a.platz.junctions || !crate::ueberspringen("mklink /J ging nicht") {
        kennungen.push("junction".into());
        kennungen.push("junction-tabu".into());
    }
    for k in &kennungen {
        a.state.oeffnen.grenze.leeren();
        let (s, v) = schicke(router_angemeldet(a.state.clone()), oeffne(k)).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{k}: {v}");
        let satz = v["fehler"].as_str().unwrap();
        assert!(!satz.contains(r"\Users") && !satz.to_lowercase().contains("geheim"), "{k}: {satz}");
    }
    assert!(a.aufnahme.alle().is_empty(), "{:?}", a.aufnahme.alle());
    assert!(a.state.speicher.clone().unwrap().vorgaenge_fuer("probe", false).unwrap().is_empty(), "nichts im Journal");
}

/// Dieselben Fälle beim Boten: Der Server lässt hier (für den Test) alles
/// durch, was die Form erlaubt (ohne Tabu-Liste, mit allen Endungen und
/// ohne Wurzelgrenze); der Bote weist trotzdem ab und öffnet nichts.
#[tokio::test]
async fn bote_weist_ab_auch_wenn_der_server_durchlaesst() {
    let a = aufbau("bote-ab");
    let mut alle_endungen: Vec<String> = crate::oeffnen::ENDUNGEN.iter().map(|e| e.to_string()).collect();
    alle_endungen.extend(VERBOTENE_ENDUNGEN.iter().map(|e| e.to_string()));
    alle_endungen.push(String::new());
    *a.state.oeffnen.regeln.lock().unwrap() = Some(crate::oeffnen::Regeln {
        tabu: None,
        tabu_pflicht: false,
        endungen: alle_endungen,
        profil: Some(a.platz.wurzel.clone()),
        quellen: vec![a.platz.wurzel.clone(), a.platz.quelle.clone()],
        verboten: Vec::new(),
        explorer_endungen: vec!["zip".into()],
        eingebaut: Vec::new(),
    });
    let mut kennungen: Vec<String> = Vec::new();
    for e in VERBOTENE_ENDUNGEN {
        kennungen.push(format!("endung-{}", e.replace('-', "")));
        kennungen.push(format!("gross-{}", e.replace('-', "")));
    }
    // Der Server prüft nur den Text; auch die andere Schreibweise weist erst der Bote ab.
    // (Sie gibt es nicht; der Satz ist derselbe wie für „Tabu“.)
    kennungen.extend(["doppelt", "tabu", "tabu-anders", "draussen", "ohne-endung"].map(String::from));
    if a.platz.junctions || !crate::ueberspringen("mklink /J ging nicht") {
        kennungen.push("junction".into());
        kennungen.push("junction-tabu".into());
    }
    for k in &kennungen {
        a.state.oeffnen.grenze.leeren();
        let (s, v) = schicke(router_angemeldet(a.state.clone()), oeffne(k)).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{k}: {v}");
    }
    assert!(a.aufnahme.alle().is_empty(), "{:?}", a.aufnahme.alle());
}

/// Ohne Anmeldung 401, GET 405, ohne Herkunft (curl) und von fremder Seite 403,
/// ohne JSON 415; nichts geöffnet.
#[tokio::test]
async fn schleusen_401_405_403_415() {
    let a = aufbau("schleusen");
    let ohne = router(a.state.clone()).layer(Extension(ConnectInfo(Verbindung::neu())));
    let (s, _) = schicke(ohne, oeffne("bauplan")).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let get = Request::get("/api/oeffnen").header(header::HOST, "localhost:8787").body(Body::empty()).unwrap();
    let (s, _) = schicke(router_angemeldet(a.state.clone()), get).await;
    assert_eq!(s, StatusCode::METHOD_NOT_ALLOWED);
    let get_link = Request::get("/api/oeffnen?projekt=probe&kennung=bauplan").header(header::HOST, "localhost:8787").body(Body::empty()).unwrap();
    let (s, _) = schicke(router_angemeldet(a.state.clone()), get_link).await;
    assert_eq!(s, StatusCode::METHOD_NOT_ALLOWED, "nie aus einem Link");
    let curl = Request::post("/api/oeffnen")
        .header(header::HOST, "localhost:8787")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"projekt": "probe", "kennung": "bauplan"}"#))
        .unwrap();
    let (s, _) = schicke(router_angemeldet(a.state.clone()), curl).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "ohne Herkunft");
    let fremd = Request::post("/api/oeffnen")
        .header(header::HOST, "localhost:8787")
        .header(header::ORIGIN, "http://evil.example")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"projekt": "probe", "kennung": "bauplan"}"#))
        .unwrap();
    let (s, _) = schicke(router_angemeldet(a.state.clone()), fremd).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "fremde Seite");
    let quer = Request::post("/api/oeffnen")
        .header(header::HOST, "localhost:8787")
        .header(header::ORIGIN, HERKUNFT)
        .header("sec-fetch-site", "cross-site")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"projekt": "probe", "kennung": "bauplan"}"#))
        .unwrap();
    let (s, _) = schicke(router_angemeldet(a.state.clone()), quer).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "cross-site");
    let text = Request::post("/api/oeffnen")
        .header(header::HOST, "localhost:8787")
        .header(header::ORIGIN, HERKUNFT)
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from(r#"{"projekt": "probe", "kennung": "bauplan"}"#))
        .unwrap();
    let (s, _) = schicke(router_angemeldet(a.state.clone()), text).await;
    assert_eq!(s, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    assert!(a.aufnahme.alle().is_empty());
}

/// Ein fremdes Windows-Konto ohne Passkey: 401 wie überall; mit dem Passkey
/// des Inhabers aus dem fremden Konto: 403 mit dem Satz der Entscheidung.
#[cfg(windows)]
#[tokio::test]
async fn passkey_aus_fremdem_konto_403() {
    use crate::anmeldung::WindowsKonto;
    use crate::anmeldung::verbindung::merken;
    use crate::api::passkey::tests::{anlegen, anmelden};
    use crate::passkey::webauthn::pruef_authenticator::PruefAuthenticator;
    let a = aufbau("passkey");
    let fremd = || {
        let v = Verbindung::neu();
        merken(v.nr, WindowsKonto::test(false));
        router(a.state.clone()).layer(Extension(ConnectInfo(v)))
    };
    let (s, _) = schicke(fremd(), oeffne("bauplan")).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "fremdes Konto ohne Passkey");
    let mut inhaber = PruefAuthenticator::es256();
    anlegen(&router_angemeldet(a.state.clone()), &inhaber).await;
    let anders = fremd();
    let (_, ausweis) = anmelden(&anders, &mut inhaber, HERKUNFT).await;
    let ausweis = ausweis.expect("Ausweis nach Anmeldung mit Passkey");
    let mut req = oeffne("bauplan");
    req.headers_mut().insert(header::COOKIE, format!("{}={ausweis}", crate::passkey::cookie::NAME).parse().unwrap());
    let (s, v) = schicke(anders, req).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{v}");
    assert_eq!(v["fehler"], SATZ_EIGENES_KONTO);
    assert!(a.aufnahme.alle().is_empty());
}

/// Mehr als 10 je Minute: 429 mit Satz, die elfte wird nicht geöffnet.
#[tokio::test]
async fn mehr_als_zehn_je_minute_429() {
    let a = aufbau("grenze");
    for i in 0..10 {
        let (s, v) = schicke(router_angemeldet(a.state.clone()), oeffne("bauplan")).await;
        assert_eq!(s, StatusCode::OK, "{i}: {v}");
    }
    let (s, v) = schicke(router_angemeldet(a.state.clone()), oeffne("bauplan")).await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS, "{v}");
    assert_eq!(v["fehler"], SATZ_GRENZE);
    assert_eq!(a.aufnahme.alle().len(), 10);
}

/// Was am Text liegt (.., UNC, Gerätepfad, Datenstrom, Tabu, AppData,
/// außerhalb), weist schon der Server ab: der Bote wird gar nicht gefragt.
/// Draußen und Tabu haben denselben Satz wie „gibt es nicht“.
#[tokio::test]
async fn server_prueft_den_text_selbst() {
    let a = aufbau("server-text");
    let gefragt = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let g = gefragt.clone();
    let ersatz: crate::oeffnen::Ersatz = Arc::new(move |_| {
        g.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Box::pin(async { Antwort::Geoeffnet })
    });
    *a.state.oeffnen.ersatz.lock().unwrap() = Some(ersatz);
    for k in ["eltern", "unc", "geraet", "geraet-punkt", "strom", "tabu", "tabu-anders", "appdata", "draussen"] {
        a.state.oeffnen.grenze.leeren();
        let (s, v) = schicke(router_angemeldet(a.state.clone()), oeffne(k)).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{k}: {v}");
        if matches!(k, "tabu" | "tabu-anders" | "draussen") {
            assert_eq!(v["fehler"], crate::oeffnen::SATZ_NICHT_DA, "{k}");
        }
    }
    assert_eq!(gefragt.load(std::sync::atomic::Ordering::SeqCst), 0, "der Bote wurde nicht gefragt");
}

/// Abnahme 5: ohne Profil des Inhabers (im Dienst leere inhaber_sid, ohne
/// USERPROFILE): 503, nie das Profil des Dienstkontos oder „.“.
#[tokio::test]
async fn ohne_profil_503() {
    let a = aufbau("ohne-profil");
    let r = a.state.oeffnen.regeln.lock().unwrap().clone().unwrap();
    *a.state.oeffnen.regeln.lock().unwrap() = Some(crate::oeffnen::Regeln { profil: None, ..r });
    let (s, v) = schicke(router_angemeldet(a.state.clone()), oeffne("bauplan")).await;
    assert_eq!((s, v["art"].as_str()), (StatusCode::SERVICE_UNAVAILABLE, Some("profil-fehlt")), "{v}");
    assert!(a.aufnahme.alle().is_empty());
}

/// Ohne Boten (Dienst ohne Bote): 503 mit Satz, nichts im Journal.
#[tokio::test]
async fn ohne_boten_503() {
    let a = aufbau("ohne-bote");
    *a.state.oeffnen.ersatz.lock().unwrap() = None;
    let (s, v) = schicke(router_angemeldet(a.state.clone()), oeffne("bauplan")).await;
    assert_eq!((s, v["art"].as_str()), (StatusCode::SERVICE_UNAVAILABLE, Some("bote-fehlt")), "{v}");
    assert!(a.state.speicher.clone().unwrap().vorgaenge_fuer("probe", false).unwrap().is_empty());
}

/// Die Regeln des Servers ohne Ersatz: Wurzeln aus Profil und Quellen, die
/// Ordner des Dienstes verboten, Tabu-Liste aus dem Daten-Ordner des Inhalts.
#[test]
fn regeln_des_servers() {
    let state = AppState::fuer_tests(&beispiel_fuer_tests());
    let r = regeln(&state);
    assert_eq!(r.profil, Some(crate::pc::inventory::home_dir()));
    assert_eq!(r.quellen, state.quellen.to_vec());
    assert!(r.tabu_pflicht);
    assert_eq!(r.endungen, crate::oeffnen::ENDUNGEN.to_vec());
    let daten = state.daten_datei.parent().unwrap().to_path_buf();
    assert!(r.verboten.contains(&daten), "Datenspeicher: {:?}", r.verboten);
    assert!(r.verboten.contains(&state.sicherungs_ordner()));
    let exe = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
    assert!(r.verboten.contains(&exe), "Programm des Dienstes");
}

/// Der Weg ist geschützt (Anmeldung als Inhaber), auch in anderer Schreibweise.
#[test]
fn weg_ist_geschuetzt() {
    assert!(crate::sicherheit::geschuetzt("/api/oeffnen"));
    assert!(crate::sicherheit::geschuetzt("/API/Oeffnen"));
}
