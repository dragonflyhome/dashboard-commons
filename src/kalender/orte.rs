//! Benannte Orte für die Rechnung (Kalender, Stufe 3): der öffentliche
//! Standardort aus `vorlagen/kalender/standorte.toml` und die privaten Orte aus
//! `projects/kalender/daten/standorte.toml`. Heimatort ist der private Ort mit
//! `heimat = true`, sonst der Standardort; nach ihm rechnen Jahreskreis und
//! Mondmonate. Ohne jeden Ort gilt das Verhalten der Stufe 2.

use super::Katalog;
use super::mond::Ort;
use crate::inhalt::store::Bestand;
use crate::vertrag::daten::{Datei, DatenInhalt, datei_lesen};
use crate::vertrag::standorte::Standorte;

/// Das Projekt, in dessen `daten/` die privaten Orte stehen.
pub const PROJEKT: &str = "kalender";

const VORLAGE: &str = include_str!("../../vorlagen/kalender/standorte.toml");

/// Nur in Tests: Liegt ein Projekt dieses Namens im Inhalt, blendet
/// [`Orte::aus_bestand`] die Vorlage aus (Fall „ohne jeden Ort“ wie Stufe 2).
/// Ein Projekt statt eines Schalters im Faden, weil der Inhalt abseits der
/// Tokio-Fäden gelesen wird; so stören sich parallele Tests nicht.
#[cfg(test)]
pub const OHNE_VORLAGE: &str = "test-ohne-vorlage";

/// Die Orte der Vorlage (beim Bauen geprüft, Test `vorlage_gueltig`).
pub fn vorlage() -> Option<Datei<Standorte>> {
    datei_lesen::<Standorte>("vorlagen/kalender/standorte.toml", VORLAGE).ok()
}

/// Ein benannter Ort: Kennung, Titel und der Rechenort.
#[derive(Debug, Clone)]
pub struct Benannt {
    pub kennung: String,
    pub titel: String,
    pub ort: Ort,
    /// Aus der öffentlichen Vorlage (seine Lage ist kein Geheimnis).
    pub oeffentlich: bool,
}

/// Alle benannten Orte, der Heimatort zuerst.
#[derive(Debug, Clone, Default)]
pub struct Orte {
    liste: Vec<Benannt>,
}

impl Orte {
    /// Aus der privaten Datei und der Vorlage. Heimatort: der private mit
    /// `heimat = true`, sonst der erste der Vorlage; er nimmt ohne Zone die
    /// Ortszeit von Windows. Private Kennungen gehen vor gleiche der Vorlage.
    pub fn aus(privat: Option<&Datei<Standorte>>, vorlage: Option<&Datei<Standorte>>) -> Orte {
        let privat = privat.map(|d| d.alle()).unwrap_or_default();
        let vorlage: Vec<_> = vorlage.map(|d| d.alle()).unwrap_or_default().into_iter().filter(|v| privat.iter().all(|p| p.kennung != v.kennung)).collect();
        let heimat = privat.iter().position(|s| s.heimat);
        let mut liste: Vec<Benannt> = Vec::new();
        let neu = |s: &crate::vertrag::standorte::Standort, heimat: bool, oeffentlich: bool| Benannt {
            kennung: s.kennung.clone(),
            titel: s.titel.clone(),
            ort: s.ort(heimat),
            oeffentlich,
        };
        match heimat {
            Some(i) => liste.push(neu(privat[i], true, false)),
            None => {
                if let Some(v) = vorlage.first() {
                    liste.push(neu(v, true, true));
                }
            }
        }
        for (_, s) in privat.iter().enumerate().filter(|(i, _)| Some(*i) != heimat) {
            liste.push(neu(s, false, false));
        }
        for v in vorlage.iter().skip(usize::from(heimat.is_none())) {
            liste.push(neu(v, false, true));
        }
        Orte { liste }
    }

    /// Aus dem Stand des Inhalts und der Vorlage; eine unlesbare private Datei zählt wie keine.
    pub fn aus_bestand(b: &Bestand) -> Orte {
        let privat = match b.daten(PROJEKT, "standorte") {
            Ok(Some((DatenInhalt::Standorte(d), _))) => Some(d),
            _ => None,
        };
        #[cfg(test)]
        let vorlage = if b.projekt(OHNE_VORLAGE).is_some() { None } else { vorlage() };
        #[cfg(not(test))]
        let vorlage = vorlage();
        Orte::aus(privat.as_ref(), vorlage.as_ref())
    }

    pub fn alle(&self) -> &[Benannt] {
        &self.liste
    }

    /// Der Heimatort (der erste).
    pub fn heimat(&self) -> Option<&Benannt> {
        self.liste.first()
    }

    pub fn finden(&self, kennung: &str) -> Option<&Benannt> {
        self.liste.iter().find(|b| b.kennung == kennung)
    }

    /// Der Blickort zur gespeicherten Kennung. Ohne Kennung oder mit einer, die
    /// es (nicht mehr) gibt, der Heimatort: Ein gelöschter Ort lässt den Kopf nicht leer.
    pub fn blickort(&self, kennung: Option<&str>) -> Option<&Benannt> {
        kennung.and_then(|k| self.finden(k)).or_else(|| self.heimat())
    }

    /// Die eingebauten Kalender, gerechnet für den Heimatort (ohne ihn wie in Stufe 2).
    pub fn katalog(&self) -> Katalog {
        let k = Katalog::eingebaut();
        self.heimat().map_or_else(|| k.clone(), |h| k.mit_ort(h.ort.clone()))
    }

    /// Der Katalog für ein Ereignis: mit `ort` für diesen benannten Ort, sonst
    /// für den Heimatort. Ein unbekannter Ort ist ein Satz (nur mit der Kennung).
    pub fn katalog_fuer(&self, ort: Option<&str>) -> Result<(Katalog, Option<&Benannt>), String> {
        match ort {
            None => Ok((self.katalog(), None)),
            Some(k) => match self.finden(k) {
                Some(b) => Ok((Katalog::eingebaut().mit_ort(b.ort.clone()), Some(b))),
                None => Err(satz_unbekannt(k)),
            },
        }
    }
}

/// Satz für einen unbekannten Ort (nur mit der Kennung).
pub fn satz_unbekannt(k: &str) -> String {
    format!(
        "Den Ort „{k}“ gibt es nicht. Benannte Orte stehen in projects/{PROJEKT}/daten/standorte.toml oder vorlagen/kalender/standorte.toml; ein Ereignis verweist mit `ort` auf ihre `kennung`."
    )
}

/// „ · Ort Ferienhaus · Europe/Lisbon“ für die Herkunft (ohne Zahlen).
pub fn ort_satz(b: &Benannt) -> String {
    format!(" · Ort {} · {}", b.titel, b.ort.zeit.art())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vorlage_gueltig() {
        let v = vorlage().expect("vorlagen/kalender/standorte.toml lesbar");
        let mitte = v.alle()[0];
        assert_eq!((mitte.kennung.as_str(), mitte.breite, mitte.laenge), ("mitte", 51.2, 10.5));
        let o = Orte::aus(None, Some(&v));
        let h = o.heimat().unwrap();
        assert!(h.oeffentlich && h.ort.zeit.art() == "Europe/Berlin");
        assert!(Orte::aus(None, None).heimat().is_none());
    }

    #[test]
    fn heimat_aus_der_privaten_datei() {
        let privat = datei_lesen::<Standorte>(
            "standorte.toml",
            "[[standort]]\nkennung = \"ort-2\"\ntitel = \"Zweiter\"\nbreite = 47.3\nlaenge = 73.9\nherkunft = \"hand\"\nzeitzone = \"Asia/Almaty\"\n\n\
             [[standort]]\nkennung = \"zuhause\"\ntitel = \"Zuhause\"\nbreite = -33.9\nlaenge = 151.2\nherkunft = \"hand\"\nzeitzone = \"Australia/Sydney\"\nheimat = true\n",
        )
        .unwrap();
        let o = Orte::aus(Some(&privat), vorlage().as_ref());
        let kennungen: Vec<&str> = o.alle().iter().map(|b| b.kennung.as_str()).collect();
        assert_eq!(kennungen, ["zuhause", "ort-2", "mitte"]);
        assert_eq!(o.katalog().ort().halbkugel, crate::kalender::mond::Halbkugel::Sued);
        assert!(o.katalog_fuer(Some("mars")).unwrap_err().contains("„mars“"));
    }

    #[test]
    fn blickort_faellt_auf_heimat() {
        let zwei = "[[standort]]
kennung = \"ort-2\"
titel = \"Zweiter\"
breite = 47.3
laenge = 73.9
herkunft = \"hand\"
zeitzone = \"Asia/Almaty\"
";
        let mit = datei_lesen::<Standorte>("standorte.toml", zwei).unwrap();
        let o = Orte::aus(Some(&mit), vorlage().as_ref());
        let k = |b: Option<&Benannt>| b.map(|b| b.kennung.clone());
        assert_eq!(k(o.blickort(None)), Some("mitte".into()), "ohne Kennung");
        assert_eq!(k(o.blickort(Some("mars"))), Some("mitte".into()), "unbekannt");
        assert_eq!(k(o.blickort(Some("ort-2"))), Some("ort-2".into()), "bekannt");
        let ohne = Orte::aus(None, vorlage().as_ref());
        assert_eq!(k(ohne.blickort(Some("ort-2"))), Some("mitte".into()), "aus standorte.toml verschwunden");
        assert!(Orte::aus(None, None).blickort(Some("ort-2")).is_none());
    }
}
