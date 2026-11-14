//! Der Datenspeicher des Dashboards (Zielbild, Kapitel c3): eine redb-Datei mit
//! allem, was zur Laufzeit entsteht (PC-Verlauf, Vorgänge). Projektordner bleiben
//! TOML in Git; hier liegt nur, was nicht in Git gehört, aber gesichert wird.
//!
//! Nur der Server öffnet die Datei. Tabellen in `tabellen`, Schema-Prüfung und
//! Umbau in `migration`, Sicherung in `sicherung`, Export in `export`, der
//! PC-Verlauf und sein Sammler in `pc_verlauf`, Vorgänge und Journal in
//! `vorgaenge`, die Aufgaben für den Inhaber in `aufgaben` (Links verschlüsselt mit
//! DPAPI, `verschluesselung`), Arbeiten der Chats und die Einstufungen des Inhabers in
//! `arbeiten` (Etappe A1), ihre Teilschritte in `schritte` (Schema 10), die Antworten des Inhabers auf Fragen in `antworten` (Etappe
//! A2), Meldungen für den Inhaber in `meldungen` (Etappe A3), die Tabellen der
//! Backend-Module (`projekt/<slug>/<name>`) in `projekte` (Etappe 4b), die Windows-Ereignisse
//! für den Verlauf in `pc_ereignisse` (Etappe 2e). Schreiben läuft über `spawn_blocking`, damit der Server nicht stockt.

pub mod antworten;
pub mod arbeiten;
pub mod aufgaben;
pub mod export;
pub mod meldungen;
pub mod migration;
pub mod passkeys;
pub mod pc_ereignisse;
pub mod pc_verlauf;
pub mod projekte;
pub mod schritte;
pub mod sicherung;
pub mod tabellen;
pub mod verschluesselung;
pub mod vorgaenge;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use redb::{Database, ReadTransaction, ReadableDatabase, TableError};

use tabellen::{META, PcMesswert};

pub use crate::aenderungen::Bereich;

/// Fehler des Datenspeichers. Der Satz ist für den Inhaber; Einzelheiten liefert
/// `einzelheiten()` fürs Protokoll.
#[derive(Debug, thiserror::Error)]
pub enum SpeicherFehler {
    #[error(
        "Die Datenbank wurde von einer neueren Fassung des Dashboards geschrieben (Schema {schema}, \
         dieses Programm kennt {}). Bitte das neuere Programm starten; die Datei bleibt unverändert.",
        migration::SCHEMA_AKTUELL
    )]
    ZuNeu { datei: PathBuf, schema: u32 },
    #[error("Die Datenbank ist schon geöffnet. Läuft das Dashboard bereits in einem anderen Fenster?")]
    Belegt { datei: PathBuf },
    #[error("Der Datenspeicher lässt sich nicht lesen oder schreiben.")]
    Datenbank(#[source] redb::Error),
    #[error("Eine Datei des Datenspeichers lässt sich nicht lesen oder schreiben.")]
    Datei(#[from] std::io::Error),
    #[error("Ein Eintrag im Datenspeicher ist beschädigt.")]
    Beschaedigt(String),
    #[error("Für diese Datenbank fehlt ein Umbauschritt (von Schema {von}); die Datei bleibt, wie sie ist.")]
    Luecke { von: u32 },
    #[error(
        "Die Tabelle {tabelle} hat in der Datei die Fassung {datei}, dieses Programm erwartet {code}. \
         Für diesen Unterschied gibt es keinen Umbauschritt; die Datei bleibt, wie sie ist."
    )]
    Fassung { tabelle: &'static str, datei: u32, code: u32 },
    #[error("Im Datenspeicher ist etwas schiefgegangen.")]
    Hintergrund(String),
    /// Kennung eines Vorgangs, den es nicht gibt (wird zu 404).
    #[error("Diesen Vorgang gibt es nicht.")]
    NichtGefunden(String),
    /// DPAPI hat abgelehnt (Einzelheiten mit Windows-Fehlercode, nie der Klartext).
    #[error("Der Link ließ sich nicht verschlüsselt speichern.")]
    Verschluesselung(String),
    /// Eine Obergrenze ist erreicht (etwa offene Aufgaben); der Satz nennt sie (wird zu 429).
    #[error("{0}")]
    Grenze(String),
    /// Ein anderer Chat ist dran (Arbeit oder Schritt); nichts geschrieben (wird zu 409,
    /// der Körper nennt `besitzer` und `seit`).
    #[error("Daran ist schon {besitzer} (seit {seit}).")]
    Besetzt { besitzer: String, seit: String },
    /// Die Änderung passt nicht zu den Regeln; der Satz sagt, was stattdessen
    /// geht. Nichts geschrieben (wird zu 422).
    #[error("{0}")]
    Ungueltig(String),
}

impl SpeicherFehler {
    /// Technische Einzelheiten, nur fürs Protokoll.
    pub fn einzelheiten(&self) -> String {
        match self {
            Self::ZuNeu { datei, schema } => format!("{}: Schema {schema}", datei.display()),
            Self::Belegt { datei } => format!("{}: schon geöffnet", datei.display()),
            Self::Datenbank(e) => e.to_string(),
            Self::Datei(e) => e.to_string(),
            Self::Beschaedigt(was) | Self::Hintergrund(was) => was.clone(),
            Self::Luecke { von } => format!("kein Schritt nach Schema {}", von + 1),
            Self::Fassung { tabelle, datei, code } => format!("fassung.{tabelle}: Datei {datei}, Programm {code}"),
            Self::NichtGefunden(id) => format!("Kennung {id:?} unbekannt"),
            Self::Verschluesselung(was) | Self::Grenze(was) | Self::Ungueltig(was) => was.clone(),
            Self::Besetzt { besitzer, seit } => format!("besetzt von {besitzer} seit {seit}"),
        }
    }
}

macro_rules! aus_redb {
    ($($typ:ty),*) => {
        $(impl From<$typ> for SpeicherFehler {
            fn from(e: $typ) -> Self {
                Self::Datenbank(e.into())
            }
        })*
    };
}
aus_redb!(redb::Error, redb::StorageError, redb::TransactionError, redb::TableError, redb::CommitError);

impl From<serde_json::Error> for SpeicherFehler {
    fn from(e: serde_json::Error) -> Self {
        Self::Beschaedigt(format!("JSON: {e}"))
    }
}

/// Der Datenspeicher. Nicht `Clone`: der Zustand hält ihn als `Arc<Speicher>`.
pub struct Speicher {
    db: Arc<Database>,
    pfad: PathBuf,
    sicherungs_ordner: PathBuf,
    /// Hält zwei Sicherungsläufe auseinander (Knopf und nächtliche Sicherung).
    sicherungs_riegel: std::sync::Mutex<()>,
    /// Testdatei (nur in Tests abgefragt).
    #[cfg_attr(not(test), allow(dead_code))]
    temporaer: bool,
    /// Live-Lücken: meldet jedes Festschreiben als Ereignis `speicher` mit
    /// Bereich (gebündelt, `aenderungen`). Ohne Melder (Werkzeuge, viele
    /// Tests) still.
    melder: std::sync::OnceLock<Arc<crate::aenderungen::Melder>>,
    /// Nur bei Testdateien: räumt nach dem Schließen der Datenbank auf. Steht
    /// nach `db`, weil Felder in dieser Reihenfolge freigegeben werden.
    _aufraeumen: Option<Aufraeumen>,
}

/// Löscht Testdatei und Sicherungsordner beim Freigeben.
struct Aufraeumen {
    datei: PathBuf,
    ordner: PathBuf,
}

impl Drop for Aufraeumen {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.datei);
        let _ = std::fs::remove_dir_all(&self.ordner);
    }
}

impl Speicher {
    /// Öffnet die Datei (legt Ordner und Datei bei Bedarf an) und prüft das Schema.
    pub fn oeffnen(pfad: &Path) -> Result<Speicher, SpeicherFehler> {
        Self::oeffnen_mit(pfad, sicherungs_ordner_fuer(pfad), None, migration::SCHRITTE)
    }

    /// Wie `oeffnen`, aber mit eigenem Sicherungsordner (Dienst: `pfade.sicherung`
    /// aus dienst.toml statt `sicherung\` neben der Datei).
    pub fn oeffnen_mit_sicherung(pfad: &Path, sicherungs_ordner: &Path) -> Result<Speicher, SpeicherFehler> {
        Self::oeffnen_mit(pfad, sicherungs_ordner.to_path_buf(), None, migration::SCHRITTE)
    }

    /// Wie `oeffnen`, mit eigenem Sicherungsordner und eigenen Umbauschritten
    /// (Tests schieben so einen Schritt ein, ohne `SCHEMA_AKTUELL` zu ändern).
    fn oeffnen_mit(
        pfad: &Path,
        sicherungs_ordner: PathBuf,
        aufraeumen: Option<Aufraeumen>,
        schritte: &[migration::Schritt],
    ) -> Result<Speicher, SpeicherFehler> {
        if let Some(ordner) = pfad.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(ordner)?;
        }
        migration::pruefkopien_aufraeumen(pfad);
        // Erst nur lesend nachsehen: eine zu neue Datei bleibt so unberührt.
        migration::vorab_pruefen(pfad, migration::ziel(schritte))?;
        let db = Database::create(pfad).map_err(|e| match e {
            redb::DatabaseError::DatabaseAlreadyOpen => SpeicherFehler::Belegt { datei: pfad.to_path_buf() },
            e => SpeicherFehler::Datenbank(e.into()),
        })?;
        let speicher = Speicher {
            db: Arc::new(db),
            pfad: pfad.to_path_buf(),
            sicherungs_ordner,
            sicherungs_riegel: std::sync::Mutex::new(()),
            temporaer: aufraeumen.is_some(),
            melder: std::sync::OnceLock::new(),
            _aufraeumen: aufraeumen,
        };
        migration::pruefen_mit(&speicher, schritte)?;
        Ok(speicher)
    }

    /// Für Tests: eine eigene Datei im Temp-Ordner, mit eigenem Sicherungsordner
    /// daneben; beides verschwindet beim Freigeben. Fasst nie die echte Datei an.
    #[cfg(test)]
    pub fn temporaer() -> Speicher {
        let pfad = test_pfad();
        let ordner = pfad.with_extension("sicherung");
        let aufraeumen = Aufraeumen { datei: pfad.clone(), ordner: ordner.clone() };
        Self::oeffnen_mit(&pfad, ordner, Some(aufraeumen), migration::SCHRITTE).expect("Testdatenbank lässt sich anlegen")
    }

    pub fn pfad(&self) -> &Path {
        &self.pfad
    }

    pub fn sicherungs_ordner(&self) -> &Path {
        &self.sicherungs_ordner
    }

    /// Ob dies eine Testdatei ist, die beim Freigeben verschwindet.
    #[cfg(test)]
    pub fn ist_temporaer(&self) -> bool {
        self.temporaer
    }

    /// Größe der Datei in Bytes (0, wenn sie sich nicht lesen lässt).
    pub fn groesse_bytes(&self) -> u64 {
        std::fs::metadata(&self.pfad).map(|m| m.len()).unwrap_or(0)
    }

    /// Ein Eintrag aus `meta`.
    pub fn meta_lesen(&self, schluessel: &str) -> Result<Option<String>, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        meta_aus(&txn, schluessel)
    }

    /// Merkt, dass die Oberfläche „Neu in `version`“ gezeigt hat. Ohne
    /// Ereignis: Nur die Meldung „Neu in …“ hängt daran, und die fragt beim Laden.
    pub fn neu_in_gezeigt(&self, version: &str) -> Result<(), SpeicherFehler> {
        let txn = self.db.begin_write()?;
        txn.open_table(META)?.insert(tabellen::META_NEU_IN_GEZEIGT, version)?;
        self.festschreiben(txn, &[])
    }

    /// Schema-Fassung der Datei (0, wenn noch keine eingetragen ist).
    pub fn schema(&self) -> Result<u32, SpeicherFehler> {
        Ok(migration::schema_lesen(&*self.db)?.unwrap_or(0))
    }

    /// Name, Beschreibung und Zahl der Einträge je Tabelle aus `tabellen::ALLE`,
    /// danach die Tabellen der Backend-Module (`projekt/<slug>/<name>`, Etappe 4b).
    pub fn tabellen_info(&self) -> Result<Vec<(String, String, u64)>, SpeicherFehler> {
        let txn = self.db.begin_read()?;
        let mut out = Vec::new();
        for t in tabellen::ALLE {
            out.push((t.name.to_string(), t.beschreibung.to_string(), (t.eintraege)(&txn)?));
        }
        for t in projekte::verzeichnis(&txn)? {
            let voll = t.voller_name();
            let anzahl = tabellen::zaehlen(projekte::definition(&voll), &txn)?;
            out.push((voll, t.beschreibung, anzahl));
        }
        Ok(out)
    }

    /// Hängt einen Messwert an den PC-Verlauf (im Hintergrund-Thread).
    pub async fn pc_messwert_anhaengen(&self, m: PcMesswert) -> Result<(), SpeicherFehler> {
        let db = self.db.clone();
        hintergrund(move || {
            let txn = db.begin_write()?;
            pc_verlauf::anhaengen(&txn, &m)?;
            txn.commit()?;
            Ok(())
        })
        .await?;
        self.gemeldet(&[Bereich::PcMessung]);
        Ok(())
    }

    /// Setzt den Melder für das Ereignis `speicher` (einmal, beim Start).
    pub fn melder_setzen(&self, melder: Arc<crate::aenderungen::Melder>) {
        let _ = self.melder.set(melder);
    }

    /// Meldet geänderte Bereiche (ohne Melder still).
    pub fn gemeldet(&self, bereiche: &[Bereich]) {
        if let Some(m) = self.melder.get() {
            for b in bereiche {
                m.melden(*b);
            }
        }
    }

    /// Schreibt die Transaktion fest und meldet danach die Bereiche
    /// (Ereignis `speicher`, gebündelt). Erst nach dem Festschreiben: Wer
    /// auf das Ereignis hin liest, sieht den neuen Stand.
    fn festschreiben(&self, txn: redb::WriteTransaction, bereiche: &[Bereich]) -> Result<(), SpeicherFehler> {
        txn.commit()?;
        self.gemeldet(bereiche);
        Ok(())
    }

    /// Führt blockierende Arbeit am Speicher in `spawn_blocking` aus.
    pub async fn im_hintergrund<T, F>(speicher: Arc<Speicher>, f: F) -> Result<T, SpeicherFehler>
    where
        T: Send + 'static,
        F: FnOnce(&Speicher) -> Result<T, SpeicherFehler> + Send + 'static,
    {
        hintergrund(move || f(&speicher)).await
    }
}

async fn hintergrund<T, F>(f: F) -> Result<T, SpeicherFehler>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, SpeicherFehler> + Send + 'static,
{
    tokio::task::spawn_blocking(f).await.map_err(|e| SpeicherFehler::Hintergrund(e.to_string()))?
}

/// Ein Eintrag aus `meta` in einer Lese-Transaktion; fehlt die Tabelle, `None`.
fn meta_aus(txn: &ReadTransaction, schluessel: &str) -> Result<Option<String>, SpeicherFehler> {
    let tabelle = match txn.open_table(META) {
        Ok(t) => t,
        Err(TableError::TableDoesNotExist(_)) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    Ok(tabelle.get(schluessel)?.map(|w| w.value().to_string()))
}

/// Sicherungsordner zu einer Datei: `sicherung\` daneben. Auch ohne offenen
/// Speicher bekannt, damit /daten ihn nennen kann, wenn die Datei kaputt ist.
pub fn sicherungs_ordner_fuer(pfad: &Path) -> PathBuf {
    match pfad.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.join("sicherung"),
        _ => PathBuf::from("sicherung"),
    }
}

/// Pfad zum Anzeigen: liegt er im Benutzerordner, als `%USERPROFILE%\…`,
/// damit der Benutzername nicht auf der Seite und in Antworten steht. Danach
/// noch [`crate::datenschutz::verbergen`]: Im Dienst ist `USERPROFILE` das
/// Profil des Dienstkontos, die Pfade liegen aber im Profil des Inhabers.
/// Der Benutzerordner kommt aus `USERPROFILE`, sonst aus `HOME` (macOS, Linux).
pub fn anzeige_pfad(pfad: &Path) -> String {
    let heim = heim_aus(std::env::var_os("USERPROFILE"), std::env::var_os("HOME"));
    crate::datenschutz::verbergen(&anzeige_pfad_mit(pfad, heim.as_deref()))
}

/// Der Benutzerordner: `USERPROFILE`, und nur wenn das fehlt oder leer ist, `HOME`.
fn heim_aus(userprofile: Option<std::ffi::OsString>, home: Option<std::ffi::OsString>) -> Option<PathBuf> {
    userprofile.filter(|p| !p.is_empty()).or(home.filter(|p| !p.is_empty())).map(PathBuf::from)
}

fn anzeige_pfad_mit(pfad: &Path, heim: Option<&Path>) -> String {
    let rest = heim.filter(|h| !h.as_os_str().is_empty()).and_then(|h| pfad.strip_prefix(h).ok());
    match rest {
        Some(r) if r.as_os_str().is_empty() => "%USERPROFILE%".to_string(),
        Some(r) => format!("%USERPROFILE%{}{}", std::path::MAIN_SEPARATOR, r.display()),
        None => pfad.display().to_string(),
    }
}

/// Neuer Pfad für eine Testdatei: Prozessnummer und Zähler, damit parallele
/// Tests sich nie eine Datei teilen.
#[cfg(test)]
pub(crate) fn test_pfad() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static ZAEHLER: AtomicU64 = AtomicU64::new(0);
    let n = ZAEHLER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("dashboard-test-{}-{n}.redb", std::process::id()))
}

/// Für Tests mit Kindprozess: startet dieses Testprogramm noch einmal, nur mit
/// dem Test `test`, und gibt ihm `pfad` in der Umgebungsvariable `variable`.
#[cfg(test)]
pub(crate) fn kind_starten(test: &str, variable: &str, pfad: &Path) -> std::process::Child {
    crate::dienst::programme::eigener_befehl(std::env::current_exe().expect("Pfad des Testprogramms"))
        .args(["--exact", test, "--nocapture"])
        .env(variable, pfad)
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("Kindprozess startet")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Kindprozess für `absturz_mitten_im_schreiben_laesst_die_datei_heil`:
    /// schreibt ohne Pause Messwerte, je Commit einen, und trägt nach jedem
    /// Commit die Zahl in `<datei>.zaehler` ein. Ohne `DASHBOARD_TEST_SCHREIBER`
    /// tut er nichts.
    #[test]
    fn schreiber_kind() {
        let Some(pfad) = std::env::var_os("DASHBOARD_TEST_SCHREIBER") else { return };
        let pfad = PathBuf::from(pfad);
        let zaehler = zaehler_pfad(&pfad);
        let s = Speicher::oeffnen(&pfad).unwrap();
        let start = chrono::Utc::now();
        // Gewollte Dauerschleife: der Elterntest beendet den Kindprozess mitten im Schreiben.
        #[allow(clippy::for_unbounded_range)]
        for n in 1u64.. {
            let m = PcMesswert {
                zeit: tabellen::zeit_schluessel(start + chrono::Duration::seconds(n as i64)),
                cpu: Some((n % 100) as f32),
                mem: 50.0,
                platte_belegt: Some(n),
                platte_gesamt: Some(n * 2),
            };
            let txn = s.db.begin_write().unwrap();
            pc_verlauf::anhaengen(&txn, &m).unwrap();
            txn.commit().unwrap();
            std::fs::write(&zaehler, n.to_string()).unwrap();
        }
    }

    fn zaehler_pfad(pfad: &Path) -> PathBuf {
        let mut name = pfad.file_name().unwrap().to_os_string();
        name.push(".zaehler");
        pfad.with_file_name(name)
    }

    /// Abnahme: Server mitten im Schreiben beenden, die Datei bleibt heil.
    #[test]
    fn absturz_mitten_im_schreiben_laesst_die_datei_heil() {
        let pfad = test_pfad();
        let zaehler = zaehler_pfad(&pfad);
        let mut kind = kind_starten("speicher::tests::schreiber_kind", "DASHBOARD_TEST_SCHREIBER", &pfad);
        // Der Zähler wird ohne Umbenennen geschrieben; ein halb geschriebener
        // Stand ist unlesbar und zählt nicht. Er steigt nur, darum gilt der größte.
        let lesen = || std::fs::read_to_string(&zaehler).ok().and_then(|t| t.trim().parse::<u64>().ok());
        let frist = Instant::now() + Duration::from_secs(8);
        let mut gesehen = 0;
        while gesehen < 20 {
            if Instant::now() > frist {
                let _ = kind.kill();
                let _ = kind.wait();
                panic!("Kindprozess hat in 8 s nur {gesehen} Werte geschrieben");
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
        let (_, _, anzahl) = s.pc_verlauf_eckdaten().unwrap();
        assert!(anzahl + 1 >= stand, "{anzahl} Werte in der Datei, gemeldet waren {stand}");
        let werte = s.pc_verlauf_lesen(chrono::DateTime::UNIX_EPOCH).unwrap();
        assert_eq!(werte.len() as u64, anzahl, "alle Werte lesbar");
        assert!(werte.windows(2).all(|w| w[0].zeit < w[1].zeit));
        assert_eq!(s.schema().unwrap(), migration::SCHEMA_AKTUELL);
        drop(s);
        std::fs::remove_file(&pfad).unwrap();
        let _ = std::fs::remove_file(&zaehler);
    }

    #[test]
    fn neue_datei_hat_schema_und_tabellen() {
        let s = Speicher::temporaer();
        assert_eq!(s.schema().unwrap(), migration::SCHEMA_AKTUELL);
        assert_eq!(s.meta_lesen(tabellen::META_FASSUNG_PC_VERLAUF).unwrap().as_deref(), Some("1"));
        let info = s.tabellen_info().unwrap();
        assert!(info.iter().any(|(n, _, e)| *n == "pc_verlauf" && *e == 0), "{info:?}");
        for (_, schluessel, fassung) in tabellen::FASSUNGEN {
            assert_eq!(s.meta_lesen(schluessel).unwrap(), Some(fassung.to_string()), "{schluessel}");
        }
        assert!(s.groesse_bytes() > 0);
        assert!(s.ist_temporaer());
    }

    #[test]
    fn testdatei_verschwindet_beim_freigeben() {
        let s = Speicher::temporaer();
        let pfad = s.pfad().to_path_buf();
        let ordner = s.sicherungs_ordner().to_path_buf();
        s.sichern().unwrap();
        assert!(pfad.is_file() && ordner.is_dir());
        drop(s);
        assert!(!pfad.exists() && !ordner.exists());
    }

    #[test]
    fn wiederoeffnen_behaelt_die_daten() {
        let pfad = test_pfad();
        let s = Speicher::oeffnen(&pfad).unwrap();
        assert_eq!(s.schema().unwrap(), migration::SCHEMA_AKTUELL);
        drop(s);
        let s = Speicher::oeffnen(&pfad).unwrap();
        assert_eq!(s.schema().unwrap(), migration::SCHEMA_AKTUELL);
        drop(s);
        std::fs::remove_file(&pfad).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn pfade_ohne_benutzernamen() {
        let heim = PathBuf::from(r"C:\Users\beispiel");
        let datei = heim.join(".var").join("dashboard").join("dashboard.redb");
        assert_eq!(anzeige_pfad_mit(&datei, Some(&heim)), r"%USERPROFILE%\.var\dashboard\dashboard.redb");
        assert_eq!(anzeige_pfad_mit(&heim, Some(&heim)), "%USERPROFILE%");
        // Nur ganze Ordnernamen: „beispiel2“ beginnt nicht mit dem Ordner „beispiel“.
        let fremd = PathBuf::from(r"C:\Users\beispiel2\x.redb");
        assert_eq!(anzeige_pfad_mit(&fremd, Some(&heim)), fremd.display().to_string());
        assert_eq!(anzeige_pfad_mit(&datei, None), datei.display().to_string());
        assert_eq!(anzeige_pfad_mit(&datei, Some(Path::new(""))), datei.display().to_string());
        assert_eq!(sicherungs_ordner_fuer(&datei), heim.join(".var").join("dashboard").join("sicherung"));
        if let Some(echt) = std::env::var_os("USERPROFILE") {
            let p = PathBuf::from(echt).join("x.redb");
            assert_eq!(anzeige_pfad(&p), format!("%USERPROFILE%{}x.redb", std::path::MAIN_SEPARATOR));
        }
    }

    /// Der Benutzerordner kommt aus `USERPROFILE`; nur ohne ihn aus `HOME`.
    /// Mit einem Profil ohne Laufwerk bleibt vom Pfad nur der Teil darunter.
    #[test]
    fn benutzerordner_aus_userprofile_sonst_home() {
        let os = |t: &str| Some(std::ffi::OsString::from(t));
        assert_eq!(heim_aus(os(r"C:\Users\beispiel"), os("/Users/beispiel2")), Some(PathBuf::from(r"C:\Users\beispiel")));
        assert_eq!(heim_aus(None, os("/Users/beispiel")), Some(PathBuf::from("/Users/beispiel")));
        assert_eq!(heim_aus(os(""), os("/Users/beispiel")), Some(PathBuf::from("/Users/beispiel")));
        assert_eq!(heim_aus(None, os("")), None);
        assert_eq!(heim_aus(None, None), None);
        // Gegenprobe: Ohne Benutzerordner bleibt der Pfad, wie er ist.
        let heim = PathBuf::from("/Users/beispiel");
        let datei = heim.join("Library").join("x.redb");
        assert_eq!(anzeige_pfad_mit(&datei, None), datei.display().to_string());
        assert_eq!(anzeige_pfad_mit(&datei, Some(&heim)), format!("%USERPROFILE%{0}Library{0}x.redb", std::path::MAIN_SEPARATOR));
        assert_eq!(anzeige_pfad_mit(&heim, Some(&heim)), "%USERPROFILE%");
        let fremd = PathBuf::from("/Users/beispiel2/x.redb");
        assert_eq!(anzeige_pfad_mit(&fremd, Some(&heim)), fremd.display().to_string());
    }

    /// Bewusste Ausnahmen von „nie blankes `txn.commit()`“ (Anleitung,
    /// Abschnitt Live-Lücken): Datei und Funktion, mit Grund. Alles andere in
    /// `src/speicher` schreibt über `Speicher::festschreiben`.
    const COMMIT_ERLAUBT: &[(&str, &str, &str)] = &[
        ("mod.rs", "festschreiben", "der eine Weg: erst festschreiben, dann melden"),
        ("mod.rs", "pc_messwert_anhaengen", "schreibt im Hintergrund-Faden ohne `&self`, meldet `pc-messung` danach selbst"),
        ("migration.rs", "anlegen", "neue Datei beim Öffnen, noch ohne Melder und ohne Leser"),
        ("migration.rs", "migrieren_mit", "Umbau des Schemas beim Öffnen, noch ohne Melder und ohne Leser"),
        ("sicherung.rs", "kopie_schreiben", "schreibt die Sicherungskopie, eine andere Datei; `sicherung` meldet `sichern`"),
        ("passkeys.rs", "passkey_aufgabe_anlegen", "Challenge: zeigt keine Seite"),
        ("passkeys.rs", "passkey_aufgabe_nehmen", "Challenge: zeigt keine Seite"),
        ("passkeys.rs", "passkey_antrag_pruefen", "meldet nur, wenn Anträge abgelaufen sind (sonst nur Challenges)"),
        ("passkeys.rs", "passkey_entscheiden", "Ablauf vorab: meldet nur, wenn Anträge abgelaufen sind"),
        ("passkeys.rs", "passkey_aufraeumen", "meldet nur geschlossene Anträge, nicht abgelaufene Challenges und Sperren"),
        ("pc_ereignisse.rs", "pc_ereignisse_einspielen", "meldet nur neue Windows-Ereignisse, nicht `zuletzt_gesehen`"),
    ];

    /// Der Name der Funktion, in der Zeile `nr` steht: die nächste Zeile
    /// darüber mit `fn <name>` (ohne Kommentare).
    fn umgebende_funktion(zeilen: &[&str], nr: usize) -> Option<String> {
        zeilen[..=nr].iter().rev().find_map(|z| {
            let z = z.trim_start();
            if z.starts_with("//") {
                return None;
            }
            let rest = z.strip_prefix("fn ").or_else(|| z.find(" fn ").map(|i| &z[i + 4..]))?;
            let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            (!name.is_empty()).then_some(name)
        })
    }

    /// Wächter (Abnahme live-luecken): Jedes `.commit()` in `src/speicher`
    /// außerhalb der Tests steht in einer Funktion aus [`COMMIT_ERLAUBT`].
    /// Ein neuer Schreibweg mit blankem Commit fällt hier auf: Er gehört über
    /// `festschreiben(txn, &[Bereich::…])`, sonst sieht keine Seite die
    /// Änderung. Jede Ausnahme muss auch noch vorkommen (keine Leichen).
    #[test]
    fn waechter_kein_blankes_commit() {
        let ordner = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src").join("speicher");
        let mut dateien = vec![ordner.clone()];
        let mut gefunden: Vec<(String, String)> = Vec::new();
        let mut fremd = Vec::new();
        while let Some(o) = dateien.pop() {
            for e in std::fs::read_dir(&o).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    dateien.push(p);
                    continue;
                }
                if p.extension().is_none_or(|x| x != "rs") {
                    continue;
                }
                let name = p.strip_prefix(&ordner).unwrap().to_string_lossy().replace('\\', "/");
                let text = std::fs::read_to_string(&p).unwrap();
                let zeilen: Vec<&str> = text.lines().collect();
                // Der Testteil am Ende (`#[cfg(test)]` vor `mod tests`) zählt nicht.
                let ende = zeilen.windows(2).position(|w| w[0].trim() == "#[cfg(test)]" && w[1].trim_start().starts_with("mod tests")).unwrap_or(zeilen.len());
                for (nr, z) in zeilen[..ende].iter().enumerate() {
                    if z.trim_start().starts_with("//") || !z.contains(".commit()") {
                        continue;
                    }
                    let f = umgebende_funktion(&zeilen, nr).unwrap_or_default();
                    if COMMIT_ERLAUBT.iter().any(|(d, fu, _)| *d == name && *fu == f) {
                        gefunden.push((name.clone(), f));
                    } else {
                        fremd.push(format!("{name}:{} in fn {f}: {}", nr + 1, z.trim()));
                    }
                }
            }
        }
        assert!(
            fremd.is_empty(),
            "Blankes commit() in src/speicher; bitte `self.festschreiben(txn, &[Bereich::…])` nehmen oder die Ausnahme mit Grund in COMMIT_ERLAUBT und in der Anleitung (docs/anleitung, Live-Lücken) eintragen:\n{}",
            fremd.join("\n")
        );
        for (d, f, grund) in COMMIT_ERLAUBT {
            assert!(!grund.is_empty());
            assert!(gefunden.iter().any(|(gd, gf)| gd == d && gf == f), "Ausnahme {d}:{f} kommt nicht mehr vor; bitte streichen");
        }
    }

    /// Der Wächter selbst: findet die Funktion um eine Zeile, auch in einer
    /// Schließung, und überspringt Kommentare.
    #[test]
    fn waechter_findet_die_umgebende_funktion() {
        let code = ["impl X {", "    pub async fn schreiben(&self) {", "        // fn falsch", "        hintergrund(move || {", "            txn.commit()?;"];
        assert_eq!(umgebende_funktion(&code, 4).as_deref(), Some("schreiben"));
        let code = ["fn frei() {", "    let f: Box<dyn FnOnce()> = x;", "    t.commit();"];
        assert_eq!(umgebende_funktion(&code, 2).as_deref(), Some("frei"));
        assert_eq!(umgebende_funktion(&["t.commit();"], 0), None);
    }

    #[test]
    fn fehler_saetze_ohne_technik() {
        let e = SpeicherFehler::ZuNeu { datei: PathBuf::from("x.redb"), schema: 12 };
        assert_eq!(
            e.to_string(),
            "Die Datenbank wurde von einer neueren Fassung des Dashboards geschrieben (Schema 12, dieses Programm \
             kennt 11). Bitte das neuere Programm starten; die Datei bleibt unverändert."
        );
        let e = SpeicherFehler::NichtGefunden("v-20261026-0007".into());
        assert_eq!(e.to_string(), "Diesen Vorgang gibt es nicht.");
        assert!(!e.to_string().contains("x.redb"));
    }
}
