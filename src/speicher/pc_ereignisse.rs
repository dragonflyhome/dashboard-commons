//! Windows-Ereignisse für den Verlauf (Etappe 2e, Schema 9): Tabellen
//! `pc_ereignisse` und `pc_ereignisse_archiv`.
//!
//! Der Sammler (`pc::sysapps`, stündlich) liest Hotfixes, Starts und
//! Update-Ereignisse der letzten 90 Tage und spielt sie hier ein: neue
//! Kennungen kommen dazu, bekannte bekommen nur `zuletzt_gesehen` neu. So
//! bleibt der Verlauf, auch wenn Windows die Ereignisanzeige leert.
//!
//! Stabile Kennungen: Ein Update, das schon da ist (gleiche KB, gleicher Tag,
//! gleiche Zeit), behält seine Kennung, auch wenn am selben Tag weitere dazu
//! kommen und der Sammler deshalb jetzt die Version anhängt (siehe
//! `pc::ereignisse`). Ein Hotfix nur mit Tag wird genauer, sobald das
//! Update-Ereignis mit Uhrzeit dazukommt.
//!
//! Was älter als [`ARCHIV_NACH_TAGEN`] Tage ist (nach `zeit`), zieht in
//! `pc_ereignisse_archiv` um (aus dem Minutentakt in
//! `api::aufgaben::spawn_ablauf`); nichts wird gelöscht. Ältere Ereignisse,
//! die der Sammler sieht (alle Hotfixes), landen gleich dort.

use std::sync::Arc;

use chrono::{DateTime, Duration, SecondsFormat};
use redb::{ReadableDatabase, ReadableTable, Table};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use super::aufgaben::Zeit;
use super::tabellen::{self, PC_EREIGNISSE, PC_EREIGNISSE_ARCHIV};
use super::vorgaenge::json_lesen;
use super::{Speicher, SpeicherFehler};
use crate::pc::ereignisse::version_aus;
use crate::vertrag::verlauf::{Ereignis, Quelle};

/// Ereignisse, deren `zeit` so viele Tage zurückliegt, stehen im Archiv.
pub const ARCHIV_NACH_TAGEN: i64 = 90;

/// Ein Eintrag in `pc_ereignisse` und `pc_ereignisse_archiv` (Schlüssel: Kennung).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PcEreignisEintrag {
    pub ereignis: Ereignis,
    /// Ortszeit, RFC 3339: der Lauf des Sammlers, der es zuerst sah.
    pub zuerst_gesehen: String,
    /// Ortszeit, RFC 3339: der letzte Lauf, der es sah.
    pub zuletzt_gesehen: String,
}

/// Was ein Lauf des Sammlers eingespielt hat.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Eingespielt {
    pub neu: usize,
    pub bekannt: usize,
}

fn zeit_text(z: &Zeit) -> String {
    z.to_rfc3339_opts(SecondsFormat::Secs, false)
}

/// Grenze zum Archiv: `jetzt` minus [`ARCHIV_NACH_TAGEN`] Tage.
pub fn archiv_grenze(jetzt: &Zeit) -> Zeit {
    *jetzt - Duration::days(ARCHIV_NACH_TAGEN)
}

/// Gehört das Ereignis ins Archiv (seine `zeit` vor der Grenze)?
fn ins_archiv(e: &Ereignis, jetzt: &Zeit) -> bool {
    DateTime::parse_from_rfc3339(&e.zeit).is_ok_and(|t| t < archiv_grenze(jetzt))
}

type Tabelle<'t> = Table<'t, &'static str, &'static [u8]>;

fn eintrag_aus<T: ReadableTable<&'static str, &'static [u8]>>(t: &T, kennung: &str) -> Result<Option<PcEreignisEintrag>, SpeicherFehler> {
    match t.get(kennung)? {
        Some(w) => Ok(Some(json_lesen(w.value(), &format!("pc_ereignisse/{kennung}"))?)),
        None => Ok(None),
    }
}

fn schreiben(t: &mut Tabelle<'_>, e: &PcEreignisEintrag) -> Result<(), SpeicherFehler> {
    t.insert(e.ereignis.kennung.as_str(), serde_json::to_vec(e)?.as_slice())?;
    Ok(())
}

/// Die Gruppe eines Updates mit KB: `update:<KB>:<Tag>` (die ersten drei Teile).
fn gruppe_von(e: &Ereignis) -> Option<String> {
    if e.quelle != Quelle::Update || !e.kennung.starts_with("update:KB") {
        return None;
    }
    let teile: Vec<&str> = e.kennung.splitn(4, ':').collect();
    (teile.len() >= 3).then(|| teile[..3].join(":"))
}

/// Kennungen in `t`, die zur Gruppe `gruppe` gehören (sie selbst und `gruppe:…`), mit ihrer Zeit.
fn gruppe_in<T: ReadableTable<&'static str, &'static [u8]>>(t: &T, gruppe: &str) -> Result<Vec<(String, String)>, SpeicherFehler> {
    let mut aus = Vec::new();
    for eintrag in t.range::<&str>(gruppe..)? {
        let (k, w) = eintrag?;
        let k = k.value();
        if k != gruppe && !k.starts_with(&format!("{gruppe}:")) {
            break;
        }
        match json_lesen::<PcEreignisEintrag>(w.value(), &format!("pc_ereignisse/{k}")) {
            Ok(alt) => aus.push((k.to_string(), alt.ereignis.zeit)),
            Err(e) => warn!("pc_ereignisse: {e} ({})", e.einzelheiten()),
        }
    }
    Ok(aus)
}

/// Welche Kennung ein Ereignis aus dem Sammler bekommt:
/// 1. Gibt es in seiner Gruppe (Update mit KB und Tag) schon eins mit
///    derselben Zeit, dessen Kennung (stabil über Läufe).
/// 2. Steht unter seiner Kennung schon ein anderes Ereignis (andere Zeit bei
///    gleicher Genauigkeit; ein ungenaueres oder genaueres am selben Tag ist
///    dasselbe), bekommt es die Version aus dem Titel oder die Minute dazu,
///    notfalls noch `-2`, `-3`.
/// 3. Sonst seine eigene.
fn kennung_waehlen(haupt: &Tabelle<'_>, archiv: &Tabelle<'_>, e: &Ereignis) -> Result<String, SpeicherFehler> {
    if let Some(g) = gruppe_von(e) {
        for t in [haupt, archiv] {
            if let Some((k, _)) = gruppe_in(t, &g)?.into_iter().find(|(_, zeit)| *zeit == e.zeit) {
                return Ok(k);
            }
        }
    }
    let belegt = |k: &str| -> Result<Option<PcEreignisEintrag>, SpeicherFehler> {
        Ok(match eintrag_aus(haupt, k)? {
            Some(a) => Some(a),
            None => eintrag_aus(archiv, k)?,
        })
    };
    // Dasselbe Ereignis, wenn eines von beiden nur ungenauer ist (Hotfix nur mit
    // Tag neben dem Update-Ereignis mit Uhrzeit, in beiden Richtungen: nach 90
    // Tagen fällt das Update-Ereignis aus der Anzeige, der Hotfix kommt weiter).
    let anderes = |alt: &PcEreignisEintrag| alt.ereignis.zeit != e.zeit && alt.ereignis.genauigkeit == e.genauigkeit;
    match belegt(&e.kennung)? {
        Some(alt) if anderes(&alt) => {}
        _ => return Ok(e.kennung.clone()),
    }
    let zusatz =
        version_aus(&e.titel).unwrap_or_else(|| DateTime::parse_from_rfc3339(&e.zeit).map(|t| t.format("%H-%M").to_string()).unwrap_or_else(|_| "neu".into()));
    let mut kennung = format!("{}:{zusatz}", e.kennung);
    let mut n = 2;
    while belegt(&kennung)?.is_some_and(|alt| anderes(&alt)) {
        kennung = format!("{}:{zusatz}-{n}", e.kennung);
        n += 1;
    }
    Ok(kennung)
}

impl Speicher {
    /// Ein Lauf des Sammlers: neue Kennungen anlegen (ältere als 90 Tage
    /// gleich im Archiv), bekannte nur mit `zuletzt_gesehen` (ein Hotfix nur
    /// mit Tag wird dabei genauer). Eine Transaktion.
    pub fn pc_ereignisse_einspielen(&self, neue: &[Ereignis], jetzt: &Zeit) -> Result<Eingespielt, SpeicherFehler> {
        let jetzt_text = zeit_text(jetzt);
        let txn = self.db.begin_write()?;
        let mut erg = Eingespielt::default();
        {
            let mut haupt = txn.open_table(PC_EREIGNISSE)?;
            let mut archiv = txn.open_table(PC_EREIGNISSE_ARCHIV)?;
            for e in neue {
                let kennung = kennung_waehlen(&haupt, &archiv, e)?;
                let mut e = e.clone();
                e.kennung = kennung;
                let (alt, tabelle) = match eintrag_aus(&haupt, &e.kennung)? {
                    Some(a) => (Some(a), &mut haupt),
                    None => match eintrag_aus(&archiv, &e.kennung)? {
                        Some(a) => (Some(a), &mut archiv),
                        None if ins_archiv(&e, jetzt) => (None, &mut archiv),
                        None => (None, &mut haupt),
                    },
                };
                let eintrag = match alt {
                    Some(mut a) => {
                        erg.bekannt += 1;
                        if a.ereignis.genauigkeit > e.genauigkeit {
                            a.ereignis = e;
                        }
                        a.zuletzt_gesehen = jetzt_text.clone();
                        a
                    }
                    None => {
                        erg.neu += 1;
                        PcEreignisEintrag { ereignis: e, zuerst_gesehen: jetzt_text.clone(), zuletzt_gesehen: jetzt_text.clone() }
                    }
                };
                schreiben(tabelle, &eintrag)?;
            }
        }
        txn.commit()?;
        // Nur neue Einträge ändern, was Seiten zeigen (Verlauf, Zahlen auf /daten);
        // bekannte bekommen nur `zuletzt_gesehen`.
        if erg.neu > 0 {
            self.gemeldet(&[super::Bereich::WindowsEreignisse]);
        }
        Ok(erg)
    }

    /// Einträge, deren `zeit` älter als 90 Tage ist, ziehen ins Archiv;
    /// nichts wird gelöscht. Auswahl und Umzug in derselben
    /// Schreib-Transaktion, damit kein Sammler-Lauf dazwischen schreibt. Steht
    /// die Kennung dort schon, gilt der spätere `zuletzt_gesehen`. Gibt die Zahl zurück.
    pub fn pc_ereignisse_archivieren(&self, jetzt: &Zeit) -> Result<usize, SpeicherFehler> {
        let txn = self.db.begin_write()?;
        let alt: Vec<PcEreignisEintrag>;
        {
            let mut haupt = txn.open_table(PC_EREIGNISSE)?;
            let mut archiv = txn.open_table(PC_EREIGNISSE_ARCHIV)?;
            alt = alle_aus(&haupt)?.into_iter().filter(|a| ins_archiv(&a.ereignis, jetzt)).collect();
            if alt.is_empty() {
                drop((haupt, archiv));
                txn.abort()?;
                return Ok(0);
            }
            for mut a in alt.iter().cloned() {
                if let Some(da) = eintrag_aus(&archiv, &a.ereignis.kennung)? {
                    if da.zuletzt_gesehen > a.zuletzt_gesehen {
                        a.zuletzt_gesehen = da.zuletzt_gesehen;
                    }
                    a.zuerst_gesehen = a.zuerst_gesehen.min(da.zuerst_gesehen);
                }
                schreiben(&mut archiv, &a)?;
                haupt.remove(a.ereignis.kennung.as_str())?;
            }
        }
        self.festschreiben(txn, &[super::Bereich::WindowsEreignisse])?;
        Ok(alt.len())
    }

    /// Alle Einträge der letzten 90 Tage, mit `mit_archiv` auch die älteren.
    /// Ohne zugesagte Reihenfolge.
    pub fn pc_ereignisse_lesen(&self, mit_archiv: bool) -> Result<Vec<PcEreignisEintrag>, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        let mut aus = match tabellen::lesen(PC_EREIGNISSE, &txn)? {
            Some(t) => alle_aus(&t)?,
            None => Vec::new(),
        };
        if mit_archiv && let Some(t) = tabellen::lesen(PC_EREIGNISSE_ARCHIV, &txn)? {
            aus.extend(alle_aus(&t)?);
        }
        Ok(aus)
    }
}

/// Alle lesbaren Einträge einer Tabelle; unlesbare stehen im Protokoll.
fn alle_aus<T: ReadableTable<&'static str, &'static [u8]>>(t: &T) -> Result<Vec<PcEreignisEintrag>, SpeicherFehler> {
    let mut aus = Vec::new();
    for eintrag in t.iter()? {
        let (k, w) = eintrag?;
        match json_lesen::<PcEreignisEintrag>(w.value(), &format!("pc_ereignisse/{}", k.value())) {
            Ok(a) => aus.push(a),
            Err(e) => warn!("pc_ereignisse: {e} ({})", e.einzelheiten()),
        }
    }
    Ok(aus)
}

/// Die Ereignisse eines Sammler-Laufs im Hintergrund einspielen (ohne
/// Datenspeicher nichts). Fehler gehen nur ins Protokoll.
pub async fn sammler_lauf(speicher: Option<&Arc<Speicher>>, ereignisse: Vec<Ereignis>) {
    let Some(s) = speicher.cloned() else { return };
    if ereignisse.is_empty() {
        return;
    }
    let jetzt = super::aufgaben::jetzt();
    match Speicher::im_hintergrund(s, move |s| s.pc_ereignisse_einspielen(&ereignisse, &jetzt)).await {
        Ok(e) => debug!("Windows-Ereignisse: {} neu, {} schon bekannt", e.neu, e.bekannt),
        Err(e) => warn!("Windows-Ereignisse nicht gespeichert: {e} ({})", e.einzelheiten()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pc::ereignisse::{self, UpdateRoh};
    use crate::vertrag::verlauf::{Art, Genauigkeit};

    fn zeit(s: &str) -> Zeit {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    fn signatur(z: &str, version: &str) -> UpdateRoh {
        UpdateRoh {
            zeit: zeit(z),
            titel: format!("Security Intelligence-Update für Microsoft Defender Antivirus - KB2267602 (Version {version}) - Current Channel (Broad)"),
        }
    }

    fn kennungen(s: &Speicher) -> Vec<String> {
        let mut k: Vec<String> = s.pc_ereignisse_lesen(true).unwrap().into_iter().map(|a| a.ereignis.kennung).collect();
        k.sort();
        k
    }

    /// Abnahme 2e: Kennungen bleiben über zwei Läufe des Sammlers gleich,
    /// auch wenn am selben Tag weitere Signatur-Updates dazukommen (dann
    /// hängt der Sammler die Version an, das erste behält seine Kennung).
    #[test]
    fn stabile_kennungen_ueber_zwei_laeufe() {
        let s = Speicher::temporaer();
        let lauf1 = zeit("2026-10-30T09:00:00+01:00");
        let a = signatur("2026-10-30T08:12:00+01:00", "1.439.141.0");
        let start = zeit("2026-10-30T07:59:01+01:00");
        let erste = ereignisse::alle(&[], std::slice::from_ref(&a), &[start], None);
        assert_eq!(s.pc_ereignisse_einspielen(&erste, &lauf1).unwrap(), Eingespielt { neu: 2, bekannt: 0 });
        assert_eq!(kennungen(&s), ["start:2026-10-30T07:59:01+01:00", "update:KB2267602:2026-10-30"]);

        // Zweiter Lauf: zwei weitere am selben Tag, derselbe Start.
        let lauf2 = zeit("2026-10-30T21:00:00+01:00");
        let roh = [a.clone(), signatur("2026-10-30T14:05:00+01:00", "1.439.150.0"), signatur("2026-10-30T20:40:00+01:00", "1.439.160.0")];
        let zweite = ereignisse::alle(&[], &roh, &[start], None);
        assert_eq!(s.pc_ereignisse_einspielen(&zweite, &lauf2).unwrap(), Eingespielt { neu: 2, bekannt: 2 });
        assert_eq!(
            kennungen(&s),
            [
                "start:2026-10-30T07:59:01+01:00",
                "update:KB2267602:2026-10-30",
                "update:KB2267602:2026-10-30:1.439.150.0",
                "update:KB2267602:2026-10-30:1.439.160.0"
            ]
        );
        let alle = s.pc_ereignisse_lesen(false).unwrap();
        let erstes = alle.iter().find(|a| a.ereignis.kennung == "update:KB2267602:2026-10-30").unwrap();
        assert_eq!(erstes.zuerst_gesehen, zeit_text(&lauf1));
        assert_eq!(erstes.zuletzt_gesehen, zeit_text(&lauf2));
        assert!(alle.iter().all(|a| a.ereignis.art == Art::Routine));

        // Dritter Lauf, das erste ist aus der Ereignisanzeige verschwunden:
        // Die beiden anderen behalten ihre Kennung, nichts kommt dazu.
        let dritte = ereignisse::alle(&[], &roh[1..], &[], None);
        assert_eq!(s.pc_ereignisse_einspielen(&dritte, &lauf2).unwrap(), Eingespielt { neu: 0, bekannt: 2 });
        // Nur noch eines am Tag (Kennung ohne Zusatz): es behält seine mit Version.
        let vierte = ereignisse::alle(&[], &roh[2..], &[], None);
        assert_eq!(vierte[0].kennung, "update:KB2267602:2026-10-30");
        assert_eq!(s.pc_ereignisse_einspielen(&vierte, &lauf2).unwrap(), Eingespielt { neu: 0, bekannt: 1 });
        assert_eq!(kennungen(&s).len(), 4);
    }

    /// Ein Hotfix nur mit Tag wird genauer, sobald das Update-Ereignis mit
    /// Uhrzeit dazukommt; die Kennung bleibt. Ein anderes Ereignis unter
    /// derselben Kennung (andere Zeit) überschreibt nichts.
    /// Abnahme 2e, Punkt 1: Nach 90 Tagen fällt das Update-Ereignis aus der
    /// Anzeige, der Hotfix (nur mit Tag) kommt weiter. Er ist dasselbe Ereignis:
    /// gleiche Kennung, kein `…:00-00`, und die Uhrzeit bleibt; auch im Archiv.
    #[test]
    fn hotfix_ohne_update_ereignis_bleibt_dasselbe() {
        let s = Speicher::temporaer();
        let hf = crate::pc::sysapps::Hotfix { id: "KB5066835".into(), description: "Security Update".into(), installed_on: "2026-10-15".into() };
        let u = UpdateRoh { zeit: zeit("2026-10-15T09:30:00+02:00"), titel: "2026-10 Kumulatives Update (KB5066835)".into() };
        let lauf1 = zeit("2026-10-30T09:00:00+01:00");
        s.pc_ereignisse_einspielen(&ereignisse::alle(std::slice::from_ref(&hf), std::slice::from_ref(&u), &[], None), &lauf1).unwrap();
        let nur_hotfix = ereignisse::alle(std::slice::from_ref(&hf), &[], &[], None);
        assert_eq!(nur_hotfix[0].genauigkeit, Genauigkeit::Tag);
        assert_eq!(s.pc_ereignisse_einspielen(&nur_hotfix, &lauf1).unwrap(), Eingespielt { neu: 0, bekannt: 1 });
        assert_eq!(kennungen(&s), ["update:KB5066835:2026-10-15"]);
        let alle = s.pc_ereignisse_lesen(true).unwrap();
        assert_eq!((alle[0].ereignis.genauigkeit, alle[0].ereignis.zeit.as_str()), (Genauigkeit::Minute, "2026-10-15T09:30:00+02:00"));
        // Vier Monate später: das Ereignis steht im Archiv, der Hotfix kommt wieder.
        let spaeter = zeit("2027-02-20T09:00:00+01:00");
        assert_eq!(s.pc_ereignisse_archivieren(&spaeter).unwrap(), 1);
        assert_eq!(s.pc_ereignisse_einspielen(&nur_hotfix, &spaeter).unwrap(), Eingespielt { neu: 0, bekannt: 1 });
        assert_eq!(kennungen(&s), ["update:KB5066835:2026-10-15"]);
        assert!(s.pc_ereignisse_lesen(false).unwrap().is_empty(), "nichts Neues in der Haupttabelle");
    }

    #[test]
    fn hotfix_wird_genauer_und_nichts_wird_ueberschrieben() {
        let s = Speicher::temporaer();
        let jetzt = zeit("2026-10-30T09:00:00+01:00");
        let hf = crate::pc::sysapps::Hotfix { id: "KB5066835".into(), description: "Security Update".into(), installed_on: "2026-10-15".into() };
        let e = ereignisse::alle(std::slice::from_ref(&hf), &[], &[], None);
        s.pc_ereignisse_einspielen(&e, &jetzt).unwrap();
        let u = UpdateRoh { zeit: zeit("2026-10-15T09:30:00+02:00"), titel: "2026-10 Kumulatives Update (KB5066835)".into() };
        let e = ereignisse::alle(&[hf], std::slice::from_ref(&u), &[], None);
        assert_eq!(s.pc_ereignisse_einspielen(&e, &jetzt).unwrap(), Eingespielt { neu: 0, bekannt: 1 });
        let alle = s.pc_ereignisse_lesen(false).unwrap();
        assert_eq!(alle.len(), 1);
        assert_eq!(alle[0].ereignis.genauigkeit, Genauigkeit::Minute);
        assert_eq!(alle[0].ereignis.kennung, "update:KB5066835:2026-10-15");
        // Ein anderes Update-Ereignis derselben KB am selben Tag, das erste
        // ist nicht mehr in der Anzeige: eigene Kennung mit Minute.
        let anders = UpdateRoh { zeit: zeit("2026-10-15T18:00:00+02:00"), titel: "2026-10 Kumulatives Update (KB5066835)".into() };
        s.pc_ereignisse_einspielen(&ereignisse::alle(&[], &[anders], &[], None), &jetzt).unwrap();
        assert_eq!(kennungen(&s), ["update:KB5066835:2026-10-15", "update:KB5066835:2026-10-15:18-00"]);
    }

    /// Älter als 90 Tage: ins Archiv, in einer Transaktion, nichts gelöscht.
    /// Ältere Ereignisse aus dem Sammler (alle Hotfixes) landen gleich dort,
    /// und ein zweiter Lauf legt sie nicht noch einmal an.
    #[test]
    fn archiv_ohne_loeschen() {
        let s = Speicher::temporaer();
        let tag1 = zeit("2026-10-30T09:00:00+01:00");
        let starts = [zeit("2026-08-15T08:00:00+02:00"), zeit("2026-10-29T08:00:00+01:00")];
        let hf = crate::pc::sysapps::Hotfix { id: "KB5000001".into(), description: "Update".into(), installed_on: "2025-01-10".into() };
        let e = ereignisse::alle(std::slice::from_ref(&hf), &[], &starts, None);
        assert_eq!(s.pc_ereignisse_einspielen(&e, &tag1).unwrap().neu, 3);
        assert_eq!(s.pc_ereignisse_lesen(false).unwrap().len(), 2, "der alte Hotfix steht gleich im Archiv");
        assert_eq!(s.pc_ereignisse_lesen(true).unwrap().len(), 3);
        assert_eq!(s.pc_ereignisse_archivieren(&tag1).unwrap(), 0, "noch nichts älter als 90 Tage");
        // Drei Wochen später ist der Start vom 15. August älter als 90 Tage.
        let spaeter = zeit("2026-11-20T09:00:00+01:00");
        assert_eq!(s.pc_ereignisse_archivieren(&spaeter).unwrap(), 1);
        assert_eq!(s.pc_ereignisse_archivieren(&spaeter).unwrap(), 0, "ein zweiter Lauf tut nichts");
        let haupt: Vec<String> = s.pc_ereignisse_lesen(false).unwrap().into_iter().map(|a| a.ereignis.kennung).collect();
        assert_eq!(haupt, ["start:2026-10-29T08:00:00+01:00"]);
        assert_eq!(kennungen(&s).len(), 3, "nichts gelöscht");
        // Der Sammler sieht dieselben noch einmal: nichts Neues, nichts doppelt.
        assert_eq!(s.pc_ereignisse_einspielen(&e, &spaeter).unwrap(), Eingespielt { neu: 0, bekannt: 3 });
        assert_eq!(kennungen(&s).len(), 3);
        assert_eq!(s.pc_ereignisse_lesen(false).unwrap().len(), 1);
        // Export und Sicherung enthalten beide Tabellen.
        let export: serde_json::Value = serde_json::from_str(&s.export().unwrap().als_json()).unwrap();
        assert_eq!(export["pc_ereignisse"].as_array().map(Vec::len), Some(1));
        assert_eq!(export["pc_ereignisse_archiv"].as_array().map(Vec::len), Some(2));
        assert!(export["pc_ereignisse_archiv"][0]["ereignis"]["quelle"].is_string(), "{export}");
    }
}
