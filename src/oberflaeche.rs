//! Die Oberfläche: das SvelteKit-Build, im Dienst aus der Kopie `bin\web`
//! (Dienst.ps1 -Neustart), sonst aus `web/build`; nach einem Bau über den
//! Boten aus `web/builds/<ordner>` (siehe `app::WebOrt`). Ausgeliefert unter `/`.
//! Alles, was keine API-Route ist, landet hier (Fallback).

use axum::extract::State;
use axum::http::{StatusCode, Uri, header};
use axum::response::{Html, IntoResponse, Redirect, Response};

use crate::app::AppState;
use crate::fehler::AppError;
use crate::inhalt::store::Bestand;

/// Dateiendung → Content-Type für Dateien aus dem Web-Build.
pub fn mime_for(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("webmanifest") => "application/manifest+json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        Some("txt") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Die festen Seiten der Oberfläche (erster Teil der Adresse, `""` ist die
/// Übersicht). Muss zu `web/src/routes` passen; der Test
/// `jede_route_der_oberflaeche_ist_eine_seite` prüft das.
/// Seit Etappe 4d dazu die System-Seiten anleitung, zielbild und entscheidungen
/// (ihre Unteradressen sind 404), seit der Freigabe in einem Schritt fassungen.
pub const SEITEN: &[&str] =
    &["", "anstehend", "pc", "schluessel", "hallo", "vorgaenge", "daten", "anmelden", "katalog", "anleitung", "zielbild", "entscheidungen", "fassungen"];

/// Die Unterseiten von `/katalog` (Adresse nach `katalog/`). Etappe 3b-2
/// (Auflage aus der Abnahme 3b): Vorher galt unter `/katalog/…` jede Adresse
/// als Seite (200); jetzt nur diese, alles andere ist 404 wie anderswo. Eine
/// neue Katalogseite kommt hier dazu; `jede_route_der_oberflaeche_ist_eine_seite`
/// meldet eine fehlende.
pub const KATALOG_SEITEN: &[&str] = &["entwuerfe/logo"];

/// Ist `rel` (Adresse ohne `/` vorn und hinten) eine Seite der Oberfläche?
/// Projektseiten nur, wenn es das Projekt (`projekt.toml`) gibt; die
/// Unterseiten prüft [`ist_unterseite`], `/neu` leitet nur weiter
/// ([`weiter_zur_hauptseite`]). Unter `katalog` nur die Seiten aus
/// [`KATALOG_SEITEN`]. Gruppenseiten `/gruppen/<slug>` (Entscheidung
/// projektgruppe) nur für eine Gruppe aus `data/gruppen.toml` mit
/// mindestens einem sichtbaren Teil.
pub fn ist_seite(rel: &str, b: &Bestand) -> bool {
    let teile: Vec<&str> = if rel.is_empty() { vec![""] } else { rel.split('/').collect() };
    match teile.as_slice() {
        [erstes] => SEITEN.contains(erstes),
        ["katalog", rest @ ..] => KATALOG_SEITEN.contains(&rest.join("/").as_str()),
        // Entscheidung archiv-status: die Archivansicht (nur diese eine Adresse).
        ["projekte", "archiviert"] => true,
        ["gruppen", slug] => b.gruppen_sichtbar().iter().any(|g| g.slug == *slug),
        ["projects", slug] | ["projects", slug, "vorschau"] => projekt_da(b, slug),
        _ => false,
    }
}

/// Dateien aus `web/build`; alles Unbekannte bekommt index.html (Einzelseiten-App,
/// die Routen entscheidet der Browser). Fehlende Build-Dateien unter `_app/` sind
/// ein echter 404, damit ein veralteter Browser-Tab nicht HTML statt JavaScript bekommt.
///
/// Etappe 3b (Fund des Prüfers): Eine Adresse, die keine Seite ist
/// (`ist_seite`), bekommt dieselbe index.html, aber mit 404. Der Browser
/// zeigt dann die Fehlerseite der Oberfläche (deutsch, mit Link zur
/// Übersicht), und wer nur den Code liest, sieht 404 statt 200.
pub async fn web(State(state): State<AppState>, uri: Uri) -> Response {
    // Unbekannte API-Adressen sind ein echter 404 als JSON, keine Seite.
    if uri.path().starts_with("/api/") {
        return AppError::nicht_gefunden(format!("Diese Adresse gibt es nicht: {}", uri.path())).into_response();
    }
    let Some(rel) = sicherer_teilpfad(uri.path()) else {
        return nicht_gefunden(uri.path());
    };
    // Der Ordner kann sich nach einem Bau ändern (`app::WebOrt`); je Anfrage einmal fragen.
    let ordner = state.web.ordner();
    let index = ordner.join("index.html");
    let candidate = if rel.as_os_str().is_empty() { index.clone() } else { ordner.join(&rel) };
    let (file, status) = if candidate.is_file() && liegt_unter(&candidate, &ordner) {
        (candidate, StatusCode::OK)
    } else if rel.starts_with("_app") {
        return nicht_gefunden(uri.path());
    } else {
        let r = rel.to_str().map(|r| r.replace('\\', "/"));
        let b = state.inhalt.bestand();
        // Die alten Adressen unter /projects/<slug>/neu leiten dauerhaft weiter.
        if let Some(ziel) = r.as_deref().and_then(|r| weiter_zur_hauptseite(r, &b)) {
            let ziel = match uri.query() {
                Some(q) => format!("{ziel}?{q}"),
                None => ziel,
            };
            return Redirect::temporary(&ziel).into_response();
        }
        let seite = r.is_some_and(|r| ist_seite(&r, &b) || ist_unterseite(&r, &b));
        (index, if seite { StatusCode::OK } else { StatusCode::NOT_FOUND })
    };
    match tokio::fs::read(&file).await {
        Ok(bytes) => (status, [(header::CONTENT_TYPE, mime_for(&file))], bytes).into_response(),
        Err(_) => (StatusCode::SERVICE_UNAVAILABLE, Html(NOT_BUILT)).into_response(),
    }
}

/// Gibt es das Projekt (`projekt.toml`, gleich ob lesbar)?
fn projekt_da(b: &Bestand, slug: &str) -> bool {
    b.projekt(slug).is_some_and(|p| p.hat_projekt_toml)
}

/// `/projects/<slug>/neu` und `/projects/<slug>/neu/<name>` (Lesezeichen
/// aus der Zeit der Vorschau, Etappe 3b-2 bis 6a) leiten dauerhaft weiter
/// auf `/projects/<slug>` bzw. `/projects/<slug>/<name>`, wenn es das
/// Projekt gibt; eine unbekannte Unterseite ist dann am Ziel 404. Sonst
/// `None` (ein unbekanntes Projekt ist 404).
pub fn weiter_zur_hauptseite(rel: &str, b: &Bestand) -> Option<String> {
    match rel.split('/').collect::<Vec<_>>().as_slice() {
        ["projects", slug, "neu"] if projekt_da(b, slug) => Some(format!("/projects/{slug}")),
        ["projects", slug, "neu", name] if projekt_da(b, slug) => Some(format!("/projects/{slug}/{name}")),
        _ => None,
    }
}

/// `/projects/<slug>/<name>` ist eine Seite, wenn es das Projekt gibt und
/// `<name>` eine Unterseite aus `seiten` ist. `neu` und `vorschau` sind feste
/// Wege, keine Unterseiten.
pub fn ist_unterseite(rel: &str, b: &Bestand) -> bool {
    match rel.split('/').collect::<Vec<_>>().as_slice() {
        ["projects", slug, name] if *name != "neu" && *name != "vorschau" && projekt_da(b, slug) => {
            b.projekt(slug).is_some_and(|p| matches!(&p.kopf, Some(Ok((k, _))) if k.seiten.iter().any(|s| s.name == *name)))
        }
        _ => false,
    }
}

/// Die Oberfläche lag unter /neu; alte Lesezeichen landen auf /.
pub async fn neu_redirect(uri: Uri) -> Response {
    let rest = uri.path().trim_start_matches("/neu").trim_start_matches('/');
    let target = match uri.query() {
        Some(q) => format!("/{rest}?{q}"),
        None => format!("/{rest}"),
    };
    Redirect::permanent(&target).into_response()
}

/// Der Teil der Adresse nach `/` als Pfad unter `web/build`, oder `None`.
///
/// Unter Windows ersetzt `join` den ganzen Pfad, wenn der zweite Teil ein
/// Laufwerk oder eine Wurzel hat (`C:/Windows/win.ini`, `//server/share`,
/// `\\?\C:\…`). Darum gilt nur: jedes Segment besteht aus `A-Z a-z 0-9 . _
/// -`, beginnt nicht mit einem Punkt (also auch kein `.` oder `..`) und ist
/// nicht leer; kein `:` (Laufwerk, alternativer Datenstrom `datei:strom`), kein
/// `\`, kein `%` (kodierte Zeichen werden nie als Datei gelesen), keine zwei
/// Schrägstriche vorn. Zum Schluss muss jeder Teil `Component::Normal` sein.
pub fn sicherer_teilpfad(pfad: &str) -> Option<std::path::PathBuf> {
    use std::path::{Component, PathBuf};
    if pfad.starts_with("//") {
        return None;
    }
    let rel = pfad.strip_prefix('/').unwrap_or(pfad);
    if rel.is_empty() {
        return Some(PathBuf::new());
    }
    let rel = rel.strip_suffix('/').unwrap_or(rel);
    let mut aus = PathBuf::new();
    for seg in rel.split('/') {
        let gut = !seg.is_empty() && !seg.starts_with('.') && seg.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
        if !gut {
            return None;
        }
        aus.push(seg);
    }
    if !aus.components().all(|c| matches!(c, Component::Normal(_))) {
        return None;
    }
    Some(aus)
}

/// Liegt `datei` (aufgelöst, ohne Verknüpfungen) wirklich unter `ordner`?
fn liegt_unter(datei: &std::path::Path, ordner: &std::path::Path) -> bool {
    match (datei.canonicalize(), ordner.canonicalize()) {
        (Ok(d), Ok(o)) => d.starts_with(&o) && d != o,
        _ => false,
    }
}

fn nicht_gefunden(what: &str) -> Response {
    AppError::nicht_gefunden(format!("Nicht gefunden: {what}")).into_response()
}

/// Hinweisseite, solange `web/build` fehlt. Absichtlich ohne Abhängigkeiten.
/// Etappe 3b: in einfachem Deutsch, ohne Befehle und Pfade (Fund des Prüfers);
/// wie man baut, steht in der Anleitung („Starten“) und in LIESMICH.txt.
const NOT_BUILT: &str = "<!doctype html><html lang=\"de\"><meta charset=\"utf-8\">\
<title>Dashboard: gleich wieder da</title>\
<body style=\"font-family:system-ui;max-width:40em;margin:3em auto;line-height:1.5\">\
<h1>Die Seite ist gleich wieder da</h1>\
<p>Das Dashboard läuft, aber die Ansicht fehlt gerade. Das passiert kurz während eines Neubaus. \
Bitte in einer Minute neu laden.</p>\
<p><a href=\"/\">Zur Übersicht</a></p>\
</body></html>";
