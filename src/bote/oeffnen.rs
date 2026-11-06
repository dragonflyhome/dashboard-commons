//! „Öffnen“ im Boten (Entscheidung lokal-oeffnen). Der Dienst hat
//! den Pfad schon als Text geprüft; der Bote prüft ihn ganz, mit seinen
//! eigenen Wurzeln (sein Profil, die Inhaltsquellen aus seinen
//! Einstellungen), den eingebauten Tabu-Namen und seiner eigenen Liste der
//! Tabu-Ordner (frisch gelesen) und seiner eigenen Liste der Endungen
//! ([`ENDUNGEN`]). Was er nicht prüfen kann, öffnet er nicht (geschlossen
//! scheitern). Er antwortet nur `Geoeffnet` oder mit einem Satz, nie mit
//! Inhalt der Datei, und schreibt nie den Pfad ins Protokoll.
//!
//! Vor der Prüfung: nur vom Dienst (die SID des Clients an der Pipe ist
//! `konten.dienst_sid`, `Aufrufer`) und höchstens 10 je Minute (eigene
//! Grenze des Boten). Eine Datei geht nur auf, wenn ihrer Endung ein Programm
//! der Positivliste ([`PROGRAMME`]) zugeordnet ist.
//!
//! Geöffnet wird über [`Oeffner`]: im Betrieb [`WindowsOeffner`] (Datei mit
//! ShellExecuteW, Verb „open“, ohne Argumente; Ordner und zip mit
//! `%SystemRoot%\explorer.exe` und dem Pfad als einzigem Argument, ein Ordner
//! mit `\` am Ende, damit die Shell keine Endung wie `.cmd` anhängt). Tests
//! setzen [`Aufnahme`] ein, die nur aufschreibt, was geöffnet würde.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::oeffnen::{self, Abweisung, Art, Regeln};
use crate::pc::inventory;
use crate::vertrag::bote::Antwort;

/// Die eigene Positivliste des Boten (gleich der Entscheidung; ein Test
/// vergleicht sie mit der des Servers).
pub const ENDUNGEN: [&str; 16] = ["pdf", "docx", "xlsx", "pptx", "odt", "ods", "odp", "txt", "csv", "md", "png", "jpg", "jpeg", "gif", "webp", "svg"];
/// zip geht nur als Ordner im Explorer auf.
pub const EXPLORER_ENDUNGEN: [&str; 1] = ["zip"];

const TEXT: &[&str] = &["notepad.exe", "code.exe"];
const PDF: &[&str] = &["msedge.exe", "acrord32.exe", "acrobat.exe"];
const WORT: &[&str] = &["winword.exe", "soffice.exe", "soffice.bin", "swriter.exe"];
const TABELLE: &[&str] = &["excel.exe", "soffice.exe", "soffice.bin", "scalc.exe"];
const VORTRAG: &[&str] = &["powerpnt.exe", "soffice.exe", "soffice.bin", "simpress.exe"];
const BILD: &[&str] = &["photos.exe", "microsoft.photos.exe", "photosapp.exe"];

/// Je Endung die Programme (Dateiname, klein), mit denen der Bote öffnet.
/// Ein anderes zugeordnetes Programm (etwa ein Browser für SVG oder ein
/// Programm fremder Cloud-Dienste für md): nicht öffnen.
pub const PROGRAMME: [(&str, &[&str]); 16] = [
    ("txt", TEXT),
    ("md", TEXT),
    ("csv", &["excel.exe", "soffice.exe", "soffice.bin", "scalc.exe", "notepad.exe", "code.exe"]),
    ("pdf", PDF),
    ("docx", WORT),
    ("odt", WORT),
    ("xlsx", TABELLE),
    ("ods", TABELLE),
    ("pptx", VORTRAG),
    ("odp", VORTRAG),
    ("png", BILD),
    ("jpg", BILD),
    ("jpeg", BILD),
    ("gif", BILD),
    ("webp", BILD),
    ("svg", BILD),
];

/// Ist `programm` (Dateiname, klein) für `endung` erlaubt?
pub fn programm_erlaubt(endung: &str, programm: &str) -> bool {
    PROGRAMME.iter().any(|(e, liste)| *e == endung && liste.contains(&programm))
}

/// Wie geöffnet wird; im Test ersetzt.
pub trait Oeffner: Send + Sync + fmt::Debug {
    /// Datei mit dem zugeordneten Programm (ShellExecute, Verb open, ohne Argumente).
    fn datei(&self, pfad: &Path) -> Result<(), String>;
    /// Ordner (oder zip) im Explorer: explorer.exe mit dem Pfad als einzigem Argument.
    fn ordner(&self, pfad: &Path, ist_ordner: bool) -> Result<(), String>;
    /// Dateiname des Programms, das Windows für diese Endung zum Öffnen nimmt
    /// (klein geschrieben); `None`, wenn es sich nicht ermitteln lässt.
    fn programm_fuer(&self, endung: &str) -> Option<String>;
}

/// Was ShellExecuteW für eine Datei bekommt: genau diese Werte, nichts aus Daten außer dem Pfad.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellAufruf {
    pub verb: &'static str,
    pub datei: PathBuf,
    /// Immer `None`: keine Argumente.
    pub parameter: Option<String>,
    /// Immer `None`: kein Arbeitsordner.
    pub ordner: Option<PathBuf>,
}

/// Der Aufruf für eine Datei.
pub fn shell_aufruf(pfad: &Path) -> ShellAufruf {
    ShellAufruf { verb: "open", datei: pfad.to_path_buf(), parameter: None, ordner: None }
}

/// Das eine Argument für explorer.exe, genau so auf der Befehlszeile: in
/// Anführungszeichen; ein Ordner endet mit `\.` (dann hängt die Shell nie
/// eine Endung an, etwa `Werkzeugkiste` → `Werkzeugkiste.cmd`; fehlt der Ordner, gibt
/// es auch `Werkzeugkiste\.` nicht). Vor dem schließenden `"` steht kein `\`:
/// explorer.exe liest seine Befehlszeile nicht nach CommandLineToArgvW. Mit
/// `\\"` öffnete er „Dokumente“ statt des Ordners (mit echten
/// Fenstern auf einen Ordner unter %TEMP% geprüft: `\\"` → Dokumente, `\"`
/// und `\."` → der Ordner). Mit `\."` lesen Explorer und CommandLineToArgvW
/// dasselbe. Der Pfad hat kein `"` (das prüft die Form).
pub fn explorer_argument(pfad: &Path, ist_ordner: bool) -> String {
    let text = pfad.display().to_string();
    if ist_ordner { format!("\"{}\\.\"", text.trim_end_matches('\\')) } else { format!("\"{text}\"") }
}

/// explorer.exe aus `%SystemRoot%` (nie über PATH, `dienst::programme`) mit
/// dem einen Argument aus [`explorer_argument`]. Fehlt er: ein Satz.
pub fn explorer_befehl(pfad: &Path, ist_ordner: bool) -> Result<(std::process::Command, String), String> {
    let mut c = crate::dienst::programme::befehl(crate::dienst::programme::Werkzeug::Explorer)?;
    let arg = explorer_argument(pfad, ist_ordner);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.raw_arg(&arg);
    }
    #[cfg(not(windows))]
    c.arg(&arg);
    Ok((c, arg))
}

/// Öffnet wirklich (nur im Betrieb des Boten).
#[derive(Debug, Default)]
pub struct WindowsOeffner;

#[cfg(windows)]
fn breit(s: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    s.encode_wide().chain(std::iter::once(0)).collect()
}

impl Oeffner for WindowsOeffner {
    #[cfg(windows)]
    fn datei(&self, pfad: &Path) -> Result<(), String> {
        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        const SW_SHOWNORMAL: i32 = 1;
        let a = shell_aufruf(pfad);
        let verb = breit(std::ffi::OsStr::new(a.verb));
        let datei = breit(a.datei.as_os_str());
        // SAFETY: nullterminierte Texte; Parameter und Ordner sind NULL (keine Argumente).
        let r = unsafe { ShellExecuteW(std::ptr::null_mut(), verb.as_ptr(), datei.as_ptr(), std::ptr::null(), std::ptr::null(), SW_SHOWNORMAL) };
        if r as isize > 32 { Ok(()) } else { Err(format!("ShellExecuteW: {}", r as isize)) }
    }

    #[cfg(not(windows))]
    fn datei(&self, _pfad: &Path) -> Result<(), String> {
        Err("nur unter Windows".into())
    }

    fn ordner(&self, pfad: &Path, ist_ordner: bool) -> Result<(), String> {
        let (mut c, _) = explorer_befehl(pfad, ist_ordner)?;
        c.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("explorer.exe: {e}"))
    }

    #[cfg(windows)]
    fn programm_fuer(&self, endung: &str) -> Option<String> {
        use windows_sys::Win32::UI::Shell::{ASSOCF_INIT_IGNOREUNKNOWN, ASSOCF_NOTRUNCATE, ASSOCSTR_EXECUTABLE, AssocQueryStringW};
        let e = breit(std::ffi::OsStr::new(&format!(".{endung}")));
        let verb = breit(std::ffi::OsStr::new("open"));
        let mut puffer = vec![0u16; 1024];
        let mut laenge = puffer.len() as u32;
        // SAFETY: Puffer mit angegebener Länge, nullterminierte Eingaben.
        let hr = unsafe {
            AssocQueryStringW(ASSOCF_INIT_IGNOREUNKNOWN | ASSOCF_NOTRUNCATE, ASSOCSTR_EXECUTABLE, e.as_ptr(), verb.as_ptr(), puffer.as_mut_ptr(), &mut laenge)
        };
        if hr != 0 {
            return None;
        }
        let n = puffer.iter().position(|&c| c == 0).unwrap_or(puffer.len());
        let pfad = PathBuf::from(String::from_utf16_lossy(&puffer[..n]));
        pfad.file_name().map(|f| f.to_string_lossy().to_lowercase())
    }

    #[cfg(not(windows))]
    fn programm_fuer(&self, _endung: &str) -> Option<String> {
        None
    }
}

/// Öffnet nie (in Unit-Tests der Standard von `Kontext::jetzt`, damit kein
/// Test aus Versehen etwas auf dem PC aufmacht).
#[derive(Debug, Default)]
pub struct Verweigerer;

impl Oeffner for Verweigerer {
    fn datei(&self, _pfad: &Path) -> Result<(), String> {
        Err("im Test wird nichts geöffnet".into())
    }
    fn ordner(&self, _pfad: &Path, _ist_ordner: bool) -> Result<(), String> {
        Err("im Test wird nichts geöffnet".into())
    }
    fn programm_fuer(&self, _endung: &str) -> Option<String> {
        None
    }
}

/// Ein aufgeschriebener Aufruf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Aufruf {
    Datei(ShellAufruf),
    Ordner { programm: PathBuf, argument: String },
}

/// Für Tests: schreibt auf, was geöffnet würde, und öffnet nichts. Ohne
/// eigene `programme` gilt je Endung das erste Programm der Positivliste.
#[derive(Debug, Default)]
pub struct Aufnahme {
    pub aufrufe: Mutex<Vec<Aufruf>>,
    /// Endung → zugeordnetes Programm (Ersatz für AssocQueryStringW).
    pub programme: Option<Vec<(String, String)>>,
}

impl Aufnahme {
    pub fn alle(&self) -> Vec<Aufruf> {
        self.aufrufe.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl Oeffner for Aufnahme {
    fn datei(&self, pfad: &Path) -> Result<(), String> {
        self.aufrufe.lock().unwrap_or_else(|e| e.into_inner()).push(Aufruf::Datei(shell_aufruf(pfad)));
        Ok(())
    }
    fn ordner(&self, pfad: &Path, ist_ordner: bool) -> Result<(), String> {
        let (c, argument) = explorer_befehl(pfad, ist_ordner)?;
        let aufruf = Aufruf::Ordner { programm: PathBuf::from(c.get_program()), argument };
        self.aufrufe.lock().unwrap_or_else(|e| e.into_inner()).push(aufruf);
        Ok(())
    }
    fn programm_fuer(&self, endung: &str) -> Option<String> {
        match &self.programme {
            Some(l) => l.iter().find(|(e, _)| e == endung).map(|(_, p)| p.clone()),
            None => PROGRAMME.iter().find(|(e, _)| *e == endung).and_then(|(_, l)| l.first()).map(|p| p.to_string()),
        }
    }
}

/// Die Regeln des Boten aus seinem Kontext: Wurzeln = sein Profil (nur ein
/// vollständiger Pfad) und die Inhaltsquellen; Tabu-Liste frisch gelesen
/// (fehlt sie: `None`, dann öffnet er nichts), dazu die eingebauten Namen;
/// seine eigenen Endungen.
pub fn regeln(k: &super::aufgaben::Kontext) -> Regeln {
    Regeln {
        profil: Some(k.profil.clone()).filter(|p| p.is_absolute()),
        quellen: k.quellen.clone(),
        verboten: k.verboten.clone(),
        tabu: inventory::ausschluss_laden(k.ausschluss.as_deref()).ok(),
        tabu_pflicht: true,
        endungen: ENDUNGEN.iter().map(|e| e.to_string()).collect(),
        explorer_endungen: EXPLORER_ENDUNGEN.iter().map(|e| e.to_string()).collect(),
        ..Regeln::default()
    }
}

/// Darf ein Client mit dieser SID „Öffnen“ schicken? Nur das Dienstkonto
/// aus den Einstellungen; ohne beide: nie.
pub fn vom_dienst(client_sid: Option<&str>, dienst_sid: Option<&str>) -> bool {
    match (client_sid, dienst_sid) {
        (Some(c), Some(d)) => crate::anmeldung::ist_inhaber(c, d),
        _ => false,
    }
}

/// Prüft und öffnet. Nur `Geoeffnet` oder ein Satz. `client_sid`: das Konto
/// des Clients an der Pipe (`None`: unbekannt, etwa `--einmal`).
pub fn oeffnen(pfad: &str, k: &super::aufgaben::Kontext, client_sid: Option<&str>) -> Antwort {
    if !vom_dienst(client_sid, k.dienst_sid.as_deref()) {
        tracing::warn!("Öffnen abgelehnt: die Anfrage kam nicht vom Dienstkonto.");
        return Antwort::fehler(Abweisung::NichtVomDienst.satz());
    }
    if !k.oeffnungen.nehmen(std::time::Instant::now()) {
        tracing::warn!("Öffnen abgelehnt: mehr als {} je Minute.", oeffnen::JE_MINUTE);
        return Antwort::fehler(Abweisung::ZuViele.satz());
    }
    let ziel = match oeffnen::pruefen(pfad, &regeln(k)) {
        Ok(z) => z,
        Err(a) => {
            tracing::warn!("Öffnen abgelehnt ({a:?}); der Pfad steht nicht im Protokoll.");
            return Antwort::fehler(a.satz());
        }
    };
    ziel_oeffnen(&ziel, k)
}

/// Öffnet einen geprüften Ort. Ob es ein Ordner ist, steht im Ergebnis der
/// Prüfung; hier wird nicht noch einmal auf der Platte nachgesehen (sonst
/// könnte ein Tausch dazwischen das `\` am Ende verschwinden lassen).
pub fn ziel_oeffnen(ziel: &oeffnen::Ziel, k: &super::aufgaben::Kontext) -> Antwort {
    let ergebnis = match ziel.art {
        Art::Ordner => k.oeffner.ordner(&ziel.pfad, ziel.ist_ordner),
        Art::Datei => {
            let endung = ziel.pfad.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            match k.oeffner.programm_fuer(&endung) {
                Some(p) if programm_erlaubt(&endung, &p) => {}
                p => {
                    tracing::warn!("Öffnen abgelehnt: .{endung} ist mit {} verknüpft.", p.as_deref().unwrap_or("nichts Bekanntem"));
                    return Antwort::fehler(Abweisung::Programm(endung).satz());
                }
            }
            k.oeffner.datei(&ziel.pfad)
        }
    };
    match ergebnis {
        Ok(()) => {
            tracing::info!("Öffnen: {} geöffnet.", if ziel.art == Art::Ordner { "Ordner" } else { "Datei" });
            Antwort::Geoeffnet
        }
        Err(e) => {
            tracing::warn!("Öffnen ging nicht: {e}");
            Antwort::fehler("Windows hat den Ort nicht geöffnet. Einzelheiten stehen im Protokoll des Boten.")
        }
    }
}

#[cfg(test)]
pub mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::bote::aufgaben::{Kontext, ausfuehren, ausfuehren_von};
    use crate::oeffnen::VERBOTENE_ENDUNGEN;
    use crate::oeffnen::tests::Platz;
    use crate::vertrag::bote::Anfrage;

    /// Erfundene SID des Dienstkontos für die Tests.
    pub const DIENST: &str = "S-1-5-80-1111-2222-3333-4444-5555";

    /// Ein Kontext des Boten im Spielplatz, mit Aufnahme statt echtem Öffnen.
    pub fn kontext(p: &Platz, aufnahme: Arc<Aufnahme>) -> Kontext {
        Kontext {
            konto_sid: "S-1-5-21-9".into(),
            gestartet: "jetzt".into(),
            profil: p.profil.clone(),
            quellen: vec![p.quelle.clone()],
            ausschluss: Some(p.liste.clone()),
            repo: None,
            bau_zeitlimit: crate::bote::bau::ZEITLIMIT,
            verboten: vec![p.dienst.clone()],
            oeffner: aufnahme,
            dienst_sid: Some(DIENST.into()),
            oeffnungen: Default::default(),
        }
    }

    /// So schickt der Dienst: mit seiner SID am anderen Ende der Pipe (Grenze je Fall frisch).
    pub async fn frage(k: &Kontext, pfad: &str) -> Antwort {
        k.oeffnungen.leeren();
        ausfuehren_von(Anfrage::Oeffnen { pfad: pfad.to_string() }, k, Some(DIENST.into())).await
    }

    fn abgelehnt(a: &Antwort) -> bool {
        matches!(a, Antwort::Fehler { .. })
    }

    fn satz(a: &Antwort) -> &str {
        match a {
            Antwort::Fehler { satz } => satz,
            _ => "",
        }
    }

    /// Erlaubte Datei: ShellExecute mit Verb open, ohne Argumente und Ordner;
    /// Ordner: explorer.exe aus SystemRoot mit dem Pfad als einzigem Argument
    /// und `\` am Ende (nie `Werkzeugkiste` → `Werkzeugkiste.cmd`); zip ohne `\`.
    #[tokio::test]
    async fn erlaubte_datei_und_ordner_mit_ersatz() {
        let p = Platz::neu("bote-erlaubt");
        let a = Arc::new(Aufnahme::default());
        let k = kontext(&p, a.clone());
        assert!(matches!(frage(&k, &p.dok("bauplan.pdf")).await, Antwort::Geoeffnet));
        assert!(matches!(frage(&k, &p.dok("Werkzeugkiste")).await, Antwort::Geoeffnet));
        assert!(matches!(frage(&k, &p.dok("archiv.zip")).await, Antwort::Geoeffnet));
        let alle = a.alle();
        assert_eq!(alle.len(), 3, "{alle:?}");
        match &alle[0] {
            Aufruf::Datei(s) => {
                assert_eq!((s.verb, s.parameter.as_deref(), s.ordner.as_deref()), ("open", None, None));
                assert!(s.datei.ends_with("bauplan.pdf"));
            }
            x => panic!("{x:?}"),
        }
        for (aufruf, ende) in [(&alle[1], r#"Werkzeugkiste\.""#), (&alle[2], "archiv.zip\"")] {
            match aufruf {
                Aufruf::Ordner { programm, argument } => {
                    assert!(programm.is_absolute(), "nie über PATH: {programm:?}");
                    assert!(programm.to_string_lossy().to_lowercase().ends_with(r"\explorer.exe"), "{programm:?}");
                    assert!(argument.starts_with('"') && argument.ends_with(ende), "{argument}");
                    assert!(!argument.contains('/') && argument.matches('"').count() == 2, "ein Argument, kein Schalter: {argument}");
                }
                x => panic!("{x:?}"),
            }
        }
        assert_eq!(explorer_argument(Path::new(r"C:\a b\Ordner"), true), r#""C:\a b\Ordner\.""#);
        assert_eq!(explorer_argument(Path::new(r"C:\a b\Ordner\"), true), r#""C:\a b\Ordner\.""#);
        // Nie ein `\` direkt vor dem schließenden `"` (Explorer läse das anders).
        assert!(!explorer_argument(Path::new(r"C:\a b\Ordner"), true).ends_with("\\\""));
        assert_eq!(explorer_argument(Path::new(r"C:\a\x.zip"), false), r#""C:\a\x.zip""#);
    }

    /// Der Bote prüft selbst: jede verbotene Endung, Großschreibung, doppelte
    /// Endung, ohne Endung, Junction hinaus, .., UNC, Gerätepfad, Datenstrom,
    /// %, Shell-Ordner, Tabu (auch anders geschrieben), AppData, Ordner des
    /// Dienstes. Nichts geöffnet; kein Satz verrät, ob es dort etwas gibt.
    #[tokio::test]
    async fn bote_weist_selbst_ab() {
        let p = Platz::neu("bote-ab");
        let a = Arc::new(Aufnahme::default());
        let k = kontext(&p, a.clone());
        let mut faelle: Vec<String> = Vec::new();
        for e in VERBOTENE_ENDUNGEN {
            faelle.push(p.dok(&format!("datei.{e}")));
            faelle.push(p.dok(&format!("GROSS.{}", e.to_uppercase())));
        }
        faelle.extend([
            p.dok("x.pdf.exe"),
            p.dok("ohne_endung"),
            format!(r"{}\..\Dokumente\bauplan.pdf", p.text(&p.profil.join("Dokumente"))),
            r"\\server\freigabe\bauplan.pdf".to_string(),
            format!(r"\\?\{}", p.dok("bauplan.pdf")),
            format!(r"\\.\{}", p.dok("bauplan.pdf")),
            format!("{}:versteckt", p.dok("bauplan.pdf")),
            p.dok("%USERPROFILE%"),
            p.dok("Ablage.{20D04FE0-3AEA-1069-A2D8-08002B30309D}"),
            p.text(&p.profil.join("Geheim Ablage").join("brief.pdf")),
            p.text(&p.profil.join("GEHEIM_ABLAGE").join("brief.pdf")),
            p.text(&p.profil.join("AppData").join("Local").join("x.pdf")),
            p.text(&p.dienst.join("x.txt")),
            p.text(&p.draussen.join("fremd.pdf")),
            p.text(&p.profil.join("projekt").join(".git").join("notiz.md")),
        ]);
        if p.junctions || !crate::ueberspringen("mklink /J ging nicht") {
            faelle.push(p.text(&p.profil.join("sprung").join("fremd.pdf")));
            faelle.push(p.text(&p.profil.join("harmlos").join("brief.pdf")));
        }
        for f in &faelle {
            let antwort = frage(&k, f).await;
            assert!(abgelehnt(&antwort), "{f}: {antwort:?}");
            let s = satz(&antwort);
            assert!(!s.contains(r"\Users") && !s.to_lowercase().contains("geheim"), "{s}");
        }
        // Gibt es nicht, liegt draußen (auch über eine Junction), Tabu: derselbe Satz.
        let fehlt = frage(&k, &p.dok("gibt-es-nicht.pdf")).await;
        assert_eq!(satz(&fehlt), crate::oeffnen::SATZ_NICHT_DA);
        for anders in [p.text(&p.draussen.join("fremd.pdf")), p.text(&p.profil.join("Geheim Ablage").join("brief.pdf"))] {
            assert_eq!(satz(&frage(&k, &anders).await), satz(&fehlt), "{anders}");
        }
        if p.junctions {
            assert_eq!(satz(&frage(&k, &p.text(&p.profil.join("sprung").join("fremd.pdf"))).await), satz(&fehlt));
        }
        assert!(a.alle().is_empty(), "nichts geöffnet: {:?}", a.alle());
    }

    /// Geschlossen scheitern: ohne Tabu-Liste (fehlt, kaputt, leer) öffnet der Bote nichts.
    #[tokio::test]
    async fn ohne_tabu_liste_nichts() {
        let p = Platz::neu("bote-liste");
        let a = Arc::new(Aufnahme::default());
        let ohne = Kontext { ausschluss: None, ..kontext(&p, a.clone()) };
        assert!(abgelehnt(&frage(&ohne, &p.dok("bauplan.pdf")).await));
        std::fs::write(&p.liste, "nie_betreten = []\n").unwrap();
        assert!(abgelehnt(&frage(&kontext(&p, a.clone()), &p.dok("bauplan.pdf")).await), "leer");
        std::fs::write(&p.liste, "kaputt").unwrap();
        assert!(abgelehnt(&frage(&kontext(&p, a.clone()), &p.dok("bauplan.pdf")).await), "kaputt");
        assert!(a.alle().is_empty());
    }

    /// Zugeordnete Programme nur aus der Positivliste (Ersatz für
    /// AssocQueryStringW): md mit dem Programm eines Cloud-Dienstes, SVG im
    /// Browser, ein unbekanntes Programm und „nichts ermittelbar“ bleiben zu;
    /// Notepad, VS Code, Edge für PDF, Excel und Fotos gehen.
    #[tokio::test]
    async fn nur_programme_der_positivliste() {
        let p = Platz::neu("bote-programme");
        std::fs::write(p.profil.join("Dokumente").join("notiz.md"), "x").unwrap();
        let faelle = [
            ("notiz.md", Some("notizblock-pro.exe"), false),
            ("notiz.md", Some("notepad.exe"), true),
            ("notiz.md", Some("code.exe"), true),
            ("bild.svg", Some("msedge.exe"), false),
            ("bild.svg", Some("chrome.exe"), false),
            ("bild.svg", Some("photos.exe"), true),
            ("bild.svg", None, false),
            ("bauplan.pdf", Some("msedge.exe"), true),
            ("bauplan.pdf", Some("unbekannt.exe"), false),
            ("tabelle.xlsx", Some("excel.exe"), true),
            ("tabelle.xlsx", Some("cmd.exe"), false),
        ];
        for (datei, programm, geht) in faelle {
            let endung = datei.rsplit('.').next().unwrap().to_string();
            let programme = programm.map(|x| (endung.clone(), x.to_string())).into_iter().collect();
            let a = Arc::new(Aufnahme { programme: Some(programme), ..Default::default() });
            let antwort = frage(&kontext(&p, a.clone()), &p.dok(datei)).await;
            assert_eq!(matches!(antwort, Antwort::Geoeffnet), geht, "{datei} mit {programm:?}: {antwort:?}");
            assert_eq!(a.alle().len(), usize::from(geht));
            if !geht {
                assert!(satz(&antwort).contains("mit einem Programm verknüpft, das das Dashboard nicht öffnet"), "{antwort:?}");
            }
        }
        for e in ENDUNGEN {
            assert!(PROGRAMME.iter().any(|(x, l)| *x == e && !l.is_empty()), "{e} ohne Programme");
        }
        assert!(!PROGRAMME.iter().any(|(_, l)| l.iter().any(|p| ["cmd.exe", "powershell.exe", "rundll32.exe", "notizblock-pro.exe"].contains(p))));
    }

    /// „Öffnen“ nur vom Dienstkonto: ohne bekannten Client (`--einmal`), mit
    /// einem anderen Konto (etwa ein Chat an der Pipe) oder ohne dienst_sid
    /// in den Einstellungen: nie. Nichts geöffnet.
    #[tokio::test]
    async fn nur_vom_dienst() {
        let p = Platz::neu("bote-dienst");
        let a = Arc::new(Aufnahme::default());
        let k = kontext(&p, a.clone());
        let anfrage = || Anfrage::Oeffnen { pfad: p.dok("bauplan.pdf") };
        let ohne = ausfuehren(anfrage(), &k).await;
        assert_eq!(satz(&ohne), Abweisung::NichtVomDienst.satz());
        let chat = ausfuehren_von(anfrage(), &k, Some("S-1-5-21-1-2-3-1001".into())).await;
        assert_eq!(satz(&chat), Abweisung::NichtVomDienst.satz());
        let ohne_einstellung = Kontext { dienst_sid: None, ..kontext(&p, a.clone()) };
        assert!(abgelehnt(&ausfuehren_von(anfrage(), &ohne_einstellung, Some(DIENST.into())).await));
        assert!(a.alle().is_empty());
        assert!(matches!(ausfuehren_von(anfrage(), &k, Some(DIENST.into())).await, Antwort::Geoeffnet));
    }

    /// Die eigene Grenze des Boten: höchstens 10 je Minute, auch wenn der Dienst mehr schickte.
    #[tokio::test]
    async fn eigene_grenze_zehn_je_minute() {
        let p = Platz::neu("bote-grenze");
        let a = Arc::new(Aufnahme::default());
        let k = kontext(&p, a.clone());
        for i in 0..10 {
            let x = ausfuehren_von(Anfrage::Oeffnen { pfad: p.dok("bauplan.pdf") }, &k, Some(DIENST.into())).await;
            assert!(matches!(x, Antwort::Geoeffnet), "{i}: {x:?}");
        }
        let elfte = ausfuehren_von(Anfrage::Oeffnen { pfad: p.dok("bauplan.pdf") }, &k, Some(DIENST.into())).await;
        assert_eq!(satz(&elfte), Abweisung::ZuViele.satz());
        assert_eq!(a.alle().len(), 10);
    }

    /// In Unit-Tests öffnet `Kontext::jetzt()` nie etwas (Verweigerer), auch vom Dienst.
    #[tokio::test]
    async fn standard_im_test_oeffnet_nie() {
        let p = Platz::neu("bote-standard");
        let k = Kontext {
            profil: p.profil.clone(),
            quellen: vec![p.quelle.clone()],
            ausschluss: Some(p.liste.clone()),
            dienst_sid: Some(DIENST.into()),
            ..Kontext::jetzt()
        };
        assert!(abgelehnt(&frage(&k, &p.dok("bauplan.pdf")).await));
        assert_eq!(shell_aufruf(Path::new(r"C:\a b\c.pdf")).parameter, None);
        let (c, _) = explorer_befehl(Path::new(r"C:\a b\Ordner"), true).unwrap();
        assert!(c.get_program().to_string_lossy().to_lowercase().ends_with("explorer.exe"));
    }

    /// Das Argument nach den Regeln von CommandLineToArgvW zerlegt: genau ein
    /// Argument, ein Ordner genau mit einem `\` am Ende, auch mit Leerzeichen
    /// im Pfad; zip ohne `\`.
    #[cfg(windows)]
    #[test]
    fn argument_nach_commandlinetoargvw() {
        fn zerlegen(zeile: &str) -> Vec<String> {
            use windows_sys::Win32::Foundation::LocalFree;
            use windows_sys::Win32::UI::Shell::CommandLineToArgvW;
            let breit: Vec<u16> = zeile.encode_utf16().chain(std::iter::once(0)).collect();
            let mut n = 0i32;
            // SAFETY: nullterminierte Zeile; das Ergebnis wird mit LocalFree freigegeben.
            unsafe {
                let argv = CommandLineToArgvW(breit.as_ptr(), &mut n);
                assert!(!argv.is_null());
                let mut aus = Vec::new();
                for i in 0..n as usize {
                    let p = *argv.add(i);
                    let mut l = 0;
                    while *p.add(l) != 0 {
                        l += 1;
                    }
                    aus.push(String::from_utf16_lossy(std::slice::from_raw_parts(p, l)));
                }
                LocalFree(argv as *mut _);
                aus
            }
        }
        for (pfad, ordner, erwartet) in [
            (r"C:\a b\Ordner", true, r"C:\a b\Ordner\."),
            (r"C:\a b\Ordner\", true, r"C:\a b\Ordner\."),
            (r"C:\ohne\Werkzeugkiste", true, r"C:\ohne\Werkzeugkiste\."),
            (r"C:\a b\archiv.zip", false, r"C:\a b\archiv.zip"),
        ] {
            let zeile = format!(r"C:\Windows\explorer.exe {}", explorer_argument(Path::new(pfad), ordner));
            assert_eq!(zerlegen(&zeile), vec![r"C:\Windows\explorer.exe".to_string(), erwartet.to_string()], "{zeile}");
        }
    }

    /// Ob ein Ordner aufgeht, steht im Ergebnis der Prüfung: Verschwindet er
    /// danach (umbenannt, daneben etwa eine `Werkzeugkiste.cmd`), bekommt der
    /// Explorer trotzdem das `\.` am Ende; der Bote sieht nicht noch einmal nach.
    #[test]
    fn ordner_behaelt_den_rueckstrich_auch_wenn_er_danach_verschwindet() {
        let p = Platz::neu("bote-tausch");
        let a = Arc::new(Aufnahme::default());
        let k = kontext(&p, a.clone());
        let ordner = p.profil.join("Dokumente").join("Werkzeugkiste");
        let ziel = crate::oeffnen::pruefen(&p.text(&ordner), &regeln(&k)).unwrap();
        assert!(ziel.ist_ordner && ziel.art == Art::Ordner);
        std::fs::remove_dir(&ordner).unwrap();
        std::fs::write(p.profil.join("Dokumente").join("Werkzeugkiste.cmd"), "x").unwrap();
        assert!(matches!(ziel_oeffnen(&ziel, &k), Antwort::Geoeffnet));
        match &a.alle()[..] {
            [Aufruf::Ordner { argument, .. }] => assert!(argument.ends_with(r#"Werkzeugkiste\.""#), "{argument}"),
            x => panic!("{x:?}"),
        }
        // zip bleibt eine Datei im Explorer, ohne `\`.
        let zip = crate::oeffnen::pruefen(&p.dok("archiv.zip"), &regeln(&k)).unwrap();
        assert!(!zip.ist_ordner && zip.art == Art::Ordner);
    }
}
