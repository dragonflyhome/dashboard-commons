//! Ende-zu-Ende: `dashboard-bote.exe` als Kindprozess mit eigener Pipe
//! (`dashboard-test-<name>-<pid>`), der Client des Dienstes verbindet sich und
//! prüft Programm und Konto des Servers sowie den Besitzer der Pipe (Etappe 7a1).
//! Dazu: viele Anfragen zugleich, Git-Stand von acht Repos über /api/projects,
//! das Protokoll des Boten (Unterordner und Ersatzordner) und der Wächter.
//!
//! Alle Pfade liegen in einem eigenen Temp-Ordner; auch `LOCALAPPDATA` zeigt
//! für die Kinder dorthin. Der Tresor wird nur mit `ranke/test-…` befragt, nie
//! beschrieben.
#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::time::{Duration, Instant};

use dashboard::bote::client::{Bote, PLAETZE, SATZ_FREMD};
use dashboard::bote::waechter::TEST_EXIT;
use dashboard::dienst::einstellungen::Einstellungen;
use dashboard::dienst::programme::{self, Werkzeug};
use dashboard::vertrag::bote::{Anfrage, Antwort};

const BOTE: &str = env!("CARGO_BIN_EXE_dashboard-bote");
const DASHBOARD: &str = env!("CARGO_BIN_EXE_dashboard");

fn schraeg(p: &Path) -> String {
    p.display().to_string().replace('\\', "/")
}

fn beispiel() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel")
}

/// Temp-Ordner mit dienst.toml; räumt beim Freigeben auf.
struct Ort {
    ordner: PathBuf,
    sid: String,
    pipe: String,
    repo: PathBuf,
    protokoll: PathBuf,
}

impl Ort {
    fn neu(name: &str) -> Ort {
        Self::mit(name, |_| {})
    }

    /// Mit Änderungen vor dem Schreiben der dienst.toml.
    fn mit(name: &str, aendern: impl FnOnce(&mut Ort)) -> Ort {
        let ordner = std::env::temp_dir().join(format!("dashboard-bote-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        std::fs::create_dir_all(&ordner).unwrap();
        let sid = dashboard::bote::win::eigene_sid().unwrap();
        let mut ort = Ort { pipe: format!("dashboard-test-{name}-{}", std::process::id()), protokoll: ordner.join("protokoll"), ordner, sid, repo: beispiel() };
        aendern(&mut ort);
        std::fs::write(ort.datei(), ort.text(BOTE, &ort.sid)).unwrap();
        ort
    }

    fn datei(&self) -> PathBuf {
        self.ordner.join("dienst.toml")
    }

    fn text(&self, programm: &str, inhaber_sid: &str) -> String {
        format!(
            "schema = 1\nrepo = \"{}\"\nport = 8788\n\n[pfade]\ndaten = \"{}\"\nsicherung = \"{}\"\nprotokoll = \"{}\"\n\n\
             [konten]\ninhaber_sid = \"{inhaber_sid}\"\ndienst_sid = \"{}\"\n\n[bote]\npipe = \"{}\"\nprogramm = \"{}\"\n",
            schraeg(&self.repo),
            schraeg(&self.ordner.join("daten/dashboard.redb")),
            schraeg(&self.ordner.join("sicherung")),
            schraeg(&self.protokoll),
            self.sid,
            self.pipe,
            programm.replace('\\', "/"),
        )
    }

    /// Einstellungen des Dienstes (Client) mit eigenem Programm und Konto.
    fn einstellungen(&self, programm: &str, inhaber_sid: &str) -> Einstellungen {
        Einstellungen::aus_text(&self.datei(), &self.text(programm, inhaber_sid)).unwrap()
    }

    /// `%LOCALAPPDATA%` der Kinder.
    fn lokal(&self) -> PathBuf {
        self.ordner.join("lokal")
    }

    fn starten(&self) -> Child {
        self.starten_mit(&[], &[])
    }

    fn starten_mit(&self, vorne: &[&str], umgebung: &[(&str, &str)]) -> Child {
        let mut c = programme::eigener_befehl(BOTE);
        c.args(vorne)
            .arg("--einstellungen")
            .arg(self.datei())
            .env("LOCALAPPDATA", self.lokal())
            .env_remove(TEST_EXIT)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for (k, v) in umgebung {
            c.env(k, v);
        }
        c.spawn().expect("dashboard-bote.exe startet")
    }

    /// Alle Tagesdateien `bote-*.log` in einem Ordner.
    fn tagesdateien(ordner: &Path) -> String {
        let Ok(rd) = std::fs::read_dir(ordner) else { return String::new() };
        rd.filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with("bote-"))
            .map(|e| std::fs::read_to_string(e.path()).unwrap_or_default())
            .collect()
    }

    /// Das Protokoll des Boten (`<protokoll>\bote\`).
    fn protokoll(&self) -> String {
        Self::tagesdateien(&self.protokoll.join("bote"))
    }

    /// Zeilen „Anfrage …“ im Protokoll des Boten.
    fn anfragen_im_protokoll(&self) -> usize {
        self.protokoll().lines().filter(|l| l.contains("Anfrage ")).count()
    }
}

impl Drop for Ort {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.ordner);
    }
}

/// Beendet das Kind auch dann, wenn eine Prüfung fehlschlägt.
struct Kind(Child);

impl Drop for Kind {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Wartet, bis die Pipe da ist (Verbinden sendet nichts).
async fn warten_bis_bereit(bote: &Bote, kind: &mut Kind) {
    let start = Instant::now();
    loop {
        match bote.verbinden() {
            Ok(_) => return,
            Err(satz) => {
                if let Ok(Some(status)) = kind.0.try_wait() {
                    panic!("Bote vorzeitig beendet ({status}): {satz}");
                }
                assert!(start.elapsed() < Duration::from_secs(15), "Bote nicht bereit: {satz}");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

/// Wartet, bis das Kind endet; gibt den Code zurück.
async fn ende_abwarten(kind: &mut Kind, hoechstens: Duration) -> Option<i32> {
    let start = Instant::now();
    loop {
        if let Some(s) = kind.0.try_wait().unwrap() {
            return s.code();
        }
        assert!(start.elapsed() < hoechstens, "Kind endet nicht");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn pipe_ende_zu_ende() {
    let ort = Ort::neu("e2e");
    let mut kind = Kind(ort.starten());
    let bote = Bote::neu(&ort.einstellungen(BOTE, &ort.sid));
    warten_bis_bereit(&bote, &mut kind).await;

    match bote.anfragen(&Anfrage::Hallo).await.unwrap() {
        Antwort::Hallo { konto_sid, version, sitzung, .. } => {
            assert_eq!(konto_sid, ort.sid);
            assert_eq!(version, env!("CARGO_PKG_VERSION"));
            // Hotfix Sitzungsabfrage: Der Bote meldet seine Sitzung (die des
            // Testlaufs), der Dienst merkt sie sich im Stand.
            let s = sitzung.expect("Sitzung im Hallo");
            assert_eq!(bote.stand().sitzung, Some(s));
            assert!(bote.stand().sitzung_gemeldet.is_some());
        }
        a => panic!("{a:?}"),
    }
    let a = bote.anfragen(&Anfrage::TresorInfo { ziel: "ranke/test-gibts-nicht".into() }).await.unwrap();
    assert!(matches!(a, Antwort::TresorInfo { vorhanden: false, gespeichert_am: None, benutzer: None }), "{a:?}");
    assert!(bote.stand().verbunden);
    assert_eq!(ort.anfragen_im_protokoll(), 2);

    // Der Bote hat dem Dienstkonto (hier: dem eigenen) das Abfragen erlaubt.
    assert!(ort.protokoll().contains("Das Dienstkonto darf Programm und Konto des Boten abfragen."), "{}", ort.protokoll());
    // Er schreibt in den Unterordner, nicht in den Protokollordner selbst und nicht in den Ersatz.
    assert!(Ort::tagesdateien(&ort.protokoll).is_empty());
    assert!(!ort.protokoll().contains("hier statt in"), "{}", ort.protokoll());
    assert!(!ort.lokal().exists());

    // Falsches Programm oder falsches Konto: abgelehnt, nichts gesendet. Mit
    // erwarteter fremder SID scheitert schon der Besitzer der Pipe (Weg 2);
    // mit der eigenen SID besteht er, und das Programm (Weg 1) entscheidet.
    for (programm, sid, grund) in [(DASHBOARD, ort.sid.as_str(), "Programm "), (BOTE, "S-1-5-18", "Besitzer der Pipe ist ")] {
        let fremd = Bote::neu(&ort.einstellungen(programm, sid));
        let satz = fremd.verbinden().err().expect("muss abgelehnt werden");
        assert!(satz.0.starts_with(SATZ_FREMD), "{satz}");
        assert!(satz.0.contains(grund), "{satz}");
        let satz = fremd.anfragen(&Anfrage::Hallo).await.unwrap_err();
        assert!(satz.0.starts_with(SATZ_FREMD), "{satz}");
        assert!(!fremd.stand().verbunden);
        assert_eq!(fremd.stand().lage, dashboard::bote::client::Lage::Getrennt);
    }
    assert_eq!(ort.anfragen_im_protokoll(), 2, "nach den Ablehnungen kam keine Anfrage an");

    // Git-Stand nur für Repos der Projektseiten (im Beispiel keins, das es gibt).
    let a = bote.anfragen(&Anfrage::GitStatus { pfad: env!("CARGO_MANIFEST_DIR").into(), privat: false }).await.unwrap();
    assert!(matches!(&a, Antwort::Fehler { satz } if satz.contains("nur für Repos")), "{a:?}");

    // Ein zweiter Bote mit demselben Namen bekommt die Pipe nicht (FIRST_PIPE_INSTANCE).
    let mut zweiter = Kind(ort.starten());
    assert_eq!(ende_abwarten(&mut zweiter, Duration::from_secs(15)).await, Some(4));

    // Der erste bedient weiter.
    assert!(matches!(bote.anfragen(&Anfrage::Hallo).await, Ok(Antwort::Hallo { .. })));
    drop(kind);
}

/// Zwölf Anfragen zugleich über den Client: alle beantwortet, höchstens
/// `PLAETZE` Verbindungen offen. Und zwölf rohe Verbindungen zugleich: der
/// Bote hat genug Instanzen (früher 4, dann „beschäftigt“).
#[tokio::test(flavor = "multi_thread")]
async fn viele_anfragen_zugleich() {
    let ort = Ort::neu("viele");
    let mut kind = Kind(ort.starten());
    let bote = Bote::neu(&ort.einstellungen(BOTE, &ort.sid));
    warten_bis_bereit(&bote, &mut kind).await;

    let fragen = (0..12).map(|i| {
        let bote = &bote;
        async move {
            let a = if i % 2 == 0 { Anfrage::Hallo } else { Anfrage::TresorInfo { ziel: format!("ranke/test-gibts-nicht-{i}") } };
            bote.anfragen(&a).await
        }
    });
    let antworten = futures_util::future::join_all(fragen).await;
    for (i, a) in antworten.iter().enumerate() {
        assert!(matches!(a, Ok(Antwort::Hallo { .. }) | Ok(Antwort::TresorInfo { .. })), "{i}: {a:?}");
    }
    assert!(bote.ruhende() <= PLAETZE, "{}", bote.ruhende());
    assert_eq!(bote.ruhende() + bote.freie_plaetze(), PLAETZE, "jede offene Verbindung hält einen Platz");
    assert_eq!(bote.stand().lage, dashboard::bote::client::Lage::Verbunden);
    assert_eq!(ort.anfragen_im_protokoll(), 12);

    // Roh: zwölf Verbindungen gleichzeitig offen, jede fragt einmal. Vorher
    // die ruhenden Verbindungen des Clients schließen (sonst wären es bis zu 20).
    drop(bote);
    let name = ort.pipe.clone();
    let roh = tokio::task::spawn_blocking(move || {
        use dashboard::bote::win::{self, Gelesen, Zeilen};
        let rohre: Vec<_> = (0..12).map(|i| win::pipe_oeffnen(&name, 5000).unwrap_or_else(|e| panic!("{i}: {e}"))).collect();
        for r in &rohre {
            r.schreiben(b"{\"art\":\"hallo\"}\n").unwrap();
        }
        rohre.iter().all(|r| matches!(Zeilen::default().naechste(r), Ok(Gelesen::Zeile(_))))
    });
    assert!(roh.await.unwrap(), "alle zwölf rohen Verbindungen beantwortet");
}

/// Ein kleines Git-Repo mit einem Commit (schnell, unabhängig vom Stand dieses Repos).
fn kleines_repo(ordner: &Path) {
    std::fs::create_dir_all(ordner).unwrap();
    std::fs::write(ordner.join("liesmich.txt"), "Test\n").unwrap();
    for args in
        [&["init", "-q"][..], &["add", "liesmich.txt"], &["-c", "user.name=Test", "-c", "user.email=test@example.invalid", "commit", "-q", "-m", "eins"]]
    {
        let out = programme::befehl(Werkzeug::Git).expect("git an festem Ort").arg("-C").arg(ordner).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }
}

/// /api/projects mit einem Projekt, das acht kleine Repos nennt: Der
/// Git-Zwischenspeicher holt ihren Stand beim Start über den Boten (höchstens
/// zwei zugleich), keins „beschäftigt“ oder „ausgelastet“; die Liste selbst
/// fragt nie git.
#[tokio::test(flavor = "multi_thread")]
async fn acht_repos_ueber_den_boten() {
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let ort = Ort::mit("acht", |o| {
        o.repo = o.ordner.join("repo");
        let projekt = o.repo.join("projects").join("acht");
        std::fs::create_dir_all(&projekt).unwrap();
        let repos: String = (0..8)
            .map(|i| {
                let ordner = o.ordner.join(format!("klein{i}"));
                kleines_repo(&ordner);
                format!("\n[[repos]]\npath = \"{}\"\nlabel = \"r{i}\"\n", schraeg(&ordner))
            })
            .collect();
        std::fs::write(projekt.join("projekt.toml"), format!("schema = 3\ntitle = \"Acht Repos\"\nstatus = \"aktiv\"\n{repos}")).unwrap();
    });
    let mut kind = Kind(ort.starten());
    let e = ort.einstellungen(BOTE, &ort.sid);
    let probe = Bote::neu(&e);
    warten_bis_bereit(&probe, &mut kind).await;

    let (state, _abschalter) = dashboard::app::AppState::aus_einstellungen(&e).unwrap();
    let app = dashboard::app::router(state);
    // Der erste Lauf beginnt mit dem Start; bis alle acht einen Stand haben,
    // antwortet die Liste trotzdem sofort (mit „wird ermittelt“).
    let ende = std::time::Instant::now() + Duration::from_secs(30);
    let (bytes, v) = loop {
        let anfrage = axum::http::Request::get("/api/projects").header("host", "127.0.0.1:8788").body(axum::body::Body::empty()).unwrap();
        let start = std::time::Instant::now();
        let res = app.clone().oneshot(anfrage).await.unwrap();
        assert_eq!(res.status(), 200);
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        assert!(start.elapsed() < Duration::from_millis(500), "Liste wartet auf git: {:?}", start.elapsed());
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let fertig = v["projects"][0]["repos"].as_array().is_some_and(|r| r.iter().all(|r| r["stand"].is_string()));
        if fertig || std::time::Instant::now() > ende {
            break (bytes, v);
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    let projekte = v["projects"].as_array().unwrap();
    assert_eq!(projekte.len(), 1, "{v}");
    let repos = projekte[0]["repos"].as_array().unwrap();
    assert_eq!(repos.len(), 8, "{v}");
    assert!(repos.iter().all(|r| r["stand"].is_string()), "nicht alle erneuert: {v}");
    // Jede Anfrage kam beim Boten an und wurde beantwortet. Das Budget von
    // git selbst (3 s je Repo im Boten) kann unter der Last der parallelen
    // Tests knapp werden; das ist dann „git antwortet nicht“, kein Pipe-Fehler.
    for r in repos {
        let fehler = r["error"].as_str();
        assert!(fehler.is_none() || fehler == Some("git antwortet nicht (Zeitlimit)"), "{r}");
    }
    assert!(repos.iter().any(|r| r["branch"].is_string()), "{v}");
    let text = String::from_utf8_lossy(&bytes);
    assert!(!text.contains("beschäftigt") && !text.contains("ausgelastet"), "{text}");
}

/// Der Bote startet erst nach dem Dienst: Der erste Lauf des
/// Git-Zwischenspeichers findet keine Pipe („Git-Stand unbekannt“). Startet
/// der Bote dann, merkt `bote_beobachten` das selbst (Hallo je Takt, keine
/// andere Anfrage nötig) und holt sofort alle Stände, lange vor dem
/// 60-s-Takt.
#[tokio::test(flavor = "multi_thread")]
async fn bote_kommt_spaeter_git_stand_sofort() {
    use dashboard::git_zwischenspeicher::{BOTE_TAKT, TAKT, Zwischenspeicher};
    use std::sync::Arc;

    let mut pfade = Vec::new();
    let ort = Ort::mit("spaeter", |o| {
        o.repo = o.ordner.join("repo");
        let projekt = o.repo.join("projects").join("spaeter");
        std::fs::create_dir_all(&projekt).unwrap();
        let repos: String = (0..2)
            .map(|i| {
                let ordner = o.ordner.join(format!("klein{i}"));
                kleines_repo(&ordner);
                pfade.push(schraeg(&ordner));
                format!("\n[[repos]]\npath = \"{}\"\nlabel = \"r{i}\"\n", schraeg(&ordner))
            })
            .collect();
        std::fs::write(projekt.join("projekt.toml"), format!("schema = 3\ntitle = \"Später\"\nstatus = \"aktiv\"\n{repos}")).unwrap();
    });
    let e = ort.einstellungen(BOTE, &ort.sid);
    let bote = Arc::new(Bote::neu(&e));
    let z = Arc::new(Zwischenspeicher::neu(vec![ort.repo.join("projects")], Some(bote.clone())));

    // Erster Lauf ohne Boten: jedes Repo kennt nur den Grund.
    assert_eq!(z.alle_erneuern().await, 2);
    for p in &pfade {
        let s = z.stand_fuer(p).expect("Stand");
        assert!(s.status.error.as_deref().is_some_and(|f| f.starts_with("Git-Stand unbekannt")), "{p}: {:?}", s.status.error);
    }
    assert!(!bote.stand().verbunden);
    z.clone().bote_beobachten(bote.clone());

    // Erst jetzt startet der Bote; sonst fragt niemand.
    let mut kind = Kind(ort.starten());
    let start = Instant::now();
    let grenze = TAKT / 3;
    while pfade.iter().any(|p| z.stand_fuer(p).is_none_or(|s| s.status.error.is_some())) {
        if let Ok(Some(status)) = kind.0.try_wait() {
            panic!("Bote vorzeitig beendet ({status})");
        }
        assert!(
            start.elapsed() < grenze,
            "nach {:?} noch kein Stand: {:?}",
            start.elapsed(),
            pfade.iter().map(|p| z.stand_fuer(p).map(|s| s.status.error)).collect::<Vec<_>>()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(bote.stand().verbunden);
    assert!(pfade.iter().all(|p| z.stand_fuer(p).is_some_and(|s| s.status.branch.is_some())));
    eprintln!("Stand nach {:?} (Takt des Beobachters {:?})", start.elapsed(), BOTE_TAKT);
}

/// Bau-Skript des Test-Repos (statt SvelteKit): wartet laut `modus.txt`
/// (`warte` 2,5 s, `gruen` gar nicht) und legt dann `index.html` mit dem
/// Ordnernamen in `DASHBOARD_BUILD_ZIEL` an.
const BAU_SKRIPT: &str = r#"import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, join } from 'node:path';
const ziel = process.env.DASHBOARD_BUILD_ZIEL;
if (readFileSync('modus.txt', 'utf8').trim() === 'warte') {
  await new Promise((r) => setTimeout(r, 2500));
}
mkdirSync(ziel, { recursive: true });
writeFileSync(join(ziel, 'index.html'), '<!doctype html><title>gebaut ' + basename(ziel) + '</title>');
"#;

/// Ein Repo mit `projects/` und einem kleinen `web/` (package.json, Bau-Skript, `web/build`).
fn bau_repo(repo: &Path, modus: &str) {
    std::fs::create_dir_all(repo.join("projects")).unwrap();
    std::fs::create_dir_all(repo.join("web").join("build")).unwrap();
    std::fs::write(repo.join("web/package.json"), r#"{ "name": "test", "private": true, "type": "module", "scripts": { "build": "node bau.mjs" } }"#).unwrap();
    std::fs::write(repo.join("web/bau.mjs"), BAU_SKRIPT).unwrap();
    std::fs::write(repo.join("web/modus.txt"), modus).unwrap();
    std::fs::write(repo.join("web/build/index.html"), "<!doctype html><title>web-build</title>").unwrap();
}

/// Etappe 4v, Bau über den Boten mit einem Test-Repo:
/// 1. Direkt über den Client: Ein langer Bau (2,5 s) blockiert keine anderen
///    Anfragen; Hallo kommt währenddessen sofort. Danach zeigt der Zeiger auf
///    den neuen Ordner, die Bauliste ist geschrieben.
/// 2. Ganzer Weg im Dienst: Vorgang `bau` im Datenspeicher, Anstoß, der
///    Baumeister fragt den Boten, der Vorgang wird grün, und `/` kommt ohne
///    Neustart aus dem neuen Ordner.
#[tokio::test(flavor = "multi_thread")]
async fn bau_ueber_den_boten() {
    use dashboard::api::vertrag::{BauZustand, VorgangsArt};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    if programme::npm_befehl_async().is_err() {
        dashboard::ueberspringen("node oder npm fehlt an den festen Orten.");
        return;
    }
    let ort = Ort::mit("bau", |o| {
        o.repo = o.ordner.join("repo");
        bau_repo(&o.repo, "warte");
    });
    let mut kind = Kind(ort.starten());
    let e = ort.einstellungen(BOTE, &ort.sid);
    let bote = std::sync::Arc::new(Bote::neu(&e));
    warten_bis_bereit(&bote, &mut kind).await;

    // 1. Langer Bau und daneben Hallo.
    let b2 = bote.clone();
    let start = Instant::now();
    let bau = tokio::spawn(async move { b2.anfragen(&Anfrage::Bau).await });
    tokio::time::sleep(Duration::from_millis(300)).await;
    for _ in 0..3 {
        let t = Instant::now();
        assert!(matches!(bote.anfragen(&Anfrage::Hallo).await, Ok(Antwort::Hallo { .. })));
        assert!(t.elapsed() < Duration::from_secs(1), "Hallo wartete {:?}", t.elapsed());
    }
    assert!(!bau.is_finished(), "der Bau läuft noch, Hallo kam trotzdem");
    let erg = match bau.await.unwrap().unwrap() {
        Antwort::Bau(erg) => erg,
        a => panic!("{a:?}"),
    };
    assert!(erg.gruen, "{erg:?}");
    assert!(start.elapsed() >= Duration::from_millis(2500));
    let erster = erg.ordner.clone().unwrap();
    let zeiger = || std::fs::read_to_string(ort.repo.join("web/builds/aktuell")).unwrap().trim().to_string();
    assert_eq!(zeiger(), erster);
    assert!(ort.repo.join("web/src/lib/projekte.generated.ts").is_file(), "Bauliste geschrieben");
    assert!(ort.protokoll().contains("Anfrage bau"), "{}", ort.protokoll());

    // 2. Der ganze Weg im Dienst.
    std::fs::write(ort.repo.join("web/modus.txt"), "gruen").unwrap();
    let (state, _abschalter) = dashboard::app::AppState::aus_einstellungen(&e).unwrap();
    let app = dashboard::app::router(state.clone());
    let seite = |app: axum::Router| async move {
        let anfrage = axum::http::Request::get("/").header("host", "127.0.0.1:8788").body(axum::body::Body::empty()).unwrap();
        let res = app.oneshot(anfrage).await.unwrap();
        assert_eq!(res.status(), 200);
        String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
    };
    assert!(seite(app.clone()).await.contains(&format!("gebaut {erster}")), "aus dem ersten Bau");
    let speicher = state.speicher.clone().expect("Datenspeicher offen");
    let (v, _) = speicher.vorgang_anlegen(VorgangsArt::Bau, "architektur", "Bitte bauen", None, "chat:architektur").unwrap();
    state.baumeister.anstossen();
    let frist = Instant::now() + Duration::from_secs(120);
    let fertig = loop {
        let jetzt = speicher.vorgang_kopf(&v.id).unwrap().unwrap();
        if jetzt.bau.as_ref().is_some_and(|b| b.zustand != BauZustand::Laeuft) {
            break jetzt;
        }
        assert!(Instant::now() < frist, "Bau nicht fertig: {jetzt:?}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    let stand = fertig.bau.unwrap();
    assert_eq!(stand.zustand, BauZustand::Gruen, "{}", stand.meldung);
    let zweiter = zeiger();
    assert_ne!(zweiter, erster);
    assert!(stand.meldung.contains(&zweiter), "{}", stand.meldung);
    assert!(seite(app).await.contains(&format!("gebaut {zweiter}")), "ohne Neustart aus dem neuen Ordner");
    assert!(ort.repo.join("web/build/index.html").is_file(), "web\\build bleibt");
}

/// Kann der Bote nicht in `<protokoll>\bote` schreiben (hier: `protokoll` ist
/// eine Datei), schreibt er nach `%LOCALAPPDATA%\Dashboard\protokoll` und sagt
/// das in der ersten Zeile.
#[tokio::test(flavor = "multi_thread")]
async fn protokoll_im_ersatzordner() {
    let ort = Ort::mit("ersatz", |o| {
        o.protokoll = o.ordner.join("protokoll-ist-eine-datei");
        std::fs::write(&o.protokoll, "keine Ordner hier").unwrap();
    });
    let mut kind = Kind(ort.starten_mit(&[], &[(TEST_EXIT, "0")]));
    assert_eq!(ende_abwarten(&mut kind, Duration::from_secs(15)).await, Some(0));
    let ersatz = Ort::tagesdateien(&ort.lokal().join("Dashboard").join("protokoll"));
    let erste = ersatz.lines().next().unwrap_or_default();
    assert!(erste.contains("Protokoll des Boten hier statt in"), "{ersatz}");
    assert!(erste.contains("protokoll-ist-eine-datei"), "{erste}");
    assert!(ersatz.contains("Bote startet"), "{ersatz}");
}

/// Der Wächter startet einen abgestürzten Boten neu (erst nach 5 s) und
/// schreibt das ins Protokoll; endet der Bote mit 0, endet auch der Wächter.
#[tokio::test(flavor = "multi_thread")]
async fn waechter_startet_neu() {
    let ort = Ort::neu("waechter");
    let mut waechter = Kind(ort.starten_mit(&["--waechter"], &[(TEST_EXIT, "9")]));
    let start = Instant::now();
    loop {
        let p = ort.protokoll();
        let abstuerze = p.matches("Bote endet absichtlich mit Code 9").count();
        if abstuerze >= 2 && p.contains("2. Absturz in der letzten Stunde); Neustart in 30 s.") {
            assert!(p.contains("Wächter: Der Bote ist mit Code 9 beendet (1. Absturz in der letzten Stunde); Neustart in 5 s."), "{p}");
            assert!(p.contains("Wächter: Neustart des Boten."), "{p}");
            assert!(start.elapsed() >= Duration::from_secs(5), "erst nach 5 s: {:?}", start.elapsed());
            break;
        }
        if let Ok(Some(s)) = waechter.0.try_wait() {
            panic!("Wächter vorzeitig beendet ({s}): {p}");
        }
        assert!(start.elapsed() < Duration::from_secs(25), "kein Neustart: {p}");
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    drop(waechter);

    let ort = Ort::neu("waechter-ende");
    let mut waechter = Kind(ort.starten_mit(&["--waechter"], &[(TEST_EXIT, "0")]));
    assert_eq!(ende_abwarten(&mut waechter, Duration::from_secs(15)).await, Some(0));
    assert!(ort.protokoll().contains("Wächter: Der Bote ist regulär beendet (Code 0); der Wächter endet auch."), "{}", ort.protokoll());

    // Code 2 (Einstellungen falsch): kein Neustart, der Wächter endet mit 2.
    let ort = Ort::neu("waechter-zwei");
    let mut waechter = Kind(ort.starten_mit(&["--waechter"], &[(TEST_EXIT, "2")]));
    assert_eq!(ende_abwarten(&mut waechter, Duration::from_secs(4)).await, Some(2), "ohne 5 s Pause beendet");
    let p = ort.protokoll();
    assert!(p.contains("Wächter: Einstellungen fehlerhaft, kein Neustart"), "{p}");
    assert_eq!(p.matches("Bote endet absichtlich mit Code 2").count(), 1, "nur ein Start: {p}");
    assert!(!p.contains("Neustart in"), "{p}");
}

#[test]
fn einmal_ohne_pipe() {
    let lauf = |json: &str| {
        let out = programme::eigener_befehl(BOTE).arg("--einmal").arg(json).stdin(Stdio::null()).output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        (out.status.code(), serde_json::from_str::<Antwort>(text.trim()).unwrap_or_else(|e| panic!("{e}: {text}")))
    };
    let (code, a) = lauf(r#"{"art":"tresor_info","daten":{"ziel":"ranke/test-gibts-nicht"}}"#);
    assert_eq!(code, Some(0));
    assert!(matches!(a, Antwort::TresorInfo { vorhanden: false, .. }), "{a:?}");
    let (code, a) = lauf(r#"{"art":"hallo"}"#);
    assert_eq!(code, Some(0));
    assert!(matches!(a, Antwort::Hallo { ref konto_sid, .. } if konto_sid.starts_with("S-1-5-")), "{a:?}");
    let (code, a) = lauf("kein json");
    assert_eq!(code, Some(2));
    assert!(matches!(a, Antwort::Fehler { .. }));
}

// ---------------------------------------------------------------------------
// Wächter und Job-Objekt (Nachbesserung 7a2): Der Bote stirbt mit dem
// Wächter; verwaiste Boten räumt der nächste Wächter weg; hält ein lebender
// Bote die Pipe, gibt der Wächter nach einem weiteren Versuch auf (Code 4).
// ---------------------------------------------------------------------------

mod prozess {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
    };

    /// Prozess- oder Job-Handle; schließt beim Freigeben.
    pub struct Handle(pub HANDLE);

    impl Handle {
        pub fn oeffnen(pid: u32) -> Option<Handle> {
            // SAFETY: nur Abfragen, Warten, Beenden; kein Erben.
            let h = unsafe { OpenProcess(PROCESS_SYNCHRONIZE | PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
            (!h.is_null()).then_some(Handle(h))
        }

        /// true, wenn der Prozess binnen `ms` endet (oder schon beendet ist).
        pub fn endet_binnen(&self, ms: u32) -> bool {
            // SAFETY: gültiges Handle mit SYNCHRONIZE.
            unsafe { WaitForSingleObject(self.0, ms) == WAIT_OBJECT_0 }
        }

        pub fn laeuft(&self) -> bool {
            // SAFETY: wie oben, ohne Warten.
            unsafe { WaitForSingleObject(self.0, 0) == WAIT_TIMEOUT }
        }

        pub fn beenden(&self) {
            // SAFETY: gültiges Handle mit PROCESS_TERMINATE.
            unsafe { TerminateProcess(self.0, 1) };
        }
    }

    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: von OpenProcess, CreateProcessW oder CreateJobObjectW.
            unsafe { CloseHandle(self.0) };
        }
    }

    /// Beendet den Prozess beim Freigeben (auch wenn eine Prüfung scheitert).
    pub struct Wegraeumen(pub Handle);

    impl Drop for Wegraeumen {
        fn drop(&mut self) {
            if self.0.laeuft() {
                self.0.beenden();
                self.0.endet_binnen(5000);
            }
        }
    }
}

/// Prozessnummer des Boten, der die Pipe `name` hält (wartet bis 15 s darauf).
async fn pipe_halter(name: &str) -> u32 {
    let start = Instant::now();
    loop {
        if let Ok(r) = dashboard::bote::win::pipe_oeffnen(name, 200) {
            return dashboard::bote::win::server_pid(&r).unwrap();
        }
        assert!(start.elapsed() < Duration::from_secs(15), "niemand hält die Pipe {name}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Wartet, bis im Protokoll `muster` steht; gibt das Protokoll zurück.
async fn protokoll_mit(ort: &Ort, muster: &str, hoechstens: Duration) -> String {
    let start = Instant::now();
    loop {
        let p = ort.protokoll();
        if p.contains(muster) {
            return p;
        }
        assert!(start.elapsed() < hoechstens, "„{muster}“ fehlt im Protokoll:\n{p}");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// (a) Der Wächter steckt schon in einem Job-Objekt des Tests (ohne
/// KILL_ON_JOB_CLOSE, wie bei der Aufgabenplanung); sein eigenes wird darin
/// verschachtelt. Wird er hart beendet, ist sein Bote binnen 3 s auch weg.
#[tokio::test(flavor = "multi_thread")]
async fn bote_stirbt_mit_dem_waechter() {
    use windows_sys::Win32::System::JobObjects::{AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob};
    use windows_sys::Win32::System::Threading::{CREATE_SUSPENDED, CreateProcessW, PROCESS_INFORMATION, ResumeThread, STARTUPINFOW};

    let ort = Ort::neu("job");
    // SAFETY: ohne Sicherheitsattribute und Namen; ohne Grenzen (kein KILL-Flag).
    let test_job = prozess::Handle(unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) });
    assert!(!test_job.0.is_null(), "{}", std::io::Error::last_os_error());

    // Der Wächter muss schon vor seinem ersten Befehl im Job stecken: darum
    // angehalten starten, zuweisen, fortsetzen. std::process kann das nicht
    // (kein Thread-Handle), also CreateProcessW direkt, ebenfalls ohne Fenster.
    let zeile = format!("\"{BOTE}\" --waechter --einstellungen \"{}\"", ort.datei().display());
    let mut zeile: Vec<u16> = zeile.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: Nullen sind gültige Werte für beide Strukturen.
    let mut si: STARTUPINFOW = unsafe { std::mem::zeroed() };
    si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    let mut pi: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: beschreibbare, nullterminierte Befehlszeile; Umgebung und Ordner vom Test.
    let ok = unsafe {
        CreateProcessW(
            std::ptr::null(),
            zeile.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            CREATE_SUSPENDED | programme::CREATE_NO_WINDOW,
            std::ptr::null(),
            std::ptr::null(),
            &si,
            &mut pi,
        )
    };
    assert!(ok != 0, "Wächter startet nicht: {}", std::io::Error::last_os_error());
    let waechter = prozess::Wegraeumen(prozess::Handle(pi.hProcess));
    let faden = prozess::Handle(pi.hThread);
    // SAFETY: gültige Handles.
    assert!(unsafe { AssignProcessToJobObject(test_job.0, waechter.0.0) } != 0, "{}", std::io::Error::last_os_error());
    // SAFETY: angehaltener Hauptfaden des Wächters.
    assert_ne!(unsafe { ResumeThread(faden.0) }, u32::MAX);
    drop(faden);

    let p = protokoll_mit(&ort, "Wächter: Bote gestartet (Prozess ", Duration::from_secs(15)).await;
    assert!(p.contains("Wächter: läuft schon in einem Job-Objekt"), "{p}");
    assert!(!p.contains("Job-Objekt lässt sich nicht"), "{p}");
    assert!(!p.contains("ins Job-Objekt legen"), "{p}");
    let kind_pid = pipe_halter(&ort.pipe).await;
    let kind = prozess::Wegraeumen(prozess::Handle::oeffnen(kind_pid).expect("Bote zu öffnen"));
    let mut drin = 0;
    // SAFETY: gültige Handles.
    assert!(unsafe { IsProcessInJob(waechter.0.0, test_job.0, &mut drin) } != 0 && drin != 0, "Wächter im Test-Job");
    // Der Bote steckt im Job des Wächters, der im Test-Job verschachtelt ist.
    assert!(unsafe { IsProcessInJob(kind.0.0, test_job.0, &mut drin) } != 0 && drin != 0, "Bote im Test-Job");
    assert!(p.contains(&format!("Wächter: Bote gestartet (Prozess {kind_pid}).")), "{p}");

    // Hart beenden wie Stop-ScheduledTask: kein Aufräumen im Wächter.
    waechter.0.beenden();
    assert!(waechter.0.endet_binnen(5000));
    assert!(kind.0.endet_binnen(3000), "Bote {kind_pid} lebt 3 s nach dem Wächter noch");
    assert!(dashboard::bote::win::pipe_oeffnen(&ort.pipe, 0).is_err(), "Pipe ist frei");
}

/// (b) Ein verwaister Bote (Elternprozess beendet, ohne Wächter) hält die
/// Pipe; ein neuer Wächter beendet ihn, und sein Bote läuft binnen 10 s.
#[tokio::test(flavor = "multi_thread")]
async fn waechter_raeumt_verwaisten_boten_weg() {
    use std::os::windows::process::CommandExt;

    let ort = Ort::neu("verwaist");
    // cmd startet den Boten mit `start /b` und endet sofort: Der Bote hat dann
    // keinen lebenden Elternprozess mehr.
    let status = programme::befehl(Werkzeug::Cmd)
        .unwrap()
        .raw_arg(format!("/c start \"\" /b \"{BOTE}\" --einstellungen \"{}\"", ort.datei().display()))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    let alt_pid = pipe_halter(&ort.pipe).await;
    let alt = prozess::Wegraeumen(prozess::Handle::oeffnen(alt_pid).expect("verwaister Bote zu öffnen"));

    let start = Instant::now();
    let mut waechter = Kind(ort.starten_mit(&["--waechter"], &[]));
    let p = protokoll_mit(&ort, &format!("Wächter: verwaisten Boten {alt_pid} beendet"), Duration::from_secs(10)).await;
    assert!(alt.0.endet_binnen(3000), "verwaister Bote lebt noch");
    assert!(p.contains("Pipe belegt; verwaisten Boten beendet"), "{p}");
    let neu_pid = loop {
        if let Ok(r) = dashboard::bote::win::pipe_oeffnen(&ort.pipe, 200) {
            break dashboard::bote::win::server_pid(&r).unwrap();
        }
        if let Ok(Some(s)) = waechter.0.try_wait() {
            panic!("Wächter beendet ({s}): {}", ort.protokoll());
        }
        assert!(start.elapsed() < Duration::from_secs(10), "neuer Bote nicht binnen 10 s: {}", ort.protokoll());
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_ne!(neu_pid, alt_pid);
    let neu = prozess::Handle::oeffnen(neu_pid).unwrap();
    drop(waechter);
    assert!(neu.endet_binnen(3000), "Bote endet mit dem Wächter");
}

/// (c) Ein lebender Bote (Elternprozess: dieser Test) hält die Pipe: kein
/// verwaister, also genau ein weiterer Versuch, dann endet der Wächter mit 4.
/// Der andere Bote bleibt unberührt.
#[tokio::test(flavor = "multi_thread")]
async fn waechter_gibt_bei_fremd_belegter_pipe_auf() {
    let ort = Ort::neu("belegt");
    let mut halter = Kind(ort.starten());
    let bote = Bote::neu(&ort.einstellungen(BOTE, &ort.sid));
    warten_bis_bereit(&bote, &mut halter).await;

    let mut waechter = Kind(ort.starten_mit(&["--waechter"], &[]));
    assert_eq!(ende_abwarten(&mut waechter, Duration::from_secs(20)).await, Some(4), "{}", ort.protokoll());
    let p = ort.protokoll();
    assert!(p.contains("Wächter: Die Pipe ist von einem anderen Boten belegt; dieser Wächter beendet sich"), "{p}");
    assert!(p.contains("kein verwaister Bote gefunden"), "{p}");
    assert_eq!(p.matches("Wächter: Bote gestartet (Prozess ").count(), 2, "genau zwei Starts: {p}");
    assert!(!p.contains("verwaisten Boten"), "{p}");
    assert!(halter.0.try_wait().unwrap().is_none(), "der lebende Bote läuft weiter");
    assert!(matches!(bote.anfragen(&Anfrage::Hallo).await, Ok(Antwort::Hallo { .. })));
}
