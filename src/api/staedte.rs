//! Städtesuche für die Kalenderseite (Stufe 3): `POST /api/kalender/staedte`
//! mit der Anfrage im Rumpf, nur angemeldet (Schleuse und Handler). Die
//! Anfrage wird nicht protokolliert und nicht gespeichert; die Treffer sind
//! öffentliche Städte aus der privat abgelegten GeoNames-Liste.

use axum::body::Bytes;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};

use super::vorgaenge::json_kopf_pruefen;
use crate::anmeldung::Angemeldet;
use crate::app::AppState;
use crate::fehler::AppError;
use crate::kalender::ortsliste::{self, Ortsliste};
use crate::vertrag::kalender::{KalenderStadt, KalenderStaedte, KalenderStaedteAnfrage};

/// Vorgabe und Obergrenze der Trefferzahl.
const VORGABE: u32 = 10;
const HOECHSTENS: u32 = 20;
/// Länger ist kein Städtename.
const ZEICHEN: usize = 100;

/// `POST /api/kalender/staedte`.
pub async fn suchen(State(state): State<AppState>, kopf: HeaderMap, angemeldet: Option<Extension<Angemeldet>>, koerper: Bytes) -> Response {
    if angemeldet.is_none() {
        return super::anmeldung::bitte_anmelden(None);
    }
    async {
        json_kopf_pruefen(&kopf)?;
        // Fehlersätze zitieren die Anfrage nicht.
        let a: KalenderStaedteAnfrage = serde_json::from_slice(&koerper)
            .map_err(|_| AppError::unlesbar("Der Inhalt ist kein passendes JSON. Erwartet wird etwa {\"anfrage\": \"Stadtname\", \"hoechstens\": 10}."))?;
        let hoechstens = a.hoechstens.unwrap_or(VORGABE);
        if !(1..=HOECHSTENS).contains(&hoechstens) {
            return Err(AppError::unlesbar(format!("„hoechstens“ liegt zwischen 1 und {HOECHSTENS}.")));
        }
        if a.anfrage.chars().count() > ZEICHEN {
            return Err(AppError::unlesbar(format!("„anfrage“ hat höchstens {ZEICHEN} Zeichen.")));
        }
        let ordner = state.inhalt.ordner().join(crate::kalender::orte::PROJEKT).join(ortsliste::ORDNER);
        let liste =
            tokio::task::spawn_blocking(move || Ortsliste::geladen(&ordner)).await.map_err(|_| AppError::intern("Die Städteliste ließ sich nicht lesen."))?;
        let (treffer, hinweis) = match liste {
            Ok(l) => (
                l.suchen(&a.anfrage, hoechstens as usize)
                    .into_iter()
                    .map(|s| KalenderStadt {
                        name: s.name.clone(),
                        land: s.land.clone(),
                        region: s.region.clone(),
                        breite: s.breite,
                        laenge: s.laenge,
                        zeitzone: s.zeitzone.clone(),
                    })
                    .collect(),
                None,
            ),
            Err(satz) => (Vec::new(), Some(satz)),
        };
        Ok::<_, AppError>(Json(KalenderStaedte { treffer, quelle: ortsliste::QUELLE.to_string(), hinweis }))
    }
    .await
    .into_response()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    use tower::ServiceExt;

    use crate::app::{AppState, router, router_angemeldet};
    use crate::kalender::ortsliste::{KOPF, LISTE, ORDNER, QUELLE, SATZ_FEHLT};

    // Erfundene Orte mit erfundenen Koordinaten.
    const PROBE: &str = "Nebelfurt\tBrumbach\tZZ\tNordmark\t12.5\t-33.75\tEurope/Berlin\n\
                         Saint-Éloi-des-Brumes\tEloiville\tZZ\tSüdmark\t-41.125\t101.5\tEurope/Paris\n";

    /// Ein Inhalt mit dem Projekt kalender; `liste`: die Städteliste samt Kopf.
    fn inhalt(name: &str, liste: bool) -> PathBuf {
        let wurzel = std::env::temp_dir().join(format!("dashboard-staedte-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wurzel);
        let p = wurzel.join("projects").join("kalender");
        std::fs::create_dir_all(p.join("daten")).unwrap();
        std::fs::write(p.join("projekt.toml"), "schema = 3\ntitle = \"Kalender\"\nstatus = \"aktiv\"\nsummary = \"S\"\n").unwrap();
        if liste {
            let o = p.join(ORDNER);
            std::fs::create_dir_all(&o).unwrap();
            std::fs::write(o.join(LISTE), PROBE).unwrap();
            let summe: String = Sha256::digest(PROBE.as_bytes()).iter().map(|b| format!("{b:02x}")).collect();
            let kopf = format!(
                "quelle = \"https://example.org/probe.zip\"\nabgerufen = 2026-11-26\nlizenz = \"CC BY 4.0\"\nsha256 = \"{}\"\nsha256_liste = \"{summe}\"\n",
                "ab".repeat(32)
            );
            std::fs::write(o.join(KOPF), kopf).unwrap();
        }
        wurzel
    }

    /// Alle Dateien unter `ordner` mit Inhalt (für „nichts verändert“).
    fn dateien(ordner: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut aus = BTreeMap::new();
        let mut offen = vec![ordner.to_path_buf()];
        while let Some(o) = offen.pop() {
            for e in std::fs::read_dir(&o).unwrap().flatten() {
                if e.file_type().unwrap().is_dir() {
                    offen.push(e.path());
                } else {
                    aus.insert(e.path(), std::fs::read(e.path()).unwrap());
                }
            }
        }
        aus
    }

    fn post(json: &str) -> Request<Body> {
        Request::post("/api/kalender/staedte")
            .header("host", "127.0.0.1:8787")
            .header("origin", "http://127.0.0.1:8787")
            .header("sec-fetch-site", "same-origin")
            .header("content-type", "application/json")
            .body(Body::from(json.to_string()))
            .unwrap()
    }

    async fn schicke(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into())))
    }

    #[tokio::test]
    async fn ohne_anmeldung_401() {
        let wurzel = inhalt("frei", true);
        let (s, v) = schicke(&router(AppState::fuer_tests(&wurzel)), post(r#"{"anfrage": "Nebelfurt"}"#)).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        assert!(!v.to_string().contains("Nebelfurt") && !v.to_string().contains("12.5"), "{v}");
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    #[tokio::test]
    async fn findet_mit_koordinaten_und_namensnennung_ohne_spuren() {
        let wurzel = inhalt("treffer", true);
        let vorher = dateien(&wurzel);
        let app = router_angemeldet(AppState::fuer_tests(&wurzel));
        let (s, v) = schicke(&app, post(r#"{"anfrage": "saint-eloi"}"#)).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert_eq!(
            v["treffer"],
            json!([{"name": "Saint-Éloi-des-Brumes", "land": "ZZ", "region": "Südmark", "breite": -41.125, "laenge": 101.5, "zeitzone": "Europe/Paris"}])
        );
        assert_eq!(v["quelle"], QUELLE);
        assert_eq!(v["hinweis"], Value::Null);
        // Über einen Nebennamen, höchstens begrenzt; leere Anfrage ohne Treffer.
        let (_, v) = schicke(&app, post(r#"{"anfrage": "brumbach", "hoechstens": 1}"#)).await;
        assert_eq!(v["treffer"][0]["name"], "Nebelfurt");
        let (s, v) = schicke(&app, post(r#"{"anfrage": "  "}"#)).await;
        assert_eq!((s, v["treffer"].as_array().map(Vec::len)), (StatusCode::OK, Some(0)));
        // Die Suche schreibt nichts in den Inhalt.
        assert_eq!(dateien(&wurzel), vorher);
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    #[tokio::test]
    async fn ohne_liste_ein_satz() {
        let wurzel = inhalt("ohne", false);
        let app = router_angemeldet(AppState::fuer_tests(&wurzel));
        let (s, v) = schicke(&app, post(r#"{"anfrage": "Nebelfurt"}"#)).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert_eq!(v["treffer"], json!([]));
        assert_eq!(v["hinweis"], SATZ_FEHLT);
        // Falsche Prüfsumme: ein Satz ohne Zahlen, keine Treffer.
        let w2 = inhalt("summe", true);
        let kopf = w2.join("projects/kalender").join(ORDNER).join(KOPF);
        let alt = std::fs::read_to_string(&kopf).unwrap();
        let summe = alt.lines().find(|z| z.starts_with("sha256_liste")).unwrap().to_string();
        std::fs::write(&kopf, alt.replace(&summe, &format!("sha256_liste = \"{}\"", "cd".repeat(32)))).unwrap();
        let (s, v) = schicke(&router_angemeldet(AppState::fuer_tests(&w2)), post(r#"{"anfrage": "Nebelfurt"}"#)).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert_eq!(v["treffer"], json!([]));
        let h = v["hinweis"].as_str().unwrap();
        assert!(h.contains("Prüfsumme") && !h.contains("cdcd") && !h.contains("abab"), "{h}");
        let _ = std::fs::remove_dir_all(&wurzel);
        let _ = std::fs::remove_dir_all(&w2);
    }

    #[tokio::test]
    async fn falscher_rumpf_422_ohne_die_eingabe() {
        let wurzel = inhalt("rumpf", true);
        let app = router_angemeldet(AppState::fuer_tests(&wurzel));
        let lang = format!(r#"{{"anfrage": "{}"}}"#, "Geheimweg".repeat(20));
        for rumpf in [
            r#"{"anfrage": "Geheimweg", "extra": 1}"#,
            r#"{"suche": "Geheimweg"}"#,
            r#"{"anfrage": "Geheimweg", "hoechstens": 21}"#,
            r#"{"anfrage": "Geheimweg", "hoechstens": 0}"#,
            r#"{"anfrage": "Geheimweg", "hoechstens": -3}"#,
            "Geheimweg",
            &lang,
        ] {
            let (s, v) = schicke(&app, post(rumpf)).await;
            assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{rumpf}: {v}");
            assert!(!v.to_string().contains("Geheimweg"), "{v}");
        }
        let _ = std::fs::remove_dir_all(&wurzel);
    }
}
