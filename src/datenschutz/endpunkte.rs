//! Tests je Endpunkt: Was ein öffentlicher Weg hinausgibt, nennt weder das
//! Profil noch den Benutzernamen; die geschützten Wege (Schritt E1, seit E2
//! auch Projekte, PC-Werte und Prüfung) liefern angemeldet im Klartext und
//! ohne Anmeldung 401. Erfundenes Konto
//! `probekonto`; jeder Test legt Texte mit dessen Profil in den Zustand und
//! prüft die Texte der Antwort.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use crate::app::{AppState, beispiel_fuer_tests, router, router_angemeldet};

const NAME: &str = "probekonto";

async fn hole(app: &Router, pfad: &str) -> (StatusCode, Value) {
    let req = Request::get(pfad).header("host", "127.0.0.1:8787").body(Body::empty()).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into())))
}

/// Alle Texte (auch Schlüssel) eines JSON-Werts.
fn texte(v: &Value, aus: &mut Vec<String>) {
    match v {
        Value::String(s) => aus.push(s.clone()),
        Value::Array(a) => a.iter().for_each(|x| texte(x, aus)),
        Value::Object(o) => o.iter().for_each(|(k, x)| {
            aus.push(k.clone());
            texte(x, aus);
        }),
        _ => {}
    }
}

/// Kein Text nennt das erfundene Konto, ein `\Users\`-Profil, ein Profil in
/// der Form von macOS oder Linux oder das echte Profil dieses Prozesses
/// (`USERPROFILE`, sonst `HOME`).
fn ohne_profil(pfad: &str, v: &Value) {
    let mut alle = Vec::new();
    texte(v, &mut alle);
    let echt = ["USERPROFILE", "HOME"].iter().find_map(|n| std::env::var(n).ok().filter(|p| p.len() > 3)).map(|p| p.to_lowercase());
    for t in &alle {
        let k = t.to_lowercase();
        assert!(!k.contains(NAME), "{pfad}: Name in „{t}“");
        assert!(!k.contains(r":\users\") && !k.contains(":/users/"), "{pfad}: Profil in „{t}“");
        assert!(!k.starts_with("/users/") && !k.starts_with("/home/") && !k.contains(" /users/") && !k.contains(" /home/"), "{pfad}: Profil in „{t}“");
        if let Some(e) = &echt {
            assert!(!k.contains(e.as_str()), "{pfad}: echtes Profil in „{t}“");
        }
    }
}

/// `/api/pc` (geschützt seit Schritt E2): `sysapps.errors` aus den
/// Lesefehlern des Sammlers im Klartext; ohne Anmeldung 401.
#[tokio::test]
async fn api_pc() {
    let state = AppState::fuer_tests(&beispiel_fuer_tests());
    let fehler = format!(r"Ereignisse: C:\Users\{NAME}\AppData\Local\x.evtx ist nicht lesbar");
    state.sysapps.write().await.lokal_fehler = vec![fehler.clone()];
    let (status, v) = hole(&router(state.clone()), "/api/pc").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{v}");
    assert!(!v.to_string().contains(NAME), "{v}");
    let (status, v) = hole(&router_angemeldet(state), "/api/pc").await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["sysapps"]["errors"][0], fehler.as_str());
}

/// `/api/pc` (geschützt seit Schritt E2): der Name des Rechners und die Namen
/// der Datenträger im Klartext. Der Test setzt erfundene Werte in den Zustand.
#[tokio::test]
async fn api_pc_rechner_und_datentraeger() {
    use crate::pc::system::DiskInfo;
    let state = AppState::fuer_tests(&beispiel_fuer_tests());
    {
        let mut s = state.snapshot.write().await;
        s.host = "beispiel-rechner".into();
        s.disks = vec![
            DiskInfo { name: "Beispiel Platte".into(), mount: "/Volumes/Beispiel Platte".into(), total: 10, used: 1, belegung: Default::default() },
            DiskInfo { name: String::new(), mount: r"C:\".into(), total: 10, used: 1, belegung: Default::default() },
        ];
    }
    let (status, v) = hole(&router_angemeldet(state), "/api/pc").await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["snapshot"]["host"], "beispiel-rechner");
    assert_eq!(v["snapshot"]["disks"][0]["name"], "Beispiel Platte");
    assert_eq!(v["snapshot"]["disks"][0]["mount"], "/Volumes/Beispiel Platte");
    // Ein leerer Name bleibt leer, ein Laufwerksbuchstabe bleibt.
    assert_eq!(v["snapshot"]["disks"][1]["name"], "");
    assert_eq!(v["snapshot"]["disks"][1]["mount"], r"C:\");
    assert!(!v.to_string().contains("%COMPUTERNAME%") && !v.to_string().contains("%VOLUME%"), "{v}");
}

// ----- Geschützte Wege: angemeldet im Klartext (Schritt E1) -----

/// Die geschützten Wege aus Schritt E1 (und E1b: die Orte, E2: Projekte,
/// PC-Werte, Prüfung). Ohne Anmeldung antworten sie 401 (Schleuse
/// `sicherheit::oeffentlich`, bei einigen zusätzlich der Handler); nur darum
/// dürfen sie im Klartext liefern.
const GESCHUETZT_KLARTEXT: [&str; 15] = [
    "/api/pc/benutzer",
    "/api/pc/verlauf/voll",
    "/api/betrieb/voll",
    "/api/daten",
    "/api/meldungen",
    "/api/projekte/dashboard/verlauf",
    "/api/projekte/dashboard/orte",
    "/api/pc",
    "/api/pc/verlauf",
    "/api/projects",
    "/api/projects/dashboard",
    "/api/projekte/dashboard",
    "/api/projekte/dashboard/stand",
    "/api/projekte/dashboard/daten/orte",
    "/api/pruefung/dashboard",
];

/// Ohne Anmeldung: 401 auf jedem dieser Wege, ohne Inhalt.
#[tokio::test]
async fn klartext_wege_ohne_anmeldung_401() {
    let state = AppState::fuer_tests(&beispiel_fuer_tests());
    meldung_mit_pfad(&state);
    let app = router(state);
    let mut offen = Vec::new();
    for pfad in GESCHUETZT_KLARTEXT {
        let (status, v) = hole(&app, pfad).await;
        if status != StatusCode::UNAUTHORIZED || v.to_string().contains(NAME) {
            offen.push(format!("{pfad}: {status}"));
        }
    }
    assert!(offen.is_empty(), "ohne Anmeldung nicht 401: {offen:?}");
}

/// `/api/pc/benutzer`: `inventory.root`, `inventory.hinweis`,
/// `autostart.command`, `winget_error` angemeldet im Klartext.
#[tokio::test]
async fn api_pc_benutzer_klartext() {
    let state = AppState::fuer_tests(&beispiel_fuer_tests());
    {
        let mut inv = state.inventory.write().await;
        inv.root = format!(r"C:\Users\{NAME}");
        inv.hinweis = Some(format!("Die Ausschlussliste fehlt unter /c/Users/{NAME}/dev/inhalt/data."));
    }
    {
        let mut s = state.sysapps.write().await;
        s.autostart = vec![crate::pc::sysapps::AutostartEntry {
            name: "Werkzeug".into(),
            command: format!(r#""C:\Users\{NAME}\AppData\Local\werkzeug.exe" --leise"#),
            source: "HKCU Run".into(),
        }];
        s.winget_error = Some(format!("winget: file:///C:/Users/{NAME}/winget.log fehlt"));
    }
    let (status, v) = hole(&router_angemeldet(state), "/api/pc/benutzer").await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["inventory"]["root"], format!(r"C:\Users\{NAME}"));
    assert_eq!(v["inventory"]["hinweis"], format!("Die Ausschlussliste fehlt unter /c/Users/{NAME}/dev/inhalt/data."));
    assert_eq!(v["sysapps"]["autostart"][0]["command"], format!(r#""C:\Users\{NAME}\AppData\Local\werkzeug.exe" --leise"#));
    assert_eq!(v["sysapps"]["winget_error"], format!("winget: file:///C:/Users/{NAME}/winget.log fehlt"));
}

/// `/api/pc/benutzer` trägt den Namen des Rechners und die Datenträger mit
/// echten Namen; `/api/pc` ohne Anmeldung gibt 401 (Schritt E2).
#[tokio::test]
async fn api_pc_benutzer_rechner_und_datentraeger() {
    use crate::pc::system::DiskInfo;
    let state = AppState::fuer_tests(&beispiel_fuer_tests());
    {
        let mut s = state.snapshot.write().await;
        s.host = "beispiel-rechner".into();
        s.disks = vec![DiskInfo { name: "Beispiel Platte".into(), mount: "/Volumes/Beispiel Platte".into(), total: 10, used: 1, belegung: Default::default() }];
    }
    let (status, v) = hole(&router_angemeldet(state.clone()), "/api/pc/benutzer").await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["rechner"], "beispiel-rechner");
    assert_eq!(v["platten"][0]["name"], "Beispiel Platte");
    assert_eq!(v["platten"][0]["mount"], "/Volumes/Beispiel Platte");
    assert_eq!(v["platten"][0]["total"], 10);
    let (status, frei) = hole(&router(state), "/api/pc").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(!frei.to_string().contains("beispiel-rechner") && !frei.to_string().contains("Beispiel Platte"), "{frei}");
}

/// Eine Systemmeldung mit Pfad im Titel und Text.
fn meldung_mit_pfad(state: &AppState) {
    let jetzt = crate::speicher::aufgaben::jetzt();
    let n = crate::speicher::meldungen::NeueMeldung {
        projekt: "dashboard".into(),
        art: crate::vertrag::anstehend::MeldungsArt::Fehler,
        schluessel: "dashboard/lesen:x".into(),
        titel: format!(r"C:\Users\{NAME}\dev ist nicht lesbar"),
        text: Some(format!(r"Zugriff verweigert: \\rechner\c$\Users\{NAME}\dev (os error 5)")),
        verweis: None,
        system: true,
        geschuetzt: false,
        von: "server".into(),
    };
    state.speicher.clone().unwrap().meldung_melden(n, crate::speicher::meldungen::Wiederholung::Zustand, &jetzt).unwrap();
}

/// `/api/pc/verlauf/voll`, `/api/pc/verlauf` (seit Schritt E2 gleich) und
/// `/api/projekte/<slug>/verlauf`: die Meldungstexte im Klartext.
#[tokio::test]
async fn verlauf_voll_und_projekt_klartext() {
    let state = AppState::fuer_tests(&beispiel_fuer_tests());
    meldung_mit_pfad(&state);
    let titel = format!(r"C:\Users\{NAME}\dev ist nicht lesbar");
    for pfad in ["/api/pc/verlauf/voll", "/api/pc/verlauf", "/api/projekte/dashboard/verlauf"] {
        let (status, v) = hole(&router_angemeldet(state.clone()), pfad).await;
        assert_eq!(status, StatusCode::OK, "{pfad}: {v}");
        let mut alle = Vec::new();
        texte(&v, &mut alle);
        assert!(alle.contains(&titel), "{pfad}: {v}");
    }
}

/// `/api/meldungen` (die PC-Seite liest dort die Systemmeldungen) im Klartext.
#[tokio::test]
async fn api_meldungen_klartext() {
    let state = AppState::fuer_tests(&beispiel_fuer_tests());
    meldung_mit_pfad(&state);
    let (status, v) = hole(&router_angemeldet(state), "/api/meldungen?projekt=dashboard").await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["meldungen"][0]["titel"], format!(r"C:\Users\{NAME}\dev ist nicht lesbar"));
    assert_eq!(v["meldungen"][0]["text"], format!(r"Zugriff verweigert: \\rechner\c$\Users\{NAME}\dev (os error 5)"));
}

/// `/api/betrieb` (öffentlich): ohne Profil (im Test liegen die Quellen oft
/// im Profil dieses Prozesses).
/// `/api/betrieb/voll` (geschützt): `einstellungen` und alle `quellen` genau
/// im Klartext, unter einem erfundenen Profil.
#[tokio::test]
async fn api_betrieb() {
    use crate::dienst::einstellungen::{Bote, Einstellungen, Inhalt, Konten, Pfade};
    use std::path::PathBuf;
    use std::sync::Arc;
    let mut state = AppState::fuer_tests(&beispiel_fuer_tests());
    let (status, v) = hole(&router(state.clone()), "/api/betrieb").await;
    assert_eq!(status, StatusCode::OK, "{v}");
    ohne_profil("/api/betrieb", &v);
    // Ein Pfad unter dem erfundenen Profil, in der Form des Systems.
    let im_profil =
        |rest: &str| -> String { if cfg!(windows) { format!(r"C:\Users\{NAME}\{}", rest.replace('/', r"\")) } else { format!("/Users/{NAME}/{rest}") } };
    let pfad = |rest: &str| PathBuf::from(im_profil(rest));
    state.einstellungen = Some(Arc::new(Einstellungen {
        datei: pfad("dashboard/dienst.toml"),
        repo: pfad("dev/dashboard"),
        port: 8787,
        pfade: Pfade {
            daten: pfad("dashboard/daten.redb"),
            sicherung: pfad("dashboard/sicherung"),
            protokoll: pfad("dashboard/protokoll"),
            web: pfad("dev/dashboard/web/build"),
            projekte: pfad("dev/inhalt/projects"),
        },
        inhalt: Inhalt {
            projekte: vec![pfad("dev/inhalt/projects"), pfad("dev/zweite/projects")],
            daten: pfad("dev/inhalt/data"),
            entscheidungen: pfad("dev/inhalt/docs/entscheidungen"),
            abschnitt: true,
        },
        konten: Konten::default(),
        bote: Bote { pipe: "dashboard-bote".into(), programm: pfad("dashboard/bote.exe") },
    }));
    state.quellen = Arc::new(vec![pfad("dev/inhalt/projects"), pfad("dev/zweite/projects")]);
    state.entscheidungen_dir = Arc::new(pfad("dev/inhalt/docs/entscheidungen"));
    let (status, v) = hole(&router_angemeldet(state.clone()), "/api/betrieb/voll").await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["einstellungen"], im_profil("dashboard/dienst.toml").as_str(), "{v}");
    assert_eq!(v["quellen"]["projekte"], serde_json::json!([im_profil("dev/inhalt/projects"), im_profil("dev/zweite/projects")]), "{v}");
    assert_eq!(v["quellen"]["daten"], im_profil("dev/inhalt/data").as_str(), "{v}");
    assert_eq!(v["quellen"]["entscheidungen"], im_profil("dev/inhalt/docs/entscheidungen").as_str(), "{v}");
    assert_eq!(v["quellen"]["inhalt_abschnitt"], true, "{v}");
    // Der öffentliche Weg nennt keine davon.
    let (status, v) = hole(&router(state), "/api/betrieb").await;
    assert_eq!(status, StatusCode::OK, "{v}");
    ohne_profil("/api/betrieb", &v);
}

/// Ein Inhalt im Temp-Ordner mit einem Projekt, dessen Kopf Profilpfade nennt
/// (Repo, Link, ein kaputter Link).
fn inhalt_mit_pfaden(name: &str) -> std::path::PathBuf {
    let wurzel = std::env::temp_dir().join(format!("dashboard-datenschutz-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&wurzel);
    let p = wurzel.join("projects").join("probe");
    std::fs::create_dir_all(p.join("daten")).unwrap();
    std::fs::write(
        p.join("projekt.toml"),
        format!(
            "schema = 3\ntitle = \"Probe\"\nstatus = \"aktiv\"\nsummary = \"S\"\n\
             processes = ['C:\\Users\\{NAME}\\werkzeug.exe']\n\
             [[checks]]\nname = \"Dienst\"\nurl = 'http://127.0.0.1:1/C:/Users/{NAME}/x'\n\
             [[links]]\nname = \"Ablage\"\nurl = 'C:\\Users\\{NAME}\\Ablage'\n\
             [[links]]\nname = \"Kaputt\"\nurl = 'siehe /c/Users/{NAME}/x'\n\
             [[repos]]\npath = 'C:\\Users\\{NAME}\\dev\\probe'\n"
        ),
    )
    .unwrap();
    wurzel
}

/// `/api/projects`, `/api/projects/<slug>`, `/api/projekte/<slug>` und
/// `/api/pruefung/<slug>` (geschützt seit Schritt E2): Repo-Pfad und -Fehler
/// (Projektkarte `r.error`), Links, Prozesse und Prüfungen im Klartext; der
/// Befund der Prüfung verbirgt bis E3 weiter (`inhalt::pruefung`). Ohne
/// Anmeldung 401 ohne Inhalt.
#[tokio::test]
async fn api_projekte_und_pruefung() {
    let wurzel = inhalt_mit_pfaden("projekte");
    let state = AppState::fuer_tests(&wurzel);
    let repo = format!(r"C:\Users\{NAME}\dev\probe");
    state.git.eintragen(
        crate::git::RepoStatus {
            path: repo.clone(),
            label: "probe".into(),
            private: false,
            branch: None,
            commit_hash: None,
            commit_date: None,
            commit_subject: None,
            dirty_files: None,
            error: Some(format!("fatal: not a git repository: '{repo}'")),
            stand: None,
        },
        chrono::Local::now(),
    );
    let app = router(state.clone());
    let angemeldet = router_angemeldet(state);
    let (status, v) = hole(&angemeldet, "/api/projects").await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let r = &v["projects"].as_array().unwrap().iter().find(|p| p["slug"] == "probe").unwrap()["repos"][0];
    assert_eq!(r["path"], repo.as_str(), "{v}");
    assert_eq!(r["error"], format!("fatal: not a git repository: '{repo}'").as_str(), "{v}");
    for pfad in ["/api/projects", "/api/projects/probe", "/api/projekte/probe", "/api/pruefung/probe"] {
        let (status, frei) = hole(&app, pfad).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{pfad}");
        ohne_profil(pfad, &frei);
        let (status, v) = hole(&angemeldet, pfad).await;
        assert_eq!(status, StatusCode::OK, "{pfad}: {v}");
        let mut alle = Vec::new();
        texte(&v, &mut alle);
        if pfad == "/api/pruefung/probe" {
            assert!(alle.iter().any(|t| t.contains("„siehe %USERPROFILE%/x“")), "{pfad}: {v}");
        } else if pfad != "/api/projects" {
            assert!(alle.iter().any(|t| *t == format!(r"C:\Users\{NAME}\Ablage")), "{pfad}: {v}");
        }
    }
    let (_, v) = hole(&angemeldet, "/api/projects/probe").await;
    assert_eq!(v["processes"][0]["name"], format!(r"C:\Users\{NAME}\werkzeug.exe").as_str(), "{v}");
    assert_eq!(v["checks"][0]["url"], format!("http://127.0.0.1:1/C:/Users/{NAME}/x").as_str(), "{v}");
    let _ = std::fs::remove_dir_all(&wurzel);
}

/// Ein Fehlersatz auf einem öffentlichen Weg (Schritt E2: `/api/betrieb`,
/// `speicher.fehler`, weiter durch `als_json`) nennt den Ort ohne Profil.
#[tokio::test]
async fn fehlerantwort_ohne_profil() {
    let satz = format!(r"Die Datei C:\Users\{NAME}\.var\dashboard\dashboard.redb ist gesperrt.");
    let state = AppState::fuer_tests_ohne_speicher(&beispiel_fuer_tests(), &satz);
    let (status, v) = hole(&router(state), "/api/betrieb").await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["speicher"]["fehler"], r"Die Datei %USERPROFILE%\.var\dashboard\dashboard.redb ist gesperrt.", "{v}");
    ohne_profil("/api/betrieb", &v);
}

/// Der Vertragsfehler (422) der Wege `/api/projekte/<slug>` und
/// `/daten/<name>`: Ein Lesefehler zitiert die Zeile, hier einen Profilpfad
/// neben einem Tippfehler. Seit Schritt E2 sind die Wege geschützt: ohne
/// Anmeldung 401 ohne Inhalt, angemeldet zitiert der Satz im Klartext.
#[tokio::test]
async fn vertragsfehler_klartext() {
    /// Räumt auch auf, wenn der Test scheitert.
    struct Weg(std::path::PathBuf);
    impl Drop for Weg {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let wurzel = std::env::temp_dir().join(format!("dashboard-datenschutz-vertrag-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&wurzel);
    let _weg = Weg(wurzel.clone());
    let p = wurzel.join("projects").join("probe");
    std::fs::create_dir_all(p.join("daten")).unwrap();
    std::fs::write(p.join("projekt.toml"), format!("schema = 3\ntitle = \"Probe\"\nstatus = 'C:\\Users\\{NAME}\\dev\\probe'\nsummary = \"S\"\n")).unwrap();
    std::fs::write(
        p.join("daten").join("orte.toml"),
        format!("[[ort]]\ntitel = 'C:\\Users\\{NAME}\\x'\nkennung = \"A B\"\npfad = 'C:\\Users\\{NAME}\\x.pdf'\n"),
    )
    .unwrap();
    let state = AppState::fuer_tests(&wurzel);
    let app = router(state.clone());
    let angemeldet = router_angemeldet(state);
    for pfad in ["/api/projekte/probe", "/api/projekte/probe/daten/orte"] {
        let (status, frei) = hole(&app, pfad).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{pfad}: {frei}");
        ohne_profil(pfad, &frei);
        // Mit Anmeldung zitiert der Satz die Zeile wirklich, im Klartext.
        let (status, v) = hole(&angemeldet, pfad).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{pfad}: {v}");
        assert!(
            v["satz"].as_str().unwrap_or_default().contains(&format!(r"C:\Users\{NAME}"))
                || v["fehler"].as_str().unwrap_or_default().contains(&format!(r"C:\Users\{NAME}")),
            "{pfad}: {v}"
        );
        assert!(!v.to_string().contains("%USERPROFILE%"), "{pfad}: {v}");
    }
    let _ = std::fs::remove_dir_all(&wurzel);
}
