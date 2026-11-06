//! Der Wächter des Boten: `dashboard-bote.exe --waechter --einstellungen <datei>`
//! startet sich selbst als Kind (`--einstellungen <datei>`, ohne `--waechter`)
//! und wartet. Endet das Kind mit 0 (Strg+C, Abmelden, gewolltes Ende), endet
//! auch der Wächter; endet es mit 2 (Einstellungen oder Aufruf falsch), endet
//! der Wächter ohne Neustart ebenfalls mit 2. Sonst startet er es neu: nach 5 s, 30 s, 2 min und danach
//! alle 2 min; bei mehr als 5 Abstürzen in der letzten Stunde nur noch alle
//! 10 min. Jeder Absturz und Neustart steht im Protokoll des Boten, in eigenen
//! Zeilen mit „Wächter:“ vorn. So verlangt es die Entscheidung („Neustart bei
//! Absturz durch eigene Schleife“); die Aufgabenplanung startet nur den Wächter.
//!
//! Das Kind steckt in einem Job-Objekt des Wächters (KILL_ON_JOB_CLOSE): Endet
//! der Wächter, auch hart (Stop-ScheduledTask, TerminateProcess), schließt
//! Windows das Handle und beendet den Boten mit. Steckt der Wächter selbst schon
//! in einem Job (Aufgabenplanung), wird das eigene darin verschachtelt (ab Windows 8).
//!
//! Endet das Kind mit 4 (Pipe belegt), sucht der Wächter verwaiste Boten
//! (gleiches Programm, Konto und Einstellungsdatei, Elternprozess weg) und beendet sie;
//! dann gibt es genau einen weiteren Start. Scheitert auch der mit 4, hält ein
//! anderer, lebender Bote die Pipe, und der Wächter endet selbst mit 4.

use std::collections::VecDeque;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use tracing::{error, info, warn};

/// Pausen vor dem 1., 2. und 3. Neustart (innerhalb einer Stunde).
pub const PAUSEN: [Duration; 3] = [Duration::from_secs(5), Duration::from_secs(30), Duration::from_secs(120)];
/// Pause ab dem 4. Neustart.
pub const PAUSE_DANACH: Duration = Duration::from_secs(120);
/// Pause, wenn es in der letzten Stunde mehr als `VIELE` Abstürze gab.
pub const PAUSE_VIELE: Duration = Duration::from_secs(10 * 60);
/// Mehr Abstürze als so viele je Stunde: nur noch alle `PAUSE_VIELE`.
pub const VIELE: usize = 5;
/// Zeitraum, in dem Abstürze zählen.
pub const STUNDE: Duration = Duration::from_secs(60 * 60);

/// Exit-Code des Boten bei falschen Einstellungen oder falschem Aufruf: kein
/// Neustart, der Wächter endet selbst mit diesem Code.
pub const ENDE_EINSTELLUNGEN: i32 = 2;

/// Exit-Code des Boten, wenn die Pipe schon belegt ist. Der Wächter beendet
/// dann verwaiste Boten und versucht es genau einmal neu; scheitert auch das,
/// endet er selbst mit diesem Code (kein Neustart-Takt).
pub const ENDE_PIPE_BELEGT: i32 = 4;

/// Pause vor dem letzten Versuch, wenn ein verwaister Bote beendet wurde.
pub const PAUSE_NACH_VERWAIST: Duration = Duration::from_secs(1);

/// Umgebungsvariable nur für Tests: der Bote endet gleich nach dem Start mit
/// diesem Code (so lässt sich ein Absturz nachstellen).
pub const TEST_EXIT: &str = "DASHBOARD_BOTE_TEST_EXIT";

/// Pause vor dem Neustart, wenn es in der letzten Stunde `abstuerze`
/// Abstürze gab (diesen eingeschlossen, also mindestens 1).
pub fn pause(abstuerze: usize) -> Duration {
    if abstuerze > VIELE {
        return PAUSE_VIELE;
    }
    PAUSEN.get(abstuerze.saturating_sub(1)).copied().unwrap_or(PAUSE_DANACH)
}

/// Merkt sich Abstürze der letzten Stunde.
#[derive(Debug, Default)]
pub struct Abstuerze(VecDeque<Instant>);

impl Abstuerze {
    /// Vermerkt einen Absturz zum Zeitpunkt `jetzt`, vergisst die älter als
    /// eine Stunde und gibt die Zahl in der letzten Stunde zurück.
    pub fn vermerken(&mut self, jetzt: Instant) -> usize {
        self.0.push_back(jetzt);
        while self.0.front().is_some_and(|t| jetzt.duration_since(*t) > STUNDE) {
            self.0.pop_front();
        }
        self.0.len()
    }
}

/// Lesbare Dauer für das Protokoll: `5 s`, `2 min`.
pub fn dauer_text(d: Duration) -> String {
    let s = d.as_secs();
    if s >= 60 && s.is_multiple_of(60) { format!("{} min", s / 60) } else { format!("{s} s") }
}

/// Die Schleife des Wächters. `programm`: dieses Programm (dashboard-bote.exe).
/// Gibt 0 zurück, wenn das Kind regulär endete.
pub fn laufen(programm: &Path, einstellungen: &Path) -> i32 {
    let mut abstuerze = Abstuerze::default();
    info!("Wächter: startet den Boten ({}).", programm.display());
    // Lebt so lange wie der Wächter; erst das Ende des Prozesses schließt es
    // hart (bei der Rückkehr aus `laufen` ist das Kind schon beendet).
    let job = job::anlegen();
    // Nach einem Code 4 gesetzt: der nächste Start ist der letzte Versuch.
    let mut belegt_versucht = false;
    loop {
        // Ohne Fenster (dienst::programme), wie jeder Kindprozess.
        let ergebnis = crate::dienst::programme::eigener_befehl(programm)
            .arg("--einstellungen")
            .arg(einstellungen)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .and_then(|mut kind| {
                // Gleich nach dem Start ins Job-Objekt. Zwischen Start und
                // Zuweisung liegen Millisekunden; endete der Wächter genau da,
                // bliebe der Bote übrig, und der nächste Wächter räumt ihn als
                // verwaist weg (Code 4). Darum genügt das ohne CREATE_SUSPENDED.
                job::zuweisen(job.as_ref(), &kind);
                info!("Wächter: Bote gestartet (Prozess {}).", kind.id());
                kind.wait()
            });
        let code = ergebnis.as_ref().ok().and_then(|s| s.code());
        let wie = match ergebnis {
            Ok(s) if s.code() == Some(0) => {
                info!("Wächter: Der Bote ist regulär beendet (Code 0); der Wächter endet auch.");
                return 0;
            }
            // Einstellungen oder Aufruf falsch: ein Neustart änderte nichts.
            Ok(s) if s.code() == Some(ENDE_EINSTELLUNGEN) => {
                error!("Wächter: Einstellungen fehlerhaft, kein Neustart (der Bote endete mit Code 2).");
                return ENDE_EINSTELLUNGEN;
            }
            Ok(s) => match s.code() {
                Some(c) => format!("mit Code {c} beendet"),
                None => "ohne Code beendet".to_string(),
            },
            Err(e) => {
                error!("Wächter: Der Bote lässt sich nicht starten ({e}).");
                "nicht gestartet".to_string()
            }
        };
        if code == Some(ENDE_PIPE_BELEGT) {
            if belegt_versucht {
                error!("Wächter: Die Pipe ist von einem anderen Boten belegt; dieser Wächter beendet sich.");
                return ENDE_PIPE_BELEGT;
            }
            belegt_versucht = true;
            let beendet = job::verwaiste_beenden(programm, einstellungen);
            let p = if beendet > 0 { PAUSE_NACH_VERWAIST } else { pause(abstuerze.vermerken(Instant::now())) };
            warn!(
                "Wächter: Der Bote ist {wie} (Pipe belegt; {}); letzter Versuch in {}.",
                if beendet > 0 { "verwaisten Boten beendet" } else { "kein verwaister Bote gefunden" },
                dauer_text(p)
            );
            std::thread::sleep(p);
            info!("Wächter: Neustart des Boten.");
            continue;
        }
        belegt_versucht = false;
        let n = abstuerze.vermerken(Instant::now());
        let p = pause(n);
        warn!("Wächter: Der Bote ist {wie} ({n}. Absturz in der letzten Stunde); Neustart in {}.", dauer_text(p));
        std::thread::sleep(p);
        info!("Wächter: Neustart des Boten.");
    }
}

/// Job-Objekt und verwaiste Boten (nur Windows; sonst ohne Wirkung).
#[cfg(windows)]
mod job {
    use std::os::windows::io::AsRawHandle;
    use std::path::Path;

    use tracing::{info, warn};
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, IsProcessInJob, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectExtendedLimitInformation, SetInformationJobObject,
    };
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
    };

    /// Das Job-Objekt des Wächters.
    pub struct Job(HANDLE);

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: eigenes, gültiges Handle. Schließen beendet, was noch im
            // Job steckt (KILL_ON_JOB_CLOSE).
            unsafe { CloseHandle(self.0) };
        }
    }

    /// Legt das Job-Objekt an. Scheitert es, läuft der Wächter ohne (mit Warnung).
    pub fn anlegen() -> Option<Job> {
        let mut drin = 0;
        // SAFETY: Pseudo-Handle des eigenen Prozesses; kein Job angegeben heißt irgendeiner.
        if unsafe { IsProcessInJob(GetCurrentProcess(), std::ptr::null_mut(), &mut drin) } != 0 && drin != 0 {
            info!("Wächter: läuft schon in einem Job-Objekt (etwa dem der Aufgabenplanung); das eigene wird darin verschachtelt.");
        }
        // SAFETY: ohne Sicherheitsattribute und ohne Namen.
        let h = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if h.is_null() {
            warn!("Wächter: Das Job-Objekt lässt sich nicht anlegen ({}); der Bote überlebt ein hartes Ende des Wächters.", std::io::Error::last_os_error());
            return None;
        }
        let job = Job(h);
        // SAFETY: Nullen sind für diese Struktur ein gültiger Wert.
        let mut grenzen: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        grenzen.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: Zeiger und Größe passen zur Informationsklasse.
        let ok = unsafe {
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&grenzen as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if ok == 0 {
            warn!("Wächter: Das Job-Objekt lässt sich nicht einstellen ({}); der Bote überlebt ein hartes Ende des Wächters.", std::io::Error::last_os_error());
            return None;
        }
        Some(job)
    }

    /// Legt das Kind ins Job-Objekt (Warnung, wenn es nicht gelingt).
    pub fn zuweisen(job: Option<&Job>, kind: &std::process::Child) {
        let Some(job) = job else { return };
        // SAFETY: beide Handles gültig; das Kind-Handle gehört `kind`.
        if unsafe { AssignProcessToJobObject(job.0, kind.as_raw_handle() as HANDLE) } == 0 {
            warn!(
                "Wächter: Der Bote {} lässt sich nicht ins Job-Objekt legen ({}); er überlebt ein hartes Ende des Wächters.",
                kind.id(),
                std::io::Error::last_os_error()
            );
        }
    }

    fn gleich(a: &Path, b: &Path) -> bool {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    }

    fn kanonisch(p: &Path) -> std::path::PathBuf {
        std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
    }

    /// Beendet verwaiste Boten: dasselbe Programm, dasselbe Konto, dieselbe
    /// Einstellungsdatei (also dieselbe Pipe; Boten anderer Einstellungen,
    /// etwa aus Tests, bleiben unberührt), kein Wächter, und der Elternprozess
    /// lebt nicht mehr (oder seine Nummer gehört inzwischen einem jüngeren
    /// Prozess). Gibt die Zahl der beendeten zurück.
    pub fn verwaiste_beenden(programm: &Path, einstellungen: &Path) -> usize {
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
        let Ok(ich_sid) = crate::bote::win::eigene_sid() else { return 0 };
        let programm = kanonisch(programm);
        let einstellungen = kanonisch(einstellungen);
        let mut sys = System::new();
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_cmd(UpdateKind::Always));
        let ich = std::process::id();
        let mut beendet = 0;
        for (pid, p) in sys.processes() {
            let pid = pid.as_u32();
            let cmd = p.cmd();
            if pid == ich || cmd.iter().any(|a| a == "--waechter") {
                continue;
            }
            let datei = cmd.iter().skip_while(|a| *a != "--einstellungen").nth(1);
            if !datei.is_some_and(|d| gleich(&kanonisch(Path::new(d)), &einstellungen)) {
                continue;
            }
            let verwaist = match p.parent().and_then(|e| sys.process(e)) {
                None => true,
                Some(eltern) => eltern.start_time() > p.start_time(),
            };
            if !verwaist {
                continue;
            }
            let Ok(pfad) = crate::bote::win::prozess_programm(pid) else { continue };
            let pfad = std::fs::canonicalize(&pfad).unwrap_or(pfad);
            if !gleich(&pfad, &programm) || crate::bote::win::prozess_sid(pid).ok().as_deref() != Some(ich_sid.as_str()) {
                continue;
            }
            // SAFETY: nur Beenden und Warten; das Handle wird gleich geschlossen.
            let h = unsafe { OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, 0, pid) };
            if h.is_null() {
                warn!("Wächter: Der verwaiste Bote {pid} lässt sich nicht öffnen ({}).", std::io::Error::last_os_error());
                continue;
            }
            // SAFETY: gültiges Handle mit PROCESS_TERMINATE und SYNCHRONIZE.
            let ok = unsafe { TerminateProcess(h, 1) != 0 && WaitForSingleObject(h, 5000) == WAIT_OBJECT_0 };
            let fehler = std::io::Error::last_os_error();
            // SAFETY: von OpenProcess.
            unsafe { CloseHandle(h) };
            if ok {
                info!("Wächter: verwaisten Boten {pid} beendet");
                beendet += 1;
            } else {
                warn!("Wächter: Der verwaiste Bote {pid} lässt sich nicht beenden ({fehler}).");
            }
        }
        beendet
    }
}

#[cfg(not(windows))]
mod job {
    pub struct Job;

    pub fn anlegen() -> Option<Job> {
        None
    }

    pub fn zuweisen(_job: Option<&Job>, _kind: &std::process::Child) {}

    pub fn verwaiste_beenden(_programm: &std::path::Path, _einstellungen: &std::path::Path) -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pausen_nach_zahl_der_abstuerze() {
        let s = |n| pause(n).as_secs();
        assert_eq!([s(1), s(2), s(3), s(4), s(5)], [5, 30, 120, 120, 120]);
        assert_eq!(s(6), 600, "mehr als 5 je Stunde");
        assert_eq!(s(50), 600);
        assert_eq!(s(0), 5, "nie weniger als die erste Pause");
    }

    #[test]
    fn abstuerze_zaehlen_nur_eine_stunde() {
        let t0 = Instant::now();
        let mut a = Abstuerze::default();
        assert_eq!(a.vermerken(t0), 1);
        assert_eq!(a.vermerken(t0 + Duration::from_secs(60)), 2);
        assert_eq!(a.vermerken(t0 + Duration::from_secs(30 * 60)), 3);
        // Eine Stunde und eine Sekunde nach dem ersten: der erste zählt nicht mehr.
        assert_eq!(a.vermerken(t0 + STUNDE + Duration::from_secs(1)), 3);
        assert_eq!(a.vermerken(t0 + 3 * STUNDE), 1);
    }

    #[test]
    fn dauer_lesbar() {
        assert_eq!(dauer_text(Duration::from_secs(5)), "5 s");
        assert_eq!(dauer_text(Duration::from_secs(120)), "2 min");
        assert_eq!(dauer_text(Duration::from_secs(90)), "90 s");
    }

    #[test]
    fn pipe_belegt_passt_zum_server() {
        #[cfg(windows)]
        assert_eq!(ENDE_PIPE_BELEGT, crate::bote::server::ENDE_PIPE_BELEGT);
        assert_ne!(ENDE_PIPE_BELEGT, ENDE_EINSTELLUNGEN);
    }
}
