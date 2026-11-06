//! `/api/antworten`: Der Inhaber beantwortet Fragen aus `daten/fragen.toml` (Etappe
//! A2, Konzept „Anstehendes“ [regeln] antworten).
//!
//! - `POST /api/antworten/<slug>/<id> {"gestellt": "2026-10-25", "antwort": "…", "notiz": "…"}`:
//!   Die Antwort des Inhabers, nur aus dem Browser (wie der Haken bei Aufgaben; ohne
//!   `Origin`/`Sec-Fetch-Site`, also per curl, 422). Das ist eine Prüfung der
//!   Kopfzeilen gegen Versehen, keine Sicherheitsgrenze (Anleitung, „Antworten
//!   auf Fragen“). Die Antwort wird gegen die Frage geprüft, wie sie jetzt in
//!   `projects/<slug>/daten/fragen.toml` steht: bei `wahl` eine der
//!   `optionen`, bei `ja-nein` „ja“ oder „nein“, bei `text` 1 bis 2000
//!   Zeichen, bei `bestaetigen` nichts oder „bestaetigt“. Dazu wahlweise
//!   `notiz`, die Anmerkung des Inhabers (höchstens 1000 Zeichen, Zeilenumbrüche
//!   erlaubt, leer zählt als keine), bei `wahl`, `ja-nein` und `bestaetigen`;
//!   bei `text` 422, dort gehört alles in die Antwort. Antwort und Vorgang
//!   `antwort` (Bezug `antwort:<slug>/<id>`, Herkunft `browser`) entstehen in
//!   einer Transaktion; das wartende `Warten.ps1` des Chats wacht sofort auf.
//!   „Etwas anderes …“ (Entscheidung, bei `wahl`, `ja-nein` und
//!   `bestaetigen`): `{"antwort": "etwas-anderes", "eigene": "…"}` mit den
//!   eigenen Worten (nicht leer, höchstens 20 000 Zeichen, Zeilenumbrüche
//!   erlaubt). Gleicher Weg wie ein Klick; `eigene` steht in der Antwort,
//!   bei einem Klick ist es `null`. Weil der Text lang sein darf, nimmt dieser
//!   Weg bis 64 KiB (`sicherheit::ANTWORT_KOERPER_HOECHSTENS`).
//! - `GET /api/antworten/<slug>/<id>`: die neueste Antwort auf diese Frage
//!   (auch aus dem Archiv), mit `notiz` (`null`: keine), für den Chat nach
//!   dem Wecken. Dieselbe Antwort steht auch in `GET /api/vorgaenge/<id>`
//!   (Feld `antwort`).
//! - `GET /api/antworten[?projekt=<slug>]`: Antworten außerhalb des Archivs,
//!   neueste zuerst (zum Nachlesen nach einer Pause).
//!
//! Zweimal antworten: Dieselbe Antwort mit derselben Anmerkung noch einmal
//! (Doppelklick, Wiederholung nach einem Netzfehler) gibt 200 mit
//! `geaendert: false` und weckt niemanden. Eine andere Antwort auf dieselbe
//! Frage gibt 409, ebenso dieselbe Antwort mit einer anderen (oder ohne)
//! Anmerkung: Eine Antwort samt Anmerkung wird nie überschrieben. Will der Inhaber
//! es sich anders überlegen, stellt der Chat die Frage neu (neues
//! `gestellt`). Fehlt die Frage in fragen.toml (nie gestellt oder vom Chat
//! schon herausgenommen): 404. Steht dort inzwischen eine neu gestellte Frage
//! mit derselben Kennung (anderes `gestellt`): 409.
//!
//! Alle Wege sind geschützt (Windows-Anmeldung, `sicherheit::geschuetzt`) und
//! wie `/api/vorgaenge` ohne Herkunftskopf für curl erlaubt; fremde Seiten
//! bleiben 403. Eine Antwort ist kein Nachweis gegenüber einem anderen Chat
//! und gibt nie allein eine riskante Handlung frei (Anleitung, „Antworten auf Fragen“).

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Uri};
use serde::Deserialize;

use super::vertrag::{Antwort, AntwortGespeichert, AntwortenListe};
use super::vorgaenge::{abfrage, angegeben, aus_browser, json_kopf_pruefen, melden, slug_pruefen, text_pruefen};
use crate::app::AppState;
use crate::fehler::AppError;
use crate::inhalt::store::ContentStore;
use crate::speicher::antworten::{self, BESTAETIGT, NeueAntwort, Speichern};
use crate::speicher::aufgaben::{self, Zeit};
use crate::speicher::{Speicher, SpeicherFehler};
use crate::vertrag::daten::{DatenInhalt, ETWAS_ANDERES, Frage, FrageArt, ist_kennung};

/// Eine Antwort als Text höchstens so viele Zeichen.
pub const TEXT_HOECHSTENS: usize = 2000;
/// Eine Anmerkung zur Antwort höchstens so viele Zeichen.
pub const NOTIZ_HOECHSTENS: usize = 1000;
/// Eigene Worte bei „Etwas anderes …“ höchstens so viele Zeichen: nur eine
/// technische Grenze gegen Missbrauch, die beim Schreiben nie im Weg ist.
pub const EIGENE_HOECHSTENS: usize = 20_000;

pub const SATZ_NUR_BROWSER: &str = "Fragen beantwortet nur der Inhaber im Browser. Ein Chat liest die Antwort mit GET /api/antworten/<projekt>/<id>.";
pub const SATZ_NICHT_OFFEN: &str = "Diese Frage ist nicht mehr offen.";
pub const SATZ_NEU_GESTELLT: &str = "Diese Frage wurde inzwischen neu gestellt. Bitte noch einmal antworten.";
pub const SATZ_KEINE_ANTWORT: &str = "Auf diese Frage gibt es noch keine Antwort.";
pub const SATZ_TEXT_LEER: &str = "Die Antwort ist leer. Bitte etwas schreiben.";
pub const SATZ_JA_NEIN: &str = "Bei dieser Frage ist die Antwort „ja“ oder „nein“.";
pub const SATZ_BESTAETIGEN: &str = "Diese Frage wird nur bestätigt; eine andere Antwort geht hier nicht.";
pub const SATZ_NOTIZ_BEI_TEXT: &str = "Bei einer Frage mit freier Antwort gibt es keine eigene Anmerkung; schreib alles in die Antwort.";
pub const SATZ_EIGENE_LEER: &str = "Das Feld ist leer. Bitte schreib in eigenen Worten, was du meinst.";
pub const SATZ_EIGENE_BEI_TEXT: &str = "Bei einer Frage mit freier Antwort schreibst du ohnehin eigene Worte; bitte ins Feld „Deine Antwort“.";
pub const SATZ_EIGENE_OHNE_ETWAS_ANDERES: &str = "Eigene Worte („eigene“) gehen nur mit der Antwort „etwas-anderes“.";
const SATZ_KENNUNG: &str = "Die Kennung der Frage ist ungültig. Erlaubt sind a–z, 0–9 und Bindestrich, etwa schlusssatz.";
const MUSTER: &str = r#"{"gestellt": "2026-10-25", "antwort": "ja", "notiz": "freiwillig"} (bei bestaetigen reicht {"gestellt": "…"}; eigene Worte: {"antwort": "etwas-anderes", "eigene": "…"})"#;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AntwortKoerper {
    #[serde(default)]
    gestellt: Option<String>,
    #[serde(default)]
    antwort: Option<String>,
    #[serde(default)]
    notiz: Option<String>,
    #[serde(default)]
    eigene: Option<String>,
}

/// Fehler des Datenspeichers: 500 mit Satz, Einzelheiten ins Protokoll.
fn speicherfehler(e: SpeicherFehler) -> AppError {
    tracing::error!("Antworten: Datenspeicher: {e} ({})", e.einzelheiten());
    AppError::intern(format!("{e} Einzelheiten stehen im Protokoll."))
}

/// Kurz für einen Satz: höchstens 60 Zeichen, eine Zeile.
fn kurz(text: &str) -> String {
    let eine_zeile = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if eine_zeile.chars().count() <= 60 {
        return eine_zeile;
    }
    let mut k: String = eine_zeile.chars().take(59).collect();
    k.push('…');
    k
}

/// Prüft die Antwort gegen die Art der Frage und gibt sie so zurück, wie sie
/// gespeichert wird. Die Sätze sind für den Inhaber.
pub fn antwort_pruefen(f: &Frage, antwort: Option<String>) -> Result<String, AppError> {
    match f.art {
        FrageArt::Wahl => {
            let a = angegeben(antwort).unwrap_or_default();
            if f.optionen.contains(&a) {
                Ok(a)
            } else {
                Err(AppError::unlesbar(format!("Diese Antwort passt nicht zur Frage. Möglich sind: {}.", f.optionen.join(", "))))
            }
        }
        FrageArt::JaNein => match angegeben(antwort).map(|a| a.to_lowercase()).as_deref() {
            Some("ja") => Ok("ja".to_string()),
            Some("nein") => Ok("nein".to_string()),
            _ => Err(AppError::unlesbar(SATZ_JA_NEIN)),
        },
        FrageArt::Text => {
            let Some(a) = angegeben(antwort) else { return Err(AppError::unlesbar(SATZ_TEXT_LEER)) };
            text_pruefen("antwort", &a, TEXT_HOECHSTENS, true)?;
            Ok(a)
        }
        FrageArt::Bestaetigen => match angegeben(antwort).as_deref() {
            None | Some(BESTAETIGT) => Ok(BESTAETIGT.to_string()),
            Some(_) => Err(AppError::unlesbar(SATZ_BESTAETIGEN)),
        },
    }
}

/// Prüft die freiwillige Anmerkung: leer zählt als keine; sonst höchstens
/// `NOTIZ_HOECHSTENS` Zeichen, ohne Steuerzeichen (Zeilenumbrüche erlaubt),
/// wie eine Antwort als Text. Bei `text` gibt es keine eigene Anmerkung.
pub fn notiz_pruefen(f: &Frage, notiz: Option<String>) -> Result<Option<String>, AppError> {
    let Some(n) = angegeben(notiz) else { return Ok(None) };
    if f.art == FrageArt::Text {
        return Err(AppError::unlesbar(SATZ_NOTIZ_BEI_TEXT));
    }
    text_pruefen("notiz", &n, NOTIZ_HOECHSTENS, true)?;
    Ok(Some(n))
}

/// Die Frage `id` aus `projects/<slug>/daten/fragen.toml` (alle Teile), aus
/// dem ContentStore (Etappe 2d: dieselbe Datei, wie die Seite sie zeigt).
/// Lässt sich die Datei nicht lesen: 422 mit dem Satz der Prüfung (Datei,
/// Zeile, was falsch ist).
fn frage_lesen(inhalt: &ContentStore, slug: &str, id: &str) -> Result<Option<Frage>, AppError> {
    match inhalt.bestand().daten(slug, "fragen") {
        Ok(Some((DatenInhalt::Fragen(d), _))) => Ok(d.inhalte().into_iter().flat_map(|t| t.frage.iter()).find(|f| f.id == id).cloned()),
        Ok(_) => Ok(None),
        Err(e) => Err(AppError::unlesbar(e.to_string())),
    }
}

/// Projekt und Kennung aus der Adresse prüfen (nur a–z, 0–9, Bindestrich;
/// kein Pfad entsteht aus der Adresse).
fn adresse_pruefen(slug: &str, id: &str) -> Result<(), AppError> {
    slug_pruefen(slug)?;
    if !ist_kennung(id) {
        return Err(AppError::unlesbar(SATZ_KENNUNG));
    }
    Ok(())
}

/// `POST /api/antworten/<slug>/<id>`: die Antwort des Inhabers, nur aus dem Browser.
pub async fn beantworten(
    State(state): State<AppState>,
    Path((slug, id)): Path<(String, String)>,
    kopf: HeaderMap,
    koerper: Bytes,
) -> Result<Json<AntwortGespeichert>, AppError> {
    json_kopf_pruefen(&kopf)?;
    let k: AntwortKoerper = serde_json::from_slice(&koerper).map_err(|e| {
        // Nur die Stelle ins Protokoll, nie der Text (er kann die Antwort des Inhabers enthalten).
        tracing::debug!("Antwort: Körper unlesbar ({:?}, Zeile {}, Spalte {})", e.classify(), e.line(), e.column());
        AppError::unlesbar(format!("Der Inhalt ist kein passendes JSON. Erwartet wird etwa {MUSTER}."))
    })?;
    if !aus_browser(&kopf) {
        return Err(AppError::unlesbar(SATZ_NUR_BROWSER));
    }
    adresse_pruefen(&slug, &id)?;
    let speicher = state.speicher()?;
    let jetzt = aufgaben::jetzt();
    let inhalt = state.inhalt.clone();
    let (s2, i2) = (slug.clone(), id.clone());
    let frage = tokio::task::spawn_blocking(move || frage_lesen(&inhalt, &s2, &i2))
        .await
        .map_err(|e| AppError::intern(format!("Die Fragen ließen sich nicht lesen ({e}).")))??;
    let Some(frage) = frage else { return Err(AppError::nicht_gefunden(SATZ_NICHT_OFFEN)) };
    if angegeben(k.gestellt).is_some_and(|g| g != frage.gestellt) {
        return Err(AppError::konflikt(SATZ_NEU_GESTELLT));
    }
    let (antwort, eigene) = mit_eigenen_worten(&frage, k.antwort, k.eigene)?;
    let notiz = notiz_pruefen(&frage, k.notiz)?;
    let neu = NeueAntwort {
        projekt: slug,
        id,
        gestellt: frage.gestellt.clone(),
        art: frage.art,
        frage: frage.text.clone(),
        antwort: antwort.clone(),
        notiz,
        eigene: eigene.clone(),
    };
    let ergebnis = Speicher::im_hintergrund(speicher, move |s| s.antwort_speichern(neu, &jetzt)).await.map_err(speicherfehler)?;
    match ergebnis {
        Speichern::Neu { antwort, vorgang, ereignis } => {
            // Ins Protokoll nur, dass geantwortet wurde, nie die Antwort selbst.
            tracing::info!("Frage {}/{} beantwortet, Vorgang {}", antwort.projekt, antwort.id, vorgang.id);
            melden(&state, &ereignis);
            super::events::anstehend_geaendert(&state.events, Some(&antwort.projekt));
            Ok(Json(AntwortGespeichert { antwort, geaendert: true }))
        }
        Speichern::Gleich(antwort) => Ok(Json(AntwortGespeichert { antwort, geaendert: false })),
        // Bei „Etwas anderes …“ zeigt der Satz die eigenen Worte, nicht den festen Wert.
        Speichern::Anders(alt) if alt.antwort == antwort && alt.eigene == eigene => Err(AppError::konflikt(format!(
            "Diese Frage hast du schon beantwortet („{}“), mit einer anderen Anmerkung. Ändern geht nur, wenn der Chat die Frage neu stellt.",
            kurz(alt.eigene.as_deref().unwrap_or(&alt.antwort))
        ))),
        Speichern::Anders(alt) => Err(AppError::konflikt(format!(
            "Diese Frage hast du schon beantwortet („{}“). Eine andere Antwort geht nur, wenn der Chat die Frage neu stellt.",
            kurz(alt.eigene.as_deref().unwrap_or(&alt.antwort))
        ))),
    }
}

/// Prüft die eigenen Worte bei „Etwas anderes …“:
/// nicht leer (nach dem Entfernen von Leerzeichen am Rand), höchstens
/// `EIGENE_HOECHSTENS` Zeichen, Zeilenumbrüche erlaubt, keine anderen
/// Steuerzeichen. Gibt den Text ohne Leerzeichen am Rand zurück. Auch für
/// Aufgaben (`api::aufgaben`).
pub fn eigene_pruefen(eigene: Option<String>) -> Result<String, AppError> {
    let Some(e) = angegeben(eigene) else { return Err(AppError::unlesbar(SATZ_EIGENE_LEER)) };
    text_pruefen("eigene", &e, EIGENE_HOECHSTENS, true)?;
    Ok(e)
}

/// Antwort und eigene Worte zusammen prüfen: `antwort = "etwas-anderes"`
/// braucht `eigene` (nicht bei `text`, dort schreibt man ohnehin eigene
/// Worte); jede andere Antwort wird wie bisher geprüft, und `eigene` muss
/// dann fehlen oder leer sein.
pub fn mit_eigenen_worten(f: &Frage, antwort: Option<String>, eigene: Option<String>) -> Result<(String, Option<String>), AppError> {
    if angegeben(antwort.clone()).is_some_and(|a| a == ETWAS_ANDERES) {
        if f.art == FrageArt::Text {
            return Err(AppError::unlesbar(SATZ_EIGENE_BEI_TEXT));
        }
        return Ok((ETWAS_ANDERES.to_string(), Some(eigene_pruefen(eigene)?)));
    }
    if angegeben(eigene).is_some() {
        return Err(AppError::unlesbar(SATZ_EIGENE_OHNE_ETWAS_ANDERES));
    }
    Ok((antwort_pruefen(f, antwort)?, None))
}

/// `GET /api/antworten/<slug>/<id>`: die neueste Antwort auf diese Frage.
pub async fn eine(State(state): State<AppState>, Path((slug, id)): Path<(String, String)>) -> Result<Json<Antwort>, AppError> {
    adresse_pruefen(&slug, &id)?;
    let liste = Speicher::im_hintergrund(state.speicher()?, move |s| s.antworten_zur_frage(&slug, &id)).await.map_err(speicherfehler)?;
    liste.into_iter().next().map(Json).ok_or_else(|| AppError::nicht_gefunden(SATZ_KEINE_ANTWORT))
}

/// `GET /api/antworten[?projekt=<slug>]`.
pub async fn liste(State(state): State<AppState>, uri: Uri) -> Result<Json<AntwortenListe>, AppError> {
    let projekt = angegeben(abfrage(&uri, "projekt"));
    if let Some(p) = &projekt {
        slug_pruefen(p)?;
    }
    // Entscheidung archiv-status: ohne `projekt` fehlen archivierte Projekte.
    let ohne = if projekt.is_none() { state.archivierte() } else { Default::default() };
    let mut antworten = Speicher::im_hintergrund(state.speicher()?, move |s| s.antworten_lesen(projekt.as_deref())).await.map_err(speicherfehler)?;
    antworten.retain(|a| !ohne.contains(&a.projekt));
    Ok(Json(AntwortenListe { antworten }))
}

/// Ein Lauf des Archivs: Antworten, die sieben Tage alt sind, ziehen nach
/// `antworten_archiv` um (nichts wird gelöscht; die Fragen bleiben
/// beantwortet). Gibt die Zahl zurück.
pub async fn archiv_lauf(state: &AppState, jetzt: Zeit) -> Result<usize, AppError> {
    let weg = Speicher::im_hintergrund(state.speicher()?, move |s| s.antworten_archivieren(&jetzt)).await.map_err(speicherfehler)?;
    if !weg.is_empty() {
        tracing::info!("{} Antwort(en) ins Archiv verschoben (älter als {} Tage)", weg.len(), antworten::ARCHIVIEREN_NACH_TAGEN);
    }
    Ok(weg.len())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;
    use crate::app::router_angemeldet as router;

    fn beispiel() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel")
    }

    /// Ein eigener Ordner mit einem Projekt `probe`, dessen fragen.toml alle
    /// vier Arten hat (erfundene Fragen). Verschwindet mit dem Wächter.
    struct Probe {
        wurzel: PathBuf,
    }

    impl Drop for Probe {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.wurzel);
        }
    }

    const FRAGEN: &str = "[[frage]]\nid = \"farbe\"\ntext = \"Welche Farbe?\"\nart = \"wahl\"\noptionen = [\"Blau\", \"Grün\"]\nempfehlung = \"Grün\"\ngestellt = \"2026-10-25\"\nprioritaet = \"jetzt\"\n\n\
                          [[frage]]\nid = \"weiter\"\ntext = \"Weitermachen?\"\nart = \"ja-nein\"\ngestellt = \"2026-10-26\"\nprioritaet = \"bald\"\n\n\
                          [[frage]]\nid = \"wunsch\"\ntext = \"Was fehlt noch?\"\nart = \"text\"\ngestellt = \"2026-10-27\"\nprioritaet = \"bald\"\n\n\
                          [[frage]]\nid = \"konzept\"\ntext = \"Konzept so bestätigen?\"\nart = \"bestaetigen\"\ngestellt = \"2026-10-28\"\nprioritaet = \"irgendwann\"\n";

    fn probe(name: &str) -> Probe {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let wurzel = std::env::temp_dir().join(format!("dashboard-test-{}-antworten-{name}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&wurzel);
        let p = wurzel.join("projects").join("probe");
        std::fs::create_dir_all(p.join("daten")).unwrap();
        std::fs::write(p.join("projekt.toml"), "schema = 3\ntitle = \"Probe\"\nstatus = \"aktiv\"\nsummary = \"Nur für Tests.\"\n").unwrap();
        std::fs::write(p.join("daten").join("fragen.toml"), FRAGEN).unwrap();
        Probe { wurzel }
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

    async fn schicke(state: &AppState, req: Request<Body>) -> (StatusCode, Value) {
        let res = router(state.clone()).oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        (status, serde_json::from_str(&text).unwrap_or_else(|e| panic!("kein JSON ({e}): {text}")))
    }

    fn ids(v: &Value) -> Vec<String> {
        v["eintraege"].as_array().unwrap().iter().map(|e| e["id"].as_str().unwrap().to_string()).collect()
    }

    /// Abnahme A2: Der Inhaber beantwortet die aktuelle Frage im Browser; der
    /// wartende Chat wacht binnen einer Sekunde auf (Vorgang `antwort` mit
    /// Bezug `antwort:<slug>/<id>`), die nächste Frage rückt nach, der Chat
    /// liest die Antwort über die API.
    #[tokio::test]
    async fn abnahme_antwort_weckt_den_chat_und_die_naechste_rueckt_nach() {
        let p = probe("abnahme");
        let state = AppState::fuer_tests(&p.wurzel);
        let mut changes = state.events.subscribe();
        let (_, l) = schicke(&state, get("/api/anstehend?projekt=probe")).await;
        // Etappe 3a+: alle offenen Fragen, neutral nach Kennung; welche zuerst kommt, wählt die Oberfläche.
        let alle_vier = ids(&l);
        assert_eq!(alle_vier.len(), 4, "{l}");
        assert!(alle_vier.contains(&"farbe".to_string()) && alle_vier.contains(&"weiter".to_string()), "{alle_vier:?}");
        let f = l["eintraege"].as_array().unwrap().iter().find(|e| e["id"] == "farbe").unwrap();
        assert_eq!(f["vermerk"], Value::Null);
        assert_eq!(f["frage"], serde_json::json!({"art": "wahl", "optionen": ["Blau", "Grün"], "empfehlung": "Grün"}));
        let (_, st) = schicke(&state, get("/api/projekte/probe/stand")).await;
        assert_eq!(st["eintraege"].as_array().unwrap().iter().filter(|e| e["art"] == "frage").count(), 4);

        // Der Chat wartet (wie Warten.ps1).
        let s = state.clone();
        let warter = tokio::spawn(async move { schicke(&s, get("/api/vorgaenge/warten?fuer=probe&timeout=10")).await });
        let frist = Instant::now() + Duration::from_secs(2);
        while state.warteraum.anzahl("probe") != 1 {
            assert!(Instant::now() < frist, "Warter meldet sich nicht an");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        // Per curl gibt es keine Antwort.
        let (status, v) = schicke(&state, post("/api/antworten/probe/farbe", r#"{"gestellt": "2026-10-25", "antwort": "Blau"}"#)).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(SATZ_NUR_BROWSER)));
        assert!(!warter.is_finished());
        assert!(state.speicher().unwrap().antworten_lesen(None).unwrap().is_empty());

        // Der Inhaber antwortet.
        let start = Instant::now();
        let (status, v) = schicke(&state, post_browser("/api/antworten/probe/farbe", r#"{"gestellt": "2026-10-25", "antwort": "Blau"}"#)).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!((v["geaendert"].as_bool(), v["antwort"]["antwort"].as_str(), v["antwort"]["von"].as_str()), (Some(true), Some("Blau"), Some("browser")));
        let (status, w) = warter.await.unwrap();
        assert!(start.elapsed() < Duration::from_secs(1), "{:?}", start.elapsed());
        assert_eq!(status, StatusCode::OK, "{w}");
        let vorgang = &w["vorgaenge"][0];
        assert_eq!((vorgang["art"].as_str(), vorgang["slug"].as_str(), vorgang["von"].as_str()), (Some("antwort"), Some("probe"), Some("browser")));
        assert_eq!(vorgang["bezug"], "antwort:probe/farbe");
        assert_eq!(vorgang["titel"], "Der Inhaber hat gewählt „Blau“: Welche Farbe?");
        assert_eq!(vorgang["id"], v["antwort"]["vorgang"]);
        let paths: Vec<String> = std::iter::from_fn(|| changes.try_recv().ok()).filter(|c| c.area == "anstehend").map(|c| c.path).collect();
        assert_eq!(paths, ["anstehend/probe"]);

        // Die nächste Frage rückt nach, obwohl „farbe“ noch in fragen.toml steht.
        let (_, l) = schicke(&state, get("/api/anstehend?projekt=probe")).await;
        let rest = ids(&l);
        assert_eq!(rest.len(), 3, "{l}");
        assert!(!rest.contains(&"farbe".to_string()) && rest.contains(&"weiter".to_string()), "{rest:?}");
        let (_, st) = schicke(&state, get("/api/projekte/probe/stand")).await;
        assert_eq!(st["eintraege"].as_array().unwrap().iter().filter(|e| e["art"] == "frage").count(), 3);

        // Der Chat liest die Antwort: über die Frage und über den Vorgang.
        let (status, a) = schicke(&state, get("/api/antworten/probe/farbe")).await;
        assert_eq!(
            (status, a["antwort"].as_str(), a["gestellt"].as_str(), a["frage"].as_str()),
            (StatusCode::OK, Some("Blau"), Some("2026-10-25"), Some("Welche Farbe?"))
        );
        let (_, d) = schicke(&state, get(&format!("/api/vorgaenge/{}", vorgang["id"].as_str().unwrap()))).await;
        assert_eq!(d["antwort"], a);
        let (_, alle) = schicke(&state, get("/api/antworten?projekt=probe")).await;
        assert_eq!(alle["antworten"].as_array().unwrap().len(), 1);
        // Ein anderer Vorgang hat kein Feld `antwort`.
        let (_, x) = schicke(&state, post("/api/vorgaenge", r#"{"art": "abnahme", "slug": "probe", "titel": "x"}"#)).await;
        let (_, d) = schicke(&state, get(&format!("/api/vorgaenge/{}", x["vorgang"]["id"].as_str().unwrap()))).await;
        assert!(d.get("antwort").is_none(), "{d}");

        // Zweimal dieselbe Antwort: 200, nichts geändert, kein neuer Vorgang.
        let cursor = state.speicher().unwrap().cursor().unwrap();
        let (status, v) = schicke(&state, post_browser("/api/antworten/probe/farbe", r#"{"gestellt": "2026-10-25", "antwort": "Blau"}"#)).await;
        assert_eq!((status, v["geaendert"].as_bool()), (StatusCode::OK, Some(false)));
        // Eine andere Antwort: 409, die alte bleibt.
        let (status, v) = schicke(&state, post_browser("/api/antworten/probe/farbe", r#"{"gestellt": "2026-10-25", "antwort": "Grün"}"#)).await;
        assert_eq!((status, v["art"].as_str()), (StatusCode::CONFLICT, Some("konflikt")));
        assert_eq!(v["fehler"], "Diese Frage hast du schon beantwortet („Blau“). Eine andere Antwort geht nur, wenn der Chat die Frage neu stellt.");
        assert_eq!(state.speicher().unwrap().cursor().unwrap(), cursor);
        let (_, a) = schicke(&state, get("/api/antworten/probe/farbe")).await;
        assert_eq!(a["antwort"], "Blau");
    }

    /// Jede Art: was passt, wird gespeichert (so, wie es gespeichert wird);
    /// was nicht passt, gibt 422 mit Satz und schreibt nichts.
    #[tokio::test]
    async fn jede_art_wird_gegen_die_frage_geprueft() {
        let p = probe("arten");
        let state = AppState::fuer_tests(&p.wurzel);
        let falsch: &[(&str, &str, &str)] = &[
            ("farbe", r#"{"antwort": "Rot"}"#, "Diese Antwort passt nicht zur Frage. Möglich sind: Blau, Grün."),
            ("farbe", r#"{"antwort": "blau"}"#, "Diese Antwort passt nicht zur Frage. Möglich sind: Blau, Grün."),
            ("farbe", r#"{}"#, "Diese Antwort passt nicht zur Frage. Möglich sind: Blau, Grün."),
            ("weiter", r#"{"antwort": "vielleicht"}"#, SATZ_JA_NEIN),
            ("weiter", r#"{}"#, SATZ_JA_NEIN),
            ("wunsch", r#"{"antwort": "   "}"#, SATZ_TEXT_LEER),
            ("wunsch", r#"{"antwort": "a\u0007b"}"#, "Die Antwort („antwort“) enthält unsichtbare Steuerzeichen; bitte nur normalen Text schreiben."),
            ("konzept", r#"{"antwort": "nein"}"#, SATZ_BESTAETIGEN),
        ];
        for (id, koerper, satz) in falsch {
            let (status, v) = schicke(&state, post_browser(&format!("/api/antworten/probe/{id}"), koerper)).await;
            assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(*satz)), "{id} {koerper}");
        }
        let lang = format!(r#"{{"antwort": "{}"}}"#, "x".repeat(TEXT_HOECHSTENS + 1));
        let (status, v) = schicke(&state, post_browser("/api/antworten/probe/wunsch", &lang)).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("Die Antwort („antwort“) ist zu lang: höchstens 2000 Zeichen.")));
        assert!(state.speicher().unwrap().antworten_lesen(None).unwrap().is_empty(), "nichts gespeichert");
        assert_eq!(state.speicher().unwrap().cursor().unwrap(), 0, "kein Vorgang");

        let gut: &[(&str, &str, &str, &str)] = &[
            ("farbe", r#"{"gestellt": "2026-10-25", "antwort": " Grün "}"#, "Grün", "Der Inhaber hat gewählt „Grün“: Welche Farbe?"),
            ("weiter", r#"{"antwort": "Ja"}"#, "ja", "Der Inhaber hat geantwortet „ja“: Weitermachen?"),
            ("wunsch", r#"{"antwort": "  Ein Logo.\nUnd ein Bild.  "}"#, "Ein Logo.\nUnd ein Bild.", "Der Inhaber hat geantwortet (Text): Was fehlt noch?"),
            ("konzept", r#"{}"#, "bestaetigt", "Der Inhaber hat bestätigt: Konzept so bestätigen?"),
        ];
        for (id, koerper, gespeichert, titel) in gut {
            let (status, v) = schicke(&state, post_browser(&format!("/api/antworten/probe/{id}"), koerper)).await;
            assert_eq!(status, StatusCode::OK, "{id}: {v}");
            assert_eq!(v["antwort"]["antwort"].as_str(), Some(*gespeichert), "{id}");
            let (_, d) = schicke(&state, get(&format!("/api/vorgaenge/{}", v["antwort"]["vorgang"].as_str().unwrap()))).await;
            assert_eq!(d["vorgang"]["titel"].as_str(), Some(*titel), "{id}");
        }
        // Bestätigen auch mit dem Wort selbst: dieselbe Antwort, nichts geändert.
        let (status, v) = schicke(&state, post_browser("/api/antworten/probe/konzept", r#"{"antwort": "bestaetigt"}"#)).await;
        assert_eq!((status, v["geaendert"].as_bool()), (StatusCode::OK, Some(false)));
        // Alle beantwortet: keine Frage mehr offen, die Ampel ist grün.
        let (_, l) = schicke(&state, get("/api/anstehend?projekt=probe")).await;
        assert!(ids(&l).is_empty(), "{l}");
        let (_, st) = schicke(&state, get("/api/projekte/probe/stand")).await;
        assert_eq!((st["eintraege"].clone(), st["gesamt"]["ampel"].as_str()), (serde_json::json!([]), Some("gruen")));
        assert_eq!(state.speicher().unwrap().cursor().unwrap(), 4, "je Antwort ein Vorgang");
    }

    /// 404, wenn die Frage nicht (mehr) in fragen.toml steht; 409, wenn sie
    /// neu gestellt wurde; Neustellen mit neuem Datum macht sie wieder offen.
    #[tokio::test]
    async fn nicht_offen_neu_gestellt_und_falsche_adressen() {
        let p = probe("offen");
        let state = AppState::fuer_tests(&p.wurzel);
        let (status, v) = schicke(&state, post_browser("/api/antworten/probe/gibt-es-nicht", r#"{"antwort": "ja"}"#)).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::NOT_FOUND, Some(SATZ_NICHT_OFFEN)));
        let (status, _) = schicke(&state, post_browser("/api/antworten/fremd/weiter", r#"{"antwort": "ja"}"#)).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "Projekt ohne fragen.toml");
        let (status, v) = schicke(&state, post_browser("/api/antworten/probe/weiter", r#"{"gestellt": "2026-10-01", "antwort": "ja"}"#)).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::CONFLICT, Some(SATZ_NEU_GESTELLT)));
        for (pfad, soll) in [
            ("/api/antworten/Probe/weiter", StatusCode::UNPROCESSABLE_ENTITY),
            ("/api/antworten/probe/-weiter", StatusCode::UNPROCESSABLE_ENTITY),
            ("/api/antworten/probe/..", StatusCode::UNPROCESSABLE_ENTITY),
            // Hotfix Prozent: `%` im Pfad unter /api ist schon an der Schleuse 400.
            ("/api/antworten/probe/%2E%2E%5Cx", StatusCode::BAD_REQUEST),
        ] {
            let (status, v) = schicke(&state, post_browser(pfad, r#"{"antwort": "ja"}"#)).await;
            assert_eq!(status, soll, "{pfad}: {v}");
        }
        let (status, _) = schicke(&state, post_browser("/api/antworten/probe/weiter", r#"{"antwort": "ja", "geheim": 1}"#)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "unbekanntes Feld");
        let req = Request::post("/api/antworten/probe/weiter")
            .header("host", "localhost:8787")
            .header("origin", "http://localhost:8787")
            .body(Body::from(r#"{"antwort": "ja"}"#))
            .unwrap();
        assert_eq!(router(state.clone()).oneshot(req).await.unwrap().status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        // Beantworten, dann stellt der Chat dieselbe Kennung neu (neues Datum): wieder offen.
        let (status, _) = schicke(&state, post_browser("/api/antworten/probe/weiter", r#"{"gestellt": "2026-10-26", "antwort": "nein"}"#)).await;
        assert_eq!(status, StatusCode::OK);
        let datei = p.wurzel.join("projects").join("probe").join("daten").join("fragen.toml");
        std::fs::write(&datei, FRAGEN.replace("gestellt = \"2026-10-26\"", "gestellt = \"2026-10-29\"")).unwrap();
        let (_, l) = schicke(&state, get("/api/anstehend?projekt=probe&art=frage")).await;
        let offen = ids(&l);
        assert!(
            offen.contains(&"farbe".to_string()) && offen.contains(&"weiter".to_string()),
            "neu gestellt: weiter ist wieder offen (welche zuerst kommt, wählt die Oberfläche): {offen:?}"
        );
        let (status, v) = schicke(&state, post_browser("/api/antworten/probe/weiter", r#"{"gestellt": "2026-10-29", "antwort": "ja"}"#)).await;
        assert_eq!((status, v["geaendert"].as_bool()), (StatusCode::OK, Some(true)), "neu gestellt: eigene Antwort");
        let (_, a) = schicke(&state, get("/api/antworten/probe/weiter")).await;
        assert_eq!((a["antwort"].as_str(), a["gestellt"].as_str()), (Some("ja"), Some("2026-10-29")), "die neueste");
        // Der Chat nimmt die Frage heraus: weg aus der Liste, die Antwort bleibt lesbar.
        std::fs::write(&datei, "").unwrap();
        let (status, v) = schicke(&state, post_browser("/api/antworten/probe/weiter", r#"{"antwort": "ja"}"#)).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::NOT_FOUND, Some(SATZ_NICHT_OFFEN)));
        let (status, _) = schicke(&state, get("/api/antworten/probe/weiter")).await;
        assert_eq!(status, StatusCode::OK);
        let (status, v) = schicke(&state, get("/api/antworten/probe/farbe")).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::NOT_FOUND, Some(SATZ_KEINE_ANTWORT)));
        let (status, _) = schicke(&state, get("/api/antworten?projekt=Gross")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    /// Alle Wege nur mit Windows-Anmeldung (401); fremde Seiten 403; ohne Datenspeicher 503.
    #[tokio::test]
    async fn geschuetzt_fremde_seiten_und_ohne_speicher() {
        let state = AppState::fuer_tests(&beispiel());
        for req in [
            get("/api/antworten"),
            get("/api/antworten/neu-form/schlusssatz"),
            post_browser("/api/antworten/neu-form/schlusssatz", r#"{"antwort": "Herzliche Grüße vom Vorstand"}"#),
            // „Etwas anderes …“: dieselbe Anmeldung wie ein Klick.
            post_browser("/api/antworten/neu-form/schlusssatz", r#"{"antwort": "etwas-anderes", "eigene": "Anders."}"#),
        ] {
            let pfad = req.uri().to_string();
            let res = crate::app::router(state.clone()).oneshot(req).await.unwrap();
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{pfad}");
        }
        let fremd = Request::post("/api/antworten/neu-form/schlusssatz")
            .header("host", "127.0.0.1:8787")
            .header("sec-fetch-site", "cross-site")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"antwort": "Herzliche Grüße vom Vorstand"}"#))
            .unwrap();
        assert_eq!(router(state.clone()).oneshot(fremd).await.unwrap().status(), StatusCode::FORBIDDEN);
        let fremd_lesen =
            Request::get("/api/antworten").header("host", "127.0.0.1:8787").header("origin", "https://fremd.invalid").body(Body::empty()).unwrap();
        assert_eq!(router(state.clone()).oneshot(fremd_lesen).await.unwrap().status(), StatusCode::FORBIDDEN);
        assert!(state.speicher().unwrap().antworten_lesen(None).unwrap().is_empty());
        // Mit den Beispiel-Dateien: die Wahl aus tests/beispiel geht.
        let (status, v) =
            schicke(&state, post_browser("/api/antworten/neu-form/schlusssatz", r#"{"gestellt": "2026-10-25", "antwort": "Wir sehen uns beim Sommerfest!"}"#))
                .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let ohne = AppState::fuer_tests_ohne_speicher(&beispiel(), "kaputt");
        for req in [get("/api/antworten"), post_browser("/api/antworten/neu-form/konzept", "{}")] {
            let (status, v) = schicke(&ohne, req).await;
            assert_eq!((status, v["art"].as_str()), (StatusCode::SERVICE_UNAVAILABLE, Some("speicher-fehlt")));
        }
    }

    /// Anmerkung: bei wahl, ja-nein und bestätigen gespeichert und für den
    /// Chat lesbar (Frage, Liste, Vorgang); gleich geprüft wie ein Text; bei
    /// text 422. Dieselbe Antwort mit derselben Anmerkung: 200 unverändert;
    /// mit einer anderen oder ohne: 409.
    #[tokio::test]
    async fn anmerkung_zur_antwort() {
        let p = probe("notiz");
        let state = AppState::fuer_tests(&p.wurzel);
        let falsch: &[(&str, String, &str)] = &[
            (
                "weiter",
                r#"{"antwort": "ja", "notiz": "a\u0007b"}"#.into(),
                "Die Anmerkung („notiz“) enthält unsichtbare Steuerzeichen; bitte nur normalen Text schreiben.",
            ),
            (
                "weiter",
                format!(r#"{{"antwort": "ja", "notiz": "{}"}}"#, "x".repeat(NOTIZ_HOECHSTENS + 1)),
                "Die Anmerkung („notiz“) ist zu lang: höchstens 1000 Zeichen.",
            ),
            ("wunsch", r#"{"antwort": "Ein Logo.", "notiz": "dazu"}"#.into(), SATZ_NOTIZ_BEI_TEXT),
        ];
        for (id, koerper, satz) in falsch {
            let (status, v) = schicke(&state, post_browser(&format!("/api/antworten/probe/{id}"), koerper)).await;
            assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(*satz)), "{id}");
        }
        assert_eq!(state.speicher().unwrap().cursor().unwrap(), 0, "nichts gespeichert");
        // Genau 1000 Zeichen gehen; Leerzeichen am Rand fallen weg, Zeilenumbrüche bleiben.
        let (status, v) =
            schicke(&state, post_browser("/api/antworten/probe/weiter", r#"{"antwort": "ja", "notiz": "  Erst ab Montag.\nBitte melden.  "}"#)).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["antwort"]["notiz"], "Erst ab Montag.\nBitte melden.");
        let (status, v) =
            schicke(&state, post_browser("/api/antworten/probe/farbe", &format!(r#"{{"antwort": "Blau", "notiz": "{}"}}"#, "y".repeat(NOTIZ_HOECHSTENS))))
                .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let vorgang = v["antwort"]["vorgang"].as_str().unwrap().to_string();
        // Leere Anmerkung zählt als keine; bei bestätigen geht eine Anmerkung.
        let (_, v) = schicke(&state, post_browser("/api/antworten/probe/wunsch", r#"{"antwort": "Ein Logo.", "notiz": "  "}"#)).await;
        assert_eq!(v["antwort"]["notiz"], Value::Null, "{v}");
        let (status, v) = schicke(&state, post_browser("/api/antworten/probe/konzept", r#"{"notiz": "Passt so."}"#)).await;
        assert_eq!((status, v["antwort"]["notiz"].as_str()), (StatusCode::OK, Some("Passt so.")));

        // Der Chat liest sie: über die Frage, die Liste und den Vorgang.
        let (_, a) = schicke(&state, get("/api/antworten/probe/weiter")).await;
        assert_eq!(a["notiz"], "Erst ab Montag.\nBitte melden.");
        let (_, alle) = schicke(&state, get("/api/antworten?projekt=probe")).await;
        assert!(alle["antworten"].as_array().unwrap().iter().any(|a| a["notiz"] == "Passt so."), "{alle}");
        let (_, d) = schicke(&state, get(&format!("/api/vorgaenge/{vorgang}"))).await;
        assert_eq!(d["antwort"]["notiz"].as_str().map(str::len), Some(NOTIZ_HOECHSTENS));
        assert_eq!(d["vorgang"]["titel"], "Der Inhaber hat gewählt „Blau“ (mit Anmerkung): Welche Farbe?");

        // Zweimal: gleich ist 200 unverändert, eine andere oder keine Anmerkung 409.
        let cursor = state.speicher().unwrap().cursor().unwrap();
        let (status, v) = schicke(&state, post_browser("/api/antworten/probe/weiter", r#"{"antwort": "ja", "notiz": "Erst ab Montag.\nBitte melden."}"#)).await;
        assert_eq!((status, v["geaendert"].as_bool()), (StatusCode::OK, Some(false)));
        for koerper in [r#"{"antwort": "ja", "notiz": "Sofort."}"#, r#"{"antwort": "ja"}"#] {
            let (status, v) = schicke(&state, post_browser("/api/antworten/probe/weiter", koerper)).await;
            assert_eq!(status, StatusCode::CONFLICT, "{koerper}");
            assert_eq!(
                v["fehler"],
                "Diese Frage hast du schon beantwortet („ja“), mit einer anderen Anmerkung. Ändern geht nur, wenn der Chat die Frage neu stellt."
            );
        }
        let (status, v) =
            schicke(&state, post_browser("/api/antworten/probe/weiter", r#"{"antwort": "nein", "notiz": "Erst ab Montag.\nBitte melden."}"#)).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert!(v["fehler"].as_str().unwrap().contains("Eine andere Antwort geht nur"), "{v}");
        assert_eq!(state.speicher().unwrap().cursor().unwrap(), cursor, "kein neuer Vorgang");
    }

    /// Entscheidung „Etwas anderes …“: Senden mit Text gibt eine
    /// Antwort mit `antwort = etwas-anderes` und `eigene`, einen Vorgang für
    /// den Chat mit Zeitstempel (er wacht auf wie bei einem Klick), und
    /// Anstehend ist aktualisiert. Bei einem Klick ist `eigene` null.
    #[tokio::test]
    async fn etwas_anderes_geht_den_weg_eines_klicks() {
        let p = probe("etwas-anderes");
        let state = AppState::fuer_tests(&p.wurzel);
        let mut changes = state.events.subscribe();
        // Der Chat wartet (wie Warten.ps1).
        let s = state.clone();
        let warter = tokio::spawn(async move { schicke(&s, get("/api/vorgaenge/warten?fuer=probe&timeout=10")).await });
        let frist = Instant::now() + Duration::from_secs(2);
        while state.warteraum.anzahl("probe") != 1 {
            assert!(Instant::now() < frist, "Warter meldet sich nicht an");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        // Wie ein Klick nur aus dem Browser.
        let koerper = r#"{"gestellt": "2026-10-25", "antwort": "etwas-anderes", "eigene": "  Lieber Grau,\nwie das Logo.  "}"#;
        let (status, v) = schicke(&state, post("/api/antworten/probe/farbe", koerper)).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(SATZ_NUR_BROWSER)));
        let start = Instant::now();
        let (status, v) = schicke(&state, post_browser("/api/antworten/probe/farbe", koerper)).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let a = &v["antwort"];
        assert_eq!(
            (a["antwort"].as_str(), a["eigene"].as_str(), a["von"].as_str()),
            (Some("etwas-anderes"), Some("Lieber Grau,\nwie das Logo."), Some("browser"))
        );
        assert!(chrono::DateTime::parse_from_rfc3339(a["zeit"].as_str().unwrap()).is_ok(), "Zeitstempel: {a}");
        let (status, w) = warter.await.unwrap();
        assert!(start.elapsed() < Duration::from_secs(1), "{:?}", start.elapsed());
        assert_eq!(status, StatusCode::OK, "{w}");
        let vorgang = &w["vorgaenge"][0];
        assert_eq!((vorgang["art"].as_str(), vorgang["bezug"].as_str()), (Some("antwort"), Some("antwort:probe/farbe")));
        assert_eq!(vorgang["titel"], "Der Inhaber hat etwas anderes geschrieben (eigene Worte): Welche Farbe?");
        assert_eq!(vorgang["id"], a["vorgang"]);
        assert!(vorgang["erstellt"].as_str().is_some_and(|z| chrono::DateTime::parse_from_rfc3339(z).is_ok()), "{vorgang}");
        // Anstehend ist aktualisiert: die Frage ist beantwortet, ein change ging hinaus.
        let paths: Vec<String> = std::iter::from_fn(|| changes.try_recv().ok()).filter(|c| c.area == "anstehend").map(|c| c.path).collect();
        assert_eq!(paths, ["anstehend/probe"]);
        let (_, l) = schicke(&state, get("/api/anstehend?projekt=probe")).await;
        assert!(!ids(&l).contains(&"farbe".to_string()), "{l}");
        // Der Chat liest den Text: über die Frage, die Liste und den Vorgang.
        let (_, gelesen) = schicke(&state, get("/api/antworten/probe/farbe")).await;
        assert_eq!(gelesen["eigene"], "Lieber Grau,\nwie das Logo.");
        let (_, d) = schicke(&state, get(&format!("/api/vorgaenge/{}", vorgang["id"].as_str().unwrap()))).await;
        assert_eq!(d["antwort"], gelesen);
        let (_, alle) = schicke(&state, get("/api/antworten?projekt=probe")).await;
        assert_eq!(alle["antworten"][0]["eigene"], "Lieber Grau,\nwie das Logo.");
        // Doppelklick: 200 unverändert; ein anderer Text: 409 mit den eigenen Worten im Satz.
        let cursor = state.speicher().unwrap().cursor().unwrap();
        let (status, v) = schicke(&state, post_browser("/api/antworten/probe/farbe", koerper)).await;
        assert_eq!((status, v["geaendert"].as_bool()), (StatusCode::OK, Some(false)));
        let (status, v) = schicke(&state, post_browser("/api/antworten/probe/farbe", r#"{"antwort": "etwas-anderes", "eigene": "Doch Blau."}"#)).await;
        assert_eq!(status, StatusCode::CONFLICT, "{v}");
        assert!(v["fehler"].as_str().unwrap().contains("(„Lieber Grau, wie das Logo.“)"), "{v}");
        assert_eq!(state.speicher().unwrap().cursor().unwrap(), cursor, "kein neuer Vorgang");

        // Auch bei ja-nein und bestätigen, mit Anmerkung daneben; ein Klick hat eigene = null.
        let (status, v) = schicke(
            &state,
            post_browser("/api/antworten/probe/weiter", r#"{"antwort": "etwas-anderes", "eigene": "Erst nach dem Urlaub.", "notiz": "Ab dem 10."}"#),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!((v["antwort"]["eigene"].as_str(), v["antwort"]["notiz"].as_str()), (Some("Erst nach dem Urlaub."), Some("Ab dem 10.")));
        let (_, d) = schicke(&state, get(&format!("/api/vorgaenge/{}", v["antwort"]["vorgang"].as_str().unwrap()))).await;
        assert_eq!(d["vorgang"]["titel"], "Der Inhaber hat etwas anderes geschrieben (eigene Worte) (mit Anmerkung): Weitermachen?");
        let (status, v) =
            schicke(&state, post_browser("/api/antworten/probe/konzept", r#"{"antwort": "etwas-anderes", "eigene": "Bitte zuerst den Plan zeigen."}"#)).await;
        assert_eq!((status, v["antwort"]["antwort"].as_str()), (StatusCode::OK, Some("etwas-anderes")), "{v}");
        let p2 = probe("etwas-anderes-klick");
        let state2 = AppState::fuer_tests(&p2.wurzel);
        let (_, v) = schicke(&state2, post_browser("/api/antworten/probe/weiter", r#"{"antwort": "ja"}"#)).await;
        assert!(v["antwort"].get("eigene").is_some_and(Value::is_null), "{v}");
    }

    /// „Etwas anderes …“: leer oder nur Leerzeichen 422 mit Satz, 20 001
    /// Zeichen 422 (nicht 413: der Weg nimmt mehr als 16 KB), genau 20 000
    /// gehen (auch als Umlaute); nicht bei text; eigene nur mit etwas-anderes.
    #[tokio::test]
    async fn etwas_anderes_wird_geprueft() {
        let p = probe("etwas-anderes-pruefen");
        let state = AppState::fuer_tests(&p.wurzel);
        let falsch: Vec<(&str, String, &str)> = vec![
            ("farbe", r#"{"antwort": "etwas-anderes"}"#.into(), SATZ_EIGENE_LEER),
            ("farbe", r#"{"antwort": "etwas-anderes", "eigene": ""}"#.into(), SATZ_EIGENE_LEER),
            ("weiter", r#"{"antwort": "etwas-anderes", "eigene": "  \n\t  "}"#.into(), SATZ_EIGENE_LEER),
            (
                "farbe",
                format!(r#"{{"antwort": "etwas-anderes", "eigene": "{}"}}"#, "x".repeat(EIGENE_HOECHSTENS + 1)),
                "Deine Antwort in eigenen Worten („eigene“) ist zu lang: höchstens 20000 Zeichen.",
            ),
            (
                "farbe",
                r#"{"antwort": "etwas-anderes", "eigene": "a\u0007b"}"#.into(),
                "Deine Antwort in eigenen Worten („eigene“) enthält unsichtbare Steuerzeichen; bitte nur normalen Text schreiben.",
            ),
            ("wunsch", r#"{"antwort": "etwas-anderes", "eigene": "Ein Logo."}"#.into(), SATZ_EIGENE_BEI_TEXT),
            ("farbe", r#"{"antwort": "Blau", "eigene": "Und Grün."}"#.into(), SATZ_EIGENE_OHNE_ETWAS_ANDERES),
        ];
        for (id, koerper, satz) in &falsch {
            let (status, v) = schicke(&state, post_browser(&format!("/api/antworten/probe/{id}"), koerper)).await;
            assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(*satz)), "{id} {}", &koerper[..koerper.len().min(80)]);
        }
        assert_eq!(state.speicher().unwrap().cursor().unwrap(), 0, "nichts gespeichert");
        // Genau 20 000 Zeichen gehen, auch als Umlaute (40 000 Bytes, mehr als 16 KB).
        let lang = "ä".repeat(EIGENE_HOECHSTENS);
        let (status, v) = schicke(&state, post_browser("/api/antworten/probe/farbe", &format!(r#"{{"antwort": "etwas-anderes", "eigene": "{lang}"}}"#))).await;
        assert_eq!(status, StatusCode::OK, "{}", &v.to_string()[..200]);
        assert_eq!(v["antwort"]["eigene"].as_str().map(|t| t.chars().count()), Some(EIGENE_HOECHSTENS));
        // Ein leeres eigene bei einem Klick zählt als keines.
        let (status, v) = schicke(&state, post_browser("/api/antworten/probe/weiter", r#"{"antwort": "nein", "eigene": " "}"#)).await;
        assert_eq!((status, v["antwort"]["eigene"].clone()), (StatusCode::OK, Value::Null), "{v}");
        // Mehr als 64 KiB bleibt zu groß (413), wie auf den Wegen der Module.
        let riesig = format!(r#"{{"antwort": "etwas-anderes", "eigene": "{}"}}"#, "x".repeat(70 * 1024));
        let res = router(state.clone()).oneshot(post_browser("/api/antworten/probe/konzept", &riesig)).await.unwrap();
        assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    /// Archiv über den Lauf (gestellte Zeit, acht Tage später): die Frage bleibt beantwortet.
    #[tokio::test]
    async fn archiv_lauf_haelt_die_frage_beantwortet() {
        let p = probe("ablage");
        let state = AppState::fuer_tests(&p.wurzel);
        schicke(&state, post_browser("/api/antworten/probe/farbe", r#"{"antwort": "Blau"}"#)).await;
        assert_eq!(archiv_lauf(&state, aufgaben::jetzt()).await.unwrap(), 0);
        assert_eq!(archiv_lauf(&state, aufgaben::jetzt() + chrono::Duration::days(8)).await.unwrap(), 1);
        let (_, alle) = schicke(&state, get("/api/antworten")).await;
        assert!(alle["antworten"].as_array().unwrap().is_empty());
        let (_, l) = schicke(&state, get("/api/anstehend?projekt=probe")).await;
        let offen = ids(&l);
        assert!(!offen.contains(&"farbe".to_string()) && offen.contains(&"weiter".to_string()), "farbe bleibt beantwortet: {offen:?}");
        let (status, a) = schicke(&state, get("/api/antworten/probe/farbe")).await;
        assert_eq!((status, a["antwort"].as_str()), (StatusCode::OK, Some("Blau")));
        let (_, daten) = schicke(&state, get("/api/daten")).await;
        let zeile = daten["tabellen"].as_array().unwrap().iter().find(|t| t["name"] == "antworten_archiv").unwrap().clone();
        assert_eq!(zeile["eintraege"], 1, "{zeile}");
        assert!(zeile["beschreibung"].as_str().unwrap().contains("nie gelöscht"), "{zeile}");
    }
}
