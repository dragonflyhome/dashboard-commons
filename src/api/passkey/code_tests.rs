//! Schritt 7b der macOS-Folge: die Wege `code/beginn` und `code/ende` und
//! der Kanalbefehl `code`, über den Router und mit dem Prüf-Authenticator.
//! Nur wo es den lokalen Kanal gibt.

use std::net::{IpAddr, Ipv4Addr};
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use axum::{Extension, Router};
use http_body_util::BodyExt;
use tower::ServiceExt;

use super::*;
use crate::app::{beispiel_fuer_tests, router};
use crate::passkey::einmalcode::{FEHLVERSUCHE, SATZ_GILT_NICHT};
use crate::passkey::webauthn::pruef_authenticator::PruefAuthenticator;
use crate::speicher::passkeys::{TITEL_CODE_AUSGEGEBEN, TITEL_CODE_VERFALLEN, titel_code_passkey};
use crate::vertrag::api::{VorgangsArt, Zustand};
use crate::vertrag::passkey::PasskeyStatus;

const HERKUNFT: &str = "http://localhost:8787";
const LOKAL: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);
const BEGINN: &str = "/api/passkey/code/beginn";
const ENDE: &str = "/api/passkey/code/ende";

fn zustand() -> AppState {
    AppState::fuer_tests(&beispiel_fuer_tests())
}

/// Der Router über eine Verbindung von diesem Rechner.
fn lokal(state: &AppState) -> Router {
    ueber(state, Verbindung::neu().mit_gegenstelle(LOKAL))
}

fn ueber(state: &AppState, verbindung: Verbindung) -> Router {
    router(state.clone()).layer(Extension(ConnectInfo(verbindung)))
}

fn post(pfad: &str, koerper: serde_json::Value) -> Request<Body> {
    Request::post(pfad)
        .header(header::HOST, "localhost:8787")
        .header(header::ORIGIN, HERKUNFT)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(koerper.to_string()))
        .unwrap()
}

fn beginn(code: &str) -> Request<Body> {
    post(BEGINN, serde_json::json!({ "code": code, "name": "Laptop" }))
}

async fn schicke(app: &Router, req: Request<Body>) -> (StatusCode, Option<String>, serde_json::Value) {
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let set = res.headers().get(header::SET_COOKIE).map(|v| v.to_str().unwrap().to_string());
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, set, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
}

/// Gibt einen Code aus, wie der Kanal es täte, und liest ihn aus der Antwortzeile.
async fn code_holen(state: &AppState) -> String {
    let zeile = einmal_code_ausgeben(state, &crate::kanal::konto_fuer_tests()).await;
    let antwort: serde_json::Value = serde_json::from_str(&zeile).unwrap();
    antwort["code"].as_str().unwrap_or_else(|| panic!("kein Code: {zeile}")).to_string()
}

fn gilt_nicht() -> serde_json::Value {
    serde_json::json!({ "fehler": SATZ_GILT_NICHT, "art": "einmal-code" })
}

fn nur_lokal_antwort() -> serde_json::Value {
    serde_json::json!({ "fehler": SATZ_NUR_LOKAL, "art": "nur-lokal" })
}

/// Titel der Vorgänge der Art `einmal-code`, alle erledigt.
fn code_vorgaenge(state: &AppState) -> Vec<String> {
    let alle = state.speicher.as_ref().unwrap().vorgaenge_alle().unwrap();
    let meine: Vec<_> = alle.into_iter().filter(|v| v.art == VorgangsArt::EinmalCode).collect();
    assert!(meine.iter().all(|v| v.zustand == Zustand::Erledigt && v.slug == "passkey" && v.von == "server"), "{meine:?}");
    meine.into_iter().map(|v| v.titel).collect()
}

/// Schlüssel und Titel der Meldungen zum Einmal-Code.
fn code_meldungen(state: &AppState) -> Vec<(String, String)> {
    let alle = state.speicher.as_ref().unwrap().meldungen_lesen(Some("dashboard")).unwrap();
    let meine: Vec<_> = alle.into_iter().filter(|m| m.schluessel.starts_with("dashboard/einmal-code")).collect();
    assert!(meine.iter().all(|m| m.art == crate::vertrag::anstehend::MeldungsArt::Info && m.system), "{meine:?}");
    meine.into_iter().map(|m| (m.schluessel, m.titel)).collect()
}

/// Wartet, bis `f` gilt (die Meldungen und der Verfall entstehen in eigenen Aufgaben).
async fn warte_bis(was: &str, f: impl Fn() -> bool) {
    for _ in 0..200 {
        if f() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("nicht eingetreten: {was}");
}

/// Legt mit einem Code einen Passkey an: beginn, Authenticator, ende.
async fn anlegen(app: &Router, code: &str, a: &PruefAuthenticator) -> (StatusCode, serde_json::Value) {
    let (s, _, o) = schicke(app, beginn(code)).await;
    if s != StatusCode::OK {
        return (s, o);
    }
    let (c, att) = a.anlegen(o["challenge"].as_str().unwrap(), HERKUNFT);
    let k = serde_json::json!({ "id": a.id_b64(), "client_daten": passkey::b64(&c), "attestation": passkey::b64(&att) });
    let (s, _, j) = schicke(app, post(ENDE, k)).await;
    (s, j)
}

#[tokio::test(flavor = "multi_thread")]
async fn guter_weg_passkey_im_hauptkonto_registriert_mit_journal_und_meldung() {
    let state = zustand();
    let app = lokal(&state);
    let code = code_holen(&state).await;
    let mut a = PruefAuthenticator::es256();
    let (s, j) = anlegen(&app, &code, &a).await;
    assert_eq!((s, j["status"].as_str()), (StatusCode::CREATED, Some("registriert")), "{j}");
    // Sofort registriert, im Hauptkonto, mit festem Namen und der eigenen Benutzernummer.
    let alle = state.speicher.as_ref().unwrap().passkeys().unwrap();
    assert_eq!(alle.len(), 1);
    let p = &alle[0];
    assert_eq!((p.status, p.hauptkonto, p.konto.as_str(), p.name.as_str()), (PasskeyStatus::Registriert, true, "Hauptkonto", "Laptop"));
    assert_eq!(p.konto_sid, format!("uid:{}", crate::unix::eigene_uid()));
    // Journal: Ausgabe und Anlegen, beide erledigt, ohne den Code.
    assert_eq!(code_vorgaenge(&state), [TITEL_CODE_AUSGEGEBEN.to_string(), titel_code_passkey("Laptop")]);
    assert_eq!(titel_code_passkey("Laptop"), "Passkey „Laptop“ mit Einmal-Code angelegt");
    // Meldungen: je eine, zur Kenntnis, vom System.
    warte_bis("zwei Meldungen", || code_meldungen(&state).len() == 2).await;
    let mut meldungen = code_meldungen(&state);
    meldungen.sort();
    assert_eq!(
        meldungen,
        [("dashboard/einmal-code".to_string(), TITEL_CODE_AUSGEGEBEN.to_string()), ("dashboard/einmal-code-passkey".to_string(), titel_code_passkey("Laptop")),]
    );
    // Angemeldet ist damit noch niemand; die Anmeldung geht danach mit dem Passkey.
    let geschuetzt = Request::get("/api/vorgaenge/uebersicht").header(header::HOST, "localhost:8787").body(Body::empty()).unwrap();
    assert_eq!(schicke(&app, geschuetzt).await.0, StatusCode::UNAUTHORIZED);
    let (_, _, o) = schicke(&app, post("/api/passkey/anmelden/beginn", serde_json::json!({}))).await;
    let (c, auth, sig) = a.anmelden(o["challenge"].as_str().unwrap(), HERKUNFT);
    let k = serde_json::json!({ "id": a.id_b64(), "client_daten": passkey::b64(&c), "auth_daten": passkey::b64(&auth), "signatur": passkey::b64(&sig) });
    let (s, set, j) = schicke(&app, post("/api/passkey/anmelden/ende", k)).await;
    assert_eq!(s, StatusCode::OK, "{j}");
    let ausweis = set.unwrap().strip_prefix(&format!("{}=", cookie::NAME)).unwrap().split(';').next().unwrap().to_string();
    let mit = Request::get("/api/vorgaenge/uebersicht")
        .header(header::HOST, "localhost:8787")
        .header(header::COOKIE, format!("{}={ausweis}", cookie::NAME))
        .body(Body::empty())
        .unwrap();
    assert_eq!(schicke(&app, mit).await.0, StatusCode::OK);
}

/// Beide Wege setzen nie ein Cookie, weder im guten noch im schlechten Fall.
#[tokio::test(flavor = "multi_thread")]
async fn kein_cookie_auf_beiden_wegen() {
    let state = zustand();
    let app = lokal(&state);
    let (s, set, _) = schicke(&app, beginn("falsch")).await;
    assert_eq!((s, set), (StatusCode::FORBIDDEN, None));
    let code = code_holen(&state).await;
    let (s, set, o) = schicke(&app, beginn(&code)).await;
    assert_eq!((s, set), (StatusCode::OK, None));
    let a = PruefAuthenticator::es256();
    let (c, att) = a.anlegen(o["challenge"].as_str().unwrap(), HERKUNFT);
    let k = serde_json::json!({ "id": a.id_b64(), "client_daten": passkey::b64(&c), "attestation": passkey::b64(&att) });
    let (s, set, _) = schicke(&app, post(ENDE, k.clone())).await;
    assert_eq!((s, set), (StatusCode::CREATED, None));
    // Dieselbe Antwort noch einmal: abgewiesen, wieder ohne Cookie.
    let (s, set, _) = schicke(&app, post(ENDE, k)).await;
    assert!(s.is_client_error(), "{s}");
    assert_eq!(set, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn falscher_code_und_kein_offener_code_bekommen_dieselbe_antwort() {
    let state = zustand();
    let app = lokal(&state);
    // Kein Code offen.
    let (s_ohne, _, j_ohne) = schicke(&app, beginn("0000-0000-0000-0000-0000-0000-00")).await;
    // Ein Code offen, ein anderer eingegeben.
    let code = code_holen(&state).await;
    let (s_falsch, _, j_falsch) = schicke(&app, beginn("0000-0000-0000-0000-0000-0000-00")).await;
    assert_eq!((s_ohne, &j_ohne), (StatusCode::FORBIDDEN, &gilt_nicht()));
    assert_eq!((s_falsch, &j_falsch), (s_ohne, &j_ohne));
    // Der offene Code gilt danach noch.
    assert_eq!(schicke(&app, beginn(&code)).await.0, StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread")]
async fn code_ist_nach_beginn_verbraucht_auch_ohne_ende() {
    let state = zustand();
    let app = lokal(&state);
    let code = code_holen(&state).await;
    assert_eq!(schicke(&app, beginn(&code)).await.0, StatusCode::OK);
    // Kein `ende`. Der Code gilt trotzdem nicht noch einmal.
    let (s, _, j) = schicke(&app, beginn(&code)).await;
    assert_eq!((s, j), (StatusCode::FORBIDDEN, gilt_nicht()));
    assert!(state.speicher.as_ref().unwrap().passkeys().unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn zwei_gleichzeitige_beginn_mit_demselben_code_genau_ein_erfolg() {
    let state = zustand();
    let app = lokal(&state);
    for _ in 0..5 {
        let code = code_holen(&state).await;
        let (a, b) = tokio::join!(schicke(&app, beginn(&code)), schicke(&app, beginn(&code)));
        let mut status = [a.0, b.0];
        status.sort();
        assert_eq!(status, [StatusCode::OK, StatusCode::FORBIDDEN]);
        // Der Verlierer bekommt denselben Satz wie bei einem falschen Code.
        let verlierer = if a.0 == StatusCode::OK { b.2 } else { a.2 };
        assert_eq!(verlierer, gilt_nicht());
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn nach_fuenf_fehlversuchen_verfallen_mit_vorgang_und_meldung() {
    let state = zustand();
    let app = lokal(&state);
    let code = code_holen(&state).await;
    for _ in 0..FEHLVERSUCHE {
        let (s, _, j) = schicke(&app, beginn("falsch")).await;
        assert_eq!((s, j), (StatusCode::FORBIDDEN, gilt_nicht()));
    }
    // Auch der richtige Code wird jetzt abgewiesen, mit derselben Antwort.
    let (s, _, j) = schicke(&app, beginn(&code)).await;
    assert_eq!((s, j), (StatusCode::FORBIDDEN, gilt_nicht()));
    // Vorgang und Meldung „verfallen“ liegen danach vor (eigene Aufgabe nach der Antwort).
    warte_bis("Vorgang verfallen", || code_vorgaenge(&state).iter().any(|t| t == TITEL_CODE_VERFALLEN)).await;
    assert_eq!(code_vorgaenge(&state), [TITEL_CODE_AUSGEGEBEN, TITEL_CODE_VERFALLEN]);
    assert_eq!(TITEL_CODE_VERFALLEN, "Einmal-Code nach 5 Fehlversuchen verfallen");
    warte_bis("Meldung verfallen", || code_meldungen(&state).iter().any(|(k, _)| k == "dashboard/einmal-code-verfallen")).await;
    // Weitere falsche Eingaben ohne offenen Code lassen nichts mehr verfallen.
    for _ in 0..FEHLVERSUCHE {
        schicke(&app, beginn("falsch")).await;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(code_vorgaenge(&state).iter().filter(|t| *t == TITEL_CODE_VERFALLEN).count(), 1);
}

/// Was an den Prüfungen der Anfrage scheitert, berührt den Code nicht: Es
/// zählt nicht als Fehlversuch und verbraucht nichts.
#[tokio::test(flavor = "multi_thread")]
async fn abweisungen_vor_dem_code_zaehlen_nicht_und_verbrauchen_nichts() {
    let state = zustand();
    let code = code_holen(&state).await;
    let koerper = || Body::from(serde_json::json!({ "code": code, "name": "Laptop" }).to_string());
    let roh = || Request::post(BEGINN).header(header::CONTENT_TYPE, "application/json");
    let gut = || roh().header(header::HOST, "localhost:8787").header(header::ORIGIN, HERKUNFT);
    let lokal_app = lokal(&state);
    let faelle: Vec<(&str, Router, Request<Body>)> = vec![
        (
            "Origin 127.0.0.1",
            lokal_app.clone(),
            roh().header(header::HOST, "localhost:8787").header(header::ORIGIN, "http://127.0.0.1:8787").body(koerper()).unwrap(),
        ),
        ("ohne Origin", lokal_app.clone(), roh().header(header::HOST, "localhost:8787").body(koerper()).unwrap()),
        (
            "fremde Seite",
            lokal_app.clone(),
            roh().header(header::HOST, "localhost:8787").header(header::ORIGIN, "http://evil.example").body(koerper()).unwrap(),
        ),
        ("cross-site", lokal_app.clone(), gut().header("sec-fetch-site", "cross-site").header(header::ORIGIN, "http://evil.example").body(koerper()).unwrap()),
        ("Host 127.0.0.1", lokal_app.clone(), roh().header(header::HOST, "127.0.0.1:8787").header(header::ORIGIN, HERKUNFT).body(koerper()).unwrap()),
        ("Host [::1]", lokal_app.clone(), roh().header(header::HOST, "[::1]:8787").header(header::ORIGIN, HERKUNFT).body(koerper()).unwrap()),
        ("keine Gegenstelle", ueber(&state, Verbindung::neu()), gut().body(koerper()).unwrap()),
        ("Gegenstelle nicht Loopback", ueber(&state, Verbindung::neu().mit_gegenstelle("192.0.2.7".parse().unwrap())), gut().body(koerper()).unwrap()),
        ("Forwarded", lokal_app.clone(), gut().header("forwarded", "for=127.0.0.1").body(koerper()).unwrap()),
        ("X-Forwarded-For", lokal_app.clone(), gut().header("x-forwarded-for", "127.0.0.1").body(koerper()).unwrap()),
        ("X-Forwarded-Host", lokal_app.clone(), gut().header("x-forwarded-host", "localhost:8787").body(koerper()).unwrap()),
        ("Via", lokal_app.clone(), gut().header("via", "1.1 zwischen").body(koerper()).unwrap()),
    ];
    // Mehr Abweisungen als Fehlversuche erlaubt wären: Zählten sie, wäre der Code danach verfallen.
    assert!(faelle.len() > FEHLVERSUCHE as usize);
    for (name, app, req) in faelle {
        let (s, set, j) = schicke(&app, req).await;
        assert!(s.is_client_error(), "{name}: {s} {j}");
        assert_ne!(j, gilt_nicht(), "{name}: die Anfrage kam bis zum Code");
        assert_eq!(set, None, "{name}");
        assert!(!j.to_string().contains(&code), "{name}: {j}");
    }
    assert!(code_vorgaenge(&state).iter().all(|t| t != TITEL_CODE_VERFALLEN));
    // Der Code gilt danach noch.
    assert_eq!(schicke(&lokal_app, beginn(&code)).await.0, StatusCode::OK);
}

/// Dieselben Prüfungen an den Wegen selbst, mit ihrem festen Satz (die
/// Schleuse davor lässt 127.0.0.1 und Kopfzeilen von Zwischenstationen durch).
#[tokio::test(flavor = "multi_thread")]
async fn beide_wege_weisen_nicht_lokale_anfragen_mit_festem_satz_ab() {
    let state = zustand();
    let app = lokal(&state);
    for pfad in [BEGINN, ENDE] {
        let k = serde_json::json!({ "code": "x", "name": "Laptop" }).to_string();
        let via = Request::post(pfad)
            .header(header::HOST, "localhost:8787")
            .header(header::ORIGIN, HERKUNFT)
            .header(header::CONTENT_TYPE, "application/json")
            .header("via", "1.1 zwischen")
            .body(Body::from(k.clone()))
            .unwrap();
        assert_eq!(schicke(&app, via).await, (StatusCode::FORBIDDEN, None, nur_lokal_antwort()), "{pfad}");
        let ohne_gegenstelle = post(pfad, serde_json::json!({ "code": "x", "name": "Laptop" }));
        assert_eq!(schicke(&ueber(&state, Verbindung::neu()), ohne_gegenstelle).await, (StatusCode::FORBIDDEN, None, nur_lokal_antwort()), "{pfad}");
        let host = Request::post(pfad)
            .header(header::HOST, "127.0.0.1:8787")
            .header(header::ORIGIN, HERKUNFT)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(k))
            .unwrap();
        assert_eq!(schicke(&app, host).await, (StatusCode::FORBIDDEN, None, nur_lokal_antwort()), "{pfad}");
    }
}

/// Ein mitgeschickter Ausweis oder ein Konto in der Anfrage ändert nichts:
/// Ohne Code gibt es keine Challenge.
#[tokio::test(flavor = "multi_thread")]
async fn ein_ausweis_ersetzt_den_code_nicht() {
    let state = zustand();
    let app = lokal(&state);
    // Ein gültiger Ausweis: Passkey über einen Code anlegen und damit anmelden.
    let code = code_holen(&state).await;
    let mut a = PruefAuthenticator::es256();
    assert_eq!(anlegen(&app, &code, &a).await.0, StatusCode::CREATED);
    let (_, _, o) = schicke(&app, post("/api/passkey/anmelden/beginn", serde_json::json!({}))).await;
    let (c, auth, sig) = a.anmelden(o["challenge"].as_str().unwrap(), HERKUNFT);
    let k = serde_json::json!({ "id": a.id_b64(), "client_daten": passkey::b64(&c), "auth_daten": passkey::b64(&auth), "signatur": passkey::b64(&sig) });
    let (s, set, _) = schicke(&app, post("/api/passkey/anmelden/ende", k)).await;
    assert_eq!(s, StatusCode::OK);
    let ausweis = set.unwrap().split(';').next().unwrap().to_string();
    // Mit dem Ausweis, aber ohne gültigen Code: dieselbe Abweisung wie ohne Ausweis.
    let mut req = beginn("falsch");
    req.headers_mut().insert(header::COOKIE, ausweis.parse().unwrap());
    let (s, set, j) = schicke(&app, req).await;
    assert_eq!((s, set, j), (StatusCode::FORBIDDEN, None, gilt_nicht()));
    assert_eq!(state.speicher.as_ref().unwrap().passkeys().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn ein_code_in_der_adresse_wird_nicht_gelesen() {
    let state = zustand();
    let app = lokal(&state);
    let code = code_holen(&state).await;
    let req = Request::post(format!("{BEGINN}?code={code}"))
        .header(header::HOST, "localhost:8787")
        .header(header::ORIGIN, HERKUNFT)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::json!({ "code": "falsch", "name": "Laptop" }).to_string()))
        .unwrap();
    let (s, _, j) = schicke(&app, req).await;
    assert_eq!((s, j), (StatusCode::FORBIDDEN, gilt_nicht()));
    // Ohne Code im Körper: kein passendes JSON, der Code in der Adresse hilft nicht.
    let req = Request::post(format!("{BEGINN}?code={code}"))
        .header(header::HOST, "localhost:8787")
        .header(header::ORIGIN, HERKUNFT)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::json!({ "name": "Laptop" }).to_string()))
        .unwrap();
    assert!(schicke(&app, req).await.0.is_client_error());
}

/// Was den Code nicht berührt, kommt zuerst: Bei unlesbarem Körper oder
/// falschem Namen ist der Code danach noch da.
#[tokio::test(flavor = "multi_thread")]
async fn unlesbarer_koerper_und_falscher_name_verbrauchen_den_code_nicht() {
    let state = zustand();
    let app = lokal(&state);
    let code = code_holen(&state).await;
    for koerper in [
        serde_json::json!({ "code": code, "name": "" }),
        serde_json::json!({ "code": code, "name": "x".repeat(200) }),
        serde_json::json!({ "code": code, "name": "Laptop", "mehr": 1 }),
        serde_json::json!({ "code": code }),
        serde_json::json!([code]),
    ] {
        let (s, _, j) = schicke(&app, post(BEGINN, koerper)).await;
        assert!(s.is_client_error() && j != gilt_nicht(), "{s} {j}");
        assert!(!j.to_string().contains(&code), "{j}");
    }
    assert_eq!(schicke(&app, beginn(&code)).await.0, StatusCode::OK);
}

/// Ohne Schlüssel der Ausweise scheitert `beginn`, bevor der Code berührt wird.
#[tokio::test(flavor = "multi_thread")]
async fn ohne_schluessel_der_ausweise_bleibt_der_code() {
    let mit = zustand();
    let code = code_holen(&mit).await;
    let mut ohne = mit.clone();
    ohne.jwt = None;
    let (s, _, j) = schicke(&lokal(&ohne), beginn(&code)).await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE, "{j}");
    // Derselbe Code-Stand (geteilt): Der Code gilt noch.
    assert_eq!(schicke(&lokal(&mit), beginn(&code)).await.0, StatusCode::OK);
}

/// Eine Challenge aus `anmelden/beginn` taugt nicht für `code/ende`.
#[tokio::test(flavor = "multi_thread")]
async fn challenge_aus_anmelden_taugt_nicht_fuer_code_ende() {
    let state = zustand();
    let app = lokal(&state);
    let (_, _, o) = schicke(&app, post("/api/passkey/anmelden/beginn", serde_json::json!({}))).await;
    let a = PruefAuthenticator::es256();
    let (c, att) = a.anlegen(o["challenge"].as_str().unwrap(), HERKUNFT);
    let k = serde_json::json!({ "id": a.id_b64(), "client_daten": passkey::b64(&c), "attestation": passkey::b64(&att) });
    let (s, set, j) = schicke(&app, post(ENDE, k)).await;
    assert!(s.is_client_error(), "{s} {j}");
    assert_eq!(set, None);
    assert!(state.speicher.as_ref().unwrap().passkeys().unwrap().is_empty());
}

/// Eine Aufgabe zum Anlegen im Hauptkonto, die nicht aus `code/beginn`
/// stammt (`einmal_code` fehlt), weist `code/ende` ab, auch wenn Konto und
/// `hauptkonto` passen.
#[tokio::test(flavor = "multi_thread")]
async fn aufgabe_ohne_einmal_code_wird_von_code_ende_abgewiesen() {
    let state = zustand();
    let app = lokal(&state);
    let speicher = state.speicher.clone().unwrap();
    let j = jetzt();
    let ohne = PasskeyAufgabe::anlegen(&code_konto(), "Laptop", &j);
    assert!(!ohne.einmal_code);
    let challenge = speicher.passkey_aufgabe_anlegen(&ohne, &j).unwrap();
    let a = PruefAuthenticator::es256();
    let (c, att) = a.anlegen(&challenge, HERKUNFT);
    let k = serde_json::json!({ "id": a.id_b64(), "client_daten": passkey::b64(&c), "attestation": passkey::b64(&att) });
    let (s, _, antwort) = schicke(&app, post(ENDE, k)).await;
    assert!(s.is_client_error(), "{s} {antwort}");
    assert!(speicher.passkeys().unwrap().is_empty());
    // Gegenprobe: Dieselbe Aufgabe mit dem Merkmal wird angenommen.
    let mit = PasskeyAufgabe::anlegen(&code_konto(), "Laptop", &j).mit_einmal_code(true);
    let challenge = speicher.passkey_aufgabe_anlegen(&mit, &j).unwrap();
    let (c, att) = a.anlegen(&challenge, HERKUNFT);
    let k = serde_json::json!({ "id": a.id_b64(), "client_daten": passkey::b64(&c), "attestation": passkey::b64(&att) });
    let (s, _, antwort) = schicke(&app, post(ENDE, k)).await;
    assert_eq!(s, StatusCode::CREATED, "{antwort}");
}

// ----- der Kanalbefehl `code` -----

#[tokio::test(flavor = "multi_thread")]
async fn kanal_gibt_code_gueltigkeit_und_port_und_schreibt_erst_ins_journal() {
    let state = zustand();
    let zeile = einmal_code_ausgeben(&state, &crate::kanal::konto_fuer_tests()).await;
    let antwort: serde_json::Value = serde_json::from_str(&zeile).unwrap();
    let code = antwort["code"].as_str().unwrap();
    assert_eq!(antwort, serde_json::json!({ "code": code, "gueltig_sekunden": 600, "port": 8787 }));
    // Form: 26 Zeichen in Gruppen zu vier.
    assert_eq!(code.len(), 32);
    assert_eq!(crate::passkey::einmalcode::normieren(code).len(), 26);
    // Der Vorgang steht schon da, wenn die Antwort kommt; der Code steht nicht darin.
    let titel = code_vorgaenge(&state);
    assert_eq!(titel, [TITEL_CODE_AUSGEGEBEN]);
    assert_eq!(TITEL_CODE_AUSGEGEBEN, "Einmal-Code ausgegeben");
    warte_bis("Meldung", || code_meldungen(&state).len() == 1).await;
    assert!(state.einmal_code.lock().unwrap().offen());
}

#[tokio::test(flavor = "multi_thread")]
async fn kanal_ohne_schluessel_der_ausweise_fester_satz_und_kein_code() {
    let mut state = zustand();
    state.jwt = None;
    let zeile = einmal_code_ausgeben(&state, &crate::kanal::konto_fuer_tests()).await;
    assert_eq!(zeile, crate::kanal::fehler_antwort(SATZ_CODE_OHNE_SCHLUESSEL));
    assert!(!state.einmal_code.lock().unwrap().offen());
    assert!(code_vorgaenge(&state).is_empty());
}

/// Ohne Datenspeicher gibt es kein Journal und darum keinen Code.
#[tokio::test(flavor = "multi_thread")]
async fn kanal_ohne_datenspeicher_fester_satz_und_kein_code() {
    let state = AppState::fuer_tests_ohne_speicher(&beispiel_fuer_tests(), "Im Test ohne Speicher.");
    assert!(state.jwt.is_some());
    let zeile = einmal_code_ausgeben(&state, &crate::kanal::konto_fuer_tests()).await;
    assert_eq!(zeile, crate::kanal::fehler_antwort(SATZ_CODE_OHNE_SPEICHER));
    assert!(!state.einmal_code.lock().unwrap().offen());
}

#[tokio::test(flavor = "multi_thread")]
async fn ein_neuer_code_ersetzt_den_alten() {
    let state = zustand();
    let app = lokal(&state);
    let alt = code_holen(&state).await;
    let neu = code_holen(&state).await;
    assert_ne!(alt, neu);
    let (s, _, j) = schicke(&app, beginn(&alt)).await;
    assert_eq!((s, j), (StatusCode::FORBIDDEN, gilt_nicht()));
    assert_eq!(schicke(&app, beginn(&neu)).await.0, StatusCode::OK);
    assert_eq!(code_vorgaenge(&state), [TITEL_CODE_AUSGEGEBEN, TITEL_CODE_AUSGEGEBEN]);
}

/// Der ganze Weg über den echten Socket: Der Kanal mit der Quelle des
/// Dienstes gibt den Code aus, und der Code gilt am Weg.
#[tokio::test(flavor = "multi_thread")]
async fn ueber_den_socket_geholt_gilt_der_code_am_weg() {
    use std::os::unix::fs::DirBuilderExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let state = zustand();
    let ordner = std::env::temp_dir().join(format!("dk-{}-code", std::process::id()));
    let _ = std::fs::remove_dir_all(&ordner);
    std::fs::DirBuilder::new().mode(0o700).create(&ordner).unwrap();
    let ort = ordner.join(crate::kanal::KANAL_DATEI);
    let quelle: Arc<dyn crate::kanal::CodeQuelle> = Arc::new(CodeAusgabe(state.clone()));
    let kanal = crate::kanal::starten(&ort, crate::unix::eigene_uid(), std::future::pending(), Some(quelle)).unwrap();
    let mut v = tokio::net::UnixStream::connect(&ort).await.unwrap();
    v.write_all(b"{\"befehl\":\"code\"}\n").await.unwrap();
    let mut zeile = String::new();
    v.read_to_string(&mut zeile).await.unwrap();
    let antwort: serde_json::Value = serde_json::from_str(zeile.trim_end()).unwrap();
    let code = antwort["code"].as_str().unwrap().to_string();
    assert_eq!(antwort["gueltig_sekunden"], 600);
    assert_eq!(schicke(&lokal(&state), beginn(&code)).await.0, StatusCode::OK);
    drop(kanal);
    let _ = std::fs::remove_dir_all(&ordner);
}

// ----- nichts nach außen -----

/// Schreibt das Protokoll dieses Threads mit.
#[derive(Clone, Default)]
struct Mitschrift(Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Mitschrift {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(b);
        Ok(b.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Alles, was mit einem Code geschehen kann: einer verfällt, mit einem
/// entsteht ein Passkey, einer wird ersetzt, einer bleibt offen. Gibt den
/// Zustand und die ausgegebenen Codes zurück.
async fn alle_wege_eines_codes() -> (AppState, Vec<String>) {
    let state = zustand();
    let app = lokal(&state);
    let mut codes = Vec::new();
    // Ein Code, der verfällt.
    let verfallen = code_holen(&state).await;
    for _ in 0..FEHLVERSUCHE {
        let (_, _, j) = schicke(&app, beginn("falsch")).await;
        assert!(!j.to_string().contains(&verfallen));
    }
    let (_, _, j) = schicke(&app, beginn(&verfallen)).await;
    assert!(!j.to_string().contains(&verfallen), "{j}");
    codes.push(verfallen);
    // Ein Code, mit dem ein Passkey entsteht.
    let gut = code_holen(&state).await;
    let a = PruefAuthenticator::es256();
    let (s, j) = anlegen(&app, &gut, &a).await;
    assert_eq!(s, StatusCode::CREATED, "{j}");
    assert!(!j.to_string().contains(&gut));
    codes.push(gut);
    // Ein Code, der ersetzt wird, und einer, der offen bleibt.
    codes.push(code_holen(&state).await);
    let offen = code_holen(&state).await;
    // Der offene Code als bloße Zeichenkette: unlesbar, er bleibt offen.
    let (s, _, j) = schicke(&app, post(BEGINN, serde_json::json!(offen))).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{j}");
    assert!(!j.to_string().contains(&offen));
    codes.push(offen);
    warte_bis("alle Meldungen", || code_meldungen(&state).len() == 3).await;
    warte_bis("Vorgang verfallen", || code_vorgaenge(&state).iter().any(|t| t == TITEL_CODE_VERFALLEN)).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    (state, codes)
}

/// Der Code steht nie im Protokoll, in einer Antwort außer der einen des
/// Kanals, im Journal oder in einer Meldung. Die Mitschrift gilt für diesen
/// Thread; die Laufzeit des Tests hat nur ihn, so erfasst sie auch die
/// Aufgaben nach der Antwort. Was der Datenspeicher in seinen eigenen
/// Threads schreibt, erfasst sie nicht; dorthin geht der Code nie, nur Titel.
///
/// Läuft allein in einem eigenen Prozess (`crate::test_allein`): Neben
/// anderen Tests verlöre die Mitschrift Zeilen und bewiese nichts. Dass sie
/// mitliest, sichern die vier Zeilen zu, die der Weg schreiben muss, eine
/// davon auf der Stufe debug.
#[tokio::test]
async fn der_code_steht_nie_im_protokoll_im_journal_oder_in_einer_meldung() {
    if !crate::test_allein::im_eigenen_prozess("api::passkey::code_tests::der_code_steht_nie_im_protokoll_im_journal_oder_in_einer_meldung") {
        return;
    }
    let mitschrift = Mitschrift::default();
    let schreiber = mitschrift.clone();
    let abonnent = tracing_subscriber::fmt().with_max_level(tracing::Level::TRACE).with_ansi(false).with_writer(move || schreiber.clone()).finish();
    let _wache = tracing::subscriber::set_default(abonnent);
    let (state, codes) = alle_wege_eines_codes().await;
    let protokoll = String::from_utf8(mitschrift.0.lock().unwrap().clone()).unwrap();
    for zeile in
        ["Einmal-Code ausgegeben", "Einmal-Code nach Fehlversuchen verfallen", "Passkey angelegt: im Hauptkonto, registriert", "Passkey: Körper unlesbar: Data"]
    {
        assert!(protokoll.contains(zeile), "die Mitschrift erfasst „{zeile}“ nicht: {protokoll}");
    }
    let speicher = state.speicher.as_ref().unwrap();
    let journal = format!("{:?}", speicher.vorgaenge_alle().unwrap());
    let meldungen = format!("{:?}", speicher.meldungen_mit_archiv().unwrap());
    assert_eq!(codes.len(), 4);
    for code in &codes {
        let normiert = crate::passkey::einmalcode::normieren(code);
        for (wo, text) in [("Protokoll", &protokoll), ("Journal", &journal), ("Meldungen", &meldungen)] {
            assert!(!text.contains(code.as_str()) && !text.to_uppercase().contains(&normiert), "der Code steht im {wo}");
        }
    }
}
