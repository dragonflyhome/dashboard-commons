//! Die Datei mit dem Schlüssel für die Ausweise (JWT, Ed25519): 32 zufällige
//! Bytes. Im Dienst liegt sie in `C:/ProgramData/Dashboard/geheim/jwt-schluessel`
//! (`Einstellungen::jwt_schluessel_datei`), sonst neben dem Datenspeicher
//! (`app::jwt_schluessel_datei`, für Tests über `DASHBOARD_JWT_SCHLUESSEL`).
//!
//! Beim ersten Start legt der Dienst sie selbst an, und zwar gleich mit enger
//! Freigabe: Ordner `geheim` und Datei bekommen eine geschützte DACL (keine
//! Vererbung von oben) mit Vollzugriff nur für SYSTEM, die Administratoren und
//! das Konto, unter dem der Dienst läuft (`NT SERVICE\Dashboard`). Ein
//! bestehender Ordner behält seine Rechte; die Datei selbst ist trotzdem eng.
//!
//! Eine vorhandene Datei wird nie überschrieben: Hat sie nicht genau 32
//! Bytes, gilt sie als kaputt, und die Anmeldung mit Passkey bleibt aus, bis
//! jemand nachsieht (sonst wären alle Ausweise auf einen Schlag ungültig, und
//! ein Fehler bliebe unbemerkt).
//!
//! Unter macOS und Linux liegt der Schlüssel ohne Einstellungsdatei in einem
//! eigenen Schlüsselordner außerhalb des Datenbaums ([`ort_ohne_einstellungen`]):
//! Geheimes steht nie neben dem Datenspeicher, der gesichert und kopiert
//! wird. Ordner (0700) und Datei (0600) entstehen gleich mit engen Rechten.
//! Vor dem Laden und vor dem Anlegen wird geprüft, an jedem Ort, auch dem
//! aus der Einstellungsdatei und aus `DASHBOARD_JWT_SCHLUESSEL`: Der Ordner
//! ist ein Ordner und keine Verknüpfung, gehört der eigenen Benutzernummer
//! und gibt Gruppe und anderen kein Recht; die Datei ist eine gewöhnliche
//! Datei mit denselben Bedingungen und genau 32 Bytes. Ein zu offener Ordner
//! oder Schlüssel wird abgelehnt und nie still nachgebessert. Grenze: Eine
//! ACL unter macOS (`chmod +a`) sieht diese Prüfung nicht.

use std::io::Write;
use std::path::Path;

use super::jwt::JwtSchluessel;

/// Lädt den Schlüssel; fehlt die Datei, wird sie angelegt. Fehler als Satz fürs Protokoll.
#[cfg(not(unix))]
pub fn laden_oder_anlegen(pfad: &Path) -> Result<JwtSchluessel, String> {
    match std::fs::read(pfad) {
        Ok(bytes) => {
            let b: [u8; 32] = bytes.try_into().map_err(|_| format!("{} hat nicht genau 32 Bytes; die Anmeldung mit Passkey bleibt aus", pfad.display()))?;
            Ok(JwtSchluessel::aus_bytes(&b))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut b = [0u8; 32];
            b.copy_from_slice(&super::zufall(32));
            match anlegen(pfad, &b) {
                Ok(()) => {
                    tracing::info!("Schlüssel für die Ausweise neu angelegt: {}", pfad.display());
                    Ok(JwtSchluessel::aus_bytes(&b))
                }
                // Ein anderer Lauf war schneller: dessen Datei gilt.
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => laden_oder_anlegen(pfad),
                Err(e) => Err(format!("{} ließ sich nicht anlegen: {e}", pfad.display())),
            }
        }
        Err(e) => Err(format!("{} ist nicht lesbar: {e}", pfad.display())),
    }
}

// ----- Ort ohne Einstellungsdatei -----

/// Name des Schlüsselordners und der Datei darin.
pub const SCHLUESSEL_ORDNER: &str = "dashboard-schluessel";
pub const SCHLUESSEL_DATEI: &str = "jwt-schluessel";

/// Die Plattform als Angabe, damit die Wahl des Orts überall prüfbar ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plattform {
    Windows,
    MacOs,
    /// Linux und andere Unix-Systeme.
    Unix,
}

impl Plattform {
    /// Die Plattform, für die dieses Programm gebaut ist.
    pub fn diese() -> Plattform {
        if cfg!(windows) {
            Plattform::Windows
        } else if cfg!(target_os = "macos") {
            Plattform::MacOs
        } else {
            Plattform::Unix
        }
    }
}

/// Ein vollständiger Pfad in der Form von macOS und Linux (als Text geprüft,
/// damit das Urteil auf jeder Plattform gleich ausfällt).
fn vollstaendig(wert: &str) -> bool {
    wert.starts_with('/')
}

/// Wo der Schlüssel ohne Einstellungsdatei liegt (`DASHBOARD_JWT_SCHLUESSEL`
/// geht dem vor, siehe `app::jwt_schluessel_datei`):
///
/// - Windows: `geheim/jwt-schluessel` neben der Datei des Datenspeichers, wie bisher.
/// - macOS: `<HOME>/Library/Application Support/dashboard-schluessel/jwt-schluessel`.
/// - Linux und andere: `<XDG_DATA_HOME>/dashboard-schluessel/jwt-schluessel`,
///   wenn `XDG_DATA_HOME` ein vollständiger Pfad ist, sonst
///   `<HOME>/.local/share/dashboard-schluessel/jwt-schluessel`.
///
/// Außerhalb von Windows nie im Datenbaum. Ohne vollständigen Wert gibt es
/// keinen Ort (`None`): kein Rückfall auf den Arbeitsordner, der unter
/// launchd `/` wäre. Die Anmeldung mit Passkey bleibt dann aus. Unter macOS
/// zählt nur `HOME`; unter Linux genügt ein vollständiges `XDG_DATA_HOME`,
/// auch wenn `HOME` fehlt, denn der Ort hängt dann nicht an `HOME`.
pub fn ort_ohne_einstellungen(plattform: Plattform, daten: &Path, home: Option<&str>, xdg_data_home: Option<&str>) -> Option<std::path::PathBuf> {
    let home = home.filter(|h| vollstaendig(h)).map(|h| h.trim_end_matches('/'));
    let ordner = match plattform {
        Plattform::Windows => return Some(daten.parent().map(Path::to_path_buf).unwrap_or_default().join("geheim").join(SCHLUESSEL_DATEI)),
        Plattform::MacOs => format!("{}/Library/Application Support/{SCHLUESSEL_ORDNER}", home?),
        Plattform::Unix => match xdg_data_home.filter(|x| vollstaendig(x)) {
            Some(xdg) => format!("{}/{SCHLUESSEL_ORDNER}", xdg.trim_end_matches('/')),
            None => format!("{}/.local/share/{SCHLUESSEL_ORDNER}", home?),
        },
    };
    Some(std::path::PathBuf::from(format!("{ordner}/{SCHLUESSEL_DATEI}")))
}

// ----- Urteil über Ordner und Datei (reine Funktionen, auf jeder Plattform geprüft) -----

/// Warum ein Schlüsselordner oder ein Schlüssel abgelehnt wird.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mangel {
    /// Der Ordner ist eine Verknüpfung oder kein Ordner.
    OrdnerKeinOrdner,
    /// Der Ordner gehört einem anderen Konto.
    OrdnerFremd,
    /// Gruppe oder andere haben ein Recht am Ordner.
    OrdnerOffen,
    /// Die Datei ist keine gewöhnliche Datei (Verknüpfung, Ordner, FIFO).
    DateiKeineDatei,
    /// Die Datei gehört einem anderen Konto.
    DateiFremd,
    /// Gruppe oder andere haben ein Recht an der Datei.
    DateiOffen,
    /// Die Datei hat nicht genau 32 Bytes.
    DateiGroesse,
}

/// Rechte für Gruppe und andere.
const FREMDE_RECHTE: u32 = 0o077;

/// Was über einen Ordner oder eine Datei bekannt ist (aus den Metadaten).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stand {
    /// Ein Ordner (für den Ordner) oder eine gewöhnliche Datei (für die Datei), keine Verknüpfung.
    pub richtige_art: bool,
    pub uid: u32,
    pub modus: u32,
    pub groesse: u64,
}

/// Ist der Schlüsselordner in Ordnung? Er ist ein Ordner und keine
/// Verknüpfung, gehört `eigene_uid` und gibt Gruppe und anderen kein Recht.
pub fn ordner_urteil(stand: Stand, eigene_uid: u32) -> Result<(), Mangel> {
    if !stand.richtige_art {
        return Err(Mangel::OrdnerKeinOrdner);
    }
    if stand.uid != eigene_uid {
        return Err(Mangel::OrdnerFremd);
    }
    if stand.modus & FREMDE_RECHTE != 0 {
        return Err(Mangel::OrdnerOffen);
    }
    Ok(())
}

/// Ist der Schlüssel in Ordnung? Eine gewöhnliche Datei, die `eigene_uid`
/// gehört, Gruppe und anderen kein Recht gibt und genau 32 Bytes hat.
pub fn datei_urteil(stand: Stand, eigene_uid: u32) -> Result<(), Mangel> {
    if !stand.richtige_art {
        return Err(Mangel::DateiKeineDatei);
    }
    if stand.uid != eigene_uid {
        return Err(Mangel::DateiFremd);
    }
    if stand.modus & FREMDE_RECHTE != 0 {
        return Err(Mangel::DateiOffen);
    }
    if stand.groesse != 32 {
        return Err(Mangel::DateiGroesse);
    }
    Ok(())
}

/// Der Satz zu einem Mangel, mit dem, was zu tun ist. `ordner` und `datei`
/// kommen schon so, wie sie gezeigt werden dürfen (ohne Profil und Namen).
pub fn mangel_satz(mangel: Mangel, ordner: &str, datei: &str) -> String {
    let aus = "Die Anmeldung mit Passkey bleibt aus";
    match mangel {
        Mangel::OrdnerKeinOrdner => format!("Der Schlüsselordner {ordner} ist kein Ordner oder eine Verknüpfung. {aus}."),
        Mangel::OrdnerFremd => format!("Der Schlüsselordner {ordner} gehört einem anderen Konto. {aus}."),
        Mangel::OrdnerOffen => format!("Der Schlüsselordner {ordner} ist für andere Konten offen. Bitte die Rechte 700 setzen. {aus}, bis das geschehen ist."),
        Mangel::DateiKeineDatei => format!("Der Schlüssel {datei} ist keine gewöhnliche Datei (Verknüpfung, Ordner oder Ähnliches). {aus}."),
        Mangel::DateiFremd => format!("Der Schlüssel {datei} gehört einem anderen Konto. {aus}."),
        Mangel::DateiOffen => format!(
            "Der Schlüssel {datei} ist für andere Konten lesbar. Bitte die Rechte 600 setzen oder die Datei entfernen (dann entsteht ein neuer Schlüssel). {aus}, bis das geschehen ist."
        ),
        Mangel::DateiGroesse => format!("{datei} hat nicht genau 32 Bytes; die Anmeldung mit Passkey bleibt aus"),
    }
}

// ----- macOS und Linux -----

/// Lädt den Schlüssel; fehlt die Datei, wird sie angelegt. Fehler als Satz fürs Protokoll.
#[cfg(unix)]
pub fn laden_oder_anlegen(pfad: &Path) -> Result<JwtSchluessel, String> {
    laden_oder_anlegen_mit(pfad, &time_machine_ausnehmen)
}

/// Wie [`laden_oder_anlegen`]; `ausnehmen` nimmt einen neu angelegten
/// Schlüsselordner von der Sicherung aus (in Tests eine eigene Funktion).
#[cfg(unix)]
pub(crate) fn laden_oder_anlegen_mit(pfad: &Path, ausnehmen: &dyn Fn(&Path) -> Result<(), String>) -> Result<JwtSchluessel, String> {
    laden_oder_anlegen_versuch(pfad, ausnehmen, true)
}

/// Ob der Schlüsselordner von der Sicherung ausgenommen wird: nur ein Ordner,
/// der in diesem Aufruf neu entstanden ist, und nur mit vollständigem Pfad.
/// Ein relativer Pfad, der mit `-` beginnt, ginge sonst als Option an das
/// Programm. Reine Funktion, damit die Bedingung auf jeder Plattform prüfbar
/// ist; unter Windows gibt es sie nur im Test.
#[cfg(any(unix, test))]
fn ausnahme_setzen(neu_angelegt: bool, ordner: &Path) -> bool {
    neu_angelegt && ordner.is_absolute()
}

/// `nochmal`: Kommt ein anderer Lauf beim Anlegen zuvor, wird genau einmal neu geladen, nicht beliebig tief.
#[cfg(unix)]
fn laden_oder_anlegen_versuch(pfad: &Path, ausnehmen: &dyn Fn(&Path) -> Result<(), String>, nochmal: bool) -> Result<JwtSchluessel, String> {
    use std::io::Read;
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
    let zeigen = |p: &Path| crate::datenschutz::verbergen(&p.display().to_string());
    let ordner = pfad
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .ok_or_else(|| format!("{} nennt keinen Ordner; die Anmeldung mit Passkey bleibt aus", zeigen(pfad)))?;
    let satz = |m: Mangel| mangel_satz(m, &zeigen(ordner), &zeigen(pfad));
    let eigene_uid = crate::unix::eigene_uid();
    // Der Ordner: anlegen, wenn er fehlt (0700, auch für fehlende Ordner darüber).
    let mut neu_angelegt = false;
    if let Err(e) = std::fs::symlink_metadata(ordner) {
        if e.kind() != std::io::ErrorKind::NotFound {
            return Err(format!("Der Schlüsselordner {} ist nicht lesbar: {e}", zeigen(ordner)));
        }
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(ordner)
            .map_err(|e| format!("Der Schlüsselordner {} ließ sich nicht anlegen: {e}", zeigen(ordner)))?;
        neu_angelegt = true;
    }
    // Derselbe Blick vor dem Laden und vor dem Anlegen: beim ersten und beim zweiten Start dasselbe Urteil.
    let m = std::fs::symlink_metadata(ordner).map_err(|e| format!("Der Schlüsselordner {} ist nicht lesbar: {e}", zeigen(ordner)))?;
    ordner_urteil(Stand { richtige_art: m.file_type().is_dir(), uid: m.uid(), modus: m.mode(), groesse: 0 }, eigene_uid).map_err(satz)?;
    // Die Ausnahme für die Sicherung erst nach dem Urteil und nur für einen
    // vollständigen Pfad: Ein relativer Pfad aus der Umgebung, der mit `-`
    // beginnt, ginge sonst als Option an das Programm. Zusätzliche
    // Absicherung, kein Schutz: Schutz sind Ort und Rechte.
    if ausnahme_setzen(neu_angelegt, ordner)
        && let Err(e) = ausnehmen(ordner)
    {
        tracing::warn!("Ausnahme für Time Machine nicht gesetzt ({e}).");
    }
    // Die Datei: nie einer Verknüpfung folgen; ohne Blockieren, falls dort ein FIFO liegt.
    let oeffnen = std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(pfad);
    let mut datei = match oeffnen {
        Ok(d) => d,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut b = [0u8; 32];
            b.copy_from_slice(&super::zufall(32));
            return match ohne_halbstand_anlegen(ordner, pfad, &b) {
                Ok(()) => {
                    tracing::info!("Schlüssel für die Ausweise neu angelegt: {}", zeigen(pfad));
                    Ok(JwtSchluessel::aus_bytes(&b))
                }
                // Ein anderer Lauf war schneller: dessen Datei gilt. Höchstens ein erneuter Versuch.
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists && nochmal => laden_oder_anlegen_versuch(pfad, ausnehmen, false),
                Err(e) => Err(format!("{} ließ sich nicht anlegen: {e}", zeigen(pfad))),
            };
        }
        // Eine Verknüpfung an dieser Stelle (ELOOP mit O_NOFOLLOW), auch eine ohne
        // Ziel. Ein FIFO kommt nicht hierher: Er lässt sich öffnen und wird unten
        // über die Art abgelehnt.
        Err(e) if e.raw_os_error() == Some(libc::ELOOP) => return Err(satz(Mangel::DateiKeineDatei)),
        Err(e) => return Err(format!("{} ist nicht lesbar: {e}", zeigen(pfad))),
    };
    // Prüfen am geöffneten Handle, nicht noch einmal über den Pfad: kein Wettlauf zwischen Prüfen und Lesen.
    let m = datei.metadata().map_err(|e| format!("{} ist nicht lesbar: {e}", zeigen(pfad)))?;
    datei_urteil(Stand { richtige_art: m.file_type().is_file(), uid: m.uid(), modus: m.mode(), groesse: m.len() }, eigene_uid).map_err(satz)?;
    let mut b = [0u8; 32];
    datei.read_exact(&mut b).map_err(|_| satz(Mangel::DateiGroesse))?;
    let mut rest = [0u8; 1];
    match datei.read(&mut rest) {
        Ok(0) => Ok(JwtSchluessel::aus_bytes(&b)),
        Ok(_) => Err(satz(Mangel::DateiGroesse)),
        Err(e) => Err(format!("{} ist nicht lesbar: {e}", zeigen(pfad))),
    }
}

/// Legt den Schlüssel ohne Halbstand an: Die 32 Bytes gehen in eine
/// Zwischendatei im selben Ordner (neu, 0600, bis auf die Platte geschrieben),
/// dann bekommt sie mit einer harten Verknüpfung den endgültigen Namen. Das
/// ist ein einziger Schritt: Unter dem Namen steht nie eine leere oder halbe
/// Datei, auch nicht für einen zweiten Prozess, der gleichzeitig startet, und
/// auch nicht, wenn das Schreiben scheitert. Gibt es den Namen schon, meldet
/// die Verknüpfung `AlreadyExists`, und die vorhandene Datei bleibt, wie sie
/// ist. Die Zwischendatei wird in jedem Fall entfernt.
#[cfg(unix)]
fn ohne_halbstand_anlegen(ordner: &Path, pfad: &Path, bytes: &[u8; 32]) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let zusatz: String = super::zufall(8).iter().map(|b| format!("{b:02x}")).collect();
    let zwischen = ordner.join(format!(".{SCHLUESSEL_DATEI}.neu-{}-{zusatz}", std::process::id()));
    let geschrieben = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&zwischen)
        .and_then(|mut d| d.write_all(bytes).and_then(|()| d.sync_all()));
    // Ohne harte Verknüpfungen (FAT, exFAT, manches Netzlaufwerk) entsteht kein
    // Schlüssel; kein Rückfall auf direktes Schreiben, aber der Satz nennt die Ursache.
    let ergebnis = geschrieben.and_then(|()| {
        std::fs::hard_link(&zwischen, pfad).map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => e,
            art => std::io::Error::new(
                art,
                format!("harte Verknüpfung nicht möglich ({e}); der Ort braucht ein Dateisystem mit harten Verknüpfungen, sonst DASHBOARD_JWT_SCHLUESSEL an einen anderen Ort legen"),
            ),
        })
    });
    let _ = std::fs::remove_file(&zwischen);
    ergebnis
}

/// Nimmt den Schlüsselordner von Time Machine aus. Nur unter macOS, nur das
/// eine Programm vom festen Ort (`/usr/bin/tmutil`) mit festen Argumenten, ohne Shell, mit
/// kurzer Frist; seine Ausgabe geht nirgendwohin. Auf anderen Systemen ohne Wirkung.
#[cfg(unix)]
fn time_machine_ausnehmen(ordner: &Path) -> Result<(), String> {
    // In den Unit-Tests der Bibliothek nie das echte Programm: Dort wird die
    // Ausnahme eingespeist (`laden_oder_anlegen_mit`). Die Integrationstests
    // starten dagegen das echte Programm mit einem Schlüsselordner in einem
    // Temp-Ordner; dort läuft `tmutil` unter macOS wirklich. Das ist harmlos,
    // die Ausnahme verschwindet mit dem Ordner.
    if !cfg!(target_os = "macos") || cfg!(test) {
        return Ok(());
    }
    use std::process::Stdio;
    // Über die Liste der Werkzeuge: voller Pfad vom festen Ort, nie über PATH.
    let mut kind = crate::dienst::programme::befehl(crate::dienst::programme::Werkzeug::Tmutil)?
        .arg("addexclusion")
        .arg(ordner)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("tmutil ließ sich nicht starten: {}", e.kind()))?;
    let frist = std::time::Instant::now() + TMUTIL_FRIST;
    loop {
        match kind.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => return Err(format!("Exit-Code {}", status.code().map_or("unbekannt".to_string(), |c| c.to_string()))),
            Ok(None) if std::time::Instant::now() < frist => std::thread::sleep(std::time::Duration::from_millis(50)),
            Ok(None) => {
                let _ = kind.kill();
                let _ = kind.wait();
                return Err("Frist abgelaufen".to_string());
            }
            Err(e) => return Err(format!("tmutil ließ sich nicht abfragen: {}", e.kind())),
        }
    }
}

/// Wie lange `tmutil` höchstens laufen darf; hängt es, geht der Start weiter.
#[cfg(unix)]
const TMUTIL_FRIST: std::time::Duration = std::time::Duration::from_secs(3);

/// SDDL der Freigabe: geschützt, Vollzugriff für SYSTEM, Administratoren und `sid`.
/// `erben`: für den Ordner (gilt auch für Dateien darin).
pub fn sddl(sid: &str, erben: bool) -> String {
    let f = if erben { "OICI" } else { "" };
    format!("D:P(A;{f};FA;;;SY)(A;{f};FA;;;BA)(A;{f};FA;;;{sid})")
}

#[cfg(windows)]
fn anlegen(pfad: &Path, bytes: &[u8; 32]) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::Foundation::{GENERIC_WRITE, INVALID_HANDLE_VALUE, LocalFree};
    use windows_sys::Win32::Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1};
    use windows_sys::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
    use windows_sys::Win32::Storage::FileSystem::{CREATE_NEW, CreateDirectoryW, CreateFileW, FILE_ATTRIBUTE_NORMAL};

    fn breit(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    /// Sicherheitsbeschreibung aus SDDL, mit `LocalFree` freigegeben.
    struct Beschreibung(PSECURITY_DESCRIPTOR);
    impl Drop for Beschreibung {
        fn drop(&mut self) {
            // SAFETY: von ConvertStringSecurityDescriptorToSecurityDescriptorW mit LocalAlloc angelegt.
            unsafe { LocalFree(self.0) };
        }
    }
    fn beschreibung(sddl: &str) -> std::io::Result<Beschreibung> {
        let text = breit(std::ffi::OsStr::new(sddl));
        let mut psd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
        // SAFETY: nullterminierter Text; psd wird von der API gefüllt und unten freigegeben.
        let ok = unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(text.as_ptr(), SDDL_REVISION_1, &mut psd, std::ptr::null_mut()) };
        if ok == 0 || psd.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Beschreibung(psd))
    }
    let attribute =
        |b: &Beschreibung| SECURITY_ATTRIBUTES { nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32, lpSecurityDescriptor: b.0, bInheritHandle: 0 };

    let sid = crate::anmeldung::sspi::eigene_sid().map_err(std::io::Error::other)?;
    if let Some(ordner) = pfad.parent().filter(|p| !p.as_os_str().is_empty())
        && !ordner.is_dir()
    {
        if let Some(darueber) = ordner.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(darueber)?;
        }
        let sd = beschreibung(&sddl(&sid, true))?;
        let sa = attribute(&sd);
        let name = breit(ordner.as_os_str());
        // SAFETY: nullterminierter Name, gültige Sicherheitsangaben für die Dauer des Aufrufs.
        if unsafe { CreateDirectoryW(name.as_ptr(), &sa) } == 0 {
            let e = std::io::Error::last_os_error();
            if !ordner.is_dir() {
                return Err(e);
            }
        }
    }
    let sd = beschreibung(&sddl(&sid, false))?;
    let sa = attribute(&sd);
    let name = breit(pfad.as_os_str());
    // SAFETY: nullterminierter Name, gültige Sicherheitsangaben; das Handle übernimmt File.
    let h = unsafe { CreateFileW(name.as_ptr(), GENERIC_WRITE, 0, &sa, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, std::ptr::null_mut()) };
    if h == INVALID_HANDLE_VALUE || h.is_null() {
        return Err(std::io::Error::last_os_error());
    }
    // SAFETY: gültiges Handle aus CreateFileW, gehört ab jetzt der Datei.
    let mut datei = unsafe { std::fs::File::from_raw_handle(h as _) };
    datei.write_all(bytes)?;
    datei.sync_all()
}

#[cfg(not(any(windows, unix)))]
fn anlegen(pfad: &Path, bytes: &[u8; 32]) -> std::io::Result<()> {
    if let Some(ordner) = pfad.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(ordner)?;
    }
    let mut datei = std::fs::OpenOptions::new().write(true).create_new(true).open(pfad)?;
    datei.write_all(bytes)?;
    datei.sync_all()
}

/// Die DACL einer Datei als SDDL (nur für Tests und Prüfungen).
#[cfg(all(windows, test))]
pub fn dacl_text(pfad: &Path) -> Result<String, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW, GetNamedSecurityInfoW, SDDL_REVISION_1, SE_FILE_OBJECT,
    };
    use windows_sys::Win32::Security::{DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR};
    let name: Vec<u16> = pfad.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
    // SAFETY: nullterminierter Name; sd wird mit LocalFree freigegeben.
    let r = unsafe {
        GetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut sd,
        )
    };
    if r != 0 {
        return Err(format!("GetNamedSecurityInfoW: {r}"));
    }
    let mut text: *mut u16 = std::ptr::null_mut();
    // SAFETY: sd stammt von oben; text wird mit LocalFree freigegeben.
    unsafe {
        let ok = ConvertSecurityDescriptorToStringSecurityDescriptorW(sd, SDDL_REVISION_1, DACL_SECURITY_INFORMATION, &mut text, std::ptr::null_mut());
        LocalFree(sd);
        if ok == 0 {
            return Err("ConvertSecurityDescriptorToStringSecurityDescriptorW".into());
        }
        let mut n = 0;
        while *text.add(n) != 0 {
            n += 1;
        }
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(text, n));
        LocalFree(text as _);
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ordner(name: &str) -> std::path::PathBuf {
        let o = std::env::temp_dir().join(format!("dashboard-geheim-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&o);
        o
    }

    #[test]
    fn wird_angelegt_und_beim_zweiten_mal_gelesen() {
        let o = ordner("anlegen");
        let pfad = o.join("geheim").join("jwt-schluessel");
        let k = laden_oder_anlegen(&pfad).unwrap();
        assert_eq!(std::fs::read(&pfad).unwrap().len(), 32);
        let t = k.ausstellen(&crate::passkey::jwt::Ausweis::neu("c", 1_793_000_000));
        let zweiter = laden_oder_anlegen(&pfad).unwrap();
        assert!(zweiter.pruefen(&t, 1_793_000_001).is_ok(), "derselbe Schlüssel");
        #[cfg(windows)]
        {
            let sid = crate::anmeldung::sspi::eigene_sid().unwrap();
            for (was, erwartet) in [(pfad.clone(), sddl(&sid, false)), (pfad.parent().unwrap().to_path_buf(), sddl(&sid, true))] {
                let dacl = dacl_text(&was).unwrap();
                assert!(dacl.starts_with("D:P"), "geschützt, ohne Vererbung: {dacl}");
                assert!(dacl.contains(&sid) && dacl.contains(";SY)") && dacl.contains(";BA)"), "{dacl}");
                // Niemand sonst: keine Benutzer, keine Jeder-Gruppe, keine angemeldeten Benutzer.
                for fremd in [";BU)", ";WD)", ";AU)", ";IU)"] {
                    assert!(!dacl.contains(fremd), "{fremd} in {dacl}");
                }
                assert_eq!(dacl.matches("(A;").count(), erwartet.matches("(A;").count(), "{dacl}");
            }
        }
        let _ = std::fs::remove_dir_all(&o);
    }

    #[test]
    fn kaputte_datei_wird_nicht_ueberschrieben() {
        let o = ordner("kaputt");
        std::fs::create_dir_all(&o).unwrap();
        let pfad = o.join("jwt-schluessel");
        std::fs::write(&pfad, b"zu kurz").unwrap();
        assert!(laden_oder_anlegen(&pfad).is_err());
        assert_eq!(std::fs::read(&pfad).unwrap(), b"zu kurz");
        let _ = std::fs::remove_dir_all(&o);
    }

    /// Der Satz einer Ablehnung; ein geladener Schlüssel ist hier ein Fehler des Tests.
    #[cfg(unix)]
    fn abgelehnt(ergebnis: Result<JwtSchluessel, String>, was: &str) -> String {
        match ergebnis {
            Err(satz) => satz,
            Ok(_) => panic!("{was}: der Schlüssel wurde geladen statt abgelehnt"),
        }
    }

    /// Die Ausnahme für die Sicherung gilt nur für einen neu angelegten Ordner
    /// mit vollständigem Pfad; ein relativer Pfad, der mit `-` beginnt, nie.
    #[test]
    fn ausnahme_nur_neu_und_vollstaendig() {
        let voll = std::env::temp_dir().join("dashboard-schluessel");
        assert!(voll.is_absolute());
        assert!(ausnahme_setzen(true, &voll));
        assert!(!ausnahme_setzen(false, &voll), "nur beim Anlegen");
        for relativ in ["-dashboard-relativ", "-rf/dashboard-schluessel", "dashboard-schluessel", ""] {
            assert!(!ausnahme_setzen(true, Path::new(relativ)), "{relativ}");
            assert!(!ausnahme_setzen(false, Path::new(relativ)), "{relativ}");
        }
    }

    /// Wo der Schlüssel ohne Einstellungsdatei liegt, je Plattform: unter
    /// Windows wie bisher neben dem Datenspeicher, sonst nie im Datenbaum.
    #[test]
    fn ort_je_plattform() {
        use std::path::PathBuf;
        let daten = Path::new("/Users/beispiel/.var/dashboard/dashboard.redb");
        let ort = |p, home, xdg| ort_ohne_einstellungen(p, daten, home, xdg);
        // Windows: neben der Datei des Datenspeichers, HOME und XDG zählen nicht.
        let windows = Path::new(r"C:\Users\beispiel\.var\dashboard\dashboard.redb");
        assert_eq!(
            ort_ohne_einstellungen(Plattform::Windows, windows, Some("/Users/beispiel"), Some("/x")),
            Some(windows.parent().unwrap().join("geheim").join("jwt-schluessel"))
        );
        assert_eq!(ort_ohne_einstellungen(Plattform::Windows, windows, None, None), Some(windows.parent().unwrap().join("geheim").join("jwt-schluessel")));
        // macOS: unter Library, XDG zählt nicht.
        let mac = Some(PathBuf::from("/Users/beispiel/Library/Application Support/dashboard-schluessel/jwt-schluessel"));
        assert_eq!(ort(Plattform::MacOs, Some("/Users/beispiel"), None), mac);
        assert_eq!(ort(Plattform::MacOs, Some("/Users/beispiel/"), Some("/Users/beispiel/daten")), mac);
        // Linux: XDG_DATA_HOME, wenn es ein vollständiger Pfad ist, sonst ~/.local/share.
        let lokal = Some(PathBuf::from("/home/beispiel/.local/share/dashboard-schluessel/jwt-schluessel"));
        assert_eq!(ort(Plattform::Unix, Some("/home/beispiel"), None), lokal);
        assert_eq!(ort(Plattform::Unix, Some("/home/beispiel"), Some("")), lokal);
        assert_eq!(ort(Plattform::Unix, Some("/home/beispiel"), Some("daten/relativ")), lokal, "XDG_DATA_HOME relativ zählt nicht");
        assert_eq!(
            ort(Plattform::Unix, Some("/home/beispiel"), Some("/home/beispiel/daten/")),
            Some(PathBuf::from("/home/beispiel/daten/dashboard-schluessel/jwt-schluessel"))
        );
        // Unter Linux genügt ein vollständiges XDG_DATA_HOME, auch ohne HOME.
        for home in [None, Some(""), Some("heim")] {
            assert_eq!(
                ort(Plattform::Unix, home, Some("/daten")),
                Some(PathBuf::from("/daten/dashboard-schluessel/jwt-schluessel")),
                "Linux mit HOME {home:?}"
            );
        }
        // Sonst ohne vollständiges HOME kein Ort, kein Rückfall auf den Arbeitsordner.
        for home in [None, Some(""), Some("heim"), Some("./heim"), Some("~")] {
            assert_eq!(ort(Plattform::MacOs, home, None), None, "macOS mit HOME {home:?}");
            assert_eq!(ort(Plattform::MacOs, home, Some("/x")), None, "macOS mit HOME {home:?}, XDG zählt dort nicht");
            assert_eq!(ort(Plattform::Unix, home, None), None, "Linux mit HOME {home:?}");
            assert_eq!(ort(Plattform::Unix, home, Some("relativ")), None, "Linux mit HOME {home:?} und relativem XDG");
        }
        // Außerhalb von Windows liegt der Ort nie im Ordner des Datenspeichers.
        for o in [ort(Plattform::MacOs, Some("/Users/beispiel"), None), ort(Plattform::Unix, Some("/Users/beispiel"), None)] {
            assert!(!o.unwrap().starts_with(daten.parent().unwrap()));
        }
        assert_eq!(Plattform::diese() == Plattform::Windows, cfg!(windows));
    }

    /// Das Urteil über den Schlüsselordner: ein Ordner, keine Verknüpfung, die
    /// eigene Benutzernummer, kein Recht für Gruppe und andere.
    #[test]
    fn urteil_ueber_den_ordner() {
        let gut = Stand { richtige_art: true, uid: 501, modus: 0o40700, groesse: 0 };
        assert_eq!(ordner_urteil(gut, 501), Ok(()));
        assert_eq!(ordner_urteil(Stand { modus: 0o40500, ..gut }, 501), Ok(()), "enger ist erlaubt");
        assert_eq!(ordner_urteil(Stand { richtige_art: false, ..gut }, 501), Err(Mangel::OrdnerKeinOrdner));
        assert_eq!(ordner_urteil(gut, 502), Err(Mangel::OrdnerFremd));
        assert_eq!(ordner_urteil(Stand { uid: 0, ..gut }, 501), Err(Mangel::OrdnerFremd));
        for modus in [0o40755, 0o40750, 0o40710, 0o40701, 0o40777, 0o40704] {
            assert_eq!(ordner_urteil(Stand { modus, ..gut }, 501), Err(Mangel::OrdnerOffen), "{modus:o}");
        }
        // Die Art geht vor, dann der Besitzer, dann die Rechte.
        assert_eq!(ordner_urteil(Stand { richtige_art: false, uid: 0, modus: 0o777, groesse: 0 }, 501), Err(Mangel::OrdnerKeinOrdner));
        assert_eq!(ordner_urteil(Stand { uid: 0, modus: 0o40777, ..gut }, 501), Err(Mangel::OrdnerFremd));
    }

    /// Das Urteil über den Schlüssel: eine gewöhnliche Datei, die eigene
    /// Benutzernummer, kein Recht für Gruppe und andere, genau 32 Bytes.
    #[test]
    fn urteil_ueber_die_datei() {
        let gut = Stand { richtige_art: true, uid: 501, modus: 0o100600, groesse: 32 };
        assert_eq!(datei_urteil(gut, 501), Ok(()));
        assert_eq!(datei_urteil(Stand { modus: 0o100400, ..gut }, 501), Ok(()), "nur lesbar ist erlaubt");
        assert_eq!(datei_urteil(Stand { richtige_art: false, ..gut }, 501), Err(Mangel::DateiKeineDatei));
        assert_eq!(datei_urteil(gut, 0), Err(Mangel::DateiFremd));
        for modus in [0o100644, 0o100640, 0o100604, 0o100660, 0o100666, 0o100601] {
            assert_eq!(datei_urteil(Stand { modus, ..gut }, 501), Err(Mangel::DateiOffen), "{modus:o}");
        }
        for groesse in [0, 7, 31, 33, 64] {
            assert_eq!(datei_urteil(Stand { groesse, ..gut }, 501), Err(Mangel::DateiGroesse), "{groesse}");
        }
        // Zu offen geht vor der Größe: Ein offener Schlüssel wird als offen gemeldet.
        assert_eq!(datei_urteil(Stand { modus: 0o100644, groesse: 7, ..gut }, 501), Err(Mangel::DateiOffen));
    }

    /// Jeder Mangel hat seinen eigenen Satz; Ordner und Datei nennen, was zu tun ist.
    #[test]
    fn saetze_unterscheiden_die_faelle() {
        let s = |m| mangel_satz(m, "ORDNER", "DATEI");
        assert!(s(Mangel::OrdnerOffen).contains("ORDNER") && s(Mangel::OrdnerOffen).contains("Rechte 700"));
        assert!(s(Mangel::DateiOffen).contains("DATEI") && s(Mangel::DateiOffen).contains("Rechte 600") && s(Mangel::DateiOffen).contains("entfernen"));
        assert!(s(Mangel::DateiGroesse).contains("nicht genau 32 Bytes"));
        let alle = [
            Mangel::OrdnerKeinOrdner,
            Mangel::OrdnerFremd,
            Mangel::OrdnerOffen,
            Mangel::DateiKeineDatei,
            Mangel::DateiFremd,
            Mangel::DateiOffen,
            Mangel::DateiGroesse,
        ];
        let saetze: std::collections::BTreeSet<String> = alle.iter().map(|m| s(*m)).collect();
        assert_eq!(saetze.len(), alle.len(), "je Mangel ein eigener Satz");
        assert!(saetze.iter().all(|t| t.contains("Anmeldung mit Passkey bleibt aus")), "{saetze:?}");
    }

    /// Unter macOS und Linux: Ordner und Datei entstehen mit 0700 und 0600,
    /// auch ein fehlender Ordner darüber; beim zweiten Laden derselbe Schlüssel.
    #[cfg(unix)]
    #[test]
    fn unix_rechte_nach_dem_anlegen() {
        use std::os::unix::fs::MetadataExt;
        let o = ordner("unix-rechte");
        let pfad = o.join("ueber").join(SCHLUESSEL_ORDNER).join(SCHLUESSEL_DATEI);
        let k = laden_oder_anlegen_mit(&pfad, &|_| Ok(())).unwrap();
        // Auch der fehlende Ordner darüber (`ueber`) entsteht mit 0700.
        for (was, modus) in [(o.join("ueber").as_path(), 0o700), (pfad.parent().unwrap(), 0o700), (pfad.as_path(), 0o600)] {
            let m = std::fs::symlink_metadata(was).unwrap();
            assert_eq!(m.mode() & 0o777, modus, "{}", was.display());
            assert_eq!(m.uid(), crate::unix::eigene_uid());
        }
        assert_eq!(std::fs::metadata(&pfad).unwrap().len(), 32);
        let t = k.ausstellen(&crate::passkey::jwt::Ausweis::neu("c", 1_793_000_000));
        assert!(laden_oder_anlegen_mit(&pfad, &|_| Ok(())).unwrap().pruefen(&t, 1_793_000_001).is_ok(), "derselbe Schlüssel");
        let _ = std::fs::remove_dir_all(&o);
    }

    /// Die Ausnahme für die Sicherung wird beim Anlegen des Ordners genau
    /// einmal gerufen, mit dem Ordner, und beim zweiten Laden nicht mehr.
    /// Scheitert sie, ist der Schlüssel trotzdem da.
    #[cfg(unix)]
    #[test]
    fn unix_ausnahme_genau_einmal_und_ohne_folge() {
        use std::cell::RefCell;
        let o = ordner("unix-ausnahme");
        let pfad = o.join(SCHLUESSEL_ORDNER).join(SCHLUESSEL_DATEI);
        let gerufen: RefCell<Vec<std::path::PathBuf>> = RefCell::new(Vec::new());
        let ausnehmen = |p: &Path| {
            gerufen.borrow_mut().push(p.to_path_buf());
            Err("Exit-Code 1".to_string())
        };
        let k = laden_oder_anlegen_mit(&pfad, &ausnehmen).expect("der Schlüssel entsteht trotz gescheiterter Ausnahme");
        assert_eq!(*gerufen.borrow(), vec![pfad.parent().unwrap().to_path_buf()]);
        let t = k.ausstellen(&crate::passkey::jwt::Ausweis::neu("c", 1_793_000_000));
        assert!(laden_oder_anlegen_mit(&pfad, &ausnehmen).unwrap().pruefen(&t, 1_793_000_001).is_ok());
        assert_eq!(gerufen.borrow().len(), 1, "beim zweiten Laden nicht mehr");
        // Ein vorhandener Ordner ohne Schlüssel: die Datei entsteht, die Ausnahme wird nicht gerufen.
        std::fs::remove_file(&pfad).unwrap();
        laden_oder_anlegen_mit(&pfad, &ausnehmen).unwrap();
        assert_eq!(gerufen.borrow().len(), 1, "nur beim Anlegen des Ordners");
        let _ = std::fs::remove_dir_all(&o);
    }

    /// Ein zu offener Schlüssel oder Ordner wird abgelehnt und nicht
    /// nachgebessert; in einem zu offenen Ordner entsteht auch kein neuer Schlüssel.
    #[cfg(unix)]
    #[test]
    fn unix_zu_offen_wird_abgelehnt() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let rechte = |p: &Path, modus: u32| std::fs::set_permissions(p, std::fs::Permissions::from_mode(modus)).unwrap();
        let o = ordner("unix-offen");
        let pfad = o.join(SCHLUESSEL_ORDNER).join(SCHLUESSEL_DATEI);
        let dir = pfad.parent().unwrap();
        laden_oder_anlegen_mit(&pfad, &|_| Ok(())).unwrap();
        let inhalt = std::fs::read(&pfad).unwrap();
        // Die Datei zu offen.
        rechte(&pfad, 0o644);
        let e = abgelehnt(laden_oder_anlegen_mit(&pfad, &|_| Ok(())), "0644 wird abgelehnt");
        assert!(e.contains("Rechte 600") && e.contains("entfernen"), "{e}");
        assert_eq!(std::fs::metadata(&pfad).unwrap().mode() & 0o777, 0o644, "nicht still nachgebessert");
        assert_eq!(std::fs::read(&pfad).unwrap(), inhalt, "nicht überschrieben");
        rechte(&pfad, 0o600);
        assert!(laden_oder_anlegen_mit(&pfad, &|_| Ok(())).is_ok(), "nach 600 wieder gut");
        // Der Ordner zu offen: auch mit gutem Schlüssel abgelehnt.
        rechte(dir, 0o755);
        let e = abgelehnt(laden_oder_anlegen_mit(&pfad, &|_| Ok(())), "Ordner 0755 wird abgelehnt");
        assert!(e.contains("Rechte 700"), "{e}");
        assert_eq!(std::fs::metadata(dir).unwrap().mode() & 0o777, 0o755, "nicht still nachgebessert");
        // In einem zu offenen Ordner entsteht kein neuer Schlüssel.
        std::fs::remove_file(&pfad).unwrap();
        assert!(laden_oder_anlegen_mit(&pfad, &|_| Ok(())).is_err());
        assert!(!pfad.exists(), "kein Schlüssel in einem offenen Ordner");
        rechte(dir, 0o700);
        assert!(laden_oder_anlegen_mit(&pfad, &|_| Ok(())).is_ok(), "nach 700 entsteht er");
        let _ = std::fs::remove_dir_all(&o);
    }

    /// Eine Verknüpfung statt der Datei oder statt des Ordners wird abgelehnt,
    /// auch wenn ihr Ziel in Ordnung wäre; ebenso ein Ordner an der Stelle der Datei.
    #[cfg(unix)]
    #[test]
    fn unix_verknuepfung_wird_abgelehnt() {
        let o = ordner("unix-verknuepfung");
        let echt = o.join("echt").join(SCHLUESSEL_DATEI);
        laden_oder_anlegen_mit(&echt, &|_| Ok(())).unwrap();
        // Verknüpfung statt der Datei, in einem guten Ordner.
        let pfad = o.join(SCHLUESSEL_ORDNER).join(SCHLUESSEL_DATEI);
        laden_oder_anlegen_mit(&pfad, &|_| Ok(())).unwrap();
        std::fs::remove_file(&pfad).unwrap();
        std::os::unix::fs::symlink(&echt, &pfad).unwrap();
        let e = abgelehnt(laden_oder_anlegen_mit(&pfad, &|_| Ok(())), "Verknüpfung statt der Datei");
        assert!(e.contains("keine gewöhnliche Datei"), "{e}");
        // Verknüpfung statt des Ordners.
        let ueber = o.join("verknuepft");
        std::os::unix::fs::symlink(echt.parent().unwrap(), &ueber).unwrap();
        let e = abgelehnt(laden_oder_anlegen_mit(&ueber.join(SCHLUESSEL_DATEI), &|_| Ok(())), "Verknüpfung statt des Ordners");
        assert!(e.contains("kein Ordner oder eine Verknüpfung"), "{e}");
        // Eine Verknüpfung ohne Ziel statt der Datei: abgelehnt, und am Ziel entsteht nichts.
        std::fs::remove_file(&pfad).unwrap();
        let ohne_ziel = o.join("gibt-es-nicht");
        std::os::unix::fs::symlink(&ohne_ziel, &pfad).unwrap();
        let e = abgelehnt(laden_oder_anlegen_mit(&pfad, &|_| Ok(())), "Verknüpfung ohne Ziel statt der Datei");
        assert!(e.contains("keine gewöhnliche Datei"), "{e}");
        assert!(!ohne_ziel.exists(), "über die Verknüpfung wird nichts angelegt");
        // Eine Verknüpfung ohne Ziel statt des Ordners: abgelehnt, und es entsteht kein Ordner.
        let haengt = o.join("haengt");
        std::os::unix::fs::symlink(o.join("auch-nicht"), &haengt).unwrap();
        let e = abgelehnt(laden_oder_anlegen_mit(&haengt.join(SCHLUESSEL_DATEI), &|_| Ok(())), "Verknüpfung ohne Ziel statt des Ordners");
        assert!(e.contains("kein Ordner oder eine Verknüpfung"), "{e}");
        assert!(!o.join("auch-nicht").exists());
        // Ein Ordner an der Stelle der Datei.
        std::fs::remove_file(&pfad).unwrap();
        std::fs::create_dir(&pfad).unwrap();
        let e = abgelehnt(laden_oder_anlegen_mit(&pfad, &|_| Ok(())), "Ordner statt der Datei");
        assert!(e.contains("keine gewöhnliche Datei"), "{e}");
        let _ = std::fs::remove_dir_all(&o);
    }

    /// Anlegen ohne Halbstand: Unter dem Namen steht sofort der ganze
    /// Schlüssel, im Ordner bleibt keine Zwischendatei, und ein vorhandener
    /// Name wird nie überschrieben.
    #[cfg(unix)]
    #[test]
    fn unix_anlegen_ohne_halbstand() {
        let o = ordner("unix-halbstand");
        let pfad = o.join(SCHLUESSEL_ORDNER).join(SCHLUESSEL_DATEI);
        let dir = pfad.parent().unwrap().to_path_buf();
        laden_oder_anlegen_mit(&pfad, &|_| Ok(())).unwrap();
        let namen = |d: &Path| -> Vec<String> { std::fs::read_dir(d).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect() };
        assert_eq!(namen(&dir), [SCHLUESSEL_DATEI], "keine Zwischendatei bleibt liegen");
        assert_eq!(std::fs::read(&pfad).unwrap().len(), 32);
        // Gibt es den Namen schon, bleibt die vorhandene Datei, wie sie ist, und die Zwischendatei verschwindet.
        let vorher = std::fs::read(&pfad).unwrap();
        let e = ohne_halbstand_anlegen(&dir, &pfad, &[9u8; 32]).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&pfad).unwrap(), vorher);
        assert_eq!(namen(&dir), [SCHLUESSEL_DATEI]);
        // Scheitert das Anlegen (kein Ordner), bleibt nichts zurück.
        let fehlt = o.join("fehlt");
        assert!(ohne_halbstand_anlegen(&fehlt, &fehlt.join(SCHLUESSEL_DATEI), &[9u8; 32]).is_err());
        assert!(!fehlt.exists());
        let _ = std::fs::remove_dir_all(&o);
    }

    /// Viele Läufe zugleich: Alle bekommen denselben Schlüssel, keiner liest
    /// eine leere oder halbe Datei.
    #[cfg(unix)]
    #[test]
    fn unix_gleichzeitig_derselbe_schluessel() {
        let o = ordner("unix-gleichzeitig");
        let pfad = o.join(SCHLUESSEL_ORDNER).join(SCHLUESSEL_DATEI);
        // Der Ordner steht schon, damit alle Läufe am Anlegen der Datei zusammentreffen.
        laden_oder_anlegen_mit(&pfad, &|_| Ok(())).unwrap();
        std::fs::remove_file(&pfad).unwrap();
        let ausweis = crate::passkey::jwt::Ausweis::neu("c", 1_793_000_000);
        // Alle Läufe warten an der Schranke und starten zusammen, damit sie sich auch dort treffen, wo `fsync` fast nichts kostet.
        let schranke = std::sync::Arc::new(std::sync::Barrier::new(16));
        let laeufe: Vec<_> = (0..16)
            .map(|_| {
                let pfad = pfad.clone();
                let schranke = schranke.clone();
                std::thread::spawn(move || {
                    schranke.wait();
                    laden_oder_anlegen_mit(&pfad, &|_| Ok(())).map_err(|e| e.to_string())
                })
            })
            .collect();
        let schluessel: Vec<JwtSchluessel> = laeufe.into_iter().map(|l| l.join().unwrap().expect("jeder Lauf bekommt einen Schlüssel")).collect();
        let t = schluessel[0].ausstellen(&ausweis);
        assert!(schluessel.iter().all(|k| k.pruefen(&t, 1_793_000_001).is_ok()), "alle haben denselben Schlüssel");
        assert_eq!(std::fs::read_dir(pfad.parent().unwrap()).unwrap().count(), 1, "keine Zwischendatei bleibt liegen");
        let _ = std::fs::remove_dir_all(&o);
    }

    /// Die Ausnahme für die Sicherung kommt erst nach dem Urteil über den
    /// Ordner und nur für einen vollständigen Pfad.
    #[cfg(unix)]
    #[test]
    fn unix_ausnahme_nur_fuer_vollstaendige_pfade() {
        use std::cell::Cell;
        let gerufen = Cell::new(0);
        let zaehlen = |_: &Path| {
            gerufen.set(gerufen.get() + 1);
            Ok(())
        };
        // Ein vollständiger Pfad: genau einmal. Den relativen Fall prüft `ausnahme_nur_neu_und_vollstaendig` ohne Platte.
        let o = ordner("unix-ausnahme-pfad");
        laden_oder_anlegen_mit(&o.join(SCHLUESSEL_ORDNER).join(SCHLUESSEL_DATEI), &zaehlen).unwrap();
        assert_eq!(gerufen.get(), 1);
        let _ = std::fs::remove_dir_all(&o);
    }

    /// Eine Datei mit falscher Größe und engen Rechten wird an der Größe
    /// abgelehnt und nicht überschrieben (auch mit mehr als 32 Bytes).
    #[cfg(unix)]
    #[test]
    fn unix_falsche_groesse_wird_nicht_ueberschrieben() {
        use std::os::unix::fs::OpenOptionsExt;
        let o = ordner("unix-groesse");
        let pfad = o.join(SCHLUESSEL_ORDNER).join(SCHLUESSEL_DATEI);
        laden_oder_anlegen_mit(&pfad, &|_| Ok(())).unwrap();
        for inhalt in [&b"zu kurz"[..], &[7u8; 33][..]] {
            std::fs::remove_file(&pfad).unwrap();
            let mut d = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&pfad).unwrap();
            d.write_all(inhalt).unwrap();
            drop(d);
            let e = abgelehnt(laden_oder_anlegen_mit(&pfad, &|_| Ok(())), "falsche Größe");
            assert!(e.contains("nicht genau 32 Bytes"), "{e}");
            assert_eq!(std::fs::read(&pfad).unwrap(), inhalt);
        }
        let _ = std::fs::remove_dir_all(&o);
    }
}
