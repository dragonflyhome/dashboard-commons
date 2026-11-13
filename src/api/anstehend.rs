//! „Anstehend“ über die API (Etappe A1):
//!
//! - `GET /api/anstehend?stufe=&schwerpunkt=&art=&projekt=&nur_fuer_mich=`:
//!   die gemeinsame Liste mit Zählern und Schwerpunkten. Geschützt wie
//!   `/api/aufgaben` (Windows-Anmeldung, ohne sie 401): die Liste nennt die
//!   Titel der Aufgaben. Klick-Links stehen nie darin, nur `hat_link`.
//! - `GET /api/projekte/<slug>/stand`: das Unter-Dashboard einer Projektseite
//!   (alte und neue Form). Frei lesbar wie die Projektseiten ([unter_dashboard]
//!   api: „Lesen ist frei“); die Aufgaben zeigt es mit Titel nur, wenn die
//!   Verbindung als Inhaber angemeldet ist, sonst nur ihre Zahl. Der Weg bleibt
//!   für Werkzeuge.
//! - `GET /api/projekte/<slug>/stand/voll` (Hotfix 3a-1): derselbe Stand,
//!   immer mit Titeln, geschützt wie `/api/anstehend`. Die Oberfläche liest
//!   ihn hier: Edge schickt Negotiate nur auf einer Verbindung, die vorher 401
//!   bekam; auf dem freien Weg fehlte die Anmeldung oft zufällig.
//! - `POST /api/einstufung {projekt, art, id, gestellt?, stufe | null}`:
//!   Der Inhaber stuft einen Eintrag um (`null`: zurück zum Vorschlag des Chats).
//!   Nur aus dem Browser wie der Haken bei Aufgaben; per curl 422. Geschützt.
//!
//! - `GET /api/projekte/<slug>/backlog` (Etappe B2): das Backlog des Projekts
//!   (Spalten nach Stand und Status, Wartende je Verantwortlichem,
//!   `crate::backlog`). Geschützt wie `/stand/voll`: es nennt Titel.
//! - `GET /api/fristen` (Etappe B3): die Fristen aller sichtbaren Projekte
//!   (`crate::fristen`), geschützt wie `/api/anstehend`.
//!
//! Beide lesenden Wege rechnen mit derselben Funktion aus einem `Inhalt`
//! (`crate::anstehend`), bei jeder Anfrage neu. Ohne Datenspeicher antworten
//! beide trotzdem, mit dem, was in den Dateien steht (Notizen, Fragen), und
//! `speicher_fehlt` bzw. einem Hinweis; nur das Einstufen gibt dann 503.

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Uri};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::{Deserialize, Deserializer};

use super::aufgaben::stufe_pruefen;
use super::vertrag::{AnstehendListe, Einstufung, EinstufungAntwort, EintragsArt, ProjektStand};
use super::vorgaenge::{abfrage, angegeben, aus_browser, json_kopf_pruefen, schalter, slug_pruefen};
use crate::anmeldung::Angemeldet;
use crate::anstehend::{self, AusDenDateien, Filter, Inhalt};
use crate::app::AppState;
use crate::fehler::AppError;
use crate::inhalt::seite;
use crate::speicher::aufgaben;
use crate::speicher::{Speicher, SpeicherFehler};
use crate::vertrag::abgeleitet::heute_von;
use crate::vertrag::daten::{DatenInhalt, ist_kennung};

pub const SATZ_EINSTUFEN_NUR_BROWSER: &str = "Umstufen kann nur der Inhaber im Browser.";
const SATZ_NICHT_DA: &str = "Diesen Eintrag gibt es nicht (mehr).";
pub const SATZ_MELDUNG_FEST: &str = "Meldungen haben eine feste Stufe; klick sie weg, wenn du sie gelesen hast.";
const SATZ_NEU_GESTELLT: &str = "Diese Frage wurde inzwischen neu gestellt; bitte die Seite neu laden.";
const MUSTER: &str = r#"{"projekt": "beispiel", "art": "frage", "id": "schlusssatz", "gestellt": "2026-10-25", "stufe": "jetzt"} (stufe null: zurück zum Vorschlag des Chats; gestellt nur bei Fragen)"#;

/// Fehler des Datenspeichers: 500 mit Satz, Einzelheiten ins Protokoll.
fn speicherfehler(e: SpeicherFehler) -> AppError {
    tracing::error!("Anstehend: Datenspeicher: {e} ({})", e.einzelheiten());
    AppError::intern(format!("{e} Einzelheiten stehen im Protokoll."))
}

/// Hinweis in der Liste, wenn der Datenspeicher fehlt (hinter dem Satz, der den Grund nennt).
pub const SATZ_OHNE_SPEICHER: &str = "Bis dahin fehlen Aufgaben, Arbeiten und deine Einstufungen; Fragen und Notizen aus den Dateien stehen da.";

/// Liest den Inhalt für eine Anfrage: Datenspeicher (ohne Links) und Dateien,
/// mit dem lokalen Datum von jetzt. Fehlt der Datenspeicher, bleibt sein
/// Teil leer (`speicher_fehlt`, mit Hinweis); die Dateien zählen trotzdem,
/// damit Notizen aus open[] und Fragen nicht verschwinden.
pub async fn inhalt_sammeln(state: &AppState) -> Result<Inhalt, AppError> {
    let jetzt = aufgaben::jetzt();
    let heute = heute_von(&jetzt);
    let aus_speicher = match state.speicher.clone() {
        Some(speicher) => Some(Speicher::im_hintergrund(speicher, move |s| s.anstehend_lesen(&jetzt)).await.map_err(speicherfehler)?),
        None => None,
    };
    // Etappe 2d: aus dem ContentStore. Im Dienst ein Blick in den
    // Arbeitsspeicher; in Tests gleicht er erst mit der Platte ab, darum
    // abseits der Tokio-Fäden.
    let inhalt_store = state.inhalt.clone();
    let dateien = tokio::task::spawn_blocking(move || AusDenDateien::aus_bestand(&inhalt_store.bestand(), heute))
        .await
        .map_err(|e| AppError::intern(format!("Die Dateien ließen sich nicht lesen ({e}).")))?;
    let speicher_fehlt = aus_speicher.is_none();
    let mut inhalt = Inhalt::aus(dateien, aus_speicher.unwrap_or_default(), heute);
    if speicher_fehlt {
        inhalt.speicher_fehlt = true;
        inhalt.hinweise.insert(0, format!("{} {SATZ_OHNE_SPEICHER}", state.speicher_satz()));
    }
    Ok(inhalt)
}

/// Die Filter aus der Abfrage; ein falscher Wert gibt 422 mit Satz.
fn filter_aus(uri: &Uri) -> Result<Filter, AppError> {
    let stufe = angegeben(abfrage(uri, "stufe")).map(|s| stufe_pruefen("stufe", &s)).transpose()?;
    let art = match angegeben(abfrage(uri, "art")) {
        None => None,
        Some(a) => Some(EintragsArt::aus_wort(&a).ok_or_else(|| AppError::unlesbar("„art“ ist aufgabe, frage, arbeit oder meldung."))?),
    };
    let projekt = angegeben(abfrage(uri, "projekt"));
    if let Some(p) = &projekt {
        slug_pruefen(p)?;
    }
    let schwerpunkt = angegeben(abfrage(uri, "schwerpunkt"));
    Ok(Filter { stufe, schwerpunkt, art, projekt, nur_fuer_mich: schalter(uri, "nur_fuer_mich")? })
}

/// `GET /api/anstehend`.
pub async fn liste(State(state): State<AppState>, uri: Uri) -> Result<Json<AnstehendListe>, AppError> {
    let filter = filter_aus(&uri)?;
    let inhalt = inhalt_sammeln(&state).await?;
    Ok(Json(anstehend::anstehend(&inhalt, &filter)))
}

/// `GET /api/projekte/<slug>/stand`: frei; Aufgaben mit Titel nur angemeldet.
pub async fn stand(State(state): State<AppState>, Path(slug): Path<String>, angemeldet: Option<Extension<Angemeldet>>) -> Result<Json<ProjektStand>, AppError> {
    stand_bauen(&state, &slug, angemeldet.is_some()).await.map(Json)
}

/// `GET /api/projekte/<slug>/stand/voll`: geschützt (`sicherheit::geschuetzt`),
/// darum immer mit Titeln. Ohne Anmeldung antwortet schon die Schleuse 401 mit
/// `WWW-Authenticate: Negotiate`; käme doch eine Anfrage ohne durch, dasselbe hier.
pub async fn stand_voll(State(state): State<AppState>, Path(slug): Path<String>, angemeldet: Option<Extension<Angemeldet>>) -> Response {
    if angemeldet.is_none() {
        return super::anmeldung::bitte_anmelden(None);
    }
    match stand_bauen(&state, &slug, true).await {
        Ok(s) => Json(s).into_response(),
        Err(e) => e.into_response(),
    }
}

/// `GET /api/projekte/<slug>/backlog` (Etappe B2): geschützt (`sicherheit::geschuetzt`),
/// ohne Anmeldung 401 wie `/stand/voll`. Liest Datenspeicher und Dateien wie
/// „Anstehend“ und schreibt nichts.
pub async fn backlog(State(state): State<AppState>, Path(slug): Path<String>, angemeldet: Option<Extension<Angemeldet>>) -> Response {
    if angemeldet.is_none() {
        return super::anmeldung::bitte_anmelden(None);
    }
    let antwort = async {
        if !seite::is_valid_slug(&slug) {
            return Err(AppError::nicht_gefunden(format!("Kein Projekt „{slug}“")));
        }
        // Läuft noch eine kanban.toml von Hand? (läuft aus, Etappe B2)
        let kanban_datei = state.inhalt.bestand().projekt(&slug).is_some_and(|p| p.namen.as_ref().is_ok_and(|n| n.iter().any(|x| x == "kanban")));
        let inhalt = inhalt_sammeln(&state).await?;
        // Reihenfolge der Etappen für „Als Nächstes“; eine unlesbare plan.toml meldet /plan.
        let mut reihenfolge = std::collections::BTreeMap::new();
        for p in crate::backlog::etappen_projekte(&inhalt, &slug) {
            if let Ok(Some((DatenInhalt::Plan(d), _))) = state.inhalt.bestand().daten(p, "plan") {
                reihenfolge.extend(d.inhalte().into_iter().flat_map(|t| &t.etappe).map(|e| (format!("{p}/{}", e.kennung), e.reihenfolge)));
            }
        }
        crate::backlog::backlog(&inhalt, &slug, kanban_datei, &reihenfolge).ok_or_else(|| AppError::nicht_gefunden(format!("Kein Projekt „{slug}“")))
    };
    match antwort.await {
        Ok(b) => Json(b).into_response(),
        Err(e) => e.into_response(),
    }
}

/// `GET /api/projekte/<slug>/plan` (Etappe B8): die Etappen aus `daten/plan.toml`
/// mit Stand und Zählung aus dem Tracker (`crate::plan`), auch archivierte
/// Arbeiten. Geschützt wie `/backlog` (nennt Titel), ohne Anmeldung 401.
/// Schreibt nichts. Ohne Datenspeicher oder lesbare `plan.toml` antwortet er
/// mit dem, was da ist, und einem Hinweis.
pub async fn plan(State(state): State<AppState>, Path(slug): Path<String>, angemeldet: Option<Extension<Angemeldet>>) -> Response {
    if angemeldet.is_none() {
        return super::anmeldung::bitte_anmelden(None);
    }
    let antwort = async {
        let bestand = state.inhalt.bestand();
        if !seite::is_valid_slug(&slug) || !bestand.ordner_da(&slug) {
            return Err(AppError::nicht_gefunden(format!("Kein Projekt „{slug}“")));
        }
        let mut hinweise = Vec::new();
        let datei = match bestand.daten(&slug, "plan") {
            Ok(Some((DatenInhalt::Plan(p), _))) => Some(p),
            Ok(_) => None,
            Err(e) => {
                hinweise.push(format!("{e} Bis das behoben ist, fehlen die Etappen."));
                None
            }
        };
        // shortcut: voller Durchlauf durch Arbeiten und Archiv je Abruf, gefiltert nach
        // etappe und Liste; Index oder Zwischenspeicher, sobald das Archiv einige tausend Arbeiten hat.
        let arbeiten = match state.speicher.clone() {
            Some(s) => Speicher::im_hintergrund(s, |s| s.arbeiten_mit_archiv()).await.map_err(speicherfehler)?,
            None => {
                hinweise.push(format!("{} Bis dahin fehlen die Arbeiten.", state.speicher_satz()));
                Vec::new()
            }
        };
        let mut p = crate::plan::plan(&slug, datei.as_ref(), &arbeiten);
        p.hinweise = hinweise;
        Ok(p)
    };
    match antwort.await {
        Ok(p) => Json(p).into_response(),
        Err(e) => e.into_response(),
    }
}

/// `GET /api/projekte/<slug>/verlauf?seit=&bis=` (Etappe B8): die Zeitleiste
/// eines Projekts in derselben Form wie `/api/pc/verlauf`, gebaut von
/// `pc::verlauf_bauen` (ohne `bis`: bis 90 Tage nach heute, für Fristen).
/// Geschützt wie `/plan`, ohne Anmeldung 401; unbekanntes Projekt 404. Schreibt nichts.
pub async fn verlauf(State(state): State<AppState>, Path(slug): Path<String>, uri: Uri, angemeldet: Option<Extension<Angemeldet>>) -> Response {
    if angemeldet.is_none() {
        return super::anmeldung::bitte_anmelden(None);
    }
    if !seite::is_valid_slug(&slug) || !state.inhalt.bestand().ordner_da(&slug) {
        return AppError::nicht_gefunden(format!("Kein Projekt „{slug}“")).into_response();
    }
    match super::pc::verlauf_bauen(&state, &uri, super::pc::Umfang::Projekt(&slug)).await {
        // Meldungstexte und Hinweise ohne Profil und Namen (`datenschutz`).
        Ok(v) => crate::datenschutz::als_json(&v).into_response(),
        Err(e) => e.into_response(),
    }
}

/// `GET /api/fristen` (Etappe B3): die Fristenliste aller sichtbaren Projekte
/// (ohne archivierte; für die gilt die Sicherheitsregel in Anstehend), nach
/// Datum, bewegliche ohne Datum am Ende (`crate::fristen::fristenliste`).
/// Geschützt (`sicherheit::geschuetzt`), ohne Anmeldung 401: die Liste nennt
/// Titel. Schreibt nichts.
pub async fn fristen(State(state): State<AppState>, angemeldet: Option<Extension<Angemeldet>>) -> Response {
    if angemeldet.is_none() {
        return super::anmeldung::bitte_anmelden(None);
    }
    match inhalt_sammeln(&state).await {
        Ok(i) => Json(crate::fristen::fristenliste(&i, None)).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn stand_bauen(state: &AppState, slug: &str, aufgaben_sichtbar: bool) -> Result<ProjektStand, AppError> {
    if !seite::is_valid_slug(slug) {
        return Err(AppError::nicht_gefunden(format!("Kein Projekt „{slug}“")));
    }
    let inhalt = inhalt_sammeln(state).await?;
    anstehend::stand(&inhalt, slug, aufgaben_sichtbar).ok_or_else(|| AppError::nicht_gefunden(format!("Kein Projekt „{slug}“")))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EinstufungKoerper {
    projekt: String,
    art: String,
    id: String,
    #[serde(default)]
    gestellt: Option<String>,
    /// Pflicht: `null` heißt zurück zum Vorschlag; fehlt das Feld, 422.
    #[serde(default, deserialize_with = "doppelt")]
    stufe: Option<Option<String>>,
}

fn doppelt<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

/// `POST /api/einstufung`: die Einstufung des Inhabers speichern. Der Eintrag muss
/// es geben (Aufgabe oder Arbeit im Datenspeicher, Frage in fragen.toml).
pub async fn einstufen(State(state): State<AppState>, kopf: HeaderMap, koerper: Bytes) -> Result<Json<EinstufungAntwort>, AppError> {
    json_kopf_pruefen(&kopf)?;
    let k: EinstufungKoerper = serde_json::from_slice(&koerper).map_err(|e| {
        tracing::debug!("Einstufung: Körper unlesbar ({:?}, Zeile {}, Spalte {})", e.classify(), e.line(), e.column());
        AppError::unlesbar(format!("Der Inhalt ist kein passendes JSON. Erwartet wird etwa {MUSTER}."))
    })?;
    if !aus_browser(&kopf) {
        return Err(AppError::unlesbar(SATZ_EINSTUFEN_NUR_BROWSER));
    }
    slug_pruefen(&k.projekt)?;
    let art = EintragsArt::aus_wort(&k.art).ok_or_else(|| AppError::unlesbar("„art“ ist aufgabe, frage oder arbeit."))?;
    if art == EintragsArt::Meldung {
        return Err(AppError::unlesbar(SATZ_MELDUNG_FEST));
    }
    if !ist_kennung(&k.id) {
        return Err(AppError::unlesbar("„id“ ist die Kennung des Eintrags, etwa a-20261029-0001 oder schlusssatz."));
    }
    let Some(stufe) = k.stufe else {
        return Err(AppError::unlesbar("„stufe“ fehlt: jetzt, bald, irgendwann oder null für den Vorschlag des Chats."));
    };
    let stufe = angegeben(stufe).map(|s| stufe_pruefen("stufe", &s)).transpose()?;
    let gestellt = angegeben(k.gestellt);
    let jetzt = aufgaben::jetzt();
    let gestellt = match art {
        EintragsArt::Frage => Some(frage_gestellt(&state, &k.projekt, &k.id, gestellt.as_deref())?),
        _ if gestellt.is_some() => return Err(AppError::unlesbar("„gestellt“ gibt es nur bei Fragen.")),
        EintragsArt::Aufgabe => {
            let (id, projekt) = (k.id.clone(), k.projekt.clone());
            // Nur ob es sie gibt und wohin sie gehört: ohne den Klick-Link zu entschlüsseln.
            let da = Speicher::im_hintergrund(state.speicher()?, move |s| Ok(s.aufgabe_ohne_link(&id)?.is_some_and(|a| a.projekt == projekt)))
                .await
                .map_err(speicherfehler)?;
            if !da {
                return Err(AppError::nicht_gefunden(SATZ_NICHT_DA));
            }
            None
        }
        EintragsArt::Meldung => return Err(AppError::unlesbar(SATZ_MELDUNG_FEST)),
        EintragsArt::Arbeit => {
            let (id, projekt) = (k.id.clone(), k.projekt.clone());
            let da =
                Speicher::im_hintergrund(state.speicher()?, move |s| Ok(s.arbeit(&id)?.is_some_and(|a| a.projekt == projekt))).await.map_err(speicherfehler)?;
            if !da {
                return Err(AppError::nicht_gefunden(SATZ_NICHT_DA));
            }
            None
        }
    };
    let e = Einstufung { projekt: k.projekt, art, id: k.id, gestellt, stufe, zeit: jetzt.to_rfc3339_opts(chrono::SecondsFormat::Secs, false) };
    let e2 = e.clone();
    let geaendert = Speicher::im_hintergrund(state.speicher()?, move |s| s.einstufung_setzen(&e2)).await.map_err(speicherfehler)?;
    if geaendert {
        tracing::info!("Einstufung {}/{}/{}: {}", e.projekt, art.wort(), e.id, e.stufe.map_or("Vorschlag", |s| s.wort()));
        super::events::anstehend_geaendert(&state.events, Some(&e.projekt));
    }
    Ok(Json(EinstufungAntwort { einstufung: e, geaendert }))
}

/// `gestellt` einer Frage aus `projects/<projekt>/daten/fragen.toml`. Fehlt
/// sie: 404; passt das mitgeschickte `gestellt` nicht: 422.
fn frage_gestellt(state: &AppState, projekt: &str, id: &str, gestellt: Option<&str>) -> Result<String, AppError> {
    let gefunden = match state.inhalt.bestand().daten(projekt, "fragen") {
        Ok(Some((DatenInhalt::Fragen(d), _))) => d.inhalte().into_iter().flat_map(|t| t.frage.iter()).find(|f| f.id == id).map(|f| f.gestellt.clone()),
        Ok(_) => None,
        Err(e) => return Err(AppError::unlesbar(e.to_string())),
    };
    let Some(datum) = gefunden else { return Err(AppError::nicht_gefunden(SATZ_NICHT_DA)) };
    match gestellt {
        Some(g) if g != datum => Err(AppError::unlesbar(SATZ_NEU_GESTELLT)),
        _ => Ok(datum),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;
    use crate::app::router_angemeldet as router;

    /// Erfundener Testwert, kein echter Link.
    const LINK: &str = "https://anmeldung.beispiel.invalid/einmal?token=erfunden-a1-geheim";
    const GEHEIM: &str = "erfunden-a1-geheim";

    fn beispiel() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel")
    }

    fn zustand() -> AppState {
        AppState::fuer_tests(&beispiel())
    }

    fn get(pfad: &str) -> Request<Body> {
        Request::get(pfad).header("host", "127.0.0.1:8787").body(Body::empty()).unwrap()
    }

    fn post(pfad: &str, koerper: &str) -> Request<Body> {
        Request::post(pfad).header("host", "127.0.0.1:8787").header("content-type", "application/json").body(Body::from(koerper.to_string())).unwrap()
    }

    fn post_browser(pfad: &str, koerper: &str) -> Request<Body> {
        Request::post(pfad)
            .header("host", "localhost:8787")
            .header("origin", "http://localhost:8787")
            .header("sec-fetch-site", "same-origin")
            .header("content-type", "application/json")
            .body(Body::from(koerper.to_string()))
            .unwrap()
    }

    async fn roh_mit(app: axum::Router, req: Request<Body>) -> (StatusCode, String) {
        let res = app.oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    async fn schicke(state: &AppState, req: Request<Body>) -> (StatusCode, Value) {
        let (status, text) = roh_mit(router(state.clone()), req).await;
        (status, serde_json::from_str(&text).unwrap_or_else(|e| panic!("kein JSON ({e}): {text}")))
    }

    async fn angelegt(state: &AppState, pfad: &str, koerper: &str) -> String {
        let (status, v) = schicke(state, post(pfad, koerper)).await;
        assert_eq!(status, StatusCode::CREATED, "{v}");
        v["id"].as_str().unwrap().to_string()
    }

    fn ids(v: &Value) -> Vec<String> {
        v["eintraege"].as_array().unwrap().iter().map(|e| e["id"].as_str().unwrap().to_string()).collect()
    }

    fn eintrag<'a>(v: &'a Value, id: &str) -> &'a Value {
        v["eintraege"].as_array().unwrap().iter().find(|e| e["id"] == id).unwrap_or_else(|| panic!("{id} fehlt: {v}"))
    }

    /// Die Beispiel-Einträge der Abnahme: eine Aufgabe (dashboard, jetzt, mit
    /// Link), eine Arbeit (neu-form, irgendwann); dazu stehen in
    /// tests/beispiel/projects/neu-form/daten/fragen.toml zwei Fragen (bald).
    async fn abnahme_eintraege(state: &AppState) -> (String, String) {
        let a = angelegt(
            state,
            "/api/aufgaben",
            &format!(r#"{{"projekt": "dashboard", "titel": "Beim Probe-Dienst anmelden", "link": "{LINK}", "prioritaet": "jetzt"}}"#),
        )
        .await;
        let r = angelegt(
            state,
            "/api/arbeiten",
            r#"{"projekt": "neu-form", "von": "chat:neu-form", "titel": "Seite in neuer Form umbauen", "stand": "laeuft", "prioritaet": "irgendwann"}"#,
        )
        .await;
        (a, r)
    }

    /// Abnahme A1: Ein Chat legt eine Aufgabe und eine Arbeit an, zwei Fragen
    /// stehen in fragen.toml; alle stehen in /api/anstehend, mit Stufe,
    /// Schwerpunkt, Ziel und ohne Klick-Link. Etappe 3a+: alle Fragen, in der
    /// neutralen Ordnung Projekt, Art, Kennung, ohne Zähler.
    #[tokio::test]
    async fn abnahme_alle_arten_nach_prioritaet_und_schwerpunkt() {
        let state = zustand();
        let (a, r) = abnahme_eintraege(&state).await;
        let (status, text) = roh_mit(router(state.clone()), get("/api/anstehend")).await;
        assert_eq!(status, StatusCode::OK, "{text}");
        // Kein Klick-Link in der Liste, nur hat_link.
        assert!(!text.contains(GEHEIM) && !text.contains("beispiel.invalid"), "Link in der Liste: {text}");
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(ids(&v), [a.clone(), "konzept".into(), "schlusssatz".into(), r.clone()]);
        assert_eq!(eintrag(&v, "schlusssatz")["vermerk"], Value::Null);
        let ea = eintrag(&v, &a);
        assert!(ea.get("link").is_none(), "{ea}");
        assert_eq!((ea["hat_link"].as_bool(), ea["art"].as_str(), ea["zustand"].as_str()), (Some(true), Some("aufgabe"), Some("offen")));
        assert_eq!((ea["stufe"]["wirksam"].as_str(), ea["stufe"]["grund"].as_str()), (Some("jetzt"), Some("vorschlag")));
        assert_eq!((ea["schwerpunkt"].as_str(), ea["quelle"].as_str()), (Some("Bienen und Honig"), Some("datenspeicher:aufgaben")));
        // Ziele: immer eine Adresse im Dashboard.
        assert_eq!(ea["ziel"], format!("/projects/dashboard#aufgabe-{a}"));
        assert_eq!(eintrag(&v, "schlusssatz")["ziel"], "/projects/neu-form#frage-schlusssatz");
        assert_eq!(eintrag(&v, &r)["ziel"], format!("/projects/neu-form#arbeit-{r}"));
        for e in v["eintraege"].as_array().unwrap() {
            assert!(e["ziel"].as_str().unwrap().starts_with("/projects/") && e["ziel_da"] == true, "{e}");
        }
        let f = eintrag(&v, "schlusssatz");
        assert_eq!(
            (f["gestellt"].as_str(), f["quelle"].as_str(), f["wartet_auf"].as_str()),
            (Some("2026-10-25"), Some("projects/neu-form/daten/fragen.toml"), Some("inhaber"))
        );
        assert_eq!((f["schwerpunkt"].as_str(), f["wartet_auf_dich"].as_bool()), (Some("Chor und Verein"), Some(true)));
        let reihenfolge = |id: &str| eintrag(&v, id)["schwerpunkt_reihenfolge"].as_i64().unwrap();
        assert!(reihenfolge("schlusssatz") < reihenfolge(&a), "Chor und Verein vor Bienen und Honig");
        assert_eq!(eintrag(&v, &r)["wartet_auf_dich"], false);
        assert!(v.get("zaehler").is_none() && v.get("schwerpunkte").is_none(), "{v}");
        // Nach Schwerpunkt.
        let (_, s) = schicke(&state, get("/api/anstehend?schwerpunkt=Chor%20und%20Verein")).await;
        assert_eq!(ids(&s), ["konzept".to_string(), "schlusssatz".into(), r.clone()]);
        let (_, s) = schicke(&state, get("/api/anstehend?schwerpunkt=Bienen+und+Honig")).await;
        assert_eq!(ids(&s), [a.as_str()]);
        // Weitere Filter.
        let (_, s) = schicke(&state, get("/api/anstehend?stufe=bald")).await;
        assert_eq!(ids(&s), ["konzept", "schlusssatz"]);
        let (_, s) = schicke(&state, get("/api/anstehend?art=arbeit&projekt=neu-form")).await;
        assert_eq!(ids(&s), [r.as_str()]);
        let (_, s) = schicke(&state, get("/api/anstehend?nur_fuer_mich=1")).await;
        assert_eq!(ids(&s), [a.clone(), "konzept".into(), "schlusssatz".into()]);
        for falsch in ["/api/anstehend?stufe=sofort", "/api/anstehend?art=notiz", "/api/anstehend?projekt=Gross", "/api/anstehend?nur_fuer_mich=ja"] {
            assert_eq!(schicke(&state, get(falsch)).await.0, StatusCode::UNPROCESSABLE_ENTITY, "{falsch}");
        }
        // Der Datenspeicher liefert die Liste, ohne einen Link zu entschlüsseln.
        let vorher = crate::speicher::verschluesselung::entschluesselt_in_diesem_thread();
        let aus = state.speicher().unwrap().anstehend_lesen(&aufgaben::jetzt()).unwrap();
        assert_eq!(aus.aufgaben.len(), 1);
        assert_eq!(crate::speicher::verschluesselung::entschluesselt_in_diesem_thread(), vorher);
    }

    /// Einstufen nur aus dem Browser (curl 422, nichts gespeichert); die
    /// Reihenfolge ändert sich; „zurück zum Vorschlag“ (null) stellt sie wieder her.
    #[tokio::test]
    async fn einstufung_nur_im_browser_und_zurueck_zum_vorschlag() {
        let state = zustand();
        let (a, r) = abnahme_eintraege(&state).await;
        let mut changes = state.events.subscribe();
        let jetzt_koerper = r#"{"projekt": "neu-form", "art": "frage", "id": "konzept", "stufe": "jetzt"}"#;
        let (status, v) = schicke(&state, post("/api/einstufung", jetzt_koerper)).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(SATZ_EINSTUFEN_NUR_BROWSER)));
        assert!(state.speicher().unwrap().einstufungen_lesen().unwrap().is_empty(), "curl darf nicht einstufen");
        let (status, v) = schicke(&state, post_browser("/api/einstufung", jetzt_koerper)).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(
            (v["geaendert"].as_bool(), v["einstufung"]["gestellt"].as_str(), v["einstufung"]["stufe"].as_str()),
            (Some(true), Some("2026-10-26"), Some("jetzt"))
        );
        let paths: Vec<String> = std::iter::from_fn(|| changes.try_recv().ok()).filter(|c| c.area == "anstehend").map(|c| c.path).collect();
        assert_eq!(paths, ["anstehend/neu-form"]);
        let (_, l) = schicke(&state, get("/api/anstehend")).await;
        // Der Inhaber stuft „konzept“ hoch: die Stufe ändert sich, die Ordnung bleibt neutral.
        assert_eq!(ids(&l), [a.clone(), "konzept".into(), "schlusssatz".into(), r.clone()]);
        let k = eintrag(&l, "konzept");
        assert_eq!(k["stufe"]["wirksam"], "jetzt");
        assert_eq!(
            (k["stufe"]["vorschlag"].as_str(), k["stufe"]["inhaber"].as_str(), k["stufe"]["grund"].as_str()),
            (Some("bald"), Some("jetzt"), Some("inhaber"))
        );
        // Dasselbe noch einmal: nichts geändert.
        let (_, v) = schicke(&state, post_browser("/api/einstufung", jetzt_koerper)).await;
        assert_eq!(v["geaendert"].as_bool(), Some(false));
        // Zurück zum Vorschlag des Chats.
        let (status, v) = schicke(
            &state,
            post_browser("/api/einstufung", r#"{"projekt": "neu-form", "art": "frage", "id": "konzept", "gestellt": "2026-10-26", "stufe": null}"#),
        )
        .await;
        assert_eq!((status, v["geaendert"].as_bool(), v["einstufung"]["stufe"].clone()), (StatusCode::OK, Some(true), Value::Null));
        let (_, l) = schicke(&state, get("/api/anstehend")).await;
        // Zurück beim Vorschlag: wieder bald.
        assert_eq!((eintrag(&l, "konzept")["stufe"]["wirksam"].as_str(), eintrag(&l, "konzept")["stufe"]["inhaber"].clone()), (Some("bald"), Value::Null));
        // Aufgabe und Arbeit lassen sich auch umstufen.
        let (status, _) =
            schicke(&state, post_browser("/api/einstufung", &format!(r#"{{"projekt": "neu-form", "art": "arbeit", "id": "{r}", "stufe": "jetzt"}}"#))).await;
        assert_eq!(status, StatusCode::OK);
        let (status, _) =
            schicke(&state, post_browser("/api/einstufung", &format!(r#"{{"projekt": "dashboard", "art": "aufgabe", "id": "{a}", "stufe": "irgendwann"}}"#)))
                .await;
        assert_eq!(status, StatusCode::OK);
        let (_, l) = schicke(&state, get("/api/anstehend")).await;
        assert_eq!((eintrag(&l, &r)["stufe"]["wirksam"].as_str(), eintrag(&l, &a)["stufe"]["wirksam"].as_str()), (Some("jetzt"), Some("irgendwann")));
        // Falsches: Stufe fehlt, unbekannte Stufe, Eintrag fehlt, falsches Projekt, alte Frage, gestellt bei Aufgaben.
        let faelle: Vec<(String, StatusCode, Option<&str>)> = vec![
            (r#"{"projekt": "neu-form", "art": "frage", "id": "konzept"}"#.into(), StatusCode::UNPROCESSABLE_ENTITY, None),
            (r#"{"projekt": "neu-form", "art": "frage", "id": "konzept", "stufe": "sofort"}"#.into(), StatusCode::UNPROCESSABLE_ENTITY, None),
            (r#"{"projekt": "neu-form", "art": "frage", "id": "gibt-es-nicht", "stufe": "bald"}"#.into(), StatusCode::NOT_FOUND, Some(SATZ_NICHT_DA)),
            (r#"{"projekt": "neu-form", "art": "meldung", "id": "x", "stufe": "bald"}"#.into(), StatusCode::UNPROCESSABLE_ENTITY, None),
            (
                r#"{"projekt": "neu-form", "art": "frage", "id": "konzept", "gestellt": "2026-10-01", "stufe": "bald"}"#.into(),
                StatusCode::UNPROCESSABLE_ENTITY,
                Some(SATZ_NEU_GESTELLT),
            ),
            (format!(r#"{{"projekt": "beispiel", "art": "aufgabe", "id": "{a}", "stufe": "bald"}}"#), StatusCode::NOT_FOUND, Some(SATZ_NICHT_DA)),
            (
                format!(r#"{{"projekt": "dashboard", "art": "aufgabe", "id": "{a}", "gestellt": "2026-10-01", "stufe": "bald"}}"#),
                StatusCode::UNPROCESSABLE_ENTITY,
                None,
            ),
            (
                "{\"projekt\": \"neu-form\", \"art\": \"arbeit\", \"id\": \"ar-19990101-0001\", \"stufe\": \"bald\"}".into(),
                StatusCode::NOT_FOUND,
                Some(SATZ_NICHT_DA),
            ),
        ];
        for (koerper, soll, satz) in &faelle {
            let (status, v) = schicke(&state, post_browser("/api/einstufung", koerper)).await;
            assert_eq!(status, *soll, "{koerper}: {v}");
            if let Some(s) = satz {
                assert_eq!(v["fehler"].as_str(), Some(*s), "{koerper}");
            }
        }
        assert_eq!(state.speicher().unwrap().einstufungen_lesen().unwrap().len(), 3);
    }

    /// Abnahme A1: Der Inhaber stuft um, die Reihenfolge ändert sich und bleibt
    /// nach dem Neuöffnen des Datenspeichers (wie nach einem Neustart).
    #[tokio::test]
    async fn umstufung_bleibt_nach_neuoeffnen() {
        let pfad = crate::speicher::test_pfad();
        let mit_datei = || {
            let mut state = AppState::fuer_tests_ohne_speicher(&beispiel(), "");
            state.speicher = Some(Arc::new(Speicher::oeffnen(&pfad).unwrap()));
            state
        };
        let state = mit_datei();
        let a = angelegt(&state, "/api/aufgaben", r#"{"projekt": "dashboard", "titel": "Irgendwann ansehen", "prioritaet": "irgendwann"}"#).await;
        let (_, l) = schicke(&state, get("/api/anstehend")).await;
        assert_eq!(eintrag(&l, &a)["stufe"]["wirksam"], "irgendwann");
        let (status, _) =
            schicke(&state, post_browser("/api/einstufung", &format!(r#"{{"projekt": "dashboard", "art": "aufgabe", "id": "{a}", "stufe": "jetzt"}}"#))).await;
        assert_eq!(status, StatusCode::OK);
        let (_, l) = schicke(&state, get("/api/anstehend")).await;
        assert_eq!(eintrag(&l, &a)["stufe"]["wirksam"], "jetzt", "sofort jetzt");
        drop(state);
        let state = mit_datei();
        let (_, l) = schicke(&state, get("/api/anstehend")).await;
        assert_eq!(eintrag(&l, &a)["stufe"]["wirksam"], "jetzt", "nach dem Neuöffnen noch jetzt");
        assert_eq!(eintrag(&l, &a)["stufe"]["inhaber"], "jetzt");
        drop(state);
        std::fs::remove_file(&pfad).unwrap();
    }

    /// Frist heute heißt jetzt, Frist morgen mindestens bald (mit dem echten Datum).
    #[tokio::test]
    async fn frist_hebt_die_stufe() {
        let state = zustand();
        let heute = heute_von(&aufgaben::jetzt());
        let morgen = (heute + chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
        let heute = heute.format("%Y-%m-%d").to_string();
        let h = angelegt(
            &state,
            "/api/arbeiten",
            &format!(
                r#"{{"projekt": "dashboard", "von": "chat:dashboard", "titel": "heute", "stand": "laeuft", "prioritaet": "irgendwann", "frist": "{heute}"}}"#
            ),
        )
        .await;
        let m = angelegt(
            &state,
            "/api/arbeiten",
            &format!(
                r#"{{"projekt": "dashboard", "von": "chat:dashboard", "titel": "morgen", "stand": "laeuft", "prioritaet": "irgendwann", "frist": "{morgen}"}}"#
            ),
        )
        .await;
        let (_, l) = schicke(&state, get("/api/anstehend?projekt=dashboard")).await;
        assert_eq!(ids(&l).len(), 2);
        let eh = eintrag(&l, &h);
        assert_eq!((eh["stufe"]["wirksam"].as_str(), eh["stufe"]["grund"].as_str(), eh["tage_bis"].as_i64()), (Some("jetzt"), Some("frist"), Some(0)));
        let em = eintrag(&l, &m);
        assert_eq!((em["stufe"]["wirksam"].as_str(), em["stufe"]["grund"].as_str(), em["tage_bis"].as_i64()), (Some("bald"), Some("frist"), Some(1)));
        // Etappe 3a-4: die Frist-Stufe je Eintrag, die Schwelle als Fakt.
        assert_eq!((eh["frist_stufe"].as_str(), em["frist_stufe"].as_str()), (Some("heute"), Some("bald")));
        assert_eq!(l["frist_bald_tage"], 7);
        // Auch die Einstufung „irgendwann“ des Inhabers hält die Frist nicht auf.
        schicke(&state, post_browser("/api/einstufung", &format!(r#"{{"projekt": "dashboard", "art": "arbeit", "id": "{h}", "stufe": "irgendwann"}}"#))).await;
        let (_, l) = schicke(&state, get("/api/anstehend?projekt=dashboard")).await;
        assert_eq!(eintrag(&l, &h)["stufe"]["wirksam"], "jetzt");
    }

    /// Unter-Dashboard: frei lesbar, Aufgaben ohne Anmeldung nur als Zahl;
    /// unbekannt 404.
    #[tokio::test]
    async fn stand_frei_aufgaben_nur_mit_anmeldung() {
        let state = zustand();
        let a = angelegt(&state, "/api/aufgaben", &format!(r#"{{"projekt": "dashboard", "titel": "Beim Probe-Dienst anmelden", "link": "{LINK}"}}"#)).await;
        angelegt(&state, "/api/arbeiten", r#"{"projekt": "dashboard", "von": "chat:dashboard", "titel": "Server bauen", "stand": "laeuft"}"#).await;
        angelegt(
            &state,
            "/api/arbeiten",
            &format!(r#"{{"projekt": "dashboard", "von": "chat:dashboard", "titel": "Wartet auf den Haken", "stand": "wartet", "wartet_auf": "inhaber", "verweis": "{a}"}}"#),
        )
        .await;
        // Ohne Anmeldung (Router ohne angemeldete Verbindung): 200, nur die Zahl.
        let (status, text) = roh_mit(crate::app::router(state.clone()), get("/api/projekte/dashboard/stand")).await;
        assert_eq!(status, StatusCode::OK, "{text}");
        assert!(!text.contains("Probe-Dienst") && !text.contains(GEHEIM), "{text}");
        let v: Value = serde_json::from_str(&text).unwrap();
        // Etappe 3a+: ohne Anmeldung keine Einträge, nur die Zahlen als Fakt.
        assert_eq!(
            (v["sichtbar"].as_bool(), v["eintraege"].clone(), v["aktualisierungen"].clone()),
            (Some(false), serde_json::json!([]), serde_json::json!([]))
        );
        assert_eq!((v.get("form"), v["schwerpunkt"].as_str()), (None, Some("Bienen und Honig")));
        assert_eq!(
            (v["gesamt"]["ampel"].as_str(), v["gesamt"]["grund"].as_str(), v["gesamt"]["status"].as_str()),
            (Some("gelb"), Some("wartet"), Some("aktiv"))
        );
        // Etappe B2: open[] ohne Anmeldung nur als Zahl.
        assert_eq!((v["notizen"].clone(), v["notizen_zahl"].as_u64()), (serde_json::json!([]), Some(2)));
        let z = &v["zahlen"];
        assert_eq!(
            (z["aufgaben"].as_u64(), z["laeuft"].as_u64(), z["wartet"].as_u64(), z["wartet_auf_dich"].as_u64()),
            (Some(1), Some(1), Some(1), Some(1)),
            "die Arbeit mit Verweis zählt nicht doppelt"
        );
        // Angemeldet: mit Titel, ohne Link.
        let (status, text) = roh_mit(router(state.clone()), get("/api/projekte/dashboard/stand")).await;
        assert_eq!(status, StatusCode::OK);
        assert!(!text.contains(GEHEIM), "{text}");
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!((v["sichtbar"].as_bool(), v["zahlen"].clone()), (Some(true), Value::Null), "mit Anmeldung zählt die Oberfläche");
        let ea = eintrag(&v, &a);
        assert_eq!(ea["titel"], "Beim Probe-Dienst anmelden");
        assert_eq!(ea["ziel"], format!("/projects/dashboard#aufgabe-{a}"));
        let wartend = v["eintraege"].as_array().unwrap().iter().find(|e| e["zustand"] == "wartet").unwrap();
        assert_eq!((wartend["vermerk"].as_str(), wartend["wartet_auf_dich"].as_bool()), (Some("wartet auf deine Antwort"), Some(false)));
        // Hotfix 3a-1: /stand/voll ist geschützt (401 mit Negotiate), angemeldet derselbe Stand.
        let (status, voll) = roh_mit(router(state.clone()), get("/api/projekte/dashboard/stand/voll")).await;
        assert_eq!(status, StatusCode::OK, "{voll}");
        assert_eq!(voll, text, "angemeldet zeigen beide Wege dasselbe");
        let res = crate::app::router(state.clone()).oneshot(get("/api/projekte/dashboard/stand/voll")).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(res.headers().get("www-authenticate").and_then(|v| v.to_str().ok()), Some("Negotiate"));
        let (status, _) = roh_mit(router(state.clone()), get("/api/projekte/gibt-es-nicht/stand/voll")).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        // Neue Form: eine Frage wartet auf den Inhaber (eine auf einmal), die Frage selbst kommt mit A2.
        // Etappe B8: Sie ist von 2026-10-25, wartet also seit über drei Tagen: rot.
        let (_, v) = schicke(&state, get("/api/projekte/neu-form/stand")).await;
        assert_eq!(
            (v["gesamt"]["ampel"].as_str(), v["gesamt"]["grund"].as_str(), v["eintraege"].as_array().unwrap().len()),
            (Some("rot"), Some("wartet-lange"), 2)
        );
        let (_, frei) = roh_mit(crate::app::router(state.clone()), get("/api/projekte/neu-form/stand")).await;
        let frei: Value = serde_json::from_str(&frei).unwrap();
        assert_eq!((frei["zahlen"]["fragen"].as_u64(), frei["zahlen"]["wartet_auf_dich"].as_u64()), (Some(2), Some(1)), "je Projekt zählt eine Frage");
        assert_eq!(v["notizen"], serde_json::json!(["Erster offener Punkt"]));
        // Ohne Einträge: grün, nur der Gesamtstatus.
        let (_, v) = schicke(&state, get("/api/projekte/schaufenster/stand")).await;
        assert_eq!((v["gesamt"]["ampel"].as_str(), v["gesamt"]["grund"].as_str()), (Some("gruen"), Some("nichts-wartet")));
        for pfad in ["/api/projekte/gibt-es-nicht/stand", "/api/projekte/Gross/stand"] {
            assert_eq!(schicke(&state, get(pfad)).await.0, StatusCode::NOT_FOUND, "{pfad}");
        }
    }

    /// Liste und Einstufung nur mit Windows-Anmeldung (401).
    #[tokio::test]
    async fn liste_und_einstufung_geschuetzt() {
        let state = zustand();
        for req in [get("/api/anstehend"), post_browser("/api/einstufung", r#"{"projekt": "x", "art": "frage", "id": "x", "stufe": null}"#)] {
            let pfad = req.uri().to_string();
            let res = crate::app::router(state.clone()).oneshot(req).await.unwrap();
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{pfad}");
        }
    }

    /// Ohne Datenspeicher gehen Notizen (open[]) und Fragen nicht verloren:
    /// /stand und /api/anstehend antworten mit dem, was in den Dateien steht,
    /// Aufgaben und Arbeiten bleiben leer. Nur das Einstufen gibt 503.
    #[tokio::test]
    async fn ohne_speicher_notizen_und_fragen_aus_den_dateien() {
        let ohne = AppState::fuer_tests_ohne_speicher(&beispiel(), "kaputt");
        // Frei lesbar, auch ohne Anmeldung.
        let (status, text) = roh_mit(crate::app::router(ohne.clone()), get("/api/projekte/dashboard/stand")).await;
        assert_eq!(status, StatusCode::OK, "{text}");
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["speicher_fehlt"], true);
        // Etappe B2: ohne Anmeldung nur die Zahl der Notizen; angemeldet die Texte.
        assert_eq!((v["notizen"].clone(), v["notizen_zahl"].as_u64()), (serde_json::json!([]), Some(2)));
        let (_, mit) = schicke(&ohne, get("/api/projekte/dashboard/stand")).await;
        assert_eq!(mit["notizen"], serde_json::json!(["Erster offener Punkt", "Zweiter offener Punkt"]));
        assert_eq!(v["zahlen"]["aufgaben"], 0);
        // Ohne Datenspeicher: unbekannt statt grün (Auflage aus A1).
        assert_eq!((v["zahlen"]["laeuft"].as_u64(), v["gesamt"]["ampel"].as_str()), (Some(0), Some("unbekannt")));
        assert_eq!(v["gesamt"]["grund"], "speicher-fehlt");
        assert!(!text.contains("kaputt"), "/stand nennt den Grund nicht: {text}");
        // Neue Form: die Frage aus fragen.toml zählt weiter.
        let (status, v) = schicke(&ohne, get("/api/projekte/neu-form/stand")).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        // Etappe B8: Die Frage wartet seit über drei Tagen (rot), auch ohne Datenspeicher.
        assert_eq!((v["speicher_fehlt"].as_bool(), v["eintraege"].as_array().unwrap().len(), v["gesamt"]["ampel"].as_str()), (Some(true), 2, Some("rot")));
        assert_eq!(v["notizen"], serde_json::json!(["Erster offener Punkt"]));
        // Die Liste: nur die Fragen, mit Hinweis vorn.
        let (status, l) = schicke(&ohne, get("/api/anstehend")).await;
        assert_eq!(status, StatusCode::OK, "{l}");
        assert_eq!(ids(&l), ["konzept", "schlusssatz"]);
        let hinweis = l["hinweise"][0].as_str().unwrap();
        assert!(hinweis.contains("Datenspeicher") && hinweis.ends_with(SATZ_OHNE_SPEICHER), "{hinweis}");
        // Mit Speicher: kein Hinweis, speicher_fehlt false.
        let (_, v) = schicke(&zustand(), get("/api/projekte/dashboard/stand")).await;
        assert_eq!(v["speicher_fehlt"], false);
        // Einstufen braucht den Datenspeicher.
        let (status, v) =
            schicke(&ohne, post_browser("/api/einstufung", r#"{"projekt": "neu-form", "art": "frage", "id": "konzept", "stufe": "jetzt"}"#)).await;
        assert_eq!((status, v["art"].as_str()), (StatusCode::SERVICE_UNAVAILABLE, Some("speicher-fehlt")));
    }
}
