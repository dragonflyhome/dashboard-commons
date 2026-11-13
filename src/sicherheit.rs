//! Die Schleusen: Code, durch den jede Anfrage läuft, bevor sie bei der
//! eigentlichen Route ankommt (Zielbild, Kapitel c). Jede prüft eine Sache.
//!
//! Reihenfolge von außen nach innen (siehe `app::router`):
//! 1 Absturzfänger, 2 Protokoll mit Nummer, 3 Adress-Prüfung, 3a kein `%` im
//! Pfad unter `/api` (Hotfix Prozent), 4 Herkunft,
//! 4b Windows-Anmeldung (Etappe 7b), 5 Zeitlimit, 6 Größe,
//! 7 Sicherheits-Kopfzeilen, 8 Zwischenspeicher.
//! Dazu die Fehlerform: JSON nur für `/api/*` oder `Accept: application/json`,
//! sonst ein lesbarer Satz als Text.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::body::Body;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Redirect, Response};
use sha2::{Digest, Sha256};

use crate::anmeldung::verbindung::Verbindung;
use crate::api::anmeldung::bitte_anmelden;
use crate::app::AppState;
use crate::fehler::{AppError, FehlerSatz};

/// Welche Adressen das Dashboard selbst sind (ohne Schema), für Host-Prüfung.
/// Der Server hört auf 127.0.0.1 und [::1] (siehe `dienst::binden`).
pub fn eigene_hosts(port: u16) -> [String; 3] {
    [format!("127.0.0.1:{port}"), format!("localhost:{port}"), format!("[::1]:{port}")]
}

/// Dieselben als Herkunft (mit Schema), für die Herkunftsprüfung.
pub fn eigene_herkuenfte(port: u16) -> Vec<String> {
    eigene_hosts(port).iter().map(|h| format!("http://{h}")).collect()
}

/// Pfade, die absichtlich lange offen bleiben: kein Zeitlimit.
pub fn ohne_zeitlimit(pfad: &str) -> bool {
    pfad == "/api/events" || pfad == FREIER_KANAL || pfad == "/api/vorgaenge/warten"
}

/// Der freie Live-Kanal (Kopfleiste, Fassung 2): liegt unter `/api/events`,
/// ist aber als genau dieser Pfad frei (siehe `geschuetzt`); alles darunter
/// bleibt geschützt.
pub const FREIER_KANAL: &str = "/api/events/frei";

// ----- 1 Absturzfänger -----

/// Aus einem Absturz im Handler wird eine höfliche Fehlerantwort. Einzelheiten
/// gehen nur ins Protokoll.
pub fn absturz(err: Box<dyn std::any::Any + Send + 'static>) -> Response {
    let grund = if let Some(s) = err.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = err.downcast_ref::<&str>() {
        (*s).to_string()
    } else {
        "unbekannt".to_string()
    };
    tracing::error!("Absturz in einer Anfrage: {grund}");
    AppError::intern("Im Dashboard ist etwas schiefgegangen. Der Fehler steht im Protokoll.").into_response()
}

// ----- 2 Protokoll: fortlaufende Nummer je Anfrage -----

#[derive(Clone, Default)]
pub struct Zaehler(Arc<AtomicU64>);

/// Nummer der Anfrage, als Erweiterung für Handler und Protokoll.
#[derive(Clone, Copy, Debug)]
pub struct AnfrageNr(pub u64);

pub async fn nummerieren(State(z): State<Zaehler>, mut req: Request, next: Next) -> Response {
    let nr = z.0.fetch_add(1, Ordering::Relaxed) + 1;
    req.extensions_mut().insert(AnfrageNr(nr));
    let mut res = next.run(req).await;
    if let Ok(v) = HeaderValue::from_str(&nr.to_string()) {
        res.headers_mut().insert("x-request-id", v);
    }
    res
}

/// Die Schicht von `protokoll`.
pub type Protokoll = tower_http::trace::TraceLayer<
    tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>,
    fn(&Request) -> tracing::Span,
    tower_http::trace::DefaultOnRequest,
    tower_http::trace::DefaultOnResponse,
>;

/// Schleuse 2: ein Span je Anfrage mit Nummer, Methode und Pfad; Antwort mit
/// Status und Dauer auf INFO. Nie Kopfzeilen, nie Körper, keine Abfrage (Query).
pub fn protokoll() -> Protokoll {
    fn span(req: &Request) -> tracing::Span {
        let nr = req.extensions().get::<AnfrageNr>().map_or(0, |n| n.0);
        tracing::info_span!("anfrage", nr, methode = %req.method(), pfad = %req.uri().path())
    }
    tower_http::trace::TraceLayer::new_for_http()
        .make_span_with(span as fn(&Request) -> tracing::Span)
        .on_request(tower_http::trace::DefaultOnRequest::new().level(tracing::Level::DEBUG))
        .on_response(tower_http::trace::DefaultOnResponse::new().level(tracing::Level::INFO))
}

// ----- 3 Adress-Prüfung (gegen DNS-Rebinding) -----

pub async fn nur_eigene_adresse(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let host =
        req.uri().authority().map(|a| a.as_str().to_string()).or_else(|| req.headers().get(header::HOST).and_then(|h| h.to_str().ok()).map(str::to_string));
    let ok = host.as_deref().is_some_and(|h| eigene_hosts(state.port).iter().any(|e| e.eq_ignore_ascii_case(h)));
    if !ok {
        // Nie den Inhalt des Host-Kopfs ins Protokoll (kommt von außen), nur seine Länge.
        match &host {
            Some(h) => tracing::warn!("Anfrage mit fremder Adresse abgelehnt (Host-Kopf: {} Zeichen)", h.chars().count()),
            None => tracing::warn!("Anfrage mit fremder Adresse abgelehnt (ohne Host-Kopf)"),
        }
        return AppError::falsche_adresse("Diese Anfrage kam nicht über die Adresse des Dashboards (127.0.0.1).").into_response();
    }
    next.run(req).await
}

// ----- 4 Herkunft: Schreiben nur vom Dashboard selbst -----

/// Ergänzt die `CsrfLayer` aus tower-http um eine strengere Regel: Schreibende
/// Anfragen ohne jede Herkunftsangabe (weder `Sec-Fetch-Site` noch `Origin`
/// noch passender `Referer`) werden abgelehnt. Die Schicht aus tower-http lässt
/// solche Anfragen durch, weil sie von keinem Browser stammen können; hier
/// geht es aber um Passwörter, darum lieber zu streng.
///
/// Unter `/api/*` und für Projektdateien (`/projects/<slug>/assets/<datei>`)
/// gilt die Browser-Regel (`Sec-Fetch-Site: cross-site` oder `same-site` → 403)
/// auch für lesende Anfragen: Eine fremde Seite soll weder Daten lesen noch
/// etwa `/api/pruefung` immer wieder auslösen. `same-origin`, `none` (Adresse
/// eingetippt) und ohne Kopfzeile (curl) bleiben erlaubt. Unter `/api/vorgaenge`
/// und `/api/aufgaben` zählt beim Lesen zusätzlich ein fremder `Origin`. Die Oberfläche selbst
/// (`/`, Seiten, `/_app/…`) bleibt für Links von außen erreichbar.
pub async fn herkunft_pflicht(State(state): State<AppState>, req: Request, next: Next) -> Response {
    // Die Chats melden Vorgänge (Etappe 2b) und Aufgaben (5a) per curl ohne
    // Browser-Kopfzeilen. Für diese Pfade gilt nur die Browser-Regel (cross-site
    // bleibt 403), nicht die Herkunftspflicht.
    let pfad = req.uri().path();
    // Seit A1 auch Arbeiten, Einstufung (curl bekommt dort vom Handler 422) und die Liste;
    // seit A2 die Antworten (lesen per curl, antworten nur aus dem Browser).
    // seit A3 die Meldungen (melden per curl, „gelesen“ nur aus dem Browser).
    let chat_weg = ["/api/vorgaenge", "/api/aufgaben", "/api/arbeiten", "/api/einstufung", "/api/anstehend", "/api/antworten", "/api/meldungen"]
        .iter()
        .any(|w| pfad.strip_prefix(w).is_some_and(|rest| rest.is_empty() || rest.starts_with('/')));
    let lesend = matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if lesend && !chat_weg && !nur_von_hier(pfad) {
        return next.run(req).await;
    }
    let h = req.headers();
    // Sagt der Browser selbst, dass die Anfrage von einer fremden Seite kommt, ist Schluss,
    // auch wenn ein Origin mitkommt (die Schicht von tower-http prüft Origin zuerst).
    let fremd = h.get("sec-fetch-site").and_then(|v| v.to_str().ok()).is_some_and(|v| matches!(v, "cross-site" | "same-site"));
    // Beim Lesen prüft die Schicht von tower-http den Origin nicht; für die Vorgänge hier selbst.
    let fremder_origin = chat_weg && h.get(header::ORIGIN).is_some_and(|o| !o.to_str().is_ok_and(|o| eigene_herkuenfte(state.port).iter().any(|e| e == o)));
    if fremd && !fremder_origin && bild_im_rahmen(&req) {
        return next.run(req).await;
    }
    if fremd || fremder_origin {
        tracing::warn!("{} {} von fremder Seite abgelehnt", req.method(), req.uri().path());
        return if lesend { abgelehnt_lesend() } else { abgelehnt() };
    }
    if lesend {
        return next.run(req).await;
    }
    let hat_sec_fetch = h.get("sec-fetch-site").is_some();
    let hat_origin = h.get(header::ORIGIN).is_some_and(|o| !o.is_empty());
    let referer_ok = h
        .get(header::REFERER)
        .and_then(|r| r.to_str().ok())
        .is_some_and(|r| eigene_herkuenfte(state.port).iter().any(|a| r == a || r.starts_with(&format!("{a}/"))));
    if !chat_weg && !(hat_sec_fetch || hat_origin || referer_ok) {
        tracing::warn!("{} {} ohne Herkunft abgelehnt", req.method(), req.uri().path());
        return abgelehnt();
    }
    next.run(req).await
}

/// Wege, die auch lesend nur das Dashboard selbst abrufen darf: `/api/*` und
/// `/projects/<slug>/assets/<datei>`.
pub fn nur_von_hier(pfad: &str) -> bool {
    if pfad == "/api" || pfad.starts_with("/api/") {
        return true;
    }
    let mut t = pfad.split('/');
    t.next() == Some("") && t.next() == Some("projects") && t.next().is_some() && t.next() == Some("assets") && t.next().is_some_and(|d| !d.is_empty())
}

/// Enge Ausnahme für die Vorschau: Der Vorschau-Rahmen (`sandbox` ohne
/// `allow-same-origin`) hat eine undurchsichtige Herkunft, der Browser schickt
/// darum für seine Bilder `Sec-Fetch-Site: cross-site`. Erlaubt ist trotzdem
/// genau: GET auf `/projects/<slug>/assets/<datei>` mit Bildendung (svg, png,
/// jpg, jpeg, webp, gif), `Sec-Fetch-Dest: image` und `Sec-Fetch-Mode: no-cors`.
/// Ein Bild im no-cors-Modus liefert dem Aufrufer keine Daten zurück, und die
/// SVG-CSP (`ASSET_SVG_CSP`) verhindert Skripte. Alles andere bleibt 403.
fn bild_im_rahmen(req: &Request) -> bool {
    const BILDENDUNGEN: [&str; 6] = ["svg", "png", "jpg", "jpeg", "webp", "gif"];
    let h = req.headers();
    let kopf_ist = |name: &str, wert: &str| h.get(name).and_then(|v| v.to_str().ok()) == Some(wert);
    let pfad = req.uri().path();
    let bild = pfad.starts_with("/projects/")
        && nur_von_hier(pfad)
        && pfad.rsplit_once('.').is_some_and(|(_, e)| !e.contains('/') && BILDENDUNGEN.contains(&e.to_ascii_lowercase().as_str()));
    *req.method() == Method::GET && bild && kopf_ist("sec-fetch-dest", "image") && kopf_ist("sec-fetch-mode", "no-cors")
}

/// Antwort der Herkunftsprüfung bei lesenden Anfragen (kein Formular im Spiel).
pub fn abgelehnt_lesend() -> Response {
    AppError::abgelehnt("Abgelehnt: Diese Anfrage kam nicht vom Dashboard selbst (http://127.0.0.1).").into_response()
}

/// Antwort der Herkunftsprüfung (auch für `CsrfLayer::with_rejection_response`).
pub fn abgelehnt() -> Response {
    AppError::abgelehnt("Abgelehnt: Das Formular kam nicht vom Dashboard selbst (http://127.0.0.1).").into_response()
}

// ----- 4b Windows-Anmeldung: das Konto des Inhabers auf den geschützten Wegen -----

/// Etappe B2: die Datei mit den Fragen an den Inhaber (`daten/fragen.toml`); ihr
/// Weg ist geschützt, und frei gehen ihre Texte nirgends hinaus.
pub const FRAGEN_DATEI: &str = "fragen";

/// Etappe B3: Ereignisse und Fristen (`daten/fristen.toml`); Titel von Fristen
/// und Namen von Ereignissen sind persönlicher Inhalt wie die Fragen.
pub const FRISTEN_DATEI: &str = "fristen";

/// Die eine Liste der geschützten Dateien in `daten/` (ohne `.toml`). Aus ihr
/// lesen `geschuetzt` (erste Prüfung, am Pfad), der Handler
/// `api::neue_form::daten` (zweite Wand, am dekodierten Namen) und
/// `api::projekte::eine` (der Daten-Teil von `/api/projects/<slug>`).
/// `orte` und `sicherung` kommen mit B4 dazu.
pub const GESCHUETZTE_DATEIEN: [&str; 2] = [FRAGEN_DATEI, FRISTEN_DATEI];

/// Ist `name` (ein Dateiname aus `daten/`, ohne `.toml`) geschützt? Groß/Klein
/// zählt nicht, wie in `geschuetzt`.
pub fn datei_geschuetzt(name: &str) -> bool {
    GESCHUETZTE_DATEIEN.iter().any(|d| d.eq_ignore_ascii_case(name))
}

// ----- 3a Prozentzeichen im Pfad (Hotfix Prozent) -----

/// Der Satz der ersten Wand.
pub const SATZ_PROZENT: &str = "Die Adresse ist fehlerhaft: Wege unter /api enthalten kein Prozentzeichen (keine %-Kodierung im Pfad).";

/// Ein Weg unter `/api` (Groß/Klein egal, damit auch `/API/…` darunter fällt).
fn api_weg(pfad: &str) -> bool {
    let p = pfad.as_bytes();
    p.len() >= 4 && p[..4].eq_ignore_ascii_case(b"/api") && (p.len() == 4 || p[4] == b'/')
}

/// Schleuse 3a (Hotfix Prozent): Ein Pfad unter `/api` mit `%` bekommt 400,
/// vor der Anmeldung (`geschuetzt` prüft den rohen Pfad) und vor jedem
/// Routing (axum dekodiert Platzhalter wie `{name}`, sonst käme
/// `/daten/%66risten` als `fristen` beim Handler an). Kein legitimer Weg unter
/// `/api` braucht `%`: Slugs (`seite::is_valid_slug`), Kennungen
/// (`vertrag::daten::ist_kennung`, `v-…`), Datennamen (`ist_datenname`) und
/// Passkey-Kennungen (base64url) kommen ohne aus. Die Abfrage (`?projekt=…`)
/// bleibt frei: Sie gehört nicht zum Pfad, und keine Zugangsregel hängt an ihr.
/// Außerhalb von `/api` hängt kein Zugang an einem Platzhalter: Die geschützten
/// Seiten (`/schluessel`, `/vorgaenge`, `/daten`) sind feste Wurzeln, Assets und
/// Oberfläche lehnen `%` selbst ab (`is_safe_asset_name`, `sicherer_teilpfad`),
/// und `anmeldung_pflicht` prüft dort zusätzlich den dekodierten Pfad.
pub async fn prozent_im_pfad(req: Request, next: Next) -> Response {
    let pfad = req.uri().path();
    if api_weg(pfad) && pfad.contains('%') {
        tracing::warn!("Pfad unter /api mit Prozentzeichen abgelehnt ({} Zeichen)", pfad.chars().count());
        return AppError::fehlerhaft(SATZ_PROZENT).into_response();
    }
    next.run(req).await
}

/// Der Pfad einmal %-dekodiert. `None`, wenn eine Kodierung kaputt ist, das
/// Ergebnis kein UTF-8 ist oder nach dem Dekodieren noch ein `%` steht
/// (doppelt kodiert, `%2566`): solche Pfade gelten nie als frei.
pub fn pfad_dekodiert(roh: &str) -> Option<String> {
    let b = roh.as_bytes();
    let mut aus = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = |c: u8| (c as char).to_digit(16);
            let (h, l) = (hex(*b.get(i + 1)?)?, hex(*b.get(i + 2)?)?);
            aus.push((h * 16 + l) as u8);
            i += 3;
        } else {
            aus.push(b[i]);
            i += 1;
        }
    }
    let text = String::from_utf8(aus).ok()?;
    (!text.contains('%')).then_some(text)
}

/// Wege, die nur das Windows-Konto des Inhabers sehen darf (`[anmeldung]`, geschuetzt):
/// die Schlüssel, die Vorgänge (auch `warten`), die Daten (der Export enthält
/// die Vorgänge), der Live-Kanal (er meldet Vorgänge; frei ist nur genau
/// `/api/events/frei`, Kopfleiste Fassung 2), die Aufgaben für den Inhaber
/// (sie enthalten einmalige Links, Etappe 5a), Arbeiten, Einstufung und die
/// Liste „Anstehend“ (Etappe A1, wie die Aufgaben), die Meldungen (Etappe A3; das Unter-Dashboard
/// `/api/projekte/<slug>/stand` bleibt frei, Aufgaben dort nur als Zahl) und die Benutzer-Daten
/// des PCs (`/api/pc/benutzer`), je mit allem darunter und für alle Methoden.
/// Seit 4d auch `/api/entscheidungen` und `/api/zielbild`, seit 6b `/api/anleitung`,
/// seit 4c `/api/bauen`.
/// Seit dem Hotfix 3a-1 auch die vollen Fassungen für die Oberfläche:
/// `/api/pc/verlauf/voll` und `/api/projekte/<slug>/stand/voll`. Edge schickt
/// Negotiate nur auf einer Verbindung, die vorher 401 bekam; ein freier Weg,
/// der mit Anmeldung mehr zeigt, wäre darum nur zufällig vollständig.
/// Frei bleiben die Projektseiten und ihre Daten, `/api/pc` (nur
/// Rechner-Werte, siehe `api::pc`), `/api/pc/verlauf` und
/// `/api/projekte/<slug>/stand` (für Werkzeuge), `/api/betrieb`, `/healthz`,
/// `/api/pruefung`, die Dateien der Oberfläche und `/api/anmeldung/*` (die
/// Probe). Groß/Klein zählt hier nicht, damit eine andere Schreibweise nie an
/// der Pflicht vorbeiführt. Seit Etappe B2 auch `/api/projekte/<slug>/backlog`
/// und `/api/projekte/<slug>/daten/fragen` (Fragen an den Inhaber sind
/// persönlicher Inhalt); die anderen Dateien in `daten/` bleiben frei. Seit
/// Etappe B3 auch `/api/projekte/<slug>/daten/fristen` und `/api/fristen`, seit
/// Etappe B8 `/api/projekte/<slug>/plan`.
pub fn geschuetzt(pfad: &str) -> bool {
    const WURZELN: [&str; 23] = [
        "/schluessel",
        "/api/schluessel",
        "/vorgaenge",
        "/api/vorgaenge",
        "/daten",
        "/api/daten",
        "/api/events",
        "/api/pc/benutzer",
        "/api/pc/verlauf/voll",
        "/api/aufgaben",
        "/api/arbeiten",
        "/api/einstufung",
        "/api/anstehend",
        "/api/antworten",
        "/api/meldungen",
        // Etappe 4d (Auflage aus der Abnahme): Entscheidungen und Zielbild
        // nennen Projekte und Pläne; nur der Inhaber liest sie. Die Seiten selbst
        // bleiben frei wie /anstehend und zeigen ohne Anmeldung den Hinweis.
        "/api/entscheidungen",
        "/api/zielbild",
        // Etappe 6b: die Anleitung nennt Wege und
        // Abläufe des Inhabers; geschützt wie das Zielbild, die Seite bleibt frei.
        "/api/anleitung",
        // Kopfleiste, Fassung 2: der volle Betriebsbericht mit
        // Pfaden und dem Anmeldestand des Inhabers; /api/betrieb selbst bleibt frei.
        "/api/betrieb/voll",
        // Kopfleiste, Fassung 3: die Nummer aller Ereignisse (zeigt, wie oft der Speicher des Inhabers
        // schreibt); /api/stand selbst bleibt frei und zählt nur Freies.
        "/api/stand/voll",
        // Etappe 4c: der Bau-Stand mit Satz (GET /api/bauen); Zustand und Zeit stehen frei
        // in /api/projects.
        "/api/bauen",
        // Entscheidung lokal-oeffnen: „Öffnen“ (POST /api/oeffnen) nur als Inhaber.
        "/api/oeffnen",
        // Etappe B3: die Fristenliste aller Projekte nennt Titel von Fristen und Aufgaben.
        "/api/fristen",
    ];
    let pfad = pfad.to_ascii_lowercase();
    // Kopfleiste, Fassung 2: der freie Kanal, genau dieser Pfad (nichts darunter, kein
    // Schrägstrich am Ende). Er trägt nur Lebenszeichen, Anmeldestand und schon freie
    // Daten (`api::events::FREIER_KANAL`); Adresse und Herkunft prüfen die Schleusen davor.
    if pfad == FREIER_KANAL {
        return false;
    }
    // Der volle Stand eines Projekts: /api/projekte/<slug>/stand/voll (mit allem darunter);
    // Etappe B2: das Backlog /api/projekte/<slug>/backlog (nennt Titel wie der volle Stand)
    // und die Fragen an den Inhaber /api/projekte/<slug>/daten/fragen (persönlicher
    // Inhalt; die anderen Dateien in daten/ bleiben frei wie die Seite).
    let teile: Vec<&str> = pfad.split('/').collect();
    let stand_voll = matches!(teile.as_slice(), ["", "api", "projekte", _, "stand", "voll", ..]);
    // Etappe B8: der Plan /api/projekte/<slug>/plan nennt Titel wie das Backlog.
    let backlog = matches!(teile.as_slice(), ["", "api", "projekte", _, "backlog" | "plan", ..]);
    // Die geschützten Dateien aus der einen Liste `GESCHUETZTE_DATEIEN`.
    let datei = matches!(teile.as_slice(), ["", "api", "projekte", _, "daten", name, ..] if datei_geschuetzt(name));
    stand_voll || backlog || datei || WURZELN.iter().any(|w| pfad.strip_prefix(w).is_some_and(|rest| rest.is_empty() || rest.starts_with('/')))
}

/// Eine Navigation des Browsers (GET/HEAD, nicht unter `/api`, mit
/// `Sec-Fetch-Dest: document` oder `Accept: text/html`).
fn ist_navigation(req: &Request) -> bool {
    if !matches!(*req.method(), Method::GET | Method::HEAD) {
        return false;
    }
    let pfad = req.uri().path();
    if pfad == "/api" || pfad.starts_with("/api/") {
        return false;
    }
    let h = req.headers();
    h.get("sec-fetch-dest").and_then(|v| v.to_str().ok()) == Some("document")
        || h.get(header::ACCEPT).and_then(|v| v.to_str().ok()).is_some_and(|a| a.contains("text/html"))
}

/// Der Browser (User-Agent) gekürzt fürs Protokoll: höchstens 80 Zeichen, nur
/// druckbare Zeichen.
pub fn browser_kurz(req: &Request) -> String {
    let ua = req.headers().get(header::USER_AGENT).and_then(|v| v.to_str().ok()).unwrap_or("ohne Angabe");
    let sauber: String = ua.chars().filter(|c| !c.is_control()).take(80).collect();
    if ua.chars().count() > 80 { format!("{sauber}…") } else { sauber }
}

/// HTML-Sonderzeichen ersetzen.
fn html_text(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

/// Auf einem geschützten Weg kam keine Anmeldung mit. Ins Protokoll kommen
/// Weg und Browser (gekürzt), damit sich Fälle wie „anderer Browser“
/// einordnen lassen. Eine Navigation des Browsers bekommt eine Seite in
/// einfachem Deutsch mit der Adresse zum Kopieren (401 mit
/// `WWW-Authenticate: Negotiate`, damit Edge trotzdem anmeldet und die Seite
/// nie sieht); alles andere die Antwort aus `bitte_anmelden`.
fn anmeldung_fehlt(req: &Request, port: u16, wo: &str) -> Response {
    tracing::info!("Anmeldung nötig, Verbindung {wo}: {} ohne Windows-Anmeldung, Browser: {}", req.uri().path(), browser_kurz(req));
    if !ist_navigation(req) {
        return bitte_anmelden(None);
    }
    let pfad_und_abfrage = req.uri().path_and_query().map_or(req.uri().path(), |p| p.as_str());
    let adresse = html_text(&format!("http://localhost:{port}{pfad_und_abfrage}"));
    let seite = format!(
        "<!doctype html><html lang=\"de\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\">\
         <title>Dashboard: Anmeldung nötig</title><body>\
         <h1>Diese Seite öffnet sich nur in Microsoft Edge</h1>\
         <p>Diese Seite öffnet sich nur in Microsoft Edge unter <a href=\"{adresse}\">{adresse}</a>. \
         Andere Browser schicken deine Windows-Anmeldung nicht mit.</p>\
         <p>Adresse zum Kopieren (anklicken, dann Strg+C):</p>\
         <p><input type=\"text\" readonly size=\"60\" value=\"{adresse}\" aria-label=\"Adresse für Microsoft Edge\"></p>\
         <p>Die Projektseiten und die PC-Werte gehen in jedem Browser.</p>\
         <p>Aus einem anderen Windows-Konto: <a href=\"{passkey}\">mit Passkey anmelden</a>.</p>\
         </body></html>",
        passkey = html_text(&anmelden_adresse(port, pfad_und_abfrage)),
    );
    let mut res = (StatusCode::UNAUTHORIZED, axum::response::Html(seite)).into_response();
    res.headers_mut().insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Negotiate"));
    res
}

/// Ein Browser öffnet eine geschützte Seite über 127.0.0.1: Dort meldet
/// Chromium nicht an (nur localhost zählt als Intranet), darum 307 auf
/// dieselbe Seite unter `http://localhost:<port>` (`[anmeldung]`, adresse).
/// Nur GET/HEAD-Navigationen (`Sec-Fetch-Dest: document` oder `Accept:
/// text/html`) und nie unter `/api/*`: API-Wege antworten 401.
fn nach_localhost(req: &Request, port: u16) -> Option<String> {
    if !matches!(*req.method(), Method::GET | Method::HEAD) {
        return None;
    }
    let pfad = req.uri().path();
    if pfad == "/api" || pfad.starts_with("/api/") {
        return None;
    }
    let h = req.headers();
    let host = req.uri().authority().map(|a| a.as_str().to_string()).or_else(|| h.get(header::HOST).and_then(|v| v.to_str().ok()).map(str::to_string))?;
    if !host.eq_ignore_ascii_case(&format!("127.0.0.1:{port}")) {
        return None;
    }
    let dokument = h.get("sec-fetch-dest").and_then(|v| v.to_str().ok()) == Some("document")
        || h.get(header::ACCEPT).and_then(|v| v.to_str().ok()).is_some_and(|a| a.contains("text/html"));
    if !dokument {
        return None;
    }
    let pfad_und_abfrage = req.uri().path_and_query().map_or(pfad, |p| p.as_str());
    Some(format!("http://localhost:{port}{pfad_und_abfrage}"))
}

/// Die Adresse der Passkey-Anmeldung mit dem Ziel `ziel` (Pfad und Abfrage,
/// prozentkodiert), immer über localhost (Passkeys gehen nicht mit einer IP).
pub fn anmelden_adresse(port: u16, ziel: &str) -> String {
    let mut kodiert = String::with_capacity(ziel.len());
    for b in ziel.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'/') {
            kodiert.push(b as char);
        } else {
            kodiert.push_str(&format!("%{b:02X}"));
        }
    }
    format!("http://localhost:{port}/anmelden?ziel={kodiert}")
}

/// Ein anderes Windows-Konto ohne gültigen Ausweis auf einem geschützten Weg
/// (Etappe 7c): Browser-Seiten leiten auf `/anmelden` (Passkey) mit dem Ziel,
/// alles andere bekommt 401 mit Art `passkey` (ohne Negotiate, sonst meldete
/// Edge dasselbe Konto immer wieder an).
fn passkey_noetig(req: &Request, port: u16) -> Response {
    if ist_navigation(req) {
        let ziel = req.uri().path_and_query().map_or(req.uri().path(), |p| p.as_str());
        return Redirect::to(&anmelden_adresse(port, ziel)).into_response();
    }
    let mut res = AppError::passkey_noetig().into_response();
    res.headers_mut().insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("Passkey realm=\"dashboard\""));
    res
}

/// Schleuse 4b: Auf den geschützten Wegen (`geschuetzt`) muss die Anfrage als
/// Inhaber gelten: Verbindung als Inhaber angemeldet (Negotiate über SSPI,
/// `anmeldung`, 7b) oder ein gültiger Ausweis nach Anmeldung mit Passkey
/// (Cookie `__Host-dashboard`, Etappe 7c). Liegt hinter der Herkunftsprüfung
/// und außerhalb (vor) dem Zeitlimit: So kommen auch `warten` und der
/// Live-Kanal, die vom Zeitlimit ausgenommen sind, nicht ohne Anmeldung durch.
/// Die Aushandlung selbst hat kein Zeitlimit (SSPI lokal, kurz).
///
/// - Verbindung schon als Inhaber angemeldet: durch, mit den Erweiterungen
///   `Angemeldet`, `Inhaber::Hauptkonto` und `WindowsKonto`.
/// - `Authorization: Negotiate …`: ein Schritt der Aushandlung. Braucht es
///   noch eine Runde, 401 mit dem Nachweis des Servers (auch auf freien Wegen,
///   der Client hat ja angefangen). Fertig: Verbindung samt Konto merken.
///   Scheitert der Nachweis: 401, neu anmelden.
/// - Gültiger Ausweis (Signatur, nur EdDSA, nicht abgelaufen, `jti` nicht
///   gesperrt, Passkey noch registriert): durch mit `Angemeldet` und
///   `Inhaber::Passkey`; ist er älter als eine Minute, kommt ein erneuerter mit
///   (höchstens 8 Stunden ab Anmeldung). Ein ungültiger wird gelöscht.
/// - Sonst auf einem geschützten Weg: fremdes Konto (Negotiate fertig, aber
///   nicht der Inhaber) → Browser-Seiten auf `/anmelden`, API 401 Art `passkey`;
///   ohne Nachweis 401 mit `WWW-Authenticate: Negotiate` (Art `anmeldung`),
///   bei einer Browser-Navigation über 127.0.0.1 307 auf localhost.
/// - Die Anträge auf einen Passkey (`api::passkey::konto_noetig`) brauchen
///   Negotiate mit irgendeinem Konto (der echte Kontoname), der Inhaber nicht.
///
/// Ins Protokoll kommt nur die Verbindungsnummer und ob die SID passte.
pub async fn anmeldung_pflicht(State(state): State<AppState>, req: Request, next: Next) -> Response {
    // Etappe 4b: schreibende Wege der Backend-Module nur mit Anmeldung. Seit
    // Etappe B2 auch lesende, außer das Modul nennt den Weg in `freie_wege`.
    let schreibend = !matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    let modul_geschuetzt = modul_weg(req.uri().path()) && (schreibend || !state.projekte.frei_lesbar(req.uri().path()));
    // Hotfix Prozent: auch der dekodierte Pfad zählt (außerhalb von `/api`, wo
    // `prozent_im_pfad` nicht greift); nie weniger geschützt als der rohe.
    let geschuetzt = geschuetzt(req.uri().path())
        || modul_geschuetzt
        || (req.uri().path().contains('%') && pfad_dekodiert(req.uri().path()).is_some_and(|d| geschuetzt(&d)));
    let konto_noetig = crate::api::passkey::konto_noetig(req.uri().path());
    let nr = req.extensions().get::<ConnectInfo<Verbindung>>().map(|c| c.0.nr);
    // Den Ausweis nur dort prüfen, wo er etwas ändert (geschützte Wege, Anträge,
    // die API); Dateien der Oberfläche brauchen keinen Blick in den Datenspeicher.
    let pfad = req.uri().path();
    let ausweis = if geschuetzt || konto_noetig || pfad == "/api" || pfad.starts_with("/api/") {
        crate::api::passkey::ausweis_aus(&state, req.headers()).await
    } else {
        crate::api::passkey::AusweisStand::Keiner
    };
    let pflicht = Pflicht { geschuetzt, konto_noetig };
    let (mut res, ausweis) = {
        #[cfg(windows)]
        {
            anmeldung_windows(&state, nr, pflicht, ausweis, req, next).await
        }
        #[cfg(not(windows))]
        {
            let _ = nr;
            ohne_windows(&state, pflicht, ausweis, req, next).await
        }
    };
    use crate::api::passkey::AusweisStand;
    match ausweis {
        AusweisStand::Gueltig { erneuert: Some(c), .. } if !res.headers().contains_key(header::SET_COOKIE) => {
            res.headers_mut().append(header::SET_COOKIE, c);
        }
        AusweisStand::Ungueltig if !res.headers().contains_key(header::SET_COOKIE) => {
            res.headers_mut().append(header::SET_COOKIE, crate::passkey::cookie::loeschen());
        }
        _ => {}
    }
    res
}

/// Was der Weg verlangt.
#[derive(Clone, Copy)]
struct Pflicht {
    /// Nur als Inhaber.
    geschuetzt: bool,
    /// Windows-Anmeldung mit irgendeinem Konto (Anträge auf Passkeys).
    konto_noetig: bool,
}

/// Mit gültigem Ausweis: Erweiterungen setzen und durch.
async fn mit_ausweis(mut req: Request, next: Next, sitzung: &crate::anmeldung::PasskeySitzung, konto: Option<crate::anmeldung::WindowsKonto>) -> Response {
    req.extensions_mut().insert(crate::anmeldung::Angemeldet);
    req.extensions_mut().insert(crate::anmeldung::Inhaber::Passkey(sitzung.clone()));
    if let Some(k) = konto {
        req.extensions_mut().insert(k);
    }
    next.run(req).await
}

#[cfg(not(windows))]
async fn ohne_windows(
    state: &AppState,
    pflicht: Pflicht,
    ausweis: crate::api::passkey::AusweisStand,
    req: Request,
    next: Next,
) -> (Response, crate::api::passkey::AusweisStand) {
    use crate::api::passkey::AusweisStand;
    if let AusweisStand::Gueltig { sitzung, .. } = &ausweis {
        if !pflicht.konto_noetig {
            return (mit_ausweis(req, next, sitzung, None).await, ausweis);
        }
    }
    if pflicht.geschuetzt || pflicht.konto_noetig {
        let res = nach_localhost(&req, state.port)
            .map(|ziel| Redirect::temporary(&ziel).into_response())
            .unwrap_or_else(|| anmeldung_fehlt(&req, state.port, "ohne Nummer"));
        return (res, ausweis);
    }
    (next.run(req).await, ausweis)
}

#[cfg(windows)]
async fn anmeldung_windows(
    state: &AppState,
    nr: Option<u64>,
    pflicht: Pflicht,
    ausweis: crate::api::passkey::AusweisStand,
    mut req: Request,
    next: Next,
) -> (Response, crate::api::passkey::AusweisStand) {
    use crate::anmeldung::aushandlung::{Ausgang, erwartete_sid, schritt_async};
    use crate::anmeldung::verbindung::{self, Stand, stand};
    use crate::anmeldung::{Angemeldet, Inhaber};
    use crate::api::anmeldung::{mit_ausgabe, nachweis_aus};
    use crate::api::passkey::AusweisStand;

    // Die Probe handelt selbst aus (sonst käme jeder Nachweis zweimal an SSPI).
    if req.uri().path().starts_with("/api/anmeldung/") {
        return (next.run(req).await, ausweis);
    }
    let Pflicht { geschuetzt, konto_noetig } = pflicht;
    let wo = nr.map_or_else(|| "ohne Nummer".to_string(), |n| n.to_string());
    let bisher = nr.map_or(Stand::Neu, stand);
    let als_inhaber = |mut req: Request, konto: Option<crate::anmeldung::WindowsKonto>| {
        req.extensions_mut().insert(Angemeldet);
        req.extensions_mut().insert(Inhaber::Hauptkonto);
        if let Some(k) = konto {
            req.extensions_mut().insert(k);
        }
        req
    };
    if bisher == Stand::Inhaber {
        let konto = nr.and_then(verbindung::konto);
        return (next.run(als_inhaber(req, konto)).await, ausweis);
    }
    // Ein Schritt der Aushandlung, wenn der Client einen Nachweis schickt.
    let mut letzte_ausgabe: Option<Vec<u8>> = None;
    match nachweis_aus(req.headers()) {
        Some(Ok(nachweis)) => {
            let erwartet = match erwartete_sid(state.einstellungen.as_deref()) {
                Ok(s) => Some(s),
                Err(e) => {
                    tracing::error!("Anmeldung: eigene SID nicht lesbar ({e})");
                    if geschuetzt || konto_noetig {
                        let res = AppError::intern("Die Windows-Anmeldung ist gerade nicht möglich.").into_response();
                        return (res, ausweis);
                    }
                    None
                }
            };
            if let Some(erwartet) = erwartet {
                match schritt_async(nr, nachweis, erwartet).await {
                    Ausgang::Weiter(antwort) => return (bitte_anmelden(Some(&antwort)), ausweis),
                    Ausgang::Fertig { ausgabe, inhaber: true, konto, .. } => {
                        let mut res = next.run(als_inhaber(req, Some(konto))).await;
                        mit_ausgabe(&mut res, &ausgabe);
                        return (res, ausweis);
                    }
                    // Fremdes Konto: gemerkt (samt Name); weiter unten entscheidet Ausweis oder Weg.
                    Ausgang::Fertig { ausgabe, .. } => letzte_ausgabe = Some(ausgabe),
                    Ausgang::OhneKontext | Ausgang::Abgewiesen(_) if !(geschuetzt || konto_noetig) => {}
                    Ausgang::OhneKontext => {
                        tracing::info!("Anmeldung, Verbindung {wo}: Fortsetzung ohne offene Aushandlung, neu beginnen");
                        return (bitte_anmelden(None), ausweis);
                    }
                    Ausgang::Abgewiesen(e) => {
                        tracing::warn!("Anmeldung, Verbindung {wo}: Nachweis nicht angenommen ({e})");
                        return (bitte_anmelden(None), ausweis);
                    }
                    Ausgang::Intern(e) => {
                        tracing::error!("Anmeldung: {e}");
                        if geschuetzt || konto_noetig {
                            let res = AppError::intern("Die Windows-Anmeldung ist gerade nicht möglich.").into_response();
                            return (res, ausweis);
                        }
                    }
                }
            }
        }
        Some(Err(())) if geschuetzt || konto_noetig => {
            tracing::warn!("Anmeldung, Verbindung {wo}: Nachweis unlesbar");
            return (bitte_anmelden(None), ausweis);
        }
        _ => {}
    }
    // Das Konto der Verbindung, wenn die Aushandlung fertig ist (jetzt nur noch fremd).
    let konto = nr.and_then(verbindung::konto).filter(|k| !k.inhaber);
    let mit_letzter = |mut res: Response| {
        if let Some(a) = &letzte_ausgabe {
            mit_ausgabe(&mut res, a);
        }
        res
    };
    if let AusweisStand::Gueltig { sitzung, .. } = &ausweis
        && (!konto_noetig || konto.is_some())
    {
        let res = mit_ausweis(req, next, sitzung, konto).await;
        return (mit_letzter(res), ausweis);
    }
    if let Some(k) = konto {
        if geschuetzt {
            tracing::info!("Anmeldung, Verbindung {wo}: fremdes Konto ohne gültigen Ausweis, weiter zum Passkey");
            return (mit_letzter(passkey_noetig(&req, state.port)), ausweis);
        }
        req.extensions_mut().insert(k);
        return (mit_letzter(next.run(req).await), ausweis);
    }
    if !(geschuetzt || konto_noetig) {
        return (next.run(req).await, ausweis);
    }
    if konto_noetig && !geschuetzt {
        // Die Anträge: erst Windows-Anmeldung, dann geht es weiter.
        return (bitte_anmelden(None), ausweis);
    }
    if let Some(ziel) = nach_localhost(&req, state.port) {
        tracing::info!("Anmeldung: Browser über 127.0.0.1, weiter nach localhost");
        return (Redirect::temporary(&ziel).into_response(), ausweis);
    }
    (anmeldung_fehlt(&req, state.port, &wo), ausweis)
}

// ----- 5 Zeitlimit -----

pub async fn zeitlimit(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if ohne_zeitlimit(req.uri().path()) {
        return next.run(req).await;
    }
    let pfad = req.uri().path().to_string();
    match tokio::time::timeout(state.zeitlimit, next.run(req)).await {
        Ok(res) => res,
        Err(_) => {
            tracing::warn!("{pfad}: Zeitlimit von {:?} überschritten", state.zeitlimit);
            AppError::zu_lange("Das hat zu lange gedauert. Bitte noch einmal versuchen.").into_response()
        }
    }
}

// ----- 6 Größe -----

/// Höchstens so viele Bytes je Anfrage (der Schlüssel-POST erlaubt 4 KB).
pub const MAX_KOERPER: usize = 16 * 1024;

/// Lehnt zu große Anfragen schon an der Längenangabe ab; die
/// `RequestBodyLimitLayer` dahinter fängt den Rest beim Lesen. Die Antwort-Wege
/// (`antwort_weg`, für „Etwas anderes …“) und die Wege der
/// Backend-Module (`/api/projekte/<slug>/x/…`, Etappe 4b) dürfen 64 KiB.
pub async fn groesse(req: Request, next: Next) -> Response {
    let modul = modul_weg(req.uri().path()) || antwort_weg(req.uri().path());
    let grenze = if modul { dashboard_projekt::rahmen::KOERPER_HOECHSTENS } else { MAX_KOERPER };
    let zu_gross = req.headers().get(header::CONTENT_LENGTH).and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<usize>().ok()).is_some_and(|n| n > grenze);
    if zu_gross {
        let satz = if modul { "Zu viele Daten auf einmal (höchstens 64 KiB)." } else { "Zu viele Daten auf einmal (höchstens 16 KB)." };
        return AppError::zu_gross(satz).into_response();
    }
    next.run(req).await
}

/// Höchstens so viele Bytes für eine Antwort des Inhabers (Frage oder Aufgabe):
/// Bei „Etwas anderes …“ dürfen die eigenen Worte
/// bis 20 000 Zeichen lang sein; mehr als 16 KB. Gleich der Grenze der
/// Modul-Wege, damit die äußere Schleuse bleibt, wie sie ist.
pub const ANTWORT_KOERPER_HOECHSTENS: usize = dashboard_projekt::rahmen::KOERPER_HOECHSTENS;

/// Ein Weg, auf dem der Inhaber antwortet: `/api/antworten/<slug>/<id>` und
/// `/api/aufgaben/<id>/erledigt`. Sie nehmen bis `ANTWORT_KOERPER_HOECHSTENS`.
pub fn antwort_weg(pfad: &str) -> bool {
    let teile: Vec<&str> = pfad.split('/').collect();
    match teile.as_slice() {
        ["", "api", "antworten", slug, id] => !slug.is_empty() && !id.is_empty(),
        ["", "api", "aufgaben", id, "erledigt"] => !id.is_empty(),
        _ => false,
    }
}

/// Ein Weg eines Backend-Moduls: `/api/projekte/<slug>/x` oder darunter.
pub fn modul_weg(pfad: &str) -> bool {
    let mut teile = pfad.split('/');
    teile.next() == Some("")
        && teile.next() == Some("api")
        && teile.next() == Some("projekte")
        && teile.next().is_some_and(|s| !s.is_empty())
        && teile.next() == Some("x")
}

// ----- 7 Sicherheits-Kopfzeilen -----

/// CSP der Oberfläche. Das SvelteKit-Build enthält genau ein eingebettetes
/// Start-Skript in index.html; dessen Prüfsumme wird beim Start berechnet, damit
/// `script-src` ohne `'unsafe-inline'` auskommt. Inline-Styles (`style="…"`)
/// setzen die Bausteine selbst, darum `style-src 'unsafe-inline'`.
pub fn csp_oberflaeche(skript_hashes: &[String]) -> String {
    let mut script = String::from("'self'");
    for h in skript_hashes {
        script.push_str(" 'sha256-");
        script.push_str(h);
        script.push('\'');
    }
    format!(
        "default-src 'self'; script-src {script}; style-src 'self' 'unsafe-inline'; img-src 'self' data:; \
         font-src 'self'; connect-src 'self'; frame-src 'self'; frame-ancestors 'none'; base-uri 'none'; \
         form-action 'self'; object-src 'none'"
    )
}

/// SHA-256 (Base64) aller `<script>…</script>`-Blöcke ohne `src` in einer HTML-Datei.
pub fn inline_skript_hashes(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find("<script") {
        let nach_tag = &rest[start..];
        let Some(tag_ende) = nach_tag.find('>') else { break };
        let tag = &nach_tag[..tag_ende];
        let inhalt_ab = &nach_tag[tag_ende + 1..];
        let Some(ende) = inhalt_ab.find("</script>") else { break };
        if !tag.contains("src=") {
            let inhalt = &inhalt_ab[..ende];
            let digest = Sha256::digest(inhalt.as_bytes());
            out.push(base64(&digest));
        }
        rest = &inhalt_ab[ende + 9..];
    }
    out
}

/// Base64 (Standard-Alphabet, mit Füllzeichen), ohne weiteres Paket.
fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if chunk.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if chunk.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

/// Setzt CSP (nur wenn die Route keine eigene gesetzt hat, Vorschau-HTML behält
/// seine), `nosniff` und `Referrer-Policy` auf jede Antwort.
pub async fn kopfzeilen(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    if !h.contains_key(header::CONTENT_SECURITY_POLICY)
        && let Ok(v) = HeaderValue::from_str(&state.csp.aktuell())
    {
        h.insert(header::CONTENT_SECURITY_POLICY, v);
    }
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    res
}

// ----- 8 Zwischenspeicher -----

pub async fn zwischenspeicher(req: Request, next: Next) -> Response {
    let unveraenderlich = req.uri().path().starts_with("/_app/immutable/");
    let mut res = next.run(req).await;
    let wert = if unveraenderlich && res.status().is_success() { "public, max-age=31536000, immutable" } else { "no-store" };
    res.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static(wert));
    res
}

// ----- Fehlerform -----

/// JSON nur für `/api/*` oder wenn der Aufrufer JSON will; sonst derselbe Satz
/// als `text/plain`, damit der Inhaber im Browser keine geschweiften Klammern sieht.
///
/// Zwei Ergänzungen:
/// - Fehlerantworten ohne eigenen Satz (axum lehnt selbst ab: 405, 400, 415,
///   422 …, ohne Körper oder mit englischem Klartext) bekommen einen Satz je
///   Status, damit jede Fehlerantwort dieselbe Form hat.
/// - Scheitert das Schlüssel-Formular im Browser an einer Schleuse (403, 408,
///   413) oder an einer unlesbaren Adresse (400), geht es mit dem Satz zurück
///   ins Formular (303 auf `/schluessel?fehler=…`) statt auf eine nackte Fehlerseite.
pub async fn fehlerform(req: Request, next: Next) -> Response {
    let h = req.headers();
    let will_json =
        req.uri().path().starts_with("/api/") || h.get(header::ACCEPT).and_then(|a| a.to_str().ok()).is_some_and(|a| a.contains("application/json"));
    let formular = *req.method() == Method::POST && req.uri().path().starts_with("/schluessel/") && !will_json && vom_browser(h);
    let res = mit_satz(next.run(req).await).await;
    if formular && matches!(res.status(), StatusCode::BAD_REQUEST | StatusCode::FORBIDDEN | StatusCode::REQUEST_TIMEOUT | StatusCode::PAYLOAD_TOO_LARGE) {
        let satz = res.extensions().get::<FehlerSatz>().map(|s| s.0.clone()).unwrap_or_else(|| satz_fuer(res.status()).1.to_string());
        tracing::info!("Schlüssel-Formular mit {} abgelehnt, zurück ins Formular", res.status());
        return Redirect::to(&format!("/schluessel?fehler={}", percent_encode(&satz))).into_response();
    }
    if will_json {
        return res;
    }
    let Some(satz) = res.extensions().get::<FehlerSatz>().cloned() else { return res };
    let (mut teile, _) = res.into_parts();
    teile.headers.remove(header::CONTENT_LENGTH);
    teile.headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/plain; charset=utf-8"));
    Response::from_parts(teile, Body::from(satz.0))
}

/// Kam die Anfrage von einem Browser, der eine Seite erwartet? Browser schicken
/// bei Formularen `Accept: text/html…` und die `Sec-Fetch-*`-Kopfzeilen; curl
/// und Skripte meist nicht, die bekommen weiter den Status selbst.
fn vom_browser(h: &axum::http::HeaderMap) -> bool {
    let will_html = h.get(header::ACCEPT).and_then(|a| a.to_str().ok()).is_some_and(|a| a.contains("text/html"));
    will_html || ["sec-fetch-site", "sec-fetch-mode", "sec-fetch-dest"].iter().any(|k| h.contains_key(*k))
}

/// Art und Satz für Fehlerantworten, die axum ohne Körper schickt.
fn satz_fuer(status: StatusCode) -> (&'static str, &'static str) {
    match status {
        StatusCode::METHOD_NOT_ALLOWED => ("methode", "Diese Adresse nimmt diese Art Anfrage nicht an."),
        StatusCode::BAD_REQUEST => ("fehlerhaft", "Die Anfrage ist fehlerhaft."),
        StatusCode::NOT_FOUND => ("nicht-gefunden", "Diese Adresse gibt es nicht."),
        StatusCode::UNSUPPORTED_MEDIA_TYPE | StatusCode::UNPROCESSABLE_ENTITY => ("unlesbar", "Die Anfrage hat die falsche Form."),
        StatusCode::FORBIDDEN => ("abgelehnt", "Die Anfrage ist fehlgeschlagen."),
        _ => ("intern", "Die Anfrage ist fehlgeschlagen."),
    }
}

/// Fehlerantwort ohne `FehlerSatz` und ohne `Content-Type` oder mit
/// `text/plain` (axum-eigene Ablehnung, etwa „Invalid URL: Invalid UTF-8 …“):
/// bekommt die feste Form `{fehler, art}` samt `FehlerSatz`. Der englische Text
/// geht nur ins Protokoll. Andere Arten (etwa `text/html`) bleiben, wie sie sind.
/// Vorhandene Kopfzeilen (etwa `allow` bei 405) bleiben erhalten.
async fn mit_satz(res: Response) -> Response {
    let status = res.status();
    let klartext = match res.headers().get(header::CONTENT_TYPE) {
        None => Some(false),
        Some(t) => t.to_str().is_ok_and(|t| t.trim_start().starts_with("text/plain")).then_some(true),
    };
    let Some(klartext) = klartext else { return res };
    if status.as_u16() < 400 || res.extensions().get::<FehlerSatz>().is_some() {
        return res;
    }
    let (art, satz) = satz_fuer(status);
    let (teile, koerper) = res.into_parts();
    if klartext {
        // Höchstens 4 KB lesen: axum-Ablehnungen sind kurz, mehr braucht das Protokoll nicht.
        let text = axum::body::to_bytes(koerper, 4096).await.unwrap_or_default();
        tracing::debug!("Antwort {status} mit Klartext bekommt die Fehlerform ({art}): {}", String::from_utf8_lossy(&text));
    } else {
        tracing::debug!("Antwort {status} ohne Körper bekommt die Fehlerform ({art})");
    }
    let mut neu = AppError { status, art, fehler: satz.to_string() }.into_response();
    for (name, wert) in &teile.headers {
        if name != header::CONTENT_LENGTH && name != header::CONTENT_TYPE {
            neu.headers_mut().append(name.clone(), wert.clone());
        }
    }
    neu
}

/// Für Weiterleitungen: alles außer Buchstaben, Ziffern und `- _ .` wird
/// %XX-kodiert (auch vom Schlüssel-Handler genutzt).
pub fn percent_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Abnahme 4d: Die Daten von /entscheidungen und /zielbild (seit 6b auch
    /// /anleitung) sind geschützt,
    /// die Seiten selbst nicht (sie zeigen ohne Anmeldung den Hinweis).
    #[test]
    fn system_seiten_daten_geschuetzt() {
        for p in ["/api/entscheidungen", "/api/zielbild", "/Api/Zielbild", "/api/entscheidungen/", "/api/anleitung", "/API/Anleitung", "/api/anleitung/x"] {
            assert!(geschuetzt(p), "{p}");
        }
        for p in ["/entscheidungen", "/zielbild", "/api/zielbilder", "/anleitung", "/api/anleitungx"] {
            assert!(!geschuetzt(p), "{p}");
        }
    }

    /// Kopfleiste, Fassung 2: Nur genau `/api/events/frei` ist frei; der
    /// geschützte Kanal und alles unter dem freien bleiben geschützt.
    #[test]
    fn freier_kanal_genau_ein_pfad() {
        assert!(!geschuetzt("/api/events/frei"));
        assert!(ohne_zeitlimit("/api/events/frei"));
        for p in ["/api/events", "/api/events/", "/api/events/frei/", "/api/events/frei/x", "/api/events/freier", "/api/events/frei2"] {
            assert!(geschuetzt(p), "{p}");
        }
    }

    #[test]
    fn base64_wie_ueblich() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn hashes_nur_fuer_eingebettete_skripte() {
        let html = "<script src=\"/a.js\"></script><script>alert(1)</script><script type=\"module\">x</script>";
        let h = inline_skript_hashes(html);
        assert_eq!(h.len(), 2);
        // sha256("alert(1)") = bhHHL3z2vDgxUt0W3dWQOrprscmda2Y5pLsLg4GF+pI=
        assert_eq!(h[0], "bhHHL3z2vDgxUt0W3dWQOrprscmda2Y5pLsLg4GF+pI=");
        let csp = csp_oberflaeche(&h);
        assert!(csp.contains("script-src 'self' 'sha256-bhHHL3z2vDgxUt0W3dWQOrprscmda2Y5pLsLg4GF+pI=' 'sha256-"));
        assert!(csp.contains("frame-ancestors 'none'"));
        assert!(!csp.contains("http:") && !csp.contains('*'));
    }

    /// Etappe 3b: Das Thema-Skript im head von `web/src/app.html` setzt hell
    /// oder dunkel vor dem ersten Bild. Der Server lässt es zu, weil er beim
    /// Ausliefern die Prüfsumme jedes eingebetteten Skripts aus index.html
    /// rechnet (`CspCache`), nie mit `'unsafe-inline'`. Dieser Test hält die
    /// geprüfte Fassung fest: Ändert sich das Skript, schlägt er an. Dann die
    /// Änderung ansehen (kein Zugriff auf Daten, kein Laden von außen) und die
    /// neue Prüfsumme aus der Fehlermeldung hier eintragen.
    #[test]
    fn thema_skript_der_vorlage() {
        const THEMA_SKRIPT_SHA256: &str = "aAk5cojiNrmU7EzAVHL2n0Bss/C4g81jNkR0FwkPAos=";
        let pfad = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("web/src/app.html");
        let html = std::fs::read_to_string(&pfad).unwrap();
        let h = inline_skript_hashes(&html);
        assert_eq!(h, [THEMA_SKRIPT_SHA256], "Prüfsumme des Thema-Skripts in app.html hat sich geändert");
        let csp = csp_oberflaeche(&h);
        let script_src = csp.split(';').find(|d| d.trim_start().starts_with("script-src")).unwrap();
        assert_eq!(script_src.trim(), format!("script-src 'self' 'sha256-{THEMA_SKRIPT_SHA256}'"));
        assert!(!script_src.contains("unsafe"), "{script_src}");
        // Das Skript liest nur den Schlüssel und setzt nur data-theme.
        let skript = &html[html.find("<script>").unwrap()..html.find("</script>").unwrap()];
        for verboten in ["fetch", "http", "import", "eval", "innerHTML", "src="] {
            assert!(!skript.contains(verboten), "{verboten} im Thema-Skript");
        }
    }

    #[test]
    fn eigene_adressen() {
        assert_eq!(eigene_hosts(8787), ["127.0.0.1:8787", "localhost:8787", "[::1]:8787"]);
        assert_eq!(eigene_herkuenfte(8788), ["http://127.0.0.1:8788", "http://localhost:8788", "http://[::1]:8788"]);
        assert!(ohne_zeitlimit("/api/events"));
        assert!(!ohne_zeitlimit("/api/projects"));
        assert!(ohne_zeitlimit("/api/vorgaenge/warten"));
        assert!(!ohne_zeitlimit("/api/vorgaenge/wartenx"), "nur genau der Warte-Weg");
        assert!(!ohne_zeitlimit("/api/vorgaenge/warten/x"));
    }

    #[test]
    fn lesend_nur_von_hier() {
        for p in ["/api", "/api/", "/api/pruefung", "/api/projects/x", "/projects/schaufenster/assets/bild.svg", "/projects/a/assets/x/y"] {
            assert!(nur_von_hier(p), "{p}");
        }
        for p in [
            "/",
            "/pc",
            "/projects/schaufenster",
            "/projects/schaufenster/assets",
            "/projects/schaufenster/assets/",
            "/_app/immutable/a.js",
            "/apix",
            "/projects/a/vorschau",
        ] {
            assert!(!nur_von_hier(p), "{p}");
        }
    }

    #[test]
    fn percent_encoding_keeps_only_plain_ascii() {
        assert_eq!(percent_encode("a-b_c.d"), "a-b_c.d");
        assert_eq!(percent_encode("ä b"), "%C3%A4%20b");
    }

    #[test]
    fn saetze_je_status() {
        assert_eq!(satz_fuer(StatusCode::METHOD_NOT_ALLOWED).0, "methode");
        assert_eq!(satz_fuer(StatusCode::BAD_REQUEST).0, "fehlerhaft");
        assert_eq!(satz_fuer(StatusCode::UNPROCESSABLE_ENTITY).0, "unlesbar");
        assert_eq!(satz_fuer(StatusCode::UNSUPPORTED_MEDIA_TYPE).0, "unlesbar");
        assert_eq!(satz_fuer(StatusCode::FORBIDDEN).0, "abgelehnt");
        assert_eq!(satz_fuer(StatusCode::BAD_GATEWAY).0, "intern");
    }
}
