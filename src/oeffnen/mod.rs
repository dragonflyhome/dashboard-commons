//! Lokale Dateien und Ordner öffnen (Entscheidung lokal-oeffnen).
//!
//! Die Oberfläche schickt nur `projekt` und `kennung` (`POST /api/oeffnen`,
//! `api::oeffnen`); der Server schlägt den Pfad in `daten/orte.toml` nach,
//! prüft ihn mit [`vorpruefen`] (nur der Text, das Dienstkonto darf das
//! Profil des Inhabers nicht lesen) und gibt ihn dem Boten
//! (`Anfrage::Oeffnen`). Der Bote prüft alles mit [`pruefen`], mit seinen
//! eigenen Wurzeln, seiner eigenen Liste der Tabu-Ordner und Endungen und
//! einer Positivliste der zugeordneten Programme (`bote::oeffnen`), und
//! öffnet erst dann. `pruefe` meldet Verstöße schon beim Bau
//! (`inhalt::pruefung`), wie der Server nur am Text ([`vorpruefen`] mit dem
//! Profil des Inhabers, [`profil_fuer_pruefung`]); ob es den Ort gibt, sieht
//! erst der Bote.
//!
//! [`vorpruefen`], ohne Blick auf die Platte:
//!
//! 1. Form ([`form_pruefen`]): absolut mit Laufwerk, kein UNC, kein
//!    Gerätepfad (`\\?\`, `\\.\`), kein Doppelpunkt nach dem Laufwerk
//!    (Datenströme), kein `.`/`..`, keine Kurznamen (`~1`), keine Gerätenamen
//!    (`NUL`, `CON` …), kein `%`, kein Teil mit `.{GUID}` am Ende (Shell-Ordner),
//!    kein Teil mit Punkt oder Leerzeichen am Ende, keine Sonderzeichen, kein `.git`.
//! 2. Tabu: die eingebauten Namen ([`EINGEBAUTE_TABU`], als Prüfsummen) und
//!    die Liste aus dem Inhalt (Groß/Klein, Leer-, Binde- und Unterstriche
//!    zählen nicht). Fehlt die Liste: nichts (geschlossen scheitern).
//! 3. Lage als Text: unter einer erlaubten Wurzel (Profil des Inhabers,
//!    Inhaltsquellen), nicht unter `<Profil>\AppData`, ProgramData oder einem
//!    Ordner des Dienstes. Verglichen wird kanonisch, wo das geht, sonst als Text.
//!
//! [`pruefen`] dazu: kein Netzlaufwerk; kanonisch auflösen (Verknüpfungen,
//! Junctions, Kurznamen), dann 1 bis 3 noch einmal mit dem Ergebnis; Ordner
//! im Explorer, Dateien nur mit einer Endung der Positivliste (zip als Ordner
//! im Explorer) und nur mit einem einzigen Namen (keine harte Verknüpfung).
//!
//! Kein Satz nennt den Pfad oder den Namen eines Tabu-Ordners. „Gibt es
//! nicht“, „liegt außerhalb“ und „Tabu“ haben denselben Satz, damit die
//! Antwort nicht verrät, ob es dort etwas gibt.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

use crate::pc::inventory::Ausschluss;

/// Endungen, die der Server als Datei öffnen lässt (Positivliste der Entscheidung).
pub const ENDUNGEN: [&str; 16] = ["pdf", "docx", "xlsx", "pptx", "odt", "ods", "odp", "txt", "csv", "md", "png", "jpg", "jpeg", "gif", "webp", "svg"];

/// Endungen, die nur als Ordner im Explorer aufgehen.
pub const EXPLORER_ENDUNGEN: [&str; 1] = ["zip"];

/// Ausführbares und Aktives aus der Entscheidung; die Positivliste schließt es
/// schon aus, die Tests nennen es trotzdem einzeln.
pub const VERBOTENE_ENDUNGEN: [&str; 28] = [
    "exe",
    "com",
    "bat",
    "cmd",
    "ps1",
    "psm1",
    "vbs",
    "js",
    "jse",
    "wsf",
    "hta",
    "msi",
    "msp",
    "scr",
    "cpl",
    "reg",
    "lnk",
    "url",
    "appref-ms",
    "library-ms",
    "search-ms",
    "settingcontent-ms",
    "chm",
    "jar",
    "docm",
    "xlsm",
    "pptm",
    "dotm",
];

/// Die Tabu-Ordner der Entscheidung, fest eingebaut zusätzlich zur Liste im
/// Inhalt (die jeder Chat ändern kann). Das Code-Repo nennt die Namen nie
/// (Prüfer vor dem Push); hier stehen nur Länge und SHA-256 der dichten
/// Form (klein, nur Buchstaben und Ziffern). Ein Teil des Pfads ist Tabu,
/// wenn ein Stück seiner dichten Form diese Prüfsumme hat.
pub const EINGEBAUTE_TABU: [(usize, &str); 2] =
    [(12, "3b0d3b3a84f893b61db5dad16b31829bc22644c12f09dd586f8d968c76d1b958"), (22, "dce6fffbef6f34865ef0407a59f36bf54841f289fdcf85c6bb4524c3da17a4a6")];

/// Höchstens so viele Öffnungen je Minute (Server und Bote je für sich).
pub const JE_MINUTE: usize = 10;

/// Höchstlänge eines Pfads in Zeichen.
const PFAD_HOECHSTENS: usize = 1000;

/// Was geöffnet wird.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Art {
    /// Mit dem zugeordneten Programm (ShellExecute, Verb open, ohne Argumente).
    Datei,
    /// Im Explorer (explorer.exe mit dem Pfad als einzigem Argument); auch zip.
    Ordner,
}

/// Ein geprüfter Ort: der kanonische Pfad (ohne `\\?\`) und wie er aufgeht.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ziel {
    pub pfad: PathBuf,
    pub art: Art,
    /// Bei der Prüfung ein echter Ordner (nicht zip). Danach wird nie wieder
    /// nachgesehen: Der Bote gibt ihn dem Explorer immer mit `\` am Ende.
    pub ist_ordner: bool,
}

/// Warum ein Ort nicht aufgeht. [`Abweisung::satz`] nennt nie den Pfad.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Abweisung {
    Leer,
    NichtAbsolut,
    Netz,
    Geraet,
    Datenstrom,
    Elternverweis,
    Zeichen,
    Kurzname,
    Shellordner,
    Git,
    Tabu,
    KeineListe,
    AppData,
    ProgramData,
    Dienst,
    AusserhalbWurzel,
    Fehlt,
    Unlesbar,
    OhneEndung,
    Endung(String),
    Komma,
    MehrereNamen,
    /// Nur im Boten: der Endung ist ein Programm zugeordnet, das nicht auf der
    /// Positivliste steht (oder keins, das sich ermitteln lässt).
    Programm(String),
    /// Nur im Boten: die Anfrage kam nicht vom Dienst.
    NichtVomDienst,
    /// Nur im Boten: mehr als [`JE_MINUTE`] in einer Minute.
    ZuViele,
}

/// Derselbe Satz für „gibt es nicht“, „liegt außerhalb“ und „Tabu“.
pub const SATZ_NICHT_DA: &str = "Diesen Ort öffnet das Dashboard nicht: Es gibt ihn nicht, oder er liegt nicht in deinem Profil oder einer Inhaltsquelle.";

/// Eine Endung zum Zeigen: nur kurz und aus Buchstaben und Ziffern, sonst „dieser Endung“.
fn endung_wort(e: &str) -> String {
    if !e.is_empty() && e.len() <= 8 && e.chars().all(|c| c.is_ascii_alphanumeric()) { format!("der Endung .{e}") } else { "dieser Endung".to_string() }
}

impl Abweisung {
    /// Ein Satz in einfachem Deutsch, ohne Pfad.
    pub fn satz(&self) -> String {
        match self {
            Self::Leer => "Bei diesem Ort steht kein Pfad.".into(),
            Self::NichtAbsolut => "Der Pfad muss vollständig mit Laufwerk sein, etwa C:\\…".into(),
            Self::Netz => "Netzlaufwerke und Netzwerkpfade öffnet das Dashboard nicht.".into(),
            Self::Geraet => "Gerätepfade öffnet das Dashboard nicht.".into(),
            Self::Datenstrom => "Ein Doppelpunkt nach dem Laufwerk (Datenstrom) ist nicht erlaubt.".into(),
            Self::Elternverweis => "Der Pfad darf kein „..“ oder „.“ enthalten.".into(),
            Self::Zeichen => "Der Pfad enthält Zeichen, die Windows anders liest, als sie dastehen (etwa % oder einen Punkt am Ende).".into(),
            Self::Kurzname => "Kurznamen mit ~ gehen nicht; bitte den vollen Namen eintragen.".into(),
            Self::Shellordner => "Shell-Ordner (Name mit .{…} am Ende) öffnet das Dashboard nicht.".into(),
            Self::Git => "Ordner .git öffnet das Dashboard nicht.".into(),
            Self::Tabu | Self::AusserhalbWurzel | Self::Fehlt => SATZ_NICHT_DA.into(),
            Self::KeineListe => "Öffnen geht gerade nicht: Die Liste der Tabu-Ordner fehlt oder ist kaputt.".into(),
            Self::AppData => "Orte unter AppData öffnet das Dashboard nicht.".into(),
            Self::ProgramData => "Orte unter ProgramData öffnet das Dashboard nicht.".into(),
            Self::Dienst => "Orte im Ordner des Dienstes öffnet das Dashboard nicht.".into(),
            Self::Unlesbar => "Dieser Ort lässt sich gerade nicht prüfen; darum bleibt er zu.".into(),
            Self::OhneEndung => "Dateien ohne Endung öffnet das Dashboard nicht.".into(),
            Self::Endung(e) => format!("Dateien mit {} öffnet das Dashboard nicht.", endung_wort(e)),
            Self::Komma => "Ordner mit Komma im Pfad kann der Explorer nicht sicher öffnen.".into(),
            Self::MehrereNamen => "Diese Datei hat mehrere Namen (harte Verknüpfung); das Dashboard öffnet sie nicht.".into(),
            Self::Programm(e) => {
                format!("Eine Datei mit {} ist mit einem Programm verknüpft, das das Dashboard nicht öffnet.", endung_wort(e))
            }
            Self::NichtVomDienst => "Öffnen nimmt der Bote nur vom Dienst an.".into(),
            Self::ZuViele => "Höchstens 10 Öffnungen je Minute. Bitte kurz warten.".into(),
        }
    }
}

/// Wogegen geprüft wird. Server und Bote bauen ihre Regeln je selbst.
#[derive(Debug, Clone)]
pub struct Regeln {
    /// Das Profil des Inhabers (erlaubte Wurzel; darunter `AppData` verboten).
    /// `None`: unbekannt, dann zählen nur die Inhaltsquellen.
    pub profil: Option<PathBuf>,
    /// Die Inhaltsquellen (erlaubte Wurzeln).
    pub quellen: Vec<PathBuf>,
    /// Ordner des Dienstes (Programm, Datenspeicher, Sicherung, Protokoll,
    /// Oberfläche). ProgramData kommt immer dazu.
    pub verboten: Vec<PathBuf>,
    /// Die Liste der Tabu-Ordner aus dem Inhalt; `None`: geladen ging nicht.
    pub tabu: Option<Ausschluss>,
    /// Ohne Liste abweisen (Server, Bote) oder Tabu nur prüfen, wenn sie da ist (`pruefe`).
    pub tabu_pflicht: bool,
    /// Die eingebauten Tabu-Namen als (Länge, SHA-256 der dichten Form);
    /// Standard [`EINGEBAUTE_TABU`], Tests setzen erfundene ein.
    pub eingebaut: Vec<(usize, String)>,
    /// Positivliste der Dateiendungen.
    pub endungen: Vec<String>,
    /// Endungen, die als Ordner im Explorer aufgehen (zip).
    pub explorer_endungen: Vec<String>,
}

impl Default for Regeln {
    fn default() -> Self {
        Regeln {
            profil: None,
            quellen: Vec::new(),
            verboten: Vec::new(),
            tabu: None,
            tabu_pflicht: true,
            eingebaut: EINGEBAUTE_TABU.iter().map(|(n, h)| (*n, h.to_string())).collect(),
            endungen: ENDUNGEN.iter().map(|e| e.to_string()).collect(),
            explorer_endungen: EXPLORER_ENDUNGEN.iter().map(|e| e.to_string()).collect(),
        }
    }
}

impl Regeln {
    /// Mit den Listen des Servers.
    pub fn des_servers(profil: Option<PathBuf>, quellen: Vec<PathBuf>, verboten: Vec<PathBuf>, tabu: Option<Ausschluss>) -> Regeln {
        Regeln { profil: profil.filter(|p| p.is_absolute()), quellen, verboten, tabu, ..Regeln::default() }
    }
}

/// Länge und SHA-256 (klein, hex) der dichten Form eines Namens: für Tests mit erfundenen Namen.
pub fn tabu_pruefsumme(name: &str) -> (usize, String) {
    let d = dicht(name);
    (d.chars().count(), hex(&Sha256::digest(d.as_bytes())))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Klein, nur Buchstaben und Ziffern.
fn dicht(s: &str) -> String {
    s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Steckt einer der eingebauten Namen in diesem Teil des Pfads?
fn eingebaut_tabu(teil: &str, eingebaut: &[(usize, String)]) -> bool {
    let zeichen: Vec<char> = dicht(teil).chars().collect();
    eingebaut
        .iter()
        .any(|(n, summe)| *n > 0 && zeichen.len() >= *n && zeichen.windows(*n).any(|w| hex(&Sha256::digest(w.iter().collect::<String>().as_bytes())) == *summe))
}

/// Die Teile nach `X:\`, ohne leere am Ende.
fn teile(pfad: &str) -> Vec<&str> {
    let rest = pfad.get(3..).unwrap_or("");
    let mut t: Vec<&str> = rest.split(['\\', '/']).collect();
    while t.last().is_some_and(|s| s.is_empty()) {
        t.pop();
    }
    t
}

/// `CON`, `NUL`, `COM1` … auch mit Endung (`nul.txt`).
fn ist_geraetename(teil: &str) -> bool {
    let stamm = teil.split('.').next().unwrap_or("").trim_end().to_uppercase();
    let nummer = |rest: &str| rest.chars().count() == 1 && rest.chars().all(|c| c.is_ascii_digit() || "\u{b9}\u{b2}\u{b3}".contains(c));
    matches!(stamm.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$")
        || stamm.strip_prefix("COM").is_some_and(nummer)
        || stamm.strip_prefix("LPT").is_some_and(nummer)
}

/// Ein Kurzname wie `PROGRA~1`: `~` mit einer Ziffer danach.
fn ist_kurzname(teil: &str) -> bool {
    teil.as_bytes().windows(2).any(|w| w[0] == b'~' && w[1].is_ascii_digit())
}

/// Ein Shell-Ordner wie `Ablage.{20D04FE0-3AEA-1069-A2D8-08002B30309D}`.
fn ist_shellordner(teil: &str) -> bool {
    let Some(rest) = teil.strip_suffix('}') else { return false };
    let Some((_, guid)) = rest.rsplit_once(".{") else { return false };
    guid.len() == 36 && guid.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

/// Schritt 1: nur die Form, ohne Blick auf die Platte.
pub fn form_pruefen(pfad: &str) -> Result<(), Abweisung> {
    if pfad.trim().is_empty() {
        return Err(Abweisung::Leer);
    }
    if pfad.chars().count() > PFAD_HOECHSTENS || pfad.chars().any(char::is_control) || pfad.contains('%') {
        return Err(Abweisung::Zeichen);
    }
    let b = pfad.as_bytes();
    let trenner = |c: u8| c == b'\\' || c == b'/';
    if b.len() >= 2 && trenner(b[0]) && trenner(b[1]) {
        // Zwei Trennzeichen vorn: Gerätepfad mit ? oder . danach, sonst UNC.
        return Err(if matches!(b.get(2), Some(b'?' | b'.')) { Abweisung::Geraet } else { Abweisung::Netz });
    }
    if !(b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && trenner(b[2])) {
        return Err(Abweisung::NichtAbsolut);
    }
    if pfad[2..].contains(':') {
        return Err(Abweisung::Datenstrom);
    }
    for t in teile(pfad) {
        if t == "." || t == ".." {
            return Err(Abweisung::Elternverweis);
        }
        if t.is_empty() || t.ends_with('.') || t.ends_with(' ') || t.starts_with(' ') || t.contains(['<', '>', '"', '|', '?', '*']) {
            return Err(Abweisung::Zeichen);
        }
        if ist_geraetename(t) {
            return Err(Abweisung::Geraet);
        }
        if ist_kurzname(t) {
            return Err(Abweisung::Kurzname);
        }
        if ist_shellordner(t) {
            return Err(Abweisung::Shellordner);
        }
        if t.eq_ignore_ascii_case(".git") {
            return Err(Abweisung::Git);
        }
    }
    Ok(())
}

/// Schritt 2: Tabu-Namen in den Teilen (auch der Dateiname selbst): die
/// eingebauten immer, die Liste aus dem Inhalt dazu.
fn tabu_pruefen(pfad: &str, regeln: &Regeln) -> Result<(), Abweisung> {
    let alle = teile(pfad);
    if alle.iter().any(|t| eingebaut_tabu(t, &regeln.eingebaut)) {
        return Err(Abweisung::Tabu);
    }
    match &regeln.tabu {
        Some(t) if alle.iter().any(|teil| t.verboten_lose(teil)) => Err(Abweisung::Tabu),
        Some(_) => Ok(()),
        None if regeln.tabu_pflicht => Err(Abweisung::KeineListe),
        None => Ok(()),
    }
}

/// Kanonischer Pfad als Text, ohne `\\?\`; ein `\\?\UNC\` bleibt als `\\…` (Netz).
pub fn kanonisch_text(p: &Path) -> String {
    let text = p.display().to_string().replace('/', "\\");
    if let Some(r) = text.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{r}");
    }
    text.strip_prefix(r"\\?\").map(str::to_string).unwrap_or(text)
}

/// Zum Vergleichen: kanonisch und klein, wenn das geht, dazu immer der Text
/// selbst (klein, mit `\`, ohne `\` am Ende). Ein Ordner, den dieses Konto
/// nicht auflösen kann (AppData, Ordner des Dienstes), fällt so nie heraus.
fn vergleichsformen(p: &Path) -> Vec<String> {
    let text = kanonisch_text(p).to_lowercase().trim_end_matches('\\').to_string();
    let mut v = vec![text];
    if let Some(k) = std::fs::canonicalize(p).ok().map(|k| kanonisch_text(&k).to_lowercase().trim_end_matches('\\').to_string())
        && !v.contains(&k)
    {
        v.push(k);
    }
    v
}

/// Liegt `pfad` (klein, mit `\`) in oder unter `ordner` (klein, ohne `\` am Ende)?
fn unter(pfad: &str, ordner: &str) -> bool {
    let o = ordner.trim_end_matches('\\');
    pfad == o || pfad.starts_with(&format!("{o}\\"))
}

/// ProgramData dieses PCs (Umgebung, sonst `C:\ProgramData`).
pub fn programdata() -> PathBuf {
    std::env::var_os("ProgramData").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
}

/// Schritt 3: die Lage. `klein`: der Pfad klein, mit `\`, ohne `\` am Ende.
fn lage_pruefen(klein: &str, regeln: &Regeln) -> Result<(), Abweisung> {
    let wurzeln: Vec<String> = regeln.profil.iter().chain(regeln.quellen.iter()).flat_map(|w| vergleichsformen(w)).collect();
    if !wurzeln.iter().any(|w| unter(klein, w)) {
        return Err(Abweisung::AusserhalbWurzel);
    }
    if regeln.profil.iter().flat_map(|p| vergleichsformen(&p.join("AppData"))).any(|a| unter(klein, &a)) {
        return Err(Abweisung::AppData);
    }
    // ProgramData nur als Text (nie aufgelöst oder berührt).
    if unter(klein, &kanonisch_text(&programdata()).to_lowercase()) {
        return Err(Abweisung::ProgramData);
    }
    if regeln.verboten.iter().flat_map(|v| vergleichsformen(v)).any(|v| unter(klein, &v)) {
        return Err(Abweisung::Dienst);
    }
    Ok(())
}

fn klein(pfad: &str) -> String {
    pfad.replace('/', "\\").to_lowercase().trim_end_matches('\\').to_string()
}

/// Ohne Blick auf den Ort selbst: Form, Tabu, Lage (Schritte 1 bis 3). So
/// prüft der Server; das Dienstkonto darf das Profil des Inhabers nicht
/// lesen, das Auflösen macht der Bote.
pub fn vorpruefen(pfad: &str, regeln: &Regeln) -> Result<(), Abweisung> {
    form_pruefen(pfad)?;
    tabu_pruefen(pfad, regeln)?;
    lage_pruefen(&klein(pfad), regeln)
}

/// Schritt 3 der Laufwerke: ein Netzlaufwerk (verbundener Buchstabe) ist nie erlaubt.
#[cfg(windows)]
fn laufwerk_pruefen(pfad: &str) -> Result<(), Abweisung> {
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
    const DRIVE_NO_ROOT_DIR: u32 = 1;
    const DRIVE_REMOTE: u32 = 4;
    let wurzel: Vec<u16> = format!("{}\\", &pfad[..2]).encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: nullterminierter Text, der Aufruf liest nur.
    match unsafe { GetDriveTypeW(wurzel.as_ptr()) } {
        DRIVE_REMOTE => Err(Abweisung::Netz),
        DRIVE_NO_ROOT_DIR => Err(Abweisung::Fehlt),
        _ => Ok(()),
    }
}

#[cfg(not(windows))]
fn laufwerk_pruefen(_pfad: &str) -> Result<(), Abweisung> {
    Ok(())
}

/// Wie viele Namen (harte Verknüpfungen) hat die Datei? Öffnet sie nur für die
/// Eckdaten (FILE_READ_ATTRIBUTES), nie zum Lesen.
#[cfg(windows)]
fn namen_zahl(p: &Path) -> Option<u32> {
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle};
    const FILE_READ_ATTRIBUTES: u32 = 0x80;
    const ALLE_TEILEN: u32 = 0x1 | 0x2 | 0x4;
    let f = std::fs::OpenOptions::new().access_mode(FILE_READ_ATTRIBUTES).share_mode(ALLE_TEILEN).open(p).ok()?;
    // SAFETY: gültiges Handle der offenen Datei, Ausgabe in eine eigene Struktur.
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetFileInformationByHandle(f.as_raw_handle() as _, &mut info) };
    (ok != 0).then_some(info.nNumberOfLinks)
}

#[cfg(not(windows))]
fn namen_zahl(p: &Path) -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(p).ok().map(|m| m.nlink() as u32)
}

/// Die ganze Prüfung (so prüft der Bote): erst
/// [`vorpruefen`] (Tabu und Lage vor jedem Blick auf die Platte), dann
/// auflösen und alles noch einmal mit dem Ergebnis.
pub fn pruefen(pfad: &str, regeln: &Regeln) -> Result<Ziel, Abweisung> {
    vorpruefen(pfad, regeln)?;
    laufwerk_pruefen(pfad)?;
    let kanonisch = match std::fs::canonicalize(pfad) {
        Ok(k) => k,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(Abweisung::Fehlt),
        Err(_) => return Err(Abweisung::Unlesbar),
    };
    let text = kanonisch_text(&kanonisch);
    form_pruefen(&text)?;
    tabu_pruefen(&text, regeln)?;
    laufwerk_pruefen(&text)?;
    lage_pruefen(&klein(&text), regeln)?;
    let meta = std::fs::metadata(&kanonisch).map_err(|_| Abweisung::Unlesbar)?;
    let explorer = |ist_ordner: bool| {
        if text.contains(',') { Err(Abweisung::Komma) } else { Ok(Ziel { pfad: PathBuf::from(&text), art: Art::Ordner, ist_ordner }) }
    };
    if meta.is_dir() {
        return explorer(true);
    }
    if !meta.is_file() {
        return Err(Abweisung::Unlesbar);
    }
    let name = kanonisch.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let Some((_, endung)) = name.rsplit_once('.').filter(|(stamm, e)| !stamm.is_empty() && !e.is_empty()) else {
        return Err(Abweisung::OhneEndung);
    };
    let endung = endung.to_lowercase();
    let als_ordner = regeln.explorer_endungen.contains(&endung);
    if !als_ordner && !regeln.endungen.contains(&endung) {
        return Err(Abweisung::Endung(endung));
    }
    match namen_zahl(&kanonisch) {
        Some(1) => {}
        Some(_) => return Err(Abweisung::MehrereNamen),
        None => return Err(Abweisung::Unlesbar),
    }
    if als_ordner { explorer(false) } else { Ok(Ziel { pfad: PathBuf::from(&text), art: Art::Datei, ist_ordner: false }) }
}

/// Höchstens [`JE_MINUTE`] Öffnungen in 60 Sekunden (gleitend).
#[derive(Debug, Default)]
pub struct Grenze(Mutex<VecDeque<Instant>>);

impl Grenze {
    /// Zählt eine Öffnung; `false`, wenn in der letzten Minute schon genug waren.
    pub fn nehmen(&self, jetzt: Instant) -> bool {
        let mut z = self.0.lock().unwrap_or_else(|e| e.into_inner());
        while z.front().is_some_and(|t| jetzt.saturating_duration_since(*t) >= Duration::from_secs(60)) {
            z.pop_front();
        }
        if z.len() >= JE_MINUTE {
            return false;
        }
        z.push_back(jetzt);
        true
    }

    /// Tests: wieder bei null anfangen.
    #[cfg(test)]
    pub fn leeren(&self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }
}

/// Ersatz für den Weg zum Boten in Tests: bekommt die Anfrage, gibt die Antwort.
#[cfg(test)]
pub type Ersatz =
    std::sync::Arc<dyn Fn(crate::vertrag::bote::Anfrage) -> futures_util::future::BoxFuture<'static, crate::vertrag::bote::Antwort> + Send + Sync>;

/// Was der Dienst fürs Öffnen behält (in `AppState`).
#[derive(Default)]
pub struct Stand {
    pub grenze: Grenze,
    /// Tests: statt des Boten.
    #[cfg(test)]
    pub ersatz: Mutex<Option<Ersatz>>,
    /// Tests: feste Regeln statt der aus Profil, Quellen und Einstellungen.
    #[cfg(test)]
    pub regeln: Mutex<Option<Regeln>>,
}

impl std::fmt::Debug for Stand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("oeffnen::Stand")
    }
}

/// Das Profil dieses Prozesses aus `USERPROFILE`, nur wenn es ein
/// vollständiger Pfad ist (nie `.`).
pub fn eigenes_profil() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(PathBuf::from).filter(|p| p.is_absolute())
}

/// Das Profil des Inhabers für die Prüfung beim Bau, wenn der Dienst es
/// gesetzt hat ([`profil_fuer_pruefung_setzen`]).
static PRUEF_PROFIL: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();

/// Der Dienst setzt beim Start das Profil des Inhabers (über die SID, sonst
/// `None`) für die Prüfung der Orte; nur das erste Setzen zählt.
pub fn profil_fuer_pruefung_setzen(profil: Option<PathBuf>) {
    let _ = PRUEF_PROFIL.set(profil.filter(|p| p.is_absolute()));
}

/// Wogegen die Prüfung beim Bau die Lage eines Orts misst: im Dienst das
/// gesetzte Profil des Inhabers (nie das des Dienstkontos), sonst (Konsole,
/// `pruefe`, Bote: sie laufen als Inhaber) das eigene.
pub fn profil_fuer_pruefung() -> Option<PathBuf> {
    match PRUEF_PROFIL.get() {
        Some(p) => p.clone(),
        None => eigenes_profil(),
    }
}

/// Die Ordner des Dienstes für die Prüfung beim Bau, wenn der Dienst sie
/// gesetzt hat ([`dienst_einrichten`]).
static PRUEF_VERBOTEN: std::sync::OnceLock<Vec<PathBuf>> = std::sync::OnceLock::new();

/// Die Ordner des Dienstes, gegen die die Prüfung beim Bau misst: im Dienst
/// die aus den Einstellungen (Datenspeicher, Sicherung, Protokoll,
/// Oberfläche, Bote, Einstellungen; wie Server und Bote), sonst nur das
/// Programm selbst ([`ordner_des_dienstes`] ohne Einstellungen).
pub fn ordner_fuer_pruefung() -> Vec<PathBuf> {
    match PRUEF_VERBOTEN.get() {
        Some(v) => v.clone(),
        None => ordner_des_dienstes(None),
    }
}

/// Beim Start des Dienstes (und mit `--konsole --einstellungen`): Profil
/// und Ordner des Dienstes für die Prüfung beim Bau setzen und dem
/// Verbergen (`datenschutz`) Profil und Namen des Inhabers geben, beides
/// aus der SID, nie das Dienstkonto. Gibt das Profil zurück (`None`: SID
/// leer oder ohne Profil; das meldet `meldungen::inhaber_lauf` als Störung).
pub fn dienst_einrichten(e: &crate::dienst::einstellungen::Einstellungen) -> Option<PathBuf> {
    let profil = inhaber_profil(Some(e));
    profil_fuer_pruefung_setzen(profil.clone());
    let _ = PRUEF_VERBOTEN.set(ordner_des_dienstes(Some(e)));
    let name = crate::datenschutz::name_aus_sid(&e.konten.inhaber_sid);
    // Ein zweites, abweichendes Setzen meldet `inhaber_setzen` selbst als Fehler im Protokoll.
    let _ = crate::datenschutz::inhaber_setzen(crate::datenschutz::Bekannt::aus(profil.as_deref(), name.as_deref()));
    if profil.is_none() {
        tracing::warn!("Das Profil des Inhabers ist unbekannt (konten.inhaber_sid leer oder ohne Profil): Kein Ort geht auf.");
    }
    profil
}

/// Das Profil des Inhabers. Mit Einstellungen (Dienst): über die SID aus der
/// Registry (`ProfileList\<SID>\ProfileImagePath`); fehlt die SID, `None`
/// (nie das Profil des Dienstkontos). Ohne Einstellungen (Konsole, läuft als
/// Inhaber): das eigene Profil.
pub fn inhaber_profil(e: Option<&crate::dienst::einstellungen::Einstellungen>) -> Option<PathBuf> {
    match e {
        None => eigenes_profil(),
        Some(e) => match e.konten.inhaber_sid.trim() {
            "" => None,
            sid => profil_aus_registry(sid),
        },
    }
}

#[cfg(windows)]
fn profil_aus_registry(sid: &str) -> Option<PathBuf> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    if !crate::dienst::einstellungen::ist_sid(sid) {
        return None;
    }
    let k = winreg::RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(format!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\{sid}")).ok()?;
    let roh: String = k.get_value("ProfileImagePath").ok()?;
    let laufwerk = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
    let p = PathBuf::from(roh.replace("%SystemDrive%", &laufwerk).replace("%SYSTEMDRIVE%", &laufwerk));
    p.is_absolute().then_some(p)
}

#[cfg(not(windows))]
fn profil_aus_registry(_sid: &str) -> Option<PathBuf> {
    None
}

/// Ordner des Dienstes, die nie aufgehen: das Programm selbst und, wenn
/// bekannt, Datenspeicher, Sicherung, Protokoll, Oberfläche, Bote und Einstellungen.
pub fn ordner_des_dienstes(e: Option<&crate::dienst::einstellungen::Einstellungen>) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf)).into_iter().collect();
    if let Some(e) = e {
        v.extend(e.pfade.daten.parent().map(Path::to_path_buf));
        v.push(e.pfade.sicherung.clone());
        v.push(e.pfade.protokoll.clone());
        v.push(e.pfade.web.clone());
        v.extend(e.bote.programm.parent().map(Path::to_path_buf));
        v.extend(e.datei.parent().map(Path::to_path_buf));
    }
    v.retain(|p| p.is_absolute());
    v
}

#[cfg(test)]
pub mod tests;
