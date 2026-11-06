//! Etappe B2 (Nacharbeit, Architektur): Ohne Anmeldung geht kein persönlicher
//! Text hinaus. Fragen an den Inhaber, Titel und Notizen von Aufgaben und
//! Arbeiten, Texte von Meldungen, offene Punkte (`open`) und Befund-Sätze der
//! Prüfung kommen nur mit Windows-Anmeldung; frei bleiben Zahlen.
//!
//! Der Test leitet die Wege aus dem Router selbst ab: Er liest jede
//! `.route("…", get(…))` aus `src/api/mod.rs` und `src/app.rs`, setzt Werte für
//! die Platzhalter ein und fragt jeden Weg ab, den `sicherheit::geschuetzt`
//! frei lässt, auch den freien Live-Kanal. Ein neuer freier Weg wird so ohne
//! Zutun mitgeprüft. Die Daten sind erfundene Spürhunde: Kommt einer davon
//! in einer freien Antwort vor, schlägt der Test fehl.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

use crate::app::{AppState, router, router_angemeldet};
use crate::sicherheit::FREIER_KANAL as FREIER_KANAL_PFAD;
use crate::sicherheit::geschuetzt;

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

/// Alle GET-Wege des Routers als Muster (`/api/projekte/{slug}/daten/{name}`),
/// ohne die Wege, die es nur in Tests gibt.
fn get_wege() -> Vec<String> {
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
            if weg.starts_with('/') && !weg.starts_with("/api/_test") && bis_zum_naechsten.contains("get(") {
                wege.push(weg.to_string());
            }
        }
    }
    wege.sort();
    wege.dedup();
    wege
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

/// Jeder freie Datenweg, ohne Anmeldung: kein Spürhund in der Antwort.
#[tokio::test]
async fn ohne_anmeldung_kein_persoenlicher_text() {
    let ordner = inhalt_mit_spuerhunden("b2");
    let state = AppState::fuer_tests(&ordner);
    // Aufgabe, Arbeit und Meldung legt der Chat an (per curl, angemeldet).
    let mut ids = vec!["v-1".to_string()];
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
    let ar = ids[3].clone();
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

    let slugs: Vec<String> =
        ["spuer", "spuerbefund", "spuerkaputt", "neu-form", "dashboard", "schaufenster", "kaputt", "neu-kaputt", "gibt-es-nicht"].map(String::from).to_vec();
    let namen: Vec<String> = ["fragen", "fristen", "kanban", "kennzahlen", "fortschritt", "ampel", "fluss", "gantt", "zeitstrahl", "orte", "eigen-notizen"]
        .map(String::from)
        .to_vec();
    let werte = [("{slug}", slugs), ("{name}", namen), ("{id}", ids), ("{file}", vec!["vorschau.html".to_string()]), ("{*path}", vec!["x".to_string()])];
    let wege = get_wege();
    assert!(wege.iter().any(|w| w == "/api/projekte/{slug}/daten/{name}"), "Router nicht gelesen: {wege:?}");
    let mut frei = Vec::new();
    for muster in &wege {
        for weg in einsetzen(muster, &werte) {
            if geschuetzt(&weg) {
                continue;
            }
            // Zusätzlich mit Abfragen, die Wege wie /api/anstehend kennen.
            for anfrage in [weg.clone(), format!("{weg}?projekt=spuer")] {
                let strom = weg == FREIER_KANAL_PFAD;
                let res = router(state.clone()).oneshot(get(&anfrage)).await.unwrap();
                let status = res.status();
                let t = text(res, Duration::from_millis(if strom { 1500 } else { 10_000 })).await;
                for hund in SPUERHUNDE {
                    assert!(!t.contains(hund), "{anfrage} ({status}) gibt ohne Anmeldung „{hund}“ heraus: {t}");
                }
                frei.push(anfrage);
            }
        }
    }
    // Die bekannten freien Wege sind darunter, die Fragen nicht.
    for w in [
        "/api/projects",
        "/api/projects/spuer",
        "/api/projekte/spuer",
        "/api/projekte/spuer/stand",
        "/api/projekte/spuer/daten/kanban",
        FREIER_KANAL_PFAD,
        "/api/stand",
        "/api/pruefung/spuer",
    ] {
        assert!(frei.iter().any(|f| f == w), "{w} fehlt unter den freien Wegen: {frei:?}");
    }
    assert!(!frei.iter().any(|f| f.starts_with("/api/projekte/spuer/daten/fragen")), "die Fragen sind geschützt");
    assert!(!frei.iter().any(|f| f.starts_with("/api/anleitung")), "die Anleitung ist geschützt (Etappe 6b)");
    assert!(!frei.iter().any(|f| f.starts_with("/api/projekte/spuer/daten/fristen") || f.starts_with("/api/fristen")), "die Fristen sind geschützt");
    for weg in ["/api/projekte/spuer/daten/fristen", "/api/fristen"] {
        let res = router(state.clone()).oneshot(get(weg)).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{weg}");
    }
    let res = router(state.clone()).oneshot(get("/api/projekte/spuer/daten/fragen")).await.unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    // Ohne Anmeldung bleiben Zahlen: der Stand nennt zwei offene Fragen und die zwei Aufgaben.
    let res = router(state.clone()).oneshot(get("/api/projekte/spuer/stand")).await.unwrap();
    let v: serde_json::Value = serde_json::from_str(&text(res, Duration::from_secs(5)).await).unwrap();
    assert_eq!((v["zahlen"]["fragen"].as_u64(), v["zahlen"]["aufgaben"].as_u64()), (Some(2), Some(2)), "{v}");
    assert_eq!((v["notizen"].as_array().map(Vec::len), v["notizen_zahl"].as_u64()), (Some(0), Some(2)), "{v}");
    // Fristen vorbei (eine Arbeit, eine aus fristen.toml): Ampel rot, ohne Titel.
    assert_eq!(
        (v["gesamt"]["ampel"].as_str(), v["gesamt"]["grund"].as_str(), v["gesamt"]["bezug"].clone()),
        (Some("rot"), Some("frist-vorbei"), serde_json::Value::Null),
        "{v}"
    );
    assert!(v.get("fristen").is_none(), "ohne Anmeldung keine Fristzeilen: {v}");
    let json = |weg: &'static str| {
        let state = state.clone();
        async move {
            let res = router(state).oneshot(get(weg)).await.unwrap();
            serde_json::from_str::<serde_json::Value>(&text(res, Duration::from_secs(10)).await).unwrap()
        }
    };
    let liste = json("/api/projects").await;
    let spuer = liste["projects"].as_array().unwrap().iter().find(|p| p["slug"] == "spuer").cloned().unwrap();
    assert_eq!((spuer["open"].as_array().map(Vec::len), spuer["open_zahl"].as_u64()), (Some(0), Some(2)), "{spuer}");
    let kopf = json("/api/projekte/spuer").await;
    assert_eq!((kopf["kopf"]["open"].as_array().map(Vec::len), kopf["open_zahl"].as_u64()), (Some(0), Some(2)), "{kopf}");
    let p = json("/api/pruefung/spuerbefund").await;
    assert!(p["befunde"].as_array().is_some_and(Vec::is_empty) && p["fehler"].as_u64().is_some_and(|f| f > 0), "nur Zahlen: {p}");
    let _ = std::fs::remove_dir_all(&ordner);
}

/// Der Leser findet die Wege, auch mehrere je Zeile und über Zeilen hinweg.
#[test]
fn wege_aus_dem_router() {
    let w = get_wege();
    for erwartet in ["/api/projects", "/api/projects/{slug}", "/api/projekte/{slug}/daten/{name}", "/api/events/frei", "/api/betrieb", "/healthz"] {
        assert!(w.iter().any(|x| x == erwartet), "{erwartet} fehlt: {w:?}");
    }
    assert!(!w.iter().any(|x| x.starts_with("/api/_test")), "Test-Wege nicht");
    assert!(!w.iter().any(|x| x == "/api/einstufung"), "nur POST: nicht dabei");
    // Etappe 6b: die Anleitung steht im Router und ist geschützt (wie das
    // Zielbild), fällt also nicht unter die freien Wege dieses Tests.
    for system in ["/api/anleitung", "/api/zielbild", "/api/entscheidungen"] {
        assert!(w.iter().any(|x| x == system), "{system} fehlt: {w:?}");
        assert!(geschuetzt(system), "{system} muss geschützt sein");
    }
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
/// (erste Wand, `sicherheit::prozent_im_pfad`); außerhalb nie 200 auf einem
/// geschützten Weg. Mit Anmeldung gehen normale Wege und Abfragen mit `%` weiter.
#[tokio::test]
async fn prozent_im_pfad_kein_umweg() {
    use crate::fehler::AppError;
    let ordner = inhalt_mit_spuerhunden("prozent");
    let state = AppState::fuer_tests(&ordner);
    let frei = router(state.clone());
    let an = router_angemeldet(state.clone());
    let mut namen: Vec<String> = crate::sicherheit::GESCHUETZTE_DATEIEN.map(String::from).to_vec();
    namen.extend(["orte", "kanban"].map(String::from));
    let werte = [
        ("{slug}", vec!["spuer".to_string()]),
        ("{name}", namen),
        ("{id}", vec!["v-20261104-0001".to_string()]),
        ("{file}", vec!["vorschau.html".to_string()]),
        ("{*path}", vec!["x".to_string()]),
    ];
    // Die Fälle aus dem Fund, ausdrücklich.
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
            let schutz = geschuetzt(&weg);
            faelle.extend(kodiert(&weg).into_iter().map(|k| (k, schutz)));
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
            assert_ne!(status, StatusCode::OK, "{weg}: {t}");
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
        let res = frei.clone().oneshot(get(weg)).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK, "{weg}");
    }

    // Zweite Wand: der Handler selbst, ohne Schleusen davor, mit dem dekodierten Namen.
    use axum::Extension;
    use axum::extract::{Path as Pfad, State};
    let satz = AppError::anmeldung_noetig().fehler;
    for name in ["fragen", "fristen", "FRAGEN", "Fristen"] {
        let res = crate::api::neue_form::daten(State(state.clone()), Pfad(("spuer".to_string(), name.to_string())), None).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{name}");
        assert!(res.headers().contains_key("www-authenticate"), "{name}");
        let t = text(res, Duration::from_secs(5)).await;
        assert!(t.contains(&satz) && !t.contains("SPUERHUND"), "{name}: {t}");
    }
    let mit = Some(Extension(crate::anmeldung::Angemeldet));
    let res = crate::api::neue_form::daten(State(state.clone()), Pfad(("spuer".to_string(), "fristen".to_string())), mit).await;
    assert_eq!(res.status(), StatusCode::OK);
    assert!(text(res, Duration::from_secs(5)).await.contains("SPUERHUND-FRIST"));
    // Die eine Liste: was `geschuetzt` am Pfad schützt, schützt der Handler am Namen.
    for name in crate::sicherheit::GESCHUETZTE_DATEIEN {
        assert!(geschuetzt(&format!("/api/projekte/spuer/daten/{name}")) && crate::sicherheit::datei_geschuetzt(name));
    }
    assert!(!crate::sicherheit::datei_geschuetzt("kanban") && !geschuetzt("/api/projekte/spuer/daten/kanban"));
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
