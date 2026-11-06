//! Bau-Stand je Projekt (Etappe 4c): grün, rot, nicht geprüft, läuft oder noch
//! nicht gebaut, dazu die Zeit der letzten guten Fassung und bei rot und nicht
//! geprüft ein Satz in einfachem Deutsch.
//!
//! Woher: Der Bote merkt sich je Projekt, was er zuletzt geprüft hat
//! (`web/.projekte-gruen/stand.json`, `bote::bau::Stand`), und legt die
//! letzte grüne Kopie unter `web/.projekte-gruen/<slug>/` ab. Der Dienst liest
//! beides nur. „Läuft“ kommt aus der Warteschlange des Baumeisters (bestellt
//! oder gerade im Bau).
//!
//! Was frei ist und was geschützt (Plan 4 sagt dazu nichts; Auslegung des
//! Betreuers des Dashboards): Zustand und Zeit stehen frei in `/api/projects` (Feld
//! `bau`), wie Status und Git-Stand. Der Satz bei rot und bei nicht geprüft steht nur geschützt
//! unter `GET /api/bauen`, wie das Ereignis `bau` nur im geschützten Kanal.
//! Der Satz nennt nie Pfade, Dateien oder Prüfwerkzeuge; die Einzelheiten
//! sieht der Chat mit `npm run pruefe -- <slug>`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, TimeZone};

use crate::app::AppState;
use crate::bote::bau::{GRUEN_ORDNER, ProjektStand, RotArt, STAND_DATEI, Stand};
use crate::inhalt::seite;
use crate::vertrag::api::{ProjektBau, ProjektBauSatz, SeitenBau};

/// Satz bei rot, wenn die letzte gute Fassung zu sehen ist.
pub const SATZ_ROT_MIT_FASSUNG: &str =
    "Die neue Fassung der Seite hat einen Fehler. Bis der Chat des Projekts ihn behebt, bleibt die letzte gute Fassung stehen.";
/// Satz bei rot ohne gute Fassung.
pub const SATZ_ROT_OHNE_FASSUNG: &str = "Die Seite hat einen Fehler und ist darum gerade nicht zu sehen. Der Chat des Projekts kann ihn beheben.";
/// Satz bei rot nach der Gesamtprüfung, gebaut aus der letzten grünen Kopie.
pub const SATZ_PASST_NICHT_MIT_FASSUNG: &str =
    "Die neue Fassung der Seite passt nicht zum Rest des Dashboards. Bis der Chat des Projekts das behebt, bleibt die letzte gute Fassung stehen.";
/// Satz bei rot nach der Gesamtprüfung, ganz ausgeschlossen (die Seite fehlt im Bau).
pub const SATZ_PASST_NICHT_OHNE_FASSUNG: &str =
    "Die Seite passt nicht zum Rest des Dashboards und ist darum gerade nicht zu sehen. Der Chat des Projekts kann das beheben.";

/// Zweig A: Ein Prüfwerkzeug lief nicht, die neue Fassung ist nicht geprüft;
/// zu sehen ist die letzte gute Fassung. Kein Rot (Zustand `nicht_geprueft`).
pub const SATZ_NICHT_GEPRUEFT_MIT_FASSUNG: &str =
    "Zu sehen ist die letzte geprüfte Fassung; die neue Fassung ist noch nicht geprüft. Der nächste Bau prüft sie noch einmal.";
/// Zweig A: nicht geprüft und ohne gute Fassung (Zustand `nicht_geprueft`, ohne `letzte_gute`).
pub const SATZ_NICHT_GEPRUEFT_OHNE_FASSUNG: &str = "Die Seite ist noch nicht geprüft und darum noch nicht zu sehen. Der nächste Bau prüft sie noch einmal.";

/// Der Bau-Stand eines Projekts, mit Satz (nur für den geschützten Weg).
#[derive(Debug, Clone, PartialEq)]
pub struct Eintrag {
    pub zustand: SeitenBau,
    pub letzte_gute: Option<DateTime<Local>>,
    /// Wann die Seite zuletzt in einen geltenden Bau kam (`gebaut_um`), nur
    /// wenn sie gerade zu sehen ist; ältere Stände: die Zeit der letzten guten Fassung.
    pub gebaut: Option<DateTime<Local>>,
    pub satz: Option<String>,
}

impl Eintrag {
    /// Für `/api/projects` (frei): ohne Satz.
    pub fn frei(&self) -> ProjektBau {
        ProjektBau { zustand: self.zustand, letzte_gute: self.letzte_gute, gebaut_um: self.gebaut }
    }

    /// Für `GET /api/bauen` (geschützt).
    pub fn mit_satz(&self, slug: &str) -> ProjektBauSatz {
        ProjektBauSatz { slug: slug.to_string(), zustand: self.zustand, letzte_gute: self.letzte_gute, gebaut_um: self.gebaut, satz: self.satz.clone() }
    }
}

/// `web/.projekte-gruen` neben `web/build` (dort schreibt der Bote).
pub fn gruen_ordner(state: &AppState) -> PathBuf {
    state.web.build().parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("web")).join(GRUEN_ORDNER)
}

/// Hat das Projekt eine eigene Seite (`Seite.svelte`, keine Verknüpfung)? Wie
/// der Bote beim Bau (`bote::bau`): nur solche kommen in die Bauliste.
pub fn hat_seite(eltern: &Path, slug: &str) -> bool {
    let datei = eltern.join(slug).join("Seite.svelte");
    std::fs::symlink_metadata(&datei).is_ok_and(|m| m.is_file()) && !seite::ist_verknuepfung(&eltern.join(slug))
}

/// Die Projekte mit eigener Seite aus allen Inhaltsquellen.
pub fn mit_seite(state: &AppState) -> BTreeSet<String> {
    state.alle_quellen().projekte().iter().filter(|(s, e)| hat_seite(e, s)).map(|(s, _)| s.clone()).collect()
}

/// Liest `stand.json`. `Ok(None)`: Es gibt die Datei nicht (der Baumeister lief
/// hier noch nie). `Err`: nicht lesbar oder kaputt; dann zeigt die Oberfläche
/// „Bau-Stand gerade nicht lesbar“ statt eines falschen Stands. Ein kurzes
/// Scheitern, während der Bote die Datei ersetzt, fängt ein zweiter und
/// dritter Versuch ab (je 30 ms später).
pub fn stand_lesen(gruen: &Path) -> Result<Option<Stand>, String> {
    let mut grund = String::new();
    for versuch in 0..3 {
        if versuch > 0 {
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        match std::fs::read_to_string(gruen.join(STAND_DATEI)) {
            Ok(t) => match serde_json::from_str(&t) {
                Ok(s) => return Ok(Some(s)),
                Err(e) => grund = format!("{STAND_DATEI} ist nicht lesbar ({e})"),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => grund = format!("{STAND_DATEI} ist nicht lesbar ({e})"),
        }
    }
    Err(grund)
}

/// Die grüne Kopie, wenn es sie gibt (Ordner mit `Seite.svelte`, keine Verknüpfung).
fn kopie(gruen: &Path, slug: &str) -> Option<PathBuf> {
    let o = gruen.join(slug);
    (o.join("Seite.svelte").is_file() && !seite::ist_verknuepfung(&o)).then_some(o)
}

/// Zeit der letzten guten Fassung: aus `stand.json`, sonst die Änderungszeit
/// der Kopie (Stände von vor 4c).
fn zeit(p: Option<&ProjektStand>, kopie: Option<&Path>) -> Option<DateTime<Local>> {
    let kopie = kopie?;
    if let Some(s) = p.and_then(|p| p.gruen_um)
        && let Some(t) = Local.timestamp_opt(s, 0).single()
    {
        return Some(t);
    }
    std::fs::metadata(kopie).and_then(|m| m.modified()).ok().map(DateTime::<Local>::from)
}

/// Der Satz bei rot, ohne Pfade. `mit_fassung`: Die letzte gute Fassung
/// steht im Bau (es gibt eine Kopie, und das Projekt ist nicht ganz ausgeschlossen).
pub fn satz_rot(art: Option<RotArt>, mit_fassung: bool) -> &'static str {
    match (art, mit_fassung) {
        (Some(RotArt::Ausgeschlossen), _) => SATZ_PASST_NICHT_OHNE_FASSUNG,
        (Some(RotArt::Zusammenbau), true) => SATZ_PASST_NICHT_MIT_FASSUNG,
        (Some(RotArt::Zusammenbau), false) => SATZ_PASST_NICHT_OHNE_FASSUNG,
        (_, true) => SATZ_ROT_MIT_FASSUNG,
        (_, false) => SATZ_ROT_OHNE_FASSUNG,
    }
}

/// Der Bau-Stand je Projekt aus `slugs`. `stand`: `None`, wenn es `stand.json`
/// nicht gibt. `im_bau`: bestellt oder gerade im Bau (Baumeister).
pub fn berechnen<'a>(gruen: &Path, stand: Option<&Stand>, slugs: impl IntoIterator<Item = &'a String>, im_bau: &BTreeSet<String>) -> BTreeMap<String, Eintrag> {
    slugs
        .into_iter()
        .map(|slug| {
            let p = stand.and_then(|s| s.projekte.get(slug));
            // Abnahme 4c: Ein ganz ausgeschlossenes Projekt fehlt im Bau; dann
            // gibt es keine „letzte gute Fassung“ zu sehen, auch mit Kopie.
            let ausgeschlossen = p.is_some_and(|p| !p.gruen && p.rot_art == Some(RotArt::Ausgeschlossen));
            let k = kopie(gruen, slug).filter(|_| !ausgeschlossen);
            let letzte_gute = zeit(p, k.as_deref());
            // Nur eine Seite, die zu sehen ist (es gibt eine gute Fassung), hat eine Bauzeit.
            let gebaut = letzte_gute.and(p.and_then(|p| p.gebaut_um).and_then(|s| Local.timestamp_opt(s, 0).single()).or(letzte_gute));
            let (zustand, satz) = if im_bau.contains(slug) {
                (SeitenBau::Laeuft, None)
            } else {
                match p {
                    Some(p) if p.gruen => (SeitenBau::Gruen, None),
                    // Zweig A: nicht geprüft ist kein Rot; zu sehen ist die Kopie, sonst nichts.
                    Some(p) if p.rot_art == Some(RotArt::NichtGeprueft) => {
                        let satz = if k.is_some() { SATZ_NICHT_GEPRUEFT_MIT_FASSUNG } else { SATZ_NICHT_GEPRUEFT_OHNE_FASSUNG };
                        (SeitenBau::NichtGeprueft, Some(satz.to_string()))
                    }
                    Some(p) => (SeitenBau::Rot, Some(satz_rot(p.rot_art, k.is_some()).to_string())),
                    // Ohne Eintrag, aber mit Kopie: grün gewesen, seitdem geändert
                    // (der Beobachter baut ohnehin neu).
                    None if k.is_some() => (SeitenBau::Gruen, None),
                    None => (SeitenBau::NochNicht, None),
                }
            };
            (slug.clone(), Eintrag { zustand, letzte_gute, gebaut, satz })
        })
        .collect()
}

/// Der Bau-Stand, wenn `stand.json` nicht lesbar ist: `unbekannt` (oder
/// „läuft“), ohne Zeit und Satz. So bleibt der Stand lesbar, und ein Bau lässt sich weiter bestellen.
pub fn unbekannt<'a>(slugs: impl IntoIterator<Item = &'a String>, im_bau: &BTreeSet<String>) -> BTreeMap<String, Eintrag> {
    slugs
        .into_iter()
        .map(|s| {
            let zustand = if im_bau.contains(s) { SeitenBau::Laeuft } else { SeitenBau::Unbekannt };
            (s.clone(), Eintrag { zustand, letzte_gute: None, gebaut: None, satz: None })
        })
        .collect()
}

/// Der Bau-Stand aller Projekte mit eigener Seite. Ist `stand.json` nicht
/// lesbar: `unbekannt` (Grund im Protokoll, höchstens alle zehn Minuten).
pub fn alle(state: &AppState) -> BTreeMap<String, Eintrag> {
    let gruen = gruen_ordner(state);
    let slugs = mit_seite(state);
    let stand = match stand_lesen(&gruen) {
        Ok(s) => s,
        Err(grund) => {
            static ZULETZT: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
            let mut z = ZULETZT.lock().unwrap_or_else(|e| e.into_inner());
            if z.is_none_or(|t| t.elapsed() >= std::time::Duration::from_secs(600)) {
                *z = Some(std::time::Instant::now());
                tracing::warn!("Bau-Stand: {grund}; die Oberfläche zeigt „Bau-Stand gerade nicht lesbar“.");
            }
            return unbekannt(&slugs, &state.baumeister.im_bau());
        }
    };
    berechnen(&gruen, stand.as_ref(), &slugs, &state.baumeister.im_bau())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dashboard-baustand-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn eintrag(gruen: bool, gruen_um: Option<i64>, rot_art: Option<RotArt>) -> ProjektStand {
        ProjektStand {
            abdruck: 1,
            umgebung: 0,
            gruen,
            satz: "Die Seite von „x“ hat einen Fehler. Erster Fund: projects/x/Seite.svelte, Zeile 3: …".into(),
            gruen_um,
            rot_art,
            gebaut_um: None,
            gruen_abdruck: None,
            versucht_um: None,
        }
    }

    fn kopie_anlegen(gruen: &Path, slug: &str) {
        std::fs::create_dir_all(gruen.join(slug)).unwrap();
        std::fs::write(gruen.join(slug).join("Seite.svelte"), "<p>gut</p>\n").unwrap();
    }

    /// Alle vier Zustände; der Satz nennt nie einen Pfad; die Zeit bleibt bei rot stehen.
    #[test]
    fn vier_zustaende_ohne_pfade() {
        let g = tmp("vier");
        kopie_anlegen(&g, "gut");
        kopie_anlegen(&g, "rot");
        let um = 1_793_450_700; // 31.10.2026, 11:45 UTC
        let stand = Stand {
            projekte: BTreeMap::from([
                ("gut".to_string(), eintrag(true, Some(um), None)),
                ("rot".to_string(), eintrag(false, Some(um), Some(RotArt::Pruefung))),
                ("ohne".to_string(), eintrag(false, None, Some(RotArt::Zusammenbau))),
                ("baut".to_string(), eintrag(true, Some(um), None)),
            ]),
        };
        let slugs: Vec<String> = ["gut", "rot", "ohne", "baut", "neu"].iter().map(|s| s.to_string()).collect();
        let im_bau = BTreeSet::from(["baut".to_string()]);
        let e = berechnen(&g, Some(&stand), &slugs, &im_bau);
        assert_eq!(e["gut"].zustand, SeitenBau::Gruen);
        assert_eq!(e["gut"].letzte_gute.map(|t| t.timestamp()), Some(um));
        assert_eq!(e["gut"].satz, None);
        assert_eq!(e["rot"].zustand, SeitenBau::Rot);
        assert_eq!(e["rot"].letzte_gute.map(|t| t.timestamp()), Some(um), "rot zeigt die Zeit der letzten guten Fassung");
        assert_eq!(e["rot"].satz.as_deref(), Some(SATZ_ROT_MIT_FASSUNG));
        assert_eq!(e["ohne"].zustand, SeitenBau::Rot);
        assert_eq!(e["ohne"].letzte_gute, None, "ohne Kopie keine letzte gute Fassung");
        assert_eq!(e["ohne"].satz.as_deref(), Some(SATZ_PASST_NICHT_OHNE_FASSUNG));
        assert_eq!(e["baut"].zustand, SeitenBau::Laeuft);
        assert_eq!(e["neu"].zustand, SeitenBau::NochNicht);
        for x in e.values() {
            let s = x.satz.clone().unwrap_or_default();
            assert!(!s.contains('/') && !s.contains('\\') && !s.contains("Seite.svelte") && !s.contains("projects"), "{s}");
        }
        // Frei: ohne Satz.
        let frei = serde_json::to_value(e["rot"].frei()).unwrap();
        let mut felder: Vec<String> = frei.as_object().unwrap().keys().cloned().collect();
        felder.sort();
        assert_eq!(felder, vec!["gebaut_um".to_string(), "letzte_gute".to_string(), "zustand".to_string()], "frei nur Zustand und Zeiten");
        // Ohne `gebaut_um` im Stand gilt die Zeit der letzten guten Fassung (frei nur Zahlen und Zeiten).
        assert_eq!(e["gut"].gebaut, e["gut"].letzte_gute);
        assert_eq!(e["ohne"].gebaut, None);
        assert_eq!(frei["zustand"], "rot");
        assert_eq!(serde_json::to_value(e["neu"].frei()).unwrap()["zustand"], "noch_nicht");
        let _ = std::fs::remove_dir_all(&g);
    }

    /// Stände von vor 4c: ohne `gruen_um` gilt die Zeit der Kopie; ohne Eintrag mit Kopie grün.
    #[test]
    fn alter_stand_und_fehlende_datei() {
        let g = tmp("alt");
        kopie_anlegen(&g, "alt");
        std::fs::write(g.join(STAND_DATEI), r#"{"projekte":{"alt":{"abdruck":5,"gruen":false,"satz":"x"}}}"#).unwrap();
        let stand = stand_lesen(&g).unwrap();
        let slugs = vec!["alt".to_string()];
        let e = berechnen(&g, stand.as_ref(), &slugs, &BTreeSet::new());
        assert_eq!(e["alt"].zustand, SeitenBau::Rot);
        assert!(e["alt"].letzte_gute.is_some(), "Zeit aus der Kopie");
        assert_eq!(e["alt"].satz.as_deref(), Some(SATZ_ROT_MIT_FASSUNG));
        // Ohne Eintrag, mit Kopie: grün.
        let e = berechnen(&g, Some(&Stand::default()), &slugs, &BTreeSet::new());
        assert_eq!(e["alt"].zustand, SeitenBau::Gruen);
        // Ohne Datei: Ok(None), alles „noch nicht“ außer mit Kopie.
        let leer = tmp("leer");
        assert_eq!(stand_lesen(&leer).unwrap(), None);
        // Kaputte Datei: Err (dann kein Bau-Stand statt eines falschen).
        std::fs::write(leer.join(STAND_DATEI), "{kaputt").unwrap();
        assert!(stand_lesen(&leer).is_err());
        let _ = std::fs::remove_dir_all(&g);
        let _ = std::fs::remove_dir_all(&leer);
    }

    /// Zweig A: Nicht geprüft ist nie rot und nie grün, sondern ein eigener
    /// Zustand: mit Kopie mit der Zeit der letzten guten Fassung, ohne Kopie
    /// ohne Zeit; der Satz ohne Pfade und Werkzeuge, frei nur der Zustand.
    #[test]
    fn nicht_geprueft_ist_nicht_rot() {
        let g = tmp("nicht-geprueft");
        kopie_anlegen(&g, "mit");
        let um = 1_793_450_700;
        let stand = Stand {
            projekte: BTreeMap::from([
                ("mit".to_string(), eintrag(false, Some(um), Some(RotArt::NichtGeprueft))),
                ("ohne".to_string(), eintrag(false, None, Some(RotArt::NichtGeprueft))),
            ]),
        };
        let slugs = vec!["mit".to_string(), "ohne".to_string()];
        let e = berechnen(&g, Some(&stand), &slugs, &BTreeSet::new());
        assert_eq!((e["mit"].zustand, e["mit"].satz.as_deref()), (SeitenBau::NichtGeprueft, Some(SATZ_NICHT_GEPRUEFT_MIT_FASSUNG)));
        assert_eq!(e["mit"].letzte_gute.map(|t| t.timestamp()), Some(um));
        assert_eq!((e["ohne"].zustand, e["ohne"].satz.as_deref()), (SeitenBau::NichtGeprueft, Some(SATZ_NICHT_GEPRUEFT_OHNE_FASSUNG)));
        assert_eq!((e["ohne"].letzte_gute, e["ohne"].gebaut), (None, None), "ohne Kopie nicht zu sehen");
        for s in [SATZ_NICHT_GEPRUEFT_MIT_FASSUNG, SATZ_NICHT_GEPRUEFT_OHNE_FASSUNG] {
            assert!(!s.contains('/') && !s.contains('\\') && !s.contains("projects") && !s.contains("werkzeug"), "{s}");
        }
        // Frei (`/api/projects`): nur Zustand und Zeiten; geschützt (`/api/bauen`) mit Satz.
        for slug in ["mit", "ohne"] {
            let frei = serde_json::to_value(e[slug].frei()).unwrap();
            assert_eq!(frei["zustand"], "nicht_geprueft", "{frei}");
            assert!(frei.get("satz").is_none() && !frei.to_string().contains("geprüft"), "{frei}");
            assert!(serde_json::to_value(e[slug].mit_satz(slug)).unwrap()["satz"].as_str().is_some_and(|s| s.contains("nicht geprüft")));
        }
        // stand.json mit der neuen Art liest sich (Name in der Datei).
        let text = serde_json::to_string(&stand).unwrap();
        assert!(text.contains("\"rot_art\":\"nicht_geprueft\""), "{text}");
        assert_eq!(serde_json::from_str::<Stand>(&text).unwrap(), stand);
        let _ = std::fs::remove_dir_all(&g);
    }

    /// Abnahme 4c: Zurückgesetzt (Kopie im Bau) zeigt die letzte gute Fassung;
    /// ganz ausgeschlossen nicht, auch wenn es eine Kopie gibt.
    #[test]
    fn ausgeschlossen_ohne_letzte_gute_fassung() {
        let g = tmp("aus");
        kopie_anlegen(&g, "zurueck");
        kopie_anlegen(&g, "raus");
        let um = 1_793_450_700;
        let stand = Stand {
            projekte: BTreeMap::from([
                ("zurueck".to_string(), eintrag(false, Some(um), Some(RotArt::Zusammenbau))),
                ("raus".to_string(), eintrag(false, Some(um), Some(RotArt::Ausgeschlossen))),
            ]),
        };
        let slugs = vec!["zurueck".to_string(), "raus".to_string()];
        let e = berechnen(&g, Some(&stand), &slugs, &BTreeSet::new());
        assert_eq!(e["zurueck"].letzte_gute.map(|t| t.timestamp()), Some(um));
        assert_eq!(e["zurueck"].satz.as_deref(), Some(SATZ_PASST_NICHT_MIT_FASSUNG));
        assert_eq!(e["raus"].zustand, SeitenBau::Rot);
        assert_eq!(e["raus"].letzte_gute, None, "fehlt im Bau: keine letzte gute Fassung");
        assert_eq!(e["raus"].satz.as_deref(), Some(SATZ_PASST_NICHT_OHNE_FASSUNG));
        assert!(!SATZ_PASST_NICHT_OHNE_FASSUNG.contains("letzte gute Fassung"));
        // stand.json nicht lesbar: unbekannt (der Knopf bleibt), im Bau läuft.
        let u = unbekannt(&slugs, &BTreeSet::from(["raus".to_string()]));
        assert_eq!((u["zurueck"].zustand, u["raus"].zustand), (SeitenBau::Unbekannt, SeitenBau::Laeuft));
        assert_eq!(serde_json::to_value(u["zurueck"].frei()).unwrap()["zustand"], "unbekannt");
        let _ = std::fs::remove_dir_all(&g);
    }
}
