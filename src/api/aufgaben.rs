//! `/api/aufgaben`: Aufgaben für den Inhaber (Etappe 5a, Entscheidung).
//!
//! Ein Chat legt per `curl` eine Aufgabe an (Titel, wahlweise ein Link oder
//! Knöpfe mit Antworten, Frist, Ablauf). Sie steht auf der Startseite unter
//! „Für dich“ und auf der Projektseite. Der Inhaber hakt sie im Browser ab; dabei
//! entsteht ein Vorgang der Art `antwort` für das Projekt, und das Warten
//! des Chats (`Warten.ps1`) wacht auf wie bei jedem Vorgang.
//!
//! „Etwas anderes …“: Derselbe Weg mit
//! `{"antwort": "etwas-anderes", "eigene": "…"}` schickt eigene Worte statt
//! eines Klicks. Die Aufgabe bleibt offen; der Vorgang `antwort` (Bezug
//! `aufgabe:<id> antwort:etwas-anderes`) trägt den Text als Notiz, und der
//! Chat entscheidet, wie es weitergeht. Knöpfe, die wie „Etwas anderes“
//! heißen, lehnt das Anlegen ab (die Oberfläche setzt den Knopf selbst).
//!
//! Alle Wege sind geschützt (Windows-Anmeldung, `sicherheit::geschuetzt`) und
//! wie `/api/vorgaenge` ohne Herkunftskopf für curl erlaubt
//! (`sicherheit::herkunft_pflicht`); fremde Seiten bleiben 403. Den Haken
//! (`erledigt`) gibt es nur aus dem Browser. Die schreibenden Wege nehmen nur
//! `Content-Type: application/json`.
//!
//! Der Link steht verschlüsselt in `aufgaben_links` (`speicher::aufgaben`) und
//! kommt nur im Klartext heraus, solange die Aufgabe offen und nicht
//! abgelaufen ist. Ein Zeitplan im Dienst setzt jede Minute abgelaufene
//! Aufgaben auf `abgelaufen` und löscht ihren Link; jede Anfrage hier tut
//! das vorher auch selbst. Anlegen, Haken, Zurückziehen und Ablauf senden
//! `change` mit Bereich `aufgaben` über den Live-Kanal.
//!
//! Ändern (Etappe B2, „Aufgabe ändern ohne neue Nummer“): `POST
//! /api/aufgaben/<id>` mit `titel`, `knopf` oder `frist` ändert eine offene
//! Aufgabe; Kennung, Link, Ablauf und die Einstufung des Inhabers bleiben, und
//! jede Änderung kommt in `verlauf`. Nur der Chat des Projekts (wie beim
//! Umbenennen einer Arbeit), nie aus dem Browser. Geschrieben wird nur in den
//! Datenspeicher, nie in den Inhalt.
//!
//! Grenzen: höchstens 20 offene Aufgaben je Projekt, 100 insgesamt (darüber
//! 429 mit Satz). Derselbe Zeitplan verschiebt nicht mehr offene Aufgaben
//! sieben Tage nach ihrer letzten Änderung ins Archiv (`aufgaben_archiv`,
//! `speicher::aufgaben`): Listen zeigen sie dann nicht mehr, Sicherung und
//! Export behalten sie. Gelöscht wird nur der Link. Für die Zahl im
//! Seitenkopf gibt es `GET /api/aufgaben/anzahl`: zählt, ohne einen Link zu
//! entschlüsseln.

use std::time::Duration;

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, Uri};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use zeroize::Zeroizing;

use super::vertrag::{AufgabeAngelegt, AufgabeAnsicht, AufgabeGeaendert, AufgabenAnzahl, AufgabenListe, Stufe};
use super::vorgaenge::{TITEL_HOECHSTENS, abfrage, angegeben, aus_browser, herkunft_pruefen, json_kopf_pruefen, melden, schalter, slug_pruefen, text_pruefen};
use crate::app::AppState;
use crate::fehler::AppError;
use crate::speicher::aufgaben::{self, Aendern, AufgabeAenderung, Filter, Haken, NeueAufgabe, Rueckmeldung, Zeit};
use crate::speicher::{Speicher, SpeicherFehler};
use crate::vertrag::abgeleitet::heute_von;
use crate::vertrag::daten::DatenInhalt;
use crate::vertrag::daten::{ETWAS_ANDERES, heisst_wie_etwas_anderes};
use crate::vertrag::fristen::{ABSTAND_MUSTER, Abstand, FristBezug};

/// Ein Link höchstens 2 KiB.
pub const LINK_HOECHSTENS: usize = 2048;
/// Höchstens so viele Knöpfe, je höchstens so viele Zeichen.
pub const KNOPF_HOECHSTENS: usize = 6;
pub const KNOPF_TEXT_HOECHSTENS: usize = 40;
/// Wie oft der Zeitplan nach abgelaufenen Aufgaben sieht.
pub const ABLAUF_PRUEFEN_ALLE: Duration = Duration::from_secs(60);

pub const SATZ_LINK: &str = "Links gehen nur mit https:// oder http://localhost (etwa http://localhost:8787/…). \
                             Andere Arten wie javascript:, data: oder file: lehnt das Dashboard ab.";
pub const SATZ_LINK_127: &str = "Bitte http://localhost statt http://127.0.0.1 nehmen; nur dort greift die Windows-Anmeldung im Browser.";
pub const SATZ_LINK_ADRESSE: &str = "Im Link fehlt die Adresse, oder vor ihr steht ein Benutzername (mit @).";
pub const SATZ_LINK_UNSICHTBAR: &str = "„link“ enthält unsichtbare Unicode-Zeichen (etwa U+200B oder eine \
                                        Richtungsumkehr). Solche Links lehnt das Dashboard ab, weil man ihr Ziel \
                                        nicht sicher lesen kann.";
pub const SATZ_LINK_PORT: &str = "Der Port im Link ist ungültig (erlaubt sind die Zahlen 0 bis 65535).";
pub const SATZ_HAKEN_NUR_BROWSER: &str = "Den Haken setzt nur der Inhaber im Browser. Ein Chat zieht eine Aufgabe mit \
                                          /zurueckgezogen zurück.";
const SATZ_NICHT_DA: &str = "Diese Aufgabe gibt es nicht.";
pub const SATZ_KNOPF_ETWAS_ANDERES: &str = "„Etwas anderes …“ setzt das Dashboard bei jeder Aufgabe selbst dazu; \
                                            bitte diesen Knopf in „knopf“ weglassen.";

const MUSTER_NEU: &str = r#"{"projekt": "dashboard", "von": "chat:dashboard", "titel": "…", "link": "https://…", "knopf": ["ja", "nein"], "frist": "2026-10-30", "ablauf": "2026-10-28T18:00:00+01:00", "prioritaet": "bald"} (nur projekt und titel sind Pflicht; statt frist eine bewegliche Frist: "frist_bezug": {"ereignis": "zusage-da", "abstand": "+14 Tage"})"#;
const MUSTER_HAKEN: &str = r#"{"antwort": "ja"} (nur bei Aufgaben mit Knöpfen; sonst leer; eigene Worte: {"antwort": "etwas-anderes", "eigene": "…"})"#;
const MUSTER_AENDERN: &str = r#"{"titel": "…", "knopf": ["ja", "nein"], "frist": "2026-11-10", "von": "chat:beispiel"} (mindestens eines von titel, knopf, frist, frist_bezug; knopf null: nur der Haken; frist null: ohne Frist; beweglich: "frist_bezug": {"ereignis": "zusage-da", "abstand": "+1 Monat"})"#;
pub const SATZ_AENDERN_NUR_CHAT: &str = "Eine Aufgabe ändert nur der Chat ihres Projekts (per curl), nicht der Browser.";
pub const SATZ_AENDERN_EIGENER_CHAT: &str = "Eine Aufgabe ändert nur der Chat ihres Projekts: `von` weglassen oder `chat:<projekt>` der Aufgabe angeben.";
pub const SATZ_AENDERN_NICHT_OFFEN: &str =
    "Diese Aufgabe ist nicht mehr offen (erledigt, abgelaufen, zurückgezogen oder im Archiv); ändern geht nur bei offenen. Bitte eine neue anlegen.";
pub const SATZ_AENDERN_NICHTS: &str = "Es fehlt, was sich ändern soll: `titel`, `knopf`, `frist` oder `frist_bezug`.";
pub const SATZ_FRIST_UND_BEZUG: &str = "Eine Frist ist entweder fest (`frist`) oder beweglich (`frist_bezug` mit `ereignis` und `abstand`), nicht beides.";
const MUSTER_ZURUECK: &str = r#"{"von": "chat:dashboard"} (freiwillig, leerer Inhalt geht auch)"#;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NeueAufgabeKoerper {
    projekt: String,
    titel: String,
    #[serde(default)]
    von: Option<String>,
    #[serde(default)]
    link: Option<String>,
    #[serde(default)]
    knopf: Option<Vec<String>>,
    #[serde(default)]
    frist: Option<String>,
    #[serde(default)]
    ablauf: Option<String>,
    /// Vorschlag für die Stufe: jetzt, bald (Standard) oder irgendwann (Etappe A1).
    #[serde(default)]
    prioritaet: Option<String>,
    /// Bewegliche Frist (Etappe B3): Ereignis aus `daten/fristen.toml` des Projekts und Abstand.
    #[serde(default)]
    frist_bezug: Option<FristBezug>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct HakenKoerper {
    #[serde(default)]
    antwort: Option<String>,
    /// Nur bei `antwort = "etwas-anderes"`: die eigenen Worte des Inhabers.
    #[serde(default)]
    eigene: Option<String>,
}

/// `POST /api/aufgaben/<id>` (Etappe B2). `null` unterscheidet sich vom Fehlen:
/// `knopf: null` heißt nur der Haken, `frist: null` ohne Frist.
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct AenderungKoerper {
    #[serde(default, deserialize_with = "doppelt")]
    titel: Option<Option<String>>,
    #[serde(default, deserialize_with = "doppelt")]
    knopf: Option<Option<Vec<String>>>,
    #[serde(default, deserialize_with = "doppelt")]
    frist: Option<Option<String>>,
    /// Etappe B3: `null` nimmt die bewegliche Frist weg.
    #[serde(default, deserialize_with = "doppelt")]
    frist_bezug: Option<Option<FristBezug>>,
    #[serde(default)]
    von: Option<String>,
}

/// Ein Feld, das da ist, auch als `null`: `Some(None)`.
fn doppelt<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(d).map(Some)
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ZurueckKoerper {
    #[serde(default)]
    #[allow(dead_code)]
    von: Option<String>,
}

// ----- Eingaben prüfen -----

/// JSON-Körper lesen. Ins Protokoll kommt nur die Stelle des Fehlers, nie
/// der Text von serde (er kann Werte enthalten, etwa einen Link).
fn koerper_lesen<T: DeserializeOwned>(koerper: &[u8], muster: &str) -> Result<T, AppError> {
    serde_json::from_slice(koerper).map_err(|e| {
        tracing::debug!("Aufgaben: Körper unlesbar ({:?}, Zeile {}, Spalte {})", e.classify(), e.line(), e.column());
        AppError::unlesbar(format!("Der Inhalt ist kein passendes JSON. Erwartet wird etwa {muster}."))
    })
}

/// Leerer Körper (nur Leerzeichen) zählt wie `{}`.
fn koerper_oder_leer<T: DeserializeOwned + Default>(koerper: &[u8], muster: &str) -> Result<T, AppError> {
    if koerper.iter().all(u8::is_ascii_whitespace) {
        return Ok(T::default());
    }
    koerper_lesen(koerper, muster)
}

/// Unsichtbare Formatzeichen (Unicode-Kategorie Cf, Stand Unicode 15.1):
/// weiche Trennung, Nullbreite, Richtungsmarken und -umkehr, Wortverbinder,
/// BOM, Tags und einige Schrift-Steuerzeichen. Sie lassen einen Link anders
/// aussehen, als er ist. Als feste Liste, ohne neues Crate.
const FORMATZEICHEN: &[(u32, u32)] = &[
    (0x00AD, 0x00AD),
    (0x0600, 0x0605),
    (0x061C, 0x061C),
    (0x06DD, 0x06DD),
    (0x070F, 0x070F),
    (0x0890, 0x0891),
    (0x08E2, 0x08E2),
    (0x180E, 0x180E),
    (0x200B, 0x200F),
    (0x202A, 0x202E),
    (0x2060, 0x2064),
    (0x2066, 0x206F),
    (0xFEFF, 0xFEFF),
    (0xFFF9, 0xFFFB),
    (0x110BD, 0x110BD),
    (0x110CD, 0x110CD),
    (0x13430, 0x1343F),
    (0x1BCA0, 0x1BCA3),
    (0x1D173, 0x1D17A),
    (0xE0001, 0xE0001),
    (0xE0020, 0xE007F),
];

/// Ob `c` ein unsichtbares Formatzeichen (Kategorie Cf) ist.
pub fn ist_formatzeichen(c: char) -> bool {
    let n = c as u32;
    FORMATZEICHEN.iter().any(|&(von, bis)| (von..=bis).contains(&n))
}

/// Trennt den Port von der Adresse und prüft ihn: fehlt er, gut; sonst 1 bis
/// 5 Ziffern, höchstens 65535 (wie `new URL` im Browser). Gibt den Host zurück.
fn port_abtrennen(adresse: &str) -> Result<&str, AppError> {
    // IPv6 in Klammern: der Port steht hinter „]“.
    let (host, port) = if adresse.starts_with('[') {
        let Some(i) = adresse.find(']') else { return Err(AppError::unlesbar(SATZ_LINK_ADRESSE)) };
        match &adresse[i + 1..] {
            "" => (adresse, None),
            rest => match rest.strip_prefix(':') {
                Some(p) => (&adresse[..=i], Some(p)),
                None => return Err(AppError::unlesbar(SATZ_LINK_ADRESSE)),
            },
        }
    } else {
        match adresse.rsplit_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (adresse, None),
        }
    };
    if let Some(p) = port {
        let gut = !p.is_empty() && p.len() <= 5 && p.bytes().all(|b| b.is_ascii_digit()) && p.parse::<u32>().is_ok_and(|n| n <= 65535);
        if !gut {
            return Err(AppError::unlesbar(SATZ_LINK_PORT));
        }
    }
    if host.is_empty() {
        return Err(AppError::unlesbar(SATZ_LINK_ADRESSE));
    }
    Ok(host)
}

/// Nur `https://<adresse>` oder `http://localhost[:port]`, höchstens 2 KiB,
/// ohne Leer-, Steuer- und unsichtbare Formatzeichen (Cf), ohne Benutzernamen
/// vor der Adresse, mit einem Port von höchstens 65535. Dieselbe Regel prüft
/// die Oberfläche (`sichererLink` in web/src/lib/aufgaben.ts).
pub fn link_pruefen(link: &str) -> Result<(), AppError> {
    if link.len() > LINK_HOECHSTENS {
        return Err(AppError::unlesbar("„link“ ist zu lang (höchstens 2 KiB)."));
    }
    if link.chars().any(ist_formatzeichen) {
        return Err(AppError::unlesbar(SATZ_LINK_UNSICHTBAR));
    }
    if link.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err(AppError::unlesbar("„link“ enthält Leer- oder Steuerzeichen."));
    }
    let klein = link.to_ascii_lowercase();
    let (rest, https) = match (klein.strip_prefix("https://"), klein.strip_prefix("http://")) {
        (Some(r), _) => (r, true),
        (None, Some(r)) => (r, false),
        _ => return Err(AppError::unlesbar(SATZ_LINK)),
    };
    let ende = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let adresse = &rest[..ende];
    if adresse.is_empty() || adresse.contains('@') || adresse.contains('\\') {
        return Err(AppError::unlesbar(SATZ_LINK_ADRESSE));
    }
    let host = port_abtrennen(adresse)?;
    if https || host == "localhost" {
        return Ok(());
    }
    if adresse.starts_with("127.") || adresse.starts_with("[::1]") {
        return Err(AppError::unlesbar(SATZ_LINK_127));
    }
    Err(AppError::unlesbar(SATZ_LINK))
}

pub const SATZ_FORMATZEICHEN: &str =
    "enthält unsichtbare Unicode-Zeichen (etwa U+200B Nullbreite oder eine Richtungsumkehr); bitte nur sichtbaren Text schreiben.";

/// Titel und Knöpfe einer Aufgabe (Anlegen und Ändern, Etappe B2): Länge und
/// Steuerzeichen wie bei jedem Text, dazu keine unsichtbaren Formatzeichen
/// (Kategorie Cf, [`ist_formatzeichen`]). So kommt auch „Etwas anderes“ mit einer
/// Nullbreite dazwischen nicht an der Prüfung auf „Etwas anderes“ vorbei.
fn sichtbar_pruefen(feld: &str, wert: &str, max: usize) -> Result<(), AppError> {
    text_pruefen(feld, wert, max, false)?;
    if wert.chars().any(ist_formatzeichen) {
        return Err(AppError::unlesbar(format!("„{feld}“ {SATZ_FORMATZEICHEN}")));
    }
    Ok(())
}

/// `knopf`: 1 bis 6 Antworten, je 1 bis 40 Zeichen, jede nur einmal.
fn knopf_pruefen(knopf: Vec<String>) -> Result<Vec<String>, AppError> {
    let knopf: Vec<String> = knopf.into_iter().map(|k| k.trim().to_string()).collect();
    if knopf.is_empty() || knopf.len() > KNOPF_HOECHSTENS {
        return Err(AppError::unlesbar(format!("„knopf“ braucht 1 bis {KNOPF_HOECHSTENS} Antworten, etwa [\"ja\", \"nein\"].")));
    }
    for (i, k) in knopf.iter().enumerate() {
        if k.is_empty() {
            return Err(AppError::unlesbar("Eine Antwort in „knopf“ ist leer."));
        }
        sichtbar_pruefen("knopf", k, KNOPF_TEXT_HOECHSTENS)?;
        if heisst_wie_etwas_anderes(k) {
            return Err(AppError::unlesbar(SATZ_KNOPF_ETWAS_ANDERES));
        }
        if knopf[..i].contains(k) {
            return Err(AppError::unlesbar(format!("Die Antwort „{k}“ steht zweimal in „knopf“.")));
        }
    }
    Ok(knopf)
}

/// Eine Stufe als Wort (`jetzt`, `bald`, `irgendwann`).
pub(super) fn stufe_pruefen(feld: &str, wort: &str) -> Result<Stufe, AppError> {
    Stufe::aus_wort(wort).ok_or_else(|| AppError::unlesbar(format!("„{feld}“ ist jetzt, bald oder irgendwann.")))
}

/// `frist`: ein Datum JJJJ-MM-TT.
pub(super) fn frist_pruefen(frist: &str) -> Result<String, AppError> {
    match chrono::NaiveDate::parse_from_str(frist, "%Y-%m-%d") {
        Ok(d) if frist.len() == 10 => Ok(d.format("%Y-%m-%d").to_string()),
        _ => Err(AppError::unlesbar("„frist“ ist ein Datum in der Form JJJJ-MM-TT, etwa 2026-10-30.")),
    }
}

/// Etappe B3: Eine bewegliche Frist braucht ein Ereignis, das es in
/// `daten/fristen.toml` des Projekts gibt (der gespeicherte Stand des Inhalts),
/// und einen lesbaren Abstand. Der Server schreibt das Ereignis nie selbst.
fn bezug_pruefen(state: &AppState, projekt: &str, b: FristBezug) -> Result<FristBezug, AppError> {
    let ereignis = b.ereignis.trim().to_string();
    let abstand = b.abstand.trim().to_string();
    if Abstand::lesen(&abstand).is_none() {
        return Err(AppError::unlesbar(format!("„frist_bezug.abstand“ ist {ABSTAND_MUSTER}. Werktage gibt es nicht.")));
    }
    let datei = format!("projects/{projekt}/daten/fristen.toml");
    match state.inhalt.bestand().daten(projekt, "fristen") {
        Ok(Some((DatenInhalt::Fristen(d), _))) if d.ereignis(&ereignis).is_some() => Ok(FristBezug { ereignis, abstand }),
        Ok(Some((DatenInhalt::Fristen(d), _))) => {
            let da: Vec<String> = d.inhalte().into_iter().flat_map(|t| &t.ereignis).map(|e| e.kennung.clone()).collect();
            Err(AppError::unlesbar(format!(
                "Das Ereignis „{ereignis}“ gibt es in {datei} nicht{}. Erst dort eintragen und committen, dann die Aufgabe anlegen.",
                if da.is_empty() { String::new() } else { format!(" (vorhanden: {})", da.join(", ")) }
            )))
        }
        Ok(_) => Err(AppError::unlesbar(format!("{datei} gibt es noch nicht. Erst das Ereignis dort eintragen und committen, dann die Aufgabe anlegen."))),
        Err(_) => Err(AppError::unlesbar(format!("{datei} lässt sich gerade nicht lesen; `pruefe` sagt, warum."))),
    }
}

/// `ablauf`: ein Zeitpunkt mit Zeitzone (RFC 3339), in der Zukunft.
fn ablauf_pruefen(ablauf: &str, jetzt: &Zeit) -> Result<String, AppError> {
    let Ok(t) = chrono::DateTime::parse_from_rfc3339(ablauf) else {
        return Err(AppError::unlesbar("„ablauf“ ist ein Zeitpunkt mit Zeitzone, etwa 2026-10-28T18:00:00+01:00."));
    };
    if t <= *jetzt {
        return Err(AppError::unlesbar("„ablauf“ liegt nicht in der Zukunft."));
    }
    Ok(t.to_rfc3339_opts(chrono::SecondsFormat::Secs, false))
}

/// Fehler des Datenspeichers: unbekannte Kennung 404, sonst 500 mit Satz.
fn aufgabenfehler(e: SpeicherFehler) -> AppError {
    match e {
        SpeicherFehler::NichtGefunden(_) => AppError::nicht_gefunden(SATZ_NICHT_DA),
        SpeicherFehler::Grenze(satz) => AppError::zu_viele(satz),
        e => {
            tracing::error!("Aufgaben: Datenspeicher: {e} ({})", e.einzelheiten());
            AppError::intern(format!("{e} Einzelheiten stehen im Protokoll."))
        }
    }
}

// ----- Ablauf -----

/// Ein Lauf des Ablaufs mit „jetzt“ als Eingabe: setzt abgelaufene Aufgaben
/// auf `abgelaufen`, löscht ihre Links und meldet je Aufgabe ein `change`.
/// Gibt die Zahl der abgelaufenen zurück.
pub async fn ablauf_lauf(state: &AppState, jetzt: Zeit) -> Result<usize, AppError> {
    let weg = Speicher::im_hintergrund(state.speicher()?, move |s| s.aufgaben_ablaufen(&jetzt)).await.map_err(aufgabenfehler)?;
    for a in &weg {
        tracing::info!("Aufgabe {} ist abgelaufen; der Link ist gelöscht", a.id);
        super::events::aufgabe_geaendert(&state.events, &a.projekt);
    }
    Ok(weg.len())
}

/// Ein Lauf des Archivs: nicht mehr offene Aufgaben, deren letzte Änderung
/// sieben Tage her ist, ziehen nach `aufgaben_archiv` um (nichts wird
/// gelöscht). Meldet je betroffenem Projekt ein `change`. Gibt die Zahl zurück.
pub async fn archiv_lauf(state: &AppState, jetzt: Zeit) -> Result<usize, AppError> {
    let weg = Speicher::im_hintergrund(state.speicher()?, move |s| s.aufgaben_archivieren(&jetzt)).await.map_err(aufgabenfehler)?;
    if !weg.is_empty() {
        tracing::info!("{} Aufgabe(n) ins Archiv verschoben (abgeschlossen vor {} Tagen)", weg.len(), aufgaben::ARCHIVIEREN_NACH_TAGEN);
        let mut projekte: Vec<&str> = weg.iter().map(|a| a.projekt.as_str()).collect();
        projekte.sort_unstable();
        projekte.dedup();
        for p in projekte {
            super::events::aufgabe_geaendert(&state.events, p);
        }
    }
    Ok(weg.len())
}

/// Zeitplan im Dienst: jede Minute `ablauf_lauf` und `archiv_lauf`, dazu die
/// Archive der Arbeiten (`arbeiten::archiv_lauf`) und Antworten (`antworten::archiv_lauf`). Endet mit dem Abschaltsignal.
pub fn spawn_ablauf(state: AppState) {
    tokio::spawn(async move {
        let mut takt = tokio::time::interval(ABLAUF_PRUEFEN_ALLE);
        takt.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let abschaltung = state.abschaltung();
        tokio::pin!(abschaltung);
        loop {
            tokio::select! {
                _ = takt.tick() => {}
                _ = &mut abschaltung => return,
            }
            let jetzt = aufgaben::jetzt();
            if let Err(e) = ablauf_lauf(&state, jetzt).await {
                tracing::warn!("Ablauf der Aufgaben: {}", e.fehler);
            }
            if let Err(e) = archiv_lauf(&state, jetzt).await {
                tracing::warn!("Archiv der Aufgaben: {}", e.fehler);
            }
            // Etappe A1: fertige Arbeiten nach sieben Tagen ins Archiv.
            if let Err(e) = super::arbeiten::archiv_lauf(&state, jetzt).await {
                tracing::warn!("Archiv der Arbeiten: {}", e.fehler);
            }
            // Etappe A2: Antworten auf Fragen nach sieben Tagen ins Archiv.
            if let Err(e) = super::antworten::archiv_lauf(&state, jetzt).await {
                tracing::warn!("Archiv der Antworten: {}", e.fehler);
            }
            // Etappe A3: gelesene und behobene Meldungen nach sieben Tagen ins Archiv.
            crate::meldungen::archiv_lauf(&state, jetzt).await;
            // Etappe 2e: Windows-Ereignisse älter als 90 Tage ins Archiv.
            super::pc::archiv_lauf(&state, jetzt).await;
            // Etappe 7c: abgelaufene Passkey-Anträge, alte Challenges und Sperren.
            if let Err(e) = super::passkey::ablauf_lauf(&state, jetzt).await {
                tracing::warn!("Ablauf der Passkey-Anträge: {}", e.fehler);
            }
        }
    });
}

// ----- Handler -----

/// `POST /api/aufgaben`: legt eine Aufgabe an (Zustand `offen`), 201 mit `id`.
pub async fn anlegen(State(state): State<AppState>, kopf: HeaderMap, koerper: Bytes) -> Result<(StatusCode, Json<AufgabeAngelegt>), AppError> {
    json_kopf_pruefen(&kopf)?;
    let neu: NeueAufgabeKoerper = koerper_lesen(&koerper, MUSTER_NEU)?;
    let jetzt = aufgaben::jetzt();
    slug_pruefen(&neu.projekt)?;
    let titel = neu.titel.trim().to_string();
    if titel.is_empty() {
        return Err(AppError::unlesbar("Der Titel fehlt."));
    }
    sichtbar_pruefen("titel", &titel, TITEL_HOECHSTENS)?;
    let link = match angegeben(neu.link) {
        Some(l) => {
            let l = Zeroizing::new(l);
            link_pruefen(&l)?;
            Some(l)
        }
        None => None,
    };
    let knopf = neu.knopf.map(knopf_pruefen).transpose()?;
    let frist = angegeben(neu.frist).map(|f| frist_pruefen(&f)).transpose()?;
    let ablauf = angegeben(neu.ablauf).map(|a| ablauf_pruefen(&a, &jetzt)).transpose()?;
    let browser = aus_browser(&kopf);
    let von = match angegeben(neu.von) {
        Some(v) => v,
        None if browser => "browser".to_string(),
        None => format!("chat:{}", neu.projekt),
    };
    herkunft_pruefen(&von, browser)?;
    let prioritaet = angegeben(neu.prioritaet).map(|p| stufe_pruefen("prioritaet", &p)).transpose()?;
    if frist.is_some() && neu.frist_bezug.is_some() {
        return Err(AppError::unlesbar(SATZ_FRIST_UND_BEZUG));
    }
    let frist_bezug = neu.frist_bezug.map(|b| bezug_pruefen(&state, &neu.projekt, b)).transpose()?;
    let n = NeueAufgabe { projekt: neu.projekt, von, titel, link, knopf, frist, ablauf, prioritaet, frist_bezug };
    let a = Speicher::im_hintergrund(state.speicher()?, move |s| s.aufgabe_anlegen(n, &jetzt)).await.map_err(aufgabenfehler)?;
    tracing::info!("Aufgabe {} für {} angelegt (Link: {})", a.id, a.projekt, if a.hat_link { "ja" } else { "nein" });
    super::events::aufgabe_geaendert(&state.events, &a.projekt);
    Ok((StatusCode::CREATED, Json(AufgabeAngelegt { id: a.id })))
}

/// `GET /api/aufgaben[?offen=1][&projekt=<slug>]`.
pub async fn liste(State(state): State<AppState>, uri: Uri) -> Result<Json<AufgabenListe>, AppError> {
    let nur_offen = schalter(&uri, "offen")?;
    let projekt = abfrage(&uri, "projekt");
    if let Some(p) = &projekt {
        slug_pruefen(p)?;
    }
    let jetzt = aufgaben::jetzt();
    ablauf_lauf(&state, jetzt).await?;
    // Entscheidung archiv-status: ohne `projekt` fehlen archivierte Projekte.
    let ohne = if projekt.is_none() { state.archivierte() } else { Default::default() };
    let filter = Filter { nur_offen, projekt };
    let mut aufgaben = Speicher::im_hintergrund(state.speicher()?, move |s| s.aufgaben_lesen(&filter, &jetzt)).await.map_err(aufgabenfehler)?;
    aufgaben.retain(|a| !ohne.contains(&a.aufgabe.projekt));
    crate::fristen::ansichten_aufloesen(&state.inhalt.bestand(), &mut aufgaben, heute_von(&jetzt));
    Ok(Json(AufgabenListe { aufgaben }))
}

/// `GET /api/aufgaben/anzahl`: nur die Zahl der offenen Aufgaben, für den
/// Seitenkopf. Liest keinen Link und entschlüsselt nichts; abgelaufene zählen
/// schon nicht mehr, auch bevor der Zeitplan sie umstellt.
pub async fn anzahl(State(state): State<AppState>) -> Result<Json<AufgabenAnzahl>, AppError> {
    let jetzt = aufgaben::jetzt();
    // Entscheidung archiv-status: archivierte Projekte zählen nicht mit.
    let ohne = state.archivierte();
    let offen = Speicher::im_hintergrund(state.speicher()?, move |s| s.aufgaben_offen_zaehlen_ohne(&jetzt, &ohne)).await.map_err(aufgabenfehler)?;
    Ok(Json(AufgabenAnzahl { offen }))
}

/// Die Aufgabe nach einem Schritt, wie die API sie zeigt.
async fn ansicht(state: &AppState, id: String, jetzt: Zeit) -> Result<AufgabeAnsicht, AppError> {
    let mut a = Speicher::im_hintergrund(state.speicher()?, move |s| s.aufgabe(&id, &jetzt))
        .await
        .map_err(aufgabenfehler)?
        .ok_or_else(|| AppError::nicht_gefunden(SATZ_NICHT_DA))?;
    crate::fristen::ansichten_aufloesen(&state.inhalt.bestand(), std::slice::from_mut(&mut a), heute_von(&jetzt));
    Ok(a)
}

/// `POST /api/aufgaben/<id>/erledigt`: der Haken des Inhabers, nur aus dem Browser.
pub async fn erledigt(State(state): State<AppState>, Path(id): Path<String>, kopf: HeaderMap, koerper: Bytes) -> Result<Json<AufgabeGeaendert>, AppError> {
    json_kopf_pruefen(&kopf)?;
    let haken: HakenKoerper = koerper_oder_leer(&koerper, MUSTER_HAKEN)?;
    if !aus_browser(&kopf) {
        return Err(AppError::unlesbar(SATZ_HAKEN_NUR_BROWSER));
    }
    let antwort = angegeben(haken.antwort);
    let jetzt = aufgaben::jetzt();
    // „Etwas anderes …“: eigene Worte als Rückmeldung; die Aufgabe bleibt offen.
    if antwort.as_deref() == Some(ETWAS_ANDERES) {
        let eigene = super::antworten::eigene_pruefen(haken.eigene)?;
        ablauf_lauf(&state, jetzt).await?;
        let id2 = id.clone();
        let ergebnis = Speicher::im_hintergrund(state.speicher()?, move |s| s.aufgabe_etwas_anderes(&id2, &eigene, &jetzt)).await.map_err(aufgabenfehler)?;
        let geaendert = match ergebnis {
            Rueckmeldung::Gesendet { aufgabe, vorgang, ereignis } => {
                // Ins Protokoll nur, dass geschrieben wurde, nie der Text.
                tracing::info!("Aufgabe {}: eigene Worte als Rückmeldung, Vorgang {} für {}", aufgabe.id, vorgang.id, vorgang.slug);
                melden(&state, &ereignis);
                super::events::aufgabe_geaendert(&state.events, &aufgabe.projekt);
                true
            }
            Rueckmeldung::Gleich(_) | Rueckmeldung::NichtOffen(_) => false,
        };
        return Ok(Json(AufgabeGeaendert { aufgabe: ansicht(&state, id, jetzt).await?, geaendert }));
    }
    if angegeben(haken.eigene).is_some() {
        return Err(AppError::unlesbar(super::antworten::SATZ_EIGENE_OHNE_ETWAS_ANDERES));
    }
    ablauf_lauf(&state, jetzt).await?;
    let id2 = id.clone();
    let ergebnis = Speicher::im_hintergrund(state.speicher()?, move |s| s.aufgabe_erledigt(&id2, antwort.as_deref(), &jetzt)).await.map_err(aufgabenfehler)?;
    let geaendert = match ergebnis {
        Haken::Erledigt { aufgabe, vorgang, ereignis } => {
            tracing::info!("Aufgabe {} erledigt, Vorgang {} für {}", aufgabe.id, vorgang.id, vorgang.slug);
            melden(&state, &ereignis);
            super::events::aufgabe_geaendert(&state.events, &aufgabe.projekt);
            true
        }
        Haken::Unveraendert(_) => false,
        Haken::FalscheAntwort { knopf: Some(knopf) } => {
            return Err(AppError::unlesbar(format!("Diese Antwort passt nicht zur Aufgabe. Möglich sind: {}.", knopf.join(", "))));
        }
        Haken::FalscheAntwort { knopf: None } => {
            return Err(AppError::unlesbar("Diese Aufgabe hat keine Knöpfe; der Haken geht ohne Antwort."));
        }
    };
    Ok(Json(AufgabeGeaendert { aufgabe: ansicht(&state, id, jetzt).await?, geaendert }))
}

/// `POST /api/aufgaben/<id>` (Etappe B2): Titel, Knöpfe oder Frist einer
/// offenen Aufgabe ändern, ohne neue Kennung. Nur der Chat des Projekts.
pub async fn aendern(State(state): State<AppState>, Path(id): Path<String>, kopf: HeaderMap, koerper: Bytes) -> Result<Json<AufgabeGeaendert>, AppError> {
    json_kopf_pruefen(&kopf)?;
    let k: AenderungKoerper = koerper_oder_leer(&koerper, MUSTER_AENDERN)?;
    if aus_browser(&kopf) {
        return Err(AppError::unlesbar(SATZ_AENDERN_NUR_CHAT));
    }
    if k.titel.is_none() && k.knopf.is_none() && k.frist.is_none() && k.frist_bezug.is_none() {
        return Err(AppError::unlesbar(SATZ_AENDERN_NICHTS));
    }
    let titel = match k.titel {
        None => None,
        Some(t) => {
            let t = t.unwrap_or_default().trim().to_string();
            if t.is_empty() {
                return Err(AppError::unlesbar("Der Titel darf nicht leer sein."));
            }
            sichtbar_pruefen("titel", &t, TITEL_HOECHSTENS)?;
            Some(t)
        }
    };
    let knopf = match k.knopf {
        None => None,
        Some(None) => Some(None),
        Some(Some(liste)) => Some(Some(knopf_pruefen(liste)?)),
    };
    let frist = match k.frist {
        None => None,
        // Löschen nur mit null; "" oder nur Leerzeichen ist ein Versehen (422).
        Some(None) => Some(None),
        Some(Some(f)) => Some(Some(frist_pruefen(f.trim())?)),
    };
    let von = angegeben(k.von).map(|v| herkunft_pruefen(&v, false).map(|()| v)).transpose()?;
    if matches!(frist, Some(Some(_))) && matches!(k.frist_bezug, Some(Some(_))) {
        return Err(AppError::unlesbar(SATZ_FRIST_UND_BEZUG));
    }
    let jetzt = aufgaben::jetzt();
    ablauf_lauf(&state, jetzt).await?;
    // Etappe B3: das Ereignis im Projekt der Aufgabe prüfen.
    let frist_bezug = match k.frist_bezug {
        None => None,
        Some(None) => Some(None),
        Some(Some(b)) => {
            let projekt = ansicht(&state, id.clone(), jetzt).await?.aufgabe.projekt;
            Some(Some(bezug_pruefen(&state, &projekt, b)?))
        }
    };
    let aenderung = AufgabeAenderung { titel, knopf, frist, frist_bezug, von };
    let id2 = id.clone();
    let ergebnis = Speicher::im_hintergrund(state.speicher()?, move |s| s.aufgabe_aendern(&id2, &aenderung, &jetzt)).await.map_err(aufgabenfehler)?;
    let geaendert = match ergebnis {
        Aendern::Geaendert(a) => {
            tracing::info!("Aufgabe {} geändert ({} Einträge im Verlauf)", a.id, a.verlauf.len());
            super::events::aufgabe_geaendert(&state.events, &a.projekt);
            true
        }
        Aendern::Unveraendert(_) => false,
        Aendern::NichtOffen(_) => return Err(AppError::unlesbar(SATZ_AENDERN_NICHT_OFFEN)),
        Aendern::FremderChat => return Err(AppError::abgelehnt(SATZ_AENDERN_EIGENER_CHAT)),
    };
    Ok(Json(AufgabeGeaendert { aufgabe: ansicht(&state, id, jetzt).await?, geaendert }))
}

/// `POST /api/aufgaben/<id>/zurueckgezogen`: der Chat zieht die Aufgabe zurück.
pub async fn zurueckgezogen(
    State(state): State<AppState>,
    Path(id): Path<String>,
    kopf: HeaderMap,
    koerper: Bytes,
) -> Result<Json<AufgabeGeaendert>, AppError> {
    json_kopf_pruefen(&kopf)?;
    let _: ZurueckKoerper = koerper_oder_leer(&koerper, MUSTER_ZURUECK)?;
    let jetzt = aufgaben::jetzt();
    ablauf_lauf(&state, jetzt).await?;
    let id2 = id.clone();
    let ergebnis = Speicher::im_hintergrund(state.speicher()?, move |s| s.aufgabe_zurueckziehen(&id2, &jetzt)).await.map_err(aufgabenfehler)?;
    if let Some(a) = &ergebnis {
        tracing::info!("Aufgabe {} zurückgezogen", a.id);
        super::events::aufgabe_geaendert(&state.events, &a.projekt);
    }
    Ok(Json(AufgabeGeaendert { aufgabe: ansicht(&state, id, jetzt).await?, geaendert: ergebnis.is_some() }))
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

    /// Erfundener Testwert, kein echter Link.
    const LINK: &str = "https://anmeldung.beispiel.invalid/einmal?token=erfunden-ab12cd34&code=987654";
    const GEHEIM: &str = "erfunden-ab12cd34";

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

    /// Wie `fetch` aus dem Dashboard selbst (eigener Origin).
    fn post_browser(pfad: &str, koerper: &str) -> Request<Body> {
        Request::post(pfad)
            .header("host", "localhost:8787")
            .header("origin", "http://localhost:8787")
            .header("sec-fetch-site", "same-origin")
            .header("content-type", "application/json")
            .body(Body::from(koerper.to_string()))
            .unwrap()
    }

    async fn roh(state: &AppState, req: Request<Body>) -> (StatusCode, String) {
        let res: Response = router(state.clone()).oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    async fn schicke(state: &AppState, req: Request<Body>) -> (StatusCode, serde_json::Value) {
        let (status, text) = roh(state, req).await;
        let v = serde_json::from_str(&text).unwrap_or_else(|e| panic!("kein JSON ({e}): {text}"));
        (status, v)
    }

    fn in_einer_stunde() -> String {
        (aufgaben::jetzt() + chrono::Duration::hours(1)).to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
    }

    async fn neu(state: &AppState, koerper: &str) -> String {
        let (status, v) = schicke(state, post("/api/aufgaben", koerper)).await;
        assert_eq!(status, StatusCode::CREATED, "{v}");
        v["id"].as_str().unwrap().to_string()
    }

    fn ids(v: &serde_json::Value) -> Vec<String> {
        v["aufgaben"].as_array().unwrap().iter().map(|a| a["id"].as_str().unwrap().to_string()).collect()
    }

    /// Nimmt alle `change` mit Bereich `aufgaben`, die schon im Kanal liegen.
    fn aufgaben_changes(rx: &mut tokio::sync::broadcast::Receiver<super::super::events::Change>) -> Vec<String> {
        let mut out = Vec::new();
        while let Ok(c) = rx.try_recv() {
            if c.area == "aufgaben" {
                out.push(c.path);
            }
        }
        out
    }

    /// Abnahme 5a: Ein Chat legt per Router-Anfrage (wie curl) eine Aufgabe mit
    /// Link und Ablauf an; sie erscheint in `offen=1` und im Projekt; der
    /// Haken des Inhabers legt einen Vorgang `antwort` an und weckt den wartenden Chat; im
    /// Export und auf /daten steht der Link nie.
    #[tokio::test]
    async fn abnahme_von_curl_bis_zum_wecken() {
        let state = zustand();
        let mut changes = state.events.subscribe();
        let koerper = format!(
            r#"{{"projekt": "dashboard", "von": "chat:dashboard", "titel": "Bei Probe-Dienst anmelden", "link": "{LINK}", "ablauf": "{}", "frist": "2026-10-28"}}"#,
            in_einer_stunde()
        );
        let id = neu(&state, &koerper).await;
        assert!(id.starts_with("a-"), "{id}");
        assert_eq!(aufgaben_changes(&mut changes), ["aufgaben/dashboard"]);

        // Offen, mit Link, auf der Startseite und im Projekt.
        let (status, offen) = schicke(&state, get("/api/aufgaben?offen=1")).await;
        assert_eq!(status, StatusCode::OK, "{offen}");
        assert_eq!(ids(&offen), [id.as_str()]);
        let a = &offen["aufgaben"][0];
        assert_eq!((a["link"].as_str(), a["hat_link"].as_bool()), (Some(LINK), Some(true)));
        assert_eq!((a["zustand"].as_str(), a["von"].as_str(), a["projekt"].as_str()), (Some("offen"), Some("chat:dashboard"), Some("dashboard")));
        assert_eq!(a["frist"], "2026-10-28");
        let (_, projekt) = schicke(&state, get("/api/aufgaben?projekt=dashboard")).await;
        assert_eq!(ids(&projekt), [id.as_str()]);
        let (_, fremd) = schicke(&state, get("/api/aufgaben?projekt=beispiel&offen=1")).await;
        assert!(ids(&fremd).is_empty());

        // Export und /daten: nie der Link, nur „Link (verborgen)“ und die Anzahl.
        export_ohne_link(&state, 1).await;

        // Der Chat wartet (wie Warten.ps1).
        let s = state.clone();
        let warter = tokio::spawn(async move { schicke(&s, get("/api/vorgaenge/warten?fuer=dashboard&timeout=10")).await });
        let frist = Instant::now() + Duration::from_secs(2);
        while state.warteraum.anzahl("dashboard") != 1 {
            assert!(Instant::now() < frist, "Warter meldet sich nicht an");
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        // Per curl gibt es keinen Haken.
        let (status, v) = schicke(&state, post(&format!("/api/aufgaben/{id}/erledigt"), "")).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(SATZ_HAKEN_NUR_BROWSER)));
        assert!(!warter.is_finished());

        // Der Inhaber hakt ab.
        let start = Instant::now();
        let (status, v) = schicke(&state, post_browser(&format!("/api/aufgaben/{id}/erledigt"), "")).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!((v["geaendert"].as_bool(), v["aufgabe"]["zustand"].as_str()), (Some(true), Some("erledigt")));
        assert_eq!(v["aufgabe"]["link"], serde_json::Value::Null);
        let (status, l) = warter.await.unwrap();
        assert!(start.elapsed() < Duration::from_secs(1), "{:?}", start.elapsed());
        assert_eq!(status, StatusCode::OK, "{l}");
        let vorgang = &l["vorgaenge"][0];
        assert_eq!((vorgang["art"].as_str(), vorgang["slug"].as_str()), (Some("antwort"), Some("dashboard")));
        assert_eq!(vorgang["bezug"].as_str(), Some(format!("aufgabe:{id}").as_str()));
        assert_eq!(vorgang["titel"], "Der Inhaber hat erledigt: Bei Probe-Dienst anmelden");
        assert_eq!(vorgang["id"], v["aufgabe"]["vorgang"]);
        assert_eq!(aufgaben_changes(&mut changes), ["aufgaben/dashboard"]);

        // Danach: nicht mehr offen, Link gelöscht, nur der Titel bleibt.
        let (_, offen) = schicke(&state, get("/api/aufgaben?offen=1")).await;
        assert!(ids(&offen).is_empty(), "{offen}");
        let (_, alle) = schicke(&state, get("/api/aufgaben?projekt=dashboard")).await;
        assert_eq!(alle["aufgaben"][0]["titel"], "Bei Probe-Dienst anmelden");
        assert_eq!(alle["aufgaben"][0]["link"], serde_json::Value::Null);
        assert_eq!(state.speicher().unwrap().aufgaben_links_anzahl(), 0);
        export_ohne_link(&state, 0).await;
        // Ein zweiter Haken ändert nichts und weckt niemanden.
        let (status, v) = schicke(&state, post_browser(&format!("/api/aufgaben/{id}/erledigt"), "{}")).await;
        assert_eq!((status, v["geaendert"].as_bool()), (StatusCode::OK, Some(false)));
        assert!(aufgaben_changes(&mut changes).is_empty());
    }

    /// Export (TOML und JSON) und /daten: kein Link, kein Geheimnis; die
    /// Links nur als Anzahl, die Aufgabe mit „Link (verborgen)“.
    async fn export_ohne_link(state: &AppState, links: u64) {
        for pfad in ["/api/daten/export?format=toml", "/api/daten/export?format=json", "/api/daten"] {
            let (status, text) = roh(state, get(pfad)).await;
            assert_eq!(status, StatusCode::OK, "{pfad}: {text}");
            assert!(!text.contains(GEHEIM) && !text.contains("beispiel.invalid"), "{pfad} enthält den Link: {text}");
        }
        let (_, json) = schicke(state, get("/api/daten/export?format=json")).await;
        assert_eq!(json["aufgaben_links"], serde_json::json!({ "anzahl": links }));
        assert_eq!(json["aufgaben"][0]["link"], crate::speicher::tabellen::LINK_VERBORGEN);
        let (_, toml) = roh(state, get("/api/daten/export?format=toml")).await;
        assert!(toml.contains("link = \"Link (verborgen)\""), "{toml}");
        let (_, daten) = schicke(state, get("/api/daten")).await;
        let zeile = daten["tabellen"].as_array().unwrap().iter().find(|t| t["name"] == "aufgaben_links").unwrap().clone();
        assert_eq!(zeile["eintraege"], links, "{zeile}");
    }

    /// Ablauf über den Zeitplan (gestellte Zeit): danach kein Link mehr, `change` gesendet.
    #[tokio::test]
    async fn nach_dem_ablauf_ist_der_link_weg() {
        let state = zustand();
        let koerper = format!(r#"{{"projekt": "dashboard", "titel": "Anmelden", "link": "{LINK}", "ablauf": "{}"}}"#, in_einer_stunde());
        let id = neu(&state, &koerper).await;
        let mut changes = state.events.subscribe();
        assert_eq!(ablauf_lauf(&state, aufgaben::jetzt()).await.unwrap(), 0, "noch nicht fällig");
        let spaeter = aufgaben::jetzt() + chrono::Duration::hours(2);
        assert_eq!(ablauf_lauf(&state, spaeter).await.unwrap(), 1);
        assert_eq!(aufgaben_changes(&mut changes), ["aufgaben/dashboard"]);
        let (_, alle) = schicke(&state, get("/api/aufgaben")).await;
        let a = &alle["aufgaben"][0];
        assert_eq!((a["id"].as_str(), a["zustand"].as_str()), (Some(id.as_str()), Some("abgelaufen")));
        assert_eq!((a["link"].clone(), a["hat_link"].as_bool()), (serde_json::Value::Null, Some(true)));
        assert_eq!(a["titel"], "Anmelden");
        let (_, offen) = schicke(&state, get("/api/aufgaben?offen=1")).await;
        assert!(ids(&offen).is_empty());
        export_ohne_link(&state, 0).await;
        // Abgelaufen: kein Haken mehr.
        let (status, v) = schicke(&state, post_browser(&format!("/api/aufgaben/{id}/erledigt"), "")).await;
        assert_eq!((status, v["geaendert"].as_bool()), (StatusCode::OK, Some(false)));
        assert_eq!(state.speicher().unwrap().cursor().unwrap(), 0, "kein Vorgang");
    }

    /// Abnahme 5a: javascript:-Link und andere falsche Eingaben: 422 mit Satz, nichts angelegt.
    #[tokio::test]
    async fn falsche_links_und_felder_werden_abgelehnt() {
        let state = zustand();
        let lang = format!("https://x.invalid/{}", "a".repeat(LINK_HOECHSTENS));
        let faelle: Vec<(String, &str)> = vec![
            (r#""link": "javascript:alert(1)""#.into(), SATZ_LINK),
            (r#""link": "JavaScript:alert(1)""#.into(), SATZ_LINK),
            (r#""link": "data:text/html,<b>x</b>""#.into(), SATZ_LINK),
            (r#""link": "file:///C:/Windows/win.ini""#.into(), SATZ_LINK),
            (r#""link": "ftp://beispiel.invalid/""#.into(), SATZ_LINK),
            (r#""link": "http://beispiel.invalid/""#.into(), SATZ_LINK),
            (r#""link": "http://localhost.beispiel.invalid/""#.into(), SATZ_LINK),
            (r#""link": "http://127.0.0.1:8787/daten""#.into(), SATZ_LINK_127),
            (r#""link": "https://nutzer@beispiel.invalid/""#.into(), SATZ_LINK_ADRESSE),
            (r#""link": "https:///pfad""#.into(), SATZ_LINK_ADRESSE),
            (r#""link": "https://a.invalid\\@b.invalid/""#.into(), SATZ_LINK_ADRESSE),
            (format!(r#""link": "{lang}""#), "„link“ ist zu lang (höchstens 2 KiB)."),
            (r#""link": "https://a.invalid/ b""#.into(), "„link“ enthält Leer- oder Steuerzeichen."),
            // Unsichtbare Zeichen (Kategorie Cf): Nullbreite, Richtungsmarken und -umkehr, BOM, Tags.
            (r#""link": "https://konto.beispiel.invalid\u200b.fremd.invalid/""#.into(), SATZ_LINK_UNSICHTBAR),
            (r#""link": "https://beispiel.invalid/\u200e""#.into(), SATZ_LINK_UNSICHTBAR),
            (r#""link": "https://beispiel.invalid/\u200f""#.into(), SATZ_LINK_UNSICHTBAR),
            (r#""link": "https://\u202edilavni.leipsieb/""#.into(), SATZ_LINK_UNSICHTBAR),
            (r#""link": "https://beispiel.invalid/\u2066x\u2069""#.into(), SATZ_LINK_UNSICHTBAR),
            (r#""link": "\ufeffhttps://beispiel.invalid/""#.into(), SATZ_LINK_UNSICHTBAR),
            (r#""link": "https://beispiel.invalid/\u00ad""#.into(), SATZ_LINK_UNSICHTBAR),
            (r#""link": "https://beispiel.invalid/\udb40\udc41""#.into(), SATZ_LINK_UNSICHTBAR),
            // Ports über 65535 oder keine Zahl.
            (r#""link": "https://beispiel.invalid:65536/""#.into(), SATZ_LINK_PORT),
            (r#""link": "https://beispiel.invalid:99999999/""#.into(), SATZ_LINK_PORT),
            (r#""link": "https://beispiel.invalid:abc/""#.into(), SATZ_LINK_PORT),
            (r#""link": "https://beispiel.invalid:/""#.into(), SATZ_LINK_PORT),
            (r#""link": "http://localhost:70000/""#.into(), SATZ_LINK_PORT),
            (r#""link": "https://[::1]:65536/""#.into(), SATZ_LINK_PORT),
            (r#""ablauf": "2020-01-01T00:00:00+01:00""#.into(), "„ablauf“ liegt nicht in der Zukunft."),
            (r#""ablauf": "morgen""#.into(), "„ablauf“ ist ein Zeitpunkt mit Zeitzone, etwa 2026-10-28T18:00:00+01:00."),
            (r#""frist": "30.10.2026""#.into(), "„frist“ ist ein Datum in der Form JJJJ-MM-TT, etwa 2026-10-30."),
            (r#""knopf": []"#.into(), "„knopf“ braucht 1 bis 6 Antworten, etwa [\"ja\", \"nein\"]."),
            (r#""knopf": ["ja", " ja "]"#.into(), "Die Antwort „ja“ steht zweimal in „knopf“."),
            (r#""von": "browser""#.into(), super::super::vorgaenge::SATZ_NUR_BROWSER),
        ];
        for (feld, satz) in &faelle {
            let koerper = format!(r#"{{"projekt": "dashboard", "titel": "t", {feld}}}"#);
            let (status, v) = schicke(&state, post("/api/aufgaben", &koerper)).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{feld}: {v}");
            assert_eq!(v["fehler"].as_str(), Some(*satz), "{feld}");
            assert!(!v.to_string().contains("alert"), "der Satz wiederholt die Eingabe nicht");
        }
        let lang_titel = format!(r#"{{"projekt": "dashboard", "titel": "{}"}}"#, "x".repeat(201));
        let (status, _) = schicke(&state, post("/api/aufgaben", &lang_titel)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let (status, _) = schicke(&state, post("/api/aufgaben", r#"{"projekt": "Dash Board", "titel": "t"}"#)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let (status, _) = schicke(&state, post("/api/aufgaben", r#"{"projekt": "dashboard", "titel": "t", "geheim": 1}"#)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "unbekanntes Feld");
        let (_, alle) = schicke(&state, get("/api/aufgaben")).await;
        assert!(ids(&alle).is_empty(), "nichts angelegt: {alle}");
        // Diese gehen.
        for link in [
            "https://beispiel.invalid",
            "http://localhost:8787/projects/dashboard",
            "http://LOCALHOST/",
            "http://localhost?x=1",
            "https://beispiel.invalid:65535/x",
            "https://[::1]:443/",
            "https://bücher.beispiel.invalid/",
        ] {
            neu(&state, &format!(r#"{{"projekt": "dashboard", "titel": "t", "link": "{link}"}}"#)).await;
        }
        // Ohne JSON-Kopf: 415.
        let req = Request::post("/api/aufgaben").header("host", "127.0.0.1:8787").body(Body::from(r#"{"projekt": "dashboard", "titel": "t"}"#)).unwrap();
        assert_eq!(roh(&state, req).await.0, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    /// Alle Zeichen der Liste sind Formatzeichen; sichtbare Nachbarn nicht.
    #[test]
    fn formatzeichen_liste() {
        for c in ['\u{200B}', '\u{200E}', '\u{200F}', '\u{202A}', '\u{202E}', '\u{2066}', '\u{2069}', '\u{FEFF}', '\u{E0041}'] {
            assert!(ist_formatzeichen(c), "U+{:04X}", c as u32);
        }
        for c in ['a', 'ü', '-', '.', '\u{2010}', '\u{2070}', '\u{FFFD}', '\u{1F600}'] {
            assert!(!ist_formatzeichen(c), "U+{:04X}", c as u32);
        }
    }

    /// Grenze gegen Flut: die 21. offene Aufgabe eines Projekts und die 101.
    /// insgesamt bekommen 429 mit Satz; nichts wird angelegt.
    #[tokio::test]
    async fn grenzen_geben_429() {
        let state = zustand();
        for i in 0..aufgaben::OFFEN_JE_PROJEKT {
            neu(&state, &format!(r#"{{"projekt": "dashboard", "titel": "t{i}"}}"#)).await;
        }
        let (status, v) = schicke(&state, post("/api/aufgaben", r#"{"projekt": "dashboard", "titel": "21."}"#)).await;
        assert_eq!((status, v["art"].as_str()), (StatusCode::TOO_MANY_REQUESTS, Some("zu-viele")), "{v}");
        assert_eq!(
            v["fehler"].as_str(),
            Some(
                "Für das Projekt dashboard sind schon 20 Aufgaben offen; mehr gehen nicht. Bitte erst eine zurückziehen oder warten, bis der Inhaber welche abhakt."
            )
        );
        for p in 1..aufgaben::OFFEN_INSGESAMT / aufgaben::OFFEN_JE_PROJEKT {
            for i in 0..aufgaben::OFFEN_JE_PROJEKT {
                neu(&state, &format!(r#"{{"projekt": "projekt-{p}", "titel": "t{i}"}}"#)).await;
            }
        }
        let (_, z) = schicke(&state, get("/api/aufgaben/anzahl")).await;
        assert_eq!(z, serde_json::json!({ "offen": 100 }));
        let (status, v) = schicke(&state, post("/api/aufgaben", r#"{"projekt": "noch-eins", "titel": "101."}"#)).await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{v}");
        assert!(v["fehler"].as_str().unwrap().starts_with("Insgesamt sind schon 100 Aufgaben offen"), "{v}");
        let (_, z) = schicke(&state, get("/api/aufgaben/anzahl")).await;
        assert_eq!(z["offen"], 100, "nichts angelegt");
    }

    /// `/api/aufgaben/anzahl`: nur die Zahl, ohne Link; ohne Anmeldung 401, ohne Speicher 503.
    #[tokio::test]
    async fn anzahl_zaehlt_offene() {
        let state = zustand();
        let (status, z) = schicke(&state, get("/api/aufgaben/anzahl")).await;
        assert_eq!((status, z), (StatusCode::OK, serde_json::json!({ "offen": 0 })));
        let id = neu(&state, &format!(r#"{{"projekt": "dashboard", "titel": "x", "link": "{LINK}"}}"#)).await;
        neu(&state, r#"{"projekt": "beispiel", "titel": "y"}"#).await;
        let (_, text) = roh(&state, get("/api/aufgaben/anzahl")).await;
        assert_eq!(text, r#"{"offen":2}"#);
        schicke(&state, post(&format!("/api/aufgaben/{id}/zurueckgezogen"), "")).await;
        let (_, z) = schicke(&state, get("/api/aufgaben/anzahl")).await;
        assert_eq!(z["offen"], 1);
        let res = crate::app::router(state.clone()).oneshot(get("/api/aufgaben/anzahl")).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        let ohne = AppState::fuer_tests_ohne_speicher(&PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel"), "kaputt");
        let (status, _) = schicke(&ohne, get("/api/aufgaben/anzahl")).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    }

    /// Archiv über den Zeitplan (gestellte Zeit, sieben Tage später): nicht
    /// mehr in der Liste, aber im Export; `change` gesendet.
    #[tokio::test]
    async fn archiv_ueber_den_zeitplan() {
        let state = zustand();
        let id = neu(&state, r#"{"projekt": "dashboard", "titel": "x"}"#).await;
        schicke(&state, post(&format!("/api/aufgaben/{id}/zurueckgezogen"), "")).await;
        let mut changes = state.events.subscribe();
        assert_eq!(archiv_lauf(&state, aufgaben::jetzt() + chrono::Duration::days(6)).await.unwrap(), 0);
        let (_, alle) = schicke(&state, get("/api/aufgaben")).await;
        assert_eq!(ids(&alle), [id.as_str()], "vor sieben Tagen noch in der Liste");
        assert_eq!(archiv_lauf(&state, aufgaben::jetzt() + chrono::Duration::days(8)).await.unwrap(), 1);
        assert_eq!(aufgaben_changes(&mut changes), ["aufgaben/dashboard"]);
        let (_, alle) = schicke(&state, get("/api/aufgaben")).await;
        assert!(ids(&alle).is_empty(), "{alle}");
        let (_, json) = schicke(&state, get("/api/daten/export?format=json")).await;
        assert_eq!(json["aufgaben_archiv"][0]["id"].as_str(), Some(id.as_str()), "{json}");
        assert_eq!(json["aufgaben_archiv"][0]["zustand"], "zurueckgezogen");
        let (_, daten) = schicke(&state, get("/api/daten")).await;
        let zeile = daten["tabellen"].as_array().unwrap().iter().find(|t| t["name"] == "aufgaben_archiv").unwrap().clone();
        assert_eq!(zeile["eintraege"], 1, "{zeile}");
        // Haken auf eine archivierte Aufgabe: 200, nichts geändert.
        let (status, v) = schicke(&state, post_browser(&format!("/api/aufgaben/{id}/erledigt"), "")).await;
        assert_eq!((status, v["geaendert"].as_bool()), (StatusCode::OK, Some(false)), "{v}");
    }

    /// Knöpfe: Antwort muss passen; sie steht im Vorgang (Bezug und Titel).
    #[tokio::test]
    async fn knopf_antwort_steht_im_vorgang() {
        let state = zustand();
        let id = neu(&state, r#"{"projekt": "beispiel", "titel": "Abschicken?", "knopf": ["ja", "nein"]}"#).await;
        let pfad = format!("/api/aufgaben/{id}/erledigt");
        let (status, v) = schicke(&state, post_browser(&pfad, "")).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("Diese Antwort passt nicht zur Aufgabe. Möglich sind: ja, nein.")));
        let (status, _) = schicke(&state, post_browser(&pfad, r#"{"antwort": "vielleicht"}"#)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let (status, v) = schicke(&state, post_browser(&pfad, r#"{"antwort": "nein"}"#)).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!(v["aufgabe"]["antwort"], "nein");
        let (_, l) = schicke(&state, get("/api/vorgaenge?fuer=beispiel&offen=1")).await;
        assert_eq!(l["vorgaenge"][0]["bezug"].as_str(), Some(format!("aufgabe:{id} antwort:nein").as_str()));
        assert_eq!(l["vorgaenge"][0]["titel"], "Der Inhaber hat geantwortet „nein“: Abschicken?");
        // Ohne Knöpfe keine Antwort.
        let id = neu(&state, r#"{"projekt": "beispiel", "titel": "Lesen"}"#).await;
        let (status, v) = schicke(&state, post_browser(&format!("/api/aufgaben/{id}/erledigt"), r#"{"antwort": "ja"}"#)).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some("Diese Aufgabe hat keine Knöpfe; der Haken geht ohne Antwort.")));
        let (status, v) = schicke(&state, post_browser("/api/aufgaben/a-19990101-0001/erledigt", "")).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::NOT_FOUND, Some(SATZ_NICHT_DA)));
    }

    /// Entscheidung „Etwas anderes …“ bei Aufgaben (mit Knöpfen und
    /// mit dem Haken): Die Aufgabe bleibt offen, der Chat bekommt den Text als
    /// Rückmeldung (Vorgang `antwort` mit Notiz und Zeitstempel), nur aus dem
    /// Browser; leer oder zu lang 422; ein Knopf „Etwas anderes“ beim Anlegen 422.
    #[tokio::test]
    async fn etwas_anderes_bleibt_offen_und_geht_als_rueckmeldung_an_den_chat() {
        let state = zustand();
        let mut rx = state.events.subscribe();
        let id = neu(&state, r#"{"projekt": "beispiel", "titel": "Abschicken?", "knopf": ["ja", "nein"]}"#).await;
        let pfad = format!("/api/aufgaben/{id}/erledigt");
        let koerper = r#"{"antwort": "etwas-anderes", "eigene": "Erst die Adresse prüfen,\ndann ja."}"#;
        // Per curl nicht.
        let (status, v) = schicke(&state, post(&pfad, koerper)).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(SATZ_HAKEN_NUR_BROWSER)));
        // Leer, nur Leerzeichen, zu lang, eigene ohne etwas-anderes: 422 mit Satz.
        let lang = format!(r#"{{"antwort": "etwas-anderes", "eigene": "{}"}}"#, "x".repeat(super::super::antworten::EIGENE_HOECHSTENS + 1));
        for (k, satz) in [
            (r#"{"antwort": "etwas-anderes"}"#, super::super::antworten::SATZ_EIGENE_LEER),
            (r#"{"antwort": "etwas-anderes", "eigene": "   "}"#, super::super::antworten::SATZ_EIGENE_LEER),
            (lang.as_str(), "Deine Antwort in eigenen Worten („eigene“) ist zu lang: höchstens 20000 Zeichen."),
            (r#"{"antwort": "ja", "eigene": "Text"}"#, super::super::antworten::SATZ_EIGENE_OHNE_ETWAS_ANDERES),
        ] {
            let (status, v) = schicke(&state, post_browser(&pfad, k)).await;
            assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(satz)), "{}", &k[..k.len().min(60)]);
        }
        let (_, l) = schicke(&state, get("/api/vorgaenge?fuer=beispiel")).await;
        assert!(l["vorgaenge"].as_array().unwrap().is_empty(), "nichts angelegt: {l}");
        aufgaben_changes(&mut rx);

        // Senden: offen, mit antwort und eigene; Vorgang mit dem Text als Notiz.
        let (status, v) = schicke(&state, post_browser(&pfad, koerper)).await;
        assert_eq!((status, v["geaendert"].as_bool()), (StatusCode::OK, Some(true)), "{v}");
        let a = &v["aufgabe"];
        assert_eq!(
            (a["zustand"].as_str(), a["antwort"].as_str(), a["eigene"].as_str()),
            (Some("offen"), Some("etwas-anderes"), Some("Erst die Adresse prüfen,\ndann ja."))
        );
        assert_eq!(aufgaben_changes(&mut rx), ["aufgaben/beispiel"]);
        let (_, l) = schicke(&state, get("/api/vorgaenge?fuer=beispiel&offen=1")).await;
        let vorgang = &l["vorgaenge"][0];
        assert_eq!(vorgang["bezug"].as_str(), Some(format!("aufgabe:{id} antwort:etwas-anderes").as_str()));
        assert_eq!(vorgang["titel"], "Der Inhaber hat etwas anderes geschrieben (eigene Worte): Abschicken?");
        assert_eq!((vorgang["notiz"].as_str(), vorgang["von"].as_str()), (Some("Erst die Adresse prüfen,\ndann ja."), Some("browser")));
        assert!(chrono::DateTime::parse_from_rfc3339(vorgang["erstellt"].as_str().unwrap()).is_ok(), "{vorgang}");
        assert_eq!(a["vorgang"], vorgang["id"]);
        let (_, d) = schicke(&state, get(&format!("/api/vorgaenge/{}", vorgang["id"].as_str().unwrap()))).await;
        assert_eq!(d["ereignisse"][0]["notiz"], "Erst die Adresse prüfen,\ndann ja.");
        // Die Aufgabe steht weiter unter den offenen (Anstehend und Zähler).
        let (_, offen) = schicke(&state, get("/api/aufgaben?offen=1&projekt=beispiel")).await;
        assert_eq!(ids(&offen), [id.as_str()]);
        assert_eq!(offen["aufgaben"][0]["eigene"], "Erst die Adresse prüfen,\ndann ja.");
        // Doppelklick: unverändert, kein zweiter Vorgang. Ein neuer Text: neue Rückmeldung.
        let (status, v) = schicke(&state, post_browser(&pfad, koerper)).await;
        assert_eq!((status, v["geaendert"].as_bool()), (StatusCode::OK, Some(false)));
        let (status, v) = schicke(&state, post_browser(&pfad, r#"{"antwort": "etwas-anderes", "eigene": "Adresse stimmt."}"#)).await;
        assert_eq!((status, v["geaendert"].as_bool(), v["aufgabe"]["eigene"].as_str()), (StatusCode::OK, Some(true), Some("Adresse stimmt.")));
        let (_, l) = schicke(&state, get("/api/vorgaenge?fuer=beispiel")).await;
        assert_eq!(l["vorgaenge"].as_array().unwrap().len(), 2, "{l}");
        // Danach ein Klick: erledigt wie immer.
        let (status, v) = schicke(&state, post_browser(&pfad, r#"{"antwort": "ja"}"#)).await;
        assert_eq!((status, v["aufgabe"]["zustand"].as_str(), v["aufgabe"]["antwort"].as_str()), (StatusCode::OK, Some("erledigt"), Some("ja")));
        // Nicht mehr offen: 200, nichts geändert.
        let (status, v) = schicke(&state, post_browser(&pfad, r#"{"antwort": "etwas-anderes", "eigene": "Noch was."}"#)).await;
        assert_eq!((status, v["geaendert"].as_bool()), (StatusCode::OK, Some(false)));

        // Auch beim Haken „erledigt“ (Aufgabe ohne Knöpfe).
        let id = neu(&state, r#"{"projekt": "beispiel", "titel": "Bauanleitung lesen"}"#).await;
        let (status, v) =
            schicke(&state, post_browser(&format!("/api/aufgaben/{id}/erledigt"), r#"{"antwort": "etwas-anderes", "eigene": "Schaffe ich erst morgen."}"#))
                .await;
        assert_eq!((status, v["aufgabe"]["zustand"].as_str()), (StatusCode::OK, Some("offen")), "{v}");
        let (status, _) = schicke(&state, post_browser("/api/aufgaben/a-19990101-0001/erledigt", koerper)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);

        // Ein Knopf, der wie „Etwas anderes“ heißt, ist beim Anlegen ein Fehler.
        for knopf in [r#"["ja", "Etwas anderes …"]"#, r#"["etwas-anderes"]"#] {
            let (status, v) = schicke(&state, post("/api/aufgaben", &format!(r#"{{"projekt": "beispiel", "titel": "x", "knopf": {knopf}}}"#))).await;
            assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(SATZ_KNOPF_ETWAS_ANDERES)), "{knopf}");
        }
    }

    /// Nachtrag der Abnahme: Eine Aufgabe mit eigenen Worten (auch nur mit
    /// Haken) liegt „beim Chat“: nicht in der Zahl im Seitenkopf, in Anstehend
    /// nicht „wartet auf dich“ (Zustand `beim-chat`, `wartet_auf =
    /// chat:<projekt>`, mit dem Text), auch nicht mit `nur_fuer_mich`. Zieht
    /// der Chat sie zurück, ist sie weg.
    #[tokio::test]
    async fn etwas_anderes_liegt_beim_chat_und_zaehlt_nicht_fuer_dich() {
        let state = zustand();
        let mit_knopf = neu(&state, r#"{"projekt": "beispiel", "titel": "Abschicken?", "knopf": ["ja", "nein"]}"#).await;
        let nur_haken = neu(&state, r#"{"projekt": "beispiel", "titel": "Bauanleitung lesen"}"#).await;
        let andere = neu(&state, r#"{"projekt": "beispiel", "titel": "Noch offen"}"#).await;
        let anzahl = |state: AppState| async move { schicke(&state, get("/api/aufgaben/anzahl")).await.1["offen"].as_u64() };
        assert_eq!(anzahl(state.clone()).await, Some(3));
        for (id, text) in [(&mit_knopf, "Erst die Adresse prüfen."), (&nur_haken, "Schaffe ich erst morgen.")] {
            let (status, v) =
                schicke(&state, post_browser(&format!("/api/aufgaben/{id}/erledigt"), &format!(r#"{{"antwort": "etwas-anderes", "eigene": "{text}"}}"#))).await;
            assert_eq!(status, StatusCode::OK, "{v}");
        }
        assert_eq!(anzahl(state.clone()).await, Some(1), "nur die Aufgabe, die noch auf dich wartet");
        let (_, l) = schicke(&state, get("/api/anstehend?art=aufgabe")).await;
        let eintrag = |id: &str| l["eintraege"].as_array().unwrap().iter().find(|e| e["id"] == id).cloned().unwrap_or_else(|| panic!("{id} fehlt: {l}"));
        for (id, text) in [(&mit_knopf, "Erst die Adresse prüfen."), (&nur_haken, "Schaffe ich erst morgen.")] {
            let e = eintrag(id);
            assert_eq!(
                (e["zustand"].as_str(), e["wartet_auf"].as_str(), e["wartet_auf_dich"].as_bool()),
                (Some("beim-chat"), Some("chat:beispiel"), Some(false)),
                "{e}"
            );
            assert_eq!(e["eigene"], text);
        }
        let e = eintrag(&andere);
        assert_eq!((e["zustand"].as_str(), e["wartet_auf_dich"].as_bool()), (Some("offen"), Some(true)));
        assert!(e.get("eigene").is_none(), "ohne eigene Worte kein Feld: {e}");
        let (_, fuer_mich) = schicke(&state, get("/api/anstehend?art=aufgabe&nur_fuer_mich=1")).await;
        let ids: Vec<&str> = fuer_mich["eintraege"].as_array().unwrap().iter().map(|e| e["id"].as_str().unwrap()).collect();
        assert_eq!(ids, [andere.as_str()]);
        // Der Chat zieht sie zurück (etwa um sie neu zu stellen): weg aus der Liste.
        let (status, _) = schicke(&state, post(&format!("/api/aufgaben/{mit_knopf}/zurueckgezogen"), "")).await;
        assert_eq!(status, StatusCode::OK);
        let (_, l) = schicke(&state, get("/api/anstehend?art=aufgabe")).await;
        assert!(l["eintraege"].as_array().unwrap().iter().all(|e| e["id"] != mit_knopf.as_str()), "{l}");
        assert_eq!(anzahl(state.clone()).await, Some(1));
    }

    /// Zurückziehen per curl: Link weg, `change`, kein Vorgang; zweimal ändert nichts.
    #[tokio::test]
    async fn chat_zieht_zurueck() {
        let state = zustand();
        let id = neu(&state, &format!(r#"{{"projekt": "dashboard", "titel": "x", "link": "{LINK}"}}"#)).await;
        let mut changes = state.events.subscribe();
        let pfad = format!("/api/aufgaben/{id}/zurueckgezogen");
        let (status, v) = schicke(&state, post(&pfad, "")).await;
        assert_eq!(status, StatusCode::OK, "{v}");
        assert_eq!((v["geaendert"].as_bool(), v["aufgabe"]["zustand"].as_str()), (Some(true), Some("zurueckgezogen")));
        assert_eq!(v["aufgabe"]["link"], serde_json::Value::Null);
        assert_eq!(aufgaben_changes(&mut changes), ["aufgaben/dashboard"]);
        assert_eq!(state.speicher().unwrap().aufgaben_links_anzahl(), 0);
        let (status, v) = schicke(&state, post(&pfad, r#"{"von": "chat:dashboard"}"#)).await;
        assert_eq!((status, v["geaendert"].as_bool()), (StatusCode::OK, Some(false)));
        assert_eq!(state.speicher().unwrap().cursor().unwrap(), 0, "kein Vorgang");
    }

    /// Abnahme 5a: ohne Windows-Anmeldung 401, auch per curl.
    #[tokio::test]
    async fn ohne_anmeldung_401() {
        let state = zustand();
        for req in
            [get("/api/aufgaben?offen=1"), post("/api/aufgaben", r#"{"projekt": "dashboard", "titel": "t"}"#), post_browser("/api/aufgaben/a-1/erledigt", "")]
        {
            let pfad = req.uri().to_string();
            let res = crate::app::router(state.clone()).oneshot(req).await.unwrap();
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{pfad}");
        }
        assert_eq!(state.speicher().unwrap().aufgaben_links_anzahl(), 0);
    }

    /// Fremde Seiten: lesend und schreibend 403, auch mit fremdem Origin ohne Sec-Fetch-Site.
    #[tokio::test]
    async fn fremde_seiten_bleiben_draussen() {
        let state = zustand();
        let id = neu(&state, &format!(r#"{{"projekt": "dashboard", "titel": "x", "link": "{LINK}"}}"#)).await;
        let lesen =
            Request::get("/api/aufgaben?offen=1").header("host", "127.0.0.1:8787").header("origin", "https://fremd.invalid").body(Body::empty()).unwrap();
        let (status, text) = roh(&state, lesen).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(!text.contains(GEHEIM));
        let haken = Request::post(format!("/api/aufgaben/{id}/erledigt"))
            .header("host", "127.0.0.1:8787")
            .header("sec-fetch-site", "cross-site")
            .header("content-type", "application/json")
            .body(Body::empty())
            .unwrap();
        assert_eq!(roh(&state, haken).await.0, StatusCode::FORBIDDEN);
        let (_, offen) = schicke(&state, get("/api/aufgaben?offen=1")).await;
        assert_eq!(ids(&offen), [id]);
    }

    /// Etappe B3: eine bewegliche Frist per API. Das Ereignis muss es in
    /// `daten/fristen.toml` des Projekts geben; die Ansicht zeigt das
    /// aufgelöste Datum, sobald der Chat das Datum des Ereignisses dort setzt
    /// (der Server schreibt nie in den Inhalt). Ändern wechselt zwischen fest
    /// und beweglich, mit Verlauf.
    #[tokio::test]
    async fn bewegliche_frist_per_api() {
        let ordner = std::env::temp_dir().join(format!("dashboard-test-{}-fristen-api", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        let p = ordner.join("projects").join("frist-probe");
        std::fs::create_dir_all(p.join("daten")).unwrap();
        std::fs::create_dir_all(ordner.join("projects").join("ohne-datei")).unwrap();
        for s in ["frist-probe", "ohne-datei"] {
            std::fs::write(
                ordner.join("projects").join(s).join("projekt.toml"),
                "schema = 3\ntitle = \"Probe\"\nstatus = \"aktiv\"\nsummary = \"Erfunden.\"\n",
            )
            .unwrap();
        }
        let datei = p.join("daten").join("fristen.toml");
        let toml = |datum: Option<&str>| {
            format!(
                "[[ereignis]]\nkennung = \"zusage-da\"\ntitel = \"Zusage ist da\"\n{}\n[[frist]]\nkennung = \"standanmeldung\"\ntitel = \"Stand anmelden\"\nereignis = \"zusage-da\"\nabstand = \"+1 Monat\"\n",
                datum.map(|d| format!("datum = \"{d}\"\n")).unwrap_or_default()
            )
        };
        std::fs::write(&datei, toml(None)).unwrap();
        let state = AppState::fuer_tests(&ordner);
        let bezug = r#""frist_bezug": {"ereignis": "zusage-da", "abstand": "+14 Tage"}"#;
        for (koerper, satz) in [
            (
                r#"{"projekt": "frist-probe", "titel": "X", "frist_bezug": {"ereignis": "zusage", "abstand": "+14 Tage"}}"#.to_string(),
                "Das Ereignis „zusage“ gibt es in projects/frist-probe/daten/fristen.toml nicht (vorhanden: zusage-da).",
            ),
            (
                r#"{"projekt": "frist-probe", "titel": "X", "frist_bezug": {"ereignis": "zusage-da", "abstand": "14 Tage"}}"#.to_string(),
                "„frist_bezug.abstand“ ist Vorzeichen, Zahl",
            ),
            (format!(r#"{{"projekt": "frist-probe", "titel": "X", "frist": "2026-12-01", {bezug}}}"#), SATZ_FRIST_UND_BEZUG),
            (format!(r#"{{"projekt": "ohne-datei", "titel": "X", {bezug}}}"#), "projects/ohne-datei/daten/fristen.toml gibt es noch nicht."),
            (r#"{"projekt": "frist-probe", "titel": "X", "frist_bezug": {"ereignis": "zusage-da"}}"#.to_string(), "kein passendes JSON"),
        ] {
            let (status, v) = schicke(&state, post("/api/aufgaben", &koerper)).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{koerper}: {v}");
            assert!(v["fehler"].as_str().unwrap_or_default().contains(satz), "{koerper}: {v}");
        }
        let id = neu(&state, &format!(r#"{{"projekt": "frist-probe", "titel": "Standanmeldung vorbereiten", {bezug}}}"#)).await;
        let eine = |state: AppState| async move {
            let (_, v) = schicke(&state, get("/api/aufgaben?projekt=frist-probe")).await;
            v["aufgaben"][0].clone()
        };
        // Vorher: hängt am Ereignis.
        let a = eine(state.clone()).await;
        assert_eq!((a["frist"].clone(), a["frist_stufe"].clone()), (serde_json::Value::Null, serde_json::Value::Null), "{a}");
        assert_eq!(
            (a["frist_bezug"]["abstand"].as_str(), a["ereignis"]["titel"].as_str(), a["ereignis"]["datum"].clone()),
            (Some("+14 Tage"), Some("Zusage ist da"), serde_json::Value::Null)
        );
        // Der Chat setzt das Datum des Ereignisses (vor zwei Tagen): aufgelöst in zwölf Tagen.
        let heute = crate::vertrag::abgeleitet::heute();
        let tag = |n: i64| (heute + chrono::Duration::days(n)).format("%Y-%m-%d").to_string();
        std::fs::write(&datei, toml(Some(&tag(-2)))).unwrap();
        let a = eine(state.clone()).await;
        assert_eq!((a["frist"].as_str(), a["frist_stufe"].as_str()), (Some(tag(12).as_str()), Some("spaeter")), "{a}");
        let (_, l) = schicke(&state, get("/api/anstehend?projekt=frist-probe")).await;
        let e = &l["eintraege"][0];
        assert_eq!(
            (e["frist"].as_str(), e["tage_bis"].as_i64(), e["ereignis"]["datum"].as_str()),
            (Some(tag(12).as_str()), Some(12), Some(tag(-2).as_str())),
            "{l}"
        );
        // Die Fristenliste des Projekts (abgeleitet) und übergreifend.
        let (status, d) = schicke(&state, get("/api/projekte/frist-probe/daten/fristen")).await;
        assert_eq!(status, StatusCode::OK, "{d}");
        let zeilen: Vec<(String, String)> = d["abgeleitet"]["fristen"]["zeilen"]
            .as_array()
            .unwrap()
            .iter()
            .map(|z| (z["quelle"].as_str().unwrap().to_string(), z["id"].as_str().unwrap().to_string()))
            .collect();
        assert_eq!(zeilen, [("aufgabe".to_string(), id.clone()), ("datei".to_string(), "standanmeldung".to_string())], "{d}");
        let (status, f) = schicke(&state, get("/api/fristen")).await;
        assert_eq!((status, f["zeilen"].as_array().map(Vec::len)), (StatusCode::OK, Some(2)), "{f}");
        // Fest statt beweglich, dann wieder beweglich, dann ohne: alles im Verlauf.
        let (_, v) = schicke(&state, post(&format!("/api/aufgaben/{id}"), r#"{"frist": "2026-12-01"}"#)).await;
        assert_eq!((v["aufgabe"]["frist"].as_str(), v["aufgabe"].get("frist_bezug")), (Some("2026-12-01"), None), "{v}");
        let (_, v) = schicke(&state, post(&format!("/api/aufgaben/{id}"), &format!("{{{bezug}}}"))).await;
        assert_eq!((v["aufgabe"]["frist"].as_str(), v["aufgabe"]["frist_bezug"]["ereignis"].as_str()), (Some(tag(12).as_str()), Some("zusage-da")), "{v}");
        let (status, v) = schicke(&state, post(&format!("/api/aufgaben/{id}"), r#"{"frist_bezug": {"ereignis": "gibt-es-nicht", "abstand": "+1 Tag"}}"#)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
        let (_, v) = schicke(&state, post(&format!("/api/aufgaben/{id}"), r#"{"frist_bezug": null}"#)).await;
        assert_eq!(
            (v["aufgabe"]["frist"].clone(), v["aufgabe"].get("frist_bezug"), v["geaendert"].as_bool()),
            (serde_json::Value::Null, None, Some(true)),
            "{v}"
        );
        let verlauf: Vec<(Option<&str>, Option<&str>)> =
            v["aufgabe"]["verlauf"].as_array().unwrap().iter().map(|e| (e["alt"].as_str(), e["neu"].as_str())).collect();
        assert_eq!(
            verlauf,
            [(Some("zusage-da +14 Tage"), Some("2026-12-01")), (Some("2026-12-01"), Some("zusage-da +14 Tage")), (Some("zusage-da +14 Tage"), None)]
        );
        // Inhalt nicht angefasst: die Datei ist, wie der Chat sie schrieb.
        assert_eq!(std::fs::read_to_string(&datei).unwrap(), toml(Some(&tag(-2))));
        let _ = std::fs::remove_dir_all(&ordner);
    }

    /// Etappe B2, „Aufgabe ändern ohne neue Nummer“: Titel, Knöpfe und Frist
    /// ändern; Kennung, Link und Verlauf bleiben, jede Änderung steht im
    /// Verlauf, niemand wird geweckt.
    #[tokio::test]
    async fn aendern_ohne_neue_nummer_mit_verlauf() {
        let state = zustand();
        let id = neu(&state, &format!(r#"{{"projekt": "beispiel", "titel": "Getränkeliste abschicken", "link": "{LINK}", "frist": "2026-11-20"}}"#)).await;
        let vorgaenge_vorher = schicke(&state, get("/api/vorgaenge?fuer=beispiel")).await.1["vorgaenge"].as_array().map_or(0, Vec::len);
        let (status, v) = schicke(
            &state,
            post(
                &format!("/api/aufgaben/{id}"),
                r#"{"titel": "Getränkeliste prüfen und abschicken", "knopf": ["abgeschickt", "später"], "frist": "2026-11-25", "von": "chat:beispiel"}"#,
            ),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{v}");
        let a = &v["aufgabe"];
        assert_eq!((a["id"].as_str(), v["geaendert"].as_bool()), (Some(id.as_str()), Some(true)), "dieselbe Nummer: {v}");
        assert_eq!(a["titel"], "Getränkeliste prüfen und abschicken");
        assert_eq!(a["knopf"], serde_json::json!(["abgeschickt", "später"]));
        assert_eq!((a["frist"].as_str(), a["zustand"].as_str()), (Some("2026-11-25"), Some("offen")));
        assert_eq!(a["link"], LINK, "der Link bleibt");
        let verlauf: Vec<(&str, Option<&str>, Option<&str>)> =
            a["verlauf"].as_array().unwrap().iter().map(|e| (e["art"].as_str().unwrap(), e["alt"].as_str(), e["neu"].as_str())).collect();
        assert_eq!(
            verlauf,
            [
                ("titel", Some("Getränkeliste abschicken"), Some("Getränkeliste prüfen und abschicken")),
                ("knopf", None, Some(r#"["abgeschickt","später"]"#)),
                ("frist", Some("2026-11-20"), Some("2026-11-25")),
            ]
        );
        assert_eq!(a["verlauf"][0]["text"], "Titel geändert: „Getränkeliste abschicken“ → „Getränkeliste prüfen und abschicken“");
        assert_eq!(a["verlauf"][0]["von"], "chat:beispiel");
        // Dasselbe noch einmal: nichts geändert, der Verlauf bleibt bei drei.
        let (status, v) = schicke(&state, post(&format!("/api/aufgaben/{id}"), r#"{"titel": "Getränkeliste prüfen und abschicken"}"#)).await;
        assert_eq!((status, v["geaendert"].as_bool(), v["aufgabe"]["verlauf"].as_array().map(Vec::len)), (StatusCode::OK, Some(false), Some(3)));
        // Knöpfe weg (nur der Haken) und Frist weg: null.
        let (_, v) = schicke(&state, post(&format!("/api/aufgaben/{id}"), r#"{"knopf": null, "frist": null}"#)).await;
        assert_eq!((v["aufgabe"]["knopf"].clone(), v["aufgabe"]["frist"].clone()), (serde_json::Value::Null, serde_json::Value::Null), "{v}");
        assert_eq!(v["aufgabe"]["verlauf"].as_array().map(Vec::len), Some(5), "nichts fällt weg");
        assert_eq!(v["aufgabe"]["verlauf"][4]["text"], "Frist geändert: 2026-11-25 → ohne Frist");
        // Niemand wird geweckt: kein neuer Vorgang.
        let vorgaenge = schicke(&state, get("/api/vorgaenge?fuer=beispiel")).await.1["vorgaenge"].as_array().map_or(0, Vec::len);
        assert_eq!(vorgaenge, vorgaenge_vorher);
        // Die Liste zeigt die geänderte Aufgabe unter derselben Kennung, nur einmal.
        let (_, l) = schicke(&state, get("/api/aufgaben?projekt=beispiel")).await;
        assert_eq!(ids(&l), [id.as_str()]);
        // Der Haken geht mit der neuen Form (ohne Knöpfe).
        let (status, v) = schicke(&state, post_browser(&format!("/api/aufgaben/{id}/erledigt"), "")).await;
        assert_eq!((status, v["aufgabe"]["zustand"].as_str()), (StatusCode::OK, Some("erledigt")), "{v}");
        // Danach: nicht mehr offen, 422.
        let (status, v) = schicke(&state, post(&format!("/api/aufgaben/{id}"), r#"{"titel": "zu spät"}"#)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
        assert_eq!(v["fehler"], SATZ_AENDERN_NICHT_OFFEN);
    }

    /// Nur der Chat des Projekts, nie der Browser; falsche Felder 422; nichts geschrieben.
    #[tokio::test]
    async fn aendern_nur_der_eigene_chat() {
        let state = zustand();
        let id = neu(&state, r#"{"projekt": "beispiel", "titel": "Bauanleitung lesen"}"#).await;
        let (status, v) = schicke(&state, post(&format!("/api/aufgaben/{id}"), r#"{"titel": "Fremd", "von": "chat:anderes"}"#)).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::FORBIDDEN, Some(SATZ_AENDERN_EIGENER_CHAT)));
        let (status, v) = schicke(&state, post_browser(&format!("/api/aufgaben/{id}"), r#"{"titel": "Aus dem Browser"}"#)).await;
        assert_eq!((status, v["fehler"].as_str()), (StatusCode::UNPROCESSABLE_ENTITY, Some(SATZ_AENDERN_NUR_CHAT)));
        for (koerper, satz) in [
            (r#"{}"#, SATZ_AENDERN_NICHTS),
            (r#"{"titel": ""}"#, "Der Titel darf nicht leer sein."),
            (r#"{"titel": null}"#, "Der Titel darf nicht leer sein."),
            (r#"{"knopf": []}"#, "„knopf“ braucht 1 bis 6 Antworten"),
            (r#"{"knopf": ["Etwas anderes …"]}"#, SATZ_KNOPF_ETWAS_ANDERES),
            (r#"{"frist": "morgen"}"#, "„frist“ ist ein Datum"),
            // Löschen nur mit null: leer oder nur Leerzeichen ist ein Versehen.
            (r#"{"frist": ""}"#, "„frist“ ist ein Datum"),
            (r#"{"frist": "   "}"#, "„frist“ ist ein Datum"),
            // Unsichtbare Formatzeichen (Cf) wie beim Anlegen, auch „Etwas anderes“ mit Nullbreite dazwischen.
            // (JSON-Escapes: der Server bekommt die echten Zeichen U+200B, U+200D, U+202E.)
            (r#"{"titel": "Unter\u200blagen"}"#, SATZ_FORMATZEICHEN),
            (r#"{"knopf": ["Etwas\u200danderes"]}"#, SATZ_FORMATZEICHEN),
            (r#"{"knopf": ["ja\u202e", "nein"]}"#, SATZ_FORMATZEICHEN),
            (r#"{"link": "https://x.invalid"}"#, "kein passendes JSON"),
        ] {
            let (status, v) = schicke(&state, post(&format!("/api/aufgaben/{id}"), koerper)).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{koerper}: {v}");
            assert!(v["fehler"].as_str().unwrap_or_default().contains(satz), "{koerper}: {v}");
        }
        let (status, _) = schicke(&state, post("/api/aufgaben/a-20990101-0001", r#"{"titel": "x"}"#)).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let (_, l) = schicke(&state, get("/api/aufgaben?projekt=beispiel")).await;
        assert_eq!(l["aufgaben"][0]["titel"], "Bauanleitung lesen");
        assert!(l["aufgaben"][0].get("verlauf").is_none(), "ohne Änderung kein Verlauf: {l}");
        // Anlegen prüft Titel und Knöpfe genauso (dieselbe Funktion).
        for koerper in [
            r#"{"projekt": "beispiel", "titel": "Abschicken?", "knopf": ["Etwas\u200danderes", "ja"]}"#,
            r#"{"projekt": "beispiel", "titel": "Ab\u200bschicken?"}"#,
        ] {
            let (status, v) = schicke(&state, post("/api/aufgaben", koerper)).await;
            assert_eq!(
                (status, v["fehler"].as_str().is_some_and(|f| f.contains(SATZ_FORMATZEICHEN))),
                (StatusCode::UNPROCESSABLE_ENTITY, true),
                "{koerper}: {v}"
            );
        }
        // Ohne `von` gilt der Chat des Projekts.
        let (status, v) = schicke(&state, post(&format!("/api/aufgaben/{id}"), r#"{"titel": "Bauanleitung lesen und abhaken"}"#)).await;
        assert_eq!((status, v["aufgabe"]["verlauf"][0]["von"].as_str()), (StatusCode::OK, Some("chat:beispiel")));
    }

    /// Eine Aufgabe beim Chat (eigene Worte) steht nach geändertem Titel oder
    /// geänderten Knöpfen wieder für den Inhaber offen; nur die Frist lässt sie beim Chat.
    #[tokio::test]
    async fn aendern_stellt_eine_aufgabe_beim_chat_neu() {
        let state = zustand();
        let id = neu(&state, r#"{"projekt": "beispiel", "titel": "Abschicken?", "knopf": ["ja", "nein"]}"#).await;
        let (status, _) =
            schicke(&state, post_browser(&format!("/api/aufgaben/{id}/erledigt"), r#"{"antwort": "etwas-anderes", "eigene": "Erst die Adresse prüfen."}"#))
                .await;
        assert_eq!(status, StatusCode::OK);
        async fn zustand_in_anstehend(state: &AppState, id: &str) -> Option<String> {
            let (_, l) = schicke(state, get("/api/anstehend?art=aufgabe")).await;
            l["eintraege"].as_array().unwrap().iter().find(|e| e["id"] == id).map(|e| e["zustand"].as_str().unwrap_or_default().to_string())
        }
        let (_, v) = schicke(&state, post(&format!("/api/aufgaben/{id}"), r#"{"frist": "2026-12-01"}"#)).await;
        assert_eq!(v["aufgabe"]["antwort"], "etwas-anderes", "nur die Frist: bleibt beim Chat: {v}");
        assert_eq!(zustand_in_anstehend(&state, &id).await.as_deref(), Some("beim-chat"));
        let (_, v) = schicke(&state, post(&format!("/api/aufgaben/{id}"), r#"{"knopf": ["ja, an die neue Adresse", "nein"]}"#)).await;
        assert_eq!(v["aufgabe"]["antwort"], serde_json::Value::Null, "{v}");
        assert_eq!(v["aufgabe"]["eigene"], "Erst die Adresse prüfen.", "seine Worte bleiben");
        let arten: Vec<&str> = v["aufgabe"]["verlauf"].as_array().unwrap().iter().map(|e| e["art"].as_str().unwrap()).collect();
        assert_eq!(arten, ["frist", "knopf", "wieder-offen"]);
        assert_eq!(zustand_in_anstehend(&state, &id).await.as_deref(), Some("offen"));
        let (_, n) = schicke(&state, get("/api/aufgaben/anzahl")).await;
        assert_eq!(n["offen"].as_u64(), Some(1), "zählt wieder für den Inhaber");
    }

    /// Etappe B2: Das Backlog ist geschützt (401 ohne Anmeldung) und zeigt die
    /// Aufgabe in „Für dich“, mit Anmeldung.
    #[tokio::test]
    async fn backlog_geschuetzt_und_mit_aufgaben() {
        let state = zustand();
        let id = neu(&state, &format!(r#"{{"projekt": "neu-form", "titel": "Bauanleitung lesen", "link": "{LINK}"}}"#)).await;
        let res = crate::app::router(state.clone()).oneshot(get("/api/projekte/neu-form/backlog")).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        let (status, b) = schicke(&state, get("/api/projekte/neu-form/backlog")).await;
        assert_eq!(status, StatusCode::OK, "{b}");
        assert_eq!(b["spalten"][0]["art"], "fuer-dich");
        // Die Frage (gestellt 25.10.) ist älter als die Aufgabe; von zwei Fragen steht nur die erste der Datei da.
        let fuer_dich: Vec<&str> = b["spalten"][0]["karten"].as_array().unwrap().iter().map(|k| k["id"].as_str().unwrap()).collect();
        assert_eq!(fuer_dich, ["schlusssatz", id.as_str()]);
        assert_eq!(b["spalten"][0]["karten"][0]["hinweis"], "noch 1 Frage wartet");
        assert_eq!((b["fragen_weitere"].as_u64(), b["kanban_datei"].as_bool()), (Some(1), Some(true)));
        assert_eq!(b["wartend"][0]["rolle"], "inhaber");
        assert!(!b.to_string().contains(GEHEIM), "kein Link im Backlog");
        // Der Baustein Fragen bekommt dieselbe Auswahl im Abgeleiteten (nur mit Anmeldung).
        let res = router(state.clone()).oneshot(get("/api/projekte/neu-form/daten/fragen")).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let d: serde_json::Value = serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
        assert_eq!(d["abgeleitet"]["warteschlange"], serde_json::json!({"sichtbar": "schlusssatz", "gestellt": "2026-10-25", "weitere": 1}));
        let (status, _) = schicke(&state, get("/api/projekte/gibt-es-nicht/backlog")).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn ohne_speicher_503() {
        let beispiel = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel");
        let state = AppState::fuer_tests_ohne_speicher(&beispiel, "kaputt");
        for req in [get("/api/aufgaben?offen=1"), post("/api/aufgaben", r#"{"projekt": "dashboard", "titel": "t"}"#)] {
            let (status, v) = schicke(&state, req).await;
            assert_eq!((status, v["art"].as_str()), (StatusCode::SERVICE_UNAVAILABLE, Some("speicher-fehlt")));
        }
    }
}
