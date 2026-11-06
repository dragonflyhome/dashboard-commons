//! Meldungen für den Inhaber (Etappe A3, Schema 6): Tabellen `meldungen` und
//! `meldungen_archiv`.
//!
//! Eine Meldung entsteht meist im Server selbst (`crate::meldungen`: Bau rot,
//! Prüfung rot, Frist nah oder vorbei, Bote fehlt, Arbeit fertig), sonst über
//! `POST /api/meldungen` vom Chat. Wiederholungen fasst der `schluessel`
//! zusammen (er enthält immer das Projekt vorn, `<projekt>/<was>`); wie eine
//! Wiederholung zählt, sagt [`Wiederholung`].
//!
//! Grenze gegen Flut: höchstens [`UNGELESEN_JE_PROJEKT`] offene (ungelesene,
//! nicht behobene) Meldungen je Projekt. Darüber legt der Server die ältesten
//! als gelesen ab (`zusammengefasst`) und zählt eine Sammelmeldung
//! `<projekt>/aeltere-meldungen` hoch; Fehler-Meldungen trifft das zuletzt.
//!
//! Verfall: Gelesene und behobene Meldungen ziehen [`ARCHIVIEREN_NACH_TAGEN`]
//! Tage danach unverändert (nur mit `archiviert`) nach `meldungen_archiv`; nie
//! gelöscht, nie überschrieben. Eine Zustands-Meldung (Fehler, Frist) bleibt,
//! solange ihr Grund besteht (`behoben` leer), auch wenn der Inhaber sie gelesen
//! hat: Sonst käme sie eine Minute nach dem Umzug als neue, ungelesene
//! Meldung wieder (`mit_schluessel` sucht nur außerhalb des Archivs).
//!
//! Alle Methoden bekommen „jetzt“ als Eingabe, damit Tests die Uhr stellen.

use chrono::{DateTime, SecondsFormat};
use redb::{ReadTransaction, ReadableDatabase, ReadableTable, WriteTransaction};
use tracing::warn;

use super::aufgaben::Zeit;
use super::tabellen::{self, MELDUNGEN, MELDUNGEN_ARCHIV, META, META_MELDUNG_ZAEHLER, VORGAENGE};
use super::vorgaenge::json_lesen;
use super::{Speicher, SpeicherFehler};
use crate::api::vertrag::{Meldung, MeldungsArt, Vorgang};

/// Höchstens so viele offene (ungelesene, nicht behobene) Meldungen je Projekt.
pub const UNGELESEN_JE_PROJEKT: usize = 20;
/// Gelesene und behobene Meldungen ziehen so viele Tage danach ins Archiv.
pub const ARCHIVIEREN_NACH_TAGEN: i64 = 7;
/// Was nach dem Projekt im Schlüssel der Sammelmeldung steht.
pub const ZUSAMMENGEFASST: &str = "aeltere-meldungen";
/// Titel der Sammelmeldung.
pub const SATZ_ZUSAMMENGEFASST: &str = "Ältere Meldungen sind als gelesen abgelegt, weil mehr als 20 ungelesen waren. Sie stehen unter „Zuletzt passiert“.";
/// Vorgänge, die so viele Tage alt sind, zählen nicht mehr als „zuletzt passiert“.
pub const ZULETZT_TAGE: i64 = 7;

fn zeit_text(z: &Zeit) -> String {
    z.to_rfc3339_opts(SecondsFormat::Secs, false)
}

/// Wie eine Meldung mit einem `schluessel` zählt, den es schon gibt (außerhalb des Archivs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wiederholung {
    /// Jedes Ereignis zählt (Bau rot, Chat meldet dasselbe noch einmal):
    /// `anzahl` hoch, `zuletzt` neu, wieder ungelesen und nicht behoben.
    Zaehlen,
    /// Ein Zustand (Prüfung rot, Frist nah, Bote fehlt): Solange die Meldung
    /// nicht behoben ist, bleibt sie, wie sie ist (der Titel wird aktuell).
    /// Ist sie behoben und kommt der Zustand wieder, zählt das wie `Zaehlen`.
    Zustand,
    /// Einmal genug (Arbeit fertig, Chat meldet fertig mit Verweis auf sie):
    /// gibt es sie schon, bleibt sie, wie sie ist.
    Einmal,
}

/// Eine neue Meldung, schon geprüft.
#[derive(Debug, Clone)]
pub struct NeueMeldung {
    pub projekt: String,
    pub art: MeldungsArt,
    /// Ganzer Schlüssel mit dem Projekt vorn (siehe [`schluessel`]).
    pub schluessel: String,
    pub titel: String,
    pub text: Option<String>,
    pub verweis: Option<String>,
    pub system: bool,
    pub geschuetzt: bool,
    pub von: String,
}

/// Der Schlüssel einer Meldung: immer mit dem Projekt vorn.
pub fn schluessel(projekt: &str, was: &str) -> String {
    format!("{projekt}/{was}")
}

/// Ergebnis von `meldung_melden`.
#[derive(Debug, Clone, PartialEq)]
pub enum Melden {
    Neu(Meldung),
    /// Wiederholung: hochgezählt (oder nach „behoben“ wieder offen).
    Hochgezaehlt(Meldung),
    /// Der Zustand besteht weiter, nur der Satz ist neu (etwa „morgen“ statt
    /// „in 2 Tagen“); weder hochgezählt noch wieder ungelesen.
    Aktualisiert(Meldung),
    /// Gab es schon; nichts geändert.
    Unveraendert(Meldung),
}

impl Melden {
    pub fn meldung(&self) -> &Meldung {
        match self {
            Self::Neu(m) | Self::Hochgezaehlt(m) | Self::Aktualisiert(m) | Self::Unveraendert(m) => m,
        }
    }

    /// Ob der Inhaber etwas Neues sehen soll (neu oder hochgezählt).
    pub fn geaendert(&self) -> bool {
        matches!(self, Self::Neu(_) | Self::Hochgezaehlt(_))
    }

    /// Ob sich in der Datei etwas geändert hat (dann `change` senden).
    pub fn geschrieben(&self) -> bool {
        !matches!(self, Self::Unveraendert(_))
    }
}

/// Ist die Meldung offen (steht sie in „Anstehend“ und im Band)? Ungelesen und nicht behoben.
pub fn offen(m: &Meldung) -> bool {
    m.gelesen.is_none() && m.behoben.is_none()
}

/// Eine offene Fehler-Meldung (für die Ampel): nicht behoben, gelesen oder nicht.
pub fn fehler_offen(m: &Meldung) -> bool {
    m.art == MeldungsArt::Fehler && m.behoben.is_none()
}

/// Besteht der Grund der Meldung noch? Fehler und Fristen sind Zustände, die
/// der Server selbst prüft und mit `behoben` aufhebt (Frist-Meldungen kommen
/// nur vom Server, immer als [`Wiederholung::Zustand`]). Solange `behoben`
/// leer ist, zieht so eine Meldung nicht ins Archiv, auch gelesen nicht.
pub fn grund_besteht(m: &Meldung) -> bool {
    matches!(m.art, MeldungsArt::Fehler | MeldungsArt::Frist) && m.behoben.is_none()
}

/// Neue Kennung `m-JJJJMMTT-NNNN` aus dem Tageszähler in `meta`; frei heißt:
/// weder in `meldungen` noch im Archiv.
fn neue_kennung(txn: &WriteTransaction, jetzt: &Zeit) -> Result<String, SpeicherFehler> {
    let tag = jetzt.format("%Y%m%d").to_string();
    let schluessel = format!("{META_MELDUNG_ZAEHLER}{tag}");
    let mut meta = txn.open_table(META)?;
    let alt = meta.get(schluessel.as_str())?.map(|w| w.value().to_string());
    let mut n: u64 = match alt {
        Some(text) => text.parse().map_err(|_| SpeicherFehler::Beschaedigt(format!("{schluessel} = {text:?} ist keine Zahl")))?,
        None => 0,
    };
    let aktiv = txn.open_table(MELDUNGEN)?;
    let archiv = txn.open_table(MELDUNGEN_ARCHIV)?;
    let id = loop {
        n += 1;
        let id = format!("m-{tag}-{n:04}");
        if aktiv.get(id.as_str())?.is_none() && archiv.get(id.as_str())?.is_none() {
            break id;
        }
    };
    meta.insert(schluessel.as_str(), n.to_string().as_str())?;
    Ok(id)
}

/// Alle lesbaren Meldungen einer Tabelle; unlesbare stehen im Protokoll.
fn alle_aus<T: ReadableTable<&'static str, &'static [u8]>>(tabelle: &T) -> Result<Vec<Meldung>, SpeicherFehler> {
    let mut out = Vec::new();
    for eintrag in tabelle.iter()? {
        let (k, w) = eintrag?;
        match json_lesen::<Meldung>(w.value(), &format!("meldungen/{}", k.value())) {
            Ok(m) => out.push(m),
            Err(e) => warn!("meldungen: {} ({})", e, e.einzelheiten()),
        }
    }
    Ok(out)
}

fn eine_aus<T: ReadableTable<&'static str, &'static [u8]>>(tabelle: &T, id: &str) -> Result<Option<Meldung>, SpeicherFehler> {
    match tabelle.get(id)? {
        Some(w) => Ok(Some(json_lesen(w.value(), &format!("meldungen/{id}"))?)),
        None => Ok(None),
    }
}

fn schreiben(txn: &WriteTransaction, m: &Meldung) -> Result<(), SpeicherFehler> {
    txn.open_table(MELDUNGEN)?.insert(m.id.as_str(), serde_json::to_vec(m)?.as_slice())?;
    Ok(())
}

/// Die Meldung mit diesem Schlüssel außerhalb des Archivs (es gibt höchstens eine).
fn mit_schluessel(txn: &WriteTransaction, schluessel: &str) -> Result<Option<Meldung>, SpeicherFehler> {
    Ok(alle_aus(&txn.open_table(MELDUNGEN)?)?.into_iter().find(|m| m.schluessel == schluessel))
}

/// Neueste zuerst (nach `zuletzt`), dann nach Kennung.
pub fn sortieren(liste: &mut [Meldung]) {
    liste.sort_by(|a, b| zeit_wert(&b.zuletzt).cmp(&zeit_wert(&a.zuletzt)).then_with(|| b.id.cmp(&a.id)));
}

/// Zeit zum Vergleichen (über Zeitumstellungen hinweg); unlesbar ganz früh.
fn zeit_wert(t: &str) -> i64 {
    DateTime::parse_from_rfc3339(t).map(|z| z.timestamp()).unwrap_or(i64::MIN)
}

/// Ist eine Meldung so alt, dass sie ins Archiv zieht? Gelesen oder behoben,
/// und das ist `ARCHIVIEREN_NACH_TAGEN` Tage her (das spätere zählt). Eine
/// Fehler- oder Frist-Meldung ohne `behoben` bleibt immer ([`grund_besteht`]).
pub fn zu_archivieren(m: &Meldung, jetzt: &Zeit) -> bool {
    if grund_besteht(m) {
        return false;
    }
    let stempel = [m.gelesen.as_deref(), m.behoben.as_deref()].into_iter().flatten().filter_map(|t| DateTime::parse_from_rfc3339(t).ok()).max();
    stempel.is_some_and(|t| t + chrono::Duration::days(ARCHIVIEREN_NACH_TAGEN) <= *jetzt)
}

/// Grenze je Projekt: Sind mehr als `UNGELESEN_JE_PROJEKT` offen, legt der
/// Server die ältesten (Fehler zuletzt) als gelesen ab und zählt die
/// Sammelmeldung hoch. Gibt zurück, ob etwas abgelegt wurde.
fn grenze_anwenden(txn: &WriteTransaction, projekt: &str, jetzt: &Zeit) -> Result<bool, SpeicherFehler> {
    let sammel = schluessel(projekt, ZUSAMMENGEFASST);
    let alle: Vec<Meldung> = alle_aus(&txn.open_table(MELDUNGEN)?)?.into_iter().filter(|m| m.projekt == projekt).collect();
    let mut offene: Vec<Meldung> = alle.iter().filter(|m| offen(m) && m.schluessel != sammel).cloned().collect();
    let sammel_offen = alle.iter().any(|m| m.schluessel == sammel && offen(m));
    if offene.len() + usize::from(sammel_offen) <= UNGELESEN_JE_PROJEKT {
        return Ok(false);
    }
    // Platz für die Sammelmeldung lassen.
    let zuviel = offene.len() - (UNGELESEN_JE_PROJEKT - 1);
    offene.sort_by(|a, b| (a.art == MeldungsArt::Fehler, zeit_wert(&a.zuletzt), &a.id).cmp(&(b.art == MeldungsArt::Fehler, zeit_wert(&b.zuletzt), &b.id)));
    let zeit = zeit_text(jetzt);
    for mut m in offene.into_iter().take(zuviel) {
        m.gelesen = Some(zeit.clone());
        m.zusammengefasst = true;
        schreiben(txn, &m)?;
    }
    let s = match alle.into_iter().find(|m| m.schluessel == sammel) {
        Some(mut s) => {
            s.anzahl += zuviel as u64;
            s.zuletzt = zeit.clone();
            s.gelesen = None;
            s.behoben = None;
            s
        }
        None => Meldung {
            id: neue_kennung(txn, jetzt)?,
            projekt: projekt.to_string(),
            art: MeldungsArt::Info,
            schluessel: sammel,
            titel: SATZ_ZUSAMMENGEFASST.to_string(),
            text: None,
            verweis: None,
            system: false,
            geschuetzt: false,
            von: "server".to_string(),
            erstellt: zeit.clone(),
            zuletzt: zeit,
            anzahl: zuviel as u64,
            gelesen: None,
            behoben: None,
            archiviert: None,
            zusammengefasst: false,
        },
    };
    schreiben(txn, &s)?;
    Ok(true)
}

/// Ergebnis von `meldung_gelesen`.
#[derive(Debug, Clone, PartialEq)]
pub enum Gelesen {
    Geaendert(Meldung),
    /// War schon gelesen oder liegt im Archiv.
    Unveraendert(Meldung),
}

impl Speicher {
    /// Meldet etwas (neu oder als Wiederholung, siehe [`Wiederholung`]) und
    /// wendet die Grenze je Projekt an, alles in einer Transaktion.
    pub fn meldung_melden(&self, neu: NeueMeldung, wiederholung: Wiederholung, jetzt: &Zeit) -> Result<Melden, SpeicherFehler> {
        let txn = self.db.begin_write()?;
        let zeit = zeit_text(jetzt);
        let ergebnis = match mit_schluessel(&txn, &neu.schluessel)? {
            Some(mut m) => {
                let zaehlt = match wiederholung {
                    Wiederholung::Zaehlen => true,
                    Wiederholung::Zustand => m.behoben.is_some(),
                    Wiederholung::Einmal => false,
                };
                if zaehlt {
                    m.anzahl += 1;
                    m.zuletzt = zeit;
                    m.gelesen = None;
                    m.behoben = None;
                    m.zusammengefasst = false;
                    m.titel = neu.titel;
                    m.text = neu.text.or(m.text);
                    Melden::Hochgezaehlt(m)
                } else if wiederholung == Wiederholung::Zustand && (m.titel != neu.titel || m.text != neu.text) {
                    // Der Zustand besteht weiter, nur mit anderem Satz (etwa „noch 1 Tag“).
                    m.titel = neu.titel;
                    m.text = neu.text;
                    schreiben(&txn, &m)?;
                    self.festschreiben(txn, &[super::Bereich::Meldungen])?;
                    return Ok(Melden::Aktualisiert(m));
                } else {
                    return Ok(Melden::Unveraendert(m));
                }
            }
            None => Melden::Neu(Meldung {
                id: neue_kennung(&txn, jetzt)?,
                projekt: neu.projekt,
                art: neu.art,
                schluessel: neu.schluessel,
                titel: neu.titel,
                text: neu.text,
                verweis: neu.verweis,
                system: neu.system,
                geschuetzt: neu.geschuetzt,
                von: neu.von,
                erstellt: zeit.clone(),
                zuletzt: zeit,
                anzahl: 1,
                gelesen: None,
                behoben: None,
                archiviert: None,
                zusammengefasst: false,
            }),
        };
        let m = ergebnis.meldung().clone();
        schreiben(&txn, &m)?;
        grenze_anwenden(&txn, &m.projekt, jetzt)?;
        // Die Grenze kann gerade diese Meldung abgelegt haben (sie war nicht die neueste).
        let m = eine_aus(&txn.open_table(MELDUNGEN)?, &m.id)?.unwrap_or(m);
        self.festschreiben(txn, &[super::Bereich::Meldungen])?;
        Ok(match ergebnis {
            Melden::Neu(_) => Melden::Neu(m),
            Melden::Hochgezaehlt(_) => Melden::Hochgezaehlt(m),
            Melden::Aktualisiert(_) => Melden::Aktualisiert(m),
            Melden::Unveraendert(_) => Melden::Unveraendert(m),
        })
    }

    /// Setzt `behoben` bei allen nicht behobenen Meldungen außerhalb des
    /// Archivs, für die `welche` zutrifft. Gibt die geänderten zurück.
    pub fn meldungen_beheben(&self, welche: impl Fn(&Meldung) -> bool, jetzt: &Zeit) -> Result<Vec<Meldung>, SpeicherFehler> {
        let kandidaten: Vec<String> = {
            let txn = self.db.begin_read()?;
            let Some(t) = tabellen::lesen(MELDUNGEN, &txn)? else { return Ok(Vec::new()) };
            alle_aus(&t)?.into_iter().filter(|m| m.behoben.is_none() && welche(m)).map(|m| m.id).collect()
        };
        if kandidaten.is_empty() {
            return Ok(Vec::new());
        }
        let txn = self.db.begin_write()?;
        let mut geaendert = Vec::new();
        for id in kandidaten {
            let Some(mut m) = eine_aus(&txn.open_table(MELDUNGEN)?, &id)? else { continue };
            if m.behoben.is_some() || !welche(&m) {
                continue;
            }
            m.behoben = Some(zeit_text(jetzt));
            schreiben(&txn, &m)?;
            geaendert.push(m);
        }
        self.festschreiben(txn, &[super::Bereich::Meldungen])?;
        Ok(geaendert)
    }

    /// Der Inhaber hat „gelesen“ geklickt. Unbekannte Kennung: `NichtGefunden`.
    pub fn meldung_gelesen(&self, id: &str, jetzt: &Zeit) -> Result<Gelesen, SpeicherFehler> {
        let txn = self.db.begin_write()?;
        let Some(mut m) = eine_aus(&txn.open_table(MELDUNGEN)?, id)? else {
            return match eine_aus(&txn.open_table(MELDUNGEN_ARCHIV)?, id)? {
                Some(m) => Ok(Gelesen::Unveraendert(m)),
                None => Err(SpeicherFehler::NichtGefunden(id.to_string())),
            };
        };
        if m.gelesen.is_some() {
            return Ok(Gelesen::Unveraendert(m));
        }
        m.gelesen = Some(zeit_text(jetzt));
        schreiben(&txn, &m)?;
        self.festschreiben(txn, &[super::Bereich::Meldungen])?;
        Ok(Gelesen::Geaendert(m))
    }

    /// Eine Meldung (auch aus dem Archiv).
    pub fn meldung(&self, id: &str) -> Result<Option<Meldung>, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        for def in [MELDUNGEN, MELDUNGEN_ARCHIV] {
            if let Some(t) = tabellen::lesen(def, &txn)?
                && let Some(m) = eine_aus(&t, id)?
            {
                return Ok(Some(m));
            }
        }
        Ok(None)
    }

    /// Alle Meldungen außerhalb des Archivs, wahlweise nur eines Projekts, neueste zuerst.
    pub fn meldungen_lesen(&self, projekt: Option<&str>) -> Result<Vec<Meldung>, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        let mut liste: Vec<Meldung> = meldungen_aus(&txn)?.into_iter().filter(|m| projekt.is_none_or(|p| p == m.projekt)).collect();
        sortieren(&mut liste);
        Ok(liste)
    }

    /// Gelesene und behobene Meldungen, die `ARCHIVIEREN_NACH_TAGEN` Tage
    /// oder länger so stehen, ziehen nach `meldungen_archiv` um (mit
    /// `archiviert`). Steht die Kennung dort schon, bleibt die Meldung, wo sie ist.
    pub fn meldungen_archivieren(&self, jetzt: &Zeit) -> Result<Vec<Meldung>, SpeicherFehler> {
        let alt: Vec<String> = {
            let txn = self.db.begin_read()?;
            let Some(t) = tabellen::lesen(MELDUNGEN, &txn)? else { return Ok(Vec::new()) };
            alle_aus(&t)?.into_iter().filter(|m| zu_archivieren(m, jetzt)).map(|m| m.id).collect()
        };
        if alt.is_empty() {
            return Ok(Vec::new());
        }
        let txn = self.db.begin_write()?;
        let mut umgezogen = Vec::new();
        {
            let mut aktiv = txn.open_table(MELDUNGEN)?;
            let mut archiv = txn.open_table(MELDUNGEN_ARCHIV)?;
            for id in &alt {
                let Some(mut m) = eine_aus(&aktiv, id)? else { continue };
                if !zu_archivieren(&m, jetzt) {
                    continue;
                }
                if archiv.get(id.as_str())?.is_some() {
                    warn!("meldungen_archiv/{id}: Kennung steht schon im Archiv; die Meldung bleibt in meldungen");
                    continue;
                }
                m.archiviert = Some(zeit_text(jetzt));
                if archiv.insert(id.as_str(), serde_json::to_vec(&m)?.as_slice())?.is_some() {
                    return Err(SpeicherFehler::Beschaedigt(format!("meldungen_archiv/{id}: Eintrag wäre überschrieben worden")));
                }
                aktiv.remove(id.as_str())?;
                umgezogen.push(m);
            }
        }
        self.festschreiben(txn, &[super::Bereich::Meldungen])?;
        Ok(umgezogen)
    }

    /// Zahl der Einträge im Archiv der Meldungen (für Tests).
    #[cfg(test)]
    pub fn meldungen_archiv_anzahl(&self) -> u64 {
        let txn = self.db.begin_read().unwrap();
        tabellen::zaehlen(MELDUNGEN_ARCHIV, &txn).unwrap()
    }

    /// Alle Meldungen, laufend und im Archiv, ohne zugesagte Reihenfolge (für
    /// den Verlauf, Etappe 2e).
    pub fn meldungen_mit_archiv(&self) -> Result<Vec<Meldung>, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        let mut liste = meldungen_aus(&txn)?;
        if let Some(t) = tabellen::lesen(MELDUNGEN_ARCHIV, &txn)? {
            liste.extend(alle_aus(&t)?);
        }
        Ok(liste)
    }
}

/// Alle Meldungen außerhalb des Archivs (für „Anstehend“, in derselben Lese-Transaktion).
pub(super) fn meldungen_aus(txn: &ReadTransaction) -> Result<Vec<Meldung>, SpeicherFehler> {
    match tabellen::lesen(MELDUNGEN, txn)? {
        Some(t) => alle_aus(&t),
        None => Ok(Vec::new()),
    }
}

/// Vorgänge, die in den letzten `ZULETZT_TAGE` Tagen geändert wurden (für
/// „Zuletzt passiert“), neueste zuerst.
pub(super) fn zuletzt_vorgaenge_aus(txn: &ReadTransaction, jetzt: &Zeit) -> Result<Vec<Vorgang>, SpeicherFehler> {
    let mut out = Vec::new();
    let grenze = (*jetzt - chrono::Duration::days(ZULETZT_TAGE)).timestamp();
    if let Some(t) = tabellen::lesen(VORGAENGE, txn)? {
        for eintrag in t.iter()? {
            let (k, w) = eintrag?;
            match json_lesen::<Vorgang>(w.value(), &format!("vorgaenge/{}", k.value())) {
                Ok(v) if zeit_wert(&v.geaendert) >= grenze => out.push(v),
                Ok(_) => {}
                Err(e) => warn!("vorgaenge: {} ({})", e, e.einzelheiten()),
            }
        }
    }
    out.sort_by(|a, b| zeit_wert(&b.geaendert).cmp(&zeit_wert(&a.geaendert)).then_with(|| b.id.cmp(&a.id)));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, TimeZone};

    fn zeit(tag: u32, stunde: u32) -> Zeit {
        FixedOffset::east_opt(3600).unwrap().with_ymd_and_hms(2026, 10, tag, stunde, 0, 0).unwrap()
    }

    fn neu(projekt: &str, art: MeldungsArt, was: &str) -> NeueMeldung {
        NeueMeldung {
            projekt: projekt.into(),
            art,
            schluessel: schluessel(projekt, was),
            titel: format!("Meldung {was}"),
            text: None,
            verweis: None,
            system: false,
            geschuetzt: false,
            von: "server".into(),
        }
    }

    /// Wiederholung: Zaehlen zählt hoch und macht wieder ungelesen; Einmal
    /// lässt alles; Zustand zählt erst nach „behoben“.
    #[test]
    fn wiederholung_zaehlt_hoch_statt_doppelt() {
        let s = Speicher::temporaer();
        let a = s.meldung_melden(neu("architektur", MeldungsArt::Fehler, "bau-rot"), Wiederholung::Zaehlen, &zeit(29, 10)).unwrap();
        let Melden::Neu(m) = a else { panic!("{a:?}") };
        assert_eq!((m.id.as_str(), m.anzahl, m.schluessel.as_str()), ("m-20261029-0001", 1, "architektur/bau-rot"));
        assert!(matches!(s.meldung_gelesen(&m.id, &zeit(29, 11)).unwrap(), Gelesen::Geaendert(_)));
        assert!(matches!(s.meldung_gelesen(&m.id, &zeit(29, 11)).unwrap(), Gelesen::Unveraendert(_)));
        let b = s.meldung_melden(neu("architektur", MeldungsArt::Fehler, "bau-rot"), Wiederholung::Zaehlen, &zeit(29, 12)).unwrap();
        let Melden::Hochgezaehlt(m2) = b else { panic!("{b:?}") };
        assert_eq!(
            (m2.id, m2.anzahl, m2.gelesen, m2.zuletzt.as_str(), m2.erstellt.as_str()),
            (m.id.clone(), 2, None, "2026-10-29T12:00:00+01:00", "2026-10-29T10:00:00+01:00")
        );
        assert_eq!(s.meldungen_lesen(None).unwrap().len(), 1, "eine Meldung, nicht zwei");
        // Einmal: bleibt.
        let f = s.meldung_melden(neu("dashboard", MeldungsArt::Fertig, "fertig:ar-1"), Wiederholung::Einmal, &zeit(29, 12)).unwrap();
        assert!(matches!(f, Melden::Neu(_)));
        let f2 = s.meldung_melden(neu("dashboard", MeldungsArt::Fertig, "fertig:ar-1"), Wiederholung::Einmal, &zeit(29, 13)).unwrap();
        assert!(matches!(&f2, Melden::Unveraendert(x) if x.anzahl == 1), "{f2:?}");
        // Zustand: bleibt, solange nicht behoben; danach zählt die Wiederholung.
        let p = neu("schaufenster", MeldungsArt::Fehler, "pruefung-rot");
        assert!(matches!(s.meldung_melden(p.clone(), Wiederholung::Zustand, &zeit(29, 10)).unwrap(), Melden::Neu(_)));
        assert!(matches!(s.meldung_melden(p.clone(), Wiederholung::Zustand, &zeit(29, 11)).unwrap(), Melden::Unveraendert(_)));
        let weg = s.meldungen_beheben(|m| m.schluessel == "schaufenster/pruefung-rot", &zeit(29, 12)).unwrap();
        assert_eq!(weg.len(), 1);
        assert!(s.meldungen_beheben(|m| m.schluessel == "schaufenster/pruefung-rot", &zeit(29, 12)).unwrap().is_empty(), "schon behoben");
        let wieder = s.meldung_melden(p, Wiederholung::Zustand, &zeit(29, 13)).unwrap();
        assert!(matches!(&wieder, Melden::Hochgezaehlt(x) if x.anzahl == 2 && x.behoben.is_none()), "{wieder:?}");
        assert!(matches!(s.meldung_gelesen("m-19990101-0001", &zeit(29, 13)), Err(SpeicherFehler::NichtGefunden(_))));
    }

    /// Grenze: höchstens 20 offene je Projekt; ältere werden als gelesen
    /// abgelegt und in einer Sammelmeldung gezählt; Fehler zuletzt.
    #[test]
    fn grenze_zwanzig_ungelesene_je_projekt() {
        let s = Speicher::temporaer();
        s.meldung_melden(neu("dashboard", MeldungsArt::Fehler, "alt-fehler"), Wiederholung::Zaehlen, &zeit(1, 1)).unwrap();
        for i in 0..25u32 {
            let t = FixedOffset::east_opt(3600).unwrap().with_ymd_and_hms(2026, 10, 2, 0, i, 0).unwrap();
            s.meldung_melden(neu("dashboard", MeldungsArt::Info, &format!("info-{i}")), Wiederholung::Zaehlen, &t).unwrap();
        }
        s.meldung_melden(neu("anderes", MeldungsArt::Info, "eins"), Wiederholung::Zaehlen, &zeit(3, 1)).unwrap();
        let alle = s.meldungen_lesen(Some("dashboard")).unwrap();
        let offene: Vec<&Meldung> = alle.iter().filter(|m| offen(m)).collect();
        assert_eq!(offene.len(), UNGELESEN_JE_PROJEKT, "{offene:?}");
        let sammel = alle.iter().find(|m| m.schluessel == "dashboard/aeltere-meldungen").expect("Sammelmeldung");
        assert_eq!((sammel.art, sammel.anzahl, offen(sammel)), (MeldungsArt::Info, 7, true));
        // Der alte Fehler bleibt offen; abgelegt sind die ältesten Infos.
        assert!(offene.iter().any(|m| m.schluessel == "dashboard/alt-fehler"));
        let abgelegt: Vec<&str> = alle.iter().filter(|m| m.zusammengefasst).map(|m| m.schluessel.as_str()).collect();
        assert_eq!(abgelegt.len(), 7);
        assert!(abgelegt.contains(&"dashboard/info-0") && !abgelegt.contains(&"dashboard/info-24"), "{abgelegt:?}");
        assert_eq!(alle.len(), 27, "nichts gelöscht: 26 Meldungen und die Sammelmeldung");
        assert_eq!(s.meldungen_lesen(Some("anderes")).unwrap().iter().filter(|m| offen(m)).count(), 1, "anderes Projekt unberührt");
    }

    /// Verfall nach sieben Tagen: gelesene und behobene ins Archiv (mit
    /// `archiviert`, sonst unverändert); offene Fehler bleiben, auch gelesen.
    #[test]
    fn archiv_nach_sieben_tagen() {
        let s = Speicher::temporaer();
        let gelesen = s.meldung_melden(neu("p", MeldungsArt::Fertig, "a"), Wiederholung::Zaehlen, &zeit(1, 10)).unwrap().meldung().clone();
        s.meldung_gelesen(&gelesen.id, &zeit(1, 12)).unwrap();
        let fehler = s.meldung_melden(neu("p", MeldungsArt::Fehler, "b"), Wiederholung::Zaehlen, &zeit(1, 10)).unwrap().meldung().clone();
        s.meldung_gelesen(&fehler.id, &zeit(1, 12)).unwrap();
        let behoben = s.meldung_melden(neu("p", MeldungsArt::Fehler, "c"), Wiederholung::Zaehlen, &zeit(1, 10)).unwrap().meldung().clone();
        s.meldungen_beheben(|m| m.id == behoben.id, &zeit(2, 10)).unwrap();
        let ungelesen = s.meldung_melden(neu("p", MeldungsArt::Info, "d"), Wiederholung::Zaehlen, &zeit(1, 10)).unwrap().meldung().clone();
        assert!(s.meldungen_archivieren(&zeit(8, 11)).unwrap().is_empty(), "vor sieben Tagen nichts");
        let weg = s.meldungen_archivieren(&zeit(8, 12)).unwrap();
        assert_eq!(weg.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), [gelesen.id.as_str()]);
        assert_eq!(weg[0].archiviert.as_deref(), Some("2026-10-08T12:00:00+01:00"));
        let weg = s.meldungen_archivieren(&zeit(9, 10)).unwrap();
        assert_eq!(weg.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), [behoben.id.as_str()]);
        let bleiben: Vec<String> = s.meldungen_lesen(None).unwrap().into_iter().map(|m| m.id).collect();
        assert_eq!(bleiben.len(), 2);
        assert!(bleiben.contains(&fehler.id) && bleiben.contains(&ungelesen.id));
        assert_eq!(s.meldungen_archiv_anzahl(), 2);
        assert!(s.meldung(&gelesen.id).unwrap().is_some(), "aus dem Archiv lesbar");
        assert!(matches!(s.meldung_gelesen(&gelesen.id, &zeit(9, 10)).unwrap(), Gelesen::Unveraendert(_)));
        let export: serde_json::Value = serde_json::from_str(&s.export().unwrap().als_json()).unwrap();
        assert_eq!(export["meldungen_archiv"].as_array().unwrap().len(), 2);
        assert_eq!(export["meldungen"].as_array().unwrap().len(), 2);
        // Dieselbe Sache kommt nach dem Archiv wieder: eine neue Meldung mit neuer Kennung.
        let neu_m = s.meldung_melden(neu("p", MeldungsArt::Fertig, "a"), Wiederholung::Zaehlen, &zeit(9, 11)).unwrap();
        assert!(matches!(&neu_m, Melden::Neu(m) if m.id != gelesen.id), "{neu_m:?}");
    }

    /// Keine Wiederkehr: Eine gelesene „Frist vorbei“, die weiter besteht,
    /// bleibt über acht Tage (Minutenlauf mit falscher Uhr: melden, dann
    /// archivieren) eine Meldung, gelesen, ×1. Erst behoben zieht sie nach
    /// sieben Tagen ins Archiv.
    #[test]
    fn gelesene_frist_kommt_nicht_wieder() {
        let s = Speicher::temporaer();
        let frist = neu("p", MeldungsArt::Frist, "frist-vorbei:arbeit-ar-1");
        let start = zeit(1, 8);
        let m = s.meldung_melden(frist.clone(), Wiederholung::Zustand, &start).unwrap().meldung().clone();
        s.meldung_gelesen(&m.id, &zeit(1, 9)).unwrap();
        // Acht Tage lang jede Stunde ein Lauf (Melden wie der Fristen-Lauf, dann das Archiv).
        for stunde in 0..(8 * 24 + 2) {
            let jetzt = start + chrono::Duration::hours(stunde);
            let r = s.meldung_melden(frist.clone(), Wiederholung::Zustand, &jetzt).unwrap();
            assert!(matches!(&r, Melden::Unveraendert(x) if x.id == m.id), "Stunde {stunde}: {r:?}");
            assert!(s.meldungen_archivieren(&jetzt).unwrap().is_empty(), "Stunde {stunde}: besteht noch, bleibt");
        }
        let alle = s.meldungen_lesen(None).unwrap();
        assert_eq!(alle.len(), 1);
        assert_eq!((alle[0].id.as_str(), alle[0].anzahl, alle[0].gelesen.is_some(), offen(&alle[0])), (m.id.as_str(), 1, true, false));
        assert_eq!(s.meldungen_archiv_anzahl(), 0);
        // Behoben (am 9.): nach sieben Tagen ins Archiv, vorher nicht.
        s.meldungen_beheben(|x| x.id == m.id, &zeit(9, 10)).unwrap();
        assert!(s.meldungen_archivieren(&zeit(16, 9)).unwrap().is_empty());
        assert_eq!(s.meldungen_archivieren(&zeit(16, 10)).unwrap().len(), 1);
        // Eine ungelesene Frist bleibt ohnehin; Einmal und Zählen gelesen ziehen wie bisher um.
        let f2 = s.meldung_melden(neu("p", MeldungsArt::Frist, "frist:frage-x"), Wiederholung::Zustand, &zeit(1, 8)).unwrap().meldung().clone();
        assert!(!zu_archivieren(&f2, &zeit(20, 8)));
    }
}
