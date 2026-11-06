//! Die Antworten des Inhabers auf Fragen (Etappe A2, Schema 5): Tabellen `antworten`
//! und `antworten_archiv`.
//!
//! Der Text einer Frage steht von Hand in `daten/fragen.toml`; die Antwort
//! liegt hier. Schlüssel ist `projekt/id/gestellt` (`antwort_schluessel`), wie
//! bei den Einstufungen: Stellt ein Chat später eine neue Frage mit derselben
//! Kennung (anderes `gestellt`), erbt sie keine alte Antwort.
//!
//! Antwort und Vorgang `antwort` (Bezug `antwort:<projekt>/<id>`, Herkunft
//! `browser`) entstehen in einer Transaktion: Nach einem Absturz ist entweder
//! beides da oder keines, und das Warten des Chats wacht auf, sobald die API
//! den Vorgang meldet. Eine Antwort wird nie überschrieben: Dieselbe Antwort
//! noch einmal ändert nichts (`Speichern::Gleich`), eine andere wird
//! abgelehnt (`Speichern::Anders`).
//!
//! Gelöscht wird nichts: Antworten ziehen
//! `ARCHIVIEREN_NACH_TAGEN` Tage nach dem Antworten unverändert nach
//! `antworten_archiv`. Eine Frage, deren Antwort im Archiv liegt, bleibt
//! beantwortet (`beantwortet_aus` liest beide Tabellen).

use std::collections::BTreeSet;

use chrono::{DateTime, SecondsFormat};
use redb::{ReadTransaction, ReadableDatabase, ReadableTable};
use tracing::warn;

use super::aufgaben::Zeit;
use super::tabellen::{self, ANTWORTEN, ANTWORTEN_ARCHIV};
use super::vorgaenge::{json_lesen, vorgang_anlegen_in};
use super::{Speicher, SpeicherFehler};
use crate::api::vertrag::{Antwort, Ereignis, Vorgang, VorgangsArt};
use crate::vertrag::daten::{ETWAS_ANDERES, FrageArt};

/// Antworten ziehen so viele Tage nach dem Antworten ins Archiv.
pub const ARCHIVIEREN_NACH_TAGEN: i64 = 7;
/// Die Antwort bei `art = "bestaetigen"`.
pub const BESTAETIGT: &str = "bestaetigt";
/// Herkunft jeder Antwort.
pub const VON_BROWSER: &str = "browser";
/// So lang darf der Titel eines Vorgangs sein (wie in `api::vorgaenge`).
const TITEL_HOECHSTENS: usize = 200;

/// Schlüssel in `antworten` und `antworten_archiv`: `projekt/id/gestellt`.
/// Projekt und Kennung bestehen nur aus a–z, 0–9 und Bindestrich, `gestellt`
/// ist ein Datum; der Schrägstrich trennt darum eindeutig.
pub fn antwort_schluessel(projekt: &str, id: &str, gestellt: &str) -> String {
    format!("{projekt}/{id}/{gestellt}")
}

/// Bezug des Vorgangs: `antwort:<projekt>/<id>` (Zielbild, Kapitel c2).
pub fn vorgangs_bezug(projekt: &str, id: &str) -> String {
    format!("antwort:{projekt}/{id}")
}

/// Liest Projekt und Kennung aus einem Bezug `antwort:<projekt>/<id>`.
pub fn aus_bezug(bezug: &str) -> Option<(&str, &str)> {
    let rest = bezug.strip_prefix("antwort:")?;
    let (projekt, id) = rest.split_once('/')?;
    (!projekt.is_empty() && !id.is_empty() && !id.contains('/')).then_some((projekt, id))
}

/// Eine neue Antwort, schon gegen die Frage geprüft (siehe `api::antworten`).
#[derive(Debug, Clone)]
pub struct NeueAntwort {
    pub projekt: String,
    pub id: String,
    pub gestellt: String,
    pub art: FrageArt,
    /// Text der Frage beim Antworten.
    pub frage: String,
    pub antwort: String,
    /// Freiwillige Anmerkung, schon geprüft (nie leer).
    pub notiz: Option<String>,
    /// Bei `antwort = "etwas-anderes"` die eigenen Worte, schon geprüft (nie leer); sonst `None`.
    pub eigene: Option<String>,
}

/// Ergebnis von `antwort_speichern`.
// Kurzlebiger Rückgabewert; eine Box brächte nur Umbau an allen Stellen, die ihn bauen und lesen.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Speichern {
    /// Gespeichert, mit dem neuen Vorgang und seinem ersten Ereignis.
    Neu { antwort: Antwort, vorgang: Vorgang, ereignis: Ereignis },
    /// Dieselbe Antwort mit derselben Anmerkung stand schon da (auch im
    /// Archiv); nichts geschrieben.
    Gleich(Antwort),
    /// Eine andere Antwort steht schon da, oder dieselbe mit einer anderen
    /// Anmerkung; nichts geschrieben.
    Anders(Antwort),
}

/// Titel des Vorgangs, höchstens 200 Zeichen. Kurze Antworten stehen darin,
/// damit der Chat sie schon im Warten sieht; einen Text und die Anmerkung
/// holt er über die API (der Titel sagt nur „mit Anmerkung“).
pub fn vorgangs_titel(art: FrageArt, frage: &str, antwort: &str, mit_notiz: bool) -> String {
    let notiz = if mit_notiz { " (mit Anmerkung)" } else { "" };
    let satz = match art {
        // „Etwas anderes …“: die eigenen Worte holt der Chat über die API (Feld `eigene`).
        _ if antwort == ETWAS_ANDERES && art != FrageArt::Text => {
            format!("Der Inhaber hat etwas anderes geschrieben (eigene Worte){notiz}: {frage}")
        }
        FrageArt::Wahl => format!("Der Inhaber hat gewählt „{antwort}“{notiz}: {frage}"),
        FrageArt::JaNein => format!("Der Inhaber hat geantwortet „{antwort}“{notiz}: {frage}"),
        FrageArt::Text => format!("Der Inhaber hat geantwortet (Text){notiz}: {frage}"),
        FrageArt::Bestaetigen => format!("Der Inhaber hat bestätigt{notiz}: {frage}"),
    };
    if satz.chars().count() <= TITEL_HOECHSTENS {
        return satz;
    }
    let mut kurz: String = satz.chars().take(TITEL_HOECHSTENS - 1).collect();
    kurz.push('…');
    kurz
}

/// Ist eine Antwort so alt, dass sie ins Archiv zieht? Eine unlesbare Zeit zählt nicht als alt.
fn zu_archivieren(a: &Antwort, jetzt: &Zeit) -> bool {
    DateTime::parse_from_rfc3339(&a.zeit).is_ok_and(|t| t + chrono::Duration::days(ARCHIVIEREN_NACH_TAGEN) <= *jetzt)
}

/// Eine Antwort aus einer Tabelle (lesend oder schreibend geöffnet).
fn antwort_aus<T: ReadableTable<&'static str, &'static [u8]>>(tabelle: &T, schluessel: &str) -> Result<Option<Antwort>, SpeicherFehler> {
    match tabelle.get(schluessel)? {
        Some(w) => Ok(Some(json_lesen(w.value(), &format!("antworten/{schluessel}"))?)),
        None => Ok(None),
    }
}

/// Alle lesbaren Antworten einer Tabelle, deren Schlüssel mit `anfang` beginnt
/// (leer: alle); unlesbare stehen im Protokoll.
fn alle_aus<T: ReadableTable<&'static str, &'static [u8]>>(tabelle: &T, anfang: &str) -> Result<Vec<Antwort>, SpeicherFehler> {
    let mut out = Vec::new();
    for eintrag in tabelle.range(anfang..)? {
        let (k, w) = eintrag?;
        if !k.value().starts_with(anfang) {
            break;
        }
        match json_lesen::<Antwort>(w.value(), &format!("antworten/{}", k.value())) {
            Ok(a) => out.push(a),
            Err(e) => warn!("antworten: {} ({})", e, e.einzelheiten()),
        }
    }
    Ok(out)
}

/// Die Schlüssel aller beantworteten Fragen (`projekt/id/gestellt`), aus
/// `antworten` und dem Archiv. Liest nur Schlüssel.
pub(super) fn beantwortet_aus(txn: &ReadTransaction) -> Result<BTreeSet<String>, SpeicherFehler> {
    let mut out = BTreeSet::new();
    for def in [ANTWORTEN, ANTWORTEN_ARCHIV] {
        if let Some(t) = tabellen::lesen(def, txn)? {
            for eintrag in t.iter()? {
                let (k, _) = eintrag?;
                out.insert(k.value().to_string());
            }
        }
    }
    Ok(out)
}

/// Neueste zuerst (in derselben Sekunde: die später gestellte Frage), dann fest nach Schlüssel.
/// Verglichen wird der Zeitpunkt, nicht der Text: Über die Zeitumstellung
/// (+02:00 auf +01:00) sortiert der Text falsch. Unlesbare Zeiten stehen hinten.
fn sortieren(liste: &mut [Antwort]) {
    let zeitpunkt = |a: &Antwort| DateTime::parse_from_rfc3339(&a.zeit).ok();
    liste.sort_by(|a, b| {
        zeitpunkt(b)
            .cmp(&zeitpunkt(a))
            .then_with(|| b.zeit.cmp(&a.zeit))
            .then_with(|| b.gestellt.cmp(&a.gestellt))
            .then_with(|| a.projekt.cmp(&b.projekt))
            .then_with(|| a.id.cmp(&b.id))
    });
}

impl Speicher {
    /// Speichert die Antwort des Inhabers und legt in derselben Transaktion den Vorgang
    /// `antwort` an. Steht unter dem Schlüssel schon eine Antwort (auch im
    /// Archiv), wird nichts geschrieben: `Gleich` oder `Anders`.
    pub fn antwort_speichern(&self, neu: NeueAntwort, jetzt: &Zeit) -> Result<Speichern, SpeicherFehler> {
        let schluessel = antwort_schluessel(&neu.projekt, &neu.id, &neu.gestellt);
        let txn = self.db.begin_write()?;
        let alt = match antwort_aus(&txn.open_table(ANTWORTEN)?, &schluessel)? {
            Some(a) => Some(a),
            None => antwort_aus(&txn.open_table(ANTWORTEN_ARCHIV)?, &schluessel)?,
        };
        if let Some(a) = alt {
            // Ohne Commit: nichts geschrieben.
            let gleich = a.antwort == neu.antwort && a.notiz == neu.notiz && a.eigene == neu.eigene;
            return Ok(if gleich { Speichern::Gleich(a) } else { Speichern::Anders(a) });
        }
        let titel = vorgangs_titel(neu.art, &neu.frage, &neu.antwort, neu.notiz.is_some());
        let bezug = vorgangs_bezug(&neu.projekt, &neu.id);
        let (vorgang, ereignis) = vorgang_anlegen_in(&txn, VorgangsArt::Antwort, &neu.projekt, &titel, Some(&bezug), VON_BROWSER)?;
        let antwort = Antwort {
            projekt: neu.projekt,
            id: neu.id,
            gestellt: neu.gestellt,
            art: neu.art,
            frage: neu.frage,
            antwort: neu.antwort,
            notiz: neu.notiz,
            eigene: neu.eigene,
            zeit: jetzt.to_rfc3339_opts(SecondsFormat::Secs, false),
            von: VON_BROWSER.to_string(),
            vorgang: vorgang.id.clone(),
        };
        if txn.open_table(ANTWORTEN)?.insert(schluessel.as_str(), serde_json::to_vec(&antwort)?.as_slice())?.is_some() {
            return Err(SpeicherFehler::Beschaedigt(format!("antworten/{schluessel}: Eintrag wäre überschrieben worden")));
        }
        self.festschreiben(txn, &[super::Bereich::Antworten, super::Bereich::Vorgaenge])?;
        Ok(Speichern::Neu { antwort, vorgang, ereignis })
    }

    /// Alle Antworten auf die Frage `id` des Projekts (jedes `gestellt`, auch
    /// aus dem Archiv), neueste zuerst.
    pub fn antworten_zur_frage(&self, projekt: &str, id: &str) -> Result<Vec<Antwort>, SpeicherFehler> {
        let anfang = format!("{projekt}/{id}/");
        let txn = self.db.begin_read()?;
        let mut liste = Vec::new();
        for def in [ANTWORTEN, ANTWORTEN_ARCHIV] {
            if let Some(t) = tabellen::lesen(def, &txn)? {
                liste.extend(alle_aus(&t, &anfang)?);
            }
        }
        sortieren(&mut liste);
        Ok(liste)
    }

    /// Die Antwort, die zum Vorgang `vorgang` gehört (über seinen Bezug
    /// `antwort:<projekt>/<id>`); sonst `None`.
    pub fn antwort_zum_vorgang(&self, vorgang: &Vorgang) -> Result<Option<Antwort>, SpeicherFehler> {
        if vorgang.art != VorgangsArt::Antwort {
            return Ok(None);
        }
        let Some((projekt, id)) = vorgang.bezug.as_deref().and_then(aus_bezug) else { return Ok(None) };
        Ok(self.antworten_zur_frage(projekt, id)?.into_iter().find(|a| a.vorgang == vorgang.id))
    }

    /// Antworten außerhalb des Archivs, wahlweise nur eines Projekts, neueste zuerst.
    pub fn antworten_lesen(&self, projekt: Option<&str>) -> Result<Vec<Antwort>, SpeicherFehler> {
        let anfang = projekt.map(|p| format!("{p}/")).unwrap_or_default();
        let txn = self.db.begin_read()?;
        let Some(t) = tabellen::lesen(ANTWORTEN, &txn)? else { return Ok(Vec::new()) };
        let mut liste = alle_aus(&t, &anfang)?;
        sortieren(&mut liste);
        Ok(liste)
    }

    /// Antworten, die `ARCHIVIEREN_NACH_TAGEN` Tage oder älter sind, ziehen
    /// unverändert nach `antworten_archiv` um. Steht der Schlüssel dort schon,
    /// bleibt die Antwort, wo sie ist (nie überschreiben). Gibt die umgezogenen zurück.
    pub fn antworten_archivieren(&self, jetzt: &Zeit) -> Result<Vec<Antwort>, SpeicherFehler> {
        let alt: Vec<String> = {
            let txn = self.db.begin_read()?;
            let Some(t) = tabellen::lesen(ANTWORTEN, &txn)? else { return Ok(Vec::new()) };
            alle_aus(&t, "")?.into_iter().filter(|a| zu_archivieren(a, jetzt)).map(|a| antwort_schluessel(&a.projekt, &a.id, &a.gestellt)).collect()
        };
        if alt.is_empty() {
            return Ok(Vec::new());
        }
        let txn = self.db.begin_write()?;
        let mut umgezogen = Vec::new();
        {
            let mut antworten = txn.open_table(ANTWORTEN)?;
            let mut archiv = txn.open_table(ANTWORTEN_ARCHIV)?;
            for schluessel in &alt {
                let roh = antworten.get(schluessel.as_str())?.map(|w| w.value().to_vec());
                let Some(roh) = roh else { continue };
                let a: Antwort = json_lesen(&roh, &format!("antworten/{schluessel}"))?;
                if !zu_archivieren(&a, jetzt) {
                    continue;
                }
                if archiv.get(schluessel.as_str())?.is_some() {
                    warn!("antworten_archiv/{schluessel}: Schlüssel steht schon im Archiv; die Antwort bleibt in antworten");
                    continue;
                }
                archiv.insert(schluessel.as_str(), roh.as_slice())?;
                antworten.remove(schluessel.as_str())?;
                umgezogen.push(a);
            }
        }
        self.festschreiben(txn, &[super::Bereich::Antworten])?;
        Ok(umgezogen)
    }

    /// Zahl der Einträge im Archiv der Antworten (für Tests).
    #[cfg(test)]
    pub fn antworten_archiv_anzahl(&self) -> u64 {
        let txn = self.db.begin_read().unwrap();
        tabellen::zaehlen(ANTWORTEN_ARCHIV, &txn).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::vertrag::Zustand;
    use chrono::{FixedOffset, TimeZone};

    fn zeit(tag: u32, stunde: u32) -> Zeit {
        FixedOffset::east_opt(3600).unwrap().with_ymd_and_hms(2026, 10, tag, stunde, 0, 0).unwrap()
    }

    fn neu(id: &str, gestellt: &str, art: FrageArt, antwort: &str) -> NeueAntwort {
        NeueAntwort {
            projekt: "beispiel".into(),
            id: id.into(),
            gestellt: gestellt.into(),
            art,
            frage: format!("Frage {id}?"),
            antwort: antwort.into(),
            notiz: None,
            eigene: None,
        }
    }

    /// Antwort und Vorgang in einem Schritt; dieselbe Antwort noch einmal
    /// ändert nichts, eine andere wird abgelehnt; neu gestellt ist ein eigener Schlüssel.
    #[test]
    fn speichern_mit_vorgang_und_ohne_ueberschreiben() {
        let s = Speicher::temporaer();
        let Speichern::Neu { antwort, vorgang, ereignis } =
            s.antwort_speichern(neu("schlusssatz", "2026-10-25", FrageArt::Wahl, "Kurz"), &zeit(29, 10)).unwrap()
        else {
            panic!("neu erwartet")
        };
        assert_eq!((antwort.von.as_str(), antwort.zeit.as_str(), antwort.vorgang.as_str()), ("browser", "2026-10-29T10:00:00+01:00", vorgang.id.as_str()));
        assert_eq!((vorgang.art, vorgang.slug.as_str(), vorgang.zustand, vorgang.von.as_str()), (VorgangsArt::Antwort, "beispiel", Zustand::Offen, "browser"));
        assert_eq!(vorgang.bezug.as_deref(), Some("antwort:beispiel/schlusssatz"));
        assert_eq!(vorgang.titel, "Der Inhaber hat gewählt „Kurz“: Frage schlusssatz?");
        assert_eq!(ereignis.vorgang, vorgang.id);
        assert_eq!(s.cursor().unwrap(), 1);
        // Noch einmal dasselbe: nichts geschrieben.
        assert!(
            matches!(s.antwort_speichern(neu("schlusssatz", "2026-10-25", FrageArt::Wahl, "Kurz"), &zeit(29, 11)).unwrap(), Speichern::Gleich(a) if a == antwort)
        );
        // Etwas anderes: abgelehnt, die alte Antwort bleibt.
        assert!(
            matches!(s.antwort_speichern(neu("schlusssatz", "2026-10-25", FrageArt::Wahl, "Lang"), &zeit(29, 11)).unwrap(), Speichern::Anders(a) if a.antwort == "Kurz")
        );
        assert_eq!(s.cursor().unwrap(), 1, "kein zweiter Vorgang");
        // Neu gestellt (anderes Datum): eigener Schlüssel, eigene Antwort.
        assert!(matches!(s.antwort_speichern(neu("schlusssatz", "2026-10-28", FrageArt::Wahl, "Lang"), &zeit(29, 12)).unwrap(), Speichern::Neu { .. }));
        let beide = s.antworten_zur_frage("beispiel", "schlusssatz").unwrap();
        assert_eq!(beide.iter().map(|a| a.antwort.as_str()).collect::<Vec<_>>(), ["Lang", "Kurz"], "neueste zuerst");
        // Eine andere Kennung, die mit derselben beginnt, gehört nicht dazu.
        s.antwort_speichern(neu("schlusssatz-2", "2026-10-25", FrageArt::JaNein, "ja"), &zeit(29, 12)).unwrap();
        assert_eq!(s.antworten_zur_frage("beispiel", "schlusssatz").unwrap().len(), 2);
        assert_eq!(s.antwort_zum_vorgang(&vorgang).unwrap(), Some(antwort));
        assert_eq!(s.antworten_lesen(Some("beispiel")).unwrap().len(), 3);
        assert!(s.antworten_lesen(Some("bewerb")).unwrap().is_empty(), "nur ganze Projektnamen");
        let txn = s.db.begin_read().unwrap();
        let keys = beantwortet_aus(&txn).unwrap();
        assert!(keys.contains("beispiel/schlusssatz/2026-10-25") && keys.contains("beispiel/schlusssatz/2026-10-28"), "{keys:?}");
    }

    /// Archiv nach sieben Tagen: unverändert, weiter beantwortet, im Export;
    /// dieselbe Antwort noch einmal gilt als gleich, eine andere als anders.
    #[test]
    fn archiv_nach_sieben_tagen() {
        let s = Speicher::temporaer();
        s.antwort_speichern(neu("alt", "2026-10-01", FrageArt::Bestaetigen, BESTAETIGT), &zeit(1, 10)).unwrap();
        s.antwort_speichern(neu("jung", "2026-10-07", FrageArt::Text, "Bitte kürzer.\nDanke"), &zeit(7, 10)).unwrap();
        assert!(s.antworten_archivieren(&zeit(8, 9)).unwrap().is_empty(), "vor sieben Tagen noch nichts");
        let weg = s.antworten_archivieren(&zeit(8, 10)).unwrap();
        assert_eq!(weg.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["alt"]);
        assert_eq!(s.antworten_archiv_anzahl(), 1);
        assert_eq!(s.antworten_lesen(None).unwrap().iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["jung"]);
        assert_eq!(s.antworten_zur_frage("beispiel", "alt").unwrap(), weg, "aus dem Archiv lesbar");
        let txn = s.db.begin_read().unwrap();
        assert!(beantwortet_aus(&txn).unwrap().contains("beispiel/alt/2026-10-01"), "bleibt beantwortet");
        drop(txn);
        assert!(matches!(s.antwort_speichern(neu("alt", "2026-10-01", FrageArt::Bestaetigen, BESTAETIGT), &zeit(9, 10)).unwrap(), Speichern::Gleich(_)));
        assert!(matches!(s.antwort_speichern(neu("alt", "2026-10-01", FrageArt::Bestaetigen, "anders"), &zeit(9, 10)).unwrap(), Speichern::Anders(_)));
        let export: serde_json::Value = serde_json::from_str(&s.export().unwrap().als_json()).unwrap();
        assert_eq!(export["antworten_archiv"][0]["id"], "alt");
        assert_eq!(export["antworten"][0]["antwort"], "Bitte kürzer.\nDanke");
        assert!(s.antworten_archivieren(&zeit(30, 10)).unwrap().len() == 1, "dann auch die jüngere");
        assert!(s.antworten_archivieren(&zeit(31, 10)).unwrap().is_empty(), "ein weiterer Lauf tut nichts");
    }

    /// Anmerkung: gehört zur Antwort. Dieselbe Antwort mit derselben
    /// Anmerkung ist gleich; mit einer anderen (oder ohne) gilt sie als anders.
    #[test]
    fn anmerkung_gehoert_zur_antwort() {
        let s = Speicher::temporaer();
        let mit = |notiz: Option<&str>| NeueAntwort { notiz: notiz.map(str::to_string), ..neu("weiter", "2026-10-26", FrageArt::JaNein, "ja") };
        let Speichern::Neu { antwort, vorgang, .. } = s.antwort_speichern(mit(Some("Aber erst\nnach Montag.")), &zeit(29, 10)).unwrap() else {
            panic!("neu erwartet")
        };
        assert_eq!(antwort.notiz.as_deref(), Some("Aber erst\nnach Montag."));
        assert_eq!(vorgang.titel, "Der Inhaber hat geantwortet „ja“ (mit Anmerkung): Frage weiter?");
        assert!(matches!(s.antwort_speichern(mit(Some("Aber erst\nnach Montag.")), &zeit(29, 11)).unwrap(), Speichern::Gleich(_)));
        assert!(matches!(s.antwort_speichern(mit(Some("Sofort.")), &zeit(29, 11)).unwrap(), Speichern::Anders(_)));
        assert!(matches!(s.antwort_speichern(mit(None), &zeit(29, 11)).unwrap(), Speichern::Anders(_)));
        assert_eq!(s.cursor().unwrap(), 1, "kein zweiter Vorgang");
        assert_eq!(s.antworten_zur_frage("beispiel", "weiter").unwrap(), vec![antwort]);
    }

    /// „Etwas anderes …“: Antwort `etwas-anderes`
    /// mit den eigenen Worten, Vorgang in derselben Transaktion; derselbe Text
    /// noch einmal ist gleich, ein anderer anders.
    #[test]
    fn etwas_anderes_mit_eigenen_worten() {
        let s = Speicher::temporaer();
        let mit = |eigene: &str| NeueAntwort { eigene: Some(eigene.to_string()), ..neu("farbe", "2026-11-01", FrageArt::Wahl, ETWAS_ANDERES) };
        let Speichern::Neu { antwort, vorgang, .. } = s.antwort_speichern(mit("Lieber Grau,\nwie das Logo."), &zeit(29, 10)).unwrap() else {
            panic!("neu erwartet")
        };
        assert_eq!((antwort.antwort.as_str(), antwort.eigene.as_deref()), (ETWAS_ANDERES, Some("Lieber Grau,\nwie das Logo.")));
        assert_eq!(vorgang.titel, "Der Inhaber hat etwas anderes geschrieben (eigene Worte): Frage farbe?");
        assert_eq!(vorgang.bezug.as_deref(), Some("antwort:beispiel/farbe"));
        assert!(matches!(s.antwort_speichern(mit("Lieber Grau,\nwie das Logo."), &zeit(29, 11)).unwrap(), Speichern::Gleich(_)));
        assert!(matches!(s.antwort_speichern(mit("Doch Blau."), &zeit(29, 11)).unwrap(), Speichern::Anders(_)));
        assert_eq!(s.cursor().unwrap(), 1, "kein zweiter Vorgang");
        assert_eq!(s.antwort_zum_vorgang(&vorgang).unwrap(), Some(antwort));
        // Ein Klick hat `eigene = null` (auch im JSON).
        s.antwort_speichern(neu("weiter", "2026-11-01", FrageArt::JaNein, "ja"), &zeit(29, 12)).unwrap();
        let json = serde_json::to_value(&s.antworten_zur_frage("beispiel", "weiter").unwrap()[0]).unwrap();
        assert_eq!(json["eigene"], serde_json::Value::Null, "{json}");
    }

    /// Antworten aus der Zeit vor der Anmerkung (ohne Feld `notiz`) bleiben
    /// lesbar, gelten als ohne Anmerkung und ziehen so auch ins Archiv.
    #[test]
    fn alte_antwort_ohne_notiz_bleibt_lesbar() {
        let s = Speicher::temporaer();
        let alt = r#"{"projekt":"beispiel","id":"alt","gestellt":"2026-10-20","art":"ja-nein","frage":"Passt das?","antwort":"ja","zeit":"2026-10-20T10:00:00+01:00","von":"browser","vorgang":"v-20261020-0001"}"#;
        {
            let txn = s.db.begin_write().unwrap();
            txn.open_table(ANTWORTEN).unwrap().insert("beispiel/alt/2026-10-20", alt.as_bytes()).unwrap();
            txn.commit().unwrap();
        }
        let gelesen = s.antworten_zur_frage("beispiel", "alt").unwrap();
        assert_eq!((gelesen.len(), gelesen[0].antwort.as_str(), gelesen[0].notiz.as_deref()), (1, "ja", None));
        assert_eq!(s.antworten_lesen(None).unwrap(), gelesen);
        // Dieselbe Antwort ohne Anmerkung ist gleich, mit Anmerkung anders.
        assert!(matches!(s.antwort_speichern(neu("alt", "2026-10-20", FrageArt::JaNein, "ja"), &zeit(29, 11)).unwrap(), Speichern::Gleich(_)));
        let mit = NeueAntwort { notiz: Some("x".into()), ..neu("alt", "2026-10-20", FrageArt::JaNein, "ja") };
        assert!(matches!(s.antwort_speichern(mit, &zeit(29, 11)).unwrap(), Speichern::Anders(_)));
        assert_eq!(s.antworten_archivieren(&zeit(30, 10)).unwrap(), gelesen, "zieht unverändert ins Archiv");
        let export: serde_json::Value = serde_json::from_str(&s.export().unwrap().als_json()).unwrap();
        assert_eq!(export["antworten_archiv"][0]["antwort"], "ja");
    }

    /// Über die Zeitumstellung zählt der Zeitpunkt: 02:15+01:00 ist nach
    /// 02:30+02:00, obwohl der Text kleiner ist. Unlesbare Zeiten stehen hinten.
    #[test]
    fn sortiert_ueber_die_zeitumstellung() {
        let a = |id: &str, zeit: &str| Antwort {
            projekt: "p".into(),
            id: id.into(),
            gestellt: "2026-10-24".into(),
            art: FrageArt::JaNein,
            frage: "?".into(),
            antwort: "ja".into(),
            notiz: None,
            eigene: None,
            zeit: zeit.into(),
            von: VON_BROWSER.into(),
            vorgang: "v".into(),
        };
        let mut liste = vec![a("kaputt", "gestern"), a("sommer", "2026-10-25T02:30:00+02:00"), a("winter", "2026-10-25T02:15:00+01:00")];
        sortieren(&mut liste);
        assert_eq!(liste.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), ["winter", "sommer", "kaputt"]);
    }

    #[test]
    fn titel_und_bezug() {
        assert_eq!(vorgangs_titel(FrageArt::JaNein, "Passt das?", "nein", false), "Der Inhaber hat geantwortet „nein“: Passt das?");
        assert_eq!(vorgangs_titel(FrageArt::Text, "Was fehlt?", "Viel Text", false), "Der Inhaber hat geantwortet (Text): Was fehlt?");
        assert_eq!(vorgangs_titel(FrageArt::Bestaetigen, "Konzept so?", BESTAETIGT, false), "Der Inhaber hat bestätigt: Konzept so?");
        let lang = vorgangs_titel(FrageArt::Wahl, &"x".repeat(300), "A", false);
        assert_eq!(lang.chars().count(), 200);
        assert!(lang.ends_with('…'));
        assert_eq!(vorgangs_titel(FrageArt::JaNein, "Passt das?", "ja", true), "Der Inhaber hat geantwortet „ja“ (mit Anmerkung): Passt das?");
        assert_eq!(vorgangs_titel(FrageArt::Bestaetigen, "Konzept so?", BESTAETIGT, true), "Der Inhaber hat bestätigt (mit Anmerkung): Konzept so?");
        assert_eq!(aus_bezug("antwort:beispiel/schlusssatz"), Some(("beispiel", "schlusssatz")));
        for falsch in ["aufgabe:a-1", "antwort:beispiel", "antwort:/x", "antwort:a/b/c"] {
            assert_eq!(aus_bezug(falsch), None, "{falsch}");
        }
        assert_eq!(antwort_schluessel("p", "f", "2026-10-25"), "p/f/2026-10-25");
    }
}
