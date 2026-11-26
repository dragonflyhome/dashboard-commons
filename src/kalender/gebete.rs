//! Gebetszeiten (Kalender, Stufe 4) auf dem Sonnenstand aus [`super::tag`] und
//! [`super::himmel`]. Rechenweisen und Varianten sind Daten in
//! `vorlagen/kalender/gebete.toml`; welche Regeln es gibt, ist ein fester Wortschatz hier.
//!
//! Bezug: adhan-js (MIT, github.com/batoulapps/adhan-js), Commit d2993d56 (develop),
//! src/PrayerTimes.ts, src/SolarTime.ts, src/CalculationMethod.ts; gelesen, nicht ausgeführt.
//! Abweichungen davon:
//! - Der Tag ist der Sonnentag, dessen wahrer Mittag auf den Kalendertag fällt
//!   ([`super::tag::sonnentag`]); die Nacht reicht vom Untergang an ihm bis zum Aufgang am nächsten.
//! - Die Schranke der Hochbreite gilt auch, wenn die Winkelzeit weniger als 5 Minuten vor dem
//!   Aufgang bzw. nach dem Untergang liegt (Auftrag S4).
//! - Keine Polar-Ausweichregel: Fehlt Auf- oder Untergang, gibt es keine Zeit, nie eine erfundene.
//! - Asr geometrisch ohne Refraktion, Zeitpunkt über die Tiefensuche des Rechenkerns.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use chrono::NaiveDate;
use serde::Deserialize;

use super::himmel;
use super::mond::Ort;
use super::tag::{self, Sonnenzeit};

/// Die Regeln, die eine Zeile haben kann.
pub const REGELN: [&str; 7] = ["fajr", "sunrise", "dhuhr", "asr-shafi", "asr-hanafi", "maghrib", "isha"];

/// Die Regeln der Hochbreite und ihr Anteil an der Nacht.
pub const HOCHBREITEN: [&str; 3] = ["siebtel", "mitte-der-nacht", "winkel"];

/// Eine Rechenweise.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Methode {
    pub kennung: String,
    pub titel: String,
    pub fajr_winkel: f64,
    pub isha_winkel: f64,
    /// Minuten je Regel, nach der Rechnung, vor dem Runden.
    #[serde(default)]
    pub zuschlaege: BTreeMap<String, i32>,
    pub quelle: String,
}

/// Eine Zeile einer Variante.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZeitDef {
    pub name: String,
    pub regel: String,
    /// Zusammengelegte Bestandteile (nur genannt, ohne eigene Zeit).
    #[serde(default)]
    pub umfasst: Vec<String>,
}

/// Eine Variante: eine Liste von Zeiten nach einer Rechenweise.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Variante {
    pub kennung: String,
    pub titel: String,
    pub methode: String,
    pub hochbreite: String,
    pub zeiten: Vec<ZeitDef>,
}

/// Die Vorlage.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vorlage {
    pub methode: Vec<Methode>,
    pub variante: Vec<Variante>,
}

const VORLAGE: &str = include_str!("../../vorlagen/kalender/gebete.toml");

/// Liest und prüft eine Vorlage; die Sätze nennen Kennungen, keine Werte.
pub fn vorlage_lesen(text: &str) -> Result<Vorlage, String> {
    let datei = "vorlagen/kalender/gebete.toml";
    let v: Vorlage = toml::from_str(text).map_err(|e| format!("{datei}: {}", e.message()))?;
    let mut kennungen = BTreeSet::new();
    for m in &v.methode {
        if !crate::vertrag::daten::ist_kennung(&m.kennung) || !kennungen.insert(m.kennung.as_str()) || m.titel.trim().is_empty() {
            return Err(format!("{datei}: Die Methode „{}“ hat keine einfache oder eine doppelte Kennung oder keinen Titel.", m.kennung));
        }
        if ![m.fajr_winkel, m.isha_winkel].iter().all(|w| (0.0..=30.0).contains(w)) {
            return Err(format!("{datei}: Bei der Methode „{}“ liegt ein Winkel außerhalb von 0 bis 30 Grad.", m.kennung));
        }
        if m.zuschlaege.iter().any(|(r, z)| !REGELN.contains(&r.as_str()) || !(-30..=30).contains(z)) {
            return Err(format!("{datei}: Bei der Methode „{}“ passt ein Zuschlag nicht (Regel aus dem Wortschatz, −30 bis +30 Minuten).", m.kennung));
        }
    }
    let mut varianten = BTreeSet::new();
    for x in &v.variante {
        let k = &x.kennung;
        if !crate::vertrag::daten::ist_kennung(k) || !varianten.insert(k.as_str()) || x.titel.trim().is_empty() || x.zeiten.is_empty() {
            return Err(format!("{datei}: Die Variante „{k}“ hat keine einfache oder eine doppelte Kennung, keinen Titel oder keine Zeiten."));
        }
        if !v.methode.iter().any(|m| m.kennung == x.methode) || !HOCHBREITEN.contains(&x.hochbreite.as_str()) {
            return Err(format!("{datei}: Die Variante „{k}“ nennt eine unbekannte Methode oder Regel der Hochbreite."));
        }
        let eigene: Vec<&str> = x.zeiten.iter().map(|z| z.regel.as_str()).collect();
        for z in &x.zeiten {
            let umfasst_gut = z.umfasst.iter().all(|u| REGELN.contains(&u.as_str()) && *u != z.regel && !eigene.contains(&u.as_str()));
            if !REGELN.contains(&z.regel.as_str()) || z.name.trim().is_empty() || !umfasst_gut {
                return Err(format!("{datei}: In der Variante „{k}“ hat eine Zeile keinen Namen, eine unbekannte Regel oder ein unpassendes `umfasst`."));
            }
        }
    }
    Ok(v)
}

/// Die eingebaute Vorlage (beim Bauen geprüft, Test `vorlage_gueltig`).
pub fn vorlage() -> &'static Vorlage {
    static V: OnceLock<Vorlage> = OnceLock::new();
    V.get_or_init(|| vorlage_lesen(VORLAGE).unwrap_or(Vorlage { methode: Vec::new(), variante: Vec::new() }))
}

/// Warum eine Zeit nach der Hochbreite steht.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hochbreite {
    /// Die Sonne sinkt nachts nicht so tief wie der Winkel.
    NichtErreicht,
    /// Der Winkel wird erreicht, aber die Schranke liegt näher an der Nacht.
    Schranke,
}

/// Eine gerechnete Zeile.
#[derive(Debug, Clone, PartialEq)]
pub struct Zeile {
    pub name: String,
    pub regel: String,
    pub umfasst: Vec<String>,
    /// Zeitpunkt (Weltzeit, Tage des Rechenkerns); `None`: keine Zeit.
    pub zeit: Option<f64>,
    pub hochbreite: Option<Hochbreite>,
}

/// Die Zeiten einer Variante am Kalendertag `d` am Ort; `None` ohne Lage, ohne
/// den Tag am Ort oder bei unbekannter Methode.
pub fn rechnen(d: NaiveDate, ort: &Ort, variante: &Variante) -> Option<Vec<Zeile>> {
    let lage = ort.lage?;
    let m = vorlage().methode.iter().find(|m| m.kennung == variante.methode)?;
    let c = tag::sonnentag(d, ort)?;
    let h0 = tag::tiefe_aufgang();
    let um = |s: Sonnenzeit| match s {
        Sonnenzeit::Um(t) => Some(t),
        _ => None,
    };
    let auf = tag::zeit(c, lage, h0, true);
    let unter = tag::zeit(c, lage, h0, false);
    let auf_morgen = tag::zeit(c.succ_opt()?, lage, h0, true);
    let mittag = tag::mittag(c, lage);
    let nacht = um(unter).zip(um(auf_morgen)).map(|(u, a)| a - u);
    let anteil = |winkel: f64| match variante.hochbreite.as_str() {
        "mitte-der-nacht" => 0.5,
        "winkel" => winkel / 60.0,
        _ => 1.0 / 7.0,
    };
    let fuenf = 5.0 / 1440.0;
    let zuschlag = |regel: &str| f64::from(*m.zuschlaege.get(regel).unwrap_or(&0)) / 1440.0;
    // Fadschr: die spätere aus Winkelzeit und Aufgang − Anteil der Nacht (wie safeFajr in adhan-js).
    let fajr = || -> (Option<f64>, Option<Hochbreite>) {
        let (Some(n), Some(a)) = (nacht, um(auf)) else { return (None, None) };
        let schranke = a - anteil(m.fajr_winkel) * n;
        match tag::zeit(c, lage, m.fajr_winkel, true) {
            Sonnenzeit::Um(t) if a - t >= fuenf && t >= schranke => (Some(t), None),
            Sonnenzeit::Um(_) => (Some(schranke), Some(Hochbreite::Schranke)),
            Sonnenzeit::BleibtDarueber => (Some(schranke), Some(Hochbreite::NichtErreicht)),
            Sonnenzeit::BleibtDarunter | Sonnenzeit::Keine => (None, None),
        }
    };
    // Ischa: die frühere aus Winkelzeit und Untergang + Anteil der Nacht (safeIsha).
    let isha = || -> (Option<f64>, Option<Hochbreite>) {
        let (Some(n), Some(u)) = (nacht, um(unter)) else { return (None, None) };
        let schranke = u + anteil(m.isha_winkel) * n;
        match tag::zeit(c, lage, m.isha_winkel, false) {
            Sonnenzeit::Um(t) if t - u >= fuenf && t <= schranke => (Some(t), None),
            Sonnenzeit::Um(_) => (Some(schranke), Some(Hochbreite::Schranke)),
            Sonnenzeit::BleibtDarueber => (Some(schranke), Some(Hochbreite::NichtErreicht)),
            Sonnenzeit::BleibtDarunter | Sonnenzeit::Keine => (None, None),
        }
    };
    // Asr: Schattenlänge k + Schatten am Mittag; Höhe h = atan(1/(k + tan|φ − δ|)), ohne Refraktion.
    let asr = |k: f64| -> Option<f64> {
        let delta = himmel::deklination(mittag, 0.0, himmel::sonnenlaenge(mittag));
        let h = (1.0 / (k + (lage.breite - delta).abs().to_radians().tan())).atan().to_degrees();
        if h <= 16.0 / 60.0 {
            return None;
        }
        um(tag::zeit(c, lage, -h, false)).filter(|t| *t > mittag)
    };
    let zeilen = variante
        .zeiten
        .iter()
        .map(|z| {
            let (zeit, hochbreite) = match z.regel.as_str() {
                "fajr" => fajr(),
                "sunrise" => (um(auf), None),
                "dhuhr" => (Some(mittag), None),
                "asr-shafi" => (asr(1.0), None),
                "asr-hanafi" => (asr(2.0), None),
                "maghrib" => (um(unter), None),
                "isha" => isha(),
                _ => (None, None),
            };
            Zeile { name: z.name.clone(), regel: z.regel.clone(), umfasst: z.umfasst.clone(), zeit: zeit.map(|t| t + zuschlag(&z.regel)), hochbreite }
        })
        .collect();
    Some(zeilen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kalender::mond::{Lage, Zeit};
    use crate::kalender::zone::zone;

    fn tag(t: &str) -> NaiveDate {
        NaiveDate::parse_from_str(t, "%Y-%m-%d").unwrap()
    }

    fn ort(breite: f64, laenge: f64, z: &str) -> Ort {
        Ort::an(Lage { breite, laenge }, Zeit::Zone(zone(z).unwrap()))
    }

    fn zeiten(d: &str, o: &Ort, variante: &str) -> Vec<(String, Option<String>, Option<Hochbreite>)> {
        let v = vorlage().variante.iter().find(|v| v.kennung == variante).unwrap();
        rechnen(tag(d), o, v).unwrap().into_iter().map(|z| (z.regel, z.zeit.and_then(|t| tag::uhrzeit(o, t, tag(d))), z.hochbreite)).collect()
    }

    #[test]
    fn vorlage_gueltig() {
        let v = vorlage_lesen(VORLAGE).unwrap();
        assert_eq!(v.variante.iter().map(|v| v.kennung.as_str()).collect::<Vec<_>>(), ["fuenf-mwl", "fuenf-isna", "drei-mwl", "drei-isna"]);
        let falsch = |alt: &str, neu: &str| vorlage_lesen(&VORLAGE.replacen(alt, neu, 1)).unwrap_err();
        assert!(falsch("fajr_winkel = 18.0", "fajr_winkel = 45.0").contains("Winkel"));
        assert!(falsch("{ dhuhr = 1 }", "{ dhuhr = 99 }").contains("Zuschlag"));
        assert!(falsch("hochbreite = \"siebtel\"", "hochbreite = \"achtel\"").contains("unbekannte"));
        assert!(falsch("umfasst = [\"isha\"]", "umfasst = [\"maghrib\"]").contains("umfasst"));
        assert!(
            falsch("{ name = \"Morgen\", regel = \"fajr\" }", "{ name = \"Morgen\", regel = \"fajr\" }, { name = \"X\", regel = \"isha\" }")
                .contains("umfasst")
        );
    }

    const ADHAN_JS: &str = include_str!("gebete-adhan-js.txt");
    const ALADHAN: &str = include_str!("gebete-aladhan-2027.txt");

    /// Eine Probe-Variante mit den sechs verglichenen Regeln, Asr nach Schule.
    fn probe(methode: &str, asr: &str, hochbreite: &str) -> Variante {
        let zeiten = ["fajr", "sunrise", "dhuhr", asr, "maghrib", "isha"]
            .iter()
            .map(|r| ZeitDef { name: r.to_string(), regel: r.to_string(), umfasst: Vec::new() })
            .collect();
        Variante { kennung: "probe".into(), titel: "Probe".into(), methode: methode.into(), hochbreite: hochbreite.into(), zeiten }
    }

    /// Ortszeit in Minuten nach Mitternacht des Tags `d`, gerundet wie `tag::uhrzeit` (nach Mitternacht über 1440).
    fn minuten(o: &Ort, t: f64, d: NaiveDate) -> i64 {
        (o.ortszeit(t + 30.0 / 86400.0).unwrap() - d.and_hms_opt(0, 0, 0).unwrap()).num_minutes()
    }

    /// „4:42 AM“ (adhan-js) oder „04:42“ (AlAdhan) in Minuten nach Mitternacht.
    fn uhr(text: &str) -> i64 {
        let (hm, pm) = match text.split_once(' ') {
            Some((hm, x)) => (hm, Some(x == "PM")),
            None => (text, None),
        };
        let (h, m) = hm.split_once(':').unwrap();
        let (h, m): (i64, i64) = (h.parse().unwrap(), m.parse().unwrap());
        let h = match pm {
            Some(pm) => h % 12 + if pm { 12 } else { 0 },
            None => h,
        };
        h * 60 + m
    }

    /// Vergleicht fajr, sunrise, dhuhr, asr, maghrib, isha mit der Quelle (±1 min) und gibt die
    /// größte Abweichung in Minuten. Ein Isha vor dem Maghrib liegt am Folgetag. `dhuhr_abzug`:
    /// Minuten, die die Quelle beim Dhuhr nicht zuschlägt.
    fn vergleiche(fall: &str, o: &Ort, d: NaiveDate, v: &Variante, soll: &[&str], dhuhr_abzug: i64) -> (i64, Vec<Zeile>) {
        let zeilen = rechnen(d, o, v).unwrap();
        let mut soll: Vec<i64> = soll.iter().map(|t| uhr(t)).collect();
        if soll[5] < soll[4] {
            soll[5] += 1440;
        }
        let mut groesste = 0;
        for (z, s) in zeilen.iter().zip(&soll) {
            let t = z.zeit.unwrap_or_else(|| panic!("{fall}: {} ohne Zeit", z.regel));
            let ist = minuten(o, t, d) - if z.regel == "dhuhr" { dhuhr_abzug } else { 0 };
            let a = (ist - s).abs();
            assert!(a <= 1, "{fall}: {} gerechnet {ist} min, Quelle {s} min, {a} min daneben", z.regel);
            groesste = groesste.max(a);
        }
        (groesste, zeilen)
    }

    /// Ausgabe von adhan-js, nicht unabhängig (gebete-adhan-js.txt): Raleigh ISNA/Hanafi 12.7.2015,
    /// Raleigh MWL/Shafi 1.12.2015, Edinburgh MWL 15.6.2020 mit allen drei Regeln der Hochbreite.
    /// Gemessen: größte Abweichung in allen fünf Fällen 0 min (Raleigh ISNA, Raleigh MWL, Edinburgh
    /// Mitte der Nacht, Siebtel, Winkel).
    #[test]
    fn gegen_adhan_js() {
        let mut groesste = Vec::new();
        for zeile in ADHAN_JS.lines().filter(|z| !z.starts_with('#') && !z.trim().is_empty()) {
            let s: Vec<&str> = zeile.split(';').collect();
            let o = ort(s[1].parse().unwrap(), s[2].parse().unwrap(), s[4]);
            let d = tag(s[3]);
            let (a, zeilen) = vergleiche(s[0], &o, d, &probe(s[5], s[6], s[7]), &s[8..14], 0);
            groesste.push((s[0], a));
            let hb = (zeilen[0].hochbreite, zeilen[5].hochbreite);
            if s[0].starts_with("edinburgh") {
                // 55,98° Mitte Juni: Die Sonne sinkt nachts nur gut 10° tief, weder 18° noch 17° werden erreicht.
                assert_eq!(hb, (Some(Hochbreite::NichtErreicht), Some(Hochbreite::NichtErreicht)), "{zeile}");
            } else {
                assert_eq!(hb, (None, None), "{zeile}");
            }
            if s[0] == "edinburgh-mitte" {
                let isha = tag::uhrzeit(&o, zeilen[5].zeit.unwrap(), d).unwrap();
                assert!(isha.ends_with(" (am 16.6.)"), "{zeile}: Ischa {isha}");
            }
        }
        assert!(groesste.len() == 5 && groesste.iter().all(|x| x.1 <= 1), "größte Abweichung je Fall in min: {groesste:?}");
    }

    /// AlAdhan-Dienst, teilweise unabhängig (gleiches Verfahren), 21.6.2027, MWL und ISNA, Asr
    /// Standard, Hochbreite je Zeile (gebete-aladhan-2027.txt): 14 abgerufene Zeilen mit
    /// ANGLE_BASED = `winkel`, dazu Kairo MWL und ISNA mit `siebtel`, aus den abgerufenen
    /// Kairo-Zeilen hergeleitet (bei MWL greift die Schranke). AlAdhan weist beim Dhuhr keinen
    /// Zuschlag aus und liegt in allen Zeilen eine Minute vor unserem Dhuhr mit +1 min:
    /// Verglichen wird darum unser Dhuhr ohne den Zuschlag der Methode.
    /// Gemessen, größte Abweichung je Fall (MWL/ISNA): London 1/1 (Maghrib eine Minute später),
    /// Paris, Moskau, Sydney, Kapstadt, Mekka, Kairo je 0/0 min, Kairo Siebtel 0/0 min.
    #[test]
    fn gegen_aladhan_2027() {
        let d = tag("2027-06-21");
        let mut groesste = Vec::new();
        for zeile in ALADHAN.lines().filter(|z| !z.starts_with('#') && !z.trim().is_empty()) {
            let s: Vec<&str> = zeile.split('|').map(str::trim).collect();
            let o = ort(s[1].parse().unwrap(), s[2].parse().unwrap(), s[3].split_whitespace().next().unwrap());
            let regel = s[11].split(',').next().unwrap();
            let fall = format!("{} {} {regel}", s[0], s[4]);
            let methode = s[4].to_lowercase();
            let (a, zeilen) = vergleiche(&fall, &o, d, &probe(&methode, "asr-shafi", regel), &s[5..11], 1);
            groesste.push((fall.clone(), a));
            if fall == "Kairo MWL siebtel" {
                assert_eq!((zeilen[0].hochbreite, zeilen[5].hochbreite), (Some(Hochbreite::Schranke), Some(Hochbreite::Schranke)), "{fall}");
            }
            if fall == "Paris MWL winkel" {
                let isha = tag::uhrzeit(&o, zeilen[5].zeit.unwrap(), d).unwrap();
                assert!(isha.ends_with(" (am 22.6.)"), "{fall}: Ischa {isha}");
            }
            // Asr früh vor Asr spät am selben Tag, an jedem Ort.
            let asr = zeiten("2027-06-21", &o, &format!("fuenf-{methode}"));
            let wert = |r: &str| asr.iter().find(|x| x.0 == r).and_then(|x| x.1.clone()).unwrap();
            assert!(wert("asr-shafi") < wert("asr-hanafi"), "{fall}: {asr:?}");
        }
        assert!(groesste.len() == 16 && groesste.iter().all(|x| x.1 <= 1), "größte Abweichung je Fall in min: {groesste:?}");
    }

    fn hochbreite(z: &[(String, Option<String>, Option<Hochbreite>)], r: &str) -> Option<Hochbreite> {
        z.iter().find(|x| x.0 == r).unwrap().2
    }

    /// Mekka 21.6.2027: Die Schranke greift bei keiner Regel. Kairo 21.6.2027 mit der Siebtel-Regel
    /// (gerechnet): Bei MWL greift sie für Fadschr und Ischa (der Winkel wird erreicht, aber
    /// Aufgang − 1/7 Nacht liegt später bzw. Untergang + 1/7 Nacht früher), bei ISNA (15°) nicht.
    #[test]
    fn schranke_mekka_und_kairo() {
        let mekka = ort(21.4225, 39.8262, "Asia/Riyadh");
        for v in ["fuenf-mwl", "fuenf-isna"] {
            let z = zeiten("2027-06-21", &mekka, v);
            assert!(z.iter().all(|x| x.1.is_some() && x.2.is_none()), "Mekka {v}: {z:?}");
        }
        let kairo = ort(30.0444, 31.2357, "Africa/Cairo");
        let mwl = zeiten("2027-06-21", &kairo, "fuenf-mwl");
        assert_eq!((hochbreite(&mwl, "fajr"), hochbreite(&mwl, "isha")), (Some(Hochbreite::Schranke), Some(Hochbreite::Schranke)), "{mwl:?}");
        let isna = zeiten("2027-06-21", &kairo, "fuenf-isna");
        assert_eq!((hochbreite(&isna, "fajr"), hochbreite(&isna, "isha")), (None, None), "{isna:?}");
        // London im Juni: 18° wird nicht erreicht → Siebtel-Regel mit Hinweis (1).
        let london = zeiten("2027-06-21", &ort(51.5074, -0.1278, "Europe/London"), "fuenf-mwl");
        assert_eq!(hochbreite(&london, "fajr"), Some(Hochbreite::NichtErreicht), "{london:?}");
    }

    /// Hergeleitet, keine Tafel: Reykjavík 21.6.2027 aus den NOAA-Zeilen in noaa-2027.txt
    /// (Aufgang 02:55, Untergang 00:04 am 22.6.) und dem gerechneten Aufgang am 22.6.; Nacht =
    /// Aufgang 22.6. − Untergang; Fadschr = Aufgang − Anteil · Nacht, Ischa = Untergang + Anteil · Nacht.
    /// Weder 18° noch 15° werden erreicht. Gemessen: größte Abweichung 0 min.
    #[test]
    fn reykjavik_hergeleitet() {
        let d = tag("2027-06-21");
        let o = ort(64.1466, -21.9426, "Atlantic/Reykjavik");
        let morgen = match tag::zeit(tag("2027-06-22"), o.lage.unwrap(), tag::tiefe_aufgang(), true) {
            Sonnenzeit::Um(t) => (o.ortszeit(t).unwrap() - d.and_hms_opt(0, 0, 0).unwrap()).num_seconds() as f64 / 60.0,
            x => panic!("Aufgang 22.6.: {x:?}"),
        };
        let (auf, unter) = (uhr("02:55") as f64, uhr("00:04") as f64 + 1440.0);
        let nacht = morgen - unter;
        let mut groesste = 0;
        for (methode, fw, iw) in [("mwl", 18.0, 17.0), ("isna", 15.0, 15.0)] {
            for (regel, fa, ia) in [("siebtel", 1.0 / 7.0, 1.0 / 7.0), ("mitte-der-nacht", 0.5, 0.5), ("winkel", fw / 60.0, iw / 60.0)] {
                let z = rechnen(d, &o, &probe(methode, "asr-shafi", regel)).unwrap();
                for (zeile, soll) in [(&z[0], auf - fa * nacht), (&z[5], unter + ia * nacht)] {
                    let a = (minuten(&o, zeile.zeit.unwrap(), d) - soll.round() as i64).abs();
                    assert!(a <= 1, "{methode} {regel} {}: {a} min daneben (hergeleitet {soll:.1} min)", zeile.regel);
                    assert_eq!(zeile.hochbreite, Some(Hochbreite::NichtErreicht), "{methode} {regel} {}", zeile.regel);
                    groesste = groesste.max(a);
                }
            }
        }
        assert!(groesste <= 1, "größte Abweichung {groesste} min");
    }

    /// Tromsø zur Mitternachtssonne: Dhuhr und Asr mit Zeit, der Rest ohne, ohne Hinweis zur Hochbreite.
    #[test]
    fn polargebiet() {
        let o = ort(69.6492, 18.9553, "Europe/Oslo");
        for v in ["fuenf-mwl", "fuenf-isna", "drei-mwl"] {
            let z = zeiten("2027-06-21", &o, v);
            for (regel, zeit, hb) in &z {
                let soll = matches!(regel.as_str(), "dhuhr" | "asr-shafi" | "asr-hanafi");
                assert_eq!(zeit.is_some(), soll, "{v} {regel}: {z:?}");
                assert_eq!(*hb, None, "{v} {regel}");
            }
        }
    }

    /// Kiritimati (UTC+14 bei 157,5° W): Der Sonnentag ist der Vortag des Kalendertags. Alle Zeiten
    /// liegen am 21.6. in der Reihenfolge des Tags; die Ischa-Schranke rechnet mit der Nacht des
    /// Sonnentags (Untergang an ihm bis Aufgang am nächsten) und greift in den Tropen nicht.
    #[test]
    fn kiritimati_sonnentag() {
        let d = tag("2027-06-21");
        let o = ort(1.9, -157.5, "Pacific/Kiritimati");
        assert_eq!(tag::sonnentag(d, &o), d.pred_opt());
        let z = zeiten("2027-06-21", &o, "fuenf-mwl");
        let texte: Vec<String> = z.iter().map(|x| x.1.clone().unwrap()).collect();
        assert!(texte.iter().all(|t| t.len() == 5) && texte.windows(2).all(|w| w[0] < w[1]), "{z:?}");
        assert!(texte[2].starts_with("12:"), "Dhuhr {}", texte[2]);
        let r = rechnen(d, &o, &probe("mwl", "asr-shafi", "siebtel")).unwrap();
        let c = tag::sonnentag(d, &o).unwrap();
        let um = |s: Sonnenzeit| match s {
            Sonnenzeit::Um(t) => t,
            x => panic!("{x:?}"),
        };
        let lage = o.lage.unwrap();
        let unter = um(tag::zeit(c, lage, tag::tiefe_aufgang(), false));
        let nacht = um(tag::zeit(c.succ_opt().unwrap(), lage, tag::tiefe_aufgang(), true)) - unter;
        let isha = r[5].zeit.unwrap();
        assert!((0.4..0.6).contains(&nacht) && isha > unter && isha < unter + nacht / 7.0, "Nacht {nacht}, Ischa {isha}, Untergang {unter}");
        assert_eq!(r[5].hochbreite, None);
    }

    /// Umstellungstag Berlin 28.3.2027: Jede Zeit trägt „CEST“ und liegt höchstens 4 min neben der
    /// des Folgetags (kein Sprung um eine Stunde).
    #[test]
    fn umstellungstag() {
        let berlin = ort(52.52, 13.405, "Europe/Berlin");
        let tag1 = zeiten("2027-03-28", &berlin, "fuenf-mwl");
        let tag2 = zeiten("2027-03-29", &berlin, "fuenf-mwl");
        for (a, b) in tag1.iter().zip(&tag2) {
            let (a1, b1) = (a.1.clone().unwrap(), b.1.clone().unwrap());
            assert!(a1.ends_with(" CEST") && b1.len() == 5, "{}: {a1} / {b1}", a.0);
            assert!((uhr(&a1[..5]) - uhr(&b1)).abs() <= 4, "{}: {a1} / {b1}", a.0);
        }
    }
}
