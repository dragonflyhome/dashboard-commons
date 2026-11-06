//! `/api/vorgaenge`: Vorgänge anlegen, abfragen, bestätigen und darauf warten
//! (Zielbild, Kapitel c2). Die Chats rufen diese Wege mit `scripts\Api.ps1` ohne
//! Browser-Kopfzeilen auf; die Herkunftspflicht gilt hier darum nicht (siehe
//! `sicherheit::herkunft_pflicht`), Anfragen fremder Seiten bleiben aber 403,
//! auch lesende. Die Herkunft `browser` und die Art `antwort` gibt es nur aus
//! dem Browser (`Origin` oder `Sec-Fetch-Site`), sonst 422. Die schreibenden
//! Wege (anlegen, `uebernommen`, `erledigt`) nehmen nur
//! `Content-Type: application/json`, sonst 415 (auch bei leerem Körper).
//!
//! Seit Etappe 7b gilt hier zusätzlich die Windows-Anmeldung (Schleuse
//! `sicherheit::anmeldung_pflicht`, auch für `warten`): Die Chats rufen mit
//! `scripts\Api.ps1` und `scripts\Warten.ps1` auf (.NET mit der Anmeldung des
//! Kontos; `curl.exe --negotiate -u :`, das curl 8.22 von
//! Windows kann kein NTLM mehr). Die Ausnahme von der Herkunftspflicht bleibt.
//!
//! Der Warte-Endpunkt hält die Leitung offen, bis es für den Slug etwas gibt
//! (*Long-Polling*). Je Slug ein `tokio::sync::Notify` im `Warteraum`; jeder
//! Schritt eines Vorgangs weckt die Warter seines Slugs.
//!
//! Art `bau` (Etappe 4v): Ein Chat oder die Oberfläche bestellt damit einen
//! Bau der Oberfläche; `slug` ist der Chat, der das Ergebnis bekommt. Der
//! Baumeister des Dienstes (`baumeister`) nimmt ihn an (Feld `bau` mit
//! `laeuft`, dann `gruen` oder `rot` und Meldung). Bis das Ergebnis da ist,
//! liefern `warten` und `offen=1` ihn nicht aus. Seit Kleinkram 2 setzt der
//! Baumeister ihn mit dem Ergebnis selbst auf `erledigt` (Satz in `notiz` und
//! `bau.meldung`, Zeit in `geaendert`); das Ergebnis weckt den Chat trotzdem
//! einmal über `warten` (ohne `seit`) wie ein neuer Vorgang
//! (`Speicher::bau_ergebnisse`, gemerkt in `warter.bau_bis`). Übernehmen und
//! Erledigen braucht der Chat dafür nicht mehr.
//!
//! Art `merge` (Etappe 4b): der Merge-Request eines Projekt-Chats für sein
//! Backend-Modul, an den Betreuer des Dashboards (`slug = "dashboard"`). Dazu die
//! Pflichtfelder `zweig` (`projekt/<projekt>-<thema>`), `commit`, `projekt`,
//! `beschreibung` (Endpunkte als TOML-Beispiel) und `pruefbericht`; sie stehen
//! danach im Feld `merge` des Vorgangs. Die Seite /vorgaenge zeigt die offenen.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, Uri, header};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use tokio::sync::{Notify, oneshot};

use super::vertrag::{ChatZeile, Ereignis, MergeAntrag, Uebersicht, VorgangAngelegt, VorgangDetail, VorgangGeaendert, VorgangsArt, VorgangsListe, Zustand};
use crate::app::AppState;
use crate::fehler::AppError;
use crate::speicher::{Speicher, SpeicherFehler};

/// `timeout` des Warte-Endpunkts in Sekunden: Standard und Obergrenze.
pub const TIMEOUT_STANDARD: u64 = 1500;
pub const TIMEOUT_HOECHSTENS: u64 = 3600;
/// So viele Warter je Slug zugleich; ein weiterer beendet den ältesten.
pub const WARTER_JE_SLUG: usize = 3;
/// Längen in Zeichen.
pub const SLUG_HOECHSTENS: usize = 64;
pub const TITEL_HOECHSTENS: usize = 200;
pub const BEZUG_HOECHSTENS: usize = 200;
pub const VON_HOECHSTENS: usize = 64;
pub const NOTIZ_HOECHSTENS: usize = 500;

// ----- Warteraum -----

/// Wer gerade wartet, je Slug. Lebt nur im Arbeitsspeicher: nach einem
/// Neustart warten die Chats einfach neu (ihr Skript versucht es wieder).
#[derive(Default)]
pub struct Warteraum {
    slugs: Mutex<HashMap<String, Arc<SlugWarter>>>,
    naechste: AtomicU64,
}

/// Die Warter eines Slugs: das Signal und die Liste der aktiven, älteste zuerst.
#[derive(Default)]
pub struct SlugWarter {
    signal: Notify,
    aktive: Mutex<Vec<AktiverWarter>>,
}

struct AktiverWarter {
    nr: u64,
    /// Ortszeit, RFC 3339.
    seit: String,
    abbruch: oneshot::Sender<()>,
}

/// Ein angemeldeter Warter. Meldet sich beim Freigeben ab, auch wenn die
/// Verbindung reißt (dann gibt axum die Antwort-Zukunft frei).
pub struct Anmeldung<'a> {
    warteraum: &'a Warteraum,
    slug: String,
    raum: Arc<SlugWarter>,
    nr: u64,
}

impl Anmeldung<'_> {
    /// Wird fertig, sobald `Warteraum::wecken` für diesen Slug läuft.
    pub fn geweckt(&self) -> tokio::sync::futures::Notified<'_> {
        self.raum.signal.notified()
    }
}

impl Drop for Anmeldung<'_> {
    /// Abmelden; war es der letzte Warter des Slugs, verschwindet auch der
    /// Eintrag in `slugs` (sonst wüchse die Karte mit jedem Slug, der je wartete).
    /// Sperren wie in `anmelden`: erst `slugs`, dann `aktive`.
    fn drop(&mut self) {
        let mut slugs = sperren(&self.warteraum.slugs);
        let leer = {
            let mut aktive = sperren(&self.raum.aktive);
            aktive.retain(|w| w.nr != self.nr);
            aktive.is_empty()
        };
        if leer && slugs.get(&self.slug).is_some_and(|r| Arc::ptr_eq(r, &self.raum)) {
            slugs.remove(&self.slug);
        }
    }
}

/// Ein Mutex auch dann, wenn ein anderer Thread darin abgestürzt ist.
fn sperren<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Warteraum {
    /// Meldet einen Warter an. Sind schon `WARTER_JE_SLUG` da, bekommt der
    /// älteste das Abbruchsignal (er antwortet leer). Der Empfänger meldet,
    /// wenn dieser Warter selbst so beendet wird. `slugs` bleibt gesperrt, bis
    /// der Warter in `aktive` steht: so räumt ein gleichzeitiges Abmelden den
    /// Eintrag nicht unter ihm weg (dann fände `wecken` ihn nicht mehr).
    pub fn anmelden(&self, slug: &str) -> (Anmeldung<'_>, oneshot::Receiver<()>) {
        let nr = self.naechste.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        let raum = {
            let mut slugs = sperren(&self.slugs);
            let raum = slugs.entry(slug.to_string()).or_default().clone();
            let mut aktive = sperren(&raum.aktive);
            while aktive.len() >= WARTER_JE_SLUG {
                let aeltester = aktive.remove(0);
                let _ = aeltester.abbruch.send(());
            }
            aktive.push(AktiverWarter { nr, seit: jetzt(), abbruch: tx });
            drop(aktive);
            raum
        };
        (Anmeldung { warteraum: self, slug: slug.to_string(), raum, nr }, rx)
    }

    /// Weckt alle, die gerade auf diesen Slug warten.
    pub fn wecken(&self, slug: &str) {
        let raum = sperren(&self.slugs).get(slug).cloned();
        if let Some(raum) = raum {
            raum.signal.notify_waiters();
        }
    }

    /// Seit wann der älteste aktive Warter dieses Slugs wartet.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn wartet_seit(&self, slug: &str) -> Option<String> {
        let raum = sperren(&self.slugs).get(slug).cloned()?;
        sperren(&raum.aktive).first().map(|w| w.seit.clone())
    }

    /// Alle Slugs, auf die gerade jemand wartet, mit dem ältesten Beginn.
    pub fn wartende(&self) -> Vec<(String, String)> {
        let raeume: Vec<(String, Arc<SlugWarter>)> = sperren(&self.slugs).iter().map(|(s, r)| (s.clone(), r.clone())).collect();
        raeume.into_iter().filter_map(|(slug, raum)| sperren(&raum.aktive).first().map(|w| (slug, w.seit.clone()))).collect()
    }

    /// Zahl der aktiven Warter eines Slugs.
    #[cfg(test)]
    pub fn anzahl(&self, slug: &str) -> usize {
        sperren(&self.slugs).get(slug).map(|r| sperren(&r.aktive).len()).unwrap_or(0)
    }

    /// Steht der Slug noch in der Karte?
    #[cfg(test)]
    pub fn kennt(&self, slug: &str) -> bool {
        sperren(&self.slugs).contains_key(slug)
    }
}

fn jetzt() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

// ----- Eingaben prüfen -----

/// Fehler des Datenspeichers: unbekannte Kennung → 404, alles andere 500
/// mit Satz (Einzelheiten ins Protokoll).
fn vorgangsfehler(e: SpeicherFehler) -> AppError {
    if let SpeicherFehler::NichtGefunden(_) = e {
        return AppError::nicht_gefunden(e.to_string());
    }
    tracing::error!("Vorgänge: Datenspeicher: {e} ({})", e.einzelheiten());
    AppError::intern("Der Datenspeicher lässt sich gerade nicht lesen oder schreiben; Einzelheiten stehen im Protokoll.")
}

/// Prozentkodierung auflösen (`%2D` → `-`, `+` → Leerzeichen).
fn dekodieren(text: &str) -> String {
    let b = text.as_bytes();
    let hex = |c: u8| (c as char).to_digit(16);
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2]))
        {
            out.push((h * 16 + l) as u8);
            i += 3;
            continue;
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Wert eines Parameters aus der Abfrage, dekodiert.
pub(super) fn abfrage(uri: &Uri, name: &str) -> Option<String> {
    uri.query()?.split('&').find_map(|teil| match teil.split_once('=') {
        Some((k, v)) if dekodieren(k) == name => Some(dekodieren(v)),
        None if dekodieren(teil) == name => Some(String::new()),
        _ => None,
    })
}

pub(super) fn slug_pruefen(slug: &str) -> Result<(), AppError> {
    let ok = !slug.is_empty() && slug.len() <= SLUG_HOECHSTENS && slug.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-');
    if ok {
        return Ok(());
    }
    Err(AppError::unlesbar(format!("Der Slug ist ungültig. Erlaubt sind 1 bis {SLUG_HOECHSTENS} Zeichen aus a–z, 0–9 und Bindestrich.")))
}

/// `?fuer=<slug>`: Pflicht.
fn fuer(uri: &Uri) -> Result<String, AppError> {
    let Some(slug) = abfrage(uri, "fuer") else {
        return Err(AppError::unlesbar("Es fehlt „fuer=<slug>“: für welchen Chat sollen die Vorgänge sein?"));
    };
    slug_pruefen(&slug)?;
    Ok(slug)
}

/// Eine ganze Zahl ab 0 aus der Abfrage, oder nichts.
fn zahl(uri: &Uri, name: &str) -> Result<Option<u64>, AppError> {
    match abfrage(uri, name) {
        None => Ok(None),
        Some(text) => text.parse().map(Some).map_err(|_| AppError::unlesbar(format!("„{name}“ muss eine ganze Zahl ab 0 sein."))),
    }
}

/// `?offen=1` (auch `offen`, `offen=true`); `offen=0` oder fehlt: aus.
pub(super) fn schalter(uri: &Uri, name: &str) -> Result<bool, AppError> {
    match abfrage(uri, name).as_deref() {
        None | Some("0") | Some("false") => Ok(false),
        Some("") | Some("1") | Some("true") => Ok(true),
        Some(_) => Err(AppError::unlesbar(format!("„{name}“ ist 1 oder 0."))),
    }
}

/// Das Feld in Alltagssprache (Auflage aus A2): Der Inhaber liest diese Sätze im
/// Browser, der Chat findet das Feld am Namen in Klammern.
pub(super) fn feld_wort(feld: &str) -> &'static str {
    match feld {
        "titel" => "Der Titel",
        "notiz" => "Die Anmerkung",
        "antwort" => "Die Antwort",
        "eigene" => "Deine Antwort in eigenen Worten",
        "text" => "Der Text",
        "von" => "Die Angabe, wer es schickt,",
        "bezug" => "Der Bezug",
        "knopf" => "Die Beschriftung eines Knopfs",
        "beschreibung" => "Die Beschreibung",
        "pruefbericht" => "Der Prüfbericht",
        _ => "Das Feld",
    }
}

/// Text prüfen: höchstens `max` Zeichen, keine Steuerzeichen (außer Zeilenumbruch, wenn `zeilen`).
/// Die Sätze nennen das Feld in Alltagssprache und seinen Namen in Klammern.
pub(super) fn text_pruefen(feld: &str, wert: &str, max: usize, zeilen: bool) -> Result<(), AppError> {
    let wort = feld_wort(feld);
    if wert.chars().count() > max {
        return Err(AppError::unlesbar(format!("{wort} („{feld}“) ist zu lang: höchstens {max} Zeichen.")));
    }
    if wert.chars().any(|c| c.is_control() && !(zeilen && (c == '\n' || c == '\r' || c == '\t'))) {
        return Err(AppError::unlesbar(format!("{wort} („{feld}“) enthält unsichtbare Steuerzeichen; bitte nur normalen Text schreiben.")));
    }
    Ok(())
}

/// Leere Zeichenkette zählt als nicht angegeben.
pub(super) fn angegeben(wert: Option<String>) -> Option<String> {
    wert.map(|w| w.trim().to_string()).filter(|w| !w.is_empty())
}

/// Kommt die Anfrage aus einem Browser (der setzt `Origin` oder `Sec-Fetch-Site`)?
pub(super) fn aus_browser(kopf: &HeaderMap) -> bool {
    kopf.contains_key(header::ORIGIN) || kopf.contains_key("sec-fetch-site")
}

pub(super) const SATZ_NUR_BROWSER: &str = "Die Herkunft „browser“ ist nur für Anfragen aus dem Dashboard selbst.";
const SATZ_NUR_ANTWORT_IM_BROWSER: &str = "Antworten entstehen nur im Browser, wenn der Inhaber auf der Seite antwortet.";

/// `von` prüfen: Länge, Steuerzeichen, und `browser` nur aus dem Browser
/// (damit ein Chat nicht aus Versehen als Inhaber auftritt).
pub(super) fn herkunft_pruefen(von: &str, browser: bool) -> Result<(), AppError> {
    text_pruefen("von", von, VON_HOECHSTENS, false)?;
    if !browser && von.eq_ignore_ascii_case("browser") {
        return Err(AppError::unlesbar(SATZ_NUR_BROWSER));
    }
    Ok(())
}

/// JSON-Körper lesen; `muster` zeigt im Fehlersatz, was erwartet wird.
fn koerper_lesen<T: DeserializeOwned>(koerper: &[u8], muster: &str) -> Result<T, AppError> {
    serde_json::from_slice(koerper).map_err(|e| {
        tracing::debug!("Vorgänge: Körper unlesbar: {e}");
        AppError::unlesbar(format!("Der Inhalt ist kein passendes JSON. Erwartet wird etwa {muster}."))
    })
}

pub const SATZ_NUR_JSON: &str = "Bitte den Körper als JSON schicken (Content-Type: application/json).";

/// Die schreibenden Wege nehmen nur `Content-Type: application/json` (auch mit
/// `; charset=utf-8`); fehlt der Kopf oder ist er anders: 415.
pub(super) fn json_kopf_pruefen(kopf: &HeaderMap) -> Result<(), AppError> {
    let ist_json = kopf
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .is_some_and(|art| art.trim().eq_ignore_ascii_case("application/json"));
    if ist_json { Ok(()) } else { Err(AppError::falsches_format(SATZ_NUR_JSON)) }
}

const MUSTER_NEU: &str = r#"{"art": "abnahme", "slug": "dashboard", "titel": "…", "bezug": "…", "von": "…"}"#;
const MUSTER_SCHRITT: &str = r#"{"notiz": "…", "von": "…"} (beides freiwillig, leerer Inhalt geht auch)"#;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NeuerVorgang {
    art: String,
    slug: String,
    titel: String,
    #[serde(default)]
    bezug: Option<String>,
    #[serde(default)]
    von: Option<String>,
    // Nur bei `merge` (Etappe 4b):
    #[serde(default)]
    zweig: Option<String>,
    #[serde(default)]
    commit: Option<String>,
    #[serde(default)]
    projekt: Option<String>,
    #[serde(default)]
    beschreibung: Option<String>,
    #[serde(default)]
    pruefbericht: Option<String>,
}

/// Längen der Angaben eines Merge-Requests (der ganze Körper bleibt unter 16 KB).
pub const ZWEIG_HOECHSTENS: usize = 120;
pub const BESCHREIBUNG_HOECHSTENS: usize = 4000;
pub const PRUEFBERICHT_HOECHSTENS: usize = 10000;

/// Die Angaben eines Merge-Requests prüfen (Etappe 4b, `[projekt_backend]`):
/// alle fünf Pflicht; Zweig `projekt/<projekt>-<thema>`, Commit als Hash.
fn merge_pruefen(neu: &mut NeuerVorgang) -> Result<MergeAntrag, AppError> {
    let fehlt =
        |feld: &str| AppError::unlesbar(format!("Für einen Merge-Request fehlt „{feld}“. Nötig sind zweig, commit, projekt, beschreibung und pruefbericht."));
    let projekt = angegeben(neu.projekt.take()).ok_or_else(|| fehlt("projekt"))?;
    slug_pruefen(&projekt)?;
    let zweig = angegeben(neu.zweig.take()).ok_or_else(|| fehlt("zweig"))?;
    let zweig_ok = zweig.len() <= ZWEIG_HOECHSTENS
        && zweig.strip_prefix(&format!("projekt/{projekt}-")).is_some_and(|thema| !thema.is_empty())
        && zweig.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'/' | b'-' | b'_' | b'.'))
        && !zweig.contains("..");
    if !zweig_ok {
        return Err(AppError::unlesbar(format!(
            "Der Zweig muss „projekt/{projekt}-<thema>“ heißen (Buchstaben, Ziffern, - _ . /, höchstens {ZWEIG_HOECHSTENS} Zeichen)."
        )));
    }
    let commit = angegeben(neu.commit.take()).ok_or_else(|| fehlt("commit"))?;
    if !(7..=40).contains(&commit.len()) || !commit.bytes().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()) {
        return Err(AppError::unlesbar("Der Commit ist ein Hash aus 7 bis 40 Zeichen 0–9 und a–f (git rev-parse HEAD)."));
    }
    let beschreibung = angegeben(neu.beschreibung.take()).ok_or_else(|| fehlt("beschreibung"))?;
    text_pruefen("beschreibung", &beschreibung, BESCHREIBUNG_HOECHSTENS, true)?;
    let pruefbericht = angegeben(neu.pruefbericht.take()).ok_or_else(|| fehlt("pruefbericht"))?;
    text_pruefen("pruefbericht", &pruefbericht, PRUEFBERICHT_HOECHSTENS, true)?;
    Ok(MergeAntrag { zweig, commit, projekt, beschreibung, pruefbericht })
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct SchrittKoerper {
    #[serde(default)]
    notiz: Option<String>,
    #[serde(default)]
    von: Option<String>,
}

/// Nach jedem Schritt: Live-Kanal und die Warter des Slugs.
pub fn melden(state: &AppState, e: &Ereignis) {
    let _ = state.vorgang_events.send(e.into());
    state.warteraum.wecken(&e.slug);
}

// ----- Handler -----

/// `POST /api/vorgaenge`: legt einen Vorgang an (Zustand `offen`).
pub async fn anlegen(State(state): State<AppState>, kopf: HeaderMap, koerper: Bytes) -> Result<(StatusCode, Json<VorgangAngelegt>), AppError> {
    json_kopf_pruefen(&kopf)?;
    let mut neu: NeuerVorgang = koerper_lesen(&koerper, MUSTER_NEU)?;
    // Etappe 7c: passkey-antrag legt nur der Server selbst an (beim Antrag auf /anmelden),
    // oeffnen ebenso (beim Öffnen eines Orts, Entscheidung lokal-oeffnen),
    // schritt auch (beim Anbieten und Zurückwecken eines Teilschritts, T3).
    let art = VorgangsArt::aus_wort(&neu.art)
        .filter(|a| !matches!(a, VorgangsArt::PasskeyAntrag | VorgangsArt::Oeffnen | VorgangsArt::Schritt))
        .ok_or_else(|| AppError::unlesbar("Diese Art gibt es nicht. Möglich sind antwort, frage, wunsch, bau, abnahme und merge."))?;
    slug_pruefen(&neu.slug)?;
    let merge = if art == VorgangsArt::Merge {
        Some(merge_pruefen(&mut neu)?)
    } else if [&neu.zweig, &neu.commit, &neu.projekt, &neu.beschreibung, &neu.pruefbericht].iter().any(|f| f.is_some()) {
        return Err(AppError::unlesbar("zweig, commit, projekt, beschreibung und pruefbericht gibt es nur bei der Art merge."));
    } else {
        None
    };
    let titel = neu.titel.trim().to_string();
    if titel.is_empty() {
        return Err(AppError::unlesbar("Der Titel fehlt."));
    }
    text_pruefen("titel", &titel, TITEL_HOECHSTENS, false)?;
    let bezug = angegeben(neu.bezug);
    if let Some(b) = &bezug {
        text_pruefen("bezug", b, BEZUG_HOECHSTENS, false)?;
    }
    let browser = aus_browser(&kopf);
    if !browser && art == VorgangsArt::Antwort {
        return Err(AppError::unlesbar(SATZ_NUR_ANTWORT_IM_BROWSER));
    }
    let von = match angegeben(neu.von) {
        Some(v) => v,
        None if browser => "browser".to_string(),
        None => "chat".to_string(),
    };
    herkunft_pruefen(&von, browser)?;
    let slug = neu.slug;
    let anlegen = move |s: &Speicher| match merge {
        Some(antrag) => s.merge_anlegen(&slug, &titel, bezug.as_deref(), &von, antrag),
        None => s.vorgang_anlegen(art, &slug, &titel, bezug.as_deref(), &von),
    };
    let (vorgang, ereignis) = Speicher::im_hintergrund(state.speicher()?, anlegen).await.map_err(vorgangsfehler)?;
    melden(&state, &ereignis);
    if art == VorgangsArt::Bau {
        // Der Baumeister nimmt ihn an; das Warten des Chats liefert ihn erst mit dem Ergebnis.
        state.baumeister.anstossen();
    }
    Ok((StatusCode::CREATED, Json(VorgangAngelegt { vorgang, ereignis })))
}

/// `GET /api/vorgaenge?fuer=<slug>[&offen=1][&seit=<nr>]`.
pub async fn liste(State(state): State<AppState>, uri: Uri) -> Result<Json<VorgangsListe>, AppError> {
    let slug = fuer(&uri)?;
    let seit = zahl(&uri, "seit")?;
    let offen = schalter(&uri, "offen")?;
    let liste = Speicher::im_hintergrund(state.speicher()?, move |s| s.vorgangs_liste(&slug, seit, offen)).await.map_err(vorgangsfehler)?;
    Ok(Json(liste))
}

/// `GET /api/vorgaenge/uebersicht`: für die Seite /vorgaenge, mit „wartet seit“ je Chat.
pub async fn uebersicht(State(state): State<AppState>) -> Result<Json<Uebersicht>, AppError> {
    // Entscheidung archiv-status: archivierte Projekte fehlen hier
    // (mit `?fuer=<slug>` sieht ihr Chat alles wie sonst).
    let ohne = state.archivierte();
    let ohne_speicher = ohne.clone();
    let mut u = Speicher::im_hintergrund(state.speicher()?, move |s| s.uebersicht_ohne(&ohne_speicher)).await.map_err(vorgangsfehler)?;
    for (slug, seit) in state.warteraum.wartende().into_iter().filter(|(s, _)| !ohne.contains(s)) {
        match u.chats.iter_mut().find(|z| z.slug == slug) {
            Some(z) => z.wartet_seit = Some(seit),
            None => u.chats.push(ChatZeile { slug, wartet_seit: Some(seit), zuletzt_uebernommen: None, offen: 0 }),
        }
    }
    u.chats.sort_by(|a, b| a.slug.cmp(&b.slug));
    Ok(Json(u))
}

/// `GET /api/vorgaenge/<id>`: ein Vorgang mit allen Ereignissen.
pub async fn einer(State(state): State<AppState>, Path(id): Path<String>) -> Result<Json<VorgangDetail>, AppError> {
    let gefunden = Speicher::im_hintergrund(state.speicher()?, move |s| s.vorgang(&id)).await.map_err(vorgangsfehler)?;
    let (vorgang, ereignisse) = gefunden.ok_or_else(|| AppError::nicht_gefunden("Diesen Vorgang gibt es nicht."))?;
    // Etappe A2: beim Vorgang `antwort` zu einer Frage gleich die Antwort des Inhabers dazu.
    let v = vorgang.clone();
    let antwort = Speicher::im_hintergrund(state.speicher()?, move |s| s.antwort_zum_vorgang(&v)).await.map_err(vorgangsfehler)?;
    Ok(Json(VorgangDetail { vorgang, ereignisse, antwort }))
}

/// `POST /api/vorgaenge/<id>/uebernommen`.
pub async fn uebernommen(State(state): State<AppState>, Path(id): Path<String>, kopf: HeaderMap, koerper: Bytes) -> Result<Json<VorgangGeaendert>, AppError> {
    weiter(state, id, kopf, koerper, Zustand::Uebernommen).await
}

/// `POST /api/vorgaenge/<id>/erledigt`.
pub async fn erledigt(State(state): State<AppState>, Path(id): Path<String>, kopf: HeaderMap, koerper: Bytes) -> Result<Json<VorgangGeaendert>, AppError> {
    weiter(state, id, kopf, koerper, Zustand::Erledigt).await
}

/// Gemeinsam für `uebernommen` und `erledigt`: idempotent, nur vorwärts.
/// Ohne `von`: `browser` aus dem Browser, sonst `chat:<slug des Vorgangs>`.
async fn weiter(state: AppState, id: String, kopf: HeaderMap, koerper: Bytes, zustand: Zustand) -> Result<Json<VorgangGeaendert>, AppError> {
    json_kopf_pruefen(&kopf)?;
    let schritt: SchrittKoerper =
        if koerper.iter().all(u8::is_ascii_whitespace) { SchrittKoerper::default() } else { koerper_lesen(&koerper, MUSTER_SCHRITT)? };
    let notiz = angegeben(schritt.notiz);
    if let Some(n) = &notiz {
        text_pruefen("notiz", n, NOTIZ_HOECHSTENS, true)?;
    }
    let browser = aus_browser(&kopf);
    let von = angegeben(schritt.von).or_else(|| browser.then(|| "browser".to_string()));
    if let Some(v) = &von {
        herkunft_pruefen(v, browser)?;
    }
    let (vorgang, ereignis) = Speicher::im_hintergrund(state.speicher()?, move |s| {
        let nicht_da = || SpeicherFehler::NichtGefunden(id.clone());
        let von = match von {
            Some(v) => v,
            None => format!("chat:{}", s.vorgang_kopf(&id)?.ok_or_else(nicht_da)?.slug),
        };
        match s.vorgang_weiter(&id, zustand, &von, notiz.as_deref())? {
            Some((v, e)) => Ok((v, Some(e))),
            None => Ok((s.vorgang_kopf(&id)?.ok_or_else(nicht_da)?, None)),
        }
    })
    .await
    .map_err(vorgangsfehler)?;
    if let Some(e) = &ereignis {
        melden(&state, e);
    }
    Ok(Json(VorgangGeaendert { vorgang, geaendert: ereignis.is_some() }))
}

/// Ob eine Liste etwas zu melden hat: mit `seit` Ereignisse, sonst Vorgänge.
fn hat_etwas(liste: &VorgangsListe, seit: Option<u64>) -> bool {
    match seit {
        Some(_) => !liste.ereignisse.is_empty(),
        None => !liste.vorgaenge.is_empty(),
    }
}

/// Einmal nachsehen. Ohne `seit` werden gelieferte Vorgänge `zugestellt`
/// (die Liste zeigt dann diesen Stand). `cursor` zählt die so geschriebenen
/// Ereignisse mit, damit ein folgendes `seit=<cursor>` nicht das eigene
/// Zustellen meldet; kam zwischen Lesen und Zustellen etwas anderes dazu,
/// bleibt er beim gelesenen Stand (lieber doppelt als verpasst).
async fn nachsehen(state: &AppState, speicher: &Arc<Speicher>, slug: &str, seit: Option<u64>) -> Result<VorgangsListe, AppError> {
    let s2 = slug.to_string();
    let mut liste = Speicher::im_hintergrund(speicher.clone(), move |s| {
        let mut l = s.vorgangs_liste(&s2, seit, true)?;
        // Kleinkram 2: Bau-Vorgänge erledigt der Baumeister selbst; ihr Ergebnis
        // weckt den Chat trotzdem einmal, wie ein neuer Vorgang.
        if seit.is_none() {
            for v in s.bau_ergebnisse(&s2)? {
                if !l.vorgaenge.iter().any(|a| a.id == v.id) {
                    l.vorgaenge.push(v);
                }
            }
        }
        Ok(l)
    })
    .await
    .map_err(vorgangsfehler)?;
    if seit.is_none() && !liste.vorgaenge.is_empty() {
        let ids: Vec<String> = liste.vorgaenge.iter().map(|v| v.id.clone()).collect();
        let zustellung = Speicher::im_hintergrund(speicher.clone(), move |s| s.zustellen(&ids, "server")).await.map_err(vorgangsfehler)?;
        if zustellung.cursor_vorher == liste.cursor {
            liste.cursor = zustellung.cursor;
        }
        for (v, e) in zustellung.geaendert {
            let _ = state.vorgang_events.send((&e).into());
            if let Some(alt) = liste.vorgaenge.iter_mut().find(|a| a.id == v.id) {
                *alt = v;
            }
        }
    }
    Ok(liste)
}

/// `GET /api/vorgaenge/warten?fuer=<slug>&timeout=<s>[&seit=<nr>]`: antwortet
/// sofort, wenn es etwas gibt, sonst beim nächsten Schritt für diesen Slug,
/// leer nach `timeout` Sekunden, wenn ein vierter Warter dazukommt oder der
/// Server beendet wird (Abschaltsignal, dann ohne Vorgänge und Ereignisse).
/// `timeout=0`: nur nachsehen (meldet sich dafür nicht als Warter an).
pub async fn warten(State(state): State<AppState>, uri: Uri) -> Result<Json<VorgangsListe>, AppError> {
    let slug = fuer(&uri)?;
    let seit = zahl(&uri, "seit")?;
    let timeout = zahl(&uri, "timeout")?.unwrap_or(TIMEOUT_STANDARD).min(TIMEOUT_HOECHSTENS);
    let speicher = state.speicher()?;
    if timeout == 0 {
        return Ok(Json(nachsehen(&state, &speicher, &slug, seit).await?));
    }
    let frist = tokio::time::Instant::now() + Duration::from_secs(timeout);
    let (anmeldung, mut abbruch) = state.warteraum.anmelden(&slug);
    let abschaltung = state.abschaltung();
    tokio::pin!(abschaltung);
    loop {
        // Erst das Signal scharf machen, dann lesen: ein Wecken zwischen
        // Lesen und Warten geht so nicht verloren.
        let geweckt = anmeldung.geweckt();
        tokio::pin!(geweckt);
        geweckt.as_mut().enable();
        let liste = nachsehen(&state, &speicher, &slug, seit).await?;
        if hat_etwas(&liste, seit) {
            return Ok(Json(liste));
        }
        tokio::select! {
            _ = &mut geweckt => {}
            _ = &mut abbruch => {
                tracing::info!("Warten für {slug}: von einem neueren Warter abgelöst");
                return Ok(Json(liste));
            }
            _ = tokio::time::sleep_until(frist) => return Ok(Json(liste)),
            _ = &mut abschaltung => {
                tracing::info!("Warten für {slug}: der Server wird beendet");
                return Ok(Json(VorgangsListe { vorgaenge: Vec::new(), ereignisse: Vec::new(), cursor: liste.cursor }));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Instant;

    use axum::body::Body;
    use axum::http::Request;
    use axum::response::Response;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;
    use crate::app::router_angemeldet as router;

    fn zustand() -> AppState {
        AppState::fuer_tests(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel"))
    }

    fn get(pfad: &str) -> Request<Body> {
        Request::get(pfad).header("host", "127.0.0.1:8787").body(Body::empty()).unwrap()
    }

    /// Wie `curl -H "Content-Type: application/json" -d …`.
    fn post(pfad: &str, koerper: &str) -> Request<Body> {
        Request::post(pfad).header("host", "127.0.0.1:8787").header("content-type", "application/json").body(Body::from(koerper.to_string())).unwrap()
    }

    async fn schicke(state: &AppState, req: Request<Body>) -> (StatusCode, serde_json::Value) {
        let res: Response = router(state.clone()).oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let v = serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("kein JSON ({e}): {}", String::from_utf8_lossy(&bytes)));
        (status, v)
    }

    async fn neu(state: &AppState, slug: &str, titel: &str) -> serde_json::Value {
        let koerper = format!(r#"{{"art": "abnahme", "slug": "{slug}", "titel": "{titel}"}}"#);
        let (status, v) = schicke(state, post("/api/vorgaenge", &koerper)).await;
        assert_eq!(status, StatusCode::CREATED, "{v}");
        v["vorgang"].clone()
    }

    /// Wartet, bis `n` Warter für `slug` angemeldet sind (höchstens 2 s).
    async fn bis_warter(state: &AppState, slug: &str, n: usize) {
        let frist = Instant::now() + Duration::from_secs(2);
        while state.warteraum.anzahl(slug) != n {
            assert!(Instant::now() < frist, "{} statt {n} Warter", state.warteraum.anzahl(slug));
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    #[test]
    fn abfrage_dekodiert() {
        let uri: Uri = "/x?fuer=beispi%65l&leer&titel=a+b%20c&kaputt=%zz%4".parse().unwrap();
        assert_eq!(abfrage(&uri, "fuer").as_deref(), Some("beispiel"));
        assert_eq!(abfrage(&uri, "leer").as_deref(), Some(""));
        assert_eq!(abfrage(&uri, "titel").as_deref(), Some("a b c"));
        assert_eq!(abfrage(&uri, "kaputt").as_deref(), Some("%zz%4"));
        assert_eq!(abfrage(&uri, "fehlt"), None);
        assert!(slug_pruefen("dashboard-2").is_ok());
        for falsch in ["", "Dashboard", "a_b", "a/b", "ä", &"x".repeat(65)] {
            assert!(slug_pruefen(falsch).is_err(), "{falsch}");
        }
    }

    #[tokio::test]
    async fn anlegen_abfragen_und_einzeln_lesen() {
        let state = zustand();
        let mut live = state.vorgang_events.subscribe();
        let koerper = r#"{"art": "abnahme", "slug": "dashboard", "titel": " Etappe 2b ", "bezug": "abnahme:2b"}"#;
        let (status, v) = schicke(&state, post("/api/vorgaenge", koerper)).await;
        assert_eq!(status, StatusCode::CREATED, "{v}");
        let id = v["vorgang"]["id"].as_str().unwrap().to_string();
        assert!(id.starts_with("v-") && id.len() == 15, "{id}");
        assert_eq!(v["vorgang"]["zustand"], "offen");
        assert_eq!(v["vorgang"]["art"], "abnahme");
        assert_eq!(v["vorgang"]["titel"], "Etappe 2b");
        assert_eq!(v["vorgang"]["von"], "chat", "curl ohne Browser-Kopfzeilen");
        assert_eq!(v["ereignis"]["nr"], 1);
        assert_eq!(v["ereignis"]["vorgang"], id.as_str());
        let ereignis = live.try_recv().expect("Live-Ereignis");
        assert_eq!((ereignis.id.as_str(), ereignis.slug.as_str(), ereignis.nr), (id.as_str(), "dashboard", 1));
        assert_eq!((ereignis.art, ereignis.zustand), (VorgangsArt::Abnahme, Zustand::Offen));
        // Aus dem Browser: `von` ist dann „browser“.
        let req = Request::post("/api/vorgaenge")
            .header("host", "127.0.0.1:8787")
            .header("origin", "http://127.0.0.1:8787")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"art": "antwort", "slug": "beispiel", "titel": "Schlusssatz"}"#))
            .unwrap();
        let (status, v) = schicke(&state, req).await;
        assert_eq!(status, StatusCode::CREATED, "{v}");
        assert_eq!(v["vorgang"]["von"], "browser");

        let (status, l) = schicke(&state, get("/api/vorgaenge?fuer=dashboard&offen=1")).await;
        assert_eq!(status, StatusCode::OK, "{l}");
        assert_eq!(l["vorgaenge"].as_array().unwrap().len(), 1, "{l}");
        assert_eq!(l["vorgaenge"][0]["id"], id.as_str());
        assert_eq!(l["ereignisse"], serde_json::json!([]));
        assert_eq!(l["cursor"], 2);
        let (status, d) = schicke(&state, get(&format!("/api/vorgaenge/{id}"))).await;
        assert_eq!(status, StatusCode::OK, "{d}");
        assert_eq!(d["vorgang"]["bezug"], "abnahme:2b");
        assert_eq!(d["ereignisse"].as_array().unwrap().len(), 1);
        let (status, f) = schicke(&state, get("/api/vorgaenge/v-19990101-0001")).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(f["art"], "nicht-gefunden");
        assert_eq!(f["fehler"], "Diesen Vorgang gibt es nicht.");
        let (status, f) = schicke(&state, get("/api/vorgaenge?offen=1")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(f["fehler"].as_str().unwrap().contains("fuer="), "{f}");
    }

    #[tokio::test]
    async fn anlegen_prueft_die_eingaben() {
        let state = zustand();
        let lang = "x".repeat(201);
        let faelle = [
            (r#"{"art": "brief", "slug": "a", "titel": "t"}"#.to_string(), "Art"),
            (r#"{"art": "bau", "slug": "Gross", "titel": "t"}"#.to_string(), "Slug"),
            (r#"{"art": "bau", "slug": "a", "titel": "  "}"#.to_string(), "Titel"),
            (format!(r#"{{"art": "bau", "slug": "a", "titel": "{lang}"}}"#), "zu lang"),
            (r#"{"art": "bau", "slug": "a", "titel": "t", "bezug": "a\nb"}"#.to_string(), "Steuerzeichen"),
            (r#"{"art": "bau", "slug": "a", "titel": "t", "farbe": "rot"}"#.to_string(), "JSON"),
            ("art=bau".to_string(), "JSON"),
            (String::new(), "JSON"),
        ];
        for (koerper, satz) in faelle {
            let (status, v) = schicke(&state, post("/api/vorgaenge", &koerper)).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{koerper}: {v}");
            assert_eq!(v["art"], "unlesbar");
            assert!(v["fehler"].as_str().unwrap().contains(satz), "{koerper}: {v}");
        }
        assert_eq!(state.speicher().unwrap().cursor().unwrap(), 0, "nichts angelegt");
    }

    /// Etappe 4b: Merge-Request als Vorgang `merge` mit zweig, commit, projekt,
    /// beschreibung und pruefbericht; die Übersicht zeigt die offenen.
    #[tokio::test]
    async fn merge_request_mit_angaben_und_in_der_uebersicht() {
        let state = zustand();
        let gut = r#"{"art": "merge", "slug": "dashboard", "titel": "Modul bauplan: Stufen", "von": "chat:bauplan",
            "zweig": "projekt/bauplan-stufen", "commit": "0123abc", "projekt": "bauplan",
            "beschreibung": "GET /api/projekte/bauplan/x/stufen\n[antwort]\ngesamt = 5", "pruefbericht": "cargo test: 6 grün"}"#;
        let (status, v) = schicke(&state, post("/api/vorgaenge", gut)).await;
        assert_eq!(status, StatusCode::CREATED, "{v}");
        let id = v["vorgang"]["id"].as_str().unwrap().to_string();
        assert_eq!(v["vorgang"]["art"], "merge");
        assert_eq!(v["vorgang"]["merge"]["zweig"], "projekt/bauplan-stufen");
        assert_eq!(v["vorgang"]["merge"]["pruefbericht"], "cargo test: 6 grün");
        let (_, d) = schicke(&state, get(&format!("/api/vorgaenge/{id}"))).await;
        assert_eq!(d["vorgang"]["merge"]["commit"], "0123abc", "{d}");
        let (_, u) = schicke(&state, get("/api/vorgaenge/uebersicht")).await;
        assert_eq!(u["merge_offen"][0]["id"], id.as_str(), "{u}");
        // Andere Arten tragen kein Feld `merge`.
        let andere = neu(&state, "dashboard", "Abnahme").await;
        assert!(andere.get("merge").is_none(), "{andere}");
        let faelle = [
            (gut.replace(r#""commit": "0123abc", "#, ""), "fehlt „commit“"),
            (gut.replace("projekt/bauplan-stufen", "main"), "projekt/bauplan-<thema>"),
            (gut.replace("projekt/bauplan-stufen", "projekt/bauplan-"), "projekt/bauplan-<thema>"),
            (gut.replace("0123abc", "XYZ"), "Hash"),
            (gut.replace(r#""projekt": "bauplan""#, r#""projekt": "Gross""#), "Slug"),
            (gut.replace(r#""art": "merge""#, r#""art": "abnahme""#), "nur bei der Art merge"),
        ];
        for (koerper, satz) in faelle {
            let (status, v) = schicke(&state, post("/api/vorgaenge", &koerper)).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{koerper}: {v}");
            assert!(v["fehler"].as_str().unwrap().contains(satz), "{satz}: {v}");
        }
        // Erledigt: nicht mehr offen.
        let (status, _) = schicke(&state, post(&format!("/api/vorgaenge/{id}/erledigt"), "")).await;
        assert_eq!(status, StatusCode::OK);
        let (_, u) = schicke(&state, get("/api/vorgaenge/uebersicht")).await;
        assert_eq!(u["merge_offen"], serde_json::json!([]), "{u}");
    }

    #[tokio::test]
    async fn doppeltes_uebernommen_aendert_nichts() {
        let state = zustand();
        let id = neu(&state, "dashboard", "a").await["id"].as_str().unwrap().to_string();
        let pfad = format!("/api/vorgaenge/{id}/uebernommen");
        let (status, v) = schicke(&state, post(&pfad, "")).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!((v["geaendert"].as_bool(), v["vorgang"]["zustand"].as_str()), (Some(true), Some("uebernommen")));
        let (status, v) = schicke(&state, post(&pfad, "  ")).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["geaendert"], false);
        assert_eq!(v["vorgang"]["letzte_nr"], 2);
        let (_, d) = schicke(&state, get(&format!("/api/vorgaenge/{id}"))).await;
        let ereignisse = d["ereignisse"].as_array().unwrap();
        assert_eq!(ereignisse.len(), 2, "{d}");
        assert_eq!(ereignisse[1]["von"], "chat:dashboard", "Standard-Herkunft ist der Chat des Vorgangs");
        // Erledigt mit Notiz, dann rückwärts: ändert nichts.
        let (status, v) = schicke(&state, post(&format!("/api/vorgaenge/{id}/erledigt"), r#"{"notiz": "Frage entfernt"}"#)).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!((v["geaendert"].as_bool(), v["vorgang"]["notiz"].as_str()), (Some(true), Some("Frage entfernt")));
        let (_, v) = schicke(&state, post(&pfad, r#"{"von": "chat:x"}"#)).await;
        assert_eq!((v["geaendert"].as_bool(), v["vorgang"]["zustand"].as_str()), (Some(false), Some("erledigt")));
        assert_eq!(state.speicher().unwrap().cursor().unwrap(), 3);
        // Unbekannte Kennung, zu lange Notiz, fremde Felder.
        let (status, v) = schicke(&state, post("/api/vorgaenge/v-19990101-0001/uebernommen", "")).await;
        assert_eq!((status, v["art"].as_str()), (StatusCode::NOT_FOUND, Some("nicht-gefunden")));
        let notiz = format!(r#"{{"notiz": "{}"}}"#, "n".repeat(501));
        assert_eq!(schicke(&state, post(&pfad, &notiz)).await.0, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(schicke(&state, post(&pfad, r#"{"zustand": "offen"}"#)).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn seit_liefert_genau_die_fehlenden() {
        let state = zustand();
        let a = neu(&state, "dashboard", "a").await; // 1
        neu(&state, "bauplan", "b").await; // 2
        let id = a["id"].as_str().unwrap();
        schicke(&state, post(&format!("/api/vorgaenge/{id}/uebernommen"), "")).await; // 3
        neu(&state, "dashboard", "c").await; // 4
        let (status, l) = schicke(&state, get("/api/vorgaenge?fuer=dashboard&seit=1")).await;
        assert_eq!(status, StatusCode::OK, "{l}");
        let nummern: Vec<u64> = l["ereignisse"].as_array().unwrap().iter().map(|e| e["nr"].as_u64().unwrap()).collect();
        assert_eq!(nummern, [3, 4]);
        assert_eq!(l["vorgaenge"].as_array().unwrap().len(), 2, "jeder betroffene Vorgang einmal: {l}");
        assert_eq!(l["cursor"], 4);
        let (_, l) = schicke(&state, get("/api/vorgaenge?fuer=dashboard&seit=4")).await;
        assert!(l["ereignisse"].as_array().unwrap().is_empty() && l["vorgaenge"].as_array().unwrap().is_empty());
        let (status, _) = schicke(&state, get("/api/vorgaenge?fuer=dashboard&seit=-1")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn warten_mit_null_liefert_sofort_und_stellt_zu() {
        let state = zustand();
        let (_, l) = schicke(&state, get("/api/vorgaenge/warten?fuer=dashboard&timeout=0")).await;
        assert_eq!(l, serde_json::json!({ "vorgaenge": [], "ereignisse": [], "cursor": 0 }));
        let v = neu(&state, "dashboard", "a").await;
        let mut live = state.vorgang_events.subscribe();
        let start = Instant::now();
        let (status, l) = schicke(&state, get("/api/vorgaenge/warten?fuer=dashboard&timeout=0")).await;
        assert!(start.elapsed() < Duration::from_secs(1));
        assert_eq!(status, StatusCode::OK, "{l}");
        assert_eq!(l["vorgaenge"][0]["id"], v["id"]);
        assert_eq!(l["vorgaenge"][0]["zustand"], "zugestellt");
        assert_eq!(l["cursor"], 2, "Cursor nach dem eigenen Zustellen");
        assert_eq!(live.try_recv().unwrap().zustand, Zustand::Zugestellt);
        assert_eq!(state.warteraum.anzahl("dashboard"), 0, "nur nachsehen meldet keinen Warter an");
        // Mit diesem Cursor weiterlesen: das eigene Zustellen kommt nicht noch einmal.
        let (_, l) = schicke(&state, get("/api/vorgaenge/warten?fuer=dashboard&timeout=0&seit=2")).await;
        assert_eq!(l, serde_json::json!({ "vorgaenge": [], "ereignisse": [], "cursor": 2 }));
        // Nicht übernommen: kommt beim nächsten Warten und in `offen=1` wieder.
        let (_, l) = schicke(&state, get("/api/vorgaenge?fuer=dashboard&offen=1")).await;
        assert_eq!(l["vorgaenge"][0]["zustand"], "zugestellt");
        let (_, l) = schicke(&state, get("/api/vorgaenge/warten?fuer=dashboard&timeout=5")).await;
        assert_eq!(l["vorgaenge"].as_array().unwrap().len(), 1);
        assert_eq!(state.speicher().unwrap().cursor().unwrap(), 2, "zweites Zustellen schreibt kein Ereignis");
        // Mit `seit`: Ereignisse, ohne zuzustellen.
        let (_, l) = schicke(&state, get("/api/vorgaenge/warten?fuer=dashboard&timeout=0&seit=1")).await;
        assert_eq!(l["ereignisse"][0]["zustand"], "zugestellt");
        assert_eq!(l["ereignisse"].as_array().unwrap().len(), 1);
        let (status, _) = schicke(&state, get("/api/vorgaenge/warten?timeout=0")).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn warten_wird_von_einem_neuen_vorgang_geweckt() {
        let state = zustand();
        let s = state.clone();
        let warter = tokio::spawn(async move { schicke(&s, get("/api/vorgaenge/warten?fuer=dashboard&timeout=10")).await });
        bis_warter(&state, "dashboard", 1).await;
        // Ein Vorgang für einen anderen Slug weckt ihn nicht (er wartet weiter).
        neu(&state, "bauplan", "fremd").await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(!warter.is_finished());
        let start = Instant::now();
        let v = neu(&state, "dashboard", "Etappe 2b bereit").await;
        let (status, l) = warter.await.unwrap();
        assert!(start.elapsed() < Duration::from_secs(1), "{:?}", start.elapsed());
        assert_eq!(status, StatusCode::OK, "{l}");
        assert_eq!(l["vorgaenge"][0]["id"], v["id"]);
        assert_eq!(l["vorgaenge"][0]["zustand"], "zugestellt");
        assert_eq!(state.warteraum.anzahl("dashboard"), 0, "abgemeldet");
        assert!(!state.warteraum.kennt("dashboard"), "kein Eintrag bleibt liegen");
        assert!(state.warteraum.wartet_seit("dashboard").is_none());
    }

    /// Der Warteraum vergisst einen Slug, sobald niemand mehr auf ihn wartet.
    #[test]
    fn warteraum_raeumt_leere_slugs_weg() {
        let raum = Warteraum::default();
        let (a, _) = raum.anmelden("dashboard");
        let (b, _) = raum.anmelden("dashboard");
        assert_eq!(raum.anzahl("dashboard"), 2);
        drop(a);
        assert!(raum.kennt("dashboard"), "einer wartet noch");
        assert!(raum.wartet_seit("dashboard").is_some());
        drop(b);
        assert!(!raum.kennt("dashboard"));
        assert!(raum.wartet_seit("dashboard").is_none());
        assert!(raum.wartende().is_empty());
        // Ein neuer Warter danach bekommt einen frischen Eintrag und wird geweckt.
        let (c, _) = raum.anmelden("dashboard");
        let geweckt = c.geweckt();
        tokio::pin!(geweckt);
        geweckt.as_mut().enable();
        raum.wecken("dashboard");
        assert!(schon_fertig(geweckt.as_mut()), "Wecken erreicht den neuen Warter");
    }

    /// Ist die Zukunft schon fertig (ohne zu warten)?
    fn schon_fertig<F: std::future::Future>(f: std::pin::Pin<&mut F>) -> bool {
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        f.poll(&mut cx).is_ready()
    }

    #[tokio::test]
    async fn vierter_warter_beendet_den_aeltesten() {
        let state = zustand();
        let mut warter = Vec::new();
        for n in 1..=3 {
            let s = state.clone();
            warter.push(tokio::spawn(async move { schicke(&s, get("/api/vorgaenge/warten?fuer=dashboard&timeout=30")).await }));
            bis_warter(&state, "dashboard", n).await;
        }
        let (_, u) = schicke(&state, get("/api/vorgaenge/uebersicht")).await;
        assert_eq!(u["chats"][0]["slug"], "dashboard", "{u}");
        assert!(u["chats"][0]["wartet_seit"].is_string(), "{u}");
        let s = state.clone();
        let vierter = tokio::spawn(async move { schicke(&s, get("/api/vorgaenge/warten?fuer=dashboard&timeout=30")).await });
        let start = Instant::now();
        let erster = warter.remove(0);
        let (status, l) = erster.await.unwrap();
        assert!(start.elapsed() < Duration::from_secs(1));
        assert_eq!(status, StatusCode::OK);
        assert_eq!(l["vorgaenge"], serde_json::json!([]));
        bis_warter(&state, "dashboard", 3).await;
        // Reißt die Leitung (hier: Aufgabe abgebrochen), meldet sich der Warter ab.
        warter.push(vierter);
        for w in warter {
            w.abort();
            let _ = w.await;
        }
        assert_eq!(state.warteraum.anzahl("dashboard"), 0);
        assert!(state.warteraum.wartet_seit("dashboard").is_none());
        assert!(!state.warteraum.kennt("dashboard"));
    }

    /// Die Herkunft `browser` und die Art `antwort` gibt es nur aus dem Browser
    /// (`Origin` oder `Sec-Fetch-Site`); per curl ohne diese Kopfzeilen: 422.
    #[tokio::test]
    async fn browser_herkunft_und_antworten_nur_aus_dem_browser() {
        let state = zustand();
        let faelle = [
            (r#"{"art": "bau", "slug": "a", "titel": "t", "von": "browser"}"#, SATZ_NUR_BROWSER),
            (r#"{"art": "bau", "slug": "a", "titel": "t", "von": " Browser "}"#, SATZ_NUR_BROWSER),
            (r#"{"art": "antwort", "slug": "a", "titel": "t"}"#, SATZ_NUR_ANTWORT_IM_BROWSER),
            (r#"{"art": "antwort", "slug": "a", "titel": "t", "von": "chat:a"}"#, SATZ_NUR_ANTWORT_IM_BROWSER),
        ];
        for (koerper, satz) in faelle {
            let (status, v) = schicke(&state, post("/api/vorgaenge", koerper)).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{koerper}: {v}");
            assert_eq!((v["art"].as_str(), v["fehler"].as_str()), (Some("unlesbar"), Some(satz)), "{koerper}");
        }
        assert_eq!(state.speicher().unwrap().cursor().unwrap(), 0, "nichts angelegt");
        // Beim Weiterschalten ebenso.
        let id = neu(&state, "dashboard", "a").await["id"].as_str().unwrap().to_string();
        let (status, v) = schicke(&state, post(&format!("/api/vorgaenge/{id}/uebernommen"), r#"{"von": "browser"}"#)).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(SATZ_NUR_BROWSER)));
        assert_eq!(state.speicher().unwrap().cursor().unwrap(), 1, "kein Schritt geschrieben");
        // Aus dem Dashboard selbst (eigener Origin) geht beides.
        let req = Request::post("/api/vorgaenge")
            .header("host", "127.0.0.1:8787")
            .header("origin", "http://127.0.0.1:8787")
            .header("content-type", "application/json; charset=utf-8")
            .body(Body::from(r#"{"art": "antwort", "slug": "beispiel", "titel": "Schlusssatz", "von": "browser"}"#))
            .unwrap();
        let (status, v) = schicke(&state, req).await;
        assert_eq!(status, StatusCode::CREATED, "{v}");
        assert_eq!((v["vorgang"]["art"].as_str(), v["vorgang"]["von"].as_str()), (Some("antwort"), Some("browser")));
        let req = Request::post(format!("/api/vorgaenge/{id}/uebernommen"))
            .header("host", "127.0.0.1:8787")
            .header("sec-fetch-site", "same-origin")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap();
        let (status, v) = schicke(&state, req).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let (_, d) = schicke(&state, get(&format!("/api/vorgaenge/{id}"))).await;
        assert_eq!(d["ereignisse"][1]["von"], "browser");
    }

    /// Server-Neustart beim Warten verliert nichts: Der Vorgang steht in der
    /// Datei, und das erste Warten nach dem Neustart stellt ihn erneut zu.
    #[tokio::test]
    async fn server_neustart_beim_warten_verliert_nichts() {
        // Eigener Ordner (nicht `temporaer()`, das löscht beim Freigeben), damit
        // auch der Sicherungsordner daneben mit aufgeräumt werden kann.
        let ordner = crate::speicher::test_pfad().with_extension("neustart");
        let _ = std::fs::remove_dir_all(&ordner);
        let pfad = ordner.join("dashboard.redb");
        let mit_speicher = |s: Speicher| {
            let mut state = zustand();
            state.speicher = Some(Arc::new(s));
            state
        };
        let state = mit_speicher(Speicher::oeffnen(&pfad).unwrap());
        let v = neu(&state, "dashboard", "Etappe 2b bereit").await;
        let (_, l) = schicke(&state, get("/api/vorgaenge/warten?fuer=dashboard&timeout=0")).await;
        assert_eq!(l["vorgaenge"][0]["zustand"], "zugestellt");
        // Ein Warter hängt, als der Server endet.
        let s = state.clone();
        let warter = tokio::spawn(async move { schicke(&s, get("/api/vorgaenge/warten?fuer=bauplan&timeout=30")).await });
        bis_warter(&state, "bauplan", 1).await;
        warter.abort();
        let _ = warter.await;
        // Der Warter meldet sich an, bevor er liest; das Lesen läuft in
        // spawn_blocking und hält den Speicher, bis es fertig ist, auch nach
        // abort(). Erst wenn alle Halter weg sind, ist die Datei frei (sonst
        // unter Last „Datei belegt“).
        let alt = Arc::downgrade(&state.speicher().unwrap());
        drop(state);
        let frist = Instant::now() + Duration::from_secs(10);
        while alt.strong_count() > 0 {
            assert!(Instant::now() < frist, "der alte Speicher wird nicht freigegeben");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let neu_geoeffnet = Speicher::oeffnen(&pfad).expect("dieselbe Datei lässt sich wieder öffnen");
        let offen = neu_geoeffnet.vorgaenge_fuer("dashboard", true).unwrap();
        assert_eq!(offen.len(), 1);
        assert_eq!((offen[0].id.as_str(), offen[0].zustand), (v["id"].as_str().unwrap(), Zustand::Zugestellt));
        let state = mit_speicher(neu_geoeffnet);
        let (status, l) = schicke(&state, get("/api/vorgaenge/warten?fuer=dashboard&timeout=0")).await;
        assert_eq!(status, StatusCode::OK, "{l}");
        assert_eq!(l["vorgaenge"].as_array().unwrap().len(), 1, "wieder zugestellt: {l}");
        assert_eq!(l["vorgaenge"][0]["id"], v["id"]);
        assert_eq!(l["cursor"], 2, "kein zweites Ereignis fürs erneute Zustellen");
        assert!(!state.warteraum.kennt("bauplan"), "der Warteraum beginnt leer");
        drop(state);
        std::fs::remove_dir_all(&ordner).unwrap();
    }

    #[tokio::test]
    async fn uebersicht_zaehlt() {
        let state = zustand();
        let a = neu(&state, "dashboard", "a").await;
        neu(&state, "bauplan", "b").await;
        schicke(&state, post(&format!("/api/vorgaenge/{}/uebernommen", a["id"].as_str().unwrap()), "")).await;
        let (status, u) = schicke(&state, get("/api/vorgaenge/uebersicht")).await;
        assert_eq!(status, StatusCode::OK, "{u}");
        let zaehler = serde_json::json!({ "offen": 1, "zugestellt": 0, "uebernommen_heute": 1, "erledigt_heute": 0 });
        assert_eq!(u["zaehler"], zaehler);
        assert_eq!(u["chats"].as_array().unwrap().len(), 2);
        assert_eq!(u["chats"][1]["slug"], "dashboard");
        assert!(u["chats"][1]["zuletzt_uebernommen"].is_string());
        assert_eq!(u["chats"][1]["wartet_seit"], serde_json::Value::Null);
        assert_eq!(u["vorgaenge"][0]["id"], a["id"], "zuletzt geändert zuerst");
        assert_eq!(u["cursor"], 3);
    }

    /// Das Live-Ereignis `vorgang` kommt im SSE-Strom an.
    #[tokio::test]
    async fn live_kanal_meldet_vorgaenge() {
        let state = zustand();
        let res = router(state.clone()).oneshot(get("/api/events")).await.unwrap();
        let mut strom = res.into_body();
        neu(&state, "dashboard", "a").await;
        let frist = Instant::now() + Duration::from_secs(3);
        let mut text = String::new();
        while !text.contains("event: vorgang") {
            assert!(Instant::now() < frist, "kein Ereignis vorgang: {text}");
            let teil = tokio::time::timeout(Duration::from_secs(3), strom.frame()).await.unwrap();
            if let Some(Ok(f)) = teil
                && let Ok(daten) = f.into_data()
            {
                text.push_str(&String::from_utf8_lossy(&daten));
            }
        }
        assert!(text.contains(r#""slug":"dashboard""#) && text.contains(r#""zustand":"offen""#), "{text}");
    }

    /// Beim Beenden kehrt ein Warter sofort leer zurück, statt bis zum
    /// Timeout zu hängen.
    #[tokio::test]
    async fn abschalten_beendet_den_warter_sofort() {
        let state = zustand();
        let s = state.clone();
        let warter = tokio::spawn(async move { schicke(&s, get("/api/vorgaenge/warten?fuer=dashboard&timeout=30")).await });
        bis_warter(&state, "dashboard", 1).await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        let start = Instant::now();
        state.abschalten_ausloesen();
        let (status, l) = tokio::time::timeout(Duration::from_secs(2), warter).await.expect("Warter hängt").unwrap();
        assert!(start.elapsed() < Duration::from_secs(2));
        assert_eq!(status, StatusCode::OK);
        assert_eq!(l, serde_json::json!({ "vorgaenge": [], "ereignisse": [], "cursor": 0 }));
        bis_warter(&state, "dashboard", 0).await;
    }

    /// Beim Beenden endet der SSE-Strom, damit die Verbindung schließt.
    #[tokio::test]
    async fn abschalten_beendet_den_live_kanal() {
        let state = zustand();
        let res = router(state.clone()).oneshot(get("/api/events")).await.unwrap();
        let mut strom = res.into_body();
        // Erst pollen: der Strom läuft (der erste PC-Wert kommt sofort).
        let erstes = tokio::time::timeout(Duration::from_millis(200), strom.frame()).await;
        assert!(!matches!(erstes, Ok(None)), "der Strom darf vor dem Signal nicht enden");
        state.abschalten_ausloesen();
        let frist = Instant::now() + Duration::from_secs(2);
        loop {
            let rest = frist.saturating_duration_since(Instant::now());
            match tokio::time::timeout(rest, strom.frame()).await.expect("SSE-Strom endet nicht") {
                None => break,
                Some(Ok(_)) => continue,
                Some(Err(e)) => panic!("Fehler im Strom: {e}"),
            }
        }
    }

    #[tokio::test]
    async fn ohne_speicher_antworten_die_vorgaenge_mit_503() {
        let beispiel = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel");
        let state = AppState::fuer_tests_ohne_speicher(&beispiel, "kaputt");
        for req in [
            get("/api/vorgaenge?fuer=a"),
            get("/api/vorgaenge/uebersicht"),
            get("/api/vorgaenge/warten?fuer=a&timeout=1"),
            get("/api/vorgaenge/v-1"),
            post("/api/vorgaenge", r#"{"art": "bau", "slug": "a", "titel": "t"}"#),
            post("/api/vorgaenge/v-1/erledigt", ""),
        ] {
            let pfad = req.uri().to_string();
            let (status, v) = schicke(&state, req).await;
            assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{pfad}: {v}");
            assert_eq!(v["art"], "speicher-fehlt");
        }
    }

    /// Etappe 2b, Auflage: die schreibenden Wege nur mit `Content-Type: application/json`.
    #[tokio::test]
    async fn schreiben_nur_mit_json_kopf() {
        let state = zustand();
        let id = neu(&state, "dashboard", "a").await["id"].as_str().unwrap().to_string();
        let koerper = r#"{"art": "bau", "slug": "a", "titel": "t"}"#;
        let pfade = ["/api/vorgaenge".to_string(), format!("/api/vorgaenge/{id}/uebernommen"), format!("/api/vorgaenge/{id}/erledigt")];
        for pfad in &pfade {
            for art in [None, Some("text/plain"), Some("application/x-www-form-urlencoded"), Some("application/jsonx")] {
                let mut req = Request::post(pfad.as_str()).header("host", "127.0.0.1:8787");
                if let Some(a) = art {
                    req = req.header("content-type", a);
                }
                let (status, v) = schicke(&state, req.body(Body::from(koerper)).unwrap()).await;
                assert_eq!(status, StatusCode::UNSUPPORTED_MEDIA_TYPE, "{pfad} {art:?}: {v}");
                assert_eq!((v["art"].as_str(), v["fehler"].as_str()), (Some("unlesbar"), Some(SATZ_NUR_JSON)), "{pfad}");
            }
        }
        assert_eq!(state.speicher().unwrap().cursor().unwrap(), 1, "nur der erste Vorgang");
        let req = Request::post("/api/vorgaenge")
            .header("host", "127.0.0.1:8787")
            .header("content-type", "Application/JSON; charset=UTF-8")
            .body(Body::from(koerper))
            .unwrap();
        assert_eq!(schicke(&state, req).await.0, StatusCode::CREATED);
    }

    /// `seit` mit gleichzeitigen Schreibern: vier Aufgaben legen je 25 Vorgänge
    /// an, während eine Leseaufgabe fortlaufend `seit=<cursor>` fragt. Was sie
    /// liest, ist am Ende genau 1 bis 100, jede Nummer einmal. Danach sind die
    /// Nummern lückenlos, und `seit` liefert genau den Rest.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn seit_mit_gleichzeitigen_schreibern() {
        let state = zustand();
        let fertig = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let leser = {
            let (state, fertig) = (state.clone(), fertig.clone());
            tokio::spawn(async move {
                let mut cursor = 0u64;
                let mut gelesen: Vec<u64> = Vec::new();
                let mut runden = 0u32;
                loop {
                    // Erst merken, ob die Schreiber fertig waren; dann ist diese Runde die letzte.
                    let letzte = fertig.load(std::sync::atomic::Ordering::SeqCst);
                    let (status, l) = schicke(&state, get(&format!("/api/vorgaenge?fuer=dashboard&seit={cursor}"))).await;
                    assert_eq!(status, StatusCode::OK, "{l}");
                    gelesen.extend(l["ereignisse"].as_array().unwrap().iter().map(|e| e["nr"].as_u64().unwrap()));
                    let neu = l["cursor"].as_u64().unwrap();
                    assert!(neu >= cursor, "cursor ging zurück: {cursor} → {neu}");
                    cursor = neu;
                    runden += 1;
                    if letzte {
                        return (gelesen, runden);
                    }
                    tokio::task::yield_now().await;
                }
            })
        };
        let aufgaben: Vec<_> = (0..4)
            .map(|a| {
                let state = state.clone();
                tokio::spawn(async move {
                    for i in 0..25 {
                        neu(&state, "dashboard", &format!("{a}-{i}")).await;
                    }
                })
            })
            .collect();
        for a in aufgaben {
            a.await.unwrap();
        }
        fertig.store(true, std::sync::atomic::Ordering::SeqCst);
        let (gelesen, runden) = leser.await.unwrap();
        let menge: std::collections::BTreeSet<u64> = gelesen.iter().copied().collect();
        assert_eq!(menge.len(), gelesen.len(), "doppelt gelesen ({runden} Runden): {gelesen:?}");
        assert_eq!(menge, (1..=100).collect(), "{runden} Runden: {gelesen:?}");
        assert!(runden > 1, "der Leser lief nicht neben den Schreibern");
        let nummern = |l: &serde_json::Value| -> Vec<u64> { l["ereignisse"].as_array().unwrap().iter().map(|e| e["nr"].as_u64().unwrap()).collect() };
        let (status, l) = schicke(&state, get("/api/vorgaenge?fuer=dashboard&seit=0")).await;
        assert_eq!(status, StatusCode::OK, "{l}");
        assert_eq!(nummern(&l), (1..=100).collect::<Vec<u64>>());
        assert_eq!(l["vorgaenge"].as_array().unwrap().len(), 100);
        assert_eq!(l["cursor"], 100);
        let (_, l) = schicke(&state, get("/api/vorgaenge?fuer=dashboard&seit=50")).await;
        assert_eq!(nummern(&l), (51..=100).collect::<Vec<u64>>());
    }
}
