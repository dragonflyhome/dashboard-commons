//! Allgemeine Endpunkte für Projekte in neuer Form (Etappe 4v), nur lesend:
//!
//! - `GET /api/projekte/<slug>`: Kopf aus `projekt.toml` und die Namen in `daten/`.
//! - `GET /api/projekte/<slug>/daten/<name>`: eine Datei aus `daten/`,
//!   geprüft mit dem Vertrag, und was der Server daraus rechnet
//!   (`vertrag::abgeleitet`, bei jeder Anfrage neu, mit dem lokalen Datum).
//!
//! Gelesen wird aus dem ContentStore (Etappe 2d, `state.inhalt`): dieselben
//! Funktionen wie vorher, nur einmal je Änderung statt je Anfrage. Das
//! Abgeleitete rechnet weiter jede Anfrage mit dem Datum von heute.
//!
//! Wie alle `/api`-Wege frei lesbar (keine Anmeldepflicht), aber hinter der
//! Herkunftsprüfung. Fehler: 404 `nicht-gefunden`; bricht eine Datei den
//! Vertrag, 422 mit `art = "vertrag"`, Datei, Zeile und Satz
//! (`vertrag::api::VertragsFehler`).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};

use super::vertrag::{DatenAntwort, HalloOrt, ProjektAntwort, VertragsFehler};
use crate::anmeldung::Angemeldet;
use crate::app::AppState;
use crate::fehler::{AppError, FehlerSatz};
use crate::inhalt::seite;
use crate::inhalt::store::Bestand;
use crate::vertrag::abgeleitet;
use crate::vertrag::daten::DatenInhalt;
use crate::vertrag::lesen::LeseFehler;

/// Fehler dieser Wege: einer wie überall oder ein Vertragsfehler (422).
pub enum Fehler {
    App(AppError),
    Vertrag(LeseFehler),
}

impl Fehler {
    /// Etappe B2: Ohne Windows-Anmeldung kein Satz, der Inhalt zitieren kann
    /// (Befund-Sätze nur mit Anmeldung). Datei und Zeile bleiben; der Satz ist neutral.
    fn frei(self, angemeldet: bool) -> Self {
        if angemeldet {
            return self;
        }
        match self {
            Self::Vertrag(e) => Self::Vertrag(LeseFehler { text: super::frei::SATZ_DATEI_KAPUTT.to_string(), ..e }),
            Self::App(e) if e.status == StatusCode::UNPROCESSABLE_ENTITY => Self::App(AppError::unlesbar(super::frei::SATZ_SEITE_KAPUTT)),
            andere => andere,
        }
    }
}

impl From<AppError> for Fehler {
    fn from(e: AppError) -> Self {
        Self::App(e)
    }
}

impl From<LeseFehler> for Fehler {
    fn from(e: LeseFehler) -> Self {
        Self::Vertrag(e)
    }
}

impl IntoResponse for Fehler {
    fn into_response(self) -> Response {
        match self {
            Self::App(e) => e.into_response(),
            Self::Vertrag(e) => {
                // Wie `AppError`: ohne Profil und Namen (`datenschutz`). Der Satz eines
                // TOML-Fehlers zitiert die Zeile, etwa einen `repos`-Eintrag mit Profilpfad.
                use crate::datenschutz::verbergen;
                let ganz = verbergen(&e.to_string());
                let koerper = VertragsFehler {
                    fehler: ganz.clone(),
                    art: "vertrag".to_string(),
                    datei: verbergen(&e.datei),
                    zeile: e.zeile.and_then(|z| u32::try_from(z).ok()),
                    satz: verbergen(&e.text),
                };
                let mut res = (StatusCode::UNPROCESSABLE_ENTITY, Json(koerper)).into_response();
                res.extensions_mut().insert(FehlerSatz(ganz));
                res
            }
        }
    }
}

/// `GET /api/projekte/<slug>`. Ohne `projekt.toml` gibt es das Projekt nicht (404).
pub async fn kopf(State(state): State<AppState>, Path(slug): Path<String>, angemeldet: Option<Extension<Angemeldet>>) -> Result<Json<ProjektAntwort>, Fehler> {
    let angemeldet = angemeldet.is_some();
    kopf_lesen(&state, slug)
        .map(|mut a| {
            // Etappe B2: open[] ist persönlicher Text; ohne Anmeldung nur die Zahl.
            a.open_zahl = a.kopf.open.len() as u64;
            if !angemeldet {
                a.kopf.open.clear();
            }
            Json(a)
        })
        .map_err(|e| e.frei(angemeldet))
}

fn kopf_lesen(state: &AppState, slug: String) -> Result<ProjektAntwort, Fehler> {
    let b = state.inhalt.bestand();
    projekt_da(&b, &slug)?;
    let p = b.projekt(&slug).ok_or_else(|| AppError::nicht_gefunden(format!("Kein Projekt „{slug}“")))?;
    let daten = p.namen.clone()?;
    // Links und Repos können lokale Pfade nennen: ohne Profil und Namen (`datenschutz`).
    let (mut kopf, _) = p.kopf.clone().ok_or_else(|| AppError::nicht_gefunden(format!("Kein Projekt „{slug}“")))??;
    crate::datenschutz::projekt_verbergen(&mut kopf);
    Ok(ProjektAntwort { kopf, daten, slug, open_zahl: 0 })
}

/// Gibt es das Projekt (`projekt.toml`)? Sonst 404.
fn projekt_da(b: &Bestand, slug: &str) -> Result<(), AppError> {
    if !seite::is_valid_slug(slug) || !b.projekt(slug).is_some_and(|p| p.hat_projekt_toml) {
        return Err(AppError::nicht_gefunden(format!("Kein Projekt „{slug}“")));
    }
    Ok(())
}

/// `GET /api/projekte/<slug>/daten/<name>`, geprüft mit dem Vertrag.
pub async fn daten(State(state): State<AppState>, Path((slug, name)): Path<(String, String)>, angemeldet: Option<Extension<Angemeldet>>) -> Response {
    let angemeldet = angemeldet.is_some();
    // Hotfix Prozent, zweite Wand: Die Schleuse prüft den rohen Pfad, axum gibt
    // hier den dekodierten Namen. Eine geschützte Datei (`sicherheit::GESCHUETZTE_DATEIEN`,
    // dieselbe Liste wie `geschuetzt`) geht nur mit Anmeldung hinaus; sonst 401 mit
    // demselben Satz wie die Schleuse.
    if !angemeldet && crate::sicherheit::datei_geschuetzt(&name) {
        tracing::warn!("Geschützte Datei ohne Anmeldung am Handler abgewiesen");
        return super::anmeldung::bitte_anmelden(None);
    }
    daten_lesen(state, slug, name).await.map_err(|e| e.frei(angemeldet)).into_response()
}

async fn daten_lesen(state: AppState, slug: String, name: String) -> Result<Json<DatenAntwort>, Fehler> {
    let b = state.inhalt.bestand();
    projekt_da(&b, &slug)?;
    let heute = abgeleitet::heute();
    let gelesen = b.daten(&slug, &name);
    drop(b);
    let Some((inhalt, tabelle)) = gelesen? else {
        return Err(AppError::nicht_gefunden(format!("Im Projekt „{slug}“ gibt es keine Datei daten/{name}.toml.")).into());
    };
    let mut abgeleitet = abgeleitet::ableiten(&tabelle, heute);
    // Etappe B2: die Warteschlange der Fragen, aus derselben Rechnung wie
    // Anstehend (mit Antworten und Einstufungen), damit der Baustein `Fragen`
    // dieselbe eine Frage zeigt. Lässt sich der Inhalt nicht sammeln, fehlt sie.
    if matches!(inhalt, DatenInhalt::Fragen(_)) {
        match super::anstehend::inhalt_sammeln(&state).await {
            Ok(i) => abgeleitet.warteschlange = Some(crate::anstehend::fragen_auswahl(&crate::anstehend::alle_eintraege(&i), &slug)),
            Err(e) => tracing::warn!("Warteschlange der Fragen für {slug}: {}", e.fehler),
        }
    }
    // Etappe B3: die Fristenliste des Projekts (fristen.toml mit Aufgaben,
    // sichtbarer Frage und Arbeiten), bewegliche aufgelöst. Lässt sich der
    // Inhalt nicht sammeln, fehlt sie (die Datei selbst steht trotzdem da).
    if matches!(inhalt, DatenInhalt::Fristen(_)) {
        match super::anstehend::inhalt_sammeln(&state).await {
            Ok(i) => abgeleitet.fristen = Some(crate::fristen::fristenliste(&i, Some(&slug))),
            Err(e) => tracing::warn!("Fristenliste für {slug}: {}", e.fehler),
        }
    }
    // Etappe 2e: der Zeitstrahl auch als Ereignisse des Verlaufs.
    if let DatenInhalt::Zeitstrahl(z) = &inhalt {
        abgeleitet.ereignisse = Some(crate::vertrag::verlauf::aus_zeitstrahl(z, heute));
    }
    // Etappe 4b: Hook `nach_lesen` des Backend-Moduls (die Middleware des
    // Projekts). Er bekommt den geprüften Inhalt; bei Fehler, Absturz oder
    // Zeitlimit geht der Inhalt unverarbeitet hinaus (Fund in /api/pruefung).
    let inhalt = if state.projekte.hat_nach_lesen(&slug) {
        match serde_json::to_value(&inhalt) {
            Ok(wert) => DatenInhalt::Eigen(state.projekte.nach_lesen(&slug, &name, wert).await),
            Err(_) => inhalt,
        }
    } else {
        inhalt
    };
    Ok(Json(DatenAntwort { abgeleitet, inhalt, name }))
}

/// Die Handeingabe mit dem Begrüßungstext des Reiters „Hallo“.
pub const HALLO_DATEN: &str = "eigen-hallo";

/// `GET /api/hallo` (frei wie die Projektseiten): welches Projekt den
/// Begrüßungstext trägt. Das Code-Repo nennt kein Projekt; es ist das erste
/// Projekt (nach Slug) im Inhalt mit `daten/eigen-hallo.toml`, sonst `null`.
pub async fn hallo(State(state): State<AppState>) -> Json<HalloOrt> {
    let b = state.inhalt.bestand();
    let projekt = b.projekte.iter().find(|(_, p)| p.in_liste && p.namen.as_ref().is_ok_and(|n| n.iter().any(|x| x == HALLO_DATEN))).map(|(s, _)| s.clone());
    Json(HalloOrt { projekt, daten: HALLO_DATEN.to_string() })
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use axum::Router;
    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use chrono::NaiveDate;
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    use crate::app::{AppState, beispiel_fuer_tests, router};
    use crate::vertrag::abgeleitet::{heute, tage_wort};

    /// Ohne Anmeldung: diese Wege sind frei lesbar wie die anderen unter /api.
    fn app() -> Router {
        router(AppState::fuer_tests(&beispiel_fuer_tests()))
    }

    async fn hole_mit(app: &Router, pfad: &str) -> (StatusCode, Value) {
        let req = Request::get(pfad).header("host", "127.0.0.1:8787").body(Body::empty()).unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into())))
    }

    async fn hole(pfad: &str) -> (StatusCode, Value) {
        hole_mit(&app(), pfad).await
    }

    /// Reiter „Hallo“: das erste Projekt (nach Slug) mit daten/eigen-hallo.toml,
    /// sonst null. Kein Slug steht im Code.
    #[tokio::test]
    async fn hallo_findet_das_projekt_mit_dem_text() {
        let (s, v) = hole("/api/hallo").await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!((v["daten"].as_str(), v["projekt"].is_null()), (Some("eigen-hallo"), true), "{v}");
        let dir = std::env::temp_dir().join(format!("dashboard-hallo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (slug, datei) in [("a-ohne", "eigen-notizen.toml"), ("b-mit", "eigen-hallo.toml"), ("c-auch", "eigen-hallo.toml")] {
            let p = dir.join("projects").join(slug);
            std::fs::create_dir_all(p.join("daten")).unwrap();
            std::fs::write(p.join("projekt.toml"), format!("schema = 3\ntitle = \"{slug}\"\nstatus = \"aktiv\"\nsummary = \"Probe\"\n")).unwrap();
            std::fs::write(p.join("daten").join(datei), "text = \"Hallo\"\n").unwrap();
        }
        let (s, v) = hole_mit(&router(AppState::fuer_tests(&dir)), "/api/hallo").await;
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v["projekt"].as_str(), Some("b-mit"), "{v}");
    }

    fn tage_bis(datum: &str) -> i64 {
        (NaiveDate::parse_from_str(datum, "%Y-%m-%d").unwrap() - heute()).num_days()
    }

    #[tokio::test]
    async fn kopf_der_neuen_form() {
        let (status, v) = hole("/api/projekte/neu-daten").await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["slug"], "neu-daten");
        // Etappe 6d: kein Feld `form` mehr, es gibt nur eine Form.
        assert!(v.get("form").is_none() && v.get("vorschau").is_none(), "{v}");
        assert_eq!(v["kopf"]["title"], "Beispiel: Daten und Ableitungen");
        assert_eq!(v["kopf"]["schema"], 3);
        assert_eq!(v["daten"], serde_json::json!(["eigen-schritte", "fortschritt", "gantt", "kennzahlen"]));
        let (status, v) = hole("/api/projekte/dashboard").await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["kopf"]["title"], "Beispiel: Dashboard", "{v}");
        assert_eq!(v["daten"], serde_json::json!(["gantt", "zeitstrahl"]));
    }

    /// Etappe 6a: Projektliste und Hauptseite ohne die Felder der alten Form.
    #[tokio::test]
    async fn projektliste_und_hauptseite_ohne_alte_form() {
        let (_, v) = hole("/api/projects").await;
        for p in v["projects"].as_array().unwrap() {
            assert!(p.get("form").is_none() && p.get("vorschau_neu").is_none() && p.get("updated").is_none(), "{p}");
        }
        let (status, d) = hole("/api/projects/dashboard").await;
        assert_eq!(status, StatusCode::OK, "{d}");
        assert!(d.get("segments").is_none(), "{d}");
        assert!(d["daten"]["gantt"].is_object(), "{d}");
    }

    #[tokio::test]
    async fn daten_mit_abgeleitetem() {
        let (status, v) = hole("/api/projekte/neu-daten/daten/eigen-schritte").await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["name"], "eigen-schritte");
        assert_eq!(v["inhalt"]["schritt"][1]["termin"], "2026-02-01", "TOML-Datum als Text");
        let a = &v["abgeleitet"];
        assert_eq!(a["heute"], heute().format("%Y-%m-%d").to_string());
        let l = &a["listen"]["schritt"];
        assert_eq!(l["anzahl"], 4);
        assert_eq!(l["erledigt"], serde_json::json!({ "feld": "erledigt", "anzahl": 2, "anteil": 0.5 }));
        assert_eq!(l["summe"], serde_json::json!({ "kosten": 1200.5 }));
        let t = &l["datum"]["termin"];
        let bis: Vec<i64> = ["2026-01-15", "2026-02-01", "2030-05-04", "2031-01-10"].map(tage_bis).to_vec();
        assert_eq!(t["tage_bis"], serde_json::json!(bis));
        assert_eq!(t["tage_seit"][0], -bis[0]);
        assert_eq!((t["naechster"].as_u64(), t["letzter"].as_u64()), (Some(2), Some(1)));

        let (status, v) = hole("/api/projekte/neu-daten/daten/gantt").await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let l = &v["abgeleitet"]["listen"]["balken"];
        assert_eq!((l["erledigt"]["feld"].as_str(), l["erledigt"]["anzahl"].as_u64()), (Some("farbe"), Some(2)), "{l}");
        assert_eq!(l["datum"]["bis"]["letzter"], 1);
        assert!(l["summe"].as_object().unwrap().is_empty());
        assert_eq!(v["inhalt"]["balken"][0]["farbe"], "ok");

        let (_, v) = hole("/api/projekte/neu-daten/daten/fortschritt").await;
        assert_eq!(v["abgeleitet"]["listen"]["zeile"]["summe"], serde_json::json!({ "erreicht": 5.0, "gesamt": 8.0, "prozent": 25.0 }));
        assert!(v["abgeleitet"]["listen"]["zeile"]["erledigt"].is_null(), "kein Status-Feld");
    }

    /// Die Ausgabe-Prüfung des Beispiels: jede Kennzahl fertig gerechnet,
    /// `berechnet` zur Erklärung dabei.
    #[tokio::test]
    async fn kennzahlen_fertig_gerechnet() {
        let (status, v) = hole("/api/projekte/neu-daten/daten/kennzahlen").await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let k = v["inhalt"]["kennzahl"].as_array().unwrap();
        let werte: Vec<&str> = k.iter().map(|k| k["wert"].as_str().unwrap()).collect();
        let seit = -tage_bis("2026-09-30");
        assert_eq!(werte, ["45 kg", "2 von 4", "50 %", "1.200,5", tage_wort(tage_bis("2030-05-04")).as_str(), tage_wort(seit).as_str(), "50 %", "5"]);
        assert!(k[0].get("berechnet").is_none());
        assert_eq!(k[1]["berechnet"], serde_json::json!({ "quelle": "eigen-schritte", "liste": null, "art": "anzahl", "feld": null }));
        assert_eq!(k[3]["einheit"], "€");
        assert_eq!(k[5]["berechnet"]["art"], "tage_seit");
        assert_eq!(v["abgeleitet"]["listen"]["kennzahl"]["anzahl"], 8);
    }

    #[tokio::test]
    async fn nicht_gefunden() {
        for pfad in [
            "/api/projekte/gibt-es-nicht",
            "/api/projekte/Gross",
            "/api/projekte/..",
            "/api/projekte/dashboard/daten/kennzahlen",
            "/api/projekte/neu-daten/daten/ampel",
            "/api/projekte/neu-daten/daten/zeitleiste",
            "/api/projekte/neu-daten/daten/Gantt",
            "/api/projekte/neu-daten/daten/gantt.toml",
            "/api/projekte/neu-daten/daten/eigen-",
        ] {
            let (status, v) = hole(pfad).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{pfad}: {v}");
            assert_eq!(v["art"], "nicht-gefunden", "{pfad}: {v}");
            assert!(v["fehler"].is_string(), "{pfad}: {v}");
        }
        // Hotfix Prozent: `%` im Pfad unter /api ist schon an der Schleuse 400.
        let (status, v) = hole("/api/projekte/neu-daten/daten/..%2Fprojekt").await;
        assert_eq!((status, v["art"].as_str()), (StatusCode::BAD_REQUEST, Some("fehlerhaft")), "{v}");
    }

    #[tokio::test]
    async fn vertragsfehler_mit_datei_und_zeile() {
        // Etappe B2: ohne Anmeldung Datei und Zeile, aber nur ein neutraler Satz.
        let (status, v) = hole("/api/projekte/neu-kaputt").await;
        assert_eq!(
            (status, v["datei"].as_str(), v["zeile"].as_u64()),
            (StatusCode::UNPROCESSABLE_ENTITY, Some("projects/neu-kaputt/projekt.toml"), Some(4)),
            "{v}"
        );
        assert_eq!(v["satz"], crate::api::frei::SATZ_DATEI_KAPUTT);
        assert!(!v.to_string().contains("aktief"), "{v}");
        let app = crate::app::router_angemeldet(AppState::fuer_tests(&beispiel_fuer_tests()));
        let (status, v) = hole_mit(&app, "/api/projekte/neu-kaputt").await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
        assert_eq!(v["art"], "vertrag");
        assert_eq!(v["datei"], "projects/neu-kaputt/projekt.toml");
        assert_eq!(v["zeile"], 4);
        assert_eq!(v["satz"], "Bei `status` steht „aktief“. Erlaubt sind: aktiv, pausiert, geplant, fertig, problem, archiviert.");
        assert_eq!(
            v["fehler"],
            "projects/neu-kaputt/projekt.toml, Zeile 4: Bei `status` steht „aktief“. Erlaubt sind: aktiv, pausiert, geplant, fertig, problem, archiviert."
        );

        // Kaputte Daten-Dateien in einem eigenen Ordner.
        let basis = std::env::temp_dir().join(format!("dash-4v-vertrag-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&basis);
        let daten = basis.join("p").join("daten");
        std::fs::create_dir_all(&daten).unwrap();
        std::fs::write(basis.join("p").join("projekt.toml"), "schema = 3\ntitle = \"P\"\nstatus = \"aktiv\"\n").unwrap();
        std::fs::write(daten.join("gantt.toml"), "[[balken]]\nname = \"A\"\nvon = \"2026-11\"\n").unwrap();
        std::fs::write(daten.join("kennzahlen.toml"), "[[kennzahl]]\ntitel = \"K\"\nberechnet = { quelle = \"ampel\", art = \"anzahl\" }\n").unwrap();
        std::fs::write(daten.join("fortschritt.toml"), "[[zeile]]\ntitel = \"F\"\nprozent = 10\n").unwrap();
        std::fs::write(daten.join("notizen.md"), "nicht im Vertrag").unwrap();
        let state = AppState::fuer_tests(&beispiel_fuer_tests()).mit_projekten(basis.clone());
        let frei = router(state.clone());
        let (_, gantt_frei) = hole_mit(&frei, "/api/projekte/p/daten/gantt").await;
        assert_eq!(
            (gantt_frei["satz"].as_str(), gantt_frei["datei"].as_str()),
            (Some(crate::api::frei::SATZ_DATEI_KAPUTT), Some("projects/p/daten/gantt.toml"))
        );
        let app = crate::app::router_angemeldet(state);
        let (s_gantt, gantt) = hole_mit(&app, "/api/projekte/p/daten/gantt").await;
        let (s_kz, kz) = hole_mit(&app, "/api/projekte/p/daten/kennzahlen").await;
        let (s_f, f) = hole_mit(&app, "/api/projekte/p/daten/fortschritt").await;
        let (s_kopf, kopf) = hole_mit(&app, "/api/projekte/p").await;
        let _ = std::fs::remove_dir_all(&basis);
        assert_eq!(s_gantt, StatusCode::UNPROCESSABLE_ENTITY, "{gantt}");
        assert_eq!(gantt["datei"], "projects/p/daten/gantt.toml", "{gantt}");
        assert!(gantt["zeile"].is_u64(), "{gantt}");
        assert_eq!(gantt["satz"], "Dem Eintrag „A“ fehlt `bis`. Beispiel: `bis = \"2027-01\"`");
        assert_eq!(s_kz, StatusCode::UNPROCESSABLE_ENTITY, "{kz}");
        assert_eq!(
            kz["fehler"],
            "projects/p/daten/kennzahlen.toml, Zeile 3: Kennzahl „K“: Die Quelle „ampel“ gibt es nicht: in `daten/` liegt keine Datei `ampel.toml`."
        );
        // Eine kaputte Datei hält die anderen nicht auf; die Liste nennt nur Vertragsnamen.
        assert_eq!(s_f, StatusCode::OK, "{f}");
        assert_eq!(s_kopf, StatusCode::OK, "{kopf}");
        assert_eq!(kopf["daten"], serde_json::json!(["fortschritt", "gantt", "kennzahlen"]));
    }

    /// Etappe 6a: Ein Ordner nur mit einer alten page.md ist kein Projekt:
    /// Kopf und Daten 404, auch wenn in `daten/` eine Datei liegt.
    #[tokio::test]
    async fn ordner_ohne_projekt_toml_gibt_es_nicht() {
        let basis = std::env::temp_dir().join(format!("dash-ohne-kopf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&basis);
        let daten = basis.join("alt").join("daten");
        std::fs::create_dir_all(&daten).unwrap();
        std::fs::write(basis.join("alt").join("page.md"), "+++\ntitle = \"Alt\"\nstatus = \"aktiv\"\n+++\n\nText.\n").unwrap();
        std::fs::write(daten.join("eigen-hallo.toml"), "titel = \"Hallo\"\n").unwrap();
        let state = AppState::fuer_tests(&beispiel_fuer_tests()).mit_projekten(basis.clone());
        let app = router(state);
        let (s, v) = hole_mit(&app, "/api/projekte/alt/daten/eigen-hallo").await;
        let (s_kopf, _) = hole_mit(&app, "/api/projekte/alt").await;
        let _ = std::fs::remove_dir_all(&basis);
        assert_eq!(s, StatusCode::NOT_FOUND, "{v}");
        assert_eq!(s_kopf, StatusCode::NOT_FOUND);
    }

    /// Etappe 2e: Der Zeitstrahl bringt seine Einträge (alle Teile) auch als
    /// Ereignisse des Verlaufs mit (`abgeleitet.ereignisse`, Quelle `hand`);
    /// andere Dateien nicht.
    #[tokio::test]
    async fn zeitstrahl_mit_ereignissen() {
        let (status, v) = hole("/api/projekte/neu-form/daten/zeitstrahl").await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let e = v["abgeleitet"]["ereignisse"].as_array().expect("ereignisse");
        let k: Vec<&str> = e.iter().map(|e| e["kennung"].as_str().unwrap()).collect();
        assert_eq!(k, ["hand:kran-geliefert", "hand:2026-12:pilot"]);
        assert_eq!((e[0]["art"].as_str(), e[0]["genauigkeit"].as_str(), e[0]["quelle"].as_str()), (Some("erledigt"), Some("tag"), Some("hand")));
        assert_eq!(e[0]["text"], "Hydraulik, Kette, Haken");
        assert_eq!(e[1]["genauigkeit"], "monat");
        // Eingabe bleibt, wie sie in der Datei steht (kennung und art, oder farbe).
        assert_eq!(v["inhalt"]["teil"]["vergangenheit"]["eintrag"][0]["kennung"], "kran-geliefert");
        assert_eq!(v["inhalt"]["teil"]["plan"]["eintrag"][0]["farbe"], "neutral");
        let (_, v) = hole("/api/projekte/neu-daten/daten/gantt").await;
        assert!(v["abgeleitet"].get("ereignisse").is_none(), "{v}");
    }

    /// Nur lesend, frei lesbar, aber hinter der Herkunftsprüfung; schnell.
    #[tokio::test]
    async fn lesend_schnell_und_mit_herkunftspruefung() {
        let app = app();
        let pfade = [
            "/api/projekte/neu-daten",
            "/api/projekte/neu-daten/daten/kennzahlen",
            "/api/projekte/neu-daten/daten/eigen-schritte",
            "/api/projekte/neu-daten/daten/gantt",
            "/api/projekte/neu-form/daten/zeitstrahl",
        ];
        // Im vollen Lauf großzügiger: dort 55,9 und 109 ms gesehen (eine Messung, viel Last).
        let grenze = crate::zeitgrenze(Duration::from_millis(50), Duration::from_millis(250));
        for pfad in pfade {
            let _ = hole_mit(&app, pfad).await;
            let start = Instant::now();
            let (status, v) = hole_mit(&app, pfad).await;
            let dauer = start.elapsed();
            assert_eq!(status, StatusCode::OK, "{pfad}: {v}");
            assert!(dauer < grenze, "{pfad}: {dauer:?}");
        }
        let fremd =
            Request::get("/api/projekte/neu-daten").header("host", "127.0.0.1:8787").header("sec-fetch-site", "cross-site").body(Body::empty()).unwrap();
        assert_eq!(app.clone().oneshot(fremd).await.unwrap().status(), StatusCode::FORBIDDEN);
        let schreiben = Request::builder()
            .method(Method::POST)
            .uri("/api/projekte/neu-daten/daten/kennzahlen")
            .header("host", "127.0.0.1:8787")
            .header("origin", "http://127.0.0.1:8787")
            .body(Body::empty())
            .unwrap();
        assert_eq!(app.clone().oneshot(schreiben).await.unwrap().status(), StatusCode::METHOD_NOT_ALLOWED);
        assert!(crate::sicherheit::nur_von_hier("/api/projekte/neu-daten"));
    }
}
