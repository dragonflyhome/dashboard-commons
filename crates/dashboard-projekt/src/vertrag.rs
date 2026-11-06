//! Vertragstypen eines Moduls als TypeScript (ts-rs), wie `src/vertrag/export.rs`
//! im Dashboard: je Typ eine Datei in `web/src/lib/vertrag/projekte/<slug>/`
//! und dazu `index.ts`. Die Seite importiert sie über `$vertrag/projekte/<slug>`.
//!
//! `vertrag_pruefen` vergleicht, was auf der Platte liegt, mit dem, was Rust
//! jetzt erzeugt, schreibt den neuen Stand und meldet, ob etwas abwich
//! (überzählige Dateien nur melden, nie löschen). Schreiben gibt es nur mit
//! dem Feature `pruefstand` (Tests). Der
//! zweite Lauf von `cargo test` ist dann grün; einchecken muss man selbst.

use std::any::TypeId;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use ts_rs::{Config, TS, TypeVisitor};

use crate::Modul;

const KOPF_INDEX: &str = "// Erzeugt von cargo test (dashboard-projekt, vertrag.rs). Nicht von Hand ändern.\n";

/// Sammelt die Vertragstypen eines Moduls samt Abhängigkeiten.
pub struct Vertrag {
    cfg: Config,
    gesehen: HashSet<TypeId>,
    /// Dateiname → (TS-Name, Inhalt).
    dateien: BTreeMap<String, (String, String)>,
    fehler: Vec<String>,
}

impl TypeVisitor for Vertrag {
    fn visit<T: TS + 'static + ?Sized>(&mut self) {
        self.aufnehmen::<T>();
    }
}

impl Vertrag {
    fn neu(ordner: PathBuf) -> Self {
        Self { cfg: Config::new().with_out_dir(ordner).with_large_int("number"), gesehen: HashSet::new(), dateien: BTreeMap::new(), fehler: Vec::new() }
    }

    /// Nimmt einen Antworttyp auf; was er enthält, kommt von selbst mit.
    pub fn typ<T: TS + 'static + ?Sized>(&mut self) -> &mut Self {
        self.aufnehmen::<T>();
        self
    }

    fn aufnehmen<T: TS + 'static + ?Sized>(&mut self) {
        let Some(pfad) = T::output_path() else { return };
        if !self.gesehen.insert(TypeId::of::<T>()) {
            return;
        }
        let datei = pfad.to_string_lossy().to_string();
        // Nur ein schlichter Dateiname: sonst führte `#[ts(export_to = …)]` das
        // Schreiben in fremde Ordner oder auf beliebige Dateien.
        if !ist_dateiname(&datei) {
            self.fehler
                .push(format!("{datei}: Typen eines Moduls brauchen einen schlichten Dateinamen (Name.ts, ohne Ordner, .. oder :); kein #[ts(export_to)]."));
            return;
        }
        let name = T::ident(&self.cfg);
        match T::export_to_string(&self.cfg) {
            Ok(text) => match self.dateien.get(&datei) {
                Some((anderer, _)) if *anderer != name => {
                    self.fehler.push(format!("{datei}: zwei Typen heißen in TypeScript gleich ({anderer}, {name})"));
                }
                Some(_) => {}
                None => {
                    self.dateien.insert(datei, (name, text));
                }
            },
            Err(e) => self.fehler.push(format!("{datei}: {e}")),
        }
        T::visit_dependencies(self);
    }

    /// Alle Dateien samt `index.ts`; leer, wenn das Modul keine Typen nennt.
    fn dateien(self) -> Result<BTreeMap<String, String>, Vec<String>> {
        if !self.fehler.is_empty() {
            return Err(self.fehler);
        }
        let mut out = BTreeMap::new();
        if self.dateien.is_empty() {
            return Ok(out);
        }
        let mut index = String::from(KOPF_INDEX);
        let mut namen: Vec<(&String, &String)> = self.dateien.iter().map(|(d, (n, _))| (n, d)).collect();
        namen.sort();
        for (name, datei) in namen {
            index.push_str(&format!("export type {{ {name} }} from './{}';\n", datei.trim_end_matches(".ts")));
        }
        for (datei, (_, text)) in self.dateien {
            out.insert(datei, text);
        }
        out.insert("index.ts".to_string(), index);
        Ok(out)
    }
}

/// Ein schlichter Dateiname für eine Typdatei: `Name.ts`, ohne Ordner, `..`
/// oder Laufwerk.
pub fn ist_dateiname(name: &str) -> bool {
    name.len() > 3
        && name.len() <= 100
        && name.ends_with(".ts")
        && !name.starts_with('.')
        && !name.contains("..")
        && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'.')
}

/// `web/src/lib/vertrag/projekte` im Repo (neben `crates/`).
fn projekte_ordner() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("web").join("src").join("lib").join("vertrag").join("projekte")
}

/// Ist das ein gültiger Slug (a–z, 0–9, Bindestrich, nicht am Rand)?
pub fn ist_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 64
        && !slug.starts_with('-')
        && !slug.ends_with('-')
        && slug.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

/// Die Dateien, die das Modul jetzt erzeugt (ohne zu schreiben).
pub fn erzeugen(modul: &dyn Modul) -> Result<BTreeMap<String, String>, Vec<String>> {
    if !ist_slug(modul.slug()) {
        return Err(vec![format!("Der Slug „{}“ ist ungültig (a–z, 0–9, Bindestrich).", modul.slug())]);
    }
    let mut v = Vertrag::neu(projekte_ordner().join(modul.slug()));
    modul.vertrag(&mut v);
    v.dateien()
}

/// Vergleicht und schreibt den neuen Stand nach `<wurzel>/<slug>/` (nur
/// Dateien, die das Modul erzeugt). Abweichungen kommen als `Err` mit je einer
/// Zeile zurück; überzählige `.ts` dort werden nur gemeldet, nie gelöscht.
#[cfg(any(test, feature = "pruefstand"))]
fn pruefen_unter(modul: &dyn Modul, wurzel: &Path) -> Result<(), Vec<String>> {
    let soll = erzeugen(modul)?;
    let slug = modul.slug();
    let dir = wurzel.join(slug);
    let ort = format!("web/src/lib/vertrag/projekte/{slug}");
    let mut abweichungen = Vec::new();
    for (datei, text) in &soll {
        match std::fs::read(dir.join(datei)) {
            Ok(da) if da == text.as_bytes() => {}
            Ok(_) => abweichungen.push(format!("{ort}/{datei} ist veraltet")),
            Err(_) => abweichungen.push(format!("{ort}/{datei} fehlt")),
        }
    }
    let ueberzaehlig: Vec<String> = std::fs::read_dir(&dir)
        .map(|rd| rd.filter_map(|e| e.ok()).filter_map(|e| e.file_name().into_string().ok()).filter(|n| n.ends_with(".ts") && !soll.contains_key(n)).collect())
        .unwrap_or_default();
    for n in &ueberzaehlig {
        abweichungen.push(format!("{ort}/{n} gehört zu keinem Rust-Typ mehr; bitte von Hand entfernen"));
    }
    if !soll.is_empty() {
        std::fs::create_dir_all(&dir).map_err(|e| vec![format!("{ort} lässt sich nicht anlegen: {e}")])?;
    }
    for (datei, text) in &soll {
        let pfad = dir.join(datei);
        if std::fs::read(&pfad).ok().as_deref() != Some(text.as_bytes()) {
            std::fs::write(&pfad, text).map_err(|e| vec![format!("{ort}/{datei} lässt sich nicht schreiben: {e}")])?;
        }
    }
    if abweichungen.is_empty() { Ok(()) } else { Err(abweichungen) }
}

/// Nur für Tests (Feature `pruefstand`): prüft und schreibt die Typen fest nach
/// `web/src/lib/vertrag/projekte/<slug>/`; `Err`, wenn etwas abwich.
#[cfg(any(test, feature = "pruefstand"))]
pub fn pruefen(modul: &dyn Modul) -> Result<(), Vec<String>> {
    pruefen_unter(modul, &projekte_ordner())
}

/// Für den Test eines Moduls: wie [`pruefen`]; schlägt fehl, wenn der Vertrag
/// nicht aktuell war (dann ist er jetzt neu geschrieben).
#[cfg(any(test, feature = "pruefstand"))]
pub fn vertrag_pruefen(modul: &dyn Modul) {
    if let Err(a) = pruefen(modul) {
        panic!(
            "Der TypeScript-Vertrag des Moduls „{}“ passte nicht zu Rust und ist jetzt neu geschrieben:\n{}\n\
             cargo test noch einmal ausführen und das Ergebnis einchecken.",
            modul.slug(),
            a.iter().map(|z| format!("  {z}")).collect::<Vec<_>>().join("\n")
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Typen(&'static str);
    impl Modul for Typen {
        fn herkunft(&self) -> crate::Herkunft {
            crate::herkunft!()
        }
        fn slug(&self) -> &'static str {
            self.0
        }
        fn router(&self) -> axum::Router<crate::ProjektKontext> {
            axum::Router::new()
        }
        fn vertrag(&self, v: &mut Vertrag) {
            v.typ::<crate::GitStand>();
        }
    }

    #[test]
    fn ziel_fest_und_ueberzaehliges_bleibt() {
        for schlecht in ["", "../x", "C:/x", "a/b", "Gross", "-a", "_vorlage"] {
            assert!(erzeugen(&Typen(schlecht)).is_err(), "{schlecht}");
        }
        let wurzel = std::env::temp_dir().join(format!("dashboard-sdk-vertrag-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wurzel);
        std::fs::create_dir_all(wurzel.join("probe")).unwrap();
        std::fs::write(wurzel.join("probe").join("Alt.ts"), "alt").unwrap();
        let a = pruefen_unter(&Typen("probe"), &wurzel).unwrap_err();
        assert!(a.iter().any(|z| z.contains("Alt.ts gehört zu keinem Rust-Typ")), "{a:?}");
        assert!(wurzel.join("probe").join("Alt.ts").is_file(), "nichts gelöscht");
        let a = pruefen_unter(&Typen("probe"), &wurzel).unwrap_err();
        assert_eq!(a.len(), 1, "nur noch die überzählige Datei: {a:?}");
        let _ = std::fs::remove_dir_all(&wurzel);
    }

    /// Ein Typ mit `#[ts(export_to)]` (Ordner davor) wird abgelehnt; es wird nichts geschrieben.
    #[derive(ts_rs::TS)]
    #[ts(export_to = "../fremd/")]
    #[allow(dead_code)]
    struct Boese {
        a: u32,
    }

    struct MitExportTo;
    impl Modul for MitExportTo {
        fn herkunft(&self) -> crate::Herkunft {
            crate::herkunft!()
        }
        fn slug(&self) -> &'static str {
            "probe"
        }
        fn router(&self) -> axum::Router<crate::ProjektKontext> {
            axum::Router::new()
        }
        fn vertrag(&self, v: &mut Vertrag) {
            v.typ::<Boese>();
        }
    }

    #[test]
    fn export_to_wird_abgelehnt_und_nichts_geschrieben() {
        let e = erzeugen(&MitExportTo).unwrap_err();
        assert!(e.iter().any(|z| z.contains("schlichten Dateinamen")), "{e:?}");
        let wurzel = std::env::temp_dir().join(format!("dashboard-sdk-export-to-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wurzel);
        std::fs::create_dir_all(&wurzel).unwrap();
        assert!(pruefen_unter(&MitExportTo, &wurzel).is_err());
        assert!(!wurzel.join("fremd").exists() && !wurzel.join("probe").exists(), "nichts geschrieben");
        for gut in ["Stufe.ts", "Architektur_Stufen.ts"] {
            assert!(ist_dateiname(gut), "{gut}");
        }
        for schlecht in ["../x/A.ts", "a/A.ts", r"a\A.ts", "C:A.ts", "/A.ts", "A.js", ".ts", "..A.ts"] {
            assert!(!ist_dateiname(schlecht), "{schlecht}");
        }
    }
}
