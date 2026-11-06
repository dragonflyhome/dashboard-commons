//! Tests für Etappe 1b: die Schleusen (Schichten um den Router).
//!
//! Test-first geschrieben: Diese Tests sind rot, bis die Schleusen gebaut sind.
//! Alle Anfragen tragen `host: 127.0.0.1:8787`, außer dort, wo die
//! Adress-Prüfung gerade getestet wird.
//!
//! Hinweise für den Bau (Vorschläge, die Vorgaben bleiben wie sie sind):
//!
//! 1. Reihenfolge der Schichten: Im Entwurf (heute `docs/zielbild/`, Seite /zielbild) liegt
//!    `sicherheits_kopfzeilen()` ganz innen und der Absturzfänger ganz außen.
//!    Dann bekommen genau die Antworten, die eine äußere Schleuse selbst
//!    erzeugt (500 vom Absturzfänger, 421, 403, 408, 413), KEINE
//!    Sicherheits-Kopfzeilen und kein `cache-control`. Die Tests unten
//!    verlangen die Kopfzeilen aber auf jeder Antwort. Vorschlag: Kopfzeilen
//!    (Schleuse 7 und 8) und `PropagateRequestIdLayer` außerhalb des
//!    Absturzfängers, oder der Absturzfänger setzt sie selbst.
//! 2. `SetRequestIdLayer` allein schreibt die Nummer nur in die Anfrage. Damit
//!    `x-request-id` in der Antwort steht, braucht es zusätzlich
//!    `PropagateRequestIdLayer`.
//! 3. `RequestBodyLimitLayer` lehnt sofort nur ab, wenn `content-length` zu
//!    groß ist. Ein Körper ohne Länge fällt erst auf, wenn der Handler ihn
//!    liest; GET-Routen lesen nie. Die Tests für „alle Routen“ setzen darum
//!    `content-length`, wie es der Browser auch tut.
//! 4. `TimeoutLayer` von tower-http antwortet mit leerem Körper. Für die
//!    Fehlerform `{fehler, art: "zu-lange"}` braucht es eine eigene Antwort
//!    (z. B. eigene `from_fn`-Schicht mit `tokio::time::timeout`).
//! 5. Zwischenspeicher: Ein 404 unter `/_app/immutable/` sollte `no-store`
//!    tragen, nicht `immutable` (sonst merkt sich der Browser den Fehler ein
//!    Jahr lang). Hier nicht getestet, weil die Vorgabe es offen lässt.
//! 6. Bestehende Tests in `app.rs`, die mit der neuen Regel kollidieren:
//!    `assets_only_with_plain_names_and_html_gets_csp` verlangt für `bild.svg`
//!    KEINE CSP (Schleuse 7 setzt dort jetzt eine), und
//!    `keys_post_needs_origin_and_known_name` erwartet JSON auf `/schluessel/…`
//!    ohne `accept: application/json` (die Fehlerform verlangt dort Klartext)
//!    sowie einen alten Namen eines echten Eintrags.
//! 7. Hängendes git (eigenes 3-s-Limit), Vorschlag für einen späteren Test:
//!    `git::inspect` bekommt eine innere Funktion `inspect_mit(programm, …)`,
//!    der Test gibt als „git“ ein Programm, das nur wartet (unter Windows etwa
//!    `powershell -NoProfile -Command Start-Sleep 30`, die git-Argumente werden
//!    dann als weitere Wörter ignoriert), und misst: Antwort nach weniger als
//!    4 s, `error` enthält „Zeitlimit“. Wichtig dabei `kill_on_drop(true)` am
//!    `Command`, sonst läuft der wartende Prozess nach dem Test weiter.
//!    Ohne Umbau ginge es auch mit echtem git und einem Test-Repo, dessen
//!    `core.fsmonitor` auf ein wartendes Skript zeigt (git status ruft es auf);
//!    das ist aber schwerer zu lesen und hängt an der git-Version.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use axum::Router;
use axum::body::Body;
use axum::http::request::Builder;
use axum::http::{Method, Request, StatusCode, header};
use axum::response::Response;
use http_body_util::BodyExt;
use tower::ServiceExt;

use crate::app::{AppState, router, router_angemeldet};

const EIGENER_HOST: &str = "127.0.0.1:8787";
const EIGENE_HERKUNFT: &str = "http://127.0.0.1:8787";

// ---------------------------------------------------------------------------
// Hilfen
// ---------------------------------------------------------------------------

/// Das Beispiel-Repo: feste Seiten, die kein anderer Chat anfasst.
fn beispiel() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel")
}

/// Der Router über eine Verbindung, die schon als Inhaber angemeldet ist (7b):
/// alle Tests der übrigen Schleusen. Die Anmeldung selbst prüfen die Tests
/// ganz unten mit `app_ohne_anmeldung`.
fn app() -> Router {
    router_angemeldet(AppState::fuer_tests(&beispiel()))
}

/// Der Router ohne Verbindungskennung und ohne Anmeldung.
fn app_ohne_anmeldung() -> Router {
    router(AppState::fuer_tests(&beispiel()))
}

fn app_mit_zeitlimit(zeit: Duration) -> Router {
    let mut state = AppState::fuer_tests(&beispiel());
    state.zeitlimit = zeit;
    router_angemeldet(state)
}

/// Eine Anfrage mit der eigenen Adresse als Host.
fn anfrage(methode: Method, pfad: &str) -> Builder {
    Request::builder().method(methode).uri(pfad).header(header::HOST, EIGENER_HOST)
}

fn hole(pfad: &str) -> Request<Body> {
    anfrage(Method::GET, pfad).body(Body::empty()).unwrap()
}

async fn schicke(app: &Router, req: Request<Body>) -> Response {
    app.clone().oneshot(req).await.unwrap()
}

async fn text(res: Response) -> String {
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8_lossy(&bytes).to_string()
}

async fn json(res: Response) -> serde_json::Value {
    let body = text(res).await;
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("kein JSON ({e}): {body}"))
}

fn kopf(res: &Response, name: &str) -> String {
    res.headers().get(name).map(|v| v.to_str().unwrap_or("<nicht lesbar>").to_string()).unwrap_or_default()
}

/// Schleuse 7: diese drei Kopfzeilen muss jede Antwort tragen.
fn pruefe_sicherheits_kopfzeilen(res: &Response, wo: &str) {
    let csp = kopf(res, "content-security-policy");
    assert!(!csp.is_empty(), "{wo}: Content-Security-Policy fehlt");
    assert!(csp.contains("default-src 'self'"), "{wo}: {csp}");
    assert!(csp.contains("frame-ancestors 'none'"), "{wo}: {csp}");
    for verboten in ["http:", "https:", "*"] {
        assert!(!csp.contains(verboten), "{wo}: CSP erlaubt fremde Quellen ({verboten}): {csp}");
    }
    assert_eq!(kopf(res, "x-content-type-options"), "nosniff", "{wo}");
    assert_eq!(kopf(res, "referrer-policy"), "no-referrer", "{wo}");
}

/// Ein großer Körper mit passender Länge, wie ihn ein Browser schickt.
fn grosser_post(pfad: &str, groesse: usize) -> Request<Body> {
    anfrage(Method::POST, pfad)
        .header(header::ORIGIN, EIGENE_HERKUNFT)
        .header(header::ACCEPT, "application/json")
        .header(header::CONTENT_LENGTH, groesse)
        .body(Body::from(vec![b'a'; groesse]))
        .unwrap()
}

// ---------------------------------------------------------------------------
// Schleuse 1: Absturzfänger
// ---------------------------------------------------------------------------

#[tokio::test]
async fn absturz_wird_hoefliche_fehlerantwort_ohne_technik() {
    let res = schicke(&app(), hole("/api/_test/panik")).await;
    assert_eq!(res.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = text(res).await;
    let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_else(|e| panic!("kein JSON ({e}): {body}"));
    assert_eq!(v["art"], "intern", "{body}");
    assert!(v["fehler"].as_str().is_some_and(|s| !s.is_empty()), "Satz für den Inhaber fehlt: {body}");
    for verboten in ["panicked", "Absichtlicher Absturz", "backtrace", "RUST_BACKTRACE", ".rs:"] {
        assert!(!body.contains(verboten), "Technik in der Antwort ({verboten}): {body}");
    }
}

// ---------------------------------------------------------------------------
// Schleuse 2: Protokoll mit Nummer
// ---------------------------------------------------------------------------

#[tokio::test]
async fn jede_antwort_hat_eine_eigene_anfrage_nummer() {
    let app = app();
    let a = schicke(&app, hole("/healthz")).await;
    let b = schicke(&app, hole("/api/projects/gibt-es-nicht")).await;
    let fremd = schicke(&app, Request::get("/healthz").header(header::HOST, "boese.example:8787").body(Body::empty()).unwrap()).await;
    let mut nummern = Vec::new();
    for (wo, res) in [("/healthz", &a), ("404", &b), ("421", &fremd)] {
        let id = kopf(res, "x-request-id");
        assert!(!id.is_empty() && id.chars().all(|c| c.is_ascii_digit()), "{wo}: x-request-id ist keine Dezimalzahl: „{id}“");
        id.parse::<u64>().unwrap_or_else(|e| panic!("{wo}: {id}: {e}"));
        nummern.push(id);
    }
    assert_ne!(nummern[0], nummern[1], "zwei Anfragen, zweimal dieselbe Nummer");
    assert_ne!(nummern[1], nummern[2]);
}

// ---------------------------------------------------------------------------
// Schleuse 3: Adress-Prüfung (gegen DNS-Rebinding)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn fremde_adresse_gibt_421() {
    let app = app();
    for host in [
        "boese.example:8787",
        "127.0.0.1:9999",
        "127.0.0.1",
        "localhost",
        "localhost.boese.example:8787",
        "127.0.0.1:8787.boese.example",
        "127.0.0.2:8787",
        // [::1] mit fremdem Port oder ohne Port ist keine eigene Adresse.
        "[::1]:9999",
        "[::1]",
    ] {
        let req = Request::get("/api/projects").header(header::HOST, host).body(Body::empty()).unwrap();
        let res = schicke(&app, req).await;
        assert_eq!(res.status(), StatusCode::MISDIRECTED_REQUEST, "Host {host}");
        let v = json(res).await;
        assert_eq!(v["art"], "falsche-adresse", "Host {host}: {v}");
        assert!(v["fehler"].as_str().is_some_and(|s| !s.is_empty()), "Host {host}: {v}");
    }
}

#[tokio::test]
async fn fremde_adresse_auf_seite_gibt_421_als_satz() {
    let req = Request::get("/").header(header::HOST, "boese.example:8787").body(Body::empty()).unwrap();
    let res = schicke(&app(), req).await;
    assert_eq!(res.status(), StatusCode::MISDIRECTED_REQUEST);
    assert_eq!(kopf(&res, "content-type"), "text/plain; charset=utf-8");
    let body = text(res).await;
    assert!(!body.trim().is_empty() && !body.starts_with('{'), "Satz erwartet: {body}");
}

#[tokio::test]
async fn ohne_host_kopfzeile_gibt_421() {
    let req = Request::get("/healthz").body(Body::empty()).unwrap();
    let res = schicke(&app(), req).await;
    assert_eq!(res.status(), StatusCode::MISDIRECTED_REQUEST);
}

#[tokio::test]
async fn eigene_adressen_gehen_durch() {
    let app = app();
    // [::1] ist wieder dabei: Aus der 1b-Nachbesserung war es entfernt, weil
    // der Server nur auf IPv4 lauschte; seit 7a hört er auch auf [::1].
    for host in ["127.0.0.1:8787", "localhost:8787", "[::1]:8787"] {
        let req = Request::get("/healthz").header(header::HOST, host).body(Body::empty()).unwrap();
        let res = schicke(&app, req).await;
        assert_eq!(res.status(), StatusCode::OK, "Host {host}");
        assert_eq!(text(res).await, "ok", "Host {host}");
    }
}

#[tokio::test]
async fn fremde_adresse_wird_vor_der_herkunft_geprueft() {
    // Auch mit passendem Origin: falscher Host bleibt 421.
    let req = Request::post("/schluessel/gibt-es-nicht")
        .header(header::HOST, "boese.example:8787")
        .header(header::ORIGIN, EIGENE_HERKUNFT)
        .body(Body::from("wert=abc"))
        .unwrap();
    assert_eq!(schicke(&app(), req).await.status(), StatusCode::MISDIRECTED_REQUEST);
}

// ---------------------------------------------------------------------------
// Schleuse 4: Herkunftsprüfung für POST
// ---------------------------------------------------------------------------

#[tokio::test]
async fn post_ohne_eigene_herkunft_gibt_403() {
    let app = app();
    let faelle: [(&str, &[(&str, &str)]); 7] = [
        ("ohne Origin und Referer", &[]),
        ("fremder Origin", &[("origin", "https://boese.example")]),
        ("Origin null", &[("origin", "null")]),
        ("ähnlicher Port", &[("origin", "http://127.0.0.1:87870")]),
        ("fremder Referer", &[("referer", "https://boese.example/schluessel")]),
        ("Referer mit unserem Namen als Teil", &[("referer", "http://127.0.0.1:8787.boese.example/")]),
        ("eigener Origin, aber Browser sagt cross-site", &[("origin", EIGENE_HERKUNFT), ("sec-fetch-site", "cross-site")]),
    ];
    for (fall, kopfzeilen) in faelle {
        let mut b = anfrage(Method::POST, "/schluessel/gibt-es-nicht").header(header::ACCEPT, "application/json");
        for (k, v) in kopfzeilen {
            b = b.header(*k, *v);
        }
        let res = schicke(&app, b.body(Body::from("wert=abc")).unwrap()).await;
        assert_eq!(res.status(), StatusCode::FORBIDDEN, "{fall}");
        let v = json(res).await;
        assert_eq!(v["art"], "abgelehnt", "{fall}: {v}");
    }
}

#[tokio::test]
async fn post_auf_jede_route_braucht_herkunft() {
    // Die Prüfung ist eine Schleuse, nicht nur im Schlüssel-Handler: auch eine
    // Route ohne POST antwortet ohne Herkunft mit 403 (nicht 405).
    let req = anfrage(Method::POST, "/api/projects").body(Body::from("x=1")).unwrap();
    let res = schicke(&app(), req).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(json(res).await["art"], "abgelehnt");
}

#[tokio::test]
async fn post_mit_eigener_herkunft_geht_durch() {
    let app = app();
    /// Fall, Host, Kopfzeilen.
    type Fall<'a> = (&'a str, &'a str, &'a [(&'a str, &'a str)]);
    let faelle: [Fall<'_>; 4] = [
        ("Origin 127.0.0.1", EIGENER_HOST, &[("origin", EIGENE_HERKUNFT)]),
        ("Referer der Schlüssel-Seite", EIGENER_HOST, &[("referer", "http://127.0.0.1:8787/schluessel")]),
        ("Origin localhost", "localhost:8787", &[("origin", "http://localhost:8787")]),
        ("Origin und same-origin", EIGENER_HOST, &[("origin", EIGENE_HERKUNFT), ("sec-fetch-site", "same-origin")]),
    ];
    for (fall, host, kopfzeilen) in faelle {
        let mut b = Request::post("/schluessel/gibt-es-nicht").header(header::HOST, host);
        for (k, v) in kopfzeilen {
            b = b.header(*k, *v);
        }
        let res = schicke(&app, b.body(Body::from("wert=abc")).unwrap()).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER, "{fall}");
    }
}

/// Lesend gilt die Herkunft nur unter `/api/*` und für Projektdateien
/// (Abnahme 2c); die Oberfläche bleibt für Links von fremden Seiten offen.
#[tokio::test]
async fn get_auf_die_oberflaeche_ist_nicht_von_der_herkunft_betroffen() {
    let app = app();
    for pfad in ["/", "/pc", "/projects/schaufenster"] {
        let req =
            anfrage(Method::GET, pfad).header(header::ORIGIN, "https://boese.example").header("sec-fetch-site", "cross-site").body(Body::empty()).unwrap();
        let res = schicke(&app, req).await;
        assert!(res.status() == StatusCode::OK || res.status() == StatusCode::SERVICE_UNAVAILABLE, "{pfad}: {}", res.status());
    }
}

/// Abnahme 2c: Fremde Seiten kommen auch lesend nicht an `/api/*` und an die
/// Projektdateien; ohne Kopfzeile (curl), `same-origin` und `none` geht es.
#[tokio::test]
async fn lesend_api_und_assets_nicht_fuer_fremde_seiten() {
    let app = app();
    for pfad in ["/api/pruefung", "/api/projects", "/projects/schaufenster/assets/bild.svg", "/projects/schaufenster/assets/vorschau.html"] {
        for fremd in ["cross-site", "same-site"] {
            for methode in [Method::GET, Method::HEAD] {
                let req = anfrage(methode.clone(), pfad).header("sec-fetch-site", fremd).body(Body::empty()).unwrap();
                let res = schicke(&app, req).await;
                assert_eq!(res.status(), StatusCode::FORBIDDEN, "{methode} {pfad} {fremd}");
                pruefe_sicherheits_kopfzeilen(&res, pfad);
            }
        }
        for erlaubt in [None, Some("same-origin"), Some("none")] {
            let mut b = anfrage(Method::GET, pfad);
            if let Some(w) = erlaubt {
                b = b.header("sec-fetch-site", w);
            }
            let res = schicke(&app, b.body(Body::empty()).unwrap()).await;
            assert_eq!(res.status(), StatusCode::OK, "{pfad} {erlaubt:?}");
        }
    }
    // Unter /api als JSON, bei den Dateien als Satz.
    let req = anfrage(Method::GET, "/api/pruefung").header("sec-fetch-site", "cross-site").body(Body::empty()).unwrap();
    assert_eq!(json(schicke(&app, req).await).await["art"], "abgelehnt");
    let req = anfrage(Method::GET, "/projects/schaufenster/assets/bild.svg").header("sec-fetch-site", "cross-site").body(Body::empty()).unwrap();
    let res = schicke(&app, req).await;
    assert!(kopf(&res, "content-type").starts_with("text/plain"), "{}", kopf(&res, "content-type"));
}

// ---------------------------------------------------------------------------
// Schleuse 5: Zeitlimit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn zu_lange_anfrage_gibt_408() {
    let app = app_mit_zeitlimit(Duration::from_millis(300));
    let start = Instant::now();
    let res = schicke(&app, hole("/api/_test/langsam")).await;
    let dauer = start.elapsed();
    assert_eq!(res.status(), StatusCode::REQUEST_TIMEOUT);
    assert!(dauer < Duration::from_millis(1500), "Zeitlimit greift zu spät: {dauer:?}");
    let v = json(res).await;
    assert_eq!(v["art"], "zu-lange", "{v}");
}

#[tokio::test]
async fn langsame_anfrage_unter_dem_zeitlimit_geht_durch() {
    // Standard 10 s: die 2-s-Route kommt durch.
    let res = schicke(&app(), hole("/api/_test/langsam")).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(text(res).await, "ok");
}

#[tokio::test]
async fn live_kanal_hat_kein_zeitlimit() {
    let res = schicke(&app_mit_zeitlimit(Duration::from_millis(300)), hole("/api/events")).await;
    assert_eq!(res.status(), StatusCode::OK);
    let typ = kopf(&res, "content-type");
    assert!(typ.starts_with("text/event-stream"), "{typ}");
    // Nicht den ganzen Körper lesen (er endet nie). Nur prüfen: Nach mehr als
    // dem Zeitlimit ist der Kanal weder abgebrochen noch mit Fehler zu.
    let mut body = res.into_body();
    match tokio::time::timeout(Duration::from_millis(700), body.frame()).await {
        Err(_) => {}          // noch offen, nichts geschickt: gut
        Ok(Some(Ok(_))) => {} // ein Ereignis oder Lebenszeichen: auch gut
        Ok(Some(Err(e))) => panic!("Live-Kanal mit Fehler abgebrochen: {e}"),
        Ok(None) => panic!("Live-Kanal wurde geschlossen"),
    }
}

// ---------------------------------------------------------------------------
// Schleuse 6: Größenlimit
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mehr_als_16_kb_gibt_auf_allen_routen_413() {
    let app = app();
    for pfad in ["/api/projects", "/healthz", "/", "/schluessel/gibt-es-nicht"] {
        let res = schicke(&app, grosser_post(pfad, 17 * 1024)).await;
        assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE, "POST {pfad}");
        assert_eq!(json(res).await["art"], "zu-gross", "POST {pfad}");
    }
    // Auch ein GET mit (unsinnig) großem Körper.
    let req = anfrage(Method::GET, "/api/projects").header(header::CONTENT_LENGTH, 17 * 1024).body(Body::from(vec![b'a'; 17 * 1024])).unwrap();
    let res = schicke(&app, req).await;
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE, "GET mit großem Körper");
    assert_eq!(json(res).await["art"], "zu-gross");
}

#[tokio::test]
async fn genau_16_kb_sind_noch_erlaubt() {
    // Grenze: 16 KB gehen durch die Schleuse (auf einer GET-Route, die den
    // Körper nicht liest). Nur „mehr als“ ist zu groß.
    let req = anfrage(Method::GET, "/healthz").header(header::CONTENT_LENGTH, 16 * 1024).body(Body::from(vec![b'a'; 16 * 1024])).unwrap();
    assert_eq!(schicke(&app(), req).await.status(), StatusCode::OK);
}

#[tokio::test]
async fn schluessel_ab_mehr_als_4_kb_gibt_413() {
    // Name `gibt-es-nicht`: Selbst wenn die Prüfung fehlt, wird nichts
    // gespeichert. Die Größe muss VOR der Namenssuche geprüft werden.
    let app = app();
    // Mit Länge in der Kopfzeile …
    let res = schicke(&app, grosser_post("/schluessel/gibt-es-nicht", 5000)).await;
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE, "mit content-length");
    assert_eq!(json(res).await["art"], "zu-gross");
    // … und ohne (der Körper fällt erst beim Lesen auf).
    let req = anfrage(Method::POST, "/schluessel/gibt-es-nicht")
        .header(header::ORIGIN, EIGENE_HERKUNFT)
        .header(header::ACCEPT, "application/json")
        .body(Body::from(vec![b'a'; 5000]))
        .unwrap();
    let res = schicke(&app, req).await;
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE, "ohne content-length");
    assert_eq!(json(res).await["art"], "zu-gross");
}

#[tokio::test]
async fn schluessel_unter_4_kb_geht_durch() {
    let mut koerper = b"wert=".to_vec();
    koerper.extend(vec![b'z'; 3000]);
    let req = anfrage(Method::POST, "/schluessel/gibt-es-nicht").header(header::ORIGIN, EIGENE_HERKUNFT).body(Body::from(koerper)).unwrap();
    let res = schicke(&app(), req).await;
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    let ziel = kopf(&res, "location");
    assert!(ziel.starts_with("/schluessel?fehler="), "{ziel}");
    assert!(!ziel.contains("zzzz"), "nie den Wert zurückgeben: {ziel}");
}

// ---------------------------------------------------------------------------
// Schleuse 7: Sicherheits-Kopfzeilen
// ---------------------------------------------------------------------------

#[tokio::test]
async fn kopfzeilen_auf_normalen_antworten() {
    let app = app();
    for pfad in ["/", "/pc", "/healthz", "/api/projects", "/api/projects/dashboard", "/api/pc", "/api/pc/benutzer", "/api/schluessel"] {
        let res = schicke(&app, hole(pfad)).await;
        pruefe_sicherheits_kopfzeilen(&res, pfad);
    }
    let res = schicke(&app, hole("/neu/pc")).await;
    pruefe_sicherheits_kopfzeilen(&res, "Weiterleitung /neu/pc");
    let req = anfrage(Method::POST, "/schluessel/gibt-es-nicht").header(header::ORIGIN, EIGENE_HERKUNFT).body(Body::from("wert=abc")).unwrap();
    pruefe_sicherheits_kopfzeilen(&schicke(&app, req).await, "303 nach POST");
}

#[tokio::test]
async fn kopfzeilen_auf_fehlerantworten() {
    let app = app();
    for pfad in [
        "/api/projects/gibt-es-nicht",
        "/api/projects/kaputt",
        "/projects/schaufenster/assets/gibt-es-nicht.png",
        "/_app/immutable/gibt-es-nicht.js",
        "/.git/config",
    ] {
        let res = schicke(&app, hole(pfad)).await;
        assert!(res.status().is_client_error(), "{pfad}: {}", res.status());
        pruefe_sicherheits_kopfzeilen(&res, pfad);
    }
    let fremd = Request::get("/").header(header::HOST, "boese.example:8787").body(Body::empty()).unwrap();
    pruefe_sicherheits_kopfzeilen(&schicke(&app, fremd).await, "421");
    let ohne_herkunft = anfrage(Method::POST, "/schluessel/x").body(Body::from("wert=abc")).unwrap();
    pruefe_sicherheits_kopfzeilen(&schicke(&app, ohne_herkunft).await, "403");
    pruefe_sicherheits_kopfzeilen(&schicke(&app, grosser_post("/api/projects", 17 * 1024)).await, "413");
    let langsam = schicke(&app_mit_zeitlimit(Duration::from_millis(300)), hole("/api/_test/langsam")).await;
    pruefe_sicherheits_kopfzeilen(&langsam, "408");
    pruefe_sicherheits_kopfzeilen(&schicke(&app, hole("/api/_test/panik")).await, "500 nach Absturz");
}

#[tokio::test]
async fn vorschau_html_behaelt_eigene_strenge_csp() {
    let res = schicke(&app(), hole("/projects/schaufenster/assets/vorschau.html")).await;
    assert_eq!(res.status(), StatusCode::OK);
    let csp = kopf(&res, "content-security-policy");
    assert!(csp.contains("sandbox allow-scripts"), "{csp}");
    assert!(csp.contains("default-src 'none'"), "{csp}");
    assert!(!csp.contains("default-src 'self'"), "die Oberflächen-CSP darf sie nicht ersetzen: {csp}");
    assert_eq!(res.headers().get_all("content-security-policy").iter().count(), 1, "genau eine CSP");
    assert_eq!(kopf(&res, "x-content-type-options"), "nosniff");
    assert_eq!(kopf(&res, "referrer-policy"), "no-referrer");
}

/// Abnahme 2c: SVG bekommt eine eigene strenge CSP (unabhängig von der
/// SVG-Prüfung und der CSP der Oberfläche), alle anderen Dateien eine knappe.
#[tokio::test]
async fn svg_und_andere_dateien_mit_eigener_csp() {
    let res = schicke(&app(), hole("/projects/schaufenster/assets/bild.svg")).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(kopf(&res, "content-type"), "image/svg+xml");
    let csp = kopf(&res, "content-security-policy");
    assert_eq!(csp, crate::api::projekte::ASSET_SVG_CSP);
    for teil in ["default-src 'none'", "style-src 'unsafe-inline'", "img-src data:", "sandbox"] {
        assert!(csp.contains(teil), "{teil}: {csp}");
    }
    assert!(!csp.contains("script-src") && !csp.contains("'self'"), "{csp}");
    assert_eq!(res.headers().get_all("content-security-policy").iter().count(), 1, "genau eine CSP");
    assert_eq!(res.headers().get_all("x-content-type-options").iter().count(), 1, "nosniff genau einmal");
    assert_eq!(kopf(&res, "x-content-type-options"), "nosniff");

    let (wurzel, assets) = temp_repo("csp");
    std::fs::write(assets.join("notiz.txt"), "Hallo").unwrap();
    std::fs::write(assets.join("bild.png"), [0x89, b'P', b'N', b'G']).unwrap();
    std::fs::write(assets.join("plan.pdf"), b"%PDF-1.4\n").unwrap();
    let app = router(AppState::fuer_tests(&wurzel));
    for (datei, art, csp) in [
        ("notiz.txt", "text/plain; charset=utf-8", "default-src 'none'; sandbox"),
        ("bild.png", "image/png", "default-src 'none'; sandbox"),
        // PDF ohne sandbox, sonst zeigt der PDF-Betrachter des Browsers nichts.
        ("plan.pdf", "application/pdf", "default-src 'none'"),
    ] {
        let res = schicke(&app, hole(&format!("/projects/p/assets/{datei}"))).await;
        assert_eq!(res.status(), StatusCode::OK, "{datei}");
        assert_eq!(kopf(&res, "content-type"), art);
        assert_eq!(kopf(&res, "content-security-policy"), csp, "{datei}");
        assert_eq!(kopf(&res, "x-content-type-options"), "nosniff", "{datei}");
    }
    let _ = std::fs::remove_dir_all(&wurzel);
}

/// Nachzug 2c: Bilder aus dem Vorschau-Rahmen (undurchsichtige Herkunft, darum
/// `cross-site`) gehen als `Sec-Fetch-Dest: image` im no-cors-Modus durch,
/// aber nur mit Bildendung und nur als Bild; sonst bleibt es 403.
#[tokio::test]
async fn bilder_fuer_den_vorschau_rahmen_trotz_cross_site() {
    let (wurzel, assets) = temp_repo("rahmen");
    std::fs::write(assets.join("bild.png"), [0x89, b'P', b'N', b'G']).unwrap();
    std::fs::write(assets.join("plan.svg"), "<svg xmlns=\"http://www.w3.org/2000/svg\"/>").unwrap();
    std::fs::write(assets.join("daten.json"), "{}").unwrap();
    std::fs::write(assets.join("vorschau.html"), "<!doctype html><p>x</p>").unwrap();
    let app = router(AppState::fuer_tests(&wurzel));
    let mit = |pfad: &str, methode: Method, ziel: &str, modus: &str| {
        anfrage(methode, pfad)
            .header("sec-fetch-site", "cross-site")
            .header("sec-fetch-dest", ziel)
            .header("sec-fetch-mode", modus)
            .header(header::ORIGIN, "null")
            .body(Body::empty())
            .unwrap()
    };
    let faelle = [
        ("/projects/p/assets/bild.png", Method::GET, "image", "no-cors", StatusCode::OK),
        ("/projects/p/assets/plan.svg", Method::GET, "image", "no-cors", StatusCode::OK),
        ("/projects/p/assets/vorschau.html", Method::GET, "iframe", "navigate", StatusCode::FORBIDDEN),
        ("/projects/p/assets/vorschau.html", Method::GET, "image", "no-cors", StatusCode::FORBIDDEN),
        ("/projects/p/assets/daten.json", Method::GET, "image", "no-cors", StatusCode::FORBIDDEN),
        ("/projects/p/assets/bild.png", Method::GET, "image", "cors", StatusCode::FORBIDDEN),
        ("/projects/p/assets/bild.png", Method::GET, "empty", "no-cors", StatusCode::FORBIDDEN),
        ("/projects/p/assets/bild.png", Method::HEAD, "image", "no-cors", StatusCode::FORBIDDEN),
        ("/api/projects", Method::GET, "image", "no-cors", StatusCode::FORBIDDEN),
    ];
    let mut ergebnisse = Vec::new();
    for (pfad, methode, ziel, modus, _) in &faelle {
        ergebnisse.push(schicke(&app, mit(pfad, methode.clone(), ziel, modus)).await.status());
    }
    let _ = std::fs::remove_dir_all(&wurzel);
    for ((pfad, methode, ziel, modus, soll), ist) in faelle.iter().zip(ergebnisse) {
        assert_eq!(ist, *soll, "{methode} {pfad} dest={ziel} mode={modus}");
    }
}

/// Nachzug 2c: lesend abgelehnt heißt „Diese Anfrage …“, beim Formular bleibt der Formular-Satz.
#[tokio::test]
async fn ablehnungssatz_nach_methode() {
    let app = app();
    let req = anfrage(Method::GET, "/api/projects").header("sec-fetch-site", "cross-site").body(Body::empty()).unwrap();
    let v = json(schicke(&app, req).await).await;
    assert_eq!(v["fehler"], "Abgelehnt: Diese Anfrage kam nicht vom Dashboard selbst (http://127.0.0.1).");
    let req =
        anfrage(Method::POST, "/api/projects").header("sec-fetch-site", "cross-site").header(header::ORIGIN, EIGENE_HERKUNFT).body(Body::empty()).unwrap();
    let v = json(schicke(&app, req).await).await;
    assert_eq!(v["fehler"], "Abgelehnt: Das Formular kam nicht vom Dashboard selbst (http://127.0.0.1).");
}

/// Ein Repo im Temp-Ordner mit `projects/p/assets/`; gibt Wurzel und `assets` zurück.
fn temp_repo(name: &str) -> (PathBuf, PathBuf) {
    let wurzel = std::env::temp_dir().join(format!("dashboard-schleusen-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&wurzel);
    let assets = wurzel.join("projects").join("p").join("assets");
    std::fs::create_dir_all(&assets).unwrap();
    std::fs::write(wurzel.join("projects").join("p").join("projekt.toml"), "schema = 3\ntitle = \"P\"\nstatus = \"aktiv\"\nsummary = \"S\"\n").unwrap();
    (wurzel, assets)
}

/// Abnahme 2c: `assets` als Junction auf einen fremden Ordner liefert nichts
/// aus (404), ein echter Ordner schon (200). Ebenso eine Junction auf den
/// Projektordner. Geht `mklink /J` nicht, wird der Test übersprungen.
#[tokio::test]
async fn assets_als_junction_gibt_404() {
    let (wurzel, assets) = temp_repo("junction");
    std::fs::write(assets.join("echt.svg"), "<svg xmlns=\"http://www.w3.org/2000/svg\"/>").unwrap();
    let fremd = wurzel.join("fremd");
    std::fs::create_dir_all(&fremd).unwrap();
    std::fs::write(fremd.join("geheim.txt"), "nicht ausliefern").unwrap();
    let q = wurzel.join("projects").join("q");
    std::fs::create_dir_all(&q).unwrap();
    let app = router(AppState::fuer_tests(&wurzel));
    let res = schicke(&app, hole("/projects/p/assets/echt.svg")).await;
    assert_eq!(res.status(), StatusCode::OK, "echter Ordner");

    let links = [q.join("assets"), wurzel.join("projects").join("r")];
    let aufraeumen = || {
        for l in &links {
            if std::fs::symlink_metadata(l).is_ok() {
                let _ = std::fs::remove_dir(l);
            }
        }
        let _ = std::fs::remove_dir_all(&wurzel);
    };
    if !crate::inhalt::seite::junction_anlegen(&links[0], &fremd) || !crate::inhalt::seite::junction_anlegen(&links[1], &fremd) {
        aufraeumen();
        eprintln!("assets_als_junction_gibt_404 übersprungen: Junction ließ sich nicht anlegen");
        return;
    }
    // Über die Junction wäre die Datei lesbar; der Server liefert sie trotzdem nicht.
    assert!(links[0].join("geheim.txt").is_file());
    let res = schicke(&app, hole("/projects/q/assets/geheim.txt")).await;
    let status = res.status();
    let satz = text(res).await;
    // Projektordner r ist selbst eine Junction (auf fremd, darin ein Ordner assets fehlt):
    std::fs::create_dir_all(fremd.join("assets")).unwrap();
    std::fs::write(fremd.join("assets").join("auch.txt"), "nicht ausliefern").unwrap();
    let r = schicke(&app, hole("/projects/r/assets/auch.txt")).await.status();
    aufraeumen();
    assert_eq!(status, StatusCode::NOT_FOUND, "{satz}");
    assert!(satz.contains("keine Verknüpfungen"), "{satz}");
    assert!(!satz.contains("nicht ausliefern"), "{satz}");
    assert_eq!(r, StatusCode::NOT_FOUND, "Projektordner als Junction");
}

/// Abnahme 2c: Dateien über der Grenze aus `assets.rs` liefert der Server
/// nicht aus (413 mit Satz), auch wenn die Prüfung nie lief.
#[tokio::test]
async fn zu_grosse_datei_gibt_413() {
    use crate::inhalt::assets;
    let (wurzel, assets_ordner) = temp_repo("gross");
    let mut svg = String::from("<svg xmlns=\"http://www.w3.org/2000/svg\">\n");
    while svg.len() as u64 <= assets::SVG_HOECHSTENS {
        svg.push_str("<rect width=\"1\" height=\"1\"/>\n");
    }
    svg.push_str("</svg>\n");
    std::fs::write(assets_ordner.join("riesig.svg"), &svg).unwrap();
    std::fs::write(assets_ordner.join("gross.html"), vec![b'a'; assets::HTML_HOECHSTENS as usize + 1]).unwrap();
    std::fs::write(assets_ordner.join("genau.png"), vec![0u8; assets::BILD_HOECHSTENS as usize]).unwrap();
    let app = router(AppState::fuer_tests(&wurzel));
    let svg_res = schicke(&app, hole("/projects/p/assets/riesig.svg")).await;
    let svg_status = svg_res.status();
    let svg_satz = text(svg_res).await;
    let html_res =
        schicke(&app, anfrage(Method::GET, "/projects/p/assets/gross.html").header(header::ACCEPT, "application/json").body(Body::empty()).unwrap()).await;
    let html_status = html_res.status();
    let html = json(html_res).await;
    let genau = schicke(&app, hole("/projects/p/assets/genau.png")).await.status();
    let _ = std::fs::remove_dir_all(&wurzel);
    assert_eq!(svg_status, StatusCode::PAYLOAD_TOO_LARGE, "{svg_satz}");
    assert_eq!(svg_satz, "Die Datei ist größer als erlaubt (höchstens 500 KB).");
    assert_eq!(html_status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(html["fehler"], "Die Datei ist größer als erlaubt (höchstens 2 MB).");
    assert_eq!(html["art"], "zu-gross");
    assert_eq!(genau, StatusCode::OK, "genau an der Grenze geht noch");
}

// ---------------------------------------------------------------------------
// Schleuse 8: Zwischenspeicher-Regeln
// ---------------------------------------------------------------------------

/// Unabhängig vom Live-Betrieb: ein eigenes Web-Verzeichnis im Temp-Ordner
/// (`build/` ohne `builds/`), nicht `web/build` des Repos. Dort baut der
/// Bote im Haupt-Repo `web/builds/aktuell`, und `WebOrt` folgte diesem
/// Zeiger; die Datei aus `web/build` gab es dann nicht (404).
#[tokio::test]
async fn unveraenderliche_dateien_lange_im_zwischenspeicher() {
    let wurzel = std::env::temp_dir().join(format!("dashboard-schleuse8-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&wurzel);
    let build = wurzel.join("build");
    let entry = build.join("_app").join("immutable").join("entry");
    std::fs::create_dir_all(&entry).unwrap();
    std::fs::write(build.join("index.html"), "<!doctype html><title>Probe</title>").unwrap();
    std::fs::write(entry.join("start.Ab12Cd.js"), "export const x = 1;\n").unwrap();
    let state = AppState::fuer_tests(&beispiel()).mit_web(build.clone());
    let pfad = "/_app/immutable/entry/start.Ab12Cd.js";
    let res = schicke(&router_angemeldet(state), hole(pfad)).await;
    let _ = std::fs::remove_dir_all(&wurzel);
    assert_eq!(res.status(), StatusCode::OK, "{pfad}");
    assert_eq!(kopf(&res, "cache-control"), "public, max-age=31536000, immutable", "{pfad}");
    assert_eq!(res.headers().get_all("cache-control").iter().count(), 1, "genau eine Regel");
}

#[tokio::test]
async fn alles_andere_wird_nicht_zwischengespeichert() {
    let app = app();
    for pfad in [
        "/",
        "/pc",
        "/healthz",
        "/api/projects",
        "/api/projects/gibt-es-nicht",
        "/projects/schaufenster/assets/bild.svg",
        "/projects/schaufenster/assets/vorschau.html",
    ] {
        let res = schicke(&app, hole(pfad)).await;
        assert_eq!(kopf(&res, "cache-control"), "no-store", "{pfad}");
    }
    let fremd = Request::get("/").header(header::HOST, "boese.example:8787").body(Body::empty()).unwrap();
    assert_eq!(kopf(&schicke(&app, fremd).await, "cache-control"), "no-store", "421");
    let ohne_herkunft = anfrage(Method::POST, "/schluessel/x").body(Body::from("wert=abc")).unwrap();
    assert_eq!(kopf(&schicke(&app, ohne_herkunft).await, "cache-control"), "no-store", "403");
}

// ---------------------------------------------------------------------------
// Fehlerform: JSON für die API, Satz für den Browser
// ---------------------------------------------------------------------------

#[tokio::test]
async fn fehler_auf_browser_weg_ist_ein_satz_und_mit_accept_json() {
    let app = app();
    let pfad = "/projects/schaufenster/assets/gibt-es-nicht.png";
    let res = schicke(&app, hole(pfad)).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    assert_eq!(kopf(&res, "content-type"), "text/plain; charset=utf-8");
    let satz = text(res).await;
    assert!(!satz.trim().is_empty() && !satz.starts_with('{'), "Satz erwartet: {satz}");

    let req = anfrage(Method::GET, pfad).header(header::ACCEPT, "application/json").body(Body::empty()).unwrap();
    let res = schicke(&app, req).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    assert!(kopf(&res, "content-type").starts_with("application/json"));
    let v = json(res).await;
    assert_eq!(v["art"], "nicht-gefunden", "{v}");
    assert_eq!(v["fehler"].as_str(), Some(satz.as_str()), "derselbe Satz in beiden Formen");
}

#[tokio::test]
async fn abgelehnter_post_ist_ein_satz_und_mit_accept_json() {
    let app = app();
    let res = schicke(&app, anfrage(Method::POST, "/schluessel/x").body(Body::from("wert=abc")).unwrap()).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(kopf(&res, "content-type"), "text/plain; charset=utf-8");
    let satz = text(res).await;
    assert!(!satz.trim().is_empty() && !satz.starts_with('{'), "Satz erwartet: {satz}");

    let req = anfrage(Method::POST, "/schluessel/x").header(header::ACCEPT, "application/json").body(Body::from("wert=abc")).unwrap();
    let res = schicke(&app, req).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let v = json(res).await;
    assert_eq!(v["art"], "abgelehnt", "{v}");
    assert_eq!(v["fehler"].as_str(), Some(satz.as_str()), "derselbe Satz in beiden Formen");
}

#[tokio::test]
async fn fehler_unter_api_ist_immer_json() {
    let res = schicke(&app(), hole("/api/projects/gibt-es-nicht")).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    assert!(kopf(&res, "content-type").starts_with("application/json"));
    let v = json(res).await;
    assert_eq!(v["art"], "nicht-gefunden");
    assert!(v["fehler"].as_str().is_some_and(|s| !s.is_empty()));
}

// ---------------------------------------------------------------------------
// Eigene Limits und Schlüssel
// ---------------------------------------------------------------------------

#[tokio::test]
async fn git_auf_fehlendem_ordner_antwortet_schnell_mit_fehler() {
    let start = Instant::now();
    let status = crate::git::inspect("C:/gibt/es/nicht", None, false).await;
    let dauer = start.elapsed();
    assert!(dauer < Duration::from_secs(4), "git hat zu lange gebraucht: {dauer:?}");
    assert!(status.error.is_some(), "fehlender Ordner muss als Fehler erscheinen");
}

#[tokio::test]
async fn unbekannter_schluessel_fuehrt_zurueck_mit_fehler() {
    let req = anfrage(Method::POST, "/schluessel/gibt-es-nicht").header(header::ORIGIN, EIGENE_HERKUNFT).body(Body::from("wert=abc&benutzer=jemand")).unwrap();
    let res = schicke(&app(), req).await;
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    let ziel = kopf(&res, "location");
    assert!(ziel.starts_with("/schluessel?fehler="), "{ziel}");
    assert!(!ziel.contains("abc") && !ziel.contains("jemand"), "nie Wert oder Benutzer zurückgeben: {ziel}");
}

#[test]
fn beispiel_schluessel_heisst_beispiel_test() {
    // Der Beispiel-Schlüssel darf nicht wie ein echter Tresor-Eintrag heißen,
    // damit kein Test aus Versehen einen echten Eintrag überschreibt.
    let datei = beispiel().join("data").join("schluessel.toml");
    let inhalt = std::fs::read_to_string(&datei).unwrap();
    let v: toml::Value = toml::from_str(&inhalt).unwrap();
    let namen: Vec<&str> = v["key"].as_array().expect("[[key]] fehlt").iter().filter_map(|k| k.get("name").and_then(|n| n.as_str())).collect();
    assert_eq!(namen, ["beispiel-test"], "{}", datei.display());
}

// ---------------------------------------------------------------------------
// Nachträge aus der Prüfung gegen das Zielbild
// ---------------------------------------------------------------------------

/// Unbekannte API-Adressen sind ein 404 als JSON, keine Seite.
#[tokio::test]
async fn unbekannte_api_adresse_gibt_404_json() {
    let res = schicke(&app(), anfrage(Method::GET, "/api/gibt/es/nicht").body(Body::empty()).unwrap()).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    assert!(res.headers()[header::CONTENT_TYPE].to_str().unwrap().starts_with("application/json"));
    let v = json(res).await;
    assert_eq!(v["art"], "nicht-gefunden");
}

/// Seiten-404 mit Accept: application/json ist JSON (dieselbe Fehlerform).
#[tokio::test]
async fn seiten_404_mit_accept_json_ist_json() {
    let res = schicke(
        &app(),
        anfrage(Method::GET, "/projects/schaufenster/assets/fehlt.png").header(header::ACCEPT, "application/json").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let v = json(res).await;
    assert_eq!(v["art"], "nicht-gefunden");
}

/// Zwei Projekte mit demselben hängenden Repo: Die Liste wartet nie auf git
/// (sie liest den Git-Zwischenspeicher), und der Zwischenspeicher fragt das
/// Repo einmal, mit dem Budget von 3 s.
#[tokio::test]
async fn haengende_repos_in_mehreren_projekten_halten_die_liste_nicht_auf() {
    let beispiel = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel-haengt");
    let mut state = AppState::fuer_tests(&beispiel);
    state.zeitlimit = Duration::from_secs(5);
    let git = state.git.clone();
    let app = router(state);
    let start = std::time::Instant::now();
    let res = schicke(&app, anfrage(Method::GET, "/api/projects").body(Body::empty()).unwrap()).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert!(start.elapsed() < Duration::from_secs(1), "{:?}", start.elapsed());
    let start = std::time::Instant::now();
    assert_eq!(git.alle_erneuern().await, 1, "ein Pfad, einmal gefragt");
    let dauer = start.elapsed();
    assert!(dauer >= crate::git::BUDGET && dauer < Duration::from_secs(5), "{dauer:?}");
    let res = schicke(&app, anfrage(Method::GET, "/api/projects").body(Body::empty()).unwrap()).await;
    let v = json(res).await;
    let projekte = v["projects"].as_array().unwrap();
    assert_eq!(projekte.len(), 2);
    for p in projekte {
        let fehler = p["repos"][0]["error"].as_str().unwrap_or("");
        assert!(fehler.contains("Zeitlimit"), "{p}");
    }
}

/// Die Chats melden Vorgänge per curl ohne Browser-Kopfzeilen: für /api/vorgaenge/*
/// gilt die Herkunftspflicht nicht (den Vorgang „1“ gibt es nicht, darum 404, nicht 403).
#[tokio::test]
async fn vorgaenge_duerfen_ohne_herkunft_gemeldet_werden() {
    let req = anfrage(Method::POST, "/api/vorgaenge/1/uebernommen").header(header::CONTENT_TYPE, "application/json").body(Body::empty()).unwrap();
    let res = schicke(&app(), req).await;
    assert_ne!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
    let res = schicke(
        &app(),
        anfrage(Method::POST, "/api/vorgaenge/1/uebernommen")
            .header("origin", "https://boese.example")
            .header("sec-fetch-site", "cross-site")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN, "Browser von fremder Seite bleibt draußen");
}

/// Etappe 2b: auch das Anlegen (`POST /api/vorgaenge`, ohne Schrägstrich am
/// Ende) geht per curl ohne Herkunft; eine fremde Seite bleibt draußen.
#[tokio::test]
async fn vorgang_anlegen_ohne_herkunft_aber_nicht_von_fremden_seiten() {
    let app = app();
    let koerper = r#"{"art": "abnahme", "slug": "dashboard", "titel": "Etappe 2b"}"#;
    let req = anfrage(Method::POST, "/api/vorgaenge").header(header::CONTENT_TYPE, "application/json").body(Body::from(koerper)).unwrap();
    let res = schicke(&app, req).await;
    assert_eq!(res.status(), StatusCode::CREATED);
    for fremd in ["cross-site", "same-site"] {
        let req = anfrage(Method::POST, "/api/vorgaenge").header("sec-fetch-site", fremd).body(Body::from(koerper)).unwrap();
        assert_eq!(schicke(&app, req).await.status(), StatusCode::FORBIDDEN, "{fremd}");
    }
    let req = anfrage(Method::POST, "/api/vorgaenge").header(header::ORIGIN, "https://boese.example").body(Body::from(koerper)).unwrap();
    assert_eq!(schicke(&app, req).await.status(), StatusCode::FORBIDDEN, "fremder Origin");
    // Andere schreibende Wege behalten die Herkunftspflicht.
    let req = anfrage(Method::POST, "/api/vorgaengex").body(Body::empty()).unwrap();
    assert_eq!(schicke(&app, req).await.status(), StatusCode::FORBIDDEN);
}

/// Zweig teilschritte (T3): Die Schritt-Wege gehen per curl ohne Herkunft bis
/// zum Handler (ohne `von` dort 422, nicht 403); eine fremde Seite bleibt
/// draußen (403), auch mit eigenem Origin, wenn der Browser cross-site sagt.
#[tokio::test]
async fn schritt_wege_per_curl_aber_nicht_von_fremden_seiten() {
    let app = app();
    for pfad in ["/api/arbeiten/ar-1/schritte", "/api/arbeiten/ar-1/schritte/1", "/api/arbeiten/ar-1/schritte/1/uebernehmen"] {
        let koerper = if pfad.ends_with("/schritte") { r#"{"titel": "t"}"# } else { "{}" };
        let req = anfrage(Method::POST, pfad).header(header::CONTENT_TYPE, "application/json").body(Body::from(koerper)).unwrap();
        let res = schicke(&app, req).await;
        assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY, "{pfad}");
        assert_eq!(json(res).await["fehler"], crate::api::arbeiten::SATZ_VON, "{pfad}");
        for fremd in ["cross-site", "same-site"] {
            let req = anfrage(Method::POST, pfad)
                .header("sec-fetch-site", fremd)
                .header(header::ORIGIN, EIGENE_HERKUNFT)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"von": "chat:x", "titel": "t"}"#))
                .unwrap();
            assert_eq!(schicke(&app, req).await.status(), StatusCode::FORBIDDEN, "{pfad} {fremd}");
        }
        let req = anfrage(Method::POST, pfad).header(header::ORIGIN, "https://boese.example").body(Body::from("{}")).unwrap();
        assert_eq!(schicke(&app, req).await.status(), StatusCode::FORBIDDEN, "{pfad} fremder Origin");
    }
}

/// Etappe 2b, Nachbesserung: Fremde Seiten kommen auch lesend nicht an die
/// Vorgänge (sonst könnte eine fremde Seite per Warten Vorgänge zustellen lassen).
/// Andere lesende Wege bleiben, wie sie sind: `GET /api/projects` mit
/// `cross-site` antwortet weiter mit 200.
#[tokio::test]
async fn vorgaenge_auch_lesend_nicht_fuer_fremde_seiten() {
    let app = app();
    let warten = "/api/vorgaenge/warten?fuer=x&timeout=0";
    for pfad in [warten, "/api/vorgaenge?fuer=x", "/api/vorgaenge/uebersicht", "/api/vorgaenge/v-1"] {
        for fremd in ["cross-site", "same-site"] {
            let req = anfrage(Method::GET, pfad).header("sec-fetch-site", fremd).body(Body::empty()).unwrap();
            let res = schicke(&app, req).await;
            assert_eq!(res.status(), StatusCode::FORBIDDEN, "{pfad} {fremd}");
            assert_eq!(json(res).await["art"], "abgelehnt", "{pfad} {fremd}");
        }
        for origin in ["https://boese.example", "null", "http://127.0.0.1:87870"] {
            let req = anfrage(Method::GET, pfad).header(header::ORIGIN, origin).body(Body::empty()).unwrap();
            assert_eq!(schicke(&app, req).await.status(), StatusCode::FORBIDDEN, "{pfad} Origin {origin}");
        }
    }
    let req = anfrage(Method::HEAD, warten).header("sec-fetch-site", "cross-site").body(Body::empty()).unwrap();
    assert_eq!(schicke(&app, req).await.status(), StatusCode::FORBIDDEN, "HEAD");
    // Ohne Kopfzeile (curl) und aus dem Dashboard selbst: 200.
    let res = schicke(&app, hole(warten)).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(json(res).await["vorgaenge"], serde_json::json!([]));
    let req = anfrage(Method::GET, warten).header("sec-fetch-site", "same-origin").header(header::ORIGIN, EIGENE_HERKUNFT).body(Body::empty()).unwrap();
    assert_eq!(schicke(&app, req).await.status(), StatusCode::OK, "same-origin");
    // Seit Abnahme 2c gilt die Browser-Regel für alle GET-Wege unter /api,
    // ein fremder Origin allein aber nur unter /api/vorgaenge.
    for fremd in ["cross-site", "same-site"] {
        let req = anfrage(Method::GET, "/api/projects").header("sec-fetch-site", fremd).body(Body::empty()).unwrap();
        assert_eq!(schicke(&app, req).await.status(), StatusCode::FORBIDDEN, "/api/projects {fremd}");
    }
    let req = anfrage(Method::GET, "/api/projects").header(header::ORIGIN, "https://boese.example").body(Body::empty()).unwrap();
    assert_eq!(schicke(&app, req).await.status(), StatusCode::OK, "/api/projects mit fremdem Origin ohne Sec-Fetch-Site");
}

/// Etappe 2b: das Warten ist vom Zeitlimit (Schleuse 5) ausgenommen. Mit einem
/// Zeitlimit von 1 s wartet es trotzdem die verlangten 2 s und antwortet leer.
#[tokio::test]
async fn warten_ist_vom_zeitlimit_ausgenommen() {
    let app = app_mit_zeitlimit(Duration::from_secs(1));
    let start = Instant::now();
    let res = schicke(&app, hole("/api/vorgaenge/warten?fuer=dashboard&timeout=2")).await;
    let dauer = start.elapsed();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(dauer >= Duration::from_millis(1900) && dauer < Duration::from_secs(4), "{dauer:?}");
    let v = json(res).await;
    assert_eq!(v["vorgaenge"], serde_json::json!([]), "{v}");
    assert_eq!(v["cursor"], 0);
    // Die anderen Wege unter /api/vorgaenge behalten das Zeitlimit (hier nur: sie antworten).
    assert_eq!(schicke(&app, hole("/api/vorgaenge?fuer=dashboard")).await.status(), StatusCode::OK);
}

// ---------------------------------------------------------------------------
// Nachbesserungen aus der Abnahme von Etappe 1b
// ---------------------------------------------------------------------------

/// axum-eigene Ablehnungen (hier 405) haben dieselbe Fehlerform wie alle anderen.
#[tokio::test]
async fn falsche_methode_unter_api_ist_json_mit_allow() {
    let req = anfrage(Method::POST, "/api/pc").header(header::ORIGIN, EIGENE_HERKUNFT).body(Body::empty()).unwrap();
    let res = schicke(&app(), req).await;
    assert_eq!(res.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert!(kopf(&res, "allow").contains("GET"), "allow-Kopf fehlt: {:?}", res.headers());
    assert!(kopf(&res, "content-type").starts_with("application/json"));
    let v = json(res).await;
    assert_eq!(v["art"], "methode", "{v}");
    assert_eq!(v["fehler"], "Diese Adresse nimmt diese Art Anfrage nicht an.");
}

/// Auf dem Browserweg wird daraus ein Satz. (`/pc` selbst geht an die Oberfläche,
/// die jede Methode annimmt; `/healthz` ist eine reine GET-Route ohne `/api/`.)
#[tokio::test]
async fn falsche_methode_auf_browser_weg_ist_ein_satz() {
    let req = anfrage(Method::POST, "/healthz").header(header::ORIGIN, EIGENE_HERKUNFT).body(Body::empty()).unwrap();
    let res = schicke(&app(), req).await;
    assert_eq!(res.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert!(kopf(&res, "allow").contains("GET"));
    assert_eq!(kopf(&res, "content-type"), "text/plain; charset=utf-8");
    assert_eq!(text(res).await, "Diese Adresse nimmt diese Art Anfrage nicht an.");
}

/// Ein Browser-Formular, das an der Herkunft scheitert, landet wieder im Formular.
#[tokio::test]
async fn schluessel_formular_von_fremder_seite_zurueck_ins_formular() {
    let req = anfrage(Method::POST, "/schluessel/beispiel-test").header("sec-fetch-site", "cross-site").body(Body::from("wert=abc")).unwrap();
    let res = schicke(&app(), req).await;
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    let ziel = kopf(&res, "location");
    assert!(ziel.starts_with("/schluessel?fehler=Abgelehnt"), "{ziel}");
    assert!(!ziel.contains("abc"), "nie den Wert zurückgeben: {ziel}");
    pruefe_sicherheits_kopfzeilen(&res, "303 statt 403");
}

/// Ebenso ein zu großes Formular (Schleuse 6).
#[tokio::test]
async fn schluessel_formular_zu_gross_zurueck_ins_formular() {
    let req = anfrage(Method::POST, "/schluessel/beispiel-test")
        .header(header::ORIGIN, EIGENE_HERKUNFT)
        .header(header::ACCEPT, "text/html,application/xhtml+xml")
        .header(header::CONTENT_LENGTH, 20000)
        .body(Body::from(vec![b'a'; 20000]))
        .unwrap();
    let res = schicke(&app(), req).await;
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    let ziel = kopf(&res, "location");
    assert!(ziel.starts_with("/schluessel?fehler=Zu%20viele%20Daten"), "{ziel}");
}

/// Ohne Browser-Kopfzeilen (curl, Skripte) bleibt es beim Status selbst.
#[tokio::test]
async fn schluessel_post_ohne_browser_behaelt_den_status() {
    let req = anfrage(Method::POST, "/schluessel/beispiel-test").header(header::ORIGIN, "https://boese.example").body(Body::from("wert=abc")).unwrap();
    assert_eq!(schicke(&app(), req).await.status(), StatusCode::FORBIDDEN);
}

// ---------------------------------------------------------------------------
// Nachbesserungen aus der Abnahme von Etappe 2a
// ---------------------------------------------------------------------------

/// axum lehnt einen Pfad mit kaputtem UTF-8 selbst ab, mit englischem Klartext.
/// Auch daraus wird die feste Fehlerform mit deutschem Satz. Seit dem Hotfix
/// Prozent kommt ein `%` unter /api gar nicht mehr bis axum (Schleuse 3a).
#[tokio::test]
async fn kaputter_pfad_unter_api_ist_json_auf_deutsch() {
    let res = schicke(&app(), hole("/api/projects/%FF")).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert!(kopf(&res, "content-type").starts_with("application/json"), "{:?}", res.headers());
    let v = json(res).await;
    assert_eq!(v["art"], "fehlerhaft", "{v}");
    assert_eq!(v["fehler"], crate::sicherheit::SATZ_PROZENT);
    assert!(!v.to_string().contains("Invalid"), "kein englischer Text: {v}");
}

/// Das Schlüssel-Formular mit kaputtem Namen geht zurück ins Formular.
#[tokio::test]
async fn schluessel_formular_mit_kaputtem_namen_zurueck_ins_formular() {
    let req = anfrage(Method::POST, "/schluessel/%FF")
        .header(header::ORIGIN, EIGENE_HERKUNFT)
        .header(header::ACCEPT, "text/html,application/xhtml+xml")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("wert=abc"))
        .unwrap();
    let res = schicke(&app(), req).await;
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    let ziel = kopf(&res, "location");
    assert_eq!(ziel, "/schluessel?fehler=Die%20Anfrage%20ist%20fehlerhaft.");
    pruefe_sicherheits_kopfzeilen(&res, "303 statt 400");
}

// ---------------------------------------------------------------------------
// Windows-Anmeldung, Probe (7a1): Negotiate über SSPI auf /api/anmeldung/probe
// ---------------------------------------------------------------------------

/// Ohne Nachweis: 401 mit `WWW-Authenticate: Negotiate`, Fehlerform wie überall.
#[tokio::test]
async fn anmeldung_ohne_nachweis_gibt_401_mit_negotiate() {
    let res = schicke(&app(), hole("/api/anmeldung/probe")).await;
    #[cfg(windows)]
    {
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(kopf(&res, "www-authenticate"), "Negotiate");
        pruefe_sicherheits_kopfzeilen(&res, "401 der Anmeldung");
        let v = json(res).await;
        assert_eq!(v["art"], "anmeldung", "{v}");
    }
    #[cfg(not(windows))]
    assert_eq!(res.status(), StatusCode::NOT_IMPLEMENTED);
}

/// Ein anderes Verfahren als Negotiate zählt wie kein Nachweis; unlesbares Base64 ist 403.
#[cfg(windows)]
#[tokio::test]
async fn anmeldung_mit_fremdem_verfahren_oder_unsinn() {
    let req = anfrage(Method::GET, "/api/anmeldung/probe").header(header::AUTHORIZATION, "Basic YTpi").body(Body::empty()).unwrap();
    assert_eq!(schicke(&app(), req).await.status(), StatusCode::UNAUTHORIZED);
    let req = anfrage(Method::GET, "/api/anmeldung/probe").header(header::AUTHORIZATION, "Negotiate !!!!").body(Body::empty()).unwrap();
    let res = schicke(&app(), req).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(json(res).await["art"], "abgelehnt");
}

/// `/api/anmeldung/wer`: SSPI mit dem Paket Negotiate ist da.
#[tokio::test]
async fn anmeldung_wer_meldet_das_paket() {
    let res = schicke(&app(), hole("/api/anmeldung/wer")).await;
    #[cfg(windows)]
    {
        assert_eq!(res.status(), StatusCode::OK);
        let v = json(res).await;
        assert_eq!(v["verfuegbar"], true, "{v}");
        assert_eq!(v["paket"], "Negotiate");
    }
    #[cfg(not(windows))]
    assert_eq!(res.status(), StatusCode::NOT_IMPLEMENTED);
}

/// Die Anfrage kommt über die Verbindung `v` (wie `into_make_service_with_connect_info`).
fn ueber(mut req: Request<Body>, v: crate::anmeldung::verbindung::Verbindung) -> Request<Body> {
    req.extensions_mut().insert(axum::extract::ConnectInfo(v));
    req
}

/// Die ganze Aushandlung wie curl --negotiate, alle Runden über die
/// Verbindung `v`: erst 401 mit Challenge, dann mit dem nächsten Nachweis.
/// Gibt die letzte Antwort und die Zahl der 401-Runden zurück.
#[cfg(windows)]
async fn aushandeln(app: &Router, methode: Method, pfad: &str, v: crate::anmeldung::verbindung::Verbindung) -> (Response, usize) {
    use crate::anmeldung::base64;
    use crate::anmeldung::sspi::client::Client;

    let mut client = Client::neu("HTTP/localhost").unwrap();
    let (mut nachweis, _) = client.schritt(None).unwrap();
    let mut runden_401 = 0;
    for _ in 0..5 {
        let req =
            anfrage(methode.clone(), pfad).header(header::AUTHORIZATION, format!("Negotiate {}", base64::kodieren(&nachweis))).body(Body::empty()).unwrap();
        let res = schicke(app, ueber(req, v)).await;
        let www = kopf(&res, "www-authenticate");
        if res.status() == StatusCode::UNAUTHORIZED {
            runden_401 += 1;
            let challenge = www.strip_prefix("Negotiate ").expect("401 mit Challenge");
            let (n, _) = client.schritt(Some(&base64::dekodieren(challenge).unwrap())).unwrap();
            nachweis = n;
            continue;
        }
        return (res, runden_401);
    }
    panic!("Aushandlung über den Router nach 5 Runden nicht fertig");
}

/// Die ganze Aushandlung über den Router, wie curl --negotiate: erst 401 mit
/// Challenge, dann mit dem nächsten Nachweis 200 und `konto: inhaber` (ohne
/// Einstellungen gilt die SID des eigenen Prozesses als Inhaber).
#[cfg(windows)]
#[tokio::test]
async fn anmeldung_ueber_den_router_als_eigenes_konto() {
    let v = crate::anmeldung::verbindung::Verbindung::neu();
    let (res, runden_401) = aushandeln(&app_ohne_anmeldung(), Method::GET, "/api/anmeldung/probe", v).await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = json(res).await;
    assert_eq!(v["konto"], "inhaber", "{v}");
    assert!(["ntlm", "kerberos"].contains(&v["mechanismus"].as_str().unwrap_or("")), "{v}");
    if v["mechanismus"] == "ntlm" {
        assert_eq!(runden_401, 1, "NTLM: genau eine Runde mit Challenge");
    }
}

// ---------------------------------------------------------------------------
// Schleuse 4b: Windows-Anmeldung als Pflicht (Etappe 7b)
// ---------------------------------------------------------------------------

/// Ohne Nachweis: 401 mit `WWW-Authenticate: Negotiate` und `{fehler, art:
/// "anmeldung"}` auf allen geschützten Wegen, für alle Methoden, auch auf
/// `warten` und dem Live-Kanal (beide ohne Zeitlimit).
#[tokio::test]
async fn ohne_nachweis_401_auf_den_geschuetzten_wegen() {
    let app = app_ohne_anmeldung();
    for pfad in [
        "/api/vorgaenge",
        "/api/vorgaenge/uebersicht",
        "/api/vorgaenge/warten?fuer=dashboard&timeout=5",
        "/api/vorgaenge/v-1",
        "/api/schluessel",
        "/api/daten",
        "/api/daten/export",
        "/api/events",
        "/api/pc/benutzer",
        "/api/pc/verlauf/voll",
        "/api/pc/verlauf/voll?seit=2026-10-01",
        "/api/projekte/dashboard/stand/voll",
        "/api/aufgaben?offen=1",
        "/api/arbeiten",
        "/api/anstehend",
        // Etappe 4d: Entscheidungen und Zielbild.
        "/api/entscheidungen",
        "/api/zielbild",
        // Etappe 6b: die Anleitung.
        "/api/anleitung",
    ] {
        let start = Instant::now();
        let res = schicke(&app, hole(pfad)).await;
        assert!(start.elapsed() < Duration::from_secs(2), "{pfad}: wartet nicht erst");
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{pfad}");
        assert_eq!(kopf(&res, "www-authenticate"), "Negotiate", "{pfad}");
        pruefe_sicherheits_kopfzeilen(&res, pfad);
        let v = json(res).await;
        assert_eq!(v["art"], "anmeldung", "{pfad}: {v}");
        assert_eq!(v["fehler"], "Bitte mit dem Windows-Konto anmelden.", "{pfad}");
    }
    // Schreibend (curl ohne Herkunft, wie die Chats) ebenso.
    for pfad in [
        "/api/vorgaenge",
        "/api/vorgaenge/1/uebernommen",
        "/api/aufgaben",
        "/api/aufgaben/a-1/zurueckgezogen",
        "/api/arbeiten",
        "/api/arbeiten/ar-1",
        // Zweig teilschritte (T3)
        "/api/arbeiten/ar-1/schritte",
        "/api/arbeiten/ar-1/schritte/1",
        "/api/arbeiten/ar-1/schritte/1/uebernehmen",
        "/api/einstufung",
        "/api/antworten/neu-form/konzept",
    ] {
        let req = anfrage(Method::POST, pfad)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"art": "abnahme", "slug": "dashboard", "titel": "x"}"#))
            .unwrap();
        let res = schicke(&app, req).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "POST {pfad}");
        assert_eq!(kopf(&res, "www-authenticate"), "Negotiate");
    }
    // Seiten ohne Browser-Kopfzeilen: 401 mit Satz als Text.
    // Andere Schreibweise: auch geschützt (ohne Route dahinter, aber ohne Ausweg).
    for pfad in ["/schluessel", "/vorgaenge", "/daten", "/API/Vorgaenge"] {
        let res = schicke(&app, hole(pfad)).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{pfad}");
        assert_eq!(kopf(&res, "www-authenticate"), "Negotiate", "{pfad}");
        assert_eq!(text(res).await, "Bitte mit dem Windows-Konto anmelden.", "{pfad}");
    }
}

/// Ein Browser ohne Windows-Anmeldung (etwa Firefox) öffnet eine geschützte
/// Seite über localhost: 401 mit `WWW-Authenticate: Negotiate` (Edge meldet
/// trotzdem an) und eine Seite in einfachem Deutsch mit der Adresse zum
/// Kopieren; die Adresse ist für HTML ersetzt. API-Wege bleiben beim Satz.
#[tokio::test]
async fn browser_ohne_anmeldung_bekommt_eine_seite_in_einfachem_deutsch() {
    let app = app_ohne_anmeldung();
    let seite = Request::get("/schluessel?x=%22%3C")
        .header(header::HOST, "localhost:8787")
        .header("sec-fetch-dest", "document")
        .header(header::USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:140.0) Gecko/20100101 Firefox/140.0")
        .body(Body::empty())
        .unwrap();
    let res = schicke(&app, seite).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(kopf(&res, "www-authenticate"), "Negotiate");
    assert!(kopf(&res, "content-type").starts_with("text/html"));
    let html = text(res).await;
    assert!(html.contains("Diese Seite öffnet sich nur in Microsoft Edge unter"), "{html}");
    assert!(html.contains("Andere Browser schicken deine Windows-Anmeldung nicht mit."), "{html}");
    assert!(html.contains("value=\"http://localhost:8787/schluessel?x=%22%3C\""), "{html}");
    assert!(!html.contains('"') || !html.contains("x=\"<"), "Adresse ist ersetzt");
    let api = Request::get("/api/vorgaenge").header(header::HOST, "localhost:8787").header(header::ACCEPT, "text/html").body(Body::empty()).unwrap();
    let res = schicke(&app, api).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert!(json(res).await["art"] == "anmeldung");
}

#[test]
fn browser_im_protokoll_gekuerzt() {
    let lang = "x".repeat(200);
    let req = Request::get("/").header(header::USER_AGENT, format!("Firefox\t{lang}")).body(Body::empty()).unwrap();
    let kurz = crate::sicherheit::browser_kurz(&req);
    assert!(kurz.starts_with("Firefox") && !kurz.contains('\t') && kurz.ends_with('…'), "{kurz}");
    assert_eq!(kurz.chars().count(), 81);
    let ohne = Request::get("/").body(Body::empty()).unwrap();
    assert_eq!(crate::sicherheit::browser_kurz(&ohne), "ohne Angabe");
}

/// Browser-Navigation über 127.0.0.1 auf eine geschützte Seite: 307 auf
/// localhost (dort meldet Chromium ohne Rückfrage an). Über localhost, unter
/// `/api/*` und für andere Methoden bleibt es 401.
#[tokio::test]
async fn browser_ueber_127_wird_nach_localhost_geleitet() {
    let app = app_ohne_anmeldung();
    let res = schicke(&app, anfrage(Method::GET, "/vorgaenge").header("sec-fetch-dest", "document").body(Body::empty()).unwrap()).await;
    assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(kopf(&res, "location"), "http://localhost:8787/vorgaenge");
    let res = schicke(&app, anfrage(Method::GET, "/schluessel?fehler=x").header(header::ACCEPT, "text/html,*/*").body(Body::empty()).unwrap()).await;
    assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(kopf(&res, "location"), "http://localhost:8787/schluessel?fehler=x");
    let lokal = Request::get("/vorgaenge").header(header::HOST, "localhost:8787").header("sec-fetch-dest", "document").body(Body::empty()).unwrap();
    assert_eq!(schicke(&app, lokal).await.status(), StatusCode::UNAUTHORIZED, "über localhost: anmelden");
    let api = anfrage(Method::GET, "/api/vorgaenge").header("sec-fetch-dest", "document").body(Body::empty()).unwrap();
    assert_eq!(schicke(&app, api).await.status(), StatusCode::UNAUTHORIZED, "API-Wege bleiben 401");
    let post = anfrage(Method::POST, "/schluessel/beispiel-test")
        .header(header::ORIGIN, EIGENE_HERKUNFT)
        .header("sec-fetch-dest", "document")
        .body(Body::from("wert=abc"))
        .unwrap();
    assert_eq!(schicke(&app, post).await.status(), StatusCode::UNAUTHORIZED, "nur GET-Navigationen");
    // Freie Seiten werden nicht umgeleitet.
    let frei = anfrage(Method::GET, "/pc").header("sec-fetch-dest", "document").body(Body::empty()).unwrap();
    assert_ne!(schicke(&app, frei).await.status(), StatusCode::TEMPORARY_REDIRECT);
}

/// Etappe 3b (Fund des Prüfers „127.0.0.1: 6× 401 je Laden“): Die Oberfläche
/// fragt geschützte Adressen nur noch, wenn eine Anmeldung möglich ist
/// (`web/src/lib/zugang.ts`, eine Frage an `/api/vorgaenge/uebersicht` je
/// Laden). Die Schleusen bleiben dabei genau wie vorher: Jede Adresse, die das
/// Layout, die Startseite, /pc, /anstehend oder eine Projektseite sonst fragt,
/// gibt ohne Anmeldung weiter 401 mit `WWW-Authenticate: Negotiate`.
#[tokio::test]
async fn geschuetzte_adressen_der_oberflaeche_bleiben_401() {
    let app = app_ohne_anmeldung();
    for pfad in [
        "/api/vorgaenge/uebersicht",
        "/api/events",
        "/api/aufgaben/anzahl",
        "/api/aufgaben?offen=1",
        "/api/anstehend",
        "/api/schluessel/passkeys/anzahl",
        "/api/meldungen?projekt=dashboard",
        "/api/pc/benutzer",
        "/api/pc/verlauf/voll?seit=2026-10-25",
        "/api/projekte/neu-form/stand/voll",
        // Etappe B2: das Backlog nennt Titel.
        "/api/projekte/neu-form/backlog",
        "/api/antworten",
    ] {
        let res = schicke(&app, hole(pfad)).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{pfad}");
        assert_eq!(kopf(&res, "www-authenticate"), "Negotiate", "{pfad}");
        assert_eq!(json(res).await["art"], "anmeldung", "{pfad}");
    }
}

/// Frei ohne Anmeldung: Projekte, Lebenszeichen, Prüfung, Probe, PC-Werte.
#[tokio::test]
async fn freie_wege_ohne_anmeldung() {
    let app = app_ohne_anmeldung();
    for pfad in ["/api/projects", "/api/projects/dashboard", "/healthz", "/api/betrieb", "/api/pc", "/projects/schaufenster/assets/bild.svg"] {
        assert_eq!(schicke(&app, hole(pfad)).await.status(), StatusCode::OK, "{pfad}");
    }
}

/// Der erste Teil eines SSE-Körpers (er endet nie), höchstens nach 3 s.
async fn erster_teil(res: Response) -> String {
    let mut body = res.into_body();
    let teil = tokio::time::timeout(Duration::from_secs(3), body.frame())
        .await
        .expect("der Kanal schickt binnen 3 s etwas")
        .expect("der Kanal ist offen")
        .expect("ohne Fehler");
    String::from_utf8_lossy(&teil.into_data().unwrap_or_default()).to_string()
}

/// Kopfleiste, Fassung 2: Ohne Anmeldung gibt der freie Kanal 200 (SSE, als
/// Erstes `anmeldung` mit `nicht_angemeldet`), der geschützte 401 mit
/// Negotiate. Kein Zeitlimit auf dem freien Kanal.
#[tokio::test]
async fn freier_kanal_ohne_anmeldung_200_geschuetzter_401() {
    let mut state = AppState::fuer_tests(&beispiel());
    state.zeitlimit = Duration::from_millis(300);
    let app = router(state);
    let res = schicke(&app, hole("/api/events/frei")).await;
    assert_eq!(res.status(), StatusCode::OK);
    let typ = kopf(&res, "content-type");
    assert!(typ.starts_with("text/event-stream"), "{typ}");
    let teil = erster_teil(res).await;
    assert!(teil.starts_with("event: anmeldung\ndata: {\"stand\":\"nicht_angemeldet\"}"), "{teil}");
    let res = schicke(&app, hole("/api/events")).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(kopf(&res, "www-authenticate"), "Negotiate");
    // Nach dem Zeitlimit ist der freie Kanal noch offen.
    let mut body = schicke(&app, hole("/api/events/frei")).await.into_body();
    let _ = body.frame().await;
    match tokio::time::timeout(Duration::from_millis(700), body.frame()).await {
        Err(_) | Ok(Some(Ok(_))) => {}
        Ok(Some(Err(e))) => panic!("freier Kanal mit Fehler abgebrochen: {e}"),
        Ok(None) => panic!("freier Kanal wurde geschlossen"),
    }
    // Darunter bleibt alles geschützt.
    for pfad in ["/api/events/frei/x", "/api/events/frei/"] {
        assert_eq!(schicke(&app, hole(pfad)).await.status(), StatusCode::UNAUTHORIZED, "{pfad}");
    }
}

/// Der freie Kanal liegt hinter allen übrigen Schleusen: fremde Adresse 421,
/// fremde Seite 403, nur GET; angemeldet als Inhaber `angemeldet`.
#[tokio::test]
async fn freier_kanal_hinter_den_schleusen() {
    let app_frei = app_ohne_anmeldung();
    let fremd = Request::get("/api/events/frei").header(header::HOST, "boese.example:8787").body(Body::empty()).unwrap();
    assert_eq!(schicke(&app_frei, fremd).await.status(), StatusCode::MISDIRECTED_REQUEST);
    for seite in ["cross-site", "same-site"] {
        let req = anfrage(Method::GET, "/api/events/frei").header("sec-fetch-site", seite).body(Body::empty()).unwrap();
        assert_eq!(schicke(&app_frei, req).await.status(), StatusCode::FORBIDDEN, "{seite}");
    }
    let req = anfrage(Method::POST, "/api/events/frei").header(header::ORIGIN, EIGENE_HERKUNFT).body(Body::empty()).unwrap();
    assert_eq!(schicke(&app_frei, req).await.status(), StatusCode::METHOD_NOT_ALLOWED);
    #[cfg(windows)]
    {
        let res = schicke(&app(), hole("/api/events/frei")).await;
        assert_eq!(res.status(), StatusCode::OK);
        let teil = erster_teil(res).await;
        assert!(teil.starts_with("event: anmeldung\ndata: {\"stand\":\"angemeldet\"}"), "{teil}");
    }
}

/// `/api/pc` ohne Anmeldung: 200 mit den Rechner-Werten, ohne ein
/// Benutzer-Feld (auch nicht für einen Angemeldeten). `/api/pc/benutzer` ohne
/// Anmeldung: 401 mit `WWW-Authenticate: Negotiate`; angemeldet 200 mit
/// winget und Schlüssel-Zählern.
#[tokio::test]
async fn pc_ohne_anmeldung_ohne_benutzerdaten() {
    let state = AppState::fuer_tests(&beispiel());
    {
        let mut apps = state.sysapps.write().await;
        apps.winget.push(crate::pc::sysapps::WingetEntry { name: "Geheim".into(), id: "x.geheim".into(), version: "1".into(), available: String::new() });
    }
    let felder = |v: &serde_json::Value| -> Vec<String> {
        let mut k: Vec<String> = v.as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default();
        k.sort();
        k
    };
    for app in [router(state.clone()), router_angemeldet(state.clone())] {
        let res = schicke(&app, hole("/api/pc")).await;
        assert_eq!(res.status(), StatusCode::OK);
        let v = json(res).await;
        assert_eq!(felder(&v), ["backup_stand", "data", "data_error", "project_backups", "snapshot", "sysapps"], "{v}");
        assert!(v["snapshot"].is_object(), "Rechner-Werte bleiben");
        assert_eq!(v["backup_stand"]["regel_321"], "nicht-erfuellt", "{v}");
        assert_eq!(v["backup_stand"]["kopien"][2]["ort"], "anderer-ort", "{v}");
        let mut apps = felder(&v["sysapps"]);
        apps.retain(|f| ["autostart", "tools", "winget", "winget_error"].contains(&f.as_str()));
        assert!(apps.is_empty(), "keine Benutzer-Felder in sysapps: {v}");
        // Etappe 3a-1: Hotfixes, Starts und Updates stehen nur im Verlauf; die Laufzeit bleibt.
        assert_eq!(felder(&v["sysapps"]), ["errors", "refreshed_at", "services_running", "services_total", "windows"], "{v}");
        assert!(v["snapshot"]["uptime_secs"].is_u64(), "{v}");
        assert!(!v.to_string().contains("Geheim"), "{v}");
    }

    let res = schicke(&router(state.clone()), hole("/api/pc/benutzer")).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(kopf(&res, "www-authenticate"), "Negotiate");
    assert_eq!(json(res).await["art"], "anmeldung");

    let res = schicke(&router_angemeldet(state), hole("/api/pc/benutzer")).await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = json(res).await;
    assert_eq!(felder(&v), ["bote", "inventory", "schluessel", "stand", "sysapps"], "{v}");
    assert_eq!(v["bote"], "da", "{v}");
    assert_eq!(v["sysapps"]["winget"][0]["name"], "Geheim");
    assert_ne!(v["schluessel"]["vollstaendig"], "unbekannt", "{v}");
    assert!(v["schluessel"]["gespeichert"].is_u64() && v["schluessel"]["fehlen"].is_u64(), "{v}");
}

/// Etappe 3a-4: Die neuen Urteile (Defender, Neustart, Belegung, Frist-Stufe,
/// Art der Verlaufs-Hinweise) sind nur aus Fakten gerechnet, die der Weg
/// schon hatte. Der freie Weg zeigt darum nicht mehr als vorher: `/api/pc`
/// hat ohne und mit Anmeldung genau dieselben Felder, nirgends Schlüssel;
/// der Stand der Schlüssel kommt nur über `/api/pc/benutzer` (ohne Anmeldung
/// 401 ohne Zahlen); der freie Verlauf hat keine Starts, Meldungen und
/// Vorgänge, dafür den Hinweis `ohne-anmeldung`; `/stand` nennt ohne
/// Anmeldung keinen Titel einer Aufgabe, auch nicht mit Frist-Stufe.
#[tokio::test]
async fn neue_urteile_frei_nicht_mehr_als_vorher() {
    let state = AppState::fuer_tests(&beispiel());
    let s = state.speicher.clone().unwrap();
    let morgen = (crate::vertrag::abgeleitet::heute() + chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
    let neu = crate::speicher::aufgaben::NeueAufgabe {
        projekt: "dashboard".into(),
        von: "test".into(),
        titel: "Geheimer Aufgabentitel".into(),
        link: None,
        knopf: None,
        frist: Some(morgen),
        ablauf: None,
        prioritaet: None,
        frist_bezug: None,
    };
    s.aufgabe_anlegen(neu, &crate::speicher::aufgaben::jetzt()).unwrap();
    let schluessel_felder = |v: &serde_json::Value| -> Vec<String> {
        let mut k: Vec<String> = v.as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default();
        k.sort();
        k
    };
    let mut je_weg = Vec::new();
    for app in [router(state.clone()), router_angemeldet(state.clone())] {
        let res = schicke(&app, hole("/api/pc")).await;
        assert_eq!(res.status(), StatusCode::OK);
        let v = json(res).await;
        let text = v.to_string();
        assert!(!text.contains("schluessel") && !text.contains("\"keys\"") && !text.contains("gespeichert"), "{v}");
        assert_eq!(
            schluessel_felder(&v["snapshot"]),
            [
                "belegung_schwellen",
                "cpu_belegung",
                "cpu_count",
                "cpu_percent",
                "defender",
                "disks",
                "history",
                "host",
                "mem_belegung",
                "mem_total",
                "mem_used",
                "neustart",
                "os",
                "taken_at",
                "uptime_secs"
            ],
            "{v}"
        );
        assert!(["in-ordnung", "ansehen", "unbekannt"].contains(&v["snapshot"]["defender"]["befund"].as_str().unwrap()), "{v}");
        assert_eq!(v["snapshot"]["defender"]["signatur_schwelle_tage"], 3);
        assert!(["aussteht", "nicht-noetig", "unbekannt"].contains(&v["snapshot"]["neustart"]["neustart"].as_str().unwrap()), "{v}");
        assert_eq!(v["snapshot"]["belegung_schwellen"], serde_json::json!({"hoch_ab": 70, "kritisch_ab": 90}));
        je_weg.push(schluessel_felder(&v));
    }
    assert_eq!(je_weg[0], je_weg[1], "frei und angemeldet dieselben Felder");

    let res = schicke(&router(state.clone()), hole("/api/pc/benutzer")).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let t = text(res).await;
    assert!(!t.contains("gespeichert") && !t.contains("vollstaendig"), "{t}");

    let res = schicke(&router(state.clone()), hole("/api/pc/verlauf")).await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = json(res).await;
    let h = v["hinweise"].as_array().unwrap();
    assert!(h.iter().all(|h| h["art"].is_string() && h["text"].is_string()), "{v}");
    assert!(h.iter().any(|h| h["art"] == "ohne-anmeldung"), "{v}");
    assert!(!v["ereignisse"].as_array().unwrap().iter().any(|e| ["start", "meldung", "vorgang"].contains(&e["quelle"].as_str().unwrap())), "{v}");
    let res = schicke(&router_angemeldet(state.clone()), hole("/api/pc/verlauf")).await;
    let v = json(res).await;
    assert!(!v["hinweise"].as_array().unwrap().iter().any(|h| h["art"] == "ohne-anmeldung"), "{v}");

    let res = schicke(&router(state.clone()), hole("/api/projekte/dashboard/stand")).await;
    assert_eq!(res.status(), StatusCode::OK);
    let t = text(res).await;
    assert!(!t.contains("Geheimer Aufgabentitel"), "{t}");
    let v: serde_json::Value = serde_json::from_str(&t).unwrap();
    assert_eq!((v["zahlen"]["frist_stufe"].as_str(), v["frist_bald_tage"].as_i64()), (Some("bald"), Some(7)), "{v}");
    assert_eq!(v["eintraege"], serde_json::json!([]), "ohne Anmeldung keine Einträge (Etappe 3a+)");
    let res = schicke(&router_angemeldet(state.clone()), hole("/api/projekte/dashboard/stand")).await;
    assert!(text(res).await.contains("Geheimer Aufgabentitel"));
    let res = schicke(&router(state.clone()), hole("/api/anstehend")).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let res = schicke(&router_angemeldet(state), hole("/api/anstehend")).await;
    let v = json(res).await;
    assert_eq!(v["frist_bald_tage"], 7);
    assert!(v["eintraege"].as_array().unwrap().iter().any(|e| e["frist_stufe"] == "bald"), "{v}");
}

/// Volle Aushandlung auf /api/vorgaenge über eine Verbindung: 200; danach
/// merkt sich die Verbindung den Inhaber (ohne `Authorization` weiter 200), eine
/// andere Verbindung nicht (401).
#[cfg(windows)]
#[tokio::test]
async fn verbindung_merkt_sich_inhaber() {
    use crate::anmeldung::verbindung::Verbindung;

    let app = app_ohne_anmeldung();
    let v = Verbindung::neu();
    let (res, _) = aushandeln(&app, Method::GET, "/api/vorgaenge/uebersicht", v).await;
    assert_eq!(res.status(), StatusCode::OK, "{}", text(res).await);
    let res = schicke(&app, ueber(hole("/api/vorgaenge/uebersicht"), v)).await;
    assert_eq!(res.status(), StatusCode::OK, "dieselbe Verbindung ohne Authorization");
    let res = schicke(&app, ueber(hole("/api/pc/benutzer"), v)).await;
    assert_eq!(res.status(), StatusCode::OK, "angemeldete Verbindung sieht Benutzer-Daten");
    assert_eq!(json(res).await["bote"], "da");
    let andere = Verbindung::neu();
    let res = schicke(&app, ueber(hole("/api/vorgaenge/uebersicht"), andere)).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "andere Verbindung");
    // Ohne Verbindungskennung gibt es kein Gedächtnis.
    assert_eq!(schicke(&app, hole("/api/vorgaenge/uebersicht")).await.status(), StatusCode::UNAUTHORIZED);
    // Eine Fortsetzung ohne offene Aushandlung: 401, neu beginnen.
    let req = anfrage(Method::GET, "/api/vorgaenge").header(header::AUTHORIZATION, "Negotiate oQcwBaADCgEA").body(Body::empty()).unwrap();
    let res = schicke(&app, ueber(req, Verbindung::neu())).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(kopf(&res, "www-authenticate"), "Negotiate");
}

/// Die Aushandlung auf einem freien Weg (der Client fängt selbst an) meldet
/// die Verbindung ebenso an.
#[cfg(windows)]
#[tokio::test]
async fn aushandlung_auf_freiem_weg_meldet_an() {
    use crate::anmeldung::verbindung::Verbindung;

    let app = app_ohne_anmeldung();
    let v = Verbindung::neu();
    let (res, _) = aushandeln(&app, Method::GET, "/api/pc", v).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(schicke(&app, ueber(hole("/api/pc/benutzer"), v)).await.status(), StatusCode::OK);
    assert_eq!(schicke(&app, ueber(hole("/api/schluessel"), v)).await.status(), StatusCode::OK);
}

/// Fremdes Konto (seit 7c): die Prüf-Funktion mit einer fremden SID merkt die
/// Verbindung als fremd. Ohne Ausweis antwortet die Schleuse auf API-Wegen
/// 401 mit Art `passkey` (ohne Negotiate, sonst meldete Edge dasselbe Konto
/// immer wieder an), Browser-Seiten leiten auf `/anmelden` mit dem Ziel.
#[cfg(windows)]
#[tokio::test]
async fn fremdes_konto_ohne_ausweis_muss_zum_passkey() {
    use crate::anmeldung::aushandlung::{erwartete_sid, urteil};
    use crate::anmeldung::verbindung::Verbindung;

    let v = Verbindung::neu();
    let inhaber = erwartete_sid(None).unwrap();
    assert!(!urteil(Some(v.nr), "S-1-5-21-1111111111-2222222222-3333333333-1001", r"PC\fremd", &inhaber));
    let app = app_ohne_anmeldung();
    let res = schicke(&app, ueber(hole("/api/vorgaenge"), v)).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    pruefe_sicherheits_kopfzeilen(&res, "401 fremdes Konto");
    assert!(!kopf(&res, "www-authenticate").contains("Negotiate"), "kein neuer Negotiate-Versuch");
    let j = json(res).await;
    assert_eq!(j["art"], "passkey", "{j}");
    // Eine Browser-Seite geht zur Anmeldung mit Passkey, über localhost, mit Ziel.
    let req = anfrage(Method::GET, "/vorgaenge?x=1").header("sec-fetch-dest", "document").body(Body::empty()).unwrap();
    let res = schicke(&app, ueber(req, v)).await;
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    assert_eq!(kopf(&res, "location"), "http://localhost:8787/anmelden?ziel=/vorgaenge%3Fx%3D1");
    // Freie Wege bleiben für die fremde Verbindung frei, ohne Benutzer-Daten.
    let res = schicke(&app, ueber(hole("/api/pc"), v)).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(schicke(&app, ueber(hole("/api/pc/benutzer"), v)).await.status(), StatusCode::UNAUTHORIZED);
    // Die Anträge (Windows-Anmeldung mit irgendeinem Konto) gehen.
    let res = schicke(&app, ueber(hole("/api/passkey/antrag"), v)).await;
    assert_eq!(res.status(), StatusCode::OK);
    let j = json(res).await;
    assert_eq!((j["konto"].as_str(), j["hauptkonto"].as_bool()), (Some(r"PC\fremd"), Some(false)), "{j}");
}

#[test]
fn liste_der_geschuetzten_wege() {
    use crate::sicherheit::geschuetzt;
    for p in [
        "/schluessel",
        "/schluessel/beispiel-test",
        "/api/schluessel",
        "/api/schluessel/x",
        "/vorgaenge",
        "/vorgaenge/",
        "/api/vorgaenge",
        "/api/vorgaenge/warten",
        "/api/vorgaenge/v-1/erledigt",
        "/daten",
        "/api/daten",
        "/api/daten/export",
        "/api/events",
        "/api/pc/benutzer",
        "/API/PC/Benutzer",
        "/api/pc/verlauf/voll",
        "/API/PC/Verlauf/Voll",
        "/api/projekte/dashboard/stand/voll",
        "/API/Projekte/Dashboard/Stand/Voll",
        "/api/projekte/x/stand/voll/mehr",
        // Etappe B2: das Backlog eines Projekts.
        "/api/projekte/dashboard/backlog",
        "/API/Projekte/Dashboard/Backlog",
        // Etappe B2: die Fragen an den Inhaber (persönlicher Inhalt).
        "/api/projekte/dashboard/daten/fragen",
        "/API/Projekte/Dashboard/Daten/Fragen",
        "/Api/Events",
        "/api/events/frei/x",
        "/api/betrieb/voll",
        "/api/aufgaben",
        "/api/aufgaben/a-20261028-0001/erledigt",
        "/API/Aufgaben",
        "/api/arbeiten",
        "/api/arbeiten/ar-20261029-0001",
        "/api/arbeiten/ar-20261029-0001/schritte/1/uebernehmen",
        "/API/Arbeiten/ar-1/Schritte",
        "/api/einstufung",
        "/api/anstehend",
        "/API/Anstehend",
        "/api/antworten",
        "/api/antworten/neu-form/schlusssatz",
        "/API/Antworten",
        "/api/schluessel/passkeys",
        "/api/schluessel/passkeys/abc/freigeben",
        "/api/schluessel/sitzungen-beenden",
        // Etappe 4d: System-Seiten mit Projekten und Plänen.
        "/api/entscheidungen",
        "/API/Entscheidungen",
        "/api/zielbild",
        "/api/zielbild/x",
        // Etappe 6b: die Anleitung, geschützt wie das Zielbild.
        "/api/anleitung",
        "/API/Anleitung",
        "/api/anleitung/x",
    ] {
        assert!(geschuetzt(p), "{p}");
    }
    for p in [
        "/",
        "/pc",
        "/projects/schaufenster",
        "/api/projects",
        "/api/pc",
        "/api/betrieb",
        "/healthz",
        "/api/pruefung",
        "/api/anmeldung/probe",
        "/api/anmeldung/wer",
        "/_app/immutable/a.js",
        "/schluesselx",
        "/api/vorgaengex",
        "/datenschutz",
        "/api/eventsx",
        // Kopfleiste, Fassung 2: genau der freie Kanal.
        "/api/events/frei",
        "/api/pc/benutzerx",
        "/api/pc/verlauf",
        "/api/pc/verlauf/vollx",
        "/api/aufgabenx",
        "/api/anstehendx",
        "/api/antwortenx",
        "/api/projekte/dashboard/stand",
        "/api/projekte/dashboard/stand/vollx",
        "/api/projekte/dashboard/standx/voll",
        "/api/projekte/dashboard/backlogx",
        "/api/projekte/dashboard/daten/kanban",
        "/api/projekte/dashboard/daten/fragenx",
        "/anmelden",
        "/api/passkey/stand",
        "/api/passkey/anmelden/beginn",
        "/api/passkey/antrag",
        // Die Seiten selbst bleiben frei (wie /anstehend); nur ihre Daten nicht.
        "/entscheidungen",
        "/zielbild",
        "/anleitung",
        "/api/entscheidungenx",
        "/api/zielbildx",
        "/api/anleitungx",
    ] {
        assert!(!geschuetzt(p), "{p}");
    }
}
