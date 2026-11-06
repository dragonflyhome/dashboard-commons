//! Tests des ContentStores: gleiche Antworten wie die Lesewege von der
//! Platte, nur Geändertes neu, genaue Ereignisse (Ereignis-Test der Etappe 2d),
//! Lesefehler ohne Verlust, Geschwindigkeit.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use tokio::sync::broadcast::error::TryRecvError;

use super::*;

fn beispiel() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel")
}

/// Ein Testordner mit `projects/` und `data/`, beim Ende gelöscht.
struct Ordner(PathBuf);

impl Ordner {
    fn neu(name: &str) -> Ordner {
        let o = std::env::temp_dir().join(format!("dash-store-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&o);
        std::fs::create_dir_all(o.join("projects")).unwrap();
        std::fs::create_dir_all(o.join("data")).unwrap();
        std::fs::write(o.join("data/pc.toml"), "").unwrap();
        std::fs::write(o.join("data/schwerpunkte.toml"), "").unwrap();
        Ordner(o)
    }

    fn projekte(&self) -> PathBuf {
        self.0.join("projects")
    }

    /// Ein Projekt in neuer Form mit einer Datei `daten/fortschritt.toml`.
    fn projekt(&self, slug: &str, prozent: u32) {
        let p = self.projekte().join(slug);
        std::fs::create_dir_all(p.join("daten")).unwrap();
        std::fs::write(p.join("projekt.toml"), format!("schema = 3\ntitle = \"Projekt {slug}\"\nstatus = \"aktiv\"\n")).unwrap();
        fortschritt(&p, prozent);
    }

    fn store(&self) -> ContentStore {
        ContentStore::laden(self.projekte(), self.0.join("data/pc.toml"), self.0.join("data/schwerpunkte.toml"), Abgleich::Beobachter)
    }
}

impl Drop for Ordner {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fortschritt(projekt: &Path, prozent: u32) {
    std::fs::write(projekt.join("daten/fortschritt.toml"), format!("[[zeile]]\ntitel = \"F\"\nprozent = {prozent}\n")).unwrap();
}

/// Alle Ereignisse, die gerade anstehen.
fn ereignisse(rx: &mut broadcast::Receiver<InhaltEreignis>) -> Vec<InhaltEreignis> {
    let mut v = Vec::new();
    loop {
        match rx.try_recv() {
            Ok(e) => v.push(e),
            Err(TryRecvError::Empty) => return v,
            Err(e) => panic!("{e:?}"),
        }
    }
}

/// Der Speicher antwortet genau wie die Lesewege von der Platte: Liste,
/// kaputte Seiten, Hauptseite, Kopf, Namen und jede Daten-Datei.
#[test]
fn gleich_wie_von_der_platte() {
    let dir = beispiel().join("projects");
    let store = ContentStore::laden(dir.clone(), beispiel().join("data/pc.toml"), beispiel().join("data/schwerpunkte.toml"), Abgleich::Beobachter);
    let b = store.bestand();
    let (platte, platte_kaputt) = seite::load_all(&dir);
    let (liste, kaputt) = b.liste();
    let slugs = |l: &[&Project]| l.iter().map(|p| p.slug.clone()).collect::<Vec<_>>();
    assert_eq!(slugs(&liste), platte.iter().map(|p| p.slug.clone()).collect::<Vec<_>>());
    assert_eq!(kaputt.iter().map(|k| (&k.slug, &k.error)).collect::<Vec<_>>(), platte_kaputt.iter().map(|k| (&k.slug, &k.error)).collect::<Vec<_>>());
    assert!(!kaputt.is_empty(), "das Beispiel hat kaputte Projekte");
    let heute = abgeleitet::heute();
    for slug in b.projekte.keys() {
        let p = b.projekt(slug).unwrap();
        assert_eq!(p.hat_projekt_toml, crate::inhalt::pruefung::ist_projekt(&dir, slug), "{slug}");
        assert_eq!(p.namen, seite::daten_namen(&dir, slug), "{slug}");
        if p.hat_projekt_toml {
            let k = b.kopf(slug).unwrap();
            assert_eq!(k.map(|(p, _)| p), seite::kopf_lesen(&dir, slug).map(|(p, _)| p), "{slug}");
        }
        for name in ["fragen", "gantt", "kennzahlen", "fortschritt", "eigen-schritte", "eigen-notizen", "gibt-es-nicht", "eigen-"] {
            assert_eq!(b.daten(slug, name), seite::daten_lesen(&dir, slug, name, heute), "{slug}/{name}");
        }
    }
    assert!(b.daten("..", "gantt").unwrap().is_none());
    assert!(b.daten("gibt-es-nicht", "gantt").unwrap().is_none());
    assert!(matches!(b.schwerpunkte, Ok(Some(_))));
    assert!(b.pc.is_ok());
    assert!(b.version >= 1);
}

/// Der Ereignis-Test: Eine Datei in einem Projekt ändert sich. Nur dieses
/// Projekt wird neu gelesen (die anderen bleiben dieselben Objekte), die
/// Version steigt, und genau ein Ereignis `projekt` geht hinaus.
#[test]
fn eine_aenderung_ein_ereignis() {
    let o = Ordner::neu("eins");
    for s in ["alpha", "beta", "gamma"] {
        o.projekt(s, 10);
    }
    let store = o.store();
    let mut rx = store.abonnieren();
    let vorher = store.bestand();
    assert_eq!(store.abgleichen(), Aenderungen::default(), "ohne Änderung nichts");
    assert!(ereignisse(&mut rx).is_empty());

    fortschritt(&o.projekte().join("beta"), 77);
    let a = store.abgleichen();
    assert_eq!(a.pfade, ["projects/beta/daten/fortschritt.toml"]);
    let nachher = store.bestand();
    assert_eq!(a.projekte, [("beta".to_string(), nachher.version)]);
    assert_eq!(nachher.version, vorher.version + 1);
    assert_eq!(ereignisse(&mut rx), [InhaltEreignis::Projekt(ProjektEreignis { slug: "beta".into(), version: nachher.version, assets: None, dateien: None })]);
    assert!(Arc::ptr_eq(&vorher.projekte["alpha"], &nachher.projekte["alpha"]), "alpha nicht neu gelesen");
    assert!(Arc::ptr_eq(&vorher.projekte["gamma"], &nachher.projekte["gamma"]));
    assert!(!Arc::ptr_eq(&vorher.projekte["beta"], &nachher.projekte["beta"]));
    assert_eq!(nachher.version_von("beta"), nachher.version);
    assert_eq!(nachher.version_von("alpha"), vorher.version_von("alpha"));
    // Der neue Inhalt ist da; wer den alten Stand noch hält, sieht weiter den alten.
    let wert = |b: &Bestand| serde_json::to_value(b.daten("beta", "fortschritt").unwrap().unwrap().0).unwrap()["zeile"][0]["prozent"].clone();
    assert_eq!(wert(&nachher), 77.0);
    assert_eq!(wert(&vorher), 10.0);
}

/// Viele Projekte auf einmal: ein Sammelereignis statt eines je Projekt.
#[test]
fn viele_aenderungen_ein_sammelereignis() {
    let o = Ordner::neu("viele");
    let slugs: Vec<String> = (0..6).map(|i| format!("p{i}")).collect();
    for s in &slugs {
        o.projekt(s, 10);
    }
    let store = o.store();
    let mut rx = store.abonnieren();
    for s in &slugs[..SAMMEL_AB] {
        fortschritt(&o.projekte().join(s), 20);
    }
    store.abgleichen();
    let e = ereignisse(&mut rx);
    assert_eq!(e.len(), SAMMEL_AB, "bis {SAMMEL_AB} einzeln: {e:?}");
    assert!(e.iter().all(|e| matches!(e, InhaltEreignis::Projekt(_))));

    for s in &slugs {
        fortschritt(&o.projekte().join(s), 30);
    }
    store.abgleichen();
    let e = ereignisse(&mut rx);
    let v = store.bestand().version;
    assert_eq!(e, [InhaltEreignis::Sammel(SammelEreignis { version: v, projekte: slugs.clone(), daten: vec![] })]);

    // Ein Git-Lauf über alle: ebenso eines; ein unbekanntes Projekt zählt nicht.
    store.git_geaendert(&slugs);
    assert!(matches!(&ereignisse(&mut rx)[..], [InhaltEreignis::Sammel(s)] if s.projekte.len() == 6));
    store.git_geaendert(&["p1".to_string(), "gibt-es-nicht".to_string()]);
    let v = store.bestand().version;
    assert_eq!(ereignisse(&mut rx), [InhaltEreignis::Projekt(ProjektEreignis { slug: "p1".into(), version: v, assets: None, dateien: None })]);
    store.git_geaendert(&["gibt-es-nicht".to_string()]);
    assert!(ereignisse(&mut rx).is_empty());
    assert_eq!(store.bestand().version, v, "nichts geändert, keine neue Version");
}

/// `data/*.toml`: Ereignis `daten` mit dem Namen; schwerpunkte und pc sind neu gelesen.
#[test]
fn datei_aus_data() {
    let o = Ordner::neu("data");
    o.projekt("alpha", 10);
    let store = o.store();
    let mut rx = store.abonnieren();
    assert!(matches!(store.bestand().schwerpunkte, Ok(Some(ref s)) if s.schwerpunkt.is_empty()));
    std::fs::write(o.0.join("data/schwerpunkte.toml"), "[[schwerpunkt]]\nname = \"Arbeit\"\nreihenfolge = 1\nprojekte = [\"alpha\"]\n").unwrap();
    std::fs::write(o.0.join("data/pc.toml"), "[backup]\ncopies = 3\n").unwrap();
    std::fs::write(o.0.join("data/schluessel.toml"), "").unwrap();
    let a = store.abgleichen();
    let b = store.bestand();
    let namen: Vec<&str> = a.daten.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(namen, ["pc", "schluessel", "schwerpunkte"]);
    assert!(a.projekte.is_empty());
    assert!(matches!(b.schwerpunkte, Ok(Some(ref s)) if s.schwerpunkt.len() == 1));
    assert_eq!(b.pc.as_ref().unwrap().backup.copies, 3);
    let e = ereignisse(&mut rx);
    assert_eq!(e.len(), 3);
    assert_eq!(e[0], InhaltEreignis::Daten(DatenEreignis { name: "pc".into(), version: b.version }));
    // Kaputtes TOML in data/ steht wie bisher als Fehler da, jetzt als Satz mit Zeile.
    std::fs::write(o.0.join("data/pc.toml"), "[backup\n").unwrap();
    store.abgleichen();
    assert_eq!(
        store.bestand().pc.as_ref().unwrap_err(),
        "data/pc.toml, Zeile 1: Die Datei ist kein gültiges TOML. Bei einer Tabellenzeile fehlt die schließende Klammer, etwa `[[balken]]`."
    );
}

/// Bilder in `assets/` und Server-Teile ändern den Inhalt nicht: `change`
/// wie bisher (Pfad), aber keine neue Version und kein Ereignis.
#[test]
fn assets_aendern_den_inhalt_nicht() {
    let o = Ordner::neu("assets");
    o.projekt("alpha", 10);
    std::fs::create_dir_all(o.projekte().join("alpha/assets")).unwrap();
    let store = o.store();
    let mut rx = store.abonnieren();
    let v = store.bestand().version;
    std::fs::write(o.projekte().join("alpha/assets/bild.png"), "x").unwrap();
    let a = store.abgleichen();
    assert_eq!(a.pfade, ["projects/alpha/assets/bild.png"]);
    assert!(a.projekte.is_empty());
    assert_eq!(store.bestand().version, v);
    assert!(ereignisse(&mut rx).is_empty());
}

/// Ein Bruch des Vertrags steht sofort da (wie bisher); ein neues und ein
/// gelöschtes Projekt kommen und gehen mit je einem Ereignis.
#[test]
fn vertragsfehler_neu_und_weg() {
    let o = Ordner::neu("vertrag");
    o.projekt("alpha", 10);
    let store = o.store();
    let mut rx = store.abonnieren();
    std::fs::write(o.projekte().join("alpha/projekt.toml"), "schema = 3\ntitle = \"A\"\nstatus = \"aktief\"\n").unwrap();
    store.abgleichen();
    let b = store.bestand();
    assert!(b.haupt("alpha").unwrap_err().contains("aktief"), "{:?}", b.haupt("alpha"));
    assert_eq!(b.liste().1.len(), 1);
    assert!(b.lesefehler_befunde().is_empty(), "ein Vertragsbruch ist kein Lesefehler");
    assert_eq!(ereignisse(&mut rx).len(), 1);

    o.projekt("beta", 5);
    store.abgleichen();
    assert!(store.bestand().haupt("beta").unwrap().is_some());
    std::fs::remove_dir_all(o.projekte().join("alpha")).unwrap();
    store.abgleichen();
    let b = store.bestand();
    assert!(b.projekt("alpha").is_none());
    assert_eq!(b.liste().0.len(), 1);
    let e = ereignisse(&mut rx);
    assert!(matches!(&e[..], [InhaltEreignis::Projekt(p1), InhaltEreignis::Projekt(p2)] if p1.slug == "beta" && p2.slug == "alpha"), "{e:?}");
}

/// Lesefehler sind kein Verlust: Ist eine Datei gerade gesperrt, bleibt der
/// letzte gute Stand, es gibt einen Fund, und beim nächsten Blick (auch ohne
/// neuen Zeitstempel) wird wieder gelesen.
#[cfg(windows)]
#[test]
fn lesefehler_behaelt_den_letzten_stand() {
    use std::os::windows::fs::OpenOptionsExt;
    let o = Ordner::neu("lesefehler");
    o.projekt("alpha", 10);
    let store = o.store();
    let datei = o.projekte().join("alpha/daten/fortschritt.toml");
    fortschritt(&o.projekte().join("alpha"), 55);
    // Gesperrt ohne Freigabe zum Lesen, wie während ein Programm schreibt.
    let sperre = std::fs::OpenOptions::new().read(true).share_mode(0).open(&datei).unwrap();
    let mut rx = store.abonnieren();
    let v0 = store.bestand().version;
    store.abgleichen();
    // Solange die Sperre bleibt: still wieder versuchen, ohne neue Version
    // und ohne Ereignis (sonst lüde die Oberfläche alle 2 s neu).
    for _ in 0..3 {
        let a = store.abgleichen();
        assert!(a.projekte.is_empty(), "{a:?}");
    }
    assert!(ereignisse(&mut rx).is_empty());
    assert_eq!(store.bestand().version, v0);
    let b = store.bestand();
    let p = b.projekt("alpha").unwrap();
    assert_eq!(p.lesefehler.len(), 1, "{:?}", p.lesefehler);
    assert!(b.haupt("alpha").unwrap().is_some(), "der letzte gute Stand bleibt");
    let wert = |b: &Bestand| serde_json::to_value(b.daten("alpha", "fortschritt").unwrap().unwrap().0).unwrap()["zeile"][0]["prozent"].clone();
    assert_eq!(wert(&b), 10.0);
    let funde = b.lesefehler_befunde();
    assert_eq!(funde.len(), 1);
    assert_eq!(funde[0].0, "alpha");
    assert!(funde[0].1.text.contains("letzten guten Stand"), "{}", funde[0].1.text);
    drop(sperre);
    // Kein neuer Zeitstempel, trotzdem neu gelesen.
    store.abgleichen();
    let b = store.bestand();
    assert!(b.projekt("alpha").unwrap().lesefehler.is_empty());
    assert_eq!(wert(&b), 55.0);
    assert!(b.lesefehler_befunde().is_empty());
    assert_eq!(ereignisse(&mut rx), [InhaltEreignis::Projekt(ProjektEreignis { slug: "alpha".into(), version: v0 + 1, assets: None, dateien: None })]);
}

/// Gesperrt schon beim ersten Lesen (ohne früheren guten Stand): wird alle
/// 2 s wieder versucht und kommt herein, sobald die Sperre weg ist; ebenso
/// `data/pc.toml`.
#[cfg(windows)]
#[test]
fn gesperrt_beim_start_wird_spaeter_gelesen() {
    use std::os::windows::fs::OpenOptionsExt;
    let o = Ordner::neu("start-gesperrt");
    o.projekt("alpha", 42);
    std::fs::write(o.0.join("data/pc.toml"), "[backup]\ncopies = 2\n").unwrap();
    let oeffnen = |p: PathBuf| std::fs::OpenOptions::new().read(true).share_mode(0).open(p).unwrap();
    let s1 = oeffnen(o.projekte().join("alpha/daten/fortschritt.toml"));
    let s2 = oeffnen(o.0.join("data/pc.toml"));
    let store = o.store();
    let b = store.bestand();
    let p = b.projekt("alpha").unwrap();
    assert!(!p.gut && p.lesefehler.len() == 1, "{:?}", p.lesefehler);
    assert!(b.pc.is_err());
    let funde = b.lesefehler_befunde();
    assert_eq!(funde.len(), 2, "{funde:?}");
    assert!(funde[0].1.text.contains("noch nicht"), "{funde:?}");
    let mut rx = store.abonnieren();
    store.abgleichen();
    assert!(ereignisse(&mut rx).is_empty(), "gesperrt: still");
    drop((s1, s2));
    store.abgleichen();
    let b = store.bestand();
    let p = b.projekt("alpha").unwrap();
    assert!(p.gut && p.lesefehler.is_empty());
    assert!(b.daten("alpha", "fortschritt").unwrap().is_some());
    assert_eq!(b.pc.as_ref().unwrap().backup.copies, 2);
    assert!(b.lesefehler_befunde().is_empty());
    let e = ereignisse(&mut rx);
    assert_eq!(e.len(), 2, "{e:?}");
}

/// Kein Lesefehler: Eine projekt.toml in UTF-16 und eine gelöschte pc.toml stehen
/// sofort als Fehler da (rote Karte), nicht still als alter Stand.
#[test]
fn utf16_und_fehlende_datei_sind_keine_lesefehler() {
    let o = Ordner::neu("utf16");
    std::fs::create_dir_all(o.projekte().join("alt")).unwrap();
    std::fs::write(o.projekte().join("alt/projekt.toml"), "schema = 3\ntitle = \"Alt\"\nstatus = \"aktiv\"\n").unwrap();
    let store = o.store();
    assert!(store.bestand().haupt("alt").unwrap().is_some());
    let mut utf16: Vec<u8> = vec![0xFF, 0xFE];
    for c in "schema = 3\ntitle = \"Alt neu\"\nstatus = \"aktiv\"\n".encode_utf16() {
        utf16.extend_from_slice(&c.to_le_bytes());
    }
    std::fs::write(o.projekte().join("alt/projekt.toml"), utf16).unwrap();
    std::fs::remove_file(o.0.join("data/pc.toml")).unwrap();
    store.abgleichen();
    let b = store.bestand();
    assert!(b.haupt("alt").is_err(), "UTF-16 muss als Fehler dastehen");
    assert_eq!(b.liste().1.len(), 1, "rote Karte");
    assert!(b.projekt("alt").unwrap().lesefehler.is_empty());
    // Ohne ganzen Pfad (der nennt den Windows-Benutzer, /api/pc ist frei).
    assert!(b.pc.as_ref().unwrap_err().starts_with("data/pc.toml lässt sich nicht lesen: "), "{:?}", b.pc);
    assert!(b.lesefehler_befunde().is_empty());
    assert!(!pc_lesen(&o.0.join("data/pc.toml")).1, "fehlt: nicht gesperrt");
    assert!(ist_gesperrt(&std::io::Error::from(std::io::ErrorKind::PermissionDenied)));
    assert!(ist_gesperrt(&std::io::Error::from_raw_os_error(32)));
    assert!(ist_gesperrt(&std::io::Error::from_raw_os_error(33)));
    assert!(!ist_gesperrt(&std::io::Error::from(std::io::ErrorKind::NotFound)));
    assert!(!ist_gesperrt(&std::io::Error::from(std::io::ErrorKind::InvalidData)));
}

/// Gezählt wird der Fehler der Leseoperation selbst: Eine kurze Sperre
/// (Virenscanner, Index) während des Lesens ist ein Lesefehler, auch wenn
/// sie gleich danach weg ist; ein späterer Blick entscheidet nichts mehr.
#[cfg(windows)]
#[test]
fn sperre_zaehlt_beim_lesen_selbst() {
    use std::os::windows::fs::OpenOptionsExt;
    let o = Ordner::neu("kurz-gesperrt");
    o.projekt("alpha", 10);
    let datei = o.projekte().join("alpha/daten/fortschritt.toml");
    let sperre = std::fs::OpenOptions::new().read(true).share_mode(0).open(&datei).unwrap();
    // Außerhalb einer Merkliste (wie auf API-Wegen) merkt sich niemand die Sperre.
    assert!(seite::neue_datei_lesen(&datei, "projects/alpha/daten/fortschritt.toml").is_err());
    assert!(!seite::merkliste_offen());
    assert_eq!(seite::mit_merkliste(|| ()).1, Vec::<String>::new(), "nichts von draußen übrig");
    // Innerhalb schon, mit dem Fehler des Lesens selbst.
    let (r, liste) = seite::mit_merkliste(|| seite::neue_datei_lesen(&datei, "projects/alpha/daten/fortschritt.toml"));
    assert!(r.is_err());
    assert_eq!(liste, ["projects/alpha/daten/fortschritt.toml"]);
    let p = ProjektStand::lesen(&o.projekte(), "alpha", abgeleitet::heute()).unwrap();
    // Die Sperre ist weg, bevor jemand den Stand auswertet.
    drop(sperre);
    assert!(!seite::merkliste_offen(), "lesen hat seine Merkliste wieder geschlossen");
    assert_eq!(p.gesperrt.len(), 1, "{:?}", p.gesperrt);
    assert_eq!(p.gesperrt[0].0, "projects/alpha/daten/fortschritt.toml");
    assert!(!p.gut);
    // Danach frei: sauber gelesen.
    let p = ProjektStand::lesen(&o.projekte(), "alpha", abgeleitet::heute()).unwrap();
    assert!(p.gesperrt.is_empty() && p.gut);
}

/// `data/pc.toml` wird genau einmal gelesen; Sperre und Satz kommen aus
/// demselben Lesen. Gesperrt: Lesefehler (letzter guter Stand gilt), frei
/// und kaputt: Fehler im Vertrag mit Zeile, keine Sperre.
#[cfg(windows)]
#[test]
fn pc_toml_einmal_gelesen() {
    use std::os::windows::fs::OpenOptionsExt;
    let o = Ordner::neu("pc-einmal");
    let datei = o.0.join("data/pc.toml");
    std::fs::write(&datei, "[backup]\ncloud_stand = \"teilweise\"\n").unwrap();
    let sperre = std::fs::OpenOptions::new().read(true).share_mode(0).open(&datei).unwrap();
    let (r, gesperrt) = pc_lesen(&datei);
    drop(sperre);
    assert!(gesperrt, "{r:?}");
    let satz = r.unwrap_err();
    assert!(satz.starts_with("data/pc.toml lässt sich nicht lesen: "), "{satz}");
    assert!(!satz.contains(&datei.display().to_string()), "ohne ganzen Pfad: {satz}");
    let (r, gesperrt) = pc_lesen(&datei);
    assert!(!gesperrt);
    assert_eq!(r.unwrap().backup.cloud_stand, Some(crate::pc::pcdata::KopieStand::Teilweise));
    std::fs::write(&datei, "[backup]\ncloud_stand = \"voll\"\n").unwrap();
    let (r, gesperrt) = pc_lesen(&datei);
    assert!(!gesperrt);
    assert_eq!(r.unwrap_err(), "data/pc.toml, Zeile 2: Bei `cloud_stand` steht „voll“. Erlaubt sind: vorhanden, teilweise, fehlt.");
    // Eine fehlende Datei ist keine Sperre.
    std::fs::remove_file(&datei).unwrap();
    assert!(!pc_lesen(&datei).1);
}

/// Verknüpfungen (Junction, wie `mklink /J`, ohne Administrator) sind nie
/// eine Sperre: Projektordner, `daten/` oder `daten/x.toml` als Verknüpfung
/// geben sofort den Fehler (rote Karte), kein stilles Behalten, kein Neulesen
/// alle 2 s.
#[cfg(windows)]
#[test]
fn verknuepfung_ist_keine_sperre() {
    let o = Ordner::neu("verknuepfung");
    o.projekt("alpha", 10);
    o.projekt("beta", 10);
    o.projekt("gamma", 10);
    let ziel = o.0.join("woanders");
    std::fs::create_dir_all(ziel.join("daten")).unwrap();
    std::fs::write(ziel.join("projekt.toml"), "schema = 3\ntitle = \"Fremd\"\nstatus = \"aktiv\"\n").unwrap();
    std::fs::write(ziel.join("daten/fortschritt.toml"), "[[zeile]]\ntitel = \"F\"\nprozent = 1\n").unwrap();
    // alpha: daten/ ist eine Verknüpfung; beta: daten/fortschritt.toml ist
    // eine Verknüpfung auf einen Ordner; delta: der ganze Projektordner.
    std::fs::remove_dir_all(o.projekte().join("alpha/daten")).unwrap();
    std::fs::remove_file(o.projekte().join("beta/daten/fortschritt.toml")).unwrap();
    // mklink nimmt nur Rückstriche: die Pfade Stück für Stück bauen.
    let p = o.projekte();
    let ok = seite::junction_anlegen(&p.join("alpha").join("daten"), &ziel.join("daten"))
        && seite::junction_anlegen(&p.join("beta").join("daten").join("fortschritt.toml"), &ziel.join("daten"))
        && seite::junction_anlegen(&p.join("delta"), &ziel);
    if !ok {
        eprintln!("Junction ließ sich nicht anlegen; Test übersprungen.");
        return;
    }
    let store = o.store();
    let mut rx = store.abonnieren();
    let b = store.bestand();
    for slug in ["alpha", "beta", "delta"] {
        let p = b.projekt(slug).unwrap_or_else(|| panic!("{slug} fehlt"));
        assert!(p.lesefehler.is_empty() && p.gut, "{slug}: {:?}", p.lesefehler);
    }
    assert!(b.haupt("alpha").unwrap_err().contains("Verknüpfung"), "{:?}", b.haupt("alpha").is_ok());
    assert!(b.daten("beta", "fortschritt").is_err(), "beta: Fehler statt alter Stand");
    assert!(b.haupt("delta").unwrap_err().contains("Verknüpfung"));
    assert!(b.lesefehler_befunde().is_empty());
    // Kein Neulesen alle 2 s, keine Ereignisse.
    let v = b.version;
    store.abgleichen();
    store.abgleichen();
    assert_eq!(store.bestand().version, v);
    assert!(ereignisse(&mut rx).is_empty());
}

/// Ein Unterordner oder eine fremde Datei in `daten/` fällt auf (vorher
/// sofort ein Fehler) und gibt ein Ereignis; ebenso, wenn `daten` verschwindet.
#[test]
fn alles_in_daten_zaehlt() {
    let o = Ordner::neu("daten-alles");
    o.projekt("alpha", 10);
    let store = o.store();
    let mut rx = store.abonnieren();
    std::fs::create_dir_all(o.projekte().join("alpha/daten/unter")).unwrap();
    let a = store.abgleichen();
    assert!(a.pfade.contains(&"projects/alpha/daten/unter".to_string()), "{a:?}");
    assert!(store.bestand().haupt("alpha").unwrap_err().contains("Unterordner"));
    assert_eq!(ereignisse(&mut rx).len(), 1);
    std::fs::remove_dir_all(o.projekte().join("alpha/daten/unter")).unwrap();
    std::fs::write(o.projekte().join("alpha/daten/notiz.txt"), "x").unwrap();
    let a = store.abgleichen();
    assert!(a.pfade.contains(&"projects/alpha/daten/notiz.txt".to_string()), "{a:?}");
    assert_eq!(a.projekte.len(), 1);
    std::fs::remove_dir_all(o.projekte().join("alpha/daten")).unwrap();
    let a = store.abgleichen();
    assert!(a.pfade.contains(&"projects/alpha/daten".to_string()), "{a:?}");
    assert!(store.bestand().daten("alpha", "fortschritt").unwrap().is_none());
}

/// Im Dienst liest ein Zugriff nie von der Platte (auch nicht am
/// Tageswechsel); das tut nur der Beobachter.
#[test]
fn zugriff_im_dienst_gleicht_nicht_ab() {
    let o = Ordner::neu("kein-abgleich");
    o.projekt("alpha", 10);
    let store = o.store();
    let v = store.bestand().version;
    o.projekt("beta", 1);
    assert_eq!(store.bestand().version, v);
    assert!(store.bestand().projekt("beta").is_none());
    store.abgleichen();
    assert!(store.bestand().projekt("beta").is_some());
}

/// Ist der Projektordner selbst nicht lesbar, bleibt alles, wie es ist.
#[test]
fn unlesbarer_ordner_ist_kein_verlust() {
    let o = Ordner::neu("ordner");
    o.projekt("alpha", 10);
    let store = o.store();
    let weg = o.0.join("projects-weg");
    std::fs::rename(o.projekte(), &weg).unwrap();
    let a = store.abgleichen();
    let b = store.bestand();
    assert!(a.projekte.is_empty());
    assert!(!b.ordner_lesbar);
    assert!(b.projekt("alpha").is_some());
    std::fs::rename(&weg, o.projekte()).unwrap();
    store.abgleichen();
    assert!(store.bestand().ordner_lesbar);
}

/// Wie beim ersten Ordner: Wird eine weitere Quelle unlesbar (hier
/// umbenannt), bleiben ihre Projekte mit dem letzten Stand, und es gibt
/// einen Satz; ein gelöschtes Projekt einer lesbaren Quelle geht trotzdem.
/// Was in der Zeit in der unlesbaren Quelle gelöscht wurde, geht danach.
#[test]
fn unlesbare_weitere_quelle_ist_kein_verlust() {
    let o = Ordner::neu("quelle-weg");
    o.projekt("alpha", 10);
    o.projekt("beta", 10);
    let extra = o.0.join("fremd/projects");
    for slug in ["solo", "zwei"] {
        let p = extra.join(slug);
        std::fs::create_dir_all(p.join("daten")).unwrap();
        std::fs::write(p.join("projekt.toml"), "schema = 3\ntitle = \"Solo\"\nstatus = \"aktiv\"\n").unwrap();
        fortschritt(&p, 30);
    }
    let q = crate::quellen::Quellen::aus_liste(vec![o.projekte(), extra.clone()]);
    let store = ContentStore::laden_aus(&q, o.0.join("data/pc.toml"), o.0.join("data/schwerpunkte.toml"), Abgleich::Beobachter);
    let mut rx = store.abonnieren();
    std::fs::rename(o.0.join("fremd"), o.0.join("fremd-weg")).unwrap();
    std::fs::remove_dir_all(o.projekte().join("beta")).unwrap();
    let a = store.abgleichen();
    let b = store.bestand();
    assert_eq!(a.projekte.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(), ["beta"]);
    assert_eq!(b.projekte.keys().collect::<Vec<_>>(), ["alpha", "solo", "zwei"]);
    assert_eq!(b.quellen_unlesbar, [crate::quellen::satz_unlesbar(1)]);
    assert_eq!(ereignisse(&mut rx).len(), 1);
    // Ein zweiter Blick ändert nichts (kein Ereignis, kein Nachlesen).
    assert!(store.abgleichen().projekte.is_empty());
    std::fs::remove_dir_all(o.0.join("fremd-weg/projects/zwei")).unwrap();
    std::fs::rename(o.0.join("fremd-weg"), o.0.join("fremd")).unwrap();
    store.abgleichen();
    let b = store.bestand();
    assert_eq!(b.projekte.keys().collect::<Vec<_>>(), ["alpha", "solo"]);
    assert_eq!(b.kopf("solo").unwrap().unwrap().0.title, "Solo");
    assert!(b.quellen_unlesbar.is_empty());
}

/// Abnahme quellen_haerten (Muss): y in Quelle 2 verdeckt y in Quelle 3.
/// Quelle 2 wird gesperrt, y dort gelöscht, Quelle 2 kommt zurück: Danach
/// gilt y aus Quelle 3, nicht still das gelöschte aus Quelle 2.
#[test]
fn quellwechsel_nach_sperre_liest_neu() {
    let o = Ordner::neu("quellwechsel");
    o.projekt("alpha", 10);
    let zwei = o.0.join("zwei/projects");
    let drei = o.0.join("drei/projects");
    for (q, titel) in [(&zwei, "Y aus 2"), (&drei, "Y aus 3")] {
        let p = q.join("y");
        std::fs::create_dir_all(p.join("daten")).unwrap();
        std::fs::write(p.join("projekt.toml"), format!("schema = 3\ntitle = \"{titel}\"\nstatus = \"aktiv\"\n")).unwrap();
        fortschritt(&p, 30);
    }
    let q = crate::quellen::Quellen::aus_liste(vec![o.projekte(), zwei.clone(), drei]);
    let store = ContentStore::laden_aus(&q, o.0.join("data/pc.toml"), o.0.join("data/schwerpunkte.toml"), Abgleich::Beobachter);
    let titel = |b: &Bestand| b.kopf("y").unwrap().unwrap().0.title;
    assert_eq!(titel(&store.bestand()), "Y aus 2");
    std::fs::rename(o.0.join("zwei"), o.0.join("zwei-weg")).unwrap();
    store.abgleichen();
    assert_eq!(titel(&store.bestand()), "Y aus 2", "gesperrt: der letzte Stand bleibt");
    std::fs::remove_dir_all(o.0.join("zwei-weg/projects/y")).unwrap();
    std::fs::rename(o.0.join("zwei-weg"), o.0.join("zwei")).unwrap();
    let a = store.abgleichen();
    assert!(a.projekte.iter().any(|(s, _)| s == "y"), "{a:?}");
    assert_eq!(titel(&store.bestand()), "Y aus 3");
}

/// Abnahme quellen_haerten (Sollte 2): Auch mit nur einer Quelle kommt eine
/// abweichende dashboard-version.toml in den Bestand (und so in die Prüfung).
#[test]
fn versionsdatei_auch_bei_einer_quelle() {
    let o = Ordner::neu("version-eine");
    o.projekt("alpha", 10);
    std::fs::write(o.0.join(crate::quellen::VERSIONSDATEI), "tag = \"v0.0.0-alt\"\n").unwrap();
    let store = o.store();
    let b = store.bestand();
    assert_eq!(b.quellen_version.len(), 1, "{:?}", b.quellen_version);
    assert!(b.quellen_version[0].starts_with("Die Inhaltsquelle projects ist gegen das Dashboard v0.0.0-alt"));
    std::fs::remove_file(o.0.join(crate::quellen::VERSIONSDATEI)).unwrap();
    store.abgleichen();
    assert!(store.bestand().quellen_version.is_empty());
}

/// Im Testbetrieb gleicht jeder Zugriff ab; geschrieben, gefragt, gesehen.
#[test]
fn bei_jedem_zugriff() {
    let o = Ordner::neu("zugriff");
    let store = ContentStore::neu(o.projekte(), o.0.join("data/pc.toml"), o.0.join("data/schwerpunkte.toml"), Abgleich::BeiJedemZugriff);
    assert!(store.bestand().projekte.is_empty());
    o.projekt("alpha", 10);
    assert!(store.bestand().projekt("alpha").is_some());
}

/// Schnell: Ein Zugriff aus dem Speicher (Liste und eine Datei) braucht
/// keine Millisekunde; die Grenze hat viel Luft.
#[test]
fn zugriff_aus_dem_speicher_ist_schnell() {
    let o = Ordner::neu("schnell");
    for i in 0..25 {
        o.projekt(&format!("p{i}"), i);
    }
    let start = Instant::now();
    let store = o.store();
    let laden = start.elapsed();
    assert!(laden < Duration::from_secs(2), "{laden:?}");
    let start = Instant::now();
    for _ in 0..100 {
        let b = store.bestand();
        assert_eq!(b.liste().0.len(), 25);
        assert!(b.daten("p7", "fortschritt").unwrap().is_some());
    }
    let je = start.elapsed() / 100;
    assert!(je < Duration::from_millis(50), "{je:?}");
}

/// Etappe 2e (Abnahme): Am Tageswechsel liest der Beobachter auch Projekte mit
/// Zeitstrahl neu, nicht nur die mit Kennzahlen.
#[test]
fn tageswechsel_liest_auch_zeitstrahl() {
    let dir = beispiel().join("projects");
    let heute = abgeleitet::heute();
    let stand = |slug: &str| ProjektStand::lesen(&dir, slug, heute).unwrap();
    assert!(stand("dashboard").haengt_vom_tag_ab(), "zeitstrahl.toml");
    assert!(stand("neu-form").haengt_vom_tag_ab(), "zeitstrahl.toml und kennzahlen.toml");
    assert!(stand("neu-daten").haengt_vom_tag_ab(), "kennzahlen.toml");
    assert!(!stand("geflecht").haengt_vom_tag_ab(), "nichts vom Datum Abhängiges");
    let o = Ordner::neu("tageswechsel-zeitstrahl");
    std::fs::create_dir_all(o.projekte().join("z/daten")).unwrap();
    std::fs::write(o.projekte().join("z/projekt.toml"), "schema = 3\ntitle = \"Z\"\nstatus = \"aktiv\"\nsummary = \"S\"\n").unwrap();
    std::fs::write(o.projekte().join("z/daten/zeitstrahl.toml"), "[[eintrag]]\nkennung = \"a\"\ndatum = \"2026-10\"\ntitel = \"A\"\n").unwrap();
    assert!(ProjektStand::lesen(&o.projekte(), "z", heute).unwrap().haengt_vom_tag_ab());
}

/// Etappe B1 (Abnahme): Eine Ampel mit `stand` hängt vom Datum ab. Nach
/// Mitternacht liest der Beobachter das Projekt neu, und der Stand kippt von
/// frisch auf alt, ohne dass sich eine Datei geändert hat.
#[test]
fn tageswechsel_kippt_ampel_stand_auf_alt() {
    use crate::vertrag::daten::{Datei, StandStufe};
    let heute = abgeleitet::heute();
    let gestern = heute.pred_opt().unwrap();
    let o = Ordner::neu("tageswechsel-ampel");
    let p = o.projekte().join("a");
    std::fs::create_dir_all(p.join("daten")).unwrap();
    std::fs::write(p.join("projekt.toml"), "schema = 3\ntitle = \"A\"\nstatus = \"aktiv\"\nsummary = \"S\"\n").unwrap();
    // Vor 7 Tagen, alt ab 7: gestern 6 Tage (frisch), heute 7 Tage (alt).
    let stand = (heute - chrono::Duration::days(7)).format("%Y-%m-%d");
    std::fs::write(p.join("daten/ampel.toml"), format!("[[zeile]]\ntitel = \"Postfach\"\nfarbe = \"gelb\"\nstand = \"{stand}\"\nalt_ab = 7\n")).unwrap();
    let store = o.store();
    let stufe = |b: &Bestand| -> (i64, StandStufe) {
        let Some((DatenInhalt::Ampel(Datei::Ganz(a)), _)) = b.daten("a", "ampel").unwrap() else { panic!("ampel fehlt") };
        let s = a.zeile[0].stand_alter.unwrap();
        (s.tage, s.stufe)
    };
    assert!(store.bestand().projekt("a").unwrap().haengt_vom_tag_ab(), "ampel.toml hängt vom Tag ab");
    // Der Bestand von gestern (wie vor Mitternacht gelesen).
    let alt = ProjektStand::lesen(&o.projekte(), "a", gestern).unwrap();
    store.aendern(|b| {
        b.tag = gestern;
        b.projekte.insert("a".into(), Arc::new(alt));
    });
    assert_eq!(stufe(&store.bestand()), (6, StandStufe::Frisch));
    // Mitternacht: kein Pfad geändert, aber der Tag.
    let a = store.abgleichen();
    assert!(a.projekte.iter().any(|(s, _)| s == "a"), "{a:?}");
    assert_eq!(stufe(&store.bestand()), (7, StandStufe::Alt));
    assert_eq!(store.bestand().tag, heute);
}

/// Etappe 4b: mehrere Inhaltsquellen. Ein Ordner mit Projekten und ein
/// einzelnes Projekt; der doppelte Slug bleibt aus der ersten Quelle, dazu ein
/// Satz. Eine Änderung im einzelnen Projekt kommt beim Abgleich an.
#[test]
fn mehrere_quellen_vereinigt_erste_bleibt() {
    let o = Ordner::neu("quellen");
    o.projekt("alpha", 10);
    o.projekt("doppelt", 20);
    let extra = o.0.join("geteilt");
    for (slug, prozent) in [("doppelt", 99), ("solo", 30)] {
        let p = extra.join(slug);
        std::fs::create_dir_all(p.join("daten")).unwrap();
        std::fs::write(p.join("projekt.toml"), format!("schema = 3\ntitle = \"Quelle 2 {slug}\"\nstatus = \"aktiv\"\n")).unwrap();
        fortschritt(&p, prozent);
    }
    let q = crate::quellen::Quellen::aus_liste(vec![o.projekte(), extra.join("doppelt"), extra.join("solo")]);
    let store = ContentStore::laden_aus(&q, o.0.join("data/pc.toml"), o.0.join("data/schwerpunkte.toml"), Abgleich::Beobachter);
    let b = store.bestand();
    assert_eq!(b.projekte.keys().collect::<Vec<_>>(), ["alpha", "doppelt", "solo"]);
    let titel = |b: &Bestand, s: &str| b.kopf(s).unwrap().unwrap().0.title;
    assert_eq!(titel(&b, "doppelt"), "Projekt doppelt", "die erste Quelle bleibt sichtbar");
    assert_eq!(titel(&b, "solo"), "Quelle 2 solo");
    assert_eq!(b.quellen_doppelt.len(), 1, "{:?}", b.quellen_doppelt);
    assert!(b.quellen_doppelt[0].starts_with("Das Projekt „doppelt“ gibt es zweimal"));
    // Änderung im einzelnen Projekt der zweiten Quelle.
    let mut rx = store.abonnieren();
    std::thread::sleep(Duration::from_millis(20));
    std::fs::write(extra.join("solo/projekt.toml"), "schema = 3\ntitle = \"Solo neu\"\nstatus = \"aktiv\"\n").unwrap();
    let a = store.abgleichen();
    assert!(a.pfade.contains(&"projects/solo/projekt.toml".to_string()), "{a:?}");
    assert_eq!(titel(&store.bestand(), "solo"), "Solo neu");
    assert_eq!(ereignisse(&mut rx).len(), 1);
    // Eine Quelle: genau wie laden mit ihrem Ordner.
    let eine = ContentStore::laden_aus(
        &crate::quellen::Quellen::ein_ordner(o.projekte()),
        o.0.join("data/pc.toml"),
        o.0.join("data/schwerpunkte.toml"),
        Abgleich::Beobachter,
    );
    assert_eq!(eine.bestand().projekte.keys().collect::<Vec<_>>(), ["alpha", "doppelt"]);
    assert!(eine.bestand().quellen_doppelt.is_empty());
}
