//! Der gemeinsame Stand für die Aktualitätsprüfung (Kopfleiste, Fassung 3;
//! Entscheidung `immer-aktuell`, Schritt 4): Ereignisnummer
//! und Inhaltsversion, die der Server liefert und die Seite vergleicht.
//!
//! - `GET /api/stand` (frei, hinter Adresse, Host und Herkunft wie alles):
//!   `ereignis` zählt nur Ereignisse, die auch der freie Kanal trägt
//!   (`projekt`, `daten` für pc und schwerpunkte, `inhalt` ohne die übrigen Dateien,
//!   `fassung`). Daraus lässt sich nichts ablesen, was der freie Kanal nicht
//!   ohnehin jedem zeigt.
//! - `GET /api/stand/voll` (geschützt wie `/api/events`): `ereignis` zählt
//!   alle Ereignisse des geschützten Kanals mit Daten (`change`, `vorgang`,
//!   `fassung`, `projekt`, `daten`, `inhalt`, `speicher`, `pc-benutzer`,
//!   `docs`). Wie oft der Datenspeicher des Inhabers schreibt, verriete seine
//!   Tätigkeit; darum nur hier.
//!
//! Nicht gezählt: die PC-Werte alle 5 s (`pc`, kein Datenstand), `anmeldung`
//! (je Verbindung), `bau` (gehört zu den Vorgängen, die Seite liest es nicht)
//! und `nachholen` (je Verbindung; der Überlauf zählt hier mit, siehe unten).
//!
//! Gezählt wird an einer Stelle: Eine Aufgabe abonniert dieselben Kanäle wie
//! die Live-Ströme und zählt je Nachricht einmal (nicht je Verbindung). Die
//! Seite zählt mit, was sie empfängt; fehlt beim Abgleich etwas, holt sie es
//! nach. Läuft diese Aufgabe selbst über (`Lagged`), zählt sie die verlorenen
//! mit, damit die Nummer nie hinter dem wahren Stand zurückbleibt. Frei zählt
//! ein Überlauf nur auf Kanälen, die auch Freies tragen (Fassungen, Inhalt);
//! auf den nur geschützten (Änderungen, Vorgänge, Bereiche: `speicher`,
//! `docs`, `pc-benutzer`) nie, sonst verriete die freie Nummer geschützte
//! Schreibvorgänge (Auflage aus der Abnahme der Kopfleiste 3).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use axum::Json;
use axum::extract::State;
use tokio::sync::broadcast::error::RecvError;

use crate::api::events::InhaltEreignis;
use crate::app::AppState;
use crate::vertrag::api::ServerStand;

/// Die zwei Zähler: frei (was der freie Kanal trägt) und alle (geschützt).
#[derive(Debug, Default)]
pub struct Ereigniszaehler {
    frei: AtomicU64,
    alle: AtomicU64,
}

impl Ereigniszaehler {
    /// Ein Ereignis; `frei`: auch auf dem freien Kanal.
    pub fn zaehlen(&self, frei: bool, n: u64) {
        self.alle.fetch_add(n, Ordering::Relaxed);
        if frei {
            self.frei.fetch_add(n, Ordering::Relaxed);
        }
    }

    /// Die Nummer für den freien Weg.
    pub fn frei(&self) -> u64 {
        self.frei.load(Ordering::Relaxed)
    }

    /// Die Nummer für den geschützten Weg.
    pub fn alle(&self) -> u64 {
        self.alle.load(Ordering::Relaxed)
    }
}

/// Was eine Nachricht eines Kanals für die Zähler bedeutet: `Some(frei)`
/// zählt, `None` ist zu Ende (Kanal geschlossen). `traegt_freies`: Kann
/// dieser Kanal überhaupt Freies tragen?
fn eins<T>(r: Result<T, RecvError>, traegt_freies: bool, frei: impl Fn(&T) -> bool) -> Option<(bool, u64)> {
    match r {
        Ok(v) => Some((traegt_freies && frei(&v), 1)),
        // Die freien unter den verlorenen kennt niemand mehr: als frei zählen,
        // dann bleibt auch die freie Nummer nie zurück (die Seite holt nach).
        // Auf einem nur geschützten Kanal ist keines frei: nur `alle` zählt.
        Err(RecvError::Lagged(n)) => Some((traegt_freies, n)),
        Err(RecvError::Closed) => None,
    }
}

/// Ein Inhalts-Ereignis ist frei, wenn der freie Kanal etwas davon trägt.
fn inhalt_frei(e: &InhaltEreignis) -> bool {
    e.nur_frei().is_some()
}

/// Startet die zählende Aufgabe (einmal je Server, im Dienst beim Start).
/// Endet, wenn alle Kanäle zu sind.
pub fn zaehlen_starten(state: &AppState) {
    let z = state.zaehler.clone();
    let mut aenderungen = state.events.subscribe();
    let mut vorgaenge = state.vorgang_events.subscribe();
    let mut fassungen = state.fassung_events.subscribe();
    let mut inhalt = state.inhalt.abonnieren();
    let mut bereiche = state.aenderungen.abonnieren();
    tokio::spawn(async move {
        let mut offen = [true; 5];
        while offen.iter().any(|o| *o) {
            let (i, r) = tokio::select! {
                // Nur geschützt: Änderungen, Vorgänge, Bereiche (auch beim Überlauf).
                r = aenderungen.recv(), if offen[0] => (0, eins(r, false, |_| false)),
                r = vorgaenge.recv(), if offen[1] => (1, eins(r, false, |_| false)),
                r = fassungen.recv(), if offen[2] => (2, eins(r, true, |_| true)),
                r = inhalt.recv(), if offen[3] => (3, eins(r, true, inhalt_frei)),
                r = bereiche.recv(), if offen[4] => (4, eins(r, false, |_| false)),
            };
            match r {
                Some((frei, n)) => z.zaehlen(frei, n),
                None => offen[i] = false,
            }
        }
    });
}

/// Der Stand; `voll` nur auf dem geschützten Weg.
fn stand(state: &AppState, voll: bool) -> ServerStand {
    let z = &state.zaehler;
    ServerStand {
        ereignis: if voll { z.alle() } else { z.frei() },
        inhalt: state.inhalt.bestand().version,
        fassung: crate::dienst::betrieb::fassung_name(state),
    }
}

/// `GET /api/stand` (frei): Nummer der freien Ereignisse, Inhaltsversion, Fassung.
pub async fn frei(State(state): State<AppState>) -> Json<ServerStand> {
    Json(stand(&state, false))
}

/// `GET /api/stand/voll` (geschützt): Nummer aller Ereignisse mit Daten.
pub async fn voll(State(state): State<AppState>) -> Json<ServerStand> {
    Json(stand(&state, true))
}

/// Für Tests und die Zähl-Aufgabe: ein neuer Zähler.
pub fn neu() -> Arc<Ereigniszaehler> {
    Arc::new(Ereigniszaehler::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::events::{Change, DatenEreignis, FassungEreignis, ProjektEreignis, SammelEreignis};
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use std::time::Duration;
    use tower::ServiceExt;

    fn get(pfad: &str) -> Request<Body> {
        Request::get(pfad).header("host", "127.0.0.1:8787").body(Body::empty()).unwrap()
    }

    async fn json(app: axum::Router, pfad: &str) -> (StatusCode, serde_json::Value) {
        let res = app.oneshot(get(pfad)).await.unwrap();
        let status = res.status();
        let b = res.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&b).unwrap_or(serde_json::Value::Null))
    }

    /// Bis die Zähl-Aufgabe gezählt hat (sie läuft nebenher).
    async fn warten_bis(f: impl Fn() -> bool) {
        for _ in 0..200 {
            if f() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("Zähler kam nicht an");
    }

    /// Frei zählt nur, was der freie Kanal trägt; geschützt zählt alles mit
    /// Daten. Die freie Nummer verrät also nichts über Vorgänge, Speicher
    /// oder Änderungen an geschützten Dateien.
    #[tokio::test]
    async fn frei_zaehlt_nur_freie_ereignisse() {
        let state = AppState::fuer_tests(&crate::app::beispiel_fuer_tests());
        zaehlen_starten(&state);
        // Geschützt: Änderung, Bereich, Daten außer pc.
        state.events.send(Change { area: "anstehend", path: "anstehend".into(), at: chrono::Local::now() }).unwrap();
        state.aenderungen.melden(crate::aenderungen::Bereich::Meldungen);
        state.inhalt.senden_fuer_tests(InhaltEreignis::Daten(DatenEreignis { name: "schluessel".into(), version: 3 }));
        warten_bis(|| state.zaehler.alle() == 3).await;
        assert_eq!(state.zaehler.frei(), 0, "nichts Freies darunter");
        // Frei: Projekt, pc, Fassung, ein Sammelereignis mit Projekten.
        state.inhalt.senden_fuer_tests(InhaltEreignis::Projekt(ProjektEreignis { slug: "schaufenster".into(), version: 4, assets: None, dateien: None }));
        state.inhalt.senden_fuer_tests(InhaltEreignis::Daten(DatenEreignis { name: "pc".into(), version: 5 }));
        state.inhalt.senden_fuer_tests(InhaltEreignis::Sammel(SammelEreignis { version: 6, projekte: vec!["schaufenster".into()], daten: vec![] }));
        state.fassung_events.send(FassungEreignis { ordner: "20261031-120000".into(), dauer_ms: 1 }).unwrap();
        warten_bis(|| state.zaehler.alle() == 7).await;
        assert_eq!(state.zaehler.frei(), 4);
    }

    /// Überlauf (`Lagged`): Auf den nur geschützten Kanälen zählt er nie frei,
    /// auf Fassungen und Inhalt frei und geschützt (die Seite holt dann nach).
    #[test]
    fn ueberlauf_geschuetzter_kanaele_zaehlt_nicht_frei() {
        assert_eq!(eins::<()>(Err(RecvError::Lagged(7)), false, |_| true), Some((false, 7)));
        assert_eq!(eins::<()>(Err(RecvError::Lagged(7)), true, |_| false), Some((true, 7)));
        assert_eq!(eins(Ok(()), false, |_| true), Some((false, 1)), "nur geschützt: nie frei");
        assert_eq!(eins::<()>(Err(RecvError::Closed), true, |_| true), None);
    }

    /// Von außen: Läuft die Zähl-Aufgabe auf den Änderungen über (der Kanal
    /// fasst 64), steigt nur die geschützte Nummer, um alle; die freie bleibt
    /// bei null.
    #[tokio::test]
    async fn ueberlauf_verraet_frei_nichts() {
        let state = AppState::fuer_tests(&crate::app::beispiel_fuer_tests());
        zaehlen_starten(&state);
        // Ohne Warten senden: Die Aufgabe kommt erst danach dran und läuft über.
        for i in 0..200 {
            state.events.send(Change { area: "anstehend", path: format!("anstehend/{i}"), at: chrono::Local::now() }).unwrap();
        }
        warten_bis(|| state.zaehler.alle() == 200).await;
        assert_eq!(state.zaehler.frei(), 0, "Überlauf der Änderungen zählt nicht frei");
    }

    /// `/api/stand` ist frei und nennt die freie Nummer; `/api/stand/voll`
    /// verlangt die Anmeldung (401) und nennt mit ihr alle.
    #[tokio::test]
    async fn frei_und_voll() {
        let state = AppState::fuer_tests(&crate::app::beispiel_fuer_tests());
        state.zaehler.zaehlen(false, 2);
        state.zaehler.zaehlen(true, 1);
        let (s, v) = json(crate::app::router(state.clone()), "/api/stand").await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v["ereignis"].as_u64(), Some(1), "{v}");
        assert!(v["inhalt"].as_u64().is_some() && v["fassung"].as_str().is_some(), "{v}");
        let (s, _) = json(crate::app::router(state.clone()), "/api/stand/voll").await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        let (s, v) = json(crate::app::router_angemeldet(state.clone()), "/api/stand/voll").await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v["ereignis"].as_u64(), Some(3), "{v}");
        // Nur diese drei Felder: keine Pfade, keine Namen.
        assert_eq!(v.as_object().map(|o| o.len()), Some(3), "{v}");
    }

    /// Die Inhaltsversion ist dieselbe wie in `/api/betrieb`.
    #[tokio::test]
    async fn inhalt_wie_betrieb() {
        let state = AppState::fuer_tests(&crate::app::beispiel_fuer_tests());
        let (_, s) = json(crate::app::router(state.clone()), "/api/stand").await;
        let (_, b) = json(crate::app::router(state.clone()), "/api/betrieb").await;
        assert_eq!(s["inhalt"], b["inhalt"]["version"]);
        assert_eq!(s["fassung"], b["fassung"]);
    }
}
