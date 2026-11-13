//! `/api/arbeiten`: Arbeiten der Chats (Etappe A1, Konzept „Anstehendes“).
//!
//! Ein Chat meldet per `curl`, was er tut oder vorhat (`POST /api/arbeiten`),
//! und schreibt es fort (`POST /api/arbeiten/<id>` mit den Feldern, die sich
//! ändern). Der Inhaber liest nur: Aus dem Browser (`Origin` oder `Sec-Fetch-Site`)
//! nehmen die schreibenden Wege nichts an (422). `GET /api/arbeiten[?projekt=]`
//! liefert alle außerhalb des Archivs.
//!
//! Seit T3 (Zweig teilschritte) ist `von` auf allen schreibenden Wegen Pflicht
//! und hat die Form `chat:<name>` (`wer_ruft`, sonst 422). Hat eine Arbeit
//! einen Besitzer, ändert nur er sie (sonst 409 mit `besitzer` und `seit`);
//! den Titel einer Arbeit ohne Besitzer ändert nur der Chat ihres Projekts
//! (sonst 403). Ein neuer Titel kommt mit altem und neuem Wert in den Verlauf
//! der Arbeit und erzeugt keine eigene Meldung.
//!
//! Etappe B8: `etappe` (`<projekt>/<kennung>`) muss in `daten/plan.toml` jenes
//! Projekts stehen (sonst 422); setzen und ändern darf sie nur, wer die Arbeit
//! verantwortet (sonst 409). Jeder Wechsel des Stands steht im Verlauf.
//!
//! Teilschritte (T3): `POST /api/arbeiten/<id>/schritte` legt einen an (201
//! mit `nr`), `…/schritte/<nr>/uebernehmen` macht ihn zu dem des Chats,
//! `…/schritte/<nr>` ändert ihn (Regeln in `speicher::schritte`). Anbieten und
//! Zurückwecken legen dabei einen Vorgang der Art `schritt` für den Empfänger
//! an, der ihn über sein Warten weckt.
//!
//! Geschützt wie `/api/vorgaenge` (Windows-Anmeldung, `sicherheit::geschuetzt`)
//! und ohne Herkunftskopf für curl erlaubt (`sicherheit::herkunft_pflicht`);
//! fremde Seiten bleiben 403. Schreiben nur mit `Content-Type: application/json`.
//! Grenze: höchstens 30 nicht fertige Arbeiten je Projekt (darüber 429).
//! Fertige ziehen nach sieben Tagen ins Archiv (Zeitplan in `aufgaben::spawn_ablauf`).
//! Jede Änderung sendet `change` mit Bereich `anstehend`.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};

use super::aufgaben::{frist_pruefen, stufe_pruefen};
use super::vertrag::{Arbeit, ArbeitAngelegt, ArbeitGeaendert, ArbeitenListe, ArbeitsStand, Besetzt, SchrittAngelegt};
use super::vorgaenge::{TITEL_HOECHSTENS, VON_HOECHSTENS, abfrage, angegeben, aus_browser, json_kopf_pruefen, slug_pruefen, text_pruefen};
use crate::app::AppState;
use crate::fehler::{AppError, FehlerSatz};
use crate::speicher::arbeiten::{ArbeitAenderung, Fortschreiben, NeueArbeit, SATZ_IM_ARCHIV, SATZ_WARTET_AUF_FEHLT};
use crate::speicher::aufgaben::{self, Zeit};
use crate::speicher::schritte::{SchrittAenderung, SchrittErgebnis, VERWAIST_NACH_STUNDEN};
use crate::speicher::{Speicher, SpeicherFehler};
use crate::vertrag::daten::{DatenInhalt, ist_kennung};

/// Eine Notiz höchstens so viele Zeichen.
pub const NOTIZ_HOECHSTENS: usize = 1000;

pub const SATZ_NUR_CHATS: &str = "Arbeiten melden nur die Chats (per curl); der Inhaber liest sie nur.";
pub const SATZ_STAND: &str = "„stand“ ist laeuft, wartet, als-naechstes oder fertig.";
pub const SATZ_WARTET_AUF: &str = "„wartet_auf“ ist inhaber, extern oder chat:<name> (etwa chat:dashboard).";
pub const SATZ_VERWEIS: &str = "„verweis“ ist die Kennung einer Frage oder Aufgabe, etwa a-20261029-0001 oder schlusssatz.";
const SATZ_NICHT_DA: &str = "Diese Arbeit gibt es nicht.";
const SATZ_SCHRITT_NICHT_DA: &str = "Diesen Schritt gibt es in dieser Arbeit nicht.";
/// `von` fehlt oder hat nicht die Form `chat:<name>` (`wer_ruft`).
pub const SATZ_VON: &str = "„von“ fehlt oder hat die falsche Form. Ergänze im Körper \"von\": \"chat:<name>\" mit deinem Chat, etwa \"von\": \"chat:dashboard\" (<name> aus a–z, 0–9 und Bindestrich).";

const MUSTER_NEU: &str = r#"{"projekt": "dashboard", "von": "chat:dashboard", "titel": "…", "stand": "laeuft", "wartet_auf": "inhaber", "verweis": "a-20261029-0001", "prioritaet": "bald", "frist": "2026-11-05", "notiz": "…", "etappe": "dashboard/b8"} (Pflicht: projekt, von, titel, stand; wartet_auf nur bei stand = wartet)"#;
const MUSTER_AENDERN: &str = r#"{"von": "chat:dashboard", "titel": "…", "stand": "wartet", "wartet_auf": "inhaber", "verweis": "…", "prioritaet": "jetzt", "frist": "2026-11-05", "notiz": "…", "etappe": "dashboard/b8"} (Pflicht: von; sonst nur die Felder, die sich ändern; null oder "" leert ein Feld, außer beim Titel)"#;
const MUSTER_SCHRITT_NEU: &str = r#"{"von": "chat:dashboard", "titel": "6b bauen", "fuer": "chat:architektur"} (Pflicht: von, titel)"#;
const MUSTER_UEBERNEHMEN: &str = r#"{"von": "chat:architektur", "statt": "chat:dashboard"} (Pflicht: von; statt nur bei einem verwaisten Schritt)"#;
const MUSTER_SCHRITT: &str = r#"{"von": "chat:dashboard", "stand": "wartet", "wartet_auf": "schritt:2", "verweis": "…", "notiz": "…", "anbieten": "chat:architektur", "freigeben": true} (Pflicht: von; sonst nur, was sich ändert)"#;
pub const SATZ_TITEL_FEHLT: &str = "Der Titel fehlt.";
pub const SATZ_TITEL_NUR_EIGENER_CHAT: &str = "Den Titel einer Arbeit ohne Besitzer ändert nur der Chat ihres Projekts (von: chat:<projekt>).";
pub const SATZ_ETAPPE: &str = "„etappe“ hat die Form <projekt>/<kennung>, etwa dashboard/b8 (die Kennung einer Etappe aus daten/plan.toml jenes Projekts).";
pub const SATZ_ETAPPE_NUR_EIGENER_CHAT: &str =
    "Die Etappe einer Arbeit setzt nur, wer sie verantwortet: ihr Besitzer, und ohne Besitzer der Chat ihres Projekts (von: chat:<projekt>).";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NeueArbeitKoerper {
    projekt: String,
    titel: String,
    stand: String,
    #[serde(default)]
    von: Option<String>,
    #[serde(default)]
    wartet_auf: Option<String>,
    #[serde(default)]
    verweis: Option<String>,
    #[serde(default)]
    prioritaet: Option<String>,
    #[serde(default)]
    frist: Option<String>,
    #[serde(default)]
    notiz: Option<String>,
    #[serde(default)]
    etappe: Option<String>,
}

/// Teiländerung: fehlt ein Feld, bleibt es; `null` oder `""` leert es.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct AenderungKoerper {
    /// Pflichtfeld der Arbeit: `null` oder `""` geben 422, statt zu leeren.
    #[serde(default, deserialize_with = "doppelt")]
    titel: Option<Option<String>>,
    #[serde(default)]
    stand: Option<String>,
    #[serde(default, deserialize_with = "doppelt")]
    wartet_auf: Option<Option<String>>,
    #[serde(default, deserialize_with = "doppelt")]
    verweis: Option<Option<String>>,
    #[serde(default)]
    prioritaet: Option<String>,
    #[serde(default, deserialize_with = "doppelt")]
    frist: Option<Option<String>>,
    #[serde(default, deserialize_with = "doppelt")]
    notiz: Option<Option<String>>,
    #[serde(default, deserialize_with = "doppelt")]
    etappe: Option<Option<String>>,
    /// Pflicht (`wer_ruft`).
    #[serde(default)]
    von: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NeuerSchrittKoerper {
    #[serde(default)]
    von: Option<String>,
    titel: String,
    #[serde(default)]
    fuer: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct UebernehmenKoerper {
    #[serde(default)]
    von: Option<String>,
    #[serde(default)]
    statt: Option<String>,
}

/// Änderung an einem Schritt: fehlt ein Feld, bleibt es; `null` oder `""` leert es.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct SchrittKoerper {
    #[serde(default)]
    von: Option<String>,
    #[serde(default)]
    stand: Option<String>,
    #[serde(default, deserialize_with = "doppelt")]
    wartet_auf: Option<Option<String>>,
    #[serde(default, deserialize_with = "doppelt")]
    verweis: Option<Option<String>>,
    #[serde(default, deserialize_with = "doppelt")]
    notiz: Option<Option<String>>,
    #[serde(default)]
    anbieten: Option<String>,
    #[serde(default)]
    freigeben: bool,
}

/// Unterscheidet „fehlt“ (`None`) von `null` (`Some(None)`).
fn doppelt<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

/// JSON-Körper lesen; ins Protokoll nur die Stelle des Fehlers.
fn koerper_lesen<T: DeserializeOwned>(koerper: &[u8], muster: &str) -> Result<T, AppError> {
    serde_json::from_slice(koerper).map_err(|e| {
        tracing::debug!("Arbeiten: Körper unlesbar ({:?}, Zeile {}, Spalte {})", e.classify(), e.line(), e.column());
        AppError::unlesbar(format!("Der Inhalt ist kein passendes JSON. Erwartet wird etwa {muster}."))
    })
}

/// Der Titel beim Anlegen und Fortschreiben: getrimmt, nicht leer, höchstens
/// `TITEL_HOECHSTENS` Zeichen.
fn titel_pruefen(wert: &str) -> Result<String, AppError> {
    let titel = wert.trim().to_string();
    if titel.is_empty() {
        return Err(AppError::unlesbar(SATZ_TITEL_FEHLT));
    }
    text_pruefen("titel", &titel, TITEL_HOECHSTENS, false)?;
    Ok(titel)
}

fn stand_pruefen(wort: &str) -> Result<ArbeitsStand, AppError> {
    ArbeitsStand::aus_wort(wort.trim()).ok_or_else(|| AppError::unlesbar(SATZ_STAND))
}

/// `chat:<name>`, `<name>` wie ein Slug (a–z, 0–9, Bindestrich, höchstens 64 Zeichen).
pub(crate) fn ist_chat(wert: &str) -> bool {
    wert.strip_prefix("chat:").is_some_and(|s| slug_pruefen(s).is_ok())
}

/// Wer ruft (f4, `wer_ruft`): `von` ist Pflicht und hat die Form `chat:<name>`,
/// sonst 422. Die eine Stelle für alle schreibenden Arbeiten- und Schritt-Wege;
/// `anbieten` und `fuer` prüft `chat_pruefen` gleich. Heute eine Angabe ohne
/// Nachweis; mit dem Chat-Schlüssel (C1) kommt der Name aus dem Schlüssel.
pub fn wer_ruft(von: Option<String>) -> Result<String, AppError> {
    angegeben(von).filter(|v| ist_chat(v)).ok_or_else(|| AppError::unlesbar(SATZ_VON))
}

/// `anbieten`, `fuer`: dieselbe Form wie `von` (nie der Inhaber).
fn chat_pruefen(feld: &str, wert: &str) -> Result<String, AppError> {
    if ist_chat(wert) {
        return Ok(wert.to_string());
    }
    Err(AppError::unlesbar(format!("„{feld}“ hat die Form chat:<name>, etwa chat:architektur (<name> aus a–z, 0–9 und Bindestrich).")))
}

/// `inhaber`, `extern` oder `chat:<slug>`.
pub fn wartet_auf_pruefen(wert: &str) -> Result<String, AppError> {
    let gut = matches!(wert, crate::anstehend::INHABER | "extern") || ist_chat(wert);
    if gut { Ok(wert.to_string()) } else { Err(AppError::unlesbar(SATZ_WARTET_AUF)) }
}

/// Bei Schritten: `schritt:<nr>`, `inhaber` oder `extern` (was davon geht, prüft der Speicher).
fn schritt_warten_pruefen(wert: &str) -> Result<String, AppError> {
    text_pruefen("wartet_auf", wert, VON_HOECHSTENS, false)?;
    Ok(wert.to_string())
}

fn verweis_pruefen(wert: &str) -> Result<String, AppError> {
    if ist_kennung(wert) { Ok(wert.to_string()) } else { Err(AppError::unlesbar(SATZ_VERWEIS)) }
}

fn notiz_pruefen(wert: &str) -> Result<String, AppError> {
    text_pruefen("notiz", wert, NOTIZ_HOECHSTENS, true)?;
    Ok(wert.to_string())
}

/// Etappe B8: `etappe` hat die Form `<projekt>/<kennung>`, und die Etappe
/// steht in `daten/plan.toml` jenes Projekts (gespeicherter Stand des Inhalts;
/// der Server schreibt nie hinein). Sonst 422.
fn etappe_pruefen(state: &AppState, wert: &str) -> Result<String, AppError> {
    let Some((projekt, kennung)) = wert.split_once('/').filter(|(p, k)| slug_pruefen(p).is_ok() && ist_kennung(k)) else {
        return Err(AppError::unlesbar(SATZ_ETAPPE));
    };
    let datei = format!("projects/{projekt}/daten/plan.toml");
    match state.inhalt.bestand().daten(projekt, "plan") {
        // Abgeschlossen heißt: vor dem Tracker, ohne jede Arbeit.
        Ok(Some((DatenInhalt::Plan(p), _))) if p.etappe(kennung).is_some_and(|e| e.abgeschlossen) => Err(AppError::unlesbar(format!(
            "Die Etappe „{kennung}“ ist in {datei} vor dem Tracker abgeschlossen; ihr gehört keine Arbeit an. Eine andere Etappe wählen."
        ))),
        Ok(Some((DatenInhalt::Plan(p), _))) if p.etappe(kennung).is_some() => Ok(wert.to_string()),
        Ok(Some((DatenInhalt::Plan(p), _))) => {
            let da: Vec<&str> = p.inhalte().into_iter().flat_map(|t| &t.etappe).map(|e| e.kennung.as_str()).collect();
            Err(AppError::unlesbar(format!(
                "Die Etappe „{kennung}“ gibt es in {datei} nicht{}. Erst dort eintragen und committen, dann die Arbeit zuordnen.",
                if da.is_empty() { String::new() } else { format!(" (vorhanden: {})", da.join(", ")) }
            )))
        }
        Ok(_) => Err(AppError::unlesbar(format!("{datei} gibt es nicht. Erst die Etappe dort eintragen und committen, dann die Arbeit zuordnen."))),
        Err(_) => Err(AppError::unlesbar(format!("{datei} lässt sich gerade nicht lesen; `pruefe` sagt, warum."))),
    }
}

/// Ein leerbares Feld: `Some(None)` und `Some(Some(""))` leeren, sonst prüfen.
fn leerbar(wert: Option<Option<String>>, pruefen: impl Fn(&str) -> Result<String, AppError>) -> Result<Option<Option<String>>, AppError> {
    match wert {
        None => Ok(None),
        Some(w) => Ok(Some(angegeben(w).map(|w| pruefen(&w)).transpose()?)),
    }
}

/// Fehler des Datenspeichers: unbekannte Kennung 404, Grenze 429, ein anderer
/// Besitzer 409, gegen die Regeln 422, sonst 500 mit Satz.
fn arbeitsfehler(e: SpeicherFehler) -> AppError {
    match e {
        SpeicherFehler::NichtGefunden(was) if was.contains("/schritte/") => AppError::nicht_gefunden(SATZ_SCHRITT_NICHT_DA),
        SpeicherFehler::NichtGefunden(_) => AppError::nicht_gefunden(SATZ_NICHT_DA),
        SpeicherFehler::Grenze(satz) => AppError::zu_viele(satz),
        e @ SpeicherFehler::Besetzt { .. } => AppError::konflikt(e.to_string()),
        SpeicherFehler::Ungueltig(satz) => AppError::unlesbar(satz),
        e => {
            tracing::error!("Arbeiten: Datenspeicher: {e} ({})", e.einzelheiten());
            AppError::intern(format!("{e} Einzelheiten stehen im Protokoll."))
        }
    }
}

/// Fehler der schreibenden Arbeiten- und Schritt-Wege: wie `AppError`; bei 409
/// nennt der Körper dazu `besitzer` und `seit` (`vertrag::Besetzt`).
pub struct ArbeitsFehler(AppError, Option<(String, String)>);

impl From<AppError> for ArbeitsFehler {
    fn from(e: AppError) -> Self {
        Self(e, None)
    }
}

impl ArbeitsFehler {
    /// 409 mit `besitzer` und `seit`; `wie` sagt, wann ein anderer dran kommt.
    fn besetzt(e: SpeicherFehler, wie: &str) -> Self {
        match e {
            SpeicherFehler::Besetzt { besitzer, seit } => {
                Self(AppError::konflikt(format!("Daran ist schon {besitzer} (seit {seit}). {wie}")), Some((besitzer, seit)))
            }
            e => Self(arbeitsfehler(e), None),
        }
    }

    /// Fehler von `POST /api/arbeiten/<id>`: Eine Arbeit lässt sich nicht anbieten oder freigeben.
    fn arbeit(e: SpeicherFehler) -> Self {
        Self::besetzt(e, "Frei wird sie, wenn er sie auf „als-naechstes“ oder „fertig“ setzt.")
    }
}

impl From<SpeicherFehler> for ArbeitsFehler {
    fn from(e: SpeicherFehler) -> Self {
        let wie = format!("Übernehmen geht, wenn er anbietet, freigibt oder fertig ist, oder nach {VERWAIST_NACH_STUNDEN} Stunden ohne Schreiben mit „statt“.");
        Self::besetzt(e, &wie)
    }
}

impl IntoResponse for ArbeitsFehler {
    fn into_response(self) -> Response {
        let (e, besitzer, seit) = match self {
            Self(e, Some((besitzer, seit))) => (e, besitzer, seit),
            Self(e, None) => return e.into_response(),
        };
        let fehler = crate::datenschutz::verbergen(&e.fehler);
        let mut res = (e.status, Json(Besetzt { fehler: fehler.clone(), art: e.art.to_string(), besitzer, seit })).into_response();
        res.extensions_mut().insert(FehlerSatz(fehler));
        res
    }
}

// ----- Archiv -----

/// Ein Lauf des Archivs: fertige Arbeiten, deren letzte Änderung sieben Tage
/// her ist, ziehen nach `arbeiten_archiv` um. Meldet je Projekt ein `change`.
pub async fn archiv_lauf(state: &AppState, jetzt: Zeit) -> Result<usize, AppError> {
    let weg = Speicher::im_hintergrund(state.speicher()?, move |s| s.arbeiten_archivieren(&jetzt)).await.map_err(arbeitsfehler)?;
    if !weg.is_empty() {
        tracing::info!("{} fertige Arbeit(en) ins Archiv verschoben", weg.len());
        let mut projekte: Vec<&str> = weg.iter().flat_map(betroffene).collect();
        projekte.sort_unstable();
        projekte.dedup();
        for p in projekte {
            super::events::anstehend_geaendert(&state.events, Some(p));
        }
    }
    Ok(weg.len())
}

/// Die Projekte, deren Seiten eine Arbeit zeigen: ihr eigenes und das ihrer
/// Etappe (Plan, Etappe B8), ohne doppelte.
fn betroffene(a: &Arbeit) -> Vec<&str> {
    let mut projekte = vec![a.projekt.as_str()];
    projekte.extend(a.etappe.as_deref().and_then(|e| e.split_once('/')).map(|(p, _)| p).filter(|p| *p != a.projekt));
    projekte
}

/// `change` (anstehend) für jedes betroffene Projekt der Arbeit, dazu für das
/// Projekt der Etappe vorher (`vorher`), wenn sie gewechselt hat.
fn arbeit_gemeldet(state: &AppState, a: &Arbeit, vorher: Option<&str>) {
    let mut projekte = betroffene(a);
    if let Some((p, _)) = vorher.and_then(|e| e.split_once('/'))
        && !projekte.contains(&p)
    {
        projekte.push(p);
    }
    for p in projekte {
        super::events::anstehend_geaendert(&state.events, Some(p));
    }
}

// ----- Handler -----

/// `POST /api/arbeiten`: legt eine Arbeit an, 201 mit `id`.
pub async fn anlegen(State(state): State<AppState>, kopf: HeaderMap, koerper: Bytes) -> Result<(StatusCode, Json<ArbeitAngelegt>), AppError> {
    json_kopf_pruefen(&kopf)?;
    let neu: NeueArbeitKoerper = koerper_lesen(&koerper, MUSTER_NEU)?;
    if aus_browser(&kopf) {
        return Err(AppError::unlesbar(SATZ_NUR_CHATS));
    }
    let von = wer_ruft(neu.von)?;
    slug_pruefen(&neu.projekt)?;
    let titel = titel_pruefen(&neu.titel)?;
    let stand = stand_pruefen(&neu.stand)?;
    let wartet_auf = angegeben(neu.wartet_auf).map(|w| wartet_auf_pruefen(&w)).transpose()?;
    match (stand, &wartet_auf) {
        (ArbeitsStand::Wartet, None) => return Err(AppError::unlesbar(SATZ_WARTET_AUF_FEHLT)),
        (s, Some(_)) if s != ArbeitsStand::Wartet => {
            return Err(AppError::unlesbar("„wartet_auf“ gibt es nur beim Stand „wartet“."));
        }
        _ => {}
    }
    let verweis = angegeben(neu.verweis).map(|v| verweis_pruefen(&v)).transpose()?;
    let prioritaet = angegeben(neu.prioritaet).map(|p| stufe_pruefen("prioritaet", &p)).transpose()?.unwrap_or_default();
    let frist = angegeben(neu.frist).map(|f| frist_pruefen(&f)).transpose()?;
    let notiz = angegeben(neu.notiz).map(|n| notiz_pruefen(&n)).transpose()?;
    let etappe = angegeben(neu.etappe).map(|e| etappe_pruefen(&state, &e)).transpose()?;
    let n = NeueArbeit { projekt: neu.projekt, von, titel, stand, wartet_auf, verweis, prioritaet, frist, notiz, etappe };
    let jetzt = aufgaben::jetzt();
    let a = Speicher::im_hintergrund(state.speicher()?, move |s| s.arbeit_anlegen(n, &jetzt)).await.map_err(arbeitsfehler)?;
    tracing::info!("Arbeit {} für {} angelegt ({})", a.id, a.projekt, a.stand.wort());
    arbeit_gemeldet(&state, &a, None);
    if a.stand == ArbeitsStand::Fertig {
        // Etappe A3: eine fertige Arbeit wird zur Meldung „fertig“.
        crate::meldungen::arbeit_fertig(&state, &a, jetzt).await;
    }
    Ok((StatusCode::CREATED, Json(ArbeitAngelegt { id: a.id })))
}

/// `GET /api/arbeiten[?projekt=<slug>]`; die Schritte stehen in jeder Arbeit.
pub async fn liste(State(state): State<AppState>, uri: Uri) -> Result<Json<ArbeitenListe>, AppError> {
    let projekt = abfrage(&uri, "projekt");
    if let Some(p) = &projekt {
        slug_pruefen(p)?;
    }
    // Entscheidung archiv-status: ohne `projekt` fehlen archivierte Projekte.
    let ohne = if projekt.is_none() { state.archivierte() } else { Default::default() };
    let mut arbeiten = Speicher::im_hintergrund(state.speicher()?, move |s| s.arbeiten_lesen(projekt.as_deref())).await.map_err(arbeitsfehler)?;
    arbeiten.retain(|a| !ohne.contains(&a.projekt));
    Ok(Json(ArbeitenListe { arbeiten }))
}

/// `POST /api/arbeiten/<id>`: schreibt eine Arbeit fort (nur die Felder im Körper).
pub async fn fortschreiben(
    State(state): State<AppState>,
    Path(id): Path<String>,
    kopf: HeaderMap,
    koerper: Bytes,
) -> Result<Json<ArbeitGeaendert>, ArbeitsFehler> {
    json_kopf_pruefen(&kopf)?;
    let k: AenderungKoerper = if koerper.iter().all(u8::is_ascii_whitespace) { AenderungKoerper::default() } else { koerper_lesen(&koerper, MUSTER_AENDERN)? };
    if aus_browser(&kopf) {
        return Err(AppError::unlesbar(SATZ_NUR_CHATS).into());
    }
    let von = wer_ruft(k.von)?;
    let titel = match k.titel {
        None => None,
        Some(t) => Some(titel_pruefen(t.as_deref().unwrap_or_default())?),
    };
    let aenderung = ArbeitAenderung {
        titel,
        von,
        stand: angegeben(k.stand).map(|s| stand_pruefen(&s)).transpose()?,
        wartet_auf: leerbar(k.wartet_auf, wartet_auf_pruefen)?,
        verweis: leerbar(k.verweis, verweis_pruefen)?,
        prioritaet: angegeben(k.prioritaet).map(|p| stufe_pruefen("prioritaet", &p)).transpose()?,
        frist: leerbar(k.frist, frist_pruefen)?,
        notiz: leerbar(k.notiz, notiz_pruefen)?,
        etappe: leerbar(k.etappe, |e| etappe_pruefen(&state, e))?,
    };
    let jetzt = aufgaben::jetzt();
    // Der Stand vorher: Die Meldung „fertig“ kommt nur beim Wechsel auf fertig,
    // nicht bei jeder Notiz an einer fertigen Arbeit (sonst käme sie nach dem
    // Archiv der Meldung wieder).
    let (vorher, ergebnis) = Speicher::im_hintergrund(state.speicher()?, move |s| {
        let vorher = s.arbeit(&id)?;
        Ok((vorher, s.arbeit_fortschreiben(&id, &aenderung, &jetzt)?))
    })
    .await
    .map_err(ArbeitsFehler::arbeit)?;
    match ergebnis {
        Fortschreiben::Geaendert(arbeit) => {
            tracing::info!("Arbeit {} fortgeschrieben ({})", arbeit.id, arbeit.stand.wort());
            arbeit_gemeldet(&state, &arbeit, vorher.as_ref().and_then(|a| a.etappe.as_deref()));
            let war_fertig = vorher.is_some_and(|a| a.stand == ArbeitsStand::Fertig);
            if arbeit.stand == ArbeitsStand::Fertig && !war_fertig {
                // Etappe A3: Meldung „fertig“ (einmal je Arbeit; war sie schon da, bleibt es dabei).
                crate::meldungen::arbeit_fertig(&state, &arbeit, jetzt).await;
            }
            if arbeit.stand != ArbeitsStand::Fertig && war_fertig {
                crate::meldungen::arbeit_wieder_offen(&state, &arbeit, jetzt).await;
            }
            Ok(Json(ArbeitGeaendert { arbeit, geaendert: true }))
        }
        Fortschreiben::Unveraendert(arbeit) => Ok(Json(ArbeitGeaendert { arbeit, geaendert: false })),
        Fortschreiben::ImArchiv(_) => Err(AppError::unlesbar(SATZ_IM_ARCHIV).into()),
        Fortschreiben::Ungueltig(satz) => Err(AppError::unlesbar(satz).into()),
        Fortschreiben::FremderTitel => Err(AppError::abgelehnt(SATZ_TITEL_NUR_EIGENER_CHAT).into()),
        Fortschreiben::FremdeEtappe => Err(AppError::konflikt(SATZ_ETAPPE_NUR_EIGENER_CHAT).into()),
    }
}

// ----- Teilschritte (T3) -----

/// Körper eines Schritt-Wegs: JSON-Kopf, nie aus dem Browser; ein leerer Körper zählt als `{}`.
fn schritt_koerper<T: DeserializeOwned + Default>(kopf: &HeaderMap, koerper: &[u8], muster: &str) -> Result<T, AppError> {
    json_kopf_pruefen(kopf)?;
    let k = if koerper.iter().all(u8::is_ascii_whitespace) { T::default() } else { koerper_lesen(koerper, muster)? };
    if aus_browser(kopf) {
        return Err(AppError::unlesbar(SATZ_NUR_CHATS));
    }
    Ok(k)
}

/// `<nr>` aus dem Pfad; keine Zahl heißt: diesen Schritt gibt es nicht (404).
fn schritt_nr(nr: &str) -> Result<u32, AppError> {
    nr.parse().map_err(|_| AppError::nicht_gefunden(SATZ_SCHRITT_NICHT_DA))
}

/// Nach jedem geschriebenen Schritt: `change` (anstehend), die Weck-Vorgänge
/// melden (Live-Kanal und Warten der Empfänger), Meldung „fertig“, wenn die
/// Arbeit dadurch fertig wurde.
async fn schritt_gemeldet(state: &AppState, r: &SchrittErgebnis, jetzt: Zeit) {
    if !r.geaendert {
        return;
    }
    tracing::info!("Arbeit {}: Schritt {} geschrieben ({})", r.arbeit.id, r.nr, r.arbeit.stand.wort());
    arbeit_gemeldet(state, &r.arbeit, None);
    for e in &r.ereignisse {
        super::vorgaenge::melden(state, e);
    }
    if r.wurde_fertig {
        crate::meldungen::arbeit_fertig(state, &r.arbeit, jetzt).await;
    }
    if r.wieder_offen {
        crate::meldungen::arbeit_wieder_offen(state, &r.arbeit, jetzt).await;
    }
}

/// `POST /api/arbeiten/<id>/schritte`: legt einen Schritt an (frei, `als-naechstes`), 201 mit `nr`.
pub async fn schritt_anlegen(
    State(state): State<AppState>,
    Path(id): Path<String>,
    kopf: HeaderMap,
    koerper: Bytes,
) -> Result<(StatusCode, Json<SchrittAngelegt>), ArbeitsFehler> {
    json_kopf_pruefen(&kopf)?;
    let k: NeuerSchrittKoerper = koerper_lesen(&koerper, MUSTER_SCHRITT_NEU)?;
    if aus_browser(&kopf) {
        return Err(AppError::unlesbar(SATZ_NUR_CHATS).into());
    }
    let von = wer_ruft(k.von)?;
    let titel = titel_pruefen(&k.titel)?;
    let fuer = angegeben(k.fuer).map(|f| chat_pruefen("fuer", &f)).transpose()?;
    let jetzt = aufgaben::jetzt();
    let r = Speicher::im_hintergrund(state.speicher()?, move |s| s.schritt_anlegen(&id, &von, &titel, fuer, &jetzt)).await?;
    schritt_gemeldet(&state, &r, jetzt).await;
    Ok((StatusCode::CREATED, Json(SchrittAngelegt { nr: r.nr })))
}

/// `POST /api/arbeiten/<id>/schritte/<nr>/uebernehmen`: der Schritt wird der
/// des Chats und läuft; ein zweites Mal 200 mit `geaendert = false`; ist ein
/// anderer dran, 409 mit `besitzer` und `seit` (verwaist: mit `statt`).
pub async fn schritt_uebernehmen(
    State(state): State<AppState>,
    Path((id, nr)): Path<(String, String)>,
    kopf: HeaderMap,
    koerper: Bytes,
) -> Result<Json<ArbeitGeaendert>, ArbeitsFehler> {
    let k: UebernehmenKoerper = schritt_koerper(&kopf, &koerper, MUSTER_UEBERNEHMEN)?;
    let von = wer_ruft(k.von)?;
    let statt = angegeben(k.statt);
    if let Some(s) = &statt {
        text_pruefen("statt", s, VON_HOECHSTENS, false)?;
    }
    let nr = schritt_nr(&nr)?;
    let jetzt = aufgaben::jetzt();
    let r = Speicher::im_hintergrund(state.speicher()?, move |s| s.schritt_uebernehmen(&id, nr, &von, statt.as_deref(), &jetzt)).await?;
    schritt_gemeldet(&state, &r, jetzt).await;
    Ok(Json(ArbeitGeaendert { geaendert: r.geaendert, arbeit: r.arbeit }))
}

/// `POST /api/arbeiten/<id>/schritte/<nr>`: Stand, Warten, Verweis, Notiz,
/// anbieten oder freigeben; nur der Besitzer (Ausnahmen in `speicher::schritte`), sonst 409.
pub async fn schritt_aendern(
    State(state): State<AppState>,
    Path((id, nr)): Path<(String, String)>,
    kopf: HeaderMap,
    koerper: Bytes,
) -> Result<Json<ArbeitGeaendert>, ArbeitsFehler> {
    let k: SchrittKoerper = schritt_koerper(&kopf, &koerper, MUSTER_SCHRITT)?;
    let von = wer_ruft(k.von)?;
    let aenderung = SchrittAenderung {
        stand: angegeben(k.stand).map(|s| stand_pruefen(&s)).transpose()?,
        wartet_auf: leerbar(k.wartet_auf, schritt_warten_pruefen)?,
        verweis: leerbar(k.verweis, verweis_pruefen)?,
        notiz: leerbar(k.notiz, notiz_pruefen)?,
        anbieten: angegeben(k.anbieten).map(|a| chat_pruefen("anbieten", &a)).transpose()?,
        freigeben: k.freigeben,
    };
    let nr = schritt_nr(&nr)?;
    let jetzt = aufgaben::jetzt();
    let r = Speicher::im_hintergrund(state.speicher()?, move |s| s.schritt_aendern(&id, nr, &von, &aenderung, &jetzt)).await?;
    schritt_gemeldet(&state, &r, jetzt).await;
    Ok(Json(ArbeitGeaendert { geaendert: r.geaendert, arbeit: r.arbeit }))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;
    use crate::app::router_angemeldet as router;

    const DASH: &str = r#""von": "chat:dashboard""#;

    fn zustand() -> AppState {
        AppState::fuer_tests(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel"))
    }

    fn get(pfad: &str) -> Request<Body> {
        Request::get(pfad).header("host", "127.0.0.1:8787").body(Body::empty()).unwrap()
    }

    /// Wie `curl -H "Content-Type: application/json" -d …` (ohne Herkunft).
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

    async fn schicke(state: &AppState, req: Request<Body>) -> (StatusCode, serde_json::Value) {
        let res = router(state.clone()).oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        (status, serde_json::from_str(&text).unwrap_or_else(|e| panic!("kein JSON ({e}): {text}")))
    }

    async fn neu(state: &AppState, koerper: &str) -> String {
        let (status, v) = schicke(state, post("/api/arbeiten", koerper)).await;
        assert_eq!(status, StatusCode::CREATED, "{v}");
        v["id"].as_str().unwrap().to_string()
    }

    fn anstehend_changes(rx: &mut tokio::sync::broadcast::Receiver<super::super::events::Change>) -> Vec<String> {
        let mut out = Vec::new();
        while let Ok(c) = rx.try_recv() {
            if c.area == "anstehend" {
                out.push(c.path);
            }
        }
        out
    }

    /// Ein Chat legt per curl eine Arbeit an und schreibt sie fort; jede
    /// Änderung sendet `change` (anstehend); Teiländerungen lassen den Rest stehen.
    #[tokio::test]
    async fn anlegen_und_fortschreiben_per_curl() {
        let state = zustand();
        let mut changes = state.events.subscribe();
        let id = neu(
            &state,
            r#"{"projekt": "dashboard", "von": "chat:dashboard", "titel": "Server für A1 bauen", "stand": "laeuft", "prioritaet": "jetzt", "frist": "2026-11-05", "notiz": "Zeile 1\nZeile 2"}"#,
        )
        .await;
        assert!(id.starts_with("ar-"), "{id}");
        assert_eq!(anstehend_changes(&mut changes), ["anstehend/dashboard"]);
        let (status, l) = schicke(&state, get("/api/arbeiten?projekt=dashboard")).await;
        assert_eq!(status, StatusCode::OK, "{l}");
        let a = &l["arbeiten"][0];
        assert_eq!((a["id"].as_str(), a["von"].as_str(), a["stand"].as_str()), (Some(id.as_str()), Some("chat:dashboard"), Some("laeuft")));
        assert_eq!((a["prioritaet"].as_str(), a["frist"].as_str(), a["wartet_auf"].clone()), (Some("jetzt"), Some("2026-11-05"), serde_json::Value::Null));
        // Auf wartet (auf den Inhaber, mit Verweis); die Frist bleibt.
        let pfad = format!("/api/arbeiten/{id}");
        let (status, v) =
            schicke(&state, post(&pfad, &format!(r#"{{{DASH}, "stand": "wartet", "wartet_auf": "inhaber", "verweis": "a-20261029-0001"}}"#))).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(
            (v["geaendert"].as_bool(), v["arbeit"]["wartet_auf"].as_str(), v["arbeit"]["frist"].as_str()),
            (Some(true), Some("inhaber"), Some("2026-11-05"))
        );
        assert_eq!(anstehend_changes(&mut changes), ["anstehend/dashboard"]);
        // Dasselbe noch einmal: nichts geändert, kein change.
        let (_, v) = schicke(&state, post(&pfad, &format!(r#"{{{DASH}, "stand": "wartet"}}"#))).await;
        assert_eq!(v["geaendert"].as_bool(), Some(false));
        assert!(anstehend_changes(&mut changes).is_empty());
        // Frist und Verweis leeren (null und ""), auf chat:<name> warten.
        let (_, v) = schicke(&state, post(&pfad, &format!(r#"{{{DASH}, "frist": null, "verweis": "", "wartet_auf": "chat:bauplan"}}"#))).await;
        assert_eq!(
            (v["arbeit"]["frist"].clone(), v["arbeit"]["verweis"].clone(), v["arbeit"]["wartet_auf"].as_str()),
            (serde_json::Value::Null, serde_json::Value::Null, Some("chat:bauplan"))
        );
        // Fertig: wartet_auf fällt weg.
        let (_, v) = schicke(&state, post(&pfad, &format!(r#"{{{DASH}, "stand": "fertig"}}"#))).await;
        assert_eq!((v["arbeit"]["stand"].as_str(), v["arbeit"]["wartet_auf"].clone()), (Some("fertig"), serde_json::Value::Null));
        // Nur von: nichts.
        let (status, v) = schicke(&state, post(&pfad, &format!("{{{DASH}}}"))).await;
        assert_eq!((status, v["geaendert"].as_bool()), (StatusCode::OK, Some(false)));
        let (status, v) = schicke(&state, post("/api/arbeiten/ar-19990101-0001", &format!(r#"{{{DASH}, "stand": "laeuft"}}"#))).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::NOT_FOUND, Some(SATZ_NICHT_DA)));
    }

    /// Falsche Eingaben: 422 mit Satz, nichts angelegt; aus dem Browser nie.
    #[tokio::test]
    async fn falsche_eingaben_und_browser() {
        let state = zustand();
        let faelle: &[(&str, &str)] = &[
            (r#""stand": "läuft""#, SATZ_STAND),
            (r#""stand": "wartet""#, SATZ_WARTET_AUF_FEHLT),
            (r#""stand": "wartet", "wartet_auf": "ranke""#, SATZ_WARTET_AUF),
            (r#""stand": "wartet", "wartet_auf": "chat:Gross""#, SATZ_WARTET_AUF),
            (r#""stand": "laeuft", "wartet_auf": "inhaber""#, "„wartet_auf“ gibt es nur beim Stand „wartet“."),
            (r#""stand": "laeuft", "verweis": "../x""#, SATZ_VERWEIS),
            (r#""stand": "laeuft", "prioritaet": "sofort""#, "„prioritaet“ ist jetzt, bald oder irgendwann."),
            (r#""stand": "laeuft", "frist": "5.11.2026""#, "„frist“ ist ein Datum in der Form JJJJ-MM-TT, etwa 2026-10-30."),
        ];
        for (feld, satz) in faelle {
            let koerper = format!(r#"{{"projekt": "dashboard", {DASH}, "titel": "t", {feld}}}"#);
            let (status, v) = schicke(&state, post("/api/arbeiten", &koerper)).await;
            assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(*satz)), "{feld}");
        }
        // von: Pflicht, chat:<name> (wer_ruft).
        for von in ["", r#", "von": "browser""#, r#", "von": "dashboard""#, r#", "von": "chat:Gross""#, r#", "von": "chat:""#, r#", "von": "inhaber""#] {
            let koerper = format!(r#"{{"projekt": "dashboard", "titel": "t", "stand": "laeuft"{von}}}"#);
            let (status, v) = schicke(&state, post("/api/arbeiten", &koerper)).await;
            assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(SATZ_VON)), "{von}");
        }
        let lang = format!(r#"{{"projekt": "dashboard", {DASH}, "titel": "t", "stand": "laeuft", "notiz": "{}"}}"#, "x".repeat(NOTIZ_HOECHSTENS + 1));
        assert_eq!(schicke(&state, post("/api/arbeiten", &lang)).await.0, StatusCode::UNPROCESSABLE_ENTITY);
        let titel = format!(r#"{{"projekt": "dashboard", {DASH}, "titel": "{}", "stand": "laeuft"}}"#, "x".repeat(201));
        assert_eq!(schicke(&state, post("/api/arbeiten", &titel)).await.0, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            schicke(&state, post("/api/arbeiten", &format!(r#"{{"projekt": "dashboard", {DASH}, "titel": "t", "stand": "laeuft", "x": 1}}"#))).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        // Der Inhaber ändert keine Arbeiten.
        let (status, v) =
            schicke(&state, post_browser("/api/arbeiten", &format!(r#"{{"projekt": "dashboard", {DASH}, "titel": "t", "stand": "laeuft"}}"#))).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(SATZ_NUR_CHATS)));
        let (_, l) = schicke(&state, get("/api/arbeiten")).await;
        assert!(l["arbeiten"].as_array().unwrap().is_empty(), "nichts angelegt: {l}");
        let id = neu(&state, &format!(r#"{{"projekt": "dashboard", {DASH}, "titel": "t", "stand": "laeuft"}}"#)).await;
        let (status, _) = schicke(&state, post_browser(&format!("/api/arbeiten/{id}"), &format!(r#"{{{DASH}, "stand": "fertig"}}"#))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let (status, v) = schicke(&state, post(&format!("/api/arbeiten/{id}"), &format!(r#"{{{DASH}, "stand": "wartet"}}"#))).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(SATZ_WARTET_AUF_FEHLT)));
        // Ohne JSON-Kopf: 415.
        let req = Request::post("/api/arbeiten").header("host", "127.0.0.1:8787").body(Body::from("{}")).unwrap();
        assert_eq!(router(state.clone()).oneshot(req).await.unwrap().status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    /// T3 (Auflage teilschritte_1): ohne von 422 auf jedem schreibenden Weg,
    /// auch bei „laeuft“ und beim Titel; nichts geschrieben.
    #[tokio::test]
    async fn laeuft_ohne_von_422() {
        let state = zustand();
        let id = neu(&state, &format!(r#"{{"projekt": "dashboard", {DASH}, "titel": "frei", "stand": "als-naechstes"}}"#)).await;
        let a = format!("/api/arbeiten/{id}");
        let wege = [
            (a.clone(), r#"{"stand": "laeuft"}"#),
            (a.clone(), r#"{"titel": "neu"}"#),
            (a.clone(), ""),
            (format!("{a}/schritte"), r#"{"titel": "s"}"#),
            (format!("{a}/schritte/1/uebernehmen"), ""),
            (format!("{a}/schritte/1"), r#"{"stand": "laeuft"}"#),
        ];
        for (pfad, koerper) in &wege {
            let (status, v) = schicke(&state, post(pfad, koerper)).await;
            assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(SATZ_VON)), "{pfad} {koerper}");
        }
        let (_, l) = schicke(&state, get("/api/arbeiten")).await;
        let x = &l["arbeiten"][0];
        assert_eq!((x["stand"].as_str(), x["titel"].as_str(), x.get("besitzer"), x.get("schritte")), (Some("als-naechstes"), Some("frei"), None, None), "{l}");
    }

    fn titel_in_anstehend(v: &serde_json::Value) -> Vec<String> {
        v["eintraege"].as_array().unwrap().iter().filter_map(|e| e["titel"].as_str().map(str::to_string)).collect()
    }

    async fn meldungen_anzahl(state: &AppState) -> usize {
        let (status, l) = schicke(state, get("/api/meldungen")).await;
        assert_eq!(status, StatusCode::OK, "{l}");
        l["meldungen"].as_array().unwrap().len()
    }

    /// Umbenennen per curl: „Anstehend“ zeigt den neuen Titel, nicht mehr den
    /// alten; keine eigene Meldung, nur der übliche `change` (anstehend); der
    /// Verlauf der Arbeit hat alten und neuen Titel.
    #[tokio::test]
    async fn titel_aendern_zeigt_anstehend_ohne_meldung() {
        let state = zustand();
        let id = neu(&state, &format!(r#"{{"projekt": "dashboard", {DASH}, "titel": "Alter Titel", "stand": "laeuft"}}"#)).await;
        let (_, s) = schicke(&state, get("/api/anstehend")).await;
        assert!(titel_in_anstehend(&s).contains(&"Alter Titel".to_string()), "{s}");
        let meldungen_vorher = meldungen_anzahl(&state).await;
        let mut changes = state.events.subscribe();
        let pfad = format!("/api/arbeiten/{id}");
        let (status, v) = schicke(&state, post(&pfad, r#"{"titel": "  Neuer Titel  ", "von": "chat:dashboard"}"#)).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!((v["geaendert"].as_bool(), v["arbeit"]["titel"].as_str()), (Some(true), Some("Neuer Titel")));
        assert_eq!(anstehend_changes(&mut changes), ["anstehend/dashboard"]);
        let (_, s) = schicke(&state, get("/api/anstehend")).await;
        let titel = titel_in_anstehend(&s);
        assert!(titel.contains(&"Neuer Titel".to_string()), "{s}");
        assert!(!titel.contains(&"Alter Titel".to_string()), "{s}");
        assert_eq!(meldungen_anzahl(&state).await, meldungen_vorher, "Umbenennen meldet nichts");
        // Verlauf: alter und neuer Titel, auch in der Liste der API.
        let (_, l) = schicke(&state, get("/api/arbeiten?projekt=dashboard")).await;
        let verlauf = &l["arbeiten"][0]["verlauf"];
        assert_eq!(verlauf.as_array().map(Vec::len), Some(2), "{l}");
        assert_eq!(verlauf[0]["art"].as_str(), Some("stand"), "angelegt mit laeuft");
        let verlauf = &verlauf[1];
        assert_eq!((verlauf["art"].as_str(), verlauf["alt"].as_str(), verlauf["neu"].as_str()), (Some("titel"), Some("Alter Titel"), Some("Neuer Titel")));
        assert_eq!(verlauf["von"].as_str(), Some("chat:dashboard"));
        assert_eq!(verlauf["text"].as_str(), Some("Titel geändert: „Alter Titel“ → „Neuer Titel“"));
        // Derselbe Titel: nichts geändert.
        let (_, v) = schicke(&state, post(&pfad, &format!(r#"{{{DASH}, "titel": "Neuer Titel"}}"#))).await;
        assert_eq!(v["geaendert"].as_bool(), Some(false));
        assert!(anstehend_changes(&mut changes).is_empty());
        // Titel und fertig zusammen: die Meldung „fertig“ kommt wie bisher (mit
        // dem neuen Titel), eine für das Umbenennen nicht. Nach dem Anlegen und zwei Titeln der Wechsel des Stands (vierter Eintrag), der Besitzer wird frei (fünfter).
        let (status, v) = schicke(&state, post(&pfad, &format!(r#"{{{DASH}, "titel": "Letzter Titel", "stand": "fertig"}}"#))).await;
        assert_eq!((status, v["arbeit"]["verlauf"].as_array().map(Vec::len)), (StatusCode::OK, Some(5)), "{v}");
        assert_eq!(v["arbeit"]["verlauf"][3]["art"].as_str(), Some("stand"), "{v}");
        assert_eq!((v["arbeit"]["verlauf"][4]["art"].as_str(), v["arbeit"].get("besitzer")), (Some("besitzer"), None), "{v}");
        let (_, m) = schicke(&state, get("/api/meldungen")).await;
        let m = m["meldungen"].as_array().unwrap();
        assert_eq!(m.len(), meldungen_vorher + 1, "{m:?}");
        assert!(m.iter().any(|x| x["titel"].as_str() == Some("Fertig: Letzter Titel")), "{m:?}");
    }

    /// Falscher Titel beim Fortschreiben: leer, nur Leerzeichen, null oder zu
    /// lang gibt 422 wie beim Anlegen; nichts geändert.
    #[tokio::test]
    async fn titel_aendern_falsche_eingaben() {
        let state = zustand();
        let id = neu(&state, &format!(r#"{{"projekt": "dashboard", {DASH}, "titel": "Bleibt", "stand": "laeuft"}}"#)).await;
        let pfad = format!("/api/arbeiten/{id}");
        for titel in [r#""""#, r#""   ""#, "null"] {
            let koerper = format!(r#"{{{DASH}, "titel": {titel}}}"#);
            let (status, v) = schicke(&state, post(&pfad, &koerper)).await;
            assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(SATZ_TITEL_FEHLT)), "{koerper}");
        }
        let lang = format!(r#"{{{DASH}, "titel": "{}"}}"#, "x".repeat(TITEL_HOECHSTENS + 1));
        assert_eq!(schicke(&state, post(&pfad, &lang)).await.0, StatusCode::UNPROCESSABLE_ENTITY);
        let genau = format!(r#"{{{DASH}, "titel": "{}"}}"#, "x".repeat(TITEL_HOECHSTENS));
        assert_eq!(schicke(&state, post(&pfad, &genau)).await.0, StatusCode::OK, "genau die Grenze geht");
        let (status, v) = schicke(&state, post(&pfad, r#"{"titel": "t", "von": "browser"}"#)).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(SATZ_VON)));
        // Aus dem Browser nie.
        assert_eq!(schicke(&state, post_browser(&pfad, &format!(r#"{{{DASH}, "titel": "t"}}"#))).await.0, StatusCode::UNPROCESSABLE_ENTITY);
        let (_, l) = schicke(&state, get("/api/arbeiten")).await;
        assert_eq!(l["arbeiten"][0]["titel"].as_str(), Some("x".repeat(TITEL_HOECHSTENS).as_str()));
        assert_eq!(l["arbeiten"][0]["verlauf"].as_array().map(Vec::len), Some(2), "{l}");
    }

    /// T3: Eine Arbeit mit Besitzer ändert nur er, auch den Titel (sonst 409
    /// mit besitzer und seit); den Titel einer Arbeit ohne Besitzer nur der
    /// Chat ihres Projekts (403). Nichts geschrieben.
    #[tokio::test]
    async fn fremder_chat_409_und_titel_403() {
        let state = zustand();
        let id = neu(&state, r#"{"projekt": "dashboard", "von": "chat:architektur", "titel": "Von ARCH", "stand": "laeuft"}"#).await;
        let pfad = format!("/api/arbeiten/{id}");
        let mut changes = state.events.subscribe();
        for koerper in [r#"{"titel": "Fremd", "von": "chat:dashboard"}"#, r#"{"notiz": "n", "stand": "fertig", "von": "chat:bauplan"}"#] {
            let (status, v) = schicke(&state, post(&pfad, koerper)).await;
            assert_eq!(
                (status, v["art"].as_str(), v["besitzer"].as_str()),
                (StatusCode::CONFLICT, Some("konflikt"), Some("chat:architektur")),
                "{koerper}: {v}"
            );
            assert!(v["seit"].as_str().is_some_and(|s| s.starts_with("20")), "{v}");
            // Eine Arbeit bietet man nicht an: der Satz nennt den Weg über den Stand.
            assert!(v["fehler"].as_str().is_some_and(|s| s.contains("„als-naechstes“ oder „fertig“") && !s.contains("anbietet")), "{v}");
        }
        assert!(anstehend_changes(&mut changes).is_empty());
        let (status, v) = schicke(&state, post(&pfad, r#"{"titel": "Umbenannt", "von": "chat:architektur"}"#)).await;
        assert_eq!((status, v["arbeit"]["titel"].as_str()), (StatusCode::OK, Some("Umbenannt")), "{v}");
        // Ohne Besitzer: Titel nur der Chat des Projekts, andere Felder jeder Chat.
        let frei = neu(&state, &format!(r#"{{"projekt": "dashboard", {DASH}, "titel": "Frei", "stand": "als-naechstes"}}"#)).await;
        let pfad = format!("/api/arbeiten/{frei}");
        for von in ["chat:bauplan", "chat:dashboard-2"] {
            let (status, v) = schicke(&state, post(&pfad, &format!(r#"{{"titel": "Fremd", "von": "{von}"}}"#))).await;
            assert_eq!(
                (status, v["art"].as_str(), v["fehler"].as_str()),
                (StatusCode::FORBIDDEN, Some("abgelehnt"), Some(SATZ_TITEL_NUR_EIGENER_CHAT)),
                "{von}"
            );
        }
        let (status, v) = schicke(&state, post(&pfad, r#"{"notiz": "n", "von": "chat:bauplan"}"#)).await;
        assert_eq!((status, v["geaendert"].as_bool(), v["arbeit"].get("besitzer")), (StatusCode::OK, Some(true), None), "{v}");
        // Wer sie auf laeuft setzt, wird Besitzer.
        let (_, v) = schicke(&state, post(&pfad, r#"{"stand": "laeuft", "von": "chat:bauplan"}"#)).await;
        assert_eq!(v["arbeit"]["besitzer"].as_str(), Some("chat:bauplan"), "{v}");
    }

    /// Etappe B8: `etappe` beim Anlegen und Fortschreiben. Falsche Form, ein
    /// Projekt ohne plan.toml und eine unbekannte Etappe geben 422 (nichts
    /// geschrieben); eine Etappe eines anderen Projekts geht. Setzen und ändern
    /// nur, wer verantwortet: sonst 409 (mit und ohne Besitzer). null leert.
    #[tokio::test]
    async fn etappe_ueber_die_api() {
        let ordner = std::env::temp_dir().join(format!("dashboard-test-{}-etappe-api", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        for (slug, plan) in [("probe", Some("b8")), ("anderes", Some("c1")), ("ohne", None)] {
            let p = ordner.join("projects").join(slug);
            std::fs::create_dir_all(p.join("daten")).unwrap();
            std::fs::write(p.join("projekt.toml"), "schema = 3\ntitle = \"Probe\"\nstatus = \"aktiv\"\nsummary = \"Erfunden.\"\n").unwrap();
            if let Some(k) = plan {
                let text = format!(
                    "[[etappe]]\nkennung = \"{k}\"\ntitel = \"T\"\nziel = \"Z\"\nreihenfolge = 1\n[[etappe]]\nkennung = \"alt\"\ntitel = \"A\"\nziel = \"Z\"\nreihenfolge = 2\nabgeschlossen = true\n"
                );
                std::fs::write(p.join("daten").join("plan.toml"), text).unwrap();
            }
        }
        let state = AppState::fuer_tests(&ordner);
        let probe = r#""projekt": "probe", "von": "chat:probe""#;
        for (etappe, satz) in [
            ("b8", SATZ_ETAPPE),
            ("probe/B8", SATZ_ETAPPE),
            ("probe/b8/x", SATZ_ETAPPE),
            ("probe/b9", "Die Etappe „b9“ gibt es in projects/probe/daten/plan.toml nicht (vorhanden: b8, alt)."),
            ("probe/alt", "Die Etappe „alt“ ist in projects/probe/daten/plan.toml vor dem Tracker abgeschlossen;"),
            ("ohne/b8", "projects/ohne/daten/plan.toml gibt es nicht."),
            ("fehlt/b8", "projects/fehlt/daten/plan.toml gibt es nicht."),
        ] {
            let koerper = format!(r#"{{{probe}, "titel": "X", "stand": "laeuft", "etappe": "{etappe}"}}"#);
            let (status, v) = schicke(&state, post("/api/arbeiten", &koerper)).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{etappe}: {v}");
            assert!(v["fehler"].as_str().is_some_and(|f| f.starts_with(satz)), "{etappe}: {v}");
        }
        let (_, v) = schicke(&state, get("/api/arbeiten?projekt=probe")).await;
        assert_eq!(v["arbeiten"].as_array().map(Vec::len), Some(0), "nichts angelegt");
        // Angelegt mit Etappe, auch der eines anderen Projekts.
        let id = neu(&state, &format!(r#"{{{probe}, "titel": "Mit", "stand": "laeuft", "etappe": "anderes/c1"}}"#)).await;
        let pfad = format!("/api/arbeiten/{id}");
        let (_, v) = schicke(&state, get("/api/arbeiten?projekt=probe")).await;
        assert_eq!(v["arbeiten"][0]["etappe"].as_str(), Some("anderes/c1"), "{v}");
        // Fortschreiben: 422 vor 409, dann 409 für einen fremden Chat, nichts geschrieben.
        let (status, _) = schicke(&state, post(&pfad, r#"{"etappe": "probe/b9", "von": "chat:probe"}"#)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let (status, v) = schicke(&state, post(&pfad, r#"{"etappe": "probe/b8", "von": "chat:anderes"}"#)).await;
        assert_eq!((status, v["besitzer"].as_str()), (StatusCode::CONFLICT, Some("chat:probe")), "{v}");
        let (status, v) = schicke(&state, post(&pfad, r#"{"etappe": "probe/b8", "von": "chat:probe"}"#)).await;
        assert_eq!((status, v["arbeit"]["etappe"].as_str()), (StatusCode::OK, Some("probe/b8")), "{v}");
        let (status, v) = schicke(&state, post(&pfad, r#"{"etappe": null, "von": "chat:probe"}"#)).await;
        assert_eq!((status, v["arbeit"].get("etappe")), (StatusCode::OK, None), "{v}");
        // Ohne Besitzer: nur der Chat des Projekts, sonst 409.
        let frei = neu(&state, &format!(r#"{{{probe}, "titel": "Frei", "stand": "als-naechstes"}}"#)).await;
        let pfad = format!("/api/arbeiten/{frei}");
        let (status, v) = schicke(&state, post(&pfad, r#"{"etappe": "probe/b8", "von": "chat:anderes"}"#)).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::CONFLICT, Some(SATZ_ETAPPE_NUR_EIGENER_CHAT)), "{v}");
        let (status, v) = schicke(&state, post(&pfad, r#"{"etappe": "probe/b8", "von": "chat:probe"}"#)).await;
        assert_eq!((status, v["arbeit"]["etappe"].as_str()), (StatusCode::OK, Some("probe/b8")), "{v}");
        // Der Wechsel des Stands steht im Verlauf.
        let (_, v) = schicke(&state, post(&pfad, r#"{"stand": "fertig", "von": "chat:probe"}"#)).await;
        let stand: Vec<&serde_json::Value> = v["arbeit"]["verlauf"].as_array().unwrap().iter().filter(|e| e["art"] == "stand").collect();
        assert_eq!((stand.len(), stand[0]["alt"].as_str(), stand[0]["neu"].as_str()), (1, Some("als-naechstes"), Some("fertig")), "{v}");
        let _ = std::fs::remove_dir_all(&ordner);
    }

    /// Etappe B8: GET /api/projekte/<slug>/plan ist geschützt (401), kennt nur
    /// vorhandene Projekte (404) und zeigt Etappen mit Stand und Zählung, auch
    /// eine archivierte Arbeit und eine aus einem anderen Projekt. Zeigt etappe
    /// auf ein anderes Projekt, kommt `change` auch für dieses (vorher und jetzt).
    #[tokio::test]
    async fn plan_ueber_die_api() {
        let ordner = std::env::temp_dir().join(format!("dashboard-test-{}-plan-api", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        for (slug, plan) in [("probe", Some("b8")), ("anderes", Some("c1"))] {
            let p = ordner.join("projects").join(slug);
            std::fs::create_dir_all(p.join("daten")).unwrap();
            std::fs::write(p.join("projekt.toml"), "schema = 3\ntitle = \"Probe\"\nstatus = \"aktiv\"\nsummary = \"Erfunden.\"\n").unwrap();
            if let Some(k) = plan {
                let text = format!(
                    "[[etappe]]\nkennung = \"{k}\"\ntitel = \"T\"\nziel = \"Z\"\nreihenfolge = 2\n[[etappe]]\nkennung = \"alt\"\ntitel = \"A\"\nziel = \"Z\"\nreihenfolge = 1\nabgeschlossen = true\n"
                );
                std::fs::write(p.join("daten").join("plan.toml"), text).unwrap();
            }
        }
        let state = AppState::fuer_tests(&ordner);
        let mut changes = state.events.subscribe();
        let res = crate::app::router(state.clone()).oneshot(get("/api/projekte/probe/plan")).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        let (status, _) = schicke(&state, get("/api/projekte/gibt-es-nicht/plan")).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let eigen = neu(&state, r#"{"projekt": "probe", "von": "chat:probe", "titel": "Eigen", "stand": "laeuft", "etappe": "probe/b8"}"#).await;
        let fertig = neu(&state, r#"{"projekt": "probe", "von": "chat:probe", "titel": "Fertig", "stand": "fertig", "etappe": "probe/b8"}"#).await;
        anstehend_changes(&mut changes);
        let fremd = neu(&state, r#"{"projekt": "anderes", "von": "chat:anderes", "titel": "Fremd", "stand": "als-naechstes", "etappe": "probe/b8"}"#).await;
        assert_eq!(anstehend_changes(&mut changes), ["anstehend/anderes", "anstehend/probe"], "auch das Projekt der Etappe");
        // Die Etappe wechselt zurück ins eigene Projekt: auch das Projekt der alten Etappe lädt neu.
        let (status, _) = schicke(&state, post(&format!("/api/arbeiten/{fremd}"), r#"{"von": "chat:anderes", "etappe": "anderes/c1"}"#)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(anstehend_changes(&mut changes), ["anstehend/anderes", "anstehend/probe"]);
        let (status, _) = schicke(&state, post(&format!("/api/arbeiten/{fremd}"), r#"{"von": "chat:anderes", "etappe": "probe/b8"}"#)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(archiv_lauf(&state, aufgaben::jetzt() + chrono::Duration::days(8)).await.unwrap(), 1);
        let (status, p) = schicke(&state, get("/api/projekte/probe/plan")).await;
        assert_eq!(status, StatusCode::OK, "{p}");
        let kennungen: Vec<(&str, &str)> =
            p["etappen"].as_array().unwrap().iter().map(|e| (e["kennung"].as_str().unwrap(), e["stand"].as_str().unwrap())).collect();
        assert_eq!(kennungen, [("alt", "vor-dem-tracker"), ("b8", "laeuft")], "{p}");
        let b8 = &p["etappen"][1];
        assert_eq!((b8["arbeiten_fertig"].as_u64(), b8["arbeiten_gesamt"].as_u64()), (Some(1), Some(3)), "{p}");
        let arbeiten: Vec<(&str, bool, Option<&str>)> =
            b8["arbeiten"].as_array().unwrap().iter().map(|a| (a["id"].as_str().unwrap(), a["archiviert"].as_bool().unwrap(), a["aus"].as_str())).collect();
        assert_eq!(arbeiten, [(eigen.as_str(), false, None), (fertig.as_str(), true, None), (fremd.as_str(), false, Some("anderes"))]);
        assert!(p["ohne_etappe"].as_array().unwrap().is_empty() && p["hinweise"].as_array().unwrap().is_empty(), "{p}");
        // Fällt die Etappe aus plan.toml, steht die Arbeit unter „ohne gültige Etappe“ (Live: anstehend/probe).
        std::fs::write(ordner.join("projects/probe/daten/plan.toml"), "[[etappe]]\nkennung = \"neu\"\ntitel = \"N\"\nziel = \"Z\"\nreihenfolge = 1\n").unwrap();
        let (_, p) = schicke(&state, get("/api/projekte/probe/plan")).await;
        assert_eq!(p["ohne_etappe"].as_array().map(Vec::len), Some(3), "{p}");
        let _ = std::fs::remove_dir_all(&ordner);
    }

    /// Etappe B8: GET /api/projekte/<slug>/verlauf ist geschützt (401), kennt
    /// nur vorhandene Projekte (404) und liefert die Form von /api/pc/verlauf
    /// aus allen Quellen des Projekts: zeitstrahl.toml (hand), Arbeiten und
    /// fertige Etappe (arbeit), Schritte, Antwort auf eine Frage (frage), roter
    /// Bau (bau, problem), anderer Vorgang, Meldung, Frist (geplant, in der
    /// Zukunft) und eine Entscheidung mit `projekte`. Nichts aus dem anderen
    /// Projekt. Gegenprobe: mit `bis` heute fehlt die Frist; ohne Datenspeicher
    /// bleibt der Eintrag von Hand, mit Hinweis.
    #[tokio::test]
    async fn verlauf_eines_projekts_ueber_die_api() {
        use crate::speicher::meldungen::{NeueMeldung, Wiederholung};
        use crate::vertrag::api::{BauStand, BauZustand, MeldungsArt, VorgangsArt};
        let ordner = std::env::temp_dir().join(format!("dashboard-test-{}-projekt-verlauf", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        let heute = aufgaben::jetzt().date_naive();
        let tag = |d: i64| (heute + chrono::Duration::days(d)).format("%Y-%m-%d").to_string();
        for slug in ["probe", "anderes"] {
            let p = ordner.join("projects").join(slug);
            std::fs::create_dir_all(p.join("daten")).unwrap();
            std::fs::write(p.join("projekt.toml"), "schema = 3\ntitle = \"Probe\"\nstatus = \"aktiv\"\nsummary = \"Erfunden.\"\n").unwrap();
        }
        let daten = ordner.join("projects/probe/daten");
        let etappe = |k: &str, n: u32| format!("[[etappe]]\nkennung = \"{k}\"\ntitel = \"Etappe {k}\"\nziel = \"Z\"\nreihenfolge = {n}\n");
        std::fs::write(daten.join("plan.toml"), format!("{}{}", etappe("b8", 1), etappe("c1", 2))).unwrap();
        std::fs::write(
            daten.join("zeitstrahl.toml"),
            format!("[[eintrag]]\nkennung = \"start\"\ndatum = \"{}\"\ntitel = \"Start\"\nart = \"erledigt\"\n", tag(-1)),
        )
        .unwrap();
        let entscheidungen = ordner.join("docs/entscheidungen");
        std::fs::create_dir_all(&entscheidungen).unwrap();
        std::fs::write(entscheidungen.join("e-probe.toml"), format!("datum = \"{}\"\nthema = \"Für probe\"\nprojekte = [\"probe\"]\n", tag(-2))).unwrap();
        std::fs::write(entscheidungen.join("e-anderes.toml"), format!("datum = \"{}\"\nthema = \"Für anderes\"\nprojekte = [\"anderes\"]\n", tag(-2))).unwrap();
        std::fs::write(entscheidungen.join("e-ohne.toml"), format!("datum = \"{}\"\nthema = \"Ohne Feld\"\n", tag(-2))).unwrap();
        let state = AppState::fuer_tests(&ordner);
        let res = crate::app::router(state.clone()).oneshot(get("/api/projekte/probe/verlauf")).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        let (status, _) = schicke(&state, get("/api/projekte/gibt-es-nicht/verlauf")).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let p = r#""projekt": "probe", "von": "chat:probe""#;
        let laeuft = neu(&state, &format!(r#"{{{p}, "titel": "Läuft", "stand": "laeuft", "etappe": "probe/b8"}}"#)).await;
        let (status, _) = schicke(&state, post(&format!("/api/arbeiten/{laeuft}/schritte"), r#"{"von": "chat:probe", "titel": "Teil"}"#)).await;
        assert_eq!(status, StatusCode::CREATED);
        let fertig = neu(&state, &format!(r#"{{{p}, "titel": "Fertig", "stand": "fertig", "etappe": "probe/c1"}}"#)).await;
        let spaeter = neu(&state, &format!(r#"{{{p}, "titel": "Später", "stand": "als-naechstes", "frist": "{}"}}"#, tag(10))).await;
        let lose = neu(&state, &format!(r#"{{{p}, "titel": "Lose fertig", "stand": "fertig"}}"#)).await;
        let verpasst = neu(&state, &format!(r#"{{{p}, "titel": "Verpasst", "stand": "als-naechstes", "frist": "{}"}}"#, tag(-1))).await;
        neu(&state, r#"{"projekt": "anderes", "von": "chat:anderes", "titel": "Fremd", "stand": "laeuft"}"#).await;
        let s = state.speicher.clone().unwrap();
        let (bau, _) = s.vorgang_anlegen(VorgangsArt::Bau, "probe", "Bau der Seite", None, "baumeister").unwrap();
        let rot = BauStand { zustand: BauZustand::Rot, meldung: "Bau rot.".into(), rote: vec!["probe".into()] };
        s.bau_setzen(std::slice::from_ref(&bau.id), &rot, "baumeister", "rot").unwrap();
        let (antwort, _) = s.vorgang_anlegen(VorgangsArt::Antwort, "probe", "Antwort: ja", Some("antwort:probe/frage-1"), "browser").unwrap();
        let (abnahme, _) = s.vorgang_anlegen(VorgangsArt::Abnahme, "probe", "Abnahme", None, "browser").unwrap();
        s.vorgang_anlegen(VorgangsArt::Abnahme, "anderes", "Fremde Abnahme", None, "browser").unwrap();
        // Doppelt zu Schritt-Einträgen bzw. Klick-Protokoll: keine eigene Zeile.
        let (schritt_v, _) = s.vorgang_anlegen(VorgangsArt::Schritt, "probe", "Schritt angeboten", None, "chat:probe").unwrap();
        let (oeffnen_v, _) = s.vorgang_anlegen(VorgangsArt::Oeffnen, "probe", "Ordner öffnen", None, "browser").unwrap();
        let meldung = |projekt: &str| NeueMeldung {
            projekt: projekt.into(),
            art: MeldungsArt::Info,
            schluessel: format!("{projekt}/hinweis"),
            titel: "Hinweis".into(),
            text: None,
            verweis: None,
            system: false,
            geschuetzt: false,
            von: "server".into(),
        };
        for projekt in ["probe", "anderes"] {
            s.meldung_melden(meldung(projekt), Wiederholung::Zustand, &aufgaben::jetzt()).unwrap();
        }
        // Dieselbe Sache als Meldung: roter Bau (steht schon als `bau`).
        let bau_rot = NeueMeldung { art: MeldungsArt::Fehler, schluessel: "probe/bau-rot".into(), titel: "Bau rot".into(), ..meldung("probe") };
        s.meldung_melden(bau_rot, Wiederholung::Zustand, &aufgaben::jetzt()).unwrap();
        let (status, v) = schicke(&state, get("/api/projekte/probe/verlauf")).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let e = v["ereignisse"].as_array().unwrap();
        let finde = |k: &str| e.iter().find(|x| x["kennung"] == k).unwrap_or_else(|| panic!("{k} fehlt: {v}"));
        let qa = |x: &serde_json::Value| (x["quelle"].as_str().unwrap().to_string(), x["art"].as_str().unwrap().to_string());
        let paar = |q: &str, a: &str| (q.to_string(), a.to_string());
        assert_eq!(qa(finde("hand:start")), paar("hand", "erledigt"));
        assert_eq!(qa(finde(&format!("arbeit:{laeuft}:begonnen"))), paar("arbeit", "begonnen"));
        let f = finde(&format!("arbeit:{fertig}:fertig"));
        assert_eq!((qa(f), f["text"].is_null()), (paar("arbeit", "erledigt"), true), "fertig angelegt: Zeitpunkt genau");
        assert_eq!(qa(finde("etappe:c1")), paar("arbeit", "erledigt"), "c1 hat nur die fertige Arbeit");
        // Verweis nur mit Ziel auf der Seite: fertig mit Etappe (Plan), offen (Aufgabenliste); fertig ohne Etappe keiner.
        assert_eq!(f["verweis"], format!("/projects/probe#arbeit-{fertig}"), "{v}");
        assert_eq!(finde(&format!("arbeit:{laeuft}:begonnen"))["verweis"], format!("/projects/probe#arbeit-{laeuft}"), "{v}");
        assert!(finde(&format!("arbeit:{lose}:fertig"))["verweis"].is_null(), "{v}");
        assert!(!e.iter().any(|x| x["kennung"] == "etappe:b8"), "b8 läuft noch: {v}");
        assert!(e.iter().any(|x| x["quelle"] == "schritt" && x["kennung"].as_str().unwrap().starts_with(&format!("schritt:{laeuft}/"))), "{v}");
        assert_eq!(qa(finde(&format!("frage:{}", antwort.id))), paar("frage", "erledigt"));
        assert_eq!(qa(finde(&format!("bau:{}", bau.id))), paar("bau", "problem"));
        assert_eq!(qa(finde(&format!("vorgang:{}", abnahme.id))), paar("vorgang", "begonnen"));
        for id in [&schritt_v.id, &oeffnen_v.id] {
            assert!(!v.to_string().contains(id.as_str()), "{id} steht als eigene Zeile: {v}");
        }
        let frist = finde(&format!("frist:arbeit:{spaeter}"));
        assert_eq!((qa(frist), frist["genauigkeit"].as_str()), (paar("frist", "geplant"), Some("tag")));
        assert_eq!(qa(finde(&format!("frist:arbeit:{verpasst}"))), paar("frist", "problem"), "verstrichen ist nicht geplant");
        assert_eq!(qa(finde("entscheidung:e-probe.toml")), paar("entscheidung", "erledigt"));
        // Kein Inhalt in zwei Zeilen: keine Meldung „Fertig: …“ und kein `bau-rot` neben dem roten Bau.
        assert!(!e.iter().any(|x| x["quelle"] == "meldung" && (x["titel"].as_str().unwrap().starts_with("Fertig") || x["titel"] == "Bau rot")), "{v}");
        assert_eq!(e.iter().filter(|x| x["quelle"] == "meldung" && x["titel"] == "Hinweis").count(), 1, "nur die Meldung des Projekts: {v}");
        assert!(!v.to_string().contains("Fremd") && !v.to_string().contains("Für anderes") && !v.to_string().contains("Ohne Feld"), "{v}");
        assert!(v["hinweise"].as_array().unwrap().is_empty(), "{v}");
        // Gegenprobe zum Ende: mit bis = heute fehlt die Frist in zehn Tagen.
        let (_, v) = schicke(&state, get(&format!("/api/projekte/probe/verlauf?bis={}", tag(0)))).await;
        assert!(!v.to_string().contains(&format!("frist:arbeit:{spaeter}")) && v.to_string().contains("hand:start"), "{v}");
        let (status, _) = schicke(&state, get("/api/projekte/probe/verlauf?seit=gestern")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        // „seit“ allein darf 400 Tage zurück; die Vorschau ohne „bis“ zählt nicht mit.
        let (status, v) = schicke(&state, get(&format!("/api/projekte/probe/verlauf?seit={}", tag(-350)))).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        // Ohne Datenspeicher: der Eintrag von Hand bleibt, dazu der Hinweis.
        let ohne = AppState::fuer_tests_ohne_speicher(&ordner, "kaputt");
        let (status, v) = schicke(&ohne, get("/api/projekte/probe/verlauf")).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert!(v.to_string().contains("hand:start") && !v.to_string().contains("arbeit:"), "{v}");
        assert!(
            v["hinweise"].as_array().unwrap().iter().any(|h| h["art"] == "ohne-speicher" && h["text"] == super::super::pc::SATZ_PROJEKT_OHNE_SPEICHER),
            "{v}"
        );
        let _ = std::fs::remove_dir_all(&ordner);
    }

    /// Grenze: die 31. offene Arbeit eines Projekts gibt 429 mit Satz.
    #[tokio::test]
    async fn grenze_gibt_429() {
        let state = zustand();
        for i in 0..crate::speicher::arbeiten::OFFEN_JE_PROJEKT {
            neu(&state, &format!(r#"{{"projekt": "dashboard", {DASH}, "titel": "t{i}", "stand": "als-naechstes"}}"#)).await;
        }
        let (status, v) = schicke(&state, post("/api/arbeiten", &format!(r#"{{"projekt": "dashboard", {DASH}, "titel": "31.", "stand": "laeuft"}}"#))).await;
        assert_eq!((status, v["art"].as_str()), (StatusCode::TOO_MANY_REQUESTS, Some("zu-viele")), "{v}");
        assert!(v["fehler"].as_str().unwrap().starts_with("Für das Projekt dashboard sind schon 30 Arbeiten offen"), "{v}");
        neu(&state, &format!(r#"{{"projekt": "dashboard", {DASH}, "titel": "fertig geht", "stand": "fertig"}}"#)).await;
    }

    /// Archiv über den Zeitplan (sieben Tage später): nicht mehr in der Liste,
    /// im Export; danach Fortschreiben 422 mit Satz.
    #[tokio::test]
    async fn archiv_ueber_den_zeitplan() {
        let state = zustand();
        let id = neu(&state, &format!(r#"{{"projekt": "dashboard", {DASH}, "titel": "x", "stand": "fertig"}}"#)).await;
        assert_eq!(archiv_lauf(&state, aufgaben::jetzt() + chrono::Duration::days(6)).await.unwrap(), 0);
        let mut changes = state.events.subscribe();
        assert_eq!(archiv_lauf(&state, aufgaben::jetzt() + chrono::Duration::days(8)).await.unwrap(), 1);
        assert_eq!(anstehend_changes(&mut changes), ["anstehend/dashboard"]);
        let (_, l) = schicke(&state, get("/api/arbeiten")).await;
        assert!(l["arbeiten"].as_array().unwrap().is_empty(), "{l}");
        let (_, json) = schicke(&state, get("/api/daten/export?format=json")).await;
        assert_eq!(json["arbeiten_archiv"][0]["id"].as_str(), Some(id.as_str()), "{json}");
        let (status, v) = schicke(&state, post(&format!("/api/arbeiten/{id}"), &format!(r#"{{{DASH}, "stand": "laeuft"}}"#))).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(SATZ_IM_ARCHIV)));
        // Auch kein Schritt an einer Arbeit im Archiv.
        let (status, v) = schicke(&state, post(&format!("/api/arbeiten/{id}/schritte"), &format!(r#"{{{DASH}, "titel": "s"}}"#))).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(SATZ_IM_ARCHIV)));
    }

    /// Teilschritte über die API (Fall 6b in kurz): anlegen 201 mit nr,
    /// übernehmen 200 und zweimal 200 mit geaendert = false, ein anderer 409
    /// mit besitzer und seit; anbieten weckt den Empfänger mit einem Vorgang,
    /// sein Übernehmen erledigt ihn; der Wartende wird nach fertig geweckt.
    /// Die Liste zeigt die Schritte.
    #[tokio::test]
    async fn schritte_ueber_die_api() {
        let state = zustand();
        let mut vorgaenge = state.vorgang_events.subscribe();
        let id = neu(&state, &format!(r#"{{"projekt": "dashboard", {DASH}, "titel": "Etappe 6", "stand": "laeuft"}}"#)).await;
        let a = format!("/api/arbeiten/{id}");
        let (status, v) = schicke(&state, post(&format!("{a}/schritte"), &format!(r#"{{{DASH}, "titel": "6b bauen"}}"#))).await;
        assert_eq!((status, v["nr"].as_u64()), (StatusCode::CREATED, Some(2)), "Schritt 1 „bisherige Arbeit“ zuerst: {v}");
        let (status, v) = schicke(&state, post(&format!("{a}/schritte"), &format!(r#"{{{DASH}, "titel": "6b Abnahme", "fuer": "chat:architektur"}}"#))).await;
        assert_eq!((status, v["nr"].as_u64()), (StatusCode::CREATED, Some(3)), "{v}");
        let (status, v) = schicke(&state, post(&format!("{a}/schritte"), &format!(r#"{{{DASH}, "titel": "x", "fuer": "inhaber"}}"#))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "fuer nur chat:<name>: {v}");
        // Übernehmen, zweimal, ein anderer.
        let (status, v) = schicke(&state, post(&format!("{a}/schritte/2/uebernehmen"), &format!("{{{DASH}}}"))).await;
        assert_eq!((status, v["geaendert"].as_bool()), (StatusCode::OK, Some(true)), "{v}");
        let (status, v) = schicke(&state, post(&format!("{a}/schritte/2/uebernehmen"), &format!("{{{DASH}}}"))).await;
        assert_eq!((status, v["geaendert"].as_bool()), (StatusCode::OK, Some(false)), "{v}");
        let (status, v) = schicke(&state, post(&format!("{a}/schritte/2/uebernehmen"), r#"{"von": "chat:architektur"}"#)).await;
        assert_eq!((status, v["besitzer"].as_str()), (StatusCode::CONFLICT, Some("chat:dashboard")), "{v}");
        assert!(v["seit"].as_str().is_some_and(|s| s.starts_with("20")) && v["fehler"].as_str().is_some_and(|f| f.contains("chat:dashboard")), "{v}");
        assert!(v["fehler"].as_str().is_some_and(|f| f.contains("anbietet") && f.contains("„statt“")), "{v}");
        let (status, _) = schicke(&state, post(&format!("{a}/schritte/2"), r#"{"von": "chat:architektur", "notiz": "n"}"#)).await;
        assert_eq!(status, StatusCode::CONFLICT, "ändern nur der Besitzer");
        // Anbieten nur an chat:<name>; an Architektur weckt sie mit einem Vorgang.
        let (status, _) = schicke(&state, post(&format!("{a}/schritte/3"), &format!(r#"{{{DASH}, "anbieten": "inhaber"}}"#))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        while vorgaenge.try_recv().is_ok() {}
        let (status, v) = schicke(&state, post(&format!("{a}/schritte/3"), &format!(r#"{{{DASH}, "anbieten": "chat:architektur"}}"#))).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let e = vorgaenge.try_recv().unwrap();
        assert_eq!((e.slug.as_str(), e.art, e.zustand), ("architektur", crate::vertrag::api::VorgangsArt::Schritt, crate::vertrag::api::Zustand::Offen));
        let (_, l) = schicke(&state, get("/api/vorgaenge?fuer=architektur&offen=1")).await;
        assert_eq!(l["vorgaenge"][0]["art"].as_str(), Some("schritt"), "{l}");
        // Architektur übernimmt: Vorgang erledigt.
        let (status, v) = schicke(&state, post(&format!("{a}/schritte/3/uebernehmen"), r#"{"von": "chat:architektur"}"#)).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let e = vorgaenge.try_recv().unwrap();
        assert_eq!((e.slug.as_str(), e.zustand), ("architektur", crate::vertrag::api::Zustand::Erledigt));
        // Architektur wartet auf Schritt 2; Dashboard setzt 2 fertig: Architektur wird geweckt.
        let (status, v) =
            schicke(&state, post(&format!("{a}/schritte/3"), r#"{"von": "chat:architektur", "stand": "wartet", "wartet_auf": "schritt:2"}"#)).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let (status, v) = schicke(&state, post(&format!("{a}/schritte/2"), &format!(r#"{{{DASH}, "stand": "fertig"}}"#))).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let e = vorgaenge.try_recv().unwrap();
        assert_eq!((e.slug.as_str(), e.zustand), ("architektur", crate::vertrag::api::Zustand::Offen));
        // Die Liste zeigt die Schritte (Typ Arbeit).
        let (_, l) = schicke(&state, get("/api/arbeiten?projekt=dashboard")).await;
        let s = &l["arbeiten"][0]["schritte"];
        assert_eq!(s.as_array().map(Vec::len), Some(3), "{l}");
        assert_eq!((s[2]["stand"].as_str(), s[2]["besitzer"].as_str()), (Some("laeuft"), Some("chat:architektur")), "{l}");
        assert!(s[2]["vorgang"].as_str().is_some_and(|v| v.starts_with("v-")), "{l}");
        // Direkt den Stand der Arbeit setzen geht nicht mehr.
        let (status, v) = schicke(&state, post(&a, &format!(r#"{{{DASH}, "stand": "fertig"}}"#))).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(crate::speicher::arbeiten::SATZ_STAND_AUS_SCHRITTEN)));
    }

    /// Meldung „fertig“ als Zustand: Ein neuer Schritt an der fertigen Arbeit
    /// hebt sie auf (behoben), das nächste Fertig bringt sie ungelesen zurück.
    #[tokio::test]
    async fn fertig_meldung_behoben_wenn_neuer_schritt() {
        let state = zustand();
        let s = state.speicher().unwrap();
        let id = neu(&state, &format!(r#"{{"projekt": "dashboard", {DASH}, "titel": "Etappe 7", "stand": "laeuft"}}"#)).await;
        let a = format!("/api/arbeiten/{id}");
        let fertig_machen = |nr: u32| {
            let (state, a) = (state.clone(), a.clone());
            async move {
                schicke(&state, post(&format!("{a}/schritte/{nr}/uebernehmen"), &format!("{{{DASH}}}"))).await;
                let (status, v) = schicke(&state, post(&format!("{a}/schritte/{nr}"), &format!(r#"{{{DASH}, "stand": "fertig"}}"#))).await;
                assert_eq!(status, StatusCode::OK, "{v}");
            }
        };
        schicke(&state, post(&format!("{a}/schritte"), &format!(r#"{{{DASH}, "titel": "bauen"}}"#))).await;
        fertig_machen(1).await;
        fertig_machen(2).await;
        let fertig = || s.meldungen_lesen(Some("dashboard")).unwrap().into_iter().find(|m| m.art == crate::api::vertrag::MeldungsArt::Fertig).unwrap();
        assert!(fertig().behoben.is_none(), "Arbeit fertig: Meldung steht");
        let (status, v) = schicke(&state, post(&format!("{a}/schritte"), &format!(r#"{{{DASH}, "titel": "nachbessern"}}"#))).await;
        assert_eq!(status, StatusCode::CREATED, "{v}");
        assert!(fertig().behoben.is_some(), "neuer Schritt: die Arbeit ist nicht mehr fertig");
        fertig_machen(3).await;
        let m = fertig();
        assert_eq!((m.behoben.is_none(), m.gelesen.is_none(), m.anzahl), (true, true, 2), "wieder fertig: Meldung wieder da");
    }

    /// Schritt-Wege: aus dem Browser 422, unbekannt 404, Grenze 429, Regeln 422.
    #[tokio::test]
    async fn schritte_browser_unbekannt_und_grenzen() {
        let state = zustand();
        let id = neu(&state, &format!(r#"{{"projekt": "dashboard", {DASH}, "titel": "frei", "stand": "als-naechstes"}}"#)).await;
        let a = format!("/api/arbeiten/{id}");
        for (pfad, koerper) in [
            (format!("{a}/schritte"), format!(r#"{{{DASH}, "titel": "s"}}"#)),
            (format!("{a}/schritte/1/uebernehmen"), format!("{{{DASH}}}")),
            (format!("{a}/schritte/1"), format!(r#"{{{DASH}, "notiz": "n"}}"#)),
        ] {
            let (status, v) = schicke(&state, post_browser(&pfad, &koerper)).await;
            assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(SATZ_NUR_CHATS)), "{pfad}");
        }
        let (_, l) = schicke(&state, get("/api/arbeiten")).await;
        assert!(l["arbeiten"][0].get("schritte").is_none(), "nichts angelegt: {l}");
        // Unbekannt: Arbeit, Schritt, keine Zahl.
        let (status, v) = schicke(&state, post("/api/arbeiten/ar-19990101-0001/schritte", &format!(r#"{{{DASH}, "titel": "s"}}"#))).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::NOT_FOUND, Some(SATZ_NICHT_DA)));
        for nr in ["9", "x"] {
            let (status, v) = schicke(&state, post(&format!("{a}/schritte/{nr}/uebernehmen"), &format!("{{{DASH}}}"))).await;
            assert_eq!((status, v["fehler"].as_str()), (StatusCode::NOT_FOUND, Some(SATZ_SCHRITT_NICHT_DA)), "{nr}");
        }
        // Grenze: 20 Schritte.
        for i in 0..crate::speicher::schritte::SCHRITTE_HOECHSTENS {
            let (status, v) = schicke(&state, post(&format!("{a}/schritte"), &format!(r#"{{{DASH}, "titel": "s{i}"}}"#))).await;
            assert_eq!(status, StatusCode::CREATED, "{v}");
        }
        let (status, v) = schicke(&state, post(&format!("{a}/schritte"), &format!(r#"{{{DASH}, "titel": "zu viel"}}"#))).await;
        assert_eq!((status, v["art"].as_str()), (StatusCode::TOO_MANY_REQUESTS, Some("zu-viele")), "{v}");
        // Regeln: ein freier Schritt lässt sich nicht ändern, ohne ihn zu übernehmen; falsche Felder.
        let (status, _) = schicke(&state, post(&format!("{a}/schritte/1"), &format!(r#"{{{DASH}, "notiz": "n"}}"#))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        for koerper in [format!(r#"{{{DASH}, "stand": "läuft"}}"#), format!(r#"{{{DASH}, "x": 1}}"#), format!(r#"{{{DASH}, "verweis": "../x"}}"#)] {
            assert_eq!(schicke(&state, post(&format!("{a}/schritte/1"), &koerper)).await.0, StatusCode::UNPROCESSABLE_ENTITY, "{koerper}");
        }
        let (status, _) = schicke(&state, post(&format!("{a}/schritte"), &format!(r#"{{{DASH}, "titel": "{}"}}"#, "x".repeat(TITEL_HOECHSTENS + 1)))).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    /// Ohne Windows-Anmeldung 401, auch per curl.
    #[tokio::test]
    async fn ohne_anmeldung_401() {
        let state = zustand();
        let koerper = r#"{"projekt": "dashboard", "von": "chat:dashboard", "titel": "t", "stand": "laeuft"}"#;
        for req in [
            get("/api/arbeiten"),
            post("/api/arbeiten", koerper),
            post("/api/arbeiten/ar-1", "{}"),
            post("/api/arbeiten/ar-1/schritte", "{}"),
            post("/api/arbeiten/ar-1/schritte/1", "{}"),
            post("/api/arbeiten/ar-1/schritte/1/uebernehmen", "{}"),
        ] {
            let pfad = req.uri().to_string();
            let res = crate::app::router(state.clone()).oneshot(req).await.unwrap();
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{pfad}");
        }
    }
}
