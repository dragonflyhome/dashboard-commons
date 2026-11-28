//! Tageszeiten am Ort (Kalender, Stufe 3): Aufgang, wahrer Mittag, Untergang,
//! die Dämmerungen aus `vorlagen/kalender/tageszeiten.toml` und die Taglänge, auf
//! dem Rechenkern in [`super::himmel`].
//!
//! - Ein Tag ist der Kalendertag am Ort; gerechnet wird in mittlerer Ortszeit der
//!   Länge, angezeigt in der Uhrzeit des Orts ([`Ort::ortszeit`]).
//! - Aufgang und Untergang: Oberrand der Sonne am Horizont, Refraktion 34′ und
//!   halber Durchmesser 16′ (0,833° auf Meereshöhe).
//! - Im Polargebiet gibt es Tage ohne Auf- oder Untergang: Dann heißt es „Sonne
//!   geht nicht unter“ oder „geht nicht auf“, nie eine erfundene Zeit.

use std::sync::OnceLock;

use chrono::NaiveDate;
use serde::Deserialize;

use super::himmel;
use super::mond::{Lage, Ort, tag_zahl};

/// Wann die Sonne eine Tiefe erreicht, oder dass sie es an dem Tag nicht tut.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Sonnenzeit {
    /// Zeitpunkt in Weltzeit (Tage wie im Rechenkern).
    Um(f64),
    /// Die Sonne bleibt den ganzen Tag über dieser Tiefe (geht nicht unter; Dämmerung ohne Ende).
    BleibtDarueber,
    /// Die Sonne bleibt den ganzen Tag darunter (geht nicht auf; Dämmerung ohne Anfang).
    BleibtDarunter,
    /// Die Suche lief nicht zusammen (nahe der Polargrenze): keine Zeit statt einer geratenen.
    Keine,
}

/// Tiefe der Sonnenmitte bei Aufgang und Untergang auf Meereshöhe: 0,833°.
pub fn tiefe_aufgang() -> f64 {
    himmel::refraktion(0.0) + 16.0 / 60.0
}

/// Eine Dämmerung aus der Vorlage.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Daemmerung {
    pub kennung: String,
    pub titel: String,
    /// Tiefe der Sonne unter dem Horizont in Grad.
    pub winkel: f64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Vorlage {
    daemmerung: Vec<Daemmerung>,
}

const VORLAGE: &str = include_str!("../../vorlagen/kalender/tageszeiten.toml");

fn vorlage_lesen(text: &str) -> Result<Vec<Daemmerung>, String> {
    let v: Vorlage = toml::from_str(text).map_err(|e| format!("vorlagen/kalender/tageszeiten.toml: {}", e.message()))?;
    for d in &v.daemmerung {
        if !crate::vertrag::daten::ist_kennung(&d.kennung) || d.titel.trim().is_empty() || !(0.0..=30.0).contains(&d.winkel) {
            return Err(format!(
                "vorlagen/kalender/tageszeiten.toml: Die Dämmerung „{}“ braucht eine einfache Kennung, einen Titel und einen Winkel von 0 bis 30 Grad.",
                d.kennung
            ));
        }
    }
    Ok(v.daemmerung)
}

/// Die Dämmerungen der Vorlage (beim Bauen geprüft, Test `vorlage_gueltig`).
pub fn daemmerungen() -> &'static [Daemmerung] {
    static D: OnceLock<Vec<Daemmerung>> = OnceLock::new();
    D.get_or_init(|| vorlage_lesen(VORLAGE).unwrap_or_default())
}

/// Wann die Sonne am Tag `d` die Tiefe `tiefe` erreicht, morgens oder abends.
pub fn zeit(d: NaiveDate, lage: Lage, tiefe: f64, morgens: bool) -> Sonnenzeit {
    let tag = tag_zahl(d);
    let start = tag + if morgens { 0.25 } else { 0.75 };
    match himmel::tiefe(start, lage.breite, lage.laenge, tiefe, morgens) {
        Some(t) => Sonnenzeit::Um(himmel::welt_aus_lokal(t, lage.laenge)),
        // Über 1: Die Sonne steht zu Mittag den ganzen Tag über der Tiefe.
        None => match himmel::sinus_abstand(tag + 0.5, lage.breite, lage.laenge, tiefe) {
            v if v > 1.0 => Sonnenzeit::BleibtDarueber,
            v if v < -1.0 => Sonnenzeit::BleibtDarunter,
            _ => Sonnenzeit::Keine,
        },
    }
}

/// Wahrer Mittag (Sonne im Süden bzw. Norden) am Tag `d`, in Weltzeit.
pub fn mittag(d: NaiveDate, lage: Lage) -> f64 {
    himmel::welt_aus_lokal(himmel::lokal_aus_scheinbar(tag_zahl(d) + 0.5, lage.laenge), lage.laenge)
}

/// Geometrische Höhe der Sonnenmitte in Grad zum Zeitpunkt `t` (Weltzeit) am Tag `d` des
/// Orts, ohne Refraktion. Der Stundenwinkel zählt ab dem wahren Mittag von `d`; ganze
/// Tage Abstand ändern ihn nicht, darum genügt der Tag am Ort statt des Sonnentags.
pub fn sonnenhoehe(t: f64, d: NaiveDate, lage: Lage) -> f64 {
    let delta = himmel::deklination(t, 0.0, himmel::sonnenlaenge(t)).to_radians();
    let h = ((t - mittag(d, lage)) * 360.0).to_radians();
    let phi = lage.breite.to_radians();
    (phi.sin() * delta.sin() + phi.cos() * delta.cos() * h.cos()).asin().to_degrees()
}

/// Die Tageszeiten eines Tags.
#[derive(Debug, Clone, PartialEq)]
pub struct Tageszeiten {
    pub aufgang: Sonnenzeit,
    pub mittag: f64,
    pub untergang: Sonnenzeit,
    /// Je Dämmerung der Vorlage: Beginn morgens und Ende abends.
    pub daemmerung: Vec<(&'static Daemmerung, Sonnenzeit, Sonnenzeit)>,
}

impl Tageszeiten {
    /// Die Taglänge in Tagen (Aufgang bis Untergang); ohne Untergang ein ganzer Tag,
    /// ohne Aufgang keiner. `None` an der Polargrenze, wenn eine Zeit fehlt (`Keine`).
    pub fn taglaenge(&self) -> Option<f64> {
        match (self.aufgang, self.untergang) {
            (Sonnenzeit::Um(a), Sonnenzeit::Um(u)) => Some((u - a).clamp(0.0, 1.0)),
            (Sonnenzeit::BleibtDarueber, Sonnenzeit::BleibtDarueber) => Some(1.0),
            (Sonnenzeit::BleibtDarunter, Sonnenzeit::BleibtDarunter) => Some(0.0),
            _ => None,
        }
    }
}

/// Die Tageszeiten am Tag `d` an der Lage `lage`.
pub fn rechnen(d: NaiveDate, lage: Lage) -> Tageszeiten {
    let h = tiefe_aufgang();
    Tageszeiten {
        aufgang: zeit(d, lage, h, true),
        mittag: mittag(d, lage),
        untergang: zeit(d, lage, h, false),
        daemmerung: daemmerungen().iter().map(|x| (x, zeit(d, lage, x.winkel, true), zeit(d, lage, x.winkel, false))).collect(),
    }
}

/// Die Tageszeiten des Kalendertags `d` am Ort: der Sonnentag, dessen wahrer
/// Mittag in Ortszeit auf `d` fällt (Aufgang davor, Untergang danach, auch über
/// Mitternacht). So stimmt der Tag auch, wo die Zone weit von der Länge abweicht
/// (Apia, Kiritimati). `None` ohne Lage oder wenn es den Tag am Ort nicht gibt.
pub fn am_tag(d: NaiveDate, ort: &Ort) -> Option<Tageszeiten> {
    Some(rechnen(sonnentag(d, ort)?, ort.lage?))
}

/// Der Sonnentag (mittlere Ortszeit) zum Kalendertag `d` am Ort: der, dessen wahrer
/// Mittag in Ortszeit auf `d` fällt. `None` ohne Lage oder wenn es den Tag am Ort nicht gibt.
// shortcut: Einen Kalendertag, den es zweimal gibt (Kwajalein 1969), sieht diese Suche
// nur einmal; aufrüsten, wenn doppelte Tage angezeigt werden sollen.
pub fn sonnentag(d: NaiveDate, ort: &Ort) -> Option<NaiveDate> {
    let lage = ort.lage?;
    [d, d.pred_opt()?, d.succ_opt()?].into_iter().find(|c| ort.tag(mittag(*c, lage)) == Some(d))
}

/// Eine Dauer in Tagen als „16 h 47 min“ (auf die Minute gerundet).
pub fn dauer_text(tage: f64) -> String {
    let min = (tage * 1440.0).round() as i64;
    format!("{} h {} min", min / 60, min % 60)
}

/// „+05:30“ aus Sekunden.
fn versatz_text(sekunden: i32) -> String {
    let (z, s) = (if sekunden < 0 { '-' } else { '+' }, sekunden.unsigned_abs());
    format!("{z}{:02}:{:02}", s / 3600, s % 3600 / 60)
}

/// Uhrzeit am Ort als „HH:MM“ (auf die Minute gerundet), mit dem Datum dahinter,
/// wenn der Zeitpunkt nicht auf den Tag `d` fällt.
pub fn uhrzeit(ort: &Ort, t: f64, d: NaiveDate) -> Option<String> {
    let z = ort.ortszeit(t + 30.0 / 86400.0)?;
    let mut uhr = z.format("%H:%M").to_string();
    // An Tagen mit Umstellung (der Versatz wechselt) steht die Zeitbezeichnung dahinter:
    // das Kürzel der Zone (etwa „CEST“), bleibt es gleich (Moskau 2014: MSK), der Versatz.
    if let Some((a, b)) = ort.beginn(d).zip(d.succ_opt().and_then(|n| ort.beginn_ab(n))).filter(|(a, b)| ort.versatz(*a) != ort.versatz(b - 1e-4)) {
        uhr = match ort.kuerzel(t).filter(|_| ort.kuerzel(a) != ort.kuerzel(b - 1e-4)) {
            Some(k) => format!("{uhr} {k}"),
            None => format!("{uhr} {}", versatz_text(ort.versatz(t))),
        };
    }
    Some(if z.date() == d { uhr } else { format!("{uhr} (am {})", z.format("%-d.%-m.")) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kalender::mond::Zeit;

    #[test]
    fn vorlage_gueltig() {
        let d = vorlage_lesen(VORLAGE).unwrap();
        assert_eq!(d.iter().map(|x| x.winkel).collect::<Vec<_>>(), [6.0, 12.0, 18.0]);
        assert!(vorlage_lesen("[[daemmerung]]\nkennung = \"x\"\ntitel = \"X\"\nwinkel = 45.0\n").is_err());
    }

    fn tag(t: &str) -> NaiveDate {
        NaiveDate::parse_from_str(t, "%Y-%m-%d").unwrap()
    }

    /// Tromsø: Mitternachtssonne im Juni, Polarnacht im Dezember (keine erfundene Zeit).
    #[test]
    fn polargebiet() {
        let tromsoe = Lage { breite: 69.6492, laenge: 18.9553 };
        let juni = rechnen(tag("2027-06-21"), tromsoe);
        assert_eq!((juni.aufgang, juni.untergang), (Sonnenzeit::BleibtDarueber, Sonnenzeit::BleibtDarueber));
        assert_eq!(juni.taglaenge(), Some(1.0));
        let dez = rechnen(tag("2027-12-21"), tromsoe);
        assert_eq!((dez.aufgang, dez.untergang), (Sonnenzeit::BleibtDarunter, Sonnenzeit::BleibtDarunter));
        assert_eq!(dez.taglaenge(), Some(0.0));
        // Im Dezember wird es zu Mittag trotzdem bürgerlich hell.
        assert!(matches!(dez.daemmerung[0].1, Sonnenzeit::Um(_)), "{:?}", dez.daemmerung[0]);
        // Reykjavík im Juni: Auf- und Untergang, aber keine astronomische Nacht.
        let rey = rechnen(tag("2027-06-21"), Lage { breite: 64.1466, laenge: -21.9426 });
        assert!(matches!((rey.aufgang, rey.untergang), (Sonnenzeit::Um(_), Sonnenzeit::Um(_))));
        assert_eq!((rey.daemmerung[2].1, rey.daemmerung[2].2), (Sonnenzeit::BleibtDarueber, Sonnenzeit::BleibtDarueber));
    }

    #[test]
    fn texte() {
        assert_eq!(dauer_text(16.0 / 24.0 + 47.0 / 1440.0), "16 h 47 min");
        let utc = Ort { halbkugel: crate::kalender::mond::Halbkugel::Nord, zeit: Zeit::Fest { sekunden: 0, naeherung: false }, lage: None };
        let d = tag("2027-06-21");
        let t = tag_zahl(d) + 23.0 / 24.0 + 59.6 / 1440.0;
        assert_eq!(uhrzeit(&utc, t, d).as_deref(), Some("00:00 (am 22.6.)"));
        assert_eq!(uhrzeit(&utc, tag_zahl(d) + 0.5, d).as_deref(), Some("12:00"));
    }

    const NOAA: &str = include_str!("noaa-2027.txt");
    const TIMEANDDATE: &str = include_str!("timeanddate-2027.txt");

    fn zone_von(stadt: &str) -> &'static str {
        match stadt.to_lowercase().as_str() {
            "london" => "Europe/London",
            "madrid" => "Europe/Madrid",
            "sydney" => "Australia/Sydney",
            "singapore" | "singapur" => "Asia/Singapore",
            "reykjavik" => "Atlantic/Reykjavik",
            "tromso" | "tromsoe" => "Europe/Oslo",
            x => panic!("unbekannte Stadt {x}"),
        }
    }

    /// „HH:MM“ oder „HH:MM:SS“ in Sekunden nach Mitternacht.
    fn sekunden(text: &str) -> i64 {
        let teile: Vec<i64> = text.split(':').map(|x| x.parse().unwrap()).collect();
        teile[0] * 3600 + teile[1] * 60 + teile.get(2).copied().unwrap_or(0)
    }

    /// Ortszeit von `t` in Sekunden nach Mitternacht des Tags `d` (über Mitternacht hinaus > 86400).
    fn ortssekunden(ort: &Ort, t: f64, d: NaiveDate) -> i64 {
        (ort.ortszeit(t).unwrap() - d.and_hms_opt(0, 0, 0).unwrap()).num_seconds()
    }

    fn felder(zeile: &str) -> std::collections::HashMap<&str, &str> {
        zeile.split(';').filter_map(|f| f.split_once('=')).collect()
    }

    /// Breitenband 0: 0–30°, 1: 30–60°, 2: 60–72°.
    fn band(breite: f64) -> usize {
        match breite.abs() {
            b if b < 30.0 => 0,
            b if b < 60.0 => 1,
            _ => 2,
        }
    }

    /// Abweichung in Minuten zwischen der auf die Minute gerundeten Rechnung und der veröffentlichten Minute.
    fn minuten_abstand(gerechnet: i64, veroeffentlicht: i64) -> i64 {
        ((gerechnet as f64 / 60.0).round() as i64 - veroeffentlicht / 60).abs()
    }

    fn um(s: Sonnenzeit) -> f64 {
        match s {
            Sonnenzeit::Um(t) => t,
            x => panic!("Zeit erwartet, {x:?}"),
        }
    }

    /// NOAA Solar Calculator, 28 Fälle (Ortszeit mit Sommerzeit). Aufgang und
    /// Untergang auf ±1 min gegen die veröffentlichte Minute, Mittag auf ±60 s
    /// gegen die veröffentlichte Sekunde.
    /// Gemessen: größte Abweichung nach Rundung je Band 0–30° / 30–60° / 60–72°:
    /// Aufgang/Untergang 0 / 1 / 0 min, Mittag 15 / 15 / 15 s.
    #[test]
    fn tageszeiten_gegen_noaa() {
        let mut groesste = [[0i64; 2]; 3];
        for zeile in NOAA.lines().filter(|z| !z.starts_with('#') && !z.trim().is_empty()) {
            let s: Vec<&str> = zeile.split(';').collect();
            let lage = Lage { breite: s[1].parse().unwrap(), laenge: s[2].parse().unwrap() };
            let d = tag(s[3]);
            let f = felder(zeile);
            let ort = Ort::an(lage, Zeit::Zone(crate::kalender::zone::zone(zone_von(s[0])).unwrap()));
            let r = rechnen(d, lage);
            let b = band(lage.breite);
            let mittag = (ortssekunden(&ort, r.mittag, d) - sekunden(f["mittag"])).abs();
            assert!(mittag <= 60, "{zeile}: Mittag {mittag} s daneben");
            groesste[b][1] = groesste[b][1].max(mittag);
            // Tromsø 21.6./21.12.: NOAA zeigt 00:00 als Platzhalter für Polartag und -nacht.
            if s[0] == "tromso" && (s[3].ends_with("06-21") || s[3].ends_with("12-21")) {
                let polar = if s[3].ends_with("06-21") { Sonnenzeit::BleibtDarueber } else { Sonnenzeit::BleibtDarunter };
                assert_eq!((r.aufgang, r.untergang), (polar, polar), "{zeile}");
                continue;
            }
            let auf = ortssekunden(&ort, um(r.aufgang), d);
            let unter = ortssekunden(&ort, um(r.untergang), d);
            let mut unter_pub = sekunden(f["untergang"]);
            // Reykjavík 21.6.: der Untergang dieses Abends liegt nach Mitternacht (00:04 am 22.6.).
            if unter_pub < sekunden(f["aufgang"]) {
                unter_pub += 86400;
            }
            for (wer, g, p) in [("Aufgang", auf, sekunden(f["aufgang"])), ("Untergang", unter, unter_pub)] {
                let a = minuten_abstand(g, p);
                assert!(a <= 1, "{zeile}: {wer} {a} min daneben (gerechnet {g} s)");
                groesste[b][0] = groesste[b][0].max(a);
            }
        }
        // Je Band: [größte Abweichung Auf-/Untergang in Minuten, Mittag in Sekunden].
        assert!(groesste.iter().all(|b| b[0] <= 1 && b[1] <= 60), "je Band [min Auf/Unter, s Mittag]: {groesste:?}");
    }

    /// timeanddate.com, 12 Fälle (eigene Koordinaten der Seite), dazu die bürgerliche Dämmerung.
    /// Gemessen: größte Abweichung nach Rundung je Band 0–30° / 30–60° / 60–72°: 1 / 1 / 1 min
    /// (die Seite rundet bzw. schneidet die Minute nicht einheitlich ab, Mittag etwa 14:20 bei 14:20:45).
    #[test]
    fn tageszeiten_gegen_timeanddate() {
        let koord = |stadt: &str| match stadt {
            "London" => (51.507, -0.127),
            "Madrid" => (40.414, -3.704),
            "Sydney" => (-33.864, 151.208),
            "Singapur" => (1.284, 103.851),
            "Reykjavik" => (64.146, -21.942),
            "Tromsoe" => (69.649, 18.955),
            x => panic!("unbekannte Stadt {x}"),
        };
        let mut groesste = [0i64; 3];
        for zeile in TIMEANDDATE.lines().filter(|z| !z.starts_with('#') && !z.trim().is_empty()) {
            let s: Vec<&str> = zeile.split(';').collect();
            let (breite, laenge) = koord(s[0]);
            let lage = Lage { breite, laenge };
            let d = tag(s[1]);
            let f = felder(zeile);
            let ort = Ort::an(lage, Zeit::Zone(crate::kalender::zone::zone(zone_von(s[0])).unwrap()));
            let r = rechnen(d, lage);
            let b = band(breite);
            let mut pruefe = |wer: &str, z: Sonnenzeit, text: &str| {
                if text == "-" {
                    assert!(!matches!(z, Sonnenzeit::Um(_)), "{zeile}: {wer} erwartet keine Zeit, {z:?}");
                    return;
                }
                let a = minuten_abstand(ortssekunden(&ort, um(z), d), sekunden(text));
                assert!(a <= 1, "{zeile}: {wer} {a} min daneben");
                groesste[b] = groesste[b].max(a);
            };
            pruefe("Mittag", Sonnenzeit::Um(r.mittag), f["mittag"]);
            pruefe("Aufgang", r.aufgang, f["aufgang"]);
            pruefe("bürgerlich Beginn", r.daemmerung[0].1, f["buergerlich_beginn"]);
            pruefe("bürgerlich Ende", r.daemmerung[0].2, f["buergerlich_ende"]);
            if s[0] == "Reykjavik" && s[1] == "2027-06-21" {
                // 00:03 ist der Untergang des Vorabends (20.6.). Der Kalendertag 21.6. zeigt den
                // Sonnentag, dessen wahrer Mittag auf den 21.6. fällt; sein Untergang folgt am 22.6.
                let vorabend = rechnen(d.pred_opt().unwrap(), lage);
                pruefe("Untergang des Vorabends", vorabend.untergang, f["untergang"]);
                let am = am_tag(d, &ort).unwrap();
                assert_eq!((ort.tag(am.mittag), &am), (Some(d), &r), "{zeile}: Sonnentag mit Mittag am 21.6.");
                let unter = uhrzeit(&ort, um(am.untergang), d).unwrap();
                assert!(unter.ends_with(" (am 22.6.)"), "{zeile}: Untergang {unter}");
            } else {
                pruefe("Untergang", r.untergang, f["untergang"]);
            }
        }
        // Je Band: größte Abweichung in Minuten, mit Mittag und bürgerlicher Dämmerung.
        assert!(groesste.iter().all(|&a| a <= 1), "je Band: {groesste:?}");
    }

    /// Polarfälle aus beiden Quellen ausdrücklich: Tromsø 21.6. ohne Untergang,
    /// 21.12. ohne Aufgang; Reykjavík 21.6. mit Untergang, aber ohne Ende
    /// irgendeiner Dämmerung („keine“).
    #[test]
    fn polarfaelle() {
        let tromsoe = Lage { breite: 69.6492, laenge: 18.9553 };
        let juni = rechnen(tag("2027-06-21"), tromsoe);
        assert_eq!(juni.untergang, Sonnenzeit::BleibtDarueber);
        assert!(juni.daemmerung.iter().all(|x| (x.1, x.2) == (Sonnenzeit::BleibtDarueber, Sonnenzeit::BleibtDarueber)));
        let dez = rechnen(tag("2027-12-21"), tromsoe);
        assert_eq!(dez.aufgang, Sonnenzeit::BleibtDarunter);
        let rey = rechnen(tag("2027-06-21"), Lage { breite: 64.1466, laenge: -21.9426 });
        assert!(matches!(rey.untergang, Sonnenzeit::Um(_)));
        for (x, beginn, ende) in &rey.daemmerung {
            assert_eq!((*beginn, *ende), (Sonnenzeit::BleibtDarueber, Sonnenzeit::BleibtDarueber), "{}", x.kennung);
        }
    }

    /// Tromsø an den Grenzen des Polartags 2027: ein Übergang von Zeiten zu
    /// „geht nicht unter“ (Mai) und zurück (Juli), ohne geratene Zeiten dazwischen.
    #[test]
    fn polartag_grenzen_tromsoe() {
        let tromsoe = Lage { breite: 69.6492, laenge: 18.9553 };
        for (von, tage, beginnt) in [("2027-05-12", 14, true), ("2027-07-18", 14, false)] {
            let mut zustaende = Vec::new();
            for d in tag(von).iter_days().take(tage) {
                let r = rechnen(d, tromsoe);
                for (wer, z) in [("Aufgang", r.aufgang), ("Untergang", r.untergang)] {
                    if let Sonnenzeit::Um(t) = z {
                        // Echte Zeiten liegen auf der richtigen Seite des Mittags und in der Nacht um Mitternacht.
                        let abstand = (t - r.mittag) * 24.0;
                        let erwartet = if wer == "Aufgang" { -12.5..-7.0 } else { 7.0..12.5 };
                        assert!(erwartet.contains(&abstand), "{d} {wer}: {abstand:.2} h vom Mittag");
                    }
                }
                // U: Auf- und Untergang; D: beide „geht nicht unter“; H: Grenztag mit nur
                // einer Zeit (im Mai Aufgang nach der letzten Nacht, im Juli Untergang in
                // die erste Nacht); K: „keine Zeit“ (Suche lief nicht zusammen).
                let zustand = match (r.aufgang, r.untergang) {
                    (Sonnenzeit::Um(_), Sonnenzeit::Um(_)) => 'U',
                    (Sonnenzeit::BleibtDarueber, Sonnenzeit::BleibtDarueber) => 'D',
                    (Sonnenzeit::Keine, _) | (_, Sonnenzeit::Keine) => 'K',
                    (Sonnenzeit::Um(_), Sonnenzeit::BleibtDarueber) if beginnt => 'H',
                    (Sonnenzeit::BleibtDarueber, Sonnenzeit::Um(_)) if !beginnt => 'H',
                    (a, u) => panic!("{d}: {a:?} / {u:?}"),
                };
                zustaende.push(zustand);
            }
            let folge: String = zustaende.iter().collect();
            // Gemessen 2027: Mai „UUUUUUHDDDDDDD“ (18.5.: Aufgang 01:01 MESZ nach der letzten Nacht, kein Untergang),
            // Juli „DDDDDDDDKUUUUU“ (26.7.: Aufgang „keine“, obwohl die Sonne die Nacht davor oben blieb, Untergang
            // 00:17 MESZ am 27.7.; taglaenge() ergibt an diesem Tag keine Angabe).
            let erwartet = if beginnt { ("UUUU", "DDDD") } else { ("DDDD", "UUUU") };
            assert!(folge.starts_with(erwartet.0) && folge.ends_with(erwartet.1), "{von}: {folge}");
            // Genau ein Wechsel; dazwischen höchstens ein Grenztag (H oder K).
            let (erst, dann) = (erwartet.0.as_bytes()[0] as char, erwartet.1.as_bytes()[0] as char);
            let mitte = folge.trim_start_matches(erst).trim_end_matches(dann);
            assert!(mitte.len() <= 1 && mitte.chars().all(|c| c == 'H' || c == 'K'), "{von}: {folge}");
        }
    }

    fn in_zone(breite: f64, laenge: f64, zone: &str) -> Ort {
        Ort::an(Lage { breite, laenge }, Zeit::Zone(crate::kalender::zone::zone(zone).unwrap()))
    }

    /// Aufgang, Mittag und Untergang des Kalendertags `d` als Text.
    fn drei(ort: &Ort, d: NaiveDate) -> [String; 3] {
        let z = am_tag(d, ort).unwrap();
        [um(z.aufgang), z.mittag, um(z.untergang)].map(|t| uhrzeit(ort, t, d).unwrap())
    }

    /// M1: Ein Kalendertag zeigt den Sonnentag, dessen wahrer Mittag in Ortszeit auf
    /// ihn fällt, auch wo die Zone weit vor der Länge liegt (Apia +13, Kiritimati +14).
    /// Reykjavík bleibt, wie es war: Untergang nach Mitternacht mit „(am 22.6.)“.
    #[test]
    fn sonnentag_nach_dem_mittag() {
        let d = tag("2027-06-21");
        for (name, ort) in [("Apia", in_zone(-13.8, -171.8, "Pacific/Apia")), ("Kiritimati", in_zone(1.9, -157.5, "Pacific/Kiritimati"))] {
            let lage = ort.lage.unwrap();
            // Der Sonnentag des mittleren Ortsdatums hätte seinen Mittag am 22.6.
            assert_eq!(ort.tag(rechnen(d, lage).mittag), d.succ_opt(), "{name}");
            let z = am_tag(d, &ort).unwrap();
            assert_eq!(ort.tag(z.mittag), Some(d), "{name}");
            assert!(um(z.aufgang) < z.mittag && z.mittag < um(z.untergang), "{name}: {z:?}");
            let [auf, mittag, unter] = drei(&ort, d);
            // Alle drei am 21.6. selbst: „HH:MM“ ohne „(am …)“, in der Reihenfolge des Tags.
            assert!([&auf, &mittag, &unter].iter().all(|t| t.len() == 5) && auf < mittag && mittag < unter, "{name}: {auf} {mittag} {unter}");
            assert!(mittag.starts_with("12:"), "{name}: Mittag {mittag}");
        }
        let rey = in_zone(64.1466, -21.9426, "Atlantic/Reykjavik");
        assert_eq!(am_tag(d, &rey).unwrap(), rechnen(d, rey.lage.unwrap()));
        // NOAA: 02:55, 13:29:27, 00:04; der Mittag liegt hier rund 15 s später und rundet auf 13:30.
        assert_eq!(drei(&rey, d), ["02:55", "13:30", "00:04 (am 22.6.)"]);
        // Ohne Lage keine Tageszeiten.
        assert_eq!(am_tag(d, &Ort::windows()), None);
    }

    /// S2/Hinweis: Taglänge nur, wenn beide Enden bekannt sind; ein Grenztag mit nur
    /// einer Zeit (Aufgang, dann „geht nicht unter“) oder „keine Zeit“ gibt keine Angabe.
    #[test]
    fn taglaenge_an_grenztagen() {
        use Sonnenzeit::{BleibtDarueber as Oben, BleibtDarunter as Unten, Keine, Um};
        let mit = |aufgang, untergang| Tageszeiten { aufgang, mittag: 0.0, untergang, daemmerung: Vec::new() }.taglaenge();
        assert_eq!(mit(Um(0.25), Oben), None);
        assert_eq!(mit(Oben, Um(0.75)), None);
        assert_eq!(mit(Keine, Um(0.75)), None);
        assert_eq!(mit(Um(0.25), Keine), None);
        assert_eq!(mit(Oben, Oben), Some(1.0));
        assert_eq!(mit(Unten, Unten), Some(0.0));
        assert_eq!(mit(Um(0.25), Um(0.75)), Some(0.5));
        // Tromsø im Mai: der Grenztag mit Aufgang, aber ohne Untergang.
        let tromsoe = Lage { breite: 69.6492, laenge: 18.9553 };
        let grenz = tag("2027-05-12").iter_days().take(14).map(|d| rechnen(d, tromsoe)).find(|r| matches!((r.aufgang, r.untergang), (Um(_), Oben)));
        assert_eq!(grenz.expect("Grenztag im Mai").taglaenge(), None);
    }

    /// S3: Ein Umstellungstag wird am Versatz erkannt. Moskau 26.10.2014 (MSK +4 → MSK +3,
    /// um 2 Uhr): gleiches Kürzel, darum der Versatz hinter der Zeit; Vor- und Folgetag ohne.
    /// Berlin an den Umstellungstagen 2027 mit „CEST“/„CET“, an gewöhnlichen Tagen ohne.
    #[test]
    fn umstellungstag_am_versatz() {
        let moskau = in_zone(55.7558, 37.6173, "Europe/Moscow");
        let d = tag("2014-10-26");
        for t in drei(&moskau, d) {
            assert!(t.ends_with(" +03:00") && t.len() == 12, "{t}");
        }
        // 0:30 Ortszeit (20:30 Weltzeit am Vortag) liegt vor der Umstellung: alter Versatz.
        assert_eq!(uhrzeit(&moskau, tag_zahl(tag("2014-10-25")) + 20.5 / 24.0, d).as_deref(), Some("00:30 +04:00"));
        for n in ["2014-10-25", "2014-10-27"] {
            assert!(drei(&moskau, tag(n)).iter().all(|t| t.len() == 5), "{n}: {:?}", drei(&moskau, tag(n)));
        }
        let berlin = in_zone(52.52, 13.405, "Europe/Berlin");
        for (n, k) in [("2027-03-28", "CEST"), ("2027-10-31", "CET")] {
            assert!(drei(&berlin, tag(n)).iter().all(|t| t.ends_with(&format!(" {k}"))), "{n}: {:?}", drei(&berlin, tag(n)));
        }
        // 0:30 Weltzeit am 28.3.2027 ist 1:30 Winterzeit, vor der Umstellung um 2 Uhr.
        assert_eq!(uhrzeit(&berlin, tag_zahl(tag("2027-03-28")) + 0.5 / 24.0, tag("2027-03-28")).as_deref(), Some("01:30 CET"));
        for n in ["2027-03-27", "2027-03-29", "2027-06-21", "2027-10-30", "2027-11-01"] {
            assert!(drei(&berlin, tag(n)).iter().all(|t| t.len() == 5), "{n}: {:?}", drei(&berlin, tag(n)));
        }
    }

    /// Sonnenhöhe an festen Tagen: am wahren Mittag 90° − |φ − δ|; zur Aufgangszeit über
    /// dem sichtbaren Horizont (geometrisch + 0,833°) nahe 0; Polartag und Polarnacht in Tromsø.
    #[test]
    fn sonnenhoehe_an_festen_tagen() {
        let berlin = Lage { breite: 52.52, laenge: 13.405 };
        let sydney = Lage { breite: -33.87, laenge: 151.21 };
        let tromsoe = Lage { breite: 69.6492, laenge: 18.9553 };
        let sichtbar = |t: f64, d: NaiveDate, l: Lage| sonnenhoehe(t, d, l) + tiefe_aufgang();
        for (l, n) in [(berlin, "2027-06-21"), (berlin, "2027-12-21"), (sydney, "2027-03-20"), (tromsoe, "2027-03-20"), (tromsoe, "2027-06-21")] {
            let m = mittag(tag(n), l);
            let delta = himmel::deklination(m, 0.0, himmel::sonnenlaenge(m));
            let h = sonnenhoehe(m, tag(n), l);
            assert!((h - (90.0 - (l.breite - delta).abs())).abs() <= 0.1, "{n} {}: {h}", l.breite);
        }
        for (l, n) in [(berlin, "2027-06-21"), (berlin, "2027-12-21"), (sydney, "2027-03-20"), (tromsoe, "2027-03-20")] {
            let a = um(rechnen(tag(n), l).aufgang);
            let h = sichtbar(a, tag(n), l);
            assert!(h.abs() <= 0.3, "{n} {}: {h}", l.breite);
        }
        let polartag = sichtbar(mittag(tag("2027-06-21"), tromsoe) + 0.5, tag("2027-06-21"), tromsoe);
        let polarnacht = sichtbar(mittag(tag("2027-12-21"), tromsoe), tag("2027-12-21"), tromsoe);
        assert!(polartag > 0.0, "{polartag}");
        assert!(polarnacht < 0.0, "{polarnacht}");
    }
}
