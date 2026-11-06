//! Anbindung an den Dienststeuerungs-Manager (SCM) mit der Kiste
//! windows-service. `starten` übergibt den Hauptfaden an den SCM; der ruft
//! `dienst_main` in einem eigenen Faden auf. Dort läuft `super::ablauf` mit
//! `ScmStatus` als Meldestelle.
//!
//! Ohne SCM (etwa in der Konsole) schlägt `starten` sofort fehl; testbar ist
//! hier nur die Zustandsfolge in `super`, mit einer Attrappe.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use tokio::sync::watch;
use windows_service::service::{ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult, ServiceStatusHandle};
use windows_service::{define_windows_service, service_dispatcher};

use super::{Dienststatus, StartFehler, Stoppart, Zustand, ablauf, dienst_laufen, stopp_anfordern};

/// Was `dienst_main` braucht; vor dem Dispatcher gesetzt.
struct Auftrag {
    name: String,
    einstellungen: PathBuf,
}

static AUFTRAG: OnceLock<Auftrag> = OnceLock::new();

define_windows_service!(ffi_service_main, dienst_main);

/// `dashboard.exe --dienst --einstellungen <datei> [--dienstname <name>]`.
/// Blockiert, bis der Dienst beendet ist. Fehler hier heißen meist: nicht vom
/// SCM gestartet (etwa von Hand in der Konsole).
pub fn starten(name: &str, einstellungen: PathBuf) -> Result<(), StartFehler> {
    let _ = AUFTRAG.set(Auftrag { name: name.to_string(), einstellungen });
    service_dispatcher::start(name, ffi_service_main).map_err(|e| {
        StartFehler::Sonst(format!(
            "Der Dienst „{name}“ lässt sich nicht beim Dienststeuerungs-Manager anmelden ({e}). \
             --dienst ist nur für den Start durch den Dienststeuerungs-Manager gedacht; von Hand bitte --konsole --einstellungen <datei> nehmen."
        ))
    })
}

/// Meldet Zustände an den SCM.
struct ScmStatus {
    handle: OnceLock<ServiceStatusHandle>,
    schritt: AtomicU32,
}

impl Dienststatus for ScmStatus {
    fn melden(&self, zustand: Zustand) {
        let Some(handle) = self.handle.get() else { return };
        let (state, controls, exit_code, wait_hint) = match zustand {
            Zustand::StartPending => (ServiceState::StartPending, ServiceControlAccept::empty(), ServiceExitCode::NO_ERROR, 30),
            Zustand::Running => (ServiceState::Running, ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN, ServiceExitCode::NO_ERROR, 0),
            // Frist beim Beenden: höchstens BEENDEN_HOECHSTENS (8 s) für offene
            // Antworten, dann bis 2 s für die Laufzeit; mit Luft 12 s. Der SCM wartet
            // nach einem Stopp standardmäßig 20 s, wir bleiben also darunter. Beim
            // Herunterfahren gilt der Hinweis nicht (Windows wartet dann nur
            // WaitToKillServiceTimeout, hier 5 s); dafür sind es dann nur 3 s
            // (BEENDEN_BEIM_HERUNTERFAHREN, Stoppart::Herunterfahren).
            Zustand::StopPending => {
                (ServiceState::StopPending, ServiceControlAccept::empty(), ServiceExitCode::NO_ERROR, super::BEENDEN_HOECHSTENS.as_secs() + 4)
            }
            Zustand::Stopped(0) => (ServiceState::Stopped, ServiceControlAccept::empty(), ServiceExitCode::NO_ERROR, 0),
            Zustand::Stopped(n) => (ServiceState::Stopped, ServiceControlAccept::empty(), ServiceExitCode::ServiceSpecific(n), 0),
        };
        // Bei wartenden Zuständen zählt der Schritt hoch, sonst 0.
        let checkpoint = if wait_hint > 0 { self.schritt.fetch_add(1, Ordering::Relaxed) + 1 } else { 0 };
        let status = ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: state,
            controls_accepted: controls,
            exit_code,
            checkpoint,
            wait_hint: Duration::from_secs(wait_hint),
            process_id: None,
        };
        if let Err(e) = handle.set_service_status(status) {
            tracing::warn!("Zustand {zustand:?} ließ sich nicht melden: {e}");
        }
    }
}

/// Vom SCM in einem eigenen Faden gerufen. Eine Panik darf hier nicht
/// unbemerkt den Faden beenden (der SCM sähe den Dienst dann ewig „läuft“
/// oder „wird beendet“): sie endet als Stopped(9) mit einem Satz im Protokoll.
/// Paniken in `dienst_laufen` fängt schon `ablauf`; dies hier fängt den Rest.
fn dienst_main(_argumente: Vec<OsString>) {
    let status = Arc::new(ScmStatus { handle: OnceLock::new(), schritt: AtomicU32::new(0) });
    let innen = status.clone();
    let ergebnis = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || dienst_main_innen(innen)));
    if let Err(nutzlast) = ergebnis {
        tracing::error!("Der Dienst ist abgestürzt und meldet Code 9: {}", super::panik_text(&*nutzlast));
        status.melden(Zustand::Stopped(9));
    }
}

fn dienst_main_innen(status: Arc<ScmStatus>) {
    let Some(auftrag) = AUFTRAG.get() else { return };
    // Trägt die Frist für offene Antworten; `None` heißt: läuft.
    let (stopp_tx, stopp_rx) = watch::channel(None);
    let handler_status = status.clone();
    let handler = move |steuerung| match steuerung {
        ServiceControl::Stop => {
            stopp_anfordern(&*handler_status, &stopp_tx, Stoppart::Stopp);
            ServiceControlHandlerResult::NoError
        }
        // Windows fährt herunter: kürzere Frist (WaitToKillServiceTimeout).
        ServiceControl::Shutdown => {
            stopp_anfordern(&*handler_status, &stopp_tx, Stoppart::Herunterfahren);
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    };
    let handle = match service_control_handler::register(&auftrag.name, handler) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("Steuer-Handler für „{}“ nicht angemeldet: {e}", auftrag.name);
            return;
        }
    };
    let _ = status.handle.set(handle);
    ablauf(&*status, |bereit| dienst_laufen(&auftrag.einstellungen, stopp_rx, bereit));
}
