//! Tests je Endpunkt: Was hinausgeht, nennt weder das Profil noch den
//! Benutzernamen. Erfundenes Konto
//! `probekonto`; jeder Test legt Texte mit dessen Profil in den Zustand und
//! prüft alle Texte der Antwort.

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

/// `/api/pc` (frei): `sysapps.errors` aus den Lesefehlern des Sammlers.
#[tokio::test]
async fn api_pc() {
    let state = AppState::fuer_tests(&beispiel_fuer_tests());
    state.sysapps.write().await.lokal_fehler = vec![format!(r"Ereignisse: C:\Users\{NAME}\AppData\Local\x.evtx ist nicht lesbar")];
    let (status, v) = hole(&router(state), "/api/pc").await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["sysapps"]["errors"][0], r"Ereignisse: %USERPROFILE%\AppData\Local\x.evtx ist nicht lesbar");
    ohne_profil("/api/pc", &v);
}

/// `/api/pc` (frei): nie der Name des Rechners, nie der Name eines
/// Datenträgers. Der Test setzt erfundene Werte in den Zustand.
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
    let (status, v) = hole(&router(state), "/api/pc").await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["snapshot"]["host"], "%COMPUTERNAME%");
    assert_eq!(v["snapshot"]["disks"][0]["name"], "%VOLUME%");
    assert_eq!(v["snapshot"]["disks"][0]["mount"], "/Volumes/%VOLUME%");
    // Ein leerer Name bleibt leer, ein Laufwerksbuchstabe bleibt.
    assert_eq!(v["snapshot"]["disks"][1]["name"], "");
    assert_eq!(v["snapshot"]["disks"][1]["mount"], r"C:\");
    let mut alle = Vec::new();
    texte(&v, &mut alle);
    for t in &alle {
        let k = t.to_lowercase();
        assert!(!k.contains("beispiel-rechner") && !k.contains("beispiel platte"), "/api/pc: Name in „{t}“");
    }
    ohne_profil("/api/pc", &v);
}

/// `/api/pc/benutzer`: `inventory.root`, `inventory.hinweis`,
/// `autostart.command`, `winget_error`.
#[tokio::test]
async fn api_pc_benutzer() {
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
    assert_eq!(v["inventory"]["root"], "%USERPROFILE%");
    assert_eq!(v["inventory"]["hinweis"], "Die Ausschlussliste fehlt unter %USERPROFILE%/dev/inhalt/data.");
    assert_eq!(v["sysapps"]["autostart"][0]["command"], r#""%USERPROFILE%\AppData\Local\werkzeug.exe" --leise"#);
    assert_eq!(v["sysapps"]["winget_error"], "winget: %USERPROFILE%/winget.log fehlt");
    ohne_profil("/api/pc/benutzer", &v);
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

/// `/api/pc/verlauf/voll`: die Meldungstexte.
#[tokio::test]
async fn api_pc_verlauf_voll() {
    let state = AppState::fuer_tests(&beispiel_fuer_tests());
    meldung_mit_pfad(&state);
    let (status, v) = hole(&router_angemeldet(state), "/api/pc/verlauf/voll").await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let mut alle = Vec::new();
    texte(&v, &mut alle);
    assert!(alle.iter().any(|t| t == r"%USERPROFILE%\dev ist nicht lesbar"), "{v}");
    ohne_profil("/api/pc/verlauf/voll", &v);
}

/// `/api/meldungen` (die PC-Seite liest dort die Systemmeldungen).
#[tokio::test]
async fn api_meldungen() {
    let state = AppState::fuer_tests(&beispiel_fuer_tests());
    meldung_mit_pfad(&state);
    let (status, v) = hole(&router_angemeldet(state), "/api/meldungen?projekt=dashboard").await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["meldungen"][0]["titel"], r"%USERPROFILE%\dev ist nicht lesbar");
    assert_eq!(v["meldungen"][0]["text"], r"Zugriff verweigert: %USERPROFILE%\dev (os error 5)");
    ohne_profil("/api/meldungen", &v);
}

/// `/api/betrieb` und `/api/betrieb/voll`: `einstellungen`, `quellen`,
/// `ordner[].stand` und Fehlersätze ohne Profil (im Test liegen die Quellen
/// oft im Profil dieses Prozesses).
#[tokio::test]
async fn api_betrieb() {
    let state = AppState::fuer_tests(&beispiel_fuer_tests());
    for pfad in ["/api/betrieb", "/api/betrieb/voll"] {
        let (status, v) = hole(&router_angemeldet(state.clone()), pfad).await;
        assert_eq!(status, StatusCode::OK, "{pfad}: {v}");
        ohne_profil(pfad, &v);
    }
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
/// `/api/pruefung/<slug>`: Repo-Pfad und -Fehler (Projektkarte `r.error`),
/// Links und der Befund, der die Adresse zitiert.
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
    let (status, v) = hole(&app, "/api/projects").await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let r = &v["projects"].as_array().unwrap().iter().find(|p| p["slug"] == "probe").unwrap()["repos"][0];
    assert_eq!(r["path"], r"%USERPROFILE%\dev\probe", "{v}");
    assert_eq!(r["error"], r"fatal: not a git repository: '%USERPROFILE%\dev\probe'", "{v}");
    ohne_profil("/api/projects", &v);
    // Befund-Sätze gibt es seit B2 nur mit Anmeldung; ohne Profil sind sie auch dann.
    let angemeldet = router_angemeldet(state);
    for pfad in ["/api/projects/probe", "/api/projekte/probe", "/api/pruefung/probe"] {
        let (_, frei) = hole(&app, pfad).await;
        ohne_profil(pfad, &frei);
        let (status, v) = hole(&angemeldet, pfad).await;
        assert_eq!(status, StatusCode::OK, "{pfad}: {v}");
        ohne_profil(pfad, &v);
        let mut alle = Vec::new();
        texte(&v, &mut alle);
        if pfad == "/api/pruefung/probe" {
            assert!(alle.iter().any(|t| t.contains("„siehe %USERPROFILE%/x“")), "{pfad}: {v}");
        } else {
            assert!(alle.iter().any(|t| t == r"%USERPROFILE%\Ablage"), "{pfad}: {v}");
        }
    }
    let _ = std::fs::remove_dir_all(&wurzel);
}

/// Fehlerantworten (`AppError`) auf einem freien Weg: Der Satz nennt den Ort
/// ohne Profil (hier: das Projekt gibt es nicht, der Satz zitiert den Slug).
#[tokio::test]
async fn fehlerantwort_ohne_profil() {
    let state = AppState::fuer_tests(&beispiel_fuer_tests());
    let (status, v) = hole(&router(state), "/api/projects/gibt-es-nicht").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{v}");
    ohne_profil("/api/projects/gibt-es-nicht", &v);
}

/// Blockierend aus der Abnahme: Der Vertragsfehler (422) der Wege
/// `/api/projekte/<slug>` und `/daten/<name>` lief an `AppError` vorbei.
/// Ein Lesefehler zitiert die Zeile, hier einen Profilpfad neben einem
/// Tippfehler; Körper (`fehler`, `satz`, `datei`) und Satz sind ohne Profil.
#[tokio::test]
async fn vertragsfehler_ohne_profil() {
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
        // Ohne Anmeldung (seit B2) nur ein neutraler Satz, der nichts zitiert.
        let (status, frei) = hole(&app, pfad).await;
        assert!(status.is_client_error(), "{pfad}: {status} {frei}");
        ohne_profil(pfad, &frei);
        // Mit Anmeldung zitiert der Satz die Zeile wirklich, aber ohne Profil.
        let (status, v) = hole(&angemeldet, pfad).await;
        assert!(status.is_client_error(), "{pfad}: {status} {v}");
        ohne_profil(pfad, &v);
        assert!(v.to_string().contains("%USERPROFILE%"), "{pfad}: {v}");
    }
    let _ = std::fs::remove_dir_all(&wurzel);
}
