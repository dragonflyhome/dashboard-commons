//! `GET /api/betrieb`: wie der Server läuft (Dienst, Konsole, Umgebung), seit
//! wann, mit welcher Einstellungsdatei, ob der Bote antwortet und ob der
//! Datenspeicher offen ist. Für Skripte (Dienst.ps1) und später die Seite
//! /betrieb. Pfade im Benutzerordner stehen als `%USERPROFILE%\…` da.
//!
//! Der Weg ist frei (ohne Windows-Anmeldung). Darum steht in `speicher` nur,
//! ob er offen ist, das Schema und der Satz aus `SpeicherFehler`, nie ein Pfad
//! oder ein Inhalt; in `sitzung` (Etappe A3) nur, ob die Sitzungsabfrage geht,
//! ob der Inhaber angemeldet ist, die Fehlerart und wann gefragt wurde, nie SID oder Name.
//! Seit der Kopfleiste, Fassung 2, dazu die Fassung der Oberfläche (nur der
//! Ordnername) und ob sich die gelesenen Ordner lesen lassen (nur kurze Namen)
//! für den Abschnitt „Dashboard“ auf /pc.

use axum::Json;
use axum::extract::State;

use crate::app::AppState;
use crate::speicher::{self, Speicher, migration};
use crate::vertrag::api::{Betrieb, InhaltStand, OrdnerStand, QuellenStand, SpeicherStand};

/// `GET /api/betrieb` (frei): ohne Pfade und ohne den Anmeldestand des
/// Inhabers (`einstellungen`, `bote_seit` und `sitzung` sind `null`; `bote` nur
/// `vorgesehen` oder `nicht vorgesehen`, nie `verbunden` oder `getrennt`). Für
/// Skripte, die ohne Anmeldung fragen (Dienst.ps1, Dienst-Einrichten.ps1).
/// Beide Wege verbergen Profil und Namen in allen Texten (`datenschutz`),
/// auch in `speicher.fehler` und `ordner[].stand`.
pub async fn betrieb(State(state): State<AppState>) -> Json<serde_json::Value> {
    crate::datenschutz::als_json(&bericht(&state, false).await)
}

/// `GET /api/betrieb/voll` (geschützt wie `/api/daten`): alles, auch die
/// Einstellungsdatei, seit wann der Bote verbunden ist und die Sitzungsabfrage.
pub async fn betrieb_voll(State(state): State<AppState>) -> Json<serde_json::Value> {
    crate::datenschutz::als_json(&bericht(&state, true).await)
}

/// Der Bericht; `voll` nur auf dem geschützten Weg.
pub async fn bericht(state: &AppState, voll: bool) -> Betrieb {
    let state = state.clone();
    let (bote, bote_seit) = match &state.bote {
        None => ("nicht vorgesehen", None),
        Some(b) => {
            let s = b.stand();
            if s.verbunden { ("verbunden", s.seit) } else { ("getrennt", s.zuletzt_gesehen) }
        }
    };
    // Kopfleiste, Fassung 2: Fassung und Lesbarkeit der Ordner für den Abschnitt „Dashboard“ auf /pc.
    let s2 = state.clone();
    let ((fassung, fassung_gebaut), ordner) =
        tokio::task::spawn_blocking(move || (fassung_stand(&s2), ordner_stand(&s2))).await.unwrap_or_else(|_| (("build".to_string(), None), Vec::new()));
    Betrieb {
        modus: state.lauf.modus.wort().to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        // Zur Bauzeit gesetzt (`DASHBOARD_COMMIT`), sonst null.
        commit: option_env!("DASHBOARD_COMMIT").filter(|c| !c.is_empty()).map(String::from),
        gestartet: state.lauf.gestartet.to_rfc3339(),
        einstellungen: state.einstellungen.as_ref().filter(|_| voll).map(|e| speicher::anzeige_pfad(&e.datei)),
        // Frei nur, ob ein Bote vorgesehen ist: Er läuft nur in der Sitzung des Inhabers, „verbunden“
        // verriete also, ob sie an der Konsole angemeldet ist (Abnahme Kopfleiste 2, Nachbesserung 1).
        bote: if voll || bote == "nicht vorgesehen" { bote.to_string() } else { "vorgesehen".to_string() },
        bote_seit: bote_seit.filter(|_| voll),
        speicher: speicher_stand(&state).await,
        sitzung: voll.then(|| state.sitzung.lock().map(|w| w.stand()).unwrap_or_default()),
        inhalt: inhalt_stand(&state),
        fassung,
        fassung_gebaut,
        ordner,
        quellen: voll.then(|| quellen_stand(&state)),
        stoerungen: stoerungen(&state),
    }
}

/// Kleinkram 2: Störungen im Betrieb. Weicht die Fassung des Bau-Prüfers (des
/// Boten, aus seinem letzten Hallo) von der des Dienstes ab, ist das eine.
pub fn stoerungen(state: &AppState) -> Vec<String> {
    let mut s = Vec::new();
    if let Some(b) = &state.bote {
        let stand = b.stand();
        if stand.pruefer_hallo {
            s.extend(crate::bote::client::pruefer_abweichung(stand.pruefer.as_deref(), &crate::werkzeug::pruefer_fassung()));
        }
    }
    s
}

/// Woher der Inhalt kommt (nur `/api/betrieb/voll`): die Quellen, aus denen
/// gerade gelesen wird, der Daten-Ordner und die Entscheidungen.
fn quellen_stand(state: &AppState) -> QuellenStand {
    let daten = match &state.einstellungen {
        Some(e) => e.inhalt.daten.clone(),
        None => state.data_file.parent().map(std::path::Path::to_path_buf).unwrap_or_default(),
    };
    QuellenStand {
        inhalt_abschnitt: state.einstellungen.as_ref().is_some_and(|e| e.inhalt.abschnitt),
        projekte: state.quellen.iter().map(|q| speicher::anzeige_pfad(q)).collect(),
        daten: speicher::anzeige_pfad(&daten),
        entscheidungen: speicher::anzeige_pfad(&state.entscheidungen_dir),
    }
}

/// Die Fassung der Oberfläche (Ordnername unter `web/builds`, sonst `build`)
/// und wann ihre `index.html` geschrieben wurde. Blockiert kurz (Zeiger und
/// Eckdaten einer Datei).
fn fassung_stand(state: &AppState) -> (String, Option<String>) {
    let ordner = state.web.ordner();
    let name = fassung_name_von(state, &ordner);
    let gebaut = std::fs::metadata(ordner.join("index.html")).and_then(|m| m.modified()).ok().map(|z| chrono::DateTime::<chrono::Local>::from(z).to_rfc3339());
    (name, gebaut)
}

/// Nur der Name der Fassung (Kopfleiste, Fassung 3: `/api/stand`). Blockiert
/// kaum (Eckdaten des Zeigers `web/builds/aktuell`).
pub fn fassung_name(state: &AppState) -> String {
    fassung_name_von(state, &state.web.ordner())
}

fn fassung_name_von(state: &AppState, ordner: &std::path::Path) -> String {
    if ordner == state.web.build() {
        "build".to_string()
    } else {
        ordner.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "build".to_string())
    }
}

/// Lesbarkeit der gelesenen Ordner (wie `meldungen::lesbarkeit_lauf`, ohne Meldung).
fn ordner_stand(state: &AppState) -> Vec<OrdnerStand> {
    crate::meldungen::gelesene_ordner(state)
        .into_iter()
        .map(|o| {
            let stand = match crate::meldungen::ordner_lesbar(&o.pfad) {
                Ok(()) => "lesbar",
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => "fehlt",
                Err(_) => "nicht lesbar",
            };
            OrdnerStand { name: o.name.to_string(), stand: stand.to_string() }
        })
        .collect()
}

/// Der Stand des ContentStores (Etappe 2d), nur Zahlen.
pub fn inhalt_stand(state: &AppState) -> InhaltStand {
    let b = state.inhalt.bestand();
    InhaltStand { version: b.version, projekte: b.projekte.len() as u64, geladen_in_ms: b.geladen_in_ms, lesefehler: b.lesefehler_befunde().len() as u64 }
}

/// Der Stand des Datenspeichers. Offen: das Schema aus der Datei (lässt es
/// sich nicht lesen, steht der Satz in `fehler`). Nicht offen: der Satz vom
/// Start und, bei einer zu neuen Datei, deren Schema.
pub async fn speicher_stand(state: &AppState) -> SpeicherStand {
    let programm_schema = migration::SCHEMA_AKTUELL;
    let Some(offen) = state.speicher.clone() else {
        let fehler = state.speicher_fehler.as_deref().filter(|s| !s.is_empty()).unwrap_or("Der Datenspeicher wurde beim Start nicht geöffnet.");
        return SpeicherStand {
            offen: false,
            schema: state.speicher_zu_neu,
            programm_schema,
            zu_neu: state.speicher_zu_neu.is_some(),
            fehler: Some(fehler.to_string()),
        };
    };
    match Speicher::im_hintergrund(offen, |s| s.schema()).await {
        Ok(schema) => SpeicherStand { offen: true, schema: Some(schema), programm_schema, zu_neu: false, fehler: None },
        Err(e) => {
            tracing::error!("Betrieb: Schema nicht lesbar: {e} ({})", e.einzelheiten());
            SpeicherStand { offen: true, schema: None, programm_schema, zu_neu: false, fehler: Some(e.to_string()) }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;
    use crate::speicher::SpeicherFehler;

    fn beispiel() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel")
    }

    /// `/api/betrieb` ohne Anmeldung (frei), Feld `speicher` als JSON.
    async fn speicher_feld(state: AppState) -> serde_json::Value {
        let req = Request::get("/api/betrieb").header("host", "127.0.0.1:8787").body(Body::empty()).unwrap();
        let res = crate::app::router(state).oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK, "frei, ohne Anmeldung");
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        v["speicher"].clone()
    }

    /// Kopfleiste, Fassung 2: Der freie Bericht nennt keine
    /// Pfade und nicht, ob der Inhaber angemeldet ist (`einstellungen`, `bote_seit`,
    /// `sitzung` sind `null`); der volle ist geschützt und nennt die Sitzung.
    #[tokio::test]
    async fn freier_bericht_ohne_pfade_und_anmeldestand() {
        let state = AppState::fuer_tests(&beispiel());
        let req = Request::get("/api/betrieb").header("host", "127.0.0.1:8787").body(Body::empty()).unwrap();
        let res = crate::app::router(state.clone()).oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let text = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        for feld in ["einstellungen", "bote_seit", "sitzung", "quellen"] {
            assert!(v[feld].is_null(), "{feld}: {text}");
        }
        assert!(!text.contains("inhaber_angemeldet"), "{text}");
        // Der Bote nur „vorgesehen“ oder „nicht vorgesehen“, nie verbunden oder getrennt.
        assert!(matches!(v["bote"].as_str(), Some("vorgesehen" | "nicht vorgesehen")), "{text}");
        assert!(!text.contains("verbunden") && !text.contains("getrennt"), "{text}");
        // Keine Pfade: weder Laufwerk noch Benutzerordner noch Schrägstrich-Pfade.
        for muster in [":\\\\", ":/", "%USERPROFILE%", "\\\\Users", "/Users", "dienst.toml"] {
            assert!(!text.contains(muster), "{muster}: {text}");
        }
        // Der volle Bericht: ohne Anmeldung 401, angemeldet mit Sitzung.
        let req = Request::get("/api/betrieb/voll").header("host", "127.0.0.1:8787").body(Body::empty()).unwrap();
        assert_eq!(crate::app::router(state.clone()).oneshot(req).await.unwrap().status(), StatusCode::UNAUTHORIZED);
        #[cfg(windows)]
        {
            let req = Request::get("/api/betrieb/voll").header("host", "127.0.0.1:8787").body(Body::empty()).unwrap();
            let res = crate::app::router_angemeldet(state).oneshot(req).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK);
            let v: serde_json::Value = serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
            assert!(v["sitzung"].is_object(), "{v}");
            // Umzug des Inhalts: woher gelesen wird (ohne Einstellungsdatei kein [inhalt]).
            assert_eq!(v["quellen"]["inhalt_abschnitt"], false, "{v}");
            assert!(v["quellen"]["projekte"].as_array().is_some_and(|a| !a.is_empty()), "{v}");
            assert!(v["quellen"]["entscheidungen"].is_string() && v["quellen"]["daten"].is_string(), "{v}");
        }
    }

    /// Kleinkram 2, Wächter: Weicht die Fassung des Bau-Prüfers (Bote, aus
    /// seinem Hallo) von der des Dienstes ab, steht in `/api/betrieb` eine
    /// Störung; gleich ist keine, vor dem ersten Hallo und ohne Boten auch nicht.
    #[tokio::test]
    async fn betrieb_meldet_abweichenden_bau_pruefer() {
        let stoerungen = |state: AppState| async move {
            let req = Request::get("/api/betrieb").header("host", "127.0.0.1:8787").body(Body::empty()).unwrap();
            let res = crate::app::router(state).oneshot(req).await.unwrap();
            let v: serde_json::Value = serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
            v["stoerungen"].as_array().unwrap().iter().map(|s| s.as_str().unwrap().to_string()).collect::<Vec<_>>()
        };
        let ohne = AppState::fuer_tests(&beispiel());
        assert!(ohne.bote.is_none());
        assert_eq!(stoerungen(ohne).await, Vec::<String>::new(), "ohne Boten keine");
        let mut state = AppState::fuer_tests(&beispiel());
        let bote = std::sync::Arc::new(crate::bote::client::Bote::fuer_tests());
        state.bote = Some(bote.clone());
        assert_eq!(stoerungen(state.clone()).await, Vec::<String>::new(), "vor dem ersten Hallo keine");
        bote.pruefer_setzen(Some(crate::werkzeug::pruefer_fassung()));
        assert_eq!(stoerungen(state.clone()).await, Vec::<String>::new(), "gleiche Fassung");
        bote.pruefer_setzen(Some("00000000000000aa".into()));
        let s = stoerungen(state.clone()).await;
        assert_eq!(s.len(), 1, "{s:?}");
        assert!(s[0].starts_with("Der Bote prüft und baut mit einer anderen Fassung des Prüfers (00000000000000aa)"), "{s:?}");
        bote.pruefer_setzen(None);
        let s = stoerungen(state).await;
        assert!(s.len() == 1 && s[0].starts_with("Der Bote nennt keine Fassung des Prüfers"), "älterer Bote: {s:?}");
    }

    #[tokio::test]
    async fn betrieb_meldet_offenen_speicher() {
        let s = speicher_feld(AppState::fuer_tests(&beispiel())).await;
        let soll = migration::SCHEMA_AKTUELL;
        assert_eq!(s, serde_json::json!({ "offen": true, "schema": soll, "programm_schema": soll, "zu_neu": false, "fehler": null }));
    }

    #[tokio::test]
    async fn betrieb_meldet_fehlenden_speicher() {
        let satz = "Ein Eintrag im Datenspeicher ist beschädigt.";
        let s = speicher_feld(AppState::fuer_tests_ohne_speicher(&beispiel(), satz)).await;
        assert_eq!((s["offen"].as_bool(), s["zu_neu"].as_bool()), (Some(false), Some(false)));
        assert_eq!((s["schema"].clone(), s["fehler"].as_str()), (serde_json::Value::Null, Some(satz)));
    }

    /// Wie nach dem Zurückrollen auf eine ältere Fassung: Die Datei hat ein
    /// höheres Schema, der Server läuft ohne Speicher und sagt das.
    #[tokio::test]
    async fn betrieb_meldet_zu_neue_datei() {
        let pfad = crate::speicher::test_pfad();
        zu_neue_datei(&pfad, migration::SCHEMA_AKTUELL + 1);
        let e = match Speicher::oeffnen(&pfad) {
            Err(e @ SpeicherFehler::ZuNeu { .. }) => e,
            Err(e) => panic!("erwartet ZuNeu, bekam {e}"),
            Ok(_) => panic!("erwartet ZuNeu, die Datei ging auf"),
        };
        let mut state = AppState::fuer_tests_ohne_speicher(&beispiel(), &e.to_string());
        state.speicher_zu_neu = Some(migration::SCHEMA_AKTUELL + 1);
        let s = speicher_feld(state).await;
        assert_eq!((s["offen"].as_bool(), s["zu_neu"].as_bool()), (Some(false), Some(true)));
        assert_eq!(s["schema"], migration::SCHEMA_AKTUELL + 1);
        assert_eq!(s["programm_schema"], migration::SCHEMA_AKTUELL);
        assert_eq!(s["fehler"].as_str(), Some(e.to_string().as_str()));
        assert!(!s.to_string().contains(".redb"), "kein Pfad auf dem freien Weg: {s}");
        std::fs::remove_file(&pfad).unwrap();
    }

    /// Eine Datei mit nur `meta` und dem Schema `schema`.
    fn zu_neue_datei(pfad: &Path, schema: u32) {
        let db = redb::Database::create(pfad).unwrap();
        let txn = db.begin_write().unwrap();
        txn.open_table(crate::speicher::tabellen::META).unwrap().insert(crate::speicher::tabellen::META_SCHEMA, schema.to_string().as_str()).unwrap();
        txn.commit().unwrap();
    }
}
