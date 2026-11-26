//! Kalender-Schnittstelle (Stufe 3): Tageszeiten und Feste für benannte Orte
//! (`GET`, in der Adresse nur Kennungen) und für abgefragte Orte (`POST`, die
//! Koordinaten im Rumpf). Alles nur angemeldet (Schleuse und Handler).
//!
//! Ein abgefragter Ort wird nicht gespeichert und nicht protokolliert: Die
//! Handler schreiben nichts ins Protokoll, was Lage oder Zeiten nennt, und
//! nichts in Dateien oder Zwischenspeicher.

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, Uri};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use chrono::{Datelike, Duration, NaiveDate};

use super::vorgaenge::{abfrage, angegeben, json_kopf_pruefen};
use crate::anmeldung::Angemeldet;
use crate::anstehend::Inhalt;
use crate::app::AppState;
use crate::fehler::AppError;
use crate::kalender::mond::{Lage, Ort};
use crate::kalender::orte::{Benannt, Orte, ort_satz};
use crate::kalender::tag::{self, Sonnenzeit};
use crate::kalender::{Anzeige, Katalog, Regel};
use crate::vertrag::daten::{Datei, datei_lesen};
use crate::vertrag::kalender::{
    KalenderAbfrage, KalenderAbfrageAntwort, KalenderFest, KalenderFeste, KalenderMerken, KalenderOrt, KalenderOrte, KalenderTag, KalenderZonen, Tageszeile,
};
use crate::vertrag::standorte::{Standorte, standorte_eindeutig, versatz_nach_laenge};

/// Höchstens so viele Tage umfasst ein Zeitraum für Feste.
pub const TAGE_HOECHSTENS: i64 = 400;

/// Ohne benannten Ort (Laientext).
pub const SATZ_OHNE_ORT: &str =
    "Es ist noch kein Ort eingetragen. Darum gibt es keine Tageszeiten, und die Feste gelten für die Nordhalbkugel in der Ortszeit von Windows.";

/// Die Jahre, für die die Kalender-Schnittstelle rechnet (ΔT des Rechenkerns von 1800 bis 2150).
const JAHRE: std::ops::RangeInclusive<i32> = 1800..=2150;

fn datum(text: &str, name: &str) -> Result<NaiveDate, AppError> {
    NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .ok()
        .filter(|d| JAHRE.contains(&d.year()))
        .ok_or_else(|| AppError::unlesbar(format!("„{name}“ ist ein Datum JJJJ-MM-TT zwischen 1800 und 2150, etwa 2027-06-21.")))
}

fn text(d: NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

/// Die aktiven Kalender: alle, die in einer `fristen.toml` unter `kalender` stehen (in Reihenfolge, einmal).
fn aktive(inhalt: &Inhalt) -> Vec<String> {
    let mut aus: Vec<String> = Vec::new();
    for k in inhalt.fristen.iter().flat_map(|d| d.inhalt.inhalte().into_iter().flat_map(|t| t.kalender.iter())) {
        if !aus.contains(k) {
            aus.push(k.clone());
        }
    }
    aus
}

/// Die Tageszeiten am Tag `d` als Zeilen; `wo`: „ · Ort X · Zone“.
fn zeilen(ort: &Ort, z: &tag::Tageszeiten, d: NaiveDate, wo: &str) -> (Vec<Tageszeile>, Option<String>) {
    let zeile = |kennung: &str, name: String, s: Sonnenzeit, nicht: [&str; 2], herkunft: String| {
        let (zeit, text) = match s {
            Sonnenzeit::Um(t) => (tag::uhrzeit(ort, t, d), None),
            Sonnenzeit::BleibtDarueber => (None, Some(nicht[0].to_string())),
            Sonnenzeit::BleibtDarunter => (None, Some(nicht[1].to_string())),
            Sonnenzeit::Keine => (None, Some("keine Zeit, nahe der Polargrenze".to_string())),
        };
        Tageszeile { kennung: kennung.into(), name, zeit, text, herkunft }
    };
    let tiefe = |w: f64| format!("Sonne {}° unter dem Horizont{wo}", w.to_string().replace('.', ","));
    let refraktion = format!("Refraktion {}°{wo}", format!("{:.3}", tag::tiefe_aufgang()).replace('.', ","));
    let mut aus = Vec::new();
    for (x, morgens, _) in z.daemmerung.iter().rev() {
        aus.push(zeile(&format!("{}-beginn", x.kennung), format!("Beginn {}", x.titel), *morgens, ["keine", "keine"], tiefe(x.winkel)));
    }
    aus.push(zeile("aufgang", "Aufgang".into(), z.aufgang, ["Sonne geht nicht unter", "Sonne geht nicht auf"], refraktion.clone()));
    aus.push(Tageszeile {
        kennung: "mittag".into(),
        name: "Wahrer Mittag".into(),
        zeit: tag::uhrzeit(ort, z.mittag, d),
        text: None,
        herkunft: format!("Sonne am höchsten, mit Zeitgleichung{wo}"),
    });
    aus.push(zeile("untergang", "Untergang".into(), z.untergang, ["Sonne geht nicht unter", "Sonne geht nicht auf"], refraktion));
    for (x, _, abends) in &z.daemmerung {
        aus.push(zeile(&format!("{}-ende", x.kennung), format!("Ende {}", x.titel), *abends, ["keine", "keine"], tiefe(x.winkel)));
    }
    (aus, z.taglaenge().map(tag::dauer_text))
}

/// Zeitart des Tags für die Anzeige: Zone mit Fassung der Datenbank, vor dem ersten
/// Wechsel „mittlere Ortszeit“, vor 1970 mit dem Zusatz, dass die Daten unsicher sind.
fn zeitart(ort: &Ort, d: NaiveDate) -> String {
    let mitte = ort.beginn_ab(d).map_or(crate::kalender::mond::tag_zahl(d), |b| b + 0.5);
    let mut art = ort.zeitart_um(mitte);
    if matches!(ort.zeit, crate::kalender::mond::Zeit::Zone(_)) {
        art = format!("{art} · Zeitzonen-Daten {}", crate::kalender::zone::tzdb_fassung());
        if d.year() < 1970 {
            art.push_str(" · vor 1970 nach IANA, unsicher");
        }
    }
    art
}

/// Der Tag an einem Ort; `titel` und `wo` (Herkunftsteil) beschreiben ihn ohne Zahlen.
fn tag_antwort(katalog: &Katalog, aktive: &[String], d: NaiveDate, titel: Option<String>, wo: &str) -> KalenderTag {
    let ort = katalog.ort().clone();
    let (zeiten, taglaenge) = match tag::am_tag(d, &ort) {
        Some(z) => zeilen(&ort, &z, d, wo),
        None => (Vec::new(), None),
    };
    KalenderTag {
        datum: text(d),
        hinweis: titel.is_none().then(|| SATZ_OHNE_ORT.to_string()),
        ort: titel,
        zeitart: zeitart(&ort, d),
        zeiten,
        taglaenge,
        laengster_tag: katalog.laengster_tag(d),
        mond: aktive.iter().find_map(|k| katalog.mond(k, d)),
    }
}

/// Die Feste der aktiven Kalender von `von` bis `bis` (beide eingeschlossen).
fn feste_antwort(katalog: &Katalog, aktive: &[String], von: NaiveDate, bis: NaiveDate, titel: Option<String>, wo: &str) -> KalenderFeste {
    let mut feste = Vec::new();
    for k in katalog.kalender().iter().filter(|k| aktive.contains(&k.kennung)) {
        for f in &k.fest {
            let kennung = format!("{}.{}", k.kennung, f.kennung);
            let regel = Regel { fest: Some(kennung.clone()), ..Default::default() };
            // Das Fest eines Jahres liegt höchstens ein Jahr neben ihm.
            for j in von.year() - 1..=bis.year() + 1 {
                let Some(d) = katalog.im_jahr(None, &regel, j).filter(|d| (von..=bis).contains(d)) else { continue };
                let (titel, herkunft, zeitraum) = match katalog.anzeige(k, f, d.year()) {
                    Anzeige::Entfaellt => continue,
                    Anzeige::Neutral { titel, herkunft } => (titel, herkunft, None),
                    Anzeige::Normal => (f.titel.clone(), katalog.herkunft(None, &regel, j), katalog.zeitraum(None, &regel, d)),
                };
                let lang = katalog.laengster_tag(d).filter(|_| matches!(f.regel.art(), Ok(crate::kalender::Art::Sonne(_))));
                let herkunft = format!("{herkunft}{}{wo}", lang.map(|l| format!(" · {l}")).unwrap_or_default());
                if !feste.iter().any(|x: &KalenderFest| x.kennung == kennung && x.datum == text(d)) {
                    feste.push(KalenderFest { kennung: kennung.clone(), datum: text(d), titel, kalender: k.titel.clone(), herkunft, zeitraum });
                }
            }
        }
    }
    feste.sort_by(|a, b| a.datum.cmp(&b.datum).then(a.kennung.cmp(&b.kennung)));
    let ort = katalog.ort();
    KalenderFeste {
        von: text(von),
        bis: text(bis),
        hinweis: if titel.is_none() { Some(SATZ_OHNE_ORT.into()) } else { ort.tropen().then(|| crate::kalender::HINWEIS_WENDEKREIS.into()) },
        ort: titel,
        zeitart: ort.zeit.art(),
        feste,
    }
}

fn zeitraum(von: Option<String>, bis: Option<String>, ab: NaiveDate) -> Result<(NaiveDate, NaiveDate), AppError> {
    let von = von.map(|v| datum(&v, "von")).transpose()?.unwrap_or(ab);
    let bis = bis.map(|b| datum(&b, "bis")).transpose()?.unwrap_or(von.checked_add_signed(Duration::days(30)).unwrap_or(von));
    if bis < von || (bis - von).num_days() > TAGE_HOECHSTENS {
        return Err(AppError::unlesbar(format!("„bis“ liegt nach „von“, höchstens {TAGE_HOECHSTENS} Tage später.")));
    }
    Ok((von, bis))
}

/// Der benannte Ort aus `ort=<kennung>` (sonst der Heimatort) mit seinem Katalog.
fn benannt<'a>(orte: &'a Orte, uri: &Uri) -> Result<(Katalog, Option<&'a Benannt>), AppError> {
    match angegeben(abfrage(uri, "ort")) {
        Some(k) => orte.katalog_fuer(Some(&k)).map_err(AppError::nicht_gefunden),
        None => Ok((orte.katalog(), orte.heimat())),
    }
}

fn wo(b: Option<&Benannt>) -> String {
    b.map(ort_satz).unwrap_or_default()
}

async fn inhalt(state: &AppState) -> Result<Inhalt, AppError> {
    super::anstehend::inhalt_sammeln(state).await
}

/// `GET /api/kalender/standorte`: die benannten Orte, nur Kennung, Titel und Zeitart.
pub async fn orte(State(state): State<AppState>, angemeldet: Option<Extension<Angemeldet>>) -> Response {
    if angemeldet.is_none() {
        return super::anmeldung::bitte_anmelden(None);
    }
    let orte = Orte::aus_bestand(&state.inhalt.bestand());
    let liste = orte
        .alle()
        .iter()
        .enumerate()
        .map(|(i, b)| KalenderOrt { kennung: b.kennung.clone(), titel: b.titel.clone(), zeitart: b.ort.zeit.art(), heimat: i == 0 })
        .collect();
    Json(KalenderOrte { orte: liste }).into_response()
}

/// `GET /api/kalender/zonen`: die Namen der Zeitzonen und die Fassung der Datenbank.
pub async fn zonen(angemeldet: Option<Extension<Angemeldet>>) -> Response {
    if angemeldet.is_none() {
        return super::anmeldung::bitte_anmelden(None);
    }
    Json(KalenderZonen { zonen: crate::kalender::zone::zonen(), tzdb: crate::kalender::zone::tzdb_fassung().to_string() }).into_response()
}

/// Kopf einer neuen privaten `standorte.toml`.
const KOPF_STANDORTE: &str = "# Eigene Orte für Kalender und Tageszeiten (privat). Ein Ort mit `heimat = true` ist der Heimatort.\n";

/// `POST /api/kalender/standorte`: einen abgefragten Ort merken (Frage 16). Er
/// kommt gerundet unter einer neutralen Kennung (`ort-<n>`) in die private
/// `standorte.toml`; festschreiben (committen) tut ihn wie alles im Inhalt ein Chat.
pub async fn merken(State(state): State<AppState>, kopf: HeaderMap, angemeldet: Option<Extension<Angemeldet>>, koerper: Bytes) -> Response {
    if angemeldet.is_none() {
        return super::anmeldung::bitte_anmelden(None);
    }
    async {
        json_kopf_pruefen(&kopf)?;
        let m: KalenderMerken = serde_json::from_slice(&koerper)
            .map_err(|_| AppError::unlesbar("Der Inhalt ist kein passendes JSON. Erwartet wird etwa {\"titel\": \"Ferienhaus\", \"breite\": 51.5, \"laenge\": -0.1, \"zeitzone\": \"Europe/London\"}."))?;
        let titel = m.titel.trim();
        if titel.is_empty() || titel.chars().count() > 60 || titel.chars().any(char::is_control) {
            return Err(AppError::unlesbar("„titel“ ist ein Name mit 1 bis 60 Zeichen, ohne Zeilenumbruch."));
        }
        if !(m.breite.is_finite() && (-90.0..=90.0).contains(&m.breite) && m.laenge.is_finite() && (-180.0..=180.0).contains(&m.laenge)) {
            return Err(AppError::unlesbar("„breite“ liegt zwischen −90 und 90, „laenge“ zwischen −180 und 180 Grad."));
        }
        if m.zeitzone.as_deref().is_some_and(|z| crate::kalender::zone::zone(z).is_none()) {
            return Err(AppError::unlesbar("Diese Zeitzone kennt der Server nicht. Gemeint ist ein Name der IANA-Liste, etwa Europe/Berlin."));
        }
        // Lesen, Kennung wählen und Schreiben am Stück: Zwei Aufrufe kurz hintereinander
        // (Doppelklick) bekämen sonst dieselbe Kennung.
        static SPERRE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
        let _sperre = SPERRE.lock().await;
        let projekt = state.inhalt.ordner().join(crate::kalender::orte::PROJEKT);
        if !projekt.join("projekt.toml").is_file() {
            return Err(AppError::unlesbar("Das Projekt kalender fehlt im Inhalt. Orte lassen sich erst merken, wenn es angelegt ist."));
        }
        let datei = projekt.join("daten").join("standorte.toml");
        let name = "projects/kalender/daten/standorte.toml";
        // Die Kennungen aus der Datei, wie sie jetzt auf der Platte steht (nicht aus dem Zwischenstand).
        let (alt, vorhanden) = match std::fs::read_to_string(&datei) {
            Ok(t) => match datei_lesen::<Standorte>(name, &t) {
                Ok(Datei::Ganz(s)) => (t, s.standort.into_iter().map(|s| s.kennung).collect::<Vec<_>>()),
                Ok(Datei::Teile { .. }) => return Err(AppError::unlesbar("Die Datei der Orte ist in Teile gegliedert; dorthin merkt der Server keinen Ort.")),
                Err(_) => return Err(AppError::unlesbar("Die Datei der Orte hat einen Fehler. Erst ihn beheben (die Prüfung nennt ihn), dann den Ort merken.")),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (KOPF_STANDORTE.to_string(), Vec::new()),
            Err(_) => return Err(AppError::intern("Die Datei der Orte ließ sich nicht lesen.")),
        };
        let vorlage: Vec<String> = crate::kalender::orte::vorlage().map(|v| v.alle().into_iter().map(|s| s.kennung.clone()).collect()).unwrap_or_default();
        let kennung = (1..).map(|n| format!("ort-{n}")).find(|k| !vorhanden.contains(k) && !vorlage.contains(k)).unwrap_or_default();
        let zone = m.zeitzone.as_deref().map(|z| format!("zeitzone = {}\n", toml::Value::String(z.to_string()))).unwrap_or_default();
        let neu = format!(
            "{}\n\n[[standort]]\nkennung = \"{kennung}\"\ntitel = {}\nbreite = {:.1}\nlaenge = {:.1}\nherkunft = \"hand\"\n{zone}",
            alt.trim_end(),
            toml::Value::String(titel.to_string()),
            runden(m.breite),
            runden(m.laenge)
        );
        // Der neue Text muss lesbar und eindeutig sein, bevor er die alte Datei ersetzt.
        let gut = datei_lesen::<Standorte>(name, &neu).and_then(|d| standorte_eindeutig(&d, name, &neu)).is_ok();
        if !gut {
            return Err(AppError::intern("Der Ort ließ sich nicht sauber anfügen; die Datei bleibt, wie sie war."));
        }
        // Neben der Datei schreiben (eindeutiger Name) und umbenennen: Der Inhalt liest nie eine halbe Datei.
        let tmp = datei.with_file_name(format!("standorte.toml.{}-{}.neu", std::process::id(), chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()));
        let geschrieben = std::fs::create_dir_all(datei.parent().unwrap_or(&datei)).and_then(|()| std::fs::write(&tmp, neu)).and_then(|()| std::fs::rename(&tmp, &datei));
        if geschrieben.is_err() {
            let _ = std::fs::remove_file(&tmp);
            return Err(AppError::intern("Der Dienst darf die Datei der Orte nicht schreiben, oder sie ist gerade belegt. Der Ort ist nicht gemerkt."));
        }
        // Gleich abgleichen, damit der nächste Aufruf den neuen Ort schon sieht.
        let inhalt = state.inhalt.clone();
        let _ = tokio::task::spawn_blocking(move || inhalt.abgleichen()).await;
        let zeitart = m.zeitzone.clone().unwrap_or_else(|| "fester Versatz, Näherung nach der Länge".into());
        Ok::<_, AppError>(Json(KalenderOrt { kennung, titel: titel.to_string(), zeitart, heimat: false }))
    }
    .await
    .into_response()
}

/// `GET /api/kalender/tag?datum=&ort=`.
pub async fn tag(State(state): State<AppState>, uri: Uri, angemeldet: Option<Extension<Angemeldet>>) -> Response {
    if angemeldet.is_none() {
        return super::anmeldung::bitte_anmelden(None);
    }
    async {
        let i = inhalt(&state).await?;
        let d = angegeben(abfrage(&uri, "datum")).map(|t| datum(&t, "datum")).transpose()?.unwrap_or(i.heute);
        let (k, b) = benannt(&i.orte, &uri)?;
        Ok::<_, AppError>(Json(tag_antwort(&k, &aktive(&i), d, b.map(|b| b.titel.clone()), &wo(b))))
    }
    .await
    .into_response()
}

/// `GET /api/kalender/feste?von=&bis=&ort=`.
pub async fn feste(State(state): State<AppState>, uri: Uri, angemeldet: Option<Extension<Angemeldet>>) -> Response {
    if angemeldet.is_none() {
        return super::anmeldung::bitte_anmelden(None);
    }
    async {
        let i = inhalt(&state).await?;
        let (von, bis) = zeitraum(angegeben(abfrage(&uri, "von")), angegeben(abfrage(&uri, "bis")), i.heute)?;
        let (k, b) = benannt(&i.orte, &uri)?;
        Ok::<_, AppError>(Json(feste_antwort(&k, &aktive(&i), von, bis, b.map(|b| b.titel.clone()), &wo(b))))
    }
    .await
    .into_response()
}

/// Auf 0,1° gerundet (etwa 10 km), wie benannte Orte.
fn runden(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

/// `POST /api/kalender/abfrage`: ein Ort nur für diese Antwort.
pub async fn abfrage_post(State(state): State<AppState>, kopf: HeaderMap, angemeldet: Option<Extension<Angemeldet>>, koerper: Bytes) -> Response {
    if angemeldet.is_none() {
        return super::anmeldung::bitte_anmelden(None);
    }
    async {
        json_kopf_pruefen(&kopf)?;
        // Kein Protokoll mit Inhalt: Der Körper nennt eine Lage.
        let a: KalenderAbfrage = serde_json::from_slice(&koerper).map_err(|_| {
            AppError::unlesbar("Der Inhalt ist kein passendes JSON. Erwartet wird etwa {\"breite\": 51.5, \"laenge\": -0.1, \"zeitzone\": \"Europe/London\"}.")
        })?;
        if !(a.breite.is_finite() && (-90.0..=90.0).contains(&a.breite) && a.laenge.is_finite() && (-180.0..=180.0).contains(&a.laenge)) {
            return Err(AppError::unlesbar("„breite“ liegt zwischen −90 und 90, „laenge“ zwischen −180 und 180 Grad."));
        }
        let (breite, laenge) = (runden(a.breite), runden(a.laenge));
        let (zeit, naeherung) = match a.zeitzone.as_deref().filter(|z| !z.trim().is_empty()) {
            Some(z) => (
                crate::kalender::mond::Zeit::Zone(
                    crate::kalender::zone::zone(z)
                        .ok_or_else(|| AppError::unlesbar("Diese Zeitzone kennt der Server nicht. Gemeint ist ein Name der IANA-Liste, etwa Europe/Berlin."))?,
                ),
                false,
            ),
            None => (versatz_nach_laenge(laenge), true),
        };
        let i = inhalt(&state).await?;
        let d = a.datum.map(|t| datum(&t, "datum")).transpose()?.unwrap_or(i.heute);
        let (von, bis) = zeitraum(a.von, a.bis, d)?;
        let art = zeit.art();
        let k = Katalog::eingebaut().mit_ort(Ort::an(Lage { breite, laenge }, zeit));
        let titel = Some("Abgefragter Ort".to_string());
        let wo = format!(" · Abgefragter Ort · {art}");
        let aktiv = aktive(&i);
        Ok::<_, AppError>(Json(KalenderAbfrageAntwort {
            breite,
            laenge,
            naeherung,
            tag: tag_antwort(&k, &aktiv, d, titel.clone(), &wo),
            feste: feste_antwort(&k, &aktiv, von, bis, titel, &wo),
        }))
    }
    .await
    .into_response()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use serde_json::Value;
    use tower::ServiceExt;

    use crate::app::{AppState, router, router_angemeldet};

    // Private Probeorte mit Werten, die sonst nirgends vorkommen (Nachtrag 9):
    // der Heimatort mit Zone, ein zweiter im Süden mit festem Versatz, dazu ein
    // Ort zwischen den Wendekreisen (öffentliche Stadtlage, nicht geprüft).
    const STANDORTE: &str = "[[standort]]\nkennung = \"ort-2\"\ntitel = \"Zweiter Ort\"\nbreite = -38.6\nlaenge = -62.4\nherkunft = \"hand\"\nversatz = \"-03:00\"\n\n\
                             [[standort]]\nkennung = \"zuhause\"\ntitel = \"Zuhause\"\nbreite = 47.3\nlaenge = 73.9\nherkunft = \"hand\"\nzeitzone = \"Asia/Almaty\"\nheimat = true\n\n\
                             [[standort]]\nkennung = \"ort-3\"\ntitel = \"Tropen\"\nbreite = 1.3\nlaenge = 103.8\nherkunft = \"stadt\"\nzeitzone = \"Asia/Singapore\"\n";
    /// Jede Schreibweise der privaten Lagen: Punkt, Komma, ganzzahlig, und der Versatz.
    const ZAHLEN: [&str; 15] = ["47.3", "47,3", "473", "73.9", "73,9", "739", "38.6", "38,6", "386", "62.4", "62,4", "624", "-03:00", "UTC-3", "UTC-03"];
    const ZONE: &str = "Asia/Almaty";
    const FRISTEN: &str = "kalender = [\"jahreskreis\", \"kirche\"]\n\n\
                           [[ereignis]]\nkennung = \"sonnwend\"\ntitel = \"Sonnwendfeier\"\nregel = { fest = \"jahreskreis.litha\" }\nort = \"zuhause\"\n\n\
                           [[ereignis]]\nkennung = \"winter\"\ntitel = \"Winterfeier\"\nregel = { fest = \"jahreskreis.yule\" }\nort = \"ort-2\"\n\n\
                           [[frist]]\nkennung = \"vorbereiten\"\ntitel = \"Vorbereiten\"\nereignis = \"sonnwend\"\nabstand = \"-3 Tage\"\n\n\
                           [[frist]]\nkennung = \"einladen\"\ntitel = \"Einladen\"\nereignis = \"winter\"\nabstand = \"-14 Tage\"\n";

    fn inhalt(name: &str) -> PathBuf {
        let wurzel = std::env::temp_dir().join(format!("dashboard-kalender-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wurzel);
        let p = wurzel.join("projects").join("kalender");
        std::fs::create_dir_all(p.join("daten")).unwrap();
        std::fs::write(p.join("projekt.toml"), "schema = 3\ntitle = \"Kalender\"\nstatus = \"aktiv\"\nsummary = \"S\"\n").unwrap();
        std::fs::write(p.join("daten").join("standorte.toml"), STANDORTE).unwrap();
        std::fs::write(p.join("daten").join("fristen.toml"), FRISTEN).unwrap();
        wurzel
    }

    fn standorte_datei(wurzel: &Path) -> PathBuf {
        wurzel.join("projects/kalender/daten/standorte.toml")
    }

    async fn schicke(app: &Router, req: Request<Body>) -> (StatusCode, Value) {
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap_or_else(|_| Value::String(String::from_utf8_lossy(&bytes).into())))
    }

    fn get(pfad: &str) -> Request<Body> {
        Request::get(pfad).header("host", "127.0.0.1:8787").body(Body::empty()).unwrap()
    }

    fn post_an(pfad: &str, json: Value) -> Request<Body> {
        Request::post(pfad)
            .header("host", "127.0.0.1:8787")
            .header("origin", "http://127.0.0.1:8787")
            .header("sec-fetch-site", "same-origin")
            .header("content-type", "application/json")
            .body(Body::from(json.to_string()))
            .unwrap()
    }

    fn post(json: Value) -> Request<Body> {
        post_an("/api/kalender/abfrage", json)
    }

    /// Steht `muster` in `text`, ohne Ziffer davor oder danach? (So schlägt „473“
    /// nicht in „14730“ an, „47.3“ aber in „-47.3“.)
    fn nennt(text: &str, muster: &str) -> bool {
        let ziffer = |c: Option<char>| c.is_some_and(|c| c.is_ascii_digit());
        text.match_indices(muster).any(|(i, _)| !ziffer(text[..i].chars().next_back()) && !ziffer(text[i + muster.len()..].chars().next()))
    }

    fn ohne_zahlen(was: &str, text: &str) {
        for z in ZAHLEN {
            assert!(!nennt(text, z), "{was} nennt {z}: {text}");
        }
    }

    /// Keine Zahl und keine Zone eines privaten Orts.
    fn ohne_zahlen_und_zone(was: &str, text: &str) {
        ohne_zahlen(was, text);
        assert!(!text.contains(ZONE), "{was} nennt die Zone: {text}");
    }

    /// Alle Wege nur angemeldet; benannte Orte nur mit Kennung, Titel und Zeitart.
    /// Heimatort ist der private mit `heimat = true`, auch wenn er nicht zuerst steht.
    #[tokio::test]
    async fn benannte_orte_ohne_koordinaten() {
        let wurzel = inhalt("benannt");
        let state = AppState::fuer_tests(&wurzel);
        let (frei, app) = (router(state.clone()), router_angemeldet(state));
        for pfad in ["/api/kalender/standorte", "/api/kalender/zonen", "/api/kalender/tag", "/api/kalender/feste", "/api/projekte/kalender/daten/standorte"] {
            assert_eq!(schicke(&frei, get(pfad)).await.0, StatusCode::UNAUTHORIZED, "{pfad}");
        }
        assert_eq!(schicke(&frei, post(serde_json::json!({"breite": 1.0, "laenge": 2.0}))).await.0, StatusCode::UNAUTHORIZED);
        let (s, v) = schicke(&app, get("/api/kalender/standorte")).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        let orte: Vec<(&str, bool, &str)> = v["orte"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| (o["kennung"].as_str().unwrap(), o["heimat"].as_bool().unwrap(), o["zeitart"].as_str().unwrap()))
            .collect();
        assert_eq!(orte, [("zuhause", true, ZONE), ("ort-2", false, "fester Versatz"), ("ort-3", false, "Asia/Singapore"), ("mitte", false, "Europe/Berlin")]);
        // Tag am Heimatort: Zeiten, längster Tag, Herkunft mit Ort und Zone.
        let (s, tag) = schicke(&app, get("/api/kalender/tag?datum=2027-06-21")).await;
        assert_eq!(s, StatusCode::OK, "{tag}");
        let aufgang = tag["zeiten"].as_array().unwrap().iter().find(|z| z["kennung"] == "aufgang").unwrap().clone();
        assert!(aufgang["zeit"].as_str().is_some_and(|z| z.len() == 5), "{aufgang}");
        assert!(aufgang["herkunft"].as_str().unwrap().ends_with("Refraktion 0,833° · Ort Zuhause · Asia/Almaty"), "{aufgang}");
        assert!(tag["laengster_tag"].as_str().is_some_and(|l| l.starts_with("Längster Tag · 15 h")), "{tag}");
        assert_eq!(tag["hinweis"], Value::Null);
        // Feste am Heimatort (Norden), am zweiten Ort (Süden, fester Versatz ohne Zahl) und in den Tropen.
        let (_, feste) = schicke(&app, get("/api/kalender/feste?von=2027-06-01&bis=2027-06-30")).await;
        assert!(feste["feste"].as_array().unwrap().iter().any(|f| f["titel"] == "Mittsommer (Litha)" && f["datum"] == "2027-06-21"), "{feste}");
        let (_, sued) = schicke(&app, get("/api/kalender/feste?von=2027-06-01&bis=2027-06-30&ort=ort-2")).await;
        let juni = sued["feste"].as_array().unwrap().iter().find(|f| f["kennung"] == "jahreskreis.yule").unwrap_or_else(|| panic!("{sued}")).clone();
        assert!(juni["herkunft"].as_str().unwrap().ends_with(" · Ort Zweiter Ort · fester Versatz"), "{juni}");
        let (_, tropen) = schicke(&app, get("/api/kalender/feste?von=2027-06-01&bis=2027-06-30&ort=ort-3")).await;
        let titel: Vec<&str> = tropen["feste"].as_array().unwrap().iter().filter_map(|f| f["titel"].as_str()).collect();
        assert!(titel.contains(&"Sonnenwende im Juni") && !titel.iter().any(|t| t.contains("Litha")), "{tropen}");
        assert!(tropen["hinweis"].as_str().is_some_and(|h| h.contains("Wendekreisen")), "{tropen}");
        let (_, tag_tropen) = schicke(&app, get("/api/kalender/tag?datum=2027-06-21&ort=ort-3")).await;
        assert!(tag_tropen["mond"]["hinweis"].is_string() && tag_tropen["mond"]["nummer"].is_null(), "{tag_tropen}");
        // Unbekannter Ort: 404 mit Kennung; falsches Datum: 422.
        let (s, v) = schicke(&app, get("/api/kalender/tag?ort=mars")).await;
        assert!(s == StatusCode::NOT_FOUND && v.to_string().contains("„mars“"), "{s} {v}");
        assert_eq!(schicke(&app, get("/api/kalender/feste?von=2027-01-01&bis=2029-01-01")).await.0, StatusCode::UNPROCESSABLE_ENTITY);
        // Nirgends eine Koordinate: alle Antworten, auch der Daten-Weg der Datei selbst.
        for pfad in [
            "/api/kalender/standorte",
            "/api/kalender/tag?datum=2027-06-21",
            "/api/kalender/tag?ort=ort-2",
            "/api/kalender/feste",
            "/api/kalender/feste?ort=ort-2",
            "/api/projekte/kalender/daten/standorte",
            "/api/projects/kalender",
            "/api/projekte/kalender/daten/fristen",
        ] {
            let (s, v) = schicke(&app, get(pfad)).await;
            assert_eq!(s, StatusCode::OK, "{pfad}: {v}");
            ohne_zahlen(pfad, &v.to_string());
        }
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    /// Ohne jeden Ort (privat keiner, Vorlage im Test ausgeblendet): Verhalten der Stufe 2 und ein Satz.
    #[tokio::test]
    async fn ohne_ort() {
        let wurzel = inhalt("ohne");
        std::fs::remove_file(standorte_datei(&wurzel)).unwrap();
        let marke = wurzel.join("projects").join(crate::kalender::orte::OHNE_VORLAGE);
        std::fs::create_dir_all(&marke).unwrap();
        std::fs::write(marke.join("projekt.toml"), "schema = 3\ntitle = \"Ohne Vorlage\"\nstatus = \"aktiv\"\nsummary = \"S\"\n").unwrap();
        let app = router_angemeldet(AppState::fuer_tests(&wurzel));
        let (_, v) = schicke(&app, get("/api/kalender/standorte")).await;
        assert_eq!(v["orte"], serde_json::json!([]));
        let (_, tag) = schicke(&app, get("/api/kalender/tag?datum=2027-06-21")).await;
        assert_eq!((tag["zeiten"].clone(), tag["zeitart"].as_str()), (serde_json::json!([]), Some("Ortszeit Windows")));
        assert_eq!(tag["hinweis"], super::SATZ_OHNE_ORT);
        let (s, feste) = schicke(&app, get("/api/kalender/feste?von=2027-06-01&bis=2027-06-30")).await;
        assert_eq!((s, feste["hinweis"].as_str()), (StatusCode::OK, Some(super::SATZ_OHNE_ORT)), "{feste}");
        let _ = std::fs::remove_dir_all(&wurzel);
        // Ohne private Datei, mit Vorlage: der Standardort ist Heimatort.
        let wurzel = inhalt("ohne-privat");
        std::fs::remove_file(standorte_datei(&wurzel)).unwrap();
        let app = router_angemeldet(AppState::fuer_tests(&wurzel));
        let (_, v) = schicke(&app, get("/api/kalender/standorte")).await;
        assert_eq!((v["orte"][0]["kennung"].as_str(), v["orte"][0]["heimat"].as_bool()), (Some("mitte"), Some(true)), "{v}");
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    /// `GET /api/kalender/zonen`: nur angemeldet, die IANA-Namen und die Fassung der Datenbank.
    #[tokio::test]
    async fn zonen() {
        let wurzel = inhalt("zonen");
        let state = AppState::fuer_tests(&wurzel);
        assert_eq!(schicke(&router(state.clone()), get("/api/kalender/zonen")).await.0, StatusCode::UNAUTHORIZED);
        let (s, v) = schicke(&router_angemeldet(state), get("/api/kalender/zonen")).await;
        assert_eq!(s, StatusCode::OK);
        let zonen: Vec<&str> = v["zonen"].as_array().unwrap().iter().filter_map(Value::as_str).collect();
        assert!(zonen.len() > 300 && zonen.contains(&"Europe/Berlin") && zonen.contains(&ZONE), "{}", zonen.len());
        assert!(v["tzdb"].as_str().is_some_and(|t| !t.is_empty()), "{v}");
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    /// `POST /api/kalender/standorte` (Ort merken): nur angemeldet, nur auf Knopfdruck,
    /// auf 0,1° gerundet, unter neutraler Kennung `ort-<n>`, neben der Datei geschrieben
    /// und umbenannt; Fehlersätze ohne Zahlen, unbekannte Zone abgewiesen.
    #[tokio::test]
    async fn ort_merken() {
        let wurzel = inhalt("merken");
        let datei = standorte_datei(&wurzel);
        std::fs::remove_file(&datei).unwrap();
        let state = AppState::fuer_tests(&wurzel);
        let (frei, app) = (router(state.clone()), router_angemeldet(state));
        let merken = |j: Value| post_an("/api/kalender/standorte", j);
        // Ohne Knopfdruck nichts: Lesen legt keine Datei an, ohne Anmeldung kein Schreiben.
        for pfad in ["/api/kalender/standorte", "/api/kalender/tag", "/api/kalender/feste"] {
            assert_eq!(schicke(&app, get(pfad)).await.0, StatusCode::OK, "{pfad}");
        }
        let (s, _) = schicke(&frei, merken(serde_json::json!({"titel": "Ferienhaus", "breite": 47.3449, "laenge": 73.8551}))).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        assert!(!datei.exists(), "ohne Knopfdruck keine Datei");
        // Abgewiesen: 422 mit Satz ohne die Zahlen, keine Datei.
        for (koerper, wort) in [
            (serde_json::json!({"titel": "Ferienhaus", "breite": 95.4321, "laenge": 73.8551}), "breite"),
            (serde_json::json!({"titel": "Ferienhaus", "breite": 47.3449, "laenge": 73.8551, "zeitzone": "Mond/Mitte"}), "Zeitzone"),
            (serde_json::json!({"titel": "", "breite": 47.3449, "laenge": 73.8551}), "titel"),
            (serde_json::json!({"titel": "Ferienhaus", "breite": "47.3449", "laenge": 73.8551}), "JSON"),
        ] {
            let (s, v) = schicke(&app, merken(koerper.clone())).await;
            let t = v.to_string();
            assert!(s == StatusCode::UNPROCESSABLE_ENTITY && t.contains(wort), "{koerper}: {s} {t}");
            for z in ["95.4", "47.3", "73.8", "73.9", "Mond"] {
                assert!(!t.contains(z), "{koerper}: Satz nennt {z}: {t}");
            }
        }
        assert!(!datei.exists(), "abgewiesen, keine Datei");
        // Gemerkt: gerundet, neutrale Kennung, die Antwort ohne Zahlen.
        let (s, v) = schicke(&app, merken(serde_json::json!({"titel": "Ferienhaus", "breite": 47.3449, "laenge": 73.8551, "zeitzone": ZONE}))).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert_eq!((v["kennung"].as_str(), v["titel"].as_str(), v["heimat"].as_bool()), (Some("ort-1"), Some("Ferienhaus"), Some(false)), "{v}");
        ohne_zahlen("Antwort auf Ort merken", &v.to_string());
        let text = std::fs::read_to_string(&datei).unwrap();
        assert!(
            text.starts_with("# Eigene Orte")
                && text
                    .contains("kennung = \"ort-1\"\ntitel = \"Ferienhaus\"\nbreite = 47.3\nlaenge = 73.9\nherkunft = \"hand\"\nzeitzone = \"Asia/Almaty\"\n"),
            "{text}"
        );
        // Ein zweiter ohne Zone: nächste freie Kennung, Näherung nach der Länge.
        let (s, v) = schicke(&app, merken(serde_json::json!({"titel": "Hütte", "breite": -38.6449, "laenge": -62.3551}))).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert_eq!((v["kennung"].as_str(), v["zeitart"].as_str()), (Some("ort-2"), Some("fester Versatz, Näherung nach der Länge")));
        let text = std::fs::read_to_string(&datei).unwrap();
        assert!(text.contains("kennung = \"ort-2\"\ntitel = \"Hütte\"\nbreite = -38.6\nlaenge = -62.4\nherkunft = \"hand\"\n"), "{text}");
        // Atomar: keine halbe Datei daneben; der Inhalt liest beide Orte.
        let daneben: Vec<_> = std::fs::read_dir(datei.parent().unwrap()).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(daneben.len(), 2, "nur fristen.toml und standorte.toml: {daneben:?}");
        let (_, v) = schicke(&app, get("/api/kalender/standorte")).await;
        let kennungen: Vec<&str> = v["orte"].as_array().unwrap().iter().filter_map(|o| o["kennung"].as_str()).collect();
        assert_eq!(kennungen, ["mitte", "ort-1", "ort-2"], "{v}");
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    /// Dateien neben der `standorte.toml`, die auf `.neu` enden (liegengebliebene Zwischendateien).
    fn zwischendateien(wurzel: &Path) -> Vec<std::ffi::OsString> {
        let ordner = wurzel.join("projects/kalender/daten");
        std::fs::read_dir(ordner).map(|r| r.flatten().map(|e| e.file_name()).filter(|n| n.to_string_lossy().ends_with(".neu")).collect()).unwrap_or_default()
    }

    const HEIMAT: &str = "[[standort]]\nkennung = \"zuhause\"\ntitel = \"Zuhause\"\nbreite = 47.3\nlaenge = 73.9\nherkunft = \"hand\"\nzeitzone = \"Asia/Almaty\"\nheimat = true\n";

    /// M2: Zwei Merk-Aufrufe ohne Abgleich dazwischen (Speicher im Beobachter-Modus wie
    /// im Dienst, gleichzeitig und nach einer Änderung an der Datei, die der Speicher
    /// noch nicht kennt): eindeutige Kennungen aus der Datei, die Datei bleibt lesbar,
    /// der Heimatort bleibt, keine Zwischendatei.
    #[tokio::test]
    async fn ort_merken_ohne_abgleich() {
        let wurzel = inhalt("merken-beobachter");
        let datei = standorte_datei(&wurzel);
        std::fs::write(&datei, HEIMAT).unwrap();
        let mut state = AppState::fuer_tests(&wurzel);
        state.inhalt = Arc::new(crate::inhalt::store::ContentStore::laden(
            wurzel.join("projects"),
            wurzel.join("data").join("pc.toml"),
            wurzel.join("data").join("schwerpunkte.toml"),
            crate::inhalt::store::Abgleich::Beobachter,
        ));
        let app = router_angemeldet(state);
        let merken = |titel: &str| post_an("/api/kalender/standorte", serde_json::json!({"titel": titel, "breite": 10.0, "laenge": 20.0}));
        let ((s1, v1), (s2, v2)) = tokio::join!(schicke(&app, merken("Eins")), schicke(&app, merken("Zwei")));
        assert_eq!((s1, s2), (StatusCode::OK, StatusCode::OK), "{v1} {v2}");
        let mut neu = [v1["kennung"].as_str().unwrap().to_string(), v2["kennung"].as_str().unwrap().to_string()];
        neu.sort();
        assert_eq!(neu, ["ort-1", "ort-2"]);
        // Von Hand angefügt, der Speicher weiß davon nichts: Die nächste Kennung kommt aus der Datei.
        let text = std::fs::read_to_string(&datei).unwrap();
        let drei = "\n[[standort]]\nkennung = \"ort-3\"\ntitel = \"Drei\"\nbreite = 1.0\nlaenge = 2.0\nherkunft = \"hand\"\n";
        std::fs::write(&datei, format!("{text}{drei}")).unwrap();
        let (s, v) = schicke(&app, merken("Vier")).await;
        assert_eq!((s, v["kennung"].as_str()), (StatusCode::OK, Some("ort-4")), "{v}");
        let text = std::fs::read_to_string(&datei).unwrap();
        let name = "projects/kalender/daten/standorte.toml";
        let gelesen = crate::vertrag::daten::datei_lesen::<crate::vertrag::standorte::Standorte>(name, &text).unwrap();
        crate::vertrag::standorte::standorte_eindeutig(&gelesen, name, &text).unwrap();
        assert!(text.starts_with(HEIMAT), "{text}");
        // Nach dem Merken abgeglichen: der Heimatort und alle vier.
        let (_, v) = schicke(&app, get("/api/kalender/standorte")).await;
        let orte: Vec<(&str, bool)> = v["orte"].as_array().unwrap().iter().map(|o| (o["kennung"].as_str().unwrap(), o["heimat"].as_bool().unwrap())).collect();
        assert_eq!(orte[0], ("zuhause", true), "{v}");
        for k in ["ort-1", "ort-2", "ort-3", "ort-4"] {
            assert!(orte.contains(&(k, false)), "{k}: {v}");
        }
        assert_eq!(zwischendateien(&wurzel), Vec::<std::ffi::OsString>::new());
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    /// M2: Eine kaputte Datei, eine Datei in Teilen und ein fehlendes Projekt `kalender`
    /// werden mit einem Satz (422) abgewiesen; die Datei bleibt, keine Zwischendatei.
    #[tokio::test]
    async fn ort_merken_lehnt_ab() {
        let koerper = serde_json::json!({"titel": "Ferienhaus", "breite": 47.3449, "laenge": 73.8551});
        let teile = HEIMAT.replace("[[standort]]", "[[teil.privat.standort]]");
        for (fall, text, wort) in [("kaputt", "[[standort]]\nkennung = \n", "Fehler"), ("teile", teile.as_str(), "Teile")] {
            let wurzel = inhalt(&format!("merken-{fall}"));
            let datei = standorte_datei(&wurzel);
            std::fs::write(&datei, text).unwrap();
            let app = router_angemeldet(AppState::fuer_tests(&wurzel));
            let (s, v) = schicke(&app, post_an("/api/kalender/standorte", koerper.clone())).await;
            assert!(s == StatusCode::UNPROCESSABLE_ENTITY && v.to_string().contains(wort), "{fall}: {s} {v}");
            ohne_zahlen(fall, &v.to_string());
            assert_eq!(std::fs::read_to_string(&datei).unwrap(), text, "{fall}: Datei unverändert");
            assert!(zwischendateien(&wurzel).is_empty(), "{fall}: {:?}", zwischendateien(&wurzel));
            let _ = std::fs::remove_dir_all(&wurzel);
        }
        // Ohne Projekt `kalender`: keine Datei, die nie gelesen würde.
        let wurzel = inhalt("merken-ohne-projekt");
        std::fs::remove_dir_all(wurzel.join("projects/kalender")).unwrap();
        let app = router_angemeldet(AppState::fuer_tests(&wurzel));
        let (s, v) = schicke(&app, post_an("/api/kalender/standorte", koerper)).await;
        assert!(s == StatusCode::UNPROCESSABLE_ENTITY && v.to_string().contains("Projekt kalender"), "{s} {v}");
        assert!(!wurzel.join("projects/kalender").exists(), "kein Ordner angelegt");
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    /// Aufgang, wahrer Mittag und Untergang aus einer Tagesantwort.
    fn drei(tag: &Value) -> [String; 3] {
        ["aufgang", "mittag", "untergang"].map(|k| {
            let z = tag["zeiten"].as_array().unwrap().iter().find(|z| z["kennung"] == k).unwrap_or_else(|| panic!("{k}: {tag}"));
            z["zeit"].as_str().unwrap_or_else(|| panic!("{k}: {z}")).to_string()
        })
    }

    /// M1 über die Schnittstelle: Apia und Kiritimati zeigen am 21.6.2027 den Sonnentag
    /// mit Mittag am 21.6. (ohne „(am …)“), Aufgang davor, Untergang danach; benannt
    /// (`GET /api/kalender/tag`) und abgefragt (`POST /api/kalender/abfrage`).
    /// Reykjavík: Untergang nach Mitternacht mit „(am 22.6.)“.
    #[tokio::test]
    async fn tageszeiten_nahe_der_datumsgrenze() {
        let wurzel = inhalt("datumsgrenze");
        let orte = [("apia", -13.8, -171.8, "Pacific/Apia"), ("kiritimati", 1.9, -157.5, "Pacific/Kiritimati")];
        let mut text = STANDORTE.to_string();
        for (k, b, l, z) in orte {
            text.push_str(&format!(
                "\n[[standort]]\nkennung = \"{k}\"\ntitel = \"{k}\"\nbreite = {b:.1}\nlaenge = {l:.1}\nherkunft = \"stadt\"\nzeitzone = \"{z}\"\n"
            ));
        }
        std::fs::write(standorte_datei(&wurzel), text).unwrap();
        let app = router_angemeldet(AppState::fuer_tests(&wurzel));
        for (k, b, l, z) in orte {
            let (s, benannt) = schicke(&app, get(&format!("/api/kalender/tag?datum=2027-06-21&ort={k}"))).await;
            assert_eq!(s, StatusCode::OK, "{benannt}");
            let (s, frei) = schicke(&app, post(serde_json::json!({"breite": b, "laenge": l, "zeitzone": z, "datum": "2027-06-21"}))).await;
            assert_eq!(s, StatusCode::OK, "{frei}");
            for (wie, tag) in [("benannt", &benannt), ("abgefragt", &frei["tag"])] {
                let [auf, mittag, unter] = drei(tag);
                assert!([&auf, &mittag, &unter].iter().all(|t| t.len() == 5) && auf < mittag && mittag < unter, "{k} {wie}: {auf} {mittag} {unter}");
                assert!(mittag.starts_with("12:"), "{k} {wie}: {mittag}");
                assert_eq!(tag["datum"], "2027-06-21");
            }
        }
        let (_, rey) =
            schicke(&app, post(serde_json::json!({"breite": 64.1466, "laenge": -21.9426, "zeitzone": "Atlantic/Reykjavik", "datum": "2027-06-21"}))).await;
        let [auf, mittag, unter] = drei(&rey["tag"]);
        assert!(auf.len() == 5 && mittag.starts_with("13:") && unter.starts_with("00:0") && unter.ends_with(" (am 22.6.)"), "{auf} {mittag} {unter}");
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    /// S4: Jahre außerhalb 1800–2150 (riesig, negativ, 2200) geben 422 mit Satz und
    /// keinen Absturz; die Ränder selbst gehen.
    #[tokio::test]
    async fn jahre_begrenzt() {
        let wurzel = inhalt("jahre");
        let app = router_angemeldet(AppState::fuer_tests(&wurzel));
        let falsch = ["+262142-01-01", "%2B262142-12-31", "262142-12-31", "-1000-01-01", "2200-01-01", "1799-12-31", "2151-01-01"];
        for d in falsch {
            for pfad in [
                format!("/api/kalender/feste?von={d}"),
                format!("/api/kalender/feste?von=2027-01-01&bis={d}"),
                format!("/api/kalender/tag?datum={d}"),
                format!("/api/kalender/tag?datum={d}&ort=ort-2"),
            ] {
                let (s, v) = schicke(&app, get(&pfad)).await;
                assert!(s == StatusCode::UNPROCESSABLE_ENTITY && v.to_string().contains("zwischen 1800 und 2150"), "{pfad}: {s} {v}");
            }
        }
        for d in ["+262142-01-01", "-1000-01-01", "2200-01-01"] {
            for feld in ["datum", "von", "bis"] {
                let (s, v) = schicke(&app, post(serde_json::json!({"breite": 1.0, "laenge": 2.0, feld: d}))).await;
                assert!(s == StatusCode::UNPROCESSABLE_ENTITY && v.to_string().contains(feld), "{feld}={d}: {s} {v}");
            }
        }
        // Die Ränder: 1800-01-01 und 2150-12-31 (dann bis 30 Tage danach, schon 2151).
        for pfad in [
            "/api/kalender/tag?datum=1800-01-01",
            "/api/kalender/tag?datum=2150-12-31",
            "/api/kalender/feste?von=2150-12-31",
            "/api/kalender/feste?von=1800-01-01",
        ] {
            let (s, v) = schicke(&app, get(pfad)).await;
            assert_eq!(s, StatusCode::OK, "{pfad}: {v}");
        }
        let (s, v) = schicke(&app, post(serde_json::json!({"breite": 1.0, "laenge": 2.0, "datum": "2150-12-31"}))).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    #[derive(Clone, Default)]
    struct Mitschrift(Arc<std::sync::Mutex<Vec<u8>>>);

    impl std::io::Write for Mitschrift {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn dateien(wurzel: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut aus = Vec::new();
        let mut offen = vec![wurzel.to_path_buf()];
        while let Some(o) = offen.pop() {
            for e in std::fs::read_dir(&o).unwrap().flatten() {
                if e.path().is_dir() {
                    offen.push(e.path());
                } else {
                    aus.push((e.path(), std::fs::read(e.path()).unwrap()));
                }
            }
        }
        aus.sort();
        aus
    }

    /// Eine freie Abfrage rechnet für die gerundete Lage und hinterlässt nichts:
    /// kein Protokoll mit der Lage, keine geänderte Datei, keine Zahl im Fehlersatz.
    #[tokio::test]
    async fn abfrage_hinterlaesst_nichts() {
        // Das Protokoll liest nur sicher mit, wenn der Test allein im Prozess läuft
        // (`crate::test_allein`, wie beim Einmal-Code nur unter Unix); sonst prüft er
        // Dateien und Antworten, und die Mitschrift kann Zeilen verlieren.
        #[cfg(unix)]
        if !crate::test_allein::im_eigenen_prozess("api::kalender::tests::abfrage_hinterlaesst_nichts") {
            return;
        }
        let mitschrift = Mitschrift::default();
        let schreiber = mitschrift.clone();
        let abonnent =
            tracing_subscriber::fmt().with_max_level(tracing::Level::TRACE).with_ansi(false).without_time().with_writer(move || schreiber.clone()).finish();
        let _wache = tracing::subscriber::set_default(abonnent);
        let wurzel = inhalt("abfrage");
        let vorher = dateien(&wurzel);
        let app = router_angemeldet(AppState::fuer_tests(&wurzel));
        let (s, v) =
            schicke(&app, post(serde_json::json!({"breite": 64.1466, "laenge": -21.9426, "zeitzone": "Atlantic/Reykjavik", "datum": "2027-06-21"}))).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert_eq!((v["breite"].as_f64(), v["laenge"].as_f64(), v["naeherung"].as_bool()), (Some(64.1), Some(-21.9), Some(false)));
        let ende = v["tag"]["zeiten"].as_array().unwrap().iter().find(|z| z["kennung"] == "astronomisch-ende").unwrap().clone();
        assert_eq!(ende["text"], "keine", "{ende}");
        assert!(v["tag"]["zeitart"].as_str().is_some_and(|z| z.starts_with("Atlantic/Reykjavik")), "{v}");
        // Ohne Zeitzone: Näherung nach der Länge.
        let (_, v) = schicke(&app, post(serde_json::json!({"breite": 64.1466, "laenge": -21.9426}))).await;
        assert_eq!(v["naeherung"], true);
        assert!(v["tag"]["zeitart"].as_str().unwrap().contains("Näherung"), "{v}");
        // Unmögliche Lage, unbekannte Zone, falsche Felder: 422 ohne die Zahlen.
        for (koerper, zahl) in [
            (serde_json::json!({"breite": 95.4321, "laenge": 1.0}), "95.43"),
            (serde_json::json!({"breite": 64.1466, "laenge": -21.9426, "zeitzone": "Mond/Mitte"}), "64.14"),
            (serde_json::json!({"breite": "64.1466", "laenge": 1.0}), "64.14"),
        ] {
            let (s, v) = schicke(&app, post(koerper)).await;
            assert!(s == StatusCode::UNPROCESSABLE_ENTITY && !v.to_string().contains(zahl), "{s} {v}");
        }
        assert_eq!(dateien(&wurzel), vorher, "keine Datei geändert oder neu");
        let protokoll = String::from_utf8(mitschrift.0.lock().unwrap().clone()).unwrap();
        for z in ["64.1", "21.9", "95.4", "64,1", "21,9"] {
            assert!(!protokoll.contains(z), "Protokoll nennt {z}: {protokoll}");
        }
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    /// Der Körper einer Antwort als Text; ein Strom höchstens `frist` lang.
    async fn strom_text(res: axum::response::Response, frist: Duration) -> String {
        let mut body = res.into_body();
        let ende = Instant::now() + frist;
        let mut aus = Vec::new();
        while let Ok(Some(Ok(f))) = tokio::time::timeout(ende.saturating_duration_since(Instant::now()), body.frame()).await {
            if let Some(d) = f.data_ref() {
                aus.extend_from_slice(d);
            }
        }
        String::from_utf8_lossy(&aus).into_owned()
    }

    /// Datenschutz über alle Wege (Abschnitt 9, Nachtrag 9): Mit einer privaten
    /// `standorte.toml` und Ereignissen mit `ort` steht keine Ziffernfolge der
    /// privaten Lagen (Punkt, Komma, ganzzahlig), kein Versatz und keine Tageszeit
    /// oder Taglänge eines privaten Orts im Daten-Weg der Projekte, in Fristen,
    /// Anstehend, dem Ereigniskanal, Journal und Datenspeicher, im Protokoll, in
    /// `dashboard pruefe` und in Lesefehlern einer kaputten Datei. Die Zone steht
    /// nur in angemeldeten Kalender-Antworten und in der Herkunft eines Ereignisses
    /// mit Ort (Abschnitt 8). Ohne Anmeldung gibt jeder dieser Wege 401.
    /// Eine Panikmeldung gibt es auf diesen Wegen nicht: Der Code dort hat außer in
    /// Tests kein `panic!`/`expect` (der Panik-Haken schriebe sonst ins Protokoll).
    #[tokio::test]
    async fn datenschutz_ueber_alle_wege() {
        #[cfg(unix)]
        if !crate::test_allein::im_eigenen_prozess("api::kalender::tests::datenschutz_ueber_alle_wege") {
            return;
        }
        let mitschrift = Mitschrift::default();
        let schreiber = mitschrift.clone();
        let abonnent =
            tracing_subscriber::fmt().with_max_level(tracing::Level::TRACE).with_ansi(false).without_time().with_writer(move || schreiber.clone()).finish();
        let _wache = tracing::subscriber::set_default(abonnent);
        assert!(nennt("Lage 47.3 Nord", "47.3") && nennt("-47,3", "47,3") && nennt("x473", "473") && !nennt("14730", "473"));
        let wurzel = inhalt("datenschutz");
        let state = AppState::fuer_tests(&wurzel);
        let (frei, app) = (router(state.clone()), router_angemeldet(state.clone()));

        // Die Tageszeiten und Taglängen der privaten Orte, wie sie angemeldet erscheinen.
        let heute = crate::vertrag::abgeleitet::heute().format("%Y-%m-%d").to_string();
        let mut zeiten: Vec<String> = Vec::new();
        let mut laengen: Vec<String> = Vec::new();
        for ort in ["zuhause", "ort-2"] {
            for d in [heute.as_str(), "2027-06-21", "2027-12-21"] {
                let (s, t) = schicke(&app, get(&format!("/api/kalender/tag?datum={d}&ort={ort}"))).await;
                assert_eq!(s, StatusCode::OK, "{t}");
                zeiten.extend(t["zeiten"].as_array().unwrap().iter().filter_map(|z| z["zeit"].as_str().map(str::to_string)));
                laengen.extend(t["taglaenge"].as_str().map(str::to_string));
            }
        }
        assert!(zeiten.len() > 20 && laengen.len() == 6, "{zeiten:?} {laengen:?}");
        let ohne_tageszeiten = |was: &str, text: &str, auch_uhrzeiten: bool| {
            for l in &laengen {
                assert!(!text.contains(l.as_str()), "{was} nennt die Taglänge {l}: {text}");
            }
            assert!(!text.contains("Längster Tag"), "{was} nennt den längsten Tag: {text}");
            // Zeitpunkte von Himmelsereignissen in Ortszeit sind in der Herkunft erlaubt (Nachtrag 9).
            if auch_uhrzeiten {
                for z in &zeiten {
                    assert!(!nennt(text, z), "{was} nennt die Uhrzeit {z}: {text}");
                }
            }
        };

        // Ohne Anmeldung: 401 auf allen Wegen, kein Wert.
        let wege = [
            "/api/kalender/standorte",
            "/api/kalender/zonen",
            "/api/kalender/tag",
            "/api/kalender/tag?ort=zuhause",
            "/api/kalender/feste?ort=ort-2",
            "/api/projekte/kalender/daten/standorte",
            "/api/projekte/kalender/daten/fristen",
            "/api/projects/kalender",
            "/api/fristen",
            "/api/anstehend",
            "/api/pruefung/kalender",
        ];
        for pfad in wege {
            let (s, v) = schicke(&frei, get(pfad)).await;
            assert_eq!(s, StatusCode::UNAUTHORIZED, "{pfad}");
            ohne_zahlen_und_zone(pfad, &v.to_string());
        }
        for (pfad, j) in [
            ("/api/kalender/abfrage", serde_json::json!({"breite": 47.3, "laenge": 73.9})),
            ("/api/kalender/standorte", serde_json::json!({"titel": "X", "breite": 47.3, "laenge": 73.9})),
        ] {
            let (s, v) = schicke(&frei, post_an(pfad, j)).await;
            assert_eq!(s, StatusCode::UNAUTHORIZED, "{pfad}");
            ohne_zahlen_und_zone(pfad, &v.to_string());
        }

        // Der Ereigniskanal, während sich die Dateien ändern.
        let res = app.clone().oneshot(get("/api/events")).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let kanal = tokio::spawn(strom_text(res, Duration::from_millis(1500)));
        tokio::time::sleep(Duration::from_millis(100)).await;
        std::fs::write(standorte_datei(&wurzel), format!("# geändert\n{STANDORTE}")).unwrap();
        std::fs::write(wurzel.join("projects/kalender/daten/fristen.toml"), format!("# geändert\n{FRISTEN}")).unwrap();

        // Angemeldet: Daten-Weg, Fristen, Anstehend. Die Ereignisse rechnen nach dem
        // privaten Ort (Herkunft mit Titel und Zone), ohne Zahl und ohne Tageszeit.
        let mut fristen_text = String::new();
        for pfad in
            ["/api/projekte/kalender/daten/standorte", "/api/projekte/kalender/daten/fristen", "/api/projects/kalender", "/api/fristen", "/api/anstehend"]
        {
            let (s, v) = schicke(&app, get(pfad)).await;
            assert_eq!(s, StatusCode::OK, "{pfad}: {v}");
            let t = v.to_string();
            ohne_zahlen(pfad, &t);
            ohne_tageszeiten(pfad, &t, false);
            // Der Daten-Weg der Fristen trägt die Herkunft der Ereignisse (mit Zone und Himmelszeiten).
            if pfad.ends_with("/daten/standorte") {
                assert!(!t.contains(ZONE), "{pfad} nennt die Zone: {t}");
                ohne_tageszeiten(pfad, &t, true);
            }
            if pfad == "/api/fristen" {
                fristen_text = t;
            }
        }
        assert!(fristen_text.contains("Ort Zuhause · Asia/Almaty") && fristen_text.contains("Ort Zweiter Ort · fester Versatz"), "{fristen_text}");
        let kanal = kanal.await.unwrap();
        assert!(kanal.contains("anmeldung"), "Kanal nicht gelesen: {kanal}");
        ohne_zahlen_und_zone("Ereigniskanal", &kanal);
        ohne_tageszeiten("Ereigniskanal", &kanal, true);

        // `dashboard pruefe` mit gültiger Datei, mit einem unbekannten Ort am Ereignis
        // und mit kaputten Standorten (dazu der Daten-Weg und die Prüfung im Server).
        let projekte = wurzel.join("projects");
        let pruefe = || format!("{:?}", crate::inhalt::pruefung::pruefe_alle(&projekte));
        let p = pruefe();
        assert!(!p.contains("Fehler"), "{p}");
        ohne_zahlen_und_zone("pruefe", &p);
        std::fs::write(wurzel.join("projects/kalender/daten/fristen.toml"), FRISTEN.replace("ort = \"ort-2\"", "ort = \"ort-9\"")).unwrap();
        let p = pruefe();
        assert!(p.contains("Beim Ereignis „winter“: Den Ort „ort-9“ gibt es nicht."), "{p}");
        ohne_zahlen_und_zone("pruefe mit unbekanntem Ort", &p);
        std::fs::write(wurzel.join("projects/kalender/daten/fristen.toml"), FRISTEN).unwrap();
        for (alt, neu) in [
            ("breite = 47.3", "breite = \"47.3\""),
            ("laenge = 73.9", "laenge = 73,9"),
            ("laenge = 73.9", "laenge = 73.9\nhoehe = 47.3"),
            ("breite = 47.3", "breite = 947.3"),
            ("breite = 47.3", "breite = 47.3.9"),
            ("zeitzone = \"Asia/Almaty\"", "zeitzone = \"Asia/Almaty-473\""),
            ("zeitzone = \"Asia/Almaty\"", "zeitzone = 473"),
            ("zeitzone = \"Asia/Almaty\"", "versatz = \"+47:39\""),
            ("versatz = \"-03:00\"", "versatz = -3"),
            ("heimat = true", "heimat = \"47.3\""),
        ] {
            assert!(STANDORTE.contains(alt), "{alt}");
            std::fs::write(standorte_datei(&wurzel), STANDORTE.replacen(alt, neu, 1)).unwrap();
            let p = pruefe();
            assert!(p.contains("standorte.toml"), "{neu}: kein Befund: {p}");
            ohne_zahlen_und_zone(&format!("pruefe bei {neu}"), &p);
            assert!(!p.contains("47:39") && !p.contains("Almaty-"), "{neu}: {p}");
            for pfad in
                ["/api/projekte/kalender/daten/standorte", "/api/pruefung/kalender", "/api/projects/kalender", "/api/kalender/standorte", "/api/fristen"]
            {
                let (_, v) = schicke(&app, get(pfad)).await;
                let t = v.to_string();
                ohne_zahlen(&format!("{pfad} bei {neu}"), &t);
                assert!(!t.contains("47:39") && !t.contains("Almaty-"), "{pfad} bei {neu}: {t}");
                if pfad != "/api/kalender/standorte" && pfad != "/api/fristen" {
                    assert!(!t.contains(ZONE), "{pfad} bei {neu} nennt die Zone: {t}");
                }
            }
        }
        std::fs::write(standorte_datei(&wurzel), STANDORTE).unwrap();

        // Journal, Meldungen und die Dateien des Datenspeichers; die übrigen Dateien des Inhalts.
        let speicher = state.speicher.as_ref().unwrap();
        let journal = format!("{:?}{:?}", speicher.vorgaenge_alle().unwrap(), speicher.meldungen_mit_archiv().unwrap());
        ohne_zahlen_und_zone("Journal", &journal);
        ohne_tageszeiten("Journal", &journal, true);
        let db = speicher.pfad().to_path_buf();
        for pfad in [db.clone(), PathBuf::from(format!("{}-wal", db.display()))] {
            if let Ok(b) = std::fs::read(&pfad) {
                let t = String::from_utf8_lossy(&b);
                ohne_zahlen_und_zone(&pfad.display().to_string(), &t);
            }
        }
        for (pfad, b) in dateien(&wurzel).into_iter().filter(|(p, _)| *p != standorte_datei(&wurzel)) {
            let t = String::from_utf8_lossy(&b);
            ohne_zahlen_und_zone(&pfad.display().to_string(), &t);
        }

        // Das Protokoll (sicher nur allein im Prozess, siehe `abfrage_hinterlaesst_nichts`).
        let protokoll = String::from_utf8(mitschrift.0.lock().unwrap().clone()).unwrap();
        ohne_zahlen_und_zone("Protokoll", &protokoll);
        ohne_tageszeiten("Protokoll", &protokoll, true);
        let _ = std::fs::remove_dir_all(&wurzel);
    }
}
