//! Abgeleitete Werte aus einer Datei in `daten/` (Entscheidung,
//! `[daten_grundsatz]`, „schnitt“): Was sich aus den Eingaben von Hand
//! rechnen lässt, steht nie in TOML. Der Server rechnet es bei jeder Anfrage
//! aus genau einer Datei (ohne Zwischenspeicher; der kommt in Etappe 2d) und
//! liefert es neben den Eingaben aus.
//!
//! Für jede Liste von Einträgen (`[[zeile]]`, `[[teil.plan.balken]]`, auch
//! verschachtelt wie `[[spalte.karte]]`) unter ihrem Pfad:
//!
//! - `anzahl`: wie viele Einträge.
//! - `erledigt`: wie viele erledigt sind und welcher Anteil (0 bis 1). Nur,
//!   wenn das Status-Feld eindeutig ist (Regel bei [`erledigt_regel`]).
//! - `summe`: je Zahlenspalte (alle gesetzten Werte sind Zahlen) die Summe.
//! - `datum`: je Datumsspalte (alle gesetzten Werte sind ein Datum
//!   JJJJ-MM-TT, als Text oder als TOML-Datum) je Eintrag `tage_bis`
//!   (Datum minus heute, negativ heißt vergangen) und `tage_seit` (das
//!   Gegenstück), dazu der Index des nächsten Eintrags (heute oder später)
//!   und des letzten vergangenen (vor heute).
//! - `marken` (Etappe B1): je Spalte mit Farb- oder Statuswörtern die
//!   Bedeutung je Eintrag, für die Marke in einer Tabellenzelle.
//!
//! Kennzahlen über eine leere Liste ergeben 0 („0 %“, bei Tagen „–“), keinen
//! Fehler (Etappe B1); `anzahl` und `anteil` zählen mit `feld` und `wert`
//! jeden beliebigen Treffer.
//!
//! „Heute“ ist das lokale Datum des PCs (Zeitzone von Windows), nicht UTC.
//! Die Rechenfunktionen bekommen es als Eingabe ([`heute_von`]), damit Tests
//! den Tageswechsel nachstellen können.
//!
//! Dazu die Kennzahlen mit `berechnet` (siehe `daten::Berechnet`): dieselben
//! Rechnungen, fertig als deutscher Text („3 von 45“, „7 %“, „12 Tage“).

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, TimeZone};
use serde::Serialize;
use ts_rs::TS;

use super::bedeutung::Bedeutung;
use super::daten::{Berechnet, BerechnetArt, Datei, Kennzahlen, ist_datenname, ist_datum};
use super::lesen::{LeseFehler, zeile_mit_ab};

/// Was der Server aus einer Datei rechnet.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct Abgeleitet {
    /// Das Datum, mit dem gerechnet wurde: lokal, JJJJ-MM-TT.
    pub heute: String,
    /// Je Liste von Einträgen, unter ihrem Pfad (`zeile`, `teil.plan.balken`,
    /// `spalte.karte`). Verschachtelte Listen sind über alle Eltern
    /// zusammengefasst, in Reihenfolge der Datei.
    pub listen: BTreeMap<String, ListeAbgeleitet>,
    /// Nur bei `zeitstrahl` (Etappe 2e): die Einträge aller Teile als
    /// Ereignisse in der Form des Verlaufs (`vertrag::verlauf`), Quelle `hand`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub ereignisse: Option<Vec<super::verlauf::Ereignis>>,
    /// Nur bei `fristen` (Etappe B3): die Fristenliste des Projekts, aus
    /// `fristen.toml` und den offenen Aufgaben, der sichtbaren Frage und den
    /// Arbeiten mit Frist (`crate::fristen`), bewegliche aufgelöst.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fristen: Option<super::fristen::FristenListe>,
    /// Nur bei `fragen` (Etappe B2): welche Frage der Warteschlange sichtbar
    /// ist und wie viele dahinter warten, aus derselben Rechnung wie
    /// „Anstehend“ (`crate::anstehend::fragen_warteschlange`, mit Antworten und
    /// Einstufungen aus dem Datenspeicher).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub warteschlange: Option<super::anstehend::FragenAuswahl>,
}

/// Abgeleitetes einer Liste.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct ListeAbgeleitet {
    pub anzahl: u32,
    /// Nur, wenn das Status-Feld eindeutig ist.
    pub erledigt: Option<Erledigt>,
    /// Je Zahlenspalte die Summe der gesetzten Werte.
    pub summe: BTreeMap<String, f64>,
    /// Je Datumsspalte die Tage.
    pub datum: BTreeMap<String, DatumsSpalte>,
    /// Etappe B1, für die Marke in einer Tabellenzelle: je Spalte, deren
    /// gesetzte Werte alle Text sind und in der mindestens ein Farb- oder
    /// Statuswort steht („fertig“, „wartet“, „rot“ …), die Bedeutung je
    /// Eintrag; ein unbekanntes Wort ist `neutral` (nur dieser Eintrag),
    /// `null`, wo der Eintrag den Wert nicht hat. Die Wörter übersetzt nur
    /// der Server.
    pub marken: BTreeMap<String, Vec<Option<Bedeutung>>>,
}

/// Erledigte Einträge einer Liste.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct Erledigt {
    /// Nach welchem Feld gezählt wurde: `erledigt`, `status`, `zustand` oder `farbe`.
    pub feld: String,
    pub anzahl: u32,
    /// 0 bis 1; bei einer leeren Liste 0.
    pub anteil: f64,
}

/// Tage je Eintrag einer Datumsspalte. Die Listen sind so lang wie die
/// Liste der Einträge; `null`, wo der Eintrag kein Datum hat.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct DatumsSpalte {
    /// Datum minus heute in Tagen: positiv kommt noch, 0 ist heute, negativ ist vorbei.
    pub tage_bis: Vec<Option<i64>>,
    /// Heute minus Datum in Tagen (das Gegenstück zu `tage_bis`).
    pub tage_seit: Vec<Option<i64>>,
    /// Index des nächsten Eintrags (heute oder später); bei Gleichstand der erste.
    pub naechster: Option<u32>,
    /// Index des letzten vergangenen Eintrags (vor heute); bei Gleichstand der erste.
    pub letzter: Option<u32>,
}

/// Das lokale Datum zu einem Zeitpunkt in seiner Zeitzone. Im Betrieb mit
/// `chrono::Local::now()` (siehe [`heute`]); Tests geben eine feste Zeit.
pub fn heute_von<Tz: TimeZone>(jetzt: &DateTime<Tz>) -> NaiveDate {
    jetzt.date_naive()
}

/// Heute auf diesem PC (Zeitzone von Windows), nicht UTC.
pub fn heute() -> NaiveDate {
    heute_von(&chrono::Local::now())
}

/// Alles Abgeleitete einer Datei (ihr TOML als Tabelle).
pub fn ableiten(tabelle: &toml::Table, heute: NaiveDate) -> Abgeleitet {
    let listen = listen_von(tabelle).into_iter().map(|(pfad, eintraege)| (pfad, liste_ableiten(&eintraege, heute))).collect();
    Abgeleitet { heute: heute.format("%Y-%m-%d").to_string(), listen, ereignisse: None, warteschlange: None, fristen: None }
}

/// Alle Listen von Einträgen unter ihrem Pfad.
pub fn listen_von(tabelle: &toml::Table) -> BTreeMap<String, Vec<&toml::Table>> {
    let mut aus = BTreeMap::new();
    sammeln(tabelle, "", &mut aus);
    aus
}

fn sammeln<'a>(t: &'a toml::Table, pfad: &str, aus: &mut BTreeMap<String, Vec<&'a toml::Table>>) {
    for (k, v) in t {
        let p = if pfad.is_empty() { k.clone() } else { format!("{pfad}.{k}") };
        match v {
            toml::Value::Table(u) => sammeln(u, &p, aus),
            toml::Value::Array(a) if !a.is_empty() && a.iter().all(toml::Value::is_table) => {
                let eintraege: Vec<&toml::Table> = a.iter().filter_map(toml::Value::as_table).collect();
                aus.entry(p.clone()).or_default().extend(eintraege.iter().copied());
                for e in eintraege {
                    sammeln(e, &p, aus);
                }
            }
            _ => {}
        }
    }
}

/// Ein Datum JJJJ-MM-TT als Text oder ein TOML-Datum (auch mit Uhrzeit).
fn datum_von(v: &toml::Value) -> Option<NaiveDate> {
    match v {
        toml::Value::String(s) if ist_datum(s, false) => NaiveDate::parse_from_str(s, "%Y-%m-%d").ok(),
        toml::Value::Datetime(d) => {
            let t = d.date?;
            NaiveDate::from_ymd_opt(i32::from(t.year), u32::from(t.month), u32::from(t.day))
        }
        _ => None,
    }
}

/// Eine Zahl. TOML erlaubt `nan` und `inf`; die zählen nicht als Zahl (im
/// JSON würden sie `null`), `pruefe` meldet sie mit Zeile.
fn zahl_von(v: &toml::Value) -> Option<f64> {
    match v {
        toml::Value::Integer(i) => Some(*i as f64),
        toml::Value::Float(f) if f.is_finite() => Some(*f),
        _ => None,
    }
}

/// Spaltennamen in Reihenfolge des ersten Vorkommens.
fn spalten<'a>(eintraege: &[&'a toml::Table]) -> Vec<&'a str> {
    let mut namen: Vec<&str> = Vec::new();
    for e in eintraege {
        for k in e.keys() {
            if !namen.contains(&k.as_str()) {
                namen.push(k);
            }
        }
    }
    namen
}

/// Gesetzte Werte einer Spalte.
fn werte<'a>(eintraege: &'a [&'a toml::Table], spalte: &'a str) -> impl Iterator<Item = &'a toml::Value> + 'a {
    eintraege.iter().filter_map(move |e| e.get(spalte))
}

fn ist_zahlenspalte(eintraege: &[&toml::Table], spalte: &str) -> bool {
    werte(eintraege, spalte).all(|v| zahl_von(v).is_some())
}

fn ist_datumsspalte(eintraege: &[&toml::Table], spalte: &str) -> bool {
    werte(eintraege, spalte).all(|v| datum_von(v).is_some())
}

/// Die Regel für „erledigt“, nur wenn sie eindeutig ist:
///
/// 1. Von den Feldern `erledigt`, `status` und `zustand` kommt in der Liste
///    genau eines vor (in mindestens einem Eintrag). Kommen zwei oder drei
///    vor, ist es nicht eindeutig: kein `erledigt`.
///    - `erledigt`: alle gesetzten Werte sind `true` oder `false`; erledigt ist `true`.
///    - `status` oder `zustand`: alle gesetzten Werte sind Text; erledigt ist
///      „fertig“, „erledigt“ oder „ok“ (groß/klein egal).
/// 2. Kommt keines davon vor, zählt `farbe`: alle gesetzten Werte sind
///    Farb- oder Bedeutungswörter; erledigt ist die Bedeutung `ok` (also
///    auch „grün“, „fertig“, „erledigt“).
///
/// Ein Eintrag ohne das Feld gilt als nicht erledigt. Hat ein Wert die
/// falsche Art (etwa `erledigt = "ja"`), gibt es kein `erledigt`.
pub fn erledigt_regel(eintraege: &[&toml::Table]) -> Option<Erledigt> {
    let kommt_vor = |f: &str| eintraege.iter().any(|e| e.contains_key(f));
    let felder: Vec<&str> = ["erledigt", "status", "zustand"].into_iter().filter(|f| kommt_vor(f)).collect();
    let feld = match felder.as_slice() {
        [] if kommt_vor("farbe") => "farbe",
        [f] => *f,
        _ => return None,
    };
    let fertig = |v: &toml::Value| -> Option<bool> {
        match (feld, v) {
            ("erledigt", toml::Value::Boolean(b)) => Some(*b),
            ("status" | "zustand", toml::Value::String(s)) => Some(matches!(s.trim().to_lowercase().as_str(), "fertig" | "erledigt" | "ok")),
            ("farbe", toml::Value::String(s)) => Bedeutung::aus_wort(s).map(|b| b == Bedeutung::Ok),
            _ => None,
        }
    };
    let mut anzahl = 0u32;
    for e in eintraege {
        if let Some(v) = e.get(feld)
            && fertig(v)?
        {
            anzahl += 1;
        }
    }
    let gesamt = eintraege.len();
    let anteil = if gesamt == 0 { 0.0 } else { f64::from(anzahl) / gesamt as f64 };
    Some(Erledigt { feld: feld.to_string(), anzahl, anteil })
}

/// Abgeleitetes einer Liste.
pub fn liste_ableiten(eintraege: &[&toml::Table], heute: NaiveDate) -> ListeAbgeleitet {
    let mut summe = BTreeMap::new();
    let mut datum = BTreeMap::new();
    let mut marken = BTreeMap::new();
    for s in spalten(eintraege) {
        if ist_zahlenspalte(eintraege, s) {
            // Nur endliche Summen: läuft sie über (etwa 1e308 + 1e308), gibt es keine.
            let x: f64 = werte(eintraege, s).filter_map(zahl_von).sum();
            if x.is_finite() {
                summe.insert(s.to_string(), glatt(x));
            }
        } else if ist_datumsspalte(eintraege, s) {
            datum.insert(s.to_string(), datums_spalte(eintraege, s, heute));
        } else if ist_markenspalte(eintraege, s) {
            marken.insert(s.to_string(), eintraege.iter().map(|e| e.get(s).map(marke_von)).collect());
        }
    }
    ListeAbgeleitet { anzahl: eintraege.len() as u32, erledigt: erledigt_regel(eintraege), summe, datum, marken }
}

/// Eine Summe ohne Rundungsrest aus Kommazahlen (0,1 + 0,2 ist 0,3, nicht
/// 0,30000000000000004): auf neun Nachkommastellen gerundet. Ab 10^15
/// nicht mehr: dort gibt es keine Nachkommastellen zu glätten, und `x * 1e9`
/// liefe bei sehr großen Zahlen über.
fn glatt(x: f64) -> f64 {
    if !x.is_finite() || x.abs() >= 1e15 {
        return x;
    }
    let r = (x * 1e9).round() / 1e9;
    if r == 0.0 { 0.0 } else { r }
}

/// Bedeutung eines Farb- oder Statusworts (wie `farbe` in TOML); ein
/// unbekanntes Wort ist neutral (nur dieser Eintrag).
fn marke_von(v: &toml::Value) -> Bedeutung {
    v.as_str().and_then(Bedeutung::aus_wort).unwrap_or(Bedeutung::Neutral)
}

/// Eine Spalte mit Marken: alle gesetzten Werte sind Text, und mindestens
/// einer ist ein Farb- oder Statuswort.
fn ist_markenspalte(eintraege: &[&toml::Table], spalte: &str) -> bool {
    werte(eintraege, spalte).all(toml::Value::is_str) && werte(eintraege, spalte).any(|v| v.as_str().and_then(Bedeutung::aus_wort).is_some())
}

fn datums_spalte(eintraege: &[&toml::Table], spalte: &str, heute: NaiveDate) -> DatumsSpalte {
    let tage_bis: Vec<Option<i64>> = eintraege.iter().map(|e| e.get(spalte).and_then(datum_von).map(|d| (d - heute).num_days())).collect();
    let tage_seit = tage_bis.iter().map(|t| t.map(|t| -t)).collect();
    let mut naechster: Option<(usize, i64)> = None;
    let mut letzter: Option<(usize, i64)> = None;
    for (i, t) in tage_bis.iter().enumerate() {
        let Some(t) = *t else { continue };
        if t >= 0 && naechster.is_none_or(|(_, n)| t < n) {
            naechster = Some((i, t));
        }
        if t < 0 && letzter.is_none_or(|(_, l)| t > l) {
            letzter = Some((i, t));
        }
    }
    DatumsSpalte { tage_bis, tage_seit, naechster: naechster.map(|(i, _)| i as u32), letzter: letzter.map(|(i, _)| i as u32) }
}

// ----- Kennzahlen mit `berechnet` -----

/// Die Dateien, aus denen Kennzahlen rechnen dürfen: Name ohne `.toml` →
/// Inhalt als Tabelle, `None`, wenn die Datei da ist, aber selbst einen
/// Fehler hat. Fehlt der Name, gibt es die Datei nicht.
pub type Quellen = BTreeMap<String, Option<toml::Table>>;

/// Ein Fehler bei `berechnet`: welcher Schlüssel (für die Zeile) und der Satz.
type Mangel = (&'static str, String);

/// Rechnet eine Kennzahl und gibt den fertigen Text.
pub fn berechnen(b: &Berechnet, quellen: &Quellen, heute: NaiveDate) -> Result<String, Mangel> {
    let q = &b.quelle;
    let (stamm, teil) = match q.split_once('.') {
        Some((s, t)) => (s, Some(t)),
        None => (q.as_str(), None),
    };
    if !ist_datenname(stamm) {
        return Err((
            "quelle",
            format!(
                "Bei `quelle` steht „{q}“. Gemeint ist der Name einer Datei in `daten/` ohne `.toml`, bei Teilen mit Punkt, etwa `quelle = \"fortschritt\"` oder `quelle = \"zeitstrahl.plan\"`."
            ),
        ));
    }
    let tabelle = match quellen.get(stamm) {
        None => return Err(("quelle", format!("Die Quelle „{q}“ gibt es nicht: in `daten/` liegt keine Datei `{stamm}.toml`."))),
        Some(None) => {
            return Err(("quelle", format!("Die Quelle `{stamm}.toml` hat selbst einen Fehler. Erst den beheben; die Prüfung nennt ihn bei dieser Datei.")));
        }
        Some(Some(t)) => t,
    };
    let praefix = match teil {
        None => String::new(),
        Some(t) => {
            let teile = tabelle.get("teil").and_then(toml::Value::as_table);
            if !teile.is_some_and(|ts| ts.get(t).is_some_and(toml::Value::is_table)) {
                let da: Vec<&str> = teile.map(|ts| ts.keys().map(String::as_str).collect()).unwrap_or_default();
                let da = if da.is_empty() { "keine Teile".to_string() } else { format!("vorhanden: {}", da.join(", ")) };
                return Err(("quelle", format!("In `{stamm}.toml` gibt es den Teil „{t}“ nicht ({da}).")));
            }
            format!("teil.{t}.")
        }
    };
    let listen = listen_von(tabelle);
    let kandidaten: Vec<&str> = listen.keys().filter_map(|p| p.strip_prefix(praefix.as_str())).filter(|p| !p.is_empty()).collect();
    // Etappe B1: eine leere Liste ergibt 0 statt eines Fehlers (siehe [`leere_liste`]).
    let bereich = match teil {
        None => Some(tabelle),
        Some(t) => tabelle.get("teil").and_then(toml::Value::as_table).and_then(|ts| ts.get(t)).and_then(toml::Value::as_table),
    };
    let leer = |l: &str| bereich.is_some_and(|t| leere_liste(stamm, t, l));
    let liste = match b.liste.as_deref() {
        Some(l) => {
            if !kandidaten.contains(&l) {
                if leer(l) {
                    return leer_geprueft(b, stamm, l);
                }
                return Err(("liste", format!("In „{q}“ gibt es die Liste „{l}“ nicht. {}", vorhanden("Listen", &kandidaten))));
            }
            l
        }
        None => match kandidaten.as_slice() {
            [l] => l,
            [] => {
                if let Some(l) = bereich.and_then(|t| einzige_leere_liste(stamm, t)) {
                    return leer_geprueft(b, stamm, &l);
                }
                return Err(("quelle", format!("In „{q}“ steht keine Liste von Einträgen (Tabellen-Array wie `[[zeile]]`).")));
            }
            [erste, ..] => {
                return Err((
                    "quelle",
                    format!("In „{q}“ gibt es mehrere Listen ({}). Bitte mit `liste` eine wählen, etwa `liste = \"{erste}\"`.", kandidaten.join(", ")),
                ));
            }
        },
    };
    let eintraege = &listen[&format!("{praefix}{liste}")];
    let abgeleitet = liste_ableiten(eintraege, heute);
    let feld = || -> Result<&str, Mangel> {
        let f = b
            .feld
            .as_deref()
            .filter(|f| !f.trim().is_empty())
            .ok_or_else(|| ("art", format!("`art = \"{}\"` braucht auch `feld`, die Spalte, aus der gerechnet wird.", b.art.wort())))?;
        let da = spalten(eintraege);
        if !da.contains(&f) {
            return Err(("feld", format!("In der Liste „{liste}“ gibt es die Spalte „{f}“ nicht. {}", vorhanden("Spalten", &da))));
        }
        Ok(f)
    };
    if eintraege.is_empty() {
        return leer_geprueft(b, stamm, liste);
    }
    // Etappe B1: `anzahl` und `anteil` mit eigenem Treffer (`feld` und `wert`).
    if let (BerechnetArt::Anzahl | BerechnetArt::Anteil, Some(w)) = (b.art, &b.wert) {
        let f = feld()?;
        let treffer = eintraege.iter().filter(|e| e.get(f).is_some_and(|v| w.passt(v))).count();
        let gesamt = eintraege.len();
        return Ok(if b.art == BerechnetArt::Anzahl {
            format!("{treffer} von {gesamt}")
        } else {
            format!("{} %", (treffer as f64 / gesamt as f64 * 100.0).round() as i64)
        });
    }
    Ok(match b.art {
        BerechnetArt::Anzahl => match &abgeleitet.erledigt {
            Some(e) => format!("{} von {}", e.anzahl, abgeleitet.anzahl),
            None => abgeleitet.anzahl.to_string(),
        },
        BerechnetArt::Anteil => {
            let e = abgeleitet.erledigt.as_ref().ok_or_else(|| {
                (
                    "art",
                    format!(
                        "Die Liste „{liste}“ hat kein eindeutiges Status-Feld, darum geht `art = \"anteil\"` nicht. Erkannt wird genau eines von `erledigt = true`, `status` oder `zustand` (fertig, erledigt, ok), sonst `farbe`."
                    ),
                )
            })?;
            format!("{} %", (e.anteil * 100.0).round() as i64)
        }
        BerechnetArt::Summe => {
            let f = feld()?;
            let s = abgeleitet.summe.get(f).ok_or_else(|| {
                ("feld", format!("Die Spalte „{f}“ in der Liste „{liste}“ ist keine Zahlenspalte. Für `summe` stehen dort Zahlen, etwa `{f} = 120`."))
            })?;
            zahl_deutsch(*s)
        }
        BerechnetArt::TageBis | BerechnetArt::TageSeit => {
            let f = feld()?;
            let d = abgeleitet.datum.get(f).ok_or_else(|| {
                (
                    "feld",
                    format!(
                        "Die Spalte „{f}“ in der Liste „{liste}“ ist keine Datumsspalte. Für `{}` stehen dort Daten, etwa `{f} = \"2026-11-02\"`.",
                        b.art.wort()
                    ),
                )
            })?;
            let tage =
                if b.art == BerechnetArt::TageBis { d.naechster.and_then(|i| d.tage_bis[i as usize]) } else { d.letzter.and_then(|i| d.tage_seit[i as usize]) };
            tage.map(tage_wort).unwrap_or_else(|| "–".to_string())
        }
    })
}

/// Die Listen der Baustein-Dateien, die es auch leer gibt (Etappe B1): Eine
/// `fragen.toml` ohne `[[frage]]` hat eine leere Liste `frage`, keinen Fehler.
fn feste_listen(stamm: &str) -> &'static [&'static str] {
    match stamm {
        "kennzahlen" => &["kennzahl"],
        "ampel" | "fortschritt" => &["zeile"],
        "zeitstrahl" => &["eintrag"],
        "gantt" => &["balken"],
        "kanban" => &["spalte", "spalte.karte"],
        "fluss" => &["kasten", "pfeil"],
        "fragen" => &["frage"],
        "orte" => &["ort"],
        "fristen" => &["ereignis", "frist"],
        _ => &[],
    }
}

/// Ist `liste` in `bereich` (die Datei oder ein Teil) eine leere Liste?
/// Ja, wenn dort ein leeres Array steht (`zeile = []`) oder wenn die Datei
/// ein Baustein ist, der diese Liste kennt ([`feste_listen`]).
fn leere_liste(stamm: &str, bereich: &toml::Table, liste: &str) -> bool {
    if feste_listen(stamm).contains(&liste) {
        return true;
    }
    let mut t = bereich;
    let mut teile = liste.split('.').peekable();
    while let Some(k) = teile.next() {
        match (t.get(k), teile.peek().is_none()) {
            (Some(toml::Value::Array(a)), true) => return a.is_empty(),
            (Some(toml::Value::Table(u)), false) => t = u,
            _ => return false,
        }
    }
    false
}

/// Ohne `liste` und ohne Einträge: die eine Liste, die leer sein darf (die
/// einzige feste Liste des Bausteins oder das einzige leere Array).
fn einzige_leere_liste(stamm: &str, bereich: &toml::Table) -> Option<String> {
    match feste_listen(stamm) {
        [l] => return Some((*l).to_string()),
        [] => {}
        _ => return None,
    }
    let leere: Vec<&String> = bereich.iter().filter(|(_, v)| v.as_array().is_some_and(Vec::is_empty)).map(|(k, _)| k).collect();
    match leere.as_slice() {
        [l] => Some((*l).clone()),
        _ => None,
    }
}

/// Die Spalten, die eine Liste eines Bausteins haben darf (aus dem Typ des
/// Eintrags); `None` bei eigenen Dateien, deren Schema der Server nicht kennt.
fn schema(stamm: &str, liste: &str) -> Option<Vec<String>> {
    use super::daten::{
        AmpelZeile, FlussKasten, FlussPfeil, FortschrittZeile, Frage, GanttBalken, KanbanKarte, KanbanSpalte, Kennzahl, Ort, ZeitstrahlEintrag, feldnamen,
    };
    Some(match (stamm, liste) {
        ("kennzahlen", "kennzahl") => feldnamen::<Kennzahl>(),
        ("ampel", "zeile") => feldnamen::<AmpelZeile>(),
        ("fortschritt", "zeile") => feldnamen::<FortschrittZeile>(),
        ("zeitstrahl", "eintrag") => feldnamen::<ZeitstrahlEintrag>(),
        ("gantt", "balken") => feldnamen::<GanttBalken>(),
        ("kanban", "spalte") => feldnamen::<KanbanSpalte>(),
        ("kanban", "spalte.karte") => feldnamen::<KanbanKarte>(),
        ("fluss", "kasten") => feldnamen::<FlussKasten>(),
        ("fluss", "pfeil") => feldnamen::<FlussPfeil>(),
        ("fragen", "frage") => feldnamen::<Frage>(),
        ("orte", "ort") => feldnamen::<Ort>(),
        ("fristen", "ereignis") => feldnamen::<super::fristen::FristEreignis>(),
        ("fristen", "frist") => feldnamen::<super::fristen::Frist>(),
        _ => return None,
    })
}

/// Eine leere Liste ergibt 0 (siehe [`leer_ergebnis`]). Kennt der Server
/// das Schema der Liste (Baustein-Datei), muss `feld` trotzdem eine ihrer
/// Spalten sein: ein Tippfehler fiele sonst erst mit dem ersten Eintrag auf.
fn leer_geprueft(b: &Berechnet, stamm: &str, liste: &str) -> Result<String, Mangel> {
    if let (Some(f), Some(da)) = (b.feld.as_deref().map(str::trim).filter(|f| !f.is_empty()), schema(stamm, liste))
        && !da.iter().any(|d| d == f)
    {
        let da: Vec<&str> = da.iter().map(String::as_str).collect();
        return Err(("feld", format!("In der Liste „{liste}“ gibt es die Spalte „{f}“ nicht (die Liste ist noch leer). Erlaubt sind: {}.", da.join(", "))));
    }
    Ok(leer_ergebnis(b.art))
}

/// Was eine Kennzahl über eine leere Liste zeigt: 0, „0 %“, bei Tagen „–“.
fn leer_ergebnis(art: BerechnetArt) -> String {
    match art {
        BerechnetArt::Anzahl | BerechnetArt::Summe => "0".to_string(),
        BerechnetArt::Anteil => "0 %".to_string(),
        BerechnetArt::TageBis | BerechnetArt::TageSeit => "–".to_string(),
    }
}

fn vorhanden(was: &str, namen: &[&str]) -> String {
    if namen.is_empty() { format!("Es gibt keine {was}.") } else { format!("Vorhanden: {}.", namen.join(", ")) }
}

/// „heute“, „1 Tag“, „12 Tage“.
pub fn tage_wort(n: i64) -> String {
    match n {
        0 => "heute".to_string(),
        1 | -1 => format!("{n} Tag"),
        _ => format!("{n} Tage"),
    }
}

/// Eine Zahl auf Deutsch: Punkt als Tausender-Trenner, Komma vor den
/// Nachkommastellen, höchstens zwei davon („1.234,5“, „-3“, „0,25“).
pub fn zahl_deutsch(x: f64) -> String {
    if !x.is_finite() {
        return x.to_string();
    }
    let hundertstel = (x.abs() * 100.0).round() as u128;
    let (ganz, rest) = (hundertstel / 100, hundertstel % 100);
    let ziffern = ganz.to_string();
    let mut s = String::new();
    for (i, c) in ziffern.chars().enumerate() {
        if i > 0 && (ziffern.len() - i) % 3 == 0 {
            s.push('.');
        }
        s.push(c);
    }
    if rest > 0 {
        if rest % 10 == 0 {
            s.push_str(&format!(",{}", rest / 10));
        } else {
            s.push_str(&format!(",{rest:02}"));
        }
    }
    if x < 0.0 && hundertstel > 0 {
        s.insert(0, '-');
    }
    s
}

/// Setzt bei allen Kennzahlen mit `berechnet` den fertigen `wert` ein.
/// `datei` ist der Pfad für Meldungen, `quelltext` der Text der Datei (für
/// die Zeilen). Gibt alle Fehler zurück, jeweils mit Zeile; Kennzahlen mit
/// Fehler behalten `wert = null`.
pub fn kennzahlen_rechnen(kennzahlen: &mut Datei<Kennzahlen>, quellen: &Quellen, heute: NaiveDate, datei: &str, quelltext: &str) -> Vec<LeseFehler> {
    let quelltext = quelltext.strip_prefix('\u{feff}').unwrap_or(quelltext);
    let mut fehler = Vec::new();
    let mut ab = 0;
    for k in kennzahlen.inhalte_mut().into_iter().flat_map(|t| t.kennzahl.iter_mut()) {
        let titel_zeile = zeile_mit_ab(quelltext, "titel", &k.titel, ab);
        if let Some(z) = titel_zeile {
            ab = z;
        }
        let Some(b) = &k.berechnet else { continue };
        match berechnen(b, quellen, heute) {
            Ok(w) => k.wert = Some(w),
            Err((schluessel, satz)) => {
                let zeile = titel_zeile.map(|z| schluessel_im_eintrag(quelltext, z, schluessel).unwrap_or(z));
                fehler.push(LeseFehler { datei: datei.to_string(), zeile, text: format!("Kennzahl „{}“: {satz}", k.titel) });
            }
        }
    }
    fehler
}

/// Zeile (ab 1) mit `schluessel = …` im selben Eintrag wie Zeile `zeile`,
/// auch in einer Inline-Tabelle (`berechnet = { quelle = … }`). Ein Eintrag
/// reicht von einer Zeile `[[…]]` bis vor die nächste.
fn schluessel_im_eintrag(text: &str, zeile: usize, schluessel: &str) -> Option<usize> {
    let zeilen: Vec<&str> = text.lines().collect();
    let kopf = |l: &&str| l.trim_start().starts_with("[[");
    let z = zeile.min(zeilen.len());
    let von = zeilen[..z].iter().rposition(&kopf).unwrap_or(0);
    let bis = zeilen[z..].iter().position(kopf).map(|i| i + z).unwrap_or(zeilen.len());
    zeilen[von..bis].iter().position(|l| hat_schluessel(l, schluessel)).map(|i| von + i + 1)
}

/// Steht in der Zeile `schluessel =` (als ganzes Wort, vor dem Kommentar)?
fn hat_schluessel(zeile: &str, schluessel: &str) -> bool {
    let zeile = zeile.split('#').next().unwrap_or("");
    zeile.match_indices(schluessel).any(|(i, _)| {
        let davor = zeile[..i].chars().next_back();
        let danach = zeile[i + schluessel.len()..].trim_start();
        !davor.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '-') && danach.starts_with('=') && !danach.starts_with("==")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vertrag::daten::datei_lesen;
    use crate::vertrag::lesen::toml_lesen;

    fn tabelle(text: &str) -> toml::Table {
        toml_lesen("t", text).unwrap()
    }

    fn tag(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    const SCHRITTE: &str = r#"
[[schritt]]
name = "Kompost umsetzen"
termin = "2026-10-20"
kosten = 120
erledigt = true

[[schritt]]
name = "Saatgut bestellen"
termin = 2026-10-30
kosten = 80.5
erledigt = false

[[schritt]]
name = "Pflanzen setzen"
termin = "2026-11-15"

[[schritt]]
name = "Beete mulchen"
termin = "2026-10-25"
kosten = 1000
erledigt = true
"#;

    #[test]
    fn liste_mit_anzahl_erledigt_summe_und_tagen() {
        let a = ableiten(&tabelle(SCHRITTE), tag("2026-10-27"));
        assert_eq!(a.heute, "2026-10-27");
        assert_eq!(a.listen.keys().collect::<Vec<_>>(), ["schritt"]);
        let l = &a.listen["schritt"];
        assert_eq!(l.anzahl, 4);
        let e = l.erledigt.as_ref().unwrap();
        assert_eq!((e.feld.as_str(), e.anzahl, e.anteil), ("erledigt", 2, 0.5));
        assert_eq!(l.summe.keys().collect::<Vec<_>>(), ["kosten"], "name und termin sind keine Zahlen");
        assert_eq!(l.summe["kosten"], 1200.5);
        let d = &l.datum["termin"];
        // Text und TOML-Datum gehen beide.
        assert_eq!(d.tage_bis, [Some(-7), Some(3), Some(19), Some(-2)]);
        assert_eq!(d.tage_seit, [Some(7), Some(-3), Some(-19), Some(2)]);
        assert_eq!(d.naechster, Some(1), "Saatgut bestellen ist am nächsten");
        assert_eq!(d.letzter, Some(3), "Beete mulchen liegt am kürzesten zurück");
        assert!(!l.datum.contains_key("name"));
        let json = serde_json::to_value(&a).unwrap();
        assert_eq!(json["listen"]["schritt"]["datum"]["termin"]["tage_bis"][2], 19);
        assert_eq!(json["listen"]["schritt"]["erledigt"]["anteil"], 0.5);
    }

    #[test]
    fn heute_zaehlt_als_naechster_und_gemischte_spalten_zaehlen_nicht() {
        let t = tabelle("[[e]]\nd = \"2026-10-27\"\nx = 1\n[[e]]\nd = \"2026-10\"\nx = \"zwei\"\n[[f]]\nd = \"2026-10-27\"\n");
        let a = ableiten(&t, tag("2026-10-27"));
        assert!(a.listen["e"].datum.is_empty(), "JJJJ-MM ist kein Tag: keine Datumsspalte");
        assert!(a.listen["e"].summe.is_empty(), "Zahl und Text gemischt: keine Zahlenspalte");
        let d = &a.listen["f"].datum["d"];
        assert_eq!((d.tage_bis[0], d.naechster, d.letzter), (Some(0), Some(0), None));
    }

    /// Lokal 23:59 und 00:01 (Mitteleuropa, Winterzeit) sind zwei Tage. Um
    /// 00:01 ist in UTC noch der Vortag; wer UTC nimmt, rechnet falsch.
    #[test]
    fn tageswechsel_nach_lokaler_zeit() {
        let mez = chrono::FixedOffset::east_opt(3600).unwrap();
        let vor = mez.with_ymd_and_hms(2026, 10, 27, 23, 59, 0).unwrap();
        let nach = mez.with_ymd_and_hms(2026, 10, 28, 0, 1, 0).unwrap();
        let t = tabelle("[[termin]]\ndatum = \"2026-10-30\"\n");
        let bis = |h: NaiveDate| ableiten(&t, h).listen["termin"].datum["datum"].tage_bis[0];
        assert_eq!(bis(heute_von(&vor)), Some(3));
        assert_eq!(bis(heute_von(&nach)), Some(2));
        let utc = heute_von(&nach.with_timezone(&chrono::Utc));
        assert_eq!(utc, tag("2026-10-27"), "in UTC ist es noch der 27.");
        assert_ne!(bis(utc), bis(heute_von(&nach)), "UTC gäbe den falschen Tag");
        // Im Betrieb: das lokale Datum des PCs.
        assert_eq!(heute(), chrono::Local::now().date_naive());
    }

    #[test]
    fn teile_und_verschachtelte_listen() {
        let t = tabelle(
            "[teil.plan]\n[[teil.plan.balken]]\nname = \"A\"\nvon = 0\nbis = 2\n[[teil.plan.balken]]\nname = \"B\"\nvon = 2\nbis = 5\n\
             [teil.ist]\n[[teil.ist.balken]]\nname = \"A\"\nvon = 0\nbis = 3\n\
             [[spalte]]\ntitel = \"Offen\"\n[[spalte.karte]]\ntitel = \"x\"\n[[spalte]]\ntitel = \"Fertig\"\n[[spalte.karte]]\ntitel = \"y\"\n[[spalte.karte]]\ntitel = \"z\"\n",
        );
        let a = ableiten(&t, tag("2026-10-27"));
        assert_eq!(a.listen.keys().collect::<Vec<_>>(), ["spalte", "spalte.karte", "teil.ist.balken", "teil.plan.balken"]);
        assert_eq!(a.listen["teil.plan.balken"].summe["bis"], 7.0);
        assert_eq!(a.listen["spalte"].anzahl, 2);
        assert_eq!(a.listen["spalte.karte"].anzahl, 3);
        // Leere Listen und Listen von Texten sind keine Einträge.
        assert!(ableiten(&tabelle("optionen = [\"a\", \"b\"]\nleer = []\n"), tag("2026-10-27")).listen.is_empty());
    }

    #[test]
    fn erledigt_nur_wenn_eindeutig() {
        let regel = |text: &str| {
            let t = tabelle(text);
            let l = listen_von(&t);
            erledigt_regel(&l["e"]).map(|e| (e.feld, e.anzahl))
        };
        assert_eq!(regel("[[e]]\nstatus = \"Fertig\"\n[[e]]\nstatus = \"offen\"\n[[e]]\nstatus = \"ok\"\n"), Some(("status".into(), 2)));
        assert_eq!(regel("[[e]]\nzustand = \"erledigt\"\n[[e]]\n"), Some(("zustand".into(), 1)));
        assert_eq!(regel("[[e]]\nfarbe = \"grün\"\n[[e]]\nfarbe = \"rot\"\n[[e]]\nfarbe = \"fertig\"\n"), Some(("farbe".into(), 2)));
        // Zwei Status-Felder: nicht eindeutig. `erledigt` als Text: falsche Art.
        assert_eq!(regel("[[e]]\nstatus = \"fertig\"\n[[e]]\nerledigt = true\n"), None);
        assert_eq!(regel("[[e]]\nerledigt = \"ja\"\n"), None);
        assert_eq!(regel("[[e]]\nfarbe = \"lila\"\n"), None);
        assert_eq!(regel("[[e]]\nname = \"a\"\n"), None);
        // Mit Status-Feld zählt farbe nicht.
        assert_eq!(regel("[[e]]\nerledigt = false\nfarbe = \"grün\"\n"), Some(("erledigt".into(), 0)));
    }

    #[test]
    fn zahlen_und_tage_auf_deutsch() {
        assert_eq!(zahl_deutsch(1200.5), "1.200,5");
        assert_eq!(zahl_deutsch(3.0), "3");
        assert_eq!(zahl_deutsch(0.25), "0,25");
        assert_eq!(zahl_deutsch(-1234567.0), "-1.234.567");
        assert_eq!(zahl_deutsch(0.999), "1");
        assert_eq!(zahl_deutsch(-0.001), "0");
        assert_eq!(tage_wort(0), "heute");
        assert_eq!(tage_wort(1), "1 Tag");
        assert_eq!(tage_wort(12), "12 Tage");
    }

    fn quellen() -> Quellen {
        let mut q = Quellen::new();
        q.insert("eigen-schritte".into(), Some(tabelle(SCHRITTE)));
        q.insert(
            "zeitstrahl".into(),
            Some(tabelle("[teil.plan]\n[[teil.plan.eintrag]]\ndatum = \"2026-11-06\"\ntitel = \"A\"\n[teil.alt]\n[[teil.alt.eintrag]]\ndatum = \"2026-10-01\"\ntitel = \"B\"\n")),
        );
        q.insert("gantt".into(), None);
        q
    }

    fn rechne(b: &str) -> Result<String, Mangel> {
        let b: Berechnet = toml_lesen("t", b).unwrap();
        berechnen(&b, &quellen(), tag("2026-10-27"))
    }

    #[test]
    fn berechnet_je_art() {
        assert_eq!(rechne("quelle = \"eigen-schritte\"\nart = \"anzahl\"").unwrap(), "2 von 4");
        assert_eq!(rechne("quelle = \"eigen-schritte\"\nliste = \"schritt\"\nart = \"anteil\"").unwrap(), "50 %");
        assert_eq!(rechne("quelle = \"eigen-schritte\"\nart = \"summe\"\nfeld = \"kosten\"").unwrap(), "1.200,5");
        assert_eq!(rechne("quelle = \"eigen-schritte\"\nart = \"tage_bis\"\nfeld = \"termin\"").unwrap(), "3 Tage");
        assert_eq!(rechne("quelle = \"eigen-schritte\"\nart = \"tage_seit\"\nfeld = \"termin\"").unwrap(), "2 Tage");
        // Ohne Status-Feld ist anzahl nur die Zahl; Teile über den Punkt.
        assert_eq!(rechne("quelle = \"zeitstrahl.plan\"\nart = \"anzahl\"").unwrap(), "1");
        assert_eq!(rechne("quelle = \"zeitstrahl.plan\"\nart = \"tage_bis\"\nfeld = \"datum\"").unwrap(), "10 Tage");
        assert_eq!(rechne("quelle = \"zeitstrahl.plan\"\nart = \"tage_seit\"\nfeld = \"datum\"").unwrap(), "–", "nichts vorbei");
        assert_eq!(rechne("quelle = \"zeitstrahl\"\nliste = \"teil.alt.eintrag\"\nart = \"tage_seit\"\nfeld = \"datum\"").unwrap(), "26 Tage");
    }

    #[test]
    fn berechnet_fehler_mit_satz() {
        let fehler = |b: &str| rechne(b).unwrap_err();
        let (k, s) = fehler("quelle = \"schritte\"\nart = \"anzahl\"");
        assert_eq!(k, "quelle");
        assert!(s.starts_with("Bei `quelle` steht „schritte“. Gemeint ist der Name einer Datei"), "{s}");
        let (k, s) = fehler("quelle = \"kanban\"\nart = \"anzahl\"");
        assert_eq!((k, s.as_str()), ("quelle", "Die Quelle „kanban“ gibt es nicht: in `daten/` liegt keine Datei `kanban.toml`."));
        assert!(fehler("quelle = \"gantt\"\nart = \"anzahl\"").1.contains("hat selbst einen Fehler"));
        assert_eq!(
            fehler("quelle = \"zeitstrahl.zukunft\"\nart = \"anzahl\"").1,
            "In `zeitstrahl.toml` gibt es den Teil „zukunft“ nicht (vorhanden: plan, alt)."
        );
        let (k, s) = fehler("quelle = \"zeitstrahl\"\nart = \"anzahl\"");
        assert_eq!(k, "quelle");
        assert!(s.contains("mehrere Listen (teil.alt.eintrag, teil.plan.eintrag)"), "{s}");
        let (k, s) = fehler("quelle = \"eigen-schritte\"\nliste = \"schritte\"\nart = \"anzahl\"");
        assert_eq!((k, s.as_str()), ("liste", "In „eigen-schritte“ gibt es die Liste „schritte“ nicht. Vorhanden: schritt."));
        let (k, s) = fehler("quelle = \"eigen-schritte\"\nart = \"summe\"\nfeld = \"preis\"");
        assert_eq!(k, "feld");
        assert_eq!(s, "In der Liste „schritt“ gibt es die Spalte „preis“ nicht. Vorhanden: name, termin, kosten, erledigt.");
        let (k, s) = fehler("quelle = \"eigen-schritte\"\nart = \"summe\"\nfeld = \"termin\"");
        assert_eq!(k, "feld");
        assert!(s.contains("„termin“ in der Liste „schritt“ ist keine Zahlenspalte"), "{s}");
        let (k, s) = fehler("quelle = \"eigen-schritte\"\nart = \"tage_bis\"\nfeld = \"kosten\"");
        assert_eq!(k, "feld");
        assert!(s.contains("ist keine Datumsspalte. Für `tage_bis`"), "{s}");
        let (k, s) = fehler("quelle = \"zeitstrahl.plan\"\nart = \"anteil\"");
        assert_eq!(k, "art");
        assert!(s.contains("kein eindeutiges Status-Feld"), "{s}");
    }

    #[test]
    fn kennzahlen_bekommen_wert_und_fehler_mit_zeile() {
        let text = "[[kennzahl]]\ntitel = \"Schritte\"\nberechnet = { quelle = \"eigen-schritte\", art = \"anzahl\" }\n\n\
                    [[kennzahl]]\ntitel = \"Kompost\"\nwert = \"45 kg\"\n\n\
                    [[kennzahl]]\ntitel = \"Kosten\"\n[kennzahl.berechnet]\nquelle = \"eigen-schritte\"\nart = \"summe\"\nfeld = \"preis\"\n";
        let mut k = datei_lesen::<Kennzahlen>("projects/x/daten/kennzahlen.toml", text).unwrap();
        let f = kennzahlen_rechnen(&mut k, &quellen(), tag("2026-10-27"), "projects/x/daten/kennzahlen.toml", text);
        assert_eq!(f.len(), 1, "{f:?}");
        assert_eq!(
            f[0].to_string(),
            "projects/x/daten/kennzahlen.toml, Zeile 14: Kennzahl „Kosten“: In der Liste „schritt“ gibt es die Spalte „preis“ nicht. Vorhanden: name, termin, kosten, erledigt."
        );
        let Datei::Ganz(k) = k else { panic!() };
        assert_eq!(k.kennzahl[0].wert.as_deref(), Some("2 von 4"));
        assert_eq!(k.kennzahl[1].wert.as_deref(), Some("45 kg"));
        assert_eq!(k.kennzahl[2].wert, None);
        let json = serde_json::to_value(&k).unwrap();
        assert_eq!(json["kennzahl"][0]["berechnet"]["art"], "anzahl", "berechnet bleibt zur Erklärung");
        assert!(json["kennzahl"][1].get("berechnet").is_none());
        // Inline-Tabelle: die Zeile mit `quelle` ist die von `berechnet`.
        let text2 = "[[kennzahl]]\ntitel = \"A\"\nberechnet = { quelle = \"kanban\", art = \"anzahl\" }\n";
        let mut k = datei_lesen::<Kennzahlen>("k", text2).unwrap();
        let f = kennzahlen_rechnen(&mut k, &quellen(), tag("2026-10-27"), "k", text2);
        assert_eq!(f[0].zeile, Some(3));
    }

    /// Etappe B1: eine leere Liste ergibt 0 (bei Tagen „–“) statt eines Fehlers.
    #[test]
    fn leere_liste_ergibt_null() {
        let mut q = Quellen::new();
        q.insert("fragen".into(), Some(tabelle("# noch keine Fragen\n")));
        q.insert("kanban".into(), Some(tabelle("[[spalte]]\ntitel = \"Offen\"\n")));
        q.insert("eigen-kosten".into(), Some(tabelle("stand = \"2026-10-01\"\nposten = []\n")));
        q.insert("eigen-teile".into(), Some(tabelle("[teil.alt]\nzeile = []\n[teil.neu]\n[[teil.neu.zeile]]\nx = 1\n")));
        q.insert("eigen-nichts".into(), Some(tabelle("titel = \"ohne Listen\"\n")));
        let r = |b: &str| {
            let b: Berechnet = toml_lesen("t", b).unwrap();
            berechnen(&b, &q, tag("2026-10-27"))
        };
        assert_eq!(r("quelle = \"fragen\"\nliste = \"frage\"\nart = \"anzahl\"").unwrap(), "0");
        assert_eq!(r("quelle = \"fragen\"\nart = \"anzahl\"").unwrap(), "0", "fragen hat genau eine Liste");
        assert_eq!(r("quelle = \"fragen\"\nart = \"anteil\"").unwrap(), "0 %");
        assert_eq!(r("quelle = \"kanban\"\nliste = \"spalte.karte\"\nart = \"anzahl\"").unwrap(), "0");
        assert_eq!(r("quelle = \"eigen-kosten\"\nliste = \"posten\"\nart = \"summe\"\nfeld = \"betrag\"").unwrap(), "0");
        assert_eq!(r("quelle = \"eigen-kosten\"\nart = \"tage_bis\"\nfeld = \"datum\"").unwrap(), "–");
        assert_eq!(r("quelle = \"eigen-kosten\"\nart = \"anzahl\"\nfeld = \"x\"\nwert = 1").unwrap(), "0");
        assert_eq!(r("quelle = \"eigen-teile.alt\"\nart = \"anzahl\"").unwrap(), "0");
        assert_eq!(r("quelle = \"eigen-teile.neu\"\nart = \"anzahl\"").unwrap(), "1");
        // Tippfehler bleiben Fehler: eine Liste, die es auch leer nicht gibt.
        let (k, s) = r("quelle = \"fragen\"\nliste = \"fragen\"\nart = \"anzahl\"").unwrap_err();
        assert_eq!((k, s.as_str()), ("liste", "In „fragen“ gibt es die Liste „fragen“ nicht. Es gibt keine Listen."));
        assert_eq!(r("quelle = \"eigen-kosten\"\nliste = \"stand\"\nart = \"anzahl\"").unwrap_err().0, "liste");
        assert!(r("quelle = \"eigen-nichts\"\nart = \"anzahl\"").unwrap_err().1.contains("steht keine Liste von Einträgen"));
        assert!(r("quelle = \"kanban\"\nart = \"anzahl\"").is_ok(), "spalte ist die einzige Liste mit Einträgen");
    }

    /// Etappe B1: `anzahl` und `anteil` mit beliebigem Feld und Wert.
    #[test]
    fn anteil_mit_feld_und_wert() {
        let mut q = Quellen::new();
        q.insert(
            "eigen-seiten".into(),
            Some(tabelle(
                "[[seite]]\nname = \"Start\"\nfreigabe = \"Freigegeben\"\nsprachen = 2\nerledigt = true\n\
                 [[seite]]\nname = \"Preise\"\nfreigabe = \"entwurf\"\nsprachen = 1\nerledigt = false\n\
                 [[seite]]\nname = \"Kontakt\"\nfreigabe = \" freigegeben \"\nsprachen = 2.0\n\
                 [[seite]]\nname = \"Blog\"\n",
            )),
        );
        let r = |b: &str| {
            let b: Berechnet = toml_lesen("t", b).unwrap();
            berechnen(&b, &q, tag("2026-10-27"))
        };
        let s = "quelle = \"eigen-seiten\"\n";
        assert_eq!(r(&format!("{s}art = \"anzahl\"\nfeld = \"freigabe\"\nwert = \"freigegeben\"")).unwrap(), "2 von 4", "groß/klein und Rand egal");
        assert_eq!(r(&format!("{s}art = \"anteil\"\nfeld = \"freigabe\"\nwert = \"freigegeben\"")).unwrap(), "50 %");
        assert_eq!(r(&format!("{s}art = \"anteil\"\nfeld = \"sprachen\"\nwert = 2")).unwrap(), "50 %", "2 und 2.0 sind gleich");
        assert_eq!(r(&format!("{s}art = \"anzahl\"\nfeld = \"erledigt\"\nwert = true")).unwrap(), "1 von 4");
        assert_eq!(r(&format!("{s}art = \"anzahl\"\nfeld = \"erledigt\"\nwert = \"true\"")).unwrap(), "0 von 4", "Text ist nicht wahr/falsch");
        let (k, satz) = r(&format!("{s}art = \"anteil\"\nfeld = \"status\"\nwert = \"ok\"")).unwrap_err();
        assert_eq!(k, "feld");
        assert_eq!(satz, "In der Liste „seite“ gibt es die Spalte „status“ nicht. Vorhanden: name, freigabe, sprachen, erledigt.");
        // Ohne `wert` gilt weiter das Status-Feld.
        assert_eq!(r(&format!("{s}art = \"anteil\"")).unwrap(), "25 %", "erledigt = true: 1 von 4");
        let json = serde_json::to_value(toml_lesen::<Berechnet>("t", "quelle = \"q\"\nart = \"anteil\"\nfeld = \"f\"\nwert = 2").unwrap()).unwrap();
        assert_eq!(json["wert"], 2.0);
    }

    /// Etappe B1: Marken (Bedeutung je Eintrag) und Summen ohne Rundungsrest.
    #[test]
    fn marken_und_glatte_summen() {
        let t = tabelle(
            "[[posten]]\nname = \"Miete\"\nbetrag = 0.1\nstand = \"bezahlt\"\nampel = \"Fertig\"\n\
             [[posten]]\nname = \"Strom\"\nbetrag = 0.2\nstand = \"offen\"\n\
             [[posten]]\nname = \"Gas\"\nbetrag = 1\nampel = \"rot\"\nnummer = 7\n\
             [[posten]]\nname = \"Wasser\"\nbetrag = 0\nnummer = \"sieben\"\n",
        );
        let l = &ableiten(&t, tag("2026-10-27")).listen["posten"];
        assert_eq!(l.summe["betrag"], 1.3, "0,1 + 0,2 + 1 + 0 ohne Rest");
        // „name“ hat kein einziges Farbwort, „nummer“ mischt Zahl und Text: keine Marken.
        assert_eq!(l.marken.keys().collect::<Vec<_>>(), ["ampel", "stand"]);
        assert_eq!(l.marken["ampel"], [Some(Bedeutung::Ok), None, Some(Bedeutung::Problem), None]);
        // Abnahme B1: ein unbekanntes Wort („bezahlt“) macht nur diesen Eintrag neutral.
        assert_eq!(l.marken["stand"], [Some(Bedeutung::Neutral), Some(Bedeutung::Wartet), None, None]);
        let json = serde_json::to_value(l).unwrap();
        assert_eq!(json["marken"]["ampel"], serde_json::json!(["ok", null, "problem", null]));
        assert_eq!(json["marken"]["stand"], serde_json::json!(["neutral", "wartet", null, null]));
        assert_eq!(glatt(-0.0000000001), 0.0);
        assert_eq!(glatt(1234.5), 1234.5);
    }

    /// Eine Spalte zur Belegung (gesichert, unsicher, widerlegt) wird zur
    /// Marke; Abstufungen ohne eigenes Wort bleiben neutral.
    #[test]
    fn marken_zur_belegung() {
        let t = tabelle(
            "[[zeile]]\nthema = \"a\"\nsicherheit = \"gesichert\"\n\
             [[zeile]]\nthema = \"b\"\nsicherheit = \"wahrscheinlich\"\n\
             [[zeile]]\nthema = \"c\"\nsicherheit = \"Unsicher\"\n\
             [[zeile]]\nthema = \"d\"\nsicherheit = \"widerlegt\"\n\
             [[zeile]]\nthema = \"e\"\n",
        );
        let l = &ableiten(&t, tag("2026-11-04")).listen["zeile"];
        assert_eq!(l.marken.keys().collect::<Vec<_>>(), ["sicherheit"]);
        assert_eq!(l.marken["sicherheit"], [Some(Bedeutung::Ok), Some(Bedeutung::Neutral), Some(Bedeutung::Wartet), Some(Bedeutung::Problem), None]);
    }

    /// Abnahme B1: `nan` und `inf` sind keine Zahlen; keine Summe läuft über
    /// oder wird `null`; `glatt` rundet sehr große Zahlen nicht mehr.
    #[test]
    fn nicht_endliche_zahlen_zaehlen_nicht() {
        let t = tabelle(
            "[[e]]\na = 1.5\nb = nan\nc = inf\nd = 1e308\n\
             [[e]]\na = 2\nb = 3\nc = 4\nd = 1e308\n\
             [[e]]\na = -inf\n",
        );
        let a = ableiten(&t, tag("2026-10-27"));
        assert!(a.listen["e"].summe.is_empty(), "a hat -inf, b nan, c inf, d läuft über: {:?}", a.listen["e"].summe);
        let json = serde_json::to_value(&a).unwrap();
        assert_eq!(json["listen"]["e"]["summe"], serde_json::json!({}), "nie null im JSON");
        let ok = tabelle("[[e]]\nx = 1e300\n[[e]]\nx = 2e300\n[[e]]\ny = 1.0e14\n[[e]]\ny = 0.5\n");
        let l = &ableiten(&ok, tag("2026-10-27")).listen["e"];
        assert_eq!(l.summe["x"], 3e300, "groß, aber endlich: ohne Rundung, ohne Überlauf");
        assert!((l.summe["y"] - 100_000_000_000_000.5).abs() < 0.01, "{}", l.summe["y"]);
        assert_eq!(glatt(1.8e300), 1.8e300);
        assert_eq!(glatt(f64::MAX), f64::MAX);
        assert!(glatt(f64::NAN).is_nan());
        assert_eq!(glatt(2e15 + 0.25), 2e15 + 0.25, "ab 1e15 keine Rundung");
        // Eine Kennzahl auf eine solche Spalte nennt einen Satz statt null.
        let mut q = Quellen::new();
        q.insert("eigen-x".into(), Some(t.clone()));
        let b: Berechnet = toml_lesen("t", "quelle = \"eigen-x\"\nart = \"summe\"\nfeld = \"b\"").unwrap();
        assert_eq!(berechnen(&b, &q, tag("2026-10-27")).unwrap_err().0, "feld");
    }

    /// Abnahme B1: Auch bei leerer Liste muss `feld` eine Spalte des Bausteins sein.
    #[test]
    fn leere_liste_prueft_feld_gegen_schema() {
        let mut q = Quellen::new();
        q.insert("fragen".into(), Some(tabelle("# leer\n")));
        q.insert("eigen-leer".into(), Some(tabelle("posten = []\n")));
        let r = |b: &str| {
            let b: Berechnet = toml_lesen("t", b).unwrap();
            berechnen(&b, &q, tag("2026-10-27"))
        };
        assert_eq!(r("quelle = \"fragen\"\nart = \"anzahl\"\nfeld = \"art\"\nwert = \"wahl\"").unwrap(), "0");
        assert_eq!(r("quelle = \"fragen\"\nliste = \"frage\"\nart = \"tage_bis\"\nfeld = \"frist\"").unwrap(), "–");
        let (k, satz) = r("quelle = \"fragen\"\nliste = \"frage\"\nart = \"anteil\"\nfeld = \"stauts\"\nwert = \"x\"").unwrap_err();
        assert_eq!(k, "feld");
        assert!(satz.starts_with("In der Liste „frage“ gibt es die Spalte „stauts“ nicht (die Liste ist noch leer). Erlaubt sind: id, text, art,"), "{satz}");
        assert_eq!(r("quelle = \"fragen\"\nart = \"summe\"\nfeld = \"kosten\"").unwrap_err().0, "feld", "auch ohne `liste`");
        // Eigene Datei: Schema unbekannt, also kein Fehler.
        assert_eq!(r("quelle = \"eigen-leer\"\nart = \"anzahl\"\nfeld = \"egal\"\nwert = 1").unwrap(), "0");
        // Alle Listen der Bausteine haben ein Schema aus dem Typ.
        for (stamm, liste) in [
            ("kennzahlen", "kennzahl"),
            ("ampel", "zeile"),
            ("fortschritt", "zeile"),
            ("zeitstrahl", "eintrag"),
            ("gantt", "balken"),
            ("kanban", "spalte"),
            ("kanban", "spalte.karte"),
            ("fluss", "kasten"),
            ("fluss", "pfeil"),
            ("fragen", "frage"),
            ("orte", "ort"),
            ("fristen", "ereignis"),
            ("fristen", "frist"),
        ] {
            let s = schema(stamm, liste).unwrap();
            assert!(s.len() >= 2 && s.iter().all(|f| !f.is_empty() && !f.contains(' ')), "{stamm}.{liste}: {s:?}");
        }
        assert!(schema("ampel", "zeile").unwrap().contains(&"stand".to_string()));
        assert!(!schema("fortschritt", "zeile").unwrap().contains(&"stufe".to_string()), "rechnet nur der Server");
    }

    #[test]
    fn schluessel_finden() {
        assert!(hat_schluessel("berechnet = { quelle = \"a\", art = \"b\" }", "quelle"));
        assert!(hat_schluessel("  feld=\"x\"", "feld"));
        assert!(!hat_schluessel("unterfeld = 1", "feld"));
        assert!(!hat_schluessel("# feld = 1", "feld"));
    }
}
