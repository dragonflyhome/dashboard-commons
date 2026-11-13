//! Live-Kanal: Server-Sent Events unter `/api/events`.
//!
//! Drei Quellen: ein Beobachter, der `projects/` und `data/` alle zwei
//! Sekunden auf geänderte Dateien prüft (Zeitstempel und Größe; bewusst ohne
//! `notify`-Crate, weil deren windows-sys-Version mit der GNU-Toolchain
//! abstürzt, siehe Anleitung, „Stolpersteine beim Bauen“), die PC-Werte alle fünf Sekunden und jeder Schritt
//! eines Vorgangs (Ereignis `vorgang`, Zielbild c2). Dazu der Bau der
//! Oberfläche (Ereignis `bau` mit slug, zustand, meldung; Etappe 4v). Liest die Seite zu
//! langsam und läuft ein Kanal über, kommt statt der verlorenen Ereignisse
//! `nachholen` (Daten `{ "grund": "ueberlauf" }`). Beim Beenden des Servers
//! endet der Strom (Abschaltsignal im `AppState`), damit die Verbindung schließt.
//!
//! Etappe 2d: Der Beobachter gleicht über den ContentStore ab
//! (`inhalt::store`). Der liest nur geänderte Projekte neu und schickt genaue
//! Ereignisse: `projekt` (slug, version), `daten` (name, version) für
//! `data/*.toml`, und bei mehr als drei Projekten zugleich ein einziges
//! `inhalt` (Sammelereignis). Das `change` je Datei bleibt daneben wie bisher.
//!
//! Derselbe Beobachter wirft alle zwei Sekunden einen leichten Blick auf die
//! Repos der Projekte (`repo_stempel`), alle zehn Sekunden einen tieferen
//! (`repo_stempel_tief`), über den Boten, wenn er es kann (er läuft als
//! Inhaber und darf in seine Repos schauen), sonst selbst in `spawn_blocking`.
//! Änderungen gehen an den Git-Zwischenspeicher. Ändert sich dort danach der
//! Git-Stand, bekommt der ContentStore die Slugs (`git_melder`) und schickt
//! `projekt` bzw. `inhalt`.
//!
//! Kopfleiste, Fassung 2 (`kopfleiste`,
//! `[fassung_2_live_zustaende]`): Daneben gibt es den freien Kanal
//! `/api/events/frei`, ohne Anmeldung, hinter allen übrigen Schleusen. Er
//! trägt nur das Lebenszeichen (Kommentar `lebenszeichen` alle 15 s), den
//! Anmeldestand des Anfragenden (`anmeldung`, ein Wort vom Server) und
//! Ereignisse zu Daten, die ohnehin frei sind: `projekt`, `inhalt`, `daten`
//! nur für `pc` und `schwerpunkte`, die PC-Werte (`pc`), `fassung` und `nachholen`. Welche Art
//! wohin darf, steht an einer Stelle ([`Art::frei`], Liste [`FREIER_KANAL`]);
//! der Wächter-Test prüft das.
//!
//! Live-Lücken (`immer-aktuell`, Punkt 2):
//! Der geschützte Kanal trägt dazu `speicher`, `pc-benutzer` und `docs`, je mit
//! Bereich (`crate::aenderungen`, gebündelt). Derselbe Beobachter sieht alle
//! 2 s in `docs/entscheidungen`, `docs/zielbild` und `assets/` der Projekte
//! ([`WeitereWachen`]); eine Änderung in `assets/` gibt `projekt` mit
//! `assets: true`. Der Fingerabdruck der Repos zählt jede Datei unter
//! `refs/heads` (Zweige mit Schrägstrich) und `packed-refs`.

use std::collections::HashMap;
use std::convert::Infallible;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream::Stream;
use serde::Serialize;
use tokio::sync::broadcast;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::{BroadcastStream, IntervalStream};
use tracing::{debug, info, warn};
use ts_rs::TS;

use axum::Extension;

use super::vertrag::{BauZustand, Ereignis, VorgangsArt, Zustand};
use crate::anmeldung::{Inhaber, WindowsKonto};
use crate::app::AppState;
use crate::git::Zwischenspeicher;
use crate::inhalt::store::ContentStore;
use crate::vertrag::passkey::{Anmeldewort, AnmeldungEreignis};

/// Was über den Kanal geht. `area` ist `projects` oder `data` (eine Datei
/// hat sich geändert), `aufgaben` (eine Aufgabe für den Inhaber ist neu,
/// abgehakt, abgelaufen oder zurückgezogen; Pfad `aufgaben/<projekt>`) oder
/// `anstehend` (Etappe A1: die Liste oder das Unter-Dashboard eines Projekts
/// ist neu zu holen; Pfad `anstehend/<projekt>` oder `anstehend`).
#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "ChangeEvent")]
pub struct Change {
    #[ts(type = "\"projects\" | \"data\" | \"aufgaben\" | \"anstehend\"")]
    pub area: &'static str,
    /// Pfad relativ zum Repo, z. B. `projects/schaufenster/projekt.toml`.
    pub path: String,
    #[ts(type = "string")]
    pub at: chrono::DateTime<chrono::Local>,
}

pub type Sender = broadcast::Sender<Change>;

pub fn channel() -> Sender {
    broadcast::channel(64).0
}

/// Ein Schritt eines Vorgangs für den Live-Kanal (Ereignis `vorgang`). Die
/// Seite holt Einzelheiten bei Bedarf über `/api/vorgaenge/<id>`.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "VorgangEvent")]
pub struct VorgangEreignis {
    pub id: String,
    pub slug: String,
    pub art: VorgangsArt,
    pub zustand: Zustand,
    /// Nummer des Ereignisses im Journal.
    pub nr: u64,
}

impl From<&Ereignis> for VorgangEreignis {
    fn from(e: &Ereignis) -> Self {
        Self { id: e.vorgang.clone(), slug: e.slug.clone(), art: e.art, zustand: e.zustand, nr: e.nr }
    }
}

pub type VorgangSender = broadcast::Sender<VorgangEreignis>;

pub fn vorgang_kanal() -> VorgangSender {
    broadcast::channel(256).0
}

/// Ein Schritt des Baus der Oberfläche (Ereignis `bau`, Etappe 4v): je Chat,
/// der einen Bau bestellt hat, einmal bei `laeuft` und einmal beim Ergebnis.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "BauEvent")]
pub struct BauEreignis {
    pub slug: String,
    pub zustand: BauZustand,
    /// Ein Satz (ohne die Zeilen der Ausgabe; die stehen im Vorgang). Seit
    /// der Abnahme 4c ohne Pfade und Befehle.
    pub meldung: String,
    /// Abnahme 4c: Der ganze Bau lief nicht durch (Bote fehlt, Zeitlimit,
    /// „Nicht gebaut“); die Seite selbst ist dann nicht unbedingt rot.
    pub gescheitert: bool,
}

pub type BauSender = broadcast::Sender<BauEreignis>;

pub fn bau_kanal() -> BauSender {
    broadcast::channel(64).0
}

/// Eine neue Fassung der Oberfläche ist da (Ereignis `fassung`, Etappe 4b):
/// Der Baumeister hat grün gebaut und den Zeiger umgestellt. Die Seite kann
/// daraufhin anbieten, neu zu laden.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "FassungEvent")]
pub struct FassungEreignis {
    /// Der Ordner unter `web/builds` (nur der Name).
    pub ordner: String,
    /// Dauer des Baus in Millisekunden.
    pub dauer_ms: u64,
}

pub type FassungSender = broadcast::Sender<FassungEreignis>;

pub fn fassung_kanal() -> FassungSender {
    broadcast::channel(16).0
}

/// Bau-Tempo, Zweig C: Stand des laufenden Baus (Ereignis `bau-fortschritt`,
/// nur Zahlen, nur geschützt; ohne Anmeldung kein Schrittzähler).
pub type BauFortschrittSender = broadcast::Sender<crate::vertrag::bote::BauFortschritt>;

pub fn bau_fortschritt_kanal() -> BauFortschrittSender {
    broadcast::channel(16).0
}

/// Ereignis `projekt` (Etappe 2d): Die Daten eines Projekts sind neu
/// gelesen (oder sein Git-Stand ist neu). Die Seite lädt nur dieses Projekt
/// und die Projektliste neu.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "ProjektEvent")]
pub struct ProjektEreignis {
    pub slug: String,
    /// Version des Inhalts (steigt mit jeder Änderung, über alle Projekte).
    pub version: u64,
    /// Live-Lücken, Punkt 5: `true`, wenn sich eine Datei in `assets/` geändert
    /// hat (Bild, Vorschau). Die Seite hängt dann `?v=<version>` an die
    /// Adressen der Dateien dieses Projekts. Sonst fehlt das Feld.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub assets: Option<bool>,
    /// Kleinigkeit aus der Abnahme live-luecken: bei `assets: true` die
    /// Namen der geänderten Dateien in `assets/` (höchstens
    /// [`ASSET_DATEIEN_HOECHSTENS`]); die Seite erneuert dann nur deren
    /// Adressen. Fehlt das Feld, gilt die Änderung für alle Dateien des
    /// Projekts. Nur auf dem geschützten Kanal; der freie bekommt es nie
    /// ([`InhaltEreignis::nur_frei`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub dateien: Option<Vec<String>>,
}

/// So viele Dateinamen trägt ein `projekt` mit `assets: true` höchstens;
/// sind es mehr, fehlt die Liste (dann gilt die Änderung für alle Dateien).
pub const ASSET_DATEIEN_HOECHSTENS: usize = 50;

/// Ereignis `daten` (Etappe 2d): `data/<name>.toml` hat sich geändert.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "DatenEvent")]
pub struct DatenEreignis {
    /// Name ohne `.toml`, etwa `pc` oder `schwerpunkte`.
    pub name: String,
    pub version: u64,
}

/// Ereignis `inhalt` (Etappe 2d): Viele Projekte auf einmal sind neu (erster
/// Git-Lauf nach dem Start, ein Chat ändert vieles zugleich). Statt eines
/// Ereignisses je Projekt kommt dieses eine; die Seite lädt einmal neu, was
/// sie zeigt.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "InhaltEvent")]
pub struct SammelEreignis {
    pub version: u64,
    pub projekte: Vec<String>,
    /// Dateien aus `data/` (Name ohne `.toml`).
    pub daten: Vec<String>,
}

/// Die Arten der Ereignisse auf den Live-Kanälen, mit ihrem Namen im
/// SSE-Strom. Beide Kanäle schicken nur über diese Arten ([`ereignis`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Art {
    /// Datei geändert, Aufgaben, Anstehend (nur geschützt).
    Change,
    /// Schritt eines Vorgangs (nur geschützt).
    Vorgang,
    /// Bau der Oberfläche je Chat (nur geschützt, gehört zu den Vorgängen).
    Bau,
    /// Neue Fassung der Oberfläche.
    Fassung,
    /// Ein Projekt ist neu gelesen.
    Projekt,
    /// `data/<name>.toml` ist neu (auf dem freien Kanal nur [`FREIE_DATEN`]).
    Daten,
    /// Viele Projekte auf einmal.
    Inhalt,
    /// PC-Werte alle 5 s.
    Pc,
    /// Ereignisse verloren, bitte neu laden.
    Nachholen,
    /// Anmeldestand des Anfragenden (schickt nur der freie Kanal).
    Anmeldung,
    /// Der Datenspeicher hat geschrieben, mit Bereich (nur geschützt; Live-Lücken).
    Speicher,
    /// Benutzer-Daten der PC-Seite neu: Ordnergrößen, Systemprogramme, Tresor,
    /// Bote da/weg (nur geschützt wie `/api/pc/benutzer` und `/api/betrieb`).
    PcBenutzer,
    /// `docs/entscheidungen` oder `docs/zielbild` geändert (nur geschützt wie
    /// `/api/entscheidungen` und `/api/zielbild` seit 4d).
    Docs,
    /// Bau-Tempo, Zweig C: Schritt und Zähler des laufenden Baus (nur geschützt).
    BauFortschritt,
}

impl Art {
    /// Alle Arten (der Test `alle_arten_vollstaendig` hält die Liste vollständig).
    pub const ALLE: [Art; 14] = [
        Art::Change,
        Art::Vorgang,
        Art::Bau,
        Art::Fassung,
        Art::Projekt,
        Art::Daten,
        Art::Inhalt,
        Art::Pc,
        Art::Nachholen,
        Art::Anmeldung,
        Art::Speicher,
        Art::PcBenutzer,
        Art::Docs,
        Art::BauFortschritt,
    ];

    /// Name des SSE-Ereignisses.
    pub fn name(self) -> &'static str {
        match self {
            Art::Change => "change",
            Art::Vorgang => "vorgang",
            Art::Bau => "bau",
            Art::Fassung => "fassung",
            Art::Projekt => "projekt",
            Art::Daten => "daten",
            Art::Inhalt => "inhalt",
            Art::Pc => "pc",
            Art::Nachholen => "nachholen",
            Art::Anmeldung => "anmeldung",
            Art::Speicher => "speicher",
            Art::PcBenutzer => "pc-benutzer",
            Art::Docs => "docs",
            Art::BauFortschritt => "bau-fortschritt",
        }
    }

    /// Die Art des Ereignisses zu einem Bereich (`aenderungen::Bereich`).
    pub fn zu_bereich(b: crate::aenderungen::Bereich) -> Art {
        use crate::aenderungen::Gruppe;
        match b.gruppe() {
            Gruppe::Speicher => Art::Speicher,
            Gruppe::PcBenutzer => Art::PcBenutzer,
            Gruppe::Docs => Art::Docs,
        }
    }

    /// Darf diese Art auf den freien Kanal? Nur was in [`FREIER_KANAL`] steht.
    pub fn frei(self) -> bool {
        FREIER_KANAL.contains(&self)
    }
}

/// Die Erlaubnisliste des freien Kanals `/api/events/frei`: Anmeldestand und
/// Ereignisse zu Daten, die schon frei sind (Projektliste und Kopf, freie
/// PC-Werte, Fassung), dazu `nachholen`; das Lebenszeichen ist ein Kommentar.
/// Keine Vorgänge, Antworten, Aufgaben, Schlüssel, Entscheidungen, Unterlagen
/// oder Verlauf. Eine neue Art kommt nur nach Entscheidung hierher.
pub const FREIER_KANAL: [Art; 7] = [Art::Fassung, Art::Projekt, Art::Daten, Art::Inhalt, Art::Pc, Art::Nachholen, Art::Anmeldung];

/// Dateien aus `data/`, deren Ereignis `daten` auf den freien Kanal darf:
/// die PC-Werte (`/api/pc` ist frei) und die Schwerpunkte (Auflage aus der
/// Abnahme der Kopfleiste 3: Namen und Reihenfolge stehen schon frei in
/// `/api/projects`, je Projekt `schwerpunkt` und `schwerpunkt_reihenfolge`;
/// so lädt die Seitenleiste auch ohne Anmeldung live nach). Das Ereignis
/// nennt nur Name und Version, nie den Inhalt. `schluessel` und alles andere nie.
/// Dazu die Projektgruppen (Entscheidung projektgruppe): Titel und
/// Teile stehen ebenso frei in `/api/projects` (Feld `gruppen`).
pub const FREIE_DATEN: [&str; 3] = ["pc", "schwerpunkte", "gruppen"];

/// Ein SSE-Ereignis der Art `art`; auf dem freien Kanal (`frei`) nur, wenn
/// die Art dort erlaubt ist. Alle Ereignisse beider Kanäle entstehen hier.
/// Auf dem freien Kanal geht jeder Text durch `datenschutz` (etwa die
/// Laufwerke im PC-Takt), wie `/api/pc`.
fn ereignis<T: Serialize>(art: Art, frei: bool, daten: &T) -> Option<Event> {
    if frei && !art.frei() {
        return None;
    }
    if frei {
        let mut wert = serde_json::to_value(daten).ok()?;
        crate::datenschutz::json_verbergen(&mut wert);
        return Event::default().event(art.name()).json_data(&wert).ok();
    }
    Event::default().event(art.name()).json_data(daten).ok()
}

/// Was der ContentStore über den Live-Kanal schickt.
#[derive(Debug, Clone, PartialEq)]
pub enum InhaltEreignis {
    Projekt(ProjektEreignis),
    Daten(DatenEreignis),
    Sammel(SammelEreignis),
}

impl InhaltEreignis {
    /// Die Art (und damit der Name) des SSE-Ereignisses.
    pub fn art(&self) -> Art {
        match self {
            Self::Projekt(_) => Art::Projekt,
            Self::Daten(_) => Art::Daten,
            Self::Sammel(_) => Art::Inhalt,
        }
    }

    /// Name des SSE-Ereignisses.
    pub fn name(&self) -> &'static str {
        self.art().name()
    }

    fn als_sse(&self, frei: bool) -> Option<Event> {
        match self {
            Self::Projekt(p) => ereignis(self.art(), frei, p),
            Self::Daten(d) => ereignis(self.art(), frei, d),
            Self::Sammel(s) => ereignis(self.art(), frei, s),
        }
    }

    /// Was davon auf den freien Kanal darf: `projekt` ohne die Dateinamen
    /// aus `assets/` (nur das Bit `assets`), `daten` nur für [`FREIE_DATEN`];
    /// ein Sammelereignis ohne die übrigen Dateien (bleibt nichts, gar nicht).
    pub fn nur_frei(&self) -> Option<InhaltEreignis> {
        match self {
            Self::Projekt(p) => Some(Self::Projekt(ProjektEreignis { dateien: None, ..p.clone() })),
            Self::Daten(d) => FREIE_DATEN.contains(&d.name.as_str()).then(|| self.clone()),
            Self::Sammel(s) => {
                let daten: Vec<String> = s.daten.iter().filter(|n| FREIE_DATEN.contains(&n.as_str())).cloned().collect();
                (!s.projekte.is_empty() || !daten.is_empty()).then(|| Self::Sammel(SammelEreignis { version: s.version, projekte: s.projekte.clone(), daten }))
            }
        }
    }
}

/// Leichter Fingerabdruck eines Repos für den Git-Zwischenspeicher: Namen,
/// Zeitstempel und Größen der obersten Ebene (ohne `.git`, ohne in Ordner wie
/// `node_modules` oder `target` hinabzusteigen) und von `HEAD`, `index` und
/// `refs/heads` im Git-Ordner. Alles andere unter `.git/` (`index.lock`,
/// `objects` …) zählt nicht: Das schreiben auch andere Programme, und eine
/// Meldung darauf liefe sonst im Kreis.
///
/// Blockiert (std::fs); nur aus `spawn_blocking` oder Tests rufen.
pub fn repo_stempel(wurzel: &Path) -> Vec<RepoEintrag> {
    let mut v = Vec::new();
    let mut eintrag = |name: String, meta: Option<std::fs::Metadata>| {
        let zeit = meta.as_ref().and_then(|m| m.modified().ok());
        let laenge = meta.as_ref().filter(|m| m.is_file()).map(|m| m.len()).unwrap_or(0);
        v.push((name, zeit, laenge));
    };
    if let Ok(rd) = std::fs::read_dir(wurzel) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name != ".git" {
                eintrag(name, e.metadata().ok());
            }
        }
    }
    match git_ordner(wurzel) {
        Ok((git_ordner, gemeinsam)) => {
            eintrag(".git/HEAD".into(), std::fs::metadata(git_ordner.join("HEAD")).ok());
            eintrag(".git/index".into(), std::fs::metadata(git_ordner.join("index")).ok());
            let heads = gemeinsam.join("refs").join("heads");
            eintrag(".git/refs/heads".into(), std::fs::metadata(&heads).ok());
            // Live-Lücken, Punkt 6: Zweige mit Schrägstrich (`refs/heads/a/b`)
            // liegen in Unterordnern; ein Commit dort ändert die Zeit von
            // `refs/heads` nicht. Darum jede Datei darunter einzeln, dazu
            // `packed-refs` (nach `git pack-refs` oder `gc` stehen die Zweige dort).
            for (name, meta) in zweige(&heads) {
                eintrag(format!(".git/refs/heads/{name}"), Some(meta));
            }
            eintrag(".git/packed-refs".into(), std::fs::metadata(gemeinsam.join("packed-refs")).ok());
        }
        Err(grund) => {
            // Kein auflösbarer Git-Ordner: nur die oberste Ebene zählt.
            einmal_melden(wurzel, &grund);
            for name in [".git/HEAD", ".git/index", ".git/refs/heads", ".git/packed-refs"] {
                eintrag(name.into(), None);
            }
        }
    }
    v.sort();
    v
}

/// Höchstens so viele Zweige zählt [`repo_stempel`] einzeln.
pub const ZWEIGE_HOECHSTENS: usize = 2000;

/// Alle Dateien unter `refs/heads` (auch in Unterordnern, also Zweige mit
/// Schrägstrich), Name mit `/`, bis [`ZWEIGE_HOECHSTENS`]. Blockiert.
fn zweige(heads: &Path) -> Vec<(String, std::fs::Metadata)> {
    let mut v = Vec::new();
    let mut stapel = vec![(heads.to_path_buf(), String::new())];
    while let Some((ordner, vorne)) = stapel.pop() {
        let Ok(rd) = std::fs::read_dir(&ordner) else { continue };
        for e in rd.flatten() {
            if v.len() >= ZWEIGE_HOECHSTENS {
                return v;
            }
            let Ok(meta) = e.metadata() else { continue };
            let name = format!("{vorne}{}", e.file_name().to_string_lossy());
            if meta.is_dir() {
                stapel.push((e.path(), format!("{name}/")));
            } else {
                v.push((name, meta));
            }
        }
    }
    v
}

/// Ein Eintrag des Fingerabdrucks: Name, Zeitstempel, Größe.
pub type RepoEintrag = (String, Option<SystemTime>, u64);

/// Höchstgröße der Dateien `.git` (Worktree) und `commondir`.
const GIT_DATEI_HOECHSTENS: u64 = 4096;

/// Eine kleine Textdatei, höchstens `GIT_DATEI_HOECHSTENS` Bytes (es wird
/// nie mehr gelesen). Nicht zu öffnen: `None`; zu groß: Grund.
fn klein_lesen(pfad: &Path) -> Result<Option<String>, String> {
    use std::io::Read;
    let Ok(f) = std::fs::File::open(pfad) else { return Ok(None) };
    let mut puffer = Vec::new();
    f.take(GIT_DATEI_HOECHSTENS + 1).read_to_end(&mut puffer).map_err(|e| format!("{} nicht lesbar: {e}", pfad.display()))?;
    if puffer.len() as u64 > GIT_DATEI_HOECHSTENS {
        return Err(format!("{} ist größer als {GIT_DATEI_HOECHSTENS} Bytes", pfad.display()));
    }
    Ok(Some(String::from_utf8_lossy(&puffer).into_owned()))
}

/// Nur Pfade auf diesem Rechner: kein UNC (`\\server\…`, `//server/…`), kein
/// `\\?\…` oder `\\.\…`. Sonst ginge alle 2 s ein Zugriff als Dienstkonto an
/// einen fremden Rechner.
fn lokal(p: &Path) -> bool {
    use std::path::{Component, Prefix};
    let s = p.as_os_str().to_string_lossy();
    let mut z = s.chars();
    if matches!((z.next(), z.next()), (Some('\\' | '/'), Some('\\' | '/'))) {
        return false;
    }
    match p.components().next() {
        Some(Component::Prefix(pre)) => matches!(pre.kind(), Prefix::Disk(_)),
        _ => true,
    }
}

/// Ein Ziel aus `.git` oder `commondir` (relativ zu `basis`), nur lokal.
fn ziel_aus(basis: &Path, wert: &str, woher: &str) -> Result<PathBuf, String> {
    let ziel = basis.join(wert);
    if lokal(Path::new(wert)) && lokal(&ziel) { Ok(ziel) } else { Err(format!("{woher} zeigt auf keinen lokalen Pfad ({wert})")) }
}

/// Der Git-Ordner eines Repos und der gemeinsame Ordner (bei einem Worktree
/// ist `.git` eine Datei mit `gitdir: …`, die Zweige liegen im Haupt-Repo).
/// Abgelehnt (`Err`): `.git` oder `commondir` zu groß, oder eines zeigt
/// nicht auf einen lokalen Pfad.
fn git_ordner(wurzel: &Path) -> Result<(PathBuf, PathBuf), String> {
    let punkt_git = wurzel.join(".git");
    if punkt_git.is_dir() {
        return Ok((punkt_git.clone(), punkt_git));
    }
    let gitdir = klein_lesen(&punkt_git)?.and_then(|t| t.lines().next().and_then(|z| z.strip_prefix("gitdir:")).map(|p| p.trim().to_string()));
    let Some(gitdir) = gitdir else { return Ok((punkt_git.clone(), punkt_git)) };
    let gitdir = ziel_aus(wurzel, &gitdir, "gitdir")?;
    let gemeinsam = match klein_lesen(&gitdir.join("commondir"))? {
        Some(c) => ziel_aus(&gitdir, c.trim(), "commondir")?,
        None => gitdir.clone(),
    };
    Ok((gitdir, gemeinsam))
}

/// Protokolliert einen abgelehnten Git-Ordner einmal je Repo.
fn einmal_melden(wurzel: &Path, grund: &str) {
    static GEMELDET: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());
    let mut g = GEMELDET.lock().unwrap_or_else(|e| e.into_inner());
    if !g.iter().any(|p| p == wurzel) {
        g.push(wurzel.to_path_buf());
        warn!("Repo {}: Git-Ordner nicht auflösbar ({grund}); nur Änderungen oben im Repo zählen.", wurzel.display());
    }
}

/// Eine Aufgabe des Projekts hat sich geändert (Etappe 5a): `change` mit
/// Bereich `aufgaben` und Pfad `aufgaben/<projekt>`, nie mit Inhalt.
pub fn aufgabe_geaendert(tx: &Sender, projekt: &str) {
    debug!("Aufgabe geändert: {projekt}");
    let _ = tx.send(Change { area: "aufgaben", path: format!("aufgaben/{projekt}"), at: chrono::Local::now() });
    anstehend_geaendert(tx, Some(projekt));
}

/// Etwas in „Anstehend“ hat sich geändert (Etappe A1): `change` mit Bereich
/// `anstehend` und Pfad `anstehend/<projekt>` (ohne Projekt, etwa bei
/// `data/schwerpunkte.toml`: `anstehend`), nie mit Inhalt. Kommt bei
/// Aufgaben, Arbeiten, Einstufungen und geänderten Dateien, aus denen die
/// Liste oder das Unter-Dashboard rechnet (fragen.toml, projekt.toml,
/// schwerpunkte.toml). Die Seite holt dann `/api/anstehend`
/// bzw. `/api/projekte/<slug>/stand` neu.
pub fn anstehend_geaendert(tx: &Sender, projekt: Option<&str>) {
    let path = match projekt {
        Some(p) => format!("anstehend/{p}"),
        None => "anstehend".to_string(),
    };
    let _ = tx.send(Change { area: "anstehend", path, at: chrono::Local::now() });
}

/// Entscheidung archiv-status: Ein archiviertes Projekt meldet sich
/// nicht auf dem Live-Kanal. `anstehend/<slug>` wird zu `anstehend` ohne
/// Projekt (die Seiten holen neu, was sie zeigen: Anstehend wegen der
/// Fristen-Regel, die eigene Projektseite folgt so weiter), `aufgaben/<slug>`
/// und `projects/<slug>/…` fallen weg (keine Zahl ändert sich, und niemand
/// sieht den Namen). `archiviert` sagt, ob ein Slug archiviert ist. Die
/// Ereignisse des ContentStores (`projekt`, `inhalt`) bleiben: Sie tragen
/// keinen Titel, und ohne sie folgten Seitenleiste und Projektseite nicht,
/// wenn sich der Status ändert.
pub fn ohne_archiv(c: Change, archiviert: impl Fn(&str) -> bool) -> Option<Change> {
    let slug = match c.area {
        "anstehend" => c.path.strip_prefix("anstehend/"),
        "aufgaben" => c.path.strip_prefix("aufgaben/"),
        "projects" => c.path.strip_prefix("projects/").and_then(|r| r.split('/').next()),
        _ => None,
    };
    match slug {
        Some(s) if archiviert(s) => (c.area == "anstehend").then(|| Change { path: "anstehend".to_string(), ..c }),
        _ => Some(c),
    }
}

/// Welches Projekt eine geänderte Datei für „Anstehend“ betrifft: `Some(Some(slug))`
/// für `projects/<slug>/projekt.toml`, `daten/fragen.toml` und (Etappe
/// B3: bewegliche Fristen der Aufgaben, nächste Frist, Archiv-Zeile) `daten/fristen.toml`
/// und (Etappe B8: der Plan lädt neu) `daten/plan.toml`,
/// `Some(None)` für `data/schwerpunkte.toml` und `data/gruppen.toml` (der
/// Gruppentitel steht an jedem Eintrag), sonst `None`.
fn anstehend_datei(path: &str) -> Option<Option<&str>> {
    if path == "data/schwerpunkte.toml" || path == "data/gruppen.toml" {
        return Some(None);
    }
    let rest = path.strip_prefix("projects/")?;
    let (slug, datei) = rest.split_once('/')?;
    matches!(datei, "projekt.toml" | "daten/fragen.toml" | "daten/fristen.toml" | "daten/plan.toml").then_some(Some(slug))
}

/// Für den Git-Zwischenspeicher: Geänderte Git-Stände gehen an den
/// ContentStore (neue Version je Projekt); der schickt `projekt` oder, bei
/// vielen zugleich, ein Sammelereignis `inhalt`.
pub fn git_melder(inhalt: Arc<ContentStore>) -> crate::git_zwischenspeicher::Melder {
    Arc::new(move |slugs: &[String]| inhalt.git_geaendert(slugs))
}

/// Jeder wievielte Blick (alle 2 s) auch tief in die Repos schaut
/// ([`repo_stempel_tief`]): alle 10 s.
pub const TIEF_JEDES: u32 = 5;

/// Beobachtet `projects/` und `data/` alle 2 s über den ContentStore
/// ([`ContentStore::abgleichen`]): Der liest nur Geändertes neu und schickt
/// `projekt`, `daten` oder `inhalt`; danach geht für jede geänderte Datei
/// wie bisher ein `change` hinaus (der Speicher ist dann schon neu). Prüft
/// dazu die Repos aller Projekte (Fingerabdruck über den Boten, wenn er es
/// kann, sonst selbst) und meldet Änderungen dort dem Git-Zwischenspeicher,
/// ebenso jede Änderung unter `projects/` (neue Repos). Endet beim
/// Abschaltsignal.
pub fn spawn_watcher(
    tx: Sender,
    inhalt: Arc<ContentStore>,
    git: Arc<Zwischenspeicher>,
    bote: Option<Arc<crate::bote::client::Bote>>,
    weitere: WeitereWachen,
    abschaltung: impl std::future::Future<Output = ()> + Send + 'static,
) {
    tokio::spawn(async move {
        tokio::pin!(abschaltung);
        let mut abtaster = RepoAbtaster { bote, bote_kann_nicht_bis: None };
        let mut flach = abtaster.abtasten(&git, false).await;
        let mut tief_zuletzt = abtaster.abtasten(&git, true).await;
        info!("Beobachte {} und data/ auf Änderungen", inhalt.ordner().display());
        // Live-Lücken: docs/ und assets/ der Projekte im selben Takt.
        info!("Beobachte {} und {} und assets/ der Projekte", weitere.entscheidungen.display(), weitere.zielbild.display());
        let mut wachen = weitere.anfangen().await;
        let mut runde: u32 = 0;
        loop {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                _ = &mut abschaltung => return,
            }
            runde = runde.wrapping_add(1);
            let store = inhalt.clone();
            let aenderungen = match tokio::task::spawn_blocking(move || store.abgleichen()).await {
                Ok(a) => a,
                Err(e) => {
                    warn!("Beobachter: Abgleich abgebrochen: {e}");
                    continue;
                }
            };
            let mut projekte_geaendert = false;
            for path in &aenderungen.pfade {
                emit(&tx, path);
                projekte_geaendert |= path.starts_with("projects/");
            }
            if projekte_geaendert {
                git.projekte_geaendert();
            }
            wachen = weitere.pruefen(wachen, &inhalt).await;
            // Neu bekannte Repos zählen erst ab dem nächsten Blick (der
            // Zwischenspeicher fragt sie ohnehin gerade ab).
            let tief = runde.is_multiple_of(TIEF_JEDES);
            let jetzt = abtaster.abtasten(&git, tief).await;
            let zuletzt = if tief { &mut tief_zuletzt } else { &mut flach };
            // Nur Abdrücke derselben Quelle vergleichen: Der Bote sieht in
            // die Repos des Inhabers, der Dienst meist nicht; ein Wechsel ist keine Änderung.
            if zuletzt.0 == jetzt.0 {
                for (pfad, stempel) in &jetzt.1 {
                    if zuletzt.1.get(pfad).is_some_and(|alt| alt != stempel) {
                        debug!("Änderung im Repo {pfad}");
                        git.aenderung_gemeldet(pfad);
                    }
                }
            }
            *zuletzt = jetzt;
        }
    });
}

/// Was der Beobachter außer `projects/`, `data/` und den Repos ansieht
/// (Live-Lücken): `docs/entscheidungen` und `docs/zielbild` (Ereignis `docs`
/// über den Melder) und `assets/` der Projekte in allen Inhaltsquellen
/// (Ereignis `projekt` mit `assets: true` über den ContentStore).
pub struct WeitereWachen {
    pub melder: Arc<crate::aenderungen::Melder>,
    /// Die Entscheidungen (wie `/api/entscheidungen` sie liest; privater Inhalt).
    pub entscheidungen: PathBuf,
    /// `docs/zielbild` im Code-Repo (wie `/api/zielbild`).
    pub zielbild: PathBuf,
    /// Die Inhaltsquellen (Elternordner der Projekte).
    pub quellen: Vec<PathBuf>,
}

type Wachen = (crate::aenderungen::DocsWache, crate::aenderungen::AssetWache);

impl WeitereWachen {
    /// Der erste Stand (abseits der Tokio-Fäden).
    async fn anfangen(&self) -> Option<Wachen> {
        let (e, z) = (self.entscheidungen.clone(), self.zielbild.clone());
        tokio::task::spawn_blocking(move || (crate::aenderungen::DocsWache::mit(&e, &z), crate::aenderungen::AssetWache::default())).await.ok()
    }

    /// Ein Blick: geänderte docs-Ordner gehen an den Melder, Projekte mit
    /// geänderten assets/ an den ContentStore. Gibt die Wachen für den
    /// nächsten Blick zurück (nach einem Abbruch neu angefangen).
    async fn pruefen(&self, wachen: Option<Wachen>, inhalt: &ContentStore) -> Option<Wachen> {
        let Some((mut d, mut a)) = wachen else { return self.anfangen().await };
        let projekte: Vec<String> = inhalt.bestand().projekte.keys().cloned().collect();
        let quellen = self.quellen.clone();
        let erg = tokio::task::spawn_blocking(move || {
            let docs = d.pruefen();
            // Ein Eintrag kann selbst ein Projekt sein: der Ordner kommt aus der aufgelösten Liste, nicht aus `eintrag/slug`.
            let q = crate::quellen::Quellen::aus_liste(quellen);
            let orte: Vec<(String, PathBuf)> = projekte.into_iter().filter_map(|s| q.ordner(&s).filter(|p| p.is_dir()).map(|p| (s, p))).collect();
            let assets = a.pruefen(&orte);
            (d, a, docs, assets)
        })
        .await;
        match erg {
            Ok((d, a, docs, assets)) => {
                for b in docs {
                    self.melder.melden(b);
                }
                if !assets.is_empty() {
                    inhalt.assets_geaendert(&assets);
                }
                Some((d, a))
            }
            Err(e) => {
                warn!("Beobachter: docs/ und assets/ nicht geprüft ({e}); die Wache fängt neu an");
                None
            }
        }
    }
}

/// Fingerabdrücke der Repos: über den Boten (er läuft als Inhaber und darf in
/// ihre Repos schauen; der Dienst meist nicht), sonst selbst. Kennt der Bote
/// die Anfrage nicht (ältere Fassung), schaut der Dienst selbst und fragt
/// erst nach [`BOTE_NEU_FRAGEN`] wieder.
struct RepoAbtaster {
    bote: Option<Arc<crate::bote::client::Bote>>,
    bote_kann_nicht_bis: Option<std::time::Instant>,
}

/// So lange fragt der Dienst einen Boten, der keine Fingerabdrücke kennt, nicht wieder.
pub const BOTE_NEU_FRAGEN: Duration = Duration::from_secs(10 * 60);

impl RepoAbtaster {
    /// Die Abdrücke aller bekannten Repos; `true`, wenn sie vom Boten kommen.
    async fn abtasten(&mut self, git: &Zwischenspeicher, tief: bool) -> (bool, HashMap<String, u64>) {
        let pfade = git.repo_pfade();
        if let Some(bote) = self.bote.clone() {
            let darf = self.bote_kann_nicht_bis.is_none_or(|t| std::time::Instant::now() >= t);
            if darf && !pfade.is_empty() {
                match ueber_boten(&bote, &pfade, tief).await {
                    Ok(m) => {
                        self.bote_kann_nicht_bis = None;
                        return (true, m);
                    }
                    Err(BoteKann::Nicht(satz)) => {
                        if self.bote_kann_nicht_bis.is_none() {
                            info!("Repo-Fingerabdrücke: Der Bote liefert sie nicht ({satz}); der Dienst schaut selbst, neuer Versuch in 10 min.");
                        }
                        self.bote_kann_nicht_bis = Some(std::time::Instant::now() + BOTE_NEU_FRAGEN);
                    }
                    Err(BoteKann::Gerade) => {}
                }
            }
        }
        (false, selbst_abtasten(pfade, tief).await)
    }
}

/// Warum der Bote keine Fingerabdrücke geliefert hat.
enum BoteKann {
    /// Er kennt die Anfrage nicht (ältere Fassung) oder lehnt sie ab.
    Nicht(String),
    /// Nicht verbunden oder ausgelastet: diesmal selbst schauen.
    Gerade,
}

async fn ueber_boten(bote: &crate::bote::client::Bote, pfade: &[String], tief: bool) -> Result<HashMap<String, u64>, BoteKann> {
    use crate::vertrag::bote::{Anfrage, Antwort};
    if !bote.stand().verbunden {
        return Err(BoteKann::Gerade);
    }
    let anfrage = Anfrage::RepoStempel { pfade: pfade.to_vec(), tief };
    match bote.anfragen_mit_frist(&anfrage, Duration::from_secs(2)).await {
        Ok(Antwort::RepoStempel(liste)) => Ok(liste.into_iter().map(|f| (f.pfad, f.stempel)).collect()),
        Ok(Antwort::Fehler { satz }) => Err(BoteKann::Nicht(satz)),
        Ok(anders) => Err(BoteKann::Nicht(format!("unpassende Antwort {}", anders.art()))),
        Err(_) => Err(BoteKann::Gerade),
    }
}

/// Fingerabdrücke der Repos, abseits der Tokio-Fäden.
async fn selbst_abtasten(pfade: Vec<String>, tief: bool) -> HashMap<String, u64> {
    tokio::task::spawn_blocking(move || {
        pfade
            .into_iter()
            .map(|p| {
                let s = fingerabdruck(Path::new(&p), tief);
                (p, s)
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

/// So viele Einträge sieht [`repo_stempel_tief`] höchstens an (je Repo).
pub const TIEF_HOECHSTENS: usize = 4000;

/// Ordner, in die der tiefe Blick nie hinabsteigt (erzeugt, nicht von Hand).
const NICHT_HINAB: &[&str] = &[".git", "node_modules", "target", "build", "builds", "dist", ".svelte-kit", ".venv", "__pycache__"];

/// Wie [`repo_stempel`], dazu Zeitstempel und Größe der Dateien darunter, bis
/// [`TIEF_HOECHSTENS`] Einträge, ohne erzeugte Ordner (`NICHT_HINAB`). So
/// fallen auch tief im Repo geänderte, nicht gestagte Dateien auf, nach
/// höchstens 10 s statt erst beim 60-s-Takt. Blockiert.
pub fn repo_stempel_tief(wurzel: &Path) -> Vec<RepoEintrag> {
    let mut v = repo_stempel(wurzel);
    let mut stapel = vec![(wurzel.to_path_buf(), String::new())];
    let mut gesehen = 0usize;
    while let Some((ordner, vorne)) = stapel.pop() {
        let Ok(rd) = std::fs::read_dir(&ordner) else { continue };
        let mut namen: Vec<_> = rd.flatten().collect();
        namen.sort_by_key(|e| e.file_name());
        for e in namen {
            gesehen += 1;
            if gesehen > TIEF_HOECHSTENS {
                v.sort();
                return v;
            }
            let name = e.file_name().to_string_lossy().to_string();
            let Ok(meta) = e.metadata() else { continue };
            let pfad = format!("{vorne}{name}");
            if meta.is_dir() {
                if !NICHT_HINAB.contains(&name.as_str()) {
                    stapel.push((e.path(), format!("{pfad}/")));
                }
            } else if !vorne.is_empty() {
                // Die oberste Ebene steht schon in `repo_stempel`.
                v.push((pfad, meta.modified().ok(), meta.len()));
            }
        }
    }
    v.sort();
    v
}

/// Ein Fingerabdruck als Zahl, flach ([`repo_stempel`]) oder tief.
pub fn fingerabdruck(wurzel: &Path, tief: bool) -> u64 {
    use std::hash::{Hash, Hasher};
    let eintraege = if tief { repo_stempel_tief(wurzel) } else { repo_stempel(wurzel) };
    let mut h = std::collections::hash_map::DefaultHasher::new();
    eintraege.hash(&mut h);
    h.finish()
}

fn emit(tx: &Sender, path: &str) {
    let area = if path.starts_with("data/") { "data" } else { "projects" };
    debug!("Änderung: {path}");
    let _ = tx.send(Change { area, path: path.to_string(), at: chrono::Local::now() });
    if let Some(projekt) = anstehend_datei(path) {
        anstehend_geaendert(tx, projekt);
    }
}

/// Kompakte PC-Werte für den Live-Kanal (alle 5 s).
#[derive(Debug, Clone, Serialize, TS)]
pub struct PcTick {
    #[ts(type = "string | null")]
    pub at: Option<chrono::DateTime<chrono::Local>>,
    pub cpu_percent: f32,
    pub mem_used: u64,
    pub mem_total: u64,
    pub disks: Vec<crate::pc::system::DiskInfo>,
    pub uptime_secs: u64,
    /// Belegung von Prozessor und Arbeitsspeicher wie im Snapshot (die Platten
    /// tragen ihre in `disks`).
    pub cpu_belegung: crate::pc::system::Belegung,
    pub mem_belegung: crate::pc::system::Belegung,
}

/// Ereignis `nachholen`: Der Kanal ist übergelaufen, weil die Seite zu
/// langsam las; Ereignisse sind verloren. Die Seite lädt dann alles neu.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "NachholEvent")]
pub struct NachholEreignis {
    #[ts(type = "\"ueberlauf\"")]
    pub grund: &'static str,
}

/// `nachholen` nach einem Überlauf von `n` Ereignissen.
fn nachholen(wo: &str, n: u64, frei: bool) -> Option<Event> {
    debug!("Live-Kanal `{wo}`: {n} Ereignisse verloren, die Seite soll nachholen");
    ereignis(Art::Nachholen, frei, &NachholEreignis { grund: "ueberlauf" })
}

/// Ein Kanal als Strom von SSE-Ereignissen der Art `art`. Läuft der Kanal
/// über (`Lagged`), kommt statt der verlorenen Ereignisse einmal `nachholen`.
/// Auf dem freien Kanal (`frei`) bleibt eine dort nicht erlaubte Art stumm.
fn kanal_strom<T>(rx: broadcast::Receiver<T>, art: Art, frei: bool) -> impl Stream<Item = Option<Event>>
where
    T: Clone + Send + Serialize + 'static,
{
    BroadcastStream::new(rx).map(move |r| match r {
        Ok(v) => ereignis(art, frei, &v),
        Err(BroadcastStreamRecvError::Lagged(n)) => nachholen(art.name(), n, frei),
    })
}

/// Wie [`kanal_strom`] auf dem geschützten Kanal, aber Ereignisse eines
/// archivierten Projekts (`slug`) bleiben stumm (Entscheidung
/// archiv-status: Vorgänge und Bauten seines Chats).
pub(crate) fn ohne_archiv_strom<T>(state: &AppState, rx: broadcast::Receiver<T>, art: Art, slug: fn(&T) -> &str) -> impl Stream<Item = Option<Event>> + use<T>
where
    T: Clone + Send + Serialize + 'static,
{
    let inhalt = state.inhalt.clone();
    BroadcastStream::new(rx).map(move |r| match r {
        Ok(v) if inhalt.bestand().archiviert(slug(&v)) => None,
        Ok(v) => ereignis(art, false, &v),
        Err(BroadcastStreamRecvError::Lagged(n)) => nachholen(art.name(), n, false),
    })
}

/// Die `change`-Ereignisse des geschützten Kanals, ohne archivierte Projekte
/// ([`ohne_archiv`], mit dem Stand des ContentStores zur Zeit des Ereignisses);
/// läuft der Kanal über, kommt einmal `nachholen`.
pub(crate) fn change_strom(state: &AppState) -> impl Stream<Item = Option<Event>> + use<> {
    let inhalt = state.inhalt.clone();
    BroadcastStream::new(state.events.subscribe()).map(move |r| match r {
        Ok(c) => {
            let bestand = inhalt.bestand();
            ohne_archiv(c, |s| bestand.archiviert(s)).and_then(|c| ereignis(Art::Change, false, &c))
        }
        Err(BroadcastStreamRecvError::Lagged(n)) => nachholen(Art::Change.name(), n, false),
    })
}

/// Die Ereignisse des ContentStores (`projekt`, `daten`, `inhalt`); läuft
/// der Kanal über, kommt einmal `nachholen`. Auf dem freien Kanal nur, was
/// [`InhaltEreignis::nur_frei`] durchlässt.
fn inhalt_strom(rx: broadcast::Receiver<InhaltEreignis>, frei: bool) -> impl Stream<Item = Option<Event>> {
    BroadcastStream::new(rx).map(move |r| match r {
        Ok(e) if frei => e.nur_frei().and_then(|e| e.als_sse(true)),
        Ok(e) => e.als_sse(false),
        Err(BroadcastStreamRecvError::Lagged(n)) => nachholen("inhalt", n, frei),
    })
}

/// Die Ereignisse mit Bereich (`speicher`, `pc-benutzer`, `docs`; Live-Lücken),
/// je nach Gruppe des Bereichs; läuft der Kanal über, einmal `nachholen`. Auf
/// dem freien Kanal hielte [`ereignis`] alle drei zurück.
fn bereich_strom(rx: broadcast::Receiver<crate::aenderungen::BereichEreignis>, frei: bool) -> impl Stream<Item = Option<Event>> {
    BroadcastStream::new(rx).map(move |r| match r {
        Ok(e) => ereignis(Art::zu_bereich(e.bereich), frei, &e),
        Err(BroadcastStreamRecvError::Lagged(n)) => nachholen("bereich", n, frei),
    })
}

/// Die PC-Werte alle 5 s (frei wie `/api/pc`).
fn pc_takt(state: &AppState, frei: bool) -> impl Stream<Item = Option<Event>> + use<> {
    let snap = state.snapshot.clone();
    IntervalStream::new(tokio::time::interval(Duration::from_secs(5))).then(move |_| {
        let snap = snap.clone();
        async move {
            let s = snap.read().await;
            let tick = PcTick {
                at: s.taken_at,
                cpu_percent: s.cpu_percent,
                mem_used: s.mem_used,
                mem_total: s.mem_total,
                disks: s.disks.clone(),
                uptime_secs: s.uptime_secs,
                cpu_belegung: s.cpu_belegung,
                mem_belegung: s.mem_belegung,
            };
            // Auch auf dem geschützten Kanal wie `/api/pc`: Laufwerke ohne Profil und Namen.
            let mut wert = serde_json::to_value(&tick).ok()?;
            crate::datenschutz::json_verbergen(&mut wert);
            ereignis(Art::Pc, frei, &wert)
        }
    })
}

/// Der geschützte Live-Kanal `/api/events` (7b): alles, auch Vorgänge.
///
/// Kopfleiste, Fassung 2 (Nachbesserung): Als Erstes kommt `anmeldung` mit
/// `angemeldet` (dasselbe Ereignis wie auf dem freien Kanal; die Seite hält je
/// Seite nur einen Kanal offen). Bei einer Sitzung mit Passkey wacht
/// [`sitzung_waechter`] über den Strom: Wird sie widerrufen oder abgemeldet,
/// kommt zum Schluss `anmeldung` mit `nicht_angemeldet` und der Strom endet;
/// läuft der Ausweis ab, endet er ohne Wort (der Browser verbindet neu, mit
/// einem erneuerten Ausweis geht es weiter, sonst 401).
pub async fn sse(State(state): State<AppState>, inhaber: Option<Extension<Inhaber>>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let erstes = tokio_stream::once(ereignis(Art::Anmeldung, false, &AnmeldungEreignis { stand: Anmeldewort::Angemeldet }));
    let changes = change_strom(&state);
    let ticks = pc_takt(&state, false);
    let vorgaenge = ohne_archiv_strom(&state, state.vorgang_events.subscribe(), Art::Vorgang, |v: &VorgangEreignis| v.slug.as_str());
    let bauten = ohne_archiv_strom(&state, state.bau_events.subscribe(), Art::Bau, |b: &BauEreignis| b.slug.as_str());
    let fassungen = kanal_strom(state.fassung_events.subscribe(), Art::Fassung, false);
    let fortschritte = kanal_strom(state.bau_fortschritt.subscribe(), Art::BauFortschritt, false);
    let inhalt = inhalt_strom(state.inhalt.abonnieren(), false);
    let bereiche = bereich_strom(state.aenderungen.abonnieren(), false);
    let stream = erstes.chain(changes.merge(vorgaenge).merge(bauten).merge(fortschritte).merge(fassungen).merge(inhalt).merge(bereiche).merge(ticks));
    let sitzung = match inhaber {
        Some(Extension(Inhaber::Passkey(s))) => Some(s),
        _ => None,
    };
    let stream = mit_sitzungswache(stream, state.clone(), sitzung, SITZUNG_TAKT).filter_map(|e| e.map(Ok));
    // Voll qualifiziert: `tokio_stream::StreamExt` kennt kein `take_until`.
    let stream = futures_util::StreamExt::take_until(stream, state.abschaltung());
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("ping"))
}

/// So oft prüft der geschützte Kanal, ob eine Sitzung mit Passkey noch gilt.
pub const SITZUNG_TAKT: Duration = if cfg!(test) { Duration::from_millis(100) } else { Duration::from_secs(5) };

/// Warum eine Sitzung mit Passkey auf dem geschützten Kanal endet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SitzungsEnde {
    /// Der Ausweis ist abgelaufen (`exp`).
    Abgelaufen,
    /// Widerrufen, abgemeldet oder alle Sitzungen beendet (wie `ausweis_aus`).
    Beendet,
}

/// Wartet, bis die Sitzung endet: Ablauf des Ausweises oder dieselbe Prüfung
/// wie die Schleuse (`Speicher::sitzung_gilt`: Sperrliste, alle Sitzungen
/// beendet, Passkey noch registriert), alle `takt`. Lässt sich der
/// Datenspeicher gerade nicht lesen, zählt das nicht als Ende.
pub async fn sitzung_waechter(state: AppState, sitzung: crate::anmeldung::PasskeySitzung, takt: Duration) -> SitzungsEnde {
    loop {
        let jetzt = crate::speicher::aufgaben::jetzt().timestamp();
        if jetzt >= sitzung.ausweis.exp {
            return SitzungsEnde::Abgelaufen;
        }
        if let Some(speicher) = state.speicher.clone() {
            let (jti, cid, anmeldung) = (sitzung.ausweis.jti.clone(), sitzung.ausweis.cid.clone(), sitzung.ausweis.auth_time);
            match crate::speicher::Speicher::im_hintergrund(speicher, move |s| s.sitzung_gilt(&jti, &cid, anmeldung)).await {
                Ok(None) => return SitzungsEnde::Beendet,
                Ok(Some(_)) => {}
                Err(e) => debug!("Live-Kanal: Sitzung nicht prüfbar ({e}), weiter"),
            }
        }
        let bis_ablauf = Duration::from_secs(u64::try_from(sitzung.ausweis.exp - jetzt).unwrap_or(0));
        tokio::time::sleep(takt.min(bis_ablauf)).await;
    }
}

/// Der Strom bis zum Ende der Sitzung (ohne Sitzung mit Passkey unverändert);
/// bei `Beendet` zum Schluss `anmeldung` mit `nicht_angemeldet`.
fn mit_sitzungswache<S>(
    strom: S,
    state: AppState,
    sitzung: Option<crate::anmeldung::PasskeySitzung>,
    takt: Duration,
) -> impl Stream<Item = Option<Event>> + use<S>
where
    S: Stream<Item = Option<Event>> + Send + 'static,
{
    let (tx, rx) = tokio::sync::oneshot::channel::<SitzungsEnde>();
    let wache = async move {
        match sitzung {
            Some(s) => {
                let ende = sitzung_waechter(state, s, takt).await;
                info!("Live-Kanal: Sitzung mit Passkey {}, Strom endet", if ende == SitzungsEnde::Beendet { "beendet" } else { "abgelaufen" });
                let _ = tx.send(ende);
            }
            None => std::future::pending::<()>().await,
        }
    };
    let schluss = futures_util::stream::once(async move {
        match rx.await {
            Ok(SitzungsEnde::Beendet) => ereignis(Art::Anmeldung, false, &AnmeldungEreignis { stand: Anmeldewort::NichtAngemeldet }),
            _ => None,
        }
    });
    futures_util::StreamExt::chain(futures_util::StreamExt::take_until(strom, wache), schluss)
}

/// Der Strom des freien Kanals ohne Lebenszeichen: zuerst `anmeldung`, dann
/// nur Arten aus [`FREIER_KANAL`]. Die Kanäle der Änderungen (`change`), der
/// Vorgänge und der Bauten abonniert er gar nicht erst; was trotzdem käme,
/// hielte [`ereignis`] zurück.
pub fn frei_strom(state: &AppState, wort: Anmeldewort) -> impl Stream<Item = Option<Event>> + use<> {
    let erstes = tokio_stream::once(ereignis(Art::Anmeldung, true, &AnmeldungEreignis { stand: wort }));
    let fassungen = kanal_strom(state.fassung_events.subscribe(), Art::Fassung, true);
    let inhalt = inhalt_strom(state.inhalt.abonnieren(), true);
    erstes.chain(fassungen.merge(inhalt).merge(pc_takt(state, true)))
}

/// Der freie Kanal `/api/events/frei` (Kopfleiste, Fassung 2): ohne
/// Anmeldung, hinter Adresse, Herkunft und den übrigen Schleusen. Der
/// Anmeldestand kommt aus dem, was die Schleuse an die Anfrage gehängt hat
/// (`api::passkey::anmeldewort`). Das Lebenszeichen ist ein Kommentar alle
/// 15 s, wie beim geschützten Kanal.
pub async fn sse_frei(
    State(state): State<AppState>,
    inhaber: Option<Extension<Inhaber>>,
    konto: Option<Extension<WindowsKonto>>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let wort = super::passkey::anmeldewort(&state, inhaber.is_some(), konto.map(|Extension(k)| k)).await;
    let stream = frei_strom(&state, wort).filter_map(|e| e.map(Ok));
    let stream = futures_util::StreamExt::take_until(stream, state.abschaltung());
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)).text("lebenszeichen"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Welche Dateien Anstehend neu rechnen lassen; seit Etappe B3 auch `daten/fristen.toml`.
    #[test]
    fn anstehend_dateien() {
        assert_eq!(anstehend_datei("projects/x/daten/fragen.toml"), Some(Some("x")));
        assert_eq!(anstehend_datei("projects/x/daten/fristen.toml"), Some(Some("x")));
        assert_eq!(anstehend_datei("projects/x/daten/plan.toml"), Some(Some("x")), "der Plan lädt neu");
        assert_eq!(anstehend_datei("projects/x/daten/zeitstrahl.toml"), None, "der Zeitstrahl zählt für keine Frist");
        assert_eq!(anstehend_datei("data/gruppen.toml"), Some(None));
    }
    use crate::inhalt::store::scan;

    /// Was ein Strom als SSE-Text schickt (bis der Kanal zu ist).
    async fn als_text(strom: impl Stream<Item = Option<Event>> + Send + 'static) -> String {
        use axum::response::IntoResponse;
        use http_body_util::BodyExt;
        let sse = Sse::new(strom.filter_map(|e| e.map(Ok::<_, Infallible>)));
        let koerper = tokio::time::timeout(Duration::from_secs(3), sse.into_response().into_body().collect()).await.unwrap().unwrap();
        String::from_utf8(koerper.to_bytes().to_vec()).unwrap()
    }

    /// Die Namen der Ereignisse in einem SSE-Text, in Reihenfolge.
    fn namen(text: &str) -> Vec<&str> {
        text.lines().filter_map(|z| z.strip_prefix("event: ")).collect()
    }

    /// `Art::ALLE` enthält jede Art genau einmal. Der `match` ohne `_` bricht
    /// den Bau, sobald eine neue Art dazukommt; dann gehört sie in `ALLE` und
    /// muss für den freien Kanal entschieden werden.
    #[test]
    fn alle_arten_vollstaendig() {
        fn stelle(a: Art) -> usize {
            match a {
                Art::Change => 0,
                Art::Vorgang => 1,
                Art::Bau => 2,
                Art::Fassung => 3,
                Art::Projekt => 4,
                Art::Daten => 5,
                Art::Inhalt => 6,
                Art::Pc => 7,
                Art::Nachholen => 8,
                Art::Anmeldung => 9,
                Art::Speicher => 10,
                Art::PcBenutzer => 11,
                Art::Docs => 12,
                Art::BauFortschritt => 13,
            }
        }
        for (i, a) in Art::ALLE.iter().enumerate() {
            assert_eq!(stelle(*a), i, "{a:?}");
        }
        let namen: std::collections::BTreeSet<_> = Art::ALLE.iter().map(|a| a.name()).collect();
        assert_eq!(namen.len(), Art::ALLE.len(), "Namen eindeutig");
    }

    /// Wächter (Kopfleiste, Fassung 2): Was nur der geschützte Kanal tragen
    /// darf (Änderungen samt Aufgaben und Anstehend, Vorgänge, Bauten), steht
    /// nicht auf der Erlaubnisliste des freien Kanals, und die Liste nennt
    /// nur die Arten aus der Entscheidung. `ereignis` hält eine nicht
    /// erlaubte Art auf dem freien Kanal zurück.
    #[test]
    fn waechter_erlaubnisliste_des_freien_kanals() {
        // Live-Lücken: Datenspeicher, Benutzer-Daten und docs/ sind geschützt.
        for a in [Art::Change, Art::Vorgang, Art::Bau, Art::Speicher, Art::PcBenutzer, Art::Docs, Art::BauFortschritt] {
            assert!(!a.frei(), "{a:?} gehört nur auf den geschützten Kanal");
            assert!(ereignis(a, true, &"x").is_none(), "{a:?}");
            assert!(ereignis(a, false, &"x").is_some(), "{a:?} geht geschützt");
        }
        let frei: Vec<&str> = FREIER_KANAL.iter().map(|a| a.name()).collect();
        assert_eq!(frei, ["fassung", "projekt", "daten", "inhalt", "pc", "nachholen", "anmeldung"]);
        for a in Art::ALLE {
            assert_eq!(a.frei(), FREIER_KANAL.contains(&a), "{a:?}");
        }
        assert_eq!(FREIE_DATEN, ["pc", "schwerpunkte", "gruppen"], "nur PC-Werte, Schwerpunkte und Gruppen aus data/");
    }

    /// Wächter von außen: Alle Kanäle des Servers bekommen je ein Ereignis,
    /// der freie Strom zeigt davon nur Erlaubtes; `change`, `vorgang` und
    /// `bau` kommen nie an, `data/schluessel.toml` auch nicht.
    #[tokio::test]
    async fn waechter_freier_strom_traegt_nichts_geschuetztes() {
        let state = crate::app::AppState::fuer_tests(&crate::app::beispiel_fuer_tests());
        let strom = frei_strom(&state, Anmeldewort::NichtAngemeldet);
        let geschuetzt = {
            let changes = kanal_strom(state.events.subscribe(), Art::Change, false);
            let vorgaenge = kanal_strom(state.vorgang_events.subscribe(), Art::Vorgang, false);
            let bauten = kanal_strom(state.bau_events.subscribe(), Art::Bau, false);
            let fortschritte = kanal_strom(state.bau_fortschritt.subscribe(), Art::BauFortschritt, false);
            let bereiche = bereich_strom(state.aenderungen.abonnieren(), false);
            changes.merge(vorgaenge).merge(bauten).merge(fortschritte).merge(bereiche)
        };
        aufgabe_geaendert(&state.events, "alpha");
        // Live-Lücken: je eine Art mit Bereich.
        for b in [crate::aenderungen::Bereich::Aufgaben, crate::aenderungen::Bereich::Bote, crate::aenderungen::Bereich::Entscheidungen] {
            state.aenderungen.melden(b);
        }
        emit(&state.events, "data/schluessel.toml");
        state
            .vorgang_events
            .send(VorgangEreignis { id: "v1".into(), slug: "alpha".into(), art: VorgangsArt::Abnahme, zustand: Zustand::Offen, nr: 1 })
            .unwrap();
        state.bau_events.send(BauEreignis { slug: "alpha".into(), zustand: BauZustand::Rot, meldung: "rot".into(), gescheitert: false }).unwrap();
        state.fassung_events.send(FassungEreignis { ordner: "20261031-120000".into(), dauer_ms: 1 }).unwrap();
        // Bau-Tempo, Zweig C: der Schrittzähler nur geschützt.
        state
            .bau_fortschritt
            .send(crate::vertrag::bote::BauFortschritt { schritt: crate::vertrag::bote::FortschrittSchritt::Pruefung, fertig: 7, gesamt: 21 })
            .unwrap();
        // Aus data/: die Schwerpunkte frei (Seitenleiste ohne Anmeldung), die Schlüssel nie.
        state.inhalt.senden_fuer_tests(InhaltEreignis::Daten(DatenEreignis { name: "schluessel".into(), version: 41 }));
        state.inhalt.senden_fuer_tests(InhaltEreignis::Daten(DatenEreignis { name: "schwerpunkte".into(), version: 42 }));
        let bis = || tokio::time::sleep(Duration::from_millis(400));
        let text = als_text(futures_util::StreamExt::take_until(strom, bis())).await;
        let n = namen(&text);
        assert!(text.contains("{\"name\":\"schwerpunkte\",\"version\":42}"), "{text}");
        assert!(!text.contains("\"version\":41"), "{text}");
        assert_eq!(n.first(), Some(&"anmeldung"), "{text}");
        assert!(text.contains("data: {\"stand\":\"nicht_angemeldet\"}"), "{text}");
        assert!(n.contains(&"fassung") && n.contains(&"pc"), "{text}");
        for name in &n {
            assert!(FREIER_KANAL.iter().any(|a| a.name() == *name), "{name} auf dem freien Kanal: {text}");
        }
        assert!(!text.contains("alpha") && !text.contains("schluessel") && !text.contains("aufgaben") && !text.contains("bereich"), "{text}");
        assert!(!text.contains("bau-fortschritt") && !text.contains("\"fertig\"") && !text.contains("\"gesamt\""), "ohne Anmeldung kein Schrittzähler: {text}");
        // Zum Vergleich: Der geschützte Kanal hat sie bekommen.
        let g = als_text(futures_util::StreamExt::take_until(geschuetzt, bis())).await;
        for name in ["change", "vorgang", "bau", "bau-fortschritt", "speicher", "pc-benutzer", "docs"] {
            assert!(namen(&g).contains(&name), "{name}: {g}");
        }
        assert!(g.contains(r#"{"schritt":"pruefung","fertig":7,"gesamt":21}"#), "{g}");
    }

    /// Bau-Tempo, Zweig C: Ohne Anmeldung kein Schrittzähler, auch nicht in
    /// der freien Projektliste (frei bleiben Zustand und Zeit). Gegenprobe:
    /// Der Fortschritt selbst hätte die gesuchten Felder.
    #[tokio::test]
    async fn projektliste_ohne_schrittzaehler() {
        use axum::body::Body;
        use axum::http::{Request, StatusCode};
        use http_body_util::BodyExt;
        use tower::ServiceExt;
        let state = AppState::fuer_tests(&crate::app::beispiel_fuer_tests());
        let f = crate::vertrag::bote::BauFortschritt { schritt: crate::vertrag::bote::FortschrittSchritt::Pruefung, fertig: 7, gesamt: 21 };
        let _rx = state.bau_fortschritt.subscribe();
        state.bau_fortschritt.send(f).unwrap();
        let felder = ["\"schritt\"", "\"fertig\"", "\"gesamt\""];
        let gegen = serde_json::to_string(&f).unwrap();
        assert!(felder.iter().all(|x| gegen.contains(x)), "{gegen}");
        let req = Request::get("/api/projects").header("host", "127.0.0.1:8787").body(Body::empty()).unwrap();
        let res = crate::app::router(state).oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let text = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
        assert!(text.contains("\"projects\""), "{text}");
        for x in felder {
            assert!(!text.contains(x), "{x} in /api/projects ohne Anmeldung");
        }
    }

    /// Der PC-Takt nennt die Laufwerke auf
    /// beiden Kanälen ohne Profil und Namen, wie `/api/pc`.
    #[tokio::test]
    async fn pc_takt_ohne_profil() {
        let state = AppState::fuer_tests(&crate::app::beispiel_fuer_tests());
        state.snapshot.write().await.disks = vec![crate::pc::system::DiskInfo {
            name: r"Ablage (C:\Users\beispiel-konto\Laufwerk)".into(),
            mount: r"C:\Users\beispiel-konto\Laufwerk".into(),
            total: 10,
            used: 1,
            belegung: Default::default(),
        }];
        for frei in [true, false] {
            let mut strom = Box::pin(pc_takt(&state, frei));
            let e = tokio::time::timeout(Duration::from_secs(3), strom.next()).await.unwrap().flatten().expect("ein Takt");
            let text = als_text(tokio_stream::once(Some(e))).await;
            assert!(text.contains(r#""mount":"%USERPROFILE%\\Laufwerk""#), "{text}");
            assert!(!text.contains("beispiel-konto") && !text.contains(r"Users\\"), "{text}");
        }
    }

    /// Inhalt auf dem freien Kanal: Projekte ja, `daten` nur für `pc` und
    /// `schwerpunkte`, im Sammelereignis nur die freien Dateien; bleibt nichts,
    /// kommt nichts.
    #[tokio::test]
    async fn freier_kanal_filtert_daten() {
        let (tx, rx) = broadcast::channel::<InhaltEreignis>(8);
        tx.send(InhaltEreignis::Projekt(ProjektEreignis { slug: "alpha".into(), version: 1, assets: None, dateien: None })).unwrap();
        tx.send(InhaltEreignis::Daten(DatenEreignis { name: "schluessel".into(), version: 2 })).unwrap();
        tx.send(InhaltEreignis::Daten(DatenEreignis { name: "pc".into(), version: 3 })).unwrap();
        tx.send(InhaltEreignis::Sammel(SammelEreignis { version: 4, projekte: vec!["a".into()], daten: vec!["schluessel".into(), "pc".into()] })).unwrap();
        tx.send(InhaltEreignis::Sammel(SammelEreignis { version: 5, projekte: vec![], daten: vec!["schwerpunkte".into(), "schluessel".into()] })).unwrap();
        tx.send(InhaltEreignis::Daten(DatenEreignis { name: "schwerpunkte".into(), version: 6 })).unwrap();
        tx.send(InhaltEreignis::Sammel(SammelEreignis { version: 7, projekte: vec![], daten: vec!["schluessel".into()] })).unwrap();
        drop(tx);
        let text = als_text(inhalt_strom(rx, true)).await;
        assert_eq!(namen(&text), ["projekt", "daten", "inhalt", "inhalt", "daten"], "{text}");
        assert!(text.contains("{\"name\":\"pc\",\"version\":3}"), "{text}");
        assert!(text.contains("{\"version\":4,\"projekte\":[\"a\"],\"daten\":[\"pc\"]}"), "{text}");
        // Schwerpunkte frei (nur Name und Version), Schlüssel nie, auch nicht im Sammelereignis.
        assert!(text.contains("{\"version\":5,\"projekte\":[],\"daten\":[\"schwerpunkte\"]}"), "{text}");
        assert!(text.contains("{\"name\":\"schwerpunkte\",\"version\":6}"), "{text}");
        assert!(!text.contains("schluessel") && !text.contains("\"version\":7"), "{text}");
    }

    /// Asset-Version je Datei: Die Dateinamen gehen nur auf den geschützten
    /// Kanal; der freie bekommt dasselbe `projekt` nur mit dem Bit `assets`.
    #[tokio::test]
    async fn dateinamen_der_assets_nur_auf_dem_geschuetzten_kanal() {
        let p = ProjektEreignis { slug: "b".into(), version: 9, assets: Some(true), dateien: Some(vec!["bild.png".into()]) };
        let (tx, rx_frei) = broadcast::channel::<InhaltEreignis>(4);
        let rx_geschuetzt = tx.subscribe();
        tx.send(InhaltEreignis::Projekt(p)).unwrap();
        drop(tx);
        let frei = als_text(inhalt_strom(rx_frei, true)).await;
        let geschuetzt = als_text(inhalt_strom(rx_geschuetzt, false)).await;
        assert!(frei.contains("data: {\"slug\":\"b\",\"version\":9,\"assets\":true}\n"), "{frei}");
        assert!(!frei.contains("bild.png"), "{frei}");
        assert!(geschuetzt.contains("{\"slug\":\"b\",\"version\":9,\"assets\":true,\"dateien\":[\"bild.png\"]}"), "{geschuetzt}");
    }

    /// Abnahme-Auflage: die Bote-Wache mit falschem Boten-Stand über den
    /// geschützten Kanal: Bote weg gibt `pc-benutzer` mit `{"bereich":"bote"}`,
    /// nie auf dem freien Kanal.
    #[tokio::test]
    async fn bote_wache_als_pc_benutzer_auf_dem_geschuetzten_kanal() {
        use crate::bote::client::{Bote, Lage};
        let (state, aus) = crate::app::AppState::fuer_tests_mit_abschalter(&crate::app::beispiel_fuer_tests(), 8791);
        let bote = Arc::new(Bote::fuer_tests());
        bote.lage_setzen(Lage::Verbunden);
        let wache = crate::aenderungen::spawn_bote_wache(bote.clone(), state.aenderungen.clone(), state.abschaltung());
        let geschuetzt = bereich_strom(state.aenderungen.abonnieren(), false);
        let frei = bereich_strom(state.aenderungen.abonnieren(), true);
        tokio::time::sleep(crate::aenderungen::BOTE_TAKT * 2).await;
        bote.lage_setzen(Lage::Getrennt);
        let bis = || tokio::time::sleep(crate::aenderungen::BOTE_TAKT * 3 + crate::aenderungen::BUENDELN + Duration::from_millis(150));
        let (g, f) = tokio::join!(als_text(futures_util::StreamExt::take_until(geschuetzt, bis())), als_text(futures_util::StreamExt::take_until(frei, bis())));
        aus.send(true).unwrap();
        tokio::time::timeout(Duration::from_secs(2), wache).await.expect("Wache endet").unwrap();
        assert_eq!(namen(&g), ["pc-benutzer"], "{g}");
        assert!(g.contains("event: pc-benutzer\ndata: {\"bereich\":\"bote\"}"), "{g}");
        assert!(f.is_empty(), "nichts auf dem freien Kanal: {f}");
    }

    #[tokio::test]
    async fn ueberlauf_meldet_nachholen_fuer_beide_kanaele() {
        let nachholen = "event: nachholen\ndata: {\"grund\":\"ueberlauf\"}\n\n";

        // Änderungen: Kanal mit Platz für vier, zwanzig Ereignisse, bevor gelesen wird.
        let (tx, rx) = broadcast::channel::<Change>(4);
        for i in 0..20 {
            tx.send(Change { area: "projects", path: format!("projects/x/{i}.toml"), at: chrono::Local::now() }).unwrap();
        }
        drop(tx);
        let text = als_text(kanal_strom(rx, Art::Change, false)).await;
        assert!(text.starts_with(nachholen), "{text}");
        assert_eq!(text.matches("event: nachholen").count(), 1, "{text}");
        assert_eq!(text.matches("event: change").count(), 4, "{text}");
        assert!(text.contains("projects/x/19.toml") && !text.contains("projects/x/15.toml"), "{text}");

        // Vorgänge genauso.
        let (tx, rx) = broadcast::channel::<VorgangEreignis>(2);
        for nr in 1..=10 {
            tx.send(VorgangEreignis { id: format!("v{nr}"), slug: "x".into(), art: VorgangsArt::Abnahme, zustand: Zustand::Offen, nr }).unwrap();
        }
        drop(tx);
        let text = als_text(kanal_strom(rx, Art::Vorgang, false)).await;
        assert!(text.starts_with(nachholen), "{text}");
        assert_eq!(text.matches("event: vorgang").count(), 2, "{text}");
        assert!(text.contains("\"nr\":10"), "{text}");

        // Ohne Überlauf kein nachholen.
        let (tx, rx) = broadcast::channel::<VorgangEreignis>(8);
        tx.send(VorgangEreignis { id: "v".into(), slug: "x".into(), art: VorgangsArt::Abnahme, zustand: Zustand::Offen, nr: 1 }).unwrap();
        drop(tx);
        let text = als_text(kanal_strom(rx, Art::Vorgang, false)).await;
        assert!(!text.contains("nachholen"), "{text}");
    }

    /// Die Ereignisse des ContentStores gehen mit ihrem Namen hinaus:
    /// `projekt`, `daten`, `inhalt`; ein Überlauf wird zu `nachholen`.
    #[tokio::test]
    async fn inhalt_ereignisse_als_sse() {
        let (tx, rx) = broadcast::channel::<InhaltEreignis>(8);
        tx.send(InhaltEreignis::Projekt(ProjektEreignis { slug: "alpha".into(), version: 7, assets: None, dateien: None })).unwrap();
        tx.send(InhaltEreignis::Daten(DatenEreignis { name: "pc".into(), version: 8 })).unwrap();
        tx.send(InhaltEreignis::Sammel(SammelEreignis { version: 9, projekte: vec!["a".into(), "b".into()], daten: vec![] })).unwrap();
        drop(tx);
        let text = als_text(inhalt_strom(rx, false)).await;
        assert!(text.contains("event: projekt\ndata: {\"slug\":\"alpha\",\"version\":7}"), "{text}");
        assert!(text.contains("event: daten\ndata: {\"name\":\"pc\",\"version\":8}"), "{text}");
        assert!(text.contains("event: inhalt\ndata: {\"version\":9,\"projekte\":[\"a\",\"b\"],\"daten\":[]}"), "{text}");

        let (tx, rx) = broadcast::channel::<InhaltEreignis>(2);
        for v in 0..10 {
            tx.send(InhaltEreignis::Projekt(ProjektEreignis { slug: "x".into(), version: v, assets: None, dateien: None })).unwrap();
        }
        drop(tx);
        let text = als_text(inhalt_strom(rx, false)).await;
        assert!(text.starts_with("event: nachholen"), "{text}");
    }

    /// Kopfleiste, Fassung 2 (Nachbesserung): Ein abgelaufener Ausweis beendet
    /// den geschützten Strom sofort, ohne Schlusswort (der Browser verbindet
    /// neu; mit erneuertem Ausweis geht es weiter, sonst 401). Ohne Sitzung mit
    /// Passkey (Windows-Anmeldung) wacht niemand, der Strom läuft weiter.
    #[tokio::test]
    async fn sitzungswache_endet_bei_ablauf() {
        let state = crate::app::AppState::fuer_tests(&crate::app::beispiel_fuer_tests());
        let jetzt = crate::speicher::aufgaben::jetzt().timestamp();
        let alt = crate::anmeldung::PasskeySitzung {
            ausweis: crate::passkey::jwt::Ausweis::neu("cid", jetzt - 2 * crate::passkey::AUSWEIS_SEKUNDEN),
            name: "Handy".into(),
        };
        let strom = tokio_stream::iter(vec![ereignis(Art::Pc, false, &"x")]).chain(tokio_stream::pending());
        let text = als_text(mit_sitzungswache(strom, state.clone(), Some(alt), Duration::from_millis(50))).await;
        assert!(!text.contains("anmeldung"), "ohne Schlusswort: {text}");
        // Ohne Sitzung: nach 300 ms noch offen.
        let strom = tokio_stream::pending::<Option<Event>>();
        let offen = tokio::time::timeout(Duration::from_millis(300), async {
            let mut s = Box::pin(mit_sitzungswache(strom, state, None, Duration::from_millis(50)));
            futures_util::StreamExt::next(&mut s).await
        })
        .await;
        assert!(offen.is_err(), "der Strom läuft weiter");
    }

    /// Der geschützte Kanal beginnt mit `anmeldung` und dem Wort `angemeldet`.
    #[tokio::test]
    async fn geschuetzter_kanal_nennt_das_wort() {
        use axum::response::IntoResponse;
        use http_body_util::BodyExt;
        let state = crate::app::AppState::fuer_tests(&crate::app::beispiel_fuer_tests());
        let res = sse(State(state), None).await.into_response();
        let mut body = res.into_body();
        let teil = tokio::time::timeout(Duration::from_secs(3), body.frame()).await.unwrap().unwrap().unwrap();
        let text = String::from_utf8_lossy(&teil.into_data().unwrap()).to_string();
        assert!(text.starts_with("event: anmeldung\ndata: {\"stand\":\"angemeldet\"}"), "{text}");
    }

    /// Der Live-Kanal über den Router: Eine geänderte Datei im Projektordner
    /// kommt als genau ein `projekt` mit neuer Version an (Ereignis-Test 2d).
    #[tokio::test]
    async fn projekt_ereignis_ueber_den_live_kanal() {
        let basis = std::env::temp_dir().join(format!("dash-ev-2d-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&basis);
        for s in ["a", "b"] {
            std::fs::create_dir_all(basis.join(s)).unwrap();
            std::fs::write(basis.join(s).join("projekt.toml"), format!("schema = 3\ntitle = \"{s}\"\nstatus = \"aktiv\"\n")).unwrap();
        }
        let state = crate::app::AppState::fuer_tests(&crate::app::beispiel_fuer_tests()).mit_projekten(basis.clone());
        let v0 = state.inhalt.bestand().version;
        let mut rx = state.inhalt.abonnieren();
        std::fs::write(basis.join("b").join("projekt.toml"), "schema = 3\ntitle = \"b neu\"\nstatus = \"aktiv\"\n").unwrap();
        let b = state.inhalt.bestand();
        let _ = std::fs::remove_dir_all(&basis);
        assert_eq!(rx.try_recv().unwrap(), InhaltEreignis::Projekt(ProjektEreignis { slug: "b".into(), version: v0 + 1, assets: None, dateien: None }));
        assert!(rx.try_recv().is_err(), "nur ein Ereignis");
        assert_eq!(b.haupt("b").unwrap().unwrap().meta.title, "b neu");
    }

    /// Änderungen oben im Repo, an HEAD, index und den Zweigen zählen;
    /// alles andere unter `.git/` und tief in Unterordnern nicht.
    #[test]
    fn repo_stempel_sieht_nur_was_zaehlt() {
        let wurzel = std::env::temp_dir().join(format!("dash-repo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wurzel);
        std::fs::create_dir_all(wurzel.join(".git/refs/heads")).unwrap();
        std::fs::create_dir_all(wurzel.join(".git/objects")).unwrap();
        std::fs::create_dir_all(wurzel.join("node_modules/tief")).unwrap();
        std::fs::write(wurzel.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(wurzel.join(".git/index"), "a").unwrap();
        std::fs::write(wurzel.join("liesmich.txt"), "a").unwrap();
        let anfang = repo_stempel(&wurzel);
        assert!(anfang.iter().any(|(n, z, _)| n == ".git/HEAD" && z.is_some()), "{anfang:?}");
        assert!(anfang.iter().any(|(n, z, _)| n == ".git/refs/heads" && z.is_some()), "{anfang:?}");
        assert!(!anfang.iter().any(|(n, _, _)| n == ".git"));

        // Fremde Schreiber unter .git und tief unten: nichts.
        std::fs::write(wurzel.join(".git/index.lock"), "x").unwrap();
        std::fs::write(wurzel.join(".git/objects/ab"), "x").unwrap();
        std::fs::write(wurzel.join("node_modules/tief/x.js"), "x").unwrap();
        assert_eq!(repo_stempel(&wurzel), anfang);

        // Datei oben geändert (Größe), neue Datei oben, index und HEAD: je eine Änderung.
        std::fs::write(wurzel.join("liesmich.txt"), "ab").unwrap();
        let s1 = repo_stempel(&wurzel);
        assert_ne!(s1, anfang);
        std::fs::write(wurzel.join("neu.rs"), "x").unwrap();
        let s2 = repo_stempel(&wurzel);
        assert_ne!(s2, s1);
        std::fs::write(wurzel.join(".git/index"), "ab").unwrap();
        let s3 = repo_stempel(&wurzel);
        std::fs::write(wurzel.join(".git/HEAD"), "ref: refs/heads/anders\n").unwrap();
        assert_ne!(repo_stempel(&wurzel), s3);
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    /// Der tiefe Blick sieht geänderte Dateien unten im Repo, aber nicht in
    /// erzeugten Ordnern wie `node_modules` oder `target`.
    #[test]
    fn tiefer_blick_sieht_dateien_unten() {
        let wurzel = std::env::temp_dir().join(format!("dash-tief-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wurzel);
        std::fs::create_dir_all(wurzel.join(".git/refs/heads")).unwrap();
        std::fs::create_dir_all(wurzel.join("src/unten")).unwrap();
        std::fs::create_dir_all(wurzel.join("node_modules/x")).unwrap();
        std::fs::write(wurzel.join("src/unten/a.rs"), "a").unwrap();
        let tief = fingerabdruck(&wurzel, true);
        std::fs::write(wurzel.join("src/unten/a.rs"), "ab").unwrap();
        // Ob der flache Blick es auch sieht, hängt davon ab, wann NTFS die
        // Zeiten der Ordner darüber nachträgt (in Läufen mal ja, mal nein);
        // geprüft wird darum nur, dass der tiefe Blick es sicher sieht.
        let tief2 = fingerabdruck(&wurzel, true);
        assert_ne!(tief2, tief, "tief sieht es");
        std::fs::write(wurzel.join("node_modules/x/y.js"), "y").unwrap();
        assert_eq!(fingerabdruck(&wurzel, true), tief2, "node_modules zählt nicht");
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    /// Ein Worktree: `.git` ist eine Datei, HEAD und index liegen im
    /// Ordner aus `gitdir:`, die Zweige im gemeinsamen Ordner.
    #[test]
    fn repo_stempel_folgt_dem_worktree() {
        let basis = std::env::temp_dir().join(format!("dash-wt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&basis);
        let haupt = basis.join("haupt/.git");
        let wt_git = haupt.join("worktrees/wt");
        std::fs::create_dir_all(haupt.join("refs/heads")).unwrap();
        std::fs::create_dir_all(&wt_git).unwrap();
        std::fs::create_dir_all(basis.join("wt")).unwrap();
        std::fs::write(wt_git.join("HEAD"), "ref: refs/heads/x\n").unwrap();
        std::fs::write(wt_git.join("commondir"), "../..\n").unwrap();
        std::fs::write(basis.join("wt/.git"), format!("gitdir: {}\n", wt_git.display())).unwrap();
        let s = repo_stempel(&basis.join("wt"));
        assert!(s.iter().any(|(n, z, _)| n == ".git/HEAD" && z.is_some()), "{s:?}");
        assert!(s.iter().any(|(n, z, _)| n == ".git/refs/heads" && z.is_some()), "{s:?}");
        let (g, gem) = git_ordner(&basis.join("wt")).expect("Worktree aufgelöst");
        assert_eq!(g, wt_git);
        assert!(gem.join("refs/heads").is_dir(), "{gem:?}");
        let _ = std::fs::remove_dir_all(&basis);
    }

    /// `gitdir:` oder `commondir` auf UNC (`\\server\…`, `//server/…`) oder
    /// `\\?\…`/`\\.\…`: abgelehnt, ohne dort nachzusehen; `repo_stempel`
    /// zählt dann nur die oberste Ebene und gerät nicht in Panik.
    #[test]
    fn git_ordner_lehnt_fremde_pfade_ab() {
        let basis = std::env::temp_dir().join(format!("dash-unc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&basis);
        std::fs::create_dir_all(&basis).unwrap();
        for ziel in [r"\\server\freigabe\repo\.git", "//server/freigabe/repo/.git", r"\\?\C:\repo\.git", r"\\?\UNC\server\freigabe\.git", r"\\.\C:\repo\.git"] {
            std::fs::write(basis.join(".git"), format!("gitdir: {ziel}\n")).unwrap();
            let e = git_ordner(&basis).expect_err(ziel);
            assert!(e.contains("keinen lokalen Pfad"), "{e}");
            let s = repo_stempel(&basis);
            assert!(s.iter().any(|(n, z, _)| n == ".git/HEAD" && z.is_none()), "{s:?}");
        }
        // commondir auf UNC, gitdir lokal.
        let wt_git = basis.join("haupt/worktrees/wt");
        std::fs::create_dir_all(&wt_git).unwrap();
        std::fs::write(wt_git.join("commondir"), "\\\\server\\freigabe\\.git\n").unwrap();
        std::fs::write(basis.join(".git"), format!("gitdir: {}\n", wt_git.display())).unwrap();
        assert!(git_ordner(&basis).expect_err("commondir UNC").contains("commondir"));
        let _ = std::fs::remove_dir_all(&basis);
    }

    /// Eine zu große `.git`-Datei oder `commondir`: abgelehnt.
    #[test]
    fn git_ordner_lehnt_zu_grosse_dateien_ab() {
        let basis = std::env::temp_dir().join(format!("dash-gross-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&basis);
        let wt_git = basis.join("haupt/worktrees/wt");
        std::fs::create_dir_all(&wt_git).unwrap();
        let gross = format!("gitdir: {}\n{}", wt_git.display(), "x".repeat(GIT_DATEI_HOECHSTENS as usize));
        std::fs::write(basis.join(".git"), gross).unwrap();
        assert!(git_ordner(&basis).expect_err("zu groß").contains("größer als"));
        std::fs::write(basis.join(".git"), format!("gitdir: {}\n", wt_git.display())).unwrap();
        std::fs::write(wt_git.join("commondir"), "../..\n".repeat(1000)).unwrap();
        assert!(git_ordner(&basis).expect_err("commondir zu groß").contains("größer als"));
        std::fs::write(wt_git.join("commondir"), "../..\n").unwrap();
        assert!(git_ordner(&basis).is_ok());
        let _ = std::fs::remove_dir_all(&basis);
    }

    #[test]
    fn lokal_nur_mit_laufwerk_oder_relativ() {
        for p in ["C:/x/.git", r"C:\x", "../..", "relativ/.git"] {
            assert!(lokal(Path::new(p)), "{p}");
        }
        for p in [r"\\server\x", "//server/x", r"\\?\C:\x", r"\\.\pipe\x", r"/\server\x"] {
            assert!(!lokal(Path::new(p)), "{p}");
        }
    }

    /// Live-Lücken, Punkt 1: Eine Schreibung in den Datenspeicher kommt als
    /// `speicher` mit Bereich auf den geschützten Kanal, gebündelt (zwei
    /// Aufgaben kurz nacheinander: ein Ereignis), eine erledigte Aufgabe
    /// meldet Aufgaben und Vorgänge.
    #[tokio::test]
    async fn speicher_ereignis_mit_bereich() {
        use crate::speicher::aufgaben::{NeueAufgabe, jetzt};
        let state = crate::app::AppState::fuer_tests(&crate::app::beispiel_fuer_tests());
        let strom = bereich_strom(state.aenderungen.abonnieren(), false);
        let speicher = state.speicher.clone().unwrap();
        let neu = |titel: &str| NeueAufgabe {
            projekt: "alpha".into(),
            von: "test".into(),
            titel: titel.into(),
            link: None,
            knopf: None,
            frist: None,
            ablauf: None,
            prioritaet: None,
            frist_bezug: None,
        };
        let (eins, zwei) = (neu("eins"), neu("zwei"));
        let s = speicher.clone();
        let a = tokio::task::spawn_blocking(move || {
            let a = s.aufgabe_anlegen(eins, &jetzt()).unwrap();
            s.aufgabe_anlegen(zwei, &jetzt()).unwrap();
            a
        })
        .await
        .unwrap();
        let bis = || tokio::time::sleep(Duration::from_millis(600));
        let text = als_text(futures_util::StreamExt::take_until(strom, bis())).await;
        assert_eq!(text.matches("event: speicher\ndata: {\"bereich\":\"aufgaben\"}").count(), 1, "{text}");

        let strom = bereich_strom(state.aenderungen.abonnieren(), false);
        let s = speicher.clone();
        tokio::task::spawn_blocking(move || s.aufgabe_erledigt(&a.id, None, &jetzt()).unwrap()).await.unwrap();
        let text = als_text(futures_util::StreamExt::take_until(strom, bis())).await;
        assert!(text.contains("{\"bereich\":\"aufgaben\"}") && text.contains("{\"bereich\":\"vorgaenge\"}"), "{text}");
        assert!(!text.contains("pc-benutzer") && !text.contains("event: docs"), "{text}");
    }

    /// Punkt 3 und 5: Eine neue Entscheidung in docs/entscheidungen gibt
    /// `docs` mit Bereich; ein geändertes Bild in assets/ ein `projekt` mit
    /// `assets: true` für genau dieses Projekt.
    #[tokio::test]
    async fn weitere_wachen_docs_und_assets() {
        let basis = std::env::temp_dir().join(format!("dash-ww-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&basis);
        let projekte = basis.join("projects");
        for s in ["a", "b"] {
            std::fs::create_dir_all(projekte.join(s).join("assets")).unwrap();
            std::fs::write(projekte.join(s).join("projekt.toml"), format!("schema = 3\ntitle = \"{s}\"\nstatus = \"aktiv\"\n")).unwrap();
            std::fs::write(projekte.join(s).join("assets/bild.png"), "1").unwrap();
        }
        std::fs::create_dir_all(basis.join("docs/entscheidungen")).unwrap();
        let state = crate::app::AppState::fuer_tests(&crate::app::beispiel_fuer_tests()).mit_projekten(projekte.clone());
        let _ = state.inhalt.bestand();
        let weitere = WeitereWachen {
            melder: state.aenderungen.clone(),
            entscheidungen: basis.join("docs/entscheidungen"),
            zielbild: basis.join("docs/zielbild"),
            quellen: vec![projekte.clone()],
        };
        let wachen = weitere.anfangen().await;
        let wachen = weitere.pruefen(wachen, &state.inhalt).await;
        let mut rx = state.inhalt.abonnieren();
        let strom = bereich_strom(state.aenderungen.abonnieren(), false);
        std::fs::write(basis.join("docs/entscheidungen/2026-11-01-neu.toml"), "thema = \"neu\"\n").unwrap();
        std::fs::write(projekte.join("b/assets/bild.png"), "22").unwrap();
        let _ = weitere.pruefen(wachen, &state.inhalt).await;
        let text = als_text(futures_util::StreamExt::take_until(strom, tokio::time::sleep(Duration::from_millis(600)))).await;
        let _ = std::fs::remove_dir_all(&basis);
        assert_eq!(namen(&text), ["docs"], "{text}");
        assert!(text.contains("{\"bereich\":\"entscheidungen\"}"), "{text}");
        let mut assets = Vec::new();
        while let Ok(e) = rx.try_recv() {
            if let InhaltEreignis::Projekt(p) = e
                && p.assets == Some(true)
            {
                assets.push(p);
            }
        }
        assert_eq!(assets.len(), 1, "{assets:?}");
        let p = &assets[0];
        assert_eq!(p.slug, "b");
        assert_eq!(p.dateien.as_deref(), Some(&["bild.png".to_string()][..]), "Asset-Version je Datei");
        let frei = InhaltEreignis::Projekt(p.clone()).nur_frei().unwrap();
        let InhaltEreignis::Projekt(frei) = frei else { panic!("projekt bleibt projekt") };
        let sse = als_text(tokio_stream::iter(vec![ereignis(Art::Projekt, true, &frei)])).await;
        assert!(sse.contains(&format!("{{\"slug\":\"b\",\"version\":{},\"assets\":true}}", p.version)), "{sse}");
    }

    /// Etappe 4b: Ein Eintrag der Liste, der selbst ein Projekt ist (zweite
    /// Quelle); ein geändertes Bild in seinem assets/ gibt `projekt` mit `assets: true`.
    #[tokio::test]
    async fn asset_wache_sieht_einzelnes_projekt_der_zweiten_quelle() {
        let basis = std::env::temp_dir().join(format!("dash-ww-quelle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&basis);
        for p in [basis.join("projects/a"), basis.join("fremd/projects/solo")] {
            std::fs::create_dir_all(p.join("assets")).unwrap();
            std::fs::write(p.join("projekt.toml"), "schema = 3\ntitle = \"T\"\nstatus = \"aktiv\"\n").unwrap();
            std::fs::write(p.join("assets/bild.png"), "1").unwrap();
        }
        let quellen = vec![basis.join("projects"), basis.join("fremd/projects/solo")];
        let state = crate::app::AppState::fuer_tests(&crate::app::beispiel_fuer_tests()).mit_quellen(quellen.clone());
        assert!(state.inhalt.bestand().projekte.contains_key("solo"));
        let weitere = WeitereWachen {
            melder: state.aenderungen.clone(),
            entscheidungen: basis.join("docs/entscheidungen"),
            zielbild: basis.join("docs/zielbild"),
            quellen,
        };
        let wachen = weitere.anfangen().await;
        let wachen = weitere.pruefen(wachen, &state.inhalt).await;
        let mut rx = state.inhalt.abonnieren();
        std::fs::write(basis.join("fremd/projects/solo/assets/bild.png"), "22").unwrap();
        let _ = weitere.pruefen(wachen, &state.inhalt).await;
        let _ = std::fs::remove_dir_all(&basis);
        let mut slugs = Vec::new();
        while let Ok(e) = rx.try_recv() {
            if let InhaltEreignis::Projekt(p) = e
                && p.assets == Some(true)
            {
                slugs.push(p.slug);
            }
        }
        assert_eq!(slugs, ["solo"]);
    }

    /// Punkt 6: Ein Commit auf einem Zweig mit Schrägstrich
    /// (`refs/heads/bauplan/x`) ändert nur eine Datei im Unterordner, nicht
    /// die Zeit von `refs/heads`; ein gepackter Zweig nur `packed-refs`.
    /// Beides muss den Fingerabdruck ändern.
    #[test]
    fn repo_stempel_sieht_zweige_mit_schraegstrich_und_packed_refs() {
        let wurzel = std::env::temp_dir().join(format!("dash-zweig-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wurzel);
        std::fs::create_dir_all(wurzel.join(".git/refs/heads/bauplan")).unwrap();
        std::fs::write(wurzel.join(".git/HEAD"), "ref: refs/heads/bauplan/x\n").unwrap();
        std::fs::write(wurzel.join(".git/refs/heads/bauplan/x"), "a".repeat(40) + "\n").unwrap();
        let anfang = repo_stempel(&wurzel);
        assert!(anfang.iter().any(|(n, z, _)| n == ".git/refs/heads/bauplan/x" && z.is_some()), "{anfang:?}");
        let heads_zeit = std::fs::metadata(wurzel.join(".git/refs/heads")).unwrap().modified().unwrap();
        // Neuer Commit: Git schreibt die Ref neu (andere Zeit; hier auch andere
        // Länge, damit der Test nicht an der Auflösung der Zeitstempel hängt).
        std::fs::write(wurzel.join(".git/refs/heads/bauplan/x"), "b".repeat(40) + "\n\n").unwrap();
        assert_eq!(std::fs::metadata(wurzel.join(".git/refs/heads")).unwrap().modified().unwrap(), heads_zeit, "refs/heads selbst unverändert");
        let s1 = repo_stempel(&wurzel);
        assert_ne!(s1, anfang, "Commit auf bauplan/x");
        // git pack-refs: die Datei verschwindet, packed-refs entsteht.
        std::fs::remove_file(wurzel.join(".git/refs/heads/bauplan/x")).unwrap();
        std::fs::write(wurzel.join(".git/packed-refs"), format!("{} refs/heads/bauplan/x\n", "b".repeat(40))).unwrap();
        let s2 = repo_stempel(&wurzel);
        assert_ne!(s2, s1);
        std::fs::write(wurzel.join(".git/packed-refs"), format!("{} refs/heads/bauplan/x\n\n", "c".repeat(40))).unwrap();
        assert_ne!(repo_stempel(&wurzel), s2, "Commit in packed-refs");
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    #[test]
    fn scan_finds_pages_and_toml() {
        let base = std::env::temp_dir().join(format!("dash-ev-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("projects/x")).unwrap();
        std::fs::create_dir_all(base.join("data")).unwrap();
        std::fs::write(base.join("projects/x/notiz.md"), "a").unwrap();
        std::fs::write(base.join("projects/x/ALT.MD"), "a").unwrap();
        std::fs::write(base.join("projects/x/notiz.txt"), "a").unwrap();
        std::fs::write(base.join("data/pc.toml"), "a").unwrap();
        let mut m = std::collections::BTreeMap::new();
        scan(&base.join("projects"), "projects/", &mut m);
        scan(&base.join("data"), "data/", &mut m);
        assert!(m.contains_key("projects/x/notiz.md"));
        assert!(m.contains_key("projects/x/ALT.MD"), "Groß geschrieben wie in der Prüfung");
        assert!(m.contains_key("data/pc.toml"));
        assert!(!m.contains_key("projects/x/notiz.txt"));
        let _ = std::fs::remove_dir_all(&base);
    }
}
