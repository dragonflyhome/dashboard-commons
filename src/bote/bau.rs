//! Der Bau der Oberfläche im Boten: der Baumeister (Etappe 4b, vorher 4v).
//! Der Dienst darf ins Repo nicht schreiben; der Bote läuft als Inhaber und
//! darf es. Der Dienst sagt nur, *wann* gebaut wird (Beobachter, Vorgang
//! `bau`, Warteschlange in `baumeister`); was geprüft und gebaut wird,
//! entscheidet der Bote hier.
//!
//! Ablauf eines Baus ([`bauen`]):
//!
//! 1. **Prüfen, was sich geändert hat.** Je Projekt (aus allen Inhaltsquellen,
//!    `quellen`) ein Fingerabdruck seiner `.svelte`- und `.ts`-Dateien. Weicht
//!    er vom gemerkten Stand (`web/.projekte-gruen/stand.json`) ab, läuft die
//!    ganze Prüfung nur für dieses Projekt (`projektpruefung::pruefe_voll`:
//!    TOML, Assets, Scanner, Stil, ESLint mit `--cache`, svelte-check, Vitest).
//!    TOML allein löst nie eine Prüfung aus. Kleinkram 2: Zurückgestellte und
//!    ausgeschlossene Projekte prüft jeder Bau neu (`neu_pruefen`). Zweig A:
//!    Neben dem Inhalts-Abdruck ([`abdruck`], mit dem Elternordner) steht ein
//!    Umgebungs-Abdruck ([`umgebung`]: Fassung des Prüfers, Konfigurationen
//!    der Werkzeuge, die tsconfig-Teile von `$app`, für Projekte mit `*.spec.ts` die Alias-Ziele,
//!    alles über die Bytes). Ändert er sich, prüft der nächste Bau neu.
//!    Übersprungen heißt nicht geprüft: Läuft ein Werkzeug nicht
//!    (`projektpruefung::ist_uebersprungen`), wird das Projekt nicht grün,
//!    nicht rot, sondern `nicht_geprueft` (aus der Kopie gebaut oder weggelassen).
//!    Zweig B: Ab `SAMMEL_AB` Projekten läuft die Prüfung gesammelt
//!    (`projektpruefung::pruefe_sammel`: ein svelte-check über alle, Stylelint
//!    je Elternordner, Rückfall je Projekt nur bei Absturz oder Zeitlimit;
//!    ESLint und Vitest je Projekt),
//!    sonst je Projekt in einer Warteschlange. Was bis [`PRUEFUNG_HOECHSTENS`]
//!    nicht fertig würde, startet nicht und ist nicht geprüft.
//! 2. **Nur grüne Projekte.** Ein grünes Projekt kommt aus seinem Ordner in die
//!    Bauliste (`web/src/lib/projekte.generated.ts`, absolute Pfade, nur
//!    explizites `import()`). Ein rotes bleibt in seiner letzten grünen Kopie
//!    `web/.projekte-gruen/<slug>/`; gibt es keine, fehlt es im Bau.
//! 3. **Gesamtprüfung.** svelte-check über alles (Oberfläche und alle Projekte
//!    der Liste). Ist sie rot, wird der Verursacher zurückgestellt (aus seiner
//!    Kopie gebaut) oder fliegt raus, und sie läuft noch einmal, höchstens
//!    [`PRUEF_RUNDEN`] Runden ([`gesamtpruefung`]: je Fundort das geänderte
//!    Projekt mit dem Fund, sonst das geänderte, das ihn auslöst). Erst danach
//!    werden die Kopien der grünen Projekte erneuert.
//! 4. **`npm run build`** (vite), gleichzeitig mit der Gesamtprüfung (spart
//!    dem Inhaber etwa 9 s); die Fassung gilt erst, wenn die Gesamtprüfung grün ist,
//!    sonst wird der Ordner verworfen (und nach einem Ausschluss neu gebaut).
//!    Gebaut wird in einen neuen Ordner `web\builds\<n>\`
//!    (`DASHBOARD_BUILD_ZIEL`), mit [`ZEITLIMIT`], niedriger Priorität
//!    (`BELOW_NORMAL`) und ohne Fenster; nach dem Zeitlimit endet der ganze
//!    Baum (Job-Objekt). Liegt danach `index.html` im Ordner, wird
//!    `web\builds\aktuell` über eine neue Datei und Umbenennen ausgetauscht;
//!    alte Ordner bis auf [`BEHALTEN`] werden gelöscht (nur diese darf der
//!    Baumeister löschen). Bei rot bleibt die alte Fassung, der halbe Ordner geht.
//!
//! Das Einspielen (`scripts\Dienst.ps1 -Neustart`, über `dashboard pruefe
//! --bauliste --svelte`) fällt mit [`bauliste_einspielen`] dasselbe Urteil
//! (Schritte 1 bis 3, dieselben Kopien und derselbe `stand.json`); nur
//! `npm run build` macht Dienst.ps1 selbst, und gebaut wird dort nur aus den
//! grünen Kopien (`nur_aus_kopien`). Dienst.ps1 hält den Boten vorher an.
//!
//! Höchstens ein Bau zugleich (`EIN_BAU`); ein zweiter wartet. Bricht die
//! Prüfung selbst (Node oder `web/node_modules` fehlen), wird nicht gebaut:
//! Die letzte gute Fassung bleibt, der Satz sagt warum.

use std::collections::VecDeque;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tracing::{info, warn};

use crate::dienst::programme;
use crate::inhalt::seite;
use crate::quellen::Quellen;
use crate::vertrag::bote::{BauErgebnis, BauFortschritt, BauSchritt, FortschrittSchritt, RotesProjekt};
use crate::werkzeug::projektpruefung::{self, CheckFund};
use crate::werkzeug::{pruefe, scanner};

/// Zeitlimit für `npm run build` (vite).
pub const ZEITLIMIT: Duration = Duration::from_secs(120);
/// So lange dürfen die Prüfungen eines Baus zusammen höchstens dauern (für
/// das Zeitlimit der Anfrage, `aufgaben::BAU_HOECHSTENS`).
pub const PRUEFUNG_HOECHSTENS: Duration = Duration::from_secs(240);
/// So viele Bau-Ordner bleiben unter `web\builds` (der aktuelle eingeschlossen).
pub const BEHALTEN: usize = 2;
/// Der Zeiger: `web\builds\aktuell`.
pub const ZEIGER: &str = "aktuell";
/// Umgebungsvariable mit dem Zielordner für vite (`vite.config.ts`, adapter).
pub const UMGEBUNG_ZIEL: &str = "DASHBOARD_BUILD_ZIEL";
/// So viele Zeilen und Bytes vom Ende der Ausgabe gehen bei rot zurück.
pub const ENDE_ZEILEN: usize = 30;
pub const ENDE_BYTES: usize = 3000;
/// Ordner der letzten grünen Kopien, in `web/`.
pub const GRUEN_ORDNER: &str = ".projekte-gruen";
/// Der gemerkte Stand je Projekt, in [`GRUEN_ORDNER`].
pub const STAND_DATEI: &str = "stand.json";
/// Höchstens so viele Runden der Gesamtprüfung schließen ein Projekt aus.
pub const PRUEF_RUNDEN: usize = 2;
/// So viele Projekte prüft der Bote zugleich (Warteschlange, Zweig B).
pub const ZUGLEICH: usize = projektpruefung::ZUGLEICH;
/// Ändert sich die Prüfung selbst, prüft der Bote alle Projekte neu. Kleinkram 2:
/// nicht mehr von Hand gezählt (die 1 blieb, als der Status `archiviert` kam),
/// sondern die Fassung des Prüfers aus seinen Quellen.
const PRUEF_FASSUNG: u64 = crate::werkzeug::PRUEFER_FASSUNG;

/// Höchstens ein Bau zugleich, auch wenn mehrere Anfragen kommen.
static EIN_BAU: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Bau-Tempo, Zweig C: Stand des laufenden Baus (`Anfrage::Fortschritt`).
/// Schreibt nur [`bauen`] (unter `EIN_BAU`); der direkte Weg der Edge-Probe
/// liest denselben Zähler.
pub static FORTSCHRITT: Fortschritt = Fortschritt(Mutex::new(None));

/// Ein Fortschritt: Schritt, fertig, gesamt; `None` ohne Bau.
pub struct Fortschritt(Mutex<Option<BauFortschritt>>);

impl Fortschritt {
    pub fn jetzt(&self) -> Option<BauFortschritt> {
        *self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn setzen(&self, f: Option<BauFortschritt>) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = f;
    }

    fn schritt(&self, schritt: FortschrittSchritt, gesamt: usize) {
        self.setzen(Some(BauFortschritt { schritt, fertig: 0, gesamt: u32::try_from(gesamt).unwrap_or(u32::MAX) }));
    }

    /// Ein Projekt der Warteschlange ist fertig.
    fn zaehlen(&self) {
        if let Some(f) = self.0.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            f.fertig = (f.fertig + 1).min(f.gesamt);
        }
    }
}

/// Leert den Fortschritt, wenn [`bauen`] endet, auf jedem Weg.
struct FortschrittLeeren;

impl Drop for FortschrittLeeren {
    fn drop(&mut self) {
        FORTSCHRITT.setzen(None);
    }
}

/// Wo gebaut wird.
#[derive(Debug, Clone)]
pub struct BauOrt {
    /// Das Repo (`repo` aus dienst.toml).
    pub repo: PathBuf,
    /// Die Projektseiten (`pfade.projekte`), der erste Ordner.
    pub projekte: PathBuf,
    /// Etappe 4b: weitere Inhaltsquellen (leer: nur `projekte`).
    pub quellen: Vec<PathBuf>,
}

impl BauOrt {
    /// Nur ein Projektordner, wie vor 4b.
    pub fn neu(repo: PathBuf, projekte: PathBuf) -> BauOrt {
        BauOrt { repo, projekte, quellen: Vec::new() }
    }

    pub fn web(&self) -> PathBuf {
        self.repo.join("web")
    }

    /// `<repo>\web\builds`.
    pub fn builds(&self) -> PathBuf {
        self.web().join("builds")
    }

    /// `<repo>\web\.projekte-gruen`.
    pub fn gruen(&self) -> PathBuf {
        self.web().join(GRUEN_ORDNER)
    }

    /// Alle Inhaltsquellen, frisch von der Platte.
    pub fn alle_quellen(&self) -> Quellen {
        if self.quellen.len() > 1 { Quellen::aus_liste(self.quellen.clone()) } else { Quellen::ein_ordner(self.projekte.clone()) }
    }
}

/// Ist `name` ein Ordnername, den der Bau vergibt (`JJJJMMTT-hhmmss`, bei
/// mehreren in derselben Sekunde mit `-2`, `-3` …)? Nur solche räumt er auf.
pub fn ist_bau_name(name: &str) -> bool {
    let b = name.as_bytes();
    let kopf = b.len() >= 15 && b[..8].iter().all(u8::is_ascii_digit) && b[8] == b'-' && b[9..15].iter().all(u8::is_ascii_digit);
    if !kopf {
        return false;
    }
    match &name[15..] {
        "" => true,
        rest => rest.strip_prefix('-').is_some_and(|n| !n.is_empty() && n.len() <= 3 && n.bytes().all(|c| c.is_ascii_digit())),
    }
}

/// Ein neuer, noch freier Ordnername für jetzt.
fn neuer_name(builds: &Path) -> String {
    let basis = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    if !builds.join(&basis).exists() {
        return basis;
    }
    (2..1000).map(|n| format!("{basis}-{n}")).find(|n| !builds.join(n).exists()).unwrap_or(basis)
}

/// Schreibt den Zeiger: erst `aktuell.neu`, dann Umbenennen auf `aktuell`
/// (ersetzt die alte Datei in einem Schritt).
pub fn zeiger_schreiben(builds: &Path, name: &str) -> std::io::Result<()> {
    let neu = builds.join(format!("{ZEIGER}.neu"));
    std::fs::write(&neu, format!("{name}\n"))?;
    std::fs::rename(&neu, builds.join(ZEIGER))
}

/// Räumt alte Bau-Ordner weg: behält die `behalten` neuesten (nach Namen,
/// also nach Zeit) und immer `aktuell`. Nur echte Ordner mit Bau-Namen,
/// keine Verknüpfungen. Gibt die Zahl der entfernten Ordner zurück.
pub fn aufraeumen(builds: &Path, behalten: usize, aktuell: &str) -> usize {
    let Ok(eintraege) = std::fs::read_dir(builds) else { return 0 };
    let mut namen: Vec<String> = eintraege
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir() && !t.is_symlink()))
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| ist_bau_name(n))
        .collect();
    namen.sort();
    let weg = namen.len().saturating_sub(behalten);
    let mut entfernt = 0;
    for name in namen.into_iter().take(weg).filter(|n| n != aktuell) {
        let pfad = builds.join(&name);
        if seite::ist_verknuepfung(&pfad) {
            continue;
        }
        match std::fs::remove_dir_all(&pfad) {
            Ok(()) => entfernt += 1,
            Err(e) => warn!("Bau: alter Ordner {} nicht entfernt: {e}", pfad.display()),
        }
    }
    entfernt
}

/// Das Ende einer Ausgabe: die letzten Zeilen, ohne Farbcodes.
#[derive(Default)]
struct Ende {
    zeilen: VecDeque<String>,
}

impl Ende {
    fn neu() -> Arc<Mutex<Ende>> {
        Arc::new(Mutex::new(Ende::default()))
    }

    fn dazu(&mut self, zeile: &str) {
        let zeile = ohne_farben(zeile.trim_end());
        if zeile.trim().is_empty() {
            return;
        }
        self.zeilen.push_back(zeile);
        while self.zeilen.len() > ENDE_ZEILEN {
            self.zeilen.pop_front();
        }
    }

    /// Höchstens `ENDE_BYTES`; geschnitten wird vorn, an einer Zeichengrenze.
    fn text(&self) -> String {
        let alles = self.zeilen.iter().cloned().collect::<Vec<_>>().join("\n");
        if alles.len() <= ENDE_BYTES {
            return alles;
        }
        let mut ab = alles.len() - ENDE_BYTES;
        while !alles.is_char_boundary(ab) {
            ab += 1;
        }
        format!("…{}", &alles[ab..])
    }
}

/// Entfernt ANSI-Steuerfolgen (`ESC [ … Buchstabe`), wie vite sie für Farben schreibt.
fn ohne_farben(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut z = s.chars().peekable();
    while let Some(c) = z.next() {
        if c == '\u{1b}' {
            if z.peek() == Some(&'[') {
                z.next();
                for d in z.by_ref() {
                    if d.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        if !c.is_control() || c == '\t' {
            out.push(c);
        }
    }
    out
}

/// Liest einen Strom zeilenweise ins gemeinsame Ende.
fn mitlesen<R: AsyncRead + Unpin + Send + 'static>(quelle: Option<R>, ende: Arc<Mutex<Ende>>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let Some(q) = quelle else { return };
        // Bytes statt Text: eine Zeile, die kein UTF-8 ist, darf das Lesen
        // nicht beenden (sonst liefe die Pipe voll und npm bliebe stehen).
        let mut leser = BufReader::new(q);
        let mut zeile = Vec::new();
        loop {
            zeile.clear();
            match leser.read_until(b'\n', &mut zeile).await {
                Ok(0) | Err(_) => break,
                Ok(_) => ende.lock().unwrap_or_else(|e| e.into_inner()).dazu(&String::from_utf8_lossy(&zeile)),
            }
        }
    })
}

// ----- Gemerkter Stand je Projekt -----

/// Was der Bote je Projekt zuletzt gesehen hat (`stand.json`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Stand {
    #[serde(default)]
    pub projekte: BTreeMap<String, ProjektStand>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjektStand {
    /// Inhalts-Abdruck ([`abdruck`]) bei der letzten Prüfung.
    pub abdruck: u64,
    /// Zweig A: Umgebungs-Abdruck ([`umgebung_fuer`]) bei der letzten
    /// Prüfung. Fehlt in älteren Ständen (0): Der nächste Bau prüft neu.
    #[serde(default)]
    pub umgebung: u64,
    pub gruen: bool,
    /// Bei rot: der Satz mit dem ersten Fund.
    #[serde(default)]
    pub satz: String,
    /// Etappe 4c: Wann die letzte grüne Kopie entstand (Unix-Sekunden). Bei
    /// rot bleibt die Zeit der letzten guten Fassung stehen. Fehlt in Ständen
    /// von vor 4c; dann gilt die Zeit des Kopie-Ordners (`baustand`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gruen_um: Option<i64>,
    /// Etappe 4c: Warum rot (für den Satz ohne Pfade in der Oberfläche).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rot_art: Option<RotArt>,
    /// Wann die Seite zuletzt in einen geltenden
    /// Bau kam (Unix-Sekunden), bei jedem grünen Bau neu gesetzt, auch ohne
    /// Änderung an der Seite und auch aus der grünen Kopie. Die Oberfläche zeigt
    /// es nicht mehr; frei als `gebaut_um` für Chats in
    /// `/api/projects`; `gruen_um` bleibt für „in Ordnung seit“. Fehlt in
    /// älteren Ständen und bei Seiten, die nie im Bau waren.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gebaut_um: Option<i64>,
    /// Nachbesserung Zweig A: Inhalts-Abdruck der letzten grünen Fassung, auch
    /// wenn der Eintrag jetzt nicht grün ist (`abdruck` ist dann der zuletzt
    /// geprüfte). Fehlt in älteren Ständen ([`ProjektStand::letzter_gruener`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gruen_abdruck: Option<u64>,
    /// Nachtrag Tempo B: Wann der Bau das Projekt zuletzt zu Ende prüfte
    /// (grün, rot oder ein eigenes Werkzeugproblem; nicht, wenn die Frist es
    /// verdrängte). Mit `gruen_um` die Reihenfolge der Prüfung ([`pruefen_mit`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub versucht_um: Option<i64>,
}

impl ProjektStand {
    /// Der Inhalts-Abdruck der letzten grünen Fassung (`None`: keine bekannt).
    pub fn letzter_gruener(&self) -> Option<u64> {
        if self.gruen { Some(self.abdruck) } else { self.gruen_abdruck }
    }
}

/// Warum ein Projekt rot ist (Etappe 4c, `stand.json`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RotArt {
    /// Die Prüfung je Projekt hat einen Fehler gefunden.
    Pruefung,
    /// Die neue Fassung passt nicht zum Rest (Gesamtprüfung); gebaut ist die
    /// letzte grüne Kopie (Abnahme 4c, Auflage aus 4b).
    Zusammenbau,
    /// Die Gesamtprüfung hat das Projekt ganz ausgeschlossen (keine grüne
    /// Kopie, oder auch die Kopie passte nicht): Die Seite fehlt im Bau.
    Ausgeschlossen,
    /// Zweig A: Ein Prüfwerkzeug ist übersprungen (fehlt, bricht ab,
    /// Zeitlimit), Fehler gab es keine. Die neue Fassung ist nicht geprüft:
    /// Gebaut wird die grüne Kopie, ohne Kopie fehlt die Seite. Kein Rot,
    /// keine Meldung; jeder Bau prüft neu ([`neu_pruefen`]).
    #[serde(rename = "nicht_geprueft")]
    NichtGeprueft,
}

/// Jetzt in Unix-Sekunden (für `gruen_um` und `gebaut_um`).
fn jetzt_sekunden() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

impl Stand {
    pub fn laden(gruen: &Path) -> Stand {
        std::fs::read_to_string(gruen.join(STAND_DATEI)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    pub fn speichern(&self, gruen: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(gruen)?;
        let neu = gruen.join(format!("{STAND_DATEI}.neu"));
        std::fs::write(&neu, serde_json::to_string_pretty(self).unwrap_or_default())?;
        std::fs::rename(&neu, gruen.join(STAND_DATEI))
    }
}

/// FNV-1a über Bytes, für beide Abdrücke.
struct Fnv(u64);

impl Fnv {
    fn neu(start: u64) -> Fnv {
        Fnv(0xcbf2_9ce4_8422_2325 ^ start)
    }

    fn dazu(&mut self, b: &[u8]) {
        for x in b {
            self.0 ^= u64::from(*x);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }
}

/// Inhalts-Abdruck eines Projekts: sein Elternordner (ein verschobenes
/// Projekt wird neu geprüft) und Pfade, Größen und Zeiten seiner `.svelte`-
/// und `.ts`-Dateien. Nur diese Dateien lösen eine Prüfung aus; was um das
/// Projekt herum gilt (Prüfer, Werkzeuge), steht im [`umgebung`]s-Abdruck.
///
/// Bewusst nicht drin (Zweig A, Punkt 7): `projekt.toml`, `daten/`,
/// Markdown und `assets/`. Der Bau-Stand prüft nur Code; TOML und Assets
/// prüfen `pruefe` und der Server beim Lesen. „Grün“ im Stand heißt also
/// „Code geprüft“, nicht „alles am Projekt geprüft“.
pub fn abdruck(eltern: &Path, slug: &str) -> u64 {
    let mut h = Fnv::neu(0);
    h.dazu(projektpruefung::vergleichbar(eltern).as_bytes());
    h.dazu(b"\0");
    for (rel, pfad) in scanner::code_dateien(eltern, slug) {
        h.dazu(rel.as_bytes());
        if let Ok(m) = std::fs::metadata(&pfad) {
            h.dazu(&m.len().to_le_bytes());
            let zeit = m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_nanos());
            h.dazu(&zeit.to_le_bytes());
        }
    }
    h.0
}

/// Die Dateien in `web/`, von denen das Urteil der Prüfung je Projekt
/// abhängt (Konfigurationen der Werkzeuge und was sie laden).
// shortcut: feste Liste ohne transitive lib-Ziele außerhalb der ALIAS_ORDNER und ohne .svelte-kit/types; aufrüsten, wenn ein Projekt-Urteil davon abhängt.
pub const UMGEBUNG_DATEIEN: &[&str] = &[
    "eslint.projekte.config.js",
    "eslint.config.js",
    "stylelint.config.js",
    "scripts/stil-projekte.mjs",
    "vitest.projekte.config.ts",
    "vitest-setup.ts",
    "tsconfig.json",
    "node_modules/.package-lock.json",
    "node_modules/$app/tsconfig.json",
];
/// Ordner in `web/`, deren Dateien ganz zählen: die tsconfig-Teile aus
/// `svelte-kit sync` (dazu `node_modules/$app/tsconfig.json` oben).
// shortcut: nur die tsconfig-Teile von $app, nicht $app/types (dort stehen nur Routen-Deklarationen, Projekte importieren $app nicht, Typfehler fängt die Gesamtprüfung); aufrüsten, sobald ein Projekt $app importiert.
pub const UMGEBUNG_ORDNER: &[&str] = &["node_modules/$app/tsconfig"];
/// Die Alias-Ziele der Projekte (`$vertrag`, `$katalog`, `$projekt`), in
/// `web/`. Sie zählen nur für Projekte mit `*.spec.ts`: Typfehler daraus
/// fängt die Gesamtprüfung bei jedem Bau, ESLint und Stylelint hängen nicht
/// daran, nur Vitest lädt sie zur Laufzeit.
// shortcut: nur diese Ordner, nicht ihre transitiven Importe (projekt/laden.svelte.ts holt ../api/client.ts usw., dazu vendor); heute hat kein Projekt einen Spec; aufrüsten, sobald das erste Projekt einen *.spec.ts bekommt.
pub const ALIAS_ORDNER: &[&str] = &["src/lib/vertrag", "src/lib/katalog", "src/lib/projekt"];

/// Alle Dateien unter `ordner` (rekursiv, ohne Verknüpfungen), sortiert.
fn dateien_unter(ordner: &Path) -> Vec<PathBuf> {
    let mut alle = Vec::new();
    let mut offen = vec![ordner.to_path_buf()];
    while let Some(o) = offen.pop() {
        let Ok(rd) = std::fs::read_dir(&o) else { continue };
        for e in rd.filter_map(Result::ok) {
            match e.file_type() {
                Ok(t) if t.is_dir() => offen.push(e.path()),
                Ok(t) if t.is_file() => alle.push(e.path()),
                _ => {}
            }
        }
    }
    alle.sort();
    alle
}

/// Umgebungs-Abdruck eines Baus, über die **Bytes** (nicht die Zeit; ein
/// Zweigwechsel ohne Inhaltsänderung löst nichts aus): die Fassung des
/// Prüfers, [`UMGEBUNG_DATEIEN`] und [`UMGEBUNG_ORDNER`]. `[ohne, mit]`:
/// der zweite Wert nimmt die [`ALIAS_ORDNER`] (ohne `*.spec.ts`) dazu, für
/// Projekte mit `*.spec.ts` ([`umgebung_fuer`]). Ändert er sich, prüft der
/// nächste Bau die Projekte neu ([`neu_pruefen`]). Eine fehlende Datei zählt
/// als eigener Wert.
pub fn umgebung(web: &Path) -> [u64; 2] {
    fn datei(h: &mut Fnv, rel: &str, pfad: &Path) {
        h.dazu(rel.as_bytes());
        match std::fs::read(pfad) {
            Ok(b) => {
                h.dazu(&(b.len() as u64).to_le_bytes());
                h.dazu(&b);
            }
            Err(_) => h.dazu(b"\0fehlt\0"),
        }
    }
    let unter = |h: &mut Fnv, ordner: &[&str]| {
        for o in ordner {
            let wurzel = web.join(o);
            h.dazu(o.as_bytes());
            for p in dateien_unter(&wurzel).into_iter().filter(|p| !p.to_string_lossy().ends_with(".spec.ts")) {
                let rel = p.strip_prefix(&wurzel).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default();
                datei(h, &rel, &p);
            }
        }
    };
    let mut h = Fnv::neu(PRUEF_FASSUNG);
    for rel in UMGEBUNG_DATEIEN {
        datei(&mut h, rel, &web.join(rel));
    }
    unter(&mut h, UMGEBUNG_ORDNER);
    let ohne = h.0;
    unter(&mut h, ALIAS_ORDNER);
    [ohne, h.0]
}

/// Der Umgebungs-Abdruck für ein Projekt: mit `*.spec.ts` der mit den Alias-Zielen.
pub fn umgebung_fuer(eltern: &Path, slug: &str, u: [u64; 2]) -> u64 {
    if scanner::code_dateien(eltern, slug).iter().any(|(r, _)| r.ends_with(".spec.ts")) { u[1] } else { u[0] }
}

/// Ist das Projekt noch so wie beim Abdruck `vorher` (vor der Prüfung)?
/// Nur dann darf seine grüne Kopie erneuert werden (Abnahme 4b). Vergleicht
/// den Inhalts-Abdruck: Eine Änderung nur an der Umgebung erneuert die Kopie.
pub fn noch_derselbe(eltern: &Path, slug: &str, vorher: u64) -> bool {
    abdruck(eltern, slug) == vorher
}

/// Erneuert die grüne Kopie `<gruen>/<slug>` aus `quelle`: nur `.svelte`
/// und `.ts` (mehr bindet eine Seite nicht ein). Erst in `<slug>.neu`, dann
/// getauscht; die alte Kopie ist danach weg (sie gehört nur dem Baumeister).
pub fn kopie_erneuern(eltern: &Path, slug: &str, gruen: &Path) -> std::io::Result<()> {
    let ziel = gruen.join(slug);
    let neu = gruen.join(format!("{slug}.neu"));
    let alt = gruen.join(format!("{slug}.alt"));
    for o in [&neu, &alt] {
        if o.exists() && !seite::ist_verknuepfung(o) {
            std::fs::remove_dir_all(o)?;
        }
    }
    for (rel, pfad) in scanner::code_dateien(eltern, slug) {
        let z = neu.join(&rel);
        if let Some(e) = z.parent() {
            std::fs::create_dir_all(e)?;
        }
        std::fs::copy(&pfad, &z)?;
    }
    std::fs::create_dir_all(&neu)?;
    if ziel.exists() {
        if seite::ist_verknuepfung(&ziel) {
            return Err(std::io::Error::other(format!("{} ist eine Verknüpfung", ziel.display())));
        }
        std::fs::rename(&ziel, &alt)?;
    }
    std::fs::rename(&neu, &ziel)?;
    if alt.exists() {
        let _ = std::fs::remove_dir_all(&alt);
    }
    Ok(())
}

/// Der Satz für ein rotes Projekt: der erste Fehler, in einfachem Deutsch.
pub fn satz_rot(slug: &str, fehler: &[crate::inhalt::pruefung::Befund], hat_kopie: bool) -> String {
    let wie = if hat_kopie { "bleibt in ihrer letzten guten Fassung" } else { "fehlt, bis sie in Ordnung ist" };
    let erster = fehler.first().map(|b| format!(" Erster Fund: {b}")).unwrap_or_default();
    let mehr = if fehler.len() > 1 { format!(" (dazu {} weitere)", fehler.len() - 1) } else { String::new() };
    format!("Die Seite von „{slug}“ hat einen Fehler und {wie}.{erster}{mehr}")
}

// ----- Gesamtprüfung -----

/// Ergebnis der Gesamtprüfung.
#[derive(Debug, Clone, PartialEq)]
pub struct Gesamt {
    /// Was gebaut wird: Slug und Ordner (Quelle oder Kopie).
    pub eintraege: Vec<(String, PathBuf)>,
    /// Ganz ausgeschlossen (fehlt im Bau), mit dem Fund, der es ausschloss.
    pub ausgeschlossen: Vec<(String, String)>,
    /// Abnahme 4c (Auflage aus 4b): auf die letzte grüne Kopie zurückgesetzt
    /// (gebaut wird die Kopie), mit dem Fund der neuen Fassung.
    pub zurueck: Vec<(String, String)>,
    /// Rot auch nach den Runden, oder die Prüfung lief nicht: Grund.
    pub fehler: Option<String>,
    /// Wie viele Läufe von svelte-check.
    pub laeufe: usize,
    /// Kleinkram 2: Zurückgestellt oder ausgeschlossen, obwohl der Fund in
    /// einem anderen, unveränderten Projekt lag: Slug des Verursachers → Slug
    /// des Projekts mit dem Fund (die Änderung des Verursachers bricht es).
    pub ausloeser: BTreeMap<String, String>,
}

impl Gesamt {
    /// Musste die Gesamtprüfung eingreifen (ausschließen oder zurücksetzen)?
    pub fn eingegriffen(&self) -> bool {
        !self.ausgeschlossen.is_empty() || !self.zurueck.is_empty()
    }
}

/// Anfang von `Gesamt::fehler`, wenn die Gesamtprüfung lief und rot blieb
/// (sonst lief sie nicht).
const GESAMT_ROT: &str = "Die Gesamtprüfung bleibt rot";

/// svelte-check über alles; bei rot greift sie beim Projekt der ersten
/// Meldung ein, höchstens [`PRUEF_RUNDEN`] Mal: Gibt es eine grüne Kopie, die
/// noch nicht gebaut wird (`kopie(slug, ordner)`), wird das Projekt aus ihr
/// gebaut (Abnahme 4c); sonst, oder passt auch die Kopie nicht, fliegt es
/// raus. `pruefen` bekommt die Einträge und liefert die Funde (oder den
/// Grund, warum es nicht lief).
///
/// Abnahme 4c (Nachbesserung 2): Eingegriffen wird zuerst bei den Projekten
/// aus `geaendert` (in dieser Runde geändert und für sich grün; ein
/// unverändertes, bisher grünes Projekt bricht die Gesamtprüfung nur durch
/// eine Änderung anderswo): erst ein geändertes mit eigener Meldung, sonst
/// ein geändertes, bei dem noch nicht eingegriffen wurde (svelte-check meldet
/// einen Konflikt oft nur bei einem der beiden, etwa der späteren
/// Erklärung). So trifft ein Konflikt das geänderte Projekt, nicht das, dessen
/// Name vorn oder hinten steht. Erst wenn kein geändertes Projekt mehr in
/// Frage kommt, gilt das Projekt der ersten Meldung.
///
/// Kleinkram 2 (falscher Schuldiger in einem früheren Bau): Geändert heißt nicht
/// nur „in dieser Runde geprüft“ (`geaendert`), sondern auch „seit der letzten
/// grünen Fassung anders“ (`seit_gruen(slug, ordner)`, der Abdruck; etwa ein
/// Projekt, das während des Baus gespeichert wurde). Je Fundort: Liegt der
/// Fund in einem geänderten Projekt B, wird B zurückgestellt. Liegt er in
/// einem unveränderten B, ist die Änderung eines anderen, geänderten
/// Projekts A der Auslöser (etwa ein geteilter Typ); dann trifft es A, und
/// `ausloeser` merkt sich A → B für den Satz. Ein Projekt, das schon aus
/// seiner Kopie gebaut wird, gilt als unverändert.
pub fn gesamtpruefung(
    mut eintraege: Vec<(String, PathBuf)>,
    mut pruefen: impl FnMut(&[(String, PathBuf)]) -> Result<Vec<CheckFund>, String>,
    kopie: impl Fn(&str, &Path) -> Option<PathBuf>,
    geaendert: &BTreeSet<String>,
    seit_gruen: impl Fn(&str, &Path) -> bool,
) -> Gesamt {
    let mut ausgeschlossen: Vec<(String, String)> = Vec::new();
    let mut zurueck: Vec<(String, String)> = Vec::new();
    let mut ausloeser: BTreeMap<String, String> = BTreeMap::new();
    let mut laeufe = 0;
    loop {
        laeufe += 1;
        let funde = match pruefen(&eintraege) {
            Ok(f) => f,
            Err(satz) => return Gesamt { eintraege, ausgeschlossen, zurueck, fehler: Some(satz), laeufe, ausloeser },
        };
        let fehler: Vec<CheckFund> = funde.into_iter().filter(|f| f.befund.ist_fehler()).collect();
        // Geändert: im Bau, nicht schon zurückgestellt, und in dieser Runde
        // geprüft oder seit der letzten grünen Fassung anders.
        let veraendert = |slug: &str| {
            !zurueck.iter().any(|(s, _)| s == slug) && eintraege.iter().find(|(s, _)| s == slug).is_some_and(|(s, o)| geaendert.contains(s) || seit_gruen(s, o))
        };
        // 1. Ein Fund in einem geänderten Projekt: es selbst.
        let selbst = fehler.iter().find(|f| veraendert(&f.slug)).map(|f| (f.slug.clone(), f.befund.clone(), None));
        // 2. Sonst liegt der erste Fund in einem unveränderten Projekt: Auslöser ist ein geändertes.
        let ausgeloest = || {
            let erste = fehler.first()?;
            let (slug, _) = eintraege.iter().find(|(s, _)| *s != erste.slug && veraendert(s))?;
            Some((slug.clone(), erste.befund.clone(), Some(erste.slug.clone())))
        };
        // 3. Sonst, wie früher, das Projekt der ersten Meldung.
        let Some((schuldig, befund, fund_bei)) = selbst.or_else(ausgeloest).or_else(|| fehler.first().map(|f| (f.slug.clone(), f.befund.clone(), None))) else {
            return Gesamt { eintraege, ausgeschlossen, zurueck, fehler: None, laeufe, ausloeser };
        };
        let eingriffe = ausgeschlossen.len() + zurueck.len();
        let stelle = eintraege.iter().position(|(s, _)| *s == schuldig);
        let Some(stelle) = stelle.filter(|_| eingriffe < PRUEF_RUNDEN) else {
            let satz = format!("{GESAMT_ROT}, auch nach {eingriffe} ausgeschlossenen oder zurückgesetzten Projekten. Erster Fund: {befund}");
            return Gesamt { eintraege, ausgeschlossen, zurueck, fehler: Some(satz), laeufe, ausloeser };
        };
        match fund_bei {
            Some(b) => {
                info!("Bau: Der Fund liegt in „{b}“ (unverändert); Auslöser ist die Änderung an „{schuldig}“.");
                ausloeser.insert(schuldig.clone(), b);
            }
            None => {
                ausloeser.remove(&schuldig);
            }
        }
        let schon_zurueck = zurueck.iter().any(|(s, _)| *s == schuldig);
        match kopie(&schuldig, &eintraege[stelle].1).filter(|_| !schon_zurueck) {
            Some(k) => {
                eintraege[stelle].1 = k;
                zurueck.push((schuldig, befund.to_string()));
            }
            None => {
                eintraege.remove(stelle);
                zurueck.retain(|(s, _)| *s != schuldig);
                ausgeschlossen.push((schuldig, befund.to_string()));
            }
        }
    }
}

/// Kleinkram 2: Der Satz für ein zurückgestelltes (`ausgeschlossen` false)
/// oder ausgeschlossenes Projekt. `fund_bei`: Der Fund lag in einem anderen,
/// unveränderten Projekt; dann nennt der Satz beide (Verursacher und Fundort).
pub fn satz_zusammenbau(slug: &str, fund: &str, fund_bei: Option<&str>, ausgeschlossen: bool) -> String {
    let wie = if ausgeschlossen { "ist in diesem Bau nicht dabei" } else { "in diesem Bau steht ihre letzte gute Fassung" };
    match fund_bei {
        Some(b) if ausgeschlossen => {
            format!("Die Seite von „{slug}“ passt nicht zum Rest: Ihre Änderung bricht die unveränderte Seite „{b}“; sie {wie}. Fund: {fund}")
        }
        Some(b) => format!("Die Seite von „{slug}“ passt nicht zum Rest: Ihre Änderung bricht die unveränderte Seite „{b}“; {wie}. Fund: {fund}"),
        None if ausgeschlossen => format!("Die Seite von „{slug}“ passt nicht zum Rest und {wie}. Fund: {fund}"),
        None => format!("Die Seite von „{slug}“ passt nicht zum Rest; {wie}. Fund: {fund}"),
    }
}

/// Die grüne Kopie von `slug` unter `gruen`, wenn es sie gibt und sie nicht
/// schon `ordner` ist (dann gibt es nichts mehr zurückzusetzen).
pub fn gruene_kopie(gruen: &Path, slug: &str, ordner: &Path) -> Option<PathBuf> {
    let k = gruen.join(slug);
    (k != ordner && k.join("Seite.svelte").is_file() && !seite::ist_verknuepfung(&k)).then_some(k)
}

// ----- Der Bau -----

/// Was vor `npm run build` feststeht.
pub(crate) struct Vorbereitet {
    /// Gebaut wird mit diesen (Slug, Ordner).
    pub(crate) eintraege: Vec<(String, PathBuf)>,
    pub(crate) neu_rot: Vec<RotesProjekt>,
    pub(crate) alte_fassung: Vec<String>,
    /// Ohne Seite in diesem Bau (rot, ohne Kopie).
    pub(crate) rote: Vec<String>,
    /// Zweig A: nicht geprüft (ein Werkzeug übersprungen), mit Kopie aus ihr
    /// gebaut, sonst ohne Seite; weder rot noch „alte Fassung“ (keine Meldung).
    pub(crate) nicht_geprueft: Vec<String>,
    pub(crate) stil: Vec<crate::inhalt::pruefung::Befund>,
    pub(crate) schritte: Vec<BauSchritt>,
    /// Die Prüfung ließ sich nicht machen, oder die Gesamtprüfung blieb rot.
    pub(crate) gescheitert: Option<String>,
    /// Nur Einspiel-Weg: Die Gesamtprüfung blieb rot (Grund); gebaut wird aus
    /// den letzten grünen Kopien.
    pub(crate) gesamt_rot: Option<String>,
    /// Die Bauliste wurde neu geschrieben.
    pub(crate) liste_neu: bool,
}

fn schritt(schritte: &mut Vec<BauSchritt>, name: &str, seit: Instant) {
    schritte.push(BauSchritt { name: name.to_string(), ms: seit.elapsed().as_millis() as u64 });
}

/// Fehlt etwas, ohne das sich nicht prüfen lässt? Dann der Satz.
fn werkzeuge_fehlen(web: &Path) -> Option<String> {
    if programme::pfad_von(programme::Werkzeug::Node).is_none() {
        return Some(programme::fehlt_satz(programme::Werkzeug::Node));
    }
    // Zweig A: auch Stylelint und Vitest (sonst wäre jedes Projekt nur „nicht geprüft“).
    for (paket, datei) in [
        ("svelte-check", "bin/svelte-check"),
        ("eslint", "bin/eslint.js"),
        ("stylelint", "package.json"),
        ("postcss-html", "package.json"),
        ("vitest", "vitest.mjs"),
    ] {
        if !web.join("node_modules").join(paket).join(datei).is_file() {
            return Some(format!("{paket} ist nicht da (web/node_modules fehlt)"));
        }
    }
    for datei in [projektpruefung::ESLINT_KONFIG, projektpruefung::VITEST_KONFIG, crate::werkzeug::stil::STIL_SKRIPT] {
        if !web.join(datei).is_file() {
            return Some(format!("web/{datei} fehlt"));
        }
    }
    None
}

/// Wird ein Projekt in diesem Bau je Projekt geprüft? Ja, wenn es noch nie
/// geprüft ist oder sein Inhalts- oder Umgebungs-Abdruck sich geändert hat
/// (Zweig A). Kleinkram 2 (Zurückgestellt
/// klebt): auch ohne Dateiänderung, wenn die Gesamtprüfung es zurückgestellt
/// (`zusammenbau`) oder ausgeschlossen hat. Sein Urteil hing nicht an ihm
/// allein, sondern am Rest, und der kann inzwischen passen; ist dann auch die
/// Gesamtprüfung grün, ist es nicht mehr zurückgestellt. Rot aus der eigenen
/// Prüfung (`pruefung`) bleibt bis zur nächsten Änderung. Nicht geprüft
/// (ein Werkzeug übersprungen) prüft jeder Bau neu.
// shortcut: ein Rot aus der eigenen Prüfung wird nach einer Änderung an den ALIAS_ORDNER (etwa $katalog) ohne *.spec.ts nicht neu geprüft (es bleibt rot, nie falsch grün); aufrüsten, wenn so ein Rot zu lange stehen bleibt.
pub fn neu_pruefen(vorher: Option<&ProjektStand>, abdruck_jetzt: u64, umgebung_jetzt: u64) -> bool {
    match vorher {
        None => true,
        Some(p) => {
            p.abdruck != abdruck_jetzt
                || p.umgebung != umgebung_jetzt
                || (!p.gruen && matches!(p.rot_art, Some(RotArt::Zusammenbau | RotArt::Ausgeschlossen | RotArt::NichtGeprueft)))
        }
    }
}

/// Der Satz für ein nicht geprüftes Projekt (im Stand und im Protokoll):
/// welches Werkzeug übersprungen ist, und was gebaut wird.
pub fn satz_nicht_geprueft(slug: &str, uebersprungen: &[crate::inhalt::pruefung::Befund], hat_kopie: bool) -> String {
    let wie = if hat_kopie { "In diesem Bau steht ihre letzte gute Fassung" } else { "Sie ist in diesem Bau nicht dabei" };
    let grund = uebersprungen.first().map(|b| format!(" {}", b.text)).unwrap_or_default();
    format!("Die neue Fassung der Seite von „{slug}“ ist nicht geprüft, weil ein Prüfwerkzeug nicht lief. {wie}; der nächste Bau prüft sie noch einmal.{grund}")
}

/// Was nach der Prüfung je Projekt feststeht (vor der Gesamtprüfung).
struct Zwischen {
    projekte: Vec<(String, PathBuf)>,
    /// Der Umgebungs-Abdruck von vor der Prüfung ([`umgebung`]).
    umgebung: [u64; 2],
    stand: Stand,
    frisch_gruen: Vec<(String, PathBuf, u64)>,
    /// Vorläufige Einträge der Bauliste.
    eintraege: Vec<(String, PathBuf)>,
    v: Vorbereitet,
}

/// Die Prüfung je Projekt (`web`, `eltern`, `slug`): im Bau
/// `projektpruefung::pruefe_voll`, in Tests eingesetzt.
type PruefeVoll<'a> = &'a (dyn Fn(&Path, &Path, &str) -> Vec<crate::inhalt::pruefung::Befund> + Sync);

/// Zweig B: die Prüfung vieler Projekte zusammen (`web`, Projekte, Frist),
/// Befunde je Projekt in derselben Reihenfolge; im Bau [`sammel`].
type PruefeSammel<'a> = &'a (dyn Fn(&Path, &[(String, PathBuf)], Instant) -> Vec<Vec<crate::inhalt::pruefung::Befund>> + Sync);

/// Die Sammelprüfung im Bau und beim Einspielen.
fn sammel(web: &Path, projekte: &[(String, PathBuf)], frist: Instant) -> Vec<Vec<crate::inhalt::pruefung::Befund>> {
    projektpruefung::pruefe_sammel(web, projekte, frist, true)
}

/// Schritte 1 und 2: nur geänderte Projekte prüfen, Einträge wählen.
/// Blockiert. Zweig B: Ab `SAMMEL_AB` Projekten prüft `sammel` sie
/// zusammen, sonst `pruefe_voll` je Projekt in einer Warteschlange; was bis
/// `frist` nicht geprüft ist, ist nicht geprüft ([`PRUEFUNG_HOECHSTENS`]).
fn pruefen_mit(ort: &BauOrt, pruefe_voll: PruefeVoll, sammel: Option<PruefeSammel>, frist: Instant) -> Zwischen {
    pruefen_mit_fortschritt(ort, pruefe_voll, sammel, frist, &Fortschritt(Mutex::new(None)))
}

/// Wie [`pruefen_mit`]; zählt dabei in `f` mit (Zweig C; im Bau [`FORTSCHRITT`]).
fn pruefen_mit_fortschritt(ort: &BauOrt, pruefe_voll: PruefeVoll, sammel: Option<PruefeSammel>, frist: Instant, f: &Fortschritt) -> Zwischen {
    let web = ort.web();
    let gruen_ordner = ort.gruen();
    let mut v = Vorbereitet {
        eintraege: Vec::new(),
        neu_rot: Vec::new(),
        alte_fassung: Vec::new(),
        rote: Vec::new(),
        nicht_geprueft: Vec::new(),
        stil: Vec::new(),
        schritte: Vec::new(),
        gescheitert: None,
        gesamt_rot: None,
        liste_neu: false,
    };
    let q = ort.alle_quellen();
    for satz in &q.doppelt {
        warn!("Bau: {satz}");
    }
    // Rechenteile (server/) aus weiteren Quellen baut niemand (nur ein Hinweis).
    for (slug, satz) in &q.rechenteile {
        info!("Bau: {slug}: {satz}");
    }
    // Projekte mit Seite.svelte (nur die kommen in die Bauliste).
    let projekte: Vec<(String, PathBuf)> = q
        .projekte()
        .iter()
        .filter(|(s, e)| std::fs::symlink_metadata(e.join(s).join("Seite.svelte")).is_ok_and(|m| !m.is_dir()))
        .map(|(s, e)| (s.clone(), e.clone()))
        .collect();
    let mut stand = Stand::laden(&gruen_ordner);
    let leer = |v: Vorbereitet, stand: Stand| Zwischen { projekte: Vec::new(), umgebung: [0, 0], stand, frisch_gruen: Vec::new(), eintraege: Vec::new(), v };
    // Ohne Projekte gibt es nichts zu prüfen (die Oberfläche selbst prüft
    // `npm run verify`); sonst braucht es Node und die Pakete.
    if !projekte.is_empty() {
        // shortcut: hier (und bei g.fehler) endet der Bau vor stand.speichern, eine zuvor grüne Seite bleibt „in Ordnung“; aufrüsten im eigenen kleinen Zweig.
        if let Some(satz) = werkzeuge_fehlen(&web) {
            v.gescheitert = Some(format!("Die Prüfung lässt sich nicht machen: {satz}."));
            return leer(v, stand);
        }
    }
    let alter_stand = stand.clone();

    // 1. Nur, was sich geändert hat (Inhalt oder Umgebung, oder noch nie
    //    geprüft ist), dazu alle zurückgestellten, ausgeschlossenen und
    //    nicht geprüften (Kleinkram 2, Zweig A, `neu_pruefen`).
    let t = Instant::now();
    let umgebung_jetzt = umgebung(&web);
    let zu_pruefen: Vec<(String, PathBuf, u64, u64)> = projekte
        .iter()
        .map(|(s, e)| (s.clone(), e.clone(), abdruck(e, s), umgebung_fuer(e, s, umgebung_jetzt)))
        .filter(|(s, _, a, u)| neu_pruefen(stand.projekte.get(s), *a, *u))
        .collect();
    // Verknüpfte Ordner und gefährliche Pfade sind rot, ohne dass ein Werkzeug sie liest.
    let mut befunde: Vec<Option<Vec<crate::inhalt::pruefung::Befund>>> = zu_pruefen
        .iter()
        .map(|(s, e, _, _)| {
            if seite::ist_verknuepfung(&e.join(s)) {
                Some(vec![crate::inhalt::pruefung::Befund::fehler(format!("projects/{s}"), None, seite::verknuepfung_satz(s))])
            } else {
                scanner::import_pfad_pruefen(&e.join(s)).err().map(|grund| vec![crate::inhalt::pruefung::Befund::fehler(format!("projects/{s}"), None, grund)])
            }
        })
        .collect();
    let mut offen: Vec<usize> = (0..zu_pruefen.len()).filter(|&i| befunde[i].is_none()).collect();
    // Reicht die Frist nicht für alle, kommen die am längsten nicht zu Ende geprüften
    // zuerst dran (ohne Zeit vor allen, dann der späteren von `gruen_um` und
    // `versucht_um`, dann der Name), nicht immer dieselben. Zeiten in der Zukunft (Uhr sprang) zählen als jetzt.
    let jetzt = jetzt_sekunden();
    offen.sort_by_key(|&i| (stand.projekte.get(&zu_pruefen[i].0).and_then(|p| p.gruen_um.max(p.versucht_um)).map(|t| t.min(jetzt)), zu_pruefen[i].0.clone()));
    let paare: Vec<(String, PathBuf)> = offen.iter().map(|&i| (zu_pruefen[i].0.clone(), zu_pruefen[i].1.clone())).collect();
    let geprueft = match sammel {
        Some(sammel) if paare.len() >= projektpruefung::SAMMEL_AB => {
            f.schritt(FortschrittSchritt::Sammelpruefung, paare.len());
            sammel(&web, &paare, frist)
        }
        _ => {
            f.schritt(FortschrittSchritt::Pruefung, paare.len());
            projektpruefung::warteschlange(&paare, ZUGLEICH, frist, "Die Prüfung je Projekt", &|s, e| {
                let b = pruefe_voll(&web, e, s);
                f.zaehlen();
                b
            })
        }
    };
    for (i, b) in offen.into_iter().zip(geprueft) {
        befunde[i] = Some(b);
    }
    // Zweig A: Ohne Ergebnis (Faden brach ab) ist das Projekt nicht geprüft, nie still grün.
    let ergebnisse: Vec<(String, PathBuf, u64, u64, Vec<crate::inhalt::pruefung::Befund>)> = zu_pruefen
        .iter()
        .zip(befunde)
        .map(|((s, e, a, u), b)| {
            let b = b.unwrap_or_else(|| projektpruefung::uebersprungen(&format!("projects/{s}"), "Die Prüfung je Projekt", "der Prüf-Faden brach ab"));
            (s.clone(), e.clone(), *a, *u, b)
        })
        .collect();
    if !zu_pruefen.is_empty() {
        let namen: Vec<&str> = zu_pruefen.iter().map(|(s, _, _, _)| s.as_str()).collect();
        schritt(&mut v.schritte, &format!("Prüfung je Projekt ({})", namen.join(", ")), t);
    }
    let mut frisch_gruen: Vec<(String, PathBuf, u64)> = Vec::new();
    for (slug, eltern, a, u, befunde) in ergebnisse {
        let fehler: Vec<_> = befunde.iter().filter(|b| b.ist_fehler()).cloned().collect();
        let uebersprungen: Vec<_> = befunde.iter().filter(|b| projektpruefung::ist_uebersprungen(b)).cloned().collect();
        v.stil.extend(befunde.into_iter().filter(|b| !b.ist_fehler() && b.text.starts_with("Stil")));
        if fehler.is_empty() && !uebersprungen.is_empty() {
            // Zweig A: Übersprungen heißt nicht geprüft, nicht grün. Kein Rot,
            // keine Meldung; gebaut wird die Kopie (Schritt 2), der nächste Bau prüft neu.
            let satz = satz_nicht_geprueft(&slug, &uebersprungen, gruen_ordner.join(&slug).join("Seite.svelte").is_file());
            warn!("Bau: {satz}");
            let alt = alter_stand.projekte.get(&slug);
            let (gruen_um, gebaut_um, gruen_abdruck) =
                (alt.and_then(|p| p.gruen_um), alt.and_then(|p| p.gebaut_um), alt.and_then(ProjektStand::letzter_gruener));
            // Verdrängt die Frist ein Werkzeug, bleibt die alte Zeit: Es kommt im nächsten Bau wieder vorn dran.
            // Nur wenn alles Übersprungene an der Frist lag; ein eigenes Zeitlimit davor zählt als Versuch.
            let versucht_um = if uebersprungen.iter().all(projektpruefung::frist_verpasst) { alt.and_then(|p| p.versucht_um) } else { Some(jetzt) };
            stand.projekte.insert(
                slug,
                ProjektStand {
                    abdruck: a,
                    umgebung: u,
                    gruen: false,
                    satz,
                    gruen_um,
                    rot_art: Some(RotArt::NichtGeprueft),
                    gebaut_um,
                    gruen_abdruck,
                    versucht_um,
                },
            );
        } else if fehler.is_empty() {
            // Erst nach der Gesamtprüfung als grün gemerkt (und kopiert). Bis
            // dahin bleibt der alte Eintrag (mit altem Abdruck, also beim
            // nächsten Bau wieder geprüft); so behält die Oberfläche den Stand
            // und die Zeit der letzten guten Fassung (Etappe 4c).
            frisch_gruen.push((slug.clone(), eltern, a));
        } else {
            let satz = satz_rot(&slug, &fehler, gruen_ordner.join(&slug).join("Seite.svelte").is_file());
            info!("Bau: {slug} ist rot: {satz}");
            if alter_stand.projekte.get(&slug).is_none_or(|p| p.gruen || p.satz != satz) {
                v.neu_rot.push(RotesProjekt { slug: slug.clone(), satz: satz.clone() });
            }
            let alt = alter_stand.projekte.get(&slug);
            let (gruen_um, gebaut_um, gruen_abdruck) =
                (alt.and_then(|p| p.gruen_um), alt.and_then(|p| p.gebaut_um), alt.and_then(ProjektStand::letzter_gruener));
            stand.projekte.insert(
                slug,
                ProjektStand {
                    abdruck: a,
                    umgebung: u,
                    gruen: false,
                    satz,
                    gruen_um,
                    rot_art: Some(RotArt::Pruefung),
                    gebaut_um,
                    gruen_abdruck,
                    versucht_um: Some(jetzt),
                },
            );
        }
    }

    // 2. Einträge: grün aus der Quelle, rot und nicht geprüft aus der Kopie
    //    (oder gar nicht). Zweig A: Aus dem Ordner kommt nur, was jetzt grün
    //    geprüft ist oder unverändert grün war; was in dieser Runde geprüft
    //    und nicht grün wurde, nie (auch nicht über einen alten grünen Eintrag).
    let mut eintraege: Vec<(String, PathBuf)> = Vec::new();
    for (slug, eltern) in &projekte {
        let geprueft = zu_pruefen.iter().any(|(s, _, _, _)| s == slug);
        let gruen = frisch_gruen.iter().any(|(s, _, _)| s == slug) || (!geprueft && stand.projekte.get(slug).is_some_and(|p| p.gruen));
        let nicht_geprueft = stand.projekte.get(slug).is_some_and(|p| !p.gruen && p.rot_art == Some(RotArt::NichtGeprueft));
        let kopie = gruen_ordner.join(slug);
        if gruen {
            // shortcut: Vite liest den Ordner erst beim Bau; wird zwischen Abdruck und Bau gespeichert, geht diese Fassung nur durch die Gesamtprüfung (Wettlauf, schon vor Zweig A); aufrüsten mit Schnappschuss oder noch_derselbe nach dem Bau.
            eintraege.push((slug.clone(), eltern.join(slug)));
        } else if nicht_geprueft {
            if kopie.join("Seite.svelte").is_file() && !seite::ist_verknuepfung(&kopie) {
                eintraege.push((slug.clone(), kopie));
            }
            v.nicht_geprueft.push(slug.clone());
        } else if kopie.join("Seite.svelte").is_file() && !seite::ist_verknuepfung(&kopie) {
            eintraege.push((slug.clone(), kopie));
            v.alte_fassung.push(slug.clone());
        } else {
            v.rote.push(slug.clone());
        }
    }
    Zwischen { projekte, umgebung: umgebung_jetzt, stand, frisch_gruen, eintraege, v }
}

/// Schritt 3: svelte-check über alles mit höchstens zwei Ausschluss-Runden.
/// Blockiert; läuft neben `npm run build` (dessen Fassung gilt erst, wenn
/// die Gesamtprüfung grün ist). Rückgabe mit der Dauer in ms.
fn gesamt_lauf(web: &Path, eintraege: Vec<(String, PathBuf)>, geaendert: BTreeSet<String>, letzte_gruene: LetzteGruene) -> (Gesamt, u64) {
    let t = Instant::now();
    let gruen = web.join(GRUEN_ORDNER);
    let g = gesamtpruefung(
        eintraege,
        |e| {
            if e.is_empty() {
                return Ok(Vec::new());
            }
            let ordner: Vec<PathBuf> = e.iter().map(|(_, o)| o.clone()).collect();
            let tsconfig =
                projektpruefung::tsconfig_schreiben(web, "gesamt", &ordner, true).map_err(|e| format!("die tsconfig lässt sich nicht schreiben ({e})"))?;
            projektpruefung::svelte_check_teil(web, &tsconfig, e, true)
        },
        |slug, ordner| gruene_kopie(&gruen, slug, ordner),
        &geaendert,
        |slug, ordner| seit_gruen_geaendert(&letzte_gruene, &gruen, slug, ordner),
    );
    (g, t.elapsed().as_millis() as u64)
}

/// Kleinkram 2: je Projekt der Elternordner und der Abdruck der letzten
/// grünen Fassung (`None`: es gibt keine grüne Fassung im Stand).
type LetzteGruene = BTreeMap<String, (PathBuf, Option<u64>)>;

fn letzte_gruene(projekte: &[(String, PathBuf)], stand: &Stand) -> LetzteGruene {
    projekte.iter().map(|(s, e)| (s.clone(), (e.clone(), stand.projekte.get(s).and_then(ProjektStand::letzter_gruener)))).collect()
}

/// Zweig A: `geaendert` für die Gesamtprüfung: die frisch grünen Projekte,
/// deren Inhalts-Abdruck von ihrer letzten grünen Fassung abweicht (oder die
/// keine haben). Nur wegen der Umgebung neu geprüft zählt nicht als geändert,
/// sonst träfe der Schuldige nach einer Umgebungsänderung alle. `z.stand`
/// hat hier noch die alten Einträge (grün erst in `abschliessen`).
fn geaendert_seit_gruen(z: &Zwischen) -> BTreeSet<String> {
    z.frisch_gruen.iter().filter(|(s, _, a)| z.stand.projekte.get(s).and_then(ProjektStand::letzter_gruener) != Some(*a)).map(|(s, _, _)| s.clone()).collect()
}

/// Kleinkram 2: Hat sich `slug` seit seiner letzten grünen Fassung geändert?
/// Gebaut aus der grünen Kopie: nein (die Kopie *ist* die letzte grüne
/// Fassung). Ohne grüne Fassung im Stand: ja. Sonst entscheidet der Abdruck
/// jetzt (auch eine Änderung während des Baus zählt).
pub fn seit_gruen_geaendert(letzte: &LetzteGruene, gruen: &Path, slug: &str, ordner: &Path) -> bool {
    if ordner == gruen.join(slug) {
        return false;
    }
    match letzte.get(slug) {
        Some((eltern, Some(a))) => abdruck(eltern, slug) != *a,
        Some((_, None)) => true,
        None => false,
    }
}

/// Nach der Gesamtprüfung: Zurückgesetzte und Ausgeschlossene rot merken,
/// Kopien der grünen erneuern, Stand und Bauliste schreiben. Blockiert.
fn abschliessen(ort: &BauOrt, z: Zwischen, g: Gesamt) -> Vorbereitet {
    let gruen_ordner = ort.gruen();
    let Zwischen { projekte, umgebung: u, mut stand, frisch_gruen, mut v, .. } = z;
    let abdruck_von = |slug: &str| projekte.iter().find(|(s, _)| s == slug).map_or(0, |(s, e)| abdruck(e, s));
    let umgebung_von = |slug: &str| projekte.iter().find(|(s, _)| s == slug).map_or(0, |(s, e)| umgebung_fuer(e, s, u));
    // Kleinkram 2: Zurückgestellte werden bei jedem Bau neu geprüft; bleibt es
    // beim selben Urteil, gilt es nicht als neu rot (keine neue Meldung).
    let schon_so = |stand: &Stand, slug: &str, satz: &str| stand.projekte.get(slug).is_some_and(|p| !p.gruen && p.satz == satz);
    // Abnahme 4c: auf die grüne Kopie zurückgesetzt, die Seite bleibt in ihrer letzten guten Fassung.
    for (slug, fund) in &g.zurueck {
        let satz = satz_zusammenbau(slug, fund, g.ausloeser.get(slug).map(String::as_str), false);
        warn!("Bau: {satz}");
        v.neu_rot.retain(|r| r.slug != *slug);
        if !schon_so(&stand, slug, &satz) {
            v.neu_rot.push(RotesProjekt { slug: slug.clone(), satz: satz.clone() });
        }
        v.rote.retain(|s| s != slug);
        v.nicht_geprueft.retain(|s| s != slug);
        if !v.alte_fassung.contains(slug) {
            v.alte_fassung.push(slug.clone());
        }
        let alt = stand.projekte.get(slug);
        let (gruen_um, gebaut_um, gruen_abdruck) = (alt.and_then(|p| p.gruen_um), alt.and_then(|p| p.gebaut_um), alt.and_then(ProjektStand::letzter_gruener));
        stand.projekte.insert(
            slug.clone(),
            ProjektStand {
                abdruck: abdruck_von(slug),
                umgebung: umgebung_von(slug),
                gruen: false,
                satz,
                gruen_um,
                rot_art: Some(RotArt::Zusammenbau),
                gebaut_um,
                gruen_abdruck,
                versucht_um: Some(jetzt_sekunden()),
            },
        );
    }
    for (slug, fund) in &g.ausgeschlossen {
        let satz = satz_zusammenbau(slug, fund, g.ausloeser.get(slug).map(String::as_str), true);
        warn!("Bau: {satz}");
        v.neu_rot.retain(|r| r.slug != *slug);
        if !schon_so(&stand, slug, &satz) {
            v.neu_rot.push(RotesProjekt { slug: slug.clone(), satz: satz.clone() });
        }
        v.rote.push(slug.clone());
        v.alte_fassung.retain(|s| s != slug);
        v.nicht_geprueft.retain(|s| s != slug);
        let alt = stand.projekte.get(slug);
        let (gruen_um, gebaut_um, gruen_abdruck) = (alt.and_then(|p| p.gruen_um), alt.and_then(|p| p.gebaut_um), alt.and_then(ProjektStand::letzter_gruener));
        stand.projekte.insert(
            slug.clone(),
            ProjektStand {
                abdruck: abdruck_von(slug),
                umgebung: umgebung_von(slug),
                gruen: false,
                satz,
                gruen_um,
                rot_art: Some(RotArt::Ausgeschlossen),
                gebaut_um,
                gruen_abdruck,
                versucht_um: Some(jetzt_sekunden()),
            },
        );
    }
    // Ein früher ausgeschlossenes Projekt, das jetzt aus seiner Kopie gebaut
    // wird (Schritt 2), zeigt wieder seine letzte gute Fassung.
    for slug in &v.alte_fassung {
        if let Some(p) = stand.projekte.get_mut(slug).filter(|p| p.rot_art == Some(RotArt::Ausgeschlossen)) {
            p.rot_art = Some(RotArt::Zusammenbau);
        }
    }
    if let Some(grund) = g.fehler {
        // Der Stand bleibt beim alten: Beim nächsten Bau wird neu geprüft.
        v.gescheitert = Some(grund);
        return v;
    }
    // Jetzt erst: Kopien der grünen Projekte erneuern und grün merken.
    let t = Instant::now();
    for (slug, eltern, a) in &frisch_gruen {
        if g.ausgeschlossen.iter().chain(&g.zurueck).any(|(s, _)| s == slug) {
            continue;
        }
        // Abnahme 4b: Der Abdruck stammt von vor der Prüfung. Hat jemand
        // seitdem gespeichert, ist der Stand im Ordner ungeprüft: nicht
        // kopieren und nicht grün merken; der Beobachter baut ohnehin neu.
        if !noch_derselbe(eltern, slug, *a) {
            info!("Bau: {slug} wurde während des Baus geändert; die grüne Kopie bleibt die alte.");
            continue;
        }
        if let Err(e) = kopie_erneuern(eltern, slug, &gruen_ordner) {
            warn!("Bau: Die grüne Kopie von {slug} ließ sich nicht erneuern: {e}");
        }
        let gebaut_um = stand.projekte.get(slug).and_then(|p| p.gebaut_um);
        stand.projekte.insert(
            slug.clone(),
            ProjektStand {
                abdruck: *a,
                umgebung: umgebung_von(slug),
                gruen: true,
                satz: String::new(),
                gruen_um: Some(jetzt_sekunden()),
                rot_art: None,
                gebaut_um,
                gruen_abdruck: Some(*a),
                versucht_um: None,
            },
        );
    }
    // Was es in keiner Quelle mehr gibt, vergisst der Stand.
    stand.projekte.retain(|s, _| projekte.iter().any(|(p, _)| p == s));
    if let Err(e) = stand.speichern(&gruen_ordner) {
        warn!("Bau: {} lässt sich nicht schreiben: {e}", gruen_ordner.join(STAND_DATEI).display());
    }
    match scanner::liste_schreiben_aus(&ort.repo.join(pruefe::BAULISTE_DATEI), &g.eintraege) {
        Ok(neu) => v.liste_neu = neu,
        Err(e) => v.gescheitert = Some(format!("{} lässt sich nicht schreiben: {e}", pruefe::BAULISTE_DATEI)),
    }
    schritt(&mut v.schritte, "Kopien und Bauliste", t);
    v.eintraege = g.eintraege;
    v
}

/// Die Gesamtprüfung (Schritt 3) für [`bauliste_einspielen_mit`]: im
/// Einspiel-Weg [`gesamt_lauf`], in Tests eingesetzt.
type GesamtLauf<'a> = &'a dyn Fn(&Path, Vec<(String, PathBuf)>, BTreeSet<String>, LetzteGruene) -> Gesamt;

/// Der Einspiel-Weg (`dashboard pruefe --bauliste --svelte`, so ruft
/// `scripts\Dienst.ps1 -Neustart`): dasselbe Urteil wie [`bauen`] mit
/// denselben Schritten 1 bis 3 (Prüfung je Projekt mit `pruefe_voll`,
/// übersprungen heißt nicht geprüft, Gesamtprüfung, Kopien, `stand.json`,
/// Bauliste), nur ohne `npm run build`: Das baut Dienst.ps1 danach selbst
/// nach `web\build`, darum am Ende [`nur_aus_kopien`]. Blockiert. Mit
/// `gescheitert` steht in der Bauliste nicht das Ergebnis; der Aufrufer
/// schreibt dann die leere. Bleibt die Gesamtprüfung rot, ist das kein
/// Scheitern: `gesamt_rot` nennt den Grund, und die Bauliste hat jedes
/// Projekt mit grüner Kopie (aus der Kopie).
// shortcut: der Einspiel-Weg merkt neu rote Projekte im Stand, ohne den Dienst zu benachrichtigen (Dienst.ps1 nennt sie nur in seiner Ausgabe); aufrüsten, wenn Meldungen für beim Einspielen rot gewordene Seiten fehlen.
pub(crate) fn bauliste_einspielen(ort: &BauOrt) -> Vorbereitet {
    // Etwas früher als die Frist in `bauliste_einspielen_mit`, also nie später.
    let frist = Instant::now() + PRUEFUNG_HOECHSTENS;
    bauliste_einspielen_mit(ort, &|w, e, s| projektpruefung::pruefe_voll(w, e, s, true, frist), Some(&sammel), &|w, e, g, l| gesamt_lauf(w, e, g, l).0)
}

fn bauliste_einspielen_mit(ort: &BauOrt, pruefe_voll: PruefeVoll, sammel: Option<PruefeSammel>, gesamt: GesamtLauf) -> Vorbereitet {
    let mut z = pruefen_mit(ort, pruefe_voll, sammel, Instant::now() + PRUEFUNG_HOECHSTENS);
    if z.v.gescheitert.is_some() {
        return z.v;
    }
    // Wie in `bauen`: Die Gesamtprüfung liest die Oberfläche mit (`web/src`
    // bindet die Bauliste ein), also erst die vorläufige Liste.
    if let Err(e) = scanner::liste_schreiben_aus(&ort.repo.join(pruefe::BAULISTE_DATEI), &z.eintraege) {
        z.v.gescheitert = Some(format!("{} lässt sich nicht schreiben: {e}", pruefe::BAULISTE_DATEI));
        return z.v;
    }
    let geaendert = geaendert_seit_gruen(&z);
    let letzte = letzte_gruene(&z.projekte, &z.stand);
    let g = gesamt(&ort.web(), z.eintraege.clone(), geaendert, letzte);
    let gruen = ort.gruen();
    let kopien: Vec<(String, PathBuf)> = z.projekte.iter().map(|(s, _)| (s.clone(), gruen.join(s))).collect();
    let mut v = abschliessen(ort, z, g);
    // Rot geblieben: Wie im Boten gilt nichts aus dieser Prüfung (der Stand
    // bleibt); der Bote behält seine letzte gute Fassung, hier wird sie aus
    // allen grünen Kopien gebaut statt ganz ohne Projektseiten.
    if v.gescheitert.as_deref().is_some_and(|g| g.starts_with(GESAMT_ROT)) {
        v.gesamt_rot = v.gescheitert.take();
        v.alte_fassung.clear();
        v.nicht_geprueft.clear();
        v.eintraege = kopien.clone();
    }
    if v.gescheitert.is_some() {
        return v;
    }
    let mut v = nur_aus_kopien(ort, v);
    if v.gesamt_rot.is_some() {
        v.rote = kopien.into_iter().map(|(s, _)| s).filter(|s| !v.eintraege.iter().any(|(e, _)| e == s)).collect();
    }
    v
}

/// Einspiel-Weg nach [`abschliessen`]: Anders als im Boten baut niemand nach
/// (`npm run build` in Dienst.ps1 liest die Ordner erst später, ein
/// Neustart baut nicht). Darum kommt jedes Projekt aus seiner grünen Kopie
/// (sie ändert nur `kopie_erneuern` mit geprüftem Stand, und der Bote steht).
/// Gleicht die Kopie dem Ordner nicht mehr (nach der Prüfung gespeichert,
/// Kopie fehlt): nicht geprüft, gebaut wird die letzte grüne Kopie oder nichts.
// shortcut: eine Kopie, die vom Ordner abweicht, lief nicht durch diese Gesamtprüfung (sie war früher grün); aufrüsten, wenn so ein Bau bricht (dann die Gesamtprüfung noch einmal mit den Kopien).
fn nur_aus_kopien(ort: &BauOrt, mut v: Vorbereitet) -> Vorbereitet {
    let gruen = ort.gruen();
    let mut stand = Stand::laden(&gruen);
    let mut eintraege = Vec::new();
    for (slug, ordner) in std::mem::take(&mut v.eintraege) {
        let kopie = gruen.join(&slug);
        let hat_kopie = kopie.join("Seite.svelte").is_file() && !seite::ist_verknuepfung(&kopie);
        if ordner != kopie && !(hat_kopie && gleiche_code_dateien(&ordner, &kopie)) {
            let satz = if hat_kopie {
                format!(
                    "Die Seite von „{slug}“ weicht von ihrer grünen Kopie ab (nach der Prüfung geändert, oder die Kopie ließ sich nicht erneuern) und ist so nicht geprüft. In diesem Bau steht ihre letzte gute Fassung; der nächste Bau prüft sie noch einmal."
                )
            } else {
                format!(
                    "Von der Seite von „{slug}“ gibt es keine grüne Kopie (nach der Prüfung geändert, oder die Kopie ließ sich nicht anlegen), so ist sie nicht geprüft. Sie ist in diesem Bau nicht dabei; der nächste Bau prüft sie noch einmal."
                )
            };
            warn!("Einspielen: {satz}");
            let alt = stand.projekte.get(&slug).cloned().unwrap_or_default();
            let gruen_abdruck = alt.letzter_gruener();
            stand.projekte.insert(slug.clone(), ProjektStand { gruen: false, satz, rot_art: Some(RotArt::NichtGeprueft), gruen_abdruck, ..alt });
            v.nicht_geprueft.push(slug.clone());
        }
        if hat_kopie {
            eintraege.push((slug, kopie));
        }
    }
    if let Err(e) = stand.speichern(&gruen) {
        warn!("Einspielen: {} lässt sich nicht schreiben: {e}", gruen.join(STAND_DATEI).display());
    }
    if let Err(e) = scanner::liste_schreiben_aus(&ort.repo.join(pruefe::BAULISTE_DATEI), &eintraege) {
        v.gescheitert = Some(format!("{} lässt sich nicht schreiben: {e}", pruefe::BAULISTE_DATEI));
    }
    v.eintraege = eintraege;
    v
}

/// Haben `a` und `b` (zwei Projektordner) dieselben `.svelte`- und
/// `.ts`-Dateien mit demselben Inhalt?
fn gleiche_code_dateien(a: &Path, b: &Path) -> bool {
    let lesen = |o: &Path| -> Option<Vec<(String, Vec<u8>)>> {
        let slug = o.file_name()?.to_str()?;
        let mut d: Vec<(String, Vec<u8>)> =
            scanner::code_dateien(o.parent()?, slug).into_iter().map(|(r, p)| Some((r, std::fs::read(p).ok()?))).collect::<Option<_>>()?;
        d.sort();
        Some(d)
    };
    lesen(a).is_some_and(|x| lesen(b) == Some(x))
}

/// Setzt `gebaut_um` für die Seiten `slugs` (die im geltenden Bau stehen)
/// auf `um`; `gruen_um` und alles andere bleibt. Seiten ohne Eintrag im
/// Stand bekommen keinen (sie sind nicht geprüft).
pub fn gebaut_setzen<'a>(stand: &mut Stand, slugs: impl IntoIterator<Item = &'a str>, um: i64) {
    for slug in slugs {
        if let Some(p) = stand.projekte.get_mut(slug) {
            p.gebaut_um = Some(um);
        }
    }
}

/// `gebaut_setzen` in `stand.json` (lesen, setzen, schreiben). Ein Fehler beim
/// Schreiben kommt nur ins Protokoll: Der Bau gilt trotzdem.
fn gebaut_merken<'a>(gruen: &Path, slugs: impl IntoIterator<Item = &'a str>, um: i64) {
    let mut stand = Stand::laden(gruen);
    gebaut_setzen(&mut stand, slugs, um);
    if let Err(e) = stand.speichern(gruen) {
        warn!("Bau: {} lässt sich nicht schreiben: {e}", gruen.join(STAND_DATEI).display());
    }
}

fn rot(start: Instant, rote: Vec<String>, satz: String, ausgabe_ende: Option<String>) -> BauErgebnis {
    BauErgebnis { gruen: false, dauer_ms: start.elapsed().as_millis() as u64, ordner: None, rote, satz, ausgabe_ende, ..Default::default() }
}

/// Entfernt einen Bau-Ordner, dessen Fassung nicht gilt (nur einen echten Ordner mit Bau-Namen).
fn bau_weg(builds: &Path, name: &str) {
    let o = builds.join(name);
    if ist_bau_name(name) && o.is_dir() && !seite::ist_verknuepfung(&o) {
        let _ = std::fs::remove_dir_all(&o);
    }
}

/// Ein Bau mit dem Zeitlimit `limit` für npm. Wartet, bis ein laufender Bau fertig ist.
///
/// Damit der Inhaber nicht doppelt wartet, laufen die Gesamtprüfung und
/// `npm run build` nebeneinander (beide mit der vorläufigen Bauliste). Die
/// neue Fassung gilt erst, wenn die Gesamtprüfung grün ist; schließt sie ein
/// Projekt aus, wird der Ordner verworfen und mit der neuen Liste noch einmal
/// gebaut; bleibt sie rot, bleibt die alte Fassung.
pub async fn bauen(ort: &BauOrt, limit: Duration) -> BauErgebnis {
    let _einer = EIN_BAU.lock().await;
    let _leeren = FortschrittLeeren;
    let start = Instant::now();
    let web = ort.web();
    if !web.join("package.json").is_file() {
        return rot(start, Vec::new(), format!("In {} fehlt package.json; der Bote baut dort nicht.", web.display()), None);
    }

    // 1. und 2.: prüfen, Einträge (blockiert, eigener Faden).
    let o = ort.clone();
    let frist = Instant::now() + PRUEFUNG_HOECHSTENS;
    let z = match tokio::task::spawn_blocking(move || {
        pruefen_mit_fortschritt(&o, &|w, e, s| projektpruefung::pruefe_voll(w, e, s, true, frist), Some(&sammel), frist, &FORTSCHRITT)
    })
    .await
    {
        Ok(z) => z,
        Err(e) => return rot(start, Vec::new(), format!("Die Prüfung vor dem Bau ist abgebrochen ({e}). Die letzte gute Fassung bleibt."), None),
    };
    let nicht_gebaut = |mut e: BauErgebnis, grund: &str| {
        e.satz = format!("Nicht gebaut: {}. Die letzte gute Fassung bleibt.", grund.trim().trim_end_matches('.'));
        e.dauer_ms = start.elapsed().as_millis() as u64;
        e
    };
    if let Some(grund) = z.v.gescheitert.clone() {
        let e = BauErgebnis { neu_rot: z.v.neu_rot.clone(), schritte: z.v.schritte.clone(), ..Default::default() };
        return nicht_gebaut(e, &grund);
    }
    let ziel_liste = ort.repo.join(pruefe::BAULISTE_DATEI);
    if let Err(e) = scanner::liste_schreiben_aus(&ziel_liste, &z.eintraege) {
        return rot(start, Vec::new(), format!("{} lässt sich nicht schreiben: {e}. Die letzte gute Fassung bleibt.", pruefe::BAULISTE_DATEI), None);
    }

    // 3. und 4.: Gesamtprüfung und npm run build nebeneinander.
    let builds = ort.builds();
    let (w, e) = (web.clone(), z.eintraege.clone());
    let geaendert = geaendert_seit_gruen(&z);
    let letzte = letzte_gruene(&z.projekte, &z.stand);
    FORTSCHRITT.schritt(FortschrittSchritt::Gesamtpruefung, 0);
    let pruefung = tokio::task::spawn_blocking(move || gesamt_lauf(&w, e, geaendert, letzte));
    let t = Instant::now();
    let lauf = npm_lauf(ort, &builds, limit, false).await;
    let bau_ms = t.elapsed().as_millis() as u64;
    let (g, g_ms) = match pruefung.await {
        Ok(x) => x,
        Err(e) => (
            Gesamt {
                eintraege: Vec::new(),
                ausgeschlossen: Vec::new(),
                zurueck: Vec::new(),
                fehler: Some(format!("die Gesamtprüfung brach ab ({e})")),
                laeufe: 0,
                ausloeser: BTreeMap::new(),
            },
            0,
        ),
    };
    let sauber = g.fehler.is_none() && !g.eingegriffen();
    let laeufe = g.laeufe;
    let o = ort.clone();
    let v = match tokio::task::spawn_blocking(move || abschliessen(&o, z, g)).await {
        Ok(v) => v,
        Err(e) => {
            if let Ok(name) = &lauf {
                bau_weg(&builds, name);
            }
            return rot(start, Vec::new(), format!("Der Abschluss der Prüfung ist abgebrochen ({e}). Die letzte gute Fassung bleibt."), None);
        }
    };
    let mut ergebnis = BauErgebnis {
        neu_rot: v.neu_rot.clone(),
        alte_fassung: v.alte_fassung.clone(),
        rote: v.rote.clone(),
        schritte: v.schritte.clone(),
        ..Default::default()
    };
    ergebnis.schritte.push(BauSchritt { name: format!("Gesamtprüfung ({laeufe} Lauf/Läufe, neben vite build)"), ms: g_ms });
    ergebnis.schritte.push(BauSchritt { name: "vite build".into(), ms: bau_ms });
    if let Some(grund) = v.gescheitert.clone() {
        if let Ok(name) = &lauf {
            bau_weg(&builds, name);
        }
        return nicht_gebaut(ergebnis, &grund);
    }
    let lauf = match lauf {
        Ok(name) if sauber => match zeiger_schreiben(&builds, &name) {
            Ok(()) => Ok(name),
            Err(e) => {
                bau_weg(&builds, &name);
                Err(Fehlschlag::neu(format!("Der Zeiger web\\builds\\{ZEIGER} lässt sich nicht schreiben: {e}."), None))
            }
        },
        // Die Gesamtprüfung hat ein Projekt ausgeschlossen oder auf seine Kopie
        // zurückgesetzt: Die Liste ist neu,
        // der erste Bau gilt nicht; noch einmal mit der neuen Liste.
        erst => {
            if let Ok(name) = &erst {
                bau_weg(&builds, name);
            }
            if sauber {
                erst
            } else {
                let t = Instant::now();
                FORTSCHRITT.schritt(FortschrittSchritt::ViteBuild, 0);
                let zweit = npm_lauf(ort, &builds, limit, true).await;
                ergebnis.schritte.push(BauSchritt { name: "vite build (nach Ausschluss)".into(), ms: t.elapsed().as_millis() as u64 });
                zweit
            }
        }
    };
    let name = match lauf {
        Ok(name) => name,
        Err(f) => {
            ergebnis.satz = format!("{} Die letzte gute Fassung bleibt.", f.satz);
            ergebnis.ausgabe_ende = f.ausgabe;
            ergebnis.dauer_ms = start.elapsed().as_millis() as u64;
            return ergebnis;
        }
    };

    // Jede Seite im geltenden Bau ist jetzt
    // gebaut, auch ohne Änderung (`gebaut_um`; `gruen_um` bleibt).
    gebaut_merken(&ort.gruen(), v.eintraege.iter().map(|(s, _)| s.as_str()), jetzt_sekunden());

    // 5. Aufräumen: nur alte Bau-Ordner, die zwei neuesten bleiben.
    let entfernt = aufraeumen(&builds, BEHALTEN, &name);
    let dauer = start.elapsed();
    let mut satz = format!("Bau grün in {} s: Die Oberfläche kommt jetzt aus web\\builds\\{name}.", dauer.as_secs());
    if !v.alte_fassung.is_empty() {
        satz.push_str(&format!(" In der letzten guten Fassung: {}.", v.alte_fassung.join(", ")));
    }
    if !v.rote.is_empty() {
        satz.push_str(&format!(" Ausgelassen, weil rot: {}.", v.rote.join(", ")));
    }
    if let Some(s) = satz_nicht_geprueft_bau(&v.nicht_geprueft) {
        satz.push(' ');
        satz.push_str(&s);
    }
    if let Some(s) = stil_satz(&v.stil) {
        satz.push(' ');
        satz.push_str(&s);
    }
    info!("Bau grün in {} ms nach {name}; {entfernt} alte Ordner entfernt; Schritte: {}", dauer.as_millis(), schritte_text(&ergebnis.schritte));
    ergebnis.gruen = true;
    ergebnis.dauer_ms = dauer.as_millis() as u64;
    ergebnis.ordner = Some(name);
    ergebnis.satz = satz;
    ergebnis
}

/// Zweig A: der Satz im Bau-Ergebnis zu den nicht geprüften Projekten (kein Rot).
pub fn satz_nicht_geprueft_bau(slugs: &[String]) -> Option<String> {
    (!slugs.is_empty()).then(|| {
        format!(
            "Nicht geprüft, weil ein Prüfwerkzeug nicht lief (gebaut aus der letzten guten Fassung, ohne sie nicht dabei; der nächste Bau prüft noch einmal): {}.",
            slugs.join(", ")
        )
    })
}

/// Die Schritte als kurzer Text fürs Protokoll.
pub fn schritte_text(s: &[BauSchritt]) -> String {
    s.iter().map(|s| format!("{} {:.1} s", s.name, s.ms as f64 / 1000.0)).collect::<Vec<_>>().join(", ")
}

/// Ein Satz zu den Hinweisen der Stil-Prüfung (Etappe 3b): welche Projekte
/// wie viele haben und wie man sie sieht. `None` ohne Hinweise. Die
/// Hinweise brechen den Bau nicht (`werkzeug::stil::STIL_ALS_FEHLER`).
pub fn stil_satz(stil: &[crate::inhalt::pruefung::Befund]) -> Option<String> {
    let mut je: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for b in stil {
        let slug = b.datei.strip_prefix("projects/").and_then(|r| r.split('/').next()).filter(|s| !s.is_empty());
        *je.entry(slug.unwrap_or("projects")).or_default() += 1;
    }
    if je.is_empty() {
        return None;
    }
    let liste: Vec<String> = je.iter().map(|(s, n)| format!("{s} ({n})")).collect();
    Some(format!(
        "Stil-Hinweise von Stylelint (werden Fehler, sobald STIL_ALS_FEHLER umgestellt ist): {}. Einzelheiten mit dashboard.exe pruefe <slug>.",
        liste.join(", ")
    ))
}

/// Warum ein Lauf von npm nicht grün war.
struct Fehlschlag {
    satz: String,
    ausgabe: Option<String>,
}

impl Fehlschlag {
    fn neu(satz: String, ausgabe: Option<String>) -> Self {
        Fehlschlag { satz, ausgabe }
    }
}

/// `npm run build` in einen neuen Ordner unter `builds`; bei grün und mit
/// `zeiger` wird der Zeiger umgestellt. Rückgabe: der Name des neuen Ordners.
async fn npm_lauf(ort: &BauOrt, builds: &Path, limit: Duration, zeiger: bool) -> Result<String, Fehlschlag> {
    let web = ort.web();
    if let Err(e) = std::fs::create_dir_all(builds) {
        return Err(Fehlschlag::neu(format!("Der Ordner {} lässt sich nicht anlegen: {e}.", builds.display()), None));
    }
    let name = neuer_name(builds);
    let ziel = builds.join(&name);
    info!("Bau: npm run build nach {}", ziel.display());
    let mut cmd = programme::npm_befehl_async().map_err(|satz| Fehlschlag::neu(format!("{satz}. Gebaut wurde nicht."), None))?;
    // Etappe 4b: niedrige Priorität, damit das Cockpit nicht ruckelt (vite erbt sie).
    programme::niedrig_async(&mut cmd);
    // Node vorn in den PATH: die Skripte in package.json rufen `node` und
    // `vite` über cmd.exe auf (npm hängt node_modules\.bin selbst davor).
    let node_ordner = Path::new(cmd.as_std().get_program()).parent().map(Path::to_path_buf).unwrap_or_default();
    let mut pfad = std::ffi::OsString::from(node_ordner.as_os_str());
    if let Some(alt) = std::env::var_os("PATH") {
        pfad.push(";");
        pfad.push(alt);
    }
    cmd.args(["run", "build"])
        .current_dir(&web)
        .env(UMGEBUNG_ZIEL, &ziel)
        .env("PATH", pfad)
        .env("NO_COLOR", "1")
        .env("FORCE_COLOR", "0")
        .env("NO_UPDATE_NOTIFIER", "1")
        .env("npm_config_update_notifier", "false")
        .env("npm_config_fund", "false")
        // Etappe 4b: die Inhaltsquellen für server.fs.allow in vite.config.ts.
        .env("DASHBOARD_PROJECTS_DIR", ort.alle_quellen().als_text())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut kind = cmd.spawn().map_err(|e| Fehlschlag::neu(format!("npm ließ sich nicht starten: {e}."), None))?;
    // Alles, was npm startet (cmd.exe, vite), hängt am Job und endet mit ihm.
    #[cfg(windows)]
    let _job = baum::anhaengen(&kind);
    let ende = Ende::neu();
    let leser = [mitlesen(kind.stdout.take(), ende.clone()), mitlesen(kind.stderr.take(), ende.clone())];
    let ergebnis = tokio::time::timeout(limit, kind.wait()).await;
    let abgelaufen = ergebnis.is_err();
    if abgelaufen {
        let _ = kind.start_kill();
        #[cfg(windows)]
        drop(_job);
        let _ = tokio::time::timeout(Duration::from_secs(5), kind.wait()).await;
    }
    // Die Leser enden, wenn alle Schreiber weg sind; nicht ewig warten.
    for l in leser {
        let _ = tokio::time::timeout(Duration::from_secs(5), l).await;
    }
    let ausgabe = ende.lock().unwrap_or_else(|e| e.into_inner()).text();
    let halb_weg = |ziel: &Path| {
        if ziel.is_dir() && !seite::ist_verknuepfung(ziel) {
            let _ = std::fs::remove_dir_all(ziel);
        }
    };
    let status = match ergebnis {
        Err(_) => {
            halb_weg(&ziel);
            let satz = format!("npm run build hat länger als {} s gedauert und wurde beendet.", limit.as_secs());
            return Err(Fehlschlag::neu(satz, Some(ausgabe)));
        }
        Ok(Err(e)) => {
            halb_weg(&ziel);
            return Err(Fehlschlag::neu(format!("Auf npm ließ sich nicht warten: {e}."), Some(ausgabe)));
        }
        Ok(Ok(s)) => s,
    };
    if !status.success() {
        halb_weg(&ziel);
        let code = status.code().map_or_else(|| "ohne Rückgabewert".to_string(), |c| format!("Rückgabe {c}"));
        return Err(Fehlschlag::neu(format!("npm run build ist gescheitert ({code})."), Some(ausgabe)));
    }

    // index.html da? Dann den Zeiger umstellen.
    if !ziel.join("index.html").is_file() || seite::ist_verknuepfung(&ziel) {
        halb_weg(&ziel);
        let satz = format!("npm run build lief durch, aber in web\\builds\\{name} liegt keine index.html.");
        return Err(Fehlschlag::neu(satz, Some(ausgabe)));
    }
    if !zeiger {
        return Ok(name);
    }
    if let Err(e) = zeiger_schreiben(builds, &name) {
        halb_weg(&ziel);
        return Err(Fehlschlag::neu(format!("Der Zeiger web\\builds\\{ZEIGER} lässt sich nicht schreiben: {e}."), None));
    }
    Ok(name)
}

/// Job-Objekt für npm und alle seine Kinder (nur Windows): Schließen des
/// Handles beendet den ganzen Baum (KILL_ON_JOB_CLOSE).
#[cfg(windows)]
mod baum {
    use tracing::warn;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectExtendedLimitInformation, SetInformationJobObject,
    };

    pub struct Job(HANDLE);

    // SAFETY: Ein Job-Handle darf zwischen Fäden wandern; es wird nur geschlossen.
    unsafe impl Send for Job {}
    unsafe impl Sync for Job {}

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: eigenes, gültiges Handle; Schließen beendet, was im Job steckt.
            unsafe { CloseHandle(self.0) };
        }
    }

    /// Legt einen Job an und das Kind hinein. Klappt es nicht: Warnung, dann
    /// beendet das Zeitlimit nur npm selbst (`kill_on_drop`).
    pub fn anhaengen(kind: &tokio::process::Child) -> Option<Job> {
        // SAFETY: ohne Sicherheitsattribute und ohne Namen.
        let h = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if h.is_null() {
            warn!("Bau: kein Job-Objekt ({})", std::io::Error::last_os_error());
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
            warn!("Bau: Job-Objekt nicht einstellbar ({})", std::io::Error::last_os_error());
            return None;
        }
        let roh = kind.raw_handle()?;
        // SAFETY: beide Handles gültig; das Kind-Handle gehört `kind`.
        if unsafe { AssignProcessToJobObject(job.0, roh as HANDLE) } == 0 {
            warn!("Bau: npm nicht im Job-Objekt ({})", std::io::Error::last_os_error());
            return None;
        }
        Some(job)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stil_satz_nennt_die_projekte() {
        use crate::inhalt::pruefung::Befund;
        assert_eq!(stil_satz(&[]), None);
        let b = [
            Befund::hinweis("projects/b/Seite.svelte", Some(3), "x"),
            Befund::hinweis("projects/a/Seite.svelte", Some(1), "x"),
            Befund::hinweis("projects/b/seiten/c.svelte", Some(2), "x"),
        ];
        assert_eq!(
            stil_satz(&b).unwrap(),
            "Stil-Hinweise von Stylelint (werden Fehler, sobald STIL_ALS_FEHLER umgestellt ist): a (1), b (2). Einzelheiten mit dashboard.exe pruefe <slug>."
        );
    }

    #[test]
    fn bau_namen() {
        for gut in ["20261027-101010", "20261027-101010-2", "20261027-101010-999"] {
            assert!(ist_bau_name(gut), "{gut}");
        }
        for schlecht in [
            "",
            "aktuell",
            "2026-10-27",
            "20261027-10101",
            "20261027_101010",
            "20261027-101010-",
            "20261027-101010-1000",
            "20261027-101010x",
            "..",
            "20261027-101010\\x",
        ] {
            assert!(!ist_bau_name(schlecht), "{schlecht}");
        }
    }

    #[test]
    fn ende_der_ausgabe_ohne_farben_und_begrenzt() {
        let mut e = Ende::default();
        e.dazu("\u{1b}[31merror\u{1b}[39m in Seite.svelte");
        e.dazu("   ");
        assert_eq!(e.text(), "error in Seite.svelte");
        for i in 0..100 {
            e.dazu(&format!("Zeile {i} {}", "ä".repeat(60)));
        }
        let t = e.text();
        assert!(t.len() <= ENDE_BYTES + "…".len(), "{}", t.len());
        assert!(t.ends_with(&format!("Zeile 99 {}", "ä".repeat(60))));
        assert!(!t.contains("Zeile 69 "), "nur die letzten {ENDE_ZEILEN} Zeilen");
    }

    #[test]
    fn aufraeumen_behaelt_die_neuesten_und_den_aktuellen() {
        let builds = std::env::temp_dir().join(format!("dashboard-bau-aufraeumen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&builds);
        for n in ["20261027-090000", "20261027-100000", "20261027-110000", "20261027-110000-2", "fremd"] {
            std::fs::create_dir_all(builds.join(n)).unwrap();
        }
        std::fs::write(builds.join("20261027-080000"), "eine Datei, kein Ordner").unwrap();
        // Der Zeiger nennt den ältesten: er bleibt trotzdem.
        assert_eq!(aufraeumen(&builds, 2, "20261027-090000"), 1);
        let mut da: Vec<String> = std::fs::read_dir(&builds).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        da.sort();
        assert_eq!(da, ["20261027-080000", "20261027-090000", "20261027-110000", "20261027-110000-2", "fremd"]);
        zeiger_schreiben(&builds, "20261027-110000-2").unwrap();
        zeiger_schreiben(&builds, "20261027-110000").unwrap();
        assert_eq!(std::fs::read_to_string(builds.join(ZEIGER)).unwrap(), "20261027-110000\n");
        assert!(!builds.join("aktuell.neu").exists());
        std::fs::remove_dir_all(&builds).unwrap();
    }

    // ----- Bauen mit einem Test-Repo (echtes npm, kein SvelteKit) -----

    /// Das Bau-Skript des Test-Repos: `modus.txt` sagt, was es tut.
    /// `gruen` legt `index.html` im Ziel an, `rot` endet mit Fehler,
    /// `haengt` startet ein Kind und schläft (Nummern in `pids.txt`).
    const BAU_SKRIPT: &str = r#"import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { spawn } from 'node:child_process';
import { join } from 'node:path';
const ziel = process.env.DASHBOARD_BUILD_ZIEL;
const modus = readFileSync('modus.txt', 'utf8').trim();
console.log('baue nach ' + ziel);
if (modus === 'rot_mit_seiten' && readFileSync('src/lib/projekte.generated.ts', 'utf8').includes('Seite.svelte')) {
  console.error('Fehler: ein Projekt bricht den Bau');
  process.exit(4);
}
if (modus === 'rot') {
  console.error('Fehler: projects/x/Seite.svelte, Zeile 3: kaputt');
  console.error('letzte Zeile der Ausgabe');
  process.exit(3);
}
if (modus === 'haengt') {
  const k = spawn(process.execPath, ['-e', 'setTimeout(() => {}, 600000)'], { stdio: 'ignore', windowsHide: true });
  writeFileSync('pids.txt', process.pid + ' ' + k.pid);
  await new Promise((r) => setTimeout(r, 600000));
}
mkdirSync(ziel, { recursive: true });
writeFileSync(join(ziel, 'index.html'), '<!doctype html><title>gebaut</title>');
"#;

    /// Ein Repo im Temp-Ordner mit `projects/` und einem kleinen `web/`.
    struct TestRepo(PathBuf);

    impl TestRepo {
        fn neu(name: &str, modus: &str) -> TestRepo {
            let r = std::env::temp_dir().join(format!("dashboard-bau-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&r);
            std::fs::create_dir_all(r.join("projects")).unwrap();
            std::fs::create_dir_all(r.join("web")).unwrap();
            std::fs::write(r.join("web/package.json"), r#"{ "name": "test", "private": true, "type": "module", "scripts": { "build": "node bau.mjs" } }"#)
                .unwrap();
            std::fs::write(r.join("web/bau.mjs"), BAU_SKRIPT).unwrap();
            let t = TestRepo(r);
            t.modus(modus);
            t
        }

        fn modus(&self, m: &str) {
            std::fs::write(self.0.join("web/modus.txt"), m).unwrap();
        }

        fn ort(&self) -> BauOrt {
            BauOrt::neu(self.0.clone(), self.0.join("projects"))
        }

        fn zeiger(&self) -> Option<String> {
            std::fs::read_to_string(self.0.join("web/builds/aktuell")).ok().map(|t| t.trim().to_string())
        }

        fn ordner(&self) -> Vec<String> {
            let mut v: Vec<String> = std::fs::read_dir(self.0.join("web/builds"))
                .map(|rd| rd.filter_map(Result::ok).filter(|e| e.path().is_dir()).filter_map(|e| e.file_name().into_string().ok()).collect())
                .unwrap_or_default();
            v.sort();
            v
        }

        /// Prüfen möglich machen: `web/node_modules` als Junction auf die
        /// echten Module des Repos, dazu eine kleine `tsconfig.json` und die
        /// Konfiguration von ESLint für Projekte. `false`, wenn Node, die
        /// Module oder die Junction fehlen (der Test überspringt dann).
        fn mit_svelte_check(&self) -> bool {
            let echt = Path::new(env!("CARGO_MANIFEST_DIR")).join("web");
            let module = echt.join("node_modules");
            if programme::pfad_von(programme::Werkzeug::Node).is_none() || !module.join("svelte-check").is_dir() || !module.join("eslint").is_dir() {
                return false;
            }
            let tsconfig = r#"{ "compilerOptions": { "strict": true, "target": "esnext", "module": "esnext", "moduleResolution": "bundler", "verbatimModuleSyntax": true, "skipLibCheck": true, "noEmit": true }, "include": [] }"#;
            std::fs::write(self.0.join("web/tsconfig.json"), tsconfig).unwrap();
            std::fs::create_dir_all(self.0.join("web/src")).unwrap();
            for d in ["eslint.projekte.config.js", "eslint.config.js", ".gitignore", "stylelint.config.js", "vitest.projekte.config.ts"] {
                std::fs::copy(echt.join(d), self.0.join("web").join(d)).unwrap();
            }
            std::fs::create_dir_all(self.0.join("web/scripts")).unwrap();
            std::fs::copy(echt.join("scripts/stil-projekte.mjs"), self.0.join("web/scripts/stil-projekte.mjs")).unwrap();
            seite::junction_anlegen(&self.0.join("web").join("node_modules"), &module)
        }

        fn projekt(&self, slug: &str, seite: &str) {
            std::fs::create_dir_all(self.0.join("projects").join(slug)).unwrap();
            std::fs::write(self.0.join("projects").join(slug).join("Seite.svelte"), seite).unwrap();
        }

        fn liste(&self) -> String {
            std::fs::read_to_string(self.0.join(pruefe::BAULISTE_DATEI)).unwrap_or_default()
        }
    }

    impl Drop for TestRepo {
        fn drop(&mut self) {
            // Erst die Junction auf node_modules lösen (nur sie, nicht rekursiv).
            let module = self.0.join("web").join("node_modules");
            if seite::ist_verknuepfung(&module) {
                let _ = std::fs::remove_dir(&module);
            }
            if seite::ist_verknuepfung(&module) {
                // Lässt sie sich nicht lösen, bleibt der Ordner lieber liegen.
                eprintln!("Achtung: Junction {} ließ sich nicht lösen; Test-Ordner bleibt.", module.display());
                return;
            }
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Eine Seite mit Titel (für die Tests mit echtem svelte-check).
    fn probe_seite(t: &str) -> String {
        format!("<script lang=\"ts\">\n\tconst titel: string = '{t}';\n</script>\n\n<p>{{titel}}</p>\n")
    }

    /// Eine globale Eigenschaft `gesamtProbe` vom Typ `typ`.
    fn global_probe(typ: &str) -> String {
        format!("export {{}};\n\ndeclare global {{\n\tinterface Window {{\n\t\tgesamtProbe: {typ};\n\t}}\n}}\n")
    }

    /// Bau 1 grün mit beiden Projekten; dann erklärt `geaendert` die
    /// Eigenschaft als number (für sich grün, mit `unveraendert` zusammen rot)
    /// und Bau 2 läuft. Gibt das Ergebnis von Bau 2 zurück.
    async fn gesamt_konflikt_aufbauen(r: &TestRepo, unveraendert: &str, geaendert: &str) -> BauErgebnis {
        let dir = r.0.join("projects");
        r.projekt(unveraendert, &probe_seite("Unverändert"));
        std::fs::write(dir.join(unveraendert).join("typen.ts"), global_probe("string")).unwrap();
        r.projekt(geaendert, &probe_seite("Geändert alt"));
        let e = bauen(&r.ort(), ZEITLIMIT).await;
        assert!(e.gruen && e.neu_rot.is_empty(), "{e:?}");
        assert!(r.ort().gruen().join(geaendert).join("Seite.svelte").is_file());
        tokio::time::sleep(Duration::from_millis(20)).await;
        r.projekt(geaendert, &probe_seite("Geändert neu"));
        std::fs::write(dir.join(geaendert).join("typen.ts"), global_probe("number")).unwrap();
        bauen(&r.ort(), ZEITLIMIT).await
    }

    /// Abnahme 4c, mit echtem svelte-check: Ein Projekt, das nur die
    /// Gesamtprüfung bricht (zwei Projekte erklären dieselbe globale
    /// Eigenschaft mit verschiedenem Typ; jedes für sich ist grün), fällt auf
    /// seine grüne Kopie zurück und bleibt in seiner letzten guten Fassung im
    /// Bau. Nachbesserung 2: Es trifft das geänderte Projekt, gleich wie die
    /// Namen sortieren; das unveränderte bleibt grün.
    async fn gesamt_konflikt(name: &str, unveraendert: &str, geaendert: &str) {
        if ohne_npm() {
            return;
        }
        let r = TestRepo::neu(name, "gruen");
        if !r.mit_svelte_check() {
            crate::ueberspringen("Node, web/node_modules oder die Junction fehlt.");
            return;
        }
        let dir = r.0.join("projects");
        let e = gesamt_konflikt_aufbauen(&r, unveraendert, geaendert).await;
        eprintln!("Bau 2 ({name}): {}; {}", e.satz, schritte_text(&e.schritte));
        assert!(e.gruen, "{e:?}");
        assert_eq!(e.alte_fassung, [geaendert], "{e:?}");
        assert!(e.rote.is_empty(), "{e:?}");
        assert_eq!(e.neu_rot.iter().map(|x| x.slug.as_str()).collect::<Vec<_>>(), [geaendert]);
        assert!(e.neu_rot[0].satz.contains("in diesem Bau steht ihre letzte gute Fassung"), "{}", e.neu_rot[0].satz);
        let kopie = scanner::import_pfad(&r.ort().gruen());
        let quelle = scanner::import_pfad(&dir);
        let liste = r.liste();
        assert!(liste.contains(&format!("import('{kopie}/{geaendert}/Seite.svelte')")), "{liste}");
        assert!(liste.contains(&format!("import('{quelle}/{unveraendert}/Seite.svelte')")), "das unveränderte bleibt aus seinem Ordner: {liste}");
        assert!(std::fs::read_to_string(r.ort().gruen().join(geaendert).join("Seite.svelte")).unwrap().contains("Geändert alt"), "die Kopie bleibt die alte");
        let s = Stand::laden(&r.ort().gruen());
        assert_eq!((s.projekte[geaendert].gruen, s.projekte[geaendert].rot_art), (false, Some(RotArt::Zusammenbau)));
        assert!(s.projekte[unveraendert].gruen, "das unveränderte bleibt grün");
    }

    /// Ein Bau ohne Änderung setzt
    /// `gebaut_um` neu, `gruen_um` bleibt; mit echtem svelte-check.
    #[tokio::test]
    async fn neu_generieren_ohne_aenderung_setzt_gebaut_um() {
        if ohne_npm() {
            return;
        }
        let r = TestRepo::neu("gebaut-um", "gruen");
        if !r.mit_svelte_check() {
            crate::ueberspringen("Node, web/node_modules oder die Junction fehlt.");
            return;
        }
        r.projekt("p", &probe_seite("Gebaut"));
        let e = bauen(&r.ort(), ZEITLIMIT).await;
        assert!(e.gruen, "{e:?}");
        let s = Stand::laden(&r.ort().gruen());
        let p = &s.projekte["p"];
        assert!(p.gruen && p.gruen_um.is_some() && p.gebaut_um.is_some(), "{p:?}");
        // Beide Zeiten um 100 s zurückstellen, statt zu warten.
        let mut alt = s.clone();
        let q = alt.projekte.get_mut("p").unwrap();
        q.gruen_um = q.gruen_um.map(|t| t - 100);
        q.gebaut_um = q.gebaut_um.map(|t| t - 100);
        let (gruen_vorher, gebaut_vorher) = (q.gruen_um, q.gebaut_um);
        alt.speichern(&r.ort().gruen()).unwrap();
        // Noch einmal bauen, an der Seite hat sich nichts geändert.
        let e = bauen(&r.ort(), ZEITLIMIT).await;
        assert!(e.gruen && !e.schritte.iter().any(|s| s.name.starts_with("Prüfung je Projekt")), "{e:?}");
        let p = Stand::laden(&r.ort().gruen()).projekte["p"].clone();
        assert_eq!(p.gruen_um, gruen_vorher, "gruen_um bleibt (in Ordnung seit)");
        assert!(p.gebaut_um.unwrap() >= gebaut_vorher.unwrap() + 100, "gebaut_um neu: {p:?}");
    }

    /// `gebaut_setzen`: nur die genannten Seiten mit Eintrag, nichts sonst.
    #[test]
    fn gebaut_setzen_nur_genannte() {
        let mut stand = Stand::default();
        stand.projekte.insert("a".into(), ProjektStand { abdruck: 1, gruen: true, gruen_um: Some(5), ..Default::default() });
        stand.projekte.insert(
            "b".into(),
            ProjektStand { abdruck: 2, gruen: false, gruen_um: Some(5), gebaut_um: Some(6), rot_art: Some(RotArt::Ausgeschlossen), ..Default::default() },
        );
        gebaut_setzen(&mut stand, ["a", "fremd"], 99);
        assert_eq!((stand.projekte["a"].gebaut_um, stand.projekte["a"].gruen_um), (Some(99), Some(5)));
        assert_eq!(stand.projekte["b"].gebaut_um, Some(6), "nicht im Bau: bleibt");
        assert!(!stand.projekte.contains_key("fremd"));
    }

    #[tokio::test]
    async fn nur_gesamtpruefung_rot_baut_aus_der_kopie() {
        gesamt_konflikt("nur-gesamt", "aa", "bb").await;
    }

    /// Nachbesserung 2: dieselbe Lage, das geänderte Projekt sortiert vorn.
    #[tokio::test]
    async fn nur_gesamtpruefung_rot_trifft_das_geaenderte_auch_vorn() {
        gesamt_konflikt("nur-gesamt-vorn", "bb", "aa").await;
    }

    /// Kleinkram 2 (Zurückgestellt klebt), mit echtem svelte-check: bb ist
    /// zurückgestellt (Konflikt mit aa). Danach passt der Rest (aa erklärt die
    /// Eigenschaft wie bb), ohne dass sich an bb etwas ändert: Der nächste Bau
    /// prüft bb trotzdem neu, die Gesamtprüfung ist grün, und bb ist nicht
    /// mehr zurückgestellt (grün, aus seinem Ordner, Kopie erneuert).
    #[tokio::test]
    async fn zurueckgestellt_wird_ohne_eigene_aenderung_wieder_gruen() {
        if ohne_npm() {
            return;
        }
        let r = TestRepo::neu("klebt", "gruen");
        if !r.mit_svelte_check() {
            crate::ueberspringen("Node, web/node_modules oder die Junction fehlt.");
            return;
        }
        gesamt_konflikt_aufbauen(&r, "aa", "bb").await;
        let s = Stand::laden(&r.ort().gruen());
        assert_eq!(s.projekte["bb"].rot_art, Some(RotArt::Zusammenbau), "Ausgangslage: bb zurückgestellt");
        let bb_vorher = abdruck(&r.0.join("projects"), "bb");

        // aa ändert sich so, dass bb wieder passt; bb selbst bleibt, wie es ist.
        tokio::time::sleep(Duration::from_millis(20)).await;
        std::fs::write(r.0.join("projects/aa/typen.ts"), global_probe("number")).unwrap();
        let e = bauen(&r.ort(), ZEITLIMIT).await;
        eprintln!("Bau 3 (klebt): {}; {}", e.satz, schritte_text(&e.schritte));
        assert_eq!(abdruck(&r.0.join("projects"), "bb"), bb_vorher, "an bb hat sich nichts geändert");
        assert!(e.gruen && e.alte_fassung.is_empty() && e.neu_rot.is_empty(), "{e:?}");
        assert!(e.schritte.iter().any(|s| s.name.starts_with("Prüfung je Projekt") && s.name.contains("bb")), "bb wurde neu geprüft: {:?}", e.schritte);
        let s = Stand::laden(&r.ort().gruen());
        assert_eq!((s.projekte["bb"].gruen, s.projekte["bb"].rot_art), (true, None), "nicht mehr zurückgestellt");
        let quelle = scanner::import_pfad(&r.0.join("projects"));
        assert!(r.liste().contains(&format!("import('{quelle}/bb/Seite.svelte')")), "{}", r.liste());
        assert!(std::fs::read_to_string(r.ort().gruen().join("bb").join("Seite.svelte")).unwrap().contains("Geändert neu"), "Kopie erneuert");

        // Grün und unverändert: der nächste Bau prüft bb nicht mehr.
        let e = bauen(&r.ort(), ZEITLIMIT).await;
        assert!(e.gruen && !e.schritte.iter().any(|s| s.name.starts_with("Prüfung je Projekt")), "{:?}", e.schritte);
    }

    /// Etappe 4b, mit echtem svelte-check und ESLint: Nur grüne Projekte
    /// kommen aus ihrem Ordner; ein rotes ohne Kopie fehlt, ein rotes mit
    /// Kopie bleibt in seiner letzten grünen Fassung (web/.projekte-gruen).
    /// Unverändertes wird nicht neu geprüft; die Liste nennt absolute Pfade.
    #[tokio::test]
    async fn nur_gruene_und_die_letzte_gruene_fassung() {
        if ohne_npm() {
            return;
        }
        let r = TestRepo::neu("nur-gruene", "gruen");
        if !r.mit_svelte_check() {
            crate::ueberspringen("Node, web/node_modules oder die Junction fehlt.");
            return;
        }
        let gut = "<script lang=\"ts\">\n\tconst titel: string = 'Gut';\n</script>\n\n<p>{titel}</p>\n";
        let kaputt = "<script lang=\"ts\">\n\tconst n: number = 'kein Wert';\n</script>\n\n<p>{n}</p>\n";
        r.projekt("gut", gut);
        r.projekt("typfehler", kaputt);
        let start = Instant::now();
        let e = bauen(&r.ort(), ZEITLIMIT).await;
        eprintln!("Bau 1: {:?}; {}; {}", start.elapsed(), e.satz, schritte_text(&e.schritte));
        assert!(e.gruen, "{e:?}");
        assert_eq!(e.rote, ["typfehler"], "{e:?}");
        assert_eq!(e.neu_rot.iter().map(|r| r.slug.as_str()).collect::<Vec<_>>(), ["typfehler"]);
        assert!(e.neu_rot[0].satz.starts_with("Die Seite von „typfehler“ hat einen Fehler und fehlt, bis sie in Ordnung ist."), "{}", e.neu_rot[0].satz);
        let quelle = scanner::import_pfad(&r.0.join("projects"));
        let liste = r.liste();
        assert!(liste.contains(&format!("import('{quelle}/gut/Seite.svelte')")) && !liste.contains("typfehler"), "{liste}");
        assert!(!liste.contains("import.meta"), "{liste}");
        assert!(r.0.join("web/.projekte-gruen/gut/Seite.svelte").is_file(), "grüne Kopie angelegt");
        assert!(!r.0.join("web/.projekte-gruen/typfehler").exists(), "rot: keine Kopie");

        // Unverändert: keine Prüfung je Projekt, kein neues Rot.
        let e = bauen(&r.ort(), ZEITLIMIT).await;
        assert!(e.gruen && e.neu_rot.is_empty(), "{e:?}");
        assert!(!e.schritte.iter().any(|s| s.name.starts_with("Prüfung je Projekt")), "{:?}", e.schritte);

        // gut wird rot: Es bleibt in seiner letzten grünen Fassung (Kopie).
        tokio::time::sleep(Duration::from_millis(20)).await;
        r.projekt("gut", kaputt);
        let e = bauen(&r.ort(), ZEITLIMIT).await;
        eprintln!("Bau 3: {}; {}", e.satz, schritte_text(&e.schritte));
        assert!(e.gruen, "{e:?}");
        assert_eq!(e.alte_fassung, ["gut"], "{e:?}");
        assert_eq!(e.neu_rot.iter().map(|r| r.slug.as_str()).collect::<Vec<_>>(), ["gut"]);
        assert!(e.neu_rot[0].satz.contains("bleibt in ihrer letzten guten Fassung"), "{}", e.neu_rot[0].satz);
        let kopie = scanner::import_pfad(&r.0.join("web/.projekte-gruen"));
        let liste = r.liste();
        assert!(liste.contains(&format!("import('{kopie}/gut/Seite.svelte')")), "{liste}");
        assert_eq!(std::fs::read_to_string(r.0.join("web/.projekte-gruen/gut/Seite.svelte")).unwrap(), gut, "die Kopie bleibt grün");

        // Wieder grün: aus der Quelle, Kopie erneuert.
        tokio::time::sleep(Duration::from_millis(20)).await;
        r.projekt("gut", &gut.replace("Gut", "Wieder gut"));
        let e = bauen(&r.ort(), ZEITLIMIT).await;
        assert!(e.gruen && e.alte_fassung.is_empty(), "{e:?}");
        assert!(r.liste().contains(&format!("import('{quelle}/gut/Seite.svelte')")));
        assert!(std::fs::read_to_string(r.0.join("web/.projekte-gruen/gut/Seite.svelte")).unwrap().contains("Wieder gut"));
        assert_eq!(r.zeiger(), e.ordner);
    }

    /// Prüfen geht nicht (web/node_modules fehlt), es gibt aber Projekte: Es
    /// wird nicht gebaut, die letzte gute Fassung bleibt.
    #[tokio::test]
    async fn gescheiterte_pruefung_laesst_die_alte_fassung() {
        if ohne_npm() {
            return;
        }
        let r = TestRepo::neu("leer", "gruen");
        let vorher = bauen(&r.ort(), ZEITLIMIT).await;
        assert!(vorher.gruen, "ohne Projekte baut es auch ohne Werkzeuge: {vorher:?}");
        r.projekt("gut", "<p>Gut</p>\n");
        let e = bauen(&r.ort(), ZEITLIMIT).await;
        assert!(!e.gruen, "{e:?}");
        assert!(e.satz.starts_with("Nicht gebaut: Die Prüfung lässt sich nicht machen: svelte-check ist nicht da"), "{}", e.satz);
        assert!(e.satz.ends_with("Die letzte gute Fassung bleibt."), "{}", e.satz);
        assert_eq!(r.zeiger(), vorher.ordner, "Zeiger unverändert");
    }

    /// Die Gesamtprüfung: Das Projekt der ersten Meldung fliegt raus, und sie
    /// läuft noch einmal; höchstens zwei Runden, dann rot.
    #[test]
    fn gesamtpruefung_hoechstens_zwei_runden() {
        use crate::inhalt::pruefung::Befund;
        let e = |s: &[&str]| -> Vec<(String, PathBuf)> { s.iter().map(|s| (s.to_string(), PathBuf::from(format!("C:/q/{s}")))).collect() };
        let fund = |slug: &str, fehler: bool| CheckFund {
            slug: slug.into(),
            befund: if fehler {
                Befund::fehler(format!("projects/{slug}/Seite.svelte"), Some(1), "svelte-check: kaputt")
            } else {
                Befund::hinweis(format!("projects/{slug}/Seite.svelte"), Some(1), "nur Hinweis")
            },
        };
        // Grün sofort: ein Lauf.
        let g = gesamtpruefung(e(&["a", "b"]), |_| Ok(vec![fund("a", false)]), |_, _| None, &BTreeSet::new(), |_, _| false);
        assert_eq!((g.laeufe, g.fehler, g.ausgeschlossen.len(), g.eintraege.len()), (1, None, 0, 2));
        // Erst rot durch b, dann grün: b fliegt raus, zwei Läufe.
        let mut runden = 0;
        let g = gesamtpruefung(
            e(&["a", "b", "c"]),
            |ein| {
                runden += 1;
                Ok(if ein.iter().any(|(s, _)| s == "b") { vec![fund("a", false), fund("b", true), fund("c", true)] } else { vec![] })
            },
            |_, _| None,
            &BTreeSet::new(),
            |_, _| false,
        );
        assert_eq!(g.laeufe, 2);
        assert_eq!(g.fehler, None);
        assert_eq!(g.eintraege.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(), ["a", "c"]);
        assert_eq!(g.ausgeschlossen.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(), ["b"]);
        // Immer rot: zwei ausgeschlossen, dann Schluss (drei Läufe).
        let g =
            gesamtpruefung(e(&["a", "b", "c", "d"]), |ein| Ok(ein.iter().map(|(s, _)| fund(s, true)).collect()), |_, _| None, &BTreeSet::new(), |_, _| false);
        assert_eq!(g.laeufe, PRUEF_RUNDEN + 1);
        assert_eq!(g.ausgeschlossen.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(), ["a", "b"]);
        assert!(
            g.fehler.as_deref().is_some_and(|f| f.starts_with("Die Gesamtprüfung bleibt rot, auch nach 2 ausgeschlossenen oder zurückgesetzten Projekten.")),
            "{g:?}"
        );
        // Lief nicht: der Grund, nichts ausgeschlossen.
        let g = gesamtpruefung(e(&["a"]), |_| Err("svelte-check fehlt".into()), |_, _| None, &BTreeSet::new(), |_, _| false);
        assert_eq!((g.fehler.as_deref(), g.ausgeschlossen.len()), (Some("svelte-check fehlt"), 0));
        // Ein Fehler, der keinem Projekt der Liste gehört, schließt nichts aus.
        let g = gesamtpruefung(e(&["a"]), |_| Ok(vec![fund("weg", true)]), |_, _| None, &BTreeSet::new(), |_, _| false);
        assert!(g.fehler.is_some() && g.ausgeschlossen.is_empty());
    }

    /// Abnahme 4c (Nachbesserung 2): Bei einem Konflikt zwischen einem
    /// unveränderten (vorn) und einem geänderten Projekt greift die
    /// Gesamtprüfung beim geänderten ein, auch wenn die erste Meldung dem
    /// unveränderten gehört. Ohne geändertes Projekt mit Fehler gilt die erste Meldung.
    #[test]
    fn gesamtpruefung_greift_zuerst_beim_geaenderten_ein() {
        use crate::inhalt::pruefung::Befund;
        let e = |s: &[&str]| -> Vec<(String, PathBuf)> { s.iter().map(|s| (s.to_string(), PathBuf::from(format!("C:/q/{s}")))).collect() };
        let fehler = |slug: &str| CheckFund { slug: slug.into(), befund: Befund::fehler(format!("projects/{slug}/x.ts"), Some(1), "Konflikt") };
        let kopie = |slug: &str, ordner: &Path| {
            let k = PathBuf::from(format!("C:/gruen/{slug}"));
            (ordner != k).then_some(k)
        };
        // Konflikt, solange „zz“ (geändert) aus der Quelle kommt; die Meldungen nennen „aa“ zuerst.
        let konflikt = |ein: &[(String, PathBuf)]| -> Result<Vec<CheckFund>, String> {
            let neu = ein.iter().any(|(s, o)| s == "zz" && o.starts_with("C:/q"));
            Ok(if neu { vec![fehler("aa"), fehler("zz")] } else { vec![] })
        };
        let geaendert = BTreeSet::from(["zz".to_string()]);
        let g = gesamtpruefung(e(&["aa", "zz"]), konflikt, kopie, &geaendert, |_, _| false);
        assert_eq!((g.laeufe, g.fehler.clone()), (2, None));
        assert_eq!(g.zurueck.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(), ["zz"], "{g:?}");
        assert_eq!(g.eintraege, vec![("aa".to_string(), PathBuf::from("C:/q/aa")), ("zz".to_string(), PathBuf::from("C:/gruen/zz"))]);
        // Ohne Angabe, was geändert ist: wie bisher die erste Meldung (aa).
        let g = gesamtpruefung(
            e(&["aa", "zz"]),
            |ein| Ok(if ein.len() == 2 && ein[0].1.starts_with("C:/q") { vec![fehler("aa"), fehler("zz")] } else { vec![] }),
            kopie,
            &BTreeSet::new(),
            |_, _| false,
        );
        assert_eq!(g.zurueck.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(), ["aa"]);
        // svelte-check meldet den Konflikt nur beim unveränderten (wie echt bei
        // der späteren Erklärung): trotzdem zuerst das geänderte.
        let nur_bei_aa = |ein: &[(String, PathBuf)]| -> Result<Vec<CheckFund>, String> {
            let neu = ein.iter().any(|(s, o)| s == "zz" && o.starts_with("C:/q"));
            Ok(if neu { vec![fehler("aa")] } else { vec![] })
        };
        let g = gesamtpruefung(e(&["aa", "zz"]), nur_bei_aa, kopie, &geaendert, |_, _| false);
        assert_eq!((g.laeufe, g.fehler.clone()), (2, None));
        assert_eq!(g.zurueck.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(), ["zz"], "{g:?}");
        // Hilft das geänderte nicht (der Fehler liegt doch beim anderen), dann die erste Meldung.
        let g = gesamtpruefung(
            e(&["aa", "bb"]),
            |ein| Ok(if ein[0].1.starts_with("C:/q") { vec![fehler("aa")] } else { vec![] }),
            kopie,
            &BTreeSet::from(["bb".to_string()]),
            |_, _| false,
        );
        assert_eq!((g.laeufe, g.fehler.clone()), (3, None));
        assert_eq!(g.zurueck.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(), ["bb", "aa"]);
    }

    /// Abnahme 4c (Auflage aus 4b): Ein Projekt, das nur die Gesamtprüfung
    /// bricht, fällt auf seine grüne Kopie zurück und wird aus ihr gebaut.
    /// Ohne Kopie, oder passt auch die Kopie nicht, fliegt es raus.
    #[test]
    fn gesamtpruefung_faellt_auf_die_gruene_kopie_zurueck() {
        use crate::inhalt::pruefung::Befund;
        let e = |s: &[&str]| -> Vec<(String, PathBuf)> { s.iter().map(|s| (s.to_string(), PathBuf::from(format!("C:/q/{s}")))).collect() };
        let fehler = |slug: &str| CheckFund { slug: slug.into(), befund: Befund::fehler(format!("projects/{slug}/Seite.svelte"), Some(1), "passt nicht") };
        let kopie = |slug: &str, ordner: &Path| {
            let k = PathBuf::from(format!("C:/gruen/{slug}"));
            (slug != "ohne" && ordner != k).then_some(k)
        };
        // b bricht nur mit der neuen Fassung (aus der Quelle): zurück auf die Kopie, zwei Läufe.
        let g = gesamtpruefung(
            e(&["a", "b"]),
            |ein| Ok(ein.iter().filter(|(s, o)| s == "b" && o.starts_with("C:/q")).map(|(s, _)| fehler(s)).collect()),
            kopie,
            &BTreeSet::new(),
            |_, _| false,
        );
        assert_eq!((g.laeufe, g.fehler.clone()), (2, None));
        assert_eq!(g.eintraege, vec![("a".to_string(), PathBuf::from("C:/q/a")), ("b".to_string(), PathBuf::from("C:/gruen/b"))]);
        assert_eq!(g.zurueck.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(), ["b"]);
        assert!(g.ausgeschlossen.is_empty() && g.eingegriffen());
        // Auch die Kopie passt nicht: dann ganz raus (nicht mehr zurückgesetzt).
        let g = gesamtpruefung(
            e(&["a", "b"]),
            |ein| Ok(ein.iter().filter(|(s, _)| s == "b").map(|(s, _)| fehler(s)).collect()),
            kopie,
            &BTreeSet::new(),
            |_, _| false,
        );
        assert_eq!((g.laeufe, g.fehler.clone()), (3, None));
        assert_eq!(g.eintraege.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(), ["a"]);
        assert!(g.zurueck.is_empty(), "{g:?}");
        assert_eq!(g.ausgeschlossen.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(), ["b"]);
        // Ohne Kopie: gleich raus, wie bisher.
        let g = gesamtpruefung(
            e(&["a", "ohne"]),
            |ein| Ok(ein.iter().filter(|(s, _)| s == "ohne").map(|(s, _)| fehler(s)).collect()),
            kopie,
            &BTreeSet::new(),
            |_, _| false,
        );
        assert_eq!((g.laeufe, g.ausgeschlossen.len(), g.zurueck.len()), (2, 1, 0));
        // Wird schon aus der Kopie gebaut (rot je Projekt): nichts zurückzusetzen, raus.
        let schon = vec![("b".to_string(), PathBuf::from("C:/gruen/b"))];
        let g = gesamtpruefung(schon, |ein| Ok(ein.iter().map(|(s, _)| fehler(s)).collect()), kopie, &BTreeSet::new(), |_, _| false);
        assert_eq!((g.ausgeschlossen.len(), g.zurueck.len(), g.eintraege.len()), (1, 0, 0));
    }

    // ----- Kleinkram 2: falscher Schuldiger -----

    fn eintraege_q(s: &[&str]) -> Vec<(String, PathBuf)> {
        s.iter().map(|s| (s.to_string(), PathBuf::from(format!("C:/q/{s}")))).collect()
    }

    fn kopie_gruen(slug: &str, ordner: &Path) -> Option<PathBuf> {
        let k = PathBuf::from(format!("C:/gruen/{slug}"));
        (ordner != k).then_some(k)
    }

    /// Fund in „slug“, solange es aus der Quelle (`C:/q`) kommt.
    fn fund_aus_quelle(slug: &str, ein: &[(String, PathBuf)]) -> Vec<CheckFund> {
        use crate::inhalt::pruefung::Befund;
        ein.iter()
            .filter(|(s, o)| s == slug && o.starts_with("C:/q"))
            .map(|(s, _)| CheckFund {
                slug: s.clone(),
                befund: Befund::fehler(format!("projects/{s}/Seite.svelte"), Some(12), "Cannot find module './orte.ts'"),
            })
            .collect()
    }

    /// Kleinkram 2 (früherer Bau): Zwei Projekte sind zugleich geändert. A
    /// (seite) ist in dieser Runde geprüft; B (probe) wurde während
    /// des Baus gespeichert und ist nur seit seiner letzten grünen Fassung anders
    /// (Abdruck). Der Fehler liegt nur in B: B wird zurückgestellt, A bleibt
    /// grün aus seinem Ordner. Früher traf es A.
    #[test]
    fn gesamtpruefung_stellt_das_geaenderte_projekt_mit_dem_fund_zurueck() {
        let geaendert = BTreeSet::from(["seite".to_string()]);
        // B ist geändert, solange es aus der Quelle kommt (seine Kopie ist die letzte grüne Fassung).
        let seit_gruen = |s: &str, o: &Path| s == "probe" && o.starts_with("C:/q");
        let g = gesamtpruefung(eintraege_q(&["probe", "seite"]), |ein| Ok(fund_aus_quelle("probe", ein)), kopie_gruen, &geaendert, seit_gruen);
        assert_eq!((g.laeufe, g.fehler.clone()), (2, None), "{g:?}");
        assert_eq!(g.zurueck.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(), ["probe"], "{g:?}");
        assert!(g.ausgeschlossen.is_empty() && g.ausloeser.is_empty(), "{g:?}");
        assert_eq!(
            g.eintraege,
            vec![("probe".to_string(), PathBuf::from("C:/gruen/probe")), ("seite".to_string(), PathBuf::from("C:/q/seite"))],
            "A bleibt aus seinem Ordner"
        );
        // Auch wenn beide in dieser Runde geprüft sind: es trifft B, wo der Fund liegt.
        let beide = BTreeSet::from(["seite".to_string(), "probe".to_string()]);
        let g = gesamtpruefung(eintraege_q(&["seite", "probe"]), |ein| Ok(fund_aus_quelle("probe", ein)), kopie_gruen, &beide, |_, _| false);
        assert_eq!(g.zurueck.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(), ["probe"], "{g:?}");
        // Der Satz nennt B als Verursacher, mit dem Fund in B.
        let satz = satz_zusammenbau("probe", &g.zurueck[0].1, g.ausloeser.get("probe").map(String::as_str), false);
        assert_eq!(
            satz,
            "Die Seite von „probe“ passt nicht zum Rest; in diesem Bau steht ihre letzte gute Fassung. Fund: projects/probe/Seite.svelte, Zeile 12: Cannot find module './orte.ts'"
        );
    }

    /// Kleinkram 2: B ist unverändert (seit seiner letzten grünen Fassung),
    /// der Fund liegt trotzdem in B: Auslöser ist die Änderung in A (etwa ein
    /// geteilter Typ). Dann bleibt A der Zurückgestellte, B bleibt grün, und
    /// der Satz nennt A als Verursacher und B als Fundort.
    #[test]
    fn gesamtpruefung_unveraendertes_b_trifft_den_ausloeser_a() {
        let geaendert = BTreeSet::from(["mail".to_string()]);
        // Fund in B, solange A aus der Quelle kommt.
        let pruefen = |ein: &[(String, PathBuf)]| -> Result<Vec<CheckFund>, String> {
            use crate::inhalt::pruefung::Befund;
            let a_neu = ein.iter().any(|(s, o)| s == "mail" && o.starts_with("C:/q"));
            Ok(if a_neu {
                vec![CheckFund {
                    slug: "beispiel-seite".into(),
                    befund: Befund::fehler("projects/beispiel-seite/Seite.svelte", Some(24), "'Orte' is declared but its value is never read."),
                }]
            } else {
                vec![]
            })
        };
        let g = gesamtpruefung(eintraege_q(&["beispiel-seite", "mail"]), pruefen, kopie_gruen, &geaendert, |_, _| false);
        assert_eq!((g.laeufe, g.fehler.clone()), (2, None), "{g:?}");
        assert_eq!(g.zurueck.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(), ["mail"], "{g:?}");
        assert_eq!(g.ausloeser.get("mail").map(String::as_str), Some("beispiel-seite"));
        assert_eq!(g.eintraege[0], ("beispiel-seite".to_string(), PathBuf::from("C:/q/beispiel-seite")), "B bleibt aus seinem Ordner");
        let satz = satz_zusammenbau("mail", &g.zurueck[0].1, g.ausloeser.get("mail").map(String::as_str), false);
        assert!(satz.starts_with("Die Seite von „mail“ passt nicht zum Rest: Ihre Änderung bricht die unveränderte Seite „beispiel-seite“; in diesem Bau steht ihre letzte gute Fassung. Fund: projects/beispiel-seite/Seite.svelte, Zeile 24"), "{satz}");
        let raus = satz_zusammenbau("mail", "x", Some("beispiel-seite"), true);
        assert!(raus.contains("„beispiel-seite“; sie ist in diesem Bau nicht dabei."), "{raus}");
    }

    /// Kleinkram 2: Mehrere Fundorte, je Fundort der richtige Verursacher. B
    /// (geändert) hat einen eigenen Fund, C (unverändert) einen, den die
    /// Änderung in A auslöst: B und A werden zurückgestellt, C nicht.
    #[test]
    fn gesamtpruefung_je_fundort() {
        use crate::inhalt::pruefung::Befund;
        let geaendert = BTreeSet::from(["a".to_string(), "b".to_string()]);
        let pruefen = |ein: &[(String, PathBuf)]| -> Result<Vec<CheckFund>, String> {
            let aus_quelle = |slug: &str| ein.iter().any(|(s, o)| s == slug && o.starts_with("C:/q"));
            let mut f = Vec::new();
            if aus_quelle("a") {
                f.push(CheckFund { slug: "c".into(), befund: Befund::fehler("projects/c/Seite.svelte", Some(1), "Typ passt nicht") });
            }
            if aus_quelle("b") {
                f.push(CheckFund { slug: "b".into(), befund: Befund::fehler("projects/b/Seite.svelte", Some(2), "kaputt") });
            }
            Ok(f)
        };
        let g = gesamtpruefung(eintraege_q(&["a", "b", "c"]), pruefen, kopie_gruen, &geaendert, |_, _| false);
        assert_eq!((g.laeufe, g.fehler.clone()), (3, None), "{g:?}");
        assert_eq!(g.zurueck.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(), ["b", "a"], "{g:?}");
        assert_eq!(g.ausloeser, BTreeMap::from([("a".to_string(), "c".to_string())]));
        assert_eq!(g.eintraege[2], ("c".to_string(), PathBuf::from("C:/q/c")), "C bleibt");
    }

    /// Kleinkram 2: Geändert seit der letzten grünen Fassung nach Abdruck;
    /// aus der Kopie gebaut nie, ohne grüne Fassung immer.
    #[test]
    fn seit_gruen_geaendert_nach_abdruck() {
        let r = TestRepo::neu("seit-gruen", "gruen");
        r.projekt("p", "<p>grün</p>\n");
        let dir = r.0.join("projects");
        let gruen = r.ort().gruen();
        let letzte: LetzteGruene = BTreeMap::from([("p".to_string(), (dir.clone(), Some(abdruck(&dir, "p")))), ("neu".to_string(), (dir.clone(), None))]);
        assert!(!seit_gruen_geaendert(&letzte, &gruen, "p", &dir.join("p")));
        assert!(!seit_gruen_geaendert(&letzte, &gruen, "fremd", &dir.join("fremd")));
        assert!(seit_gruen_geaendert(&letzte, &gruen, "neu", &dir.join("neu")));
        std::fs::write(dir.join("p/Seite.svelte"), "<p>während des Baus gespeichert</p>\n").unwrap();
        assert!(seit_gruen_geaendert(&letzte, &gruen, "p", &dir.join("p")));
        assert!(!seit_gruen_geaendert(&letzte, &gruen, "p", &gruen.join("p")), "die Kopie ist die letzte grüne Fassung");
        // Aus dem Stand: grüne Einträge ihr Abdruck, andere ihr `gruen_abdruck`.
        let mut stand = Stand::default();
        stand.projekte.insert(
            "p".into(),
            ProjektStand {
                abdruck: 7,
                umgebung: 0,
                gruen: true,
                satz: String::new(),
                gruen_um: None,
                rot_art: None,
                gebaut_um: None,
                gruen_abdruck: None,
                versucht_um: None,
            },
        );
        stand.projekte.insert(
            "rot".into(),
            ProjektStand {
                abdruck: 8,
                umgebung: 0,
                gruen: false,
                satz: "x".into(),
                gruen_um: None,
                rot_art: Some(RotArt::Pruefung),
                gebaut_um: None,
                gruen_abdruck: None,
                versucht_um: None,
            },
        );
        stand.projekte.insert(
            "nicht".into(),
            ProjektStand { abdruck: 9, gruen: false, rot_art: Some(RotArt::NichtGeprueft), gruen_abdruck: Some(6), ..Default::default() },
        );
        let l = letzte_gruene(&[("p".into(), dir.clone()), ("rot".into(), dir.clone()), ("nicht".into(), dir.clone())], &stand);
        assert_eq!((l["p"].1, l["rot"].1, l["nicht"].1), (Some(7), None, Some(6)));
    }

    // ----- Kleinkram 2: zurückgestellt klebt -----

    /// Zurückgestellte und ausgeschlossene Projekte prüft jeder Bau neu, auch
    /// ohne Dateiänderung; rot aus der eigenen Prüfung und grüne nur bei Änderung.
    #[test]
    fn zurueckgestellte_werden_neu_geprueft() {
        let p = |gruen: bool, rot_art: Option<RotArt>| ProjektStand {
            abdruck: 5,
            umgebung: 0,
            gruen,
            satz: String::new(),
            gruen_um: None,
            rot_art,
            gebaut_um: None,
            gruen_abdruck: None,
            versucht_um: None,
        };
        assert!(neu_pruefen(None, 5, 0), "nie geprüft");
        assert!(!neu_pruefen(Some(&p(true, None)), 5, 0), "grün und unverändert");
        assert!(neu_pruefen(Some(&p(true, None)), 6, 0), "geändert");
        assert!(neu_pruefen(Some(&p(false, Some(RotArt::Zusammenbau))), 5, 0), "zurückgestellt");
        assert!(neu_pruefen(Some(&p(false, Some(RotArt::Ausgeschlossen))), 5, 0), "ausgeschlossen");
        assert!(!neu_pruefen(Some(&p(false, Some(RotArt::Pruefung))), 5, 0), "rot aus der eigenen Prüfung bleibt bis zur Änderung");
        assert!(!neu_pruefen(Some(&p(false, None)), 5, 0), "rot von vor 4c ohne Art");
        // Zweig A: Umgebung geändert, und nicht geprüft prüft jeder Bau neu.
        assert!(neu_pruefen(Some(&p(true, None)), 5, 1), "Umgebung geändert");
        assert!(neu_pruefen(Some(&p(false, Some(RotArt::Pruefung))), 5, 1), "rot, Umgebung geändert");
        assert!(neu_pruefen(Some(&p(false, Some(RotArt::NichtGeprueft))), 5, 0), "nicht geprüft");
    }

    /// Abnahme 4c: Nach der Gesamtprüfung steht ein zurückgesetztes Projekt
    /// in der letzten guten Fassung (rot, Art `zusammenbau`, Zeit bleibt), ein
    /// ausgeschlossenes fehlt (Art `ausgeschlossen`); beide werden nicht grün
    /// und ihre Kopie bleibt; die Bauliste nennt die Kopie.
    #[test]
    fn abschliessen_merkt_zurueckgesetzt_und_ausgeschlossen() {
        let r = TestRepo::neu("abschluss", "gruen");
        let gut = "<p>alt und gut</p>\n";
        for s in ["zurueck", "raus"] {
            r.projekt(s, gut);
            kopie_erneuern(&r.0.join("projects"), s, &r.ort().gruen()).unwrap();
            r.projekt(s, "<p>neu, passt nicht</p>\n");
        }
        let dir = r.0.join("projects");
        let um = Some(1_793_450_700);
        let mut stand = Stand::default();
        for s in ["zurueck", "raus"] {
            stand.projekte.insert(
                s.into(),
                ProjektStand {
                    abdruck: 1,
                    umgebung: 0,
                    gruen: true,
                    satz: String::new(),
                    gruen_um: um,
                    rot_art: None,
                    gebaut_um: um,
                    gruen_abdruck: None,
                    versucht_um: None,
                },
            );
        }
        let frisch: Vec<(String, PathBuf, u64)> = ["zurueck", "raus"].iter().map(|s| (s.to_string(), dir.clone(), abdruck(&dir, s))).collect();
        let v = Vorbereitet {
            eintraege: Vec::new(),
            neu_rot: Vec::new(),
            alte_fassung: Vec::new(),
            rote: Vec::new(),
            nicht_geprueft: Vec::new(),
            stil: Vec::new(),
            schritte: Vec::new(),
            gescheitert: None,
            gesamt_rot: None,
            liste_neu: false,
        };
        let z = Zwischen {
            projekte: vec![("zurueck".into(), dir.clone()), ("raus".into(), dir.clone())],
            umgebung: [0, 0],
            stand,
            frisch_gruen: frisch,
            eintraege: Vec::new(),
            v,
        };
        let kopie = r.ort().gruen().join("zurueck");
        let g = Gesamt {
            eintraege: vec![("zurueck".into(), kopie.clone())],
            ausgeschlossen: vec![("raus".into(), "projects/raus/Seite.svelte, Zeile 1: passt nicht".into())],
            zurueck: vec![("zurueck".into(), "projects/zurueck/Seite.svelte, Zeile 1: passt nicht".into())],
            fehler: None,
            laeufe: 3,
            ausloeser: BTreeMap::new(),
        };
        let v = abschliessen(&r.ort(), z, g);
        assert_eq!(v.alte_fassung, ["zurueck"]);
        assert_eq!(v.rote, ["raus"]);
        assert_eq!(v.neu_rot.iter().map(|x| x.slug.as_str()).collect::<Vec<_>>(), ["zurueck", "raus"]);
        assert!(v.neu_rot[0].satz.contains("in diesem Bau steht ihre letzte gute Fassung"), "{}", v.neu_rot[0].satz);
        let s = Stand::laden(&r.ort().gruen());
        let z = &s.projekte["zurueck"];
        assert_eq!((z.gruen, z.rot_art, z.gruen_um), (false, Some(RotArt::Zusammenbau), um));
        let x = &s.projekte["raus"];
        assert_eq!((x.gruen, x.rot_art, x.gruen_um), (false, Some(RotArt::Ausgeschlossen), um));
        assert_eq!((z.gebaut_um, x.gebaut_um), (um, um), "gebaut_um bleibt beim Zurücksetzen und Ausschließen");
        for s in ["zurueck", "raus"] {
            assert_eq!(std::fs::read_to_string(r.ort().gruen().join(s).join("Seite.svelte")).unwrap(), gut, "{s}: die Kopie bleibt die gute");
        }
        let liste = r.liste();
        let quelle = scanner::import_pfad(&r.ort().gruen());
        assert!(liste.contains(&format!("import('{quelle}/zurueck/Seite.svelte')")) && !liste.contains("raus"), "{liste}");
    }

    // ----- Zweig A: übersprungen heißt nicht geprüft; Umgebungs-Abdruck -----

    impl TestRepo {
        /// Attrappen der Pakete, die `werkzeuge_fehlen` sucht (die Prüfung
        /// selbst wird eingesetzt). `false`, wenn Node fehlt.
        fn werkzeuge_attrappe(&self) -> bool {
            if programme::pfad_von(programme::Werkzeug::Node).is_none() {
                crate::ueberspringen("Node fehlt.");
                return false;
            }
            for (paket, datei) in [
                ("svelte-check", "bin/svelte-check"),
                ("eslint", "bin/eslint.js"),
                ("stylelint", "package.json"),
                ("postcss-html", "package.json"),
                ("vitest", "vitest.mjs"),
            ] {
                let p = self.0.join("web/node_modules").join(paket).join(datei);
                std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                std::fs::write(p, "").unwrap();
            }
            for datei in ["eslint.projekte.config.js", "vitest.projekte.config.ts", "scripts/stil-projekte.mjs"] {
                let p = self.0.join("web").join(datei);
                std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                std::fs::write(p, "").unwrap();
            }
            assert_eq!(werkzeuge_fehlen(&self.0.join("web")), None);
            true
        }
    }

    fn gesamt_gruen(eintraege: Vec<(String, PathBuf)>) -> Gesamt {
        Gesamt { eintraege, ausgeschlossen: Vec::new(), zurueck: Vec::new(), fehler: None, laeufe: 1, ausloeser: BTreeMap::new() }
    }

    fn ohne_befund(_: &Path, _: &Path, _: &str) -> Vec<crate::inhalt::pruefung::Befund> {
        Vec::new()
    }

    /// Schritte 1 und 2 nur mit `pruefe_voll` je Projekt (ohne Sammelprüfung, Frist weit weg).
    fn pruefen_je_projekt(ort: &BauOrt, pruefe_voll: PruefeVoll) -> Zwischen {
        pruefen_mit(ort, pruefe_voll, None, Instant::now() + PRUEFUNG_HOECHSTENS)
    }

    // ----- Zweig B: Sammelprüfung, Warteschlange, Frist -----

    /// Zweig B: Ist die Frist (`PRUEFUNG_HOECHSTENS`) um, startet keine
    /// Prüfung mehr; die Projekte sind nicht geprüft (Kopie oder weg), nie grün
    /// und nie rot.
    #[test]
    fn nach_der_frist_nicht_geprueft() {
        let r = TestRepo::neu("frist", "gruen");
        if !r.werkzeuge_attrappe() {
            return;
        }
        let (dir, gruen) = (r.0.join("projects"), r.ort().gruen());
        r.projekt("mit", "<p>alt und gut</p>\n");
        kopie_erneuern(&dir, "mit", &gruen).unwrap();
        r.projekt("mit", "<p>neu</p>\n");
        r.projekt("ohne", "<p>neu</p>\n");
        let gerufen = std::sync::atomic::AtomicUsize::new(0);
        let zaehlt = |_: &Path, _: &Path, _: &str| -> Vec<crate::inhalt::pruefung::Befund> {
            gerufen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Vec::new()
        };
        let z = pruefen_mit(&r.ort(), &zaehlt, None, Instant::now());
        assert_eq!(gerufen.load(std::sync::atomic::Ordering::SeqCst), 0, "nach der Frist startet nichts");
        assert!(z.frisch_gruen.is_empty() && z.v.rote.is_empty() && z.v.neu_rot.is_empty(), "nicht grün, nicht rot");
        assert_eq!(z.v.nicht_geprueft, ["mit", "ohne"]);
        assert_eq!(z.eintraege, vec![("mit".to_string(), gruen.join("mit"))], "nur die Kopie");
        assert!(z.stand.projekte["ohne"].satz.contains("Frist"), "{}", z.stand.projekte["ohne"].satz);
    }

    /// Zweig B: Ab `SAMMEL_AB` geänderten Projekten prüft die Sammelprüfung
    /// sie zusammen, darunter `pruefe_voll` je Projekt. Liefert die
    /// Sammelprüfung zu wenig, ist der Rest nicht geprüft, nie grün.
    #[test]
    fn ab_sammel_ab_gesammelt() {
        let r = TestRepo::neu("sammel-weg", "gruen");
        if !r.werkzeuge_attrappe() {
            return;
        }
        let je = std::sync::atomic::AtomicUsize::new(0);
        let voll = |_: &Path, _: &Path, _: &str| -> Vec<crate::inhalt::pruefung::Befund> {
            je.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Vec::new()
        };
        let gesammelt = std::sync::Mutex::new(Vec::new());
        let sammel = |_: &Path, p: &[(String, PathBuf)], _: Instant| -> Vec<Vec<crate::inhalt::pruefung::Befund>> {
            gesammelt.lock().unwrap().push(p.iter().map(|(s, _)| s.clone()).collect::<Vec<_>>());
            vec![Vec::new(); p.len() - 1]
        };
        for s in ["a", "b", "c"] {
            r.projekt(s, "<p>x</p>\n");
        }
        let z = pruefen_mit(&r.ort(), &voll, Some(&sammel), Instant::now() + PRUEFUNG_HOECHSTENS);
        assert_eq!((je.load(std::sync::atomic::Ordering::SeqCst), z.frisch_gruen.len()), (3, 3), "drei: je Projekt");
        assert!(gesammelt.lock().unwrap().is_empty());
        for s in ["a", "b", "c", "d"] {
            r.projekt(s, &format!("<p>{s} neu</p>\n"));
        }
        let z = pruefen_mit(&r.ort(), &voll, Some(&sammel), Instant::now() + PRUEFUNG_HOECHSTENS);
        assert_eq!(je.load(std::sync::atomic::Ordering::SeqCst), 3, "vier: nicht mehr je Projekt");
        assert_eq!(*gesammelt.lock().unwrap(), [["a", "b", "c", "d"]]);
        assert_eq!(z.frisch_gruen.iter().map(|(s, _, _)| s.as_str()).collect::<Vec<_>>(), ["a", "b", "c"]);
        assert_eq!(z.v.nicht_geprueft, ["d"], "ohne Ergebnis nicht geprüft");
    }

    /// Bau-Tempo, Zweig C: Der Fortschritt zählt je fertigem Projekt der
    /// Warteschlange von 0 bis n; die Sammelprüfung ist ein Schritt mit n
    /// Seiten. Ein eigener Zähler statt `FORTSCHRITT` (andere Tests bauen daneben).
    #[test]
    fn fortschritt_zaehlt_die_pruefung() {
        let r = TestRepo::neu("fortschritt", "gruen");
        if !r.werkzeuge_attrappe() {
            return;
        }
        let f = Fortschritt(Mutex::new(None));
        let gesehen = Mutex::new(Vec::new());
        let voll = |_: &Path, _: &Path, _: &str| -> Vec<crate::inhalt::pruefung::Befund> {
            gesehen.lock().unwrap().push(f.jetzt());
            Vec::new()
        };
        let sammel = |_: &Path, p: &[(String, PathBuf)], _: Instant| -> Vec<Vec<crate::inhalt::pruefung::Befund>> {
            gesehen.lock().unwrap().push(f.jetzt());
            vec![Vec::new(); p.len()]
        };
        for s in ["a", "b", "c"] {
            r.projekt(s, "<p>x</p>\n");
        }
        pruefen_mit_fortschritt(&r.ort(), &voll, Some(&sammel), Instant::now() + PRUEFUNG_HOECHSTENS, &f);
        let je = |fertig| Some(BauFortschritt { schritt: FortschrittSchritt::Pruefung, fertig, gesamt: 3 });
        assert_eq!(f.jetzt(), je(3), "nach drei Projekten 3 von 3");
        let mut v = gesehen.lock().unwrap().drain(..).collect::<Vec<_>>();
        v.sort_by_key(|x| x.map(|x| x.fertig));
        assert!(v.len() == 3 && v.iter().all(|x| x.is_some_and(|x| x.schritt == FortschrittSchritt::Pruefung && x.gesamt == 3 && x.fertig < 3)), "{v:?}");
        assert_eq!(v[0], je(0), "zu Beginn 0 von 3");
        for s in ["a", "b", "c", "d"] {
            r.projekt(s, &format!("<p>{s} neu</p>\n"));
        }
        pruefen_mit_fortschritt(&r.ort(), &voll, Some(&sammel), Instant::now() + PRUEFUNG_HOECHSTENS, &f);
        let sammel_schritt = Some(BauFortschritt { schritt: FortschrittSchritt::Sammelpruefung, fertig: 0, gesamt: 4 });
        assert_eq!(*gesehen.lock().unwrap(), [sammel_schritt], "Sammelprüfung: ein Schritt mit 4 Seiten");
        // Die Zahl bleibt in ihren Grenzen, wenn mehr fertig meldet als gesamt.
        f.schritt(FortschrittSchritt::Pruefung, 1);
        f.zaehlen();
        f.zaehlen();
        assert_eq!(f.jetzt().map(|x| (x.fertig, x.gesamt)), Some((1, 1)));
    }

    /// Muss des Nachtrags Tempo B: Die Prüfung nimmt die am längsten nicht
    /// grün geprüften zuerst (ohne Stand vor allen, dann `gruen_um`, bei
    /// gleicher Zeit nach dem Namen), damit eine knappe Frist nicht immer
    /// dieselben trifft. Gegenprobe: ohne Zeiten im Stand alphabetisch.
    #[test]
    fn am_laengsten_ungeprueft_zuerst() {
        let r = TestRepo::neu("reihenfolge", "gruen");
        if !r.werkzeuge_attrappe() {
            return;
        }
        let gesammelt = std::sync::Mutex::new(Vec::new());
        let sammel = |_: &Path, p: &[(String, PathBuf)], _: Instant| -> Vec<Vec<crate::inhalt::pruefung::Befund>> {
            gesammelt.lock().unwrap().push(p.iter().map(|(s, _)| s.clone()).collect::<Vec<_>>());
            vec![Vec::new(); p.len()]
        };
        let zeiten = [("a", Some(30)), ("b", None), ("c", Some(10)), ("d", Some(20)), ("e", Some(10))];
        let stand_mit = |mit_zeit: bool| {
            let mut s = Stand::default();
            for (slug, um) in zeiten {
                r.projekt(slug, &format!("<p>{slug} {mit_zeit}</p>\n"));
                // Abdruck 0 passt nie: jedes Projekt wird geprüft.
                if let (true, Some(um)) = (mit_zeit, um) {
                    s.projekte.insert(slug.into(), ProjektStand { gruen: true, gruen_um: Some(um), ..Default::default() });
                }
            }
            s.speichern(&r.ort().gruen()).unwrap();
        };
        stand_mit(true);
        pruefen_mit(&r.ort(), &ohne_befund, Some(&sammel), Instant::now() + PRUEFUNG_HOECHSTENS);
        stand_mit(false);
        pruefen_mit(&r.ort(), &ohne_befund, Some(&sammel), Instant::now() + PRUEFUNG_HOECHSTENS);
        assert_eq!(*gesammelt.lock().unwrap(), [["b", "c", "e", "d", "a"], ["a", "b", "c", "d", "e"]]);
    }

    /// Sollte der Gegenprüfung Tempo B: Was ein Bau zu Ende prüft, auch nicht
    /// geprüft aus eigenem Grund (hier Vitest lief nicht durch), rückt nach
    /// hinten (`versucht_um`); was die Frist verdrängte, behält seine Zeit und
    /// kommt im nächsten Bau zuerst. So nehmen ZUGLEICH dauerhaft nicht
    /// geprüfte Projekte den übrigen nicht in jedem Bau die einzige Runde.
    /// Gegenprobe (Mutation): nur nach `gruen_um` sortiert, kämen x1 bis x3 wieder zuerst.
    #[test]
    fn verdraengte_kommen_im_naechsten_bau_zuerst() {
        let r = TestRepo::neu("reihenfolge-bauten", "gruen");
        if !r.werkzeuge_attrappe() {
            return;
        }
        let gesammelt = std::sync::Mutex::new(Vec::new());
        // Die Frist reicht für ZUGLEICH Projekte: Die vorderen haben ein eigenes Werkzeugproblem (ESLint reißt sein
        // Zeitlimit, danach verpasst Vitest die Frist; Auflage der Abnahme: zählt als Versuch), die übrigen verpassen die Frist.
        let sammel = |_: &Path, p: &[(String, PathBuf)], _: Instant| -> Vec<Vec<crate::inhalt::pruefung::Befund>> {
            gesammelt.lock().unwrap().push(p.iter().map(|(s, _)| s.clone()).collect::<Vec<_>>());
            let ort = |s: &str| format!("projects/{s}");
            p.iter()
                .enumerate()
                .map(|(i, (s, _))| match i < ZUGLEICH {
                    true => [
                        projektpruefung::uebersprungen(&ort(s), "ESLint", "ESLint hat nach 120 s nicht geantwortet"),
                        projektpruefung::uebersprungen(&ort(s), "Vitest", projektpruefung::SATZ_FRIST),
                    ]
                    .concat(),
                    false => projektpruefung::uebersprungen(&ort(s), "Vitest", projektpruefung::SATZ_FRIST),
                })
                .collect()
        };
        let mut s = Stand::default();
        for slug in ["a", "b", "x1", "x2", "x3"] {
            // Abdruck 0 passt nie: jedes Projekt wird geprüft.
            r.projekt(slug, &format!("<p>{slug}</p>\n"));
            let alt = if slug.starts_with('x') {
                ProjektStand { rot_art: Some(RotArt::NichtGeprueft), ..Default::default() }
            } else {
                ProjektStand { gruen: true, gruen_um: Some(5), ..Default::default() }
            };
            s.projekte.insert(slug.into(), alt);
        }
        s.speichern(&r.ort().gruen()).unwrap();
        for bau in 0..2 {
            let z = pruefen_mit(&r.ort(), &ohne_befund, Some(&sammel), Instant::now() + PRUEFUNG_HOECHSTENS);
            assert_eq!(z.v.nicht_geprueft, ["a", "b", "x1", "x2", "x3"]);
            let p = &z.stand.projekte;
            // Bau 0: x3 geprüft, a verdrängt (ohne Zeit); Bau 1: jetzt b geprüft.
            let geprueft = if bau == 0 { p["x3"].versucht_um.is_some() && p["a"].versucht_um.is_none() } else { p["b"].versucht_um.is_some() };
            assert!(geprueft, "Bau {bau}: {p:?}");
            z.stand.speichern(&r.ort().gruen()).unwrap();
        }
        assert_eq!(*gesammelt.lock().unwrap(), [["x1", "x2", "x3", "a", "b"], ["a", "b", "x1", "x2", "x3"]]);
    }

    /// Zweig B, Muss der Abnahme: Mit echten Werkzeugen ergibt die
    /// Sammelprüfung dieselbe rote Menge (und dieselben Befunde) wie die
    /// Prüfung je Projekt; kein Werkzeug ist übersprungen. Darunter ein
    /// Projekt mit verbotenem `declare global`, das im gemeinsamen Lauf den
    /// Typfehler eines anderen verdecken würde.
    #[test]
    fn sammel_und_je_projekt_dieselbe_rote_menge() {
        let r = TestRepo::neu("sammel-gleich", "gruen");
        if !r.mit_svelte_check() {
            crate::ueberspringen("Node, web/node_modules oder die Junction fehlt.");
            return;
        }
        let dir = r.0.join("projects");
        let seiten = [
            ("gut-a", probe_seite("Gut A")),
            ("gut-b", probe_seite("Gut B")),
            ("typfehler", "<script lang=\"ts\">\n\tconst n: number = 'kein Wert';\n</script>\n\n<p>{n}</p>\n".to_string()),
            ("lint", "<script lang=\"ts\">\n\tconst t: string = 'Lint';\n\tdebugger;\n</script>\n\n<p>{t}</p>\n".to_string()),
            ("global", probe_seite("Global")),
            ("maskiert", "<script lang=\"ts\">\n\tconst m: number = window.maskeProbe;\n</script>\n\n<p>{m}</p>\n".to_string()),
        ];
        for (s, seite) in &seiten {
            r.projekt(s, seite);
            // Mit Kopf, sonst läuft der Rust-Teil (Scanner) nicht.
            std::fs::write(dir.join(s).join("projekt.toml"), format!("schema = 3\ntitle = \"{s}\"\nstatus = \"aktiv\"\nsummary = \"Probe.\"\n")).unwrap();
        }
        std::fs::write(dir.join("global/typen.ts"), "export {};\n\ndeclare global {\n\tinterface Window {\n\t\tmaskeProbe: number;\n\t}\n}\n").unwrap();
        let web = r.0.join("web");
        let projekte: Vec<(String, PathBuf)> = seiten.iter().map(|(s, _)| (s.to_string(), dir.clone())).collect();
        let rot = |b: &[Vec<crate::inhalt::pruefung::Befund>]| -> BTreeSet<String> {
            projekte.iter().zip(b).filter(|(_, b)| b.iter().any(|x| x.ist_fehler())).map(|((s, _), _)| s.clone()).collect()
        };
        let saetze = |b: &[Vec<crate::inhalt::pruefung::Befund>]| -> Vec<BTreeSet<String>> {
            b.iter().map(|b| b.iter().map(|x| format!("{} {x}", x.ist_fehler())).collect()).collect()
        };
        let t = Instant::now();
        let gesammelt = projektpruefung::pruefe_sammel(&web, &projekte, Instant::now() + PRUEFUNG_HOECHSTENS, true);
        let t_sammel = t.elapsed();
        let t = Instant::now();
        let einzeln: Vec<_> = projekte.iter().map(|(s, e)| projektpruefung::pruefe_voll(&web, e, s, true, Instant::now() + PRUEFUNG_HOECHSTENS)).collect();
        eprintln!("Sammelprüfung {:?}, je Projekt nacheinander {:?}", t_sammel, t.elapsed());
        for b in gesammelt.iter().chain(&einzeln) {
            assert!(!b.iter().any(projektpruefung::ist_uebersprungen), "kein Werkzeug übersprungen: {b:#?}");
        }
        let erwartet: BTreeSet<String> = ["global", "lint", "maskiert", "typfehler"].iter().map(|s| s.to_string()).collect();
        assert_eq!(rot(&einzeln), erwartet, "{einzeln:#?}");
        assert_eq!(rot(&gesammelt), erwartet, "dieselbe rote Menge: {gesammelt:#?}");
        assert_eq!(saetze(&gesammelt), saetze(&einzeln), "dieselben Befunde");

        // Durch den Bau-Schritt: Sammelprüfung und (ohne Stand) je Projekt, gleiche rote Menge im Stand.
        let rot_im_stand = |z: &Zwischen| -> BTreeSet<String> {
            z.stand.projekte.iter().filter(|(_, p)| p.rot_art == Some(RotArt::Pruefung)).map(|(s, _)| s.clone()).collect()
        };
        let z = pruefen_mit(
            &r.ort(),
            &|w, e, s| projektpruefung::pruefe_voll(w, e, s, true, Instant::now() + PRUEFUNG_HOECHSTENS),
            Some(&sammel),
            Instant::now() + PRUEFUNG_HOECHSTENS,
        );
        assert!(z.v.nicht_geprueft.is_empty(), "{:?}", z.v.nicht_geprueft);
        assert_eq!(rot_im_stand(&z), erwartet);
        let z = pruefen_je_projekt(&r.ort(), &|w, e, s| projektpruefung::pruefe_voll(w, e, s, true, Instant::now() + PRUEFUNG_HOECHSTENS));
        assert!(z.v.nicht_geprueft.is_empty(), "{:?}", z.v.nicht_geprueft);
        assert_eq!(rot_im_stand(&z), erwartet);
    }

    /// Zweig A, Punkt 1: Projekt vorher grün, ein Werkzeug übersprungen: In
    /// der Bauliste steht seine Kopie, nicht der Ordner; ohne Kopie fehlt es,
    /// aber nicht rot (keine Meldung, nicht in `rote` oder `alte_fassung`).
    /// Der nächste Bau prüft beide neu.
    #[test]
    fn uebersprungen_baut_die_kopie_und_ist_nicht_rot() {
        let r = TestRepo::neu("nicht-geprueft", "gruen");
        if !r.werkzeuge_attrappe() {
            return;
        }
        let dir = r.0.join("projects");
        let gruen = r.ort().gruen();
        r.projekt("mit", "<p>alt und gut</p>\n");
        kopie_erneuern(&dir, "mit", &gruen).unwrap();
        r.projekt("mit", "<p>neu und ungeprüft, länger</p>\n");
        r.projekt("ohne", "<p>neu</p>\n");
        let um = Some(1_793_450_700);
        let mut stand = Stand::default();
        stand.projekte.insert("mit".into(), ProjektStand { abdruck: 1, gruen: true, gruen_um: um, gebaut_um: um, ..Default::default() });
        stand.speichern(&gruen).unwrap();

        let eslint_weg =
            |_: &Path, _: &Path, s: &str| projektpruefung::uebersprungen(&format!("projects/{s}"), "ESLint", "ESLint hat länger als 120 s gedauert");
        let z = pruefen_je_projekt(&r.ort(), &eslint_weg);
        assert_eq!(z.eintraege, vec![("mit".to_string(), gruen.join("mit"))], "nur die Kopie, nie der Ordner");
        assert!(z.frisch_gruen.is_empty());
        assert_eq!(z.v.nicht_geprueft, ["mit", "ohne"]);
        assert!(z.v.rote.is_empty() && z.v.alte_fassung.is_empty() && z.v.neu_rot.is_empty(), "nicht rot");
        for s in ["mit", "ohne"] {
            let p = &z.stand.projekte[s];
            assert_eq!((p.gruen, p.rot_art), (false, Some(RotArt::NichtGeprueft)), "{s}");
            assert!(p.satz.contains("nicht geprüft") && p.satz.contains("ESLint ist übersprungen"), "{}", p.satz);
        }
        assert_eq!(z.stand.projekte["mit"].gruen_um, um, "die Zeit der letzten guten Fassung bleibt");
        let g = gesamt_gruen(z.eintraege.clone());
        let v = abschliessen(&r.ort(), z, g);
        assert!(v.gescheitert.is_none() && v.rote.is_empty() && v.alte_fassung.is_empty() && v.neu_rot.is_empty(), "{:?}", v.gescheitert);
        assert_eq!(v.nicht_geprueft, ["mit", "ohne"]);
        let liste = r.liste();
        let (kopie, quelle) = (scanner::import_pfad(&gruen), scanner::import_pfad(&dir));
        assert!(liste.contains(&format!("import('{kopie}/mit/Seite.svelte')")), "{liste}");
        assert!(!liste.contains(&format!("{quelle}/mit")) && !liste.contains("ohne"), "{liste}");
        assert_eq!(std::fs::read_to_string(gruen.join("mit/Seite.svelte")).unwrap(), "<p>alt und gut</p>\n", "die Kopie bleibt die alte");
        assert!(satz_nicht_geprueft_bau(&v.nicht_geprueft).unwrap().ends_with(": mit, ohne."));
        assert_eq!(satz_nicht_geprueft_bau(&[]), None);

        // Nächster Bau, die Werkzeuge laufen: beide werden neu geprüft und grün.
        let z = pruefen_je_projekt(&r.ort(), &ohne_befund);
        assert_eq!(z.frisch_gruen.iter().map(|(s, _, _)| s.as_str()).collect::<Vec<_>>(), ["mit", "ohne"]);
        assert_eq!(z.eintraege, vec![("mit".to_string(), dir.join("mit")), ("ohne".to_string(), dir.join("ohne"))]);
        assert!(z.v.nicht_geprueft.is_empty());
        // Nachbesserung Zweig A: Der nicht geprüfte Eintrag behält den Abdruck
        // der letzten grünen Fassung (hier 1). „mit“ ist geändert, weil sein
        // Inhalt davon abweicht, „ohne“, weil es nie grün war.
        assert_eq!((z.stand.projekte["mit"].gruen_abdruck, z.stand.projekte["ohne"].gruen_abdruck), (Some(1), None));
        assert_eq!(geaendert_seit_gruen(&z), BTreeSet::from(["mit".to_string(), "ohne".to_string()]));
    }

    /// Nachbesserung Zweig A (SOLLTE 3): Erst ändert sich nur die Umgebung,
    /// dann ist ein Werkzeug übersprungen (nicht geprüft), dann wird das
    /// Projekt grün: Sein Inhalt ist seit der letzten grünen Fassung gleich,
    /// also ist es in der Gesamtprüfung kein Schuldiger.
    #[test]
    fn umgebung_dann_uebersprungen_dann_gruen_ohne_schuldigen() {
        let r = TestRepo::neu("umgebung-uebersprungen", "gruen");
        if !r.werkzeuge_attrappe() {
            return;
        }
        let (dir, gruen, web) = (r.0.join("projects"), r.ort().gruen(), r.0.join("web"));
        r.projekt("p", "<p>gut</p>\n");
        kopie_erneuern(&dir, "p", &gruen).unwrap();
        let a = abdruck(&dir, "p");
        let mut stand = Stand::default();
        stand.projekte.insert(
            "p".into(),
            ProjektStand { abdruck: a, umgebung: umgebung_fuer(&dir, "p", umgebung(&web)), gruen: true, gruen_abdruck: Some(a), ..Default::default() },
        );
        stand.speichern(&gruen).unwrap();
        // 1. Nur die Umgebung ändert sich, und ESLint läuft nicht.
        std::fs::write(web.join("eslint.projekte.config.js"), "export default [ ];\n").unwrap();
        let eslint_weg =
            |_: &Path, _: &Path, s: &str| projektpruefung::uebersprungen(&format!("projects/{s}"), "ESLint", "ESLint hat länger als 120 s gedauert");
        let z = pruefen_je_projekt(&r.ort(), &eslint_weg);
        assert_eq!(z.stand.projekte["p"].rot_art, Some(RotArt::NichtGeprueft));
        let g = gesamt_gruen(z.eintraege.clone());
        abschliessen(&r.ort(), z, g);
        let s = Stand::laden(&gruen);
        assert_eq!((s.projekte["p"].gruen, s.projekte["p"].gruen_abdruck), (false, Some(a)), "der Abdruck der grünen Fassung bleibt");
        // 2. Nächster Bau, die Werkzeuge laufen: grün, aber nicht geändert.
        let z = pruefen_je_projekt(&r.ort(), &ohne_befund);
        assert_eq!(z.frisch_gruen, vec![("p".to_string(), dir.clone(), a)]);
        assert!(geaendert_seit_gruen(&z).is_empty(), "Inhalt seit grün gleich: in der Gesamtprüfung nicht geändert");
        let letzte = letzte_gruene(&z.projekte, &z.stand);
        assert!(!seit_gruen_geaendert(&letzte, &gruen, "p", &dir.join("p")), "kein Schuldiger");
    }

    /// Nachbesserung Zweig A (MUSS 1): Stürzt Stylelint bei einer Datei ab
    /// (Regel `absturz`, gemeinsamer Lauf fertig), ist das Projekt nicht
    /// grün; in der Bauliste steht seine grüne Kopie, nicht der Ordner.
    #[test]
    fn stil_absturz_bei_einer_datei_macht_nicht_gruen() {
        let r = TestRepo::neu("stil-absturz", "gruen");
        if !r.werkzeuge_attrappe() {
            return;
        }
        let (dir, gruen) = (r.0.join("projects"), r.ort().gruen());
        r.projekt("p", "<p>alt und gut</p>\n");
        kopie_erneuern(&dir, "p", &gruen).unwrap();
        let mut stand = Stand::default();
        stand.projekte.insert("p".into(), ProjektStand { abdruck: 1, gruen: true, ..Default::default() });
        stand.speichern(&gruen).unwrap();
        r.projekt("p", "<p>neu, Stil nicht geprüft</p>\n");
        let stil_absturz = |_: &Path, e: &Path, s: &str| {
            crate::werkzeug::stil::pruefen_mit(e, &[s.to_string()], &format!("projects/{s}"), &|_| {
                Ok((
                    r#"{"nr":0,"zeile":null,"regel":"absturz","text":"Cannot read properties of undefined"}"#.to_string() + "\nFERTIG 1 1\n",
                    String::new(),
                    Some(0),
                ))
            })
        };
        let z = pruefen_je_projekt(&r.ort(), &stil_absturz);
        assert!(z.frisch_gruen.is_empty(), "nicht grün");
        assert_eq!(z.eintraege, vec![("p".to_string(), gruen.join("p"))], "die Kopie, nie der Ordner");
        assert_eq!(z.stand.projekte["p"].rot_art, Some(RotArt::NichtGeprueft));
        assert!(z.v.rote.is_empty() && z.v.neu_rot.is_empty(), "nicht rot");
    }

    /// Lücke der Gegenprüfung (Einspielen): `pruefe --bauliste --svelte`
    /// fällt dasselbe Urteil wie der Bote. Stürzt Stylelint ab oder läuft
    /// ESLint nicht, ist das Projekt nicht grün: Mit Kopie steht die Kopie in
    /// der Bauliste, ohne Kopie fehlt es; nie der Ordner. Laufen die
    /// Werkzeuge wieder, kommen beide grün aus dem Ordner.
    #[test]
    fn einspielen_stil_absturz_und_eslint_weg_nie_gruen() {
        let r = TestRepo::neu("einspielen", "gruen");
        if !r.werkzeuge_attrappe() {
            return;
        }
        let (dir, gruen) = (r.0.join("projects"), r.ort().gruen());
        r.projekt("mit", "<p>alt und gut</p>\n");
        kopie_erneuern(&dir, "mit", &gruen).unwrap();
        let mut stand = Stand::default();
        stand.projekte.insert("mit".into(), ProjektStand { abdruck: 1, gruen: true, gruen_abdruck: Some(1), ..Default::default() });
        stand.speichern(&gruen).unwrap();
        r.projekt("mit", "<p>neu, Stil nicht geprüft</p>\n");
        r.projekt("ohne", "<p>neu, ESLint lief nicht</p>\n");
        let werkzeug_weg = |_: &Path, e: &Path, s: &str| {
            if s == "mit" {
                crate::werkzeug::stil::pruefen_mit(e, &[s.to_string()], &format!("projects/{s}"), &|_| {
                    Ok((
                        r#"{"nr":0,"zeile":null,"regel":"absturz","text":"Cannot read properties of undefined"}"#.to_string() + "\nFERTIG 1 1\n",
                        String::new(),
                        Some(0),
                    ))
                })
            } else {
                projektpruefung::uebersprungen(&format!("projects/{s}"), "ESLint", "ESLint hat länger als 120 s gedauert")
            }
        };
        let gesamt: GesamtLauf = &|_, e, _, _| gesamt_gruen(e);
        let (text, code) = pruefe::einspiel_text(&r.ort(), bauliste_einspielen_mit(&r.ort(), &werkzeug_weg, None, gesamt));
        assert_eq!(code, 0, "{text}");
        assert!(text.contains("NICHT GEPRÜFT mit: ") && text.contains("NICHT GEPRÜFT ohne: ") && text.contains("ESLint ist übersprungen"), "{text}");
        // Nicht geprüft heißt nie GRÜN, auch nicht aus der Kopie; ohne Kopie FEHLT es.
        assert!(!text.contains("ROT ") && text.contains("\nGRÜN keins\nGEBAUT mit\nFEHLT ohne\n"), "{text}");
        assert!(text.ends_with("geschrieben (1 in der Liste, alle aus der grünen Kopie; 0 rot, 2 nicht geprüft).\n"), "{text}");
        let liste = r.liste();
        let (kopie, quelle) = (scanner::import_pfad(&gruen), scanner::import_pfad(&dir));
        assert!(liste.contains(&format!("import('{kopie}/mit/Seite.svelte')")), "{liste}");
        assert!(!liste.contains(&format!("{quelle}/")) && !liste.contains("ohne"), "nie der Ordner: {liste}");
        assert_eq!(std::fs::read_to_string(gruen.join("mit/Seite.svelte")).unwrap(), "<p>alt und gut</p>\n", "die Kopie bleibt die alte");
        let s = Stand::laden(&gruen);
        assert_eq!(
            (s.projekte["mit"].rot_art, s.projekte["ohne"].rot_art),
            (Some(RotArt::NichtGeprueft), Some(RotArt::NichtGeprueft)),
            "derselbe Stand wie im Boten"
        );

        // Die Werkzeuge laufen wieder: beide grün, Kopien erneuert, gebaut aus den Kopien.
        let (text, code) = pruefe::einspiel_text(&r.ort(), bauliste_einspielen_mit(&r.ort(), &ohne_befund, None, gesamt));
        assert_eq!(code, 0, "{text}");
        assert!(text.starts_with("GRÜN mit, ohne\nGEBAUT mit, ohne\n") && !text.contains("FEHLT"), "{text}");
        let liste = r.liste();
        assert!(liste.contains(&format!("import('{kopie}/ohne/Seite.svelte')")) && !liste.contains(&format!("{quelle}/")), "{liste}");
        assert_eq!(std::fs::read_to_string(gruen.join("mit/Seite.svelte")).unwrap(), "<p>neu, Stil nicht geprüft</p>\n");
    }

    /// Etappe 6d (Auflage a): Eine `*.md` im Projekt hat in beiden Baupfaden
    /// dasselbe Urteil. `pruefe_voll` meldet sie als Fehler; im Boten
    /// (`pruefen_je_projekt`, `abschliessen`) wie im Einspiel-Weg (`pruefe
    /// --bauliste --svelte`, `bauliste_einspielen`) ist das Projekt rot: ohne
    /// grüne Kopie nicht im Bau, mit Kopie aus der Kopie, nie aus dem Ordner.
    #[test]
    fn markdown_im_projekt_in_beiden_baupfaden_rot() {
        use crate::inhalt::pruefung::{self, SATZ_MARKDOWN};
        let r = TestRepo::neu("markdown", "gruen");
        let (dir, gruen) = (r.0.join("projects"), r.ort().gruen());
        let kopf = "schema = 3\ntitle = \"Probe\"\nstatus = \"aktiv\"\nsummary = \"Für den Test.\"\n";
        for s in ["mit", "ohne"] {
            r.projekt(s, "<p>gut</p>\n");
            std::fs::write(dir.join(s).join("projekt.toml"), kopf).unwrap();
        }
        // „mit“ war grün (Kopie im Stand), dann neuer Code und eine Notiz; „ohne“ ist neu.
        kopie_erneuern(&dir, "mit", &gruen).unwrap();
        let mut vorher = Stand::default();
        vorher.projekte.insert("mit".into(), ProjektStand { abdruck: 1, gruen: true, gruen_abdruck: Some(1), ..Default::default() });
        r.projekt("mit", "<p>neu</p>\n");
        std::fs::write(dir.join("mit").join("notiz.md"), "Notiz\n").unwrap();
        std::fs::write(dir.join("ohne").join("PLAN.MD"), "# Plan\n").unwrap();
        // Die echte Prüfung je Projekt (ohne web/node_modules: die Werkzeuge übersprungen).
        let voll = projektpruefung::pruefe_voll(&r.0.join("web"), &dir, "ohne", true, Instant::now() + PRUEFUNG_HOECHSTENS);
        assert!(voll.iter().any(|b| b.ist_fehler() && b.datei == "projects/ohne/PLAN.MD" && b.text == SATZ_MARKDOWN), "{voll:#?}");
        if !r.werkzeuge_attrappe() {
            return;
        }
        // Ihr erster Schritt, ohne die Werkzeuge (die Attrappen liefern kein Urteil).
        let je_projekt = |_: &Path, e: &Path, s: &str| pruefung::pruefe_projekt(e, s);
        let urteil = || {
            let s = Stand::laden(&gruen);
            (r.liste(), ["mit", "ohne"].map(|p| (s.projekte[p].gruen, s.projekte[p].rot_art)))
        };
        // Bote.
        vorher.speichern(&gruen).unwrap();
        let z = pruefen_je_projekt(&r.ort(), &je_projekt);
        let g = gesamt_gruen(z.eintraege.clone());
        let bote = abschliessen(&r.ort(), z, g);
        assert!(bote.gescheitert.is_none(), "{:?}", bote.gescheitert);
        assert_eq!(bote.eintraege, vec![("mit".to_string(), gruen.join("mit"))], "nur die Kopie");
        let im_boten = urteil();
        assert_eq!(im_boten.1, [(false, Some(RotArt::Pruefung)); 2]);
        // Einspiel-Weg, vom selben Stand aus.
        std::fs::remove_file(r.0.join(pruefe::BAULISTE_DATEI)).unwrap();
        vorher.speichern(&gruen).unwrap();
        let gesamt: GesamtLauf = &|_, e, _, _| gesamt_gruen(e);
        let (text, code) = pruefe::einspiel_text(&r.ort(), bauliste_einspielen_mit(&r.ort(), &je_projekt, None, gesamt));
        assert_eq!(code, 0, "{text}");
        assert!(text.contains("FEHLT ohne") && !text.contains("GRÜN mit"), "{text}");
        assert_eq!(urteil(), im_boten, "dieselbe Bauliste, derselbe Stand");
        assert!(!r.liste().contains(&scanner::import_pfad(&dir)), "nie der Ordner: {}", r.liste());
    }

    /// Nachbesserung Einspielen: Wird während der Prüfung gespeichert (hier
    /// in der Prüfung je Projekt), ist der Ordner ungeprüft. In der Bauliste
    /// steht die letzte grüne Kopie, ohne Kopie fehlt das Projekt; beide sind
    /// NICHT GEPRÜFT (im Stand auch), nie GRÜN aus dem Ordner. Ein
    /// unverändertes grünes Projekt kommt aus seiner frischen Kopie.
    #[test]
    fn einspielen_nach_der_pruefung_gespeichert_nie_aus_dem_ordner() {
        let r = TestRepo::neu("einspielen-gespeichert", "gruen");
        if !r.werkzeuge_attrappe() {
            return;
        }
        let (dir, gruen) = (r.0.join("projects"), r.ort().gruen());
        r.projekt("mit", "<p>alt und gut</p>\n");
        kopie_erneuern(&dir, "mit", &gruen).unwrap();
        r.projekt("mit", "<p>neu</p>\n");
        r.projekt("neu", "<p>neu</p>\n");
        r.projekt("ruhig", "<p>ruhig</p>\n");
        let speichern = |_: &Path, e: &Path, s: &str| {
            if s != "ruhig" {
                std::thread::sleep(std::time::Duration::from_millis(20));
                std::fs::write(e.join(s).join("Seite.svelte"), "<p>ungeprüft gespeichert</p>\n").unwrap();
            }
            Vec::new()
        };
        let gesamt: GesamtLauf = &|_, e, _, _| gesamt_gruen(e);
        let (text, code) = pruefe::einspiel_text(&r.ort(), bauliste_einspielen_mit(&r.ort(), &speichern, None, gesamt));
        assert_eq!(code, 0, "{text}");
        assert!(text.contains("NICHT GEPRÜFT mit: Die Seite von „mit“ weicht von ihrer grünen Kopie ab (nach der Prüfung geändert"), "{text}");
        assert!(text.contains("NICHT GEPRÜFT neu: Von der Seite von „neu“ gibt es keine grüne Kopie"), "{text}");
        assert!(text.contains("NICHT GEPRÜFT neu: ") && text.contains("\nGRÜN ruhig\nGEBAUT mit, ruhig\nFEHLT neu\n"), "{text}");
        let (liste, kopie, quelle) = (r.liste(), scanner::import_pfad(&gruen), scanner::import_pfad(&dir));
        assert!(liste.contains(&format!("import('{kopie}/mit/Seite.svelte')")) && liste.contains(&format!("import('{kopie}/ruhig/Seite.svelte')")), "{liste}");
        assert!(!liste.contains(&format!("{quelle}/")) && !liste.contains("neu/Seite"), "nie der Ordner: {liste}");
        assert_eq!(std::fs::read_to_string(gruen.join("mit/Seite.svelte")).unwrap(), "<p>alt und gut</p>\n", "die Kopie bleibt die alte");
        let s = Stand::laden(&gruen);
        assert_eq!((s.projekte["mit"].rot_art, s.projekte["neu"].rot_art), (Some(RotArt::NichtGeprueft), Some(RotArt::NichtGeprueft)));
        assert!(s.projekte["ruhig"].gruen, "{:?}", s.projekte["ruhig"]);
    }

    /// Einspielen mit zwei Inhaltsquellen (`BauOrt.quellen`, wie
    /// `pruefe --bauliste --svelte` sie aus DASHBOARD_PROJECTS_DIR nimmt): Ein
    /// grünes Projekt aus der zweiten Quelle steht in der Bauliste.
    #[test]
    fn einspielen_mit_zwei_quellen() {
        let r = TestRepo::neu("einspielen-quellen", "gruen");
        if !r.werkzeuge_attrappe() {
            return;
        }
        let zweite = r.0.join("zweite");
        std::fs::create_dir_all(zweite.join("weiter")).unwrap();
        std::fs::write(zweite.join("weiter/Seite.svelte"), "<p>aus der zweiten Quelle</p>\n").unwrap();
        r.projekt("erstes", "<p>erstes</p>\n");
        let ort = BauOrt { quellen: vec![r.0.join("projects"), zweite], ..r.ort() };
        let gesamt: GesamtLauf = &|_, e, _, _| gesamt_gruen(e);
        let (text, code) = pruefe::einspiel_text(&ort, bauliste_einspielen_mit(&ort, &ohne_befund, None, gesamt));
        assert_eq!(code, 0, "{text}");
        assert!(text.starts_with("GRÜN erstes, weiter\nGEBAUT erstes, weiter\n"), "{text}");
        let kopie = scanner::import_pfad(&ort.gruen());
        assert!(r.liste().contains(&format!("import('{kopie}/weiter/Seite.svelte')")), "{}", r.liste());
        assert_eq!(std::fs::read_to_string(ort.gruen().join("weiter/Seite.svelte")).unwrap(), "<p>aus der zweiten Quelle</p>\n");
    }

    /// Einspielen bei roter Gesamtprüfung: wie im Boten gilt die letzte gute
    /// Fassung. In der Bauliste steht jedes Projekt mit grüner Kopie (aus der
    /// Kopie, nie aus dem Ordner), ohne Kopie fehlt es; keins ist GRÜN, der
    /// Stand bleibt, Rückgabe 0. Läuft die Gesamtprüfung gar nicht, bleibt es
    /// bei der leeren Liste und Rückgabe 1.
    #[test]
    fn einspielen_gesamtpruefung_rot_baut_die_gruenen_kopien() {
        let r = TestRepo::neu("einspielen-gesamt-rot", "gruen");
        if !r.werkzeuge_attrappe() {
            return;
        }
        let (dir, gruen) = (r.0.join("projects"), r.ort().gruen());
        r.projekt("alt", "<p>alt und gut</p>\n");
        kopie_erneuern(&dir, "alt", &gruen).unwrap();
        let mut stand = Stand::default();
        stand.projekte.insert("alt".into(), ProjektStand { abdruck: 1, gruen: true, gruen_abdruck: Some(1), ..Default::default() });
        stand.speichern(&gruen).unwrap();
        r.projekt("alt", "<p>neu, bricht den Rest</p>\n");
        r.projekt("neu", "<p>neu, ohne Kopie</p>\n");
        let rot: GesamtLauf = &|_, e, _, _| Gesamt {
            fehler: Some(format!("{GESAMT_ROT}, auch nach 0 ausgeschlossenen oder zurückgesetzten Projekten. Erster Fund: x")),
            ..gesamt_gruen(e)
        };
        let (text, code) = pruefe::einspiel_text(&r.ort(), bauliste_einspielen_mit(&r.ort(), &ohne_befund, None, rot));
        assert_eq!(code, 0, "{text}");
        assert!(text.starts_with("GESAMTPRÜFUNG ROT Die Gesamtprüfung bleibt rot, auch nach 0 "), "{text}");
        assert!(text.contains("gebaut werden die letzten grünen Kopien") && text.contains("\nGRÜN keins\nGEBAUT alt\nFEHLT neu\n"), "{text}");
        assert!(text.ends_with("(1 in der Liste, alle aus der grünen Kopie; Gesamtprüfung rot, 1 ohne Kopie).\n"), "{text}");
        let (liste, kopie, quelle) = (r.liste(), scanner::import_pfad(&gruen), scanner::import_pfad(&dir));
        assert!(liste.contains(&format!("import('{kopie}/alt/Seite.svelte')")), "{liste}");
        assert!(!liste.contains(&format!("{quelle}/")) && !liste.contains("neu/Seite"), "nie der Ordner: {liste}");
        assert_eq!(std::fs::read_to_string(gruen.join("alt/Seite.svelte")).unwrap(), "<p>alt und gut</p>\n", "die Kopie bleibt die alte");
        let s = Stand::laden(&gruen);
        assert!(s.projekte["alt"].gruen && s.projekte["alt"].abdruck == 1 && !s.projekte.contains_key("neu"), "der Stand bleibt: {:?}", s.projekte);

        // Die Gesamtprüfung läuft nicht: gescheitert, leere Liste.
        let lief_nicht: GesamtLauf = &|_, e, _, _| Gesamt { fehler: Some("svelte-check lief nicht".into()), ..gesamt_gruen(e) };
        let (text, code) = pruefe::einspiel_text(&r.ort(), bauliste_einspielen_mit(&r.ort(), &ohne_befund, None, lief_nicht));
        assert_eq!(code, 1, "{text}");
        assert!(text.starts_with("GESCHEITERT ") && text.contains("svelte-check lief nicht"), "{text}");
        assert!(r.liste().contains("export const projektSeiten: Record<string, Laden> = {};"), "{}", r.liste());
    }

    /// Einspiel-Weg über den Befehl: Fehlt ESLint (das Paket), ist der Bau
    /// gescheitert: die leere Bauliste, Rückgabe 1, kein Projekt grün.
    #[test]
    fn einspielen_ohne_eslint_ist_leer() {
        let r = TestRepo::neu("einspielen-ohne-eslint", "gruen");
        if !r.werkzeuge_attrappe() {
            return;
        }
        r.projekt("p", "<p>gut</p>\n");
        std::fs::remove_file(r.0.join("web/node_modules/eslint/bin/eslint.js")).unwrap();
        std::fs::create_dir_all(r.0.join("web/src/lib")).unwrap();
        std::fs::write(r.0.join(pruefe::BAULISTE_DATEI), "// alte Liste mit p\n").unwrap();
        let (text, code) = pruefe::lauf(&r.0.join("projects"), &["--bauliste".to_string(), "--svelte".to_string()]);
        assert_eq!(code, 1, "{text}");
        assert!(
            text.starts_with("GESCHEITERT Die Prüfung für die Bauliste ist gescheitert: Die Prüfung lässt sich nicht machen: eslint ist nicht da"),
            "{text}"
        );
        let liste = r.liste();
        assert!(liste.contains("export const projektSeiten: Record<string, Laden> = {};") && !liste.contains("alte Liste"), "{liste}");
        assert!(!r.ort().gruen().join("p").exists(), "keine grüne Kopie");
    }

    /// Zweig A, Punkt 2: Fehlt eines der fünf Pakete, ist der Bau „nicht
    /// gebaut“ (nicht still jedes Projekt „nicht geprüft“).
    #[test]
    fn werkzeuge_fehlen_nennt_jedes_paket() {
        let r = TestRepo::neu("werkzeuge-fehlen", "gruen");
        if !r.werkzeuge_attrappe() {
            return;
        }
        let web = r.0.join("web");
        for (paket, datei) in [
            ("svelte-check", "bin/svelte-check"),
            ("eslint", "bin/eslint.js"),
            ("stylelint", "package.json"),
            ("postcss-html", "package.json"),
            ("vitest", "vitest.mjs"),
        ] {
            let p = web.join("node_modules").join(paket).join(datei);
            std::fs::remove_file(&p).unwrap();
            assert!(werkzeuge_fehlen(&web).is_some_and(|s| s.starts_with(paket)), "{paket}");
            std::fs::write(&p, "").unwrap();
        }
        // Nachbesserung Zweig A: auch die Konfigurationen und das Stil-Skript in web/.
        for datei in ["eslint.projekte.config.js", "vitest.projekte.config.ts", "scripts/stil-projekte.mjs"] {
            let p = web.join(datei);
            std::fs::remove_file(&p).unwrap();
            assert_eq!(werkzeuge_fehlen(&web), Some(format!("web/{datei} fehlt")), "{datei}");
            std::fs::write(&p, "").unwrap();
        }
        assert_eq!(werkzeuge_fehlen(&web), None);
    }

    /// Zweig A, Punkt 2: Ein Lauf von Stylelint mit `Err` (fehlt, Zeitlimit)
    /// macht das Projekt nicht grün; es gilt als nicht geprüft.
    #[test]
    fn stylelint_mit_err_macht_nicht_gruen() {
        let r = TestRepo::neu("stil-err", "gruen");
        if !r.werkzeuge_attrappe() {
            return;
        }
        r.projekt("p", "<p>x</p>\n");
        let stil_weg = |_: &Path, e: &Path, s: &str| {
            crate::werkzeug::stil::pruefen_mit(e, &[s.to_string()], &format!("projects/{s}"), &|_| Err("Die Stil-Prüfung dauerte länger als 60 s".into()))
        };
        let z = pruefen_je_projekt(&r.ort(), &stil_weg);
        assert!(z.frisch_gruen.is_empty() && z.eintraege.is_empty(), "nicht grün, ohne Kopie nicht im Bau");
        assert_eq!(z.stand.projekte["p"].rot_art, Some(RotArt::NichtGeprueft));
        assert!(z.v.rote.is_empty() && z.v.neu_rot.is_empty());

        // Bricht der Prüf-Faden ab, ist das Projekt ebenso nicht geprüft (vorher fiel es still weg).
        let bricht: &(dyn Fn(&Path, &Path, &str) -> Vec<crate::inhalt::pruefung::Befund> + Sync) = &|_, _, _| panic!("Absichtlicher Abbruch im Test");
        let z = pruefen_je_projekt(&r.ort(), bricht);
        assert!(z.frisch_gruen.is_empty() && z.eintraege.is_empty());
        assert_eq!(z.stand.projekte["p"].rot_art, Some(RotArt::NichtGeprueft));
        assert!(z.stand.projekte["p"].satz.contains("Prüf-Faden brach ab"), "{}", z.stand.projekte["p"].satz);
    }

    /// Zweig A, Punkte 3 und 5: Ändern sich nur die Bytes der Umgebung,
    /// prüft der nächste Bau neu, das Projekt gilt in der Gesamtprüfung nicht
    /// als geändert, und seine grüne Kopie wird erneuert. Gleiche Bytes mit
    /// neuer Zeit lösen nichts aus.
    #[test]
    fn nur_umgebung_geaendert_erneuert_die_kopie() {
        let r = TestRepo::neu("umgebung", "gruen");
        if !r.werkzeuge_attrappe() {
            return;
        }
        let (dir, gruen, web) = (r.0.join("projects"), r.ort().gruen(), r.0.join("web"));
        std::fs::write(web.join("eslint.projekte.config.js"), "export default [];\n").unwrap();
        r.projekt("p", "<p>aktuell</p>\n");
        let u = umgebung(&web);
        let a = abdruck(&dir, "p");
        let mut stand = Stand::default();
        stand.projekte.insert("p".into(), ProjektStand { abdruck: a, umgebung: umgebung_fuer(&dir, "p", u), gruen: true, ..Default::default() });
        stand.speichern(&gruen).unwrap();
        std::fs::create_dir_all(gruen.join("p")).unwrap();
        std::fs::write(gruen.join("p/Seite.svelte"), "<p>veraltete Kopie</p>\n").unwrap();

        // Unverändert, auch mit neu geschriebener Konfiguration (gleiche Bytes): nichts zu prüfen.
        std::fs::write(web.join("eslint.projekte.config.js"), "export default [];\n").unwrap();
        let z = pruefen_je_projekt(&r.ort(), &ohne_befund);
        assert!(z.frisch_gruen.is_empty() && z.v.schritte.is_empty(), "{:?}", z.v.schritte);
        assert_eq!(z.eintraege, vec![("p".to_string(), dir.join("p"))]);

        // Ein Byte mehr in der Konfiguration: neu geprüft.
        std::fs::write(web.join("eslint.projekte.config.js"), "export default [ ];\n").unwrap();
        let z = pruefen_je_projekt(&r.ort(), &ohne_befund);
        assert_eq!(z.frisch_gruen, vec![("p".to_string(), dir.clone(), a)], "Inhalts-Abdruck unverändert");
        assert!(z.v.schritte.iter().any(|s| s.name == "Prüfung je Projekt (p)"), "{:?}", z.v.schritte);
        let letzte = letzte_gruene(&z.projekte, &z.stand);
        assert!(!seit_gruen_geaendert(&letzte, &gruen, "p", &dir.join("p")), "nur die Umgebung: kein Schuldiger");
        assert!(geaendert_seit_gruen(&z).is_empty(), "nur die Umgebung: in der Gesamtprüfung nicht geändert");
        let g = gesamt_gruen(z.eintraege.clone());
        abschliessen(&r.ort(), z, g);
        assert_eq!(std::fs::read_to_string(gruen.join("p/Seite.svelte")).unwrap(), "<p>aktuell</p>\n", "Kopie erneuert");
        let s = Stand::laden(&gruen);
        assert_eq!((s.projekte["p"].abdruck, s.projekte["p"].umgebung, s.projekte["p"].gruen), (a, umgebung(&web)[0], true));
        // Danach wieder nichts zu prüfen.
        assert!(pruefen_je_projekt(&r.ort(), &ohne_befund).frisch_gruen.is_empty());
    }

    /// Zweig A, Punkte 3, 4 und 6: der Umgebungs-Abdruck über die Bytes (mit
    /// den tsconfig-Teilen von `$app`, `eslint.config.js`, `vitest-setup.ts`),
    /// die Alias-Ziele nur mit `*.spec.ts` und ohne Specs darin; der
    /// Inhalts-Abdruck mit dem Elternordner. `$app/types` zählt nicht.
    #[test]
    fn umgebung_und_alias_ziele() {
        let r = TestRepo::neu("umgebung-teile", "gruen");
        let web = r.0.join("web");
        let u0 = umgebung(&web);
        let typen = web.join("node_modules/$app/types/index.d.ts");
        std::fs::create_dir_all(typen.parent().unwrap()).unwrap();
        std::fs::write(&typen, "export {};\n").unwrap();
        assert_eq!(umgebung(&web), u0, "$app/types (nur Routen-Deklarationen) zählt nicht");
        for (datei, inhalt) in [
            ("eslint.config.js", "export const grenze = 1;\n"),
            ("vitest-setup.ts", "export {};\n"),
            ("node_modules/$app/tsconfig.json", "{}\n"),
            ("node_modules/$app/tsconfig/service-worker.json", "{}\n"),
        ] {
            let vorher = umgebung(&web);
            std::fs::create_dir_all(web.join(datei).parent().unwrap()).unwrap();
            std::fs::write(web.join(datei), inhalt).unwrap();
            let nachher = umgebung(&web);
            assert!(nachher[0] != vorher[0] && nachher[1] != vorher[1], "{datei}");
        }
        assert_ne!(umgebung(&web), u0);
        // Alias-Ziele: nur der zweite Wert, Specs darin zählen nicht.
        let u1 = umgebung(&web);
        std::fs::create_dir_all(web.join("src/lib/katalog")).unwrap();
        std::fs::write(web.join("src/lib/katalog/Bild.svelte"), "<img alt=\"\" />\n").unwrap();
        let u2 = umgebung(&web);
        assert_eq!(u2[0], u1[0], "ohne Specs zählen die Alias-Ziele nicht");
        assert_ne!(u2[1], u1[1]);
        std::fs::write(web.join("src/lib/katalog/Bild.svelte.spec.ts"), "export {};\n").unwrap();
        assert_eq!(umgebung(&web), u2, "*.spec.ts in den Alias-Zielen zählt nicht");
        // Je Projekt: mit *.spec.ts der zweite Wert.
        let dir = r.0.join("projects");
        r.projekt("ohne", "<p>x</p>\n");
        r.projekt("mit", "<p>x</p>\n");
        std::fs::write(dir.join("mit/rechnen.spec.ts"), "export {};\n").unwrap();
        assert_eq!((umgebung_fuer(&dir, "ohne", u2), umgebung_fuer(&dir, "mit", u2)), (u2[0], u2[1]));
        // Inhalts-Abdruck: ein verschobenes Projekt (anderer Elternordner) ist ein anderes.
        let anders = r.0.join("andere-quelle");
        std::fs::create_dir_all(anders.join("ohne")).unwrap();
        std::fs::copy(dir.join("ohne/Seite.svelte"), anders.join("ohne/Seite.svelte")).unwrap();
        let zeit = std::fs::metadata(dir.join("ohne/Seite.svelte")).unwrap().modified().unwrap();
        std::fs::File::options().write(true).open(anders.join("ohne/Seite.svelte")).unwrap().set_modified(zeit).unwrap();
        assert_ne!(abdruck(&anders, "ohne"), abdruck(&dir, "ohne"), "anderer Elternordner, gleiche Dateien");
    }

    /// Fingerabdruck: nur .svelte und .ts zählen, TOML nie.
    #[test]
    fn abdruck_nur_svelte_und_ts() {
        let r = TestRepo::neu("abdruck", "gruen");
        r.projekt("p", "<p>a</p>\n");
        let dir = r.0.join("projects");
        let a = abdruck(&dir, "p");
        std::fs::write(dir.join("p/projekt.toml"), "schema = 3\n").unwrap();
        std::fs::create_dir_all(dir.join("p/daten")).unwrap();
        std::fs::write(dir.join("p/daten/x.toml"), "a = 1\n").unwrap();
        assert_eq!(abdruck(&dir, "p"), a, "TOML ändert nichts");
        std::fs::write(dir.join("p/hilfe.ts"), "export const x = 1;\n").unwrap();
        let b = abdruck(&dir, "p");
        assert_ne!(b, a, "neue .ts-Datei");
        std::fs::write(dir.join("p/Seite.svelte"), "<p>ab</p>\n").unwrap();
        assert_ne!(abdruck(&dir, "p"), b, "geänderte .svelte-Datei");
    }

    /// Abnahme 4b: Wird zwischen Prüfung und Kopie gespeichert, gilt der
    /// Stand als ungeprüft und darf nicht als grün in die Kopie.
    #[test]
    fn gespeichert_waehrend_des_baus_wird_nicht_gruen() {
        let r = TestRepo::neu("waehrend", "gruen");
        r.projekt("p", "<p>geprüft</p>\n");
        let dir = r.0.join("projects");
        let vorher = abdruck(&dir, "p");
        assert!(noch_derselbe(&dir, "p", vorher));
        // Jemand speichert, während der Bau läuft (andere Größe, damit der
        // Abdruck auch bei grober Uhr sicher abweicht).
        std::fs::write(dir.join("p/Seite.svelte"), "<p>ungeprüft und länger</p>\n").unwrap();
        assert!(!noch_derselbe(&dir, "p", vorher), "der Stand nach dem Speichern ist ungeprüft");
    }
    fn ohne_npm() -> bool {
        let fehlt = programme::npm_befehl_async().is_err();
        if fehlt {
            crate::ueberspringen("node oder npm fehlt an den festen Orten.");
        }
        fehlt
    }

    /// Grün: der Zeiger zeigt auf den neuen Ordner mit index.html, die
    /// Bauliste ist geschrieben; nach drei Bauten bleiben zwei Ordner.
    #[tokio::test]
    async fn gruen_stellt_den_zeiger_um_und_raeumt_auf() {
        if ohne_npm() {
            return;
        }
        let r = TestRepo::neu("gruen", "gruen");
        let e = bauen(&r.ort(), ZEITLIMIT).await;
        assert!(e.gruen, "{e:?}");
        let name = e.ordner.clone().unwrap();
        assert!(ist_bau_name(&name), "{name}");
        assert_eq!(r.zeiger().as_deref(), Some(name.as_str()));
        assert!(r.0.join("web/builds").join(&name).join("index.html").is_file());
        assert!(e.satz.starts_with("Bau grün in "), "{}", e.satz);
        assert!(e.rote.is_empty() && e.ausgabe_ende.is_none());
        let liste = std::fs::read_to_string(r.0.join(pruefe::BAULISTE_DATEI)).unwrap();
        assert!(liste.starts_with(scanner::BAULISTE_KOPF), "{liste}");
        assert!(!r.0.join("web/build").exists(), "web\\build bleibt unberührt");
        for _ in 0..2 {
            assert!(bauen(&r.ort(), ZEITLIMIT).await.gruen);
        }
        let ordner = r.ordner();
        assert_eq!(ordner.len(), BEHALTEN, "{ordner:?}");
        assert_eq!(r.zeiger().as_ref(), ordner.last(), "der neueste gilt");
    }

    /// Rot: npm endet mit Fehler. Der Zeiger bleibt, der halbe Ordner geht,
    /// die Antwort nennt das Ende der Ausgabe.
    #[tokio::test]
    async fn rot_laesst_den_zeiger_und_nennt_die_ausgabe() {
        if ohne_npm() {
            return;
        }
        let r = TestRepo::neu("rot", "gruen");
        let gut = bauen(&r.ort(), ZEITLIMIT).await;
        assert!(gut.gruen, "{gut:?}");
        r.modus("rot");
        let e = bauen(&r.ort(), ZEITLIMIT).await;
        assert!(!e.gruen, "{e:?}");
        assert!(e.satz.contains("npm run build ist gescheitert (Rückgabe 3)"), "{}", e.satz);
        let ende = e.ausgabe_ende.clone().unwrap();
        assert!(ende.contains("projects/x/Seite.svelte, Zeile 3: kaputt") && ende.contains("letzte Zeile der Ausgabe"), "{ende}");
        assert_eq!(r.zeiger(), gut.ordner, "Zeiger unverändert");
        assert_eq!(r.ordner(), vec![gut.ordner.clone().unwrap()], "kein halber Ordner bleibt");
        // Ohne index.html im Ziel ist es auch rot.
        std::fs::write(r.0.join("web/bau.mjs"), "console.log('nichts gebaut');").unwrap();
        let e = bauen(&r.ort(), ZEITLIMIT).await;
        assert!(!e.gruen && e.satz.contains("keine index.html"), "{e:?}");
        assert_eq!(r.zeiger(), gut.ordner);
    }

    /// Zeitlimit: das Skript schläft länger; npm und alles, was es gestartet
    /// hat, ist danach beendet, und der Bau ist rot.
    #[tokio::test]
    async fn zeitlimit_beendet_den_ganzen_baum() {
        if ohne_npm() {
            return;
        }
        let r = TestRepo::neu("zeitlimit", "haengt");
        let start = Instant::now();
        let e = bauen(&r.ort(), Duration::from_secs(4)).await;
        // Gemessen wird der npm-Schritt selbst: `bauen` wartet vorher auf
        // EIN_BAU (im vollen Lauf auf die anderen Bau-Tests) und prüft neben
        // npm; das zählt nicht zum Zeitlimit. Ohne Abbruch dauerte er 600 s.
        let npm = e.schritte.iter().find(|s| s.name == "vite build").map(|s| Duration::from_millis(s.ms));
        let grenze = crate::zeitgrenze(Duration::from_secs(10), Duration::from_secs(20));
        assert!(npm.is_some_and(|d| d < grenze), "npm-Schritt {npm:?}, gesamt {:?}: {e:?}", start.elapsed());
        assert!(!e.gruen, "{e:?}");
        assert!(e.satz.contains("länger als 4 s"), "{}", e.satz);
        assert!(r.zeiger().is_none());
        let pids = std::fs::read_to_string(r.0.join("web/pids.txt")).expect("das Skript lief bis zum Schlafen");
        let pids: Vec<u32> = pids.split_whitespace().map(|p| p.parse().unwrap()).collect();
        assert_eq!(pids.len(), 2);
        // Kurz Zeit lassen, bis Windows die Prozesse abgeräumt hat.
        let lebt = |pid: u32| {
            let mut s = sysinfo::System::new();
            s.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[sysinfo::Pid::from_u32(pid)]), true);
            s.process(sysinfo::Pid::from_u32(pid)).is_some()
        };
        let frist = Instant::now() + Duration::from_secs(5);
        while pids.iter().any(|p| lebt(*p)) && Instant::now() < frist {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        for p in &pids {
            assert!(!lebt(*p), "Prozess {p} läuft noch");
        }
    }

    /// Ohne package.json baut der Bote nicht (und schreibt nichts).
    #[tokio::test]
    async fn ohne_package_json_rot() {
        let r = TestRepo::neu("ohne-paket", "gruen");
        std::fs::remove_file(r.0.join("web/package.json")).unwrap();
        let e = bauen(&r.ort(), ZEITLIMIT).await;
        assert!(!e.gruen && e.satz.contains("fehlt package.json"), "{e:?}");
        assert!(!r.0.join(pruefe::BAULISTE_DATEI).exists());
    }
}
