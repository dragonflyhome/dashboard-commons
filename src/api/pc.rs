//! `/api/pc`: Messwerte, Systemanwendungen (Rechner-Teil), `data/pc.toml`
//! und Backup je Projekt. Der Weg ist frei, die Rechner-Werte sieht jeder
//! (`[anmeldung]`).
//!
//! `/api/pc/benutzer`: die Benutzer-Daten, die sonst der Bote unter dem Konto des
//! Konto liest (Ordnergrößen, winget, Autostart, Werkzeuge), und die
//! Schlüssel-Zähler. Der Weg ist geschützt (`sicherheit::geschuetzt`): Ohne
//! Windows-Anmeldung antwortet die Schleuse 401 mit `WWW-Authenticate:
//! Negotiate`, erst darauf schickt der Browser seinen Nachweis. Auf einem
//! freien Weg täte er das nie (Etappe 7b).
//!
//! `/api/pc/verlauf` (Etappe 2e): der Verlauf des PCs als Ereignisse
//! (`vertrag::verlauf`), frei wie `/api/pc`; mit Anmeldung zusätzlich
//! Starts, Systemmeldungen und die Vorgänge des Dashboards. Der Weg bleibt
//! für Werkzeuge (curl).
//!
//! `/api/pc/verlauf/voll` (Hotfix 3a-1): derselbe Verlauf, immer mit Starts,
//! Systemmeldungen und Vorgängen, geschützt wie `/api/pc/benutzer`. Die
//! Oberfläche liest ihn hier: Edge schickt Negotiate nur auf einer
//! Verbindung, die vorher 401 bekam. Auf dem freien Weg landete die Anfrage
//! oft auf einer neuen Verbindung ohne Anmeldung, dann fehlten die Starts.

use axum::extract::State;
use axum::http::Uri;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use chrono::{DateTime, FixedOffset, Local, NaiveDate};

use super::vertrag::{PcBenutzer, PcReport, ProjectBackup, SchluesselGrund, SchluesselStand, SysAppsBenutzer, SysAppsRechner};
use super::vorgaenge::{abfrage, angegeben};
use crate::anmeldung::Angemeldet;
use crate::app::AppState;
use crate::fehler::AppError;
use crate::pc::pcdata::{self, PcData};
use crate::speicher::aufgaben::Zeit;
use crate::speicher::{Speicher, pc_ereignisse, sicherung};
use crate::vertrag::abgeleitet;
use crate::vertrag::anstehend::{Meldung, MeldungsArt};
use crate::vertrag::api::{Vorgang, Zustand as VorgangsZustand};
use crate::vertrag::bedeutung::Bedeutung;
use crate::vertrag::verlauf::{self, Art, Ereignis, Genauigkeit, Quelle, Verlauf, VerlaufHinweis, VerlaufHinweisArt as HinweisArt};

/// `GET /api/pc` (frei). Profilpfade und der Benutzername sind verborgen
/// (`datenschutz`), etwa in `sysapps.errors` und `data_error`.
pub async fn bericht(State(state): State<AppState>) -> Json<serde_json::Value> {
    crate::datenschutz::als_json(&bericht_bauen(&state).await)
}

/// Der Bericht für `/api/pc`, noch mit allen Texten.
pub async fn bericht_bauen(state: &AppState) -> PcReport {
    let snapshot = state.snapshot.read().await.clone();
    // data/pc.toml und die Projekte aus dem ContentStore (Etappe 2d).
    let bestand = state.inhalt.bestand();
    let (data, data_error) = match &bestand.pc {
        Ok(d) => (d.clone(), None),
        Err(e) => (Default::default(), Some(e.clone())),
    };
    let sysapps = {
        let s = state.sysapps.read().await;
        SysAppsRechner {
            refreshed_at: s.refreshed_at,
            windows: s.windows.clone(),
            services_running: s.services_running.clone(),
            services_total: s.services_total,
            errors: s.lokal_fehler.clone(),
        }
    };
    // Entscheidung archiv-status: archivierte Projekte fehlen im Cockpit.
    let (projects, _) = bestand.liste_sichtbar();
    let project_backups = projects
        .iter()
        .filter_map(|p| {
            p.meta.backup.as_ref().map(|b| ProjectBackup {
                slug: p.slug.clone(),
                title: p.meta.title.clone(),
                chat: p.meta.chat.clone(),
                backup: b.clone(),
                bedeutung: Bedeutung::aus_wort_oder_neutral(b),
                backup_note: p.meta.backup_note.clone(),
            })
        })
        .collect();
    let backup_stand = data_error.is_none().then(|| data.backup.lage());
    PcReport { snapshot, sysapps, data, data_error, backup_stand, project_backups }
}

/// `GET /api/pc/benutzer` (geschützt). Auch hier ohne Profil und Namen
/// (`inventory.root`, `inventory.hinweis`, `autostart.command`, `winget_error`).
pub async fn benutzer(State(state): State<AppState>) -> Json<serde_json::Value> {
    crate::datenschutz::als_json(&benutzer_bauen(&state).await)
}

/// Die Benutzer-Daten für `/api/pc/benutzer`, noch mit allen Texten.
pub async fn benutzer_bauen(state: &AppState) -> PcBenutzer {
    let inventory = state.inventory.read().await.clone();
    let schluessel = schluessel_stand(state).await;
    let s = state.sysapps.read().await;
    let (bote, stand) = if state.bote.is_some() {
        let fehlt = s.benutzer_fehler.is_some() || s.benutzer_stand.is_none() || schluessel.grund == Some(SchluesselGrund::BoteFehlt);
        (if fehlt { "bote-fehlt" } else { "da" }, s.benutzer_stand)
    } else {
        // Ohne Boten liest der Dienst selbst, zusammen mit dem Rechner-Teil.
        ("da", s.refreshed_at)
    };
    let sysapps = SysAppsBenutzer { autostart: s.autostart.clone(), tools: s.tools.clone(), winget: s.winget.clone(), winget_error: s.winget_error.clone() };
    PcBenutzer { inventory, sysapps, schluessel, bote, stand }
}

/// Stand der Schlüssel. Mit Boten über ihn; antwortet er nicht (getrennt
/// oder ausgelastet), ist der Stand `unbekannt` mit `bote-fehlt`. Lässt sich
/// die Liste nicht lesen, `unbekannt` mit `liste-unlesbar`.
async fn schluessel_stand(state: &AppState) -> SchluesselStand {
    match super::schluessel::ansichten(state).await {
        Ok((v, hinweis)) => schluessel_stand_aus(&v, hinweis.is_some()),
        Err(e) => {
            tracing::warn!("Schlüssel-Liste nicht lesbar: {e}");
            SchluesselStand::unbekannt(SchluesselGrund::ListeUnlesbar)
        }
    }
}

/// Der Stand aus den Ansichten: antwortet der Bote nicht, `bote-fehlt`;
/// meldet er für einen Schlüssel einen Fehler, `tresor-fehler` (Etappe 3a+:
/// unbekannt, nie als „fehlt“ gezählt); sonst die Zahlen.
pub(crate) fn schluessel_stand_aus(v: &[crate::schluessel::KeyView], bote_fehlt: bool) -> SchluesselStand {
    use crate::vertrag::api::TresorStand;
    if bote_fehlt {
        return SchluesselStand::unbekannt(SchluesselGrund::BoteFehlt);
    }
    if v.iter().any(|k| k.stand() == TresorStand::Unbekannt) {
        return SchluesselStand::unbekannt(SchluesselGrund::TresorFehler);
    }
    let gespeichert = v.iter().filter(|k| k.stand() == TresorStand::Gespeichert).count();
    SchluesselStand::aus_zahlen(gespeichert, v.len() - gespeichert)
}

// ----- Verlauf (Etappe 2e) -----

/// `GET /api/pc/verlauf?seit=…&bis=…`: der Verlauf des PCs aus allen
/// Quellen, ohne zugesagte Reihenfolge. `seit` und `bis` als JJJJ-MM-TT
/// (`seit` ab Tagesanfang, `bis` bis Tagesende) oder RFC 3339; ohne Angabe
/// die letzten 90 Tage bis jetzt. Falsche Angabe oder mehr als 400 Tage: 422 mit Satz.
///
/// Frei lesbar wie `/api/pc` (Einträge von Hand, Updates, Installation,
/// Sicherungen); mit Anmeldung zusätzlich die Starts (sie sind ein Profil der
/// Anwesenheit, Regel aus A1), Systemmeldungen und die Vorgänge des Dashboards
/// (Slug `dashboard`). Das Archiv der Windows-Ereignisse wird gelesen, wenn
/// `seit` vor der Grenze von 90 Tagen liegt. Beim Standard ist das immer so:
/// `seit` fällt dann auf den Tagesanfang vor 90 Tagen, die Grenze auf jetzt
/// minus 90 Tage; das Archiv trägt also die Stunden dazwischen bei.
pub async fn verlauf(State(state): State<AppState>, uri: Uri, angemeldet: Option<Extension<Angemeldet>>) -> Result<Json<serde_json::Value>, AppError> {
    verlauf_bauen(&state, &uri, angemeldet.is_some()).await.map(|v| crate::datenschutz::als_json(&v))
}

/// `GET /api/pc/verlauf/voll?seit=…&bis=…`: wie [`verlauf`], aber geschützt
/// (`sicherheit::geschuetzt`), darum immer mit Starts, Systemmeldungen und
/// Vorgängen. Ohne Anmeldung antwortet schon die Schleuse 401 mit
/// `WWW-Authenticate: Negotiate`; käme doch eine Anfrage ohne durch, dasselbe hier.
pub async fn verlauf_voll(State(state): State<AppState>, uri: Uri, angemeldet: Option<Extension<Angemeldet>>) -> Response {
    if angemeldet.is_none() {
        return super::anmeldung::bitte_anmelden(None);
    }
    match verlauf_bauen(&state, &uri, true).await {
        // Meldungstexte und Hinweise ohne Profil und Namen (`datenschutz`).
        Ok(v) => crate::datenschutz::als_json(&v).into_response(),
        Err(e) => e.into_response(),
    }
}

/// Der Verlauf für beide Wege; `mit_anmeldung` entscheidet über Starts,
/// Systemmeldungen und Vorgänge.
async fn verlauf_bauen(state: &AppState, uri: &Uri, mit_anmeldung: bool) -> Result<Verlauf, AppError> {
    let jetzt = Local::now().fixed_offset();
    let (seit, bis) = zeitraum(uri, &jetzt)?;
    let heute = abgeleitet::heute();
    let mut ereignisse = Vec::new();
    let mut hinweise = Vec::new();
    match &state.inhalt.bestand().pc {
        Ok(d) => ereignisse.extend(aus_pc_toml(d, heute, &mut hinweise)),
        Err(satz) => hinweise.push(VerlaufHinweis::neu(HinweisArt::HandFehlt, format!("Einträge von Hand fehlen: {satz}"))),
    }
    for f in &state.sysapps.read().await.lokal_fehler {
        hinweise.push(VerlaufHinweis::neu(HinweisArt::SammlerUnvollstaendig, format!("Windows-Ereignisse, letzter Lauf des Sammlers unvollständig: {f}")));
    }
    let mit_archiv = seit < pc_ereignisse::archiv_grenze(&jetzt);
    match state.speicher.clone() {
        None => hinweise.push(VerlaufHinweis::neu(HinweisArt::OhneSpeicher, SATZ_OHNE_SPEICHER)),
        Some(s) => {
            let (pc, meldungen, vorgaenge) = Speicher::im_hintergrund(s, move |s| {
                let pc = s.pc_ereignisse_lesen(mit_archiv)?;
                let meldungen = if mit_anmeldung { s.meldungen_mit_archiv()? } else { Vec::new() };
                let vorgaenge = if mit_anmeldung { s.vorgaenge_alle()? } else { Vec::new() };
                Ok((pc, meldungen, vorgaenge))
            })
            .await
            .map_err(|e| {
                tracing::error!("Verlauf: Datenspeicher: {e} ({})", e.einzelheiten());
                AppError::intern(format!("{e} Einzelheiten stehen im Protokoll."))
            })?;
            if mit_archiv && let Some(ab) = windows_ereignisse_ab(&pc).filter(|ab| seit < *ab) {
                hinweise.push(VerlaufHinweis::neu(
                    HinweisArt::BeginntAb,
                    format!(
                        "Starts und Update-Ereignisse gibt es erst ab {} (90 Tage vor dem ersten Lauf des Sammlers); Hotfixes sind vollständig.",
                        ab.format("%Y-%m-%d")
                    ),
                ));
            }
            // Starts nur mit Anmeldung: 90 Tage Startzeiten zeigen, wann jemand da war.
            ereignisse.extend(pc.into_iter().map(|a| a.ereignis).filter(|e| mit_anmeldung || e.quelle != Quelle::Start));
            ereignisse.extend(meldungen.iter().filter(|m| m.system).map(aus_meldung));
            // Entscheidung archiv-status: kein Merge-Antrag für ein archiviertes Projekt.
            let ohne = state.archivierte();
            ereignisse.extend(vorgaenge.iter().filter(|v| v.slug == VORGAENGE_SLUG && !crate::speicher::vorgaenge::ist_still(v, &ohne)).map(aus_vorgang));
        }
    }
    // Wo der offene Speicher sichert (im Dienst `pfade.sicherung`), sonst der Ordner aus den Einstellungen.
    let ordner = match &state.speicher {
        Some(s) => s.sicherungs_ordner().to_path_buf(),
        None => state.sicherungs_ordner(),
    };
    match tokio::task::spawn_blocking(move || sicherung::sicherungen_im_ordner(&ordner)).await {
        Ok(Ok(liste)) => ereignisse.extend(liste.iter().map(aus_sicherung)),
        andere => {
            // Ohne Pfad im Satz: er nennt den Windows-Benutzer, der Weg ist frei.
            let grund = match andere {
                Ok(Err(e)) => e.to_string(),
                Err(e) => e.to_string(),
                Ok(Ok(_)) => String::new(),
            };
            tracing::warn!("Verlauf: Sicherungsordner nicht lesbar: {grund}");
            hinweise.push(VerlaufHinweis::neu(HinweisArt::SicherungenFehlen, "Die Sicherungen fehlen: Der Sicherungsordner lässt sich nicht lesen."));
        }
    }
    if !mit_anmeldung {
        hinweise.push(VerlaufHinweis::neu(HinweisArt::OhneAnmeldung, SATZ_OHNE_ANMELDUNG));
    }
    ereignisse.retain(|e| verlauf::im_zeitraum(e, &seit, &bis));
    Ok(Verlauf { ereignisse, seit: verlauf::zeit_text(&seit), bis: verlauf::zeit_text(&bis), hinweise })
}

/// Hinweis ohne Anmeldung.
pub const SATZ_OHNE_ANMELDUNG: &str = "Ohne Anmeldung fehlen die Starts, Meldungen und Vorgänge.";
/// Nur die Vorgänge dieses Chats gehören in den Verlauf des PCs.
pub const VORGAENGE_SLUG: &str = "dashboard";
/// Hinweis ohne Datenspeicher.
pub const SATZ_OHNE_SPEICHER: &str =
    "Der Datenspeicher ist nicht offen: Updates, Starts und die Installation von Windows fehlen, ebenso Meldungen und Vorgänge.";

/// Standard für `seit`: so viele Tage vor heute (Tagesanfang).
pub const VERLAUF_TAGE: i64 = 90;
/// Etappe 3a-1: höchstens so viele Tage auf einmal (länger: 422 mit Satz).
/// Eine Abfrage bleibt so klein, auch wenn das Archiv über Jahre wächst.
pub const VERLAUF_HOECHSTENS_TAGE: i64 = 400;

/// `seit` und `bis` aus der Abfrage (siehe [`verlauf`]).
fn zeitraum(uri: &Uri, jetzt: &DateTime<FixedOffset>) -> Result<(DateTime<FixedOffset>, DateTime<FixedOffset>), AppError> {
    let seit = match angegeben(abfrage(uri, "seit")) {
        Some(t) => zeit_aus_abfrage("seit", &t, false)?,
        None => verlauf::tagesanfang(jetzt.date_naive() - chrono::Duration::days(VERLAUF_TAGE)).fixed_offset(),
    };
    let bis = match angegeben(abfrage(uri, "bis")) {
        Some(t) => zeit_aus_abfrage("bis", &t, true)?,
        None => *jetzt,
    };
    if seit > bis {
        return Err(AppError::unlesbar("„seit“ liegt nach „bis“. Bitte einen Zeitraum angeben, der vorwärts läuft."));
    }
    if bis - seit > chrono::Duration::days(VERLAUF_HOECHSTENS_TAGE) {
        return Err(AppError::unlesbar(format!(
            "Der Zeitraum ist zu lang: höchstens {VERLAUF_HOECHSTENS_TAGE} Tage auf einmal. Bitte „seit“ und „bis“ enger wählen, etwa seit=2026-01-01&bis=2026-12-31."
        )));
    }
    Ok((seit, bis))
}

/// Ein Datum JJJJ-MM-TT (Tagesanfang, bei `ende` Tagesende) oder RFC 3339.
fn zeit_aus_abfrage(name: &str, text: &str, ende: bool) -> Result<DateTime<FixedOffset>, AppError> {
    if let Ok(t) = DateTime::parse_from_rfc3339(text) {
        return Ok(t);
    }
    let tag = NaiveDate::parse_from_str(text, "%Y-%m-%d").ok().filter(|_| text.len() == 10);
    match tag {
        Some(t) if !ende => Ok(verlauf::tagesanfang(t).fixed_offset()),
        Some(t) => match t.succ_opt() {
            Some(n) => Ok(verlauf::tagesanfang(n).fixed_offset() - chrono::Duration::seconds(1)),
            None => Ok(verlauf::tagesanfang(t).fixed_offset()),
        },
        None => Err(AppError::unlesbar(format!(
            "Bei „{name}“ steht „{text}“. Erlaubt ist ein Datum JJJJ-MM-TT oder eine Zeit nach RFC 3339, etwa {name}=2026-10-01 oder {name}=2026-10-01T08:00:00+02:00."
        ))),
    }
}

/// Ab wann es Starts und Update-Ereignisse gibt: 90 Tage vor dem ersten Lauf
/// des Sammlers (so weit liest er die Ereignisanzeige). `None` ohne Einträge.
fn windows_ereignisse_ab(pc: &[pc_ereignisse::PcEreignisEintrag]) -> Option<DateTime<FixedOffset>> {
    pc.iter().filter_map(|a| DateTime::parse_from_rfc3339(&a.zuerst_gesehen).ok()).min().map(|t| t - chrono::Duration::days(pc_ereignisse::ARCHIV_NACH_TAGEN))
}

/// `data/pc.toml` `[[history]]` als Ereignisse (Quelle `hand`). Einträge ohne
/// `kennung` (alte Form, bis Schritt 6) bekommen eine aus Datum und Titel;
/// dazu ein Hinweis, ebenso für Einträge ohne lesbares Datum und doppelte Kennungen.
pub fn aus_pc_toml(d: &PcData, heute: NaiveDate, hinweise: &mut Vec<VerlaufHinweis>) -> Vec<Ereignis> {
    let mut liste = Vec::new();
    let mut ohne_kennung = 0;
    for h in &d.history {
        if h.kennung.as_deref().is_none_or(|k| k.trim().is_empty()) {
            ohne_kennung += 1;
        }
        let hand = verlauf::Hand { kennung: h.kennung.as_deref(), datum: &h.date, titel: &h.title, text: Some(&h.note), art: None, farbe: None };
        match hand.ereignis(heute) {
            Some(e) => liste.push(e),
            None => hinweise.push(VerlaufHinweis::neu(
                HinweisArt::DatumUnlesbar,
                format!("{}: Der Eintrag „{}“ hat kein lesbares Datum („{}“) und fehlt im Verlauf.", pcdata::DATEI, h.title, h.date),
            )),
        }
    }
    if ohne_kennung > 0 {
        hinweise.push(VerlaufHinweis::neu(
            HinweisArt::OhneKennung,
            format!(
                "{}: {ohne_kennung} von {} Einträgen in [[history]] ohne `kennung` (bis Schritt 6 erlaubt); ihre Kennung ist aus Datum und Titel gebildet und ändert sich mit ihnen.",
                pcdata::DATEI,
                d.history.len()
            ),
        ));
    }
    if verlauf::eindeutig_machen(&mut liste) > 0 {
        hinweise.push(VerlaufHinweis::neu(
            HinweisArt::KennungDoppelt,
            format!(
                "{}: Eine `kennung` in [[history]] steht doppelt da; der Verlauf gibt jeder Wiederholung die nächste freie Endung (-2, -3, …).",
                pcdata::DATEI
            ),
        ));
    }
    liste
}

/// Eine Systemmeldung als Ereignis: `zeit` ist `erstellt`, die Art kommt aus
/// dem Zustand. Fehler und Fristen: offen `problem`, behoben `erledigt`;
/// Hinweis und fertig: `routine` (so bestätigt).
pub fn aus_meldung(m: &Meldung) -> Ereignis {
    let art = match m.art {
        MeldungsArt::Fehler | MeldungsArt::Frist if m.behoben.is_none() => Art::Problem,
        MeldungsArt::Fehler | MeldungsArt::Frist => Art::Erledigt,
        MeldungsArt::Info | MeldungsArt::Fertig => Art::Routine,
    };
    Ereignis {
        kennung: format!("meldung:{}", m.id),
        zeit: m.erstellt.clone(),
        genauigkeit: Genauigkeit::Minute,
        bis: None,
        titel: m.titel.clone(),
        text: m.text.clone(),
        quelle: Quelle::Meldung,
        art,
        verweis: Some("/anstehend".to_string()),
    }
}

/// Ein Vorgang als Ereignis: `zeit` ist `erstellt`; erledigt `erledigt`,
/// sonst (offen, zugestellt, übernommen) `begonnen`. Einen Zustand
/// „zurückgezogen“ (dann `abgebrochen`) haben Vorgänge heute nicht.
pub fn aus_vorgang(v: &Vorgang) -> Ereignis {
    Ereignis {
        kennung: format!("vorgang:{}", v.id),
        zeit: v.erstellt.clone(),
        genauigkeit: Genauigkeit::Minute,
        bis: None,
        titel: v.titel.clone(),
        text: v.notiz.clone(),
        quelle: Quelle::Vorgang,
        art: if v.zustand == VorgangsZustand::Erledigt { Art::Erledigt } else { Art::Begonnen },
        verweis: Some("/vorgaenge".to_string()),
    }
}

/// Eine Sicherung als Ereignis: tägliche `routine`, vor einem Umbau `erledigt`.
pub fn aus_sicherung(s: &sicherung::SicherungsDatei) -> Ereignis {
    let (titel, art) = match &s.vor {
        None => ("Tägliche Sicherung des Datenspeichers".to_string(), Art::Routine),
        Some(z) => (format!("Sicherung des Datenspeichers vor {z}"), Art::Erledigt),
    };
    Ereignis {
        kennung: format!("sicherung:{}", s.name),
        zeit: verlauf::zeit_text(&s.zeit),
        genauigkeit: Genauigkeit::Minute,
        bis: None,
        titel,
        text: Some(s.name.clone()),
        quelle: Quelle::Sicherung,
        art,
        verweis: Some("/daten".to_string()),
    }
}

/// Ein Lauf des Archivs der Windows-Ereignisse (aus dem Minutentakt in
/// `api::aufgaben::spawn_ablauf`): älter als 90 Tage ins Archiv.
pub async fn archiv_lauf(state: &AppState, jetzt: Zeit) -> usize {
    let Some(speicher) = state.speicher.clone() else { return 0 };
    match Speicher::im_hintergrund(speicher, move |s| s.pc_ereignisse_archivieren(&jetzt)).await {
        Ok(n) => {
            if n > 0 {
                tracing::info!("{n} Windows-Ereignis(se) ins Archiv verschoben (älter als {} Tage)", pc_ereignisse::ARCHIV_NACH_TAGEN);
            }
            n
        }
        Err(e) => {
            tracing::warn!("Archiv der Windows-Ereignisse: {e} ({})", e.einzelheiten());
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use chrono::{Duration, Timelike};
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    use super::*;
    use crate::app::{beispiel_fuer_tests, router, router_angemeldet};
    use crate::speicher::meldungen::{NeueMeldung, Wiederholung};
    use crate::vertrag::api::VorgangsArt;

    async fn hole(app: &Router, pfad: &str) -> (StatusCode, Value) {
        let req = Request::get(pfad).header("host", "127.0.0.1:8787").body(Body::empty()).unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into())))
    }

    fn kennungen(v: &Value) -> Vec<String> {
        let mut k: Vec<String> = v["ereignisse"].as_array().unwrap().iter().map(|e| e["kennung"].as_str().unwrap().to_string()).collect();
        k.sort();
        k
    }

    /// Ein Zustand mit Daten in jeder Quelle: zwei Starts (vor 2 und vor 100
    /// Tagen, der zweite im Archiv), eine Systemmeldung, eine Meldung eines
    /// Projekts, ein Vorgang, eine Sicherung; dazu die drei Einträge von Hand
    /// aus tests/beispiel/data/pc.toml.
    fn zustand_mit_daten() -> (AppState, DateTime<FixedOffset>) {
        let state = AppState::fuer_tests(&beispiel_fuer_tests());
        let s = state.speicher.clone().unwrap();
        let jetzt = crate::speicher::aufgaben::jetzt().with_nanosecond(0).unwrap();
        let starts = [jetzt - Duration::days(2), jetzt - Duration::days(100)];
        s.pc_ereignisse_einspielen(&crate::pc::ereignisse::starts(&starts), &jetzt).unwrap();
        let meldung = |projekt: &str, system: bool| NeueMeldung {
            projekt: projekt.into(),
            art: MeldungsArt::Fehler,
            schluessel: format!("{projekt}/bote-fehlt"),
            titel: "Der Bote fehlt.".into(),
            text: None,
            verweis: None,
            system,
            geschuetzt: false,
            von: "server".into(),
        };
        s.meldung_melden(meldung("dashboard", true), Wiederholung::Zustand, &jetzt).unwrap();
        s.meldung_melden(meldung("schaufenster", false), Wiederholung::Zustand, &jetzt).unwrap();
        s.vorgang_anlegen(VorgangsArt::Abnahme, "dashboard", "Abnahme Verlauf", None, "browser").unwrap();
        s.vorgang_anlegen(VorgangsArt::Frage, "schaufenster", "Frage an den Inhaber", None, "chat").unwrap();
        s.sichern().unwrap();
        (state, jetzt)
    }

    /// Ohne Anmeldung: frei lesbar, Einträge von Hand und Sicherung, aber keine
    /// Starts (Anwesenheit), Meldungen und Vorgänge (mit Hinweis). Standard: 90 Tage.
    #[tokio::test]
    async fn verlauf_ohne_anmeldung_ohne_meldungen_und_vorgaenge() {
        let (state, jetzt) = zustand_mit_daten();
        let app = router(state);
        let (status, v) = hole(&app, "/api/pc/verlauf").await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let k = kennungen(&v);
        assert!(!k.iter().any(|k| k.starts_with("start:") || k.starts_with("meldung:") || k.starts_with("vorgang:")), "{k:?}");
        assert!(k.iter().any(|k| k.starts_with("sicherung:dashboard-")), "{k:?}");
        assert!(v["hinweise"].as_array().unwrap().iter().any(|h| h["art"] == "ohne-anmeldung" && h["text"] == SATZ_OHNE_ANMELDUNG), "{v}");
        let seit = DateTime::parse_from_rfc3339(v["seit"].as_str().unwrap()).unwrap();
        assert_eq!(seit.date_naive(), jetzt.date_naive() - Duration::days(VERLAUF_TAGE));
        assert_eq!((seit.hour(), seit.minute()), (0, 0));
        assert!(DateTime::parse_from_rfc3339(v["bis"].as_str().unwrap()).is_ok());
        let sicherung = v["ereignisse"].as_array().unwrap().iter().find(|e| e["quelle"] == "sicherung").unwrap();
        assert_eq!((sicherung["art"].as_str(), sicherung["verweis"].as_str()), (Some("routine"), Some("/daten")));
        // Einträge von Hand (Beispiel über vier Tage).
        let (_, v) = hole(&app, "/api/pc/verlauf?seit=2026-10-01&bis=2026-10-31").await;
        let k = kennungen(&v);
        for h in ["hand:erster", "hand:zweiter", "hand:dritter"] {
            assert!(k.contains(&h.to_string()), "{k:?}");
        }
        let e = v["ereignisse"].as_array().unwrap().iter().find(|e| e["kennung"] == "hand:zweiter").unwrap();
        assert_eq!((e["quelle"].as_str(), e["art"].as_str(), e["genauigkeit"].as_str()), (Some("hand"), Some("erledigt"), Some("tag")));
        assert_eq!(e["text"], "b");
    }

    /// Mit Anmeldung: dazu die Starts, die Systemmeldung (offen: problem) und
    /// der Vorgang des Dashboards (offen: begonnen); die Meldung und der Vorgang
    /// eines anderen Projekts gehören nicht in den PC-Verlauf.
    #[tokio::test]
    async fn verlauf_mit_anmeldung_mit_meldungen_und_vorgaengen() {
        let (state, jetzt) = zustand_mit_daten();
        let app = router_angemeldet(state);
        let (status, v) = hole(&app, "/api/pc/verlauf").await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let start = format!("start:{}", verlauf::zeit_text(&(jetzt - Duration::days(2))));
        assert!(kennungen(&v).contains(&start), "{v}");
        let e = v["ereignisse"].as_array().unwrap();
        assert_eq!(e.iter().filter(|e| e["quelle"] == "vorgang").count(), 1, "nur der Vorgang des Dashboards: {v}");
        let meldungen: Vec<&Value> = e.iter().filter(|e| e["quelle"] == "meldung").collect();
        assert_eq!(meldungen.len(), 1, "nur die Systemmeldung: {v}");
        assert_eq!((meldungen[0]["art"].as_str(), meldungen[0]["verweis"].as_str()), (Some("problem"), Some("/anstehend")));
        let vorgang = e.iter().find(|e| e["quelle"] == "vorgang").expect("Vorgang");
        assert_eq!(vorgang["art"], "begonnen");
        assert!(vorgang["kennung"].as_str().unwrap().starts_with("vorgang:v-"));
        assert!(!v["hinweise"].as_array().unwrap().iter().any(|h| h["art"] == "ohne-anmeldung" && h["text"] == SATZ_OHNE_ANMELDUNG), "{v}");
    }

    /// Hotfix 3a-1: `/api/pc/verlauf/voll` ist geschützt. Ohne Anmeldung 401 mit
    /// `WWW-Authenticate: Negotiate` (erst darauf schickt Edge den Nachweis),
    /// angemeldet derselbe Verlauf mit Starts, Meldung und Vorgang, ohne den
    /// Hinweis; `seit` und `bis` gelten wie auf dem freien Weg.
    #[tokio::test]
    async fn verlauf_voll_nur_mit_anmeldung() {
        let (state, jetzt) = zustand_mit_daten();
        let req = Request::get("/api/pc/verlauf/voll").header("host", "127.0.0.1:8787").body(Body::empty()).unwrap();
        let res = router(state.clone()).oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(res.headers().get("www-authenticate").and_then(|v| v.to_str().ok()), Some("Negotiate"));
        let app = router_angemeldet(state);
        let (status, v) = hole(&app, "/api/pc/verlauf/voll").await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let start = format!("start:{}", verlauf::zeit_text(&(jetzt - Duration::days(2))));
        let k = kennungen(&v);
        assert!(k.contains(&start), "{k:?}");
        assert!(k.iter().any(|k| k.starts_with("meldung:")) && k.iter().any(|k| k.starts_with("vorgang:")), "{k:?}");
        assert!(!v["hinweise"].as_array().unwrap().iter().any(|h| h["art"] == "ohne-anmeldung" && h["text"] == SATZ_OHNE_ANMELDUNG), "{v}");
        let (_, frei) = hole(&app, "/api/pc/verlauf").await;
        assert_eq!(kennungen(&frei), k, "angemeldet zeigen beide Wege dasselbe");
        let tag = |d: i64| (jetzt - Duration::days(d)).format("%Y-%m-%d").to_string();
        let (_, v) = hole(&app, &format!("/api/pc/verlauf/voll?seit={}&bis={}", tag(120), tag(50))).await;
        assert!(!kennungen(&v).contains(&start), "{v}");
        let (status, _) = hole(&app, "/api/pc/verlauf/voll?seit=gestern").await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    /// `seit` älter als 90 Tage: auch das Archiv; `seit` und `bis` begrenzen
    /// (auch als RFC 3339); falsche Angaben und `seit` nach `bis`: 422 mit Satz.
    #[tokio::test]
    async fn verlauf_zeitraum_archiv_und_falsche_angaben() {
        let (state, jetzt) = zustand_mit_daten();
        let app = router_angemeldet(state);
        let alt = format!("start:{}", verlauf::zeit_text(&(jetzt - Duration::days(100))));
        let neu = format!("start:{}", verlauf::zeit_text(&(jetzt - Duration::days(2))));
        let tag = |d: i64| (jetzt - Duration::days(d)).format("%Y-%m-%d").to_string();
        let (_, v) = hole(&app, &format!("/api/pc/verlauf?seit={}", tag(120))).await;
        let k = kennungen(&v);
        assert!(k.contains(&alt) && k.contains(&neu), "{k:?}");
        // Nur bis vor 50 Tagen: der neue Start fehlt.
        let (_, v) = hole(&app, &format!("/api/pc/verlauf?seit={}&bis={}", tag(120), tag(50))).await;
        let k = kennungen(&v);
        assert!(k.contains(&alt) && !k.contains(&neu), "{k:?}");
        // bis = JJJJ-MM-TT schließt den ganzen Tag ein, RFC 3339 gilt genau.
        let (_, v) = hole(&app, &format!("/api/pc/verlauf?seit={}&bis={}", tag(3), tag(2))).await;
        assert!(kennungen(&v).contains(&neu), "{v}");
        let genau = verlauf::zeit_text(&(jetzt - Duration::days(2) - Duration::seconds(1))).replace('+', "%2B");
        let (_, v) = hole(&app, &format!("/api/pc/verlauf?seit={}&bis={genau}", tag(3))).await;
        assert!(!kennungen(&v).contains(&neu), "eine Sekunde davor: {v}");
        for (abfrage, stueck) in [
            ("seit=gestern", "Bei „seit“ steht „gestern“. Erlaubt ist ein Datum JJJJ-MM-TT oder eine Zeit nach RFC 3339"),
            ("bis=2026-13-01", "Bei „bis“ steht „2026-13-01“."),
            ("seit=2026-10-30&bis=2026-10-01", "„seit“ liegt nach „bis“."),
            ("seit=2025-01-01&bis=2026-10-30", "Der Zeitraum ist zu lang: höchstens 400 Tage auf einmal."),
        ] {
            let (status, v) = hole(&app, &format!("/api/pc/verlauf?{abfrage}")).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{abfrage}: {v}");
            assert!(v["fehler"].as_str().unwrap().starts_with(stueck), "{abfrage}: {v}");
        }
    }

    /// Ohne Datenspeicher: Einträge von Hand trotzdem, dazu der Hinweis.
    #[tokio::test]
    async fn verlauf_ohne_speicher() {
        let state = AppState::fuer_tests_ohne_speicher(&beispiel_fuer_tests(), "kaputt");
        let (status, v) = hole(&router(state), "/api/pc/verlauf?seit=2026-10-01&bis=2026-10-31").await;
        assert_eq!(status, StatusCode::OK, "{v}");
        // Sicherungen kommen aus dem Ordner neben der (nie angelegten) Datei im Temp-Ordner; hier nur die von Hand.
        let hand: Vec<String> = kennungen(&v).into_iter().filter(|k| !k.starts_with("sicherung:")).collect();
        assert_eq!(hand, ["hand:dritter", "hand:erster", "hand:zweiter"]);
        assert!(v["hinweise"].as_array().unwrap().iter().any(|h| h["art"] == "ohne-speicher" && h["text"] == SATZ_OHNE_SPEICHER), "{v}");
    }

    #[test]
    fn pc_toml_ohne_kennung_und_doppelt() {
        let d = pcdata::lesen(
            "[[history]]\ndate = \"2026-10-12\"\ntitle = \"Netzteil getauscht\"\n\n[[history]]\nkennung = \"a\"\ndate = \"2026-10\"\ntitle = \"A\"\n\n\
             [[history]]\nkennung = \"a\"\ndate = \"irgendwann\"\ntitle = \"B\"\n\n[[history]]\nkennung = \"a\"\ndate = \"2026\"\ntitle = \"C\"\n",
        )
        .unwrap();
        let mut h = Vec::new();
        let e = aus_pc_toml(&d, NaiveDate::from_ymd_opt(2026, 10, 30).unwrap(), &mut h);
        let k: Vec<&str> = e.iter().map(|e| e.kennung.as_str()).collect();
        assert_eq!(k, ["hand:2026-10-12:netzteil-getauscht", "hand:a", "hand:a-2"]);
        assert_eq!(h.len(), 3, "{h:?}");
        assert!(h[0].text.starts_with("data/pc.toml: Der Eintrag „B“ hat kein lesbares Datum"), "{h:?}");
        assert!(h[1].text.starts_with("data/pc.toml: 1 von 4 Einträgen in [[history]] ohne `kennung`"), "{h:?}");
        // Etappe 3a+: der Satz verspricht nicht mehr fest „-2, -3“ (eindeutig_machen nimmt die nächste freie Endung).
        assert!(h[2].text.contains("nächste freie Endung") && !h[2].text.contains("hängt -2, -3 an"), "{h:?}");
        let arten: Vec<HinweisArt> = h.iter().map(|h| h.art).collect();
        assert_eq!(arten, [HinweisArt::DatumUnlesbar, HinweisArt::OhneKennung, HinweisArt::KennungDoppelt]);
        assert_eq!(serde_json::to_value(&h[0]).unwrap()["art"], "datum-unlesbar");
    }

    #[test]
    fn meldung_und_sicherung_als_ereignis() {
        let m = |art, behoben: Option<&str>| Meldung {
            id: "m-20261030-0001".into(),
            projekt: "dashboard".into(),
            art,
            schluessel: "dashboard/x".into(),
            titel: "T".into(),
            text: Some("ganz".into()),
            verweis: None,
            system: true,
            geschuetzt: false,
            von: "server".into(),
            erstellt: "2026-10-30T08:00:00+01:00".into(),
            zuletzt: "2026-10-30T09:00:00+01:00".into(),
            anzahl: 2,
            gelesen: None,
            behoben: behoben.map(str::to_string),
            archiviert: None,
            zusammengefasst: false,
        };
        let art = |a, b| aus_meldung(&m(a, b)).art;
        assert_eq!(art(MeldungsArt::Fehler, None), Art::Problem);
        assert_eq!(art(MeldungsArt::Fehler, Some("2026-10-30T10:00:00+01:00")), Art::Erledigt);
        assert_eq!(art(MeldungsArt::Frist, None), Art::Problem);
        assert_eq!(art(MeldungsArt::Info, None), Art::Routine);
        assert_eq!(art(MeldungsArt::Fertig, None), Art::Routine);
        let e = aus_meldung(&m(MeldungsArt::Info, None));
        assert_eq!((e.kennung.as_str(), e.zeit.as_str(), e.text.as_deref()), ("meldung:m-20261030-0001", "2026-10-30T08:00:00+01:00", Some("ganz")));
        let tag = NaiveDate::from_ymd_opt(2026, 10, 30).unwrap();
        let s = |vor: Option<&str>| sicherung::SicherungsDatei {
            name: "dashboard-2026-10-30-vor-schema-8.redb".into(),
            zeit: verlauf::tagesanfang(tag),
            vor: vor.map(str::to_string),
        };
        let e = aus_sicherung(&s(Some("schema-8")));
        assert_eq!((e.kennung.as_str(), e.art, e.quelle), ("sicherung:dashboard-2026-10-30-vor-schema-8.redb", Art::Erledigt, Quelle::Sicherung));
        assert_eq!(aus_sicherung(&s(None)).art, Art::Routine);
    }
}
