//! Kalender, Stufe 2: Sonnentage und Mondmonate in Ortszeit, auf dem
//! Rechenkern in [`super::himmel`].
//!
//! - Ein Himmelsereignis ist ein Zeitpunkt in Weltzeit; sein Tag ist der Tag in
//!   Ortszeit ([`Ort`]). Für Tests wird die Ortszeit eingesetzt.
//! - Ein Mondmonat läuft von Neumond zu Neumond (Nummer k: ab dem k-ten Neumond).
//!   Welcher Monat einen Zeitpunkt enthält, entscheiden Zeitpunkte, nicht Tage.
//! - Der kürzeste Tag (Wintersonnenwende) hängt von der Halbkugel ab; sie folgt
//!   aus der Breite des Orts ([`Ort::an`]), ohne Lage gilt der Norden.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Offset, TimeZone, Timelike};

use super::himmel;

/// Die vier Sonnenereignisse, benannt nach dem Monat, in dem sie liegen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sonnenereignis {
    SonnenwendeDezember,
    GleicheMaerz,
    SonnenwendeJuni,
    GleicheSeptember,
}

impl Sonnenereignis {
    pub const ALLE: [(&'static str, Sonnenereignis); 4] = [
        ("sonnenwende-dezember", Sonnenereignis::SonnenwendeDezember),
        ("gleiche-maerz", Sonnenereignis::GleicheMaerz),
        ("sonnenwende-juni", Sonnenereignis::SonnenwendeJuni),
        ("gleiche-september", Sonnenereignis::GleicheSeptember),
    ];

    pub fn aus(text: &str) -> Option<Sonnenereignis> {
        Self::ALLE.iter().find(|(n, _)| *n == text).map(|(_, s)| *s)
    }

    /// Länge der Sonne in Grad und der Monat, in dem das Ereignis liegt.
    fn laenge_und_monat(self) -> (f64, u32) {
        match self {
            Sonnenereignis::GleicheMaerz => (0.0, 3),
            Sonnenereignis::SonnenwendeJuni => (90.0, 6),
            Sonnenereignis::GleicheSeptember => (180.0, 9),
            Sonnenereignis::SonnenwendeDezember => (270.0, 12),
        }
    }

    /// Der Zeitpunkt (Weltzeit) im Jahr `jahr`.
    pub fn zeitpunkt(self, jahr: i32) -> Option<f64> {
        let (laenge, monat) = self.laenge_und_monat();
        // Ab dem Ersten des Monats suchen: Die Ereignisse liegen zwischen dem 19. und 23.
        Some(himmel::sonnenlaenge_ab(laenge, tag_zahl(NaiveDate::from_ymd_opt(jahr, monat, 1)?)))
    }

    /// Das Ereignis auf der anderen Halbkugel in derselben Rolle (Wende im
    /// Winter, Gleiche im Frühling …): Dezember ↔ Juni, März ↔ September.
    pub fn gespiegelt(self) -> Sonnenereignis {
        match self {
            Sonnenereignis::SonnenwendeDezember => Sonnenereignis::SonnenwendeJuni,
            Sonnenereignis::SonnenwendeJuni => Sonnenereignis::SonnenwendeDezember,
            Sonnenereignis::GleicheMaerz => Sonnenereignis::GleicheSeptember,
            Sonnenereignis::GleicheSeptember => Sonnenereignis::GleicheMaerz,
        }
    }

    /// Name für Sätze, etwa „Sonnenwende im Dezember“.
    pub fn name(self) -> &'static str {
        match self {
            Sonnenereignis::SonnenwendeDezember => "Sonnenwende im Dezember",
            Sonnenereignis::GleicheMaerz => "Tag-und-Nacht-Gleiche im März",
            Sonnenereignis::SonnenwendeJuni => "Sonnenwende im Juni",
            Sonnenereignis::GleicheSeptember => "Tag-und-Nacht-Gleiche im September",
        }
    }
}

/// Ein Sonnentag in einer Regel: fest nach dem Monat (`sonnenwende-dezember` …)
/// oder nach seiner Rolle im Jahr (`wende-winter` …), die der Rechenort auf einen
/// Monat abbildet: Auf der Südhalbkugel ist die Wende im Winter die im Juni.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Himmelstag {
    Monat(Sonnenereignis),
    /// Die Rolle, benannt nach dem Ereignis, das sie auf der Nordhalbkugel hat.
    Rolle(Sonnenereignis),
}

impl Himmelstag {
    pub const ROLLEN: [(&'static str, Sonnenereignis); 4] = [
        ("wende-winter", Sonnenereignis::SonnenwendeDezember),
        ("gleiche-fruehling", Sonnenereignis::GleicheMaerz),
        ("wende-sommer", Sonnenereignis::SonnenwendeJuni),
        ("gleiche-herbst", Sonnenereignis::GleicheSeptember),
    ];

    pub fn aus(text: &str) -> Option<Himmelstag> {
        Sonnenereignis::aus(text).map(Himmelstag::Monat).or_else(|| Self::ROLLEN.iter().find(|(n, _)| *n == text).map(|(_, s)| Himmelstag::Rolle(*s)))
    }

    /// Das Ereignis auf der Halbkugel `h`.
    pub fn am(self, h: Halbkugel) -> Sonnenereignis {
        match (self, h) {
            (Himmelstag::Monat(s), _) | (Himmelstag::Rolle(s), Halbkugel::Nord) => s,
            (Himmelstag::Rolle(s), Halbkugel::Sued) => s.gespiegelt(),
        }
    }
}

/// Die vier Mondphasen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Neumond,
    ErstesViertel,
    Vollmond,
    LetztesViertel,
}

impl Phase {
    pub const ALLE: [(&'static str, Phase); 4] =
        [("neumond", Phase::Neumond), ("erstes-viertel", Phase::ErstesViertel), ("vollmond", Phase::Vollmond), ("letztes-viertel", Phase::LetztesViertel)];

    pub fn aus(text: &str) -> Option<Phase> {
        Self::ALLE.iter().find(|(n, _)| *n == text).map(|(_, p)| *p)
    }

    pub fn grad(self) -> f64 {
        match self {
            Phase::Neumond => 0.0,
            Phase::ErstesViertel => 90.0,
            Phase::Vollmond => 180.0,
            Phase::LetztesViertel => 270.0,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Phase::Neumond => "Neumond",
            Phase::ErstesViertel => "erstes Viertel",
            Phase::Vollmond => "Vollmond",
            Phase::LetztesViertel => "letztes Viertel",
        }
    }
}

/// Die Halbkugel bestimmt, welche Sonnenwende der kürzeste Tag ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Halbkugel {
    Nord,
    Sued,
}

/// Grenze der Tropen in Grad Breite: die Wendekreise. Dazwischen folgen die
/// Jahreszeiten dem Regen, nicht der Tageslänge (Konzept Abschnitt 3a, Frage 14).
pub const WENDEKREIS: f64 = 23.44;

/// Wie die Uhr am Ort geht.
#[derive(Debug, Clone)]
pub enum Zeit {
    /// Die Ortszeit von Windows (ohne Lage: Verhalten der Stufe 2).
    Windows,
    /// Eine Zeitzone mit ihrer Geschichte (IANA, etwa Europe/Berlin).
    Zone(super::zone::Zone),
    /// Ein fester Versatz zur Weltzeit in Sekunden; `naeherung`: aus der Länge geschätzt.
    Fest { sekunden: i32, naeherung: bool },
    /// Eine Regel als Funktion (Tests).
    Test(fn(NaiveDateTime) -> i32),
}

impl Zeit {
    /// Die Zeitart für die Anzeige, etwa „Europe/Berlin“ oder „Ortszeit Windows“.
    /// Ein fester Versatz steht ohne Zahl da (er verriete die Länge).
    pub fn art(&self) -> String {
        match self {
            Zeit::Windows => "Ortszeit Windows".into(),
            Zeit::Zone(z) => z.name().into(),
            Zeit::Fest { naeherung: true, .. } => "fester Versatz, Näherung nach der Länge".into(),
            Zeit::Fest { .. } => "fester Versatz".into(),
            Zeit::Test(_) => "Ortszeit".into(),
        }
    }
}

/// Unix-Zeitpunkt 0 als Tag des Rechenkerns (1.1.1970).
const UNIX_TAG: f64 = 719_163.0;

fn unix_sekunden(t: f64) -> i64 {
    ((t - UNIX_TAG) * 86400.0).round() as i64
}

/// Lage eines Orts in Grad: Breite (Norden positiv), Länge (Osten positiv).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lage {
    pub breite: f64,
    pub laenge: f64,
}

/// Der Rechenort: Halbkugel, Uhr und, wenn bekannt, die Lage. Ohne Lage gibt es
/// keine Tageszeiten (Verhalten der Stufe 2).
#[derive(Debug, Clone)]
pub struct Ort {
    pub halbkugel: Halbkugel,
    pub zeit: Zeit,
    pub lage: Option<Lage>,
}

/// Der Tag als Zahl wie im Rechenkern (Mitternacht Weltzeit).
pub fn tag_zahl(d: NaiveDate) -> f64 {
    f64::from(d.num_days_from_ce())
}

/// Ein Zeitpunkt des Rechenkerns als Datum mit Uhrzeit in Weltzeit (auf die Sekunde).
pub fn weltzeit(t: f64) -> Option<NaiveDateTime> {
    let tag = t.floor();
    let d = NaiveDate::from_num_days_from_ce_opt(tag as i32)?;
    Some(d.and_hms_opt(0, 0, 0)? + Duration::seconds(((t - tag) * 86400.0).round() as i64))
}

/// Ein Zeitpunkt in Weltzeit als Zahl des Rechenkerns (Gegenstück zu [`weltzeit`]): Tag plus Sekunden seit Mitternacht.
pub fn aus_weltzeit(utc: NaiveDateTime) -> f64 {
    tag_zahl(utc.date()) + f64::from(utc.time().num_seconds_from_midnight()) / 86400.0
}

fn windows_versatz(utc: NaiveDateTime) -> i32 {
    chrono::Local.offset_from_utc_datetime(&utc).fix().local_minus_utc()
}

impl Ort {
    /// Nordhalbkugel in der Ortszeit von Windows, ohne Lage.
    pub fn windows() -> Ort {
        Ort { halbkugel: Halbkugel::Nord, zeit: Zeit::Windows, lage: None }
    }

    /// Ein Ort mit Lage; die Halbkugel folgt aus dem Vorzeichen der Breite (0 = Nord).
    pub fn an(lage: Lage, zeit: Zeit) -> Ort {
        Ort { halbkugel: if lage.breite < 0.0 { Halbkugel::Sued } else { Halbkugel::Nord }, zeit, lage: Some(lage) }
    }

    /// Liegt der Ort zwischen den Wendekreisen?
    pub fn tropen(&self) -> bool {
        self.lage.is_some_and(|l| l.breite.abs() < WENDEKREIS)
    }

    /// Mittlere Ortszeit der Länge in Sekunden (Weltzeit + Länge/15 Stunden).
    fn mittlere_ortszeit(&self) -> Option<i32> {
        self.lage.map(|l| (l.laenge * 240.0).round() as i32)
    }

    /// Führt die Zone zum Zeitpunkt `t` noch mittlere Ortszeit (vor ihrem ersten Wechsel)?
    pub fn ist_lmt(&self, t: f64) -> bool {
        matches!(&self.zeit, Zeit::Zone(z) if z.ist_lmt(unix_sekunden(t)))
    }

    /// Versatz zur Weltzeit in Sekunden zum Zeitpunkt `t`. Solange die Zone
    /// mittlere Ortszeit führt, die des Orts selbst (nicht die ihrer Hauptstadt).
    pub fn versatz(&self, t: f64) -> i32 {
        match &self.zeit {
            Zeit::Zone(z) => match self.mittlere_ortszeit() {
                Some(m) if self.ist_lmt(t) => m,
                _ => z.versatz(unix_sekunden(t)),
            },
            Zeit::Windows => weltzeit(t).map_or(0, windows_versatz),
            Zeit::Fest { sekunden, .. } => *sekunden,
            Zeit::Test(f) => weltzeit(t).map_or(0, f),
        }
    }

    /// Zeitart zum Zeitpunkt `t`: „mittlere Ortszeit“ vor dem ersten Wechsel der Zone, sonst [`Zeit::art`].
    pub fn zeitart_um(&self, t: f64) -> String {
        if self.ist_lmt(t) { "mittlere Ortszeit".into() } else { self.zeit.art() }
    }

    /// Kürzel der Zone zum Zeitpunkt `t` (etwa „CEST“), nur bei einer Zone.
    pub fn kuerzel(&self, t: f64) -> Option<String> {
        match &self.zeit {
            Zeit::Zone(z) => Some(z.kuerzel(unix_sekunden(t))),
            _ => None,
        }
    }

    /// Datum und Uhrzeit in Ortszeit.
    pub fn ortszeit(&self, t: f64) -> Option<NaiveDateTime> {
        weltzeit(t)?.checked_add_signed(Duration::seconds(i64::from(self.versatz(t))))
    }

    /// Der Tag in Ortszeit.
    pub fn tag(&self, t: f64) -> Option<NaiveDate> {
        self.ortszeit(t).map(|x| x.date())
    }

    /// Der Zeitpunkt, an dem der Tag `d` in Ortszeit beginnt: der früheste mit
    /// diesem Datum (fehlt Mitternacht, der Moment der Umstellung; gibt es sie
    /// zweimal, die frühere). `None`, wenn es den Tag am Ort nicht gibt.
    pub fn beginn(&self, d: NaiveDate) -> Option<f64> {
        let t = tag_zahl(d);
        if let Zeit::Zone(z) = &self.zeit {
            let b = UNIX_TAG + z.tagesbeginn(d)? as f64 / 86400.0;
            // Vor dem ersten Wechsel: Mitternacht der eigenen mittleren Ortszeit.
            return Some(match self.mittlere_ortszeit() {
                Some(m) if self.ist_lmt(b) => t - f64::from(m) / 86400.0,
                _ => b,
            });
        }
        // Ohne Zone (Windows, fester Versatz, Test): die früheste Mitternacht nach einem der Versätze um den Tag.
        let v = |x: f64| f64::from(self.versatz(x)) / 86400.0;
        Some([-0.5, 0.0, 0.5].into_iter().map(|s| t - v(t + s)).filter(|b| self.tag(*b) == Some(d)).min_by(f64::total_cmp).unwrap_or(t - v(t)))
    }

    /// Beginn des Tags `d` oder, wenn es ihn am Ort nicht gibt, des nächsten vorhandenen.
    pub fn beginn_ab(&self, d: NaiveDate) -> Option<f64> {
        d.iter_days().take(3).find_map(|x| self.beginn(x))
    }

    /// Der kürzeste Tag (Wintersonnenwende) vor dem Festjahr `jahr`: im Norden
    /// im Dezember davor, im Süden im Juni desselben Jahres.
    pub fn kuerzester_tag(&self, jahr: i32) -> Option<f64> {
        match self.halbkugel {
            Halbkugel::Nord => Sonnenereignis::SonnenwendeDezember.zeitpunkt(jahr - 1),
            Halbkugel::Sued => Sonnenereignis::SonnenwendeJuni.zeitpunkt(jahr),
        }
    }

    /// Der Mondmonat um den `n`-ten Vollmond nach dem kürzesten Tag vor dem
    /// Festjahr `jahr`. Gezählt wird ab dem Tag danach: Ein Vollmond am Tag der
    /// Wende zählt nicht mit.
    pub fn monat_um_vollmond(&self, jahr: i32, n: u8) -> Option<i32> {
        let wende = self.kuerzester_tag(jahr)?;
        let wendetag = self.tag(wende)?;
        let k = monat_bei(wende);
        let erster = if self.tag(vollmond(k))? > wendetag { k } else { k + 1 };
        Some(erster + i32::from(n) - 1)
    }

    /// Der Mondmonat, in dem der Tag `d` beginnt, wenn an ihm ein Neumond liegt;
    /// sonst der Monat, der den Tag enthält. So gehört der Anfangstag eines
    /// Monats immer zu ihm.
    pub fn monat_ab_tag(&self, d: NaiveDate) -> i32 {
        monat_bei(self.beginn_ab(d.succ_opt().unwrap_or(d)).unwrap_or_else(|| tag_zahl(d) + 1.0) - 1e-6)
    }
}

/// Der Mondmonat, der den Zeitpunkt `t` enthält (Neumond k ≤ t < Neumond k + 1).
pub fn monat_bei(t: f64) -> i32 {
    let n = himmel::neumond_ab(t);
    if himmel::neumond(n) > t { n - 1 } else { n }
}

/// Anfang (Neumond) des Monats k.
pub fn anfang(k: i32) -> f64 {
    himmel::neumond(k)
}

/// Der Vollmond im Monat k.
pub fn vollmond(k: i32) -> f64 {
    himmel::phase_ab(180.0, himmel::neumond(k))
}

/// Ortszeit für Tests: Mitteleuropa mit Sommerzeit vom letzten Sonntag im März
/// bis zum letzten Sonntag im Oktober, je 1 Uhr Weltzeit.
#[cfg(test)]
pub fn mitteleuropa(utc: NaiveDateTime) -> i32 {
    use chrono::Weekday;
    let letzter_sonntag = |monat: u32| {
        let mut d = NaiveDate::from_ymd_opt(utc.year(), monat, 31).unwrap_or_default();
        while d.weekday() != Weekday::Sun {
            d = d.pred_opt().unwrap_or(d);
        }
        d.and_hms_opt(1, 0, 0).unwrap_or_default()
    };
    if utc >= letzter_sonntag(3) && utc < letzter_sonntag(10) { 7200 } else { 3600 }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Wendekreise: 23,4° liegt in den Tropen, 23,5° nicht; Breite 0 zählt zum Norden.
    #[test]
    fn wendekreis_grenze() {
        let an = |breite: f64| Ort::an(Lage { breite, laenge: 0.0 }, Zeit::Fest { sekunden: 0, naeherung: false });
        assert!(an(23.4).tropen() && an(-23.4).tropen());
        assert!(!an(23.5).tropen() && !an(-23.5).tropen());
        assert!(an(0.0).tropen());
        assert_eq!(an(0.0).halbkugel, Halbkugel::Nord);
        assert_eq!(an(-0.1).halbkugel, Halbkugel::Sued);
        assert!(!Ort::windows().tropen(), "ohne Lage keine Tropen");
    }

    /// Weltzeit als Zahl: Mitternacht ist der Tag selbst, Mittag ein halber Tag mehr; hin und zurück gleich.
    #[test]
    fn aus_weltzeit_und_zurueck() {
        let mitternacht = NaiveDate::from_ymd_opt(2027, 6, 21).unwrap().and_hms_opt(0, 0, 0).unwrap();
        assert_eq!(aus_weltzeit(mitternacht), tag_zahl(mitternacht.date()));
        assert_eq!(aus_weltzeit(mitternacht + Duration::hours(12)), tag_zahl(mitternacht.date()) + 0.5);
        let z = NaiveDate::from_ymd_opt(2100, 12, 31).unwrap().and_hms_opt(23, 59, 59).unwrap();
        assert_eq!(weltzeit(aus_weltzeit(z)), Some(z));
    }
}
