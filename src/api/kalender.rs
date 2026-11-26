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
use crate::kalender::gebete::{Hochbreite, Variante};
use crate::kalender::himmel;
use crate::kalender::mond::{self, Halbkugel, Lage, Ort};
use crate::kalender::orte::{Benannt, Orte, ort_satz};
use crate::kalender::tag::{self, Sonnenzeit};
use crate::kalender::{Anzeige, Katalog, Regel};
use crate::vertrag::daten::{Datei, DatenInhalt, datei_lesen};
use crate::vertrag::gebete::{Gebete, GebeteAuswahl, gebete_eindeutig};
use crate::vertrag::kalender::{
    GebeteOrtAuswahl, GebeteVariante, Gebetsliste, Gebetszeile, KalenderAbfrage, KalenderAbfrageAntwort, KalenderFest, KalenderFeste, KalenderGebete,
    KalenderGebeteSpeichern, KalenderHeimat, KalenderJahr, KalenderKopf, KalenderMerken, KalenderOrt, KalenderOrte, KalenderTag, KalenderZonen, Tageszeile,
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

/// Ein Tag, den es am Ort nicht gibt (Laientext; Apia übersprang den 30.12.2011).
pub const SATZ_TAG_FEHLT: &str = "Diesen Tag gibt es an diesem Ort nicht: Die Uhren dort haben ihn übersprungen. Darum gibt es keine Tageszeiten.";

/// Fehlen in einer Liste der Gebetszeiten Zeiten (Polargebiet), ein Satz ohne Zahlen.
pub const SATZ_GEBETE_POLAR: &str =
    "An diesem Tag fehlen hier Zeiten, weil die Sonne dafür nicht hoch oder nicht tief genug steht. Erfundene Zeiten gibt es nicht.";

/// Die gespeicherte Auswahl lässt sich nicht lesen.
const SATZ_GEBETE_DATEI: &str =
    "Die gespeicherte Auswahl der Gebetszeiten hat einen Fehler, den die Prüfung meldet. Bis er behoben ist, erscheinen keine Gebetszeiten.";

/// Eine Variante, die die Vorlage nicht kennt (ohne sie zu nennen).
const SATZ_VARIANTE_UNBEKANNT: &str = "Eine der gewählten Varianten der Gebetszeiten kennt der Server nicht.";

/// Namen der Regeln, für `umfasst`.
const REGEL_NAMEN: [(&str, &str); 7] = [
    ("fajr", "Fadschr"),
    ("sunrise", "Sonnenaufgang"),
    ("dhuhr", "Dhuhr"),
    ("asr-shafi", "Asr früh"),
    ("asr-hanafi", "Asr spät"),
    ("maghrib", "Maghrib"),
    ("isha", "Ischa"),
];

fn regel_name(r: &str) -> String {
    REGEL_NAMEN.iter().find(|(k, _)| *k == r).map_or(r, |(_, n)| n).to_string()
}

/// Name der Regel der Hochbreite für Herkunft und Hinweis („nach der …“).
fn hochbreite_name(h: &str) -> &'static str {
    match h {
        "mitte-der-nacht" => "Regel der Nachtmitte",
        "winkel" => "Winkel-Regel",
        _ => "Siebtel-Regel",
    }
}

/// Die Gebetszeiten einer Variante am Tag `d`; `wo` wie bei den Tageszeiten.
fn gebetsliste(ort: &Ort, d: NaiveDate, v: &Variante, wo: &str) -> Gebetsliste {
    let methode = crate::vertrag::gebete::vorlage().methode.iter().find(|m| m.kennung == v.methode);
    let kurz = v.methode.to_uppercase();
    let regel = hochbreite_name(&v.hochbreite);
    let refraktion = format!("{:.3}", tag::tiefe_aufgang()).replace('.', ",");
    let zeilen: Vec<Gebetszeile> = crate::kalender::gebete::rechnen(d, ort, v)
        .unwrap_or_default()
        .into_iter()
        .map(|z| {
            let winkel = methode.map_or(0.0, |m| if z.regel == "fajr" { m.fajr_winkel } else { m.isha_winkel });
            let grad = format!("{}°", winkel.to_string().replace('.', ","));
            // Die Regel der Hochbreite steht nur in der Herkunft, wenn sie an diesem Tag greift.
            let wie = match z.regel.as_str() {
                "fajr" | "isha" if z.hochbreite.is_some() => format!("{kurz} {grad} · {regel}"),
                "fajr" | "isha" => format!("{kurz} {grad}"),
                "sunrise" | "maghrib" => format!("{kurz} · Refraktion {refraktion}°"),
                "dhuhr" => format!("{kurz} · wahrer Mittag"),
                "asr-shafi" => format!("{kurz} · Schatten einfach"),
                "asr-hanafi" => format!("{kurz} · Schatten doppelt"),
                _ => kurz.clone(),
            };
            let zuschlag = methode.and_then(|m| m.zuschlaege.get(&z.regel)).filter(|n| **n != 0).map(|n| format!(" · {n:+} min")).unwrap_or_default();
            let hinweis = z.hochbreite.map(|h| match h {
                Hochbreite::NichtErreicht => format!("nach der {regel}, weil die Sonne nachts nicht {grad} unter den Horizont sinkt"),
                Hochbreite::Schranke => format!("nach der {regel}"),
            });
            let zeit = z.zeit.and_then(|t| tag::uhrzeit(ort, t, d));
            Gebetszeile {
                name: z.name,
                text: zeit.is_none().then(|| "keine Zeit".to_string()),
                zeit,
                umfasst: z.umfasst.iter().map(|r| regel_name(r)).collect(),
                hinweis,
                herkunft: format!("{wie}{zuschlag}{wo}"),
            }
        })
        .collect();
    let hinweis = zeilen.iter().any(|z| z.zeit.is_none()).then(|| SATZ_GEBETE_POLAR.to_string());
    Gebetsliste { kennung: v.kennung.clone(), titel: v.titel.clone(), zeilen, hinweis }
}

/// Die Varianten der Vorlage zu den Kennungen, in der Reihenfolge der Vorlage (jede einmal).
fn varianten_von(kennungen: &[String]) -> Vec<&'static Variante> {
    crate::vertrag::gebete::vorlage().variante.iter().filter(|v| kennungen.contains(&v.kennung)).collect()
}

/// Kennt die Vorlage jede dieser Varianten?
fn alle_bekannt(kennungen: &[String]) -> bool {
    kennungen.iter().all(|k| crate::vertrag::gebete::vorlage().variante.iter().any(|v| v.kennung == *k))
}

/// Die gespeicherte Auswahl im Projekt `kalender`; `None`, wenn die Datei einen Fehler hat.
fn auswahl_lesen(b: &crate::inhalt::store::Bestand) -> Option<Vec<GebeteAuswahl>> {
    match b.daten(crate::kalender::orte::PROJEKT, "gebete") {
        Ok(Some((DatenInhalt::Gebete(d), _))) => Some(d.alle().into_iter().cloned().collect()),
        Ok(_) => Some(Vec::new()),
        Err(_) => None,
    }
}

/// Der Tag an einem Ort; `titel` und `wo` (Herkunftsteil) beschreiben ihn ohne Zahlen;
/// `gebete`: die Varianten der Gebetszeiten, die erscheinen.
fn tag_antwort(katalog: &Katalog, aktive: &[String], d: NaiveDate, titel: Option<String>, wo: &str, gebete: &[&Variante]) -> KalenderTag {
    let ort = katalog.ort().clone();
    let (zeiten, taglaenge) = match tag::am_tag(d, &ort) {
        Some(z) => zeilen(&ort, &z, d, wo),
        None => (Vec::new(), None),
    };
    let fehlt = ort.lage.is_some() && tag::sonnentag(d, &ort).is_none();
    KalenderTag {
        datum: text(d),
        hinweis: if titel.is_none() { Some(SATZ_OHNE_ORT.to_string()) } else { fehlt.then(|| SATZ_TAG_FEHLT.to_string()) },
        gebete: if fehlt { Vec::new() } else { gebete.iter().map(|v| gebetsliste(&ort, d, v, wo)).collect() },
        ort: titel,
        zeitart: zeitart(&ort, d),
        zeiten,
        taglaenge,
        laengster_tag: katalog.laengster_tag(d),
        mond: aktive.iter().find_map(|k| katalog.mond(k, d)),
        zaehlungen: katalog.zaehlzeilen(aktive, d),
    }
}

/// Die Feste der aktiven Kalender von `von` bis `bis` (beide eingeschlossen).
fn feste_antwort(katalog: &Katalog, aktive: &[String], von: NaiveDate, bis: NaiveDate, titel: Option<String>, wo: &str) -> KalenderFeste {
    let mut feste = Vec::new();
    for k in katalog.kalender().iter().filter(|k| aktive.contains(&k.kennung)) {
        for f in &k.fest {
            let kennung = format!("{}.{}", k.kennung, f.kennung);
            let regel = Regel { fest: Some(kennung.clone()), ..Default::default() };
            for (d, j) in katalog.vorkommen(None, &regel, von, bis) {
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
    Json(orte_antwort(&state.inhalt.bestand())).into_response()
}

/// Die benannten Orte, der Heimatort zuerst; nur Kennung, Titel und Zeitart.
fn orte_antwort(b: &crate::inhalt::store::Bestand) -> KalenderOrte {
    let orte = Orte::aus_bestand(b);
    let liste = orte
        .alle()
        .iter()
        .enumerate()
        .map(|(i, b)| KalenderOrt { kennung: b.kennung.clone(), titel: b.titel.clone(), zeitart: b.ort.zeit.art(), heimat: i == 0 })
        .collect();
    KalenderOrte { orte: liste }
}

/// `GET /api/kalender/zonen`: die Namen der Zeitzonen und die Fassung der Datenbank.
pub async fn zonen(angemeldet: Option<Extension<Angemeldet>>) -> Response {
    if angemeldet.is_none() {
        return super::anmeldung::bitte_anmelden(None);
    }
    Json(KalenderZonen { zonen: crate::kalender::zone::zonen(), tzdb: crate::kalender::zone::tzdb_fassung().to_string() }).into_response()
}

/// Die Varianten der Vorlage und die gespeicherte Auswahl je Ort.
fn gebete_antwort(b: &crate::inhalt::store::Bestand) -> KalenderGebete {
    let orte = Orte::aus_bestand(b);
    let gelesen = auswahl_lesen(b);
    KalenderGebete {
        varianten: crate::vertrag::gebete::vorlage().variante.iter().map(|v| GebeteVariante { kennung: v.kennung.clone(), titel: v.titel.clone() }).collect(),
        hinweis: gelesen.is_none().then(|| SATZ_GEBETE_DATEI.to_string()),
        auswahl: gelesen
            .unwrap_or_default()
            .into_iter()
            .map(|a| GebeteOrtAuswahl { fehlt: orte.finden(&a.ort).is_none(), ort: a.ort, varianten: a.varianten })
            .collect(),
    }
}

/// `GET /api/kalender/gebete`: Varianten der Vorlage und Auswahl je Ort.
pub async fn gebete(State(state): State<AppState>, angemeldet: Option<Extension<Angemeldet>>) -> Response {
    if angemeldet.is_none() {
        return super::anmeldung::bitte_anmelden(None);
    }
    Json(gebete_antwort(&state.inhalt.bestand())).into_response()
}

/// Kopf der `gebete.toml`; die Datei schreibt nur der Server, jedes Mal ganz.
const KOPF_GEBETE: &str = "# Auswahl der Gebetszeiten je Ort (privat). Diese Datei schreibt der Server aus der Seite Kalender.\n";

/// Der ganze Text einer `gebete.toml` aus der Auswahl.
fn gebete_text(liste: &[GebeteAuswahl]) -> String {
    let mut t = KOPF_GEBETE.to_string();
    for a in liste {
        let varianten: Vec<String> = a.varianten.iter().map(|v| toml::Value::String(v.clone()).to_string()).collect();
        t.push_str(&format!("\n[[auswahl]]\nort = {}\nvarianten = [{}]\n", toml::Value::String(a.ort.clone()), varianten.join(", ")));
    }
    t
}

/// `POST /api/kalender/gebete`: die Auswahl an einem benannten Ort speichern; eine
/// leere Liste entfernt sie (auch für einen Ort, den es nicht mehr gibt).
pub async fn gebete_speichern(State(state): State<AppState>, kopf: HeaderMap, angemeldet: Option<Extension<Angemeldet>>, koerper: Bytes) -> Response {
    if angemeldet.is_none() {
        return super::anmeldung::bitte_anmelden(None);
    }
    async {
        json_kopf_pruefen(&kopf)?;
        let s: KalenderGebeteSpeichern = serde_json::from_slice(&koerper)
            .map_err(|_| AppError::unlesbar("Der Inhalt ist kein passendes JSON. Erwartet wird etwa {\"ort\": \"zuhause\", \"varianten\": [\"fuenf-mwl\"]}."))?;
        if !alle_bekannt(&s.varianten) {
            return Err(AppError::unlesbar(SATZ_VARIANTE_UNBEKANNT));
        }
        let varianten: Vec<String> = varianten_von(&s.varianten).into_iter().map(|v| v.kennung.clone()).collect();
        let bekannt = Orte::aus_bestand(&state.inhalt.bestand()).finden(&s.ort).is_some();
        if !crate::vertrag::daten::ist_kennung(&s.ort) || (!bekannt && !varianten.is_empty()) {
            return Err(AppError::unlesbar("Gebetszeiten lassen sich nur für benannte Orte speichern. Für einen anderen Ort gelten sie nur in der Abfrage."));
        }
        let saetze = super::schreiben::Saetze {
            ohne_projekt: "Das Projekt kalender fehlt im Inhalt. Die Auswahl lässt sich erst speichern, wenn es angelegt ist.",
            alt_fehlerhaft: "Die Auswahl ließ sich nicht speichern; die Datei hat einen Fehler, den die Prüfung meldet, oder ist in Teile gegliedert. Es bleibt bei der bisherigen Auswahl.",
            neu_fehlerhaft: "Die Auswahl ließ sich nicht sauber speichern; es bleibt bei der bisherigen Auswahl.",
            nicht_geschrieben: "Der Dienst darf die Datei der Auswahl nicht schreiben, oder sie ist gerade belegt. Es bleibt bei der bisherigen Auswahl.",
        };
        let ort = s.ort;
        super::schreiben::privat_schreiben::<Gebete, ()>(
            &state,
            "gebete",
            &saetze,
            |alt| {
                let mut liste = alt.map(|(_, g)| g.auswahl).unwrap_or_default();
                match liste.iter().position(|a| a.ort == ort) {
                    Some(i) if varianten.is_empty() => {
                        liste.remove(i);
                    }
                    Some(i) => liste[i].varianten = varianten,
                    None if !varianten.is_empty() => liste.push(GebeteAuswahl { ort, varianten }),
                    None => {}
                }
                Ok((Some(gebete_text(&liste)), ()))
            },
            gebete_eindeutig,
        )
        .await?;
        Ok::<_, AppError>(Json(gebete_antwort(&state.inhalt.bestand())))
    }
    .await
    .into_response()
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
        // Kennung aus der Datei auf der Platte, unter der Sperre des gemeinsamen Schreibwegs:
        // Zwei Aufrufe kurz hintereinander (Doppelklick) bekommen verschiedene Kennungen.
        let vorlage: Vec<String> = crate::kalender::orte::vorlage().map(|v| v.alle().into_iter().map(|s| s.kennung.clone()).collect()).unwrap_or_default();
        let zone = m.zeitzone.as_deref().map(|z| format!("zeitzone = {}\n", toml::Value::String(z.to_string()))).unwrap_or_default();
        let saetze = super::schreiben::Saetze {
            ohne_projekt: "Das Projekt kalender fehlt im Inhalt. Orte lassen sich erst merken, wenn es angelegt ist.",
            alt_fehlerhaft: "Der Ort ließ sich nicht merken; die Datei der Orte hat einen Fehler, den die Prüfung meldet, oder ist in Teile gegliedert. Es bleibt bei den bisherigen Orten.",
            neu_fehlerhaft: "Der Ort ließ sich nicht sauber anfügen; es bleibt bei den bisherigen Orten.",
            nicht_geschrieben: "Der Dienst darf die Datei der Orte nicht schreiben, oder sie ist gerade belegt. Der Ort ist nicht gemerkt.",
        };
        let kennung = super::schreiben::privat_schreiben::<Standorte, String>(
            &state,
            "standorte",
            &saetze,
            |alt| {
                let (text, vorhanden): (&str, Vec<String>) = match &alt {
                    Some((t, s)) => (t, s.standort.iter().map(|s| s.kennung.clone()).collect()),
                    None => (KOPF_STANDORTE, Vec::new()),
                };
                let kennung = (1..).map(|n| format!("ort-{n}")).find(|k| !vorhanden.contains(k) && !vorlage.contains(k)).unwrap_or_default();
                let neu = format!(
                    "{}\n\n[[standort]]\nkennung = \"{kennung}\"\ntitel = {}\nbreite = {:.1}\nlaenge = {:.1}\nherkunft = \"hand\"\n{zone}",
                    text.trim_end(),
                    toml::Value::String(titel.to_string()),
                    runden(m.breite),
                    runden(m.laenge)
                );
                Ok((Some(neu), kennung))
            },
            standorte_eindeutig,
        )
        .await?;
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
        let gewaehlt = match b {
            Some(b) => {
                let auswahl = auswahl_lesen(&state.inhalt.bestand()).unwrap_or_default();
                auswahl.into_iter().find(|a| a.ort == b.kennung).map(|a| varianten_von(&a.varianten)).unwrap_or_default()
            }
            None => Vec::new(),
        };
        Ok::<_, AppError>(Json(tag_antwort(&k, &aktive(&i), d, b.map(|b| b.titel.clone()), &wo(b), &gewaehlt)))
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
        let gebete = a.gebete.unwrap_or_default();
        if !alle_bekannt(&gebete) {
            return Err(AppError::unlesbar(SATZ_VARIANTE_UNBEKANNT));
        }
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
            tag: tag_antwort(&k, &aktiv, d, titel.clone(), &wo, &varianten_von(&gebete)),
            feste: feste_antwort(&k, &aktiv, von, bis, titel, &wo),
            jahr: if a.jahr == Some(true) { Some(jahr_antwort(&k, d)?) } else { None },
        }))
    }
    .await
    .into_response()
}

// ----- Stufe 6: Kopfleiste, Heimatort, Jahreskreis -----

/// Der Kalender, dessen Festjahr der Jahreskreis auf der Seite zeigt.
const JAHRESKREIS: &str = "jahreskreis";

/// Heimatort ist nur ein eigener Ort oder der Standardort (ohne Werte).
pub const SATZ_HEIMAT_NICHT: &str = "Heimatort kann nur ein eigener Ort oder der Standardort sein.";

/// Der Himmel von heute am Ort `ort` für die Kopfleiste: Auf- und Untergang am Tag
/// `heute` (fehlt einer, ein Satz ohne Zahlen), die Mondphase zum Zeitpunkt `jetzt_t`
/// (Weltzeit, [`mond::aus_weltzeit`]) und der Tag des nächsten Vollmonds ab Beginn von heute.
pub fn kopf_antwort(ort: &Benannt, heute: NaiveDate, jetzt_t: f64) -> KalenderKopf {
    let o = &ort.ort;
    let z = tag::am_tag(heute, o);
    let um = |s: Sonnenzeit| match s {
        Sonnenzeit::Um(t) => tag::uhrzeit(o, t, heute),
        _ => None,
    };
    let (aufgang, untergang) = z.as_ref().map_or((None, None), |z| (um(z.aufgang), um(z.untergang)));
    let satz = match z.as_ref().map(|z| z.aufgang) {
        _ if aufgang.is_some() && untergang.is_some() => None,
        Some(Sonnenzeit::BleibtDarunter) => Some("Die Sonne geht heute nicht auf."),
        Some(Sonnenzeit::BleibtDarueber) => Some("Die Sonne geht heute nicht unter."),
        _ => Some("Heute gibt es hier keine Zeit für Auf- und Untergang."),
    };
    let beginn = o.beginn_ab(heute).unwrap_or_else(|| mond::tag_zahl(heute));
    KalenderKopf {
        ort: ort.titel.clone(),
        aufgang,
        untergang,
        text: satz.map(str::to_string),
        phase_grad: himmel::mondphase(jetzt_t).round() % 360.0,
        phase: Katalog::eingebaut().mit_ort(o.clone()).phase_am(o.monat_ab_tag(heute), heute),
        sued: o.halbkugel == Halbkugel::Sued,
        naechster_vollmond: o.tag(himmel::phase_ab(180.0, beginn)).map(text).unwrap_or_default(),
    }
}

/// `GET /api/kalender/kopf`: Mond, Sonne und Ort von heute am Heimatort (ohne `inhalt_sammeln`).
pub async fn kopf(State(state): State<AppState>, angemeldet: Option<Extension<Angemeldet>>) -> Response {
    if angemeldet.is_none() {
        return super::anmeldung::bitte_anmelden(None);
    }
    let orte = Orte::aus_bestand(&state.inhalt.bestand());
    match orte.heimat() {
        Some(h) => {
            let jetzt = mond::aus_weltzeit(chrono::Utc::now().naive_utc());
            Json(kopf_antwort(h, crate::vertrag::abgeleitet::heute(), jetzt)).into_response()
        }
        None => AppError::nicht_gefunden(SATZ_OHNE_ORT).into_response(),
    }
}

/// Das Festjahr des Jahreskreises am Ort des Katalogs, das den Tag `d` enthält.
fn jahr_antwort(k: &Katalog, d: NaiveDate) -> Result<KalenderJahr, AppError> {
    k.festjahr(JAHRESKREIS, d).ok_or_else(|| AppError::unlesbar("Für diesen Tag lässt sich kein Festjahr rechnen."))
}

/// `GET /api/kalender/jahr?ort=&datum=`: das Festjahr am benannten Ort (sonst am Heimatort),
/// ohne `datum` für heute; ohne `inhalt_sammeln`, unabhängig von den aktiven Kalendern.
pub async fn jahr(State(state): State<AppState>, uri: Uri, angemeldet: Option<Extension<Angemeldet>>) -> Response {
    if angemeldet.is_none() {
        return super::anmeldung::bitte_anmelden(None);
    }
    async {
        let d = angegeben(abfrage(&uri, "datum")).map(|t| datum(&t, "datum")).transpose()?.unwrap_or_else(crate::vertrag::abgeleitet::heute);
        let orte = Orte::aus_bestand(&state.inhalt.bestand());
        let (k, _) = benannt(&orte, &uri)?;
        Ok::<_, AppError>(Json(jahr_antwort(&k, d)?))
    }
    .await
    .into_response()
}

/// Der Text der privaten `standorte.toml` mit `heimat = true` genau beim Ort `ziel`
/// (`None`: bei keinem). Er entsteht aus dem alten Text, nicht aus der Struktur: Zeilen
/// `heimat = …` in den `[[standort]]`-Blöcken fallen weg, hinter der Zeile
/// `kennung = "<ziel>"` kommt `heimat = true`; alles andere bleibt Byte für Byte.
/// Eine andere Form (andere Tabellen, Inline-Tabellen, die Kennung anders geschrieben)
/// gibt `None`, ebenso ein Ergebnis, das nicht genau diesen Heimatort trägt.
fn heimat_text(alt: &str, ziel: Option<&str>) -> Option<String> {
    let kennung = ziel.map(|z| format!("kennung = {}", toml::Value::String(z.to_string())));
    let mut neu = String::with_capacity(alt.len() + 16);
    let (mut im_block, mut gesetzt) = (false, 0);
    for zeile in alt.split_inclusive('\n') {
        let t = zeile.trim();
        if t.starts_with('[') {
            if t != "[[standort]]" {
                return None;
            }
            im_block = true;
        } else if im_block && t.strip_prefix("heimat").is_some_and(|r| r.trim_start().starts_with('=')) {
            continue;
        }
        neu.push_str(zeile);
        if im_block && kennung.as_deref() == Some(t) {
            let ende = if zeile.ends_with("\r\n") { "\r\n" } else { "\n" };
            if !zeile.ends_with('\n') {
                neu.push_str(ende);
            }
            neu.push_str("heimat = true");
            neu.push_str(ende);
            gesetzt += 1;
        }
    }
    let Ok(Datei::Ganz(g)) = datei_lesen::<Standorte>("standorte.toml", &neu) else { return None };
    let heimat: Vec<&str> = g.standort.iter().filter(|s| s.heimat).map(|s| s.kennung.as_str()).collect();
    (gesetzt == usize::from(ziel.is_some()) && heimat == ziel.into_iter().collect::<Vec<_>>()).then_some(neu)
}

/// `POST /api/kalender/heimat`: ein eigener Ort oder der Standardort wird Heimatort. Ist er
/// es schon, oder fehlt bei Wahl des Standardorts die private Datei, bleibt alles, wie es ist.
pub async fn heimat(State(state): State<AppState>, kopf: HeaderMap, angemeldet: Option<Extension<Angemeldet>>, koerper: Bytes) -> Response {
    if angemeldet.is_none() {
        return super::anmeldung::bitte_anmelden(None);
    }
    async {
        json_kopf_pruefen(&kopf)?;
        let h: KalenderHeimat =
            serde_json::from_slice(&koerper).map_err(|_| AppError::unlesbar("Der Inhalt ist kein passendes JSON. Erwartet wird etwa {\"ort\": \"zuhause\"}."))?;
        let standard = crate::kalender::orte::vorlage().and_then(|v| v.alle().first().map(|s| s.kennung.clone()));
        let saetze = super::schreiben::Saetze {
            ohne_projekt: "Das Projekt kalender fehlt im Inhalt. Ein Heimatort lässt sich erst wählen, wenn es angelegt ist.",
            alt_fehlerhaft: "Der Heimatort ließ sich nicht wählen; die Datei der Orte hat einen Fehler, den die Prüfung meldet, oder eine Form, die der Server nicht umschreibt. Es bleibt beim bisherigen Heimatort.",
            neu_fehlerhaft: "Der Heimatort ließ sich nicht sauber setzen; es bleibt beim bisherigen.",
            nicht_geschrieben: "Der Dienst darf die Datei der Orte nicht schreiben, oder sie ist gerade belegt. Es bleibt beim bisherigen Heimatort.",
        };
        super::schreiben::privat_schreiben::<Standorte, ()>(
            &state,
            "standorte",
            &saetze,
            |alt| {
                let privat = alt.as_ref().is_some_and(|(_, s)| s.standort.iter().any(|x| x.kennung == h.ort));
                let ziel = if privat {
                    Some(h.ort.as_str())
                } else if standard.as_deref() == Some(h.ort.as_str()) {
                    None
                } else {
                    return Err(AppError::unlesbar(SATZ_HEIMAT_NICHT));
                };
                let Some((text, _)) = alt else { return Ok((None, ())) };
                let neu = heimat_text(text, ziel).ok_or_else(|| AppError::unlesbar(saetze.alt_fehlerhaft))?;
                Ok(((neu != text).then_some(neu), ()))
            },
            standorte_eindeutig,
        )
        .await?;
        Ok::<_, AppError>(Json(orte_antwort(&state.inhalt.bestand())))
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
    /// Was von einer aktiven Zählung und einem Ereignis mit `zaehlung` nie hinausgehen darf
    /// (außer in angemeldeten Antworten).
    const ZAEHL_WOERTER: [&str; 5] = ["hochland", "Hochland", "B'atz'", "Waqxaqib", "Cholq'ij"];
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

    /// Stufe 5: Die aktiven Zählungen stehen in GET tag und POST abfrage in der Reihenfolge der
    /// Vorlage, unabhängig vom Ort; ohne aktive Zählung ist die Liste leer. Waqxaqib' B'atz'
    /// steht 2025 zweimal in den Festen.
    #[tokio::test]
    async fn zaehlungen_im_tag() {
        let wurzel = inhalt("zaehlungen");
        let app = router_angemeldet(AppState::fuer_tests(&wurzel));
        let (_, t) = schicke(&app, get("/api/kalender/tag?datum=2026-11-26")).await;
        assert_eq!(t["zaehlungen"], serde_json::json!([]), "{t}");
        let fristen = FRISTEN.replace("kalender = [\"jahreskreis\", \"kirche\"]", "kalender = [\"dreamspell\", \"hochland\", \"maya-584283\"]");
        std::fs::write(wurzel.join("projects/kalender/daten/fristen.toml"), fristen).unwrap();
        let kennungen = |t: &Value| t["zaehlungen"].as_array().unwrap().iter().map(|z| z["kennung"].as_str().unwrap().to_string()).collect::<Vec<_>>();
        for pfad in ["/api/kalender/tag?datum=2026-11-26", "/api/kalender/tag?datum=2026-11-26&ort=ort-2"] {
            let (s, t) = schicke(&app, get(pfad)).await;
            assert_eq!(s, StatusCode::OK, "{t}");
            assert_eq!(kennungen(&t), ["maya-584283", "hochland", "dreamspell"], "{t}");
            let maya = &t["zaehlungen"][0];
            assert_eq!(maya["stellung"], "13.0.14.2.8 · 9 Lamat · 1 Mak · Kalenderrunde 17048");
            assert_eq!(maya["teile"].as_array().unwrap().len(), 4);
            assert_eq!((maya["stufe"].as_str(), maya["hinweis"].clone()), (Some("belegt"), Value::Null));
            assert!(maya["herkunft"].as_str().unwrap().starts_with("Maya · "), "{maya}");
            assert_eq!(t["zaehlungen"][2]["stufe"], "neuzeitlich");
        }
        let (_, a) = schicke(&app, post(serde_json::json!({"breite": 51.5, "laenge": -0.1, "datum": "2026-11-26"}))).await;
        assert_eq!(kennungen(&a["tag"]), ["maya-584283", "hochland", "dreamspell"], "{a}");
        let (_, f) = schicke(&app, get("/api/kalender/feste?von=2025-01-01&bis=2025-12-31")).await;
        let tage: Vec<&str> =
            f["feste"].as_array().unwrap().iter().filter(|x| x["kennung"] == "hochland.waqxaqib-batz").filter_map(|x| x["datum"].as_str()).collect();
        assert_eq!(tage, ["2025-01-18", "2025-10-05"], "{f}");
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

    /// Schickt alle Anfragen zugleich, jede in einer eigenen Aufgabe (auf einer
    /// mehrfädigen Laufzeit also wirklich nebeneinander), und gibt die Antworten.
    async fn zugleich(app: &Router, anfragen: Vec<Request<Body>>) -> Vec<(StatusCode, Value)> {
        let aufgaben: Vec<_> = anfragen
            .into_iter()
            .map(|r| {
                let app = app.clone();
                tokio::spawn(async move { schicke(&app, r).await })
            })
            .collect();
        let mut aus = Vec::new();
        for a in aufgaben {
            aus.push(a.await.unwrap());
        }
        aus
    }

    /// M2 und Sperre des Schreibwegs: Merk-Aufrufe ohne Abgleich dazwischen (Speicher im
    /// Beobachter-Modus wie im Dienst), sechs gleichzeitig auf mehreren Fäden und einer nach
    /// einer Änderung an der Datei, die der Speicher noch nicht kennt: eindeutige Kennungen
    /// aus der Datei, die Datei bleibt lesbar, der Heimatort bleibt, keine Zwischendatei.
    /// Dann sechs Auswahlen der Gebetszeiten gleichzeitig: jede genau einmal in der Datei.
    /// Grenze: Ob sich die Aufrufe wirklich überlappen, hängt vom Planer ab; ohne Sperre
    /// fiel der Test in Probeläufen jedes Mal, sicher ist das aber nicht.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
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
        let antworten = zugleich(&app, (1..=6).map(|n| merken(&format!("Ort {n}"))).collect()).await;
        let mut neu: Vec<String> = antworten
            .iter()
            .map(|(s, v)| {
                assert_eq!(*s, StatusCode::OK, "{v}");
                v["kennung"].as_str().unwrap().to_string()
            })
            .collect();
        neu.sort();
        assert_eq!(neu, ["ort-1", "ort-2", "ort-3", "ort-4", "ort-5", "ort-6"]);
        // Von Hand angefügt, der Speicher weiß davon nichts: Die nächste Kennung kommt aus der Datei.
        let text = std::fs::read_to_string(&datei).unwrap();
        let sieben = "\n[[standort]]\nkennung = \"ort-7\"\ntitel = \"Sieben\"\nbreite = 1.0\nlaenge = 2.0\nherkunft = \"hand\"\n";
        std::fs::write(&datei, format!("{text}{sieben}")).unwrap();
        let (s, v) = schicke(&app, merken("Acht")).await;
        assert_eq!((s, v["kennung"].as_str()), (StatusCode::OK, Some("ort-8")), "{v}");
        let text = std::fs::read_to_string(&datei).unwrap();
        let name = "projects/kalender/daten/standorte.toml";
        let gelesen = crate::vertrag::daten::datei_lesen::<crate::vertrag::standorte::Standorte>(name, &text).unwrap();
        crate::vertrag::standorte::standorte_eindeutig(&gelesen, name, &text).unwrap();
        assert!(text.starts_with(HEIMAT), "{text}");
        // Nach dem Merken abgeglichen: der Heimatort und alle acht.
        let (_, v) = schicke(&app, get("/api/kalender/standorte")).await;
        let orte: Vec<(&str, bool)> = v["orte"].as_array().unwrap().iter().map(|o| (o["kennung"].as_str().unwrap(), o["heimat"].as_bool().unwrap())).collect();
        assert_eq!(orte[0], ("zuhause", true), "{v}");
        for n in 1..=8 {
            assert!(orte.contains(&(format!("ort-{n}").as_str(), false)), "ort-{n}: {v}");
        }
        assert_eq!(zwischendateien(&wurzel), Vec::<std::ffi::OsString>::new());

        // Sechs Auswahlen der Gebetszeiten zugleich, je ein Ort: Keine geht verloren.
        let orte = ["zuhause", "ort-1", "ort-2", "ort-3", "ort-4", "ort-5"];
        let antworten =
            zugleich(&app, orte.iter().map(|o| post_an("/api/kalender/gebete", serde_json::json!({"ort": o, "varianten": ["fuenf-mwl"]}))).collect()).await;
        for (s, v) in &antworten {
            assert_eq!(*s, StatusCode::OK, "{v}");
        }
        let text = std::fs::read_to_string(gebete_datei(&wurzel)).unwrap();
        let name = "projects/kalender/daten/gebete.toml";
        let gelesen = crate::vertrag::daten::datei_lesen::<crate::vertrag::gebete::Gebete>(name, &text).unwrap();
        crate::vertrag::gebete::gebete_eindeutig(&gelesen, name, &text).unwrap();
        for o in orte {
            assert_eq!(text.matches(&format!("ort = \"{o}\"\n")).count(), 1, "{o}: {text}");
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

    /// Der Text des Ereigniskanals ohne die Zeitstempel `at`: Jede `data:`-Zeile als
    /// JSON gelesen und `at` auf jeder Ebene entfernt, sonst träfe eine Uhrzeit wie
    /// die einer Probezeit zufällig den Zeitstempel. Andere Zeilen bleiben, wie sie sind.
    fn kanal_ohne_zeitstempel(kanal: &str) -> String {
        fn weg(v: &mut Value) {
            match v {
                Value::Object(m) => {
                    m.remove("at");
                    m.values_mut().for_each(weg);
                }
                Value::Array(a) => a.iter_mut().for_each(weg),
                _ => {}
            }
        }
        let zeile = |l: &str| match l.strip_prefix("data:").map(|d| serde_json::from_str::<Value>(d.trim())) {
            Some(Ok(mut v)) => {
                weg(&mut v);
                format!("data: {v}")
            }
            _ => l.to_string(),
        };
        kanal.lines().map(zeile).collect::<Vec<_>>().join("\n")
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
        // Stufe 5: eine aktive Zählung und ein Ereignis mit `zaehlung` (nennen Kanal, Journal und Protokoll nicht).
        let fristen = FRISTEN.replace("\"kirche\"]", "\"kirche\", \"hochland\"]")
            + "\n[[ereignis]]\nkennung = \"zaehltag\"\ntitel = \"Feier\"\nregel = { zaehlung = { variante = \"hochland\", zahl = 8, zeichen = \"B'atz'\" } }\n\n\
               [[frist]]\nkennung = \"zaehlfrist\"\ntitel = \"Vorher\"\nereignis = \"zaehltag\"\nabstand = \"-1 Tag\"\n";
        std::fs::write(wurzel.join("projects/kalender/daten/fristen.toml"), &fristen).unwrap();
        let ohne_zaehlung = |was: &str, text: &str| {
            for w in ZAEHL_WOERTER {
                assert!(!text.contains(w), "{was} nennt {w}: {text}");
            }
        };
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
            "/api/kalender/kopf",
            "/api/kalender/jahr",
            "/api/kalender/jahr?ort=zuhause",
        ];
        for pfad in wege {
            let (s, v) = schicke(&frei, get(pfad)).await;
            assert_eq!(s, StatusCode::UNAUTHORIZED, "{pfad}");
            ohne_zahlen_und_zone(pfad, &v.to_string());
        }
        for (pfad, j) in [
            ("/api/kalender/abfrage", serde_json::json!({"breite": 47.3, "laenge": 73.9})),
            ("/api/kalender/standorte", serde_json::json!({"titel": "X", "breite": 47.3, "laenge": 73.9})),
            ("/api/kalender/heimat", serde_json::json!({"ort": "zuhause"})),
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
        std::fs::write(wurzel.join("projects/kalender/daten/fristen.toml"), format!("# geändert\n{fristen}")).unwrap();

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
        // Angemeldet steht das Ereignis der Zählung in den Fristen (sonst prüfte der Rest nichts).
        assert!(fristen_text.contains("260-Tage-Zyklus der Zählung Hochland"), "{fristen_text}");
        // Stufe 6: Kopfleiste und Festjahr angemeldet mit Titel und Zeiten, ohne Zahl der Lage und ohne Zone.
        for pfad in ["/api/kalender/kopf", "/api/kalender/jahr", "/api/kalender/jahr?ort=ort-2"] {
            let (s, v) = schicke(&app, get(pfad)).await;
            assert_eq!(s, StatusCode::OK, "{pfad}: {v}");
            ohne_zahlen_und_zone(pfad, &v.to_string());
        }
        let kanal = kanal.await.unwrap();
        assert!(kanal.contains("anmeldung"), "Kanal nicht gelesen: {kanal}");
        let kanal = kanal_ohne_zeitstempel(&kanal);
        assert!(kanal.contains("anmeldung") && !kanal.contains("\"at\""), "{kanal}");
        ohne_zahlen_und_zone("Ereigniskanal", &kanal);
        ohne_tageszeiten("Ereigniskanal", &kanal, true);
        ohne_zaehlung("Ereigniskanal", &kanal);

        // `dashboard pruefe` mit gültiger Datei, mit einem unbekannten Ort am Ereignis
        // und mit kaputten Standorten (dazu der Daten-Weg und die Prüfung im Server).
        let projekte = wurzel.join("projects");
        let pruefe = || format!("{:?}", crate::inhalt::pruefung::pruefe_alle(&projekte));
        let p = pruefe();
        assert!(!p.contains("Fehler"), "{p}");
        ohne_zahlen_und_zone("pruefe", &p);
        std::fs::write(wurzel.join("projects/kalender/daten/fristen.toml"), fristen.replace("ort = \"ort-2\"", "ort = \"ort-9\"")).unwrap();
        let p = pruefe();
        assert!(p.contains("Beim Ereignis „winter“: Den Ort „ort-9“ gibt es nicht."), "{p}");
        ohne_zahlen_und_zone("pruefe mit unbekanntem Ort", &p);
        // Ein unbekanntes Zeichen: Der Satz nennt weder Zeichen noch Zahl.
        std::fs::write(
            wurzel.join("projects/kalender/daten/fristen.toml"),
            fristen.replace("zahl = 8, zeichen = \"B'atz'\"", "zahl = 7, zeichen = \"Spuerhund\""),
        )
        .unwrap();
        let p = pruefe();
        assert!(p.contains("passt die `regel` nicht") && !p.contains("Spuerhund") && !p.contains("zahl = 7"), "{p}");
        // Die erlaubten Zählungen der öffentlichen Vorlage darf der Satz nennen, das Zeichen am Ereignis nicht.
        assert!(!p.contains("B'atz'"), "{p}");
        std::fs::write(wurzel.join("projects/kalender/daten/fristen.toml"), &fristen).unwrap();
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
        ohne_zaehlung("Journal", &journal);
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
        ohne_zaehlung("Protokoll", &protokoll);
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    // ----- Stufe 4: Gebetszeiten -----

    /// Probeauswahl: die Probevariante (nur in Tests, `vertrag::gebete::vorlage`) und eine echte.
    const GEBETE: &str = "[[auswahl]]\nort = \"zuhause\"\nvarianten = [\"spuerhund-gebet\", \"fuenf-mwl\"]\n";
    /// Was von der Probeauswahl nie hinausgehen darf (außer in angemeldeten Kalender-Antworten).
    const AUSWAHL_WOERTER: [&str; 4] = ["spuerhund", "SPUERHUND", "fuenf-mwl", "Fünf Zeiten"];

    fn gebete_datei(wurzel: &Path) -> PathBuf {
        wurzel.join("projects/kalender/daten/gebete.toml")
    }

    fn ohne_auswahl(was: &str, text: &str) {
        for w in AUSWAHL_WOERTER {
            assert!(!text.contains(w), "{was} nennt {w}: {text}");
        }
    }

    /// Die Zeile einer Liste der Gebetszeiten nach Namen.
    fn gebetszeile(liste: &Value, name: &str) -> Value {
        liste["zeilen"].as_array().unwrap().iter().find(|z| z["name"] == name).unwrap_or_else(|| panic!("{name}: {liste}")).clone()
    }

    /// Die Liste einer Variante aus einer Tagesantwort.
    fn gebetsliste(tag: &Value, kennung: &str) -> Value {
        tag["gebete"].as_array().unwrap().iter().find(|l| l["kennung"] == kennung).unwrap_or_else(|| panic!("{kennung}: {tag}")).clone()
    }

    /// `GET /api/kalender/gebete` und `POST /api/kalender/gebete`: nur angemeldet;
    /// speichern, ändern, leeren; ganze Datei mit fester Kopfzeile in der Reihenfolge
    /// der Vorlage; abgewiesen werden unbekannte Varianten und Orte, die Teile-Form und
    /// ein fehlendes Projekt, ohne Zwischendatei und ohne die Varianten im Satz.
    #[tokio::test]
    async fn gebete_auswahl_speichern() {
        let wurzel = inhalt("gebete-speichern");
        let datei = gebete_datei(&wurzel);
        let state = AppState::fuer_tests(&wurzel);
        let (frei, app) = (router(state.clone()), router_angemeldet(state));
        let speichern = |ort: &str, v: &[&str]| post_an("/api/kalender/gebete", serde_json::json!({"ort": ort, "varianten": v}));
        assert_eq!(schicke(&frei, get("/api/kalender/gebete")).await.0, StatusCode::UNAUTHORIZED);
        let (s, v) = schicke(&frei, speichern("zuhause", &["fuenf-mwl"])).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{v}");
        assert!(!datei.exists());
        // Ohne Datei: alle Varianten der Vorlage, keine Auswahl, keine Gebetszeiten.
        let (s, v) = schicke(&app, get("/api/kalender/gebete")).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        let kennungen: Vec<&str> = v["varianten"].as_array().unwrap().iter().filter_map(|x| x["kennung"].as_str()).collect();
        assert_eq!(kennungen, ["fuenf-mwl", "fuenf-isna", "drei-mwl", "drei-isna", "spuerhund-gebet"]);
        assert_eq!((v["auswahl"].clone(), v["hinweis"].clone()), (serde_json::json!([]), Value::Null));
        let (_, tag) = schicke(&app, get("/api/kalender/tag?datum=2027-06-21")).await;
        assert_eq!(tag["gebete"], serde_json::json!([]));

        // Speichern: in der Reihenfolge der Vorlage, jede einmal.
        let (s, v) = schicke(&app, speichern("zuhause", &["drei-isna", "fuenf-mwl", "drei-isna"])).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert_eq!(v["auswahl"], serde_json::json!([{"ort": "zuhause", "varianten": ["fuenf-mwl", "drei-isna"], "fehlt": false}]));
        let text = std::fs::read_to_string(&datei).unwrap();
        assert!(text.starts_with("# Auswahl der Gebetszeiten") && text.ends_with("ort = \"zuhause\"\nvarianten = [\"fuenf-mwl\", \"drei-isna\"]\n"), "{text}");
        let (s, v) = schicke(&app, speichern("mitte", &["fuenf-isna"])).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert_eq!(v["auswahl"].as_array().unwrap().len(), 2, "{v}");

        // Die Zeilen am Heimatort: Zeit, Hinweis der Hochbreite, Herkunft mit Methode, Regel und Ort.
        let (_, tag) = schicke(&app, get("/api/kalender/tag?datum=2027-06-21")).await;
        let fuenf = gebetsliste(&tag, "fuenf-mwl");
        assert_eq!(fuenf["zeilen"].as_array().unwrap().len(), 7, "{fuenf}");
        assert_eq!(fuenf["hinweis"], Value::Null, "{fuenf}");
        for z in fuenf["zeilen"].as_array().unwrap() {
            assert!(z["zeit"].as_str().is_some_and(|t| t.len() == 5) && z["text"].is_null(), "{z}");
            assert!(z["herkunft"].as_str().unwrap().starts_with("MWL") && z["herkunft"].as_str().unwrap().ends_with(" · Ort Zuhause · Asia/Almaty"), "{z}");
        }
        let ischa = gebetszeile(&fuenf, "Ischa");
        // Die Regel der Hochbreite steht nur in der Herkunft, wenn sie greift.
        let regel = if ischa["hinweis"].is_null() { "" } else { " · Siebtel-Regel" };
        assert!(ischa["hinweis"].is_null() || ischa["hinweis"] == "nach der Siebtel-Regel", "{ischa}");
        assert_eq!(ischa["herkunft"], format!("MWL 17°{regel} · Ort Zuhause · Asia/Almaty"), "{ischa}");
        assert_eq!(gebetszeile(&fuenf, "Dhuhr")["herkunft"], "MWL · wahrer Mittag · +1 min · Ort Zuhause · Asia/Almaty");
        assert!(gebetszeile(&fuenf, "Asr früh")["zeit"].as_str() < gebetszeile(&fuenf, "Asr spät")["zeit"].as_str());
        let drei = gebetsliste(&tag, "drei-isna");
        assert_eq!(gebetszeile(&drei, "Abend und Nacht")["umfasst"], serde_json::json!(["Ischa"]));
        assert_eq!(gebetszeile(&drei, "Mittag und Nachmittag")["umfasst"], serde_json::json!(["Asr früh"]));
        // Am Standardort nur seine Auswahl, an einem Ort ohne Auswahl keine.
        let (_, mitte) = schicke(&app, get("/api/kalender/tag?datum=2027-06-21&ort=mitte")).await;
        let titel: Vec<&str> = mitte["gebete"].as_array().unwrap().iter().filter_map(|l| l["kennung"].as_str()).collect();
        assert_eq!(titel, ["fuenf-isna"]);
        let (_, sued) = schicke(&app, get("/api/kalender/tag?datum=2027-06-21&ort=ort-2")).await;
        assert_eq!(sued["gebete"], serde_json::json!([]));

        // Leeren entfernt den Eintrag des Orts.
        let (s, v) = schicke(&app, speichern("mitte", &[])).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        let text = std::fs::read_to_string(&datei).unwrap();
        assert!(!text.contains("mitte") && text.contains("zuhause"), "{text}");

        // Abgewiesen, Datei unverändert, Sätze ohne Varianten.
        for (koerper, wort) in [
            (serde_json::json!({"ort": "zuhause", "varianten": ["spuerhund-unbekannt"]}), "kennt der Server nicht"),
            (serde_json::json!({"ort": "mars", "varianten": ["fuenf-mwl"]}), "benannte Orte"),
            (serde_json::json!({"ort": "Zu Hause", "varianten": ["fuenf-mwl"]}), "benannte Orte"),
            (serde_json::json!({"ort": "zuhause", "varianten": "fuenf-mwl"}), "JSON"),
        ] {
            let (s, v) = schicke(&app, post_an("/api/kalender/gebete", koerper.clone())).await;
            assert!(s == StatusCode::UNPROCESSABLE_ENTITY && v.to_string().contains(wort), "{koerper}: {s} {v}");
            assert!(!v.to_string().contains("spuerhund") && !v.to_string().contains("fuenf-mwl\""), "{koerper}: {v}");
            assert_eq!(std::fs::read_to_string(&datei).unwrap(), text);
        }

        // Ein Ort, den es nicht mehr gibt: GET meldet ihn, leeren geht.
        std::fs::write(&datei, format!("{text}\n[[auswahl]]\nort = \"weg\"\nvarianten = [\"drei-mwl\"]\n")).unwrap();
        let (_, v) = schicke(&app, get("/api/kalender/gebete")).await;
        assert!(v["auswahl"].as_array().unwrap().iter().any(|a| a["ort"] == "weg" && a["fehlt"] == true), "{v}");
        let (s, v) = schicke(&app, speichern("weg", &[])).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert!(!std::fs::read_to_string(&datei).unwrap().contains("weg"));

        // Kaputte Datei: GET mit Satz, Tageszeiten ohne Gebete, Speichern abgewiesen.
        std::fs::write(&datei, "[[auswahl]]\nort = \n").unwrap();
        let (_, v) = schicke(&app, get("/api/kalender/gebete")).await;
        assert!(v["hinweis"].as_str().is_some_and(|h| h.contains("Fehler")), "{v}");
        let (s, tag) = schicke(&app, get("/api/kalender/tag?datum=2027-06-21")).await;
        assert!(s == StatusCode::OK && tag["gebete"] == serde_json::json!([]), "{tag}");
        assert_eq!(zwischendateien(&wurzel), Vec::<std::ffi::OsString>::new());
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    /// Teile-Form, kaputte Datei und fehlendes Projekt `kalender`: 422 mit Satz, ohne
    /// Bitte um Handarbeit; die Datei bleibt, keine Zwischendatei.
    #[tokio::test]
    async fn gebete_speichern_lehnt_ab() {
        let koerper = serde_json::json!({"ort": "zuhause", "varianten": ["fuenf-mwl"]});
        let teile = GEBETE.replace("[[auswahl]]", "[[teil.privat.auswahl]]");
        for (fall, text, wort) in
            [("kaputt", "[[auswahl]]\nort = \n", "Fehler"), ("teile", teile.as_str(), "Teile"), ("doppelt", &format!("{GEBETE}\n{GEBETE}"), "Fehler")]
        {
            let wurzel = inhalt(&format!("gebete-{fall}"));
            let datei = gebete_datei(&wurzel);
            std::fs::write(&datei, text).unwrap();
            let app = router_angemeldet(AppState::fuer_tests(&wurzel));
            let (s, v) = schicke(&app, post_an("/api/kalender/gebete", koerper.clone())).await;
            let t = v.to_string();
            assert!(s == StatusCode::UNPROCESSABLE_ENTITY && t.contains(wort) && t.contains("bisherigen Auswahl"), "{fall}: {s} {v}");
            ohne_auswahl(fall, &t);
            assert_eq!(std::fs::read_to_string(&datei).unwrap(), text, "{fall}: Datei unverändert");
            assert!(zwischendateien(&wurzel).is_empty(), "{fall}");
            let _ = std::fs::remove_dir_all(&wurzel);
        }
        let wurzel = inhalt("gebete-ohne-projekt");
        std::fs::remove_dir_all(wurzel.join("projects/kalender")).unwrap();
        let app = router_angemeldet(AppState::fuer_tests(&wurzel));
        // Der Standardort der Vorlage gibt es auch ohne Projekt.
        let (s, v) = schicke(&app, post_an("/api/kalender/gebete", serde_json::json!({"ort": "mitte", "varianten": ["fuenf-mwl"]}))).await;
        assert!(s == StatusCode::UNPROCESSABLE_ENTITY && v.to_string().contains("Projekt kalender"), "{s} {v}");
        assert!(!wurzel.join("projects/kalender").exists());
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    /// `POST /api/kalender/abfrage` mit `gebete`: Zeilen nur für diese Antwort, nichts
    /// gespeichert. Hochbreite (London im Juni: 18° nicht erreicht) und Polargebiet
    /// (Tromsø: ein Satz ohne Zahlen, Dhuhr und Asr mit Zeit, der Rest „keine Zeit“).
    #[tokio::test]
    async fn abfrage_mit_gebetszeiten() {
        let wurzel = inhalt("gebete-abfrage");
        let vorher = dateien(&wurzel);
        let app = router_angemeldet(AppState::fuer_tests(&wurzel));
        let abfrage =
            |b: f64, l: f64, z: &str, g: Value| post(serde_json::json!({"breite": b, "laenge": l, "zeitzone": z, "datum": "2027-06-21", "gebete": g}));
        let (s, v) = schicke(&app, abfrage(51.5, -0.1, "Europe/London", serde_json::json!(["fuenf-mwl"]))).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        let fajr = gebetszeile(&gebetsliste(&v["tag"], "fuenf-mwl"), "Fadschr");
        assert_eq!(fajr["hinweis"], "nach der Siebtel-Regel, weil die Sonne nachts nicht 18° unter den Horizont sinkt", "{fajr}");
        assert_eq!(fajr["herkunft"], "MWL 18° · Siebtel-Regel · Abgefragter Ort · Europe/London");
        // Im Winter greift die Regel nicht: kein Hinweis, die Herkunft nennt sie nicht.
        let winter = serde_json::json!({"breite": 51.5, "laenge": -0.1, "zeitzone": "Europe/London", "datum": "2027-12-21", "gebete": ["fuenf-mwl"]});
        let (_, v) = schicke(&app, post(winter)).await;
        let fajr = gebetszeile(&gebetsliste(&v["tag"], "fuenf-mwl"), "Fadschr");
        assert_eq!(
            (fajr["hinweis"].clone(), fajr["herkunft"].clone()),
            (Value::Null, serde_json::json!("MWL 18° · Abgefragter Ort · Europe/London")),
            "{fajr}"
        );
        let (_, v) = schicke(&app, abfrage(69.6, 19.0, "Europe/Oslo", serde_json::json!(["fuenf-mwl", "drei-mwl"]))).await;
        let liste = gebetsliste(&v["tag"], "fuenf-mwl");
        assert_eq!(liste["hinweis"], super::SATZ_GEBETE_POLAR);
        assert!(!super::SATZ_GEBETE_POLAR.chars().any(|c| c.is_ascii_digit()));
        for (name, zeit) in
            [("Dhuhr", true), ("Asr früh", true), ("Asr spät", true), ("Fadschr", false), ("Sonnenaufgang", false), ("Maghrib", false), ("Ischa", false)]
        {
            let z = gebetszeile(&liste, name);
            assert_eq!((z["zeit"].is_string(), z["text"].as_str()), (zeit, (!zeit).then_some("keine Zeit")), "{name}: {z}");
        }
        assert_eq!(v["tag"]["gebete"].as_array().unwrap().len(), 2);
        // Ohne `gebete`: keine Listen; eine unbekannte Variante: 422 ohne ihren Namen.
        let (_, v) = schicke(&app, post(serde_json::json!({"breite": 51.5, "laenge": -0.1}))).await;
        assert_eq!(v["tag"]["gebete"], serde_json::json!([]));
        let (s, v) = schicke(&app, abfrage(51.5, -0.1, "Europe/London", serde_json::json!(["spuerhund-unbekannt"]))).await;
        assert!(s == StatusCode::UNPROCESSABLE_ENTITY && !v.to_string().contains("spuerhund"), "{s} {v}");
        assert_eq!(dateien(&wurzel), vorher, "nichts gespeichert");
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    /// Nachgang S3: Einen Tag, den es am Ort nicht gibt (Apia, 30.12.2011), zeigt
    /// ein Laiensatz statt einer leeren Liste, benannt und abgefragt; die Tage davor und danach gehen.
    #[tokio::test]
    async fn tag_den_es_nicht_gibt() {
        let wurzel = inhalt("apia-2011");
        let apia = "\n[[standort]]\nkennung = \"apia\"\ntitel = \"Apia\"\nbreite = -13.8\nlaenge = -171.8\nherkunft = \"stadt\"\nzeitzone = \"Pacific/Apia\"\n";
        std::fs::write(standorte_datei(&wurzel), format!("{STANDORTE}{apia}")).unwrap();
        std::fs::write(gebete_datei(&wurzel), "[[auswahl]]\nort = \"apia\"\nvarianten = [\"fuenf-mwl\"]\n").unwrap();
        let app = router_angemeldet(AppState::fuer_tests(&wurzel));
        let (s, benannt) = schicke(&app, get("/api/kalender/tag?datum=2011-12-30&ort=apia")).await;
        assert_eq!(s, StatusCode::OK, "{benannt}");
        let (_, frei) = schicke(
            &app,
            post(serde_json::json!({"breite": -13.8, "laenge": -171.8, "zeitzone": "Pacific/Apia", "datum": "2011-12-30", "gebete": ["fuenf-mwl"]})),
        )
        .await;
        for tag in [&benannt, &frei["tag"]] {
            assert_eq!(tag["hinweis"], super::SATZ_TAG_FEHLT, "{tag}");
            assert_eq!((tag["zeiten"].clone(), tag["gebete"].clone()), (serde_json::json!([]), serde_json::json!([])), "{tag}");
        }
        for d in ["2011-12-29", "2011-12-31"] {
            let (_, t) = schicke(&app, get(&format!("/api/kalender/tag?datum={d}&ort=apia"))).await;
            assert!(t["hinweis"].is_null() && !t["zeiten"].as_array().unwrap().is_empty() && t["gebete"].as_array().unwrap().len() == 1, "{d}: {t}");
        }
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    /// Datenschutz der Auswahl über alle Wege (Auftrag S4): Mit der Probeauswahl stehen
    /// weder ihre Varianten noch der Name „SPUERHUND-GEBET“ noch seine Zeit im Daten-Weg,
    /// in Fristen, Anstehend, dem Ereigniskanal, Journal und Datenspeicher, in
    /// `dashboard pruefe`, der Prüf-Schnittstelle, in Lesefehlern einer kaputten Datei
    /// und im Protokoll. Ohne Anmeldung 401 ohne Inhalt; angemeldet sind sie in den
    /// Kalender-Antworten da (sonst prüfte der Test nichts).
    #[tokio::test]
    async fn gebete_datenschutz_ueber_alle_wege() {
        #[cfg(unix)]
        if !crate::test_allein::im_eigenen_prozess("api::kalender::tests::gebete_datenschutz_ueber_alle_wege") {
            return;
        }
        let mitschrift = Mitschrift::default();
        let schreiber = mitschrift.clone();
        let abonnent =
            tracing_subscriber::fmt().with_max_level(tracing::Level::TRACE).with_ansi(false).without_time().with_writer(move || schreiber.clone()).finish();
        let _wache = tracing::subscriber::set_default(abonnent);
        let wurzel = inhalt("gebete-datenschutz");
        std::fs::write(gebete_datei(&wurzel), GEBETE).unwrap();
        let state = AppState::fuer_tests(&wurzel);
        let (frei, app) = (router(state.clone()), router_angemeldet(state.clone()));

        // Angemeldet da: die Probezeile mit Zeit und die Auswahl.
        let heute = crate::vertrag::abgeleitet::heute().format("%Y-%m-%d").to_string();
        let mut zeiten = Vec::new();
        for d in [heute.as_str(), "2027-06-21"] {
            let (_, t) = schicke(&app, get(&format!("/api/kalender/tag?datum={d}"))).await;
            let probe = gebetszeile(&gebetsliste(&t, "spuerhund-gebet"), "SPUERHUND-GEBET");
            zeiten.push(probe["zeit"].as_str().unwrap().to_string());
        }
        let (_, v) = schicke(&app, get("/api/kalender/gebete")).await;
        assert!(v["auswahl"].to_string().contains("spuerhund-gebet"), "{v}");
        let ohne = |was: &str, text: &str, auch_zeiten: bool| {
            ohne_auswahl(was, text);
            if auch_zeiten {
                for z in &zeiten {
                    assert!(!nennt(text, z), "{was} nennt die Zeit {z}: {text}");
                }
            }
        };

        // Ohne Anmeldung: 401 auf allen Wegen.
        for pfad in [
            "/api/kalender/gebete",
            "/api/kalender/tag",
            "/api/projekte/kalender/daten/gebete",
            "/api/projects/kalender",
            "/api/fristen",
            "/api/anstehend",
            "/api/pruefung/kalender",
        ] {
            let (s, v) = schicke(&frei, get(pfad)).await;
            assert_eq!(s, StatusCode::UNAUTHORIZED, "{pfad}");
            ohne(pfad, &v.to_string(), true);
        }
        for (pfad, j) in [
            ("/api/kalender/gebete", serde_json::json!({"ort": "zuhause", "varianten": ["spuerhund-gebet"]})),
            ("/api/kalender/abfrage", serde_json::json!({"breite": 1.0, "laenge": 2.0, "gebete": ["spuerhund-gebet"]})),
        ] {
            let (s, v) = schicke(&frei, post_an(pfad, j)).await;
            assert_eq!(s, StatusCode::UNAUTHORIZED, "{pfad}");
            ohne(pfad, &v.to_string(), true);
        }

        // Der Ereigniskanal, während sich die Auswahl ändert (über die Seite und von außen).
        let res = app.clone().oneshot(get("/api/events")).await.unwrap();
        let kanal = tokio::spawn(strom_text(res, Duration::from_millis(1500)));
        tokio::time::sleep(Duration::from_millis(100)).await;
        let (s, v) = schicke(&app, post_an("/api/kalender/gebete", serde_json::json!({"ort": "mitte", "varianten": ["spuerhund-gebet"]}))).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        std::fs::write(gebete_datei(&wurzel), format!("# geändert\n{GEBETE}")).unwrap();

        // Angemeldet: Daten-Weg, Projekt, Fristen, Anstehend, Prüfung.
        for pfad in ["/api/projekte/kalender/daten/gebete", "/api/projects/kalender", "/api/fristen", "/api/anstehend", "/api/pruefung/kalender"] {
            let (s, v) = schicke(&app, get(pfad)).await;
            assert_eq!(s, StatusCode::OK, "{pfad}: {v}");
            // Fristen und Anstehend tragen Himmelszeiten in der Herkunft; dort nur die Namen.
            ohne(pfad, &v.to_string(), !matches!(pfad, "/api/fristen" | "/api/anstehend" | "/api/projects/kalender"));
        }
        let kanal = kanal.await.unwrap();
        assert!(kanal.contains("anmeldung"), "Kanal nicht gelesen: {kanal}");
        let kanal = kanal_ohne_zeitstempel(&kanal);
        assert!(kanal.contains("anmeldung") && kanal.contains("gebete.toml") && !kanal.contains("\"at\""), "{kanal}");
        ohne("Ereigniskanal", &kanal, true);

        // `dashboard pruefe`: gültig, mit unbekanntem Ort und mit kaputten Dateien; dazu
        // der Daten-Weg und die Prüfung im Server.
        let projekte = wurzel.join("projects");
        let pruefe = || format!("{:?}", crate::inhalt::pruefung::pruefe_alle(&projekte));
        let p = pruefe();
        assert!(!p.contains("gebete.toml"), "{p}");
        std::fs::write(gebete_datei(&wurzel), GEBETE.replace("zuhause", "ort-9")).unwrap();
        let p = pruefe();
        assert!(p.contains("Die Auswahl der Gebetszeiten nennt den Ort „ort-9“, den es nicht mehr gibt."), "{p}");
        ohne("pruefe mit unbekanntem Ort", &p, true);
        let (_, v) = schicke(&app, get("/api/kalender/gebete")).await;
        assert!(v["auswahl"].as_array().unwrap().iter().any(|a| a["ort"] == "ort-9" && a["fehlt"] == true), "{v}");
        for (alt, neu) in [
            ("\"spuerhund-gebet\", ", "\"spuerhund-x\", "),
            ("varianten = [", "varianten = \"spuerhund-gebet\"\nx = ["),
            ("varianten = [", "varianten = [[\"spuerhund-gebet\"], "),
            ("ort = \"zuhause\"", "ort = \"spuerhund gebet\""),
            ("ort = \"zuhause\"", "ort = [\"spuerhund-gebet\"]"),
            ("]\n", "\n"),
            ("varianten", "spuerhund-gebet"),
        ] {
            assert!(GEBETE.contains(alt), "{alt}");
            std::fs::write(gebete_datei(&wurzel), GEBETE.replacen(alt, neu, 1)).unwrap();
            let p = pruefe();
            assert!(p.contains("gebete.toml"), "{neu}: kein Befund: {p}");
            ohne(&format!("pruefe bei {neu}"), &p, true);
            for pfad in ["/api/projekte/kalender/daten/gebete", "/api/pruefung/kalender", "/api/projects/kalender", "/api/kalender/gebete", "/api/fristen"] {
                let (_, v) = schicke(&app, get(pfad)).await;
                // Die Varianten der Vorlage (mit der Probevariante) listet die Auswahl immer.
                let t = if pfad == "/api/kalender/gebete" { format!("{}{}", v["auswahl"], v["hinweis"]) } else { v.to_string() };
                for w in ["spuerhund", "SPUERHUND"] {
                    assert!(!t.contains(w), "{pfad} bei {neu} nennt {w}: {t}");
                }
            }
            // Speichern verweigert, ohne die Auswahl zu nennen.
            let (s, v) = schicke(&app, post_an("/api/kalender/gebete", serde_json::json!({"ort": "zuhause", "varianten": ["fuenf-isna"]}))).await;
            assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{neu}: {v}");
            ohne(&format!("Speichern bei {neu}"), &v.to_string(), true);
        }
        std::fs::write(gebete_datei(&wurzel), GEBETE).unwrap();

        // Journal, Meldungen, Datenspeicher und die übrigen Dateien des Inhalts.
        let speicher = state.speicher.as_ref().unwrap();
        let journal = format!("{:?}{:?}", speicher.vorgaenge_alle().unwrap(), speicher.meldungen_mit_archiv().unwrap());
        ohne("Journal", &journal, true);
        let db = speicher.pfad().to_path_buf();
        for pfad in [db.clone(), PathBuf::from(format!("{}-wal", db.display()))] {
            if let Ok(b) = std::fs::read(&pfad) {
                ohne(&pfad.display().to_string(), &String::from_utf8_lossy(&b), false);
            }
        }
        for (pfad, b) in dateien(&wurzel).into_iter().filter(|(p, _)| *p != gebete_datei(&wurzel)) {
            ohne(&pfad.display().to_string(), &String::from_utf8_lossy(&b), false);
        }

        // Das Protokoll (sicher nur allein im Prozess, siehe `abfrage_hinterlaesst_nichts`).
        let protokoll = String::from_utf8(mitschrift.0.lock().unwrap().clone()).unwrap();
        ohne("Protokoll", &protokoll, true);
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    // ----- Stufe 6: Kopfleiste, Heimatort, Jahreskreis -----

    fn d(t: &str) -> chrono::NaiveDate {
        chrono::NaiveDate::parse_from_str(t, "%Y-%m-%d").unwrap()
    }

    fn mittag_utc(t: &str) -> f64 {
        crate::kalender::mond::aus_weltzeit(d(t).and_hms_opt(12, 0, 0).unwrap())
    }

    fn benannt_an(kennung: &str, breite: f64, laenge: f64, zone: &str) -> crate::kalender::orte::Benannt {
        let zeit = crate::kalender::mond::Zeit::Zone(crate::kalender::zone::zone(zone).unwrap());
        crate::kalender::orte::Benannt {
            kennung: kennung.into(),
            titel: kennung.into(),
            ort: crate::kalender::mond::Ort::an(crate::kalender::mond::Lage { breite, laenge }, zeit),
            oeffentlich: false,
        }
    }

    /// Die Kopfleiste rechnet wie der Tag: Auf- und Untergang am Standardort gleich den
    /// Zeilen von GET tag; am Tag des Vollmonds (19.6.2027) ist er der nächste und das
    /// Phasenwort „Vollmond“, die Phase in Grad nahe 180; am Tag danach der nächste im Juli.
    /// Tromsø am 21.12.: Polarnacht, ein Satz statt der Zeiten. Sydney: gespiegelt.
    #[test]
    fn kopf_antwort_fachlich() {
        let orte = crate::kalender::orte::Orte::aus(None, crate::kalender::orte::vorlage().as_ref());
        let mitte = orte.heimat().unwrap();
        for tag in ["2027-03-20", "2027-06-21", "2027-12-21"] {
            let k = super::kopf_antwort(mitte, d(tag), mittag_utc(tag));
            let t = super::tag_antwort(&orte.katalog(), &[], d(tag), Some(mitte.titel.clone()), "", &[]);
            let zeile = |kennung: &str| t.zeiten.iter().find(|z| z.kennung == kennung).and_then(|z| z.zeit.clone());
            assert_eq!((k.aufgang.clone(), k.untergang.clone(), k.text.clone()), (zeile("aufgang"), zeile("untergang"), None), "{tag}");
            assert_eq!(k.ort, "Mitte Deutschlands (Mühlhausen/Thüringen)");
            assert!(!k.sued);
        }
        let k = super::kopf_antwort(mitte, d("2027-06-19"), mittag_utc("2027-06-19"));
        assert_eq!((k.naechster_vollmond.as_str(), k.phase.as_str()), ("2027-06-19", "Vollmond"), "{k:?}");
        assert!((170.0..=190.0).contains(&k.phase_grad) && k.phase_grad.fract() == 0.0, "{k:?}");
        let k = super::kopf_antwort(mitte, d("2027-06-20"), mittag_utc("2027-06-20"));
        assert_eq!((k.naechster_vollmond.as_str(), k.phase.as_str()), ("2027-07-18", "abnehmend"), "{k:?}");
        let tromsoe = benannt_an("tromsoe", 69.6, 19.0, "Europe/Oslo");
        let k = super::kopf_antwort(&tromsoe, d("2027-12-21"), mittag_utc("2027-12-21"));
        assert_eq!((k.aufgang, k.untergang, k.text.as_deref()), (None, None, Some("Die Sonne geht heute nicht auf.")));
        let k = super::kopf_antwort(&tromsoe, d("2027-06-21"), mittag_utc("2027-06-21"));
        assert_eq!(k.text.as_deref(), Some("Die Sonne geht heute nicht unter."));
        assert!(super::kopf_antwort(&benannt_an("sydney", -33.9, 151.2, "Australia/Sydney"), d("2027-06-21"), mittag_utc("2027-06-21")).sued);
    }

    /// `GET /api/kalender/kopf`: nur angemeldet; Form; der Heimatort mit Titel, ohne Zahl und Zone.
    #[tokio::test]
    async fn kopf_schnittstelle() {
        let wurzel = inhalt("kopf");
        let state = AppState::fuer_tests(&wurzel);
        let (s, v) = schicke(&router(state.clone()), get("/api/kalender/kopf")).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        ohne_zahlen_und_zone("kopf ohne Anmeldung", &v.to_string());
        let (s, v) = schicke(&router_angemeldet(state), get("/api/kalender/kopf")).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        let felder: Vec<&str> = v.as_object().unwrap().keys().map(String::as_str).collect();
        assert_eq!(felder, ["ort", "aufgang", "untergang", "text", "phase_grad", "phase", "sued", "naechster_vollmond"]);
        assert_eq!((v["ort"].as_str(), v["sued"].as_bool()), (Some("Zuhause"), Some(false)));
        assert!(v["aufgang"].as_str().is_some_and(|a| a.len() == 5) && v["naechster_vollmond"].as_str().is_some_and(|t| t.len() == 10), "{v}");
        ohne_zahlen_und_zone("kopf", &v.to_string());
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    /// `GET /api/kalender/jahr` und der Zweig in `POST /api/kalender/abfrage`: nur angemeldet,
    /// am Heimatort mit Monaten, zwischen den Wendekreisen ohne, falsches Datum 422, ohne Koordinaten.
    #[tokio::test]
    async fn jahr_schnittstelle() {
        let wurzel = inhalt("jahr");
        let state = AppState::fuer_tests(&wurzel);
        assert_eq!(schicke(&router(state.clone()), get("/api/kalender/jahr")).await.0, StatusCode::UNAUTHORIZED);
        let app = router_angemeldet(state);
        let (s, v) = schicke(&app, get("/api/kalender/jahr?datum=2027-06-21")).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert!(v["monate"].as_array().is_some_and(|m| m.len() >= 12) && v["sonnentage"].as_array().unwrap().len() == 4, "{v}");
        ohne_zahlen_und_zone("jahr", &v.to_string());
        let (_, tropen) = schicke(&app, get("/api/kalender/jahr?datum=2027-06-21&ort=ort-3")).await;
        assert!(tropen["monate"].as_array().unwrap().is_empty() && tropen["hinweis"].as_str().is_some_and(|h| h.contains("Wendekreisen")), "{tropen}");
        let (_, sued) = schicke(&app, get("/api/kalender/jahr?datum=2027-07-15&ort=ort-2")).await;
        assert_eq!(sued["sonnentage"][0]["kennung"], "wende-winter");
        assert!(sued["sonnentage"][0]["datum"].as_str().unwrap().starts_with("2027-06"), "{sued}");
        let (s, _) = schicke(&app, get("/api/kalender/jahr?datum=2200-01-01")).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(schicke(&app, get("/api/kalender/jahr?ort=mars")).await.0, StatusCode::NOT_FOUND);
        let (s, a) =
            schicke(&app, post(serde_json::json!({"breite": -33.9, "laenge": 151.2, "zeitzone": "Australia/Sydney", "datum": "2027-07-15", "jahr": true})))
                .await;
        assert_eq!(s, StatusCode::OK, "{a}");
        assert_eq!(a["jahr"]["sonnentage"][0]["kennung"], "wende-winter");
        let (_, ohne) = schicke(&app, post(serde_json::json!({"breite": 1.0, "laenge": 2.0}))).await;
        assert_eq!(ohne["jahr"], Value::Null);
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    /// Eine von Hand geschriebene Datei mit Kommentaren und `heimat = false`.
    const HANDSCHRIFT: &str = "# Meine Orte\r\n\r\n[[standort]]\r\nkennung = \"zuhause\"   \r\ntitel = \"Zuhause\" # Wohnung\r\nbreite = 47.3\r\nlaenge = 73.9\r\nherkunft = \"hand\"\r\nzeitzone = \"Asia/Almaty\"\r\nheimat = true\r\n\r\n# Der zweite\r\n[[standort]]\r\nkennung = \"ort-2\"\r\nheimat = false\r\ntitel = \"Zweiter Ort\"\r\nbreite = -38.6\r\nlaenge = -62.4\r\nherkunft = \"hand\"\r\nversatz = \"-03:00\"\r\n";

    /// Der neue Text aus dem alten: alle Zeilen außer `heimat` bleiben Byte für Byte, höchstens ein `heimat`.
    #[test]
    fn heimat_text_aus_dem_alten() {
        let ohne_heimat = |t: &str| t.split_inclusive('\n').filter(|z| !z.trim_start().starts_with("heimat")).collect::<String>();
        let neu = super::heimat_text(HANDSCHRIFT, Some("ort-2")).unwrap();
        assert_eq!(ohne_heimat(&neu), ohne_heimat(HANDSCHRIFT));
        assert_eq!(neu.matches("heimat").count(), 1, "{neu}");
        assert!(neu.contains("kennung = \"ort-2\"\r\nheimat = true\r\ntitel = \"Zweiter Ort\""), "{neu}");
        let keiner = super::heimat_text(HANDSCHRIFT, None).unwrap();
        assert_eq!(keiner, ohne_heimat(HANDSCHRIFT));
        // Leerzeichen hinter der Kennung bleiben; eine Kennung ohne Zeilenende am Schluss bekommt eins.
        let zuhause = super::heimat_text(HANDSCHRIFT, Some("zuhause")).unwrap();
        let soll = "kennung = \"zuhause\"   \r\nheimat = true\r\ntitel";
        assert!(zuhause.contains(soll) && zuhause.matches("heimat").count() == 1, "{zuhause}");
        let ohne_ende = "[[standort]]\nkennung = \"a\"\ntitel = \"A\"\nbreite = 1.0\nlaenge = 2.0\nherkunft = \"hand\"\nversatz = \"+01:00\"\n[[standort]]\ntitel = \"B\"\nbreite = 1.0\nlaenge = 2.0\nherkunft = \"hand\"\nversatz = \"+01:00\"\nkennung = \"b\"";
        assert!(super::heimat_text(ohne_ende, Some("b")).is_some_and(|t| t.ends_with("kennung = \"b\"\nheimat = true\n")));
        // Andere Formen: Inline-Tabellen, Teile, eine fremde Tabelle.
        for fremd in [
            "standort = [{ kennung = \"a\", titel = \"A\", breite = 1.0, laenge = 2.0, herkunft = \"hand\", versatz = \"+01:00\" }]\n",
            "[[teil.privat.standort]]\nkennung = \"a\"\n",
            "[[standort]]\nkennung = \"a\"\n[standort.x]\n",
        ] {
            assert!(super::heimat_text(fremd, Some("a")).is_none(), "{fremd}");
        }
    }

    /// `POST /api/kalender/heimat`: privat → privat, privat → Standardort, schon Heimat (nichts
    /// geschrieben), unbekannter Ort (422 ohne Werte), ohne Datei und Standardort (keine Datei),
    /// ohne Anmeldung 401, ohne Projekt der Satz wie bei „Ort merken“.
    #[tokio::test]
    async fn heimat_waehlen() {
        let wurzel = inhalt("heimat");
        let datei = standorte_datei(&wurzel);
        std::fs::write(&datei, HANDSCHRIFT).unwrap();
        let state = AppState::fuer_tests(&wurzel);
        let (frei, app) = (router(state.clone()), router_angemeldet(state));
        let waehlen = |ort: &str| post_an("/api/kalender/heimat", serde_json::json!({ "ort": ort }));
        assert_eq!(schicke(&frei, waehlen("ort-2")).await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(std::fs::read_to_string(&datei).unwrap(), HANDSCHRIFT);
        let heimat = |v: &Value| v["orte"][0]["kennung"].as_str().map(str::to_string);
        // Privat → privat.
        let (s, v) = schicke(&app, waehlen("ort-2")).await;
        assert_eq!((s, heimat(&v)), (StatusCode::OK, Some("ort-2".into())), "{v}");
        let text = std::fs::read_to_string(&datei).unwrap();
        assert_eq!(text.matches("heimat").count(), 1, "{text}");
        for zeile in HANDSCHRIFT.split_inclusive('\n').filter(|z| !z.starts_with("heimat")) {
            assert!(text.contains(zeile), "{zeile:?} fehlt: {text}");
        }
        ohne_zahlen("Antwort auf heimat", &v.to_string());
        // Schon Heimat: 200, Datei unverändert.
        let (s, _) = schicke(&app, waehlen("ort-2")).await;
        assert_eq!((s, std::fs::read_to_string(&datei).unwrap()), (StatusCode::OK, text));
        // Privat → Standardort: kein `heimat` mehr, der Standardort steht zuerst.
        let (s, v) = schicke(&app, waehlen("mitte")).await;
        assert_eq!((s, heimat(&v)), (StatusCode::OK, Some("mitte".into())), "{v}");
        assert!(!std::fs::read_to_string(&datei).unwrap().contains("heimat"));
        // Unbekannter Ort: 422, Satz ohne Werte, Datei bleibt.
        let vorher = std::fs::read_to_string(&datei).unwrap();
        let (s, v) = schicke(&app, waehlen("mars")).await;
        assert_eq!((s, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(super::SATZ_HEIMAT_NICHT)), "{v}");
        assert_eq!(std::fs::read_to_string(&datei).unwrap(), vorher);
        assert!(zwischendateien(&wurzel).is_empty());
        // Ohne private Datei und Standardort: 200, keine Datei; ein anderer Ort: 422.
        std::fs::remove_file(&datei).unwrap();
        assert_eq!(schicke(&app, waehlen("mitte")).await.0, StatusCode::OK);
        assert_eq!(schicke(&app, waehlen("zuhause")).await.0, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(!datei.exists(), "keine Datei angelegt");
        let _ = std::fs::remove_dir_all(&wurzel);
        // Ohne Projekt `kalender`.
        let wurzel = inhalt("heimat-ohne-projekt");
        std::fs::remove_dir_all(wurzel.join("projects/kalender")).unwrap();
        let (s, v) = schicke(&router_angemeldet(AppState::fuer_tests(&wurzel)), waehlen("mitte")).await;
        assert!(s == StatusCode::UNPROCESSABLE_ENTITY && v.to_string().contains("Projekt kalender"), "{s} {v}");
        let _ = std::fs::remove_dir_all(&wurzel);
    }
}
