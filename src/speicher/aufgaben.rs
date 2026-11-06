//! Aufgaben für den Inhaber (Etappe 5a): Tabellen `aufgaben`, `aufgaben_links` und
//! `aufgaben_archiv`.
//!
//! Eine Aufgabe steht ohne Link in `aufgaben`. Ein Link (oft ein einmaliger
//! Anmeldelink mit Geheimnis) steht nur verschlüsselt in `aufgaben_links`
//! (`verschluesselung`, DPAPI). Sobald die Aufgabe nicht mehr offen ist
//! (erledigt, abgelaufen, zurückgezogen), ist der Eintrag dort gelöscht; es
//! bleibt nur der Titel.
//!
//! Grenzen gegen Flut: höchstens `OFFEN_JE_PROJEKT` offene Aufgaben je
//! Projekt und `OFFEN_INSGESAMT` insgesamt (darüber `SpeicherFehler::Grenze`,
//! in der API 429). Erledigte, abgelaufene und zurückgezogene Aufgaben ziehen
//! `ARCHIVIEREN_NACH_TAGEN` Tage nach ihrer letzten Änderung nach
//! `aufgaben_archiv` um. Gelöscht wird nichts (Daten werden nie
//! gelöscht); das Archiv steht in Sicherung und Export, nur Listen und Zähler
//! lesen es nicht. Eine eigene Tabelle statt eines Feldes, damit `aufgaben`
//! klein bleibt (der Zeitplan liest sie jede Minute ganz). Sie gehört zu
//! Schema 3, das mit 5a erst kommt. Einmalige Links werden weiterhin gelöscht.
//!
//! Alle Methoden bekommen „jetzt“ als Eingabe, damit Tests die Zeit stellen
//! können. Jeder Lesezugriff prüft den Ablauf selbst: Ein abgelaufener Link
//! wird nie entschlüsselt, auch wenn der Zeitplan ihn noch nicht gelöscht hat.

use chrono::{DateTime, FixedOffset, SecondsFormat};
use redb::{ReadableDatabase, ReadableTable, WriteTransaction};
use tracing::warn;
use zeroize::Zeroizing;

use super::tabellen::{self, AUFGABEN, AUFGABEN_ARCHIV, AUFGABEN_LINKS, META, META_AUFGABE_ZAEHLER};
use super::vorgaenge::{json_lesen, vorgang_anlegen_in, vorgang_anlegen_mit_notiz_in};
use super::{Speicher, SpeicherFehler, verschluesselung};
use crate::api::vertrag::{Aufgabe, AufgabeAnsicht, AufgabeVerlauf, AufgabeVerlaufArt, AufgabenZustand, Ereignis, Stufe, Vorgang, VorgangsArt};
use crate::vertrag::daten::ETWAS_ANDERES;

/// Zeitpunkt mit Zeitzone; im Betrieb `jetzt()`, in Tests gestellt.
pub type Zeit = DateTime<FixedOffset>;

/// So viele erledigte, abgelaufene oder zurückgezogene Aufgaben liefert eine
/// Liste ohne `offen=1` höchstens (die zuletzt geänderten).
pub const ALTE_HOECHSTENS: usize = 200;

/// Höchstens so viele offene Aufgaben je Projekt …
pub const OFFEN_JE_PROJEKT: u64 = 20;
/// … und insgesamt.
pub const OFFEN_INSGESAMT: u64 = 100;
/// Nicht mehr offene Aufgaben ziehen so viele Tage nach ihrer letzten Änderung ins Archiv.
pub const ARCHIVIEREN_NACH_TAGEN: i64 = 7;

/// Jetzt in Ortszeit.
pub fn jetzt() -> Zeit {
    chrono::Local::now().fixed_offset()
}

fn zeit_text(z: &Zeit) -> String {
    z.to_rfc3339_opts(SecondsFormat::Secs, false)
}

/// Eine neue Aufgabe, schon geprüft (siehe `api::aufgaben`).
pub struct NeueAufgabe {
    pub projekt: String,
    pub von: String,
    pub titel: String,
    /// Der Link im Klartext; der Puffer wird nach dem Verschlüsseln überschrieben.
    pub link: Option<Zeroizing<String>>,
    pub knopf: Option<Vec<String>>,
    pub frist: Option<String>,
    pub ablauf: Option<String>,
    /// Vorschlag des Chats für die Stufe (Etappe A1); `None`: bald.
    pub prioritaet: Option<Stufe>,
    /// Bewegliche Frist (Etappe B3), schon geprüft; dann ist `frist` leer.
    pub frist_bezug: Option<crate::vertrag::fristen::FristBezug>,
}

/// Wer liest: nur offene, nur ein Projekt.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub nur_offen: bool,
    pub projekt: Option<String>,
}

/// Ergebnis von `aufgabe_erledigt`.
// Kurzlebiger Rückgabewert; eine Box brächte nur Umbau an allen Stellen, die ihn bauen und lesen.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Haken {
    /// Abgehakt; dazu der neue Vorgang (Art `antwort`) und sein Ereignis.
    Erledigt { aufgabe: Aufgabe, vorgang: Vorgang, ereignis: Ereignis },
    /// War schon nicht mehr offen (oder ist gerade abgelaufen); nichts geschrieben.
    Unveraendert(Aufgabe),
    /// Die Antwort passt nicht zu den Knöpfen der Aufgabe; nichts geschrieben.
    FalscheAntwort { knopf: Option<Vec<String>> },
}

/// Ergebnis von `aufgabe_etwas_anderes`.
// Kurzlebiger Rückgabewert; eine Box brächte nur Umbau an allen Stellen, die ihn bauen und lesen.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Rueckmeldung {
    /// Gespeichert (die Aufgabe bleibt offen); dazu der neue Vorgang und sein Ereignis.
    Gesendet { aufgabe: Aufgabe, vorgang: Vorgang, ereignis: Ereignis },
    /// Derselbe Text stand schon da; nichts geschrieben.
    Gleich(Aufgabe),
    /// Die Aufgabe ist nicht mehr offen (oder abgelaufen); nichts geschrieben.
    NichtOffen(Aufgabe),
}

/// Etappe B2, „Aufgabe ändern ohne neue Nummer“: Titel, Knöpfe und Frist
/// einer offenen Aufgabe, schon geprüft (siehe `api::aufgaben::aendern`).
/// Was `None` ist, bleibt. Kennung, Link, Ablauf, Stufe und die Einstufung
/// des Inhabers bleiben immer.
#[derive(Debug, Clone, Default)]
pub struct AufgabeAenderung {
    pub titel: Option<String>,
    /// `Some(None)`: keine Knöpfe mehr, nur der Haken.
    pub knopf: Option<Option<Vec<String>>>,
    /// `Some(None)`: ohne Frist. Beides ersetzt auch eine bewegliche Frist.
    pub frist: Option<Option<String>>,
    /// Etappe B3: `Some(Some(_))` macht die Frist beweglich (eine feste fällt
    /// weg), `Some(None)` nimmt die bewegliche Frist weg.
    pub frist_bezug: Option<Option<crate::vertrag::fristen::FristBezug>>,
    /// Wer ändert, wie angegeben; `None`: der Chat des Projekts.
    pub von: Option<String>,
}

/// Ergebnis von `aufgabe_aendern`.
#[derive(Debug)]
pub enum Aendern {
    /// Geändert und mit Verlauf geschrieben.
    Geaendert(Aufgabe),
    /// Alles stand schon so; nichts geschrieben.
    Unveraendert(Aufgabe),
    /// Die Aufgabe ist nicht mehr offen (auch abgelaufen oder im Archiv); nichts geschrieben.
    NichtOffen(Aufgabe),
    /// `von` ist nicht der Chat des Projekts der Aufgabe; nichts geschrieben.
    FremderChat,
}

/// Knöpfe als Text für den Verlauf: die JSON-Liste, ohne Knöpfe `None`.
fn knopf_text(knopf: &Option<Vec<String>>) -> Option<String> {
    knopf.as_ref().map(|k| serde_json::to_string(k).unwrap_or_default())
}

/// Wendet eine Änderung an und schreibt je geändertem Feld einen Eintrag in
/// den Verlauf (Zeit `zeit`, von `von`). Lag die Aufgabe beim Chat (eigene
/// Worte des Inhabers) und ändern sich Titel oder Knöpfe, steht sie danach
/// wieder für den Inhaber offen (die Antwort „etwas-anderes“ fällt weg, seine
/// Worte bleiben in `eigene`); das kommt auch in den Verlauf. Gibt zurück, ob
/// sich etwas geändert hat. Prüft nicht, ob die Aufgabe offen ist.
pub fn aenderung_anwenden(a: &mut Aufgabe, aenderung: &AufgabeAenderung, von: &str, zeit: &str) -> bool {
    let eintrag =
        |art, alt: Option<String>, neu: Option<String>, text: String| AufgabeVerlauf { zeit: zeit.to_string(), von: von.to_string(), art, alt, neu, text };
    let war_beim_chat = beim_chat(a);
    let mut neu_gestellt = false;
    let mut verlauf = Vec::new();
    if let Some(t) = aenderung.titel.as_ref().filter(|t| **t != a.titel) {
        verlauf.push(eintrag(AufgabeVerlaufArt::Titel, Some(a.titel.clone()), Some(t.clone()), format!("Titel geändert: „{}“ → „{t}“", a.titel)));
        a.titel = t.clone();
        neu_gestellt = true;
    }
    if let Some(k) = aenderung.knopf.as_ref().filter(|k| **k != a.knopf) {
        let (alt, neu) = (knopf_text(&a.knopf), knopf_text(k));
        let wort = |k: &Option<Vec<String>>| {
            k.as_ref().map_or_else(|| "nur der Haken".to_string(), |k| k.iter().map(|x| format!("„{x}“")).collect::<Vec<_>>().join(", "))
        };
        verlauf.push(eintrag(AufgabeVerlaufArt::Knopf, alt, neu, format!("Knöpfe geändert: {} → {}", wort(&a.knopf), wort(k))));
        a.knopf = k.clone();
        neu_gestellt = true;
    }
    // Feste und bewegliche Frist (Etappe B3) als ein Wert: es gilt höchstens eine.
    let vorher = frist_text(a);
    if let Some(f) = &aenderung.frist {
        // Eine feste Frist (oder `null`: ohne Frist) ersetzt auch eine bewegliche.
        a.frist = f.clone();
        a.frist_bezug = None;
    }
    if let Some(b) = &aenderung.frist_bezug {
        a.frist_bezug = b.clone();
        if b.is_some() {
            a.frist = None;
        }
    }
    let nachher = frist_text(a);
    if nachher != vorher {
        let wort = |f: &Option<String>| f.clone().unwrap_or_else(|| "ohne Frist".to_string());
        let text = format!("Frist geändert: {} → {}", wort(&vorher), wort(&nachher));
        verlauf.push(eintrag(AufgabeVerlaufArt::Frist, vorher, nachher, text));
    }
    if war_beim_chat && neu_gestellt {
        a.antwort = None;
        verlauf.push(eintrag(
            AufgabeVerlaufArt::WiederOffen,
            Some(crate::anstehend::ZUSTAND_BEIM_CHAT.to_string()),
            Some("offen".to_string()),
            "Wieder offen für den Inhaber (nach seinen eigenen Worten neu gestellt)".to_string(),
        ));
    }
    let geaendert = !verlauf.is_empty();
    // Nichts fällt weg (Daten werden nie gelöscht); Ändern ist selten, der Verlauf bleibt klein.
    a.verlauf.extend(verlauf);
    geaendert
}

/// Die Frist einer Aufgabe als ein Text für den Verlauf: das Datum, bei
/// beweglicher Frist Ereignis und Abstand (`zusage-da +14 Tage`), sonst `None`.
pub fn frist_text(a: &Aufgabe) -> Option<String> {
    match &a.frist_bezug {
        Some(b) => Some(format!("{} {}", b.ereignis, b.abstand)),
        None => a.frist.clone(),
    }
}

/// Liegt die offene Aufgabe beim Chat? So ist es, wenn der Inhaber zuletzt mit
/// „Etwas anderes …“ eigene Worte geschickt hat (Entscheidung,
/// Nachtrag der Abnahme): Sie wartet dann nicht mehr auf ihn (nicht in der
/// Zahl, nicht „für dich“), bis der Chat sie abschließt, zurückzieht oder neu stellt.
pub fn beim_chat(a: &Aufgabe) -> bool {
    a.zustand == AufgabenZustand::Offen && a.antwort.as_deref() == Some(ETWAS_ANDERES)
}

/// Ist die Aufgabe offen, aber ihr Ablauf erreicht? Ein unlesbarer Ablauf
/// zählt als erreicht (lieber den Link verlieren als zu lange zeigen).
pub fn abgelaufen(a: &Aufgabe, jetzt: &Zeit) -> bool {
    a.zustand == AufgabenZustand::Offen && a.ablauf.as_deref().is_some_and(|t| DateTime::parse_from_rfc3339(t).map_or(true, |t| t <= *jetzt))
}

/// Neue Kennung `a-JJJJMMTT-NNNN` aus dem Tageszähler in `meta`.
fn neue_kennung(txn: &WriteTransaction, jetzt: &Zeit) -> Result<String, SpeicherFehler> {
    let tag = jetzt.format("%Y%m%d").to_string();
    let schluessel = format!("{META_AUFGABE_ZAEHLER}{tag}");
    let mut meta = txn.open_table(META)?;
    let alt = meta.get(schluessel.as_str())?.map(|w| w.value().to_string());
    let mut n: u64 = match alt {
        Some(text) => text.parse().map_err(|_| SpeicherFehler::Beschaedigt(format!("{schluessel} = {text:?} ist keine Zahl")))?,
        None => 0,
    };
    // Frei heißt: weder in `aufgaben` noch im Archiv (das Archiv überschreibt nie).
    let aufgaben = txn.open_table(AUFGABEN)?;
    let archiv = txn.open_table(AUFGABEN_ARCHIV)?;
    let id = loop {
        n += 1;
        let id = format!("a-{tag}-{n:04}");
        if aufgaben.get(id.as_str())?.is_none() && archiv.get(id.as_str())?.is_none() {
            break id;
        }
    };
    meta.insert(schluessel.as_str(), n.to_string().as_str())?;
    Ok(id)
}

/// Eine Aufgabe aus `aufgaben` (lesend oder schreibend geöffnet).
fn aufgabe_aus<T: ReadableTable<&'static str, &'static [u8]>>(tabelle: &T, id: &str) -> Result<Option<Aufgabe>, SpeicherFehler> {
    match tabelle.get(id)? {
        Some(w) => Ok(Some(json_lesen(w.value(), &format!("aufgaben/{id}"))?)),
        None => Ok(None),
    }
}

/// Setzt eine offene Aufgabe auf `zustand`, löscht ihren Link und schreibt sie.
fn abschliessen(txn: &WriteTransaction, a: &mut Aufgabe, zustand: AufgabenZustand, jetzt: &Zeit) -> Result<(), SpeicherFehler> {
    a.zustand = zustand;
    a.geaendert = zeit_text(jetzt);
    txn.open_table(AUFGABEN_LINKS)?.remove(a.id.as_str())?;
    txn.open_table(AUFGABEN)?.insert(a.id.as_str(), serde_json::to_vec(a)?.as_slice())?;
    Ok(())
}

/// Der Link einer Aufgabe im Klartext, nur wenn sie offen und nicht
/// abgelaufen ist. Lässt er sich nicht entschlüsseln (anderes Konto, Datei
/// von einem anderen PC), `None` und ein Satz im Protokoll ohne Inhalt.
fn link_fuer<T: ReadableTable<&'static str, &'static [u8]>>(links: Option<&T>, a: &Aufgabe, jetzt: &Zeit) -> Option<String> {
    if a.zustand != AufgabenZustand::Offen || !a.hat_link || abgelaufen(a, jetzt) {
        return None;
    }
    let geheim = match links?.get(a.id.as_str()) {
        Ok(Some(w)) => w.value().to_vec(),
        Ok(None) => return None,
        Err(e) => {
            warn!("aufgaben_links/{}: nicht lesbar ({e})", a.id);
            return None;
        }
    };
    match verschluesselung::entschluesseln(&geheim) {
        Ok(klar) => match std::str::from_utf8(&klar) {
            Ok(t) => Some(t.to_string()),
            Err(_) => {
                warn!("aufgaben_links/{}: kein Text nach dem Entschlüsseln", a.id);
                None
            }
        },
        Err(e) => {
            warn!("aufgaben_links/{}: lässt sich nicht entschlüsseln ({e})", a.id);
            None
        }
    }
}

/// Zählt offene (nicht abgelaufene) Aufgaben in `tabelle`: insgesamt und für
/// `projekt`. Liest nur `aufgaben`, nie `aufgaben_links`: entschlüsselt nichts.
fn offene_zaehlen<T: ReadableTable<&'static str, &'static [u8]>>(
    tabelle: &T,
    projekt: Option<&str>,
    ohne: &std::collections::BTreeSet<String>,
    jetzt: &Zeit,
) -> Result<(u64, u64), SpeicherFehler> {
    let (mut gesamt, mut im_projekt) = (0, 0);
    for eintrag in tabelle.iter()? {
        let (k, w) = eintrag?;
        let a: Aufgabe = match json_lesen(w.value(), &format!("aufgaben/{}", k.value())) {
            Ok(a) => a,
            Err(e) => {
                warn!("aufgaben: {} ({})", e, e.einzelheiten());
                continue;
            }
        };
        if a.zustand == AufgabenZustand::Offen && !abgelaufen(&a, jetzt) && !ohne.contains(&a.projekt) {
            gesamt += 1;
            if projekt == Some(a.projekt.as_str()) {
                im_projekt += 1;
            }
        }
    }
    Ok((gesamt, im_projekt))
}

/// Vor dem Anlegen: Ist eine Grenze erreicht, `SpeicherFehler::Grenze` mit Satz.
fn grenze_pruefen(txn: &WriteTransaction, projekt: &str, jetzt: &Zeit) -> Result<(), SpeicherFehler> {
    let (gesamt, im_projekt) = offene_zaehlen(&txn.open_table(AUFGABEN)?, Some(projekt), &Default::default(), jetzt)?;
    if im_projekt >= OFFEN_JE_PROJEKT {
        return Err(SpeicherFehler::Grenze(format!(
            "Für das Projekt {projekt} sind schon {OFFEN_JE_PROJEKT} Aufgaben offen; mehr gehen nicht. Bitte erst eine zurückziehen oder warten, bis der Inhaber welche abhakt."
        )));
    }
    if gesamt >= OFFEN_INSGESAMT {
        return Err(SpeicherFehler::Grenze(format!(
            "Insgesamt sind schon {OFFEN_INSGESAMT} Aufgaben offen; mehr gehen nicht. Bitte erst eine zurückziehen oder warten, bis der Inhaber welche abhakt."
        )));
    }
    Ok(())
}

/// Ist eine nicht mehr offene Aufgabe so alt, dass sie ins Archiv zieht? Eine
/// unlesbare Zeit zählt nicht als alt (lieber in der Liste lassen).
fn zu_archivieren(a: &Aufgabe, jetzt: &Zeit) -> bool {
    a.zustand != AufgabenZustand::Offen
        && DateTime::parse_from_rfc3339(&a.geaendert).is_ok_and(|t| t + chrono::Duration::days(ARCHIVIEREN_NACH_TAGEN) <= *jetzt)
}

/// Eine Aufgabe aus dem Archiv (für Haken und Zurückziehen nach dem Umzug).
fn aus_archiv(txn: &WriteTransaction, id: &str) -> Result<Option<Aufgabe>, SpeicherFehler> {
    aufgabe_aus(&txn.open_table(AUFGABEN_ARCHIV)?, id)
}

/// Wie die API eine Aufgabe zeigt: abgelaufen, auch wenn der Zeitplan noch nicht da war.
fn ansicht(mut a: Aufgabe, link: Option<String>, jetzt: &Zeit) -> AufgabeAnsicht {
    if abgelaufen(&a, jetzt) {
        a.zustand = AufgabenZustand::Abgelaufen;
    }
    // Etappe 3a+: wie nah die Frist ist, aus derselben Regel wie in Anstehend.
    let heute = crate::vertrag::abgeleitet::heute_von(jetzt);
    let frist_stufe = a.frist.as_deref().and_then(|f| chrono::NaiveDate::parse_from_str(f, "%Y-%m-%d").ok()).map(|f| crate::anstehend::frist_stufe(f, heute));
    AufgabeAnsicht { aufgabe: a, link, frist_stufe, ereignis: None }
}

/// Titel des Vorgangs zum Haken, höchstens 200 Zeichen.
fn vorgangs_titel(a: &Aufgabe) -> String {
    kuerzen(match &a.antwort {
        Some(antwort) => format!("Der Inhaber hat geantwortet „{antwort}“: {}", a.titel),
        None => format!("Der Inhaber hat erledigt: {}", a.titel),
    })
}

/// Höchstens 200 Zeichen (wie der Titel eines Vorgangs), sonst mit „…“ gekürzt.
fn kuerzen(satz: String) -> String {
    if satz.chars().count() <= 200 {
        return satz;
    }
    let mut kurz: String = satz.chars().take(199).collect();
    kurz.push('…');
    kurz
}

/// Bezug des Vorgangs: `aufgabe:<id>`, mit Antwort `aufgabe:<id> antwort:<antwort>`.
pub fn vorgangs_bezug(id: &str, antwort: Option<&str>) -> String {
    match antwort {
        Some(a) => format!("aufgabe:{id} antwort:{a}"),
        None => format!("aufgabe:{id}"),
    }
}

/// Reihenfolge: offene zuerst, nach Frist (ohne Frist zuletzt), dann nach
/// Alter; danach die übrigen, zuletzt geänderte zuerst.
fn sortieren(liste: &mut [AufgabeAnsicht]) {
    liste.sort_by(|a, b| {
        let (a, b) = (&a.aufgabe, &b.aufgabe);
        let offen_a = a.zustand == AufgabenZustand::Offen;
        let offen_b = b.zustand == AufgabenZustand::Offen;
        offen_b.cmp(&offen_a).then_with(|| {
            if offen_a {
                let frist = |x: &Aufgabe| (x.frist.is_none(), x.frist.clone());
                frist(a).cmp(&frist(b)).then_with(|| a.erstellt.cmp(&b.erstellt)).then_with(|| a.id.cmp(&b.id))
            } else {
                b.geaendert.cmp(&a.geaendert).then_with(|| b.id.cmp(&a.id))
            }
        })
    });
}

impl Speicher {
    /// Legt eine Aufgabe an (Zustand `offen`); einen Link verschlüsselt in
    /// `aufgaben_links`, beides in einer Transaktion.
    pub fn aufgabe_anlegen(&self, neu: NeueAufgabe, jetzt: &Zeit) -> Result<Aufgabe, SpeicherFehler> {
        let geheim = match &neu.link {
            Some(link) => Some(verschluesselung::verschluesseln(link.as_bytes()).map_err(SpeicherFehler::Verschluesselung)?),
            None => None,
        };
        let txn = self.db.begin_write()?;
        grenze_pruefen(&txn, &neu.projekt, jetzt)?;
        let id = neue_kennung(&txn, jetzt)?;
        let zeit = zeit_text(jetzt);
        let a = Aufgabe {
            id,
            projekt: neu.projekt,
            von: neu.von,
            titel: neu.titel,
            knopf: neu.knopf,
            frist: neu.frist,
            ablauf: neu.ablauf,
            zustand: AufgabenZustand::Offen,
            antwort: None,
            eigene: None,
            erstellt: zeit.clone(),
            geaendert: zeit,
            hat_link: geheim.is_some(),
            vorgang: None,
            prioritaet: neu.prioritaet,
            verlauf: Vec::new(),
            frist_bezug: neu.frist_bezug,
        };
        txn.open_table(AUFGABEN)?.insert(a.id.as_str(), serde_json::to_vec(&a)?.as_slice())?;
        if let Some(g) = &geheim {
            txn.open_table(AUFGABEN_LINKS)?.insert(a.id.as_str(), g.as_slice())?;
        }
        self.festschreiben(txn, &[super::Bereich::Aufgaben])?;
        Ok(a)
    }

    /// Aufgaben nach `filter`, sortiert (`sortieren`). Den Link gibt es nur
    /// bei offenen, nicht abgelaufenen Aufgaben.
    pub fn aufgaben_lesen(&self, filter: &Filter, jetzt: &Zeit) -> Result<Vec<AufgabeAnsicht>, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        let Some(tabelle) = tabellen::lesen(AUFGABEN, &txn)? else { return Ok(Vec::new()) };
        let links = tabellen::lesen(AUFGABEN_LINKS, &txn)?;
        let mut offen = Vec::new();
        let mut alte = Vec::new();
        for eintrag in tabelle.iter()? {
            let (k, w) = eintrag?;
            let a: Aufgabe = match json_lesen(w.value(), &format!("aufgaben/{}", k.value())) {
                Ok(a) => a,
                Err(e) => {
                    warn!("aufgaben: {} ({})", e, e.einzelheiten());
                    continue;
                }
            };
            if filter.projekt.as_deref().is_some_and(|p| p != a.projekt) {
                continue;
            }
            let ist_offen = a.zustand == AufgabenZustand::Offen && !abgelaufen(&a, jetzt);
            if filter.nur_offen && !ist_offen {
                continue;
            }
            let link = link_fuer(links.as_ref(), &a, jetzt);
            let v = ansicht(a, link, jetzt);
            if ist_offen { offen.push(v) } else { alte.push(v) }
        }
        sortieren(&mut alte);
        alte.truncate(ALTE_HOECHSTENS);
        offen.extend(alte);
        sortieren(&mut offen);
        Ok(offen)
    }

    /// Eine Aufgabe, wie die API sie zeigt (auch aus dem Archiv, dann ohne Link).
    pub fn aufgabe(&self, id: &str, jetzt: &Zeit) -> Result<Option<AufgabeAnsicht>, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        let Some(tabelle) = tabellen::lesen(AUFGABEN, &txn)? else { return Ok(None) };
        let Some(a) = aufgabe_aus(&tabelle, id)? else {
            let archiv = tabellen::lesen(AUFGABEN_ARCHIV, &txn)?;
            let a = match archiv {
                Some(t) => aufgabe_aus(&t, id)?,
                None => None,
            };
            return Ok(a.map(|a| ansicht(a, None, jetzt)));
        };
        let links = tabellen::lesen(AUFGABEN_LINKS, &txn)?;
        let link = link_fuer(links.as_ref(), &a, jetzt);
        Ok(Some(ansicht(a, link, jetzt)))
    }

    /// Eine Aufgabe ohne ihren Link (auch aus dem Archiv): reicht, um zu
    /// prüfen, ob es sie gibt und zu welchem Projekt sie gehört. Entschlüsselt nichts.
    pub fn aufgabe_ohne_link(&self, id: &str) -> Result<Option<Aufgabe>, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        if let Some(t) = tabellen::lesen(AUFGABEN, &txn)?
            && let Some(a) = aufgabe_aus(&t, id)?
        {
            return Ok(Some(a));
        }
        match tabellen::lesen(AUFGABEN_ARCHIV, &txn)? {
            Some(t) => aufgabe_aus(&t, id),
            None => Ok(None),
        }
    }

    /// Der Haken des Inhabers: Die Aufgabe wird `erledigt` (mit `antwort`, wenn sie
    /// Knöpfe hat), ihr Link gelöscht, und für ihr Projekt entsteht ein
    /// Vorgang der Art `antwort` (Bezug `aufgabe:<id>`), alles in einer
    /// Transaktion. So weckt das Warten des Chats ihn wie bei jedem Vorgang.
    pub fn aufgabe_erledigt(&self, id: &str, antwort: Option<&str>, jetzt: &Zeit) -> Result<Haken, SpeicherFehler> {
        let txn = self.db.begin_write()?;
        let alt = aufgabe_aus(&txn.open_table(AUFGABEN)?, id)?;
        let Some(mut a) = alt else {
            // Im Archiv ist sie längst abgeschlossen: nichts zu tun.
            return match aus_archiv(&txn, id)? {
                Some(a) => Ok(Haken::Unveraendert(a)),
                None => Err(SpeicherFehler::NichtGefunden(id.to_string())),
            };
        };
        if a.zustand != AufgabenZustand::Offen || abgelaufen(&a, jetzt) {
            // Ohne Commit: nichts geschrieben.
            return Ok(Haken::Unveraendert(a));
        }
        let passt = match (&a.knopf, antwort) {
            (Some(knopf), Some(antwort)) => knopf.iter().any(|k| k == antwort),
            (None, None) => true,
            _ => false,
        };
        if !passt {
            return Ok(Haken::FalscheAntwort { knopf: a.knopf });
        }
        a.antwort = antwort.map(str::to_string);
        let titel = vorgangs_titel(&a);
        let bezug = vorgangs_bezug(&a.id, antwort);
        let (vorgang, ereignis) = vorgang_anlegen_in(&txn, VorgangsArt::Antwort, &a.projekt, &titel, Some(&bezug), "browser")?;
        a.vorgang = Some(vorgang.id.clone());
        abschliessen(&txn, &mut a, AufgabenZustand::Erledigt, jetzt)?;
        self.festschreiben(txn, &[super::Bereich::Aufgaben, super::Bereich::Vorgaenge])?;
        Ok(Haken::Erledigt { aufgabe: a, vorgang, ereignis })
    }

    /// „Etwas anderes …“: Der Inhaber schreibt statt
    /// eines Klicks eigene Worte. Die Aufgabe bleibt offen (samt Link); sie
    /// bekommt `antwort = "etwas-anderes"` und `eigene`, und in derselben
    /// Transaktion entsteht ein Vorgang `antwort` mit Bezug
    /// `aufgabe:<id> antwort:etwas-anderes` und dem Text als Notiz: die
    /// Rückmeldung an den Chat. Derselbe Text noch einmal (Doppelklick)
    /// ändert nichts; ein anderer Text ist eine neue Rückmeldung.
    pub fn aufgabe_etwas_anderes(&self, id: &str, eigene: &str, jetzt: &Zeit) -> Result<Rueckmeldung, SpeicherFehler> {
        let txn = self.db.begin_write()?;
        let alt = aufgabe_aus(&txn.open_table(AUFGABEN)?, id)?;
        let Some(mut a) = alt else {
            return match aus_archiv(&txn, id)? {
                Some(a) => Ok(Rueckmeldung::NichtOffen(a)),
                None => Err(SpeicherFehler::NichtGefunden(id.to_string())),
            };
        };
        if a.zustand != AufgabenZustand::Offen || abgelaufen(&a, jetzt) {
            return Ok(Rueckmeldung::NichtOffen(a));
        }
        if a.antwort.as_deref() == Some(ETWAS_ANDERES) && a.eigene.as_deref() == Some(eigene) {
            return Ok(Rueckmeldung::Gleich(a));
        }
        let titel = kuerzen(format!("Der Inhaber hat etwas anderes geschrieben (eigene Worte): {}", a.titel));
        let bezug = vorgangs_bezug(&a.id, Some(ETWAS_ANDERES));
        let (vorgang, ereignis) = vorgang_anlegen_mit_notiz_in(&txn, VorgangsArt::Antwort, &a.projekt, &titel, Some(&bezug), "browser", Some(eigene))?;
        a.antwort = Some(ETWAS_ANDERES.to_string());
        a.eigene = Some(eigene.to_string());
        a.vorgang = Some(vorgang.id.clone());
        a.geaendert = zeit_text(jetzt);
        txn.open_table(AUFGABEN)?.insert(a.id.as_str(), serde_json::to_vec(&a)?.as_slice())?;
        self.festschreiben(txn, &[super::Bereich::Aufgaben, super::Bereich::Vorgaenge])?;
        Ok(Rueckmeldung::Gesendet { aufgabe: a, vorgang, ereignis })
    }

    /// Etappe B2: Der Chat ändert Titel, Knöpfe oder Frist einer offenen
    /// Aufgabe, ohne neue Kennung (`aenderung_anwenden`, mit Verlauf). Nur
    /// der Chat des Projekts (`von` fehlt oder ist `chat:<projekt>`), in
    /// derselben Transaktion geprüft. Link, Ablauf, Stufe, Einstufung des
    /// Inhabers und Vorgänge bleiben; geweckt wird niemand.
    pub fn aufgabe_aendern(&self, id: &str, aenderung: &AufgabeAenderung, jetzt: &Zeit) -> Result<Aendern, SpeicherFehler> {
        let txn = self.db.begin_write()?;
        let alt = aufgabe_aus(&txn.open_table(AUFGABEN)?, id)?;
        let Some(mut a) = alt else {
            return match aus_archiv(&txn, id)? {
                Some(a) => Ok(Aendern::NichtOffen(a)),
                None => Err(SpeicherFehler::NichtGefunden(id.to_string())),
            };
        };
        let eigener = format!("chat:{}", a.projekt);
        if aenderung.von.as_ref().is_some_and(|v| *v != eigener) {
            return Ok(Aendern::FremderChat);
        }
        if a.zustand != AufgabenZustand::Offen || abgelaufen(&a, jetzt) {
            return Ok(Aendern::NichtOffen(a));
        }
        let zeit = zeit_text(jetzt);
        let von = aenderung.von.clone().unwrap_or(eigener);
        if !aenderung_anwenden(&mut a, aenderung, &von, &zeit) {
            return Ok(Aendern::Unveraendert(a));
        }
        a.geaendert = zeit;
        txn.open_table(AUFGABEN)?.insert(a.id.as_str(), serde_json::to_vec(&a)?.as_slice())?;
        self.festschreiben(txn, &[super::Bereich::Aufgaben])?;
        Ok(Aendern::Geaendert(a))
    }

    /// Der Chat zieht eine offene Aufgabe zurück; ihr Link wird gelöscht.
    /// War sie nicht mehr offen: `Ok(None)`, nichts geschrieben.
    pub fn aufgabe_zurueckziehen(&self, id: &str, jetzt: &Zeit) -> Result<Option<Aufgabe>, SpeicherFehler> {
        let txn = self.db.begin_write()?;
        let alt = aufgabe_aus(&txn.open_table(AUFGABEN)?, id)?;
        let Some(mut a) = alt else {
            return match aus_archiv(&txn, id)? {
                Some(_) => Ok(None),
                None => Err(SpeicherFehler::NichtGefunden(id.to_string())),
            };
        };
        if a.zustand != AufgabenZustand::Offen || abgelaufen(&a, jetzt) {
            return Ok(None);
        }
        abschliessen(&txn, &mut a, AufgabenZustand::Zurueckgezogen, jetzt)?;
        self.festschreiben(txn, &[super::Bereich::Aufgaben])?;
        Ok(Some(a))
    }

    /// Der Zeitplan (und jeder Zugriff der API vorab): Offene Aufgaben, deren
    /// Ablauf erreicht ist, werden `abgelaufen`, ihr Link gelöscht. Dazu fallen
    /// Links weg, deren Aufgabe nicht mehr offen ist oder fehlt (sollte es nie
    /// geben). Schreibt nur, wenn es etwas zu tun gibt. Gibt die abgelaufenen zurück.
    pub fn aufgaben_ablaufen(&self, jetzt: &Zeit) -> Result<Vec<Aufgabe>, SpeicherFehler> {
        let (faellig, verwaist) = {
            let txn = self.db.begin_read()?;
            let Some(tabelle) = tabellen::lesen(AUFGABEN, &txn)? else { return Ok(Vec::new()) };
            let mut faellig = Vec::new();
            for eintrag in tabelle.iter()? {
                let (k, w) = eintrag?;
                match json_lesen::<Aufgabe>(w.value(), &format!("aufgaben/{}", k.value())) {
                    Ok(a) if abgelaufen(&a, jetzt) => faellig.push(a.id),
                    Ok(_) => {}
                    Err(e) => warn!("aufgaben: {} ({})", e, e.einzelheiten()),
                }
            }
            let mut verwaist = Vec::new();
            if let Some(links) = tabellen::lesen(AUFGABEN_LINKS, &txn)? {
                for eintrag in links.iter()? {
                    let (k, _) = eintrag?;
                    let id = k.value();
                    let offen = aufgabe_aus(&tabelle, id)?.is_some_and(|a| a.zustand == AufgabenZustand::Offen);
                    if !offen {
                        verwaist.push(id.to_string());
                    }
                }
            }
            (faellig, verwaist)
        };
        if faellig.is_empty() && verwaist.is_empty() {
            return Ok(Vec::new());
        }
        let txn = self.db.begin_write()?;
        let mut geaendert = Vec::new();
        for id in &faellig {
            let alt = aufgabe_aus(&txn.open_table(AUFGABEN)?, id)?;
            let Some(mut a) = alt else { continue };
            if !abgelaufen(&a, jetzt) {
                continue;
            }
            abschliessen(&txn, &mut a, AufgabenZustand::Abgelaufen, jetzt)?;
            geaendert.push(a);
        }
        if !verwaist.is_empty() {
            warn!("{} Link(s) ohne offene Aufgabe gelöscht", verwaist.len());
            let mut links = txn.open_table(AUFGABEN_LINKS)?;
            for id in &verwaist {
                links.remove(id.as_str())?;
            }
        }
        self.festschreiben(txn, &[super::Bereich::Aufgaben])?;
        Ok(geaendert)
    }

    /// Zahl der offenen, nicht abgelaufenen Aufgaben (für den Seitenkopf).
    /// Liest nur `aufgaben`; kein Link wird gelesen oder entschlüsselt.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn aufgaben_offen_zaehlen(&self, jetzt: &Zeit) -> Result<u64, SpeicherFehler> {
        self.aufgaben_offen_zaehlen_ohne(jetzt, &Default::default())
    }

    /// Wie [`Self::aufgaben_offen_zaehlen`], ohne die Aufgaben der Projekte in
    /// `ohne` (Entscheidung archiv-status: archivierte zählen nirgends mit).
    pub fn aufgaben_offen_zaehlen_ohne(&self, jetzt: &Zeit, ohne: &std::collections::BTreeSet<String>) -> Result<u64, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        let Some(tabelle) = tabellen::lesen(AUFGABEN, &txn)? else { return Ok(0) };
        // Aufgaben beim Chat warten nicht auf den Inhaber; die Grenzen beim
        // Anlegen (offene_zaehlen) zählen sie mit. Archivierte zählen nirgends.
        let mut n = 0;
        for eintrag in tabelle.iter()? {
            let (k, w) = eintrag?;
            match json_lesen::<Aufgabe>(w.value(), &format!("aufgaben/{}", k.value())) {
                Ok(a) if a.zustand == AufgabenZustand::Offen && !abgelaufen(&a, jetzt) && !beim_chat(&a) && !ohne.contains(&a.projekt) => n += 1,
                Ok(_) => {}
                Err(e) => warn!("aufgaben: {} ({})", e, e.einzelheiten()),
            }
        }
        Ok(n)
    }

    /// Erledigte, abgelaufene und zurückgezogene Aufgaben, deren letzte
    /// Änderung `ARCHIVIEREN_NACH_TAGEN` Tage oder länger her ist, ziehen
    /// unverändert nach `aufgaben_archiv` um (in einer Transaktion; nichts
    /// geht verloren). Ein Link, falls noch einer da wäre, wird gelöscht (das
    /// ist für einmalige Links so entschieden). Schreibt nur, wenn es etwas zu
    /// tun gibt. Gibt die archivierten zurück.
    pub fn aufgaben_archivieren(&self, jetzt: &Zeit) -> Result<Vec<Aufgabe>, SpeicherFehler> {
        let alt: Vec<String> = {
            let txn = self.db.begin_read()?;
            let Some(tabelle) = tabellen::lesen(AUFGABEN, &txn)? else { return Ok(Vec::new()) };
            let mut alt = Vec::new();
            for eintrag in tabelle.iter()? {
                let (k, w) = eintrag?;
                if let Ok(a) = json_lesen::<Aufgabe>(w.value(), &format!("aufgaben/{}", k.value()))
                    && zu_archivieren(&a, jetzt)
                {
                    alt.push(a.id);
                }
            }
            alt
        };
        if alt.is_empty() {
            return Ok(Vec::new());
        }
        let txn = self.db.begin_write()?;
        let mut umgezogen = Vec::new();
        {
            let mut aufgaben = txn.open_table(AUFGABEN)?;
            let mut archiv = txn.open_table(AUFGABEN_ARCHIV)?;
            let mut links = txn.open_table(AUFGABEN_LINKS)?;
            for id in &alt {
                let Some(a) = aufgabe_aus(&aufgaben, id)? else { continue };
                if !zu_archivieren(&a, jetzt) {
                    continue;
                }
                // Die Bytes wandern unverändert ins Archiv.
                let roh = aufgaben.get(id.as_str())?.map(|w| w.value().to_vec());
                let Some(roh) = roh else { continue };
                // Nie überschreiben: Steht die Kennung schon im Archiv, bleibt
                // die Aufgabe in `aufgaben` (sie erscheint dann weiter in Listen).
                if archiv.get(id.as_str())?.is_some() {
                    warn!("aufgaben_archiv/{id}: Kennung steht schon im Archiv; die Aufgabe bleibt in aufgaben");
                    continue;
                }
                if archiv.insert(id.as_str(), roh.as_slice())?.is_some() {
                    return Err(SpeicherFehler::Beschaedigt(format!("aufgaben_archiv/{id}: Eintrag wäre überschrieben worden")));
                }
                aufgaben.remove(id.as_str())?;
                links.remove(id.as_str())?;
                umgezogen.push(a);
            }
        }
        self.festschreiben(txn, &[super::Bereich::Aufgaben])?;
        Ok(umgezogen)
    }

    /// Zahl der Einträge im Archiv (für Tests).
    #[cfg(test)]
    pub fn aufgaben_archiv_anzahl(&self) -> u64 {
        let txn = self.db.begin_read().unwrap();
        tabellen::zaehlen(AUFGABEN_ARCHIV, &txn).unwrap()
    }

    /// Zahl der Einträge in `aufgaben_links` (für Tests).
    #[cfg(test)]
    pub fn aufgaben_links_anzahl(&self) -> u64 {
        let txn = self.db.begin_read().unwrap();
        tabellen::zaehlen(AUFGABEN_LINKS, &txn).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// Erfundener Testwert, kein echter Link.
    pub const LINK: &str = "https://beispiel.invalid/anmelden?token=erfunden-4711-geheim";

    fn zeit(stunde: u32, minute: u32) -> Zeit {
        FixedOffset::east_opt(3600).unwrap().with_ymd_and_hms(2026, 10, 28, stunde, minute, 0).unwrap()
    }

    fn neu(projekt: &str, titel: &str, link: Option<&str>, ablauf: Option<Zeit>) -> NeueAufgabe {
        NeueAufgabe {
            projekt: projekt.into(),
            von: format!("chat:{projekt}"),
            titel: titel.into(),
            link: link.map(|l| Zeroizing::new(l.to_string())),
            knopf: None,
            frist: None,
            ablauf: ablauf.map(|z| zeit_text(&z)),
            prioritaet: None,
            frist_bezug: None,
        }
    }

    fn enthaelt(bytes: &[u8], suche: &str) -> bool {
        bytes.windows(suche.len()).any(|w| w == suche.as_bytes())
    }

    /// Abnahme 5a: Der Klartext des Links steht nie in der Datei; der Titel schon.
    #[test]
    fn link_steht_nicht_im_klartext_in_der_datei() {
        let pfad = crate::speicher::test_pfad();
        let s = Speicher::oeffnen(&pfad).unwrap();
        let a = s.aufgabe_anlegen(neu("dashboard", "Bei Probe-Dienst anmelden", Some(LINK), Some(zeit(12, 0))), &zeit(10, 0)).unwrap();
        assert!(a.hat_link);
        // Lesbar für dasselbe Konto.
        let v = s.aufgabe(&a.id, &zeit(10, 1)).unwrap().unwrap();
        assert_eq!(v.link.as_deref(), Some(LINK));
        drop(s);
        let bytes = std::fs::read(&pfad).unwrap();
        assert!(enthaelt(&bytes, "Bei Probe-Dienst anmelden"), "der Titel steht drin (die Suche funktioniert)");
        assert!(!enthaelt(&bytes, LINK), "Link im Klartext in der Datei");
        assert!(!enthaelt(&bytes, "erfunden-4711-geheim"), "Geheimnis im Klartext in der Datei");
        assert!(!enthaelt(&bytes, "beispiel.invalid"), "Adresse im Klartext in der Datei");
        std::fs::remove_file(&pfad).unwrap();
    }

    /// Etappe 3a+: Die Ansicht trägt die Frist-Stufe vom Server, am lokalen
    /// Datum von `jetzt` gerechnet (gestern, heute, morgen, in acht Tagen,
    /// ohne und unlesbare Frist); kurz nach Mitternacht ist morgen heute.
    #[test]
    fn ansicht_traegt_die_frist_stufe() {
        use crate::api::vertrag::FristStufe;
        let s = Speicher::temporaer();
        let mut faelle = Vec::new();
        for (frist, soll) in [
            (Some("2026-10-27"), Some(FristStufe::Ueberfaellig)),
            (Some("2026-10-28"), Some(FristStufe::Heute)),
            (Some("2026-10-29"), Some(FristStufe::Bald)),
            (Some("2026-11-05"), Some(FristStufe::Spaeter)),
            (None, None),
        ] {
            let mut n = neu("dashboard", "Mit Frist", None, None);
            n.frist = frist.map(str::to_string);
            faelle.push((s.aufgabe_anlegen(n, &zeit(10, 0)).unwrap().id, soll));
        }
        for (id, soll) in &faelle {
            assert_eq!(s.aufgabe(id, &zeit(10, 1)).unwrap().unwrap().frist_stufe, *soll, "{id}");
        }
        let mut kaputt = s.aufgabe(&faelle[0].0, &zeit(10, 1)).unwrap().unwrap().aufgabe;
        kaputt.frist = Some("bald".into());
        assert_eq!(ansicht(kaputt, None, &zeit(10, 1)).frist_stufe, None);
        // Um 0:00 des 29. ist die Frist vom 29. heute (lokales Datum, nicht UTC).
        let mitternacht = FixedOffset::east_opt(3600).unwrap().with_ymd_and_hms(2026, 10, 29, 0, 0, 0).unwrap();
        assert_eq!(s.aufgabe(&faelle[2].0, &mitternacht).unwrap().unwrap().frist_stufe, Some(FristStufe::Heute));
        let json = serde_json::to_value(s.aufgabe(&faelle[1].0, &zeit(10, 1)).unwrap().unwrap()).unwrap();
        assert_eq!(json["frist_stufe"], "heute");
    }

    /// Nach einer Kopie der Datei (die tägliche Sicherung) ist der Link im
    /// selben Konto wieder lesbar, solange er nicht abgelaufen ist.
    #[test]
    fn sicherungskopie_ist_im_selben_konto_lesbar() {
        let s = Speicher::temporaer();
        let a = s.aufgabe_anlegen(neu("dashboard", "Anmelden", Some(LINK), Some(zeit(12, 0))), &zeit(10, 0)).unwrap();
        let kopie = s.sichern().unwrap();
        let toml = std::fs::read_to_string(&kopie.export).unwrap();
        assert!(!toml.contains("erfunden-4711") && toml.contains(tabellen::LINK_VERBORGEN), "{toml}");
        assert!(!enthaelt(&std::fs::read(&kopie.datei).unwrap(), "erfunden-4711-geheim"));
        let k = Speicher::oeffnen(&kopie.datei).unwrap();
        let v = k.aufgabe(&a.id, &zeit(11, 59)).unwrap().unwrap();
        assert_eq!(v.link.as_deref(), Some(LINK));
        // Nach dem Ablauf gibt auch die Kopie den Link nicht mehr heraus.
        let v = k.aufgabe(&a.id, &zeit(12, 0)).unwrap().unwrap();
        assert_eq!((v.link, v.aufgabe.zustand), (None, AufgabenZustand::Abgelaufen));
    }

    /// Ablauf mit gestellter Zeit: vorher nichts, danach abgelaufen, Link gelöscht, Titel bleibt.
    #[test]
    fn ablauf_loescht_den_link_und_behaelt_den_titel() {
        let s = Speicher::temporaer();
        let a = s.aufgabe_anlegen(neu("dashboard", "Anmelden", Some(LINK), Some(zeit(12, 0))), &zeit(10, 0)).unwrap();
        let b = s.aufgabe_anlegen(neu("dashboard", "Ohne Ablauf", Some(LINK), None), &zeit(10, 0)).unwrap();
        assert_eq!(s.aufgaben_links_anzahl(), 2);
        assert!(s.aufgaben_ablaufen(&zeit(11, 59)).unwrap().is_empty(), "noch nicht fällig");
        assert_eq!(s.aufgaben_links_anzahl(), 2);
        // Der Lesezugriff prüft selbst: um 12:00 kein Link mehr, auch vor dem Zeitplan.
        let offen = s.aufgaben_lesen(&Filter { nur_offen: true, projekt: None }, &zeit(12, 0)).unwrap();
        assert_eq!(offen.iter().map(|v| v.aufgabe.id.as_str()).collect::<Vec<_>>(), [b.id.as_str()]);
        let alle = s.aufgaben_lesen(&Filter::default(), &zeit(12, 0)).unwrap();
        let va = alle.iter().find(|v| v.aufgabe.id == a.id).unwrap();
        assert_eq!((va.link.as_deref(), va.aufgabe.zustand), (None, AufgabenZustand::Abgelaufen));
        // Der Zeitplan schreibt es fest.
        let weg = s.aufgaben_ablaufen(&zeit(12, 1)).unwrap();
        assert_eq!(weg.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), [a.id.as_str()]);
        assert_eq!(s.aufgaben_links_anzahl(), 1, "nur der Link von b bleibt");
        let v = s.aufgabe(&a.id, &zeit(9, 0)).unwrap().unwrap();
        assert_eq!((v.aufgabe.titel.as_str(), v.aufgabe.zustand, v.aufgabe.hat_link), ("Anmelden", AufgabenZustand::Abgelaufen, true));
        assert_eq!(v.link, None, "auch mit früherer Zeit kommt der Link nicht wieder");
        assert!(s.aufgaben_ablaufen(&zeit(13, 0)).unwrap().is_empty(), "ein zweiter Lauf tut nichts");
    }

    /// Haken: Link weg, Vorgang `antwort` mit Bezug; zweiter Haken ändert nichts.
    #[test]
    fn haken_loescht_den_link_und_legt_den_vorgang_an() {
        let s = Speicher::temporaer();
        let mut n = neu("beispiel", "Beispiel abschicken?", Some(LINK), None);
        n.knopf = Some(vec!["ja".into(), "nein".into()]);
        let a = s.aufgabe_anlegen(n, &zeit(10, 0)).unwrap();
        assert!(matches!(s.aufgabe_erledigt(&a.id, None, &zeit(10, 5)).unwrap(), Haken::FalscheAntwort { .. }));
        assert!(matches!(s.aufgabe_erledigt(&a.id, Some("vielleicht"), &zeit(10, 5)).unwrap(), Haken::FalscheAntwort { .. }));
        assert_eq!(s.cursor().unwrap(), 0, "nichts geschrieben");
        let Haken::Erledigt { aufgabe, vorgang, ereignis } = s.aufgabe_erledigt(&a.id, Some("ja"), &zeit(10, 6)).unwrap() else {
            panic!("erwartet: erledigt");
        };
        assert_eq!((aufgabe.zustand, aufgabe.antwort.as_deref()), (AufgabenZustand::Erledigt, Some("ja")));
        assert_eq!(aufgabe.vorgang.as_deref(), Some(vorgang.id.as_str()));
        assert_eq!((vorgang.art, vorgang.slug.as_str(), vorgang.von.as_str()), (VorgangsArt::Antwort, "beispiel", "browser"));
        assert_eq!(vorgang.bezug.as_deref(), Some(format!("aufgabe:{} antwort:ja", a.id).as_str()));
        assert_eq!(vorgang.titel, "Der Inhaber hat geantwortet „ja“: Beispiel abschicken?");
        assert_eq!(ereignis.slug, "beispiel");
        assert_eq!(s.aufgaben_links_anzahl(), 0);
        let v = s.aufgabe(&a.id, &zeit(10, 7)).unwrap().unwrap();
        assert_eq!((v.link, v.aufgabe.titel.as_str()), (None, "Beispiel abschicken?"));
        assert!(matches!(s.aufgabe_erledigt(&a.id, Some("ja"), &zeit(10, 8)).unwrap(), Haken::Unveraendert(_)));
        assert!(s.aufgabe_zurueckziehen(&a.id, &zeit(10, 9)).unwrap().is_none());
        assert_eq!(s.cursor().unwrap(), 1, "nur ein Vorgang, ein Ereignis");
        assert!(matches!(s.aufgabe_erledigt("a-19990101-0001", None, &zeit(10, 0)), Err(SpeicherFehler::NichtGefunden(_))));
    }

    /// Grenze je Projekt: die 21. offene Aufgabe eines Projekts geht nicht;
    /// andere Projekte schon; eine geschlossene oder abgelaufene zählt nicht mit.
    #[test]
    fn grenze_je_projekt() {
        let s = Speicher::temporaer();
        let mut ids = Vec::new();
        for i in 0..OFFEN_JE_PROJEKT {
            ids.push(s.aufgabe_anlegen(neu("dashboard", &format!("t{i}"), None, None), &zeit(10, 0)).unwrap().id);
        }
        let e = s.aufgabe_anlegen(neu("dashboard", "zu viel", None, None), &zeit(10, 1)).unwrap_err();
        assert!(matches!(&e, SpeicherFehler::Grenze(satz) if satz.contains("Projekt dashboard") && satz.contains("20")), "{e}");
        assert_eq!(s.aufgaben_offen_zaehlen(&zeit(10, 1)).unwrap(), OFFEN_JE_PROJEKT, "nichts angelegt");
        s.aufgabe_anlegen(neu("beispiel", "anderes Projekt", None, None), &zeit(10, 1)).unwrap();
        s.aufgabe_zurueckziehen(&ids[0], &zeit(10, 2)).unwrap().unwrap();
        s.aufgabe_anlegen(neu("dashboard", "wieder Platz", Some(LINK), Some(zeit(11, 0))), &zeit(10, 3)).unwrap();
        assert!(s.aufgabe_anlegen(neu("dashboard", "voll", None, None), &zeit(10, 4)).is_err());
        // Nach dem Ablauf (auch vor dem Zeitplan) zählt sie nicht mehr.
        s.aufgabe_anlegen(neu("dashboard", "nach Ablauf", None, None), &zeit(11, 0)).unwrap();
    }

    /// Grenze insgesamt: die 101. offene Aufgabe geht nicht, auch in einem neuen Projekt.
    #[test]
    fn grenze_insgesamt() {
        let s = Speicher::temporaer();
        for p in 0..OFFEN_INSGESAMT / OFFEN_JE_PROJEKT {
            for i in 0..OFFEN_JE_PROJEKT {
                s.aufgabe_anlegen(neu(&format!("projekt-{p}"), &format!("t{i}"), None, None), &zeit(10, 0)).unwrap();
            }
        }
        assert_eq!(s.aufgaben_offen_zaehlen(&zeit(10, 0)).unwrap(), OFFEN_INSGESAMT);
        let e = s.aufgabe_anlegen(neu("ganz-neu", "zu viel", None, None), &zeit(10, 1)).unwrap_err();
        assert!(matches!(&e, SpeicherFehler::Grenze(satz) if satz.starts_with("Insgesamt sind schon 100")), "{e}");
    }

    /// Archiv mit gestellter Zeit: nach knapp sieben Tagen nichts, nach
    /// sieben Tagen sind erledigte, abgelaufene und zurückgezogene nicht mehr
    /// in der Liste, aber unverändert im Archiv und im Export; offene bleiben.
    /// Gelöscht wird nichts außer den Links.
    #[test]
    fn archiv_nach_sieben_tagen() {
        let s = Speicher::temporaer();
        let tage = |n: i64| zeit(10, 30) + chrono::Duration::days(n);
        let offen = s.aufgabe_anlegen(neu("dashboard", "bleibt offen", None, None), &zeit(10, 0)).unwrap();
        let e = s.aufgabe_anlegen(neu("dashboard", "erledigt", None, None), &zeit(10, 0)).unwrap();
        let z = s.aufgabe_anlegen(neu("dashboard", "zurück", Some(LINK), None), &zeit(10, 0)).unwrap();
        let a = s.aufgabe_anlegen(neu("dashboard", "abgelaufen", Some(LINK), Some(zeit(10, 10))), &zeit(10, 0)).unwrap();
        let a2 = s.aufgabe_anlegen(neu("beispiel", "abgelaufen 2", None, Some(zeit(10, 10))), &zeit(10, 0)).unwrap();
        assert!(matches!(s.aufgabe_erledigt(&e.id, None, &zeit(10, 30)).unwrap(), Haken::Erledigt { .. }));
        s.aufgabe_zurueckziehen(&z.id, &zeit(10, 30)).unwrap().unwrap();
        assert_eq!(s.aufgaben_ablaufen(&zeit(10, 30)).unwrap().len(), 2);
        let knapp = tage(7) - chrono::Duration::minutes(1);
        assert!(s.aufgaben_archivieren(&knapp).unwrap().is_empty(), "vor sieben Tagen noch nichts");
        assert_eq!(s.aufgaben_lesen(&Filter::default(), &knapp).unwrap().len(), 5);
        let erledigt_vorher = s.aufgabe(&e.id, &knapp).unwrap().unwrap().aufgabe;
        let mut archiviert: Vec<String> = s.aufgaben_archivieren(&tage(7)).unwrap().into_iter().map(|a| a.id).collect();
        archiviert.sort();
        let mut soll = vec![e.id.clone(), z.id.clone(), a.id.clone(), a2.id.clone()];
        soll.sort();
        assert_eq!(archiviert, soll);
        // Listen und Zähler sehen das Archiv nicht mehr.
        let rest = s.aufgaben_lesen(&Filter::default(), &tage(7)).unwrap();
        assert_eq!(rest.iter().map(|v| v.aufgabe.id.as_str()).collect::<Vec<_>>(), [offen.id.as_str()]);
        assert_eq!(s.aufgaben_offen_zaehlen(&tage(7)).unwrap(), 1);
        // Aber nichts ist weg: Archiv, Einzelabruf, Export.
        assert_eq!(s.aufgaben_archiv_anzahl(), 4);
        assert_eq!(s.aufgabe(&e.id, &tage(7)).unwrap().unwrap().aufgabe, erledigt_vorher);
        let export: serde_json::Value = serde_json::from_str(&s.export().unwrap().als_json()).unwrap();
        let mut im_export: Vec<String> = export["aufgaben_archiv"].as_array().unwrap().iter().map(|a| a["id"].as_str().unwrap().to_string()).collect();
        im_export.sort();
        assert_eq!(im_export, soll);
        let z_export = export["aufgaben_archiv"].as_array().unwrap().iter().find(|x| x["id"] == z.id.as_str()).unwrap();
        assert_eq!(z_export["link"], tabellen::LINK_VERBORGEN, "der Link selbst steht nie im Export");
        assert_eq!(z_export["titel"], "zurück");
        assert_eq!(s.aufgaben_links_anzahl(), 0);
        // Haken oder Zurückziehen auf Archiviertes ändert nichts (kein 404).
        assert!(matches!(s.aufgabe_erledigt(&e.id, None, &tage(8)).unwrap(), Haken::Unveraendert(_)));
        assert!(s.aufgabe_zurueckziehen(&z.id, &tage(8)).unwrap().is_none());
        // Ein zweiter Lauf tut nichts; ein späterer nimmt die nächste.
        assert!(s.aufgaben_archivieren(&tage(30)).unwrap().is_empty());
        s.aufgabe_zurueckziehen(&offen.id, &tage(31)).unwrap().unwrap();
        assert_eq!(s.aufgaben_archivieren(&tage(38)).unwrap().len(), 1);
        assert_eq!(s.aufgaben_archiv_anzahl(), 5);
        assert!(s.aufgaben_lesen(&Filter::default(), &tage(38)).unwrap().is_empty());
    }

    /// Das Archiv überschreibt nie: Steht eine Kennung schon im Archiv, bleibt
    /// die Aufgabe in `aufgaben`, der Archiv-Eintrag unverändert; und eine neue
    /// Kennung meidet auch Kennungen im Archiv.
    #[test]
    fn archiv_ueberschreibt_nie() {
        let s = Speicher::temporaer();
        let fremd = br#"{"fremd": "schon im Archiv"}"#;
        let a = s.aufgabe_anlegen(neu("dashboard", "erledigt", None, None), &zeit(10, 0)).unwrap();
        s.aufgabe_zurueckziehen(&a.id, &zeit(10, 1)).unwrap().unwrap();
        {
            let txn = s.db.begin_write().unwrap();
            txn.open_table(AUFGABEN_ARCHIV).unwrap().insert(a.id.as_str(), fremd.as_slice()).unwrap();
            // Eine Kennung des nächsten Tages liegt schon im Archiv, der Tageszähler fehlt.
            txn.open_table(AUFGABEN_ARCHIV).unwrap().insert("a-20261029-0001", fremd.as_slice()).unwrap();
            txn.commit().unwrap();
        }
        let spaeter = zeit(10, 1) + chrono::Duration::days(8);
        assert!(s.aufgaben_archivieren(&spaeter).unwrap().is_empty(), "nichts umgezogen");
        let txn = s.db.begin_read().unwrap();
        let archiv = txn.open_table(AUFGABEN_ARCHIV).unwrap();
        assert_eq!(archiv.get(a.id.as_str()).unwrap().unwrap().value(), fremd.as_slice(), "Archiv unverändert");
        assert!(txn.open_table(AUFGABEN).unwrap().get(a.id.as_str()).unwrap().is_some(), "bleibt in aufgaben");
        drop(archiv);
        drop(txn);
        let morgen = FixedOffset::east_opt(3600).unwrap().with_ymd_and_hms(2026, 10, 29, 9, 0, 0).unwrap();
        let b = s.aufgabe_anlegen(neu("dashboard", "neu", None, None), &morgen).unwrap();
        assert_eq!(b.id, "a-20261029-0002", "die Kennung im Archiv wird nicht noch einmal vergeben");
    }

    /// Die Zahl für den Seitenkopf entschlüsselt keinen Link (Zähler im Test);
    /// die volle Liste schon (Gegenprobe, dass der Zähler zählt).
    #[test]
    fn anzahl_entschluesselt_nichts() {
        let s = Speicher::temporaer();
        for i in 0..3 {
            s.aufgabe_anlegen(neu("dashboard", &format!("t{i}"), Some(LINK), None), &zeit(10, 0)).unwrap();
        }
        s.aufgabe_anlegen(neu("dashboard", "abgelaufen", Some(LINK), Some(zeit(10, 5))), &zeit(10, 0)).unwrap();
        let vorher = verschluesselung::entschluesselt_in_diesem_thread();
        assert_eq!(s.aufgaben_offen_zaehlen(&zeit(10, 10)).unwrap(), 3);
        assert_eq!(verschluesselung::entschluesselt_in_diesem_thread(), vorher, "die Zählung hat entschlüsselt");
        let liste = s.aufgaben_lesen(&Filter { nur_offen: true, projekt: None }, &zeit(10, 10)).unwrap();
        assert_eq!(liste.len(), 3);
        assert_eq!(verschluesselung::entschluesselt_in_diesem_thread(), vorher + 3, "Gegenprobe");
    }

    /// Zum Einstufen reicht die Aufgabe ohne Link: nichts entschlüsselt,
    /// auch aus dem Archiv; unbekannt `None`. Gegenprobe mit `aufgabe`.
    #[test]
    fn aufgabe_ohne_link_entschluesselt_nichts() {
        let s = Speicher::temporaer();
        let a = s.aufgabe_anlegen(neu("dashboard", "mit Link", Some(LINK), None), &zeit(10, 0)).unwrap();
        let vorher = verschluesselung::entschluesselt_in_diesem_thread();
        let gelesen = s.aufgabe_ohne_link(&a.id).unwrap().unwrap();
        assert_eq!((gelesen.id.as_str(), gelesen.projekt.as_str(), gelesen.titel.as_str()), (a.id.as_str(), "dashboard", "mit Link"));
        assert!(s.aufgabe_ohne_link("a-19990101-0001").unwrap().is_none());
        assert_eq!(verschluesselung::entschluesselt_in_diesem_thread(), vorher, "ohne Link entschlüsselt nichts");
        assert!(s.aufgabe(&a.id, &zeit(10, 1)).unwrap().unwrap().link.is_some());
        assert_eq!(verschluesselung::entschluesselt_in_diesem_thread(), vorher + 1, "Gegenprobe");
    }

    #[test]
    fn zurueckziehen_loescht_den_link() {
        let s = Speicher::temporaer();
        let a = s.aufgabe_anlegen(neu("dashboard", "x", Some(LINK), None), &zeit(10, 0)).unwrap();
        let z = s.aufgabe_zurueckziehen(&a.id, &zeit(10, 1)).unwrap().unwrap();
        assert_eq!(z.zustand, AufgabenZustand::Zurueckgezogen);
        assert_eq!(s.aufgaben_links_anzahl(), 0);
        assert!(s.aufgabe_zurueckziehen(&a.id, &zeit(10, 2)).unwrap().is_none());
        assert_eq!(s.cursor().unwrap(), 0, "kein Vorgang");
    }

    #[test]
    fn sortiert_nach_frist_ohne_frist_zuletzt() {
        let s = Speicher::temporaer();
        let mut ids = Vec::new();
        for (titel, frist) in [("ohne", None), ("spaet", Some("2026-11-05")), ("frueh", Some("2026-10-27")), ("ohne2", None)] {
            let mut n = neu("dashboard", titel, None, None);
            n.frist = frist.map(str::to_string);
            ids.push(s.aufgabe_anlegen(n, &zeit(10, 0)).unwrap().id);
        }
        s.aufgabe_anlegen(neu("anderes", "fremd", None, None), &zeit(10, 0)).unwrap();
        let f = Filter { nur_offen: true, projekt: Some("dashboard".into()) };
        let titel: Vec<String> = s.aufgaben_lesen(&f, &zeit(10, 1)).unwrap().into_iter().map(|v| v.aufgabe.titel).collect();
        assert_eq!(titel, ["frueh", "spaet", "ohne", "ohne2"]);
        assert_eq!(ids[0], "a-20261028-0001");
        assert_eq!(s.aufgaben_lesen(&Filter::default(), &zeit(10, 1)).unwrap().len(), 5);
    }
}
