//! Feste Pfade der externen Programme (Nachbesserung vor dem Merge von 7a1).
//!
//! Windows sucht ein Programm ohne Pfad zuerst im Ordner des aufrufenden
//! Programms (`bin`), und den darf der Inhaber ändern. Dienst und Bote starten
//! darum nichts über die PATH-Suche, sondern nur mit vollem Pfad aus festen
//! Orten: System32 (aus `SystemRoot`), Program Files, die Git-Installation
//! laut Registry, im Boten auch die WindowsApps des Inhabers und `.cargo\bin`. Liegt
//! ein Programm an keinem dieser Orte, gilt es als „nicht gefunden“.
//!
//! Die Orte werden einmal je Prozess ermittelt (`Orte::ermitteln`), ob die
//! Datei da ist, bei jedem Aufruf (`pfad_von`).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Die externen Programme, die Dienst und Bote starten.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Werkzeug {
    /// `git.exe` aus der Git-Installation (`cmd\` oder `bin\`).
    Git,
    /// `powershell.exe` aus System32 (Windows PowerShell 5.1).
    WindowsPowerShell,
    /// `pwsh.exe` (PowerShell 7) aus Program Files oder WindowsApps.
    PowerShell7,
    /// `cmd.exe` aus System32 (nur Tests: Verknüpfungen anlegen).
    Cmd,
    /// `winget.exe` aus WindowsApps des Kontos (läuft im Boten als Inhaber).
    Winget,
    /// `cargo.exe` aus `%USERPROFILE%\.cargo\bin`.
    Cargo,
    /// `rustc.exe` aus `%USERPROFILE%\.cargo\bin`.
    Rustc,
    /// `node.exe` aus `%ProgramFiles%\nodejs`.
    Node,
    /// `explorer.exe` aus `%SystemRoot%` (Bote: Ordner öffnen, Entscheidung lokal-oeffnen).
    Explorer,
}

impl Werkzeug {
    /// Name für Sätze und Protokoll.
    pub fn name(self) -> &'static str {
        match self {
            Werkzeug::Git => "git",
            Werkzeug::WindowsPowerShell => "powershell.exe",
            Werkzeug::PowerShell7 => "pwsh",
            Werkzeug::Cmd => "cmd.exe",
            Werkzeug::Winget => "winget",
            Werkzeug::Cargo => "cargo",
            Werkzeug::Rustc => "rustc",
            Werkzeug::Node => "node",
            Werkzeug::Explorer => "explorer.exe",
        }
    }
}

/// Die festen Orte, aus denen Programme kommen dürfen. Nur absolute Pfade;
/// eine leere oder relative Umgebungsvariable zählt als nicht gesetzt.
#[derive(Debug, Clone, Default)]
pub struct Orte {
    /// `%SystemRoot%`, sonst `C:\Windows`.
    pub system_root: Option<PathBuf>,
    /// `%ProgramFiles%`, sonst `C:\Program Files`.
    pub program_files: Option<PathBuf>,
    /// `%LOCALAPPDATA%` des Kontos.
    pub local_app_data: Option<PathBuf>,
    /// `%USERPROFILE%` des Kontos.
    pub user_profile: Option<PathBuf>,
    /// Git-Installation laut `HKLM\SOFTWARE\GitForWindows\InstallPath`.
    pub git_installation: Option<PathBuf>,
}

/// Rückfall, wenn die Registry keine Git-Installation nennt.
const GIT_RUECKFALL: &str = r"C:\Program Files\Git";

/// Nur absolute Pfade aus einer Umgebungsvariablen.
fn absolut(wert: Option<std::ffi::OsString>) -> Option<PathBuf> {
    wert.map(PathBuf::from).filter(|p| p.is_absolute())
}

impl Orte {
    /// Liest Umgebung und Registry dieses Prozesses.
    pub fn ermitteln() -> Orte {
        let env = |name: &str| absolut(std::env::var_os(name));
        Orte {
            system_root: env("SystemRoot").or_else(|| Some(PathBuf::from(r"C:\Windows"))),
            program_files: env("ProgramFiles").or_else(|| Some(PathBuf::from(r"C:\Program Files"))),
            local_app_data: env("LOCALAPPDATA"),
            user_profile: env("USERPROFILE"),
            git_installation: git_aus_registry(),
        }
    }

    /// Die Orte, an denen `w` liegen darf, in der Reihenfolge der Suche.
    pub fn kandidaten(&self, w: Werkzeug) -> Vec<PathBuf> {
        if cfg!(not(windows)) {
            return match w {
                Werkzeug::Git => vec![PathBuf::from("/usr/bin/git")],
                _ => Vec::new(),
            };
        }
        let unter = |basis: &Option<PathBuf>, rest: &[&str]| basis.as_ref().map(|b| rest.iter().fold(b.clone(), |p, teil| p.join(teil)));
        let system32 = |datei: &str| unter(&self.system_root, &["System32", datei]);
        let liste = match w {
            Werkzeug::Git => {
                let mut l = Vec::new();
                for basis in [self.git_installation.clone(), Some(PathBuf::from(GIT_RUECKFALL))] {
                    l.push(unter(&basis, &["cmd", "git.exe"]));
                    l.push(unter(&basis, &["bin", "git.exe"]));
                }
                l
            }
            Werkzeug::WindowsPowerShell => vec![system32(r"WindowsPowerShell\v1.0\powershell.exe")],
            Werkzeug::PowerShell7 => {
                vec![unter(&self.program_files, &["PowerShell", "7", "pwsh.exe"]), unter(&self.local_app_data, &["Microsoft", "WindowsApps", "pwsh.exe"])]
            }
            Werkzeug::Cmd => vec![system32("cmd.exe")],
            Werkzeug::Winget => vec![unter(&self.local_app_data, &["Microsoft", "WindowsApps", "winget.exe"])],
            Werkzeug::Cargo => vec![unter(&self.user_profile, &[".cargo", "bin", "cargo.exe"])],
            Werkzeug::Rustc => vec![unter(&self.user_profile, &[".cargo", "bin", "rustc.exe"])],
            Werkzeug::Node => vec![unter(&self.program_files, &["nodejs", "node.exe"])],
            Werkzeug::Explorer => vec![unter(&self.system_root, &["explorer.exe"])],
        };
        liste.into_iter().flatten().filter(|p| p.is_absolute()).collect()
    }

    /// Der erste Kandidat, der als Datei da ist.
    pub fn pfad_von(&self, w: Werkzeug) -> Option<PathBuf> {
        self.kandidaten(w).into_iter().find(|p| da(p))
    }
}

/// Ist die Datei da? WindowsApps enthält Aliase (Reparse-Punkte vom Typ
/// AppExecLink), denen `metadata` nicht immer folgen kann; dann zählt der
/// Eintrag selbst.
fn da(p: &Path) -> bool {
    match std::fs::metadata(p) {
        Ok(m) => m.is_file(),
        Err(_) => std::fs::symlink_metadata(p).is_ok_and(|m| !m.is_dir()),
    }
}

#[cfg(windows)]
fn git_aus_registry() -> Option<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE};
    let key = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey_with_flags(r"SOFTWARE\GitForWindows", KEY_QUERY_VALUE).ok()?;
    let pfad = PathBuf::from(key.get_value::<String, _>("InstallPath").ok()?);
    pfad.is_absolute().then_some(pfad)
}

#[cfg(not(windows))]
fn git_aus_registry() -> Option<PathBuf> {
    None
}

static ORTE: OnceLock<Orte> = OnceLock::new();

/// Voller Pfad von `w` an einem festen Ort, `None` wenn es dort nicht liegt.
/// Nie über die PATH-Suche.
pub fn pfad_von(w: Werkzeug) -> Option<PathBuf> {
    ORTE.get_or_init(Orte::ermitteln).pfad_von(w)
}

/// Satz, wenn ein Programm fehlt.
pub fn fehlt_satz(w: Werkzeug) -> String {
    format!("{} nicht gefunden (nur an festen Orten gesucht, nicht im PATH)", w.name())
}

// --- Ohne Fenster -----------------------------------------------------------
//
// Regel: Kein Kindprozess öffnet ein sichtbares Fenster. Startet ein Prozess
// ohne eigene Konsole (der Bote, der Dienst, die Aufgabenplanung) ein
// Konsolenprogramm (git, winget, powershell, cmd), legt Windows sonst eine
// neue Konsole an, die kurz auf dem Bildschirm aufblitzt, auch in
// Bildschirmfreigaben. `CREATE_NO_WINDOW` gibt dem Kind eine Konsole ohne
// Fenster; umgeleitete Standard-Handles (Pipes) arbeiten wie gewohnt.
//
// Darum entsteht jeder `Command` nur hier: `befehl`/`befehl_async` für die
// Werkzeuge (fester Pfad und ohne Fenster zusammen), `eigener_befehl` für
// eigene Programme (`current_exe`, die exe-Dateien in Tests). Ein Test
// (`kein_command_ausserhalb`) hält neue Stellen außerhalb dieser Datei auf.

/// `CREATE_NO_WINDOW` aus den Prozess-Erstellungsschaltern von Windows.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Setzt `CREATE_NO_WINDOW` (nur Windows; sonst unverändert).
pub fn ohne_fenster(cmd: &mut std::process::Command) -> &mut std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Setzt `CREATE_NO_WINDOW` beim tokio-Befehl (nur Windows; sonst unverändert).
pub fn ohne_fenster_async(cmd: &mut tokio::process::Command) -> &mut tokio::process::Command {
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// Befehl für ein Werkzeug: voller Pfad von einem festen Ort, ohne Fenster.
/// Fehlt das Werkzeug, der Satz aus `fehlt_satz`.
pub fn befehl(w: Werkzeug) -> Result<std::process::Command, String> {
    let pfad = pfad_von(w).ok_or_else(|| fehlt_satz(w))?;
    let mut c = std::process::Command::new(pfad);
    ohne_fenster(&mut c);
    Ok(c)
}

/// Wie `befehl`, als tokio-Befehl.
pub fn befehl_async(w: Werkzeug) -> Result<tokio::process::Command, String> {
    let pfad = pfad_von(w).ok_or_else(|| fehlt_satz(w))?;
    let mut c = tokio::process::Command::new(pfad);
    ohne_fenster_async(&mut c);
    Ok(c)
}

/// `BELOW_NORMAL_PRIORITY_CLASS` (Etappe 4b): der Baumeister baut mit
/// niedriger Priorität, damit das Cockpit beim Speichern nicht ruckelt.
/// Kinder des Kindes (vite, esbuild) erben die Klasse von Windows.
pub const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;

/// Wie `befehl`, ohne Fenster UND mit niedriger Priorität (Baumeister, 4b).
pub fn befehl_niedrig(w: Werkzeug) -> Result<std::process::Command, String> {
    let mut c = befehl(w)?;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS);
    }
    Ok(c)
}

/// Wie `befehl_async`, ohne Fenster UND mit niedriger Priorität (Baumeister, 4b).
pub fn befehl_niedrig_async(w: Werkzeug) -> Result<tokio::process::Command, String> {
    let mut c = befehl_async(w)?;
    #[cfg(windows)]
    c.creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS);
    Ok(c)
}

/// Stellt einen tokio-Befehl auf niedrige Priorität, ohne Fenster (Baumeister, 4b).
pub fn niedrig_async(cmd: &mut tokio::process::Command) -> &mut tokio::process::Command {
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS);
    cmd
}

/// npm ohne npm.cmd: `node.exe` vom festen Ort und daneben
/// `node_modules\npm\bin\npm-cli.js` als erstes Argument. npm.cmd ginge nur
/// über cmd.exe, und das braucht die PATH-Suche. Fehlt eines, ein Satz.
pub fn npm_befehl_async() -> Result<tokio::process::Command, String> {
    let node = pfad_von(Werkzeug::Node).ok_or_else(|| fehlt_satz(Werkzeug::Node))?;
    let cli = npm_cli(&node);
    if !cli.is_file() {
        return Err(format!("npm nicht gefunden ({} fehlt)", cli.display()));
    }
    let mut c = tokio::process::Command::new(node);
    ohne_fenster_async(&mut c);
    c.arg(cli);
    Ok(c)
}

/// `npm-cli.js` neben `node.exe` (so legt der Node-Installer es ab).
pub fn npm_cli(node: &Path) -> PathBuf {
    let ordner = node.parent().map(Path::to_path_buf).unwrap_or_default();
    ordner.join("node_modules").join("npm").join("bin").join("npm-cli.js")
}

/// Befehl für ein eigenes Programm mit bekanntem vollem Pfad (`current_exe`,
/// `CARGO_BIN_EXE_…` in Tests), ohne Fenster. Nie für Werkzeuge: die gehen
/// über `befehl`, damit sie nur von festen Orten kommen.
pub fn eigener_befehl(programm: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    let mut c = std::process::Command::new(programm);
    ohne_fenster(&mut c);
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Temp-Ordner mit leeren Programmdateien; räumt beim Freigeben auf.
    struct Ordner(PathBuf);

    impl Ordner {
        fn neu(name: &str) -> Ordner {
            let o = std::env::temp_dir().join(format!("dashboard-programme-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&o);
            std::fs::create_dir_all(&o).unwrap();
            Ordner(o)
        }

        fn datei(&self, rest: &str) -> PathBuf {
            let p = self.0.join(rest);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, b"").unwrap();
            p
        }
    }

    impl Drop for Ordner {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn orte(o: &Ordner) -> Orte {
        Orte {
            system_root: Some(o.0.join("Windows")),
            program_files: Some(o.0.join("Programme")),
            local_app_data: Some(o.0.join("Lokal")),
            user_profile: Some(o.0.join("Profil")),
            git_installation: Some(o.0.join("Git")),
        }
    }

    #[cfg(windows)]
    #[test]
    fn vorhandene_programme_an_festen_orten() {
        let o = Ordner::neu("da");
        let orte = orte(&o);
        let ps = o.datei(r"Windows\System32\WindowsPowerShell\v1.0\powershell.exe");
        let cmd = o.datei(r"Windows\System32\cmd.exe");
        let pwsh = o.datei(r"Programme\PowerShell\7\pwsh.exe");
        let node = o.datei(r"Programme\nodejs\node.exe");
        let winget = o.datei(r"Lokal\Microsoft\WindowsApps\winget.exe");
        let cargo = o.datei(r"Profil\.cargo\bin\cargo.exe");
        let rustc = o.datei(r"Profil\.cargo\bin\rustc.exe");
        let git_bin = o.datei(r"Git\bin\git.exe");
        assert_eq!(orte.pfad_von(Werkzeug::WindowsPowerShell), Some(ps));
        assert_eq!(orte.pfad_von(Werkzeug::Cmd), Some(cmd));
        assert_eq!(orte.pfad_von(Werkzeug::PowerShell7), Some(pwsh));
        assert_eq!(orte.pfad_von(Werkzeug::Node), Some(node));
        assert_eq!(orte.pfad_von(Werkzeug::Winget), Some(winget));
        assert_eq!(orte.pfad_von(Werkzeug::Cargo), Some(cargo));
        assert_eq!(orte.pfad_von(Werkzeug::Rustc), Some(rustc));
        // Nur bin\git.exe da: das gilt; liegt auch cmd\git.exe da, geht es vor.
        assert_eq!(orte.pfad_von(Werkzeug::Git), Some(git_bin));
        let git_cmd = o.datei(r"Git\cmd\git.exe");
        assert_eq!(orte.pfad_von(Werkzeug::Git), Some(git_cmd));
        // PowerShell 7 aus WindowsApps, wenn Program Files keins hat.
        std::fs::remove_file(o.0.join(r"Programme\PowerShell\7\pwsh.exe")).unwrap();
        let pwsh_apps = o.datei(r"Lokal\Microsoft\WindowsApps\pwsh.exe");
        assert_eq!(orte.pfad_von(Werkzeug::PowerShell7), Some(pwsh_apps));
    }

    #[cfg(windows)]
    #[test]
    fn fehlende_programme_sind_nicht_gefunden() {
        let o = Ordner::neu("fehlt");
        let mut orte = orte(&o);
        // Git ohne Registry-Eintrag: nur der Rückfall unter C:\Program Files.
        orte.git_installation = None;
        for w in [Werkzeug::WindowsPowerShell, Werkzeug::Cmd, Werkzeug::PowerShell7, Werkzeug::Node, Werkzeug::Winget, Werkzeug::Cargo, Werkzeug::Rustc] {
            assert_eq!(orte.pfad_von(w), None, "{w:?}");
        }
        assert_eq!(orte.kandidaten(Werkzeug::Git), [PathBuf::from(r"C:\Program Files\Git\cmd\git.exe"), PathBuf::from(r"C:\Program Files\Git\bin\git.exe")]);
        // Ein Ordner mit dem Namen des Programms zählt nicht.
        std::fs::create_dir_all(o.0.join(r"Programme\nodejs\node.exe")).unwrap();
        assert_eq!(orte.pfad_von(Werkzeug::Node), None);
        // Ohne Umgebungsvariable gibt es keinen Kandidaten, auch keinen relativen.
        let leer = Orte::default();
        assert!(leer.kandidaten(Werkzeug::Winget).is_empty());
        assert!(leer.kandidaten(Werkzeug::Cargo).is_empty());
        assert!(leer.kandidaten(Werkzeug::WindowsPowerShell).is_empty());
    }

    #[test]
    fn relative_umgebung_zaehlt_nicht() {
        assert_eq!(absolut(Some("bin".into())), None);
        assert_eq!(absolut(Some("".into())), None);
        assert_eq!(absolut(None), None);
        let ganz = std::env::temp_dir();
        assert_eq!(absolut(Some(ganz.clone().into_os_string())), Some(ganz));
    }

    /// Auf diesem Rechner: alles Gefundene ist absolut und liegt nicht im
    /// Ordner des Testprogramms; Windows PowerShell und cmd gibt es immer.
    #[cfg(windows)]
    #[test]
    fn echte_orte_dieses_rechners() {
        let bin = std::env::current_exe().unwrap().parent().unwrap().to_path_buf();
        for w in [Werkzeug::WindowsPowerShell, Werkzeug::Cmd] {
            let p = pfad_von(w).unwrap_or_else(|| panic!("{w:?} fehlt"));
            assert!(p.ends_with(if w == Werkzeug::Cmd { r"System32\cmd.exe" } else { r"System32\WindowsPowerShell\v1.0\powershell.exe" }));
        }
        for w in [
            Werkzeug::Git,
            Werkzeug::WindowsPowerShell,
            Werkzeug::PowerShell7,
            Werkzeug::Cmd,
            Werkzeug::Winget,
            Werkzeug::Cargo,
            Werkzeug::Rustc,
            Werkzeug::Node,
        ] {
            let gefunden = pfad_von(w);
            eprintln!("{w:?}: {gefunden:?}");
            if let Some(p) = gefunden {
                assert!(p.is_absolute(), "{w:?}: {}", p.display());
                assert!(!p.starts_with(&bin), "{w:?}: {}", p.display());
            }
        }
        // Den Explorer gibt es immer, direkt in SystemRoot (nie aus dem PATH).
        assert!(pfad_von(Werkzeug::Explorer).is_some_and(|p| p.ends_with("explorer.exe") && !p.starts_with(&bin)));
        assert_eq!(fehlt_satz(Werkzeug::Node), "node nicht gefunden (nur an festen Orten gesucht, nicht im PATH)");
    }

    /// Die Befehle ohne Fenster laufen: voller Pfad, Ergebnis kommt über die
    /// Pipe zurück. Das Flag selbst lässt sich nicht auslesen; dass jede
    /// Stelle es bekommt, prüft `kein_command_ausserhalb`.
    #[cfg(windows)]
    #[test]
    fn befehle_ohne_fenster_laufen() {
        let mut c = befehl(Werkzeug::Cmd).unwrap();
        assert!(Path::new(c.get_program()).is_absolute());
        let out = c.args(["/c", "echo", "da"]).output().unwrap();
        assert!(out.status.success());
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "da");
        let mut eigen = std::process::Command::new(pfad_von(Werkzeug::Cmd).unwrap());
        assert_eq!(ohne_fenster(&mut eigen).args(["/c", "exit", "3"]).status().unwrap().code(), Some(3));
        assert_eq!(CREATE_NO_WINDOW, 0x0800_0000);
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn befehl_async_ohne_fenster_laeuft() {
        let out = befehl_async(Werkzeug::Cmd).unwrap().args(["/c", "exit", "0"]).output().await.unwrap();
        assert!(out.status.success());
        assert_eq!(befehl(Werkzeug::Node).is_err(), pfad_von(Werkzeug::Node).is_none());
    }

    #[test]
    fn npm_liegt_neben_node() {
        let cli = npm_cli(Path::new(r"C:\Program Files\nodejs\node.exe"));
        assert_eq!(cli, PathBuf::from(r"C:\Program Files\nodejs\node_modules\npm\bin\npm-cli.js"));
        // Auf diesem Rechner: mit Node ist auch npm-cli.js da.
        if pfad_von(Werkzeug::Node).is_some() {
            let c = npm_befehl_async().unwrap();
            let args: Vec<_> = c.as_std().get_args().collect();
            assert!(Path::new(args[0]).ends_with(r"npm\bin\npm-cli.js"), "{args:?}");
        }
    }

    /// Wächter gegen neue Stellen: Außerhalb dieser Datei entsteht kein
    /// `Command` direkt, sondern nur über `befehl`, `befehl_async` oder
    /// `eigener_befehl` (alle ohne Fenster). Durchsucht alle .rs-Dateien unter
    /// `src\` und `tests\`.
    #[test]
    fn kein_command_ausserhalb() {
        // Zusammengesetzt, damit diese Zeile sich nicht selbst findet.
        let muster = ["Command", "::new"].concat();
        let wurzel = Path::new(env!("CARGO_MANIFEST_DIR"));
        let diese = wurzel.join("src").join("dienst").join("programme.rs");
        let mut dateien = Vec::new();
        let mut offen = vec![wurzel.join("src"), wurzel.join("tests")];
        while let Some(ordner) = offen.pop() {
            for e in std::fs::read_dir(&ordner).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    offen.push(p);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    dateien.push(p);
                }
            }
        }
        assert!(dateien.len() > 20, "zu wenige .rs-Dateien gefunden: {}", dateien.len());
        let mut funde = Vec::new();
        for p in dateien.iter().filter(|p| **p != diese) {
            let text = std::fs::read_to_string(p).unwrap();
            for (nr, zeile) in text.lines().enumerate() {
                if zeile.contains(&muster) {
                    funde.push(format!("{}:{}: {}", p.display(), nr + 1, zeile.trim()));
                }
            }
        }
        assert!(funde.is_empty(), "Kindprozesse nur über dienst::programme::befehl/befehl_async/eigener_befehl (ohne Fenster):\n{}", funde.join("\n"));
    }
}
