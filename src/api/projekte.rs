//! `/api/projects`, `/api/projects/<slug>` und die Dateien aus
//! `projects/<slug>/assets/`. Die Seiten kommen aus dem ContentStore
//! (Etappe 2d, `state.inhalt`), der Git-Stand aus dem Git-Zwischenspeicher;
//! keine der beiden Listen liest im Anfragepfad von der Platte.

use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use tracing::error;

use super::vertrag::{ArchivListe, ArchivProjekt, CheckResult, ProcResult, ProjectDetail, ProjectList, ProjectSummary};
use crate::anmeldung::Angemeldet;
use crate::app::AppState;
use crate::fehler::AppError;
use crate::git::RepoStatus;
use crate::inhalt::assets;
use crate::inhalt::seite::Broken;
use crate::inhalt::seite::{self, Project};

/// Lebenszeichen und Git-Stand eines Projekts einsammeln. Den Git-Stand gibt
/// der Zwischenspeicher (`state.git`), ohne git zu starten.
pub async fn extras(state: &AppState, p: &Project) -> (Vec<RepoStatus>, Vec<CheckResult>, Vec<ProcResult>) {
    // Die Lebenszeichen laufen gleichzeitig, jedes mit eigenem Budget von 3 s;
    // ein totes Ziel hält die Seite nicht auf.
    let pruefungen = p.meta.checks.iter().map(|c| async {
        let r = match tokio::time::timeout(crate::git::BUDGET, crate::pc::system::http_check(&c.url)).await {
            Ok(r) => r,
            Err(_) => Err("keine Antwort (Zeitlimit)".to_string()),
        };
        CheckResult {
            name: c.name.clone(),
            url: crate::datenschutz::verbergen(&c.url),
            ok: matches!(r, Ok(code) if code < 400),
            status: r.as_ref().ok().copied(),
            error: r.err().map(|e| crate::datenschutz::verbergen(&e)),
        }
    });
    let repos = repos_frei(state, p);
    let checks = futures_util::future::join_all(pruefungen).await;
    let snap = state.snapshot.read().await;
    let processes = p.meta.processes.iter().map(|n| ProcResult { name: crate::datenschutz::verbergen(n), running: snap.process_running(n) }).collect();
    (repos, checks, processes)
}

/// Der Git-Stand eines Projekts für die Antwort: Pfad und Fehlertext ohne
/// Profil und Benutzernamen (`datenschutz`; `/api/projects` ist frei).
fn repos_frei(state: &AppState, p: &Project) -> Vec<RepoStatus> {
    state.git.repos_fuer(p).into_iter().map(crate::datenschutz::repo_verbergen).collect()
}

/// Kopfleiste, Fassung 3: der Schwerpunkt je Projekt für Liste und Einzelseite
/// (ohne lesbare Datei alle unter „Sonstiges“, wie in „Anstehend“).
fn schwerpunkte(bestand: &crate::inhalt::store::Bestand) -> crate::vertrag::schwerpunkte::Schwerpunkte {
    match &bestand.schwerpunkte {
        Ok(Some(s)) => s.clone(),
        _ => crate::vertrag::schwerpunkte::Schwerpunkte::default(),
    }
}

/// `GET /api/projects`
///
/// Etappe B2: Ohne Windows-Anmeldung stehen die offenen Punkte nur als Zahl
/// (`open_zahl`) da, und kaputte Seiten nur mit einem neutralen Satz (der
/// Fehlersatz kann Inhalt zitieren; er steht in der Prüfung, mit Anmeldung).
pub async fn liste(State(state): State<AppState>, angemeldet: Option<Extension<Angemeldet>>) -> Json<ProjectList> {
    let angemeldet = angemeldet.is_some();
    let bestand = state.inhalt.bestand();
    // Entscheidung archiv-status: archivierte Projekte fehlen hier
    // (Seitenleiste, Schublade, Startseite); sie stehen nur in `/api/archiv`.
    let (list, mut broken) = bestand.liste_sichtbar();
    // Der Satz einer kaputten Seite kann einen Pfad nennen: ohne Profil und Namen.
    for b in &mut broken {
        b.error = crate::datenschutz::verbergen(&b.error);
    }
    let schwerpunkte = schwerpunkte(&bestand);
    // Etappe 4c: Bau-Stand der Projekte mit eigener Seite (frei, ohne Satz).
    let s = state.clone();
    let bau = tokio::task::spawn_blocking(move || crate::baustand::alle(&s)).await.unwrap_or_default();
    // Git-Stand nur aus dem Zwischenspeicher: kein git im Anfragepfad.
    let projects = list
        .iter()
        .map(|p| {
            ProjectSummary::from_project(p, repos_frei(&state, p))
                .mit_schwerpunkt(schwerpunkte.von(&p.slug))
                .mit_bau(bau.get(&p.slug).map(crate::baustand::Eintrag::frei))
                .ohne_texte(angemeldet)
        })
        .collect();
    let broken = if angemeldet { broken } else { broken.into_iter().map(|b| Broken { error: super::frei::SATZ_SEITE_KAPUTT.to_string(), ..b }).collect() };
    // Entscheidung projektgruppe: die Gruppen mit ihren sichtbaren Teilen.
    let gruppen = bestand.gruppen_sichtbar();
    Json(ProjectList { projects, broken, gruppen })
}

/// `GET /api/archiv` (Entscheidung archiv-status): die
/// archivierten Projekte mit Titel und „seit“, nach Titel. Frei wie
/// `/api/projects`. Ein archiviertes Projekt mit kaputter Hauptseite steht
/// mit dem Titel aus `projekt.toml` da.
pub async fn archiv(State(state): State<AppState>) -> Json<ArchivListe> {
    Json(archiv_liste(&state.inhalt.bestand()))
}

pub fn archiv_liste(bestand: &crate::inhalt::store::Bestand) -> ArchivListe {
    let tag = |z: Option<chrono::DateTime<chrono::Local>>| z.map(|z| z.format("%Y-%m-%d").to_string());
    let mut projekte: Vec<ArchivProjekt> = bestand
        .archivierte()
        .into_iter()
        .map(|slug| {
            let kopf = bestand.kopf(&slug).and_then(Result::ok);
            let haupt = bestand.haupt(&slug).ok().flatten();
            let titel = haupt.map(|h| h.meta.title.clone()).or_else(|| kopf.as_ref().map(|(k, _)| k.title.clone())).unwrap_or_else(|| slug.clone());
            // `archiviert_seit` aus projekt.toml; sonst das Datum der Datei mit dem
            // Status (projekt.toml), ohne lesbaren Kopf das des Projekts.
            let seit = match &kopf {
                Some((k, _)) if k.archiviert_seit.is_some() => k.archiviert_seit.clone(),
                Some((_, zeit)) => tag(*zeit),
                None => tag(haupt.and_then(|h| h.file_modified)),
            };
            ArchivProjekt { slug, titel, seit }
        })
        .collect();
    projekte.sort_by(|a, b| a.titel.cmp(&b.titel).then_with(|| a.slug.cmp(&b.slug)));
    ArchivListe { projekte }
}

/// `GET /api/projects/<slug>`: eine Seite mit allem, was sie zeigt.
///
/// Etappe B2: Die Fragen an den Inhaber (`daten.fragen`) sind persönlicher Inhalt
/// und stehen nur mit Anmeldung darin; ohne fehlt das Feld (`null`). Die Fragen
/// selbst kommen über den geschützten Weg `/api/projekte/<slug>/daten/fragen`.
pub async fn eine(State(state): State<AppState>, Path(slug): Path<String>, angemeldet: Option<Extension<Angemeldet>>) -> Result<Json<ProjectDetail>, AppError> {
    let bestand = state.inhalt.bestand();
    let p = match bestand.haupt(&slug) {
        Ok(Some(p)) => p.clone(),
        Ok(None) => return Err(AppError::nicht_gefunden(format!("Kein Projekt „{slug}“"))),
        Err(e) => {
            error!("API Seite {slug}: {e}");
            // Etappe B2: Der Satz kann Inhalt zitieren; ohne Anmeldung ein neutraler.
            return Err(AppError::unlesbar(if angemeldet.is_some() { e } else { super::frei::SATZ_SEITE_KAPUTT.to_string() }));
        }
    };
    let schwerpunkt = schwerpunkte(&bestand).von(&p.slug);
    drop(bestand);
    let (repos, checks, processes) = extras(&state, &p).await;
    Ok(Json(ProjectDetail {
        head: ProjectSummary::from_project(&p, repos).mit_schwerpunkt(schwerpunkt).ohne_texte(angemeldet.is_some()),
        // Lokale Pfade sind als Link erlaubt (die Oberfläche zeigt sie ohne Link); ohne Profil.
        links: p.meta.links.iter().map(|l| crate::vertrag::projekt::Link { name: l.name.clone(), url: crate::datenschutz::verbergen(&l.url) }).collect(),
        checks,
        processes,
        seiten: p.seiten.clone(),
        daten: p.daten.clone().map(|mut d| {
            if angemeldet.is_none() {
                // Fragen (B2), Fristtitel und Ereignisnamen (B3) sind persönlicher Inhalt;
                // dieselbe Liste wie `sicherheit::geschuetzt` (Hotfix Prozent).
                for name in crate::sicherheit::GESCHUETZTE_DATEIEN {
                    d.nimm(name);
                }
            }
            d
        }),
    }))
}

/// Für HTML-Dateien in `assets/`: eigene eingebettete Skripte ja, aber nichts aus dem Netz,
/// keine Formulare. Kein frame-ancestors: der sandboxed Rahmen der Vorschau hat eine
/// undurchsichtige Herkunft, und der Server ist ohnehin nur auf 127.0.0.1 erreichbar.
pub const ASSET_HTML_CSP: &str = "default-src 'none'; img-src 'self' data:; style-src 'unsafe-inline'; script-src 'unsafe-inline'; font-src 'self' data:; connect-src 'none'; form-action 'none'; base-uri 'none'; sandbox allow-scripts";

/// Für SVG aus `assets/`: keine Skripte, nichts nachladen außer `data:`-Bildern,
/// eigene Inline-Stile ja. Hängt nicht an der SVG-Prüfung (die sich umgehen lässt)
/// und nicht an der CSP der Oberfläche; `sandbox` gibt dem SVG eine eigene,
/// undurchsichtige Herkunft, wenn es direkt geöffnet wird.
pub const ASSET_SVG_CSP: &str =
    "default-src 'none'; style-src 'unsafe-inline'; img-src data:; font-src 'none'; connect-src 'none'; form-action 'none'; base-uri 'none'; sandbox";

/// Für alle anderen Dateien aus `assets/` (Bilder, Text, JSON …).
pub const ASSET_SONST_CSP: &str = "default-src 'none'; sandbox";

/// Für PDF: ohne `sandbox`, sonst zeigt der eingebaute PDF-Betrachter von
/// Chrome und Edge nichts an.
pub const ASSET_PDF_CSP: &str = "default-src 'none'";

/// Content-Type für Dateien aus `assets/`.
pub fn asset_mime(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        Some("pdf") => "application/pdf",
        Some("json") => "application/json",
        // Eigene HTML-Dateien der Chats (z. B. eine Vorschau), keine Inhalte aus dem Netz.
        Some("html") | Some("htm") => "text/html; charset=utf-8",
        Some("txt") => "text/plain; charset=utf-8",
        Some("csv") => "text/csv; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Nur einfache Dateinamen, keine Unterordner, damit niemand aus dem Ordner
/// ausbricht; keine Windows-Gerätenamen (`con`, `nul.txt` …).
pub fn is_safe_asset_name(file: &str) -> bool {
    !file.is_empty()
        && file.len() <= 128
        && file.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        && !file.starts_with('.')
        && !assets::ist_geraetename(file)
}

/// Liest eine Datei aus `<projects>/<slug>/assets/`, aber nur, wenn weder der
/// Projektordner noch `assets` noch die Datei eine Verknüpfung ist und der
/// aufgelöste Pfad direkt im aufgelösten Ordner `assets` liegt. Höchstens
/// [`assets::groesse_grenze`] Bytes, sonst 413.
fn asset_lesen(projects: &std::path::Path, slug: &str, file: &str) -> Result<Vec<u8>, AppError> {
    use std::io::Read;
    let fehlt = || AppError::nicht_gefunden(format!("Nicht gefunden: {slug}/assets/{file}"));
    let projekt = projects.join(slug);
    let ordner = projekt.join("assets");
    let pfad = ordner.join(file);
    if [&projekt, &ordner, &pfad].into_iter().any(|p| seite::ist_verknuepfung(p)) {
        tracing::warn!("{slug}/assets/{file}: Verknüpfung, nicht ausgeliefert");
        return Err(AppError::nicht_gefunden(format!(
            "Nicht gefunden: {slug}/assets/{file}. Das Dashboard liefert nur echte Dateien aus dem Ordner assets aus, keine Verknüpfungen."
        )));
    }
    let echt = std::fs::canonicalize(&pfad).map_err(|_| fehlt())?;
    let echt_ordner = std::fs::canonicalize(projects).map_err(|_| fehlt())?.join(slug).join("assets");
    if !seite::liegt_direkt_in(&echt, &echt_ordner) {
        tracing::warn!("{slug}/assets/{file}: liegt aufgelöst nicht in assets, nicht ausgeliefert");
        return Err(fehlt());
    }
    let datei = std::fs::File::open(&echt).map_err(|_| fehlt())?;
    let meta = datei.metadata().map_err(|_| fehlt())?;
    if !meta.is_file() {
        return Err(fehlt());
    }
    let endung = file.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
    let grenze = assets::groesse_grenze(endung);
    let zu_gross = || AppError::zu_gross(format!("Die Datei ist größer als erlaubt (höchstens {}).", assets::grenze_wort(grenze)));
    if meta.len() > grenze {
        return Err(zu_gross());
    }
    // Auch wenn die Datei zwischendurch wächst: nie mehr als die Grenze lesen.
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    datei.take(grenze + 1).read_to_end(&mut bytes).map_err(|_| fehlt())?;
    if bytes.len() as u64 > grenze {
        return Err(zu_gross());
    }
    Ok(bytes)
}

/// Bilder und andere Dateien aus `projects/<slug>/assets/`, jede mit eigener
/// CSP: HTML mit [`ASSET_HTML_CSP`], SVG mit [`ASSET_SVG_CSP`],
/// PDF mit [`ASSET_PDF_CSP`], alles andere mit [`ASSET_SONST_CSP`]. `nosniff`
/// setzt die Schleuse für Kopfzeilen.
pub async fn asset(State(state): State<AppState>, Path((slug, file)): Path<(String, String)>) -> Result<Response, AppError> {
    if !seite::is_valid_slug(&slug) || !is_safe_asset_name(&file) {
        return Err(AppError::nicht_gefunden(format!("Nicht gefunden: {slug}/assets/{file}")));
    }
    let mime = asset_mime(std::path::Path::new(&file));
    // Etappe 4b: bei mehreren Inhaltsquellen aus der Quelle des Projekts.
    let projects = state.projekt_eltern(&slug);
    let (s, f) = (slug.clone(), file.clone());
    let bytes = tokio::task::spawn_blocking(move || asset_lesen(&projects, &s, &f)).await.map_err(|e| {
        error!("Asset {slug}/{file}: {e}");
        AppError::intern("Die Datei ließ sich nicht lesen. Der Fehler steht im Protokoll.")
    })??;
    let csp = if mime.starts_with("text/html") {
        ASSET_HTML_CSP
    } else if mime == "image/svg+xml" {
        ASSET_SVG_CSP
    } else if mime == "application/pdf" {
        ASSET_PDF_CSP
    } else {
        ASSET_SONST_CSP
    };
    Ok(([(header::CONTENT_TYPE, mime), (header::CONTENT_SECURITY_POLICY, csp)], bytes).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_names() {
        assert!(is_safe_asset_name("bild.png"));
        assert!(is_safe_asset_name("Entwurf-2.html"));
        assert!(!is_safe_asset_name(".versteckt"));
        assert!(!is_safe_asset_name("unter/bild.png"));
        assert!(!is_safe_asset_name("..%2Fpage.md"));
        assert!(!is_safe_asset_name(""));
        // Windows-Gerätenamen, auch mit Endung und groß geschrieben.
        for name in ["con", "NUL", "nul.txt", "con.svg", "Aux.png", "com1.json", "LPT9.pdf", "prn"] {
            assert!(!is_safe_asset_name(name), "{name}");
        }
        assert!(is_safe_asset_name("console.svg"));
        assert!(is_safe_asset_name("com10.png"));
    }

    #[test]
    fn asset_arten() {
        let art = |n: &str| asset_mime(std::path::Path::new(n));
        assert_eq!(art("daten.json"), "application/json");
        assert_eq!(art("liste.CSV"), "text/csv; charset=utf-8");
        assert_eq!(art("notiz.txt"), "text/plain; charset=utf-8");
        assert_eq!(art("plan.svg"), "image/svg+xml");
        assert_eq!(art("unbekannt.xyz"), "application/octet-stream");
    }

    /// Kopfleiste, Fassung 3: Die Projektliste (frei) nennt je Projekt den
    /// Schwerpunkt mit `reihenfolge`, wie „Anstehend“; ohne Zuordnung
    /// „Sonstiges“ mit eins mehr als der letzte Schwerpunkt.
    #[tokio::test]
    async fn liste_nennt_den_schwerpunkt() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use http_body_util::BodyExt;
        use tower::ServiceExt;
        let state = AppState::fuer_tests(&crate::app::beispiel_fuer_tests());
        let req = Request::get("/api/projects").header("host", "127.0.0.1:8787").body(Body::empty()).unwrap();
        // Ohne Anmeldung: die Liste ist frei.
        let res = crate::app::router(state).oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let v: serde_json::Value = serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        let p = |slug: &str| v["projects"].as_array().unwrap().iter().find(|p| p["slug"] == slug).cloned().unwrap();
        assert_eq!((p("schaufenster")["schwerpunkt"].as_str(), p("schaufenster")["schwerpunkt_reihenfolge"].as_i64()), (Some("Chor und Verein"), Some(4)));
        assert_eq!((p("dashboard")["schwerpunkt"].as_str(), p("dashboard")["schwerpunkt_reihenfolge"].as_i64()), (Some("Bienen und Honig"), Some(5)));
        assert_eq!((p("geflecht")["schwerpunkt"].as_str(), p("geflecht")["schwerpunkt_reihenfolge"].as_i64()), (Some("Sonstiges"), Some(6)));
    }

    /// Die Einzelseite nennt denselben Schwerpunkt wie die Liste (vorher stand dort immer „Sonstiges“).
    #[tokio::test]
    async fn eine_nennt_den_schwerpunkt() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use http_body_util::BodyExt;
        use tower::ServiceExt;
        let state = AppState::fuer_tests(&crate::app::beispiel_fuer_tests());
        for (slug, name, reihenfolge) in [("schaufenster", "Chor und Verein", 4), ("dashboard", "Bienen und Honig", 5), ("geflecht", "Sonstiges", 6)] {
            let req = Request::get(format!("/api/projects/{slug}")).header("host", "127.0.0.1:8787").body(Body::empty()).unwrap();
            let res = crate::app::router(state.clone()).oneshot(req).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK, "{slug}");
            let v: serde_json::Value = serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
            assert_eq!((v["schwerpunkt"].as_str(), v["schwerpunkt_reihenfolge"].as_i64()), (Some(name), Some(reihenfolge)), "{slug}");
        }
    }
}
