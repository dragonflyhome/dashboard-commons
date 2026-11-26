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
            (Sonnenzeit::BleibtDarueber, _) | (_, Sonnenzeit::BleibtDarueber) => Some(1.0),
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

/// Eine Dauer in Tagen als „16 h 47 min“ (auf die Minute gerundet).
pub fn dauer_text(tage: f64) -> String {
    let min = (tage * 1440.0).round() as i64;
    format!("{} h {} min", min / 60, min % 60)
}

/// Uhrzeit am Ort als „HH:MM“ (auf die Minute gerundet), mit dem Datum dahinter,
/// wenn der Zeitpunkt nicht auf den Tag `d` fällt.
pub fn uhrzeit(ort: &Ort, t: f64, d: NaiveDate) -> Option<String> {
    let z = ort.ortszeit(t + 30.0 / 86400.0)?;
    let mut uhr = z.format("%H:%M").to_string();
    // An Tagen mit Umstellung steht das Kürzel der Zone dahinter (etwa „CEST“).
    let umstellung = ort.beginn(d).zip(d.succ_opt().and_then(|n| ort.beginn_ab(n))).is_some_and(|(a, b)| ort.kuerzel(a) != ort.kuerzel(b - 1e-4));
    if let Some(k) = ort.kuerzel(t).filter(|_| umstellung) {
        uhr = format!("{uhr} {k}");
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
                // 00:03 ist der Untergang des Vorabends (20.6.); der dieses Abends liegt am 22.6.
                let vorabend = rechnen(d.pred_opt().unwrap(), lage);
                pruefe("Untergang des Vorabends", vorabend.untergang, f["untergang"]);
                assert!(ortssekunden(&ort, um(r.untergang), d) > 86400, "{zeile}: Untergang erst nach Mitternacht");
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
}
