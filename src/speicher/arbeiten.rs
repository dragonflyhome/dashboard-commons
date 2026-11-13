//! Arbeiten der Chats und die Einstufungen des Inhabers (Etappe A1, Schema 4):
//! Tabellen `arbeiten`, `arbeiten_archiv` und `einstufungen`.
//!
//! Eine Arbeit legt der Chat über die API an und schreibt sie fort (Titel,
//! Stand, worauf sie wartet, Verweis, Vorschlag der Stufe, Frist, Notiz); ein
//! geänderter Titel kommt mit altem und neuem Wert in `verlauf`. Grenze
//! gegen Flut: höchstens `OFFEN_JE_PROJEKT` nicht fertige Arbeiten je Projekt
//! (darüber `SpeicherFehler::Grenze`, in der API 429). Fertige Arbeiten ziehen
//! `ARCHIVIEREN_NACH_TAGEN` Tage nach ihrer letzten Änderung unverändert nach
//! `arbeiten_archiv` um, wie die Aufgaben: nie gelöscht, nie überschrieben.
//!
//! Eine Einstufung gehört zu Projekt, Art und Kennung (bei Fragen dazu
//! `gestellt`). „Zurück zum Vorschlag“ setzt `stufe` auf null; gelöscht wird
//! auch hier nichts.
//!
//! Für „Anstehend“ liest `anstehend_lesen` offene Aufgaben (ohne Links: es
//! wird nichts entschlüsselt), Arbeiten und Einstufungen in einer
//! Lese-Transaktion. Alle Methoden bekommen „jetzt“ als Eingabe.

use chrono::{DateTime, SecondsFormat};
use redb::{ReadableDatabase, ReadableTable, WriteTransaction};
use tracing::warn;

use super::aufgaben::{Zeit, abgelaufen};
use super::tabellen::{self, ARBEITEN, ARBEITEN_ARCHIV, AUFGABEN, EINSTUFUNGEN, META, META_ARBEIT_ZAEHLER};
use super::vorgaenge::json_lesen;
use super::{Speicher, SpeicherFehler};
use crate::api::vertrag::{Arbeit, ArbeitVerlauf, ArbeitVerlaufArt, ArbeitsStand, Aufgabe, AufgabenZustand, Einstufung, EintragsArt, Meldung, Stufe, Vorgang};

/// Höchstens so viele nicht fertige Arbeiten je Projekt.
pub const OFFEN_JE_PROJEKT: u64 = 30;
/// Fertige Arbeiten ziehen so viele Tage nach ihrer letzten Änderung ins Archiv.
pub const ARCHIVIEREN_NACH_TAGEN: i64 = 7;

pub(super) fn zeit_text(z: &Zeit) -> String {
    z.to_rfc3339_opts(SecondsFormat::Secs, false)
}

/// Eine neue Arbeit, schon geprüft (siehe `api::arbeiten`).
#[derive(Debug, Clone)]
pub struct NeueArbeit {
    pub projekt: String,
    pub von: String,
    pub titel: String,
    pub stand: ArbeitsStand,
    pub wartet_auf: Option<String>,
    pub verweis: Option<String>,
    pub prioritaet: Stufe,
    pub frist: Option<String>,
    pub notiz: Option<String>,
    /// `<projekt>/<kennung>`, schon gegen `daten/plan.toml` geprüft.
    pub etappe: Option<String>,
}

/// Teiländerung einer Arbeit, schon geprüft. Äußeres `None`: bleibt, wie es
/// ist; `Some(None)`: Feld leeren.
#[derive(Debug, Clone, Default)]
pub struct ArbeitAenderung {
    /// Neuer Titel (schon geprüft); eine Änderung kommt in den Verlauf.
    pub titel: Option<String>,
    /// Wer ändert (`chat:<name>`, Pflicht, geprüft mit `api::arbeiten::wer_ruft`).
    pub von: String,
    pub stand: Option<ArbeitsStand>,
    pub wartet_auf: Option<Option<String>>,
    pub verweis: Option<Option<String>>,
    pub prioritaet: Option<Stufe>,
    pub frist: Option<Option<String>>,
    pub notiz: Option<Option<String>>,
    /// `<projekt>/<kennung>`, schon gegen `daten/plan.toml` geprüft; kommt in den Verlauf.
    pub etappe: Option<Option<String>>,
}

/// Ergebnis von `arbeit_fortschreiben`.
#[derive(Debug)]
pub enum Fortschreiben {
    Geaendert(Arbeit),
    /// Alles war schon so; nichts geschrieben.
    Unveraendert(Arbeit),
    /// Die Arbeit ist fertig und schon im Archiv; nichts geschrieben.
    ImArchiv(Arbeit),
    /// Die Änderung passt nicht zusammen (Satz für den Inhaber); nichts geschrieben.
    Ungueltig(String),
    /// Den Titel einer Arbeit ohne Besitzer ändert nur der Chat ihres Projekts; nichts geschrieben.
    FremderTitel,
    /// Die Etappe einer Arbeit ohne Besitzer setzt nur der Chat ihres Projekts; nichts geschrieben.
    FremdeEtappe,
}

/// Satz, wenn `wartet` ohne `wartet_auf` kommt.
pub const SATZ_WARTET_AUF_FEHLT: &str = "Bei „stand“: „wartet“ fehlt „wartet_auf“: inhaber, chat:<name> oder extern.";
/// Satz, wenn eine Arbeit mit Schritten ihren Stand direkt bekommen soll.
pub const SATZ_STAND_AUS_SCHRITTEN: &str = "Diese Arbeit hat Schritte; ihr Stand ergibt sich aus ihnen. Bitte den Stand am Schritt ändern.";
/// Satz, wenn eine Arbeit im Archiv geändert werden soll.
pub const SATZ_IM_ARCHIV: &str = "Diese Arbeit ist fertig und liegt schon im Archiv; bitte eine neue anlegen.";
/// Höchstens so viele Einträge im Verlauf einer Arbeit (darüber 429).
pub const VERLAUF_HOECHSTENS: usize = 500;

/// Wendet `aenderung` auf `a` an. `wartet_auf` gilt nur beim Stand `wartet`:
/// Wechselt der Stand weg davon, fällt es weg; steht der Stand danach auf
/// `wartet` ohne `wartet_auf`, ist das ein Fehler. Hat die Arbeit Schritte,
/// kommen Stand, `wartet_auf` und `verweis` aus ihnen. Gibt zurück, ob sich etwas
/// geändert hat (ohne `geaendert` zu setzen).
pub fn aenderung_anwenden(a: &mut Arbeit, aenderung: &ArbeitAenderung) -> Result<bool, String> {
    if !a.schritte.is_empty() && (aenderung.stand.is_some() || aenderung.wartet_auf.is_some() || aenderung.verweis.is_some()) {
        return Err(SATZ_STAND_AUS_SCHRITTEN.to_string());
    }
    let vorher = a.clone();
    if let Some(t) = &aenderung.titel {
        a.titel = t.clone();
    }
    if let Some(s) = aenderung.stand {
        a.stand = s;
    }
    if let Some(w) = &aenderung.wartet_auf {
        a.wartet_auf = w.clone();
    }
    if let Some(v) = &aenderung.verweis {
        a.verweis = v.clone();
    }
    if let Some(p) = aenderung.prioritaet {
        a.prioritaet = p;
    }
    if let Some(f) = &aenderung.frist {
        a.frist = f.clone();
    }
    if let Some(n) = &aenderung.notiz {
        a.notiz = n.clone();
    }
    if let Some(e) = &aenderung.etappe {
        a.etappe = e.clone();
    }
    if a.stand == ArbeitsStand::Wartet {
        if a.wartet_auf.is_none() {
            return Err(SATZ_WARTET_AUF_FEHLT.to_string());
        }
    } else if aenderung.wartet_auf.as_ref().is_some_and(Option::is_some) {
        return Err("„wartet_auf“ gibt es nur beim Stand „wartet“.".to_string());
    } else {
        a.wartet_auf = None;
    }
    Ok(*a != vorher)
}

/// Der Eintrag im Verlauf für einen geänderten Titel.
fn titel_verlauf(zeit: &str, von: String, alt: &str, neu: &str) -> ArbeitVerlauf {
    ArbeitVerlauf {
        zeit: zeit.to_string(),
        von,
        art: ArbeitVerlaufArt::Titel,
        schritt: None,
        alt: alt.to_string(),
        neu: neu.to_string(),
        text: format!("Titel geändert: „{alt}“ → „{neu}“"),
    }
}

/// Der Eintrag im Verlauf für einen Wechsel des Arbeitsstands (Schema 11).
pub(super) fn stand_verlauf(zeit: &str, von: &str, alt: ArbeitsStand, neu: ArbeitsStand) -> ArbeitVerlauf {
    ArbeitVerlauf {
        zeit: zeit.to_string(),
        von: von.to_string(),
        art: ArbeitVerlaufArt::Stand,
        schritt: None,
        alt: alt.wort().to_string(),
        neu: neu.wort().to_string(),
        text: format!("Stand: {} → {}.", alt.wort(), neu.wort()),
    }
}

/// Hängt einen Eintrag an den Verlauf. Nichts fällt weg (Daten werden nie
/// gelöscht); darum gegen Flut höchstens `VERLAUF_HOECHSTENS` Einträge
/// (`SpeicherFehler::Grenze`, die Transaktion schreibt dann nichts).
pub(super) fn verlauf_anhaengen(a: &mut Arbeit, eintrag: ArbeitVerlauf) -> Result<(), SpeicherFehler> {
    if a.verlauf.len() >= VERLAUF_HOECHSTENS {
        return Err(SpeicherFehler::Grenze(format!(
            "Der Verlauf der Arbeit {} hat schon {VERLAUF_HOECHSTENS} Einträge; mehr gehen nicht. Bitte eine neue Arbeit anlegen.",
            a.id
        )));
    }
    a.verlauf.push(eintrag);
    Ok(())
}

/// Läuft oder wartet die Arbeit (oder ein Schritt) mit diesem Stand, braucht sie einen Besitzer.
pub(super) fn braucht_besitzer(stand: ArbeitsStand) -> bool {
    matches!(stand, ArbeitsStand::Laeuft | ArbeitsStand::Wartet)
}

/// Schlüssel in `einstufungen`: `projekt/art/id`, bei Fragen `projekt/frage/id/gestellt`.
pub fn einstufung_schluessel(projekt: &str, art: EintragsArt, id: &str, gestellt: Option<&str>) -> String {
    match (art, gestellt) {
        (EintragsArt::Frage, Some(g)) => format!("{projekt}/{}/{id}/{g}", art.wort()),
        _ => format!("{projekt}/{}/{id}", art.wort()),
    }
}

/// Neue Kennung `ar-JJJJMMTT-NNNN` aus dem Tageszähler in `meta`; frei heißt:
/// weder in `arbeiten` noch im Archiv.
fn neue_kennung(txn: &WriteTransaction, jetzt: &Zeit) -> Result<String, SpeicherFehler> {
    let tag = jetzt.format("%Y%m%d").to_string();
    let schluessel = format!("{META_ARBEIT_ZAEHLER}{tag}");
    let mut meta = txn.open_table(META)?;
    let alt = meta.get(schluessel.as_str())?.map(|w| w.value().to_string());
    let mut n: u64 = match alt {
        Some(text) => text.parse().map_err(|_| SpeicherFehler::Beschaedigt(format!("{schluessel} = {text:?} ist keine Zahl")))?,
        None => 0,
    };
    let arbeiten = txn.open_table(ARBEITEN)?;
    let archiv = txn.open_table(ARBEITEN_ARCHIV)?;
    let id = loop {
        n += 1;
        let id = format!("ar-{tag}-{n:04}");
        if arbeiten.get(id.as_str())?.is_none() && archiv.get(id.as_str())?.is_none() {
            break id;
        }
    };
    meta.insert(schluessel.as_str(), n.to_string().as_str())?;
    Ok(id)
}

/// Eine Arbeit aus einer Tabelle (lesend oder schreibend geöffnet).
pub(super) fn arbeit_aus<T: ReadableTable<&'static str, &'static [u8]>>(tabelle: &T, id: &str) -> Result<Option<Arbeit>, SpeicherFehler> {
    match tabelle.get(id)? {
        Some(w) => Ok(Some(json_lesen(w.value(), &format!("arbeiten/{id}"))?)),
        None => Ok(None),
    }
}

/// Alle lesbaren Arbeiten einer Tabelle; unlesbare stehen im Protokoll.
fn alle_aus<T: ReadableTable<&'static str, &'static [u8]>>(tabelle: &T) -> Result<Vec<Arbeit>, SpeicherFehler> {
    let mut out = Vec::new();
    for eintrag in tabelle.iter()? {
        let (k, w) = eintrag?;
        match json_lesen::<Arbeit>(w.value(), &format!("arbeiten/{}", k.value())) {
            Ok(a) => out.push(a),
            Err(e) => warn!("arbeiten: {} ({})", e, e.einzelheiten()),
        }
    }
    Ok(out)
}

/// Nicht fertige Arbeiten des Projekts in `tabelle`.
pub(super) fn offene_im_projekt<T: ReadableTable<&'static str, &'static [u8]>>(tabelle: &T, projekt: &str) -> Result<u64, SpeicherFehler> {
    Ok(alle_aus(tabelle)?.iter().filter(|a| a.projekt == projekt && a.stand != ArbeitsStand::Fertig).count() as u64)
}

pub(super) fn grenze_satz(projekt: &str) -> SpeicherFehler {
    SpeicherFehler::Grenze(format!(
        "Für das Projekt {projekt} sind schon {OFFEN_JE_PROJEKT} Arbeiten offen (nicht fertig); mehr gehen nicht. Bitte erst eine auf „fertig“ setzen."
    ))
}

/// Ist eine fertige Arbeit so alt, dass sie ins Archiv zieht? Eine unlesbare
/// Zeit zählt nicht als alt.
fn zu_archivieren(a: &Arbeit, jetzt: &Zeit) -> bool {
    a.stand == ArbeitsStand::Fertig && DateTime::parse_from_rfc3339(&a.geaendert).is_ok_and(|t| t + chrono::Duration::days(ARCHIVIEREN_NACH_TAGEN) <= *jetzt)
}

/// Reihenfolge für `GET /api/arbeiten`: nicht fertige zuerst, dann nach
/// Vorschlag der Stufe, Frist (ohne zuletzt), Alter.
pub fn sortieren(liste: &mut [Arbeit]) {
    liste.sort_by(|a, b| {
        let schluessel = |x: &Arbeit| (x.stand == ArbeitsStand::Fertig, x.prioritaet, x.frist.is_none(), x.frist.clone(), x.erstellt.clone(), x.id.clone());
        schluessel(a).cmp(&schluessel(b))
    });
}

/// Was „Anstehend“ aus dem Datenspeicher braucht, in einem Zug gelesen.
#[derive(Debug, Clone, Default)]
pub struct AusDemSpeicher {
    /// Offene, nicht abgelaufene Aufgaben (ohne Links).
    pub aufgaben: Vec<Aufgabe>,
    /// Alle Arbeiten außerhalb des Archivs (auch fertige).
    pub arbeiten: Vec<Arbeit>,
    pub einstufungen: Vec<Einstufung>,
    /// Schlüssel `projekt/id/gestellt` aller beantworteten Fragen (Etappe A2,
    /// auch aus dem Archiv): Diese Fragen sind nicht mehr offen.
    pub beantwortet: std::collections::BTreeSet<String>,
    /// Alle Meldungen außerhalb des Archivs (Etappe A3), auch gelesene.
    pub meldungen: Vec<Meldung>,
    /// Vorgänge der letzten sieben Tage, neueste zuerst (für „Zuletzt passiert“).
    pub vorgaenge: Vec<Vorgang>,
    /// Etappe B2 (Spalte „Fertig“ im Backlog): nicht mehr offene Aufgaben
    /// außerhalb des Archivs (erledigt, zurückgezogen, abgelaufen; eine offene
    /// mit erreichtem Ablauf zählt hier als abgelaufen), ohne Links.
    pub aufgaben_abgeschlossen: Vec<Aufgabe>,
}

impl Speicher {
    /// Legt eine Arbeit an (in einer Transaktion mit der Grenze).
    pub fn arbeit_anlegen(&self, neu: NeueArbeit, jetzt: &Zeit) -> Result<Arbeit, SpeicherFehler> {
        let txn = self.db.begin_write()?;
        if neu.stand != ArbeitsStand::Fertig && offene_im_projekt(&txn.open_table(ARBEITEN)?, &neu.projekt)? >= OFFEN_JE_PROJEKT {
            return Err(grenze_satz(&neu.projekt));
        }
        let id = neue_kennung(&txn, jetzt)?;
        let zeit = zeit_text(jetzt);
        let a = Arbeit {
            id,
            projekt: neu.projekt,
            besitzer: braucht_besitzer(neu.stand).then(|| neu.von.clone()),
            von: neu.von,
            titel: neu.titel,
            stand: neu.stand,
            wartet_auf: neu.wartet_auf,
            verweis: neu.verweis,
            prioritaet: neu.prioritaet,
            frist: neu.frist,
            notiz: neu.notiz,
            erstellt: zeit.clone(),
            geaendert: zeit,
            verlauf: Vec::new(),
            schritte: Vec::new(),
            etappe: neu.etappe,
        };
        txn.open_table(ARBEITEN)?.insert(a.id.as_str(), serde_json::to_vec(&a)?.as_slice())?;
        self.festschreiben(txn, &[super::Bereich::Arbeiten])?;
        Ok(a)
    }

    /// Schreibt eine Arbeit fort. Unbekannte Kennung: `NichtGefunden`. Wird
    /// eine fertige wieder offen, gilt die Grenze. Alles in einer
    /// Schreib-Transaktion: Hat die Arbeit einen Besitzer, ändert nur er sie
    /// (sonst `Besetzt`), auch den Titel; den Titel einer Arbeit ohne Besitzer
    /// ändert nur der Chat ihres Projekts (`FremderTitel`). Eine Arbeit ohne
    /// Besitzer übernimmt, wer sie auf `laeuft` oder `wartet` setzt.
    // Bei `als-naechstes` oder `fertig` wird er frei (mit Eintrag im Verlauf).
    pub fn arbeit_fortschreiben(&self, id: &str, aenderung: &ArbeitAenderung, jetzt: &Zeit) -> Result<Fortschreiben, SpeicherFehler> {
        let txn = self.db.begin_write()?;
        let alt = arbeit_aus(&txn.open_table(ARBEITEN)?, id)?;
        let Some(mut a) = alt else {
            return match arbeit_aus(&txn.open_table(ARBEITEN_ARCHIV)?, id)? {
                Some(a) => Ok(Fortschreiben::ImArchiv(a)),
                None => Err(SpeicherFehler::NichtGefunden(id.to_string())),
            };
        };
        let von = &aenderung.von;
        let fremd = *von != format!("chat:{}", a.projekt);
        match &a.besitzer {
            Some(besitzer) if besitzer != von => return Err(SpeicherFehler::Besetzt { besitzer: besitzer.clone(), seit: a.geaendert.clone() }),
            None if aenderung.titel.is_some() && fremd => {
                return Ok(Fortschreiben::FremderTitel);
            }
            None if aenderung.etappe.as_ref().is_some_and(|e| *e != a.etappe) && fremd => {
                return Ok(Fortschreiben::FremdeEtappe);
            }
            _ => {}
        }
        let war_fertig = a.stand == ArbeitsStand::Fertig;
        let alter_stand = a.stand;
        let alter_titel = a.titel.clone();
        let alte_etappe = a.etappe.clone();
        match aenderung_anwenden(&mut a, aenderung) {
            Err(satz) => return Ok(Fortschreiben::Ungueltig(satz)),
            Ok(false) => return Ok(Fortschreiben::Unveraendert(a)),
            Ok(true) => {}
        }
        if war_fertig && a.stand != ArbeitsStand::Fertig && offene_im_projekt(&txn.open_table(ARBEITEN)?, &a.projekt)? >= OFFEN_JE_PROJEKT {
            return Err(grenze_satz(&a.projekt));
        }
        a.geaendert = zeit_text(jetzt);
        if a.titel != alter_titel {
            let eintrag = titel_verlauf(&a.geaendert, von.clone(), &alter_titel, &a.titel);
            verlauf_anhaengen(&mut a, eintrag)?;
        }
        if a.etappe != alte_etappe {
            let (alt, neu) = (alte_etappe.unwrap_or_default(), a.etappe.clone().unwrap_or_default());
            let text = format!("Etappe geändert: „{alt}“ → „{neu}“");
            let eintrag = ArbeitVerlauf { zeit: a.geaendert.clone(), von: von.clone(), art: ArbeitVerlaufArt::Etappe, schritt: None, alt, neu, text };
            verlauf_anhaengen(&mut a, eintrag)?;
        }
        if a.stand != alter_stand {
            let eintrag = stand_verlauf(&a.geaendert, von, alter_stand, a.stand);
            // Wechsel auf als-naechstes oder fertig zählt nicht gegen die Grenze (wie
            // „nicht mehr dran“): Schließen geht immer; zurück kommt nur über die Grenze.
            if braucht_besitzer(a.stand) { verlauf_anhaengen(&mut a, eintrag)? } else { a.verlauf.push(eintrag) }
        }
        if a.besitzer.is_none() && a.schritte.is_empty() && braucht_besitzer(a.stand) {
            a.besitzer = Some(von.clone());
            let eintrag = ArbeitVerlauf {
                zeit: a.geaendert.clone(),
                von: von.clone(),
                art: ArbeitVerlaufArt::Besitzer,
                schritt: None,
                alt: String::new(),
                neu: von.clone(),
                text: format!("{von} hat die Arbeit übernommen."),
            };
            verlauf_anhaengen(&mut a, eintrag)?;
        }
        // Als nächstes oder fertig braucht keinen Besitzer; sonst bekäme jeder andere für immer 409.
        if let Some(alt) = a.besitzer.clone().filter(|_| a.schritte.is_empty() && !braucht_besitzer(a.stand)) {
            a.besitzer = None;
            // Zählt nicht gegen die Grenze (Auflage teilschritte_1_runde_2): sonst ließe sich
            // eine Arbeit mit vollem Verlauf nicht schließen. Höchstens einer über der Grenze,
            // denn Besitzer wird nur, wer über verlauf_anhaengen schreibt.
            a.verlauf.push(ArbeitVerlauf {
                zeit: a.geaendert.clone(),
                von: von.clone(),
                art: ArbeitVerlaufArt::Besitzer,
                schritt: None,
                alt: alt.clone(),
                neu: String::new(),
                text: format!("{alt} ist nicht mehr dran."),
            });
        }
        txn.open_table(ARBEITEN)?.insert(a.id.as_str(), serde_json::to_vec(&a)?.as_slice())?;
        self.festschreiben(txn, &[super::Bereich::Arbeiten])?;
        Ok(Fortschreiben::Geaendert(a))
    }

    /// Eine Arbeit (auch aus dem Archiv).
    pub fn arbeit(&self, id: &str) -> Result<Option<Arbeit>, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        if let Some(t) = tabellen::lesen(ARBEITEN, &txn)?
            && let Some(a) = arbeit_aus(&t, id)?
        {
            return Ok(Some(a));
        }
        match tabellen::lesen(ARBEITEN_ARCHIV, &txn)? {
            Some(t) => arbeit_aus(&t, id),
            None => Ok(None),
        }
    }

    /// Alle Arbeiten außerhalb des Archivs, wahlweise nur eines Projekts, sortiert (`sortieren`).
    pub fn arbeiten_lesen(&self, projekt: Option<&str>) -> Result<Vec<Arbeit>, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        let Some(t) = tabellen::lesen(ARBEITEN, &txn)? else { return Ok(Vec::new()) };
        let mut liste: Vec<Arbeit> = alle_aus(&t)?.into_iter().filter(|a| projekt.is_none_or(|p| p == a.projekt)).collect();
        sortieren(&mut liste);
        Ok(liste)
    }

    /// Fertige Arbeiten, deren letzte Änderung `ARCHIVIEREN_NACH_TAGEN` Tage
    /// oder länger her ist, ziehen unverändert nach `arbeiten_archiv` um. Steht
    /// die Kennung dort schon, bleibt die Arbeit, wo sie ist (nie überschreiben).
    pub fn arbeiten_archivieren(&self, jetzt: &Zeit) -> Result<Vec<Arbeit>, SpeicherFehler> {
        let alt: Vec<String> = {
            let txn = self.db.begin_read()?;
            let Some(t) = tabellen::lesen(ARBEITEN, &txn)? else { return Ok(Vec::new()) };
            alle_aus(&t)?.into_iter().filter(|a| zu_archivieren(a, jetzt)).map(|a| a.id).collect()
        };
        if alt.is_empty() {
            return Ok(Vec::new());
        }
        let txn = self.db.begin_write()?;
        let mut umgezogen = Vec::new();
        {
            let mut arbeiten = txn.open_table(ARBEITEN)?;
            let mut archiv = txn.open_table(ARBEITEN_ARCHIV)?;
            for id in &alt {
                let Some(a) = arbeit_aus(&arbeiten, id)? else { continue };
                if !zu_archivieren(&a, jetzt) {
                    continue;
                }
                let roh = arbeiten.get(id.as_str())?.map(|w| w.value().to_vec());
                let Some(roh) = roh else { continue };
                if archiv.get(id.as_str())?.is_some() {
                    warn!("arbeiten_archiv/{id}: Kennung steht schon im Archiv; die Arbeit bleibt in arbeiten");
                    continue;
                }
                if archiv.insert(id.as_str(), roh.as_slice())?.is_some() {
                    return Err(SpeicherFehler::Beschaedigt(format!("arbeiten_archiv/{id}: Eintrag wäre überschrieben worden")));
                }
                arbeiten.remove(id.as_str())?;
                umgezogen.push(a);
            }
        }
        self.festschreiben(txn, &[super::Bereich::Arbeiten])?;
        Ok(umgezogen)
    }

    /// Speichert die Einstufung des Inhabers (`stufe = null`: zurück zum Vorschlag).
    /// Gibt zurück, ob sich die Stufe geändert hat; ist alles schon so, wird
    /// nichts geschrieben.
    pub fn einstufung_setzen(&self, e: &Einstufung) -> Result<bool, SpeicherFehler> {
        let schluessel = einstufung_schluessel(&e.projekt, e.art, &e.id, e.gestellt.as_deref());
        let txn = self.db.begin_write()?;
        let alt: Option<Einstufung> = match txn.open_table(EINSTUFUNGEN)?.get(schluessel.as_str())? {
            Some(w) => Some(json_lesen(w.value(), &format!("einstufungen/{schluessel}"))?),
            None => None,
        };
        let vorher = alt.as_ref().and_then(|a| a.stufe);
        if vorher == e.stufe {
            return Ok(false);
        }
        txn.open_table(EINSTUFUNGEN)?.insert(schluessel.as_str(), serde_json::to_vec(e)?.as_slice())?;
        self.festschreiben(txn, &[super::Bereich::Einstufung])?;
        Ok(true)
    }

    /// Alle Einstufungen (auch die mit `stufe = null`).
    pub fn einstufungen_lesen(&self) -> Result<Vec<Einstufung>, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        einstufungen_aus(&txn)
    }

    /// Offene Aufgaben (ohne Links), Arbeiten, Einstufungen, die Schlüssel
    /// der beantworteten Fragen, Meldungen und die Vorgänge der letzten Tage in einer Lese-Transaktion. Liest `aufgaben_links` nie: nichts wird entschlüsselt.
    pub fn anstehend_lesen(&self, jetzt: &Zeit) -> Result<AusDemSpeicher, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        let mut aufgaben = Vec::new();
        let mut aufgaben_abgeschlossen = Vec::new();
        if let Some(t) = tabellen::lesen(AUFGABEN, &txn)? {
            for eintrag in t.iter()? {
                let (k, w) = eintrag?;
                match json_lesen::<Aufgabe>(w.value(), &format!("aufgaben/{}", k.value())) {
                    Ok(a) if a.zustand == AufgabenZustand::Offen && !abgelaufen(&a, jetzt) => aufgaben.push(a),
                    Ok(mut a) => {
                        // Offen, aber der Ablauf ist erreicht: wie die API sie zeigt.
                        if a.zustand == AufgabenZustand::Offen {
                            a.zustand = AufgabenZustand::Abgelaufen;
                        }
                        aufgaben_abgeschlossen.push(a);
                    }
                    Err(e) => warn!("aufgaben: {} ({})", e, e.einzelheiten()),
                }
            }
        }
        let arbeiten = match tabellen::lesen(ARBEITEN, &txn)? {
            Some(t) => alle_aus(&t)?,
            None => Vec::new(),
        };
        Ok(AusDemSpeicher {
            aufgaben,
            arbeiten,
            einstufungen: einstufungen_aus(&txn)?,
            beantwortet: super::antworten::beantwortet_aus(&txn)?,
            meldungen: super::meldungen::meldungen_aus(&txn)?,
            vorgaenge: super::meldungen::zuletzt_vorgaenge_aus(&txn, jetzt)?,
            aufgaben_abgeschlossen,
        })
    }

    /// Zahl der Einträge im Archiv der Arbeiten (für Tests).
    #[cfg(test)]
    pub fn arbeiten_archiv_anzahl(&self) -> u64 {
        let txn = self.db.begin_read().unwrap();
        tabellen::zaehlen(ARBEITEN_ARCHIV, &txn).unwrap()
    }
}

fn einstufungen_aus(txn: &redb::ReadTransaction) -> Result<Vec<Einstufung>, SpeicherFehler> {
    let mut out = Vec::new();
    if let Some(t) = tabellen::lesen(EINSTUFUNGEN, txn)? {
        for eintrag in t.iter()? {
            let (k, w) = eintrag?;
            match json_lesen::<Einstufung>(w.value(), &format!("einstufungen/{}", k.value())) {
                Ok(e) => out.push(e),
                Err(e) => warn!("einstufungen: {} ({})", e, e.einzelheiten()),
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, TimeZone};

    fn zeit(tag: u32, stunde: u32) -> Zeit {
        FixedOffset::east_opt(3600).unwrap().with_ymd_and_hms(2026, 10, tag, stunde, 0, 0).unwrap()
    }

    fn neu(projekt: &str, titel: &str, stand: ArbeitsStand) -> NeueArbeit {
        NeueArbeit {
            projekt: projekt.into(),
            von: format!("chat:{projekt}"),
            titel: titel.into(),
            stand,
            wartet_auf: (stand == ArbeitsStand::Wartet).then(|| "inhaber".to_string()),
            verweis: None,
            prioritaet: Stufe::Bald,
            frist: None,
            notiz: None,
            etappe: None,
        }
    }

    /// Jede Änderung kommt von einem Chat (`wer_ruft`); hier der des Projekts.
    fn von_dash() -> ArbeitAenderung {
        ArbeitAenderung { von: "chat:dashboard".into(), ..Default::default() }
    }

    /// Etappe B8: Jeder Wechsel des Stands von Hand steht im Verlauf (alt und
    /// neu als Stand-Wort); eine Änderung ohne Wechsel schreibt keinen.
    #[test]
    fn stand_wechsel_von_hand_im_verlauf() {
        let s = Speicher::temporaer();
        let a = s.arbeit_anlegen(neu("dashboard", "x", ArbeitsStand::AlsNaechstes), &zeit(29, 10)).unwrap();
        assert!(a.verlauf.is_empty(), "Anlegen ist kein Wechsel");
        let staende = |a: &Arbeit| -> Vec<(String, String)> {
            a.verlauf.iter().filter(|v| v.art == ArbeitVerlaufArt::Stand).map(|v| (v.alt.clone(), v.neu.clone())).collect()
        };
        let mut x = a;
        for st in [ArbeitsStand::Laeuft, ArbeitsStand::Fertig] {
            let Fortschreiben::Geaendert(y) = s.arbeit_fortschreiben(&x.id, &ArbeitAenderung { stand: Some(st), ..von_dash() }, &zeit(29, 11)).unwrap() else {
                panic!()
            };
            x = y;
        }
        let notiz = ArbeitAenderung { notiz: Some(Some("n".into())), ..von_dash() };
        let Fortschreiben::Geaendert(x) = s.arbeit_fortschreiben(&x.id, &notiz, &zeit(29, 12)).unwrap() else { panic!() };
        assert_eq!(staende(&x), [("als-naechstes".into(), "laeuft".into()), ("laeuft".into(), "fertig".into())]);
        let v = x.verlauf.iter().find(|v| v.art == ArbeitVerlaufArt::Stand).unwrap();
        assert_eq!((v.von.as_str(), v.text.as_str()), ("chat:dashboard", "Stand: als-naechstes → laeuft."));
    }

    /// Etappe B8: Die Etappe setzt nur, wer die Arbeit verantwortet: mit
    /// Besitzer nur er (sonst `Besetzt`), ohne Besitzer der Chat des Projekts
    /// (sonst `FremdeEtappe`); nichts geschrieben. Gesetzt und geleert steht im Verlauf.
    #[test]
    fn etappe_nur_wer_verantwortet() {
        let s = Speicher::temporaer();
        let etappe = |e: Option<&str>, von: &str| ArbeitAenderung { etappe: Some(e.map(String::from)), von: von.into(), ..Default::default() };
        let mit = s.arbeit_anlegen(neu("dashboard", "mit", ArbeitsStand::Laeuft), &zeit(29, 10)).unwrap();
        let ohne = s.arbeit_anlegen(neu("dashboard", "ohne", ArbeitsStand::AlsNaechstes), &zeit(29, 10)).unwrap();
        assert!(matches!(s.arbeit_fortschreiben(&mit.id, &etappe(Some("dashboard/b8"), "chat:pruefung"), &zeit(29, 11)), Err(SpeicherFehler::Besetzt { .. })));
        assert!(matches!(
            s.arbeit_fortschreiben(&ohne.id, &etappe(Some("dashboard/b8"), "chat:pruefung"), &zeit(29, 11)).unwrap(),
            Fortschreiben::FremdeEtappe
        ));
        assert_eq!(s.arbeit(&ohne.id).unwrap().unwrap(), ohne, "nichts geschrieben");
        for (id, wer) in [(&mit.id, "chat:dashboard"), (&ohne.id, "chat:dashboard")] {
            let Fortschreiben::Geaendert(x) = s.arbeit_fortschreiben(id, &etappe(Some("architektur/b8"), wer), &zeit(29, 11)).unwrap() else { panic!() };
            assert_eq!(x.etappe.as_deref(), Some("architektur/b8"));
            let Fortschreiben::Geaendert(x) = s.arbeit_fortschreiben(id, &etappe(None, wer), &zeit(29, 12)).unwrap() else { panic!() };
            assert_eq!(x.etappe, None);
            let e: Vec<(&str, &str)> = x.verlauf.iter().filter(|v| v.art == ArbeitVerlaufArt::Etappe).map(|v| (v.alt.as_str(), v.neu.as_str())).collect();
            assert_eq!(e, [("", "architektur/b8"), ("architektur/b8", "")]);
        }
        // Dieselbe Etappe noch einmal ändert nichts, auch nicht für einen fremden Chat.
        assert!(matches!(s.arbeit_fortschreiben(&ohne.id, &etappe(None, "chat:pruefung"), &zeit(29, 13)).unwrap(), Fortschreiben::Unveraendert(_)));
    }

    /// Auflage teilschritte_1_runde_2: Bei vollem Verlauf (500) geht fertig noch,
    /// die Einträge „Stand“ und „nicht mehr dran“ zählen nicht; Besitzer werden geht danach nicht mehr (429).
    #[test]
    fn fertig_bei_vollem_verlauf() {
        let s = Speicher::temporaer();
        let a = s.arbeit_anlegen(neu("dashboard", "voll", ArbeitsStand::Laeuft), &zeit(29, 10)).unwrap();
        let mut voll = a.clone();
        voll.verlauf = vec![titel_verlauf("2026-10-29T10:00:00+01:00", "chat:dashboard".into(), "a", "b"); VERLAUF_HOECHSTENS];
        let txn = s.db.begin_write().unwrap();
        txn.open_table(ARBEITEN).unwrap().insert(a.id.as_str(), serde_json::to_vec(&voll).unwrap().as_slice()).unwrap();
        txn.commit().unwrap();
        let zu = ArbeitAenderung { stand: Some(ArbeitsStand::Fertig), ..von_dash() };
        let Fortschreiben::Geaendert(x) = s.arbeit_fortschreiben(&a.id, &zu, &zeit(29, 11)).unwrap() else { panic!() };
        assert_eq!((x.stand, x.besitzer, x.verlauf.len()), (ArbeitsStand::Fertig, None, VERLAUF_HOECHSTENS + 2), "Stand und nicht mehr dran");
        let auf = ArbeitAenderung { stand: Some(ArbeitsStand::Laeuft), ..von_dash() };
        assert!(matches!(s.arbeit_fortschreiben(&a.id, &auf, &zeit(29, 12)), Err(SpeicherFehler::Grenze(_))));
    }

    #[test]
    fn anlegen_fortschreiben_und_kennung() {
        let s = Speicher::temporaer();
        let a = s.arbeit_anlegen(neu("dashboard", "Server bauen", ArbeitsStand::Laeuft), &zeit(29, 10)).unwrap();
        assert_eq!(a.id, "ar-20261029-0001");
        let b = s.arbeit_anlegen(neu("dashboard", "Oberfläche", ArbeitsStand::AlsNaechstes), &zeit(29, 10)).unwrap();
        assert_eq!(b.id, "ar-20261029-0002");
        // Auf wartet ohne wartet_auf: abgelehnt, nichts geschrieben.
        let w = ArbeitAenderung { stand: Some(ArbeitsStand::Wartet), ..von_dash() };
        assert!(matches!(s.arbeit_fortschreiben(&a.id, &w, &zeit(29, 11)).unwrap(), Fortschreiben::Ungueltig(satz) if satz == SATZ_WARTET_AUF_FEHLT));
        let w = ArbeitAenderung {
            stand: Some(ArbeitsStand::Wartet),
            wartet_auf: Some(Some("inhaber".into())),
            verweis: Some(Some("a-20261029-0001".into())),
            ..von_dash()
        };
        let Fortschreiben::Geaendert(neu_a) = s.arbeit_fortschreiben(&a.id, &w, &zeit(29, 11)).unwrap() else { panic!() };
        assert_eq!((neu_a.stand, neu_a.wartet_auf.as_deref(), neu_a.geaendert.as_str()), (ArbeitsStand::Wartet, Some("inhaber"), "2026-10-29T11:00:00+01:00"));
        assert!(matches!(s.arbeit_fortschreiben(&a.id, &w, &zeit(29, 12)).unwrap(), Fortschreiben::Unveraendert(_)));
        // Weg von wartet: wartet_auf fällt weg, der Verweis bleibt.
        let l = ArbeitAenderung { stand: Some(ArbeitsStand::Laeuft), ..von_dash() };
        let Fortschreiben::Geaendert(x) = s.arbeit_fortschreiben(&a.id, &l, &zeit(29, 12)).unwrap() else { panic!() };
        assert_eq!((x.wartet_auf, x.verweis.as_deref()), (None, Some("a-20261029-0001")));
        // wartet_auf bei einem anderen Stand: abgelehnt.
        let falsch = ArbeitAenderung { wartet_auf: Some(Some("extern".into())), ..von_dash() };
        assert!(matches!(s.arbeit_fortschreiben(&a.id, &falsch, &zeit(29, 12)).unwrap(), Fortschreiben::Ungueltig(_)));
        // Felder leeren mit Some(None).
        let leer = ArbeitAenderung { verweis: Some(None), ..von_dash() };
        let Fortschreiben::Geaendert(x) = s.arbeit_fortschreiben(&a.id, &leer, &zeit(29, 13)).unwrap() else { panic!() };
        assert_eq!(x.verweis, None);
        assert!(matches!(s.arbeit_fortschreiben("ar-19990101-0001", &l, &zeit(29, 13)), Err(SpeicherFehler::NichtGefunden(_))));
        let liste = s.arbeiten_lesen(Some("dashboard")).unwrap();
        assert_eq!(liste.len(), 2);
        assert!(s.arbeiten_lesen(Some("anderes")).unwrap().is_empty());
    }

    /// Ein neuer Titel kommt mit altem und neuem Wert in den Verlauf; derselbe
    /// Titel ändert nichts; der Verlauf verliert keinen Eintrag.
    #[test]
    fn titel_aendern_schreibt_den_verlauf() {
        let s = Speicher::temporaer();
        let a = s.arbeit_anlegen(neu("dashboard", "alt", ArbeitsStand::Laeuft), &zeit(29, 10)).unwrap();
        assert!(a.verlauf.is_empty());
        let t = |titel: &str| ArbeitAenderung { titel: Some(titel.into()), ..von_dash() };
        let Fortschreiben::Geaendert(x) = s.arbeit_fortschreiben(&a.id, &t("neu"), &zeit(29, 11)).unwrap() else { panic!() };
        assert_eq!(x.titel, "neu");
        assert_eq!(
            x.verlauf,
            [ArbeitVerlauf {
                zeit: "2026-10-29T11:00:00+01:00".into(),
                von: "chat:dashboard".into(),
                art: ArbeitVerlaufArt::Titel,
                schritt: None,
                alt: "alt".into(),
                neu: "neu".into(),
                text: "Titel geändert: „alt“ → „neu“".into(),
            }]
        );
        assert!(matches!(s.arbeit_fortschreiben(&a.id, &t("neu"), &zeit(29, 12)).unwrap(), Fortschreiben::Unveraendert(_)));
        // Nur der Stand: kein Titel-Eintrag, der Wechsel des Stands und der frei gewordene Besitzer.
        let l = ArbeitAenderung { stand: Some(ArbeitsStand::AlsNaechstes), ..von_dash() };
        let Fortschreiben::Geaendert(x) = s.arbeit_fortschreiben(&a.id, &l, &zeit(29, 12)).unwrap() else { panic!() };
        assert_eq!(x.verlauf.iter().map(|v| v.art).collect::<Vec<_>>(), [ArbeitVerlaufArt::Titel, ArbeitVerlaufArt::Stand, ArbeitVerlaufArt::Besitzer]);
        for i in 0..60 {
            s.arbeit_fortschreiben(&a.id, &t(&format!("t{i}")), &zeit(29, 13)).unwrap();
        }
        let x = s.arbeit(&a.id).unwrap().unwrap();
        assert_eq!(x.verlauf.len(), 63, "kein Eintrag fällt weg");
        assert_eq!((x.verlauf[3].alt.as_str(), x.verlauf[3].neu.as_str()), ("neu", "t0"));
        // Ältere Arbeiten ohne `verlauf` bleiben lesbar.
        let roh = serde_json::to_value(&a).unwrap();
        assert!(roh.get("verlauf").is_none(), "leer wird nicht geschrieben: {roh}");
        assert_eq!(serde_json::from_value::<Arbeit>(roh).unwrap(), a);
    }

    /// Grenze: die 31. offene Arbeit eines Projekts geht nicht; fertige zählen
    /// nicht; eine fertige wieder öffnen geht nur mit Platz.
    #[test]
    fn grenze_dreissig_offene_je_projekt() {
        let s = Speicher::temporaer();
        let mut ids = Vec::new();
        for i in 0..OFFEN_JE_PROJEKT {
            ids.push(s.arbeit_anlegen(neu("dashboard", &format!("t{i}"), ArbeitsStand::Laeuft), &zeit(29, 10)).unwrap().id);
        }
        let e = s.arbeit_anlegen(neu("dashboard", "zu viel", ArbeitsStand::AlsNaechstes), &zeit(29, 10)).unwrap_err();
        assert!(matches!(&e, SpeicherFehler::Grenze(satz) if satz.contains("Projekt dashboard") && satz.contains("30")), "{e}");
        s.arbeit_anlegen(neu("anderes", "geht", ArbeitsStand::Laeuft), &zeit(29, 10)).unwrap();
        let fertig = s.arbeit_anlegen(neu("dashboard", "schon fertig", ArbeitsStand::Fertig), &zeit(29, 10)).unwrap();
        let auf = ArbeitAenderung { stand: Some(ArbeitsStand::Laeuft), ..von_dash() };
        assert!(matches!(s.arbeit_fortschreiben(&fertig.id, &auf, &zeit(29, 11)), Err(SpeicherFehler::Grenze(_))));
        let zu = ArbeitAenderung { stand: Some(ArbeitsStand::Fertig), ..von_dash() };
        assert!(matches!(s.arbeit_fortschreiben(&ids[0], &zu, &zeit(29, 11)).unwrap(), Fortschreiben::Geaendert(_)));
        assert!(matches!(s.arbeit_fortschreiben(&fertig.id, &auf, &zeit(29, 11)).unwrap(), Fortschreiben::Geaendert(_)));
    }

    /// Archiv nach sieben Tagen: nur fertige, unverändert, nicht mehr in der
    /// Liste, aber im Archiv und im Export; Fortschreiben danach ändert nichts.
    #[test]
    fn archiv_nach_sieben_tagen() {
        let s = Speicher::temporaer();
        let offen = s.arbeit_anlegen(neu("dashboard", "läuft", ArbeitsStand::Laeuft), &zeit(1, 10)).unwrap();
        let f = s.arbeit_anlegen(neu("dashboard", "fertig", ArbeitsStand::Fertig), &zeit(1, 10)).unwrap();
        let knapp = zeit(8, 9);
        assert!(s.arbeiten_archivieren(&knapp).unwrap().is_empty(), "vor sieben Tagen noch nichts");
        let weg = s.arbeiten_archivieren(&zeit(8, 10)).unwrap();
        assert_eq!(weg.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), [f.id.as_str()]);
        assert_eq!(weg[0], f, "unverändert");
        assert_eq!(s.arbeiten_lesen(None).unwrap().iter().map(|a| a.id.clone()).collect::<Vec<_>>(), [offen.id.as_str()]);
        assert_eq!(s.arbeiten_archiv_anzahl(), 1);
        assert_eq!(s.arbeit(&f.id).unwrap(), Some(f.clone()), "aus dem Archiv lesbar");
        let export: serde_json::Value = serde_json::from_str(&s.export().unwrap().als_json()).unwrap();
        assert_eq!(export["arbeiten_archiv"][0]["id"].as_str(), Some(f.id.as_str()));
        let auf = ArbeitAenderung { stand: Some(ArbeitsStand::Laeuft), ..von_dash() };
        assert!(matches!(s.arbeit_fortschreiben(&f.id, &auf, &zeit(9, 10)).unwrap(), Fortschreiben::ImArchiv(_)));
        assert!(s.arbeiten_archivieren(&zeit(30, 10)).unwrap().is_empty(), "ein zweiter Lauf tut nichts");
        // Eine neue Kennung meidet das Archiv.
        let n = s.arbeit_anlegen(neu("dashboard", "neu", ArbeitsStand::Laeuft), &zeit(1, 12)).unwrap();
        assert_eq!(n.id, "ar-20261001-0003");
    }

    /// Einstufung: setzen, ändern, zurück zum Vorschlag (bleibt als Eintrag
    /// mit null); Fragen mit `gestellt` sind ein eigener Schlüssel.
    #[test]
    fn einstufung_setzen_und_zuruecknehmen() {
        let s = Speicher::temporaer();
        let e = |stufe: Option<Stufe>, gestellt: Option<&str>| Einstufung {
            projekt: "beispiel".into(),
            art: EintragsArt::Frage,
            id: "schlusssatz".into(),
            gestellt: gestellt.map(str::to_string),
            stufe,
            zeit: "2026-10-29T10:00:00+01:00".into(),
        };
        assert!(!s.einstufung_setzen(&e(None, Some("2026-10-25"))).unwrap(), "zurück ohne Einstufung: nichts zu tun");
        assert!(s.einstufungen_lesen().unwrap().is_empty());
        assert!(s.einstufung_setzen(&e(Some(Stufe::Jetzt), Some("2026-10-25"))).unwrap());
        assert!(!s.einstufung_setzen(&e(Some(Stufe::Jetzt), Some("2026-10-25"))).unwrap());
        assert!(s.einstufung_setzen(&e(Some(Stufe::Irgendwann), Some("2026-10-26"))).unwrap(), "neu gestellt: eigener Schlüssel");
        assert_eq!(s.einstufungen_lesen().unwrap().len(), 2);
        assert!(s.einstufung_setzen(&e(None, Some("2026-10-25"))).unwrap());
        let alle = s.einstufungen_lesen().unwrap();
        assert_eq!(alle.len(), 2, "der Eintrag bleibt");
        assert!(alle.iter().any(|x| x.gestellt.as_deref() == Some("2026-10-25") && x.stufe.is_none()));
        assert_eq!(einstufung_schluessel("p", EintragsArt::Aufgabe, "a-1", Some("x")), "p/aufgabe/a-1");
        assert_eq!(einstufung_schluessel("p", EintragsArt::Frage, "f", Some("2026-10-25")), "p/frage/f/2026-10-25");
    }
}
