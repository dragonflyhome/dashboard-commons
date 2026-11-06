//! `pruefe`: Darf der Dienst jede Datei und jeden Ordner der Inhaltsquellen
//! lesen?
//!
//! Der typische Fall: Eine Datei wurde aus `%TEMP%` in den Inhalt verschoben
//! und hat ihre Rechte mitgenommen, ohne Vererbung. `pruefe` läuft als
//! Inhaber und liest sie; das Dienstkonto (`NT SERVICE\Dashboard`) nicht,
//! und die Seite zeigt den alten Stand oder nichts. Jeder solche Ort ist ein
//! FEHLER mit dem Befehl zum Beheben (`icacls "<Pfad im Inhalt>" /reset`,
//! durch den Inhaber im Ordner des Inhalts), nie mit einem Profilpfad.
//!
//! Das Dienstkonto kommt aus den Einstellungen (`[konten] dienst_sid` in
//! `dienst.toml`; die Datei aus `DASHBOARD_EINSTELLUNGEN`, sonst
//! [`EINSTELLUNGEN_STANDARD`]), nie fest aus dem Code. Fehlt sie oder die SID,
//! sagt ein HINWEIS, dass die Prüfung entfiel.
//!
//! Gelesen wird die DACL jedes Orts (`GetNamedSecurityInfoW`), nicht das
//! Token des Dienstes ([`lesen_erlaubt`]). Lesbar heißt: Die vollen Rechte von
//! `GENERIC_READ` (`FILE_GENERIC_READ`: Daten lesen bzw. Ordner auflisten,
//! erweiterte Attribute, Attribute, Rechte lesen, Synchronisieren) sind über
//! alle zutreffenden Einträge „erlauben“ zusammen gewährt, und keines davon
//! ist in einem Eintrag „verweigern“ verboten, auch nicht bedingt (callback).
//! Zutreffend sind die SID des Dienstes und die Gruppen, in denen jedes
//! Dienstkonto steht ([`GRUPPEN_DES_DIENSTES`]). Generische Rechte werden wie
//! bei Dateien abgebildet. Bedingtes Erlauben zählt nicht; Einträge nur zum
//! Vererben auch nicht. Eine leere DACL (NULL) erlaubt alles.
//!
//! Geprüft werden die Quellen, `data/` und `docs/entscheidungen/` daneben
//! ([`ziele`]): genau die Ordner des Inhalts, die der Dienst liest (wie
//! `meldungen::gelesene_ordner`); `docs/` selbst darf er nicht auflisten.
//! Verknüpfungen und Junctions, `.git`, `node_modules`, `target`,
//! `.svelte-kit` und `builds` bleiben außen vor; in einen Ordner, den der
//! Dienst nicht lesen darf, steigt die Prüfung nicht ab (ein Befund mit
//! `/reset /t` für alles darin).
//!
//! Nur unter Windows; auf anderen Systemen entfällt die Prüfung ohne Befund.

use std::path::{Path, PathBuf};

use crate::inhalt::pruefung::Befund;

/// Wo das Einrichtungsskript die Einstellungen ablegt.
pub const EINSTELLUNGEN_STANDARD: &str = r"C:\ProgramData\Dashboard\dienst.toml";
/// Umgebungsvariable für eine andere Einstellungsdatei.
pub const EINSTELLUNGEN_VAR: &str = "DASHBOARD_EINSTELLUNGEN";

/// Gruppen, in denen jedes Dienstkonto steht: Jeder, Authentifizierte
/// Benutzer, Benutzer, DIENST, ALL SERVICES, LOKAL, Diese Organisation.
pub const GRUPPEN_DES_DIENSTES: [&str; 7] = ["S-1-1-0", "S-1-5-11", "S-1-5-32-545", "S-1-5-6", "S-1-5-80-0", "S-1-2-0", "S-1-5-15"];

/// Ordner, die die Prüfung überspringt (Ablage von git, Werkzeuge, Bau).
pub const UEBERSPRINGEN: [&str; 5] = [".git", "node_modules", "target", ".svelte-kit", "builds"];

/// `FILE_GENERIC_READ`: READ_CONTROL, SYNCHRONIZE, FILE_READ_DATA,
/// FILE_READ_EA, FILE_READ_ATTRIBUTES.
pub const LESEN: u32 = 0x0012_0089;

/// Generische Rechte, abgebildet wie bei Dateien (GENERIC_READ, _WRITE,
/// _EXECUTE, _ALL); die übrigen Bits bleiben.
pub fn abbilden(maske: u32) -> u32 {
    let mut m = maske & 0x0FFF_FFFF;
    if maske & 0x8000_0000 != 0 {
        m |= LESEN;
    }
    if maske & 0x4000_0000 != 0 {
        m |= 0x0012_0116; // FILE_GENERIC_WRITE
    }
    if maske & 0x2000_0000 != 0 {
        m |= 0x0012_00A0; // FILE_GENERIC_EXECUTE
    }
    if maske & 0x1000_0000 != 0 {
        m |= 0x001F_01FF; // FILE_ALL_ACCESS
    }
    m
}

/// Ein Eintrag der DACL, soweit die Prüfung ihn braucht.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ace {
    /// ACE-Typ: 0 erlauben, 1 verweigern, 5/6 mit Objekt, 9/10 bedingt
    /// (callback), 11/12 bedingt mit Objekt.
    pub art: u8,
    /// ACE-Flags (0x08: nur zum Vererben).
    pub flags: u8,
    pub maske: u32,
    /// Gilt der Eintrag für das Dienstkonto oder eine seiner Gruppen?
    pub passt: bool,
}

/// Die Regel aus dem Kopf: alle Bits von [`LESEN`] erlaubt, keines verboten.
pub fn lesen_erlaubt(aces: &[Ace]) -> bool {
    let (mut erlaubt, mut verboten) = (0u32, 0u32);
    for a in aces.iter().filter(|a| a.passt && a.flags & 0x08 == 0) {
        let m = abbilden(a.maske) & LESEN;
        match a.art {
            0 => erlaubt |= m,
            1 | 6 | 10 | 12 => verboten |= m,
            // Bedingtes oder objektbezogenes Erlauben zählt nicht.
            _ => {}
        }
    }
    erlaubt & LESEN == LESEN && verboten == 0
}

/// Das Dienstkonto: SID und Name zum Zeigen (`NT SERVICE\Dashboard`, ohne
/// Namen bei Windows die SID).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dienstkonto {
    pub sid: String,
    pub name: String,
}

impl Dienstkonto {
    /// Aus geladenen Einstellungen; ohne `dienst_sid` ein Satz.
    pub fn aus(e: &crate::dienst::einstellungen::Einstellungen) -> Result<Dienstkonto, String> {
        let sid = e.konten.dienst_sid.trim().to_string();
        if sid.is_empty() {
            return Err("In den Einstellungen des Dienstes fehlt `konten.dienst_sid`.".to_string());
        }
        let name = match crate::datenschutz::konto_aus_sid(&sid) {
            Some((d, n)) if !d.is_empty() => format!("{d}\\{n}"),
            Some((_, n)) => n,
            None => format!("Das Dienstkonto ({sid})"),
        };
        Ok(Dienstkonto { sid, name })
    }
}

/// Das Dienstkonto aus der Einstellungsdatei dieses PCs.
pub fn dienstkonto() -> Result<Dienstkonto, String> {
    let datei = std::env::var_os(EINSTELLUNGEN_VAR).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(EINSTELLUNGEN_STANDARD));
    let e = crate::dienst::einstellungen::Einstellungen::laden(&datei).map_err(|s| s.0)?;
    Dienstkonto::aus(&e)
}

/// Satz, wenn die Prüfung entfällt (ohne Einstellungen oder SID).
pub fn hinweis_ohne_konto(grund: &str) -> Befund {
    Befund::hinweis("dienst.toml", None, format!("Ob der Dienst alle Dateien des Inhalts lesen darf, ist nicht geprüft: {grund}"))
}

/// Was `pruefe` prüft, je (Ordner, Ordner des Inhalts): die Quellen (oder
/// mit `projekt` nur dessen Ordner darin) und `data/` und `docs/entscheidungen/` neben jeder
/// Quelle (im Ordner des Inhalts, den der Dienst ebenso liest). Ohne doppelte.
pub fn ziele(quellen: &[PathBuf], projekt: Option<&str>) -> Vec<(PathBuf, PathBuf)> {
    let mut v: Vec<(PathBuf, PathBuf)> = Vec::new();
    let mut dazu = |ziel: PathBuf, basis: &Path| {
        if ziel.is_dir() && !v.iter().any(|(z, _)| *z == ziel) {
            v.push((ziel, basis.to_path_buf()));
        }
    };
    for q in quellen {
        let basis = q.parent().map(Path::to_path_buf).unwrap_or_else(|| q.clone());
        match projekt {
            Some(s) => dazu(q.join(s), &basis),
            None => dazu(q.clone(), &basis),
        }
        dazu(basis.join("data"), &basis);
        dazu(basis.join("docs").join("entscheidungen"), &basis);
    }
    v
}

/// Für `pruefe`: die Befunde zu allen `ziele` (je ein Ordner; relativ zu
/// `basis`, dem Ordner des Inhalts, stehen Datei und Befehl im Satz). Ohne
/// Dienstkonto ein Hinweis; unter anderen Systemen nichts.
pub fn pruefen(ziele: &[(PathBuf, PathBuf)]) -> Vec<Befund> {
    if !cfg!(windows) || ziele.is_empty() {
        return Vec::new();
    }
    match dienstkonto() {
        Ok(k) => ziele.iter().flat_map(|(ziel, basis)| befunde(ziel, basis, &k)).collect(),
        Err(grund) => vec![hinweis_ohne_konto(&grund)],
    }
}

/// Je Datei oder Ordner unter `ziel` (auch `ziel` selbst), den `konto` nicht
/// lesen darf, ein Fehler.
pub fn befunde(ziel: &Path, basis: &Path, konto: &Dienstkonto) -> Vec<Befund> {
    let mut aus = Vec::new();
    if !cfg!(windows) || !ziel.is_dir() {
        return aus;
    }
    let mut stapel = vec![ziel.to_path_buf()];
    if darf_lesen(ziel, &konto.sid) == Some(false) {
        aus.push(befund(ziel, true, basis, konto));
        return aus;
    }
    while let Some(ordner) = stapel.pop() {
        let Ok(eintraege) = std::fs::read_dir(&ordner) else { continue };
        let mut liste: Vec<_> = eintraege.flatten().collect();
        liste.sort_by_key(|e| e.file_name());
        for e in liste {
            let Ok(art) = e.file_type() else { continue };
            // Verknüpfungen und Junctions (unter Windows auch als Ordner gemeldet) nicht.
            if art.is_symlink() || ist_umleitung(&e.path()) {
                continue;
            }
            let pfad = e.path();
            if art.is_dir() && UEBERSPRINGEN.iter().any(|u| e.file_name().eq_ignore_ascii_case(u)) {
                continue;
            }
            match darf_lesen(&pfad, &konto.sid) {
                Some(false) => aus.push(befund(&pfad, art.is_dir(), basis, konto)),
                _ if art.is_dir() => stapel.push(pfad),
                _ => {}
            }
        }
    }
    aus.sort_by(|a, b| a.datei.cmp(&b.datei));
    aus
}

/// Der Befund zu einem Ort: Datei relativ zum Inhalt, der Befehl zum Beheben.
fn befund(pfad: &Path, ordner: bool, basis: &Path, konto: &Dienstkonto) -> Befund {
    let rel = pfad.strip_prefix(basis).unwrap_or(pfad);
    let datei = rel.display().to_string().replace('\\', "/");
    let befehl = rel.display().to_string().replace('/', "\\");
    let (was, t) = if ordner { ("diesen Ordner", " /t") } else { ("diese Datei", "") };
    Befund::fehler(
        datei,
        None,
        format!(
            "{} darf {was} nicht lesen; die Seiten zeigen sie darum nicht (oft: aus %TEMP% verschoben, die Rechte kamen ohne Vererbung mit). \
             Beheben als Inhaber im Ordner des Inhalts: `icacls \"{befehl}\" /reset{t}`.",
            konto.name
        ),
    )
}

/// Eine Junction oder ein anderer Analysepunkt (Reparse Point)?
#[cfg(windows)]
fn ist_umleitung(p: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    std::fs::symlink_metadata(p).is_ok_and(|m| m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
}

#[cfg(not(windows))]
fn ist_umleitung(_p: &Path) -> bool {
    false
}

/// Darf das Konto mit der SID `sid` den Ort lesen (siehe Kopf)? `None`, wenn
/// sich die DACL nicht lesen lässt.
#[cfg(windows)]
pub fn darf_lesen(pfad: &Path, sid: &str) -> Option<bool> {
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::{ERROR_SUCCESS, LocalFree};
    use windows_sys::Win32::Security::Authorization::{ConvertStringSidToSidW, GetNamedSecurityInfoW, SE_FILE_OBJECT};
    use windows_sys::Win32::Security::{ACL, DACL_SECURITY_INFORMATION, EqualSid, GetAce, PSECURITY_DESCRIPTOR, PSID};

    /// Eine SID aus Text, freigegeben in Drop.
    struct Sid(PSID);
    impl Drop for Sid {
        fn drop(&mut self) {
            // SAFETY: von ConvertStringSidToSidW mit LocalAlloc angelegt.
            unsafe { LocalFree(self.0 as _) };
        }
    }
    let sids: Vec<Sid> = std::iter::once(sid)
        .chain(GRUPPEN_DES_DIENSTES)
        .filter_map(|t| {
            let breit: Vec<u16> = t.trim().encode_utf16().chain(std::iter::once(0)).collect();
            let mut s: PSID = null_mut();
            // SAFETY: nullterminierter Text; die SID gibt Drop frei.
            (unsafe { ConvertStringSidToSidW(breit.as_ptr(), &mut s) } != 0 && !s.is_null()).then_some(Sid(s))
        })
        .collect();
    let breit: Vec<u16> = pfad.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let mut dacl: *mut ACL = null_mut();
    let mut sd: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: nullterminierter Pfad; `dacl` zeigt danach in `sd`, das unten mit LocalFree freigegeben wird.
    let r = unsafe { GetNamedSecurityInfoW(breit.as_ptr(), SE_FILE_OBJECT, DACL_SECURITY_INFORMATION, null_mut(), null_mut(), &mut dacl, null_mut(), &mut sd) };
    if r != ERROR_SUCCESS {
        return None;
    }
    let ergebnis = if dacl.is_null() {
        true
    } else {
        let mut aces = Vec::new();
        // SAFETY: `dacl` ist gültig, solange `sd` lebt; GetAce liefert Zeiger in die ACL.
        unsafe {
            for i in 0..u32::from((*dacl).AceCount) {
                let mut ace: *mut core::ffi::c_void = null_mut();
                if GetAce(dacl, i, &mut ace) == 0 || ace.is_null() {
                    continue;
                }
                let kopf = ace as *const u8;
                let (art, flags) = (*kopf, *kopf.add(1));
                let maske = kopf.add(4).cast::<u32>().read_unaligned();
                // Die SID: bei den Objekt-Typen hinter Objekt-Flags und bis zu zwei GUIDs.
                let sid_ab = match art {
                    0 | 1 | 9 | 10 => Some(8usize),
                    5 | 6 | 11 | 12 => {
                        let objekt_flags = kopf.add(8).cast::<u32>().read_unaligned();
                        Some(12 + 16 * ((objekt_flags & 1) + ((objekt_flags >> 1) & 1)) as usize)
                    }
                    _ => None,
                };
                let passt = sid_ab.is_some_and(|ab| sids.iter().any(|s| EqualSid(kopf.add(ab) as PSID, s.0) != 0));
                aces.push(Ace { art, flags, maske, passt });
            }
        }
        lesen_erlaubt(&aces)
    };
    // SAFETY: von GetNamedSecurityInfoW mit LocalAlloc angelegt.
    unsafe { LocalFree(sd as _) };
    Some(ergebnis)
}

#[cfg(not(windows))]
pub fn darf_lesen(_pfad: &Path, _sid: &str) -> Option<bool> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ace(art: u8, maske: u32) -> Ace {
        Ace { art, flags: 0, maske, passt: true }
    }

    /// Auflage der Abnahme: Lesbar nur mit allen Rechten von
    /// GENERIC_READ zusammen und ohne ein verbotenes; generische Rechte
    /// abgebildet; bedingte Verbote zählen, bedingtes Erlauben nicht.
    #[test]
    fn regel_der_leserechte() {
        assert!(lesen_erlaubt(&[ace(0, LESEN)]));
        assert!(lesen_erlaubt(&[ace(0, 0x8000_0000)]), "GENERIC_READ");
        assert!(lesen_erlaubt(&[ace(0, 0x1000_0000)]), "GENERIC_ALL");
        assert!(lesen_erlaubt(&[ace(0, 0x001F_01FF)]), "Vollzugriff");
        // Zusammen aus zwei Einträgen (etwa Gruppe und Dienst).
        assert!(lesen_erlaubt(&[ace(0, 0x0000_0089), ace(0, 0x0012_0000)]));
        // Nur „Daten lesen“ reicht nicht.
        assert!(!lesen_erlaubt(&[ace(0, 0x0000_0001)]));
        assert!(!lesen_erlaubt(&[]));
        // Ein verbotenes Bit, auch hinter dem Erlauben und auch bedingt.
        assert!(!lesen_erlaubt(&[ace(0, LESEN), ace(1, 0x0000_0080)]));
        assert!(!lesen_erlaubt(&[ace(0, LESEN), ace(10, 0x0000_0001)]), "bedingtes Verbot");
        assert!(!lesen_erlaubt(&[ace(0, LESEN), ace(12, 0x8000_0000)]), "bedingtes Verbot mit Objekt");
        // Ein Verbot ohne Leserecht (Schreiben) stört nicht.
        assert!(lesen_erlaubt(&[ace(0, LESEN), ace(1, 0x0000_0002)]));
        // Bedingtes Erlauben zählt nicht, ein Eintrag nur zum Vererben auch nicht.
        assert!(!lesen_erlaubt(&[ace(9, LESEN)]));
        assert!(!lesen_erlaubt(&[Ace { art: 0, flags: 0x08, maske: LESEN, passt: true }]));
        // Was für ein anderes Konto gilt, zählt nicht, weder erlaubt noch verboten.
        assert!(!lesen_erlaubt(&[Ace { art: 0, flags: 0, maske: LESEN, passt: false }]));
        assert!(lesen_erlaubt(&[ace(0, LESEN), Ace { art: 1, flags: 0, maske: LESEN, passt: false }]));
        assert_eq!(GRUPPEN_DES_DIENSTES.iter().filter(|g| ["S-1-5-80-0", "S-1-2-0", "S-1-5-15"].contains(g)).count(), 3);
    }

    /// Auflage: `data/` und `docs/entscheidungen/` (nicht ganz `docs/`) neben der Quelle werden mitgeprüft, ohne doppelte.
    #[test]
    fn ziele_mit_data_und_docs() {
        let w = std::env::temp_dir().join(format!("dashboard-rechte-ziele-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&w);
        for d in ["projects/probe", "data", "docs/entscheidungen", "docs/sonst"] {
            std::fs::create_dir_all(w.join(d)).unwrap();
        }
        let q = vec![w.join("projects")];
        let z = ziele(&q, None);
        assert_eq!(z, [(w.join("projects"), w.clone()), (w.join("data"), w.clone()), (w.join("docs").join("entscheidungen"), w.clone())]);
        let z = ziele(&[w.join("projects"), w.join("projects")], Some("probe"));
        assert_eq!(z, [(w.join("projects").join("probe"), w.clone()), (w.join("data"), w.clone()), (w.join("docs").join("entscheidungen"), w.clone())]);
        let _ = std::fs::remove_dir_all(&w);
    }

    #[cfg(windows)]
    mod windows {
        use super::super::*;

        /// SID für das Dienstkonto im Test: LOKALER DIENST (S-1-5-19), den
        /// icacls kennt und der in keiner der Gruppen steht.
        const DIENST: &str = "S-1-5-19";
        /// Erfundene SID, die Windows nicht kennt.
        const FREMD: &str = "S-1-5-80-1111111111-2222222222-3333333333-4444444444-5555555555";

        fn icacls(pfad: &Path, args: &[&str]) {
            let programm = PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into())).join("System32").join("icacls.exe");
            let aus = crate::dienst::programme::eigener_befehl(programm).arg(pfad).args(args).output().expect("icacls");
            assert!(aus.status.success(), "icacls {args:?}: {}", String::from_utf8_lossy(&aus.stdout));
        }

        /// Setzt die DACL einer Datei aus SDDL (für Einträge, die icacls nicht kann).
        fn sddl(pfad: &Path, text: &str) {
            use std::os::windows::ffi::OsStrExt;
            use windows_sys::Win32::Foundation::LocalFree;
            use windows_sys::Win32::Security::Authorization::ConvertStringSecurityDescriptorToSecurityDescriptorW;
            use windows_sys::Win32::Security::{DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, SetFileSecurityW};
            const PROTECTED_DACL_SECURITY_INFORMATION: u32 = 0x8000_0000;
            let breit: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            let mut sd: PSECURITY_DESCRIPTOR = std::ptr::null_mut();
            // SAFETY: nullterminierter Text; `sd` gibt LocalFree unten frei.
            let ok = unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(breit.as_ptr(), 1, &mut sd, std::ptr::null_mut()) };
            assert!(ok != 0, "SDDL: {}", std::io::Error::last_os_error());
            let datei: Vec<u16> = pfad.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
            // SAFETY: gültiger Pfad und Deskriptor.
            let gesetzt = unsafe { SetFileSecurityW(datei.as_ptr(), DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION, sd) };
            let fehler = std::io::Error::last_os_error();
            // SAFETY: von ConvertStringSecurityDescriptorToSecurityDescriptorW angelegt.
            unsafe { LocalFree(sd as _) };
            assert!(gesetzt != 0, "SetFileSecurityW: {fehler}");
        }

        /// Räumt auch auf, wenn der Test scheitert.
        struct Weg(PathBuf);
        impl Drop for Weg {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }

        /// Zusatzpunkt 5 mit den Auflagen: Eine Datei (und ein Ordner) unter
        /// %TEMP% ohne vererbte Rechte, nur mit dem Recht des Inhabers, darf der
        /// Dienst nicht lesen: FEHLER mit `icacls … /reset`, ohne Profilpfad.
        /// Nur „Daten lesen“ reicht nicht; ein Verbot (auch bedingt) macht
        /// unlesbar; ALL SERVICES zählt als Gruppe; `target`, `.svelte-kit`
        /// und `builds` bleiben außen vor; `data/` wird mitgeprüft.
        #[test]
        fn datei_ohne_vererbung_ist_fehler() {
            let basis = std::env::temp_dir().join(format!("dashboard-rechte-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&basis);
            let _weg = Weg(basis.clone());
            let ziel = basis.join("projects").join("probe");
            for d in ["daten", "gesperrt", "target", ".svelte-kit", "builds"] {
                std::fs::create_dir_all(ziel.join(d)).unwrap();
            }
            std::fs::create_dir_all(basis.join("data")).unwrap();
            let dateien = [
                "daten/verschoben.toml",
                "daten/jeder.toml",
                "daten/dienst.toml",
                "daten/normal.toml",
                "daten/teil.toml",
                "daten/alle-dienste.toml",
                "daten/verboten.toml",
                "daten/bedingt.toml",
                "gesperrt/innen.toml",
                "target/x.toml",
                ".svelte-kit/x.toml",
                "builds/x.toml",
            ];
            for d in dateien {
                std::fs::write(ziel.join(d), "x").unwrap();
            }
            std::fs::write(basis.join("data").join("pc.toml"), "x").unwrap();
            let eigene = crate::bote::win::eigene_sid().unwrap();
            let ich = format!("*{eigene}:F");
            // Wie aus %TEMP% verschoben: keine Vererbung, nur der Inhaber.
            for d in [
                "daten/verschoben.toml",
                "daten/jeder.toml",
                "daten/dienst.toml",
                "daten/teil.toml",
                "daten/alle-dienste.toml",
                "gesperrt",
                "target/x.toml",
                ".svelte-kit/x.toml",
                "builds/x.toml",
            ] {
                icacls(&ziel.join(d), &["/inheritance:r", "/grant:r", &ich]);
            }
            icacls(&basis.join("data").join("pc.toml"), &["/inheritance:r", "/grant:r", &ich]);
            icacls(&ziel.join("daten/jeder.toml"), &["/grant", "*S-1-1-0:R"]);
            icacls(&ziel.join("daten/dienst.toml"), &["/grant", &format!("*{DIENST}:R")]);
            icacls(&ziel.join("daten/teil.toml"), &["/grant", &format!("*{DIENST}:(RD)")]);
            icacls(&ziel.join("daten/alle-dienste.toml"), &["/grant", "*S-1-5-80-0:R"]);
            // Der Rest erbt, was %TEMP% vergibt; damit der Dienst ihn lesen darf,
            // bekommt der Testordner ein vererbbares Leserecht.
            icacls(&basis, &["/grant", &format!("*{DIENST}:(OI)(CI)R")]);
            icacls(&ziel.join("daten/verboten.toml"), &["/deny", &format!("*{DIENST}:(RA)")]);
            sddl(&ziel.join("daten/bedingt.toml"), &format!("D:P(A;;FA;;;{eigene})(A;;FR;;;{DIENST})(XD;;FR;;;WD;(Member_of {{SID(BA)}}))"));

            let konto = Dienstkonto { sid: DIENST.to_string(), name: "Das Dienstkonto".to_string() };
            for (d, soll) in [
                ("daten/verschoben.toml", false),
                ("daten/jeder.toml", true),
                ("daten/dienst.toml", true),
                ("daten/normal.toml", true),
                ("daten/teil.toml", false),
                ("daten/alle-dienste.toml", true),
                ("daten/verboten.toml", false),
                ("daten/bedingt.toml", false),
            ] {
                assert_eq!(darf_lesen(&ziel.join(d), DIENST), Some(soll), "{d}");
            }
            let mut b = befunde(&ziel, &basis, &konto);
            b.extend(befunde(&basis.join("data"), &basis, &konto));
            let dateien: Vec<&str> = b.iter().map(|b| b.datei.as_str()).collect();
            assert_eq!(
                dateien,
                [
                    "projects/probe/daten/bedingt.toml",
                    "projects/probe/daten/teil.toml",
                    "projects/probe/daten/verboten.toml",
                    "projects/probe/daten/verschoben.toml",
                    "projects/probe/gesperrt",
                    "data/pc.toml",
                ],
                "{b:#?}"
            );
            assert!(b.iter().all(Befund::ist_fehler));
            assert!(b[3].text.contains(r#"`icacls "projects\probe\daten\verschoben.toml" /reset`"#), "{}", b[3]);
            assert!(b[4].text.contains(r#"`icacls "projects\probe\gesperrt" /reset /t`"#), "{}", b[4]);
            for x in &b {
                assert!(x.text.starts_with("Das Dienstkonto darf"), "{x}");
                assert!(!x.text.contains(r":\") && !x.text.contains("Users"), "kein Profilpfad: {x}");
            }
            drop(_weg);
            assert!(!basis.exists(), "aufgeräumt");
        }

        /// Das Dienstkonto kommt aus den Einstellungen (`konten.dienst_sid`);
        /// ohne sie ein Satz statt einer Prüfung.
        #[test]
        fn dienstkonto_aus_den_einstellungen() {
            let ordner = std::env::temp_dir().join(format!("dashboard-rechte-einst-{}", std::process::id()));
            let _weg = Weg(ordner.clone());
            std::fs::create_dir_all(ordner.join("repo")).unwrap();
            let p = |r: &str| ordner.join(r).display().to_string().replace('\\', "/");
            let text = |sid: &str| {
                format!(
                    "schema = 1\nrepo = \"{}\"\nport = 8787\n\n[pfade]\ndaten = \"{}\"\nsicherung = \"{}\"\nprotokoll = \"{}\"\n\n[konten]\ninhaber_sid = \"\"\ndienst_sid = \"{sid}\"\n\n[bote]\npipe = \"dashboard-bote\"\nprogramm = \"{}\"\n",
                    p("repo"),
                    p("daten/dashboard.redb"),
                    p("sicherung"),
                    p("protokoll"),
                    p("bin/dashboard-bote.exe"),
                )
            };
            let datei = ordner.join("dienst.toml");
            let e = crate::dienst::einstellungen::Einstellungen::aus_text(&datei, &text(FREMD)).unwrap();
            let k = Dienstkonto::aus(&e).unwrap();
            assert_eq!(k.sid, FREMD);
            assert!(k.name.contains(FREMD), "unbekanntes Konto: die SID im Namen: {}", k.name);
            // Ein Konto, das Windows kennt, bekommt seinen Namen.
            let e = crate::dienst::einstellungen::Einstellungen::aus_text(&datei, &text("S-1-5-80-0")).unwrap();
            assert!(!Dienstkonto::aus(&e).unwrap().name.contains("S-1-5"), "bekanntes Konto mit Namen");
            let e = crate::dienst::einstellungen::Einstellungen::aus_text(&datei, &text("")).unwrap();
            assert!(Dienstkonto::aus(&e).unwrap_err().contains("dienst_sid"));
            assert!(hinweis_ohne_konto("x").text.contains("nicht geprüft"));
        }
    }
}
