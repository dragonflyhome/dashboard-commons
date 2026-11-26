//! Zeitzonen mit Geschichte (Kalender, Stufe 3): ein schmaler Baustein auf
//! jiff samt mitgelieferter IANA-Datenbank. Nach außen gehen nur Unix-Sekunden,
//! Zahlen und Texte; chrono bleibt die Datumsbibliothek im übrigen Code, und
//! Fehlertexte von jiff gehen nie hinaus (sie nennen die Zone).

use chrono::{Datelike, NaiveDate};
use jiff::Timestamp;
use jiff::tz::TimeZone;

/// Eine Zeitzone der IANA-Datenbank.
#[derive(Debug, Clone)]
pub struct Zone(TimeZone);

impl PartialEq for Zone {
    fn eq(&self, other: &Self) -> bool {
        self.name() == other.name()
    }
}

/// Die Zone zu einem IANA-Namen, etwa `Europe/Berlin`; `None`, wenn es sie nicht gibt.
pub fn zone(name: &str) -> Option<Zone> {
    jiff::tz::db().get(name).ok().map(Zone)
}

/// Alle Namen der Datenbank, sortiert.
pub fn zonen() -> Vec<String> {
    let mut v: Vec<String> = jiff::tz::db().available().map(|n| n.to_string()).collect();
    v.sort();
    v
}

/// Fassung der mitgelieferten Datenbank, etwa „2026a“.
pub fn tzdb_fassung() -> &'static str {
    let v = jiff_tzdb::VERSION;
    if v.trim().is_empty() { "unbekannt" } else { v }
}

fn zeitpunkt(sek: i64) -> Timestamp {
    Timestamp::from_second(sek).unwrap_or(Timestamp::UNIX_EPOCH)
}

impl Zone {
    pub fn name(&self) -> &str {
        self.0.iana_name().unwrap_or("")
    }

    /// Versatz zur Weltzeit in Sekunden zum Unix-Zeitpunkt `sek`.
    pub fn versatz(&self, sek: i64) -> i32 {
        self.0.to_offset(zeitpunkt(sek)).seconds()
    }

    /// Kürzel des geltenden Versatzes, etwa „CEST“ oder „LMT“.
    pub fn kuerzel(&self, sek: i64) -> String {
        self.0.to_offset_info(zeitpunkt(sek)).abbreviation().to_string()
    }

    /// Führt die Zone hier noch mittlere Ortszeit (Kürzel „LMT“, vor ihrem ersten Wechsel)?
    pub fn ist_lmt(&self, sek: i64) -> bool {
        self.kuerzel(sek) == "LMT"
    }

    /// Unix-Zeitpunkt, an dem der Tag `d` in dieser Zone beginnt: die früheste
    /// Zeit mit diesem Datum (fehlt Mitternacht, der Moment der Umstellung; gibt
    /// es sie zweimal, die frühere). `None`, wenn es den Tag in der Zone nicht gibt.
    pub fn tagesbeginn(&self, d: NaiveDate) -> Option<i64> {
        let datum = jiff::civil::Date::new(i16::try_from(d.year()).ok()?, i8::try_from(d.month()).ok()?, i8::try_from(d.day()).ok()?).ok()?;
        let z = datum.to_zoned_datetime(self.0.clone()).ok()?;
        (z.date() == datum).then(|| z.timestamp().as_second())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(t: &str) -> NaiveDate {
        NaiveDate::parse_from_str(t, "%Y-%m-%d").unwrap()
    }

    fn sek(t: &str) -> i64 {
        chrono::NaiveDateTime::parse_from_str(t, "%Y-%m-%d %H:%M").unwrap().and_utc().timestamp()
    }

    #[test]
    fn grundlagen() {
        let b = zone("Europe/Berlin").unwrap();
        assert_eq!(b.name(), "Europe/Berlin");
        assert_eq!(b.versatz(sek("2027-07-01 12:00")), 7200);
        assert_eq!(b.kuerzel(sek("2027-01-01 12:00")), "CET");
        assert!(zone("Mond/Mitte").is_none());
        assert!(zonen().iter().any(|z| z == "Asia/Kolkata"));
        assert!(tzdb_fassung().starts_with("20"), "{}", tzdb_fassung());
        // Apia übersprang den 30.12.2011.
        let apia = zone("Pacific/Apia").unwrap();
        assert_eq!(apia.tagesbeginn(tag("2011-12-30")), None);
        assert_eq!(apia.tagesbeginn(tag("2011-12-31")), Some(sek("2011-12-30 10:00")));
    }

    fn sek_s(t: &str) -> i64 {
        chrono::NaiveDateTime::parse_from_str(t, "%Y-%m-%d %H:%M:%S").unwrap().and_utc().timestamp()
    }

    /// Wanduhr in der Zone zu `s` mit dem Versatz `v`, als „JJJJ-MM-TT HH:MM:SS“.
    fn wand(s: i64, v: i32) -> String {
        chrono::DateTime::from_timestamp(s + i64::from(v), 0).unwrap().naive_utc().format("%Y-%m-%d %H:%M:%S").to_string()
    }

    /// Konzept 5b, Zonentabelle (dazu Berlin 1945 aus Nachtrag 7): je Wechsel
    /// Weltzeit, Wanduhr und Versatz eine Sekunde vorher und im Moment selbst.
    const TABELLE: [(&str, &str, &str, i32, &str, i32); 17] = [
        ("Europe/Berlin", "1893-03-31 23:06:32", "1893-04-01 00:00:00", 3208, "1893-04-01 00:06:32", 3600),
        ("Europe/Berlin", "1916-04-30 22:00:00", "1916-04-30 23:00:00", 3600, "1916-05-01 00:00:00", 7200),
        ("Europe/Berlin", "1945-05-24 00:00:00", "1945-05-24 02:00:00", 7200, "1945-05-24 03:00:00", 10800),
        ("Europe/Berlin", "1945-09-24 00:00:00", "1945-09-24 03:00:00", 10800, "1945-09-24 02:00:00", 7200),
        ("Europe/Berlin", "1945-11-18 01:00:00", "1945-11-18 03:00:00", 7200, "1945-11-18 02:00:00", 3600),
        ("Europe/Berlin", "1947-05-11 01:00:00", "1947-05-11 03:00:00", 7200, "1947-05-11 04:00:00", 10800),
        ("Europe/Berlin", "1947-06-29 00:00:00", "1947-06-29 03:00:00", 10800, "1947-06-29 02:00:00", 7200),
        ("Europe/Berlin", "1980-04-06 01:00:00", "1980-04-06 02:00:00", 3600, "1980-04-06 03:00:00", 7200),
        ("Europe/Moscow", "2011-03-26 23:00:00", "2011-03-27 02:00:00", 10800, "2011-03-27 03:00:00", 14400),
        ("Europe/Moscow", "2014-10-25 22:00:00", "2014-10-26 02:00:00", 14400, "2014-10-26 01:00:00", 10800),
        ("Pacific/Apia", "2011-12-30 10:00:00", "2011-12-30 00:00:00", -36000, "2011-12-31 00:00:00", 50400),
        ("America/Sao_Paulo", "2019-02-17 02:00:00", "2019-02-17 00:00:00", -7200, "2019-02-16 23:00:00", -10800),
        ("Australia/Lord_Howe", "2025-10-04 15:30:00", "2025-10-05 02:00:00", 37800, "2025-10-05 02:30:00", 39600),
        ("Asia/Kathmandu", "1985-12-31 18:30:00", "1986-01-01 00:00:00", 19800, "1986-01-01 00:15:00", 20700),
        ("Europe/Dublin", "2025-10-26 01:00:00", "2025-10-26 02:00:00", 3600, "2025-10-26 01:00:00", 0),
        ("America/Santiago", "2025-09-07 04:00:00", "2025-09-07 00:00:00", -14400, "2025-09-07 01:00:00", -10800),
        ("Asia/Beirut", "2025-03-29 22:00:00", "2025-03-30 00:00:00", 7200, "2025-03-30 01:00:00", 10800),
    ];

    #[test]
    fn zonentabelle_konzept_5b() {
        let f = tzdb_fassung();
        for (name, utc, vor, v_vor, nach, v_nach) in TABELLE {
            let z = zone(name).unwrap_or_else(|| panic!("{name} fehlt in tzdb {f}"));
            let s = sek_s(utc);
            let fall = format!("{name} {utc} UT, tzdb {f}");
            assert_eq!(z.versatz(s - 1), v_vor, "Versatz vorher: {fall}");
            assert_eq!(z.versatz(s), v_nach, "Versatz nachher: {fall}");
            assert_eq!(wand(s, v_vor), vor, "Wanduhr vorher: {fall}");
            assert_eq!(wand(s, v_nach), nach, "Wanduhr nachher: {fall}");
        }
    }

    #[test]
    fn kuerzel_an_den_wechseln() {
        let f = tzdb_fassung();
        let k = |name: &str, utc: &str| zone(name).unwrap().kuerzel(sek_s(utc));
        for (name, utc, erwartet) in [
            ("Europe/Berlin", "1893-03-31 23:06:31", "LMT"),
            ("Europe/Berlin", "1893-03-31 23:06:32", "CET"),
            ("Europe/Berlin", "1916-04-30 22:00:00", "CEST"),
            ("Europe/Berlin", "1945-05-24 00:00:00", "CEMT"),
            ("Europe/Berlin", "1947-05-11 01:00:00", "CEMT"),
            ("Europe/Berlin", "1980-04-06 01:00:00", "CEST"),
            ("Europe/Dublin", "2025-10-26 00:59:59", "IST"),
            ("Europe/Dublin", "2025-10-26 01:00:00", "GMT"),
            ("Asia/Kathmandu", "1985-12-31 18:30:00", "+0545"),
            ("Australia/Lord_Howe", "2025-10-04 15:30:00", "+11"),
        ] {
            assert_eq!(k(name, utc), erwartet, "{name} {utc} UT, tzdb {f}");
        }
        let b = zone("Europe/Berlin").unwrap();
        assert!(b.ist_lmt(sek_s("1893-03-31 23:06:31")) && !b.ist_lmt(sek_s("1893-03-31 23:06:32")), "tzdb {f}");
    }

    /// Tagesbeginn: fehlende Mitternacht → Moment der Umstellung; fehlender Tag → `None`.
    #[test]
    fn tagesbeginn_ohne_mitternacht() {
        let f = tzdb_fassung();
        let b = |name: &str, d: &str| zone(name).unwrap().tagesbeginn(tag(d));
        assert_eq!(b("America/Santiago", "2025-09-07"), Some(sek("2025-09-07 04:00")), "tzdb {f}");
        assert_eq!(b("America/Santiago", "2025-09-06"), Some(sek("2025-09-06 04:00")), "tzdb {f}");
        assert_eq!(b("Asia/Beirut", "2025-03-30"), Some(sek("2025-03-29 22:00")), "tzdb {f}");
        assert_eq!(b("Asia/Beirut", "2025-03-31"), Some(sek("2025-03-30 21:00")), "tzdb {f}");
        // Einführung der MEZ: 00:00 bis 00:06:32 gibt es am 1.4.1893 nicht.
        assert_eq!(b("Europe/Berlin", "1893-04-01"), Some(sek_s("1893-03-31 23:06:32")), "tzdb {f}");
        // São Paulo 2019: 23:00 bis 24:00 am 16.2. doppelt, Mitternacht nicht.
        assert_eq!(b("America/Sao_Paulo", "2019-02-17"), Some(sek("2019-02-17 03:00")), "tzdb {f}");
        assert_eq!(b("Pacific/Apia", "2011-12-30"), None, "tzdb {f}");
        assert_eq!(b("Pacific/Apia", "2011-12-29"), Some(sek("2011-12-29 10:00")), "tzdb {f}");
    }

    /// Dieselben Wechsel über den Rechenort ([`Ort`]) samt Tagesbeginn und Anzeige.
    #[test]
    fn rechenort_mit_zone() {
        use crate::kalender::mond::{Lage, Ort, Zeit, tag_zahl};
        use crate::kalender::tag::uhrzeit;
        let f = tzdb_fassung();
        let unix = tag_zahl(tag("1970-01-01"));
        let t = |s: i64| unix + s as f64 / 86400.0;
        let mitte = Lage { breite: 51.2, laenge: 10.5 };
        let berlin = Ort::an(mitte, Zeit::Zone(zone("Europe/Berlin").unwrap()));
        for (_, utc, _, v_vor, _, v_nach) in TABELLE.iter().filter(|z| z.0 == "Europe/Berlin").skip(1) {
            let s = sek_s(utc);
            assert_eq!((berlin.versatz(t(s - 1)), berlin.versatz(t(s))), (*v_vor, *v_nach), "{utc}, tzdb {f}");
        }
        // Am Umstellungstag steht das Kürzel hinter der Uhrzeit, sonst nicht.
        assert_eq!(uhrzeit(&berlin, t(sek("1980-04-06 12:00")), tag("1980-04-06")).as_deref(), Some("14:00 CEST"), "tzdb {f}");
        assert_eq!(uhrzeit(&berlin, t(sek("1980-04-07 12:00")), tag("1980-04-07")).as_deref(), Some("14:00"), "tzdb {f}");
        let ort = |name: &str| Ort::an(mitte, Zeit::Zone(zone(name).unwrap()));
        let nah = |a: Option<f64>, b: i64| a.is_some_and(|a| (a - t(b)).abs() < 1e-6);
        assert!(nah(ort("America/Santiago").beginn(tag("2025-09-07")), sek("2025-09-07 04:00")), "tzdb {f}");
        assert!(nah(ort("Asia/Beirut").beginn(tag("2025-03-30")), sek("2025-03-29 22:00")), "tzdb {f}");
        let apia = ort("Pacific/Apia");
        assert_eq!(apia.beginn(tag("2011-12-30")), None, "tzdb {f}");
        assert!(nah(apia.beginn_ab(tag("2011-12-30")), sek("2011-12-30 10:00")), "tzdb {f}");
        let lh = ort("Australia/Lord_Howe");
        let s = sek_s("2025-10-04 15:30:00");
        assert_eq!((lh.versatz(t(s - 1)), lh.versatz(t(s))), (37800, 39600), "tzdb {f}");
    }

    /// Mittlere Ortszeit nur, wo die Zone LMT führt: der Standardort (Länge 10,5°)
    /// am 1.1.1890 bei +0:42:00, Etc/UTC vorher und heute bei 0.
    #[test]
    fn mittlere_ortszeit() {
        use crate::kalender::mond::{Lage, Ort, Zeit, tag_zahl};
        let f = tzdb_fassung();
        let mitte = Lage { breite: 51.2, laenge: 10.5 };
        let berlin = Ort::an(mitte, Zeit::Zone(zone("Europe/Berlin").unwrap()));
        let t = tag_zahl(tag("1890-01-01")) + 0.5;
        assert_eq!(berlin.versatz(t), 42 * 60, "tzdb {f}");
        assert_eq!(berlin.zeitart_um(t), "mittlere Ortszeit");
        assert_eq!(berlin.kuerzel(t).as_deref(), Some("LMT"));
        // Mitternacht der eigenen mittleren Ortszeit: 23:18 UT am Vortag.
        let b = berlin.beginn(tag("1890-01-01")).unwrap();
        assert!((b - (tag_zahl(tag("1890-01-01")) - 42.0 / 1440.0)).abs() < 1e-9, "{b}");
        let heute = tag_zahl(tag("2026-11-26")) + 0.5;
        assert_eq!(berlin.zeitart_um(heute), "Europe/Berlin");
        assert_eq!(berlin.versatz(heute), 3600);
        let utc = Ort::an(mitte, Zeit::Zone(zone("Etc/UTC").unwrap()));
        for d in ["1850-06-01", "1890-01-01", "2026-11-26"] {
            let t = tag_zahl(tag(d)) + 0.5;
            assert_eq!(utc.versatz(t), 0, "{d}, tzdb {f}");
            assert!(!utc.ist_lmt(t), "{d}");
            assert_eq!(utc.zeitart_um(t), "Etc/UTC");
        }
    }
}
