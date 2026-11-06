//! Wo liegen die Dateien? Größen der Ordner direkt unter dem Benutzerordner.
//! Das Zählen dauert Minuten, darum läuft es im Hintergrund und das Ergebnis
//! liegt als JSON im Cache-Ordner, damit ein Neustart nicht neu zählen muss.
//!
//! Welche Ordner nie betreten werden, steht nicht im Code, sondern in einer
//! Ausschlussliste im Inhalt (`inventar-ausschluss.toml` im Daten-Ordner des
//! Inhalts, Umzug des Inhalts; Auflage der Archiv-Rolle):
//!
//! ```toml
//! # Ordnernamen, die das Inventar nie betritt (Teil des Namens, ohne Groß/Klein).
//! nie_betreten = ["…", "…"]
//! ```
//!
//! **Geschlossen scheitern:** Fehlt die Liste, ist sie leer oder lässt sie
//! sich nicht lesen, wird der Benutzerordner gar nicht durchlaufen. Die
//! PC-Seite zeigt dann nur Laufwerke und ihre Größen und einen Satz
//! ([`Inventory::hinweis`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{info, warn};
use ts_rs::TS;

pub type SharedInventory = Arc<RwLock<Inventory>>;

/// Nach dieser Zeit wird neu gezählt.
pub const MAX_AGE: Duration = Duration::from_secs(6 * 60 * 60);

/// Höchstens so groß darf die Ausschlussliste sein (sie hat wenige Zeilen).
const LISTE_HOECHSTENS: u64 = 64 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(rename = "InventoryEntry")]
pub struct Entry {
    pub name: String,
    pub bytes: u64,
    pub files: u64,
    pub is_dir: bool,
    /// Nicht betreten (privat); Größe unbekannt.
    pub skipped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, TS)]
pub struct Inventory {
    pub root: String,
    #[ts(type = "string | null")]
    pub computed_at: Option<DateTime<Local>>,
    pub duration_secs: u64,
    pub entries: Vec<Entry>,
    /// Die Fächer der Chats unter `Projekte\`, je eines pro Chat.
    #[serde(default)]
    pub face: Vec<Entry>,
    /// Ein Satz, wenn nicht gezählt wurde, weil die Ausschlussliste fehlt,
    /// leer oder kaputt ist (geschlossen scheitern). Dann sind `entries` und
    /// `face` leer.
    #[serde(default)]
    pub hinweis: Option<String>,
    #[serde(skip)]
    pub computing: bool,
    #[serde(skip)]
    pub error: Option<String>,
}

/// Die geladene Ausschlussliste: Namensteile in Kleinbuchstaben, nie leer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ausschluss(Vec<String>);

impl Ausschluss {
    /// Aus Namensteilen; leere Einträge zählen nicht. `None`, wenn nichts übrig bleibt.
    pub fn aus(namen: &[&str]) -> Option<Ausschluss> {
        let v: Vec<String> = namen.iter().map(|n| n.trim().to_lowercase()).filter(|n| !n.is_empty()).collect();
        (!v.is_empty()).then_some(Ausschluss(v))
    }

    /// Darf ein Ordner dieses Namens nie betreten werden? Teil des Namens, ohne Groß/Klein.
    pub fn verboten(&self, name: &str) -> bool {
        let lower = name.to_lowercase();
        self.0.iter().any(|f| lower.contains(f.as_str()))
    }

    /// Wie [`Ausschluss::verboten`], nur strenger (fürs Öffnen, Entscheidung
    /// lokal-oeffnen): Leer-, Binde-, Unterstriche, Punkte und
    /// andere Zeichen außer Buchstaben und Ziffern zählen nicht, damit eine
    /// andere Schreibweise („Name_Teil“, „NAME-TEIL“) nicht vorbeikommt.
    pub fn verboten_lose(&self, name: &str) -> bool {
        let dicht = |s: &str| s.to_lowercase().chars().filter(|c| c.is_alphanumeric()).collect::<String>();
        let n = dicht(name);
        self.verboten(name) || self.0.iter().map(|f| dicht(f)).any(|f| !f.is_empty() && n.contains(&f))
    }
}

/// Der Satz, wenn die Liste fehlt, leer oder unlesbar ist.
pub fn satz_ohne_liste(grund: &str) -> String {
    format!(
        "Die Ordnergrößen werden nicht gezählt: Die Ausschlussliste der privaten Ordner {grund}. Zu sehen sind nur die Laufwerke und ihre Größen. Die Liste gehört als inventar-ausschluss.toml in den Daten-Ordner des Inhalts (/anleitung, „Inventar: Ausschlussliste im Inhalt“). {SATZ_ABSICHT}"
    )
}

/// Für den Inhaber: warum nichts gezählt wird (Absicht, kein Absturz).
pub const SATZ_ABSICHT: &str = "Ohne Liste zählt das Dashboard mit Absicht keine Ordner, damit kein privater Ordner auf die Seite kommt.";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RohListe {
    nie_betreten: Vec<String>,
}

/// Lädt die Ausschlussliste. `Err` mit dem Satz für die PC-Seite, wenn sie
/// fehlt (auch `None`), leer, zu groß oder kaputt ist; dann wird nicht gezählt.
pub fn ausschluss_laden(datei: Option<&Path>) -> Result<Ausschluss, String> {
    let Some(datei) = datei else { return Err(satz_ohne_liste("ist nicht eingestellt")) };
    let meta = match std::fs::metadata(datei) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(satz_ohne_liste("fehlt")),
        Err(_) => return Err(satz_ohne_liste("lässt sich nicht lesen")),
    };
    if !meta.is_file() || meta.len() > LISTE_HOECHSTENS {
        return Err(satz_ohne_liste("lässt sich nicht lesen"));
    }
    let Ok(text) = std::fs::read_to_string(datei) else { return Err(satz_ohne_liste("lässt sich nicht lesen")) };
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let roh: RohListe = toml::from_str(text).map_err(|_| satz_ohne_liste("ist kaputt (kein gültiges TOML mit nie_betreten = [ … ])"))?;
    let namen: Vec<&str> = roh.nie_betreten.iter().map(String::as_str).collect();
    Ausschluss::aus(&namen).ok_or_else(|| satz_ohne_liste("ist leer"))
}

/// Benutzerordner, z. B. `C:\Users\<name>`.
pub fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

/// Cache-Datei unter `~/.cache/dashboard/`.
pub fn cache_file() -> PathBuf {
    home_dir().join(".cache").join("dashboard").join("folders.json")
}

/// Summiert Dateigrößen unterhalb eines Ordners. Folgt keinen Verknüpfungen
/// (Junctions, Symlinks) und betritt verbotene Ordner nicht.
pub fn dir_size(dir: &Path, aus: &Ausschluss) -> (u64, u64) {
    let mut bytes = 0u64;
    let mut files = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for entry in rd.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            let ft = meta.file_type();
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if aus.verboten(&name) {
                    continue;
                }
                stack.push(entry.path());
            } else if ft.is_file() {
                bytes += meta.len();
                files += 1;
            }
        }
    }
    (bytes, files)
}

/// Zählt alle Einträge direkt unter `root`, mit der Liste; ohne gültige Liste
/// wird nichts durchlaufen (nur der Satz).
pub fn compute_mit(root: &Path, liste: &Result<Ausschluss, String>) -> Inventory {
    let start = std::time::Instant::now();
    let aus = match liste {
        Ok(a) => a,
        Err(satz) => {
            return Inventory {
                root: root.display().to_string(),
                computed_at: Some(Local::now()),
                duration_secs: 0,
                hinweis: Some(satz.clone()),
                ..Default::default()
            };
        }
    };
    let mut entries = Vec::new();
    if let Ok(rd) = std::fs::read_dir(root) {
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.file_type().is_symlink() {
                continue;
            }
            if meta.is_dir() {
                if aus.verboten(&name) {
                    entries.push(Entry { name, bytes: 0, files: 0, is_dir: true, skipped: true });
                    continue;
                }
                let (bytes, files) = dir_size(&entry.path(), aus);
                entries.push(Entry { name, bytes, files, is_dir: true, skipped: false });
            } else {
                entries.push(Entry { name, bytes: meta.len(), files: 1, is_dir: false, skipped: false });
            }
        }
    }
    entries.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.name.cmp(&b.name)));
    let face = face_entries(&root.join("Projekte"), aus);
    Inventory {
        root: root.display().to_string(),
        computed_at: Some(Local::now()),
        duration_secs: start.elapsed().as_secs(),
        entries,
        face,
        hinweis: None,
        computing: false,
        error: None,
    }
}

/// Unterordner von `Projekte\` mit Größe, alphabetisch.
fn face_entries(face: &Path, aus: &Ausschluss) -> Vec<Entry> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(face) else { return out };
    for entry in rd.flatten() {
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_dir() || meta.file_type().is_symlink() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if aus.verboten(&name) {
            continue;
        }
        let (bytes, files) = dir_size(&entry.path(), aus);
        out.push(Entry { name, bytes, files, is_dir: true, skipped: false });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn load_cache(path: &Path) -> Option<Inventory> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save_cache(path: &Path, inv: &Inventory) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_string_pretty(inv) {
        Ok(json) => {
            if let Err(e) = std::fs::write(path, json) {
                warn!("Ordner-Cache nicht geschrieben: {e}");
            }
        }
        Err(e) => warn!("Ordner-Cache nicht serialisiert: {e}"),
    }
}

/// Frisch, wenn gezählt und jünger als [`MAX_AGE`]. Ein Stand ohne Liste
/// (mit `hinweis`) ist nie frisch: Beim nächsten Takt wird wieder nach der
/// Liste gesehen.
pub fn is_fresh(inv: &Inventory) -> bool {
    inv.hinweis.is_none() && inv.computed_at.map(|t| (Local::now() - t).to_std().unwrap_or_default() < MAX_AGE).unwrap_or(false)
}

/// Hintergrundaufgabe: Cache laden, bei Bedarf zählen, alle paar Stunden
/// wiederholen. `ausschluss` ist die Ausschlussliste; fehlt sie oder taugt
/// sie nicht, wird weder der Cache geladen noch gezählt (nur der Satz).
pub fn spawn(shared: SharedInventory, root: PathBuf, cache: PathBuf, ausschluss: PathBuf, melder: std::sync::Arc<crate::aenderungen::Melder>) {
    tokio::spawn(async move {
        let liste_da = ausschluss_laden(Some(&ausschluss)).is_ok();
        if let Some(cached) = load_cache(&cache).filter(|_| liste_da)
            && cached.root == root.display().to_string()
        {
            info!("Ordnergrößen aus Cache ({})", cache.display());
            *shared.write().await = cached;
        }
        loop {
            let fresh = is_fresh(&*shared.read().await);
            if !fresh {
                shared.write().await.computing = true;
                let r = root.clone();
                let a = ausschluss.clone();
                let result = tokio::task::spawn_blocking(move || {
                    let liste = ausschluss_laden(Some(&a));
                    if let Err(satz) = &liste {
                        warn!("Ordnergrößen: {satz}");
                    } else {
                        info!("Zähle Ordnergrößen unter {} …", r.display());
                    }
                    compute_mit(&r, &liste)
                })
                .await;
                let mut guard = shared.write().await;
                match result {
                    Ok(inv) => {
                        if inv.hinweis.is_none() {
                            info!("Ordnergrößen fertig in {} s", inv.duration_secs);
                            save_cache(&cache, &inv);
                        }
                        *guard = inv;
                        // Live-Lücken: neue Ordnergrößen als `pc-benutzer` mit Bereich `inventar`.
                        melder.melden(crate::aenderungen::Bereich::Inventar);
                    }
                    Err(e) => {
                        guard.computing = false;
                        guard.error = Some(format!("Zählen abgebrochen: {e}"));
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(15 * 60)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ein eigener Temp-Ordner je Test.
    struct Ort(PathBuf);
    impl Drop for Ort {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn ort(name: &str) -> Ort {
        let p = std::env::temp_dir().join(format!("dash-inv-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        Ort(p)
    }

    /// Neutrale Testnamen; die echten stehen nur im Inhalt.
    fn liste() -> Ausschluss {
        Ausschluss::aus(&["geheimer ordner", "nur für mich"]).unwrap()
    }

    #[test]
    fn verbotene_namen_ohne_gross_klein() {
        let a = liste();
        assert!(a.verboten("Geheimer Ordner"));
        assert!(a.verboten("NUR FÜR MICH (alt)"));
        assert!(!a.verboten("Dokumente"));
        assert!(Ausschluss::aus(&["", "  "]).is_none());
    }

    /// Ein Benutzerordner mit einem gesperrten Ordner und dem Fach `Projekte`.
    fn baum(base: &Path) {
        std::fs::create_dir_all(base.join("a/sub")).unwrap();
        std::fs::create_dir_all(base.join("Geheimer Ordner")).unwrap();
        std::fs::write(base.join("a/x.bin"), vec![0u8; 1000]).unwrap();
        std::fs::write(base.join("a/sub/y.bin"), vec![0u8; 500]).unwrap();
        std::fs::write(base.join("Geheimer Ordner/secret.txt"), b"nein").unwrap();
        std::fs::write(base.join("loose.txt"), b"12345").unwrap();
    }

    #[test]
    fn counts_and_skips() {
        let o = ort("zaehlen");
        let base = &o.0;
        baum(base);
        let inv = compute_mit(base, &Ok(liste()));
        assert!(inv.hinweis.is_none());
        let a = inv.entries.iter().find(|e| e.name == "a").unwrap();
        assert_eq!((a.bytes, a.files), (1500, 2));
        let p = inv.entries.iter().find(|e| e.name == "Geheimer Ordner").unwrap();
        assert!(p.skipped);
        assert_eq!(p.bytes, 0);
        let l = inv.entries.iter().find(|e| e.name == "loose.txt").unwrap();
        assert!(!l.is_dir);
        assert_eq!(inv.entries[0].name, "a", "größter Eintrag zuerst");
        std::fs::create_dir_all(base.join("Projekte/chat1")).unwrap();
        std::fs::write(base.join("Projekte/chat1/n.md"), b"abc").unwrap();
        let inv = compute_mit(base, &Ok(liste()));
        assert_eq!(inv.face.len(), 1);
        assert_eq!(inv.face[0].bytes, 3);
    }

    /// Geschlossen scheitern: keine Liste, eine leere oder eine kaputte, dann
    /// wird nichts durchlaufen und ein Satz steht da. Mit guter Liste gezählt.
    #[test]
    fn ohne_gute_liste_wird_nicht_gezaehlt() {
        let o = ort("liste");
        let base = o.0.join("profil");
        baum(&base);
        let datei = o.0.join("inventar-ausschluss.toml");
        let pruefen = |liste: Result<Ausschluss, String>, wort: &str| {
            let inv = compute_mit(&base, &liste);
            assert!(inv.entries.is_empty() && inv.face.is_empty(), "{wort}: nichts gezählt: {inv:?}");
            let satz = inv.hinweis.clone().unwrap();
            assert!(satz.contains(wort) && satz.contains("nur die Laufwerke"), "{satz}");
            // Für den Inhaber: kein Fehler zum Suchen, die Erklärung bleibt.
            assert!(satz.ends_with("(/anleitung, „Inventar: Ausschlussliste im Inhalt“). Ohne Liste zählt das Dashboard mit Absicht keine Ordner, damit kein privater Ordner auf die Seite kommt."), "{satz}");
            assert!(!is_fresh(&inv), "ohne Liste nie frisch");
        };
        // fehlt
        pruefen(ausschluss_laden(Some(&datei)), "fehlt");
        pruefen(ausschluss_laden(None), "nicht eingestellt");
        // leer: leere Datei, leere Liste, nur Leerzeichen
        std::fs::write(&datei, "").unwrap();
        pruefen(ausschluss_laden(Some(&datei)), "kaputt");
        std::fs::write(&datei, "nie_betreten = []\n").unwrap();
        pruefen(ausschluss_laden(Some(&datei)), "leer");
        std::fs::write(&datei, "nie_betreten = [\"  \"]\n").unwrap();
        pruefen(ausschluss_laden(Some(&datei)), "leer");
        // kaputt: kein TOML, falsches Feld, falscher Typ, ein Ordner statt Datei
        std::fs::write(&datei, "nie_betreten = [\"offen\"\n").unwrap();
        pruefen(ausschluss_laden(Some(&datei)), "kaputt");
        std::fs::write(&datei, "anderes = [\"x\"]\n").unwrap();
        pruefen(ausschluss_laden(Some(&datei)), "kaputt");
        std::fs::write(&datei, "nie_betreten = \"x\"\n").unwrap();
        pruefen(ausschluss_laden(Some(&datei)), "kaputt");
        std::fs::write(&datei, [0xff, 0xfe, 0x00]).unwrap();
        pruefen(ausschluss_laden(Some(&datei)), "nicht lesen");
        let ordner = o.0.join("als-ordner.toml");
        std::fs::create_dir_all(&ordner).unwrap();
        pruefen(ausschluss_laden(Some(&ordner)), "nicht lesen");
        // gut
        std::fs::write(&datei, "\u{feff}# Kommentar\nnie_betreten = [\"Geheimer Ordner\", \"nur für mich\"]\n").unwrap();
        let liste = ausschluss_laden(Some(&datei));
        assert_eq!(liste, Ok(liste_gut()));
        let inv = compute_mit(&base, &liste);
        assert!(inv.hinweis.is_none() && inv.entries.iter().any(|e| e.name == "Geheimer Ordner" && e.skipped));
    }

    fn liste_gut() -> Ausschluss {
        liste()
    }
}
