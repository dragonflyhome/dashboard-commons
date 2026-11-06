//! Tests der Prüfung (Entscheidung lokal-oeffnen, `[form].tests`).
//! Alles in einem eigenen Temp-Ordner mit einem erfundenen Profil; nichts
//! wird geöffnet. Der erfundene Tabu-Name ist „geheim ablage“.

use super::*;

/// Erfundener Tabu-Name für alle Tests.
pub const TABU: &str = "geheim ablage";

/// Ein Spielplatz: erfundenes Profil, eine Inhaltsquelle, ein Ordner draußen,
/// ein Ordner des Dienstes, die Tabu-Liste als Datei.
pub struct Platz {
    pub wurzel: PathBuf,
    pub profil: PathBuf,
    pub quelle: PathBuf,
    pub draussen: PathBuf,
    pub dienst: PathBuf,
    pub liste: PathBuf,
    /// Gingen die Junctions (`mklink /J`)?
    pub junctions: bool,
}

fn datei(p: &Path) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, b"x").unwrap();
}

impl Platz {
    pub fn neu(name: &str) -> Platz {
        let wurzel = std::env::temp_dir().join(format!("dashboard-oeffnen-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&wurzel);
        let profil = wurzel.join("profil");
        let quelle = wurzel.join("quelle");
        let draussen = wurzel.join("draussen");
        let dienst = profil.join("dienst");
        let dok = profil.join("Dokumente");
        for f in ["bauplan.pdf", "GROSS.PDF", "tabelle.xlsx", "bild.svg", "archiv.zip", "ohne_endung", "x.pdf.exe", "harte.pdf"] {
            datei(&dok.join(f));
        }
        for e in VERBOTENE_ENDUNGEN {
            datei(&dok.join(format!("datei.{e}")));
            datei(&dok.join(format!("GROSS.{}", e.to_uppercase())));
        }
        std::fs::create_dir_all(dok.join("Werkzeugkiste")).unwrap();
        std::fs::create_dir_all(dok.join("komma,ordner")).unwrap();
        datei(&profil.join("AppData").join("Local").join("x.pdf"));
        datei(&profil.join("Geheim Ablage").join("brief.pdf"));
        datei(&profil.join("projekt").join(".git").join("notiz.md"));
        datei(&dienst.join("x.txt"));
        datei(&quelle.join("notiz.md"));
        datei(&draussen.join("fremd.pdf"));
        let liste = wurzel.join("ausschluss.toml");
        std::fs::write(&liste, format!("nie_betreten = [\"{TABU}\"]\n")).unwrap();
        let j1 = crate::inhalt::seite::junction_anlegen(&profil.join("sprung"), &draussen);
        let j2 = crate::inhalt::seite::junction_anlegen(&profil.join("harmlos"), &profil.join("Geheim Ablage"));
        let j3 = crate::inhalt::seite::junction_anlegen(&profil.join("innen"), &dok);
        Platz { wurzel, profil, quelle, draussen, dienst, liste, junctions: j1 && j2 && j3 }
    }

    /// Ein Pfad im Platz als Text mit Rückstrichen.
    pub fn text(&self, p: &Path) -> String {
        p.display().to_string().replace('/', "\\")
    }

    pub fn dok(&self, name: &str) -> String {
        self.text(&self.profil.join("Dokumente").join(name))
    }

    pub fn regeln(&self) -> Regeln {
        Regeln::des_servers(Some(self.profil.clone()), vec![self.quelle.clone()], vec![self.dienst.clone()], Ausschluss::aus(&[TABU]))
    }
}

impl Drop for Platz {
    fn drop(&mut self) {
        // Junctions zuerst lösen (remove_dir löscht nur die Verknüpfung, nie das Ziel).
        for j in ["sprung", "harmlos", "innen"] {
            let _ = std::fs::remove_dir(self.profil.join(j));
        }
        let _ = std::fs::remove_dir_all(&self.wurzel);
    }
}

fn ab(pfad: &str, r: &Regeln) -> Abweisung {
    match pruefen(pfad, r) {
        Err(a) => a,
        Ok(z) => panic!("hätte abgewiesen werden sollen: {pfad} → {z:?}"),
    }
}

#[test]
fn erlaubte_datei_und_ordner() {
    let p = Platz::neu("erlaubt");
    let r = p.regeln();
    let z = pruefen(&p.dok("bauplan.pdf"), &r).unwrap();
    assert_eq!(z.art, Art::Datei);
    assert!(z.pfad.ends_with("bauplan.pdf") && !z.pfad.display().to_string().starts_with(r"\\"), "{z:?}");
    assert_eq!(pruefen(&p.dok("GROSS.PDF"), &r).unwrap().art, Art::Datei, "Endung in Großschreibung");
    assert_eq!(pruefen(&p.dok("Werkzeugkiste"), &r).unwrap().art, Art::Ordner);
    assert_eq!(pruefen(&p.dok("archiv.zip"), &r).unwrap().art, Art::Ordner, "zip nur als Ordner im Explorer");
    assert_eq!(pruefen(&p.text(&p.quelle.join("notiz.md")), &r).unwrap().art, Art::Datei, "Inhaltsquelle");
    // Schrägstriche gehen auch (Windows liest sie gleich).
    assert!(pruefen(&p.dok("bauplan.pdf").replace('\\', "/"), &r).is_ok());
    if p.junctions || !crate::ueberspringen("mklink /J ging nicht") {
        // Eine Junction, die in der Wurzel bleibt, ist erlaubt; geöffnet wird das Ziel.
        let z = pruefen(&p.text(&p.profil.join("innen").join("bauplan.pdf")), &r).unwrap();
        assert!(z.pfad.display().to_string().contains("Dokumente"), "{z:?}");
    }
}

#[test]
fn jede_verbotene_endung_einzeln() {
    let p = Platz::neu("endungen");
    let r = p.regeln();
    for e in VERBOTENE_ENDUNGEN {
        assert!(!ENDUNGEN.contains(&e) && !EXPLORER_ENDUNGEN.contains(&e), "{e} steht in der Positivliste");
        assert_eq!(ab(&p.dok(&format!("datei.{e}")), &r), Abweisung::Endung(e.to_string()), "{e}");
        assert_eq!(ab(&p.dok(&format!("GROSS.{}", e.to_uppercase())), &r), Abweisung::Endung(e.to_string()), "{e} groß");
    }
    assert_eq!(ab(&p.dok("x.pdf.exe"), &r), Abweisung::Endung("exe".into()), "doppelte Endung");
    assert_eq!(ab(&p.dok("ohne_endung"), &r), Abweisung::OhneEndung);
}

#[test]
fn form_und_geraete() {
    let p = Platz::neu("form");
    let r = p.regeln();
    let fall = |pfad: &str, erwartet: Abweisung| assert_eq!(ab(pfad, &r), erwartet, "{pfad}");
    fall(&format!("{}\\..\\Dokumente\\bauplan.pdf", p.text(&p.profil.join("Dokumente"))), Abweisung::Elternverweis);
    fall(&format!("{}\\.\\bauplan.pdf", p.text(&p.profil.join("Dokumente"))), Abweisung::Elternverweis);
    fall(r"\\server\freigabe\bauplan.pdf", Abweisung::Netz);
    fall(r"//server/freigabe/bauplan.pdf", Abweisung::Netz);
    fall(&format!(r"\\?\{}", p.dok("bauplan.pdf")), Abweisung::Geraet);
    fall(&format!(r"\\.\{}", p.dok("bauplan.pdf")), Abweisung::Geraet);
    fall(r"\\?\UNC\server\freigabe\x.pdf", Abweisung::Geraet);
    fall(r"\\.\PhysicalDrive0", Abweisung::Geraet);
    fall(&format!("{}:versteckt", p.dok("bauplan.pdf")), Abweisung::Datenstrom);
    fall(&format!("{}::$DATA", p.dok("bauplan.pdf")), Abweisung::Datenstrom);
    fall(r"Dokumente\bauplan.pdf", Abweisung::NichtAbsolut);
    fall(r"C:bauplan.pdf", Abweisung::NichtAbsolut);
    fall(r"\Users\beispiel\bauplan.pdf", Abweisung::NichtAbsolut);
    fall("", Abweisung::Leer);
    fall(r"C:\Users\beispiel\NUL", Abweisung::Geraet);
    fall(r"C:\Users\beispiel\com1.txt", Abweisung::Geraet);
    fall(r"C:\PROGRA~1\x.pdf", Abweisung::Kurzname);
    fall(r"C:\Users\beispiel\x.pdf.", Abweisung::Zeichen);
    fall(r"C:\Users\beispiel\x.pdf ", Abweisung::Zeichen);
    fall(r"C:\Users\beispiel\*.pdf", Abweisung::Zeichen);
    fall("C:\\Users\\beispiel\\a\u{7}.pdf", Abweisung::Zeichen);
    fall(r"C:\Users\beispiel\\doppelt.pdf", Abweisung::Zeichen);
    fall(&p.text(&p.profil.join("projekt").join(".git").join("notiz.md")), Abweisung::Git);
    fall(&p.text(&p.profil.join("projekt").join(".GIT").join("notiz.md")), Abweisung::Git);
}

#[test]
fn tabu_auch_in_anderer_schreibweise_und_ohne_die_platte() {
    let p = Platz::neu("tabu");
    let r = p.regeln();
    assert_eq!(ab(&p.text(&p.profil.join("Geheim Ablage").join("brief.pdf")), &r), Abweisung::Tabu);
    assert_eq!(ab(&p.text(&p.profil.join("Geheim Ablage")), &r), Abweisung::Tabu, "der Ordner selbst");
    // Andere Schreibweisen; es gibt sie nicht, trotzdem Tabu (nicht „fehlt“): geprüft wird vor der Platte.
    for anders in ["GEHEIM ABLAGE", "geheim_ablage", "Geheim-Ablage", "geheimablage", "alt Geheim  Ablage 2"] {
        assert_eq!(ab(&p.text(&p.profil.join(anders).join("x.pdf")), &r), Abweisung::Tabu, "{anders}");
    }
    assert_eq!(ab(&p.text(&p.profil.join("Dokumente").join("geheim ablage.pdf")), &r), Abweisung::Tabu, "auch der Dateiname");
    if p.junctions || !crate::ueberspringen("mklink /J ging nicht") {
        // Harmloser Name, Junction in den Tabu-Ordner: nach dem Auflösen Tabu.
        assert_eq!(ab(&p.text(&p.profil.join("harmlos").join("brief.pdf")), &r), Abweisung::Tabu);
    }
    // Ohne Liste: nichts (Server und Bote); `pruefe` prüft dann nur den Rest.
    let ohne = Regeln { tabu: None, ..p.regeln() };
    assert_eq!(ab(&p.dok("bauplan.pdf"), &ohne), Abweisung::KeineListe);
    let pruefe = Regeln { tabu: None, tabu_pflicht: false, ..p.regeln() };
    assert!(pruefen(&p.dok("bauplan.pdf"), &pruefe).is_ok());
}

#[test]
fn wurzeln_junctions_appdata_dienst() {
    let p = Platz::neu("wurzeln");
    let r = p.regeln();
    assert_eq!(ab(&p.text(&p.draussen.join("fremd.pdf")), &r), Abweisung::AusserhalbWurzel);
    assert_eq!(ab(&p.text(&p.profil.join("AppData").join("Local").join("x.pdf")), &r), Abweisung::AppData);
    assert_eq!(ab(&p.text(&p.profil.join("appdata").join("local").join("x.pdf")), &r), Abweisung::AppData, "klein geschrieben");
    assert_eq!(ab(&p.text(&p.dienst.join("x.txt")), &r), Abweisung::Dienst);
    assert_eq!(ab(&p.dok("gibt-es-nicht.pdf"), &r), Abweisung::Fehlt);
    if p.junctions || !crate::ueberspringen("mklink /J ging nicht") {
        assert_eq!(ab(&p.text(&p.profil.join("sprung").join("fremd.pdf")), &r), Abweisung::AusserhalbWurzel, "Junction hinaus");
        assert_eq!(ab(&p.text(&p.profil.join("sprung")), &r), Abweisung::AusserhalbWurzel, "Junction hinaus (Ordner)");
    }
    // Ohne Profil und Quellen ist nichts erlaubt.
    let leer = Regeln { profil: None, quellen: Vec::new(), ..p.regeln() };
    assert_eq!(ab(&p.dok("bauplan.pdf"), &leer), Abweisung::AusserhalbWurzel);
    // Vergleich der Ordner: Grenze an einem Trennzeichen, nicht mitten im Namen.
    assert!(unter(r"c:\users\a\x", r"c:\users\a"));
    assert!(unter(r"c:\users\a", r"c:\users\a\"));
    assert!(!unter(r"c:\users\beispiel1\x", r"c:\users\beispiel"));
    // ProgramData wird hier nicht berührt (Regel der Tests); nur der Vergleich.
    let pd = kanonisch_text(&programdata()).to_lowercase();
    assert!(unter(&format!(r"{pd}\dashboard\dienst.toml"), &pd));
}

#[test]
fn harte_verknuepfung_und_komma() {
    let p = Platz::neu("hart");
    let r = p.regeln();
    std::fs::hard_link(p.profil.join("Dokumente").join("harte.pdf"), p.profil.join("Dokumente").join("zweiter-name.pdf")).unwrap();
    assert_eq!(ab(&p.dok("harte.pdf"), &r), Abweisung::MehrereNamen);
    assert_eq!(ab(&p.dok("komma,ordner"), &r), Abweisung::Komma);
}

#[test]
fn saetze_nennen_keinen_pfad_und_keinen_tabu_namen() {
    let alle = [
        Abweisung::Leer,
        Abweisung::NichtAbsolut,
        Abweisung::Netz,
        Abweisung::Geraet,
        Abweisung::Datenstrom,
        Abweisung::Elternverweis,
        Abweisung::Zeichen,
        Abweisung::Kurzname,
        Abweisung::Git,
        Abweisung::Tabu,
        Abweisung::KeineListe,
        Abweisung::AppData,
        Abweisung::ProgramData,
        Abweisung::Dienst,
        Abweisung::AusserhalbWurzel,
        Abweisung::Fehlt,
        Abweisung::Unlesbar,
        Abweisung::OhneEndung,
        Abweisung::Endung("exe".into()),
        Abweisung::Komma,
        Abweisung::MehrereNamen,
        Abweisung::Programm("md".into()),
        Abweisung::Programm("appref-ms".into()),
        Abweisung::Shellordner,
        Abweisung::NichtVomDienst,
        Abweisung::ZuViele,
    ];
    for a in alle {
        let s = a.satz();
        assert!(!s.is_empty() && !s.to_lowercase().contains(TABU) && !s.contains(r"\Users"), "{a:?}: {s}");
    }
}

#[test]
fn listen_wie_in_der_entscheidung() {
    let positiv = ["pdf", "docx", "xlsx", "pptx", "odt", "ods", "odp", "txt", "csv", "md", "png", "jpg", "jpeg", "gif", "webp", "svg"];
    assert_eq!(ENDUNGEN, positiv);
    assert_eq!(crate::bote::oeffnen::ENDUNGEN, positiv, "die eigene Liste des Boten");
    assert_eq!(EXPLORER_ENDUNGEN, ["zip"]);
    assert_eq!(crate::bote::oeffnen::EXPLORER_ENDUNGEN, ["zip"]);
}

#[test]
fn grenze_zehn_je_minute() {
    let g = Grenze::default();
    let t = Instant::now();
    for i in 0..JE_MINUTE {
        assert!(g.nehmen(t + Duration::from_secs(i as u64)), "{i}");
    }
    assert!(!g.nehmen(t + Duration::from_secs(30)), "die elfte in der Minute");
    assert!(g.nehmen(t + Duration::from_secs(61)), "eine Minute nach der ersten wieder frei");
}

#[test]
fn ausschluss_lose_schreibweisen() {
    let a = Ausschluss::aus(&[TABU]).unwrap();
    for n in ["Geheim Ablage", "GEHEIM_ABLAGE", "geheim-ablage", "x geheimablage y", "Geheim.Ablage"] {
        assert!(a.verboten_lose(n), "{n}");
    }
    assert!(!a.verboten_lose("Ablage geheim"));
    assert!(!a.verboten_lose("Dokumente"));
}

/// `pruefe` meldet Orte, die gegen die Regeln verstoßen, schon beim Bau:
/// Fehler mit Kennung und Zeile (nie der Pfad). Ob es einen Ort gibt, prüft
/// nur der Bote: ein fehlender Ort ist kein Befund.
#[test]
fn pruefe_meldet_orte_beim_bau() {
    use crate::inhalt::pruefung::{Schwere, orte_befunde, pruefe_projekt};
    use crate::vertrag::daten::ProjektDaten;
    let p = Platz::neu("pruefe");
    let mut text = String::new();
    let mut ort = |k: &str, pfad: &str| text += &format!("[[ort]]\nkennung = \"{k}\"\ntitel = \"{k}\"\npfad = '{pfad}'\n\n");
    ort("gut", &p.dok("bauplan.pdf"));
    ort("ordner", &p.dok("Werkzeugkiste"));
    ort("exe", &p.dok("datei.exe"));
    ort("exe-fehlt", &p.dok("gibt-es-nicht.exe"));
    ort("unc", r"\\server\freigabe\x.pdf");
    ort("tabu", &p.text(&p.profil.join("Geheim Ablage").join("brief.pdf")));
    ort("appdata", &p.text(&p.profil.join("AppData").join("Local").join("x.pdf")));
    ort("fehlt", &p.dok("gibt-es-nicht.pdf"));
    let mut d = ProjektDaten::default();
    d.datei_einlesen("orte.toml", "projects/probe/daten/orte.toml", &text).unwrap();
    let mut befunde = Vec::new();
    orte_befunde(d.orte.as_ref().unwrap(), &text, "projects/probe/daten/orte.toml", &p.regeln(), &mut befunde);
    let fehler: Vec<&str> = befunde.iter().filter(|b| b.schwere == Schwere::Fehler).map(|b| b.text.as_str()).collect();
    for k in ["exe", "exe-fehlt", "unc", "tabu", "appdata"] {
        assert!(fehler.iter().any(|t| t.contains(&format!("„{k}“"))), "{k}: {befunde:?}");
    }
    assert_eq!(fehler.len(), 5, "{befunde:?}");
    assert!(befunde.iter().all(|b| b.schwere == Schwere::Fehler), "kein Hinweis für den fehlenden Ort: {befunde:?}");
    for b in &befunde {
        assert!(b.zeile.is_some(), "{b:?}");
        assert!(!b.text.contains(r"\Users") && !b.text.contains("Dokumente") && !b.text.to_lowercase().contains("geheim"), "{b:?}");
    }
    // Über `pruefe <slug>`: dieselbe Datei in einem Projekt; die Form-Fehler kommen auch ohne Tabu-Liste.
    let projekte = p.wurzel.join("projekte");
    std::fs::create_dir_all(projekte.join("probe").join("daten")).unwrap();
    std::fs::write(projekte.join("probe").join("projekt.toml"), "schema = 3\ntitle = \"Probe\"\nstatus = \"aktiv\"\nsummary = \"Für den Test.\"\n").unwrap();
    std::fs::write(projekte.join("probe").join("daten").join("orte.toml"), &text).unwrap();
    let alle = pruefe_projekt(&projekte, "probe");
    let orte: Vec<_> = alle.iter().filter(|b| b.datei.ends_with("orte.toml")).collect();
    for k in ["exe", "exe-fehlt", "unc"] {
        assert!(orte.iter().any(|b| b.schwere == Schwere::Fehler && b.text.contains(&format!("„{k}“"))), "{k}: {orte:?}");
    }
}

/// Abnahme 1: Die eingebauten Tabu-Namen gelten auch, wenn die Liste im
/// Inhalt leer ist (dann geht ohnehin nichts auf) oder andere Namen trägt.
/// Geprüft mit einem erfundenen eingebauten Namen; die echten stehen nur als
/// Prüfsumme im Code.
#[test]
fn eingebaute_tabu_namen_auch_mit_leerer_oder_geaenderter_liste() {
    let p = Platz::neu("eingebaut");
    std::fs::create_dir_all(p.profil.join("Alte Truhe")).unwrap();
    std::fs::write(p.profil.join("Alte Truhe").join("brief.pdf"), "x").unwrap();
    let eingebaut = vec![tabu_pruefsumme("alte truhe")];
    // Liste im Inhalt geändert: nur noch ein anderer Name.
    let geaendert = Regeln { eingebaut: eingebaut.clone(), tabu: Ausschluss::aus(&["ganz anderes"]), ..p.regeln() };
    for schreibweise in ["Alte Truhe", "ALTE_TRUHE", "alte-truhe", "x altetruhe 2"] {
        assert_eq!(ab(&p.text(&p.profil.join(schreibweise).join("brief.pdf")), &geaendert), Abweisung::Tabu, "{schreibweise}");
    }
    assert!(pruefen(&p.dok("bauplan.pdf"), &geaendert).is_ok(), "sonst geht es");
    // Liste leer (lädt nicht): Server und Bote öffnen nichts; `pruefe` meldet den eingebauten Namen trotzdem.
    let leer = std::env::temp_dir().join(format!("dashboard-oeffnen-leer-{}.toml", std::process::id()));
    std::fs::write(&leer, "nie_betreten = []\n").unwrap();
    let tabu = crate::pc::inventory::ausschluss_laden(Some(&leer)).ok();
    assert!(tabu.is_none());
    let ohne = Regeln { eingebaut: eingebaut.clone(), tabu, ..p.regeln() };
    assert_eq!(ab(&p.dok("bauplan.pdf"), &ohne), Abweisung::KeineListe);
    let beim_bau = Regeln { tabu_pflicht: false, ..ohne };
    assert_eq!(ab(&p.text(&p.profil.join("Alte Truhe").join("brief.pdf")), &beim_bau), Abweisung::Tabu);
    let _ = std::fs::remove_file(&leer);
    // Die echten: zwei, nur als Länge und SHA-256 (64 Zeichen hex), im Standard der Regeln.
    assert_eq!(EINGEBAUTE_TABU.len(), 2);
    for (n, h) in EINGEBAUTE_TABU {
        assert!(n > 0 && h.len() == 64 && h.chars().all(|c| c.is_ascii_hexdigit()));
    }
    assert_eq!(Regeln::default().eingebaut.len(), 2);
    assert_eq!(tabu_pruefsumme("Alte-Truhe"), tabu_pruefsumme("alte truhe"));
}

/// Abnahme 2: kein `%` (Umgebungsvariablen) und keine Shell-Ordner `.{GUID}`.
#[test]
fn prozent_und_shellordner() {
    let r = Regeln::default();
    assert_eq!(form_pruefen(r"C:\Users\beispiel\%USERPROFILE%\x.pdf"), Err(Abweisung::Zeichen));
    assert_eq!(form_pruefen(r"C:\Users\beispiel\100%.pdf"), Err(Abweisung::Zeichen));
    assert_eq!(form_pruefen(r"C:\Users\beispiel\Ablage.{20D04FE0-3AEA-1069-A2D8-08002B30309D}"), Err(Abweisung::Shellordner));
    assert_eq!(form_pruefen(r"C:\Users\beispiel\Ablage.{645ff040-5081-101b-9f08-00aa002f954e}\x.pdf"), Err(Abweisung::Shellordner));
    assert!(form_pruefen(r"C:\Users\beispiel\Notizen {alt}\x.pdf").is_ok());
    let _ = r;
}

/// Abnahme 6: Gesperrte Ordner, die sich nicht auflösen lassen (es gibt sie
/// nicht, oder das Konto darf sie nicht lesen), gelten als Text weiter.
#[test]
fn gesperrte_ordner_auch_als_text() {
    let p = Platz::neu("text");
    std::fs::remove_dir_all(p.profil.join("AppData")).unwrap();
    let r = Regeln { verboten: vec![p.profil.join("gibt-es-nicht-dienst")], ..p.regeln() };
    // AppData fehlt (wie für ein Konto, das es nicht lesen darf): trotzdem gesperrt.
    assert_eq!(vorpruefen(&p.text(&p.profil.join("AppData").join("x.pdf")), &r), Err(Abweisung::AppData));
    assert_eq!(vorpruefen(&p.text(&p.profil.join("appdata").join("x.pdf")), &r), Err(Abweisung::AppData));
    assert_eq!(vorpruefen(&p.text(&p.profil.join("gibt-es-nicht-dienst").join("x.txt")), &r), Err(Abweisung::Dienst));
    // Eine Wurzel, die sich nicht auflösen lässt, gilt als Text.
    let wurzel = Regeln { profil: Some(PathBuf::from(r"C:\Users\beispiel-gibt-es-nicht")), quellen: Vec::new(), ..p.regeln() };
    assert!(vorpruefen(r"C:\Users\beispiel-gibt-es-nicht\Dokumente\x.pdf", &wurzel).is_ok());
    assert_eq!(vorpruefen(r"C:\Users\beispiel-anders\x.pdf", &wurzel), Err(Abweisung::AusserhalbWurzel));
}

/// Abnahme 7: Lage und Tabu vor jedem Blick auf die Platte; „gibt es nicht“,
/// „liegt außerhalb“ und „Tabu“ haben denselben Satz. Eine Endung steht nur
/// im Satz, wenn sie kurz ist und nur Buchstaben und Ziffern hat.
#[test]
fn kein_orakel_fuer_vorhandenes() {
    let p = Platz::neu("orakel");
    let r = p.regeln();
    // Außerhalb und Tabu werden abgewiesen, bevor geprüft wird, ob es den Ort gibt.
    assert_eq!(vorpruefen(&p.text(&p.draussen.join("fremd.pdf")), &r), Err(Abweisung::AusserhalbWurzel));
    assert_eq!(vorpruefen(&p.text(&p.draussen.join("gibt-es-nicht.pdf")), &r), Err(Abweisung::AusserhalbWurzel));
    assert_eq!(vorpruefen(&p.text(&p.profil.join("Geheim Ablage").join("gibt-es-nicht.pdf")), &r), Err(Abweisung::Tabu));
    let fehlt = Abweisung::Fehlt.satz();
    assert_eq!(Abweisung::AusserhalbWurzel.satz(), fehlt);
    assert_eq!(Abweisung::Tabu.satz(), fehlt);
    assert_eq!(fehlt, SATZ_NICHT_DA);
    assert!(Abweisung::Endung("exe".into()).satz().contains(".exe"));
    assert!(!Abweisung::Endung("appref-ms".into()).satz().contains("appref"));
    assert!(!Abweisung::Endung("x<script>".into()).satz().contains("script"));
    assert!(!Abweisung::Endung("sehrlangeendung".into()).satz().contains("sehrlange"));
}

/// Abnahme 5: Ohne USERPROFILE nie „.“ als Wurzel; im Dienst ohne
/// inhaber_sid kein Profil (nie das des Dienstkontos).
#[test]
fn profil_nie_punkt_und_nie_das_des_dienstes() {
    if let Some(p) = eigenes_profil() {
        assert!(p.is_absolute());
    }
    assert_eq!(Regeln::des_servers(Some(PathBuf::from(".")), Vec::new(), Vec::new(), None).profil, None);
    let ordner = std::env::temp_dir().join(format!("dashboard-oeffnen-einst-{}", std::process::id()));
    std::fs::create_dir_all(ordner.join("repo")).unwrap();
    let p = |r: &str| ordner.join(r).display().to_string().replace('\\', "/");
    let text = |sid: &str| {
        format!(
            "schema = 1\nrepo = \"{}\"\nport = 8787\n\n[pfade]\ndaten = \"{}\"\nsicherung = \"{}\"\nprotokoll = \"{}\"\n\n[konten]\ninhaber_sid = \"{sid}\"\n\n[bote]\npipe = \"dashboard-bote\"\nprogramm = \"{}\"\n",
            p("repo"),
            p("daten/dashboard.redb"),
            p("sicherung"),
            p("protokoll"),
            p("bin/dashboard-bote.exe"),
        )
    };
    let datei = ordner.join("dienst.toml");
    let ohne_sid = crate::dienst::einstellungen::Einstellungen::aus_text(&datei, &text("")).unwrap();
    assert_eq!(inhaber_profil(Some(&ohne_sid)), None);
    // Eine SID, die es auf diesem PC nicht gibt: auch kein Profil.
    let fremd = crate::dienst::einstellungen::Einstellungen::aus_text(&datei, &text("S-1-5-21-1111111111-2222222222-3333333333-1999")).unwrap();
    assert_eq!(inhaber_profil(Some(&fremd)), None);
    // Die eigene SID: das eigene Profil aus der Registry.
    #[cfg(windows)]
    {
        let ich = crate::bote::win::eigene_sid().unwrap();
        let eigen = crate::dienst::einstellungen::Einstellungen::aus_text(&datei, &text(&ich)).unwrap();
        let profil = inhaber_profil(Some(&eigen)).expect("eigenes Profil aus ProfileList");
        assert!(profil.is_absolute());
        if let Some(e) = eigenes_profil() {
            assert_eq!(profil.display().to_string().to_lowercase(), e.display().to_string().to_lowercase());
        }
    }
    let _ = std::fs::remove_dir_all(&ordner);
}

/// Die Prüfung im Dienst lief unter einem Konto, das das
/// Profil des Inhabers nicht lesen darf, und meldete fehlende Orte als Fehler
/// („Probe-Orte fehlen“). Beim Bau zählt nur der Text: Ein Ort im Profil des
/// Inhabers, den es (hier) nicht gibt oder der sich nicht lesen lässt, ist kein
/// Befund; Form, Endung, Tabu und Lage als Text bleiben Fehler. Ohne bekanntes
/// Profil entfällt nur die Lage, mit einem Hinweis.
#[test]
fn orte_beim_bau_nur_als_text_ohne_blick_auf_die_platte() {
    use crate::inhalt::pruefung::{Schwere, orte_befunde};
    use crate::vertrag::daten::ProjektDaten;
    let p = Platz::neu("nur-text");
    // Ein Profil, das es auf der Platte nicht gibt: wie das Profil des Inhabers aus Sicht des Dienstkontos.
    let fremd = p.wurzel.join("Benutzer").join("inhaber");
    let mut text = String::new();
    let mut ort = |k: &str, pfad: &str| text += &format!("[[ort]]\nkennung = \"{k}\"\ntitel = \"{k}\"\npfad = '{pfad}'\n\n");
    ort("unsichtbar", &p.text(&fremd.join("Dokumente").join("brief.pdf")));
    ort("unsichtbarer-ordner", &p.text(&fremd.join("Projekte")));
    ort("quelle", &p.text(&p.quelle.join("gibt-es-nicht.md")));
    ort("draussen", &p.text(&p.draussen.join("fremd.pdf")));
    ort("appdata", &p.text(&fremd.join("AppData").join("x.pdf")));
    ort("tabu", &p.text(&fremd.join("Geheim Ablage").join("brief.pdf")));
    ort("punkte", &p.text(&fremd.join("..").join("x.pdf")));
    ort("skript", &p.text(&fremd.join("start.ps1")));
    let mut d = ProjektDaten::default();
    d.datei_einlesen("orte.toml", "projects/probe/daten/orte.toml", &text).unwrap();
    let pruefen_mit = |profil: Option<PathBuf>| {
        let regeln = Regeln { tabu_pflicht: false, profil, ..p.regeln() };
        let mut befunde = Vec::new();
        orte_befunde(d.orte.as_ref().unwrap(), &text, "projects/probe/daten/orte.toml", &regeln, &mut befunde);
        befunde
    };
    let fehler_von = |befunde: &[crate::inhalt::pruefung::Befund]| -> Vec<String> {
        let mut k: Vec<String> = befunde
            .iter()
            .filter(|b| b.schwere == Schwere::Fehler)
            .map(|b| b.text.split('„').nth(1).and_then(|r| r.split('“').next()).unwrap_or("").to_string())
            .collect();
        k.sort();
        k
    };
    // Profil des Inhabers bekannt (im Dienst über die SID), aber nicht lesbar.
    let mit = pruefen_mit(Some(fremd.clone()));
    assert_eq!(fehler_von(&mit), ["appdata", "draussen", "punkte", "skript", "tabu"], "{mit:?}");
    assert!(mit.iter().all(|b| b.schwere == Schwere::Fehler), "{mit:?}");
    // Profil unbekannt: die Lage entfällt („draussen“ ist dann kein Fehler), ein Hinweis sagt es.
    let ohne = pruefen_mit(None);
    assert_eq!(fehler_von(&ohne), ["punkte", "skript", "tabu"], "{ohne:?}");
    let hinweise: Vec<_> = ohne.iter().filter(|b| b.schwere == Schwere::Hinweis).collect();
    assert_eq!(hinweise.len(), 1, "{ohne:?}");
    assert!(hinweise[0].text.contains("Profil des Inhabers"), "{hinweise:?}");
    for b in mit.iter().chain(&ohne) {
        assert!(!b.text.contains("inhaber") && !b.text.contains("Dokumente"), "kein Pfad im Satz: {b:?}");
    }
}

/// Auftrag der Architektur: `pruefe` misst Dateien mit Endung an
/// der Positivliste (wie der Bote), nicht nur an der Sperrliste. Ein Ordner
/// mit Punkt im Namen geht mit `\` am Ende; `.ssh` (Punkt vorn) hat keine
/// Endung. Ohne Profil sagt der Hinweis, dass dann kein Ort aufgeht.
#[test]
fn orte_beim_bau_mit_positivliste_und_ehrlichem_hinweis() {
    use crate::inhalt::pruefung::{SATZ_ORDNER_MIT_PUNKT, SATZ_ORTE_OHNE_PROFIL, Schwere, ort_endung, orte_befunde};
    use crate::vertrag::daten::ProjektDaten;
    assert_eq!(ort_endung(r"C:\x\Brief.PDF"), Some("pdf".into()));
    assert_eq!(ort_endung(r"C:\x\ordner.v2\"), None);
    assert_eq!(ort_endung(r"C:\x\.ssh"), None);
    assert_eq!(ort_endung(r"C:\x\ohne"), None);
    assert_eq!(ort_endung(r"C:\x\Fotos_etc._"), None, "kein Endungs-Muster: ein Ordnername");
    assert_eq!(ort_endung(r"C:\x\Fassung 1.0"), None, "nur Ziffern: keine Endung");
    assert_eq!(ort_endung(r"C:\x\a.appref-ms"), Some("appref-ms".into()));
    let p = Platz::neu("positiv");
    let mut text = String::new();
    let mut ort = |k: &str, pfad: &str| text += &format!("[[ort]]\nkennung = \"{k}\"\ntitel = \"{k}\"\npfad = '{pfad}'\n\n");
    ort("pdf", &p.dok("bauplan.pdf"));
    ort("zip", &p.dok("archiv.zip"));
    ort("ordner", &p.dok("Werkzeugkiste"));
    ort("punkt-ordner", &(p.dok("ordner.v2") + "\\"));
    ort("punkt-vorn", &p.text(&p.profil.join(".ssh")));
    ort("rtf", &p.dok("notiz.rtf"));
    ort("html", &p.dok("seite.html"));
    ort("exe", &p.dok("datei.exe"));
    let mut d = ProjektDaten::default();
    d.datei_einlesen("orte.toml", "projects/probe/daten/orte.toml", &text).unwrap();
    let pruefen_mit = |profil: Option<PathBuf>| {
        let regeln = Regeln { profil, ..p.regeln() };
        let mut befunde = Vec::new();
        orte_befunde(d.orte.as_ref().unwrap(), &text, "projects/probe/daten/orte.toml", &regeln, &mut befunde);
        befunde
    };
    let mit = pruefen_mit(Some(p.profil.clone()));
    let fehler: Vec<&str> = mit.iter().filter(|b| b.schwere == Schwere::Fehler).map(|b| b.text.as_str()).collect();
    assert_eq!(fehler.len(), 3, "{mit:#?}");
    for k in ["rtf", "html"] {
        let t = fehler.iter().find(|t| t.contains(&format!("„{k}“"))).unwrap_or_else(|| panic!("{k}: {mit:#?}"));
        assert!(t.contains(&format!("der Endung .{k}")) && t.ends_with(SATZ_ORDNER_MIT_PUNKT), "{t}");
    }
    let exe = fehler.iter().find(|t| t.contains("„exe“")).unwrap();
    assert!(!exe.contains(SATZ_ORDNER_MIT_PUNKT), "{exe}");
    // Ohne Profil: derselbe Endungs-Fehler, dazu der ehrliche Hinweis.
    let ohne = pruefen_mit(None);
    let hinweise: Vec<&str> = ohne.iter().filter(|b| b.schwere == Schwere::Hinweis).map(|b| b.text.as_str()).collect();
    assert_eq!(hinweise, [SATZ_ORTE_OHNE_PROFIL], "{ohne:#?}");
    assert!(
        SATZ_ORTE_OHNE_PROFIL.contains("kein Ort geht auf, auch keiner in deinem Profil")
            || SATZ_ORTE_OHNE_PROFIL.contains("geht kein Ort auf, auch keiner in deinem Profil")
    );
    assert_eq!(ohne.iter().filter(|b| b.schwere == Schwere::Fehler).count(), 3, "{ohne:#?}");
}

/// Die Prüfung beim Bau bekommt die Ordner
/// des Dienstes aus den Einstellungen (wie Server und Bote). Ohne gesetzte
/// Einstellungen (Konsole, Tests) nur das Programm; mit Einstellungen auch
/// Sicherung, Protokoll, Datenspeicher, Oberfläche und Bote. Ein Ort dort
/// ist ein Fehler, auch wenn er im Profil liegt.
#[test]
fn ordner_des_dienstes_beim_bau_aus_den_einstellungen() {
    use crate::inhalt::pruefung::{Schwere, orte_befunde};
    use crate::vertrag::daten::ProjektDaten;
    assert_eq!(ordner_fuer_pruefung(), ordner_des_dienstes(None));
    let p = Platz::neu("dienst-einst");
    std::fs::create_dir_all(p.profil.join("repo")).unwrap();
    let ort_text = |r: &str| p.text(&p.profil.join(r)).replace('\\', "/");
    let einst = format!(
        "schema = 1\nrepo = \"{}\"\nport = 8787\n\n[pfade]\ndaten = \"{}\"\nsicherung = \"{}\"\nprotokoll = \"{}\"\n\n[konten]\ninhaber_sid = \"\"\n\n[bote]\npipe = \"dashboard-bote\"\nprogramm = \"{}\"\n",
        ort_text("repo"),
        ort_text("dienstdaten/dashboard.redb"),
        ort_text("sicherung"),
        ort_text("protokoll"),
        ort_text("bin/dashboard-bote.exe"),
    );
    let e = crate::dienst::einstellungen::Einstellungen::aus_text(&p.wurzel.join("dienst.toml"), &einst).unwrap();
    let verboten = ordner_des_dienstes(Some(&e));
    let mut text = String::new();
    for (k, r) in [("sicherung", r"sicherung\stand.txt"), ("protokoll", r"protokoll\heute.txt"), ("daten", r"dienstdaten\x.txt"), ("bote", r"bin\liesmich.txt")]
    {
        text += &format!("[[ort]]\nkennung = \"{k}\"\ntitel = \"{k}\"\npfad = '{}'\n\n", p.text(&p.profil.join(r)));
    }
    let mut d = ProjektDaten::default();
    d.datei_einlesen("orte.toml", "projects/probe/daten/orte.toml", &text).unwrap();
    let mit_regeln = |verboten: Vec<PathBuf>| {
        let regeln = Regeln { verboten, ..p.regeln() };
        let mut b = Vec::new();
        orte_befunde(d.orte.as_ref().unwrap(), &text, "projects/probe/daten/orte.toml", &regeln, &mut b);
        b
    };
    let ohne = mit_regeln(ordner_des_dienstes(None));
    assert!(ohne.is_empty(), "ohne Einstellungen kennt pruefe diese Ordner nicht: {ohne:#?}");
    let mit = mit_regeln(verboten);
    assert_eq!(mit.iter().filter(|b| b.schwere == Schwere::Fehler && b.text.contains("Ordner des Dienstes")).count(), 4, "{mit:#?}");
}
