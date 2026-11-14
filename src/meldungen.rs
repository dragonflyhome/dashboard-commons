//! Meldungen aus Ereignissen (Etappe A3, Konzept „Anstehendes“,
//! [daten] vom_server_berechnet): Der Server erzeugt Meldungen selbst und
//! hebt sie wieder auf, wenn der Grund weg ist. Gespeichert wird in
//! `speicher::meldungen`; jede Änderung sendet `change` mit Bereich
//! `anstehend` (Pfad `anstehend/<projekt>`), damit Band, Zahl am Reiter,
//! Tab-Titel und Unter-Dashboard ohne Neuladen folgen.
//!
//! | Ereignis | Art | Schlüssel | Wiederholung |
//! |---|---|---|---|
//! | Bau rot (je Chat, der gebaut hat) | fehler | `<slug>/bau-rot` | zählt hoch; behoben beim nächsten grünen Bau |
//! | Seite fehlt im Bau (Scanner rot) | fehler | `<slug>/bau-rot` | Zustand |
//! | Prüfung rot (Fehler im Prüfbericht) | fehler | `<slug>/pruefung-rot` | Zustand; behoben, sobald ohne Fehler |
//! | Frist in höchstens zwei Tagen | frist | `<projekt>/frist:<art>-<id>` | Zustand; behoben, wenn erledigt oder verschoben |
//! | Frist vorbei | frist | `<projekt>/frist-vorbei:<art>-<id>` | Zustand; ebenso |
//! | Bote fehlt > 10 min, der Inhaber angemeldet | fehler, System | `dashboard/bote-fehlt` | Zustand; behoben, sobald er da ist |
//! | Ordner nicht lesbar (kein Recht, Hotfix 4d) | fehler, System | `dashboard/lesen:<ordner>` | Zustand; Prüfung beim Start und jede Minute, behoben, sobald lesbar |
//! | Profil des Inhabers unbekannt (Dienst, `konten.inhaber_sid` leer oder ohne Profil) | fehler, System | `dashboard/inhaber-fehlt` | Zustand; Prüfung beim Start, behoben beim nächsten Start mit Profil |
//! | Arbeit auf fertig | fertig | `<projekt>/fertig:<arbeit-id>` | einmal |
//! | Chat meldet fertig mit Verweis | fertig | `<projekt>/fertig:<verweis>` | einmal (dieselbe wie oben) |
//! | Chat meldet ohne Verweis | fertig/info | `<projekt>/<art>:<titel>` | zählt hoch |
//!
//! Systemmeldungen gehören zur PC-Seite und zum Projekt `dashboard`
//! ([ziele] verantwortung); im Band steht „PC“. Die Prüfung läuft beim Start
//! für alle Projekte und danach für jedes Projekt, dessen Dateien sich ändern
//! (Live-Kanal); Fristen und Bote jede Minute. Uhr und Sitzungsabfrage sind
//! Eingaben der Rechnung, damit Tests sie stellen können.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast::error::RecvError;
use tracing::{debug, info, warn};

use crate::anstehend::{self, Inhalt};
use crate::api::vertrag::{AnstehendEintrag, Arbeit, EintragsArt, Meldung, MeldungsArt};
use crate::app::AppState;
use crate::bote::client::BoteStand;
use crate::inhalt::pruefung::{self, Befund};
use crate::pc::sitzung::{self, Bericht, Protokoll, SitzungsAbfrage, SitzungsBefund, SitzungsFehler};
use crate::speicher::Speicher;
use crate::speicher::aufgaben::{self, Zeit};
use crate::speicher::meldungen::{Melden, NeueMeldung, Wiederholung, schluessel};

/// Eine Frist in höchstens so vielen Tagen meldet sich.
pub const FRIST_NAH_TAGE: i64 = 2;
/// So lange darf der Bote fehlen, bevor es eine Meldung gibt.
pub const BOTE_FEHLT_NACH: chrono::Duration = chrono::Duration::minutes(10);
/// Projekt der Systemmeldungen (Betreuer des Dashboards, PC-Seite).
pub const SYSTEM_PROJEKT: &str = "dashboard";
/// So oft schaut die Schleife nach Fristen und Boten.
pub const TAKT: Duration = Duration::from_secs(60);
/// Herkunft der Meldungen, die der Server selbst erzeugt.
pub const VON_SERVER: &str = "server";

pub const SATZ_BOTE_FEHLT: &str = "Der Bote fehlt seit mehr als 10 Minuten, obwohl du angemeldet bist.";
pub const TEXT_BOTE_FEHLT: &str = "Ohne Boten fehlen die Benutzer-Daten der PC-Seite, und die Oberfläche lässt sich nicht bauen. \
     Er startet sonst bei der Anmeldung von selbst; die PC-Seite zeigt, seit wann er fehlt.";

fn neu(projekt: &str, art: MeldungsArt, was: &str, titel: String) -> NeueMeldung {
    NeueMeldung {
        projekt: projekt.to_string(),
        art,
        schluessel: schluessel(projekt, was),
        titel: kuerzen(&titel, crate::api::vorgaenge::TITEL_HOECHSTENS),
        text: None,
        verweis: None,
        system: false,
        geschuetzt: false,
        von: VON_SERVER.to_string(),
    }
}

/// Schneidet auf höchstens `max` Zeichen.
pub fn kuerzen(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut t: String = text.chars().take(max.saturating_sub(1)).collect();
    t.push('…');
    t
}

// ----- Speichern und melden -----

/// Speichert eine Meldung und sendet `change`, wenn der Inhaber etwas Neues sehen
/// soll. Ohne Datenspeicher oder bei einem Fehler: nichts (steht im Protokoll).
pub async fn melden(state: &AppState, neu: NeueMeldung, wiederholung: Wiederholung, jetzt: Zeit) -> Option<Melden> {
    let speicher = state.speicher.clone()?;
    let projekt = neu.projekt.clone();
    match Speicher::im_hintergrund(speicher, move |s| s.meldung_melden(neu, wiederholung, &jetzt)).await {
        Ok(m) => {
            if m.geaendert() {
                let x = m.meldung();
                info!("Meldung {} ({}, {}): {} ×{}", x.id, x.art.wort(), x.schluessel, if matches!(m, Melden::Neu(_)) { "neu" } else { "wieder" }, x.anzahl);
            }
            // Auch ein neuer Satz bei gleichem Zustand („morgen“ statt „in 2 Tagen“) soll
            // zu sehen sein; ist gar nichts geschrieben, bleibt der Live-Kanal still
            // (sonst lüde jede Seite jede Minute neu).
            if m.geschrieben() {
                crate::api::events::anstehend_geaendert(&state.events, Some(&projekt));
            }
            Some(m)
        }
        Err(e) => {
            warn!("Meldung für {projekt}: {e} ({})", e.einzelheiten());
            None
        }
    }
}

/// Hebt Meldungen auf (`behoben`), für die `welche` zutrifft; sendet je Projekt `change`.
pub async fn beheben<F>(state: &AppState, welche: F, jetzt: Zeit) -> usize
where
    F: Fn(&crate::api::vertrag::Meldung) -> bool + Send + 'static,
{
    let Some(speicher) = state.speicher.clone() else { return 0 };
    match Speicher::im_hintergrund(speicher, move |s| s.meldungen_beheben(welche, &jetzt)).await {
        Ok(weg) => {
            let projekte: BTreeSet<String> = weg.iter().map(|m| m.projekt.clone()).collect();
            for m in &weg {
                info!("Meldung {} ({}) behoben", m.id, m.schluessel);
            }
            for p in projekte {
                crate::api::events::anstehend_geaendert(&state.events, Some(&p));
            }
            weg.len()
        }
        Err(e) => {
            warn!("Meldungen beheben: {e} ({})", e.einzelheiten());
            0
        }
    }
}

/// Ein Lauf des Archivs: gelesene und behobene Meldungen nach sieben Tagen
/// (aus dem Minutentakt in `api::aufgaben::spawn_ablauf`).
pub async fn archiv_lauf(state: &AppState, jetzt: Zeit) -> usize {
    let Some(speicher) = state.speicher.clone() else { return 0 };
    match Speicher::im_hintergrund(speicher, move |s| s.meldungen_archivieren(&jetzt)).await {
        Ok(weg) => {
            if !weg.is_empty() {
                info!("{} Meldung(en) ins Archiv verschoben", weg.len());
                let projekte: BTreeSet<String> = weg.iter().map(|m| m.projekt.clone()).collect();
                for p in projekte {
                    crate::api::events::anstehend_geaendert(&state.events, Some(&p));
                }
            }
            weg.len()
        }
        Err(e) => {
            warn!("Archiv der Meldungen: {e} ({})", e.einzelheiten());
            0
        }
    }
}

// ----- Bau -----

/// Ergebnis eines Baus der Oberfläche (aus `baumeister::eine_runde`).
/// Rot: je Chat, der gebaut hat, eine Fehler-Meldung (zählt hoch). Projekte,
/// die der Scanner abgelehnt hat (`rote`), fehlen im Bau: auch sie bekommen
/// eine. Grün: alle Bau-Meldungen außer denen der `rote` sind behoben.
pub async fn bau_ergebnis(state: &AppState, slugs: &[String], gruen: bool, satz: &str, rote: &[String], jetzt: Zeit) {
    if !gruen {
        for slug in slugs {
            let mut n = neu(slug, MeldungsArt::Fehler, "bau-rot", "Der Bau der Oberfläche ist rot.".to_string());
            n.text = Some(kuerzen(satz, crate::api::arbeiten::NOTIZ_HOECHSTENS));
            melden(state, n, Wiederholung::Zaehlen, jetzt).await;
        }
    }
    for slug in rote {
        let mut n = neu(slug, MeldungsArt::Fehler, "bau-rot", "Die Seite fehlt im Bau: Der Scanner hat sie abgelehnt.".to_string());
        n.text = Some("Die Prüfung der Seite (Scanner) hat etwas gefunden; der Bericht steht beim Bau-Vorgang.".to_string());
        melden(state, n, Wiederholung::Zustand, jetzt).await;
    }
    if gruen {
        let rote: BTreeSet<String> = rote.iter().cloned().collect();
        beheben(state, move |m| m.schluessel == schluessel(&m.projekt, "bau-rot") && !rote.contains(&m.projekt), jetzt).await;
    }
}

/// Etappe 4b: Ergebnis eines Baus des Baumeisters (Beobachter oder Vorgang).
/// Bau rot: je beteiligtem Projekt „Der Bau der Oberfläche ist rot“ (auf der
/// Startseite, zählt hoch); die alte Fassung bleibt. Je Projekt, das rot ist:
/// eine Meldung mit dem Satz des Baumeisters, ob es in seiner letzten guten
/// Fassung bleibt oder fehlt. Bau grün: alle anderen Bau-Meldungen sind behoben.
pub async fn bau_ergebnis_4b(state: &AppState, slugs: &[String], e: &crate::vertrag::bote::BauErgebnis, satz: &str, jetzt: Zeit) {
    if !e.gruen {
        for slug in slugs {
            let mut n = neu(slug, MeldungsArt::Fehler, "bau-rot", "Der Bau der Oberfläche ist rot; die letzte gute Fassung bleibt.".to_string());
            n.text = Some(kuerzen(satz, crate::api::arbeiten::NOTIZ_HOECHSTENS));
            melden(state, n, Wiederholung::Zaehlen, jetzt).await;
        }
    }
    let saetze: std::collections::BTreeMap<&str, &str> = e.neu_rot.iter().map(|r| (r.slug.as_str(), r.satz.as_str())).collect();
    let mut noch_rot: BTreeSet<String> = BTreeSet::new();
    for (slug, titel) in e
        .alte_fassung
        .iter()
        .map(|s| (s, "Die Seite bleibt in ihrer letzten guten Fassung: Die Prüfung hat etwas gefunden."))
        .chain(e.rote.iter().map(|s| (s, "Die Seite fehlt im Bau: Die Prüfung hat etwas gefunden.")))
    {
        noch_rot.insert(slug.clone());
        let mut n = neu(slug, MeldungsArt::Fehler, "bau-rot", titel.to_string());
        let text = saetze.get(slug.as_str()).copied().unwrap_or("Einzelheiten mit dashboard.exe pruefe <slug>.");
        n.text = Some(kuerzen(text, crate::api::arbeiten::NOTIZ_HOECHSTENS));
        melden(state, n, Wiederholung::Zustand, jetzt).await;
    }
    if e.gruen {
        beheben(state, move |m| m.schluessel == schluessel(&m.projekt, "bau-rot") && !noch_rot.contains(&m.projekt), jetzt).await;
    }
}

// ----- Prüfung -----

/// Die Meldung zur Prüfung eines Projekts, wenn sie Fehler hat (Hinweise zählen nicht).
pub fn pruefung_meldung(slug: &str, befunde: &[Befund]) -> Option<NeueMeldung> {
    let fehler: Vec<&Befund> = befunde.iter().filter(|b| b.ist_fehler()).collect();
    let erster = fehler.first()?;
    let titel = if fehler.len() == 1 {
        format!("Die Prüfung von „{slug}“ ist rot: 1 Fehler.")
    } else {
        format!("Die Prüfung von „{slug}“ ist rot: {} Fehler.", fehler.len())
    };
    let mut n = neu(slug, MeldungsArt::Fehler, "pruefung-rot", titel);
    let zeile = erster.zeile.map(|z| format!(", Zeile {z}")).unwrap_or_default();
    n.text = Some(kuerzen(&format!("{}{zeile}: {}", erster.datei, erster.text), crate::api::arbeiten::NOTIZ_HOECHSTENS));
    Some(n)
}

/// Ist der Ordner des Projekts wirklich weg? Nur „nicht gefunden“ zählt, und
/// der Projektordner darüber muss sich lesen lassen; jeder andere Fehler
/// (gesperrt, kein Zugriff) ist ein Lesefehler und kein Beheben.
pub fn projekt_weg(projekte: &Path, slug: &str) -> bool {
    std::fs::read_dir(projekte).is_ok() && matches!(std::fs::symlink_metadata(projekte.join(slug)), Err(e) if e.kind() == std::io::ErrorKind::NotFound)
}

/// Wie [`projekt_weg`] über alle Inhaltsquellen (Etappe 4b, Umzug des
/// Inhalts): weg nur, wenn sich jede Quelle lesen lässt und keine das
/// Projekt (noch) hat.
pub fn projekt_weg_aus(quellen: &[PathBuf], slug: &str) -> bool {
    match quellen {
        [] => false,
        [eine] => projekt_weg(eine, slug),
        mehrere => {
            let q = crate::quellen::Quellen::aus_liste(mehrere.to_vec());
            q.eltern(slug).is_none() && q.eintraege.iter().all(|e| std::fs::read_dir(&e.pfad).is_ok())
        }
    }
}

/// Prüft ein Projekt und meldet (oder hebt auf). Gibt es das Projekt nicht
/// mehr (Ordner weg), ist eine offene Prüfungs-Meldung behoben. Liegt der
/// Ordner noch da, aber ohne projekt.toml (etwa während ein Chat schreibt),
/// bleibt sie, wie sie ist. Dateien, die sich nicht lesen lassen, sind in
/// der Prüfung selbst Fehler, also nie ein Beheben.
/// Geprüft wird in der Quelle, in der das Projekt liegt (Etappe 4b).
pub async fn pruefung_lauf(state: &AppState, slug: &str, jetzt: Zeit) {
    let dir = state.projekt_eltern(slug);
    let quellen = state.quellen.to_vec();
    let s = slug.to_string();
    let ergebnis = tokio::task::spawn_blocking(move || {
        if pruefung::wird_geprueft(&dir, &s) { (Some(pruefung::pruefe_projekt(&dir, &s)), false) } else { (None, projekt_weg_aus(&quellen, &s)) }
    })
    .await;
    let (befunde, weg) = match ergebnis {
        Ok(b) => b,
        Err(e) => {
            warn!("Prüfung von {slug} abgebrochen: {e}");
            return;
        }
    };
    let k = schluessel(slug, "pruefung-rot");
    match befunde {
        Some(b) => match pruefung_meldung(slug, &b) {
            Some(n) => {
                melden(state, n, Wiederholung::Zustand, jetzt).await;
            }
            None => {
                beheben(state, move |m| m.schluessel == k, jetzt).await;
            }
        },
        None if weg => {
            beheben(state, move |m| m.schluessel == k, jetzt).await;
        }
        None => debug!("Prüfung von {slug}: keine projekt.toml, der Ordner ist aber da; nichts behoben"),
    }
}

/// Die Entscheidungen wie bei `dashboard pruefe` (unlesbar, unbekannte
/// Projekte): eine eigene Meldung beim System-Projekt, sonst fiele eine kaputte
/// Entscheidung im Betrieb still aus. Neben den Projekten in [`pruefung_alle`].
/// shortcut: läuft nur beim Start und nach einem Überlauf, nicht bei jeder Änderung in docs/entscheidungen; aufrüsten, wenn das Ereignis `docs` hier ankommt.
pub async fn entscheidungen_lauf(state: &AppState, jetzt: Zeit) {
    let ordner = state.entscheidungen_dir.as_ref().clone();
    let quellen = state.quellen.to_vec();
    let befunde = match tokio::task::spawn_blocking(move || crate::werkzeug::pruefe::pruefe_entscheidungen(&ordner, &quellen)).await {
        Ok(b) => b,
        Err(e) => {
            warn!("Prüfung der Entscheidungen abgebrochen: {e}");
            return;
        }
    };
    let fehler: Vec<&Befund> = befunde.iter().filter(|b| b.ist_fehler()).collect();
    let Some(erster) = fehler.first() else {
        let k = schluessel(SYSTEM_PROJEKT, "entscheidungen-rot");
        beheben(state, move |m| m.schluessel == k, jetzt).await;
        return;
    };
    let mut n = neu(SYSTEM_PROJEKT, MeldungsArt::Fehler, "entscheidungen-rot", format!("Die Prüfung der Entscheidungen ist rot: {} Fehler.", fehler.len()));
    let zeile = erster.zeile.map(|z| format!(", Zeile {z}")).unwrap_or_default();
    n.text = Some(kuerzen(&format!("{}{zeile}: {}", erster.datei, erster.text), crate::api::arbeiten::NOTIZ_HOECHSTENS));
    melden(state, n, Wiederholung::Zustand, jetzt).await;
}

/// Prüft alle Projekte aller Inhaltsquellen (beim Start und nach einem
/// Überlauf des Live-Kanals).
pub async fn pruefung_alle(state: &AppState, jetzt: Zeit) {
    let quellen = state.quellen.to_vec();
    let ordner = quellen.clone();
    let alle = match tokio::task::spawn_blocking(move || pruefung::pruefe_quellen(&quellen)).await {
        Ok(a) => a,
        Err(e) => {
            warn!("Prüfung aller Projekte abgebrochen: {e}");
            return;
        }
    };
    entscheidungen_lauf(state, jetzt).await;
    let mut gesehen = BTreeSet::new();
    for (slug, befunde) in &alle {
        gesehen.insert(slug.clone());
        match pruefung_meldung(slug, befunde) {
            Some(n) => {
                melden(state, n, Wiederholung::Zustand, jetzt).await;
            }
            None => {
                let k = schluessel(slug, "pruefung-rot");
                beheben(state, move |m| m.schluessel == k, jetzt).await;
            }
        }
    }
    // Projekte, die es nicht mehr gibt (Ordner weg): ihre Prüfungs-Meldung ist
    // erledigt. Ließ sich der Projektordner nicht lesen, ist nichts weg.
    beheben(
        state,
        move |m| m.schluessel == schluessel(&m.projekt, "pruefung-rot") && !gesehen.contains(&m.projekt) && projekt_weg_aus(&ordner, &m.projekt),
        jetzt,
    )
    .await;
}

/// Welches Projekt eine geänderte Datei betrifft (`projects/<slug>/…`).
pub fn projekt_aus_pfad(pfad: &str) -> Option<&str> {
    let (slug, rest) = pfad.strip_prefix("projects/")?.split_once('/')?;
    (crate::inhalt::seite::is_valid_slug(slug) && !rest.is_empty()).then_some(slug)
}

// ----- Arbeit fertig -----

/// Eine Arbeit steht auf fertig: Meldung „fertig“ (einmal je Arbeit; meldet
/// der Chat zusätzlich fertig mit Verweis auf sie, ist es dieselbe). Als
/// Zustand wie die Fristen: Wurde sie behoben, weil die Arbeit wieder offen
/// war ([`arbeit_wieder_offen`]), kommt sie beim nächsten Fertig wieder.
pub async fn arbeit_fertig(state: &AppState, a: &Arbeit, jetzt: Zeit) {
    let mut n = neu(&a.projekt, MeldungsArt::Fertig, &format!("fertig:{}", a.id), format!("Fertig: {}", a.titel));
    n.verweis = Some(a.id.clone());
    n.von = a.von.clone();
    melden(state, n, Wiederholung::Zustand, jetzt).await;
}

/// Eine fertige Arbeit ist wieder offen (neuer Schritt, Stand zurück): Ihre
/// Meldung „fertig“ stimmt nicht mehr und ist behoben.
pub async fn arbeit_wieder_offen(state: &AppState, a: &Arbeit, jetzt: Zeit) {
    let s = schluessel(&a.projekt, &format!("fertig:{}", a.id));
    beheben(state, move |m| m.schluessel == s, jetzt).await;
}

// ----- Fristen -----

/// Die Frist-Meldungen zu den Einträgen: Frist in höchstens zwei Tagen
/// (`frist:`) oder vorbei (`frist-vorbei:`). Meldungen selbst haben keine Frist.
pub fn fristen_meldungen(eintraege: &[AnstehendEintrag]) -> Vec<NeueMeldung> {
    let mut out = Vec::new();
    for e in eintraege.iter().filter(|e| e.art != EintragsArt::Meldung) {
        let Some(t) = e.tage_bis else { continue };
        let anker = format!("{}-{}", e.art.wort(), e.id);
        let (was, titel) = match t {
            t if t < 0 => {
                let wann = if t == -1 { "seit gestern".to_string() } else { format!("seit {} Tagen", -t) };
                (format!("frist-vorbei:{anker}"), format!("Frist vorbei ({wann}): {}", e.titel))
            }
            0 => (format!("frist:{anker}"), format!("Frist heute: {}", e.titel)),
            1 => (format!("frist:{anker}"), format!("Frist morgen: {}", e.titel)),
            t if t <= FRIST_NAH_TAGE => (format!("frist:{anker}"), format!("Frist in {t} Tagen: {}", e.titel)),
            _ => continue,
        };
        let mut n = neu(&e.projekt, MeldungsArt::Frist, &was, titel);
        n.verweis = Some(e.id.clone());
        n.geschuetzt = e.art == EintragsArt::Aufgabe;
        out.push(n);
    }
    out
}

/// Kommt die Frist-Meldung aus einer Frage (`daten/fragen.toml`)? Aufgaben
/// und Arbeiten kommen aus dem Datenspeicher; der wird ganz gelesen oder der
/// Lauf fällt aus.
pub fn frist_aus_fragen(m: &Meldung) -> bool {
    let frage = EintragsArt::Frage.wort();
    let was = m.schluessel.strip_prefix(&format!("{}/", m.projekt)).unwrap_or("");
    [format!("frist:{frage}-"), format!("frist-vorbei:{frage}-")].iter().any(|v| was.starts_with(v.as_str()))
}

/// Ist der Grund einer Frist-Meldung sicher weg, oder fehlt sie nur, weil
/// eine Datei gerade nicht lesbar war? Lesefehler sind kein Beheben: Fragen
/// eines Projekts, dessen fragen.toml (oder der ganze Projektordner) sich
/// nicht lesen ließ, bleiben, wie sie sind. Ein Projekt, das nicht in der
/// Liste steht, gilt nur als weg, wenn sein Ordner fehlt.
pub fn frist_sicher_weg(m: &Meldung, lesefehler: &anstehend::Lesefehler, projekte: &BTreeSet<String>, quellen: &[PathBuf]) -> bool {
    if !frist_aus_fragen(m) {
        return true;
    }
    if lesefehler.ordner || lesefehler.fragen.contains(&m.projekt) {
        return false;
    }
    projekte.contains(&m.projekt) || projekt_weg_aus(quellen, &m.projekt)
}

/// Fristen aus einem gelesenen Inhalt melden; Frist-Meldungen, deren Grund
/// weg ist (erledigt, verschoben, aus „nah“ wurde „vorbei“), sind behoben,
/// aber nicht die, deren Datei sich gerade nicht lesen ließ ([`frist_sicher_weg`]).
pub async fn fristen_lauf_mit(state: &AppState, inhalt: &Inhalt, jetzt: Zeit) {
    if inhalt.speicher_fehlt {
        return;
    }
    // Je Projekt nur die Frage, die der Inhaber sieht (Etappe 3a+: die Liste hat jetzt alle Fragen).
    // Archivierte Projekte melden sich nicht (Entscheidung archiv-status): keine
    // Frist-Meldung; ihre nahen Fristen stehen neutral in „Anstehend“ (`archiv_fristen`).
    // Frist-Meldungen, die schon da sind, gelten damit als behoben.
    let archiviert = anstehend::archivierte(inhalt);
    let alle: Vec<_> = anstehend::alle_eintraege(inhalt).into_iter().filter(|e| !archiviert.contains(e.projekt.as_str())).collect();
    let sichtbar: Vec<_> = anstehend::eine_frage_je_projekt(&alle).into_iter().cloned().collect();
    let neue = fristen_meldungen(&sichtbar);
    let faellig: BTreeSet<String> = neue.iter().map(|n| n.schluessel.clone()).collect();
    for n in neue {
        melden(state, n, Wiederholung::Zustand, jetzt).await;
    }
    let lesefehler = inhalt.lesefehler.clone();
    if !lesefehler.fragen.is_empty() || lesefehler.ordner {
        debug!("Fristen: Lesefehler bei {:?} (Ordner: {}); deren Frist-Meldungen bleiben", lesefehler.fragen, lesefehler.ordner);
    }
    let projekte: BTreeSet<String> = inhalt.projekte.iter().map(|p| p.slug.clone()).collect();
    let ordner = state.quellen.to_vec();
    beheben(state, move |m| m.art == MeldungsArt::Frist && !faellig.contains(&m.schluessel) && frist_sicher_weg(m, &lesefehler, &projekte, &ordner), jetzt)
        .await;
}

async fn fristen_lauf(state: &AppState, jetzt: Zeit) {
    match crate::api::anstehend::inhalt_sammeln(state).await {
        Ok(inhalt) => fristen_lauf_mit(state, &inhalt, jetzt).await,
        Err(e) => warn!("Fristen: {}", e.fehler),
    }
}

// ----- Bote -----

/// Was mit der Meldung „Bote fehlt“ zu tun ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoteUrteil {
    Nichts,
    Melden,
    Beheben,
}

/// Die Regel ([nach_stellungnahme_designteam]): eine Meldung nur, wenn der Inhaber
/// angemeldet ist und der Bote länger als zehn Minuten fehlt. `fehlt_seit`
/// merkt sich, seit wann er (bei angemeldetem Inhaber) fehlt; abgemeldet zählt
/// die Zeit nicht. Ist er da, ist eine offene Meldung behoben.
pub fn bote_urteil(verbunden: bool, angemeldet: bool, fehlt_seit: &mut Option<Zeit>, jetzt: Zeit) -> BoteUrteil {
    if verbunden {
        *fehlt_seit = None;
        return BoteUrteil::Beheben;
    }
    if !angemeldet {
        *fehlt_seit = None;
        return BoteUrteil::Nichts;
    }
    let seit = *fehlt_seit.get_or_insert(jetzt);
    if jetzt - seit > BOTE_FEHLT_NACH { BoteUrteil::Melden } else { BoteUrteil::Nichts }
}

/// Die Meldung „Bote fehlt“ (System, PC-Seite).
pub fn bote_meldung() -> NeueMeldung {
    let mut n = neu(SYSTEM_PROJEKT, MeldungsArt::Fehler, "bote-fehlt", SATZ_BOTE_FEHLT.to_string());
    n.text = Some(TEXT_BOTE_FEHLT.to_string());
    n.system = true;
    n
}

/// Wendet das Urteil an.
pub async fn bote_lauf_mit(state: &AppState, verbunden: bool, angemeldet: bool, fehlt_seit: &mut Option<Zeit>, jetzt: Zeit) -> BoteUrteil {
    let urteil = bote_urteil(verbunden, angemeldet, fehlt_seit, jetzt);
    urteil_anwenden(state, urteil, jetzt).await;
    urteil
}

/// Meldet oder behebt „Bote fehlt“ nach dem Urteil.
async fn urteil_anwenden(state: &AppState, urteil: BoteUrteil, jetzt: Zeit) {
    match urteil {
        BoteUrteil::Melden => {
            melden(state, bote_meldung(), Wiederholung::Zustand, jetzt).await;
        }
        BoteUrteil::Beheben => {
            let k = schluessel(SYSTEM_PROJEKT, "bote-fehlt");
            beheben(state, move |m| m.schluessel == k, jetzt).await;
        }
        BoteUrteil::Nichts => {}
    }
}

/// Trägt den Befund der Sitzungsprüfung in `state.sitzung` ein (für
/// `/api/betrieb`, mit Quelle) und schreibt beim Wechsel einmal ins
/// Protokoll: `warn`, wenn die Abfrage bei Windows scheitert oder der Inhaber nicht
/// sieht, obwohl der Bote verbunden ist; `info`, wenn sie wieder geht. Gibt
/// zurück, ob der Inhaber angemeldet ist (ohne Quelle: nein, dann keine Meldung).
pub fn sitzung_eintragen(state: &AppState, befund: &SitzungsBefund, bote_verbunden: bool, jetzt: Zeit) -> bool {
    let zeit = jetzt.to_rfc3339_opts(chrono::SecondsFormat::Secs, false);
    let protokoll = match state.sitzung.lock() {
        Ok(mut w) => w.eintragen(befund, bote_verbunden, &zeit),
        Err(_) => None,
    };
    match protokoll {
        Some(Protokoll::Warnung(satz)) => warn!("{satz}"),
        Some(Protokoll::WiederDa) => info!("Sitzungsabfrage geht wieder."),
        None => {}
    }
    befund.inhaber_angemeldet()
}

/// Der letzte Bericht des Boten über seine Sitzung aus seinem Stand.
pub fn bericht(stand: &BoteStand) -> Option<Bericht> {
    let gemeldet = chrono::DateTime::parse_from_rfc3339(stand.sitzung_gemeldet.as_deref()?).ok()?;
    Some(Bericht { sitzung: stand.sitzung, gemeldet })
}

/// Ist der Inhaber angemeldet? Jede Minute, auch wenn der Bote verbunden ist: So
/// zeigt `/api/betrieb`, ob die Abfrage unter dem Dienstkonto geht und welche
/// Quelle gilt (Regel in `pc::sitzung`).
async fn sitzung_pruefen(state: &AppState, abfrage: Arc<dyn SitzungsAbfrage>, stand: &BoteStand, jetzt: Zeit) -> SitzungsBefund {
    let sid = state.einstellungen.as_ref().map(|e| e.konten.inhaber_sid.clone()).unwrap_or_default();
    let (b, verbunden) = (bericht(stand), stand.verbunden);
    let q = abfrage.clone();
    let befund = match tokio::task::spawn_blocking(move || sitzung::pruefen(&*q, b.as_ref(), verbunden, &sid, jetzt)).await {
        Ok(f) => f,
        // Die Konsolennummer braucht keine Rechte und blockiert nicht.
        Err(e) => sitzung::ersatz(SitzungsFehler::Abgebrochen(e.to_string()), b.as_ref(), verbunden, abfrage.konsole(), jetzt),
    };
    sitzung_eintragen(state, &befund, verbunden, jetzt);
    befund
}

/// Ein Takt für den Boten: Sitzung prüfen, dann das Urteil anwenden. Die
/// Abfrage ist ersetzbar (Tests). Sagt Windows sicher „nicht angemeldet“
/// (die gemeldete Sitzung ist weg, die Konsole nicht die des Inhabers), ist eine
/// offene „Bote fehlt“ behoben; bei `unbekannt` bleibt sie offen, bis der
/// Bote wieder da ist.
pub async fn bote_takt(state: &AppState, abfrage: Arc<dyn SitzungsAbfrage>, stand: &BoteStand, fehlt_seit: &mut Option<Zeit>, jetzt: Zeit) -> BoteUrteil {
    let befund = sitzung_pruefen(state, abfrage, stand, jetzt).await;
    let angemeldet = befund.inhaber_angemeldet() || stand.verbunden;
    let urteil = match bote_urteil(stand.verbunden, angemeldet, fehlt_seit, jetzt) {
        BoteUrteil::Nichts if befund.windows_sagt_abgemeldet() => BoteUrteil::Beheben,
        u => u,
    };
    urteil_anwenden(state, urteil, jetzt).await;
    urteil
}

// ----- Profil des Inhabers -----

/// Titel der Störung, wenn der Dienst das Profil des Inhabers nicht kennt.
pub const SATZ_INHABER_FEHLT: &str = "Der Dienst kennt das Konto des Inhabers nicht: Kein Ort geht auf.";
/// Text, wenn `konten.inhaber_sid` leer ist.
pub const TEXT_SID_FEHLT: &str = "In den Einstellungen des Dienstes fehlt `konten.inhaber_sid`. Solange öffnet „Öffnen“ keinen Ort, auch keinen in deinem Profil, \
     und die Prüfung der Orte kennt dein Profil nicht. Die SID trägt Dienst-Einrichten.ps1 ein (als Administrator); danach den Dienst neu starten.";
/// Text, wenn die SID da ist, Windows zu ihr aber kein Profil nennt.
pub const TEXT_PROFIL_FEHLT: &str = "Zur SID in `konten.inhaber_sid` nennt Windows kein Profil (vertippt, oder das Konto gibt es nicht mehr). \
     Solange öffnet „Öffnen“ keinen Ort, auch keinen in deinem Profil. Bitte die SID mit Dienst-Einrichten.ps1 neu eintragen und den Dienst neu starten.";

/// Die Störung „Inhaber unbekannt“ (System, PC-Seite), wenn sie gilt: nur im
/// Dienst (mit Einstellungen), bei leerer SID oder ohne Profil zur SID.
/// Konsole ohne Einstellungen läuft als Inhaber und kennt sein Profil selbst.
pub fn inhaber_meldung(mit_einstellungen: bool, sid: &str, profil_bekannt: bool) -> Option<NeueMeldung> {
    if !mit_einstellungen || profil_bekannt {
        return None;
    }
    let mut n = neu(SYSTEM_PROJEKT, MeldungsArt::Fehler, "inhaber-fehlt", SATZ_INHABER_FEHLT.to_string());
    n.text = Some(if sid.trim().is_empty() { TEXT_SID_FEHLT } else { TEXT_PROFIL_FEHLT }.to_string());
    n.system = true;
    Some(n)
}

/// Meldet die Störung oder hebt sie auf (beim Start; die SID ändert sich nur
/// mit einem Neustart). Mit `warn` im Protokoll, wenn sie gilt.
pub async fn inhaber_lauf_mit(state: &AppState, meldung: Option<NeueMeldung>, jetzt: Zeit) {
    match meldung {
        Some(n) => {
            warn!("{} {}", n.titel, n.text.as_deref().unwrap_or(""));
            melden(state, n, Wiederholung::Zustand, jetzt).await;
        }
        None => {
            let k = schluessel(SYSTEM_PROJEKT, "inhaber-fehlt");
            beheben(state, move |m| m.schluessel == k, jetzt).await;
        }
    }
}

/// Ein Lauf mit den Einstellungen dieses Dienstes (Profil aus der Registry).
pub async fn inhaber_lauf(state: &AppState, jetzt: Zeit) {
    let e = state.einstellungen.clone();
    let meldung = tokio::task::spawn_blocking(move || {
        let e = e.as_deref();
        let sid = e.map(|e| e.konten.inhaber_sid.clone()).unwrap_or_default();
        inhaber_meldung(e.is_some(), &sid, crate::oeffnen::inhaber_profil(e).is_some())
    })
    .await
    .unwrap_or(None);
    inhaber_lauf_mit(state, meldung, jetzt).await;
}

// ----- Lesbarkeit der Ordner (Hotfix 4d) -----

/// Ein Ordner, den der Server liest: Kennung für den Schlüssel der Meldung,
/// kurzer Name für den Satz (nie der volle Pfad mit dem Benutzernamen) und
/// der Pfad selbst.
#[derive(Debug, Clone)]
pub struct GelesenerOrdner {
    pub kennung: String,
    pub name: String,
    pub pfad: PathBuf,
}

impl GelesenerOrdner {
    fn neu(kennung: &str, name: &str, pfad: PathBuf) -> Self {
        GelesenerOrdner { kennung: kennung.to_string(), name: name.to_string(), pfad }
    }
}

/// Alle Ordner, die der Server liest: jede Inhaltsquelle (`projects`, bei
/// mehreren `projects (2)` …), `data` (der Daten-Ordner des Inhalts), `web`
/// (der Ordner der Oberfläche, aus dem er gerade ausliefert),
/// `docs\entscheidungen` (seit dem Umzug im privaten Inhalt),
/// `docs\zielbild` und `docs\anleitung` (Code-Repo, die Anleitung seit 6b).
/// Dieselben Ordner bekommen in
/// Dienst-Einrichten.ps1 das Lese-Recht.
pub fn gelesene_ordner(state: &AppState) -> Vec<GelesenerOrdner> {
    let docs = crate::api::system_seiten::docs_ordner(state);
    let mut v: Vec<GelesenerOrdner> = Vec::new();
    for (nr, q) in state.quellen.iter().enumerate() {
        let kennung = if nr == 0 { "projects".to_string() } else { format!("projects-{}", nr + 1) };
        v.push(GelesenerOrdner::neu(&kennung, &crate::quellen::name(nr), q.clone()));
    }
    v.push(GelesenerOrdner::neu("data", "data", state.daten_ordner()));
    v.push(GelesenerOrdner::neu("web", "web", state.web.ordner()));
    v.push(GelesenerOrdner::neu("docs-entscheidungen", r"docs\entscheidungen", crate::api::system_seiten::entscheidungen_ordner(state)));
    v.push(GelesenerOrdner::neu("docs-zielbild", r"docs\zielbild", docs.join("zielbild")));
    // Etappe 6b: die Anleitung, neben dem Zielbild im Code-Repo.
    v.push(GelesenerOrdner::neu("docs-anleitung", r"docs\anleitung", docs.join("anleitung")));
    v
}

/// Lässt sich der Ordner mit den Rechten dieses Prozesses lesen? Öffnet ihn
/// und liest den ersten Eintrag (unter Windows scheitert schon das Öffnen
/// ohne Recht zum Auflisten).
pub fn ordner_lesbar(pfad: &Path) -> std::io::Result<()> {
    let mut rd = std::fs::read_dir(pfad)?;
    if let Some(e) = rd.next() {
        e?;
    }
    Ok(())
}

/// Die Systemmeldung „darf nicht lesen“ für einen Ordner (PC-Seite).
pub fn lese_meldung(o: &GelesenerOrdner, e: &std::io::Error) -> NeueMeldung {
    let (titel, text) = if e.kind() == std::io::ErrorKind::PermissionDenied {
        (
            format!("Das Dashboard darf den Ordner {} nicht lesen.", o.name),
            format!(
                "Was dort liegt, fehlt auf den Seiten. {} Die Meldung verschwindet danach binnen einer Minute von selbst.",
                crate::api::system_seiten::leserecht_hilfe(&o.name)
            ),
        )
    } else {
        (
            format!("Das Dashboard kann den Ordner {} gerade nicht lesen.", o.name),
            "Was dort liegt, fehlt auf den Seiten. Einzelheiten stehen im Protokoll; die Meldung verschwindet von selbst, sobald der Ordner wieder lesbar ist."
                .to_string(),
        )
    };
    let mut n = neu(SYSTEM_PROJEKT, MeldungsArt::Fehler, &format!("lesen:{}", o.kennung), titel);
    n.text = Some(text);
    n.system = true;
    n
}

/// Prüft jeden gelesenen Ordner mit `pruefen` (im Betrieb `ordner_lesbar`;
/// Tests setzen einen Fehler ein). Ein unlesbarer Ordner wird eine
/// Systemmeldung (Zustand: einmal, nicht je Lauf); ein lesbarer hebt seine
/// auf. Ein fehlender Ordner ist kein Rechte-Problem und bleibt still (die
/// Seiten zeigen dann ihren Platzhalter). Gibt die Namen der unlesbaren zurück.
pub async fn lesbarkeit_lauf_mit<F>(state: &AppState, pruefen: F, jetzt: Zeit) -> Vec<String>
where
    F: Fn(&Path) -> std::io::Result<()> + Send + 'static,
{
    let ordner = gelesene_ordner(state);
    let ergebnisse = match tokio::task::spawn_blocking(move || {
        ordner
            .into_iter()
            .map(|o| {
                let r = pruefen(&o.pfad);
                (o, r)
            })
            .collect::<Vec<_>>()
    })
    .await
    {
        Ok(v) => v,
        Err(e) => {
            warn!("Lesbarkeit der Ordner nicht geprüft: {e}");
            return Vec::new();
        }
    };
    let mut nicht = Vec::new();
    let mut gut = BTreeSet::new();
    for (o, r) in ergebnisse {
        match r {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                let gemeldet = melden(state, lese_meldung(&o, &e), Wiederholung::Zustand, jetzt).await;
                if matches!(gemeldet, Some(Melden::Neu(_)) | Some(Melden::Hochgezaehlt(_))) {
                    warn!("Das Dashboard darf den Ordner {} nicht lesen ({e}).", o.name);
                }
                nicht.push(o.name);
            }
            _ => {
                gut.insert(schluessel(SYSTEM_PROJEKT, &format!("lesen:{}", o.kennung)));
            }
        }
    }
    beheben(state, move |m| gut.contains(&m.schluessel), jetzt).await;
    nicht
}

/// Ein Lauf mit den echten Rechten dieses Prozesses.
pub async fn lesbarkeit_lauf(state: &AppState, jetzt: Zeit) -> Vec<String> {
    lesbarkeit_lauf_mit(state, ordner_lesbar, jetzt).await
}

// ----- Schleife -----

/// Startet die Schleife (eine je Dienst, nur mit Datenspeicher): Prüfung
/// aller Projekte und der Lesbarkeit aller gelesenen Ordner beim Start,
/// danach je geändertem Projekt; jede Minute Fristen, Bote und Lesbarkeit
/// (so ist die Meldung binnen einer Minute weg, wenn der Inhaber das Recht setzt).
pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        let abschaltung = state.abschaltung();
        tokio::pin!(abschaltung);
        let mut kanal = state.events.subscribe();
        pruefung_alle(&state, aufgaben::jetzt()).await;
        if lesbarkeit_lauf(&state, aufgaben::jetzt()).await.is_empty() {
            info!("Alle gelesenen Ordner sind lesbar (projects, data, web, docs\\entscheidungen, docs\\zielbild, docs\\anleitung, soweit vorhanden).");
        }
        // Im Dienst ohne Profil des Inhabers (leere SID): eine Störung auf der PC-Seite.
        inhaber_lauf(&state, aufgaben::jetzt()).await;
        let mut takt = tokio::time::interval(TAKT);
        takt.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut fehlt_seit: Option<Zeit> = None;
        let wts: Arc<dyn SitzungsAbfrage> = Arc::new(sitzung::Wts);
        loop {
            tokio::select! {
                _ = takt.tick() => {
                    let jetzt = aufgaben::jetzt();
                    fristen_lauf(&state, jetzt).await;
                    lesbarkeit_lauf(&state, jetzt).await;
                    if let Some(bote) = state.bote.clone() {
                        bote_takt(&state, wts.clone(), &bote.stand(), &mut fehlt_seit, jetzt).await;
                    }
                }
                c = kanal.recv() => match c {
                    Ok(c) if c.area == "projects" => {
                        if let Some(slug) = projekt_aus_pfad(&c.path) {
                            pruefung_lauf(&state, slug, aufgaben::jetzt()).await;
                        }
                    }
                    Ok(_) => {}
                    Err(RecvError::Lagged(_)) => pruefung_alle(&state, aufgaben::jetzt()).await,
                    Err(RecvError::Closed) => return,
                },
                _ = &mut abschaltung => return,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::{FixedOffset, TimeZone};

    use super::*;
    use crate::api::vertrag::{Stufe, StufenAngabe, StufenGrund};
    use crate::pc::sitzung::tests::FalscheAbfrage;
    use crate::speicher::meldungen::offen;
    use crate::vertrag::bote::SitzungsZustand;

    fn beispiel() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel")
    }

    fn zeit(tag: u32, stunde: u32, minute: u32) -> Zeit {
        FixedOffset::east_opt(3600).unwrap().with_ymd_and_hms(2026, 10, tag, stunde, minute, 0).unwrap()
    }

    fn meldungen(state: &AppState) -> Vec<crate::api::vertrag::Meldung> {
        state.speicher().unwrap().meldungen_lesen(None).unwrap()
    }

    /// Hotfix 4d: Ein Ordner, den der Prozess nicht lesen darf, wird eine
    /// Systemmeldung auf der PC-Seite mit kurzem Namen (ohne Pfad); ein
    /// fehlender bleibt still; ist er wieder lesbar, ist sie behoben.
    #[tokio::test]
    async fn unlesbarer_ordner_wird_systemmeldung() {
        use std::io::{Error, ErrorKind};
        let state = AppState::fuer_tests(&beispiel());
        let docs = crate::api::system_seiten::docs_ordner(&state);
        let gesperrt = docs.join("entscheidungen");
        let fehlt = docs.join("zielbild");
        let sperre = {
            let (g, f) = (gesperrt.clone(), fehlt.clone());
            move |p: &Path| {
                if p == g {
                    Err(Error::new(ErrorKind::PermissionDenied, r"Zugriff verweigert: C:\Users\jemand\dev"))
                } else if p == f {
                    Err(Error::from(ErrorKind::NotFound))
                } else {
                    Ok(())
                }
            }
        };
        let mut live = state.events.subscribe();
        assert_eq!(lesbarkeit_lauf_mit(&state, sperre.clone(), zeit(31, 9, 0)).await, [r"docs\entscheidungen"]);
        assert_eq!(lesbarkeit_lauf_mit(&state, sperre, zeit(31, 9, 1)).await, [r"docs\entscheidungen"]);
        let m = meldungen(&state);
        assert_eq!(m.len(), 1, "einmal, nicht je Lauf; der fehlende Ordner meldet nichts: {m:?}");
        assert_eq!((m[0].system, m[0].projekt.as_str(), m[0].anzahl, m[0].art), (true, "dashboard", 1, MeldungsArt::Fehler));
        assert_eq!(m[0].titel, r"Das Dashboard darf den Ordner docs\entscheidungen nicht lesen.");
        let text = m[0].text.as_deref().unwrap();
        // Auflage 6d: docs\entscheidungen liegt im privaten Inhalt; dort hilft kein Chat, nur der Administrator.
        assert!(text.contains("dort hilft kein Chat") && text.contains(crate::api::system_seiten::BEFEHL_LESERECHT), "{text}");
        for s in [m[0].titel.as_str(), text] {
            assert!(!s.contains("Users") && !s.contains("jemand") && !s.contains(":\\"), "kein Pfad: {s}");
        }
        assert_eq!(anstehend::meldung_ziel(&m[0]), format!("/pc#meldung-{}", m[0].id));
        let pfade: Vec<String> = std::iter::from_fn(|| live.try_recv().ok()).map(|c| c.path).collect();
        assert!(pfade.contains(&"anstehend/dashboard".to_string()), "{pfade:?}");

        // Der Inhaber hat das Recht gesetzt: alles lesbar, die Meldung ist behoben.
        assert!(lesbarkeit_lauf_mit(&state, |_: &Path| Ok(()), zeit(31, 9, 2)).await.is_empty());
        let m = meldungen(&state);
        assert!(m[0].behoben.is_some() && !offen(&m[0]), "{m:?}");
    }

    /// Kennt der Dienst das Profil des
    /// Inhabers nicht (leere `konten.inhaber_sid` oder kein Profil zur SID),
    /// ist das eine Störung auf der PC-Seite (System, Fehler), einmal, nicht je
    /// Lauf; mit Profil ist sie behoben. Die Konsole ohne Einstellungen läuft
    /// als Inhaber und meldet nichts.
    #[tokio::test]
    async fn fehlende_inhaber_sid_ist_stoerung() {
        assert!(inhaber_meldung(false, "", false).is_none(), "Konsole ohne Einstellungen");
        assert!(inhaber_meldung(true, "S-1-5-21-1-2", true).is_none(), "Profil bekannt");
        let leer = inhaber_meldung(true, " ", false).expect("leere SID");
        assert_eq!((leer.system, leer.art, leer.schluessel.as_str()), (true, MeldungsArt::Fehler, "dashboard/inhaber-fehlt"));
        assert_eq!(leer.text.as_deref(), Some(TEXT_SID_FEHLT));
        assert_eq!(inhaber_meldung(true, "S-1-5-21-1-2", false).unwrap().text.as_deref(), Some(TEXT_PROFIL_FEHLT));

        let state = AppState::fuer_tests(&beispiel());
        inhaber_lauf_mit(&state, Some(leer.clone()), zeit(31, 9, 0)).await;
        inhaber_lauf_mit(&state, Some(leer), zeit(31, 9, 1)).await;
        let m = meldungen(&state);
        assert_eq!(m.len(), 1, "{m:?}");
        assert_eq!((m[0].system, m[0].projekt.as_str(), m[0].anzahl, m[0].titel.as_str()), (true, "dashboard", 1, SATZ_INHABER_FEHLT));
        assert_eq!(anstehend::meldung_ziel(&m[0]), format!("/pc#meldung-{}", m[0].id));
        // Nach dem Neustart mit Profil: behoben.
        inhaber_lauf_mit(&state, None, zeit(31, 9, 2)).await;
        let m = meldungen(&state);
        assert!(m[0].behoben.is_some() && !offen(&m[0]), "{m:?}");
        // Der Lauf mit den Einstellungen des Tests (keine): nichts Neues.
        inhaber_lauf(&state, zeit(31, 9, 3)).await;
        assert_eq!(meldungen(&state).iter().filter(|m| offen(m)).count(), 0);
    }

    /// Mit den echten Rechten: Das Beispiel-Repo ist ganz lesbar, ein
    /// fehlender Ordner zählt nicht als unlesbar.
    #[tokio::test]
    async fn beispiel_ist_lesbar() {
        let state = AppState::fuer_tests(&beispiel());
        assert!(lesbarkeit_lauf(&state, zeit(31, 9, 0)).await.is_empty());
        assert!(meldungen(&state).is_empty());
        assert_eq!(ordner_lesbar(&beispiel().join("gibt-es-nicht")).unwrap_err().kind(), std::io::ErrorKind::NotFound);
        let namen: Vec<String> = gelesene_ordner(&state).into_iter().map(|o| o.name).collect();
        assert_eq!(namen, ["projects", "data", "web", r"docs\entscheidungen", r"docs\zielbild", r"docs\anleitung"]);
        // Auflage 6d: wer das Lese-Recht setzt, je Ordner (Chat nur im Code-Repo, "kein Chat" im Inhalt; -Inhalt nie, das Skript liest [inhalt]).
        for (name, chat, inhalt) in [
            ("projects", false, true),
            ("projects (2)", false, true),
            ("data", false, true),
            ("web", false, false),
            (r"docs\entscheidungen", false, true),
            (r"docs\zielbild", true, false),
            (r"docs\anleitung", true, false),
        ] {
            let s = crate::api::system_seiten::leserecht_hilfe(name);
            assert_eq!((s.contains("Betreuer des Dashboards"), s.contains("dort hilft kein Chat")), (chat, inhalt), "{name}: {s}");
            assert!(!s.contains("-Inhalt"), "{name}: {s}");
            assert!(s.contains(crate::api::system_seiten::BEFEHL_LESERECHT), "{name}: {s}");
        }
    }

    /// Umzug des Inhalts: Jede Inhaltsquelle, der Daten-Ordner und die
    /// privaten Entscheidungen stehen in der Lese-Prüfung; eine gesperrte
    /// zweite Quelle wird eine Meldung mit kurzem Namen.
    #[tokio::test]
    async fn alle_quellen_in_der_lesepruefung() {
        use std::io::{Error, ErrorKind};
        let extra = std::env::temp_dir().join(format!("dashboard-lesen-quellen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&extra);
        std::fs::create_dir_all(extra.join("beispiel-a")).unwrap();
        std::fs::write(extra.join("beispiel-a/projekt.toml"), "schema = 3\n").unwrap();
        let mut state = AppState::fuer_tests(&beispiel()).mit_quellen(vec![beispiel().join("projects"), extra.clone()]);
        state.entscheidungen_dir = std::sync::Arc::new(extra.join("docs-privat"));
        let o = gelesene_ordner(&state);
        let namen: Vec<&str> = o.iter().map(|o| o.name.as_str()).collect();
        assert_eq!(namen, ["projects", "projects (2)", "data", "web", r"docs\entscheidungen", r"docs\zielbild", r"docs\anleitung"]);
        assert_eq!(o[1].pfad, extra);
        assert_eq!(o[4].pfad, extra.join("docs-privat"));
        let g = extra.clone();
        let nicht =
            lesbarkeit_lauf_mit(&state, move |p: &Path| if p == g { Err(Error::from(ErrorKind::PermissionDenied)) } else { Ok(()) }, zeit(31, 10, 0)).await;
        assert_eq!(nicht, ["projects (2)"]);
        assert!(meldungen(&state).iter().any(|m| m.titel == "Das Dashboard darf den Ordner projects (2) nicht lesen."));
        let _ = std::fs::remove_dir_all(&extra);
    }

    /// Bote: erst nach mehr als zehn Minuten und nur, wenn der Inhaber angemeldet
    /// ist; abgemeldet zählt die Zeit nicht; ist er wieder da, behoben.
    #[test]
    fn bote_regel_mit_uhr_und_sitzung() {
        let mut seit = None;
        assert_eq!(bote_urteil(false, true, &mut seit, zeit(30, 8, 0)), BoteUrteil::Nichts);
        assert_eq!(bote_urteil(false, true, &mut seit, zeit(30, 8, 10)), BoteUrteil::Nichts, "genau zehn Minuten: noch nicht");
        assert_eq!(bote_urteil(false, true, &mut seit, zeit(30, 8, 11)), BoteUrteil::Melden);
        // Abgemeldet: keine Meldung, die Zeit beginnt nach der Anmeldung neu.
        assert_eq!(bote_urteil(false, false, &mut seit, zeit(30, 23, 0)), BoteUrteil::Nichts);
        assert_eq!(seit, None);
        assert_eq!(bote_urteil(false, true, &mut seit, zeit(31, 7, 0)), BoteUrteil::Nichts);
        assert_eq!(bote_urteil(false, true, &mut seit, zeit(31, 7, 5)), BoteUrteil::Nichts);
        assert_eq!(bote_urteil(true, true, &mut seit, zeit(31, 7, 6)), BoteUrteil::Beheben);
        assert_eq!(bote_urteil(false, true, &mut seit, zeit(31, 7, 7)), BoteUrteil::Nichts, "neu ab jetzt");
        assert_eq!(bote_urteil(false, true, &mut seit, zeit(31, 7, 18)), BoteUrteil::Melden);
    }

    /// Bote fehlt als Systemmeldung: Ziel PC-Seite, Projekt dashboard, Ampel
    /// rot; wieder da: behoben, Ampel nicht mehr rot.
    #[tokio::test]
    async fn bote_fehlt_systemmeldung_und_behoben() {
        let state = AppState::fuer_tests(&beispiel());
        let mut seit = None;
        let mut live = state.events.subscribe();
        assert_eq!(bote_lauf_mit(&state, false, true, &mut seit, zeit(30, 8, 0)).await, BoteUrteil::Nichts);
        assert!(meldungen(&state).is_empty());
        assert_eq!(bote_lauf_mit(&state, false, false, &mut seit, zeit(30, 8, 20)).await, BoteUrteil::Nichts, "abgemeldet: nichts");
        assert_eq!(bote_lauf_mit(&state, false, true, &mut seit, zeit(30, 8, 21)).await, BoteUrteil::Nichts);
        assert_eq!(bote_lauf_mit(&state, false, true, &mut seit, zeit(30, 8, 32)).await, BoteUrteil::Melden);
        assert_eq!(bote_lauf_mit(&state, false, true, &mut seit, zeit(30, 8, 33)).await, BoteUrteil::Melden);
        let m = meldungen(&state);
        assert_eq!(m.len(), 1, "einmal, nicht je Minute");
        assert_eq!((m[0].system, m[0].projekt.as_str(), m[0].anzahl, m[0].art), (true, "dashboard", 1, MeldungsArt::Fehler));
        assert_eq!(anstehend::meldung_ziel(&m[0]), format!("/pc#meldung-{}", m[0].id));
        let pfade: Vec<String> = std::iter::from_fn(|| live.try_recv().ok()).map(|c| c.path).collect();
        assert!(pfade.contains(&"anstehend/dashboard".to_string()), "{pfade:?}");
        let inhalt = crate::api::anstehend::inhalt_sammeln(&state).await.unwrap();
        assert_eq!(anstehend::stand(&inhalt, "dashboard", true).unwrap().gesamt.ampel, crate::api::vertrag::StandAmpel::Rot);
        bote_lauf_mit(&state, true, true, &mut seit, zeit(30, 8, 40)).await;
        let m = meldungen(&state);
        assert!(m[0].behoben.is_some() && !offen(&m[0]));
        let inhalt = crate::api::anstehend::inhalt_sammeln(&state).await.unwrap();
        assert_ne!(anstehend::stand(&inhalt, "dashboard", true).unwrap().gesamt.ampel, crate::api::vertrag::StandAmpel::Rot);
    }

    /// Bau: rot zählt je Bau hoch (eine Meldung je Chat), Scanner-rote
    /// Projekte bekommen eine, grün hebt alle anderen auf.
    #[tokio::test]
    async fn bau_rot_zaehlt_und_gruen_behebt() {
        let state = AppState::fuer_tests(&beispiel());
        let slugs = vec!["architektur".to_string()];
        bau_ergebnis(&state, &slugs, false, "npm run build ist gescheitert.", &[], zeit(30, 9, 0)).await;
        bau_ergebnis(&state, &slugs, false, "Wieder gescheitert.", &[], zeit(30, 9, 5)).await;
        let m = meldungen(&state);
        assert_eq!(m.len(), 1);
        assert_eq!((m[0].schluessel.as_str(), m[0].anzahl, m[0].text.as_deref()), ("architektur/bau-rot", 2, Some("Wieder gescheitert.")));
        bau_ergebnis(&state, &slugs, true, "Grün.", &["schaufenster".to_string()], zeit(30, 9, 10)).await;
        let m = meldungen(&state);
        let arch = m.iter().find(|x| x.projekt == "architektur").unwrap();
        let web = m.iter().find(|x| x.projekt == "schaufenster").unwrap();
        assert!(arch.behoben.is_some(), "grün: behoben");
        assert!(web.behoben.is_none() && web.schluessel == "schaufenster/bau-rot", "fehlt im Bau: offen");
        bau_ergebnis(&state, &slugs, true, "Grün.", &[], zeit(30, 9, 20)).await;
        assert!(meldungen(&state).iter().all(|x| x.behoben.is_some()));
    }

    /// Prüfung: rot mit Fehler (nicht mit Hinweisen), Text mit Datei und
    /// Zeile; ein zweiter Lauf zählt nicht hoch; ohne Fehler behoben.
    #[tokio::test]
    async fn pruefung_rot_und_wieder_gruen() {
        let befunde = vec![
            Befund::hinweis("projects/x/projekt.toml", Some(1), "Nur ein Hinweis."),
            Befund::fehler("projects/x/daten/gantt.toml", Some(4), "„start“ fehlt."),
        ];
        let n = pruefung_meldung("x", &befunde).unwrap();
        assert_eq!((n.schluessel.as_str(), n.art), ("x/pruefung-rot", MeldungsArt::Fehler));
        assert_eq!(n.titel, "Die Prüfung von „x“ ist rot: 1 Fehler.");
        assert_eq!(n.text.as_deref(), Some("projects/x/daten/gantt.toml, Zeile 4: „start“ fehlt."));
        assert!(pruefung_meldung("x", &befunde[..1]).is_none(), "Hinweise allein sind nicht rot");
        // Im Beispiel-Repo: alle prüfen; die kaputten Beispielprojekte sind rot, die guten nicht.
        let state = AppState::fuer_tests(&beispiel());
        let bericht = crate::inhalt::pruefung::pruefe_alle(&state.projects_dir);
        let rot: BTreeSet<String> = bericht.iter().filter(|(_, b)| b.iter().any(Befund::ist_fehler)).map(|(s, _)| s.clone()).collect();
        pruefung_alle(&state, zeit(30, 10, 0)).await;
        pruefung_alle(&state, zeit(30, 10, 1)).await;
        let m = meldungen(&state);
        let gemeldet: BTreeSet<String> = m.iter().filter(|x| x.schluessel.ends_with("/pruefung-rot")).map(|x| x.projekt.clone()).collect();
        assert_eq!(gemeldet, rot);
        assert!(m.iter().all(|x| x.anzahl == 1), "Zustand zählt nicht je Lauf");
        // Eine Meldung zu einem Projekt, das es nicht gibt, ist nach dem nächsten Lauf behoben.
        let alt = pruefung_meldung("gibt-es-nicht", &befunde).unwrap();
        melden(&state, alt, Wiederholung::Zustand, zeit(30, 10, 2)).await;
        pruefung_alle(&state, zeit(30, 10, 3)).await;
        assert!(meldungen(&state).iter().find(|x| x.projekt == "gibt-es-nicht").unwrap().behoben.is_some());
        assert_eq!(projekt_aus_pfad("projects/schaufenster/projekt.toml"), Some("schaufenster"));
        assert_eq!(projekt_aus_pfad("projects/schaufenster"), None);
        assert_eq!(projekt_aus_pfad("data/pc.toml"), None);
    }

    /// Eine kaputte Entscheidung macht beim Lauf über alle eine eigene Meldung
    /// beim System-Projekt; repariert ist sie behoben.
    #[tokio::test]
    async fn entscheidungen_rot_und_wieder_gruen() {
        let ordner = std::env::temp_dir().join(format!("dashboard-meldung-entscheidungen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        std::fs::create_dir_all(&ordner).unwrap();
        std::fs::write(ordner.join("a.toml"), "thema = \"A\"\npfad = \"C:\\qordner\"\n").unwrap();
        let mut state = AppState::fuer_tests(&beispiel());
        state.entscheidungen_dir = std::sync::Arc::new(ordner.clone());
        pruefung_alle(&state, zeit(30, 11, 0)).await;
        let k = schluessel(SYSTEM_PROJEKT, "entscheidungen-rot");
        let m = meldungen(&state);
        let e = m.iter().find(|x| x.schluessel == k).expect("Meldung zur kaputten Entscheidung");
        assert_eq!((e.projekt.as_str(), e.art, e.behoben.is_none()), (SYSTEM_PROJEKT, MeldungsArt::Fehler, true));
        assert_eq!(e.titel, "Die Prüfung der Entscheidungen ist rot: 1 Fehler.");
        assert!(e.text.as_deref().unwrap_or_default().starts_with("docs/entscheidungen/a.toml, Zeile 2: "), "{:?}", e.text);
        std::fs::write(ordner.join("a.toml"), "thema = \"A\"\npfad = 'C:\\qordner'\n").unwrap();
        pruefung_alle(&state, zeit(30, 11, 1)).await;
        let behoben = meldungen(&state).into_iter().find(|x| x.schluessel == k).unwrap().behoben;
        let _ = std::fs::remove_dir_all(&ordner);
        assert!(behoben.is_some(), "repariert: behoben");
    }

    fn eintrag(art: EintragsArt, id: &str, tage: Option<i64>) -> AnstehendEintrag {
        AnstehendEintrag {
            art,
            id: id.into(),
            projekt: "dashboard".into(),
            projekt_titel: None,
            gruppe: None,
            schwerpunkt: "PC".into(),
            schwerpunkt_reihenfolge: 5,
            wartet_auf_dich: false,
            titel: format!("Titel {id}"),
            stufe: StufenAngabe { vorschlag: Stufe::Bald, inhaber: None, wirksam: Stufe::Bald, grund: StufenGrund::Vorschlag },
            frist: tage.map(|_| "2026-10-30".into()),
            tage_bis: tage,
            frist_stufe: None,
            zustand: "offen".into(),
            wartet_auf: None,
            verweis: None,
            vermerk: None,
            ziel: String::new(),
            ziel_da: true,
            quelle: String::new(),
            hat_link: false,
            knopf: None,
            gestellt: None,
            frage: None,
            meldung: None,
            von: None,
            erstellt: String::new(),
            eigene: None,
            warteschlange: None,
            frist_bezug: None,
            ereignis: None,
            schritte: Vec::new(),
            offene_verweise: Vec::new(),
        }
    }

    /// Fristen: heute, morgen, in zwei Tagen und vorbei melden sich; drei
    /// Tage nicht; Aufgaben-Titel gelten als geschützt.
    #[test]
    fn fristen_regel() {
        let l = vec![
            eintrag(EintragsArt::Aufgabe, "a-1", Some(0)),
            eintrag(EintragsArt::Arbeit, "ar-1", Some(2)),
            eintrag(EintragsArt::Arbeit, "ar-2", Some(3)),
            eintrag(EintragsArt::Frage, "f", Some(-1)),
            eintrag(EintragsArt::Arbeit, "ar-3", Some(-4)),
            eintrag(EintragsArt::Arbeit, "ar-4", None),
            eintrag(EintragsArt::Meldung, "m-1", Some(-1)),
        ];
        let n = fristen_meldungen(&l);
        let k: Vec<(&str, &str, bool)> = n.iter().map(|m| (m.schluessel.as_str(), m.titel.as_str(), m.geschuetzt)).collect();
        assert_eq!(
            k,
            [
                ("dashboard/frist:aufgabe-a-1", "Frist heute: Titel a-1", true),
                ("dashboard/frist:arbeit-ar-1", "Frist in 2 Tagen: Titel ar-1", false),
                ("dashboard/frist-vorbei:frage-f", "Frist vorbei (seit gestern): Titel f", false),
                ("dashboard/frist-vorbei:arbeit-ar-3", "Frist vorbei (seit 4 Tagen): Titel ar-3", false),
            ]
        );
        assert!(n.iter().all(|m| m.art == MeldungsArt::Frist && m.verweis.is_some()));
    }

    /// Fristen im Lauf: einmal gemeldet (nicht je Minute), neuer Satz ohne
    /// neues „ungelesen“; erledigt: behoben. Die Ampel wird bei „vorbei“ rot.
    #[tokio::test]
    async fn fristen_lauf_meldet_und_behebt() {
        let state = AppState::fuer_tests(&beispiel());
        let jetzt = aufgaben::jetzt();
        let heute = crate::vertrag::abgeleitet::heute_von(&jetzt);
        let tag = |d: i64| (heute + chrono::Duration::days(d)).format("%Y-%m-%d").to_string();
        let s = state.speicher().unwrap();
        let neu_arbeit = |titel: &str, frist: String| crate::speicher::arbeiten::NeueArbeit {
            projekt: "dashboard".into(),
            von: "chat:dashboard".into(),
            titel: titel.into(),
            stand: crate::api::vertrag::ArbeitsStand::Laeuft,
            wartet_auf: None,
            verweis: None,
            prioritaet: Stufe::Bald,
            frist: Some(frist),
            notiz: None,
            etappe: None,
        };
        let morgen = s.arbeit_anlegen(neu_arbeit("Morgen fällig", tag(1)), &jetzt).unwrap();
        let vorbei = s.arbeit_anlegen(neu_arbeit("Schon vorbei", tag(-2)), &jetzt).unwrap();
        s.arbeit_anlegen(neu_arbeit("Noch Zeit", tag(5)), &jetzt).unwrap();
        for _ in 0..2 {
            let inhalt = crate::api::anstehend::inhalt_sammeln(&state).await.unwrap();
            fristen_lauf_mit(&state, &inhalt, jetzt).await;
        }
        let m = meldungen(&state);
        assert_eq!(m.len(), 2, "{m:?}");
        assert!(m.iter().all(|x| x.anzahl == 1 && offen(x)));
        assert!(m.iter().any(|x| x.schluessel == format!("dashboard/frist:arbeit-{}", morgen.id)));
        assert!(m.iter().any(|x| x.schluessel == format!("dashboard/frist-vorbei:arbeit-{}", vorbei.id)));
        let inhalt = crate::api::anstehend::inhalt_sammeln(&state).await.unwrap();
        let st = anstehend::stand(&inhalt, "dashboard", true).unwrap();
        assert_eq!(st.gesamt.ampel, crate::api::vertrag::StandAmpel::Rot);
        // Frist-Meldungen stehen unter Jetzt in der Liste.
        let l = anstehend::anstehend(&inhalt, &anstehend::Filter { art: Some(EintragsArt::Meldung), ..Default::default() });
        assert_eq!(l.eintraege.len(), 2);
        assert!(l.eintraege.iter().all(|e| e.stufe.wirksam == Stufe::Jetzt && e.stufe.grund == StufenGrund::Fest));
        // Die Arbeit mit Frist morgen ist fertig: Meldung „fertig“, Frist-Meldung behoben.
        let fertig = crate::speicher::arbeiten::ArbeitAenderung {
            stand: Some(crate::api::vertrag::ArbeitsStand::Fertig),
            von: "chat:dashboard".into(),
            ..Default::default()
        };
        s.arbeit_fortschreiben(&morgen.id, &fertig, &jetzt).unwrap();
        let inhalt = crate::api::anstehend::inhalt_sammeln(&state).await.unwrap();
        fristen_lauf_mit(&state, &inhalt, jetzt).await;
        let m = meldungen(&state);
        assert!(m.iter().find(|x| x.schluessel.contains(&morgen.id)).unwrap().behoben.is_some());
        assert!(m.iter().find(|x| x.schluessel.contains(&vorbei.id)).unwrap().behoben.is_none());
    }

    /// Ein Projekt im Temp-Ordner (neue Form) mit `daten/fragen.toml`.
    fn temp_projekte(name: &str) -> PathBuf {
        let wurzel = std::env::temp_dir().join(format!("dashboard-test-{}-meldungen-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wurzel);
        let p = wurzel.join("projects").join("probe");
        std::fs::create_dir_all(p.join("daten")).unwrap();
        std::fs::write(
            p.join("projekt.toml"),
            "schema = 3
title = \"Probe\"
status = \"aktiv\"
summary = \"Nur für Tests.\"
",
        )
        .unwrap();
        wurzel
    }

    /// Lesefehler sind kein Beheben: Ist fragen.toml kurz kaputt (ein Chat
    /// schreibt gerade) oder der Projektordner nicht lesbar, bleibt die
    /// Frist-Meldung der Frage, wie sie ist (gelesen, ×1); danach kommt sie
    /// nicht ungelesen mit höherer Zahl wieder. Erst wenn die Frage wirklich
    /// weg ist, ist sie behoben.
    #[tokio::test]
    async fn lesefehler_behebt_keine_frist() {
        let wurzel = temp_projekte("frist");
        let fragen = wurzel.join("projects").join("probe").join("daten").join("fragen.toml");
        let state = AppState::fuer_tests(&wurzel);
        let jetzt = aufgaben::jetzt();
        let gestern = (crate::vertrag::abgeleitet::heute_von(&jetzt) - chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
        let frage = format!(
            "[[frage]]
id = \"f\"
text = \"Passt das?\"
art = \"ja-nein\"
gestellt = \"2026-10-01\"
frist = \"{gestern}\"
"
        );
        std::fs::write(&fragen, &frage).unwrap();
        let lauf = |state: AppState| async move {
            let inhalt = crate::api::anstehend::inhalt_sammeln(&state).await.unwrap();
            fristen_lauf_mit(&state, &inhalt, aufgaben::jetzt()).await;
        };
        lauf(state.clone()).await;
        let m = meldungen(&state);
        assert_eq!(m.len(), 1, "{m:?}");
        assert_eq!(m[0].schluessel, "probe/frist-vorbei:frage-f");
        assert!(frist_aus_fragen(&m[0]));
        state.speicher().unwrap().meldung_gelesen(&m[0].id, &jetzt).unwrap();
        // Kaputt (halb geschrieben): nicht behoben.
        std::fs::write(
            &fragen,
            "[[frage]]
id = \"f",
        )
        .unwrap();
        lauf(state.clone()).await;
        assert!(meldungen(&state)[0].behoben.is_none(), "Lesefehler ist kein Beheben");
        // Ganzer Projektordner nicht lesbar (hier: weggerückt): auch nicht.
        let projekte = wurzel.join("projects");
        let weg = wurzel.join("projects-weg");
        std::fs::write(&fragen, &frage).unwrap();
        std::fs::rename(&projekte, &weg).unwrap();
        lauf(state.clone()).await;
        assert!(meldungen(&state)[0].behoben.is_none(), "Ordner unlesbar ist kein Beheben");
        std::fs::rename(&weg, &projekte).unwrap();
        // Wieder lesbar: dieselbe Meldung, gelesen, ×1.
        lauf(state.clone()).await;
        let m = meldungen(&state);
        assert_eq!((m.len(), m[0].anzahl, m[0].gelesen.is_some(), m[0].behoben.is_none()), (1, 1, true, true), "{m:?}");
        // Die Frage ist wirklich weg (Datei lesbar, ohne sie): behoben.
        std::fs::write(&fragen, "").unwrap();
        lauf(state.clone()).await;
        assert!(meldungen(&state)[0].behoben.is_some(), "wirklich weg: behoben");
        std::fs::remove_dir_all(&wurzel).unwrap();
    }

    /// Prüfung: Ein Projektordner ohne projekt.toml (ein Chat schreibt
    /// gerade) behebt nichts, auch kein nicht lesbarer
    /// Projektordner; erst wenn der Ordner weg ist.
    #[tokio::test]
    async fn pruefung_behebt_nur_wenn_der_ordner_weg_ist() {
        let wurzel = temp_projekte("pruefung");
        let projekte = wurzel.join("projects");
        let state = AppState::fuer_tests(&wurzel);
        let befunde = vec![Befund::fehler("projects/probe/projekt.toml", Some(1), "Kaputt.")];
        melden(&state, pruefung_meldung("probe", &befunde).unwrap(), Wiederholung::Zustand, zeit(30, 10, 0)).await;
        std::fs::remove_file(projekte.join("probe").join("projekt.toml")).unwrap();
        pruefung_lauf(&state, "probe", zeit(30, 10, 1)).await;
        pruefung_alle(&state, zeit(30, 10, 2)).await;
        assert!(meldungen(&state)[0].behoben.is_none(), "Ordner noch da: nichts behoben");
        let weg = wurzel.join("projects-weg");
        std::fs::rename(&projekte, &weg).unwrap();
        pruefung_alle(&state, zeit(30, 10, 3)).await;
        pruefung_lauf(&state, "probe", zeit(30, 10, 3)).await;
        assert!(meldungen(&state)[0].behoben.is_none(), "Projektordner unlesbar: nichts behoben");
        std::fs::rename(&weg, &projekte).unwrap();
        std::fs::remove_dir_all(projekte.join("probe")).unwrap();
        pruefung_alle(&state, zeit(30, 10, 4)).await;
        let m = meldungen(&state);
        assert!(m[0].behoben.is_some() && m[0].anzahl == 1, "Ordner weg: behoben {m:?}");
        std::fs::remove_dir_all(&wurzel).unwrap();
    }

    async fn betrieb_sitzung(state: AppState) -> serde_json::Value {
        use axum::body::Body;
        use http_body_util::BodyExt;
        use tower::ServiceExt;
        // Seit der Kopfleiste, Fassung 2, nur im geschützten vollen Bericht; frei ist sie null.
        let req = axum::http::Request::get("/api/betrieb").header("host", "127.0.0.1:8787").body(Body::empty()).unwrap();
        let res = crate::app::router(state.clone()).oneshot(req).await.unwrap();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        assert!(serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["sitzung"].is_null(), "frei ohne Sitzung");
        let req = axum::http::Request::get("/api/betrieb/voll").header("host", "127.0.0.1:8787").body(Body::empty()).unwrap();
        let res = crate::app::router_angemeldet(state).oneshot(req).await.unwrap();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["sitzung"].clone()
    }

    /// Der Stand des Boten nach einem Hallo um `gemeldet` aus Sitzung 2.
    fn bote_stand(verbunden: bool, gemeldet: Zeit) -> BoteStand {
        BoteStand {
            verbunden,
            sitzung: Some(crate::vertrag::bote::BoteSitzung { id: 2, konsole: true, zustand: Some(SitzungsZustand::Aktiv) }),
            sitzung_gemeldet: Some(gemeldet.to_rfc3339()),
            ..Default::default()
        }
    }

    fn abfrage(q: FalscheAbfrage) -> Arc<dyn SitzungsAbfrage> {
        Arc::new(q)
    }

    /// „Bote fehlt“ fällt nicht still aus: Der Befund steht in /api/betrieb
    /// (geht, angemeldet, Quelle, Fehlerart, Zeit; ohne SID und Namen).
    #[tokio::test]
    async fn sitzungsabfrage_in_betrieb() {
        let state = AppState::fuer_tests(&beispiel());
        assert_eq!(
            betrieb_sitzung(state.clone()).await,
            serde_json::json!({"geht": null, "inhaber_angemeldet": null, "quelle": null, "fehler": null, "geprueft": null})
        );
        let kaputt =
            abfrage(FalscheAbfrage { konsole: Some(1), kaputt: Some(SitzungsFehler::Wts("Zugriff verweigert (os error 5)".into())), ..Default::default() });
        // Ohne Bericht des Boten: unbekannt, zählt als abgemeldet.
        let mut seit = None;
        bote_takt(&state, kaputt.clone(), &BoteStand::default(), &mut seit, zeit(30, 8, 0)).await;
        assert_eq!(seit, None, "scheitert ohne Bericht: zählt als abgemeldet");
        assert_eq!(
            betrieb_sitzung(state.clone()).await,
            serde_json::json!({"geht": false, "inhaber_angemeldet": null, "quelle": "unbekannt", "fehler": "wts", "geprueft": "2026-10-30T08:00:00+01:00"})
        );
        let geht = abfrage(FalscheAbfrage::default().mit(2, SitzungsZustand::Aktiv, None));
        bote_takt(&state, geht, &bote_stand(true, zeit(30, 8, 1)), &mut seit, zeit(30, 8, 1)).await;
        assert_eq!(
            betrieb_sitzung(state.clone()).await,
            serde_json::json!({"geht": true, "inhaber_angemeldet": true, "quelle": "windows", "fehler": null, "geprueft": "2026-10-30T08:01:00+01:00"})
        );
    }

    /// Hotfix Sitzungsabfrage, ganzer Ablauf mit erfundener Windows-Seite:
    /// Der Bote meldet Sitzung 2 und verschwindet. Solange Windows Sitzung 2
    /// angemeldet nennt, kommt „Bote fehlt“ nach mehr als zehn Minuten; kommt
    /// er wieder, ist sie behoben; ist die Sitzung weg, kommt nichts.
    #[tokio::test]
    async fn bote_fehlt_nach_gemeldeter_sitzung() {
        let state = AppState::fuer_tests(&beispiel());
        let da = abfrage(FalscheAbfrage { konsole: Some(2), ..Default::default() }.mit(2, SitzungsZustand::Aktiv, None));
        let mut seit = None;
        let hallo = zeit(30, 8, 0);
        assert_eq!(bote_takt(&state, da.clone(), &bote_stand(true, hallo), &mut seit, hallo).await, BoteUrteil::Beheben);
        // Bote weg, Sitzung noch da.
        for minute in [1, 6, 11] {
            assert_eq!(bote_takt(&state, da.clone(), &bote_stand(false, hallo), &mut seit, zeit(30, 8, minute)).await, BoteUrteil::Nichts, "{minute}");
        }
        assert!(meldungen(&state).is_empty());
        assert_eq!(bote_takt(&state, da.clone(), &bote_stand(false, hallo), &mut seit, zeit(30, 8, 12)).await, BoteUrteil::Melden);
        let m = meldungen(&state);
        assert_eq!((m.len(), m[0].schluessel.as_str(), m[0].behoben.is_some()), (1, "dashboard/bote-fehlt", false));
        // Er kommt wieder: behoben.
        let wieder = zeit(30, 8, 15);
        assert_eq!(bote_takt(&state, da.clone(), &bote_stand(true, wieder), &mut seit, wieder).await, BoteUrteil::Beheben);
        assert!(meldungen(&state)[0].behoben.is_some(), "behoben");
        // Der Inhaber meldet sich ab: Sitzung 2 weg, an der Konsole der Anmeldebildschirm.
        let weg = abfrage(FalscheAbfrage { konsole: Some(3), ..Default::default() }.mit(3, SitzungsZustand::Anderer, None));
        for minute in [20, 31, 45] {
            assert_eq!(
                bote_takt(&state, weg.clone(), &bote_stand(false, wieder), &mut seit, zeit(30, 8, minute)).await,
                BoteUrteil::Beheben,
                "Windows sagt sicher abgemeldet"
            );
        }
        assert_eq!(seit, None);
        assert!(meldungen(&state).iter().all(|m| m.behoben.is_some()), "keine neue Meldung");
        let s = betrieb_sitzung(state.clone()).await;
        assert_eq!((&s["geht"], &s["inhaber_angemeldet"], &s["quelle"]), (&serde_json::json!(true), &serde_json::json!(false), &serde_json::json!("windows")));
    }

    fn wts_259() -> SitzungsFehler {
        SitzungsFehler::Wts("WTSQuerySessionInformationW: Es sind keine Daten mehr verfügbar. (os error 259)".into())
    }

    /// Windows antwortet nicht (259 unter dem Dienstkonto), der Bote hatte
    /// Sitzung 2 an der Konsole gemeldet und verschwindet, die Konsole hat
    /// weiter Nummer 2: „Bote fehlt“ kommt nach mehr als zehn Minuten; kommt
    /// er wieder, ist sie behoben.
    #[tokio::test]
    async fn bote_fehlt_bei_gleicher_konsolennummer() {
        let state = AppState::fuer_tests(&beispiel());
        let kaputt = abfrage(FalscheAbfrage { konsole: Some(2), kaputt: Some(wts_259()), ..Default::default() });
        let mut seit = None;
        let hallo = zeit(30, 8, 0);
        assert_eq!(bote_takt(&state, kaputt.clone(), &bote_stand(true, hallo), &mut seit, hallo).await, BoteUrteil::Beheben);
        assert_eq!(bote_takt(&state, kaputt.clone(), &bote_stand(false, hallo), &mut seit, zeit(30, 8, 1)).await, BoteUrteil::Nichts);
        assert_eq!(bote_takt(&state, kaputt.clone(), &bote_stand(false, hallo), &mut seit, zeit(30, 8, 12)).await, BoteUrteil::Melden);
        assert_eq!(
            betrieb_sitzung(state.clone()).await,
            serde_json::json!({"geht": false, "inhaber_angemeldet": true, "quelle": "bote", "fehler": "wts", "geprueft": "2026-10-30T08:12:00+01:00"})
        );
        let wieder = zeit(30, 8, 20);
        assert_eq!(bote_takt(&state, kaputt.clone(), &bote_stand(true, wieder), &mut seit, wieder).await, BoteUrteil::Beheben);
        assert!(meldungen(&state).iter().all(|m| m.behoben.is_some()), "behoben");
    }

    /// Abmeldung ist kein Alarm: Windows antwortet nicht, der Bote ist weg,
    /// und die Konsole hat eine neue Nummer (Anmeldebildschirm). Dann ist
    /// unbekannt, ob der Inhaber angemeldet ist, und es kommt keine Meldung, auch
    /// nicht nach Stunden.
    #[tokio::test]
    async fn abmeldung_ist_kein_alarm() {
        let state = AppState::fuer_tests(&beispiel());
        let vorher = abfrage(FalscheAbfrage { konsole: Some(2), kaputt: Some(wts_259()), ..Default::default() });
        let nachher = abfrage(FalscheAbfrage { konsole: Some(3), kaputt: Some(wts_259()), ..Default::default() });
        let mut seit = None;
        let hallo = zeit(30, 18, 0);
        bote_takt(&state, vorher, &bote_stand(true, hallo), &mut seit, hallo).await;
        for (stunde, minute) in [(18, 1), (18, 12), (18, 30), (20, 0), (23, 59)] {
            let u = bote_takt(&state, nachher.clone(), &bote_stand(false, hallo), &mut seit, zeit(30, stunde, minute)).await;
            assert_eq!(u, BoteUrteil::Nichts, "{stunde}:{minute}");
        }
        assert_eq!(seit, None);
        assert!(meldungen(&state).is_empty(), "kein falscher Alarm");
        let s = betrieb_sitzung(state.clone()).await;
        assert_eq!(
            s,
            serde_json::json!({"geht": false, "inhaber_angemeldet": null, "quelle": "unbekannt", "fehler": "wts", "geprueft": "2026-10-30T23:59:00+01:00"})
        );
    }

    /// Der Rückfall ohne Konsolenbezug gilt nie länger als die Frist bis
    /// „Bote fehlt“: sonst sähe eine Abmeldung wie ein Absturz aus.
    #[test]
    fn bericht_ohne_konsole_kuerzer_als_alarm() {
        assert!(sitzung::BERICHT_OHNE_KONSOLE <= BOTE_FEHLT_NACH);
    }

    /// Ablauf ohne Konsolenbezug (Fernsitzung, `konsole: false`): Windows
    /// antwortet nicht, der Bote verschwindet. Der Bericht gilt höchstens
    /// fünf Minuten, die Zehn-Minuten-Regel wird nie erreicht: kein Alarm.
    #[tokio::test]
    async fn ohne_konsolenbezug_kein_alarm_aus_dem_rueckfall() {
        let state = AppState::fuer_tests(&beispiel());
        let kaputt = abfrage(FalscheAbfrage { konsole: Some(1), kaputt: Some(wts_259()), ..Default::default() });
        let fern = |verbunden, gemeldet: Zeit| {
            let mut s = bote_stand(verbunden, gemeldet);
            s.sitzung = Some(crate::vertrag::bote::BoteSitzung { id: 2, konsole: false, zustand: Some(SitzungsZustand::Aktiv) });
            s
        };
        let mut seit = None;
        let hallo = zeit(30, 8, 0);
        bote_takt(&state, kaputt.clone(), &fern(true, hallo), &mut seit, hallo).await;
        for minute in 1..=40 {
            let u = bote_takt(&state, kaputt.clone(), &fern(false, hallo), &mut seit, zeit(30, 8, minute)).await;
            assert_ne!(u, BoteUrteil::Melden, "Minute {minute}");
        }
        assert!(meldungen(&state).is_empty(), "kein Alarm");
        assert_eq!(betrieb_sitzung(state.clone()).await["quelle"], serde_json::json!("unbekannt"));
    }

    /// Eine offene „Bote fehlt“ ist behoben, sobald Windows sicher „nicht
    /// angemeldet“ sagt (gemeldete Sitzung weg). Bei `unbekannt` bleibt sie
    /// offen; der Bote behebt sie, wenn er wieder da ist.
    #[tokio::test]
    async fn bote_fehlt_behoben_wenn_windows_abgemeldet_sagt() {
        let da = abfrage(FalscheAbfrage { konsole: Some(2), ..Default::default() }.mit(2, SitzungsZustand::Aktiv, None));
        let weg = abfrage(FalscheAbfrage { konsole: Some(3), ..Default::default() }.mit(3, SitzungsZustand::Anderer, None));
        let unbekannt = abfrage(FalscheAbfrage { konsole: Some(3), kaputt: Some(wts_259()), ..Default::default() });
        let hallo = zeit(30, 8, 0);
        let offen = |state: &AppState| meldungen(state).iter().filter(|m| m.behoben.is_none()).count();
        // Windows sagt abgemeldet: behoben.
        let state = AppState::fuer_tests(&beispiel());
        let mut seit = None;
        for minute in [1, 12] {
            bote_takt(&state, da.clone(), &bote_stand(false, hallo), &mut seit, zeit(30, 8, minute)).await;
        }
        assert_eq!(offen(&state), 1, "Alarm offen");
        assert_eq!(bote_takt(&state, weg.clone(), &bote_stand(false, hallo), &mut seit, zeit(30, 8, 13)).await, BoteUrteil::Beheben);
        assert_eq!(offen(&state), 0, "Windows sagt abgemeldet: behoben");
        // Unbekannt: bleibt offen, bis der Bote wieder da ist.
        let state = AppState::fuer_tests(&beispiel());
        let mut seit = None;
        for minute in [1, 12] {
            bote_takt(&state, da.clone(), &bote_stand(false, hallo), &mut seit, zeit(30, 8, minute)).await;
        }
        for minute in [13, 30, 59] {
            assert_eq!(bote_takt(&state, unbekannt.clone(), &bote_stand(false, hallo), &mut seit, zeit(30, 8, minute)).await, BoteUrteil::Nichts);
        }
        assert_eq!(offen(&state), 1, "unbekannt: bleibt offen");
        let wieder = zeit(30, 9, 0);
        assert_eq!(bote_takt(&state, unbekannt.clone(), &bote_stand(true, wieder), &mut seit, wieder).await, BoteUrteil::Beheben);
        assert_eq!(offen(&state), 0, "der Bote behebt sie");
    }
}
