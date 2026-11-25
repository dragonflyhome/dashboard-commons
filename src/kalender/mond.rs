//! Kalender, Stufe 2: Sonnentage und Mondmonate in Ortszeit, auf dem
//! Rechenkern in [`super::himmel`].
//!
//! - Ein Himmelsereignis ist ein Zeitpunkt in Weltzeit; sein Tag ist der Tag in
//!   Ortszeit ([`Ort`]). Für Tests wird die Ortszeit eingesetzt.
//! - Ein Mondmonat läuft von Neumond zu Neumond (Nummer k: ab dem k-ten Neumond).
//!   Welcher Monat einen Zeitpunkt enthält, entscheiden Zeitpunkte, nicht Tage.
//! - Der kürzeste Tag (Wintersonnenwende) hängt von der Halbkugel ab; sie ist ein
//!   Eingang des Orts, Vorgabe Norden (die Wahl aus dem Ort kommt mit Stufe 3).

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, Offset, TimeZone};

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

/// Wo gerechnet wird: Halbkugel und Ortszeit (Versatz zur Weltzeit in
/// Sekunden für einen Zeitpunkt in Weltzeit).
#[derive(Debug, Clone, Copy)]
pub struct Ort {
    pub halbkugel: Halbkugel,
    pub versatz: fn(NaiveDateTime) -> i32,
}

/// Der Tag als Zahl wie im Rechenkern (Mitternacht Weltzeit).
pub fn tag_zahl(d: NaiveDate) -> f64 {
    f64::from(d.num_days_from_ce())
}

/// Ein Zeitpunkt des Rechenkerns als Datum mit Uhrzeit in Weltzeit (auf die Sekunde).
fn weltzeit(t: f64) -> Option<NaiveDateTime> {
    let tag = t.floor();
    let d = NaiveDate::from_num_days_from_ce_opt(tag as i32)?;
    Some(d.and_hms_opt(0, 0, 0)? + Duration::seconds(((t - tag) * 86400.0).round() as i64))
}

fn windows_versatz(utc: NaiveDateTime) -> i32 {
    chrono::Local.offset_from_utc_datetime(&utc).fix().local_minus_utc()
}

impl Ort {
    /// Nordhalbkugel in der Ortszeit von Windows.
    pub fn windows() -> Ort {
        Ort { halbkugel: Halbkugel::Nord, versatz: windows_versatz }
    }

    /// Datum und Uhrzeit in Ortszeit.
    pub fn ortszeit(&self, t: f64) -> Option<NaiveDateTime> {
        let u = weltzeit(t)?;
        u.checked_add_signed(Duration::seconds(i64::from((self.versatz)(u))))
    }

    /// Der Tag in Ortszeit.
    pub fn tag(&self, t: f64) -> Option<NaiveDate> {
        self.ortszeit(t).map(|x| x.date())
    }

    /// Der Zeitpunkt, an dem der Tag `d` in Ortszeit beginnt. Der Versatz gilt
    /// um Mitternacht Weltzeit; die Zeitumstellung liegt nachts nach 1 Uhr.
    // shortcut: In Zonen, die vor 1 Uhr Weltzeit umstellen (etwa Sydney), liegt der Beginn am
    // Umstellungstag eine Stunde daneben; aufrüsten mit dem Versatz zur Ortsmitternacht, wenn mit Stufe 3 Orte kommen.
    pub fn beginn(&self, d: NaiveDate) -> f64 {
        let t = tag_zahl(d);
        let v = weltzeit(t).map_or(0, self.versatz);
        t - f64::from(v) / 86400.0
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
        monat_bei(self.beginn(d.succ_opt().unwrap_or(d)) - 1e-6)
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
