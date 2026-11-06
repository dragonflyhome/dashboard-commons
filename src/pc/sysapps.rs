//! Systemanwendungen und Windows-Eckdaten, alles nur lesend:
//! Autostart (Registry + Startordner), laufende Dienste, installierte
//! Werkzeuge, winget-Liste, Windows-Version, Updates und letzte Starts.
//! PowerShell-Aufrufe sind langsam, darum Hintergrund und stündlich.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, FixedOffset, Local, TimeZone, Timelike};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{info, warn};
use ts_rs::TS;

use crate::dienst::programme::{self, Werkzeug};
use crate::pc::ereignisse::{self, Installiert, UpdateRoh};
use crate::vertrag::verlauf::Ereignis;

pub type SharedSysApps = Arc<RwLock<SysApps>>;

#[derive(Debug, Clone, Serialize, Default, TS)]
pub struct SysApps {
    #[ts(type = "string | null")]
    pub refreshed_at: Option<DateTime<Local>>,
    pub windows: WindowsInfo,
    pub autostart: Vec<AutostartEntry>,
    pub services_running: Vec<Service>,
    pub services_total: usize,
    pub tools: Vec<Tool>,
    pub winget: Vec<WingetEntry>,
    pub winget_error: Option<String>,
    pub errors: Vec<String>,
    /// Nur mit Boten: Fehler der eigenen Messung (Dienste, Updates, Starts);
    /// `errors` ist diese Liste plus der Satz zu den Benutzer-Daten.
    #[serde(skip)]
    pub lokal_fehler: Vec<String>,
    /// Nur mit Boten: wann die Benutzer-Daten (winget, Autostart, Werkzeuge)
    /// zuletzt vom Boten kamen.
    #[serde(skip)]
    pub benutzer_stand: Option<DateTime<Local>>,
    /// Nur mit Boten: warum die Benutzer-Daten gerade nicht kamen.
    #[serde(skip)]
    pub benutzer_fehler: Option<String>,
    /// Etappe 2e: die Windows-Ereignisse dieses Laufs für den Verlauf (alle
    /// Hotfixes, Starts und Update-Ereignisse der letzten 90 Tage, die
    /// Installation). Der Sammler legt sie im Datenspeicher ab
    /// (`speicher::pc_ereignisse`). Seit Etappe 3a-1 stehen Hotfixes, Starts
    /// und Updates nur noch dort und im Verlauf (`/api/pc/verlauf`), nicht mehr in `/api/pc`.
    #[serde(skip)]
    pub ereignisse: Vec<Ereignis>,
}

/// So viele Starts und Update-Ereignisse liest der Sammler höchstens je Lauf
/// (`-MaxEvents` in `PS_REPORT`). Kommen so viele, fehlen vielleicht ältere;
/// dann steht ein Satz in `errors` und damit als Hinweis im Verlauf.
pub const MAX_EREIGNISSE: usize = 5000;

impl SysApps {
    /// `errors` neu aus eigener Messung und Boten-Satz zusammensetzen.
    pub fn fehler_zusammenfassen(&mut self) {
        self.errors = self.lokal_fehler.clone();
        if let Some(f) = &self.benutzer_fehler {
            self.errors.push(f.clone());
        }
    }
}

#[derive(Debug, Clone, Serialize, Default, TS)]
pub struct WindowsInfo {
    pub product: String,
    pub version: String,
    pub build: String,
    #[ts(type = "string | null")]
    pub installed_at: Option<DateTime<Local>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct AutostartEntry {
    pub name: String,
    pub command: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Service {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "DisplayName")]
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Tool {
    pub name: String,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
pub struct WingetEntry {
    pub name: String,
    pub id: String,
    pub version: String,
    pub available: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct Hotfix {
    #[serde(rename = "HotFixID")]
    pub id: String,
    #[serde(rename = "Description", default)]
    pub description: String,
    #[serde(rename = "InstalledOn", default)]
    pub installed_on: String,
}

#[derive(Debug, Deserialize)]
struct PsReport {
    #[serde(default)]
    services: Vec<Service>,
    #[serde(default)]
    services_total: usize,
    #[serde(default)]
    hotfixes: Vec<Hotfix>,
    #[serde(default)]
    boots: Vec<String>,
    #[serde(default)]
    updates: Vec<PsUpdate>,
    #[serde(default)]
    errors: Vec<String>,
}

/// Ein Update-Ereignis, wie PowerShell es meldet: `Zeit` mit Sekunden und
/// Zone für den Verlauf.
#[derive(Debug, Deserialize)]
struct PsUpdate {
    #[serde(rename = "Zeit", default)]
    zeit: String,
    #[serde(rename = "Title")]
    title: String,
}

const PS_REPORT: &str = r#"
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
$ErrorActionPreference = 'Continue'
$errs = @()
$svc = @(); $total = 0
try { $all = Get-Service; $total = $all.Count
      $svc = @($all | Where-Object Status -eq 'Running' | Sort-Object DisplayName | Select-Object Name, DisplayName) }
catch { $errs += "Dienste: $($_.Exception.Message)" }
$hf = @()
try { $hf = @(Get-HotFix | Sort-Object InstalledOn -Descending | Select-Object HotFixID, Description,
      @{n='InstalledOn'; e={ if ($_.InstalledOn) { $_.InstalledOn.ToString('yyyy-MM-dd') } else { '' } }}) }
catch { $errs += "Updates: $($_.Exception.Message)" }
$seit = (Get-Date).Date.AddDays(-90)
$boots = @()
try { $boots = @(Get-WinEvent -FilterHashtable @{LogName='System'; ProviderName='Microsoft-Windows-Kernel-General'; Id=12; StartTime=$seit} -MaxEvents 5000 -ErrorAction Stop |
      ForEach-Object { $_.TimeCreated.ToString('o') }) }
catch { if ($_.FullyQualifiedErrorId -notlike 'NoMatchingEventsFound*') { $errs += "Startzeiten: $($_.Exception.Message)" } }
$upd = @()
try { $upd = @(Get-WinEvent -FilterHashtable @{LogName='System'; ProviderName='Microsoft-Windows-WindowsUpdateClient'; Id=19; StartTime=$seit} -MaxEvents 5000 -ErrorAction Stop |
      ForEach-Object { [pscustomobject]@{ Zeit = $_.TimeCreated.ToString('o'); Title = [string]$_.Properties[0].Value } }) }
catch { if ($_.FullyQualifiedErrorId -notlike 'NoMatchingEventsFound*') { $errs += "Update-Ereignisse: $($_.Exception.Message)" } }
[pscustomobject]@{ services = $svc; services_total = $total; hotfixes = $hf; boots = $boots; updates = $upd; errors = $errs } |
  ConvertTo-Json -Compress -Depth 4
"#;

/// Startet ein Programm mit Zeitlimit (höchstens 5 Minuten). Läuft es zu lange,
/// fällt das Future beim Ende des `timeout` weg, und `kill_on_drop` beendet den
/// Kindprozess, statt ihn im Hintergrund weiterlaufen zu lassen. Das Programm
/// kommt nur von einem festen Ort (`dienst::programme`), nie aus dem PATH.
async fn run(w: Werkzeug, args: &[&str], secs: u64) -> Result<String, String> {
    let cmd = w.name();
    // Fester Pfad und ohne Fenster (keine aufblitzende Konsole).
    let mut befehl = programme::befehl_async(w)?;
    let fut = befehl.args(args).kill_on_drop(true).output();
    let out =
        tokio::time::timeout(Duration::from_secs(secs.min(300)), fut).await.map_err(|_| format!("{cmd}: Zeitlimit"))?.map_err(|e| format!("{cmd}: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(format!("{cmd}: {}", err.lines().next().unwrap_or("Fehler")))
    }
}

/// Version eines Werkzeugs; `None` („nicht gefunden“), wenn es an keinem
/// festen Ort liegt oder nicht antwortet.
async fn tool_version(name: &'static str, w: Werkzeug, args: &[&str]) -> Tool {
    let version = run(w, args, 20).await.ok().and_then(|s| s.lines().map(str::trim).find(|l| !l.is_empty()).map(|l| l.to_string()));
    Tool { name: name.to_string(), version }
}

/// Zerlegt die Tabelle von `winget list` anhand der Kopfzeile in Spalten.
pub fn parse_winget(text: &str) -> Vec<WingetEntry> {
    let lines: Vec<&str> = text.lines().map(|l| l.trim_end_matches('\r')).collect();
    // Spaltenanfänge als Zeichen-Index (nicht Byte), weil „Verfügbar" ein ü enthält.
    let char_pos = |hay: &str, needle: &str| hay.find(needle).map(|b| hay[..b].chars().count() + 1);
    let Some(h) = lines.iter().position(|l| l.starts_with("Name") && (l.contains(" Id") || l.contains(" ID")) && l.contains("Version")) else {
        return Vec::new();
    };
    let header = lines[h];
    let id_pos = char_pos(header, " Id").or_else(|| char_pos(header, " ID")).unwrap_or(0);
    let ver_pos = char_pos(header, " Version").unwrap_or(0);
    let avail_pos = char_pos(header, " Verfügbar").or_else(|| char_pos(header, " Available"));
    let src_pos = char_pos(header, " Quelle").or_else(|| char_pos(header, " Source"));
    let cut = |l: &str, a: usize, b: Option<usize>| -> String {
        let chars: Vec<char> = l.chars().collect();
        let end = b.unwrap_or(chars.len()).min(chars.len());
        if a >= end {
            return String::new();
        }
        chars[a..end].iter().collect::<String>().trim().to_string()
    };
    let mut out = Vec::new();
    for l in lines.iter().skip(h + 1) {
        if l.trim().is_empty() || l.starts_with('-') || l.chars().count() <= ver_pos {
            continue;
        }
        let name = cut(l, 0, Some(id_pos));
        let id = cut(l, id_pos, Some(ver_pos));
        let version = cut(l, ver_pos, avail_pos.or(src_pos));
        let available = match (avail_pos, src_pos) {
            (Some(a), s) => cut(l, a, s),
            _ => String::new(),
        };
        if name.is_empty() || name.contains("Upgrades verfügbar") || name.contains("upgrades available") {
            continue;
        }
        out.push(WingetEntry { name, id, version, available });
    }
    out
}

#[cfg(windows)]
pub fn autostart() -> Vec<AutostartEntry> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE};
    let mut out = Vec::new();
    let places: [(winreg::HKEY, &str, &'static str); 3] = [
        (HKEY_CURRENT_USER, r"Software\Microsoft\Windows\CurrentVersion\Run", "Benutzer"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run", "Alle Benutzer"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run", "Alle Benutzer (32-bit)"),
    ];
    for (hive, path, source) in places {
        if let Ok(key) = RegKey::predef(hive).open_subkey_with_flags(path, KEY_QUERY_VALUE) {
            for (name, value) in key.enum_values().flatten() {
                out.push(AutostartEntry { name, command: value.to_string(), source: source.to_string() });
            }
        }
    }
    let startup = crate::pc::inventory::home_dir().join(r"AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup");
    if let Ok(rd) = std::fs::read_dir(startup) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.eq_ignore_ascii_case("desktop.ini") {
                continue;
            }
            out.push(AutostartEntry { name, command: e.path().display().to_string(), source: "Startordner".to_string() });
        }
    }
    out
}

#[cfg(not(windows))]
pub fn autostart() -> Vec<AutostartEntry> {
    Vec::new()
}

#[cfg(windows)]
fn read_windows_info() -> WindowsInfo {
    use winreg::RegKey;
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE};
    let mut info = WindowsInfo::default();
    if let Ok(key) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion", KEY_QUERY_VALUE) {
        info.product = key.get_value::<String, _>("ProductName").unwrap_or_default();
        info.version = key.get_value::<String, _>("DisplayVersion").unwrap_or_default();
        info.build = key.get_value::<String, _>("CurrentBuild").unwrap_or_default();
        if let Ok(secs) = key.get_value::<u32, _>("InstallDate") {
            info.installed_at = Local.timestamp_opt(secs as i64, 0).single();
        }
    }
    info
}

#[cfg(not(windows))]
fn read_windows_info() -> WindowsInfo {
    WindowsInfo::default()
}

/// Versionen der Werkzeuge an festen Orten (je höchstens 20 s); was dort nicht
/// liegt, ist „nicht gefunden“, auch wenn es im PATH stünde.
pub async fn werkzeuge() -> Vec<Tool> {
    vec![
        tool_version("Git", Werkzeug::Git, &["--version"]).await,
        tool_version("PowerShell 7", Werkzeug::PowerShell7, &["-NoProfile", "-Command", "$PSVersionTable.PSVersion.ToString()"]).await,
        tool_version("Windows PowerShell", Werkzeug::WindowsPowerShell, &["-NoProfile", "-Command", "$PSVersionTable.PSVersion.ToString()"]).await,
        tool_version("Rust (cargo)", Werkzeug::Cargo, &["--version"]).await,
        tool_version("Rust (rustc)", Werkzeug::Rustc, &["--version"]).await,
        tool_version("Node.js", Werkzeug::Node, &["--version"]).await,
        tool_version("winget", Werkzeug::Winget, &["--version"]).await,
    ]
}

/// `winget list` (höchstens 120 s). Eine leere Liste gilt als Fehler.
pub async fn winget_liste() -> Result<Vec<WingetEntry>, String> {
    let text = run(Werkzeug::Winget, &["list", "--disable-interactivity"], 120).await?;
    let liste = parse_winget(&text);
    if liste.is_empty() {
        return Err("winget-Liste konnte nicht gelesen werden".to_string());
    }
    Ok(liste)
}

/// Alles, was kein Benutzerkonto braucht: Windows-Version, Dienste, Updates,
/// Starts. Fehler stehen in `errors` und `lokal_fehler`.
pub async fn collect_lokal() -> SysApps {
    let mut s = SysApps { refreshed_at: Some(Local::now()), ..Default::default() };
    s.windows = read_windows_info();
    match run(Werkzeug::WindowsPowerShell, &["-NoProfile", "-NonInteractive", "-Command", PS_REPORT], 90).await {
        Ok(json) => match serde_json::from_str::<PsReport>(json.trim()) {
            Ok(r) => bericht_einsetzen(&mut s, r),
            Err(e) => s.errors.push(format!("PowerShell-Antwort unlesbar: {e}")),
        },
        Err(e) => s.errors.push(e),
    }
    s.lokal_fehler = s.errors.clone();
    s
}

/// Zeit aus PowerShell (`ToString('o')`, mit Bruchteilen und Zone), auf die Sekunde.
fn zeit_lesen(text: &str) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(text.trim()).ok().and_then(|t| t.with_nanosecond(0))
}

/// Den Bericht von PowerShell in `s` einsetzen: die Windows-Ereignisse für
/// den Verlauf (Etappe 3a-1: nur noch dort). Hat die Ereignisanzeige so viele
/// Starts oder Update-Ereignisse geliefert, wie der Sammler höchstens liest
/// (`MAX_EREIGNISSE`), fehlen vielleicht ältere: dazu ein Satz in `errors`.
fn bericht_einsetzen(s: &mut SysApps, r: PsReport) {
    let starts: Vec<DateTime<FixedOffset>> = r.boots.iter().filter_map(|b| zeit_lesen(b)).collect();
    let roh: Vec<UpdateRoh> = r.updates.iter().filter_map(|u| Some(UpdateRoh { zeit: zeit_lesen(&u.zeit)?, titel: u.title.clone() })).collect();
    let produkt = format!("{} {}", s.windows.product.trim(), s.windows.version.trim()).trim().to_string();
    let inst = s.windows.installed_at.map(|zeit| Installiert { zeit, produkt });
    s.ereignisse = ereignisse::alle(&r.hotfixes, &roh, &starts, inst.as_ref());
    s.services_running = r.services;
    s.services_total = r.services_total;
    s.errors = r.errors;
    for (anzahl, was) in [(r.boots.len(), "Startzeiten"), (r.updates.len(), "Update-Ereignisse")] {
        if anzahl >= MAX_EREIGNISSE {
            s.errors.push(format!(
                "{was}: Die Ereignisanzeige hat {MAX_EREIGNISSE} oder mehr Einträge der letzten 90 Tage; der Sammler liest höchstens {MAX_EREIGNISSE}, ältere können fehlen."
            ));
        }
    }
}

/// Alles zusammen in diesem Prozess (Altweg ohne Boten).
pub async fn collect() -> SysApps {
    let mut s = collect_lokal().await;
    s.autostart = autostart();
    s.tools = werkzeuge().await;
    match winget_liste().await {
        Ok(liste) => s.winget = liste,
        Err(e) => s.winget_error = Some(e),
    }
    info!("Systemanwendungen gelesen: {} Autostart, {} Dienste laufen, {} winget-Pakete", s.autostart.len(), s.services_running.len(), s.winget.len());
    s
}

/// Sammler ohne Boten: stündlich alles; die Windows-Ereignisse gehen in den
/// Datenspeicher (`speicher::pc_ereignisse`), wenn er offen ist.
pub fn spawn(shared: SharedSysApps, speicher: Option<Arc<crate::speicher::Speicher>>, melder: Arc<crate::aenderungen::Melder>) {
    tokio::spawn(async move {
        loop {
            let mut s = collect().await;
            if !s.errors.is_empty() {
                warn!("Systemanwendungen, Teilfehler: {:?}", s.errors);
            }
            crate::speicher::pc_ereignisse::sammler_lauf(speicher.as_ref(), std::mem::take(&mut s.ereignisse)).await;
            *shared.write().await = s;
            // Live-Lücken: neuer Lauf als `pc-benutzer` mit Bereich `systemprogramme`.
            melder.melden(crate::aenderungen::Bereich::Systemprogramme);
            tokio::time::sleep(Duration::from_secs(60 * 60)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn winget_table_parses() {
        let text = "\
   -\r
Name                       Id                          Version      Verfügbar   Quelle\r
-------------------------------------------------------------------------------------\r
Git                        Git.Git                     2.56.0                   winget\r
PowerShell 7-x64           Microsoft.PowerShell        7.6.0.0      7.6.1.0     winget\r
Rustup: the Rust toolchain Rustlang.Rustup             1.28.2                   winget\r
Irgendwas ohne Quelle      ABC                         1.0\r
2 Upgrades verfügbar.\r
";
        let list = parse_winget(text);
        assert_eq!(list.len(), 4);
        assert_eq!(list[0].id, "Git.Git");
        assert_eq!(list[1].version, "7.6.0.0");
        assert_eq!(list[1].available, "7.6.1.0");
        assert_eq!(list[2].name, "Rustup: the Rust toolchain");
        assert_eq!(list[3].version, "1.0");
    }

    /// Etappe 3a-1: Ereignisse nur für den Verlauf; bei `MAX_EREIGNISSE` Starts
    /// ein Satz in `errors` (er erscheint als Hinweis im Verlauf).
    #[test]
    fn bericht_mit_vielen_starts_meldet_die_grenze() {
        assert!(PS_REPORT.matches(&format!("-MaxEvents {MAX_EREIGNISSE} ")).count() == 2, "Grenze im Skript wie MAX_EREIGNISSE");
        let start = "2026-10-30T08:00:00.1234567+01:00".to_string();
        let r = PsReport {
            services: Vec::new(),
            services_total: 0,
            hotfixes: Vec::new(),
            boots: vec![start; MAX_EREIGNISSE],
            updates: vec![PsUpdate { zeit: "2026-10-30T09:00:00+01:00".into(), title: "Security Intelligence-Update (KB2267602)".into() }],
            errors: vec!["Dienste: kaputt".into()],
        };
        let mut s = SysApps::default();
        bericht_einsetzen(&mut s, r);
        assert_eq!(s.errors.len(), 2, "{:?}", s.errors);
        assert!(s.errors[1].starts_with("Startzeiten: Die Ereignisanzeige hat 5000 oder mehr Einträge"), "{:?}", s.errors);
        assert!(!s.ereignisse.is_empty());

        let mut s = SysApps::default();
        let wenige = PsReport { services: Vec::new(), services_total: 0, hotfixes: Vec::new(), boots: Vec::new(), updates: Vec::new(), errors: Vec::new() };
        bericht_einsetzen(&mut s, wenige);
        assert!(s.errors.is_empty());
    }

    #[test]
    fn winget_without_header_is_empty() {
        assert!(parse_winget("kaputt\n").is_empty());
    }
}
