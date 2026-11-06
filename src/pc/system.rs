//! Zustand des PCs: Prozessor, Speicher, Platten, Laufzeit, ausstehender
//! Neustart, Defender. Wird im Hintergrund regelmäßig neu eingesammelt,
//! damit jede Seite sofort antwortet.

use serde::Serialize;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use ts_rs::TS;

use chrono::{DateTime, Local};
use sysinfo::{Disks, ProcessesToUpdate, System};
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

pub type SharedSnapshot = Arc<RwLock<Snapshot>>;

#[derive(Debug, Clone, Serialize, Default, TS)]
pub struct Snapshot {
    #[ts(type = "string | null")]
    pub taken_at: Option<DateTime<Local>>,
    pub host: String,
    pub os: String,
    pub cpu_percent: f32,
    pub cpu_count: usize,
    pub mem_used: u64,
    pub mem_total: u64,
    pub disks: Vec<DiskInfo>,
    pub uptime_secs: u64,
    /// Wie voll der Prozessor gerade ist (Stufe nach [`BelegungsSchwellen`]).
    pub cpu_belegung: Belegung,
    /// Wie voll der Arbeitsspeicher ist.
    pub mem_belegung: Belegung,
    /// Ab wie viel Prozent eine Belegung `hoch` und `kritisch` ist (Fakt, damit
    /// die Oberfläche die Schwelle nennen kann; gerechnet wird hier).
    pub belegung_schwellen: BelegungsSchwellen,
    /// Ob ein Neustart aussteht, mit Gründen.
    pub neustart: NeustartLage,
    pub defender: DefenderStatus,
    /// Namen aller laufenden Programme (klein geschrieben), für Prozess-Checks.
    #[serde(skip)]
    pub process_names: HashSet<String>,
    /// Verlauf der letzten Minuten für die Verlaufskurven.
    pub history: Vec<Sample>,
}

#[derive(Debug, Clone, Serialize, Copy, TS)]
pub struct Sample {
    #[ts(type = "string")]
    pub at: DateTime<Local>,
    /// `null`, wenn die Prozessorlast unbekannt war (Etappe 3a+: eine Lücke, keine 0).
    pub cpu: Option<f32>,
    pub mem: f32,
}

/// So viele Messwerte bleiben im Speicher (alle 5 s, also 15 Minuten).
pub const HISTORY_LEN: usize = 180;

impl Snapshot {
    pub fn mem_percent(&self) -> f32 {
        percent(self.mem_used, self.mem_total)
    }

    pub fn process_running(&self, name: &str) -> bool {
        self.process_names.contains(&name.to_lowercase())
    }

    /// Die erste Messung nach dem Start: Prozent 0, Belegung aber
    /// `unbekannt`, damit die Oberfläche die ersten Sekunden nicht grün zeigt.
    /// Sie landet weder in `history` noch in `pc_verlauf` als 0 (Lücke).
    pub fn erste_messung(&mut self) {
        self.cpu_unbekannt();
    }

    /// Setzt die Prozessorlast einer Messung (Etappe 3a+). `None` (GetSystemTimes
    /// scheiterte oder kein Abstand) und eine Folge von genau 0 % ([`NullFolge`])
    /// geben `unbekannt` mit 0 %; es gibt keinen Rückfall auf sysinfo.
    pub fn cpu_setzen(&mut self, prozent: Option<f32>, nullen: &mut NullFolge) {
        match prozent {
            Some(p) if !nullen.melden(p) => {
                self.cpu_percent = p;
                self.belegung_rechnen();
            }
            Some(_) => self.cpu_unbekannt(),
            None => {
                *nullen = NullFolge::default();
                self.cpu_unbekannt();
            }
        }
    }

    fn cpu_unbekannt(&mut self) {
        self.cpu_percent = 0.0;
        self.belegung_rechnen();
        self.cpu_belegung = Belegung::Unbekannt;
    }

    /// Ob die Prozessorlast gemessen ist (sonst eine Lücke im Verlauf).
    pub fn cpu_gemessen(&self) -> bool {
        self.cpu_belegung != Belegung::Unbekannt
    }

    /// Rechnet die Belegung von Prozessor, Arbeitsspeicher und Platten neu
    /// (nach jeder Messung und wenn `cpu_percent` nachträglich gesetzt wird).
    /// Vor der ersten Messung (`taken_at` leer) ist alles `unbekannt`.
    pub fn belegung_rechnen(&mut self) {
        let gemessen = self.taken_at.is_some();
        self.cpu_belegung = if gemessen { Belegung::aus_prozent(self.cpu_percent) } else { Belegung::Unbekannt };
        self.mem_belegung = if gemessen { Belegung::aus_anteil(self.mem_used, self.mem_total) } else { Belegung::Unbekannt };
        for d in &mut self.disks {
            d.belegung = Belegung::aus_anteil(d.used, d.total);
        }
        self.belegung_schwellen = BelegungsSchwellen::default();
    }
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct DiskInfo {
    pub name: String,
    pub mount: String,
    pub total: u64,
    pub used: u64,
    /// Wie voll die Platte ist (Stufe nach [`BelegungsSchwellen`]).
    pub belegung: Belegung,
}

// ----- Belegung (Etappe 3a-4) -----

/// Ab so viel Prozent ist eine Belegung `hoch`.
pub const BELEGUNG_HOCH_AB: u8 = 70;
/// Ab so viel Prozent ist eine Belegung `kritisch`.
pub const BELEGUNG_KRITISCH_AB: u8 = 90;

/// Wie voll Prozessor, Arbeitsspeicher oder eine Platte ist. Die Schwellen
/// stehen als Konstanten hier und gehen als Fakt mit ([`BelegungsSchwellen`]);
/// die Farbe wählt die Oberfläche.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Belegung {
    /// Unter [`BELEGUNG_HOCH_AB`] Prozent.
    Normal,
    /// Ab [`BELEGUNG_HOCH_AB`] Prozent.
    Hoch,
    /// Ab [`BELEGUNG_KRITISCH_AB`] Prozent.
    Kritisch,
    /// Noch nicht gemessen oder ohne Größe (Gesamt 0).
    #[default]
    Unbekannt,
}

impl Belegung {
    /// Aus einem Anteil, ohne Rundung: genau 70 % ist `hoch`, genau 90 % `kritisch`.
    pub fn aus_anteil(teil: u64, ganz: u64) -> Self {
        if ganz == 0 {
            return Self::Unbekannt;
        }
        let (teil, ganz) = (u128::from(teil) * 100, u128::from(ganz));
        if teil >= ganz * u128::from(BELEGUNG_KRITISCH_AB) {
            Self::Kritisch
        } else if teil >= ganz * u128::from(BELEGUNG_HOCH_AB) {
            Self::Hoch
        } else {
            Self::Normal
        }
    }

    /// Aus Prozent (Prozessor); keine Zahl ist `unbekannt`.
    pub fn aus_prozent(p: f32) -> Self {
        if !p.is_finite() {
            Self::Unbekannt
        } else if p >= f32::from(BELEGUNG_KRITISCH_AB) {
            Self::Kritisch
        } else if p >= f32::from(BELEGUNG_HOCH_AB) {
            Self::Hoch
        } else {
            Self::Normal
        }
    }
}

/// Die Schwellen der Belegung in Prozent, als Fakt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub struct BelegungsSchwellen {
    pub hoch_ab: u8,
    pub kritisch_ab: u8,
}

impl Default for BelegungsSchwellen {
    fn default() -> Self {
        Self { hoch_ab: BELEGUNG_HOCH_AB, kritisch_ab: BELEGUNG_KRITISCH_AB }
    }
}

// ----- Neustart (Etappe 3a-4) -----

/// Ob ein Neustart aussteht.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Neustart {
    Aussteht,
    NichtNoetig,
    /// Nicht lesbar (etwa außerhalb von Windows) oder noch nicht gelesen.
    #[default]
    Unbekannt,
}

/// Warum ein Neustart aussteht: welcher Eintrag in der Registry ihn verlangt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum NeustartGrund {
    /// `WindowsUpdate\Auto Update\RebootRequired`.
    WindowsUpdate,
    /// `Component Based Servicing\RebootPending`.
    Systemkomponenten,
    /// `PendingFileRenameOperations`: Dateien warten auf Austausch.
    DateienWarten,
}

/// Stand des Neustarts: `neustart` und, wenn er aussteht, die Gründe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default, TS)]
pub struct NeustartLage {
    pub neustart: Neustart,
    /// Leer, außer bei `aussteht`.
    pub gruende: Vec<NeustartGrund>,
}

impl NeustartLage {
    /// Aus den gefundenen Gründen: keiner heißt `nicht-noetig`.
    pub fn aus_gruenden(gruende: Vec<NeustartGrund>) -> Self {
        let neustart = if gruende.is_empty() { Neustart::NichtNoetig } else { Neustart::Aussteht };
        Self { neustart, gruende }
    }
}

// ----- Defender (Etappe 3a-4) -----

/// Signaturen, die älter sind als so viele Tage, sind `signatur-alt`; genau
/// so alt ist noch in Ordnung.
pub const SIGNATUR_SCHWELLE_TAGE: u32 = 3;

/// Befund zum Defender.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default, TS)]
#[serde(rename_all = "kebab-case")]
pub enum DefenderBefund {
    /// Virenschutz und Echtzeitschutz an, Signaturen frisch.
    InOrdnung,
    /// Mindestens ein Grund in `gruende`.
    Ansehen,
    /// Noch nicht gelesen oder die Abfrage scheiterte (Grund in `gruende`).
    #[default]
    Unbekannt,
}

/// Warum der Defender `ansehen` oder `unbekannt` ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum DefenderGrund {
    VirenschutzAus,
    EchtzeitAus,
    /// Älter als `signatur_schwelle_tage`.
    SignaturAlt,
    /// Virenschutz und Echtzeit an, aber das Alter der Signaturen fehlt.
    SignaturAlterUnbekannt,
    /// Die erste Abfrage läuft noch.
    NochNichtGelesen,
    /// Die Abfrage scheiterte (dann steht der Grund in `error`), oder sie
    /// lief durch, lieferte aber Virenschutz oder Echtzeit nicht (dann ohne
    /// `error`).
    AbfrageFehlgeschlagen,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct DefenderStatus {
    #[ts(type = "string | null")]
    pub checked_at: Option<DateTime<Local>>,
    pub antivirus_enabled: Option<bool>,
    pub realtime_enabled: Option<bool>,
    pub signature_version: Option<String>,
    pub signature_updated: Option<String>,
    pub signature_age_days: Option<u32>,
    pub last_quick_scan: Option<String>,
    pub error: Option<String>,
    /// Die Regel (siehe [`DefenderStatus::bewerten`]), hier gerechnet.
    pub befund: DefenderBefund,
    /// Kennungen in kebab-case, nie Sätze; bei `in-ordnung` leer.
    pub gruende: Vec<DefenderGrund>,
    /// [`SIGNATUR_SCHWELLE_TAGE`] als Fakt.
    pub signatur_schwelle_tage: u32,
}

impl Default for DefenderStatus {
    fn default() -> Self {
        Self {
            checked_at: None,
            antivirus_enabled: None,
            realtime_enabled: None,
            signature_version: None,
            signature_updated: None,
            signature_age_days: None,
            last_quick_scan: None,
            error: None,
            befund: DefenderBefund::Unbekannt,
            gruende: vec![DefenderGrund::NochNichtGelesen],
            signatur_schwelle_tage: SIGNATUR_SCHWELLE_TAGE,
        }
    }
}

impl DefenderStatus {
    /// Rechnet `befund` und `gruende` aus den gelesenen Werten:
    ///
    /// - noch nie gelesen: `unbekannt` (`noch-nicht-gelesen`)
    /// - Fehler bei der Abfrage: `unbekannt` (`abfrage-fehlgeschlagen`)
    /// - je ein Grund für Virenschutz aus, Echtzeit aus und Signaturen älter
    ///   als [`SIGNATUR_SCHWELLE_TAGE`]; mit einem davon `ansehen`, auch wenn
    ///   ein anderer Wert fehlt
    /// - sonst fehlt Virenschutz oder Echtzeit: `unbekannt` (`abfrage-fehlgeschlagen`);
    ///   fehlt nur das Signaturalter: `unbekannt` (`signatur-alter-unbekannt`)
    /// - sonst `in-ordnung`. Unbekannt ist nie in Ordnung (Abnahme 3a-4).
    pub fn bewerten(mut self) -> Self {
        self.signatur_schwelle_tage = SIGNATUR_SCHWELLE_TAGE;
        let (befund, gruende) = if self.checked_at.is_none() {
            (DefenderBefund::Unbekannt, vec![DefenderGrund::NochNichtGelesen])
        } else if self.error.is_some() {
            (DefenderBefund::Unbekannt, vec![DefenderGrund::AbfrageFehlgeschlagen])
        } else {
            let mut g = Vec::new();
            if self.antivirus_enabled == Some(false) {
                g.push(DefenderGrund::VirenschutzAus);
            }
            if self.realtime_enabled == Some(false) {
                g.push(DefenderGrund::EchtzeitAus);
            }
            if self.signature_age_days.is_some_and(|t| t > SIGNATUR_SCHWELLE_TAGE) {
                g.push(DefenderGrund::SignaturAlt);
            }
            if !g.is_empty() {
                (DefenderBefund::Ansehen, g)
            } else if self.antivirus_enabled.is_none() || self.realtime_enabled.is_none() {
                (DefenderBefund::Unbekannt, vec![DefenderGrund::AbfrageFehlgeschlagen])
            } else if self.signature_age_days.is_none() {
                (DefenderBefund::Unbekannt, vec![DefenderGrund::SignaturAlterUnbekannt])
            } else {
                (DefenderBefund::InOrdnung, g)
            }
        };
        self.befund = befund;
        self.gruende = gruende;
        self
    }
}

pub fn percent(part: u64, whole: u64) -> f32 {
    if whole == 0 { 0.0 } else { (part as f64 * 100.0 / whole as f64) as f32 }
}

// ----- Prozessor-Auslastung (Hotfix 3a-4) -----
//
// Im Dienst lieferte sysinfo (`global_cpu_usage`, über PDH) dauerhaft genau
// 0 %, während Windows 65 % meldete. GetSystemTimes zählt Leerlauf-, Kern- und
// Benutzerzeit aller Prozessoren und geht in jedem Konto, auch im Dienst.

/// Gezählte Zeiten aller Prozessoren in 100-ns-Schritten; `kern` enthält den Leerlauf.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuZeiten {
    pub leerlauf: u64,
    pub kern: u64,
    pub benutzer: u64,
}

/// Auslastung in Prozent zwischen zwei Ständen; `None`, wenn keine Zeit
/// verging oder die Zähler rückwärts liefen.
pub fn cpu_auslastung(vorher: CpuZeiten, jetzt: CpuZeiten) -> Option<f32> {
    let leerlauf = jetzt.leerlauf.checked_sub(vorher.leerlauf)?;
    let gesamt = jetzt.kern.checked_sub(vorher.kern)? + jetzt.benutzer.checked_sub(vorher.benutzer)?;
    if gesamt == 0 || leerlauf > gesamt {
        return None;
    }
    Some(((gesamt - leerlauf) as f64 * 100.0 / gesamt as f64) as f32)
}

#[cfg(windows)]
fn cpu_zeiten() -> Option<CpuZeiten> {
    use windows_sys::Win32::Foundation::FILETIME;
    use windows_sys::Win32::System::Threading::GetSystemTimes;
    let leer = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
    let (mut l, mut k, mut b) = (leer, leer, leer);
    // SAFETY: drei gültige Zeiger auf FILETIME auf dem Stack.
    if unsafe { GetSystemTimes(&mut l, &mut k, &mut b) } == 0 {
        return None;
    }
    let zahl = |f: FILETIME| (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime);
    Some(CpuZeiten { leerlauf: zahl(l), kern: zahl(k), benutzer: zahl(b) })
}

#[cfg(not(windows))]
fn cpu_zeiten() -> Option<CpuZeiten> {
    None
}

/// So viele Messungen in Folge mit genau 0 % gelten als Messfehler: die
/// Belegung ist dann `unbekannt`, nicht `normal`.
pub const CPU_NULL_FOLGE: u32 = 3;

/// Zählt Messungen mit genau 0 % in Folge.
#[derive(Debug, Default)]
pub struct NullFolge(u32);

impl NullFolge {
    /// Meldet eine Messung; `true`, wenn die Folge den Messfehler anzeigt.
    pub fn melden(&mut self, prozent: f32) -> bool {
        self.0 = if prozent == 0.0 { self.0.saturating_add(1) } else { 0 };
        self.0 >= CPU_NULL_FOLGE
    }
}

/// Startet die Hintergrund-Sammlung. Hardware alle paar Sekunden,
/// Defender nur alle paar Minuten (PowerShell-Aufruf ist langsam).
pub fn spawn_collector(shared: SharedSnapshot) {
    tokio::spawn(async move {
        let mut sys = System::new();
        let mut defender_next = tokio::time::Instant::now();
        let mut defender = DefenderStatus::default();
        let mut history: std::collections::VecDeque<Sample> = std::collections::VecDeque::with_capacity(HISTORY_LEN);
        let mut first = true;
        let mut zeiten_vorher = cpu_zeiten();
        let mut nullen = NullFolge::default();
        loop {
            if tokio::time::Instant::now() >= defender_next {
                defender = read_defender().await;
                defender_next = tokio::time::Instant::now() + Duration::from_secs(10 * 60);
            }
            let def = defender.clone();
            let handle = tokio::task::spawn_blocking(move || {
                let snap = collect_hardware(&mut sys, def);
                (sys, snap)
            });
            match handle.await {
                Ok((s, mut snap)) => {
                    sys = s;
                    // Nur GetSystemTimes (Hotfix 3a-4); scheitert es, ist die Last unbekannt.
                    let zeiten = cpu_zeiten();
                    let prozent = match (zeiten_vorher, zeiten) {
                        (Some(a), Some(b)) => cpu_auslastung(a, b),
                        _ => None,
                    };
                    zeiten_vorher = zeiten;
                    if first {
                        // Die erste Messung: unbekannt, nicht in history (Lücke statt 0).
                        first = false;
                        snap.erste_messung();
                        *shared.write().await = snap;
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        continue;
                    }
                    snap.cpu_setzen(prozent, &mut nullen);
                    if history.len() >= HISTORY_LEN {
                        history.pop_front();
                    }
                    history.push_back(Sample {
                        at: snap.taken_at.unwrap_or_else(Local::now),
                        cpu: snap.cpu_gemessen().then_some(snap.cpu_percent),
                        mem: snap.mem_percent(),
                    });
                    snap.history = history.iter().copied().collect();
                    *shared.write().await = snap;
                    debug!("PC-Status aktualisiert");
                }
                Err(e) => {
                    warn!("Sammeln des PC-Status fehlgeschlagen: {e}");
                    sys = System::new();
                }
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

fn collect_hardware(sys: &mut System, defender: DefenderStatus) -> Snapshot {
    sys.refresh_memory();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    let disks = Disks::new_with_refreshed_list();
    let disks = disks
        .list()
        .iter()
        .filter(|d| d.total_space() > 0)
        .map(|d| DiskInfo {
            name: d.name().to_string_lossy().to_string(),
            mount: d.mount_point().to_string_lossy().to_string(),
            total: d.total_space(),
            used: d.total_space().saturating_sub(d.available_space()),
            belegung: Belegung::Unbekannt,
        })
        .collect();
    let process_names = sys.processes().values().map(|p| p.name().to_string_lossy().to_lowercase()).collect();
    let mut snap = Snapshot {
        taken_at: Some(Local::now()),
        host: System::host_name().unwrap_or_default(),
        os: System::long_os_version().unwrap_or_default(),
        // Die Last setzt der Sammler aus GetSystemTimes (`Snapshot::cpu_setzen`).
        cpu_percent: 0.0,
        cpu_count: std::thread::available_parallelism().map_or(0, |n| n.get()),
        mem_used: sys.used_memory(),
        mem_total: sys.total_memory(),
        disks,
        uptime_secs: System::uptime(),
        cpu_belegung: Belegung::Unbekannt,
        mem_belegung: Belegung::Unbekannt,
        belegung_schwellen: BelegungsSchwellen::default(),
        neustart: read_reboot_pending(),
        defender,
        process_names,
        history: Vec::new(),
    };
    snap.belegung_rechnen();
    snap
}

#[cfg(windows)]
fn read_reboot_pending() -> NeustartLage {
    use winreg::RegKey;
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE};
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let mut gruende = Vec::new();
    let keys = [
        (r"SOFTWARE\Microsoft\Windows\CurrentVersion\WindowsUpdate\Auto Update\RebootRequired", NeustartGrund::WindowsUpdate),
        (r"SOFTWARE\Microsoft\Windows\CurrentVersion\Component Based Servicing\RebootPending", NeustartGrund::Systemkomponenten),
    ];
    for (path, grund) in keys {
        if hklm.open_subkey_with_flags(path, KEY_QUERY_VALUE).is_ok() {
            gruende.push(grund);
        }
    }
    if let Ok(sm) = hklm.open_subkey_with_flags(r"SYSTEM\CurrentControlSet\Control\Session Manager", KEY_QUERY_VALUE) {
        let pending: Result<Vec<String>, _> = sm.get_value("PendingFileRenameOperations");
        if matches!(pending, Ok(ref v) if !v.is_empty()) {
            gruende.push(NeustartGrund::DateienWarten);
        }
    }
    NeustartLage::aus_gruenden(gruende)
}

#[cfg(not(windows))]
fn read_reboot_pending() -> NeustartLage {
    NeustartLage::default()
}

const DEFENDER_SCRIPT: &str = r#"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$ErrorActionPreference = 'Stop'
$s = Get-MpComputerStatus
$o = [ordered]@{
  av = $s.AntivirusEnabled
  rt = $s.RealTimeProtectionEnabled
  sig = [string]$s.AntivirusSignatureVersion
  sig_date = $(if ($s.AntivirusSignatureLastUpdated) { $s.AntivirusSignatureLastUpdated.ToString('yyyy-MM-dd HH:mm') } else { '' })
  sig_age = $s.AntivirusSignatureAge
  quick = $(if ($s.QuickScanEndTime) { $s.QuickScanEndTime.ToString('yyyy-MM-dd HH:mm') } else { '' })
}
$o | ConvertTo-Json -Compress
"#;

/// Fragt den Defender über PowerShell ab und bewertet ihn
/// ([`DefenderStatus::bewerten`]). Geht ohne Adminrechte.
async fn read_defender() -> DefenderStatus {
    read_defender_werte().await.bewerten()
}

/// Nur die Werte, ohne Befund. Fehlt ein Wert, kommt er als null (kein
/// `[bool]` oder `[int]` im Skript: `$null` würde sonst zu false oder 0, also
/// zu „aus“ oder „frisch“).
async fn read_defender_werte() -> DefenderStatus {
    let mut status = DefenderStatus { checked_at: Some(Local::now()), ..Default::default() };
    if !cfg!(windows) {
        status.error = Some("nur unter Windows".to_string());
        return status;
    }
    // Voller Pfad aus System32, nie aus dem PATH (dienst::programme).
    let w = crate::dienst::programme::Werkzeug::WindowsPowerShell;
    // Ohne Fenster, damit keine Konsole aufblitzt.
    let mut ps = match crate::dienst::programme::befehl_async(w) {
        Ok(c) => c,
        Err(satz) => {
            status.error = Some(satz);
            return status;
        }
    };
    // kill_on_drop: beim Zeitlimit fällt das Future weg und PowerShell wird beendet.
    let fut = ps.args(["-NoProfile", "-NonInteractive", "-Command", DEFENDER_SCRIPT]).kill_on_drop(true).output();
    let out = match tokio::time::timeout(Duration::from_secs(40), fut).await {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => {
            status.error = Some(format!("PowerShell startet nicht: {e}"));
            return status;
        }
        Err(_) => {
            status.error = Some("Abfrage dauerte zu lange".to_string());
            return status;
        }
    };
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        status.error = Some(err.lines().next().unwrap_or("Fehler").to_string());
        return status;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    match serde_json::from_str::<serde_json::Value>(text.trim()) {
        Ok(v) => {
            status.antivirus_enabled = v["av"].as_bool();
            status.realtime_enabled = v["rt"].as_bool();
            status.signature_version = non_empty(v["sig"].as_str());
            status.signature_updated = non_empty(v["sig_date"].as_str());
            status.signature_age_days = signatur_alter(&v["sig_age"]);
            status.last_quick_scan = non_empty(v["quick"].as_str());
            info!("Defender-Stand gelesen");
        }
        Err(e) => status.error = Some(format!("Antwort unlesbar: {e}")),
    }
    status
}

/// Sonderwerte von `AntivirusSignatureAge` (UInt32), die kein Alter sind:
/// „nie aktualisiert“ oder ein nicht gesetzter Zähler.
pub const SIGNATUR_ALTER_SONDERWERTE: [u64; 2] = [u32::MAX as u64, u16::MAX as u64];

/// Das Alter der Signaturen in Tagen aus der Antwort des Skripts. Das
/// Skript gibt den UInt32 unverändert weiter (kein `[int]`, das bei
/// 4294967295 überliefe und das ganze Skript scheitern ließe); `null`,
/// keine Zahl und die Sonderwerte ([`SIGNATUR_ALTER_SONDERWERTE`]) sind
/// unbekannt, also `signatur-alter-unbekannt`.
pub fn signatur_alter(wert: &serde_json::Value) -> Option<u32> {
    let tage = wert.as_u64()?;
    if SIGNATUR_ALTER_SONDERWERTE.contains(&tage) {
        return None;
    }
    u32::try_from(tage).ok()
}

fn non_empty(s: Option<&str>) -> Option<String> {
    s.filter(|s| !s.is_empty()).map(str::to_string)
}

/// Einfacher HTTP-Check: antwortet die Adresse? Nur `http://`, ohne TLS.
pub async fn http_check(url: &str) -> Result<u16, String> {
    let rest = url.strip_prefix("http://").ok_or_else(|| "nur http:// wird geprüft".to_string())?;
    let (hostport, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let addr = if hostport.contains(':') { hostport.to_string() } else { format!("{hostport}:80") };
    let connect = tokio::net::TcpStream::connect(&addr);
    let mut stream = tokio::time::timeout(Duration::from_secs(2), connect)
        .await
        .map_err(|_| "keine Antwort (Zeitlimit)".to_string())?
        .map_err(|e| format!("nicht erreichbar: {e}"))?;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let req = format!("GET {path} HTTP/1.0\r\nHost: {hostport}\r\nConnection: close\r\n\r\n");
    stream.write_all(req.as_bytes()).await.map_err(|e| e.to_string())?;
    let mut buf = [0u8; 64];
    let n = tokio::time::timeout(Duration::from_secs(3), stream.read(&mut buf))
        .await
        .map_err(|_| "keine Antwort (Zeitlimit)".to_string())?
        .map_err(|e| e.to_string())?;
    let head = String::from_utf8_lossy(&buf[..n]);
    head.split_whitespace().nth(1).and_then(|c| c.parse::<u16>().ok()).ok_or_else(|| "Antwort ist kein HTTP".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_handles_zero() {
        assert_eq!(percent(1, 0), 0.0);
        assert!((percent(1, 4) - 25.0).abs() < 0.01);
    }

    fn gelesen(av: Option<bool>, rt: Option<bool>, alter: Option<u32>) -> DefenderStatus {
        DefenderStatus { checked_at: Some(Local::now()), antivirus_enabled: av, realtime_enabled: rt, signature_age_days: alter, ..Default::default() }
            .bewerten()
    }

    /// Etappe 3a+: das Alter kommt als UInt32 ohne `[int]` aus dem Skript;
    /// Sonderwerte, null und Unlesbares sind unbekannt (signatur-alter-unbekannt).
    #[test]
    fn defender_signatur_alter_mit_sonderwerten() {
        use serde_json::json;
        assert!(!DEFENDER_SCRIPT.contains("[int]"), "kein [int]: 4294967295 liefe über");
        assert!(DEFENDER_SCRIPT.contains("sig_age = $s.AntivirusSignatureAge"));
        assert_eq!(signatur_alter(&json!(0)), Some(0));
        assert_eq!(signatur_alter(&json!(3)), Some(3));
        assert_eq!(signatur_alter(&json!(65534)), Some(65534));
        for unbekannt in [json!(4294967295u64), json!(65535), json!(null), json!(-1), json!(2.5), json!("3"), json!(5_000_000_000u64)] {
            assert_eq!(signatur_alter(&unbekannt), None, "{unbekannt}");
        }
        // Aus der ganzen Antwort des Skripts: Sonderwert gibt unbekannt mit eigenem Grund.
        let v: serde_json::Value = serde_json::from_str(r#"{"av":true,"rt":true,"sig":"1.2","sig_date":"","sig_age":4294967295,"quick":""}"#).unwrap();
        let d = gelesen(v["av"].as_bool(), v["rt"].as_bool(), signatur_alter(&v["sig_age"]));
        assert_eq!((d.befund, d.gruende), (DefenderBefund::Unbekannt, vec![DefenderGrund::SignaturAlterUnbekannt]));
        // Ohne error, aber Virenschutz fehlt: abfrage-fehlgeschlagen (die Doku sagt es jetzt).
        let d = gelesen(None, Some(true), Some(1));
        assert_eq!((d.error.clone(), d.gruende), (None, vec![DefenderGrund::AbfrageFehlgeschlagen]));
    }

    /// Grenzfall der Schwelle: 2 und genau 3 Tage sind in Ordnung, 4 Tage alt.
    #[test]
    fn defender_signatur_an_der_schwelle() {
        assert_eq!(SIGNATUR_SCHWELLE_TAGE, 3);
        for (tage, befund, gruende) in
            [(2, DefenderBefund::InOrdnung, vec![]), (3, DefenderBefund::InOrdnung, vec![]), (4, DefenderBefund::Ansehen, vec![DefenderGrund::SignaturAlt])]
        {
            let d = gelesen(Some(true), Some(true), Some(tage));
            assert_eq!((d.befund, d.gruende), (befund, gruende), "{tage} Tage");
            assert_eq!(d.signatur_schwelle_tage, 3);
        }
    }

    #[test]
    fn defender_gruende_und_unbekannt() {
        let d = gelesen(Some(false), Some(false), Some(9));
        assert_eq!(d.befund, DefenderBefund::Ansehen);
        assert_eq!(d.gruende, [DefenderGrund::VirenschutzAus, DefenderGrund::EchtzeitAus, DefenderGrund::SignaturAlt]);
        // Alter unbekannt, Virenschutz und Echtzeit an: unbekannt, nie in Ordnung.
        let d = gelesen(Some(true), Some(true), None);
        assert_eq!((d.befund, d.gruende), (DefenderBefund::Unbekannt, vec![DefenderGrund::SignaturAlterUnbekannt]));
        // Eines aus: ansehen, auch wenn ein anderer Wert fehlt.
        let d = gelesen(Some(false), Some(true), None);
        assert_eq!((d.befund, d.gruende), (DefenderBefund::Ansehen, vec![DefenderGrund::VirenschutzAus]));
        let d = gelesen(None, Some(false), Some(1));
        assert_eq!((d.befund, d.gruende), (DefenderBefund::Ansehen, vec![DefenderGrund::EchtzeitAus]));
        let d = gelesen(Some(true), None, Some(5));
        assert_eq!((d.befund, d.gruende), (DefenderBefund::Ansehen, vec![DefenderGrund::SignaturAlt]));
        // Alles fehlt: unbekannt.
        let d = gelesen(None, None, None);
        assert_eq!((d.befund, d.gruende), (DefenderBefund::Unbekannt, vec![DefenderGrund::AbfrageFehlgeschlagen]));
        let d = gelesen(Some(true), None, Some(1));
        assert_eq!((d.befund, d.gruende), (DefenderBefund::Unbekannt, vec![DefenderGrund::AbfrageFehlgeschlagen]));
        assert_eq!(serde_json::to_value(DefenderGrund::SignaturAlterUnbekannt).unwrap(), "signatur-alter-unbekannt");
        // Noch nie gelesen: unbekannt, nie in Ordnung.
        let d = DefenderStatus::default();
        assert_eq!((d.befund, d.gruende.as_slice()), (DefenderBefund::Unbekannt, &[DefenderGrund::NochNichtGelesen][..]));
        assert_eq!(DefenderStatus::default().bewerten().gruende, [DefenderGrund::NochNichtGelesen]);
        // Fehler oder fehlende Werte: unbekannt mit Grund.
        let d = gelesen(None, Some(true), Some(1));
        assert_eq!((d.befund, d.gruende), (DefenderBefund::Unbekannt, vec![DefenderGrund::AbfrageFehlgeschlagen]));
        let mut f = gelesen(Some(true), Some(true), Some(1));
        f.error = Some("Abfrage dauerte zu lange".into());
        assert_eq!(f.bewerten().befund, DefenderBefund::Unbekannt);
        let json = serde_json::to_value(gelesen(Some(true), Some(false), Some(4))).unwrap();
        assert_eq!(json["befund"], "ansehen");
        assert_eq!(json["gruende"], serde_json::json!(["echtzeit-aus", "signatur-alt"]));
        assert_eq!(json["signatur_schwelle_tage"], 3);
    }

    #[test]
    fn neustart_aus_gruenden() {
        let l = NeustartLage::aus_gruenden(vec![]);
        assert_eq!((l.neustart, l.gruende.len()), (Neustart::NichtNoetig, 0));
        let l = NeustartLage::aus_gruenden(vec![NeustartGrund::WindowsUpdate, NeustartGrund::DateienWarten]);
        assert_eq!(l.neustart, Neustart::Aussteht);
        let json = serde_json::to_value(&l).unwrap();
        assert_eq!(json, serde_json::json!({"neustart": "aussteht", "gruende": ["windows-update", "dateien-warten"]}));
        assert_eq!(serde_json::to_value(NeustartLage::default()).unwrap()["neustart"], "unbekannt");
        assert_eq!(serde_json::to_value(Neustart::NichtNoetig).unwrap(), "nicht-noetig");
    }

    #[test]
    fn cpu_auslastung_aus_zeiten() {
        let a = CpuZeiten { leerlauf: 1_000, kern: 2_000, benutzer: 500 };
        // 1000 vergangen (kern 600 davon 350 Leerlauf, benutzer 400): 650 belegt.
        let b = CpuZeiten { leerlauf: 1_350, kern: 2_600, benutzer: 900 };
        assert_eq!(cpu_auslastung(a, b), Some(65.0));
        assert_eq!(cpu_auslastung(a, a), None);
        assert_eq!(cpu_auslastung(b, a), None);
        let leer = CpuZeiten { leerlauf: 2_000, kern: 3_000, benutzer: 500 };
        assert_eq!(cpu_auslastung(a, leer), Some(0.0));
        #[cfg(windows)]
        {
            let x = cpu_zeiten().expect("GetSystemTimes");
            assert!(x.kern >= x.leerlauf);
        }
    }

    /// Hotfix 3a-4: genau 0 % über mehrere Messungen ist ein Messfehler.
    #[test]
    fn cpu_genau_null_in_folge_ist_unbekannt() {
        let mut n = NullFolge::default();
        assert!(!n.melden(0.0));
        assert!(!n.melden(0.0));
        assert!(n.melden(0.0));
        assert!(n.melden(0.0));
        assert!(!n.melden(0.4));
        assert!(!n.melden(0.0));
        assert_eq!(CPU_NULL_FOLGE, 3);
    }

    /// Erste Messung: 0 % wäre normal, also grün; die Belegung ist `unbekannt`.
    #[test]
    fn erste_cpu_messung_ist_unbekannt() {
        let mut s = Snapshot { taken_at: Some(Local::now()), cpu_percent: 100.0, mem_used: 1, mem_total: 4, ..Default::default() };
        s.erste_messung();
        assert_eq!((s.cpu_percent, s.cpu_belegung, s.mem_belegung), (0.0, Belegung::Unbekannt, Belegung::Normal));
        s.cpu_percent = 10.0;
        s.belegung_rechnen();
        assert_eq!(s.cpu_belegung, Belegung::Normal);
    }

    /// Etappe 3a+: ohne Messung (GetSystemTimes scheitert) und bei einer
    /// Folge von genau 0 % ist die Last unbekannt, nie ein stilles 0 aus
    /// sysinfo; `cpu_gemessen` sagt dem Verlauf, dass hier eine Lücke ist.
    #[test]
    fn cpu_setzen_ohne_messung_und_bei_nullfolge_unbekannt() {
        let mut s = Snapshot { taken_at: Some(Local::now()), mem_used: 1, mem_total: 4, ..Default::default() };
        let mut n = NullFolge::default();
        s.cpu_setzen(Some(12.0), &mut n);
        assert_eq!((s.cpu_percent, s.cpu_belegung, s.cpu_gemessen()), (12.0, Belegung::Normal, true));
        s.cpu_setzen(None, &mut n);
        assert_eq!((s.cpu_percent, s.cpu_belegung, s.cpu_gemessen()), (0.0, Belegung::Unbekannt, false));
        assert_eq!(s.mem_belegung, Belegung::Normal, "der Arbeitsspeicher bleibt gemessen");
        s.cpu_setzen(Some(0.0), &mut n);
        s.cpu_setzen(Some(0.0), &mut n);
        assert!(s.cpu_gemessen(), "zweimal 0 ist noch eine Messung");
        s.cpu_setzen(Some(0.0), &mut n);
        assert_eq!((s.cpu_belegung, s.cpu_gemessen()), (Belegung::Unbekannt, false));
        s.cpu_setzen(Some(5.0), &mut n);
        assert_eq!((s.cpu_percent, s.cpu_gemessen()), (5.0, true));
        s.erste_messung();
        assert!(!s.cpu_gemessen());
        // Ohne sysinfo-Prozessor zählt die Zahl der Kerne trotzdem.
        assert!(std::thread::available_parallelism().map_or(0, |n| n.get()) > 0);
    }

    /// Grenzfälle der Belegung: genau 70 % ist hoch, genau 90 % kritisch.
    #[test]
    fn belegung_an_den_schwellen() {
        use Belegung::*;
        assert_eq!((BELEGUNG_HOCH_AB, BELEGUNG_KRITISCH_AB), (70, 90));
        assert_eq!(Belegung::aus_anteil(69, 100), Normal);
        assert_eq!(Belegung::aus_anteil(70, 100), Hoch);
        assert_eq!(Belegung::aus_anteil(89, 100), Hoch);
        assert_eq!(Belegung::aus_anteil(90, 100), Kritisch);
        assert_eq!(Belegung::aus_anteil(100, 100), Kritisch);
        // Große Platten, genau auf der Schwelle und ein Byte darunter.
        let tb = 2_000_398_934_016u64;
        assert_eq!(Belegung::aus_anteil(tb / 10 * 7, tb / 10 * 10), Hoch);
        assert_eq!(Belegung::aus_anteil(tb / 10 * 7 - 1, tb / 10 * 10), Normal);
        assert_eq!(Belegung::aus_anteil(tb / 10 * 9, tb / 10 * 10), Kritisch);
        assert_eq!(Belegung::aus_anteil(tb / 10 * 9 - 1, tb / 10 * 10), Hoch);
        assert_eq!(Belegung::aus_anteil(5, 0), Unbekannt);
        assert_eq!(Belegung::aus_prozent(69.9), Normal);
        assert_eq!(Belegung::aus_prozent(70.0), Hoch);
        assert_eq!(Belegung::aus_prozent(90.0), Kritisch);
        assert_eq!(Belegung::aus_prozent(f32::NAN), Unbekannt);
    }

    #[test]
    fn belegung_im_snapshot() {
        let mut s = Snapshot::default();
        s.belegung_rechnen();
        assert_eq!((s.cpu_belegung, s.mem_belegung), (Belegung::Unbekannt, Belegung::Unbekannt), "vor der ersten Messung");
        s.taken_at = Some(Local::now());
        s.cpu_percent = 12.0;
        s.mem_used = 90;
        s.mem_total = 100;
        s.disks = vec![DiskInfo { name: "C".into(), mount: "C:\\".into(), total: 100, used: 70, belegung: Belegung::Unbekannt }];
        s.belegung_rechnen();
        assert_eq!((s.cpu_belegung, s.mem_belegung, s.disks[0].belegung), (Belegung::Normal, Belegung::Kritisch, Belegung::Hoch));
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["belegung_schwellen"], serde_json::json!({"hoch_ab": 70, "kritisch_ab": 90}));
        assert_eq!(json["disks"][0]["belegung"], "hoch");
        assert_eq!(json["neustart"]["neustart"], "unbekannt");
    }

    #[tokio::test]
    async fn http_check_rejects_https_and_closed_ports() {
        assert!(http_check("https://example.org").await.is_err());
        // Port 9 ist auf Windows normalerweise zu.
        assert!(http_check("http://127.0.0.1:9/").await.is_err());
    }
}
