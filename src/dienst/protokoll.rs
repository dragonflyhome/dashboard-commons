//! Protokoll des Dienstes: Zeilen ohne Farben in `<protokoll>/dienst-JJJJ-MM-TT.log`,
//! je Tag eine Datei, im Konsolen-Modus zusätzlich in der Konsole. Ohne eigene
//! Kiste: ein `MakeWriter`, der beim Datumswechsel die Datei wechselt.
//!
//! Beim Start und bei jedem Datumswechsel räumt `aufraeumen` Protokolle älter
//! als 14 Tage weg, aber nur Dateien der eigenen Muster: der Dienst
//! `dienst-JJJJ-MM-TT.log` im Protokollordner und `bote-JJJJ-MM-TT.log` im
//! Unterordner `bote\` (dort schreibt der Bote), der Bote nur seine eigenen.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::NaiveDate;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// So viele Tage bleiben Protokolle liegen.
pub const AUFBEWAHREN_TAGE: i64 = 14;

/// Welche Protokolle `aufraeumen` kennt.
const PRAEFIXE: [&str; 2] = ["dienst", "bote"];

/// Dateiname für einen Tag: `dienst-2026-10-26.log`.
pub fn dateiname(praefix: &str, datum: NaiveDate) -> String {
    format!("{praefix}-{}.log", datum.format("%Y-%m-%d"))
}

/// Datum aus einem Dateinamen der Form `<praefix>-JJJJ-MM-TT.log` mit einem
/// der bekannten Präfixe; sonst `None`.
pub fn datum_aus_name(name: &str) -> Option<NaiveDate> {
    let (praefix, _) = name.split_once('-')?;
    if !PRAEFIXE.contains(&praefix) {
        return None;
    }
    datum_mit_praefix(name, praefix)
}

/// Datum aus `<praefix>-JJJJ-MM-TT.log` für genau dieses Präfix.
fn datum_mit_praefix(name: &str, praefix: &str) -> Option<NaiveDate> {
    let tag = name.strip_prefix(praefix)?.strip_prefix('-')?.strip_suffix(".log")?;
    if tag.len() != 10 || !tag.bytes().enumerate().all(|(i, b)| if i == 4 || i == 7 { b == b'-' } else { b.is_ascii_digit() }) {
        return None;
    }
    NaiveDate::parse_from_str(tag, "%Y-%m-%d").ok()
}

/// Löscht Protokolle `<praefix>-JJJJ-MM-TT.log` in `ordner`, die älter als
/// `AUFBEWAHREN_TAGE` sind. Gibt die gelöschten Namen zurück. Andere Dateien
/// (auch die mit anderem Präfix) bleiben unberührt; Unterordner ebenso.
pub fn aufraeumen(ordner: &Path, praefix: &str, heute: NaiveDate) -> Vec<String> {
    let Ok(eintraege) = std::fs::read_dir(ordner) else { return Vec::new() };
    let mut weg = Vec::new();
    for e in eintraege.filter_map(Result::ok) {
        let Ok(name) = e.file_name().into_string() else { continue };
        let Some(datum) = datum_mit_praefix(&name, praefix) else { continue };
        if (heute - datum).num_days() > AUFBEWAHREN_TAGE && e.path().is_file() && std::fs::remove_file(e.path()).is_ok() {
            weg.push(name);
        }
    }
    weg.sort();
    weg
}

/// Was ein Protokoll aufräumt: (Ordner, Präfix) je Muster.
pub type Raeumen = Vec<(PathBuf, String)>;

/// Für den Dienst: `dienst-*.log` im Protokollordner und `bote-*.log` im
/// Unterordner `bote\`. Für den Boten (und alles andere): nur die eigenen.
pub fn raeumen_fuer(praefix: &str, ordner: &Path) -> Raeumen {
    let mut r = vec![(ordner.to_path_buf(), praefix.to_string())];
    if praefix == "dienst" {
        r.push((ordner.join(crate::bote::PROTOKOLL_UNTERORDNER), "bote".to_string()));
    }
    r
}

/// Räumt alle Muster auf; gibt die gelöschten Namen zurück.
fn alle_aufraeumen(raeumen: &Raeumen, heute: NaiveDate) -> Vec<String> {
    raeumen.iter().flat_map(|(o, p)| aufraeumen(o, p, heute)).collect()
}

type Uhr = Box<dyn Fn() -> NaiveDate + Send + Sync>;

/// Schreibt in die Datei des aktuellen Tages; wechselt sie beim Datumswechsel
/// und räumt dabei alte Protokolle weg (ein Dienst läuft oft wochenlang).
pub struct Tagesdatei {
    ordner: PathBuf,
    praefix: String,
    uhr: Uhr,
    raeumen: Raeumen,
    offen: Mutex<Option<(NaiveDate, File)>>,
}

impl Tagesdatei {
    /// Mit der Ortszeit als Uhr.
    pub fn neu(ordner: &Path, praefix: &str, raeumen: Raeumen) -> Arc<Tagesdatei> {
        Self::mit_uhr(ordner, praefix, raeumen, Box::new(|| chrono::Local::now().date_naive()))
    }

    /// Mit eigener Uhr (Tests).
    pub fn mit_uhr(ordner: &Path, praefix: &str, raeumen: Raeumen, uhr: Uhr) -> Arc<Tagesdatei> {
        Arc::new(Tagesdatei { ordner: ordner.to_path_buf(), praefix: praefix.to_string(), uhr, raeumen, offen: Mutex::new(None) })
    }

    /// Datei des heutigen Tages (nach der Uhr).
    pub fn aktuelle_datei(&self) -> PathBuf {
        self.ordner.join(dateiname(&self.praefix, (self.uhr)()))
    }

    fn schreiben(&self, buf: &[u8]) -> io::Result<()> {
        let heute = (self.uhr)();
        let mut offen = self.offen.lock().unwrap_or_else(|e| e.into_inner());
        if offen.as_ref().is_none_or(|(tag, _)| *tag != heute) {
            let wechsel = offen.is_some();
            let mut datei = OpenOptions::new().create(true).append(true).open(self.ordner.join(dateiname(&self.praefix, heute)))?;
            // Beim Start räumt `einrichten_mit` auf; hier nur beim Wechsel. Kein
            // `tracing` hier (wir sind mitten im Schreiben), darum direkt in die Datei.
            if wechsel {
                let weg = alle_aufraeumen(&self.raeumen, heute);
                if !weg.is_empty() {
                    let zeit = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S%.6f%:z");
                    let _ = writeln!(datei, "{zeit}  INFO Alte Protokolle gelöscht (Datumswechsel): {}", weg.join(", "));
                }
            }
            *offen = Some((heute, datei));
        }
        let (_, datei) = offen.as_mut().expect("eben geöffnet");
        datei.write_all(buf)?;
        datei.flush()
    }
}

/// Ein Schreiber für eine Zeile; `tracing` holt je Ereignis einen.
pub struct TagesSchreiber(Arc<Tagesdatei>);

impl Write for TagesSchreiber {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.schreiben(buf)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Hülle, damit `MakeWriter` für den geteilten Zeiger gilt.
#[derive(Clone)]
pub struct Protokolldatei(pub Arc<Tagesdatei>);

impl<'a> MakeWriter<'a> for Protokolldatei {
    type Writer = TagesSchreiber;

    fn make_writer(&'a self) -> Self::Writer {
        TagesSchreiber(self.0.clone())
    }
}

/// Feldformat der Datei. Ein eigener Typ, weil beide Schichten sonst die schon
/// formatierten Felder der Spans teilen (`FormattedFields<DefaultFields>`):
/// die Datei bekäme dann die Farben der Konsole mit.
struct DateiFelder(tracing_subscriber::fmt::format::DefaultFields);

impl<'w> tracing_subscriber::fmt::FormatFields<'w> for DateiFelder {
    fn format_fields<R: tracing_subscriber::field::RecordFields>(&self, writer: tracing_subscriber::fmt::format::Writer<'w>, fields: R) -> std::fmt::Result {
        self.0.format_fields(writer, fields)
    }
}

/// Filter: `RUST_LOG`, sonst wie bisher (der Bote schreibt als `dashboard_bote`).
fn filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("dashboard=info,dashboard_bote=info,tower_http=info"))
}

/// Richtet das Protokoll des Dienstes ein (`dienst-JJJJ-MM-TT.log`), siehe `einrichten_mit`.
pub fn einrichten(ordner: Option<&Path>, konsole: bool) -> Option<PathBuf> {
    einrichten_mit("dienst", ordner, konsole)
}

/// Richtet das Protokoll ein (einmal je Prozess), Dateien `<praefix>-JJJJ-MM-TT.log`
/// (`dienst` oder `bote`). `ordner`: Protokollordner, `None` = nur Konsole.
/// `konsole`: zusätzlich in die Konsole schreiben.
/// Lässt sich der Ordner nicht anlegen, bleibt nur die Konsole, mit Warnung.
/// Gibt die Datei des heutigen Tages zurück, wenn in eine geschrieben wird.
pub fn einrichten_mit(praefix: &str, ordner: Option<&Path>, konsole: bool) -> Option<PathBuf> {
    einrichten_mit_hinweis(praefix, ordner, konsole, None)
}

/// Wie `einrichten_mit`; `hinweis` steht als erste Zeile (Warnung) im
/// Protokoll, etwa warum der Bote in den Ersatzordner schreibt.
pub fn einrichten_mit_hinweis(praefix: &str, ordner: Option<&Path>, konsole: bool, hinweis: Option<&str>) -> Option<PathBuf> {
    let mut warnung = None;
    let raeumen = ordner.map(|o| raeumen_fuer(praefix, o)).unwrap_or_default();
    let datei = ordner.and_then(|o| match std::fs::create_dir_all(o) {
        Ok(()) => Some(Tagesdatei::neu(o, praefix, raeumen.clone())),
        Err(e) => {
            warnung = Some(format!("Der Protokollordner {} lässt sich nicht anlegen ({e}). Das Protokoll geht nur in die Konsole.", o.display()));
            None
        }
    });
    let pfad = datei.as_ref().map(|d| d.aktuelle_datei());
    let datei_schicht =
        datei.map(|d| tracing_subscriber::fmt::layer().with_ansi(false).fmt_fields(DateiFelder(Default::default())).with_writer(Protokolldatei(d)));
    // Ohne Datei bleibt die Konsole immer an, sonst ginge alles verloren.
    let konsole_schicht = (konsole || pfad.is_none()).then(tracing_subscriber::fmt::layer);
    let _ = tracing_subscriber::registry().with(filter()).with(konsole_schicht).with(datei_schicht).try_init();
    if let Some(h) = hinweis {
        tracing::warn!("{h}");
    }
    if let Some(w) = warnung {
        tracing::warn!("{w}");
    }
    if pfad.is_some() {
        let weg = alle_aufraeumen(&raeumen, chrono::Local::now().date_naive());
        if !weg.is_empty() {
            tracing::info!("Alte Protokolle gelöscht: {}", weg.join(", "));
        }
    }
    pfad
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ordner(name: &str) -> PathBuf {
        let o = std::env::temp_dir().join(format!("dashboard-protokoll-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&o);
        std::fs::create_dir_all(&o).unwrap();
        o
    }

    fn tag(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn dateiname_und_muster() {
        assert_eq!(dateiname("dienst", tag("2026-10-26")), "dienst-2026-10-26.log");
        assert_eq!(datum_aus_name("dienst-2026-10-26.log"), Some(tag("2026-10-26")));
        assert_eq!(datum_aus_name("bote-2026-01-02.log"), Some(tag("2026-01-02")));
        for fremd in [
            "dienst-2026-10-26.txt",
            "andere-2026-10-26.log",
            "dienst-2026-10-26.log.alt",
            "dienst-26-10-2026.log",
            "dienst-2026-13-01.log",
            "dienst-2026-10-26-x.log",
            "dienst.log",
        ] {
            assert_eq!(datum_aus_name(fremd), None, "{fremd}");
        }
    }

    #[test]
    fn wechselt_die_datei_beim_datumswechsel() {
        let o = ordner("wechsel");
        let heute = Arc::new(Mutex::new(tag("2026-10-26")));
        let uhr = heute.clone();
        let d = Tagesdatei::mit_uhr(&o, "dienst", Vec::new(), Box::new(move || *uhr.lock().unwrap()));
        let mw = Protokolldatei(d);
        mw.make_writer().write_all(b"erste Zeile\n").unwrap();
        mw.make_writer().write_all(b"zweite Zeile\n").unwrap();
        *heute.lock().unwrap() = tag("2026-10-27");
        mw.make_writer().write_all(b"neuer Tag\n").unwrap();
        assert_eq!(std::fs::read_to_string(o.join("dienst-2026-10-26.log")).unwrap(), "erste Zeile\nzweite Zeile\n");
        assert_eq!(std::fs::read_to_string(o.join("dienst-2026-10-27.log")).unwrap(), "neuer Tag\n");
        drop(mw);
        let _ = std::fs::remove_dir_all(&o);
    }

    /// Beim Datumswechsel räumt der Schreiber auf (Dienst: eigene Dateien im
    /// Ordner, die des Boten im Unterordner) und vermerkt es in der neuen Datei.
    #[test]
    fn raeumt_beim_datumswechsel_auf() {
        let o = ordner("wechsel-raeumen");
        let b = o.join("bote");
        std::fs::create_dir_all(&b).unwrap();
        for (ort, name) in [
            (&o, "dienst-2026-10-12.log"), // am 27. 15 Tage alt: weg
            (&o, "dienst-2026-10-13.log"), // 14 Tage: bleibt
            (&o, "bote-2026-01-01.log"),   // falscher Ordner für bote: bleibt
            (&b, "bote-2026-10-01.log"),   // weg
            (&b, "dienst-2026-01-01.log"), // falscher Ordner für dienst: bleibt
        ] {
            std::fs::write(ort.join(name), "x").unwrap();
        }
        let heute = Arc::new(Mutex::new(tag("2026-10-26")));
        let uhr = heute.clone();
        let d = Tagesdatei::mit_uhr(&o, "dienst", raeumen_fuer("dienst", &o), Box::new(move || *uhr.lock().unwrap()));
        let mw = Protokolldatei(d);
        // Erstes Öffnen ist kein Wechsel: nichts gelöscht.
        mw.make_writer().write_all(b"start\n").unwrap();
        assert!(o.join("dienst-2026-10-12.log").exists() && b.join("bote-2026-10-01.log").exists());
        *heute.lock().unwrap() = tag("2026-10-27");
        mw.make_writer().write_all(b"neuer Tag\n").unwrap();
        assert!(!o.join("dienst-2026-10-12.log").exists());
        assert!(!b.join("bote-2026-10-01.log").exists());
        for bleibt in [o.join("dienst-2026-10-13.log"), o.join("bote-2026-01-01.log"), b.join("dienst-2026-01-01.log")] {
            assert!(bleibt.exists(), "{}", bleibt.display());
        }
        let neu = std::fs::read_to_string(o.join("dienst-2026-10-27.log")).unwrap();
        assert!(neu.contains("Alte Protokolle gelöscht (Datumswechsel): dienst-2026-10-12.log, bote-2026-10-01.log\n"), "{neu}");
        assert!(neu.ends_with("neuer Tag\n"), "{neu}");
        drop(mw);
        let _ = std::fs::remove_dir_all(&o);
    }

    #[test]
    fn aufraeumen_nach_14_tagen_nur_eigene_dateien() {
        let o = ordner("aufraeumen");
        for name in [
            "dienst-2026-10-26.log",
            "dienst-2026-10-12.log", // genau 14 Tage: bleibt
            "dienst-2026-10-11.log", // 15 Tage: weg
            "bote-2026-09-01.log",   // anderes Präfix: bleibt hier
            "bote-2026-10-20.log",
            "notiz-2026-01-01.log", // fremdes Muster: bleibt
            "dienst-2026-01-01.txt",
            "dienst-x-2026-01-01.log",
        ] {
            std::fs::write(o.join(name), "x").unwrap();
        }
        let weg = aufraeumen(&o, "dienst", tag("2026-10-26"));
        assert_eq!(weg, ["dienst-2026-10-11.log"]);
        let weg = aufraeumen(&o, "bote", tag("2026-10-26"));
        assert_eq!(weg, ["bote-2026-09-01.log"]);
        let mut rest: Vec<String> = std::fs::read_dir(&o).unwrap().filter_map(|e| e.ok()?.file_name().into_string().ok()).collect();
        rest.sort();
        assert_eq!(
            rest,
            [
                "bote-2026-10-20.log",
                "dienst-2026-01-01.txt",
                "dienst-2026-10-12.log",
                "dienst-2026-10-26.log",
                "dienst-x-2026-01-01.log",
                "notiz-2026-01-01.log"
            ]
        );
        let _ = std::fs::remove_dir_all(&o);
    }

    #[test]
    fn raeumen_je_programm() {
        let o = Path::new("C:/p");
        assert_eq!(raeumen_fuer("dienst", o), [(o.to_path_buf(), "dienst".to_string()), (o.join("bote"), "bote".to_string())]);
        assert_eq!(raeumen_fuer("bote", o), [(o.to_path_buf(), "bote".to_string())]);
    }
}
