//! `/api/schluessel` (nur Status, nie Werte) und der Formular-POST
//! `/schluessel/<name>`: der einzige schreibende Weg neben den Antworten.

use axum::Json;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect, Response};
use tracing::{info, warn};
use zeroize::Zeroizing;

use super::vertrag::{KeyList, KeyStatus};
use crate::app::AppState;
use crate::fehler::AppError;
use crate::schluessel;
use crate::sicherheit::percent_encode;
use crate::vertrag::bote::{Anfrage, Antwort, Wert};

/// Satz, wenn der Bote vorgesehen ist, aber nicht antwortet (POST).
pub const SATZ_OHNE_BOTEN: &str = "Der Bote läuft nicht; bitte am PC anmelden, dann klappt das Speichern.";
/// Hinweis der Liste, wenn der Bote nicht antwortet.
pub const HINWEIS_OHNE_BOTEN: &str = "Der Bote läuft nicht; ob die Schlüssel im Tresor stehen, ist gerade unbekannt. Bitte am PC anmelden.";

/// Anforderungen mit Zustand im Tresor: ohne Boten direkt (Altweg), mit
/// Boten über ihn. Antwortet der Bote nicht, bleibt `saved_at` leer, und der
/// zweite Wert ist der Hinweis dazu.
pub async fn ansichten(state: &AppState) -> anyhow::Result<(Vec<schluessel::KeyView>, Option<String>)> {
    let Some(bote) = &state.bote else {
        return Ok((schluessel::views(&state.keys_file)?, None));
    };
    let anforderungen = schluessel::load(&state.keys_file)?;
    let fragen = anforderungen.iter().map(|r| async move { bote.anfragen(&Anfrage::TresorInfo { ziel: schluessel::target_for(&r.name) }).await });
    let antworten = futures_util::future::join_all(fragen).await;
    let mut hinweis = None;
    let ansichten = anforderungen
        .into_iter()
        .zip(antworten)
        .map(|(req, antwort)| {
            let (saved_at, unbekannt) = match antwort.as_ref().ok().and_then(|a| tresor_antwort(&req.name, a)) {
                Some(stand) => stand,
                None => {
                    // Überlastet ist nicht „läuft nicht“: dann der Satz mit dem letzten Stand.
                    let stand = bote.stand();
                    hinweis = Some(match stand.lage {
                        crate::bote::client::Lage::Ueberlastet => crate::bote::client::satz_ausgelastet(stand.zuletzt_gesehen.as_deref()),
                        _ => HINWEIS_OHNE_BOTEN.to_string(),
                    });
                    (None, true)
                }
            };
            schluessel::KeyView { target: schluessel::target_for(&req.name), req, saved_at, unbekannt }
        })
        .collect();
    Ok((ansichten, hinweis))
}

/// Was eine Antwort des Boten für einen Schlüssel heißt: gespeichert am
/// (oder nicht) und ob der Stand unbekannt ist. Ein Fehler des Boten für
/// diesen Schlüssel heißt „unbekannt“, nicht „fehlt“ (Etappe 3a+; früher rot
/// gezählt). `None`: keine passende Antwort (der Bote fehlt oder ist ausgelastet).
pub(crate) fn tresor_antwort(name: &str, antwort: &Antwort) -> Option<(Option<chrono::DateTime<chrono::Local>>, bool)> {
    match antwort {
        Antwort::TresorInfo { gespeichert_am, .. } => {
            Some((gespeichert_am.as_deref().and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok()).map(|t| t.with_timezone(&chrono::Local)), false))
        }
        Antwort::Fehler { satz } => {
            warn!("Bote, Tresor {name}: {satz}");
            Some((None, true))
        }
        _ => None,
    }
}

/// Nur Status je Schlüssel, nie Werte.
pub async fn liste(State(state): State<AppState>) -> Json<KeyList> {
    let (keys, error) = match ansichten(&state).await {
        Ok((v, hinweis)) => (
            v.into_iter()
                .map(|k| KeyStatus {
                    stand: k.stand(),
                    name: k.req.name,
                    zweck: k.req.zweck,
                    hinweis: k.req.hinweis,
                    angefordert_von: k.req.angefordert_von,
                    target: k.target,
                    saved_at: k.saved_at,
                })
                .collect(),
            hinweis,
        ),
        Err(e) => {
            warn!("data/schluessel.toml lässt sich nicht lesen: {e:#}");
            (Vec::new(), Some("Die Datei data/schluessel.toml ist fehlerhaft. Einzelheiten stehen im Protokoll des Servers.".to_string()))
        }
    };
    Json(KeyList { keys, error })
}

/// Formular-POST: nur vom Dashboard selbst, höchstens 4 KB, Wert geht direkt
/// in den Credential Manager und wird danach überschrieben (ebenso Benutzername
/// und der rohe Formularkörper, auf jedem Weg aus der Funktion). Nichts davon wird
/// geloggt oder in die Antwort geschrieben. Zurück geht es immer auf die
/// Schlüssel-Seite der Oberfläche, Ergebnis als `?gespeichert=` oder `?fehler=`.
pub async fn speichern(State(state): State<AppState>, Path(name): Path<String>, body: Body) -> Result<Response, AppError> {
    if !schluessel::is_valid_name(&name) {
        return Err(AppError::nicht_gefunden(format!("Nicht gefunden: {name}")));
    }
    let bytes = axum::body::to_bytes(body, 4096).await.map_err(|_| AppError::zu_gross("Zu viele Daten."))?;
    // `Bytes` sind unveränderlich: sofort in einen eigenen Puffer kopieren, der
    // beim Verlassen der Funktion (auch auf jedem Fehlerweg) überschrieben wird.
    let roh = Zeroizing::new(bytes.to_vec());
    drop(bytes);
    let request = match schluessel::load(&state.keys_file) {
        Ok(list) => list.into_iter().find(|k| k.name == name),
        Err(e) => {
            warn!("data/schluessel.toml lässt sich nicht lesen: {e:#}");
            return Ok(zurueck_mit_fehler("Die Datei data/schluessel.toml ist fehlerhaft. Einzelheiten stehen im Protokoll des Servers."));
        }
    };
    let Some(request) = request else {
        return Ok(zurueck_mit_fehler(&format!("„{name}“ ist nicht in data/schluessel.toml angefordert.")));
    };
    // Benutzername: freiwillig vom Formular (landet nur im Tresor, nie im Repo),
    // sonst der schon im Tresor stehende (nicht stumm überschreiben), sonst der
    // Eintrag aus schluessel.toml, sonst der Zweck. Alle Kopien werden überschrieben.
    let feld = Zeroizing::new(schluessel::form_value(&roh, "benutzer").unwrap_or_default());
    let benutzer = Zeroizing::new(feld.trim().to_string());
    drop(feld);
    if benutzer.chars().count() > 200 {
        return Ok(zurueck_mit_fehler("Der Benutzername ist zu lang (höchstens 200 Zeichen)."));
    }
    let wert = Zeroizing::new(schluessel::form_value(&roh, "wert").unwrap_or_default());
    drop(roh);
    let target = schluessel::target_for(&name);
    if let Some(bote) = &state.bote {
        return Ok(ueber_boten(bote, &name, &target, &benutzer, &wert, &request, &state.aenderungen).await);
    }
    let im_tresor: Option<Zeroizing<String>> =
        if benutzer.is_empty() { schluessel::cred_info(&target).and_then(|mut i| i.username.take()).map(Zeroizing::new) } else { None };
    let benutzername: &str = if !benutzer.is_empty() {
        &benutzer
    } else if let Some(u) = &im_tresor {
        u
    } else {
        request.username()
    };
    let trimmed = wert.trim();
    let outcome = if trimmed.is_empty() { Err(anyhow::anyhow!("Das Feld war leer.")) } else { schluessel::cred_write(&target, benutzername, trimmed) };
    Ok(match outcome {
        Ok(()) => {
            info!("Schlüssel {name} gespeichert (Credential Manager)");
            // Live-Lücken: andere Tabs (/schluessel, /pc) sehen den neuen Tresor-Stand.
            state.aenderungen.melden(crate::aenderungen::Bereich::Tresor);
            Redirect::to(&format!("/schluessel?gespeichert={name}")).into_response()
        }
        Err(e) => {
            warn!("Schlüssel {name} nicht gespeichert: {e:#}");
            let grund = if e.to_string().contains("leer") {
                "Das Feld war leer."
            } else if e.to_string().contains("zu lang") {
                "Der Wert ist zu lang."
            } else {
                "Der Windows-Tresor hat den Wert nicht angenommen. Einzelheiten stehen im Protokoll des Servers."
            };
            zurueck_mit_fehler(&format!("„{name}“ nicht gespeichert: {grund}"))
        }
    })
}

/// Speichern über den Boten: dieselben Regeln für den Benutzernamen, der Wert
/// geht nur in die Pipe und wird danach überschrieben. Antwortet der Bote
/// nicht, wird nichts gespeichert, und die Seite sagt, warum.
async fn ueber_boten(
    bote: &crate::bote::client::Bote,
    name: &str,
    target: &str,
    benutzer: &str,
    wert: &str,
    request: &schluessel::KeyRequest,
    melder: &std::sync::Arc<crate::aenderungen::Melder>,
) -> Response {
    let trimmed = wert.trim();
    if trimmed.is_empty() {
        return zurueck_mit_fehler(&format!("„{name}“ nicht gespeichert: Das Feld war leer."));
    }
    let benutzername: Zeroizing<String> = if !benutzer.is_empty() {
        Zeroizing::new(benutzer.to_string())
    } else {
        match bote.anfragen(&Anfrage::TresorInfo { ziel: target.to_string() }).await {
            Ok(Antwort::TresorInfo { benutzer: Some(u), .. }) => Zeroizing::new(u),
            Ok(Antwort::TresorInfo { .. }) => Zeroizing::new(request.username().to_string()),
            Ok(Antwort::Fehler { satz }) => return zurueck_mit_fehler(&format!("„{name}“ nicht gespeichert: {satz}")),
            Ok(_) | Err(_) => return zurueck_mit_fehler(SATZ_OHNE_BOTEN),
        }
    };
    let anfrage = Anfrage::TresorSchreiben { ziel: target.to_string(), benutzer: benutzername.to_string(), wert: Wert::neu(trimmed) };
    let ergebnis = bote.anfragen(&anfrage).await;
    drop(anfrage);
    match ergebnis {
        Ok(Antwort::Ok) => {
            info!("Schlüssel {name} gespeichert (Credential Manager, über den Boten)");
            melder.melden(crate::aenderungen::Bereich::Tresor);
            Redirect::to(&format!("/schluessel?gespeichert={name}")).into_response()
        }
        Ok(Antwort::Fehler { satz }) => {
            warn!("Schlüssel {name} nicht gespeichert: {satz}");
            zurueck_mit_fehler(&format!("„{name}“ nicht gespeichert: {satz}"))
        }
        Ok(_) | Err(_) => zurueck_mit_fehler(SATZ_OHNE_BOTEN),
    }
}

fn zurueck_mit_fehler(msg: &str) -> Response {
    Redirect::to(&format!("/schluessel?fehler={}", percent_encode(msg))).into_response()
}

// ----- Tresor-Wache (Live-Lücken, Auflage aus der Abnahme) -----

/// So oft vergleicht die Tresor-Wache den Stand im Tresor.
pub const TRESOR_TAKT: std::time::Duration = if cfg!(test) { std::time::Duration::from_millis(50) } else { std::time::Duration::from_secs(60) };

/// Der Abdruck des Tresors: je angefordertem Schlüssel Name und ob er im
/// Tresor steht. Nie ein Wert, kein Benutzername, keine Zeit.
pub type TresorAbdruck = Vec<(String, bool)>;

/// Holt den Abdruck: ohne Boten direkt (`CredReadW`, nur ob es den Eintrag
/// gibt), mit Boten über `TresorInfo` (nur `vorhanden`). `None`, wenn sich der
/// Stand gerade nicht sagen lässt (Bote weg oder ausgelastet, Fehler für einen
/// Schlüssel, schluessel.toml unlesbar); dann bleibt der letzte Abdruck stehen.
/// Schreibt nichts ins Protokoll (sonst jede Minute dieselbe Zeile).
pub async fn tresor_abdruck(state: &AppState) -> Option<TresorAbdruck> {
    let anforderungen = schluessel::load(&state.keys_file).ok()?;
    let Some(bote) = &state.bote else {
        return tokio::task::spawn_blocking(move || {
            anforderungen
                .into_iter()
                .map(|r| {
                    let da = schluessel::cred_exists(&schluessel::target_for(&r.name)).is_some();
                    (r.name, da)
                })
                .collect()
        })
        .await
        .ok();
    };
    if !bote.stand().verbunden {
        return None;
    }
    let mut abdruck = Vec::with_capacity(anforderungen.len());
    for r in anforderungen {
        match bote.anfragen(&Anfrage::TresorInfo { ziel: schluessel::target_for(&r.name) }).await {
            Ok(Antwort::TresorInfo { vorhanden, .. }) => abdruck.push((r.name, vorhanden)),
            _ => return None,
        }
    }
    Some(abdruck)
}

/// Merkt sich den letzten Abdruck und sagt, ob sich etwas geändert hat.
#[derive(Debug, Default)]
pub struct TresorWache {
    zuletzt: Option<TresorAbdruck>,
}

impl TresorWache {
    /// `true`, wenn `neu` vom letzten bekannten Abdruck abweicht. Der erste
    /// Abdruck ist keine Änderung; ein unbekannter Stand (`None`) auch nicht,
    /// er lässt den letzten bekannten stehen.
    pub fn pruefen(&mut self, neu: Option<TresorAbdruck>) -> bool {
        let Some(neu) = neu else { return false };
        let geaendert = self.zuletzt.as_ref().is_some_and(|alt| *alt != neu);
        self.zuletzt = Some(neu);
        geaendert
    }
}

/// Eine Runde der Wache: Abdruck holen, vergleichen, bei Änderung
/// `pc-benutzer` mit Bereich `tresor` melden. Gibt zurück, ob gemeldet wurde.
pub async fn tresor_runde(state: &AppState, wache: &mut TresorWache) -> bool {
    let geaendert = wache.pruefen(tresor_abdruck(state).await);
    if geaendert {
        info!("Tresor: Stand der angeforderten Schlüssel hat sich geändert");
        state.aenderungen.melden(crate::aenderungen::Bereich::Tresor);
    }
    geaendert
}

/// Vergleicht alle [`TRESOR_TAKT`] den Tresor-Stand (auch Schlüssel, die in
/// der Windows-Anmeldeinformationsverwaltung gelöscht oder von Hand angelegt
/// wurden; das Dashboard selbst meldet nur, was es schreibt). Endet beim
/// Abschaltsignal.
pub fn spawn_tresor_wache(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let aus = state.abschaltung();
        tokio::pin!(aus);
        let mut wache = TresorWache::default();
        loop {
            tresor_runde(&state, &mut wache).await;
            tokio::select! {
                _ = tokio::time::sleep(TRESOR_TAKT) => {}
                _ = &mut aus => return,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    /// Auflage aus der Abnahme 3a-4 (Etappe 3a+): Meldet der Bote für einen
    /// Schlüssel einen Fehler, ist dessen Stand unbekannt, nicht „fehlt“; der
    /// Gesamtstand ist dann unbekannt (`tresor-fehler`), nie rot.
    #[test]
    fn fehler_des_boten_je_schluessel_ist_unbekannt() {
        use crate::schluessel::{KeyRequest, KeyView};
        use crate::vertrag::api::{SchluesselGrund, TresorStand, Vollstaendig};
        use crate::vertrag::bote::Antwort;

        let fehler = super::tresor_antwort("a", &Antwort::Fehler { satz: "Zugriff verweigert".into() }).unwrap();
        assert_eq!(fehler, (None, true));
        let da = super::tresor_antwort("b", &Antwort::TresorInfo { vorhanden: true, gespeichert_am: Some("2026-10-30T10:00:00+01:00".into()), benutzer: None })
            .unwrap();
        assert!(da.0.is_some() && !da.1);
        let fehlt = super::tresor_antwort("c", &Antwort::TresorInfo { vorhanden: false, gespeichert_am: None, benutzer: None }).unwrap();
        assert_eq!(fehlt, (None, false));
        assert_eq!(super::tresor_antwort("d", &Antwort::Ok), None, "keine passende Antwort: wie Bote fehlt");

        let ansicht = |name: &str, (saved_at, unbekannt): (Option<chrono::DateTime<chrono::Local>>, bool)| KeyView {
            req: KeyRequest { name: name.into(), zweck: "Zweck".into(), hinweis: String::new(), angefordert_von: String::new(), benutzer: String::new() },
            target: crate::schluessel::target_for(name),
            saved_at,
            unbekannt,
        };
        let v = vec![ansicht("a", fehler), ansicht("b", da), ansicht("c", fehlt)];
        let stand: Vec<TresorStand> = v.iter().map(KeyView::stand).collect();
        assert_eq!(stand, [TresorStand::Unbekannt, TresorStand::Gespeichert, TresorStand::Fehlt]);
        let s = crate::api::pc::schluessel_stand_aus(&v, false);
        assert_eq!((s.vollstaendig, s.grund, s.fehlen), (Vollstaendig::Unbekannt, Some(SchluesselGrund::TresorFehler), 0));
        assert_eq!(serde_json::to_value(SchluesselGrund::TresorFehler).unwrap(), "tresor-fehler");
        // Ohne den Fehler: ein Schlüssel fehlt wirklich (rot), einer ist da.
        let s = crate::api::pc::schluessel_stand_aus(&v[1..], false);
        assert_eq!((s.vollstaendig, s.gespeichert, s.fehlen), (Vollstaendig::Nein, 1, 1));
        // Bote fehlt ganz: bote-fehlt wie bisher.
        assert_eq!(crate::api::pc::schluessel_stand_aus(&v, true).grund, Some(SchluesselGrund::BoteFehlt));
    }

    /// Tresor-Wache: der erste Abdruck ist keine Änderung, ein unbekannter
    /// Stand auch nicht (der letzte bekannte bleibt), ein fehlender oder neuer
    /// Schlüssel schon, je einmal.
    #[test]
    fn tresor_wache_vergleicht_nur_namen_und_vorhanden() {
        let ab = |v: &[(&str, bool)]| Some(v.iter().map(|(n, d)| (n.to_string(), *d)).collect::<super::TresorAbdruck>());
        let mut w = super::TresorWache::default();
        assert!(!w.pruefen(ab(&[("a", true), ("b", true)])), "erster Blick");
        assert!(!w.pruefen(ab(&[("a", true), ("b", true)])));
        assert!(!w.pruefen(None), "unbekannt ist keine Änderung");
        assert!(w.pruefen(ab(&[("a", true), ("b", false)])), "b gelöscht");
        assert!(!w.pruefen(ab(&[("a", true), ("b", false)])), "nur einmal");
        assert!(!w.pruefen(None));
        assert!(w.pruefen(ab(&[("a", true), ("b", true)])), "nach unbekannt gegen den letzten bekannten Stand");
        assert!(w.pruefen(ab(&[("a", true), ("b", true), ("c", false)])), "neue Anforderung");
    }

    /// Tresor-Wache von außen (ohne Boten): ein eigener Testeintrag
    /// `ranke/test-…`, nie ein echter. Löschen in der Anmeldeinformations-
    /// verwaltung gibt `pc-benutzer` mit Bereich `tresor`; nur ob der Eintrag
    /// da ist, wird gelesen, nie der Wert. Beim Abschaltsignal endet die Wache.
    #[cfg(windows)]
    #[tokio::test]
    async fn tresor_wache_sieht_geloeschten_schluessel() {
        use std::path::PathBuf;

        use crate::aenderungen::Bereich;
        use crate::app::AppState;
        use crate::schluessel;

        /// Räumt den eigenen Tresor-Eintrag und den Ordner auf, auch wenn der
        /// Test vorher scheitert (nur `ranke/test-tresor-wache-<pid>`).
        struct Aufraeumen {
            target: String,
            ordner: PathBuf,
        }
        impl Drop for Aufraeumen {
            fn drop(&mut self) {
                if self.target.starts_with("ranke/test-tresor-wache-") {
                    let _ = schluessel::cred_delete(&self.target);
                }
                let _ = std::fs::remove_dir_all(&self.ordner);
            }
        }

        let _nacheinander = schluessel::TRESOR_TESTS.lock().await;
        let name = format!("test-tresor-wache-{}", std::process::id());
        let target = schluessel::target_for(&name);
        assert_eq!(target, format!("ranke/test-tresor-wache-{}", std::process::id()));
        let ordner = std::env::temp_dir().join(format!("dashboard-test-{}-tresor-wache", std::process::id()));
        let _aufraeumen = Aufraeumen { target: target.clone(), ordner: ordner.clone() };
        std::fs::create_dir_all(&ordner).unwrap();
        let keys = ordner.join("schluessel.toml");
        std::fs::write(&keys, format!("[[key]]\nname = \"{name}\"\nzweck = \"Test der Tresor-Wache\"\n")).unwrap();
        let _ = schluessel::cred_delete(&target);
        schluessel::cred_write(&target, "Test der Tresor-Wache", "nur-ein-test").unwrap();

        let beispiel = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel");
        let mut state = AppState::fuer_tests_ohne_speicher(&beispiel, "");
        state.keys_file = std::sync::Arc::new(keys);
        let mut rx = state.aenderungen.abonnieren();
        let mut wache = super::TresorWache::default();
        let erst = super::tresor_runde(&state, &mut wache).await;
        let abdruck = super::tresor_abdruck(&state).await;
        let geloescht = schluessel::cred_delete(&target);
        let danach = super::tresor_runde(&state, &mut wache).await;
        let nochmal = super::tresor_runde(&state, &mut wache).await;
        // Abnahme 4c: mit Frist auf das Ereignis warten statt einer festen Pause
        // (unter Last kam es nach BUENDELN + 150 ms manchmal noch nicht).
        let ereignis = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv()).await;
        // Danach kein zweites (gebündelt): eine Bündelzeit und etwas mehr warten.
        let zweites = tokio::time::timeout(crate::aenderungen::BUENDELN + std::time::Duration::from_millis(300), rx.recv()).await;
        // Die Wache als Aufgabe endet beim Abschaltsignal.
        let (state2, aus) = AppState::fuer_tests_mit_abschalter(&beispiel, 8790);
        let aufgabe = super::spawn_tresor_wache(state2);
        aus.send(true).unwrap();
        let ende = tokio::time::timeout(std::time::Duration::from_secs(2), aufgabe).await;

        geloescht.unwrap();
        assert!(!erst, "erster Blick");
        assert_eq!(abdruck, Some(vec![(name.clone(), true)]));
        assert!(danach, "gelöscht ist eine Änderung");
        assert!(!nochmal, "nur einmal");
        let ereignis = ereignis.expect("Ereignis binnen 5 s").expect("Ereignis empfangen");
        assert_eq!(ereignis.bereich, Bereich::Tresor);
        assert!(zweites.is_err(), "nur ein Ereignis: {zweites:?}");
        assert!(ende.is_ok(), "Tresor-Wache endet beim Abschaltsignal");
    }

    /// Auflage aus der Abnahme 2a: bleibt das Feld „Benutzername“ beim
    /// Neuspeichern leer, behält der Eintrag den Namen aus dem Tresor. Nur ein
    /// eigener Testeintrag `ranke/test-…`, nie ein echter; am Ende gelöscht.
    #[cfg(windows)]
    #[tokio::test]
    async fn leerer_benutzername_behaelt_den_namen_aus_dem_tresor() {
        use std::path::PathBuf;

        use axum::http::{Request, StatusCode, header};
        use tower::ServiceExt;

        use crate::app::{AppState, router_angemeldet as router};
        use crate::schluessel;

        let _nacheinander = schluessel::TRESOR_TESTS.lock().await;
        let name = format!("test-benutzer-bleibt-{}", std::process::id());
        let target = schluessel::target_for(&name);
        assert!(target.starts_with("ranke/test-"));
        let ordner = std::env::temp_dir().join(format!("dashboard-test-{}-schluessel", std::process::id()));
        std::fs::create_dir_all(&ordner).unwrap();
        let keys = ordner.join("schluessel.toml");
        std::fs::write(&keys, format!("[[key]]\nname = \"{name}\"\nzweck = \"Zweck aus der TOML\"\nbenutzer = \"Name aus der TOML\"\n")).unwrap();
        let _ = schluessel::cred_delete(&target);
        schluessel::cred_write(&target, "Name aus dem Tresor", "alter-testwert").unwrap();

        let beispiel = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel");
        let mut state = AppState::fuer_tests_ohne_speicher(&beispiel, "");
        state.keys_file = std::sync::Arc::new(keys);
        let post = Request::post(format!("/schluessel/{name}"))
            .header("host", "127.0.0.1:8787")
            .header("origin", "http://127.0.0.1:8787")
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .body(axum::body::Body::from("benutzer=+&wert=neuer-testwert"))
            .unwrap();
        let res = router(state).oneshot(post).await.unwrap();
        // Erst lesen und aufräumen, dann prüfen: ein Fehlschlag lässt keinen Eintrag liegen.
        let info = schluessel::cred_info(&target);
        let geloescht = schluessel::cred_delete(&target);
        let _ = std::fs::remove_dir_all(&ordner);

        assert_eq!(res.status(), StatusCode::SEE_OTHER);
        let ziel = res.headers()[header::LOCATION].to_str().unwrap();
        assert_eq!(ziel, format!("/schluessel?gespeichert={name}"));
        let info = info.expect("Eintrag nach dem Speichern vorhanden");
        assert_eq!(info.username.as_deref(), Some("Name aus dem Tresor"), "nicht stumm überschrieben");
        geloescht.unwrap();
    }
}
