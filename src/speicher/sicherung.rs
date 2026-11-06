//! Sicherung (Zielbild, Kapitel c3): Eine offene Datenbankdatei kann man nicht
//! gefahrlos kopieren. Darum schreibt der Server selbst aus einer
//! Lese-Transaktion eine Kopie `sicherung/dashboard-JJJJ-MM-TT.redb` und
//! daneben den Export als TOML. Die letzten 14 jeder Art bleiben.
//!
//! Vor einem Umbau heißt die Sicherung `dashboard-JJJJ-MM-TT-vor-schema-<alt>.redb`
//! (samt `.toml`); diese bleiben alle, die 14er-Regel lässt sie stehen.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Local, NaiveDate};
use redb::{Database, MultimapTableHandle, ReadTransaction, ReadableDatabase, TableHandle};
use tracing::{info, warn};

use super::tabellen::{self, META};
use super::{Speicher, SpeicherFehler, export};

/// So viele Sicherungen jeder Art bleiben.
pub const BEHALTEN: usize = 14;
/// Wie oft die nächtliche Sicherung nachsieht, ob heute schon gesichert wurde.
pub const PRUEFEN_ALLE: Duration = Duration::from_secs(60 * 60);

/// Wo die Sicherung liegt.
#[derive(Debug, Clone)]
pub struct Sicherung {
    pub datei: PathBuf,
    pub export: PathBuf,
    /// Tag der Sicherung, JJJJ-MM-TT.
    pub datum: String,
}

impl Speicher {
    /// Sichert unter dem heutigen Datum (Ortszeit).
    pub fn sichern(&self) -> Result<Sicherung, SpeicherFehler> {
        self.sichern_als(&heute(), None)
    }

    /// Sicherung vor einem Umbau von Schema `alt`: eigener Name, wird nie aufgeräumt.
    pub fn sichern_vor_schema(&self, alt: u32) -> Result<Sicherung, SpeicherFehler> {
        self.sichern_als(&heute(), Some(format!("schema-{alt}")))
    }

    /// Sicherung vor dem Umbau einer Tabelle eines Backend-Moduls (Etappe 4b):
    /// `dashboard-JJJJ-MM-TT-vor-<zusatz>.redb`, `zusatz` nur aus a–z, 0–9 und
    /// Bindestrich (etwa `projekt-architektur-verlauf-1`); wird nie aufgeräumt.
    pub fn sichern_vor(&self, zusatz: &str) -> Result<Sicherung, SpeicherFehler> {
        if !ist_zusatz(zusatz) {
            return Err(SpeicherFehler::Hintergrund(format!("Name der Sicherung ungültig: {zusatz:?}")));
        }
        self.sichern_als(&heute(), Some(zusatz.to_string()))
    }

    /// Sichert unter `datum` (JJJJ-MM-TT), mit `vor_schema` unter dem Namen für
    /// den Umbau. Eine Sicherung desselben Namens wird ersetzt; geschrieben wird
    /// erst in `.tmp`, dann umbenannt. Zwei Läufe zugleich warten aufeinander.
    fn sichern_als(&self, datum: &str, vor: Option<String>) -> Result<Sicherung, SpeicherFehler> {
        // Ein vergifteter Riegel (Absturz in einem früheren Lauf) sperrt nicht für immer.
        let _riegel = self.sicherungs_riegel.lock().unwrap_or_else(|e| e.into_inner());
        std::fs::create_dir_all(&self.sicherungs_ordner)?;
        let stamm = match vor {
            None => format!("dashboard-{datum}"),
            Some(zusatz) => format!("dashboard-{datum}-vor-{zusatz}"),
        };
        let datei = self.sicherungs_ordner.join(format!("{stamm}.redb"));
        let export_datei = self.sicherungs_ordner.join(format!("{stamm}.toml"));
        let tmp_datei = self.sicherungs_ordner.join(format!("{stamm}.redb.tmp"));
        let tmp_export = self.sicherungs_ordner.join(format!("{stamm}.toml.tmp"));
        let ergebnis = (|| {
            let txn = self.db.begin_read()?;
            kopie_schreiben(&txn, &tmp_datei)?;
            let toml = export::aus_transaktion(&txn)?.als_toml();
            drop(txn);
            std::fs::write(&tmp_export, toml)?;
            std::fs::rename(&tmp_datei, &datei)?;
            std::fs::rename(&tmp_export, &export_datei)?;
            Ok::<(), SpeicherFehler>(())
        })();
        if let Err(e) = ergebnis {
            let _ = std::fs::remove_file(&tmp_datei);
            let _ = std::fs::remove_file(&tmp_export);
            return Err(e);
        }
        let txn = self.db.begin_write()?;
        txn.open_table(META)?.insert(tabellen::META_LETZTE_SICHERUNG, datum)?;
        self.festschreiben(txn, &[super::Bereich::Sicherung])?;
        for endung in ["redb", "toml"] {
            if let Err(e) = aufraeumen(&self.sicherungs_ordner, endung) {
                warn!("Alte Sicherungen (.{endung}) nicht aufgeräumt: {e}");
            }
        }
        info!("Datenspeicher gesichert: {}", datei.display());
        Ok(Sicherung { datei, export: export_datei, datum: datum.to_string() })
    }

    /// Datum der letzten Sicherung (JJJJ-MM-TT) aus `meta`.
    pub fn letzte_sicherung(&self) -> Option<String> {
        match self.meta_lesen(tabellen::META_LETZTE_SICHERUNG) {
            Ok(d) => d,
            Err(e) => {
                warn!("Letzte Sicherung nicht lesbar: {}", e.einzelheiten());
                None
            }
        }
    }
}

/// Heutiges Datum in Ortszeit, JJJJ-MM-TT.
pub fn heute() -> String {
    chrono::Local::now().date_naive().format("%Y-%m-%d").to_string()
}

/// Neue redb-Datei mit allen Tabellen aus `tabellen::ALLE` und allen
/// Tabellen der Backend-Module aus dem Verzeichnis (Etappe 4b).
fn kopie_schreiben(txn: &ReadTransaction, ziel: &Path) -> Result<(), SpeicherFehler> {
    tabellen_pruefen(txn)?;
    let _ = std::fs::remove_file(ziel);
    let kopie = Database::create(ziel).map_err(|e| SpeicherFehler::Datenbank(e.into()))?;
    let schreiben = kopie.begin_write()?;
    for t in tabellen::ALLE {
        (t.kopieren)(txn, &schreiben)?;
    }
    for t in super::projekte::verzeichnis(txn)? {
        let voll = t.voller_name();
        tabellen::kopieren(super::projekte::definition(&voll), txn, &schreiben)?;
    }
    schreiben.commit()?;
    Ok(())
}

/// Bricht ab, wenn die Datei Tabellen hat, die weder in `tabellen::ALLE`
/// noch im Verzeichnis der Modul-Tabellen stehen: eine Sicherung ohne sie wäre
/// unvollständig, ausgerechnet vor einem Umbau.
fn tabellen_pruefen(txn: &ReadTransaction) -> Result<(), SpeicherFehler> {
    let module: Vec<String> = super::projekte::verzeichnis(txn)?.iter().map(|t| t.voller_name()).collect();
    let unbekannt: Vec<String> = txn
        .list_tables()?
        .map(|t| t.name().to_string())
        .chain(txn.list_multimap_tables()?.map(|t| t.name().to_string()))
        .filter(|name| !tabellen::ALLE.iter().any(|t| t.name == name) && !module.contains(name))
        .collect();
    if !unbekannt.is_empty() {
        return Err(SpeicherFehler::Hintergrund(format!("Sicherung abgebrochen: Tabellen {unbekannt:?} stehen nicht in tabellen::ALLE")));
    }
    Ok(())
}

/// Zusatz im Namen einer Sicherung vor einem Umbau: a–z, 0–9, Bindestrich.
fn ist_zusatz(zusatz: &str) -> bool {
    !zusatz.is_empty() && zusatz.len() <= 120 && zusatz.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Ob `datum` die Form JJJJ-MM-TT hat.
fn ist_datum(datum: &str) -> bool {
    datum.len() == 10 && datum.bytes().enumerate().all(|(i, b)| if i == 4 || i == 7 { b == b'-' } else { b.is_ascii_digit() })
}

/// Ob `name` genau `dashboard-JJJJ-MM-TT.<endung>` ist.
fn ist_sicherung(name: &str, endung: &str) -> bool {
    let Some(datum) = name.strip_prefix("dashboard-").and_then(|r| r.strip_suffix(endung)) else {
        return false;
    };
    let Some(datum) = datum.strip_suffix('.') else { return false };
    ist_datum(datum)
}

/// Ob `name` eine liegengebliebene Zwischendatei einer Sicherung ist:
/// `dashboard-JJJJ-MM-TT[-vor-schema-N].<endung>.tmp` (seit 4b auch
/// `-vor-<zusatz>` aus a–z, 0–9 und Bindestrich).
fn ist_rest(name: &str, endung: &str) -> bool {
    let Some(stamm) =
        name.strip_prefix("dashboard-").and_then(|r| r.strip_suffix(".tmp")).and_then(|r| r.strip_suffix(endung)).and_then(|r| r.strip_suffix('.'))
    else {
        return false;
    };
    let (datum, rest) = stamm.split_at_checked(10).unwrap_or((stamm, ""));
    ist_datum(datum)
        && (rest.is_empty()
            || rest
                .strip_prefix("-vor-")
                .is_some_and(|n| ist_zusatz(n) && n.strip_prefix("schema-").is_none_or(|z| !z.is_empty() && z.bytes().all(|b| b.is_ascii_digit()))))
}

/// Eine Sicherung im Ordner (für den Verlauf, Etappe 2e).
#[derive(Debug, Clone, PartialEq)]
pub struct SicherungsDatei {
    /// Dateiname, etwa `dashboard-2026-10-30.redb`.
    pub name: String,
    /// Wann sie geschrieben wurde (Änderungszeit der Datei, sonst Mitternacht des Datums im Namen).
    pub zeit: DateTime<Local>,
    /// Bei einer Sicherung vor einem Umbau der Zusatz (`schema-8`); bei der täglichen `None`.
    pub vor: Option<String>,
}

/// Die Sicherungen (`.redb`) im Ordner: tägliche (`dashboard-JJJJ-MM-TT.redb`)
/// und vor einem Umbau (`dashboard-JJJJ-MM-TT-vor-<zusatz>.redb`). Andere
/// Dateien, `.toml` und `.tmp` zählen nicht. Fehlt der Ordner, ist die Liste leer.
pub fn sicherungen_im_ordner(ordner: &Path) -> std::io::Result<Vec<SicherungsDatei>> {
    let eintraege = match std::fs::read_dir(ordner) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut aus = Vec::new();
    for e in eintraege.filter_map(Result::ok).filter(|e| e.file_type().is_ok_and(|t| t.is_file())) {
        let Ok(name) = e.file_name().into_string() else { continue };
        let Some(stamm) = name.strip_prefix("dashboard-").and_then(|r| r.strip_suffix(".redb")) else { continue };
        let (datum, rest) = stamm.split_at_checked(10).unwrap_or((stamm, ""));
        if !ist_datum(datum) {
            continue;
        }
        let vor = match rest.strip_prefix("-vor-") {
            None if rest.is_empty() => None,
            Some(z) if ist_zusatz(z) => Some(z.to_string()),
            _ => continue,
        };
        let geaendert = e.metadata().and_then(|m| m.modified()).ok().map(DateTime::<Local>::from);
        let zeit = match geaendert {
            Some(z) => z,
            None => {
                let Ok(tag) = NaiveDate::parse_from_str(datum, "%Y-%m-%d") else { continue };
                crate::vertrag::verlauf::tagesanfang(tag)
            }
        };
        aus.push(SicherungsDatei { name, zeit, vor });
    }
    Ok(aus)
}

/// Behält von den Sicherungen mit `endung` nur die letzten `BEHALTEN` (nach
/// Dateiname, also nach Datum) und löscht liegengebliebene `.tmp` dieser Art,
/// gleich welchen Datums. Sicherungen vor einem Umbau und andere Dateien
/// bleiben unberührt.
pub(super) fn aufraeumen(ordner: &Path, endung: &str) -> std::io::Result<usize> {
    let alle: Vec<String> = std::fs::read_dir(ordner)?
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    let mut geloescht = 0;
    for rest in alle.iter().filter(|n| ist_rest(n, endung)) {
        std::fs::remove_file(ordner.join(rest))?;
        geloescht += 1;
    }
    let mut namen: Vec<&String> = alle.iter().filter(|n| ist_sicherung(n, endung)).collect();
    namen.sort();
    let zu_viele = namen.len().saturating_sub(BEHALTEN);
    for name in &namen[..zu_viele] {
        std::fs::remove_file(ordner.join(name))?;
    }
    Ok(geloescht + zu_viele)
}

/// Farbe für /daten: „ok“ = heute oder gestern, „warn“ = 2 bis 7 Tage alt,
/// „bad“ = älter oder nie.
pub fn klasse(letzte: Option<&str>, heute: NaiveDate) -> &'static str {
    let Some(datum) = letzte.and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()) else {
        return "bad";
    };
    match (heute - datum).num_days() {
        ..=1 => "ok",
        2..=7 => "warn",
        _ => "bad",
    }
}

/// Nächtliche Sicherung: beim Start und dann jede Stunde nachsehen, ob heute
/// schon gesichert wurde; sonst sichern. Fehler gehen nur ins Protokoll.
pub fn spawn_naechtlich(speicher: Arc<Speicher>) {
    tokio::spawn(async move {
        let mut takt = tokio::time::interval(PRUEFEN_ALLE);
        takt.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            takt.tick().await;
            if speicher.letzte_sicherung().as_deref() == Some(heute().as_str()) {
                continue;
            }
            if let Err(e) = Speicher::im_hintergrund(speicher.clone(), |s| s.sichern()).await {
                warn!("Nächtliche Sicherung fehlgeschlagen: {e} ({})", e.einzelheiten());
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speicher::tabellen::{PC_VERLAUF, PcMesswert, zeit_schluessel};
    use redb::{ReadOnlyDatabase, TableDefinition};

    /// Auflage aus der Abnahme 2a: jede Tabelle aus `ALLE` steht in der
    /// Sicherungskopie als Tabelle und im Export als Feld (außer bewusst
    /// ausgelassenen heiklen Tabellen, die dort dann auch fehlen müssen).
    #[test]
    fn sicherung_enthaelt_jede_tabelle() {
        let s = Speicher::temporaer();
        let sicherung = s.sichern().unwrap();
        let db = ReadOnlyDatabase::open(&sicherung.datei).unwrap();
        let txn = db.begin_read().unwrap();
        let in_kopie: Vec<String> =
            txn.list_tables().unwrap().map(|t| t.name().to_string()).chain(txn.list_multimap_tables().unwrap().map(|t| t.name().to_string())).collect();
        let export: toml::Table = toml::from_str(&std::fs::read_to_string(&sicherung.export).unwrap()).unwrap();
        let direkt: serde_json::Value = serde_json::from_str(&s.export().unwrap().als_json()).unwrap();
        for t in tabellen::ALLE {
            assert!(in_kopie.iter().any(|n| n == t.name), "{} fehlt in der Kopie: {in_kopie:?}", t.name);
            let erwartet = t.exportieren.is_some();
            assert_eq!(export.contains_key(t.name), erwartet, "{} im TOML-Export: {export:?}", t.name);
            assert_eq!(direkt.get(t.name).is_some(), erwartet, "{} im JSON-Export", t.name);
        }
        assert_eq!(in_kopie.len(), tabellen::ALLE.len(), "keine Tabelle zu viel: {in_kopie:?}");
    }

    #[test]
    fn unbekannte_tabelle_bricht_die_sicherung_ab() {
        let s = Speicher::temporaer();
        const FREMD: TableDefinition<&str, &str> = TableDefinition::new("fremd");
        let txn = s.db.begin_write().unwrap();
        txn.open_table(FREMD).unwrap().insert("x", "y").unwrap();
        txn.commit().unwrap();
        let e = s.sichern().unwrap_err();
        assert!(matches!(e, SpeicherFehler::Hintergrund(_)), "{e:?}");
        assert!(e.einzelheiten().contains("\"fremd\""), "{}", e.einzelheiten());
        assert!(s.letzte_sicherung().is_none());
        let reste: Vec<_> = std::fs::read_dir(s.sicherungs_ordner()).unwrap().filter_map(Result::ok).collect();
        assert!(reste.is_empty(), "keine halbe Sicherung: {reste:?}");
    }

    #[tokio::test]
    async fn kopie_enthaelt_den_messwert() {
        let s = Speicher::temporaer();
        let m = PcMesswert { zeit: zeit_schluessel(chrono::Utc::now()), cpu: Some(42.0), mem: 10.0, platte_belegt: Some(5), platte_gesamt: Some(9) };
        s.pc_messwert_anhaengen(m.clone()).await.unwrap();
        let sicherung = s.sichern().unwrap();
        assert_eq!(s.letzte_sicherung(), Some(heute()));
        assert!(sicherung.export.is_file());
        assert!(std::fs::read_to_string(&sicherung.export).unwrap().contains("cpu = 42"));
        // Ein zweites Mal am selben Tag ersetzt die Kopie.
        let sicherung = s.sichern().unwrap();
        let db = ReadOnlyDatabase::open(&sicherung.datei).unwrap();
        let txn = db.begin_read().unwrap();
        let tabelle = txn.open_table(PC_VERLAUF).unwrap();
        let wert = tabelle.get(m.zeit.as_str()).unwrap().expect("Messwert in der Kopie");
        let gelesen: PcMesswert = serde_json::from_slice(wert.value()).unwrap();
        assert_eq!(gelesen, m);
        let meta = txn.open_table(META).unwrap();
        assert_eq!(meta.get(tabellen::META_SCHEMA).unwrap().unwrap().value(), crate::speicher::migration::SCHEMA_AKTUELL.to_string());
        let reste: Vec<_> = std::fs::read_dir(s.sicherungs_ordner()).unwrap().filter_map(Result::ok).collect();
        assert_eq!(reste.len(), 2, "keine .tmp-Reste: {reste:?}");
    }

    #[test]
    fn aufraeumen_behaelt_vierzehn_und_fremde_dateien() {
        let s = Speicher::temporaer();
        let ordner = s.sicherungs_ordner().to_path_buf();
        std::fs::create_dir_all(&ordner).unwrap();
        for tag in 1..=16 {
            std::fs::write(ordner.join(format!("dashboard-2026-09-{tag:02}.redb")), "x").unwrap();
        }
        std::fs::write(ordner.join("notizen.redb"), "fremd").unwrap();
        std::fs::write(ordner.join("dashboard-2026-09-01.redb.alt"), "fremd").unwrap();
        assert_eq!(aufraeumen(&ordner, "redb").unwrap(), 2);
        let mut namen: Vec<String> = std::fs::read_dir(&ordner).unwrap().filter_map(|e| e.ok()?.file_name().into_string().ok()).collect();
        namen.sort();
        assert_eq!(namen.len(), 14 + 2);
        assert!(!namen.contains(&"dashboard-2026-09-01.redb".to_string()));
        assert!(!namen.contains(&"dashboard-2026-09-02.redb".to_string()));
        assert!(namen.contains(&"dashboard-2026-09-03.redb".to_string()));
        assert!(namen.contains(&"notizen.redb".to_string()));
        assert!(namen.contains(&"dashboard-2026-09-01.redb.alt".to_string()));
    }

    #[test]
    fn umbau_sicherungen_bleiben_und_reste_verschwinden() {
        let s = Speicher::temporaer();
        let ordner = s.sicherungs_ordner().to_path_buf();
        std::fs::create_dir_all(&ordner).unwrap();
        for tag in 1..=16 {
            std::fs::write(ordner.join(format!("dashboard-2026-09-{tag:02}.redb")), "x").unwrap();
            std::fs::write(ordner.join(format!("dashboard-2026-09-{tag:02}-vor-schema-1.redb")), "x").unwrap();
        }
        std::fs::write(ordner.join("dashboard-2020-01-01.redb.tmp"), "alt").unwrap();
        std::fs::write(ordner.join("dashboard-2020-01-01-vor-schema-3.redb.tmp"), "alt").unwrap();
        std::fs::write(ordner.join("dashboard-2020-01-01.toml.tmp"), "andere Art").unwrap();
        std::fs::write(ordner.join("notizen.redb.tmp"), "fremd").unwrap();
        assert_eq!(aufraeumen(&ordner, "redb").unwrap(), 2 + 2);
        let namen: Vec<String> = std::fs::read_dir(&ordner).unwrap().filter_map(|e| e.ok()?.file_name().into_string().ok()).collect();
        assert_eq!(namen.iter().filter(|n| n.contains("-vor-schema-") && n.ends_with(".redb")).count(), 16);
        assert!(!namen.iter().any(|n| n.ends_with(".redb.tmp") && n.starts_with("dashboard-")), "{namen:?}");
        assert!(namen.contains(&"dashboard-2020-01-01.toml.tmp".to_string()));
        assert!(namen.contains(&"notizen.redb.tmp".to_string()));
        // Die Sicherung vor einem Umbau trägt ihren eigenen Namen.
        let vor = s.sichern_vor_schema(1).unwrap();
        let name = vor.datei.file_name().unwrap().to_str().unwrap().to_string();
        assert_eq!(name, format!("dashboard-{}-vor-schema-1.redb", heute()));
        assert!(vor.export.is_file());
        assert!(ist_rest("dashboard-2026-10-25-vor-schema-12.toml.tmp", "toml"));
        assert!(!ist_rest("dashboard-2026-10-25-vor-schema-.redb.tmp", "redb"));
        assert!(!ist_rest("dashboard-2026-10-25x.redb.tmp", "redb"));
    }

    #[test]
    fn zwei_sicherungen_zugleich_warten_aufeinander() {
        let s = Arc::new(Speicher::temporaer());
        let laeufe: Vec<_> = (0..4)
            .map(|_| {
                let s = s.clone();
                std::thread::spawn(move || s.sichern().map(|_| ()))
            })
            .collect();
        for lauf in laeufe {
            lauf.join().unwrap().unwrap();
        }
        let namen: Vec<String> = std::fs::read_dir(s.sicherungs_ordner()).unwrap().filter_map(|e| e.ok()?.file_name().into_string().ok()).collect();
        assert_eq!(namen.len(), 2, "eine Kopie, ein Export, keine Reste: {namen:?}");
    }

    #[test]
    fn sicherungen_fuer_den_verlauf() {
        let ordner = std::env::temp_dir().join(format!("dashboard-test-{}-sicherungen-liste", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        assert_eq!(sicherungen_im_ordner(&ordner).unwrap(), [], "fehlt der Ordner: leer");
        std::fs::create_dir_all(&ordner).unwrap();
        for name in [
            "dashboard-2026-10-29.redb",
            "dashboard-2026-10-29.toml",
            "dashboard-2026-10-30-vor-schema-8.redb",
            "dashboard-2026-10-30.redb.tmp",
            "dashboard-kaputt.redb",
            "notizen.redb",
        ] {
            std::fs::write(ordner.join(name), "x").unwrap();
        }
        let mut l = sicherungen_im_ordner(&ordner).unwrap();
        l.sort_by(|a, b| a.name.cmp(&b.name));
        let namen: Vec<(&str, Option<&str>)> = l.iter().map(|s| (s.name.as_str(), s.vor.as_deref())).collect();
        assert_eq!(namen, [("dashboard-2026-10-29.redb", None), ("dashboard-2026-10-30-vor-schema-8.redb", Some("schema-8"))]);
        assert!(l.iter().all(|s| (Local::now() - s.zeit).num_minutes().abs() < 10), "Zeit der Datei");
        std::fs::remove_dir_all(&ordner).unwrap();
    }

    #[test]
    fn namensmuster_genau() {
        assert!(ist_sicherung("dashboard-2026-10-25.redb", "redb"));
        assert!(ist_sicherung("dashboard-2026-10-25.toml", "toml"));
        assert!(!ist_sicherung("dashboard-2026-10-25.toml", "redb"));
        assert!(!ist_sicherung("dashboard-2026-1-25.redb", "redb"));
        assert!(!ist_sicherung("dashboard-2026-10-25x.redb", "redb"));
        assert!(!ist_sicherung("dashboard-2026-10-25.redb.tmp", "redb"));
    }

    #[test]
    fn klasse_nach_alter() {
        let heute = NaiveDate::from_ymd_opt(2026, 10, 25).unwrap();
        assert_eq!(klasse(Some("2026-10-25"), heute), "ok");
        assert_eq!(klasse(Some("2026-10-24"), heute), "ok");
        assert_eq!(klasse(Some("2026-10-23"), heute), "warn");
        assert_eq!(klasse(Some("2026-10-18"), heute), "warn");
        assert_eq!(klasse(Some("2026-10-17"), heute), "bad");
        assert_eq!(klasse(None, heute), "bad");
        assert_eq!(klasse(Some("kaputt"), heute), "bad");
    }
}
