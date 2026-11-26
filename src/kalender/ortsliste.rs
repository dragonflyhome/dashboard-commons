//! Ortsliste zum Suchen einer Stadt (Kalender, Stufe 3): eine gekürzte Liste
//! aus GeoNames `cities15000`. Wo Treffer angezeigt werden, steht der Hinweis
//! „GeoNames (geonames.org), CC BY 4.0“.
//!
//! Format: UTF-8, eine Stadt je Zeile, Felder durch Tabulator getrennt;
//! Zeilen mit `#` am Anfang und leere Zeilen zählen nicht:
//!
//! `name<TAB>nebennamen<TAB>land<TAB>region<TAB>breite<TAB>laenge<TAB>zeitzone`
//!
//! - `nebennamen`: weitere Namen, durch Komma getrennt (darf leer sein),
//! - `land`: Ländercode aus zwei Buchstaben,
//! - `breite`, `laenge`: Grad als Dezimalzahl (−90 bis 90, −180 bis 180),
//! - `zeitzone`: IANA-Name, etwa `Europe/Berlin`.
//!
//! Fehlersätze nennen nur die Zeilennummer, nie den Inhalt der Zeile und nie
//! eine Koordinate.

/// Eine Stadt der Liste. Bewusst ohne `Serialize`: Koordinaten sollen nie
/// versehentlich nach außen gehen.
#[derive(Debug, Clone, PartialEq)]
pub struct Stadt {
    pub name: String,
    pub land: String,
    pub region: String,
    pub breite: f64,
    pub laenge: f64,
    pub zeitzone: String,
}

/// Die gelesene Liste mit den gefalteten Suchnamen je Stadt.
#[derive(Debug, Clone, Default)]
pub struct Ortsliste {
    staedte: Vec<Stadt>,
    /// Je Stadt der gefaltete Name und die gefalteten Nebennamen.
    schluessel: Vec<Vec<String>>,
}

impl Ortsliste {
    /// Liest die Liste; die erste fehlerhafte Zeile bricht ab.
    pub fn lesen(text: &str) -> Result<Ortsliste, String> {
        let mut liste = Ortsliste::default();
        for (i, zeile) in text.lines().enumerate() {
            let zeile = zeile.trim_end_matches('\r');
            if zeile.trim().is_empty() || zeile.starts_with('#') {
                continue;
            }
            let nr = i + 1;
            let felder: Vec<&str> = zeile.split('\t').collect();
            let [name, neben, land, region, breite, laenge, zeitzone] = felder[..] else {
                return Err(format!("Zeile {nr} der Ortsliste hat nicht genau sieben Felder."));
            };
            let name = name.trim();
            if name.is_empty() {
                return Err(format!("Zeile {nr} der Ortsliste hat keinen Namen."));
            }
            let land = land.trim();
            if land.len() != 2 || !land.chars().all(|c| c.is_ascii_alphabetic()) {
                return Err(format!("Zeile {nr} der Ortsliste hat keinen Ländercode aus zwei Buchstaben."));
            }
            let breite = grad(breite, 90.0).ok_or_else(|| format!("Zeile {nr} der Ortsliste hat keine gültige Breite."))?;
            let laenge = grad(laenge, 180.0).ok_or_else(|| format!("Zeile {nr} der Ortsliste hat keine gültige Länge."))?;
            let zeitzone = zeitzone.trim();
            if super::zone::zone(zeitzone).is_none() {
                return Err(format!("Zeile {nr} der Ortsliste hat keine bekannte Zeitzone."));
            }
            let mut s = vec![falten(name)];
            s.extend(neben.split(',').map(str::trim).filter(|n| !n.is_empty()).map(falten));
            liste.schluessel.push(s);
            liste.staedte.push(Stadt {
                name: name.to_string(),
                land: land.to_ascii_uppercase(),
                region: region.trim().to_string(),
                breite,
                laenge,
                zeitzone: zeitzone.to_string(),
            });
        }
        Ok(liste)
    }

    pub fn alle(&self) -> &[Stadt] {
        &self.staedte
    }

    /// Städte, deren Name oder Nebenname zur Anfrage passt, ohne Rücksicht auf
    /// Groß- und Kleinschreibung und Akzente: erst genaue Treffer, dann solche,
    /// die mit der Anfrage beginnen, sonst in der Reihenfolge der Liste.
    pub fn suchen(&self, anfrage: &str, hoechstens: usize) -> Vec<&Stadt> {
        let q = falten(anfrage.trim());
        if q.is_empty() {
            return Vec::new();
        }
        let mut treffer: Vec<(u8, &Stadt)> = self
            .staedte
            .iter()
            .zip(&self.schluessel)
            .filter_map(|(stadt, s)| {
                if s.contains(&q) {
                    Some((0, stadt))
                } else if s.iter().any(|n| n.starts_with(&q)) {
                    Some((1, stadt))
                } else {
                    None
                }
            })
            .collect();
        // sort_by_key ist stabil: gleicher Rang behält die Reihenfolge der Liste.
        treffer.sort_by_key(|(rang, _)| *rang);
        treffer.into_iter().take(hoechstens).map(|(_, s)| s).collect()
    }
}

/// Eine endliche Gradzahl im Bereich ±grenze.
fn grad(feld: &str, grenze: f64) -> Option<f64> {
    feld.trim().parse::<f64>().ok().filter(|g| g.is_finite() && g.abs() <= grenze)
}

/// Gängige lateinische Zeichen mit Akzent und ihre Grundform (nach dem Kleinschreiben).
const FALTUNG: &[(&str, &str)] = &[
    ("äáàâãåāăą", "a"),
    ("æ", "ae"),
    ("çćč", "c"),
    ("ďđ", "d"),
    ("éèêëēėęě", "e"),
    ("ğ", "g"),
    ("íìîïīı", "i"),
    ("ł", "l"),
    ("ñńň", "n"),
    ("öóòôõøō", "o"),
    ("œ", "oe"),
    ("řŕ", "r"),
    ("ß", "ss"),
    ("śšşș", "s"),
    ("ťţț", "t"),
    ("üúùûůūű", "u"),
    ("ýÿ", "y"),
    ("źżž", "z"),
];

/// Kleinschreibung ohne Akzente, damit „Ångström“ und „angstrom“ gleich sind.
fn falten(s: &str) -> String {
    let mut aus = String::with_capacity(s.len());
    for c in s.to_lowercase().chars() {
        match FALTUNG.iter().find(|(von, _)| von.contains(c)) {
            Some((_, nach)) => aus.push_str(nach),
            None => aus.push(c),
        }
    }
    aus
}

#[cfg(test)]
mod tests {
    use super::*;

    // Erfundene Orte mit erfundenen Koordinaten.
    const PROBE: &str = "# Probe, frei erfunden\n\
        Nebelfurt\tNebelfurth,Brumbach\tZZ\tNordmark\t12.5\t-33.75\tEurope/Berlin\n\
        Saint-Éloi-des-Brumes\tEloiville\tZZ\tSüdmark\t-41.125\t101.5\tEurope/Paris\n\
        \n\
        Nebelfurt-Ost\t\tZZ\tNordmark\t12.625\t-33.5\tEurope/Berlin\n\
        Översandby\tOversand\tZY\tInselkreis\t66.25\t7.875\tAsia/Tokyo\n";

    fn namen(v: &[&Stadt]) -> Vec<String> {
        v.iter().map(|s| s.name.clone()).collect()
    }

    #[test]
    fn liest_die_probe() {
        let l = Ortsliste::lesen(PROBE).unwrap();
        assert_eq!(l.alle().len(), 4);
        let s = &l.alle()[1];
        assert_eq!(s.name, "Saint-Éloi-des-Brumes");
        assert_eq!(s.land, "ZZ");
        assert_eq!(s.region, "Südmark");
        assert_eq!(s.breite, -41.125);
        assert_eq!(s.laenge, 101.5);
        assert_eq!(s.zeitzone, "Europe/Paris");
    }

    #[test]
    fn findet_nach_name_und_nebenname() {
        let l = Ortsliste::lesen(PROBE).unwrap();
        assert_eq!(namen(&l.suchen("Översandby", 10)), ["Översandby"]);
        assert_eq!(namen(&l.suchen("Brumbach", 10)), ["Nebelfurt"]);
        assert_eq!(namen(&l.suchen("eloiville", 10)), ["Saint-Éloi-des-Brumes"]);
        assert!(l.suchen("Atlantis", 10).is_empty());
        assert!(l.suchen("  ", 10).is_empty());
    }

    #[test]
    fn ohne_akzente_und_schreibweise() {
        let l = Ortsliste::lesen(PROBE).unwrap();
        assert_eq!(namen(&l.suchen("OVERSANDBY", 10)), ["Översandby"]);
        assert_eq!(namen(&l.suchen("saint-eloi", 10)), ["Saint-Éloi-des-Brumes"]);
        assert_eq!(falten("Ærø Łódź Straße Œuvre Ñandú Ça"), "aero lodz strasse oeuvre nandu ca");
    }

    #[test]
    fn genaue_treffer_vor_anfaengen() {
        let l = Ortsliste::lesen(PROBE).unwrap();
        // „Nebelfurt-Ost“ beginnt mit der Anfrage, „Nebelfurt“ passt genau.
        assert_eq!(namen(&l.suchen("nebelfurt", 10)), ["Nebelfurt", "Nebelfurt-Ost"]);
        // Nur Anfänge: Reihenfolge der Liste.
        assert_eq!(namen(&l.suchen("nebel", 10)), ["Nebelfurt", "Nebelfurt-Ost"]);
        // Der genaue Treffer über einen Nebennamen steht ebenfalls vorn.
        let l2 = Ortsliste::lesen("Nebelfurt-Ost\t\tZZ\tA\t1.5\t2.5\tUTC\nAndersheim\tNebelfurt\tZZ\tB\t3.5\t4.5\tUTC\n").unwrap();
        assert_eq!(namen(&l2.suchen("Nebelfurt", 10)), ["Andersheim", "Nebelfurt-Ost"]);
    }

    #[test]
    fn hoechstens_begrenzt() {
        let l = Ortsliste::lesen(PROBE).unwrap();
        assert_eq!(namen(&l.suchen("nebel", 1)), ["Nebelfurt"]);
        assert!(l.suchen("nebel", 0).is_empty());
    }

    #[test]
    fn fehlerhafte_zeilen_ohne_zahlen_aus_der_zeile() {
        let kopf = "# Kopf\nNebelfurt\t\tZZ\tA\t12.5\t-33.75\tEurope/Berlin\n";
        let faelle = [
            "Kaputtheim\t\tZZ\tA\t47.25\t200.5\tEurope/Berlin",
            "Kaputtheim\t\tZZ\tA\t94.75\t8.25\tEurope/Berlin",
            "Kaputtheim\t\tZZ\tA\tNaN\t8.25\tEurope/Berlin",
            "Kaputtheim\t\tZZ\tA\t47.25\t8.25\tMond/Krater",
            "Kaputtheim\t\tZZ\tA\t47.25\t8.25",
            "Kaputtheim\t\tZZZ\tA\t47.25\t8.25\tEurope/Berlin",
            "\t\tZZ\tA\t47.25\t8.25\tEurope/Berlin",
        ];
        for zeile in faelle {
            let f = Ortsliste::lesen(&format!("{kopf}{zeile}\n")).unwrap_err();
            assert!(f.contains("Zeile 3"), "{f}");
            for verboten in ["47", "25", "200", "94", "75", "8.", "Kaputtheim", "Krater", "NaN"] {
                assert!(!f.contains(verboten), "{f} enthält {verboten}");
            }
        }
    }
}
