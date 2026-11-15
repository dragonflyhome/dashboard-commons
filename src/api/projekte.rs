//! `/api/projects`, `/api/projects/<slug>` und die Dateien aus
//! `projects/<slug>/assets/`. Die Seiten kommen aus dem ContentStore
//! (Etappe 2d, `state.inhalt`), der Git-Stand aus dem Git-Zwischenspeicher;
//! keine der beiden Listen liest im Anfragepfad von der Platte.
//!
//! Seit Schritt E2 sind alle diese Wege geschützt (`sicherheit::oeffentlich`
//! nennt keinen); sie liefern im Klartext, ohne Schwärzen.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use tracing::error;

use super::vertrag::{ArchivListe, ArchivProjekt, CheckResult, ProcResult, ProjectDetail, ProjectList, ProjectSummary};
use crate::app::AppState;
use crate::fehler::AppError;
use crate::git::RepoStatus;
use crate::inhalt::assets;
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
        CheckResult { name: c.name.clone(), url: c.url.clone(), ok: matches!(r, Ok(code) if code < 400), status: r.as_ref().ok().copied(), error: r.err() }
    });
    let repos = state.git.repos_fuer(p);
    let checks = futures_util::future::join_all(pruefungen).await;
    let snap = state.snapshot.read().await;
    let processes = p.meta.processes.iter().map(|n| ProcResult { name: n.clone(), running: snap.process_running(n) }).collect();
    (repos, checks, processes)
}

/// Kopfleiste, Fassung 3: der Schwerpunkt je Projekt für Liste und Einzelseite
/// (ohne lesbare Datei alle unter „Sonstiges“, wie in „Anstehend“).
fn schwerpunkte(bestand: &crate::inhalt::store::Bestand) -> crate::vertrag::schwerpunkte::Schwerpunkte {
    match &bestand.schwerpunkte {
        Ok(Some(s)) => s.clone(),
        _ => crate::vertrag::schwerpunkte::Schwerpunkte::default(),
    }
}

/// `GET /api/projects` (geschützt): mit offenen Punkten und den Sätzen kaputter Seiten.
pub async fn liste(State(state): State<AppState>) -> Json<ProjectList> {
    let bestand = state.inhalt.bestand();
    // Entscheidung archiv-status: archivierte Projekte fehlen hier
    // (Seitenleiste, Schublade, Startseite); sie stehen nur in `/api/archiv`.
    let (list, broken) = bestand.liste_sichtbar();
    let schwerpunkte = schwerpunkte(&bestand);
    // Etappe 4c: Bau-Stand der Projekte mit eigener Seite (ohne Satz).
    let s = state.clone();
    let bau = tokio::task::spawn_blocking(move || crate::baustand::alle(&s)).await.unwrap_or_default();
    // Git-Stand nur aus dem Zwischenspeicher: kein git im Anfragepfad.
    let projects = list
        .iter()
        .map(|p| {
            ProjectSummary::from_project(p, state.git.repos_fuer(p))
                .mit_schwerpunkt(schwerpunkte.von(&p.slug))
                .mit_bau(bau.get(&p.slug).map(crate::baustand::Eintrag::frei))
        })
        .collect();
    // Entscheidung projektgruppe: die Gruppen mit ihren sichtbaren Teilen.
    let gruppen = bestand.gruppen_sichtbar();
    Json(ProjectList { projects, broken, gruppen })
}

/// `GET /api/archiv` (Entscheidung archiv-status): die
/// archivierten Projekte mit Titel und „seit“, nach Titel. Geschützt wie
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

/// `GET /api/projects/<slug>` (geschützt): eine Seite mit allem, was sie zeigt,
/// auch den Fragen und Fristen in `daten`.
pub async fn eine(State(state): State<AppState>, Path(slug): Path<String>) -> Result<Json<ProjectDetail>, AppError> {
    let bestand = state.inhalt.bestand();
    let p = match bestand.haupt(&slug) {
        Ok(Some(p)) => p.clone(),
        Ok(None) => return Err(AppError::nicht_gefunden(format!("Kein Projekt „{slug}“"))),
        Err(e) => {
            error!("API Seite {slug}: {e}");
            return Err(AppError::unlesbar(e));
        }
    };
    let schwerpunkt = schwerpunkte(&bestand).von(&p.slug);
    drop(bestand);
    let (repos, checks, processes) = extras(&state, &p).await;
    Ok(Json(ProjectDetail {
        head: ProjectSummary::from_project(&p, repos).mit_schwerpunkt(schwerpunkt),
        // Lokale Pfade sind als Link erlaubt (die Oberfläche zeigt sie ohne Link).
        links: p.meta.links.clone(),
        checks,
        processes,
        seiten: p.seiten.clone(),
        daten: p.daten.clone(),
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

/// Die Bilder in `assets/` (einfache Namen, Bildendung).
fn bildnamen(ordner: &std::path::Path) -> Vec<String> {
    let Ok(eintraege) = std::fs::read_dir(ordner) else { return Vec::new() };
    eintraege
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| is_safe_asset_name(n) && asset_mime(std::path::Path::new(n)).starts_with("image/"))
        .collect()
}

/// Höchstens so viele Bytes Bilder bettet [`bilder_einbetten`] in eine Antwort ein.
const EINBETTEN_GRENZE: u64 = 8 * 1024 * 1024;

/// Schritt E2: Ein Entwurf läuft mit undurchsichtiger Herkunft (Rahmen und
/// CSP `sandbox`); seine Bildanfragen sind `cross-site` und tragen darum kein
/// Passkey-Cookie (SameSite=Strict), bekämen also 401. Der Server bettet die
/// Bilder aus `assets/`, die der Entwurf in Anführungszeichen oder `url(…)`
/// nennt (auch mit `./`), als `data:` ein; die CSP erlaubt `img-src data:`.
/// Je Antwort höchstens `grenze` Bytes Bilder (ein mehrfach genanntes Bild
/// zählt je Nennung), in der Reihenfolge, in der der Entwurf sie zuerst nennt;
/// ab dem ersten Bild, das nicht mehr passt, bleiben die übrigen Adressen.
/// shortcut: srcset-Listen, Adressen mit `?…`, von Skripten gebaute Namen und Bilder über [`EINBETTEN_GRENZE`] (8 MiB Bild-Bytes vor base64 je Antwort) bleiben Adressen und zeigen im Rahmen kein Bild, seit Schritt E2b auch angemeldet (Anfragen aus dem Rahmen sind cross-site und bekommen 403 aus der Herkunftsprüfung), aufrüsten, wenn ein Entwurf sie braucht.
fn bilder_einbetten(html: Vec<u8>, namen: &[String], lesen: &dyn Fn(&str) -> Option<Vec<u8>>, grenze: u64) -> Vec<u8> {
    let mut text = match String::from_utf8(html) {
        Ok(t) => t,
        Err(e) => return e.into_bytes(),
    };
    const KLAMMERN: [(&str, &str); 3] = [("\"", "\""), ("'", "'"), ("(", ")")];
    let mut genannt: Vec<_> = namen
        .iter()
        .filter_map(|name| {
            let formen: Vec<(String, &str, &str)> =
                KLAMMERN.iter().flat_map(|&(auf, zu)| [(format!("{auf}{name}{zu}"), auf, zu), (format!("{auf}./{name}{zu}"), auf, zu)]).collect();
            let erste = formen.iter().filter_map(|(muster, ..)| text.find(muster.as_str())).min()?;
            Some((erste, name, formen))
        })
        .collect();
    genannt.sort_by_key(|(erste, ..)| *erste);
    let mut summe = 0u64;
    for (_, name, formen) in genannt {
        let Some(bytes) = lesen(name) else { continue };
        // Jede Nennung bekommt eine eigene Kopie, also zählt jede gegen die Grenze.
        let nennungen = formen.iter().map(|(muster, ..)| text.matches(muster.as_str()).count() as u64).sum::<u64>();
        summe += bytes.len() as u64 * nennungen;
        if summe > grenze {
            break;
        }
        let daten = format!("data:{};base64,{}", asset_mime(std::path::Path::new(name)), crate::anmeldung::base64::kodieren(&bytes));
        for (muster, auf, zu) in &formen {
            text = text.replace(muster.as_str(), &format!("{auf}{daten}{zu}"));
        }
    }
    text.into_bytes()
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
    let html = mime.starts_with("text/html");
    // Etappe 4b: bei mehreren Inhaltsquellen aus der Quelle des Projekts.
    let projects = state.projekt_eltern(&slug);
    let (s, f) = (slug.clone(), file.clone());
    let bytes = tokio::task::spawn_blocking(move || {
        let bytes = asset_lesen(&projects, &s, &f)?;
        if !html {
            return Ok(bytes);
        }
        let namen = bildnamen(&projects.join(&s).join("assets"));
        Ok(bilder_einbetten(bytes, &namen, &|name| asset_lesen(&projects, &s, name).ok(), EINBETTEN_GRENZE))
    })
    .await
    .map_err(|e| {
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

    /// Schritt E2: Bilder des Entwurfs werden in allen Schreibweisen der
    /// Anführung eingebettet; ein längerer Name, der den kürzeren enthält,
    /// bleibt unberührt; HTML, das kein UTF-8 ist, bleibt Byte für Byte gleich.
    #[test]
    fn bilder_einbetten_formen_und_grenzen() {
        let namen = ["a.png".to_string(), "plan.svg".to_string(), "weg.png".to_string()];
        let lesen = |n: &str| match n {
            "a.png" => Some(b"A".to_vec()),
            "plan.svg" => Some(b"<svg/>".to_vec()),
            _ => None,
        };
        let ein = |html: &str| String::from_utf8(bilder_einbetten(html.as_bytes().to_vec(), &namen, &lesen, EINBETTEN_GRENZE)).unwrap();
        let a = "data:image/png;base64,QQ==";
        assert_eq!(ein("<img src=\"a.png\">"), format!("<img src=\"{a}\">"));
        assert_eq!(ein("<img src='./a.png'>"), format!("<img src='{a}'>"));
        assert_eq!(ein("x{background:url(a.png)}"), format!("x{{background:url({a})}}"));
        assert_eq!(ein("<img src=\"plan.svg\">"), "<img src=\"data:image/svg+xml;base64,PHN2Zy8+\">");
        for bleibt in ["<img src=\"ba.png\">", "<img src=\"a.png?v=2\">", "Text a.png", "<img src=\"weg.png\">", "<img src=\"/a.png\">"] {
            assert_eq!(ein(bleibt), bleibt);
        }
        let kein_utf8 = vec![b'"', 0xff, b'"', b'a', b'.', b'p', b'n', b'g', b'"'];
        assert_eq!(bilder_einbetten(kein_utf8.clone(), &namen, &lesen, EINBETTEN_GRENZE), kein_utf8);
    }

    /// Abnahme E2 (H2): Eingebettet wird in der Reihenfolge, in der der
    /// Entwurf die Bilder nennt (nicht in der des Ordners), bis die Grenze
    /// erreicht ist; ab dem ersten Bild, das nicht passt, bleiben alle übrigen
    /// Adressen, auch kleinere danach.
    #[test]
    fn bilder_einbetten_bis_zur_grenze() {
        let namen = ["a.png".to_string(), "b.png".to_string(), "c.png".to_string()];
        let lesen = |n: &str| match n {
            "a.png" => Some(b"A".to_vec()),
            "b.png" => Some(b"BBB".to_vec()),
            "c.png" => Some(b"C".to_vec()),
            _ => None,
        };
        let ein = |html: &str, grenze| String::from_utf8(bilder_einbetten(html.as_bytes().to_vec(), &namen, &lesen, grenze)).unwrap();
        let (a, c) = ("\"data:image/png;base64,QQ==\"", "\"data:image/png;base64,Qw==\"");
        assert_eq!(ein("\"c.png\" \"a.png\" \"b.png\"", 2), format!("{c} {a} \"b.png\""), "c und a passen, b nicht");
        assert_eq!(ein("\"c.png\" \"a.png\" \"b.png\"", 1), format!("{c} \"a.png\" \"b.png\""), "nur das zuerst genannte");
        assert_eq!(ein("\"c.png\" \"b.png\" \"a.png\"", 2), format!("{c} \"b.png\" \"a.png\""), "nach b bleibt auch a");
        assert_eq!(ein("\"b.png\" \"c.png\"", 0), "\"b.png\" \"c.png\"");
        assert!(ein("\"b.png\" \"c.png\"", 5).matches("data:").count() == 2, "unter der Grenze alle");
        // Gegenprüfung E2 Runde 2: jede Nennung zählt, auch in anderer Form.
        let dreimal = "\"a.png\" './a.png' url(a.png)";
        assert_eq!(ein(dreimal, 2), dreimal, "drei Nennungen zu je 1 Byte passen nicht in 2");
        assert_eq!(ein(dreimal, 3).matches("data:").count(), 3, "genau an der Grenze");
        assert_eq!(ein("\"a.png\" \"a.png\" \"c.png\"", 2), format!("{a} {a} \"c.png\""), "a zählt doppelt, danach passt c nicht");
    }

    /// Kopfleiste, Fassung 3: Die Projektliste nennt je Projekt den
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
        let res = crate::app::router_angemeldet(state).oneshot(req).await.unwrap();
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
            let res = crate::app::router_angemeldet(state.clone()).oneshot(req).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK, "{slug}");
            let v: serde_json::Value = serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
            assert_eq!((v["schwerpunkt"].as_str(), v["schwerpunkt_reihenfolge"].as_i64()), (Some(name), Some(reihenfolge)), "{slug}");
        }
    }
}
