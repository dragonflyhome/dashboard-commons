//! Schema-Fassung und Umbau (Zielbild, Kapitel c3, „Migrationen“).
//!
//! In `meta` steht `schema` und je Tabelle `fassung.<tabelle>`. Beim Öffnen:
//! - kein `schema`: neue Datei, Tabellen anlegen, Fassungen eintragen;
//! - höher als dieses Programm kennt: ablehnen, Datei bleibt unberührt;
//! - niedriger: erst Sicherung, dann die fehlenden Schritte aus `SCHRITTE`,
//!   jeder in einer eigenen Transaktion, die auch `schema` hochsetzt.

use std::path::Path;

use redb::{Database, ReadOnlyDatabase, ReadableDatabase, WriteTransaction};
use tracing::{debug, info, warn};

use super::tabellen::{
    self, ANTWORTEN, ANTWORTEN_ARCHIV, ARBEITEN, ARBEITEN_ARCHIV, AUFGABEN, AUFGABEN_ARCHIV, AUFGABEN_LINKS, EINSTUFUNGEN, EREIGNISSE, JWT_SPERRLISTE,
    MELDUNGEN, MELDUNGEN_ARCHIV, META, PASSKEY_ANTRAEGE, PASSKEY_AUFGABEN, PASSKEYS, PC_EREIGNISSE, PC_EREIGNISSE_ARCHIV, PC_VERLAUF, PROJEKT_TABELLEN,
    VORGAENGE, VORGAENGE_JE_SLUG, WARTER,
};
use super::{Speicher, SpeicherFehler};

/// Schema-Fassung, die dieses Programm schreibt und versteht.
/// 1: `meta`, `pc_verlauf` (Etappe 2a). 2: dazu die Vorgänge (Etappe 2b).
/// 3: dazu die Aufgaben für den Inhaber und ihre verschlüsselten Links (Etappe 5a).
/// 4: dazu Arbeiten der Chats, ihr Archiv und die Einstufungen des Inhabers (Etappe A1).
/// 5: dazu die Antworten des Inhabers auf Fragen und ihr Archiv (Etappe A2).
/// 6: dazu Meldungen und ihr Archiv (Etappe A3).
/// 7: dazu das Verzeichnis der Tabellen der Backend-Module (Etappe 4b); deren
///    Tabellen `projekt/<slug>/<name>` legt `speicher::projekte` beim Start an.
///    Ältere Programme öffnen die Datei dann nicht mehr (ihre Sicherung kennte
///    die Modul-Tabellen nicht), und Vorgänge der Art `merge` sind ihnen fremd.
/// 8: dazu Passkeys, ihre Challenges, Anträge und die Sperrliste der Ausweise
///    (Etappe 7c). Vorgänge der Art `passkey-antrag` sind älteren Programmen fremd.
/// 9: dazu die Windows-Ereignisse für den Verlauf und ihr Archiv (Etappe 2e).
/// 10: Arbeiten in Fassung 2 mit `besitzer` und `schritte` (Zweig teilschritte).
///    Ältere Programme öffnen die Datei nicht mehr: sie würden Schritte beim
///    Zurückschreiben still verlieren.
pub const SCHEMA_AKTUELL: u32 = 10;

/// Ein Umbauschritt: hebt die Datei von `nach - 1` auf `nach`.
pub struct Schritt {
    pub nach: u32,
    pub lauf: fn(&WriteTransaction) -> Result<(), SpeicherFehler>,
}

/// Alle Umbauschritte, aufsteigend.
pub const SCHRITTE: &[Schritt] = &[
    Schritt { nach: 2, lauf: migration_2 },
    Schritt { nach: 3, lauf: migration_3 },
    Schritt { nach: 4, lauf: migration_4 },
    Schritt { nach: 5, lauf: migration_5 },
    Schritt { nach: 6, lauf: migration_6 },
    Schritt { nach: 7, lauf: migration_7 },
    Schritt { nach: 8, lauf: migration_8 },
    Schritt { nach: 9, lauf: migration_9 },
    Schritt { nach: 10, lauf: migration_10 },
];

/// Schema 1 → 2: die Tabellen der Vorgänge anlegen (leer) und ihre Fassungen eintragen.
fn migration_2(txn: &WriteTransaction) -> Result<(), SpeicherFehler> {
    vorgangs_tabellen_anlegen(txn)
}

/// Schema 2 → 3: die Tabellen der Aufgaben anlegen (leer) und ihre Fassungen
/// eintragen. Alles andere in der Datei bleibt, wie es ist.
fn migration_3(txn: &WriteTransaction) -> Result<(), SpeicherFehler> {
    aufgaben_tabellen_anlegen(txn)
}

/// Schema 3 → 4: `arbeiten`, `arbeiten_archiv` und `einstufungen` anlegen
/// (leer) und ihre Fassungen eintragen. Aufgaben bekommen ihr neues Feld
/// `prioritaet` nicht hier: es ist freiwillig, alte Einträge gelten als `bald`.
fn migration_4(txn: &WriteTransaction) -> Result<(), SpeicherFehler> {
    anstehend_tabellen_anlegen(txn)
}

/// Schema 4 → 5: `antworten` und `antworten_archiv` anlegen (leer) und ihre
/// Fassungen eintragen. Alles andere in der Datei bleibt, wie es ist.
fn migration_5(txn: &WriteTransaction) -> Result<(), SpeicherFehler> {
    antworten_tabellen_anlegen(txn)
}

/// Schema 5 → 6: `meldungen` und `meldungen_archiv` anlegen (leer) und ihre
/// Fassungen eintragen. Alles andere in der Datei bleibt, wie es ist.
fn migration_6(txn: &WriteTransaction) -> Result<(), SpeicherFehler> {
    meldungen_tabellen_anlegen(txn)
}

/// Schema 6 → 7: das Verzeichnis `projekt_tabellen` anlegen (leer) und seine
/// Fassung eintragen. Alles andere in der Datei bleibt, wie es ist.
fn migration_7(txn: &WriteTransaction) -> Result<(), SpeicherFehler> {
    projekt_tabellen_anlegen(txn)
}

/// Schema 7 → 8: `passkeys`, `passkey_aufgaben`, `passkey_antraege` und
/// `jwt_sperrliste` anlegen (leer) und ihre Fassungen eintragen. Alles andere
/// in der Datei bleibt, wie es ist.
fn migration_8(txn: &WriteTransaction) -> Result<(), SpeicherFehler> {
    passkey_tabellen_anlegen(txn)
}

/// Schema 8 → 9: `pc_ereignisse` und `pc_ereignisse_archiv` anlegen (leer)
/// und ihre Fassungen eintragen. Alles andere in der Datei bleibt, wie es ist.
fn migration_9(txn: &WriteTransaction) -> Result<(), SpeicherFehler> {
    pc_ereignis_tabellen_anlegen(txn)
}

/// Schema 9 → 10: laufende und wartende Arbeiten bekommen `besitzer = von`;
/// sonst wird nichts umgeschrieben (das Archiv hat nur fertige). Danach die
/// Fassung 2 für `arbeiten` und `arbeiten_archiv`. Unlesbare Einträge bleiben,
/// wie sie sind, und stehen im Protokoll.
fn migration_10(txn: &WriteTransaction) -> Result<(), SpeicherFehler> {
    use crate::api::vertrag::{Arbeit, ArbeitsStand};
    let mut arbeiten = txn.open_table(ARBEITEN)?;
    let mut neu: Vec<(String, Vec<u8>)> = Vec::new();
    for eintrag in redb::ReadableTable::iter(&arbeiten)? {
        let (k, w) = eintrag?;
        let id = k.value().to_string();
        match serde_json::from_slice::<Arbeit>(w.value()) {
            Ok(mut a) if a.besitzer.is_none() && matches!(a.stand, ArbeitsStand::Laeuft | ArbeitsStand::Wartet) => {
                a.besitzer = Some(a.von.clone());
                neu.push((id, serde_json::to_vec(&a)?));
            }
            Ok(_) => {}
            Err(e) => warn!("arbeiten/{id}: beim Umbau auf Schema 10 unlesbar, bleibt unverändert ({e})"),
        }
    }
    for (id, json) in neu {
        arbeiten.insert(id.as_str(), json.as_slice())?;
    }
    fassungen_eintragen(
        txn,
        &[
            // Fest 2: ein späterer Schritt hebt selbst weiter.
            ("arbeiten", tabellen::META_FASSUNG_ARBEITEN, 2),
            ("arbeiten_archiv", tabellen::META_FASSUNG_ARBEITEN_ARCHIV, 2),
        ],
    )
}

/// Legt `pc_ereignisse` und `pc_ereignisse_archiv` an und trägt ihre Fassungen ein.
fn pc_ereignis_tabellen_anlegen(txn: &WriteTransaction) -> Result<(), SpeicherFehler> {
    txn.open_table(PC_EREIGNISSE)?;
    txn.open_table(PC_EREIGNISSE_ARCHIV)?;
    fassungen_eintragen(txn, tabellen::FASSUNGEN_PC_EREIGNISSE)
}

/// Legt die vier Tabellen der Passkeys an und trägt ihre Fassungen ein.
fn passkey_tabellen_anlegen(txn: &WriteTransaction) -> Result<(), SpeicherFehler> {
    txn.open_table(PASSKEYS)?;
    txn.open_table(PASSKEY_AUFGABEN)?;
    txn.open_table(PASSKEY_ANTRAEGE)?;
    txn.open_table(JWT_SPERRLISTE)?;
    fassungen_eintragen(txn, tabellen::FASSUNGEN_PASSKEY)
}

/// Legt `projekt_tabellen` an und trägt seine Fassung ein.
fn projekt_tabellen_anlegen(txn: &WriteTransaction) -> Result<(), SpeicherFehler> {
    txn.open_table(PROJEKT_TABELLEN)?;
    fassungen_eintragen(txn, tabellen::FASSUNGEN_PROJEKTE)
}

/// Legt `meldungen` und `meldungen_archiv` an und trägt ihre Fassungen ein.
fn meldungen_tabellen_anlegen(txn: &WriteTransaction) -> Result<(), SpeicherFehler> {
    txn.open_table(MELDUNGEN)?;
    txn.open_table(MELDUNGEN_ARCHIV)?;
    fassungen_eintragen(txn, tabellen::FASSUNGEN_MELDUNGEN)
}

/// Legt `antworten` und `antworten_archiv` an und trägt ihre Fassungen ein.
fn antworten_tabellen_anlegen(txn: &WriteTransaction) -> Result<(), SpeicherFehler> {
    txn.open_table(ANTWORTEN)?;
    txn.open_table(ANTWORTEN_ARCHIV)?;
    fassungen_eintragen(txn, tabellen::FASSUNGEN_ANTWORTEN)
}

/// Legt `arbeiten`, `arbeiten_archiv` und `einstufungen` an und trägt ihre Fassungen ein.
fn anstehend_tabellen_anlegen(txn: &WriteTransaction) -> Result<(), SpeicherFehler> {
    txn.open_table(ARBEITEN)?;
    txn.open_table(ARBEITEN_ARCHIV)?;
    txn.open_table(EINSTUFUNGEN)?;
    fassungen_eintragen(txn, tabellen::FASSUNGEN_ANSTEHEND)
}

/// Legt `ereignisse`, `vorgaenge`, `vorgaenge_je_slug` und `warter` an und trägt ihre Fassungen ein.
fn vorgangs_tabellen_anlegen(txn: &WriteTransaction) -> Result<(), SpeicherFehler> {
    txn.open_table(EREIGNISSE)?;
    txn.open_table(VORGAENGE)?;
    txn.open_table(VORGAENGE_JE_SLUG)?;
    txn.open_table(WARTER)?;
    fassungen_eintragen(txn, tabellen::FASSUNGEN_VORGAENGE)
}

/// Legt `aufgaben`, `aufgaben_links` und `aufgaben_archiv` an und trägt ihre Fassungen ein.
fn aufgaben_tabellen_anlegen(txn: &WriteTransaction) -> Result<(), SpeicherFehler> {
    txn.open_table(AUFGABEN)?;
    txn.open_table(AUFGABEN_LINKS)?;
    txn.open_table(AUFGABEN_ARCHIV)?;
    fassungen_eintragen(txn, tabellen::FASSUNGEN_AUFGABEN)
}

fn fassungen_eintragen(txn: &WriteTransaction, fassungen: &[(&str, &str, u32)]) -> Result<(), SpeicherFehler> {
    let mut meta = txn.open_table(META)?;
    for (_, schluessel, fassung) in fassungen {
        meta.insert(*schluessel, fassung.to_string().as_str())?;
    }
    Ok(())
}

/// Liest `schema` aus einer offenen Datenbank; fehlt es, `None`.
pub(super) fn schema_lesen<D: ReadableDatabase>(db: &D) -> Result<Option<u32>, SpeicherFehler> {
    let txn = db.begin_read()?;
    match super::meta_aus(&txn, tabellen::META_SCHEMA)? {
        None => Ok(None),
        Some(text) => text.parse().map(Some).map_err(|_| SpeicherFehler::Beschaedigt(format!("schema = {text:?} ist keine Zahl"))),
    }
}

/// Vor dem Öffnen zum Schreiben: eine bestehende Datei nur lesend öffnen und
/// ablehnen, wenn ihr Schema zu neu ist.
///
/// Nach einem unsauberen Ende (Absturz, Strom weg) lässt redb die Datei nur
/// lesend nicht öffnen (`RepairAborted`), und das Öffnen zum Schreiben würde
/// sie reparieren, also hineinschreiben. Darum dann eine Kopie daneben
/// (`<datei>.pruefung-<pid>.tmp`) reparierend öffnen, dort das Schema lesen
/// und die Kopie löschen; das Original bleibt bis zur Entscheidung unberührt.
pub(super) fn vorab_pruefen(pfad: &Path, ziel: u32) -> Result<(), SpeicherFehler> {
    let leer = std::fs::metadata(pfad).map(|m| m.len() == 0).unwrap_or(true);
    if leer {
        return Ok(());
    }
    let schema = match ReadOnlyDatabase::open(pfad) {
        Ok(db) => schema_lesen(&db)?,
        Err(redb::DatabaseError::DatabaseAlreadyOpen) => {
            return Err(SpeicherFehler::Belegt { datei: pfad.to_path_buf() });
        }
        Err(e) => {
            debug!("{}: lesend nicht zu öffnen ({e}), Prüfung an einer Kopie", pfad.display());
            schema_an_kopie(pfad)?
        }
    };
    match schema {
        Some(schema) if schema > ziel => Err(SpeicherFehler::ZuNeu { datei: pfad.to_path_buf(), schema }),
        _ => Ok(()),
    }
}

/// Name der Prüfkopie dieses Prozesses: `<datei>.pruefung-<pid>.tmp`.
fn pruefkopie_name(datei: &str, pid: u32) -> String {
    format!("{datei}.pruefung-{pid}.tmp")
}

/// Ob `name` eine Prüfkopie zu `datei` ist (`<datei>.pruefung-<Ziffern>.tmp`).
fn ist_pruefkopie(name: &str, datei: &str) -> bool {
    name.strip_prefix(datei)
        .and_then(|r| r.strip_prefix(".pruefung-"))
        .and_then(|r| r.strip_suffix(".tmp"))
        .is_some_and(|pid| !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()))
}

/// Vor dem Öffnen: Prüfkopien, die ein früherer Lauf liegen ließ (Absturz
/// mitten in `schema_an_kopie`), im Ordner der Datei löschen. Nur genau dieses
/// Muster; andere Dateien bleiben. Gibt die Zahl der gelöschten zurück.
pub(super) fn pruefkopien_aufraeumen(pfad: &Path) -> usize {
    let Some(datei) = pfad.file_name().and_then(|n| n.to_str()) else { return 0 };
    let ordner = match pfad.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let Ok(eintraege) = std::fs::read_dir(ordner) else { return 0 };
    let mut geloescht = 0;
    for eintrag in eintraege.filter_map(Result::ok) {
        let Ok(name) = eintrag.file_name().into_string() else { continue };
        if !ist_pruefkopie(&name, datei) || !eintrag.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        match std::fs::remove_file(eintrag.path()) {
            Ok(()) => geloescht += 1,
            Err(e) => warn!("Alte Prüfkopie {} nicht gelöscht: {e}", eintrag.path().display()),
        }
    }
    if geloescht > 0 {
        info!("{geloescht} liegengebliebene Prüfkopie(n) neben {} gelöscht", pfad.display());
    }
    geloescht
}

/// Schema aus einer reparierten Kopie von `pfad`. Lässt sich auch die Kopie
/// nicht öffnen, `None`: dann entscheidet das Öffnen des Originals.
fn schema_an_kopie(pfad: &Path) -> Result<Option<u32>, SpeicherFehler> {
    let datei = pfad.file_name().unwrap_or_default().to_string_lossy();
    let kopie = pfad.with_file_name(pruefkopie_name(&datei, std::process::id()));
    std::fs::copy(pfad, &kopie)?;
    let ergebnis = match Database::create(&kopie) {
        Ok(db) => schema_lesen(&db),
        Err(e) => {
            debug!("{}: auch die Kopie lässt sich nicht öffnen ({e})", pfad.display());
            Ok(None)
        }
    };
    if let Err(e) = std::fs::remove_file(&kopie) {
        warn!("Prüfkopie {} nicht gelöscht: {e}", kopie.display());
    }
    ergebnis
}

/// Schema, auf das `schritte` eine Datei bringen: das höchste Ziel der
/// Schritte, mindestens `SCHEMA_AKTUELL`. Mit `SCHRITTE` ist das `SCHEMA_AKTUELL`.
pub(super) fn ziel(schritte: &[Schritt]) -> u32 {
    schritte.iter().map(|s| s.nach).max().unwrap_or(0).max(SCHEMA_AKTUELL)
}

/// Nach dem Öffnen: neue Datei einrichten, zu neue ablehnen, alte mit
/// `schritte` umbauen, danach die Fassung jeder Tabelle prüfen. Im Betrieb mit
/// `SCHRITTE`; Tests geben eigene Schritte, um den ganzen Weg zu prüfen.
pub(super) fn pruefen_mit(speicher: &Speicher, schritte: &[Schritt]) -> Result<(), SpeicherFehler> {
    let ziel = ziel(schritte);
    match schema_lesen(&*speicher.db)? {
        None => anlegen(&speicher.db),
        Some(schema) if schema > ziel => Err(SpeicherFehler::ZuNeu { datei: speicher.pfad.clone(), schema }),
        Some(schema) if schema < ziel => {
            migrieren_mit(speicher, schritte)?;
            match schema_lesen(&*speicher.db)? {
                Some(s) if s == ziel => fassungen_pruefen(&*speicher.db),
                _ => Err(SpeicherFehler::Luecke { von: schema }),
            }
        }
        Some(_) => fassungen_pruefen(&*speicher.db),
    }
}

/// Fassung je Tabelle (`fassung.<tabelle>` in `meta`) gegen die Konstante im
/// Code. Nach allen Umbauschritten muss sie stimmen; sonst kennt dieses
/// Programm die Form der Einträge nicht. Fehlt der Eintrag, nur ein Hinweis
/// im Protokoll (die Datei stammt dann von vor der Fassungs-Zählung).
fn fassungen_pruefen<D: ReadableDatabase>(db: &D) -> Result<(), SpeicherFehler> {
    let txn = db.begin_read()?;
    for (tabelle, schluessel, code) in tabellen::FASSUNGEN {
        let Some(text) = super::meta_aus(&txn, schluessel)? else {
            warn!("{schluessel}: Eintrag fehlt, Fassung nicht geprüft");
            continue;
        };
        let datei: u32 = text.parse().map_err(|_| SpeicherFehler::Beschaedigt(format!("{schluessel} = {text:?} ist keine Zahl")))?;
        if datei != *code {
            return Err(SpeicherFehler::Fassung { tabelle, datei, code: *code });
        }
    }
    Ok(())
}

/// Neue Datei: alle Tabellen anlegen, Schema und Fassungen eintragen.
fn anlegen(db: &Database) -> Result<(), SpeicherFehler> {
    let txn = db.begin_write()?;
    {
        let mut meta = txn.open_table(META)?;
        meta.insert(tabellen::META_SCHEMA, SCHEMA_AKTUELL.to_string().as_str())?;
        meta.insert(tabellen::META_FASSUNG_PC_VERLAUF, tabellen::FASSUNG_PC_VERLAUF.to_string().as_str())?;
        txn.open_table(PC_VERLAUF)?;
    }
    vorgangs_tabellen_anlegen(&txn)?;
    aufgaben_tabellen_anlegen(&txn)?;
    anstehend_tabellen_anlegen(&txn)?;
    antworten_tabellen_anlegen(&txn)?;
    meldungen_tabellen_anlegen(&txn)?;
    projekt_tabellen_anlegen(&txn)?;
    passkey_tabellen_anlegen(&txn)?;
    pc_ereignis_tabellen_anlegen(&txn)?;
    txn.commit()?;
    info!("Datenspeicher neu angelegt (Schema {SCHEMA_AKTUELL})");
    Ok(())
}

/// Kern des Umbaus: führt alle Schritte aus `schritte` aus, die über dem
/// Schema der Datei liegen, der Reihe nach. Vorher eine Sicherung.
pub(super) fn migrieren_mit(speicher: &Speicher, schritte: &[Schritt]) -> Result<(), SpeicherFehler> {
    let mut schema = schema_lesen(&*speicher.db)?.unwrap_or(0);
    let offen: Vec<&Schritt> = schritte.iter().filter(|s| s.nach > schema).collect();
    if offen.is_empty() {
        return Ok(());
    }
    let sicherung = speicher.sichern_vor_schema(schema)?;
    info!("Vor dem Umbau gesichert: {}", sicherung.datei.display());
    for schritt in offen {
        if schritt.nach != schema + 1 {
            return Err(SpeicherFehler::Luecke { von: schema });
        }
        let txn = speicher.db.begin_write()?;
        (schritt.lauf)(&txn)?;
        {
            let mut meta = txn.open_table(META)?;
            meta.insert(tabellen::META_SCHEMA, schritt.nach.to_string().as_str())?;
        }
        txn.commit()?;
        schema = schritt.nach;
        info!("Datenspeicher auf Schema {schema} umgebaut");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use redb::ReadableTableMetadata;
    use sha2::{Digest, Sha256};

    fn test_schritt(txn: &WriteTransaction) -> Result<(), SpeicherFehler> {
        let mut meta = txn.open_table(META)?;
        meta.insert("test.migriert", "ja")?;
        Ok(())
    }

    /// Eine Datei, wie Etappe 2a sie schrieb: Schema 1, nur `meta` und
    /// `pc_verlauf`, darin ein Messwert.
    fn schema_1_datei(pfad: &Path) -> tabellen::PcMesswert {
        let m = tabellen::PcMesswert {
            zeit: tabellen::zeit_schluessel(chrono::Utc::now()),
            cpu: Some(12.5),
            mem: 40.0,
            platte_belegt: Some(1),
            platte_gesamt: Some(2),
        };
        let db = Database::create(pfad).unwrap();
        let txn = db.begin_write().unwrap();
        {
            let mut meta = txn.open_table(META).unwrap();
            meta.insert(tabellen::META_SCHEMA, "1").unwrap();
            meta.insert(tabellen::META_FASSUNG_PC_VERLAUF, "1").unwrap();
            let json = serde_json::to_vec(&m).unwrap();
            txn.open_table(PC_VERLAUF).unwrap().insert(m.zeit.as_str(), json.as_slice()).unwrap();
        }
        txn.commit().unwrap();
        m
    }

    /// Namen der Tabellen in der Datei.
    fn tabellen_in(s: &Speicher) -> Vec<String> {
        use redb::TableHandle;
        let txn = s.db.begin_read().unwrap();
        let mut namen: Vec<String> = txn.list_tables().unwrap().map(|t| t.name().to_string()).collect();
        namen.sort();
        namen
    }

    #[test]
    fn umbau_sichert_vorher_und_setzt_schema() {
        let s = Speicher::temporaer();
        assert_eq!(s.schema().unwrap(), SCHEMA_AKTUELL);
        assert!(s.letzte_sicherung().is_none());
        migrieren_mit(&s, &[Schritt { nach: 11, lauf: test_schritt }]).unwrap();
        assert_eq!(s.schema().unwrap(), 11);
        assert_eq!(s.meta_lesen("test.migriert").unwrap().as_deref(), Some("ja"));
        let datum = s.letzte_sicherung().expect("Sicherung vor dem Umbau");
        let kopie = s.sicherungs_ordner().join(format!("dashboard-{datum}-vor-schema-10.redb"));
        assert!(kopie.is_file());
        // Die Sicherung zeigt den Stand vor dem Umbau.
        let db = ReadOnlyDatabase::open(&kopie).unwrap();
        assert_eq!(schema_lesen(&db).unwrap(), Some(10));
        // Ein zweiter Lauf hat nichts mehr zu tun.
        migrieren_mit(&s, &[Schritt { nach: 11, lauf: test_schritt }]).unwrap();
        assert_eq!(s.schema().unwrap(), 11);
    }

    /// Abnahme 2a und 2b: der ganze Öffnungsweg (vorab prüfen, öffnen,
    /// `pruefen_mit`, sichern, umbauen) mit den echten Schritten von Schema 1 bis 7.
    #[test]
    fn oeffnen_baut_eine_alte_datei_um() {
        let pfad = crate::speicher::test_pfad();
        let ordner = pfad.with_extension("sicherung");
        let m = schema_1_datei(&pfad);
        let s = Speicher::oeffnen_mit(&pfad, ordner.clone(), None, SCHRITTE).unwrap();
        assert_eq!(s.schema().unwrap(), SCHEMA_AKTUELL);
        let namen = tabellen_in(&s);
        let erwartet: Vec<&str> = vec![
            "antworten",
            "antworten_archiv",
            "arbeiten",
            "arbeiten_archiv",
            "aufgaben",
            "aufgaben_archiv",
            "aufgaben_links",
            "einstufungen",
            "ereignisse",
            "jwt_sperrliste",
            "meldungen",
            "meldungen_archiv",
            "meta",
            "passkey_antraege",
            "passkey_aufgaben",
            "passkeys",
            "pc_ereignisse",
            "pc_ereignisse_archiv",
            "pc_verlauf",
            "projekt_tabellen",
            "vorgaenge",
            "vorgaenge_je_slug",
            "warter",
        ];
        assert_eq!(namen, erwartet);
        for (_, schluessel, fassung) in tabellen::FASSUNGEN {
            assert_eq!(s.meta_lesen(schluessel).unwrap(), Some(fassung.to_string()), "{schluessel}");
        }
        // Der Messwert hat den Umbau überstanden.
        assert_eq!(s.pc_verlauf_lesen(chrono::DateTime::UNIX_EPOCH).unwrap(), vec![m]);
        let datum = s.letzte_sicherung().expect("Sicherung vor dem Umbau");
        let kopie = ordner.join(format!("dashboard-{datum}-vor-schema-1.redb"));
        assert!(kopie.is_file(), "{}", kopie.display());
        assert!(ordner.join(format!("dashboard-{datum}-vor-schema-1.toml")).is_file());
        assert_eq!(schema_lesen(&ReadOnlyDatabase::open(&kopie).unwrap()).unwrap(), Some(1));
        // Die neuen Tabellen sind benutzbar.
        let art = crate::api::vertrag::VorgangsArt::Abnahme;
        let (v, _) = s.vorgang_anlegen(art, "x", "Probe", None, "server").unwrap();
        drop(s);
        // Ein zweites Öffnen hat nichts mehr zu tun und behält alles.
        let s = Speicher::oeffnen_mit(&pfad, ordner.clone(), None, SCHRITTE).unwrap();
        assert_eq!(s.schema().unwrap(), SCHEMA_AKTUELL);
        assert_eq!(s.vorgang(&v.id).unwrap().map(|(v, _)| v.titel).as_deref(), Some("Probe"));
        drop(s);
        std::fs::remove_file(&pfad).unwrap();
        std::fs::remove_dir_all(&ordner).unwrap();
    }

    /// Abnahme 5a: Eine Datei mit Schema 2 (PC-Verlauf und Vorgänge, ohne die
    /// Tabellen der Aufgaben) wird ohne Datenverlust auf Schema 3 gebracht.
    #[test]
    fn oeffnen_baut_schema_2_ohne_datenverlust_um() {
        use crate::api::vertrag::{AufgabenZustand, VorgangsArt};
        let pfad = crate::speicher::test_pfad();
        let ordner = pfad.with_extension("sicherung");
        let m = schema_1_datei(&pfad);
        let vorgang = {
            let db = Database::create(&pfad).unwrap();
            let txn = db.begin_write().unwrap();
            migration_2(&txn).unwrap();
            let (v, _) =
                super::super::vorgaenge::vorgang_anlegen_in(&txn, VorgangsArt::Abnahme, "dashboard", "Vor Etappe 5a", Some("abnahme:4v"), "chat:dashboard")
                    .unwrap();
            txn.open_table(META).unwrap().insert(tabellen::META_SCHEMA, "2").unwrap();
            txn.commit().unwrap();
            v
        };
        let s = Speicher::oeffnen_mit(&pfad, ordner.clone(), None, SCHRITTE).unwrap();
        assert_eq!(s.schema().unwrap(), SCHEMA_AKTUELL);
        let namen = tabellen_in(&s);
        assert!(namen.contains(&"aufgaben".to_string()) && namen.contains(&"aufgaben_links".to_string()), "{namen:?}");
        for (_, schluessel, fassung) in tabellen::FASSUNGEN {
            assert_eq!(s.meta_lesen(schluessel).unwrap(), Some(fassung.to_string()), "{schluessel}");
        }
        // Nichts verloren: Messwert, Vorgang mit Ereignis, Tageszähler.
        assert_eq!(s.pc_verlauf_lesen(chrono::DateTime::UNIX_EPOCH).unwrap(), vec![m]);
        let (v, ereignisse) = s.vorgang(&vorgang.id).unwrap().unwrap();
        assert_eq!((v, ereignisse.len()), (vorgang.clone(), 1));
        assert_eq!(s.cursor().unwrap(), 1);
        // Die Sicherung davor hat Schema 2 und noch keine Aufgaben.
        let datum = s.letzte_sicherung().expect("Sicherung vor dem Umbau");
        let kopie = ordner.join(format!("dashboard-{datum}-vor-schema-2.redb"));
        let alt = ReadOnlyDatabase::open(&kopie).unwrap();
        assert_eq!(schema_lesen(&alt).unwrap(), Some(2));
        drop(alt);
        // Die neuen Tabellen sind benutzbar, die Nummern der Vorgänge laufen weiter.
        let jetzt = crate::speicher::aufgaben::jetzt();
        let neu = crate::speicher::aufgaben::NeueAufgabe {
            projekt: "dashboard".into(),
            von: "chat:dashboard".into(),
            titel: "Probe".into(),
            link: None,
            knopf: None,
            frist: None,
            ablauf: None,
            prioritaet: None,
            frist_bezug: None,
        };
        let a = s.aufgabe_anlegen(neu, &jetzt).unwrap();
        let haken = s.aufgabe_erledigt(&a.id, None, &jetzt).unwrap();
        assert!(matches!(haken, crate::speicher::aufgaben::Haken::Erledigt { .. }), "{haken:?}");
        assert_eq!(s.aufgabe(&a.id, &jetzt).unwrap().unwrap().aufgabe.zustand, AufgabenZustand::Erledigt);
        assert_eq!(s.cursor().unwrap(), 2);
        drop(s);
        std::fs::remove_file(&pfad).unwrap();
        std::fs::remove_dir_all(&ordner).unwrap();
    }

    /// Abnahme A1: Eine Datei mit Schema 3 (wie 5a sie schrieb: Messwert,
    /// Vorgang, Aufgabe mit verschlüsseltem Link, ohne das Feld `prioritaet`)
    /// wird ohne Datenverlust auf Schema 4 gebracht; vorher die Sicherung
    /// `vor-schema-3`. Die alte Aufgabe gilt danach als `bald`.
    #[test]
    fn oeffnen_baut_schema_3_ohne_datenverlust_um() {
        use crate::api::vertrag::{ArbeitsStand, Einstufung, EintragsArt, Stufe, VorgangsArt};
        let pfad = crate::speicher::test_pfad();
        let ordner = pfad.with_extension("sicherung");
        let m = schema_1_datei(&pfad);
        let link = "https://beispiel.invalid/anmelden?token=erfunden-schema-3";
        // So stand eine Aufgabe in 5a in der Datei (ohne `prioritaet`).
        let alt_json = r#"{"id":"a-20261028-0001","projekt":"dashboard","von":"chat:dashboard","titel":"Vor A1 angelegt","knopf":null,"frist":"2026-11-01","ablauf":null,"zustand":"offen","antwort":null,"erstellt":"2026-10-28T10:00:00+01:00","geaendert":"2026-10-28T10:00:00+01:00","hat_link":true,"vorgang":null}"#;
        let vorgang = {
            let db = Database::create(&pfad).unwrap();
            let txn = db.begin_write().unwrap();
            migration_2(&txn).unwrap();
            migration_3(&txn).unwrap();
            let (v, _) = super::super::vorgaenge::vorgang_anlegen_in(&txn, VorgangsArt::Abnahme, "dashboard", "Vor A1", None, "chat:dashboard").unwrap();
            txn.open_table(AUFGABEN).unwrap().insert("a-20261028-0001", alt_json.as_bytes()).unwrap();
            let geheim = crate::speicher::verschluesselung::verschluesseln(link.as_bytes()).unwrap();
            txn.open_table(AUFGABEN_LINKS).unwrap().insert("a-20261028-0001", geheim.as_slice()).unwrap();
            txn.open_table(META).unwrap().insert(tabellen::META_SCHEMA, "3").unwrap();
            txn.commit().unwrap();
            v
        };
        let s = Speicher::oeffnen_mit(&pfad, ordner.clone(), None, SCHRITTE).unwrap();
        assert_eq!(s.schema().unwrap(), SCHEMA_AKTUELL);
        let namen = tabellen_in(&s);
        for t in ["arbeiten", "arbeiten_archiv", "einstufungen", "aufgaben", "aufgaben_links", "aufgaben_archiv"] {
            assert!(namen.contains(&t.to_string()), "{t} fehlt: {namen:?}");
        }
        for (_, schluessel, fassung) in tabellen::FASSUNGEN {
            assert_eq!(s.meta_lesen(schluessel).unwrap(), Some(fassung.to_string()), "{schluessel}");
        }
        // Nichts verloren: Messwert, Vorgang, Aufgabe mit Link (im selben Konto lesbar).
        assert_eq!(s.pc_verlauf_lesen(chrono::DateTime::UNIX_EPOCH).unwrap(), vec![m]);
        assert_eq!(s.vorgang(&vorgang.id).unwrap().map(|(v, _)| v), Some(vorgang));
        let jetzt = chrono::DateTime::parse_from_rfc3339("2026-10-29T10:00:00+01:00").unwrap();
        let a = s.aufgabe("a-20261028-0001", &jetzt).unwrap().unwrap();
        assert_eq!((a.aufgabe.titel.as_str(), a.aufgabe.prioritaet, a.link.as_deref()), ("Vor A1 angelegt", None, Some(link)));
        // Die Sicherung davor hat Schema 3 und noch keine Arbeiten.
        let datum = s.letzte_sicherung().expect("Sicherung vor dem Umbau");
        let kopie = ordner.join(format!("dashboard-{datum}-vor-schema-3.redb"));
        let alt = ReadOnlyDatabase::open(&kopie).unwrap();
        assert_eq!(schema_lesen(&alt).unwrap(), Some(3));
        drop(alt);
        // Die neuen Tabellen sind benutzbar; die alte Aufgabe gilt als bald.
        let neu = crate::speicher::arbeiten::NeueArbeit {
            projekt: "dashboard".into(),
            von: "chat:dashboard".into(),
            titel: "Nach A1".into(),
            stand: ArbeitsStand::Laeuft,
            wartet_auf: None,
            verweis: None,
            prioritaet: Stufe::Bald,
            frist: None,
            notiz: None,
        };
        let arbeit = s.arbeit_anlegen(neu, &jetzt).unwrap();
        let e = Einstufung {
            projekt: "dashboard".into(),
            art: EintragsArt::Arbeit,
            id: arbeit.id.clone(),
            gestellt: None,
            stufe: Some(Stufe::Jetzt),
            zeit: "2026-10-29T10:00:00+01:00".into(),
        };
        assert!(s.einstufung_setzen(&e).unwrap());
        let aus = s.anstehend_lesen(&jetzt).unwrap();
        assert_eq!((aus.aufgaben.len(), aus.arbeiten.len(), aus.einstufungen.len()), (1, 1, 1));
        let inhalt = crate::anstehend::Inhalt { heute: jetzt.date_naive(), speicher: aus, ..Default::default() };
        let l = crate::anstehend::alle_eintraege(&inhalt);
        let alte = l.iter().find(|e| e.id == "a-20261028-0001").unwrap();
        assert_eq!((alte.stufe.vorschlag, alte.stufe.wirksam), (Stufe::Bald, Stufe::Bald));
        drop(s);
        std::fs::remove_file(&pfad).unwrap();
        std::fs::remove_dir_all(&ordner).unwrap();
    }

    /// Abnahme A2: Eine Datei mit Schema 4 (wie A1 sie schrieb: Messwert,
    /// Vorgang, Aufgabe mit verschlüsseltem Link, Arbeit, Einstufung einer
    /// Frage) wird ohne Datenverlust auf Schema 5 gebracht; vorher die
    /// Sicherung `vor-schema-4`. Ein zweites Öffnen hat nichts mehr zu tun.
    #[test]
    fn oeffnen_baut_schema_4_ohne_datenverlust_um() {
        use crate::api::vertrag::{Einstufung, EintragsArt, Stufe, VorgangsArt};
        use crate::speicher::antworten::NeueAntwort;
        use crate::vertrag::daten::FrageArt;
        let pfad = crate::speicher::test_pfad();
        let ordner = pfad.with_extension("sicherung");
        let m = schema_1_datei(&pfad);
        let link = "https://beispiel.invalid/anmelden?token=erfunden-schema-4";
        let aufgabe_json = r#"{"id":"a-20261029-0001","projekt":"dashboard","von":"chat:dashboard","titel":"Vor A2 angelegt","knopf":null,"frist":null,"ablauf":null,"zustand":"offen","antwort":null,"erstellt":"2026-10-29T10:00:00+01:00","geaendert":"2026-10-29T10:00:00+01:00","hat_link":true,"vorgang":null,"prioritaet":"jetzt"}"#;
        let arbeit_json = r#"{"id":"ar-20261029-0001","projekt":"dashboard","von":"chat:dashboard","titel":"Vor A2","stand":"laeuft","wartet_auf":null,"verweis":null,"prioritaet":"bald","frist":null,"notiz":null,"erstellt":"2026-10-29T10:00:00+01:00","geaendert":"2026-10-29T10:00:00+01:00"}"#;
        let einstufung = Einstufung {
            projekt: "beispiel".into(),
            art: EintragsArt::Frage,
            id: "schlusssatz".into(),
            gestellt: Some("2026-10-25".into()),
            stufe: Some(Stufe::Jetzt),
            zeit: "2026-10-29T10:00:00+01:00".into(),
        };
        let vorgang = {
            let db = Database::create(&pfad).unwrap();
            let txn = db.begin_write().unwrap();
            migration_2(&txn).unwrap();
            migration_3(&txn).unwrap();
            migration_4(&txn).unwrap();
            let (v, _) = super::super::vorgaenge::vorgang_anlegen_in(&txn, VorgangsArt::Abnahme, "dashboard", "Vor A2", None, "chat:dashboard").unwrap();
            txn.open_table(AUFGABEN).unwrap().insert("a-20261029-0001", aufgabe_json.as_bytes()).unwrap();
            let geheim = crate::speicher::verschluesselung::verschluesseln(link.as_bytes()).unwrap();
            txn.open_table(AUFGABEN_LINKS).unwrap().insert("a-20261029-0001", geheim.as_slice()).unwrap();
            txn.open_table(ARBEITEN).unwrap().insert("ar-20261029-0001", arbeit_json.as_bytes()).unwrap();
            let e = serde_json::to_vec(&einstufung).unwrap();
            txn.open_table(EINSTUFUNGEN).unwrap().insert("beispiel/frage/schlusssatz/2026-10-25", e.as_slice()).unwrap();
            txn.open_table(META).unwrap().insert(tabellen::META_SCHEMA, "4").unwrap();
            txn.commit().unwrap();
            v
        };
        let s = Speicher::oeffnen_mit(&pfad, ordner.clone(), None, SCHRITTE).unwrap();
        assert_eq!(s.schema().unwrap(), SCHEMA_AKTUELL);
        let namen = tabellen_in(&s);
        for t in ["antworten", "antworten_archiv", "arbeiten", "einstufungen", "aufgaben_links"] {
            assert!(namen.contains(&t.to_string()), "{t} fehlt: {namen:?}");
        }
        for (_, schluessel, fassung) in tabellen::FASSUNGEN {
            assert_eq!(s.meta_lesen(schluessel).unwrap(), Some(fassung.to_string()), "{schluessel}");
        }
        // Nichts verloren: Messwert, Vorgang, Aufgabe mit Link, Arbeit, Einstufung.
        assert_eq!(s.pc_verlauf_lesen(chrono::DateTime::UNIX_EPOCH).unwrap(), vec![m]);
        assert_eq!(s.vorgang(&vorgang.id).unwrap().map(|(v, _)| v), Some(vorgang.clone()));
        let jetzt = chrono::DateTime::parse_from_rfc3339("2026-10-29T12:00:00+01:00").unwrap();
        let a = s.aufgabe("a-20261029-0001", &jetzt).unwrap().unwrap();
        assert_eq!((a.aufgabe.titel.as_str(), a.link.as_deref()), ("Vor A2 angelegt", Some(link)));
        assert_eq!(s.arbeit("ar-20261029-0001").unwrap().map(|a| a.titel).as_deref(), Some("Vor A2"));
        assert_eq!(s.einstufungen_lesen().unwrap(), vec![einstufung]);
        // Die Sicherung davor hat Schema 4 und noch keine Antworten.
        let datum = s.letzte_sicherung().expect("Sicherung vor dem Umbau");
        let kopie = ordner.join(format!("dashboard-{datum}-vor-schema-4.redb"));
        let alt = ReadOnlyDatabase::open(&kopie).unwrap();
        assert_eq!(schema_lesen(&alt).unwrap(), Some(4));
        drop(alt);
        // Die neue Tabelle ist benutzbar; die Nummern der Vorgänge laufen weiter.
        let neu = NeueAntwort {
            projekt: "beispiel".into(),
            id: "schlusssatz".into(),
            gestellt: "2026-10-25".into(),
            art: FrageArt::JaNein,
            frage: "Passt das?".into(),
            antwort: "ja".into(),
            notiz: None,
            eigene: None,
        };
        let gespeichert = s.antwort_speichern(neu, &jetzt).unwrap();
        assert!(matches!(gespeichert, crate::speicher::antworten::Speichern::Neu { .. }), "{gespeichert:?}");
        assert_eq!(s.cursor().unwrap(), 2);
        drop(s);
        // Ein zweites Öffnen hat nichts mehr zu tun und behält alles.
        let sicherungen = std::fs::read_dir(&ordner).unwrap().count();
        let s = Speicher::oeffnen_mit(&pfad, ordner.clone(), None, SCHRITTE).unwrap();
        assert_eq!(s.schema().unwrap(), SCHEMA_AKTUELL);
        assert_eq!(std::fs::read_dir(&ordner).unwrap().count(), sicherungen, "keine neue Sicherung");
        assert_eq!(s.antworten_lesen(Some("beispiel")).unwrap().len(), 1);
        assert_eq!(s.vorgang(&vorgang.id).unwrap().map(|(v, _)| v), Some(vorgang));
        drop(s);
        std::fs::remove_file(&pfad).unwrap();
        std::fs::remove_dir_all(&ordner).unwrap();
    }

    /// Abnahme A3: Eine Datei mit Schema 5 (wie A2 sie schrieb: Messwert,
    /// Vorgang, Aufgabe mit verschlüsseltem Link, Arbeit, Einstufung, Antwort)
    /// wird ohne Datenverlust auf Schema 6 gebracht; vorher die Sicherung
    /// `vor-schema-5`. Die Meldungen stehen danach auf /daten, in Sicherung
    /// und Export. Ein zweites Öffnen hat nichts mehr zu tun.
    #[test]
    fn oeffnen_baut_schema_5_ohne_datenverlust_um() {
        use crate::api::vertrag::{MeldungsArt, VorgangsArt};
        use crate::speicher::meldungen::{NeueMeldung, Wiederholung};
        let pfad = crate::speicher::test_pfad();
        let ordner = pfad.with_extension("sicherung");
        let m = schema_1_datei(&pfad);
        let link = "https://beispiel.invalid/anmelden?token=erfunden-schema-5";
        let aufgabe_json = r#"{"id":"a-20261030-0001","projekt":"dashboard","von":"chat:dashboard","titel":"Vor A3 angelegt","knopf":null,"frist":null,"ablauf":null,"zustand":"offen","antwort":null,"erstellt":"2026-10-30T10:00:00+01:00","geaendert":"2026-10-30T10:00:00+01:00","hat_link":true,"vorgang":null,"prioritaet":"jetzt"}"#;
        let arbeit_json = r#"{"id":"ar-20261030-0001","projekt":"dashboard","von":"chat:dashboard","titel":"Vor A3","stand":"laeuft","wartet_auf":null,"verweis":null,"prioritaet":"bald","frist":null,"notiz":null,"erstellt":"2026-10-30T10:00:00+01:00","geaendert":"2026-10-30T10:00:00+01:00"}"#;
        let antwort_json = r#"{"projekt":"beispiel","id":"schlusssatz","gestellt":"2026-10-25","art":"ja-nein","frage":"Passt das?","antwort":"ja","notiz":"Gern","zeit":"2026-10-30T10:00:00+01:00","von":"browser","vorgang":"v-20261030-0009"}"#;
        let einstufung_json =
            r#"{"projekt":"beispiel","art":"frage","id":"schlusssatz","gestellt":"2026-10-25","stufe":"jetzt","zeit":"2026-10-29T10:00:00+01:00"}"#;
        let vorgang = {
            let db = Database::create(&pfad).unwrap();
            let txn = db.begin_write().unwrap();
            migration_2(&txn).unwrap();
            migration_3(&txn).unwrap();
            migration_4(&txn).unwrap();
            migration_5(&txn).unwrap();
            let (v, _) = super::super::vorgaenge::vorgang_anlegen_in(&txn, VorgangsArt::Abnahme, "dashboard", "Vor A3", None, "chat:dashboard").unwrap();
            txn.open_table(AUFGABEN).unwrap().insert("a-20261030-0001", aufgabe_json.as_bytes()).unwrap();
            let geheim = crate::speicher::verschluesselung::verschluesseln(link.as_bytes()).unwrap();
            txn.open_table(AUFGABEN_LINKS).unwrap().insert("a-20261030-0001", geheim.as_slice()).unwrap();
            txn.open_table(ARBEITEN).unwrap().insert("ar-20261030-0001", arbeit_json.as_bytes()).unwrap();
            txn.open_table(EINSTUFUNGEN).unwrap().insert("beispiel/frage/schlusssatz/2026-10-25", einstufung_json.as_bytes()).unwrap();
            txn.open_table(ANTWORTEN).unwrap().insert("beispiel/schlusssatz/2026-10-25", antwort_json.as_bytes()).unwrap();
            txn.open_table(META).unwrap().insert(tabellen::META_SCHEMA, "5").unwrap();
            txn.commit().unwrap();
            v
        };
        let s = Speicher::oeffnen_mit(&pfad, ordner.clone(), None, SCHRITTE).unwrap();
        assert_eq!(s.schema().unwrap(), SCHEMA_AKTUELL);
        let namen = tabellen_in(&s);
        for t in ["meldungen", "meldungen_archiv", "antworten", "antworten_archiv", "arbeiten", "einstufungen", "aufgaben_links"] {
            assert!(namen.contains(&t.to_string()), "{t} fehlt: {namen:?}");
        }
        for (_, schluessel, fassung) in tabellen::FASSUNGEN {
            assert_eq!(s.meta_lesen(schluessel).unwrap(), Some(fassung.to_string()), "{schluessel}");
        }
        // Nichts verloren: Messwert, Vorgang, Aufgabe mit Link, Arbeit, Einstufung, Antwort mit Anmerkung.
        assert_eq!(s.pc_verlauf_lesen(chrono::DateTime::UNIX_EPOCH).unwrap(), vec![m]);
        assert_eq!(s.vorgang(&vorgang.id).unwrap().map(|(v, _)| v), Some(vorgang.clone()));
        let jetzt = chrono::DateTime::parse_from_rfc3339("2026-10-30T12:00:00+01:00").unwrap();
        let a = s.aufgabe("a-20261030-0001", &jetzt).unwrap().unwrap();
        assert_eq!((a.aufgabe.titel.as_str(), a.link.as_deref()), ("Vor A3 angelegt", Some(link)));
        assert_eq!(s.arbeit("ar-20261030-0001").unwrap().map(|a| a.titel).as_deref(), Some("Vor A3"));
        assert_eq!(s.einstufungen_lesen().unwrap().len(), 1);
        let antworten = s.antworten_lesen(Some("beispiel")).unwrap();
        assert_eq!((antworten.len(), antworten[0].notiz.as_deref()), (1, Some("Gern")));
        // Die Sicherung davor hat Schema 5 und noch keine Meldungen.
        let datum = s.letzte_sicherung().expect("Sicherung vor dem Umbau");
        let kopie = ordner.join(format!("dashboard-{datum}-vor-schema-5.redb"));
        let alt = ReadOnlyDatabase::open(&kopie).unwrap();
        assert_eq!(schema_lesen(&alt).unwrap(), Some(5));
        drop(alt);
        // Die neue Tabelle ist benutzbar; sie steht auf /daten (Tabellenliste), in Sicherung und Export.
        let neu = NeueMeldung {
            projekt: "dashboard".into(),
            art: MeldungsArt::Info,
            schluessel: "dashboard/probe".into(),
            titel: "Nach A3".into(),
            text: None,
            verweis: None,
            system: false,
            geschuetzt: false,
            von: "server".into(),
        };
        s.meldung_melden(neu, Wiederholung::Zaehlen, &jetzt).unwrap();
        let info = s.tabellen_info().unwrap();
        assert!(info.iter().any(|(n, _, e)| *n == "meldungen" && *e == 1), "{info:?}");
        assert!(info.iter().any(|(n, _, e)| *n == "meldungen_archiv" && *e == 0), "{info:?}");
        let export: serde_json::Value = serde_json::from_str(&s.export().unwrap().als_json()).unwrap();
        assert_eq!(export["meldungen"][0]["titel"], "Nach A3");
        assert!(!export.to_string().contains("erfunden-schema-5"), "kein Link im Export");
        let sicherung = s.sichern().unwrap();
        let kopie = ReadOnlyDatabase::open(&sicherung.datei).unwrap();
        assert_eq!(kopie.begin_read().unwrap().open_table(MELDUNGEN).unwrap().len().unwrap(), 1);
        drop(kopie);
        drop(s);
        // Ein zweites Öffnen hat nichts mehr zu tun und behält alles.
        let sicherungen = std::fs::read_dir(&ordner).unwrap().count();
        let s = Speicher::oeffnen_mit(&pfad, ordner.clone(), None, SCHRITTE).unwrap();
        assert_eq!(s.schema().unwrap(), SCHEMA_AKTUELL);
        assert_eq!(std::fs::read_dir(&ordner).unwrap().count(), sicherungen, "keine neue Sicherung");
        assert_eq!(s.meldungen_lesen(None).unwrap().len(), 1);
        assert_eq!(s.vorgang(&vorgang.id).unwrap().map(|(v, _)| v), Some(vorgang));
        drop(s);
        std::fs::remove_file(&pfad).unwrap();
        std::fs::remove_dir_all(&ordner).unwrap();
    }

    /// Etappe 4b: eine Datei mit Schema 6 (Messwert, Vorgang) wird ohne
    /// Datenverlust auf Schema 7 gebracht, vorher die Sicherung `vor-schema-6`.
    /// Danach lassen sich Tabellen eines Moduls einrichten; sie stehen auf
    /// /daten, in Sicherung und Export. Ein zweites Öffnen hat nichts zu tun.
    #[test]
    fn oeffnen_baut_schema_6_ohne_datenverlust_um() {
        use crate::api::vertrag::VorgangsArt;
        use dashboard_projekt::TabellenDef;
        let pfad = crate::speicher::test_pfad();
        let ordner = pfad.with_extension("sicherung");
        let m = schema_1_datei(&pfad);
        let vorgang = {
            let db = Database::create(&pfad).unwrap();
            let txn = db.begin_write().unwrap();
            for schritt in [migration_2, migration_3, migration_4, migration_5, migration_6] {
                schritt(&txn).unwrap();
            }
            let (v, _) = super::super::vorgaenge::vorgang_anlegen_in(&txn, VorgangsArt::Abnahme, "dashboard", "Vor 4b", None, "chat").unwrap();
            txn.open_table(META).unwrap().insert(tabellen::META_SCHEMA, "6").unwrap();
            txn.commit().unwrap();
            v
        };
        let s = Speicher::oeffnen_mit(&pfad, ordner.clone(), None, SCHRITTE).unwrap();
        assert_eq!(s.schema().unwrap(), SCHEMA_AKTUELL);
        assert!(tabellen_in(&s).contains(&"projekt_tabellen".to_string()));
        assert_eq!(s.meta_lesen(tabellen::META_FASSUNG_PROJEKT_TABELLEN).unwrap().as_deref(), Some("1"));
        assert_eq!(s.pc_verlauf_lesen(chrono::DateTime::UNIX_EPOCH).unwrap(), vec![m]);
        assert_eq!(s.vorgang(&vorgang.id).unwrap().map(|(v, _)| v), Some(vorgang.clone()));
        let datum = s.letzte_sicherung().expect("Sicherung vor dem Umbau");
        let alt = ReadOnlyDatabase::open(ordner.join(format!("dashboard-{datum}-vor-schema-6.redb"))).unwrap();
        assert_eq!(schema_lesen(&alt).unwrap(), Some(6));
        drop(alt);
        s.projekt_tabellen_einrichten("architektur", &[TabellenDef::neu("verlauf", "Verlauf")]).unwrap();
        s.projekt_eintrag_schreiben("architektur", "verlauf", 1, "2026-10-30", br#"{"abgenommen":1}"#).unwrap();
        assert!(s.tabellen_info().unwrap().iter().any(|(n, _, e)| n == "projekt/architektur/verlauf" && *e == 1));
        let export: serde_json::Value = serde_json::from_str(&s.export().unwrap().als_json()).unwrap();
        assert_eq!(export["projekt/architektur/verlauf"]["2026-10-30"]["abgenommen"], 1);
        let sicherung = s.sichern().unwrap();
        let kopie = ReadOnlyDatabase::open(&sicherung.datei).unwrap();
        let def = super::super::projekte::definition("projekt/architektur/verlauf");
        assert_eq!(kopie.begin_read().unwrap().open_table(def).unwrap().len().unwrap(), 1);
        drop(kopie);
        drop(s);
        let sicherungen = std::fs::read_dir(&ordner).unwrap().count();
        let s = Speicher::oeffnen_mit(&pfad, ordner.clone(), None, SCHRITTE).unwrap();
        assert_eq!(s.schema().unwrap(), SCHEMA_AKTUELL);
        assert_eq!(std::fs::read_dir(&ordner).unwrap().count(), sicherungen, "keine neue Sicherung");
        assert_eq!(s.projekt_anzahl("architektur", "verlauf", 1).unwrap(), 1);
        drop(s);
        std::fs::remove_file(&pfad).unwrap();
        std::fs::remove_dir_all(&ordner).unwrap();
    }

    /// Etappe 7c: eine Datei mit Schema 7 (Messwert, Vorgang, Aufgabe) wird
    /// ohne Datenverlust auf Schema 8 gebracht, vorher die Sicherung
    /// `vor-schema-7`. Danach gibt es die vier Passkey-Tabellen; im Export
    /// steht nur `passkeys`, in der Sicherung alle vier. Ein zweites Öffnen hat
    /// nichts zu tun.
    #[test]
    fn oeffnen_baut_schema_7_ohne_datenverlust_um() {
        use crate::anmeldung::WindowsKonto;
        use crate::api::vertrag::VorgangsArt;
        use crate::passkey::webauthn::{self, Erwartung, pruef_authenticator::PruefAuthenticator};
        use crate::speicher::passkeys::{AufgabenArt, PasskeyAufgabe};
        let pfad = crate::speicher::test_pfad();
        let ordner = pfad.with_extension("sicherung");
        let m = schema_1_datei(&pfad);
        let vorgang = {
            let db = Database::create(&pfad).unwrap();
            let txn = db.begin_write().unwrap();
            for schritt in [migration_2, migration_3, migration_4, migration_5, migration_6, migration_7] {
                schritt(&txn).unwrap();
            }
            let (v, _) = super::super::vorgaenge::vorgang_anlegen_in(&txn, VorgangsArt::Abnahme, "dashboard", "Vor 7c", None, "chat").unwrap();
            txn.open_table(META).unwrap().insert(tabellen::META_SCHEMA, "7").unwrap();
            txn.commit().unwrap();
            v
        };
        let s = Speicher::oeffnen_mit(&pfad, ordner.clone(), None, SCHRITTE).unwrap();
        assert_eq!(s.schema().unwrap(), SCHEMA_AKTUELL);
        for t in ["passkeys", "passkey_aufgaben", "passkey_antraege", "jwt_sperrliste"] {
            assert!(tabellen_in(&s).contains(&t.to_string()), "{t}");
        }
        for (_, schluessel, fassung) in tabellen::FASSUNGEN_PASSKEY {
            assert_eq!(s.meta_lesen(schluessel).unwrap(), Some(fassung.to_string()), "{schluessel}");
        }
        assert_eq!(s.pc_verlauf_lesen(chrono::DateTime::UNIX_EPOCH).unwrap(), vec![m]);
        assert_eq!(s.vorgang(&vorgang.id).unwrap().map(|(v, _)| v), Some(vorgang.clone()));
        let datum = s.letzte_sicherung().expect("Sicherung vor dem Umbau");
        let alt = ReadOnlyDatabase::open(ordner.join(format!("dashboard-{datum}-vor-schema-7.redb"))).unwrap();
        assert_eq!(schema_lesen(&alt).unwrap(), Some(7));
        drop(alt);
        // Die neuen Tabellen sind benutzbar: ein Passkey, ein Antrag, eine Sperre.
        let jetzt = crate::speicher::aufgaben::jetzt();
        let e = Erwartung { herkunft: "http://localhost:8787", rp_id: "localhost" };
        let a = PruefAuthenticator::es256();
        let k = WindowsKonto { sid: "S-1-5-21-9-9-9-1001".into(), name: r"PC\anders".into(), inhaber: false };
        let ch = s.passkey_aufgabe_anlegen(&PasskeyAufgabe::anlegen(&k, "Handy", &jetzt), &jetzt).unwrap();
        let (c, att) = a.anlegen(&ch, e.herkunft);
        let neu = webauthn::anlegen_pruefen(&c, &att, &a.id, e).unwrap();
        let aufgabe = s.passkey_aufgabe_nehmen(&ch, AufgabenArt::Anlegen, &jetzt).unwrap();
        let (p, _) = s.passkey_anlegen(&neu, &aufgabe, &jetzt).unwrap();
        s.ausweis_sperren("jti-x", jetzt.timestamp() + 900, &jetzt).unwrap();
        s.passkey_aufgabe_anlegen(&PasskeyAufgabe::anmelden(&jetzt), &jetzt).unwrap();
        let export: serde_json::Value = serde_json::from_str(&s.export().unwrap().als_json()).unwrap();
        assert_eq!(export["passkeys"][0]["id"], p.id.as_str());
        for heikel in ["passkey_aufgaben", "passkey_antraege", "jwt_sperrliste"] {
            assert!(export.get(heikel).is_none(), "{heikel} gehört nicht in den Export");
        }
        let sicherung = s.sichern().unwrap();
        let kopie = ReadOnlyDatabase::open(&sicherung.datei).unwrap();
        let txn = kopie.begin_read().unwrap();
        for def in [tabellen::PASSKEYS, tabellen::PASSKEY_AUFGABEN, tabellen::PASSKEY_ANTRAEGE, tabellen::JWT_SPERRLISTE] {
            assert_eq!(txn.open_table(def).unwrap().len().unwrap(), 1, "{def}");
        }
        drop(txn);
        drop(kopie);
        drop(s);
        let sicherungen = std::fs::read_dir(&ordner).unwrap().count();
        let s = Speicher::oeffnen_mit(&pfad, ordner.clone(), None, SCHRITTE).unwrap();
        assert_eq!(s.schema().unwrap(), SCHEMA_AKTUELL);
        assert_eq!(std::fs::read_dir(&ordner).unwrap().count(), sicherungen, "keine neue Sicherung");
        assert_eq!(s.passkey(&p.id).unwrap().map(|p| p.konto), Some(r"PC\anders".to_string()));
        drop(s);
        std::fs::remove_file(&pfad).unwrap();
        std::fs::remove_dir_all(&ordner).unwrap();
    }

    /// Etappe 2e: eine Datei mit Schema 8 (Messwert, Vorgang) wird ohne
    /// Datenverlust auf Schema 9 gebracht, vorher die Sicherung `vor-schema-8`.
    /// Danach gibt es `pc_ereignisse` und `pc_ereignisse_archiv` (leer), in
    /// /daten, Sicherung und Export. Ein zweites Öffnen hat nichts zu tun.
    #[test]
    fn oeffnen_baut_schema_8_ohne_datenverlust_um() {
        use crate::api::vertrag::VorgangsArt;
        let pfad = crate::speicher::test_pfad();
        let ordner = pfad.with_extension("sicherung");
        let m = schema_1_datei(&pfad);
        let vorgang = {
            let db = Database::create(&pfad).unwrap();
            let txn = db.begin_write().unwrap();
            for schritt in [migration_2, migration_3, migration_4, migration_5, migration_6, migration_7, migration_8] {
                schritt(&txn).unwrap();
            }
            let (v, _) = super::super::vorgaenge::vorgang_anlegen_in(&txn, VorgangsArt::Abnahme, "dashboard", "Vor 2e", None, "chat").unwrap();
            txn.open_table(META).unwrap().insert(tabellen::META_SCHEMA, "8").unwrap();
            txn.commit().unwrap();
            v
        };
        let s = Speicher::oeffnen_mit(&pfad, ordner.clone(), None, SCHRITTE).unwrap();
        assert_eq!(s.schema().unwrap(), SCHEMA_AKTUELL);
        for t in ["pc_ereignisse", "pc_ereignisse_archiv"] {
            assert!(tabellen_in(&s).contains(&t.to_string()), "{t}");
        }
        for (_, schluessel, fassung) in tabellen::FASSUNGEN_PC_EREIGNISSE {
            assert_eq!(s.meta_lesen(schluessel).unwrap(), Some(fassung.to_string()), "{schluessel}");
        }
        assert_eq!(s.pc_verlauf_lesen(chrono::DateTime::UNIX_EPOCH).unwrap(), vec![m]);
        assert_eq!(s.vorgang(&vorgang.id).unwrap().map(|(v, _)| v), Some(vorgang.clone()));
        assert!(s.pc_ereignisse_lesen(true).unwrap().is_empty(), "neue Tabellen sind leer");
        let datum = s.letzte_sicherung().expect("Sicherung vor dem Umbau");
        let alt = ReadOnlyDatabase::open(ordner.join(format!("dashboard-{datum}-vor-schema-8.redb"))).unwrap();
        assert_eq!(schema_lesen(&alt).unwrap(), Some(8));
        drop(alt);
        let info = s.tabellen_info().unwrap();
        assert!(info.iter().any(|(n, _, e)| n == "pc_ereignisse" && *e == 0), "{info:?}");
        assert!(info.iter().any(|(n, _, e)| n == "pc_ereignisse_archiv" && *e == 0), "{info:?}");
        // Benutzbar, und in Export und Sicherung.
        let start = chrono::DateTime::parse_from_rfc3339("2026-10-30T07:59:01+01:00").unwrap();
        let jetzt = crate::speicher::aufgaben::jetzt();
        s.pc_ereignisse_einspielen(&crate::pc::ereignisse::starts(&[start]), &jetzt).unwrap();
        let export: serde_json::Value = serde_json::from_str(&s.export().unwrap().als_json()).unwrap();
        assert!(export["pc_ereignisse"].is_array() && export["pc_ereignisse_archiv"].is_array(), "{export}");
        let sicherung = s.sichern().unwrap();
        let kopie = ReadOnlyDatabase::open(&sicherung.datei).unwrap();
        let txn = kopie.begin_read().unwrap();
        let in_kopie = txn.open_table(tabellen::PC_EREIGNISSE).unwrap().len().unwrap() + txn.open_table(tabellen::PC_EREIGNISSE_ARCHIV).unwrap().len().unwrap();
        assert_eq!(in_kopie, 1);
        drop(txn);
        drop(kopie);
        drop(s);
        let sicherungen = std::fs::read_dir(&ordner).unwrap().count();
        let s = Speicher::oeffnen_mit(&pfad, ordner.clone(), None, SCHRITTE).unwrap();
        assert_eq!(s.schema().unwrap(), SCHEMA_AKTUELL);
        assert_eq!(std::fs::read_dir(&ordner).unwrap().count(), sicherungen, "keine neue Sicherung");
        assert_eq!(s.pc_ereignisse_lesen(true).unwrap().len(), 1);
        drop(s);
        std::fs::remove_file(&pfad).unwrap();
        std::fs::remove_dir_all(&ordner).unwrap();
    }

    /// Zweig teilschritte (T1): eine Datei mit Schema 9 wird auf Schema 10
    /// gebracht, vorher die Sicherung `vor-schema-9`. Laufende und wartende
    /// Arbeiten bekommen `besitzer = von`, alles andere bleibt Byte für Byte;
    /// ein alter Satz liest sich und schreibt sich gleich zurück. Ein
    /// Programm, das nur Schema 9 kennt, lehnt die Datei danach ab, ohne sie
    /// zu ändern. Ein zweites Öffnen hat nichts zu tun.
    #[test]
    fn oeffnen_baut_schema_9_ohne_datenverlust_um() {
        use crate::api::vertrag::Arbeit;
        let pfad = crate::speicher::test_pfad();
        let ordner = pfad.with_extension("sicherung");
        let m = schema_1_datei(&pfad);
        let alt = |id: &str, stand: &str, wartet_auf: &str| {
            format!(
                r#"{{"id":"{id}","projekt":"dashboard","von":"chat:architektur","titel":"Vor T1","stand":"{stand}","wartet_auf":{wartet_auf},"verweis":null,"prioritaet":"bald","frist":null,"notiz":"dran: chat:dashboard","erstellt":"2026-11-04T10:00:00+01:00","geaendert":"2026-11-04T11:00:00+01:00","verlauf":[{{"zeit":"2026-11-04T11:00:00+01:00","von":"chat:dashboard","art":"titel","alt":"a","neu":"Vor T1","text":"Titel geändert: „a“ → „Vor T1“"}}]}}"#
            )
        };
        let laeuft = alt("ar-20261104-0001", "laeuft", "null");
        let wartet = alt("ar-20261104-0002", "wartet", r#""inhaber""#);
        let naechstes = alt("ar-20261104-0003", "als-naechstes", "null");
        let fertig = alt("ar-20261104-0004", "fertig", "null");
        {
            let db = Database::create(&pfad).unwrap();
            let txn = db.begin_write().unwrap();
            for schritt in [migration_2, migration_3, migration_4, migration_5, migration_6, migration_7, migration_8, migration_9] {
                schritt(&txn).unwrap();
            }
            {
                let mut meta = txn.open_table(META).unwrap();
                meta.insert(tabellen::META_FASSUNG_ARBEITEN, "1").unwrap();
                meta.insert(tabellen::META_FASSUNG_ARBEITEN_ARCHIV, "1").unwrap();
                meta.insert(tabellen::META_SCHEMA, "9").unwrap();
                let mut t = txn.open_table(ARBEITEN).unwrap();
                for (id, json) in [("ar-20261104-0001", &laeuft), ("ar-20261104-0002", &wartet), ("ar-20261104-0003", &naechstes)] {
                    t.insert(id, json.as_bytes()).unwrap();
                }
                txn.open_table(ARBEITEN_ARCHIV).unwrap().insert("ar-20261104-0004", fertig.as_bytes()).unwrap();
            }
            txn.commit().unwrap();
        }
        let s = Speicher::oeffnen_mit(&pfad, ordner.clone(), None, SCHRITTE).unwrap();
        assert_eq!(s.schema().unwrap(), 10);
        for (_, schluessel, fassung) in tabellen::FASSUNGEN {
            assert_eq!(s.meta_lesen(schluessel).unwrap(), Some(fassung.to_string()), "{schluessel}");
        }
        assert_eq!(s.meta_lesen(tabellen::META_FASSUNG_ARBEITEN).unwrap().as_deref(), Some("2"));
        assert_eq!(s.pc_verlauf_lesen(chrono::DateTime::UNIX_EPOCH).unwrap(), vec![m]);
        let roh = |tabelle: redb::TableDefinition<&str, &[u8]>, id: &str| {
            let txn = s.db.begin_read().unwrap();
            let t = txn.open_table(tabelle).unwrap();
            let w = t.get(id).unwrap().unwrap();
            String::from_utf8(w.value().to_vec()).unwrap()
        };
        // Nur laufende und wartende: besitzer = von, sonst kein Byte anders.
        let mit = |json: &str| json.replacen(r#""von":"chat:architektur","#, r#""von":"chat:architektur","besitzer":"chat:architektur","#, 1);
        assert_eq!(roh(ARBEITEN, "ar-20261104-0001"), mit(&laeuft));
        assert_eq!(roh(ARBEITEN, "ar-20261104-0002"), mit(&wartet));
        assert_eq!(roh(ARBEITEN, "ar-20261104-0003"), naechstes);
        assert_eq!(roh(ARBEITEN_ARCHIV, "ar-20261104-0004"), fertig);
        // Ein alter Satz liest sich und schreibt sich gleich zurück.
        for json in [&laeuft, &wartet, &naechstes, &fertig] {
            let a: Arbeit = serde_json::from_str(json).unwrap();
            assert!(a.schritte.is_empty() && a.verlauf[0].schritt.is_none());
            assert_eq!(&serde_json::to_string(&a).unwrap(), json);
        }
        let a = s.arbeit("ar-20261104-0002").unwrap().unwrap();
        assert_eq!((a.besitzer.as_deref(), a.notiz.as_deref()), (Some("chat:architektur"), Some("dran: chat:dashboard")));
        // Die Sicherung davor hat Schema 9 und die alten Bytes.
        let datum = s.letzte_sicherung().expect("Sicherung vor dem Umbau");
        let kopie = ReadOnlyDatabase::open(ordner.join(format!("dashboard-{datum}-vor-schema-9.redb"))).unwrap();
        assert_eq!(schema_lesen(&kopie).unwrap(), Some(9));
        let txn = kopie.begin_read().unwrap();
        let w = txn.open_table(ARBEITEN).unwrap().get("ar-20261104-0001").unwrap().unwrap().value().to_vec();
        assert_eq!(w, laeuft.as_bytes());
        drop(txn);
        drop(kopie);
        // Export und Sicherung tragen den Besitzer mit.
        let export: serde_json::Value = serde_json::from_str(&s.export().unwrap().als_json()).unwrap();
        let liste = export["arbeiten"].as_array().unwrap();
        assert!(liste.iter().any(|a| a["besitzer"] == "chat:architektur"), "{export}");
        assert_eq!(export["arbeiten_archiv"][0]["id"], "ar-20261104-0004");
        let sicherung = s.sichern().unwrap();
        let kopie = ReadOnlyDatabase::open(&sicherung.datei).unwrap();
        let txn = kopie.begin_read().unwrap();
        let w = txn.open_table(ARBEITEN).unwrap().get("ar-20261104-0001").unwrap().unwrap().value().to_vec();
        assert_eq!(w, mit(&laeuft).as_bytes());
        drop(txn);
        drop(kopie);
        drop(s);
        // Ein Programm, das nur Schema 9 kennt, lehnt die Datei ab; sie bleibt, wie sie ist.
        let vorher = std::fs::read(&pfad).unwrap();
        let e = vorab_pruefen(&pfad, 9).unwrap_err();
        assert!(matches!(e, SpeicherFehler::ZuNeu { schema: 10, .. }), "{e:?}");
        assert_eq!(Sha256::digest(&vorher), Sha256::digest(std::fs::read(&pfad).unwrap()));
        // Ein zweites Öffnen hat nichts zu tun.
        let sicherungen = std::fs::read_dir(&ordner).unwrap().count();
        let s = Speicher::oeffnen_mit(&pfad, ordner.clone(), None, SCHRITTE).unwrap();
        assert_eq!(std::fs::read_dir(&ordner).unwrap().count(), sicherungen, "keine neue Sicherung");
        assert_eq!(s.arbeiten_lesen(None).unwrap().len(), 3);
        drop(s);
        std::fs::remove_file(&pfad).unwrap();
        std::fs::remove_dir_all(&ordner).unwrap();
    }

    /// Mehrere Schritte nacheinander, mit einem eingeschobenen Schritt nach 11;
    /// danach ist die Datei für dieses Programm zu neu.
    #[test]
    fn oeffnen_baut_ueber_mehrere_schritte_um() {
        let pfad = crate::speicher::test_pfad();
        let ordner = pfad.with_extension("sicherung");
        schema_1_datei(&pfad);
        let schritte = [
            Schritt { nach: 2, lauf: migration_2 },
            Schritt { nach: 3, lauf: migration_3 },
            Schritt { nach: 4, lauf: migration_4 },
            Schritt { nach: 5, lauf: migration_5 },
            Schritt { nach: 6, lauf: migration_6 },
            Schritt { nach: 7, lauf: migration_7 },
            Schritt { nach: 8, lauf: migration_8 },
            Schritt { nach: 9, lauf: migration_9 },
            Schritt { nach: 10, lauf: migration_10 },
            Schritt { nach: 11, lauf: test_schritt },
        ];
        let s = Speicher::oeffnen_mit(&pfad, ordner.clone(), None, &schritte).unwrap();
        assert_eq!(s.schema().unwrap(), 11);
        assert_eq!(s.meta_lesen("test.migriert").unwrap().as_deref(), Some("ja"));
        assert!(tabellen_in(&s).contains(&"meldungen".to_string()));
        assert!(tabellen_in(&s).contains(&"ereignisse".to_string()));
        assert!(tabellen_in(&s).contains(&"einstufungen".to_string()));
        assert!(tabellen_in(&s).contains(&"antworten".to_string()));
        assert!(tabellen_in(&s).contains(&"projekt_tabellen".to_string()));
        assert!(tabellen_in(&s).contains(&"pc_ereignisse_archiv".to_string()));
        drop(s);
        let e = Speicher::oeffnen(&pfad).err().expect("Schema 11 ist für dieses Programm zu neu");
        assert!(matches!(e, SpeicherFehler::ZuNeu { schema: 11, .. }), "{e:?}");
        std::fs::remove_file(&pfad).unwrap();
        std::fs::remove_dir_all(&ordner).unwrap();
    }

    /// Fehlt ein Schritt im Öffnungsweg, bleibt die Datei auf ihrem Schema.
    #[test]
    fn oeffnen_mit_luecke_bricht_ab() {
        let pfad = crate::speicher::test_pfad();
        let ordner = pfad.with_extension("sicherung");
        drop(Speicher::oeffnen(&pfad).unwrap());
        let e = Speicher::oeffnen_mit(&pfad, ordner.clone(), None, &[Schritt { nach: 12, lauf: test_schritt }]).err().expect("Lücke");
        assert!(matches!(e, SpeicherFehler::Luecke { von: 10 }), "{e:?}");
        assert_eq!(Speicher::oeffnen(&pfad).unwrap().schema().unwrap(), SCHEMA_AKTUELL);
        std::fs::remove_file(&pfad).unwrap();
        let _ = std::fs::remove_dir_all(&ordner);
    }

    #[test]
    fn schritte_passen_zum_schema() {
        assert_eq!(ziel(SCHRITTE), SCHEMA_AKTUELL, "kein Schritt über SCHEMA_AKTUELL hinaus");
        if SCHEMA_AKTUELL > 1 {
            assert_eq!(SCHRITTE.last().map(|s| s.nach), Some(SCHEMA_AKTUELL), "Schritt bis SCHEMA_AKTUELL fehlt");
        }
    }

    #[test]
    fn liegengebliebene_pruefkopien_verschwinden_beim_oeffnen() {
        let ordner = std::env::temp_dir().join(format!("dashboard-test-{}-pruefkopien", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        std::fs::create_dir_all(&ordner).unwrap();
        let pfad = ordner.join("dashboard.redb");
        for name in ["dashboard.redb.pruefung-123.tmp", "dashboard.redb.pruefung-4.tmp"] {
            std::fs::write(ordner.join(name), "rest").unwrap();
        }
        let bleiben = [
            "dashboard.redb.pruefung-.tmp",
            "dashboard.redb.pruefung-12x.tmp",
            "dashboard.redb.pruefung-1.tmp.alt",
            "anders.redb.pruefung-1.tmp",
            "notizen.txt",
        ];
        for name in bleiben {
            std::fs::write(ordner.join(name), "fremd").unwrap();
        }
        let s = Speicher::oeffnen(&pfad).unwrap();
        drop(s);
        let mut namen: Vec<String> = std::fs::read_dir(&ordner).unwrap().filter_map(|e| e.ok()?.file_name().into_string().ok()).collect();
        namen.sort();
        let mut erwartet: Vec<String> = bleiben.iter().map(|n| n.to_string()).collect();
        erwartet.push("dashboard.redb".to_string());
        erwartet.sort();
        assert_eq!(namen, erwartet);
        assert_eq!(pruefkopien_aufraeumen(&pfad), 0, "nichts mehr zu tun");
        assert_eq!(pruefkopie_name("dashboard.redb", 7), "dashboard.redb.pruefung-7.tmp");
        assert!(ist_pruefkopie(&pruefkopie_name("x.redb", 42), "x.redb"));
        std::fs::remove_dir_all(&ordner).unwrap();
    }

    #[test]
    fn luecke_in_den_schritten_bricht_ab() {
        let s = Speicher::temporaer();
        let e = migrieren_mit(&s, &[Schritt { nach: 12, lauf: test_schritt }]).unwrap_err();
        assert!(matches!(e, SpeicherFehler::Luecke { von: 10 }), "{e:?}");
        assert_eq!(s.schema().unwrap(), SCHEMA_AKTUELL);
    }

    #[test]
    fn zu_neue_datei_wird_abgelehnt_und_bleibt_unveraendert() {
        let pfad = crate::speicher::test_pfad();
        {
            let db = Database::create(&pfad).unwrap();
            let txn = db.begin_write().unwrap();
            txn.open_table(META).unwrap().insert(tabellen::META_SCHEMA, "99").unwrap();
            txn.commit().unwrap();
        }
        let vorher = std::fs::read(&pfad).unwrap();
        let e = Speicher::oeffnen(&pfad).err().expect("muss abgelehnt werden");
        assert!(matches!(e, SpeicherFehler::ZuNeu { schema: 99, .. }), "{e:?}");
        assert!(e.to_string().contains("Schema 99, dieses Programm kennt 10"), "{e}");
        let nachher = std::fs::read(&pfad).unwrap();
        assert_eq!(vorher.len(), nachher.len());
        assert_eq!(Sha256::digest(&vorher), Sha256::digest(&nachher));
        std::fs::remove_file(&pfad).unwrap();
    }

    /// Kindprozess für den nächsten Test: legt eine Datei mit Schema 99 an und
    /// endet, ohne sie zu schließen. Ohne `DASHBOARD_TEST_ZU_NEU` tut er nichts.
    #[test]
    fn zu_neu_kind() {
        let Some(pfad) = std::env::var_os("DASHBOARD_TEST_ZU_NEU") else { return };
        let db = Database::create(&pfad).unwrap();
        let txn = db.begin_write().unwrap();
        txn.open_table(META).unwrap().insert(tabellen::META_SCHEMA, "99").unwrap();
        txn.commit().unwrap();
        std::mem::forget(db);
        std::process::exit(0);
    }

    /// Nach einem unsauberen Ende lässt redb die Datei nur lesend nicht öffnen.
    /// Auch dann wird eine zu neue Datei abgelehnt, ohne dass ein Byte sich ändert.
    #[test]
    fn zu_neue_datei_nach_unsauberem_ende_bleibt_unveraendert() {
        let pfad = crate::speicher::test_pfad();
        let test = "speicher::migration::tests::zu_neu_kind";
        let status = crate::speicher::kind_starten(test, "DASHBOARD_TEST_ZU_NEU", &pfad).wait().unwrap();
        assert!(status.success(), "Kindprozess: {status}");
        assert!(matches!(ReadOnlyDatabase::open(&pfad), Err(redb::DatabaseError::RepairAborted)), "die Datei muss unsauber sein, sonst prüft der Test nichts");
        let vorher = std::fs::read(&pfad).unwrap();
        let e = Speicher::oeffnen(&pfad).err().expect("muss abgelehnt werden");
        assert!(matches!(e, SpeicherFehler::ZuNeu { schema: 99, .. }), "{e:?}");
        let nachher = std::fs::read(&pfad).unwrap();
        assert_eq!(Sha256::digest(&vorher), Sha256::digest(&nachher), "Original darf sich nicht ändern");
        let ordner = pfad.parent().unwrap();
        let name = pfad.file_name().unwrap().to_str().unwrap().to_string();
        let reste: Vec<_> =
            std::fs::read_dir(ordner).unwrap().filter_map(|e| e.ok()?.file_name().into_string().ok()).filter(|n| n.starts_with(&name) && n != &name).collect();
        assert!(reste.is_empty(), "Prüfkopie liegt noch: {reste:?}");
        std::fs::remove_file(&pfad).unwrap();
    }

    #[test]
    fn falsche_fassung_einer_tabelle_wird_abgelehnt() {
        for (schluessel, tabelle) in [(tabellen::META_FASSUNG_PC_VERLAUF, "pc_verlauf"), (tabellen::META_FASSUNG_EREIGNISSE, "ereignisse")] {
            let pfad = crate::speicher::test_pfad();
            {
                let db = Database::create(&pfad).unwrap();
                let txn = db.begin_write().unwrap();
                {
                    let mut meta = txn.open_table(META).unwrap();
                    meta.insert(tabellen::META_SCHEMA, "10").unwrap();
                    meta.insert(schluessel, "2").unwrap();
                }
                txn.commit().unwrap();
            }
            let e = Speicher::oeffnen(&pfad).err().expect("muss abgelehnt werden");
            assert!(matches!(e, SpeicherFehler::Fassung { tabelle: t, datei: 2, code: 1 } if t == tabelle), "{e:?}");
            let satz = format!("Die Tabelle {tabelle} hat in der Datei die Fassung 2, dieses Programm erwartet 1.");
            assert!(e.to_string().starts_with(&satz), "{e}");
            std::fs::remove_file(&pfad).unwrap();
        }
    }
}
