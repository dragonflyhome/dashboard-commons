//! Schritt E2: Ohne Anmeldung antwortet das Dashboard nur auf ausdrücklich
//! öffentliche Wege (`sicherheit::oeffentlich`); alles andere gibt 401.
//! Vorher (Etappe B2) war es eine Sperrliste mit freien Projektwegen, die nur
//! Zahlen zeigten.
//!
//! Der Wächtertest leitet die Wege aus dem Router selbst ab: Er liest jede
//! `.route("…", get(…).post(…))` aus `src/api/mod.rs` und `src/app.rs`, nimmt
//! die Seiten der Oberfläche (`oberflaeche::SEITEN` und die Projekt-, Gruppen-
//! und Katalogseiten) und die Wege eines Testmoduls dazu, setzt Werte für die
//! Platzhalter ein und fragt jeden Weg ohne Anmeldung ab: roh, groß
//! geschrieben, mit HEAD und prozentkodiert. Ein neuer Weg wird so ohne Zutun
//! mitgeprüft. Die Daten sind erfundene Spürhunde: Kommt einer davon in einer
//! Antwort ohne Anmeldung vor, schlägt der Test fehl.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use tower::ServiceExt;

use crate::app::{AppState, router, router_angemeldet};
use crate::sicherheit::FREIER_KANAL as FREIER_KANAL_PFAD;
use crate::sicherheit::oeffentlich;

/// Die Spürhunde: erfundene Texte, die nie frei hinausgehen dürfen.
const SPUERHUNDE: [&str; 15] = [
    // Zweig teilschritte (T4): Titel und Besitzer eines Schritts.
    "SPUERHUND-SCHRITT",
    "spuerhund-besitzer",
    // Etappe B3: Titel von Fristen und Namen von Ereignissen (`daten/fristen.toml`).
    "SPUERHUND-FRIST",
    "SPUERHUND-EREIGNIS",
    "SPUERHUND-OFFEN",
    "SPUERHUND-BEFUND",
    "SPUERHUND-STATUS",
    "SPUERHUND-FRAGE",
    "SPUERHUND-OPTION",
    "SPUERHUND-EMPFEHLUNG",
    "SPUERHUND-AUFGABE",
    "SPUERHUND-ARBEIT",
    "SPUERHUND-NOTIZ",
    "SPUERHUND-MELDUNG",
    "SPUERHUND-MELDETEXT",
];

/// Die Quellen, aus denen der Router gebaut wird.
const ROUTER_QUELLEN: [&str; 2] = [include_str!("api/mod.rs"), include_str!("app.rs")];

/// Alle Wege des Routers als Paare aus Methode und Muster
/// (`/api/projekte/{slug}/daten/{name}`), ohne die Wege, die es nur in Tests gibt.
fn alle_wege() -> Vec<(Method, String)> {
    let mut wege = Vec::new();
    for quelle in ROUTER_QUELLEN {
        let mut rest = quelle;
        while let Some(i) = rest.find(".route(") {
            rest = &rest[i + ".route(".len()..];
            let Some(start) = rest.find('"') else { break };
            let Some(laenge) = rest[start + 1..].find('"') else { break };
            let weg = &rest[start + 1..start + 1 + laenge];
            let danach: String = rest[start + 1 + laenge..].chars().take(160).collect();
            let bis_zum_naechsten = danach.split(".route(").next().unwrap_or_default();
            if !weg.starts_with('/') || weg.starts_with("/api/_test") {
                continue;
            }
            for (wort, methode) in [("get(", Method::GET), ("post(", Method::POST)] {
                if bis_zum_naechsten.contains(wort) {
                    wege.push((methode, weg.to_string()));
                }
            }
        }
    }
    wege.sort_by(|a, b| (a.1.as_str(), a.0.as_str()).cmp(&(b.1.as_str(), b.0.as_str())));
    wege.dedup();
    wege
}

/// Die GET-Wege des Routers (wie [`alle_wege`]).
fn get_wege() -> Vec<String> {
    alle_wege().into_iter().filter(|(m, _)| *m == Method::GET).map(|(_, w)| w).collect()
}

/// Die Seiten der Oberfläche als Muster (alle GET), dazu ein unbekannter Weg.
fn seiten_wege() -> Vec<String> {
    let mut s: Vec<String> = crate::oberflaeche::SEITEN.iter().map(|x| format!("/{x}")).collect();
    s.extend(crate::oberflaeche::KATALOG_SEITEN.iter().map(|x| format!("/katalog/{x}")));
    s.extend(
        ["/projects/{slug}", "/projects/{slug}/vorschau", "/projects/{slug}/{seite}", "/gruppen/{gruppe}", "/projekte/archiviert", "/gibt-es-nicht"]
            .map(String::from),
    );
    s
}

/// Setzt für jeden Platzhalter alle Werte ein.
fn einsetzen(muster: &str, werte: &[(&str, Vec<String>)]) -> Vec<String> {
    let mut aus = vec![muster.to_string()];
    for (platz, liste) in werte {
        let mut neu = Vec::new();
        for w in &aus {
            if w.contains(platz) {
                neu.extend(liste.iter().map(|x| w.replace(platz, x)));
            } else {
                neu.push(w.clone());
            }
        }
        aus = neu;
    }
    aus
}

fn kopieren(von: &Path, nach: &Path) {
    std::fs::create_dir_all(nach).unwrap();
    for e in std::fs::read_dir(von).unwrap() {
        let e = e.unwrap();
        let ziel = nach.join(e.file_name());
        if e.file_type().unwrap().is_dir() {
            kopieren(&e.path(), &ziel);
        } else {
            std::fs::copy(e.path(), ziel).unwrap();
        }
    }
}

fn get(pfad: &str) -> Request<Body> {
    Request::get(pfad).header("host", "127.0.0.1:8787").body(Body::empty()).unwrap()
}

fn post(pfad: &str, koerper: &str) -> Request<Body> {
    Request::post(pfad).header("host", "127.0.0.1:8787").header("content-type", "application/json").body(Body::from(koerper.to_string())).unwrap()
}

/// Der Körper einer Antwort als Text; ein Strom (Live-Kanal) höchstens `frist` lang.
async fn text(res: axum::response::Response, frist: Duration) -> String {
    let mut body = res.into_body();
    let ende = Instant::now() + frist;
    let mut aus = Vec::new();
    loop {
        let rest = ende.saturating_duration_since(Instant::now());
        if rest.is_zero() {
            break;
        }
        match tokio::time::timeout(rest, body.frame()).await {
            Ok(Some(Ok(f))) => {
                if let Some(d) = f.data_ref() {
                    aus.extend_from_slice(d);
                }
            }
            Ok(_) | Err(_) => break,
        }
    }
    String::from_utf8_lossy(&aus).into_owned()
}

/// Ein Beispiel-Inhalt mit einem Projekt voller Spürhunde (Kopie von tests/beispiel).
fn inhalt_mit_spuerhunden(zweck: &str) -> PathBuf {
    let ordner = std::env::temp_dir().join(format!("dashboard-test-{}-frei-{zweck}", std::process::id()));
    let _ = std::fs::remove_dir_all(&ordner);
    kopieren(&crate::app::beispiel_fuer_tests(), &ordner);
    let p = ordner.join("projects").join("spuer");
    std::fs::create_dir_all(p.join("daten")).unwrap();
    std::fs::write(
        p.join("projekt.toml"),
        "schema = 3\ntitle = \"Spürhund\"\nstatus = \"aktiv\"\nsummary = \"Erfundenes Projekt für den Test.\"\nopen = [\"SPUERHUND-OFFEN eins\", \"SPUERHUND-OFFEN zwei\"]\n",
    )
    .unwrap();
    // Ein Befund-Satz, der einen Wert aus der Datei zitiert (die Quelle gibt es nicht),
    // in einem eigenen Projekt (die Seite lässt sich dann nicht lesen).
    let b = ordner.join("projects").join("spuerbefund");
    std::fs::create_dir_all(b.join("daten")).unwrap();
    std::fs::write(
        b.join("projekt.toml"),
        "schema = 3
title = \"Befund\"
status = \"aktiv\"
summary = \"x\"
",
    )
    .unwrap();
    std::fs::write(
        b.join("daten").join("kennzahlen.toml"),
        "[[kennzahl]]\ntitel = \"Zahl\"\nberechnet = { quelle = \"SPUERHUND-BEFUND\", art = \"anzahl\" }\n",
    )
    .unwrap();
    // Eine kaputte Seite, deren Fehlersatz einen Wert zitiert.
    let k = ordner.join("projects").join("spuerkaputt");
    std::fs::create_dir_all(&k).unwrap();
    std::fs::write(k.join("projekt.toml"), "schema = 3\ntitle = \"Kaputt\"\nstatus = \"SPUERHUND-STATUS\"\nsummary = \"x\"\n").unwrap();
    std::fs::write(
        p.join("daten").join("fragen.toml"),
        "[[frage]]\nid = \"eins\"\ntext = \"SPUERHUND-FRAGE eins?\"\nart = \"wahl\"\noptionen = [\"SPUERHUND-OPTION a\", \"SPUERHUND-OPTION b\"]\nempfehlung = \"SPUERHUND-OPTION a\"\ngestellt = \"2026-11-03\"\nprioritaet = \"bald\"\n\n\
         [[frage]]\nid = \"zwei\"\ntext = \"SPUERHUND-FRAGE zwei?\"\nart = \"text\"\nempfehlung = \"SPUERHUND-EMPFEHLUNG\"\ngestellt = \"2026-11-03\"\nprioritaet = \"bald\"\n",
    )
    .unwrap();
    // Etappe B3: eine Frist von gestern (Ampel rot, Titel nur angemeldet), eine bewegliche, die hängt.
    let gestern = (crate::vertrag::abgeleitet::heute() - chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
    std::fs::write(
        p.join("daten").join("fristen.toml"),
        format!(
            "[[ereignis]]\nkennung = \"brief\"\ntitel = \"SPUERHUND-EREIGNIS Brief\"\n\n\
             [[frist]]\nkennung = \"vorbei\"\ntitel = \"SPUERHUND-FRIST vorbei\"\ndatum = \"{gestern}\"\n\n\
             [[frist]]\nkennung = \"spaeter\"\ntitel = \"SPUERHUND-FRIST später\"\nereignis = \"brief\"\nabstand = \"+14 Tage\"\n"
        ),
    )
    .unwrap();
    ordner
}

/// Eine Anfrage wie aus dem Dashboard selbst: schreibend mit eigener Herkunft
/// und leerem JSON, damit nur die Anmeldung entscheidet.
fn anfrage(methode: &Method, pfad: &str) -> Request<Body> {
    let b = Request::builder().method(methode.clone()).uri(pfad).header("host", "127.0.0.1:8787");
    if matches!(*methode, Method::GET | Method::HEAD) {
        return b.body(Body::empty()).unwrap();
    }
    b.header("origin", "http://127.0.0.1:8787").header("content-type", "application/json").body(Body::from("{}")).unwrap()
}

/// Der Zustand mit den Spürhunden, einer Gruppe und dem Testmodul (Slug
/// `neu-form`, dessen Wege früher frei erklärt waren; seit E3 sind alle geschützt).
fn zustand_mit_spuerhunden(zweck: &str) -> (AppState, PathBuf) {
    let ordner = inhalt_mit_spuerhunden(zweck);
    std::fs::write(
        ordner.join("data").join("gruppen.toml"),
        "[[gruppe]]\nslug = \"spuer-gruppe\"\ntitel = \"Spürhund-Gruppe\"\nteile = [\"spuer\", \"spuerbefund\"]\n",
    )
    .unwrap();
    let mut state = AppState::fuer_tests(&ordner);
    let reg = crate::projekte::Register::fuer_zustand_ungeprueft(&state, vec![crate::projekte::eintrag(crate::projekte::tests::MitFreiemWeg)]);
    reg.tabellen_einrichten(state.speicher.as_ref().unwrap());
    state.projekte = Arc::new(reg);
    (state, ordner)
}

/// Prüft eine Antwort ohne Anmeldung: nie ein Spürhund; gibt Status und Text zurück.
async fn ohne_spuerhund(app: &axum::Router, methode: &Method, weg: &str) -> (StatusCode, String) {
    let res = app.clone().oneshot(anfrage(methode, weg)).await.unwrap();
    let status = res.status();
    let strom = weg == FREIER_KANAL_PFAD;
    let t = text(res, Duration::from_millis(if strom { 1500 } else { 10_000 })).await;
    for hund in SPUERHUNDE {
        assert!(!t.contains(hund), "{methode} {weg} ({status}) gibt ohne Anmeldung „{hund}“ heraus: {t}");
    }
    (status, t)
}

/// Der Wächter (Schritt E2): Jeder Weg des Routers, jede Seite und jeder Weg
/// eines Moduls steht in `oeffentlich` oder gibt ohne Anmeldung 401, roh, groß
/// geschrieben, mit HEAD und prozentkodiert (kodiert oder kaputt: 400 oder 401).
/// Kein öffentlicher Weg gibt einen Spürhund heraus. Mit Anmeldung sind die
/// Spürhunde da (sonst prüfte der Test nichts).
#[tokio::test]
async fn waechter_jeder_weg_oeffentlich_oder_401() {
    let (state, ordner) = zustand_mit_spuerhunden("waechter");
    // Aufgabe, Arbeit und Meldung legt der Chat an (per curl, angemeldet).
    let mut ids = Vec::new();
    for (weg, koerper) in [
        ("/api/aufgaben", r#"{"projekt": "spuer", "titel": "SPUERHUND-AUFGABE", "frist": "2026-11-04"}"#),
        ("/api/aufgaben", r#"{"projekt": "spuer", "titel": "SPUERHUND-AUFGABE beweglich", "frist_bezug": {"ereignis": "brief", "abstand": "+1 Monat"}}"#),
        (
            "/api/arbeiten",
            r#"{"projekt": "spuer", "von": "chat:spuer", "titel": "SPUERHUND-ARBEIT", "stand": "wartet", "wartet_auf": "inhaber", "notiz": "SPUERHUND-NOTIZ", "frist": "2026-11-01"}"#,
        ),
        ("/api/meldungen", r#"{"projekt": "spuer", "art": "info", "titel": "SPUERHUND-MELDUNG", "text": "SPUERHUND-MELDETEXT"}"#),
    ] {
        let res = router_angemeldet(state.clone()).oneshot(post(weg, koerper)).await.unwrap();
        let status = res.status();
        let t = text(res, Duration::from_secs(2)).await;
        assert!(status.is_success(), "{weg}: {status} {t}");
        let v: serde_json::Value = serde_json::from_str(&t).unwrap();
        ids.push(v["id"].as_str().unwrap().to_string());
    }
    // T4: Die Arbeit bekommt einen Schritt mit Titel, Besitzer und Notiz.
    let ar = ids[2].clone();
    for (weg, koerper) in [
        (format!("/api/arbeiten/{ar}/schritte"), r#"{"von": "chat:spuer", "titel": "SPUERHUND-SCHRITT bauen"}"#),
        (format!("/api/arbeiten/{ar}/schritte/2/uebernehmen"), r#"{"von": "chat:spuerhund-besitzer"}"#),
        (format!("/api/arbeiten/{ar}/schritte/2"), r#"{"von": "chat:spuerhund-besitzer", "notiz": "SPUERHUND-NOTIZ am Schritt"}"#),
    ] {
        let res = router_angemeldet(state.clone()).oneshot(post(&weg, koerper)).await.unwrap();
        let status = res.status();
        assert!(status.is_success(), "{weg}: {status} {}", text(res, Duration::from_secs(2)).await);
    }
    // Mit Anmeldung sind die Spürhunde da (sonst prüfte der Test nichts).
    for (weg, hund) in [
        ("/api/projekte/spuer/stand", "SPUERHUND-SCHRITT"),
        ("/api/projekte/spuer/stand", "spuerhund-besitzer"),
        ("/api/anstehend?projekt=spuer", "SPUERHUND-SCHRITT"),
        ("/api/projekte/spuer/daten/fragen", "SPUERHUND-FRAGE"),
        ("/api/projects/spuer", "SPUERHUND-FRAGE"),
        ("/api/projekte/spuer/stand", "SPUERHUND-ARBEIT"),
        ("/api/anstehend?projekt=spuer", "SPUERHUND-MELDETEXT"),
        ("/api/projects", "SPUERHUND-OFFEN"),
        ("/api/projekte/spuer", "SPUERHUND-OFFEN"),
        ("/api/pruefung", "SPUERHUND-BEFUND"),
        ("/api/pruefung/spuerbefund", "SPUERHUND-BEFUND"),
        ("/api/projects", "SPUERHUND-STATUS"),
        ("/api/projekte/spuer/daten/fristen", "SPUERHUND-FRIST"),
        ("/api/projekte/spuer/daten/fristen", "SPUERHUND-EREIGNIS"),
        ("/api/projects/spuer", "SPUERHUND-EREIGNIS"),
        ("/api/fristen", "SPUERHUND-FRIST"),
        ("/api/projekte/spuer/stand/voll", "SPUERHUND-FRIST vorbei"),
    ] {
        let res = router_angemeldet(state.clone()).oneshot(get(weg)).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK, "{weg}");
        assert!(text(res, Duration::from_secs(5)).await.contains(hund), "{weg}: mit Anmeldung fehlt {hund}");
    }

    let werte = [
        ("{slug}", vec!["spuer".to_string(), "gibt-es-nicht".to_string()]),
        ("{name}", ["fragen", "fristen", "orte", "kanban"].map(String::from).to_vec()),
        ("{id}", vec![ids[0].clone(), ids[2].clone()]),
        ("{nr}", vec!["2".to_string()]),
        ("{file}", vec!["vorschau.html".to_string()]),
        ("{*path}", vec!["x".to_string()]),
        ("{seite}", vec!["plan".to_string()]),
        ("{gruppe}", vec!["spuer-gruppe".to_string()]),
    ];
    let mut muster = alle_wege();
    assert!(muster.iter().any(|(m, w)| *m == Method::POST && w == "/api/passkey/abmelden"), "Router nicht gelesen: {muster:?}");
    muster.extend(seiten_wege().into_iter().map(|w| (Method::GET, w)));
    // Die Wege des Testmoduls: erklärt frei, seit E2 ohne Wirkung.
    for w in ["/api/projekte/neu-form/x/zahlen", "/api/projekte/neu-form/x/stufe/2", "/api/projekte/neu-form/x/geheim"] {
        muster.push((Method::GET, w.to_string()));
    }
    muster.push((Method::POST, "/api/projekte/neu-form/x/zahlen".to_string()));
    let bau = |rel: &Path| crate::oberflaeche::bau_datei(&state.web, rel);
    let app = router(state.clone());
    let mut oeffentliche = Vec::new();
    let mut gezaehlt = 0;
    for (methode, m) in &muster {
        for weg in einsetzen(m, &werte) {
            let mut varianten: Vec<(Method, String)> = vec![(methode.clone(), weg.clone()), (methode.clone(), weg.to_uppercase())];
            if *methode == Method::GET {
                varianten.push((Method::HEAD, weg.clone()));
            }
            varianten.extend(kodiert(&weg).into_iter().map(|k| (methode.clone(), k)));
            for (methode, w) in varianten {
                gezaehlt += 1;
                let (status, t) = ohne_spuerhund(&app, &methode, &w).await;
                if oeffentlich(&methode, &w, &bau) {
                    oeffentliche.push(format!("{methode} {w}"));
                } else if w.contains('%') {
                    // Kodiert oder kaputt: 400 oder 401, nie 2xx, nie Inhalt (Spürhunde oben).
                    // Im Test antwortet unter /api die Prozent-Wand zuerst (400); im Dienst
                    // fragt Windows-Negotiate vor dieser Wand, darum sagt er dort 401.
                    assert!(status == StatusCode::BAD_REQUEST || status == StatusCode::UNAUTHORIZED, "{methode} {w}: {status} {t}");
                } else {
                    assert_eq!(status, StatusCode::UNAUTHORIZED, "{methode} {w} ist nicht öffentlich: {t}");
                }
            }
        }
    }
    assert!(gezaehlt > 2000, "zu wenige Wege: {gezaehlt}");
    oeffentliche.sort();
    oeffentliche.dedup();
    // Kodiert öffentlich ist nur die Umleitung unter /neu/ (ohne Blick in den Bestand).
    let (kodiert_oeffentlich, oeffentliche): (Vec<String>, Vec<String>) = oeffentliche.into_iter().partition(|w| w.contains('%'));
    assert!(kodiert_oeffentlich.iter().all(|w| w.starts_with("GET /neu/")), "{kodiert_oeffentlich:?}");
    // Schritt 7b der macOS-Folge: Außerhalb von Windows kommen die zwei Wege
    // über einen Einmal-Code dazu. Unter Windows liest der Wächter sie auch
    // aus dem Quelltext; dort sind sie nicht öffentlich und geben 401.
    let erwartet: Vec<&str> = [
        "GET /",
        "GET /anmelden",
        "GET /api/anmeldung/probe",
        "GET /api/anmeldung/wer",
        "GET /api/betrieb",
        "GET /api/events/frei",
        "GET /api/fassungen",
        "GET /api/passkey/antrag",
        "GET /api/passkey/stand",
        "GET /api/stand",
        "GET /fassungen",
        "GET /healthz",
        "GET /neu",
        "GET /neu/",
        "GET /neu/x",
        "HEAD /",
        "HEAD /anmelden",
        "HEAD /fassungen",
        "HEAD /healthz",
        "HEAD /neu",
        "HEAD /neu/",
        "HEAD /neu/x",
        "POST /api/passkey/abmelden",
        "POST /api/passkey/anmelden/beginn",
        "POST /api/passkey/anmelden/ende",
        "POST /api/passkey/antrag/beginn",
        "POST /api/passkey/antrag/ende",
    ]
    .into_iter()
    .chain(if cfg!(unix) { vec!["POST /api/passkey/code/beginn", "POST /api/passkey/code/beleg", "POST /api/passkey/code/ende"] } else { Vec::new() })
    .collect();
    assert_eq!(oeffentliche, erwartet, "die öffentlichen Wege des Routers");
    let _ = std::fs::remove_dir_all(&ordner);
}

/// Schritt E2: Ohne Anmeldung derselbe Status und derselbe Körper für
/// vorhandene und erfundene Projekte, Unterseiten, Gruppen und Projektwege
/// (bis auf den Weg selbst in der Adresse zum Kopieren); mit Anmeldung sind
/// sie verschieden (sonst prüfte der Test nichts). Als Navigation des
/// Browsers und als einfache Anfrage.
#[tokio::test]
async fn gleiche_antwort_fuer_vorhandene_und_erfundene() {
    let (state, ordner) = zustand_mit_spuerhunden("gleich");
    let app = router(state.clone());
    let an = router_angemeldet(state.clone());
    let antwort = |app: axum::Router, pfad: String, navigation: bool| async move {
        let mut b = Request::get(&pfad).header("host", "localhost:8787");
        if navigation {
            b = b.header("sec-fetch-dest", "document").header(header::ACCEPT, "text/html");
        }
        let res = app.oneshot(b.body(Body::empty()).unwrap()).await.unwrap();
        let status = res.status();
        let ort = res.headers().get(header::LOCATION).and_then(|v| v.to_str().ok()).unwrap_or_default().to_string();
        let t = text(res, Duration::from_secs(5)).await;
        (status, ort, t)
    };
    for (da, erfunden) in [
        ("/projects/spuer", "/projects/gibtsnicht"),
        ("/projects/neu-form/plan", "/projects/neu-form/gibtsnicht"),
        ("/gruppen/spuer-gruppe", "/gruppen/gibtsnicht"),
        ("/api/projects/spuer", "/api/projects/gibtsnicht"),
        ("/api/projekte/spuer/stand", "/api/projekte/gibtsnicht/stand"),
        ("/projects/schaufenster/assets/bild.svg", "/projects/schaufenster/assets/gibtsnicht"),
    ] {
        for navigation in [true, false] {
            let (s1, o1, t1) = antwort(app.clone(), da.to_string(), navigation).await;
            let (s2, o2, t2) = antwort(app.clone(), erfunden.to_string(), navigation).await;
            let gleich = |x: &str, weg: &str| x.replace(weg, "WEG");
            assert_eq!(
                (s1, gleich(&o1, da), gleich(&t1, da)),
                (s2, gleich(&o2, erfunden), gleich(&t2, erfunden)),
                "{da} / {erfunden}, Navigation {navigation}"
            );
            assert!(s1 == StatusCode::UNAUTHORIZED || s1 == StatusCode::SEE_OTHER, "{da}: {s1}");
            assert!(!t1.contains("Spürhund") && !t1.contains("Spuerhund"), "{da}: {t1}");
        }
        // Mit Anmeldung unterscheiden sie sich (Seite 200 oder Daten gegen 404).
        let (s1, _, t1) = antwort(an.clone(), da.to_string(), false).await;
        let (s2, _, t2) = antwort(an.clone(), erfunden.to_string(), false).await;
        // Ohne Bau der Oberfläche (503) lassen sich Seiten nicht unterscheiden.
        assert!(s1 != s2 || t1 != t2 || s1 == StatusCode::SERVICE_UNAVAILABLE, "{da} / {erfunden}: angemeldet gleich ({s1})");
    }
    let _ = std::fs::remove_dir_all(&ordner);
}

/// Schritt E2: Auf Seiten (auch öffentlichen) ist nur GET und HEAD öffentlich;
/// jede andere Methode verlangt die Anmeldung, auch mit eigener Herkunft.
#[tokio::test]
async fn seiten_nur_mit_get_und_head() {
    let app = router(AppState::fuer_tests(&crate::app::beispiel_fuer_tests()));
    for pfad in ["/", "/anmelden", "/fassungen", "/pc", "/projects/schaufenster", "/neu/x", "/_app/version.json"] {
        for methode in [Method::POST, Method::PUT, Method::DELETE, Method::PATCH, Method::OPTIONS] {
            let res = app.clone().oneshot(anfrage(&methode, pfad)).await.unwrap();
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{methode} {pfad}");
        }
    }
}

/// Schritt E2: `/api`, `/api/`, `/API/x`, unbekannte Wege unter `/api` und unter
/// `/api/anmeldung/` (bis E2 übersprungen) geben ohne Anmeldung 401, vor 404.
#[tokio::test]
async fn api_wurzel_und_unbekannte_wege_401() {
    let app = router(AppState::fuer_tests(&crate::app::beispiel_fuer_tests()));
    for pfad in ["/api", "/api/", "/API/x", "/Api/Betrieb", "/api/gibt-es-nicht", "/api/anmeldung/gibt-es-nicht", "/api/anmeldung/", "/api/anmeldung/probe/"] {
        let res = app.clone().oneshot(get(pfad)).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{pfad}");
        // JSON unter /api/ (klein geschrieben), sonst derselbe Satz als Text (Fehlerform).
        let t = text(res, Duration::from_secs(2)).await;
        let v: serde_json::Value = serde_json::from_str(&t).unwrap_or_else(|_| serde_json::json!({ "art": "anmeldung", "fehler": t }));
        assert_eq!((v["art"].as_str(), v["fehler"].as_str()), (Some("anmeldung"), Some("Bitte mit dem Windows-Konto anmelden.")), "{pfad}: {v}");
    }
    // Angemeldet ist ein unbekannter Weg weiter 404.
    let res = router_angemeldet(AppState::fuer_tests(&crate::app::beispiel_fuer_tests())).oneshot(get("/api/gibt-es-nicht")).await.unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

/// Schritt E2: `/api/passkey/abmelden` ist öffentlich und geht auch mit einem
/// ungültigen Ausweis: 204, das Cookie wird gelöscht.
#[tokio::test]
async fn abmelden_mit_ungueltigem_ausweis_204() {
    let app = router(AppState::fuer_tests(&crate::app::beispiel_fuer_tests()));
    let req = Request::post("/api/passkey/abmelden")
        .header("host", "127.0.0.1:8787")
        .header("origin", "http://127.0.0.1:8787")
        .header(header::COOKIE, format!("{}=kaputt.kaputt.kaputt", crate::passkey::cookie::NAME))
        .body(Body::empty())
        .unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let cookie = res.headers().get(header::SET_COOKIE).and_then(|v| v.to_str().ok()).unwrap_or_default().to_string();
    assert!(cookie.starts_with(&format!("{}=", crate::passkey::cookie::NAME)) && cookie.contains("Max-Age=0"), "{cookie}");
}

/// Der Leser findet die Wege, auch mehrere je Zeile und über Zeilen hinweg.
#[test]
fn wege_aus_dem_router() {
    let w = get_wege();
    for erwartet in ["/api/projects", "/api/projects/{slug}", "/api/projekte/{slug}/daten/{name}", "/api/events/frei", "/api/betrieb", "/healthz"] {
        assert!(w.iter().any(|x| x == erwartet), "{erwartet} fehlt: {w:?}");
    }
    assert!(!w.iter().any(|x| x.starts_with("/api/_test")), "Test-Wege nicht");
    assert!(!w.iter().any(|x| x == "/api/einstufung"), "nur POST: nicht unter GET");
    let alle = alle_wege();
    for (m, erwartet) in
        [(Method::POST, "/api/einstufung"), (Method::GET, "/api/vorgaenge"), (Method::POST, "/api/vorgaenge"), (Method::POST, "/api/antworten/{slug}/{id}")]
    {
        assert!(alle.iter().any(|(x, y)| *x == m && y == erwartet), "{m} {erwartet} fehlt: {alle:?}");
    }
    // Etappe 6b: die Anleitung steht im Router und ist nicht öffentlich (wie das Zielbild).
    for system in ["/api/anleitung", "/api/zielbild", "/api/entscheidungen"] {
        assert!(w.iter().any(|x| x == system), "{system} fehlt: {w:?}");
        assert!(!oeffentlich(&Method::GET, system, &|_| true), "{system} darf nicht öffentlich sein");
    }
    assert!(seiten_wege().iter().any(|s| s == "/anmelden") && seiten_wege().iter().any(|s| s == "/projects/{slug}"));
    assert_eq!(einsetzen("/a/{slug}/b/{name}", &[("{slug}", vec!["x".into(), "y".into()]), ("{name}", vec!["n".into()])]), ["/a/x/b/n", "/a/y/b/n"]);
}

/// Kodierte Abwandlungen eines Weges (Hotfix Prozent): je Abschnitt das erste
/// Zeichen als `%XX` (Hex groß und klein), doppelt kodiert (`%25XX`) und der
/// ganze Abschnitt kodiert; je Schrägstrich ab dem zweiten `%2F` und `%2f`;
/// dazu `%2e%2e`-Versuche vor dem letzten Abschnitt.
fn kodiert(weg: &str) -> Vec<String> {
    let teile: Vec<String> = weg.split('/').map(String::from).collect();
    let mit = |i: usize, neu: String| {
        let mut t = teile.clone();
        t[i] = neu;
        t.join("/")
    };
    let mut aus = Vec::new();
    for (i, teil) in teile.iter().enumerate() {
        let Some(&b) = teil.as_bytes().first() else { continue };
        let rest = &teil[1..];
        aus.push(mit(i, format!("%{b:02X}{rest}")));
        aus.push(mit(i, format!("%{b:02x}{rest}")));
        aus.push(mit(i, format!("%25{b:02X}{rest}")));
        aus.push(mit(i, teil.bytes().map(|b| format!("%{b:02x}")).collect()));
    }
    for i in 2..teile.len() {
        for strich in ["%2F", "%2f"] {
            let mut t = teile.clone();
            let zusammen = format!("{}{strich}{}", t[i - 1], t[i]);
            t.splice(i - 1..=i, [zusammen]);
            aus.push(t.join("/"));
        }
    }
    if let Some((vorn, letzt)) = weg.rsplit_once('/') {
        aus.push(format!("{vorn}/x/%2e%2e/{letzt}"));
        aus.push(format!("{vorn}/x/%2E%2E/{letzt}"));
        aus.push(format!("{vorn}/x/..%2f{letzt}"));
        aus.push(format!("{vorn}/%2e%2e{vorn}/{letzt}"));
    }
    aus
}

/// Hotfix Prozent (Fund live: `/daten/%66risten` gab ohne Anmeldung 200). Jeder
/// GET-Weg des Routers (abgeleitet wie oben, neue Wege kommen von selbst dazu),
/// jede kodierte Abwandlung, ohne Anmeldung: nie ein Spürhund; unter `/api` 400
/// (erste Wand, `sicherheit::prozent_im_pfad`); außerhalb 401, außer die
/// Abwandlung ist selbst öffentlich (nur die Umleitung unter `/neu/`). Mit
/// Anmeldung gehen normale Wege und Abfragen mit `%` weiter.
#[tokio::test]
async fn prozent_im_pfad_kein_umweg() {
    use crate::fehler::AppError;
    let ordner = inhalt_mit_spuerhunden("prozent");
    let state = AppState::fuer_tests(&ordner);
    let frei = router(state.clone());
    let an = router_angemeldet(state.clone());
    const DATEIEN: [&str; 4] = ["fragen", "fristen", "orte", "kanban"];
    let namen: Vec<String> = DATEIEN.map(String::from).to_vec();
    let werte = [
        ("{slug}", vec!["spuer".to_string()]),
        ("{name}", namen),
        ("{id}", vec!["v-20261104-0001".to_string()]),
        ("{file}", vec!["vorschau.html".to_string()]),
        ("{*path}", vec!["x".to_string()]),
    ];
    // Die Fälle aus dem Fund, ausdrücklich.
    let oeff = |w: &str| oeffentlich(&Method::GET, w, &|rel| crate::oberflaeche::bau_datei(&state.web, rel));
    let mut faelle: Vec<(String, bool)> = [
        "/api/projekte/spuer/daten/%66ragen",
        "/api/projekte/spuer/daten/%66risten",
        "/api/projekte/spuer/daten/%46RAGEN",
        "/api/projekte/spuer/daten/%6Frte",
        "/api/projekte/spuer/daten/%6frte",
        "/api/projekte/spuer/daten%2Ffragen",
        "/api/projekte/spuer/daten%2ffristen",
        "/api/projekte/spuer/daten/%2566ragen",
        "/api/projekte/spuer/daten/%252566risten",
        "/api/projekte/%73puer/daten/fragen",
        "/api/projekte/%73puer/daten/fristen",
        "/api/projekte/%73puer/stand/voll",
        "/api/projekte/spuer/x/%2e%2e/daten/fragen",
        "/api/projekte/spuer/daten/%2e%2e/daten/fristen",
        "/api/projekte/spuer/stand/%2e%2e/daten/fragen",
    ]
    .map(|w| (w.to_string(), true))
    .to_vec();
    for muster in get_wege() {
        for weg in einsetzen(&muster, &werte) {
            faelle.extend(kodiert(&weg).into_iter().map(|k| (k.clone(), !oeff(&k))));
        }
    }
    assert!(faelle.len() > 500, "zu wenige Abwandlungen: {}", faelle.len());
    for (weg, schutz) in &faelle {
        let res = frei.clone().oneshot(get(weg)).await.unwrap();
        let status = res.status();
        let t = text(res, Duration::from_secs(5)).await;
        for hund in SPUERHUNDE {
            assert!(!t.contains(hund), "{weg} ({status}) gibt ohne Anmeldung „{hund}“ heraus: {t}");
        }
        if weg.starts_with("/api/") {
            assert_eq!(status, StatusCode::BAD_REQUEST, "{weg}: {t}");
            assert!(t.contains("\"fehlerhaft\""), "{weg}: {t}");
        } else if *schutz {
            assert_eq!(status, StatusCode::UNAUTHORIZED, "{weg}: {t}");
        } else {
            assert!(weg.starts_with("/neu/"), "kodiert öffentlich: {weg}");
        }
    }
    // Schreibende Wege: dieselbe Wand.
    for weg in ["/api/bauen/%73puer", "/api/antworten/spuer/%65ins", "/api/projekte/spuer/x/%67ut"] {
        let res = frei.clone().oneshot(post(weg, "{}")).await.unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST, "{weg}");
    }
    // Mit Anmeldung: normale Wege gehen, die Wand gilt trotzdem; `%` in der Abfrage geht.
    let res = an.clone().oneshot(get("/api/projekte/spuer/daten/fragen")).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(text(res, Duration::from_secs(5)).await.contains("SPUERHUND-FRAGE"));
    let res = an.clone().oneshot(get("/api/projekte/spuer/daten/%66ragen")).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    for weg in ["/api/projects/spuer?x=%66%25", "/api/anstehend?projekt=%73puer"] {
        let res = an.clone().oneshot(get(weg)).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK, "{weg}");
    }
    for weg in ["/api/projekte/spuer?x=%66", "/api/projekte/spuer/stand?projekt=%73puer"] {
        let res = an.clone().oneshot(get(weg)).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK, "{weg}");
        let res = frei.clone().oneshot(get(weg)).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{weg}");
    }

    // Zweite Wand: der Handler selbst, ohne Schleusen davor, mit dem dekodierten Namen.
    use axum::Extension;
    use axum::extract::{Path as Pfad, State};
    let satz = AppError::anmeldung_noetig().fehler;
    // Seit E3 jede Datei, auch eine ohne persönlichen Inhalt (`kanban` liegt in neu-form).
    for (slug, name) in [("spuer", "fragen"), ("spuer", "fristen"), ("spuer", "FRAGEN"), ("spuer", "Fristen"), ("neu-form", "kanban")] {
        let res = crate::api::neue_form::daten(State(state.clone()), Pfad((slug.to_string(), name.to_string())), None).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{name}");
        assert!(res.headers().contains_key("www-authenticate"), "{name}");
        let t = text(res, Duration::from_secs(5)).await;
        assert!(t.contains(&satz) && !t.contains("SPUERHUND") && !t.contains("spalte"), "{name}: {t}");
    }
    for (slug, name, inhalt) in [("spuer", "fristen", "SPUERHUND-FRIST"), ("neu-form", "kanban", "zuerst")] {
        let mit = Some(Extension(crate::anmeldung::Angemeldet));
        let res = crate::api::neue_form::daten(State(state.clone()), Pfad((slug.to_string(), name.to_string())), mit).await;
        assert_eq!(res.status(), StatusCode::OK, "{name}");
        assert!(text(res, Duration::from_secs(5)).await.contains(inhalt), "{name}");
    }
    // Die erste Wand: am Pfad ist seit E2 keine Datei öffentlich.
    for name in DATEIEN {
        assert!(!oeff(&format!("/api/projekte/spuer/daten/{name}")), "{name}");
    }
    let _ = std::fs::remove_dir_all(&ordner);
}

/// Die Dekodierung der zweiten Wand: einmal, doppelt kodiert und kaputt nie frei.
#[test]
fn pfad_dekodiert_einmal() {
    use crate::sicherheit::pfad_dekodiert;
    assert_eq!(pfad_dekodiert("/daten/%66ragen").as_deref(), Some("/daten/fragen"));
    assert_eq!(pfad_dekodiert("/daten/%6Frte").as_deref(), Some("/daten/orte"));
    assert_eq!(pfad_dekodiert("/a%2Fb").as_deref(), Some("/a/b"));
    assert_eq!(pfad_dekodiert("/daten/%2566ragen"), None);
    assert_eq!(pfad_dekodiert("/daten/%6"), None);
    assert_eq!(pfad_dekodiert("/daten/%zz"), None);
    assert_eq!(pfad_dekodiert("/daten/%ff"), None);
}
