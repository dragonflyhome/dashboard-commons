//! Vorgänge und ihr Journal (Zielbild, Kapitel c2): Tabellen `ereignisse`,
//! `vorgaenge`, `vorgaenge_je_slug` und `warter`.
//!
//! Jeder Schritt eines Vorgangs hängt ein Ereignis mit fortlaufender Nummer an
//! und schreibt den neuen Stand des Vorgangs, beides in **einer**
//! Schreibtransaktion: nach einem Absturz ist entweder beides da oder keines.
//! Zustände gehen nur vorwärts; ein Schritt, der nichts ändern würde, schreibt
//! nichts. Alle Methoden blockieren; der Server ruft sie über
//! `Speicher::im_hintergrund` auf.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use chrono::{Local, SecondsFormat};
use redb::{ReadTransaction, ReadableDatabase, ReadableTable, WriteTransaction};
use serde::de::DeserializeOwned;
use tracing::warn;

use super::tabellen::{self, EREIGNISSE, META, META_VORGANG_ZAEHLER, VORGAENGE, VORGAENGE_JE_SLUG, WARTER, Warter};
use super::{Speicher, SpeicherFehler};
use crate::api::vertrag::{BauStand, BauZustand, ChatZeile, Ereignis, MergeAntrag, Uebersicht, Vorgang, VorgangsArt, VorgangsListe, VorgangsZaehler, Zustand};

/// So viele Vorgänge zeigt die Übersicht (die zuletzt geänderten).
pub const UEBERSICHT_HOECHSTENS: usize = 100;

/// Ergebnis von `Speicher::zustellen`.
#[derive(Debug)]
pub struct Zustellung {
    /// Die Vorgänge, die dabei `zugestellt` wurden, je mit ihrem Ereignis.
    pub geaendert: Vec<(Vorgang, Ereignis)>,
    /// Höchste Ereignisnummer vor dem Schreiben.
    pub cursor_vorher: u64,
    /// Höchste Ereignisnummer danach (die zuletzt geschriebene).
    pub cursor: u64,
}

/// Jetzt in Ortszeit, RFC 3339 auf die Sekunde (`2026-10-26T09:14:00+01:00`).
pub(super) fn jetzt() -> String {
    Local::now().to_rfc3339_opts(SecondsFormat::Secs, false)
}

pub(super) fn json_lesen<T: DeserializeOwned>(wert: &[u8], was: &str) -> Result<T, SpeicherFehler> {
    serde_json::from_slice(wert).map_err(|e| SpeicherFehler::Beschaedigt(format!("{was}: {e}")))
}

/// Ein Vorgang aus `vorgaenge` (lesend oder schreibend geöffnet).
fn vorgang_aus<T: ReadableTable<&'static str, &'static [u8]>>(tabelle: &T, id: &str) -> Result<Option<Vorgang>, SpeicherFehler> {
    match tabelle.get(id)? {
        Some(wert) => Ok(Some(json_lesen(wert.value(), &format!("vorgaenge/{id}"))?)),
        None => Ok(None),
    }
}

/// Höchste Ereignisnummer; 0, wenn das Journal leer ist.
fn letzte_nr<T: ReadableTable<u64, &'static [u8]>>(tabelle: &T) -> Result<u64, SpeicherFehler> {
    Ok(tabelle.last()?.map(|(k, _)| k.value()).unwrap_or(0))
}

// ----- Schreiben (immer innerhalb einer Schreibtransaktion) -----

/// Neue Kennung `v-JJJJMMTT-NNNN` aus dem Tageszähler in `meta`. Sollte es die
/// Kennung schon geben (Zähler verloren), wird weitergezählt.
fn neue_kennung(txn: &WriteTransaction) -> Result<String, SpeicherFehler> {
    let tag = Local::now().format("%Y%m%d").to_string();
    let schluessel = format!("{META_VORGANG_ZAEHLER}{tag}");
    let mut meta = txn.open_table(META)?;
    let alt = meta.get(schluessel.as_str())?.map(|w| w.value().to_string());
    let mut n: u64 = match alt {
        Some(text) => text.parse().map_err(|_| SpeicherFehler::Beschaedigt(format!("{schluessel} = {text:?} ist keine Zahl")))?,
        None => 0,
    };
    let vorgaenge = txn.open_table(VORGAENGE)?;
    let id = loop {
        n += 1;
        let id = format!("v-{tag}-{n:04}");
        if vorgaenge.get(id.as_str())?.is_none() {
            break id;
        }
    };
    meta.insert(schluessel.as_str(), n.to_string().as_str())?;
    Ok(id)
}

/// Ein Schritt: Vorgang auf `zustand` setzen, Ereignis anhängen, beides schreiben.
fn schritt(txn: &WriteTransaction, v: &mut Vorgang, zustand: Zustand, von: &str, notiz: Option<&str>, zeit: &str) -> Result<Ereignis, SpeicherFehler> {
    schritt_mit_bau(txn, v, zustand, von, notiz, zeit, None)
}

/// Wie `schritt`; `bau` steht dann im Ereignis (ein Schritt des Baus).
fn schritt_mit_bau(
    txn: &WriteTransaction,
    v: &mut Vorgang,
    zustand: Zustand,
    von: &str,
    notiz: Option<&str>,
    zeit: &str,
    bau: Option<BauZustand>,
) -> Result<Ereignis, SpeicherFehler> {
    let mut ereignisse = txn.open_table(EREIGNISSE)?;
    let nr = letzte_nr(&ereignisse)? + 1;
    v.zustand = zustand;
    v.geaendert = zeit.to_string();
    v.letzte_nr = nr;
    if let Some(n) = notiz {
        v.notiz = Some(n.to_string());
    }
    let e = Ereignis {
        nr,
        zeit: zeit.to_string(),
        vorgang: v.id.clone(),
        art: v.art,
        slug: v.slug.clone(),
        zustand,
        titel: v.titel.clone(),
        bezug: v.bezug.clone(),
        von: von.to_string(),
        notiz: notiz.map(str::to_string),
        bau,
    };
    ereignisse.insert(nr, serde_json::to_vec(&e)?.as_slice())?;
    txn.open_table(VORGAENGE)?.insert(v.id.as_str(), serde_json::to_vec(v)?.as_slice())?;
    Ok(e)
}

/// Ändert den Eintrag eines Chats in `warter`.
fn warter_aendern(txn: &WriteTransaction, slug: &str, f: impl FnOnce(&mut Warter)) -> Result<(), SpeicherFehler> {
    let mut tabelle = txn.open_table(WARTER)?;
    let alt = tabelle.get(slug)?.map(|w| w.value().to_vec());
    let mut w: Warter = match alt {
        Some(bytes) => json_lesen(&bytes, &format!("warter/{slug}"))?,
        None => Warter::default(),
    };
    f(&mut w);
    tabelle.insert(slug, serde_json::to_vec(&w)?.as_slice())?;
    Ok(())
}

/// Ein Vorgang der Art `bau`, dessen Bau noch nicht fertig ist (nicht
/// angenommen oder läuft). Er gehört dem Baumeister des Dienstes: Das Warten
/// des Chats liefert ihn erst mit dem Ergebnis (grün oder rot), sonst bekäme
/// der Chat seine eigene Bestellung gleich zurück und das Ergebnis nie.
pub fn wartet_auf_bau(v: &Vorgang) -> bool {
    v.art == VorgangsArt::Bau && v.bau.as_ref().is_none_or(|b| b.zustand == BauZustand::Laeuft)
}

/// Kleinkram 2: Wer einen Bau-Vorgang mit dem Ergebnis erledigt (der
/// Baumeister des Dienstes, `baumeister::VON`).
pub const BAU_ERLEDIGT_VON: &str = "baumeister";

/// Kleinkram 2: Ergebnisse von Bau-Vorgängen des Chats `slug`, die der
/// Baumeister selbst erledigt hat und die sein Warten noch nicht geliefert hat
/// (Ereignisnummer über `warter.bau_bis`). Das Warten liefert sie einmal, wie
/// einen neuen Vorgang; danach nie wieder.
fn bau_ergebnisse_in(txn: &ReadTransaction, slug: &str) -> Result<Vec<Vorgang>, SpeicherFehler> {
    let bis = match tabellen::lesen(WARTER, txn)? {
        Some(t) => match t.get(slug)? {
            Some(w) => json_lesen::<Warter>(w.value(), &format!("warter/{slug}"))?.bau_bis.unwrap_or(0),
            None => 0,
        },
        None => 0,
    };
    let ereignisse = tabellen::lesen(EREIGNISSE, txn)?;
    let mut out = Vec::new();
    for v in vorgaenge_fuer_in(txn, slug, false)? {
        if v.art != VorgangsArt::Bau || v.zustand != Zustand::Erledigt || v.letzte_nr <= bis || wartet_auf_bau(&v) {
            continue;
        }
        // Nur, was der Baumeister erledigt hat (ein Chat, der selbst erledigt, kennt das Ergebnis schon).
        let vom_baumeister = match &ereignisse {
            Some(t) => match t.get(v.letzte_nr)? {
                Some(w) => json_lesen::<Ereignis>(w.value(), &format!("ereignisse/{}", v.letzte_nr))?.von == BAU_ERLEDIGT_VON,
                None => false,
            },
            None => false,
        };
        if vom_baumeister {
            out.push(v);
        }
    }
    Ok(out)
}

// ----- Lesen (innerhalb einer Lesetransaktion) -----

fn cursor_in(txn: &ReadTransaction) -> Result<u64, SpeicherFehler> {
    match tabellen::lesen(EREIGNISSE, txn)? {
        Some(t) => letzte_nr(&t),
        None => Ok(0),
    }
}

/// Vorgänge eines Chats über das Verzeichnis, nach Kennung sortiert.
/// `nur_offen`: nur die, die noch nicht übernommen sind.
fn vorgaenge_fuer_in(txn: &ReadTransaction, slug: &str, nur_offen: bool) -> Result<Vec<Vorgang>, SpeicherFehler> {
    let (Some(index), Some(vorgaenge)) = (tabellen::lesen(VORGAENGE_JE_SLUG, txn)?, tabellen::lesen(VORGAENGE, txn)?) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for eintrag in index.range::<(&str, &str)>((slug, "")..)? {
        let (k, _) = eintrag?;
        let (s, id) = k.value();
        if s != slug {
            break;
        }
        match vorgang_aus(&vorgaenge, id) {
            Ok(Some(v)) if !nur_offen || (v.zustand < Zustand::Uebernommen && !wartet_auf_bau(&v)) => out.push(v),
            Ok(_) => {}
            Err(e) => warn!("vorgaenge: {} ({})", e, e.einzelheiten()),
        }
    }
    Ok(out)
}

/// Ereignisse nach Nummer `nr` (ausschließlich), wahlweise nur für einen Chat.
fn ereignisse_seit_in(txn: &ReadTransaction, slug: Option<&str>, nr: u64) -> Result<Vec<Ereignis>, SpeicherFehler> {
    let Some(tabelle) = tabellen::lesen(EREIGNISSE, txn)? else { return Ok(Vec::new()) };
    let Some(ab) = nr.checked_add(1) else { return Ok(Vec::new()) };
    let mut out = Vec::new();
    for eintrag in tabelle.range(ab..)? {
        let (k, w) = eintrag?;
        match json_lesen::<Ereignis>(w.value(), &format!("ereignisse/{}", k.value())) {
            Ok(e) if slug.is_none_or(|s| s == e.slug) => out.push(e),
            Ok(_) => {}
            Err(e) => warn!("ereignisse: {} ({})", e, e.einzelheiten()),
        }
    }
    Ok(out)
}

/// Legt in einer offenen Schreibtransaktion einen Vorgang im Zustand `offen`
/// an, mit seinem ersten Ereignis und dem Eintrag im Verzeichnis. Auch für
/// andere Teile des Speichers, die im selben Schritt einen Vorgang brauchen
/// (der Haken einer Aufgabe).
pub(super) fn vorgang_anlegen_in(
    txn: &WriteTransaction,
    art: VorgangsArt,
    slug: &str,
    titel: &str,
    bezug: Option<&str>,
    von: &str,
) -> Result<(Vorgang, Ereignis), SpeicherFehler> {
    vorgang_anlegen_mit_notiz_in(txn, art, slug, titel, bezug, von, None)
}

/// Wie `vorgang_anlegen_in`, mit einer Notiz am ersten Ereignis (und am
/// Vorgang): etwa die eigenen Worte des Inhabers bei „Etwas anderes …“ zu einer
/// Aufgabe, damit der Chat sie mit dem Vorgang bekommt.
pub(super) fn vorgang_anlegen_mit_notiz_in(
    txn: &WriteTransaction,
    art: VorgangsArt,
    slug: &str,
    titel: &str,
    bezug: Option<&str>,
    von: &str,
    notiz: Option<&str>,
) -> Result<(Vorgang, Ereignis), SpeicherFehler> {
    let id = neue_kennung(txn)?;
    let zeit = jetzt();
    let mut v = Vorgang {
        id,
        art,
        slug: slug.to_string(),
        titel: titel.to_string(),
        bezug: bezug.map(str::to_string),
        zustand: Zustand::Offen,
        erstellt: zeit.clone(),
        geaendert: zeit.clone(),
        letzte_nr: 0,
        von: von.to_string(),
        notiz: None,
        bau: None,
        merge: None,
    };
    let e = schritt(txn, &mut v, Zustand::Offen, von, notiz, &zeit)?;
    txn.open_table(VORGAENGE_JE_SLUG)?.insert((v.slug.as_str(), v.id.as_str()), ())?;
    Ok((v, e))
}

/// `Speicher::vorgang_weiter` in einer offenen Schreibtransaktion (auch für
/// andere Teile des Speichers, etwa die Passkey-Anträge, Etappe 7c).
/// `Ok(None)`: schon so weit, nichts geschrieben.
pub(super) fn vorgang_weiter_in(
    txn: &WriteTransaction,
    id: &str,
    neu: Zustand,
    von: &str,
    notiz: Option<&str>,
) -> Result<Option<(Vorgang, Ereignis)>, SpeicherFehler> {
    let alt = vorgang_aus(&txn.open_table(VORGAENGE)?, id)?;
    let Some(mut v) = alt else {
        return Err(SpeicherFehler::NichtGefunden(id.to_string()));
    };
    if v.zustand >= neu {
        return Ok(None);
    }
    let zeit = jetzt();
    let e = schritt(txn, &mut v, neu, von, notiz, &zeit)?;
    if neu == Zustand::Uebernommen {
        warter_aendern(txn, &v.slug, |w| w.zuletzt_uebernommen = Some(zeit.clone()))?;
    }
    Ok(Some((v, e)))
}

impl Speicher {
    /// Legt einen Vorgang im Zustand `offen` an, mit seinem ersten Ereignis
    /// und dem Eintrag im Verzeichnis, alles in einer Transaktion.
    pub fn vorgang_anlegen(&self, art: VorgangsArt, slug: &str, titel: &str, bezug: Option<&str>, von: &str) -> Result<(Vorgang, Ereignis), SpeicherFehler> {
        let txn = self.db.begin_write()?;
        let (v, e) = vorgang_anlegen_in(&txn, art, slug, titel, bezug, von)?;
        self.festschreiben(txn, &[super::Bereich::Vorgaenge])?;
        Ok((v, e))
    }

    /// Legt einen Merge-Request an (Art `merge`, Etappe 4b): Vorgang im
    /// Zustand `offen` mit den Angaben in `merge`, alles in einer Transaktion.
    pub fn merge_anlegen(&self, slug: &str, titel: &str, bezug: Option<&str>, von: &str, antrag: MergeAntrag) -> Result<(Vorgang, Ereignis), SpeicherFehler> {
        let txn = self.db.begin_write()?;
        let (mut v, e) = vorgang_anlegen_in(&txn, VorgangsArt::Merge, slug, titel, bezug, von)?;
        v.merge = Some(antrag);
        txn.open_table(VORGAENGE)?.insert(v.id.as_str(), serde_json::to_vec(&v)?.as_slice())?;
        self.festschreiben(txn, &[super::Bereich::Vorgaenge])?;
        Ok((v, e))
    }

    /// Eine Öffnung im Journal (Entscheidung lokal-oeffnen): ein
    /// Vorgang der Art `oeffnen` für das Projekt, gleich `erledigt`, mit genau
    /// einem Ereignis (Zeit, Projekt, Kennung im Bezug `ort:<projekt>/<kennung>`,
    /// nie der Pfad). Weil er nie offen war, liefert ihn kein Warten eines Chats.
    pub fn oeffnung_vermerken(&self, projekt: &str, kennung: &str) -> Result<(Vorgang, Ereignis), SpeicherFehler> {
        let txn = self.db.begin_write()?;
        let zeit = jetzt();
        let mut v = Vorgang {
            id: neue_kennung(&txn)?,
            art: VorgangsArt::Oeffnen,
            slug: projekt.to_string(),
            titel: format!("Geöffnet: {kennung}"),
            bezug: Some(format!("ort:{projekt}/{kennung}")),
            zustand: Zustand::Erledigt,
            erstellt: zeit.clone(),
            geaendert: zeit.clone(),
            letzte_nr: 0,
            von: "browser".to_string(),
            notiz: None,
            bau: None,
            merge: None,
        };
        let e = schritt(&txn, &mut v, Zustand::Erledigt, "browser", None, &zeit)?;
        txn.open_table(VORGAENGE_JE_SLUG)?.insert((v.slug.as_str(), v.id.as_str()), ())?;
        self.festschreiben(txn, &[super::Bereich::Vorgaenge])?;
        Ok((v, e))
    }

    /// Bringt einen Vorgang vorwärts. Ist er schon so weit oder weiter, bleibt
    /// alles, wie es ist (`Ok(None)`, nichts geschrieben). Unbekannte Kennung:
    /// `SpeicherFehler::NichtGefunden`. Bei `uebernommen` merkt sich `warter`
    /// die Zeit für den Chat.
    pub fn vorgang_weiter(&self, id: &str, neu: Zustand, von: &str, notiz: Option<&str>) -> Result<Option<(Vorgang, Ereignis)>, SpeicherFehler> {
        let txn = self.db.begin_write()?;
        let Some(fertig) = vorgang_weiter_in(&txn, id, neu, von, notiz)? else {
            // Ohne Commit: die Transaktion wird verworfen, die Datei bleibt unberührt.
            return Ok(None);
        };
        self.festschreiben(txn, &[super::Bereich::Vorgaenge])?;
        Ok(Some(fertig))
    }

    /// Das Warten hat diese Vorgänge geliefert: alle, die noch `offen` sind,
    /// werden `zugestellt` (je mit Ereignis), und `warter` merkt sich die Zeit
    /// je Chat. Eine Transaktion. Gibt die geänderten Vorgänge zurück und den
    /// Cursor vor und nach dem Schreiben.
    pub fn zustellen(&self, ids: &[String], von: &str) -> Result<Zustellung, SpeicherFehler> {
        let txn = self.db.begin_write()?;
        let zeit = jetzt();
        let cursor_vorher = letzte_nr(&txn.open_table(EREIGNISSE)?)?;
        let mut z = Zustellung { geaendert: Vec::new(), cursor_vorher, cursor: cursor_vorher };
        let geaendert = &mut z.geaendert;
        let mut slugs = HashSet::new();
        // Kleinkram 2: geliefertes Ergebnis eines erledigten Bau-Vorgangs, je Chat die höchste Nummer.
        let mut bau_bis: BTreeMap<String, u64> = BTreeMap::new();
        for id in ids {
            let alt = vorgang_aus(&txn.open_table(VORGAENGE)?, id)?;
            let Some(mut v) = alt else { continue };
            if v.art == VorgangsArt::Bau && v.zustand == Zustand::Erledigt {
                let n = bau_bis.entry(v.slug.clone()).or_default();
                *n = (*n).max(v.letzte_nr);
                slugs.insert(v.slug.clone());
                continue;
            }
            if v.zustand >= Zustand::Uebernommen {
                continue;
            }
            slugs.insert(v.slug.clone());
            if v.zustand == Zustand::Offen {
                let e = schritt(&txn, &mut v, Zustand::Zugestellt, von, None, &zeit)?;
                geaendert.push((v, e));
            }
        }
        if slugs.is_empty() {
            return Ok(z);
        }
        for slug in &slugs {
            let bis = bau_bis.get(slug).copied();
            warter_aendern(&txn, slug, |w| {
                w.zuletzt_geweckt = Some(zeit.clone());
                if let Some(b) = bis {
                    w.bau_bis = Some(w.bau_bis.unwrap_or(0).max(b));
                }
            })?;
        }
        self.festschreiben(txn, &[super::Bereich::Vorgaenge])?;
        z.cursor = z.geaendert.iter().map(|(_, e)| e.nr).max().unwrap_or(cursor_vorher);
        Ok(z)
    }

    /// Alle Vorgänge der Art `bau`, die auf einen Bau warten (`wartet_auf_bau`)
    /// und noch nicht übernommen sind, nach Kennung (also nach Alter).
    pub fn offene_bauten(&self) -> Result<Vec<Vorgang>, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        let Some(tabelle) = tabellen::lesen(VORGAENGE, &txn)? else { return Ok(Vec::new()) };
        let mut out = Vec::new();
        for eintrag in tabelle.iter()? {
            let (k, w) = eintrag?;
            match json_lesen::<Vorgang>(w.value(), &format!("vorgaenge/{}", k.value())) {
                Ok(v) if v.zustand < Zustand::Uebernommen && wartet_auf_bau(&v) => out.push(v),
                Ok(_) => {}
                Err(e) => warn!("vorgaenge: {} ({})", e, e.einzelheiten()),
            }
        }
        Ok(out)
    }

    /// Kleinkram 2: die Ergebnisse erledigter Bau-Vorgänge, die das Warten
    /// des Chats `slug` noch nicht geliefert hat (`bau_ergebnisse_in`).
    pub fn bau_ergebnisse(&self, slug: &str) -> Result<Vec<Vorgang>, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        bau_ergebnisse_in(&txn, slug)
    }

    /// Setzt den Bau mehrerer Vorgänge (Art `bau`, noch nicht übernommen) auf
    /// `stand`, je mit einem Ereignis (Notiz = `notiz`), alles in einer
    /// Transaktion. Bei `laeuft` bleibt der Zustand des Vorgangs; mit dem
    /// Ergebnis (grün oder rot) wird er `erledigt` (Kleinkram 2). Vorgänge, die es nicht gibt, die
    /// nicht `bau` sind, schon übernommen sind oder genau diesen Stand haben,
    /// bleiben unberührt. Gibt die geänderten zurück.
    pub fn bau_setzen(&self, ids: &[String], stand: &BauStand, von: &str, notiz: &str) -> Result<Vec<(Vorgang, Ereignis)>, SpeicherFehler> {
        let txn = self.db.begin_write()?;
        let zeit = jetzt();
        let mut geaendert = Vec::new();
        for id in ids {
            let alt = vorgang_aus(&txn.open_table(VORGAENGE)?, id)?;
            let Some(mut v) = alt else { continue };
            if v.art != VorgangsArt::Bau || v.zustand >= Zustand::Uebernommen || v.bau.as_ref() == Some(stand) {
                continue;
            }
            v.bau = Some(stand.clone());
            // Kleinkram 2: Mit dem Ergebnis (grün oder rot) ist der Vorgang erledigt;
            // der Chat bekommt es trotzdem einmal über sein Warten (`bau_ergebnisse`).
            let zustand = if stand.zustand == BauZustand::Laeuft { v.zustand } else { Zustand::Erledigt };
            let e = schritt_mit_bau(&txn, &mut v, zustand, von, Some(notiz), &zeit, Some(stand.zustand))?;
            geaendert.push((v, e));
        }
        if geaendert.is_empty() {
            return Ok(geaendert);
        }
        self.festschreiben(txn, &[super::Bereich::Vorgaenge])?;
        Ok(geaendert)
    }

    /// Vorgänge eines Chats, nach Kennung sortiert; `nur_nicht_uebernommen`
    /// lässt die übernommenen und erledigten weg.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn vorgaenge_fuer(&self, slug: &str, nur_nicht_uebernommen: bool) -> Result<Vec<Vorgang>, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        vorgaenge_fuer_in(&txn, slug, nur_nicht_uebernommen)
    }

    /// Alle Vorgänge in ihrem jetzigen Stand, ohne zugesagte Reihenfolge (für
    /// den Verlauf, Etappe 2e). Unlesbare stehen im Protokoll.
    pub fn vorgaenge_alle(&self) -> Result<Vec<Vorgang>, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        let mut alle = Vec::new();
        if let Some(tabelle) = tabellen::lesen(VORGAENGE, &txn)? {
            for eintrag in tabelle.iter()? {
                let (k, w) = eintrag?;
                match json_lesen::<Vorgang>(w.value(), &format!("vorgaenge/{}", k.value())) {
                    Ok(v) => alle.push(v),
                    Err(e) => warn!("vorgaenge: {} ({})", e, e.einzelheiten()),
                }
            }
        }
        Ok(alle)
    }

    /// Ein Vorgang ohne seine Ereignisse.
    pub fn vorgang_kopf(&self, id: &str) -> Result<Option<Vorgang>, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        match tabellen::lesen(VORGAENGE, &txn)? {
            Some(t) => vorgang_aus(&t, id),
            None => Ok(None),
        }
    }

    /// Ein Vorgang mit allen seinen Ereignissen, nach Nummer.
    pub fn vorgang(&self, id: &str) -> Result<Option<(Vorgang, Vec<Ereignis>)>, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        let Some(vorgaenge) = tabellen::lesen(VORGAENGE, &txn)? else { return Ok(None) };
        let Some(v) = vorgang_aus(&vorgaenge, id)? else { return Ok(None) };
        let Some(tabelle) = tabellen::lesen(EREIGNISSE, &txn)? else { return Ok(Some((v, Vec::new()))) };
        let mut ereignisse = Vec::new();
        // Spätere Ereignisse als `letzte_nr` gehören nicht zu ihm.
        for eintrag in tabelle.range(..=v.letzte_nr)? {
            let (k, w) = eintrag?;
            match json_lesen::<Ereignis>(w.value(), &format!("ereignisse/{}", k.value())) {
                Ok(e) if e.vorgang == id => ereignisse.push(e),
                Ok(_) => {}
                Err(e) => warn!("ereignisse: {} ({})", e, e.einzelheiten()),
            }
        }
        Ok(Some((v, ereignisse)))
    }

    /// Ereignisse nach Nummer `nr` (ausschließlich), wahlweise nur für einen Chat.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn ereignisse_seit(&self, slug: Option<&str>, nr: u64) -> Result<Vec<Ereignis>, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        ereignisse_seit_in(&txn, slug, nr)
    }

    /// Höchste Ereignisnummer (0 bei leerem Journal).
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn cursor(&self) -> Result<u64, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        cursor_in(&txn)
    }

    /// Antwort für `GET /api/vorgaenge?fuer=…` aus einer Lesetransaktion, damit
    /// `cursor` genau zum Gelesenen passt. Mit `seit`: die Ereignisse danach und
    /// die betroffenen Vorgänge (jeder einmal, in der Reihenfolge des ersten
    /// Ereignisses). Sonst die Vorgänge des Chats, mit `nur_offen` nur die nicht übernommenen.
    pub fn vorgangs_liste(&self, slug: &str, seit: Option<u64>, nur_offen: bool) -> Result<VorgangsListe, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        let cursor = cursor_in(&txn)?;
        let Some(nr) = seit else {
            let vorgaenge = vorgaenge_fuer_in(&txn, slug, nur_offen)?;
            return Ok(VorgangsListe { vorgaenge, ereignisse: Vec::new(), cursor });
        };
        let ereignisse = ereignisse_seit_in(&txn, Some(slug), nr)?;
        let mut vorgaenge = Vec::new();
        if let Some(tabelle) = tabellen::lesen(VORGAENGE, &txn)? {
            let mut gesehen = HashSet::new();
            for e in &ereignisse {
                if gesehen.insert(e.vorgang.as_str())
                    && let Some(v) = vorgang_aus(&tabelle, &e.vorgang)?
                {
                    vorgaenge.push(v);
                }
            }
        }
        Ok(VorgangsListe { vorgaenge, ereignisse, cursor })
    }

    /// Alles für die Seite /vorgaenge. `wartet_seit` bleibt leer; das weiß nur
    /// der Warteraum im Server.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn uebersicht(&self) -> Result<Uebersicht, SpeicherFehler> {
        self.uebersicht_ohne(&BTreeSet::new())
    }

    /// Wie [`Self::uebersicht`], ohne die Projekte in `ohne` (Entscheidung
    /// archiv-status): ihre Vorgänge und Merge-Anträge für sie
    /// ([`ist_still`]), ihre Zeilen und ihre Zähler fehlen.
    pub fn uebersicht_ohne(&self, ohne: &BTreeSet<String>) -> Result<Uebersicht, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        let cursor = cursor_in(&txn)?;
        let mut zaehler = VorgangsZaehler::default();
        let mut chats: BTreeMap<String, ChatZeile> = BTreeMap::new();
        let zeile = |slug: &str| ChatZeile { slug: slug.to_string(), wartet_seit: None, zuletzt_uebernommen: None, offen: 0 };
        let mut alle = Vec::new();
        if let Some(tabelle) = tabellen::lesen(VORGAENGE, &txn)? {
            for eintrag in tabelle.iter()? {
                let (k, w) = eintrag?;
                let v: Vorgang = match json_lesen(w.value(), &format!("vorgaenge/{}", k.value())) {
                    Ok(v) => v,
                    Err(e) => {
                        warn!("vorgaenge: {} ({})", e, e.einzelheiten());
                        continue;
                    }
                };
                if ist_still(&v, ohne) {
                    continue;
                }
                match v.zustand {
                    Zustand::Offen => zaehler.offen += 1,
                    Zustand::Zugestellt => zaehler.zugestellt += 1,
                    _ => {}
                }
                let z = chats.entry(v.slug.clone()).or_insert_with(|| zeile(&v.slug));
                if v.zustand < Zustand::Uebernommen {
                    z.offen += 1;
                }
                alle.push(v);
            }
        }
        if let Some(tabelle) = tabellen::lesen(WARTER, &txn)? {
            for eintrag in tabelle.iter()? {
                let (k, w) = eintrag?;
                let slug = k.value();
                if ohne.contains(slug) {
                    continue;
                }
                match json_lesen::<Warter>(w.value(), &format!("warter/{slug}")) {
                    Ok(w) => {
                        let z = chats.entry(slug.to_string()).or_insert_with(|| zeile(slug));
                        z.zuletzt_uebernommen = w.zuletzt_uebernommen;
                    }
                    Err(e) => warn!("warter: {} ({})", e, e.einzelheiten()),
                }
            }
        }
        // Heute: vom Ende des Journals rückwärts, bis zum ersten Ereignis von gestern.
        if let Some(tabelle) = tabellen::lesen(EREIGNISSE, &txn)? {
            let heute = Local::now().format("%Y-%m-%d").to_string();
            for eintrag in tabelle.iter()?.rev() {
                let (k, w) = eintrag?;
                let e: Ereignis = match json_lesen(w.value(), &format!("ereignisse/{}", k.value())) {
                    Ok(e) => e,
                    Err(e) => {
                        warn!("ereignisse: {} ({})", e, e.einzelheiten());
                        continue;
                    }
                };
                if e.zeit.get(..10) != Some(heute.as_str()) {
                    break;
                }
                if ohne.contains(&e.slug) {
                    continue;
                }
                match e.zustand {
                    Zustand::Uebernommen => zaehler.uebernommen_heute += 1,
                    Zustand::Erledigt => zaehler.erledigt_heute += 1,
                    _ => {}
                }
            }
        }
        alle.sort_by_key(|v| std::cmp::Reverse(v.letzte_nr));
        // Etappe 4b: offene Merge-Requests vollständig, auch jenseits der 100.
        let merge_offen: Vec<Vorgang> = alle.iter().filter(|v| v.art == VorgangsArt::Merge && v.zustand < Zustand::Erledigt).cloned().collect();
        alle.truncate(UEBERSICHT_HOECHSTENS);
        Ok(Uebersicht { zaehler, chats: chats.into_values().collect(), vorgaenge: alle, merge_offen, cursor })
    }
}

/// Gehört der Vorgang zu einem Projekt in `ohne` (Entscheidung
/// archiv-status)? Sein Chat ist es, oder er ist ein Merge-Antrag
/// für dessen Modul.
pub fn ist_still(v: &Vorgang, ohne: &BTreeSet<String>) -> bool {
    ohne.contains(&v.slug) || v.merge.as_ref().is_some_and(|m| ohne.contains(&m.projekt))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use redb::ReadableTableMetadata;

    fn anlegen(s: &Speicher, slug: &str, titel: &str) -> Vorgang {
        s.vorgang_anlegen(VorgangsArt::Abnahme, slug, titel, Some("abnahme:2b"), "chat").unwrap().0
    }

    fn zahl_ereignisse(s: &Speicher) -> u64 {
        let txn = s.db.begin_read().unwrap();
        tabellen::zaehlen(EREIGNISSE, &txn).unwrap()
    }

    #[test]
    fn anlegen_vergibt_kennung_nummer_und_verzeichnis() {
        let s = Speicher::temporaer();
        assert_eq!(s.cursor().unwrap(), 0);
        let (a, ea) = s.vorgang_anlegen(VorgangsArt::Antwort, "beispiel", "Schlusssatz", None, "browser").unwrap();
        let b = anlegen(&s, "dashboard", "Etappe 2b");
        let tag = Local::now().format("%Y%m%d").to_string();
        assert_eq!(a.id, format!("v-{tag}-0001"));
        assert_eq!(b.id, format!("v-{tag}-0002"));
        assert_eq!((a.zustand, a.letzte_nr, ea.nr, ea.zustand), (Zustand::Offen, 1, 1, Zustand::Offen));
        assert_eq!((ea.vorgang.as_str(), ea.von.as_str(), ea.slug.as_str()), (a.id.as_str(), "browser", "beispiel"));
        assert_eq!(b.letzte_nr, 2);
        assert_eq!(s.cursor().unwrap(), 2);
        assert_eq!(s.meta_lesen(&format!("vorgang.zaehler.{tag}")).unwrap().as_deref(), Some("2"));
        assert_eq!(s.vorgaenge_fuer("beispiel", true).unwrap(), vec![a.clone()]);
        assert_eq!(s.vorgaenge_fuer("dashboard", false).unwrap(), vec![b]);
        assert!(s.vorgaenge_fuer("bewerb", false).unwrap().is_empty(), "nur ganze Slugs");
        let (v, ereignisse) = s.vorgang(&a.id).unwrap().unwrap();
        assert_eq!((v, ereignisse), (a, vec![ea]));
        assert!(s.vorgang("v-19990101-0001").unwrap().is_none());
    }

    #[test]
    fn doppeltes_uebernommen_aendert_nichts() {
        let s = Speicher::temporaer();
        let v = anlegen(&s, "dashboard", "x");
        let (nach, e) = s.vorgang_weiter(&v.id, Zustand::Uebernommen, "chat:dashboard", None).unwrap().unwrap();
        assert_eq!((nach.zustand, nach.letzte_nr, e.nr), (Zustand::Uebernommen, 2, 2));
        let groesse = s.groesse_bytes();
        assert!(s.vorgang_weiter(&v.id, Zustand::Uebernommen, "chat:dashboard", None).unwrap().is_none());
        assert_eq!(s.cursor().unwrap(), 2, "keine neue Nummer");
        assert_eq!(zahl_ereignisse(&s), 2);
        assert_eq!(s.vorgang_kopf(&v.id).unwrap().unwrap(), nach);
        assert_eq!(s.groesse_bytes(), groesse);
        // Auch `zugestellt` danach ist rückwärts.
        assert!(s.zustellen(std::slice::from_ref(&v.id), "server").unwrap().geaendert.is_empty());
        assert_eq!(s.cursor().unwrap(), 2);
    }

    #[test]
    fn rueckwaerts_aendert_nichts_und_vorwaerts_darf_springen() {
        let s = Speicher::temporaer();
        let v = anlegen(&s, "dashboard", "x");
        let (nach, _) = s.vorgang_weiter(&v.id, Zustand::Erledigt, "chat:dashboard", Some("fertig")).unwrap().unwrap();
        assert_eq!((nach.zustand, nach.notiz.as_deref()), (Zustand::Erledigt, Some("fertig")));
        for z in [Zustand::Offen, Zustand::Zugestellt, Zustand::Uebernommen, Zustand::Erledigt] {
            assert!(s.vorgang_weiter(&v.id, z, "chat:dashboard", Some("nochmal")).unwrap().is_none(), "{z:?}");
        }
        assert_eq!(s.vorgang_kopf(&v.id).unwrap().unwrap(), nach);
        assert_eq!(s.cursor().unwrap(), 2);
        // Kein `uebernommen` geschehen: der Warter-Eintrag fehlt.
        assert!(s.uebersicht().unwrap().chats[0].zuletzt_uebernommen.is_none());
    }

    #[test]
    fn unbekannte_kennung_ist_ein_eigener_fehler() {
        let s = Speicher::temporaer();
        let e = s.vorgang_weiter("v-20261026-0007", Zustand::Uebernommen, "chat", None).unwrap_err();
        assert!(matches!(&e, SpeicherFehler::NichtGefunden(id) if id == "v-20261026-0007"), "{e:?}");
        assert_eq!(e.to_string(), "Diesen Vorgang gibt es nicht.");
        assert_eq!(s.cursor().unwrap(), 0);
    }

    #[test]
    fn seit_liefert_genau_die_fehlenden() {
        let s = Speicher::temporaer();
        let a = anlegen(&s, "dashboard", "a"); // 1
        let b = anlegen(&s, "architektur", "b"); // 2
        s.vorgang_weiter(&a.id, Zustand::Uebernommen, "chat:dashboard", None).unwrap(); // 3
        let c = anlegen(&s, "dashboard", "c"); // 4
        s.vorgang_weiter(&b.id, Zustand::Erledigt, "chat:architektur", None).unwrap(); // 5
        let nummern = |slug: Option<&str>, nr| -> Vec<u64> { s.ereignisse_seit(slug, nr).unwrap().iter().map(|e| e.nr).collect() };
        assert_eq!(nummern(None, 0), [1, 2, 3, 4, 5]);
        assert_eq!(nummern(None, 3), [4, 5]);
        assert_eq!(nummern(Some("dashboard"), 0), [1, 3, 4]);
        assert_eq!(nummern(Some("dashboard"), 1), [3, 4]);
        assert_eq!(nummern(Some("dashboard"), 4), Vec::<u64>::new());
        assert_eq!(nummern(None, u64::MAX), Vec::<u64>::new());
        // Die Liste dazu: jeder betroffene Vorgang einmal, im jetzigen Stand.
        let liste = s.vorgangs_liste("dashboard", Some(0), false).unwrap();
        assert_eq!(liste.cursor, 5);
        let ids: Vec<&str> = liste.vorgaenge.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(ids, [a.id.as_str(), c.id.as_str()]);
        assert_eq!(liste.vorgaenge[0].zustand, Zustand::Uebernommen);
        let offen = s.vorgangs_liste("dashboard", None, true).unwrap();
        assert_eq!(offen.vorgaenge, vec![c]);
        assert!(offen.ereignisse.is_empty());
    }

    #[test]
    fn zustellen_nur_offene_und_merkt_die_zeit() {
        let s = Speicher::temporaer();
        let a = anlegen(&s, "dashboard", "a");
        let b = anlegen(&s, "dashboard", "b");
        s.vorgang_weiter(&b.id, Zustand::Uebernommen, "chat:dashboard", None).unwrap();
        let ids = vec![a.id.clone(), b.id.clone(), "v-19990101-0001".to_string()];
        let z = s.zustellen(&ids, "server").unwrap();
        assert_eq!((z.cursor_vorher, z.cursor), (3, 4), "Cursor vor und nach dem Schreiben");
        let geaendert = z.geaendert;
        assert_eq!(geaendert.len(), 1);
        let (v, e) = &geaendert[0];
        assert_eq!((v.id.as_str(), v.zustand, e.von.as_str(), e.nr), (a.id.as_str(), Zustand::Zugestellt, "server", 4));
        // Ein zweites Zustellen ändert nichts mehr; „zugestellt“ bleibt in der Liste der Offenen.
        let z = s.zustellen(&ids, "server").unwrap();
        assert!(z.geaendert.is_empty());
        assert_eq!((z.cursor_vorher, z.cursor), (4, 4), "nichts geschrieben");
        assert_eq!(s.cursor().unwrap(), 4);
        assert_eq!(s.vorgaenge_fuer("dashboard", true).unwrap().len(), 1);
        let txn = s.db.begin_read().unwrap();
        let tabelle = txn.open_table(WARTER).unwrap();
        let w: Warter = serde_json::from_slice(tabelle.get("dashboard").unwrap().unwrap().value()).unwrap();
        assert!(w.zuletzt_geweckt.is_some() && w.zuletzt_uebernommen.is_some(), "{w:?}");
    }

    #[test]
    fn uebersicht_zaehlt_und_sortiert() {
        let s = Speicher::temporaer();
        let a = anlegen(&s, "dashboard", "a");
        let b = anlegen(&s, "architektur", "b");
        let c = anlegen(&s, "dashboard", "c");
        s.zustellen(std::slice::from_ref(&c.id), "server").unwrap();
        s.vorgang_weiter(&b.id, Zustand::Uebernommen, "chat:architektur", None).unwrap();
        s.vorgang_weiter(&b.id, Zustand::Erledigt, "chat:architektur", None).unwrap();
        let u = s.uebersicht().unwrap();
        let erwartet = VorgangsZaehler { offen: 1, zugestellt: 1, uebernommen_heute: 1, erledigt_heute: 1 };
        assert_eq!(u.zaehler, erwartet);
        assert_eq!(u.cursor, 6);
        let slugs: Vec<(&str, u64)> = u.chats.iter().map(|z| (z.slug.as_str(), z.offen)).collect();
        assert_eq!(slugs, [("architektur", 0), ("dashboard", 2)]);
        assert!(u.chats[0].zuletzt_uebernommen.is_some() && u.chats[1].zuletzt_uebernommen.is_none());
        assert!(u.chats.iter().all(|z| z.wartet_seit.is_none()));
        let ids: Vec<&str> = u.vorgaenge.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(ids, [b.id.as_str(), c.id.as_str(), a.id.as_str()], "zuletzt geändert zuerst");
    }

    #[test]
    fn export_und_sicherung_enthalten_die_vorgaenge() {
        let s = Speicher::temporaer();
        let v = anlegen(&s, "dashboard", "Etappe „2b“");
        s.vorgang_weiter(&v.id, Zustand::Uebernommen, "chat:dashboard", None).unwrap();
        let e = s.export().unwrap();
        assert_eq!(e.tabellen["ereignisse"].as_array().unwrap().len(), 2);
        assert_eq!(e.tabellen["vorgaenge"][0]["zustand"], "uebernommen");
        assert_eq!(e.tabellen["vorgaenge_je_slug"][0], serde_json::json!(["dashboard", v.id]));
        assert!(e.tabellen["warter"]["dashboard"]["zuletzt_uebernommen"].is_string());
        let toml = e.als_toml();
        assert!(toml.contains("[[ereignisse]]") && toml.contains("[warter.dashboard]"), "{toml}");
        let kopie = s.sichern().unwrap();
        let db = redb::ReadOnlyDatabase::open(&kopie.datei).unwrap();
        let txn = db.begin_read().unwrap();
        assert_eq!(txn.open_table(EREIGNISSE).unwrap().len().unwrap(), 2);
        assert_eq!(txn.open_table(VORGAENGE_JE_SLUG).unwrap().len().unwrap(), 1);
    }

    // ----- Absturz mitten im Schreiben -----

    fn zaehler_pfad(pfad: &Path) -> PathBuf {
        let mut name = pfad.file_name().unwrap().to_os_string();
        name.push(".zaehler");
        pfad.with_file_name(name)
    }

    /// Kindprozess für den nächsten Test: legt ohne Pause Vorgänge an und
    /// übernimmt jeden zweiten; nach jedem Vorgang steht die Zahl der fertigen
    /// in `<datei>.zaehler`. Ohne `DASHBOARD_TEST_VORGAENGE` tut er nichts.
    #[test]
    fn vorgaenge_schreiber_kind() {
        let Some(pfad) = std::env::var_os("DASHBOARD_TEST_VORGAENGE") else { return };
        let pfad = PathBuf::from(pfad);
        let zaehler = zaehler_pfad(&pfad);
        let s = Speicher::oeffnen(&pfad).unwrap();
        // Gewollte Dauerschleife: der Elterntest beendet den Kindprozess mitten im Schreiben.
        #[allow(clippy::for_unbounded_range)]
        for n in 1u64.. {
            let slug = if n % 3 == 0 { "architektur" } else { "dashboard" };
            let (v, _) = s.vorgang_anlegen(VorgangsArt::Abnahme, slug, &format!("Nr. {n}"), None, "chat").unwrap();
            if n % 2 == 0 {
                s.vorgang_weiter(&v.id, Zustand::Uebernommen, "chat", None).unwrap();
            }
            std::fs::write(&zaehler, n.to_string()).unwrap();
        }
    }

    /// Abnahme 2b: Server mitten im Schreiben beenden, nach dem Öffnen sind
    /// alle fertigen Vorgänge und Ereignisse da, und jeder Vorgang zeigt auf
    /// ein Ereignis, das es gibt.
    #[test]
    fn absturz_mitten_im_schreiben_verliert_keinen_vorgang() {
        let pfad = crate::speicher::test_pfad();
        let zaehler = zaehler_pfad(&pfad);
        let test = "speicher::vorgaenge::tests::vorgaenge_schreiber_kind";
        let mut kind = crate::speicher::kind_starten(test, "DASHBOARD_TEST_VORGAENGE", &pfad);
        let lesen = || std::fs::read_to_string(&zaehler).ok().and_then(|t| t.trim().parse::<u64>().ok());
        let frist = Instant::now() + Duration::from_secs(8);
        let mut gesehen = 0;
        while gesehen < 20 {
            if Instant::now() > frist {
                let _ = kind.kill();
                let _ = kind.wait();
                panic!("Kindprozess hat in 8 s nur {gesehen} Vorgänge geschrieben");
            }
            if let Some(status) = kind.try_wait().unwrap() {
                panic!("Kindprozess endete von selbst: {status}");
            }
            gesehen = gesehen.max(lesen().unwrap_or(0));
            std::thread::sleep(Duration::from_millis(5));
        }
        kind.kill().unwrap();
        kind.wait().unwrap();
        let stand = gesehen.max(lesen().unwrap_or(0));

        let s = Speicher::oeffnen(&pfad).expect("Datei lässt sich nach dem Absturz öffnen");
        let mut alle = s.vorgaenge_fuer("dashboard", false).unwrap();
        alle.extend(s.vorgaenge_fuer("architektur", false).unwrap());
        let anzahl = alle.len() as u64;
        assert!(anzahl >= stand, "{anzahl} Vorgänge in der Datei, gemeldet waren {stand}");
        let ereignisse = s.ereignisse_seit(None, 0).unwrap();
        let nummern: HashSet<u64> = ereignisse.iter().map(|e| e.nr).collect();
        assert_eq!(nummern.len(), ereignisse.len());
        assert_eq!(ereignisse.last().map(|e| e.nr), Some(ereignisse.len() as u64), "Nummern ohne Lücke");
        let uebernommen = alle.iter().filter(|v| v.zustand == Zustand::Uebernommen).count();
        assert_eq!(ereignisse.len(), alle.len() + uebernommen, "je Schritt genau ein Ereignis");
        for v in &alle {
            assert!(nummern.contains(&v.letzte_nr), "{}: Ereignis {} fehlt", v.id, v.letzte_nr);
            let (_, eigene) = s.vorgang(&v.id).unwrap().unwrap();
            assert_eq!(eigene.last().map(|e| (e.nr, e.zustand)), Some((v.letzte_nr, v.zustand)));
        }
        // Die Nummern im Titel lückenlos von 1 an: kein fertiger Vorgang fehlt.
        let mut titel: Vec<u64> = alle.iter().map(|v| v.titel[4..].parse().unwrap()).collect();
        titel.sort();
        assert!(titel.iter().enumerate().all(|(i, n)| *n == i as u64 + 1), "{titel:?}");
        drop(s);
        std::fs::remove_file(&pfad).unwrap();
        let _ = std::fs::remove_file(&zaehler);
    }
}
