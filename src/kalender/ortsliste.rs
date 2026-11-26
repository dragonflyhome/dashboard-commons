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
//!
//! Ablage im privaten Inhalt: `projects/kalender/staedte/staedte.tsv` und daneben
//! die Kopfdatei `staedte-kopf.toml` mit `quelle`, `abgerufen` (Datum), `lizenz`
//! („CC BY 4.0“), `sha256` (der heruntergeladenen Originaldatei) und
//! `sha256_liste` (der gekürzten Liste). Nicht in `daten/`: dort ist jede Datei
//! ein Baustein und die Prüfung wiese die Liste ab.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use serde::Deserialize;
use sha2::{Digest, Sha256};

/// Ordner der Liste im Projekt `kalender`.
pub const ORDNER: &str = "staedte";
pub const LISTE: &str = "staedte.tsv";
pub const KOPF: &str = "staedte-kopf.toml";
/// Namensnennung, wo Treffer angezeigt werden.
pub const QUELLE: &str = "GeoNames (geonames.org), CC BY 4.0";
pub const SATZ_FEHLT: &str = "Die Städteliste ist noch nicht eingerichtet; Breite und Länge gehen immer.";
const SATZ_OHNE_KOPF: &str =
    "Zur Städteliste fehlt die Kopfdatei mit Quelle, Lizenz und Prüfsumme; darum wird sie nicht benutzt. Breite und Länge gehen immer.";
const SATZ_KOPF: &str =
    "Die Kopfdatei der Städteliste ist unvollständig oder nennt nicht die Lizenz CC BY 4.0; darum wird die Liste nicht benutzt. Breite und Länge gehen immer.";
const SATZ_PRUEFSUMME: &str =
    "Die Städteliste passt nicht zur Prüfsumme in ihrer Kopfdatei; sie wurde verändert und wird nicht benutzt. Breite und Länge gehen immer.";
const SATZ_UNLESBAR: &str = "Die Städteliste ließ sich nicht lesen. Breite und Länge gehen immer.";

/// Kopfdatei neben der Liste.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Kopf {
    quelle: String,
    abgerufen: toml::value::Datetime,
    lizenz: String,
    sha256: String,
    sha256_liste: String,
}

fn ist_sha256(s: &str) -> bool {
    s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit())
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Zeitstempel und Größe der beiden Dateien (Schlüssel des Zwischenspeichers).
type Stempel = [(SystemTime, u64); 2];

fn stempel(ordner: &Path) -> Option<Stempel> {
    let s = |n: &str| std::fs::metadata(ordner.join(n)).ok().map(|m| (m.modified().unwrap_or(SystemTime::UNIX_EPOCH), m.len()));
    Some([s(LISTE)?, s(KOPF)?])
}

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

    /// Liest Liste und Kopfdatei aus `ordner` und prüft die Prüfsumme der Liste.
    /// Der Fehler ist ein Satz für die Seite, ohne Werte aus den Dateien.
    pub fn laden(ordner: &Path) -> Result<Ortsliste, String> {
        let bytes = match std::fs::read(ordner.join(LISTE)) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(SATZ_FEHLT.into()),
            Err(_) => return Err(SATZ_UNLESBAR.into()),
        };
        let kopf = match std::fs::read_to_string(ordner.join(KOPF)) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(SATZ_OHNE_KOPF.into()),
            Err(_) => return Err(SATZ_UNLESBAR.into()),
        };
        let kopf: Kopf = toml::from_str(&kopf).map_err(|_| SATZ_KOPF.to_string())?;
        let datum_allein = kopf.abgerufen.date.is_some() && kopf.abgerufen.time.is_none();
        if kopf.quelle.trim().is_empty() || !datum_allein || kopf.lizenz.trim() != "CC BY 4.0" || !ist_sha256(&kopf.sha256) || !ist_sha256(&kopf.sha256_liste) {
            return Err(SATZ_KOPF.into());
        }
        if !hex(&Sha256::digest(&bytes)).eq_ignore_ascii_case(&kopf.sha256_liste) {
            return Err(SATZ_PRUEFSUMME.into());
        }
        let text = String::from_utf8(bytes).map_err(|_| SATZ_UNLESBAR.to_string())?;
        Ortsliste::lesen(&text).map_err(|f| format!("{f} Breite und Länge gehen immer."))
    }

    /// Wie [`Ortsliste::laden`], aber gemerkt, solange sich Zeitstempel und Größe
    /// beider Dateien nicht ändern. Fehler werden nicht gemerkt.
    pub fn geladen(ordner: &Path) -> Result<Arc<Ortsliste>, String> {
        // Je Ordner ein Eintrag; im Betrieb gibt es genau einen Ordner.
        static MERKER: Mutex<Vec<(PathBuf, Stempel, Arc<Ortsliste>)>> = Mutex::new(Vec::new());
        let merker = || MERKER.lock().unwrap_or_else(|e| e.into_inner());
        let vorher = stempel(ordner);
        if let Some(s) = vorher
            && let Some((_, _, l)) = merker().iter().find(|(p, st, _)| p == ordner && *st == s)
        {
            return Ok(l.clone());
        }
        let liste = Arc::new(Ortsliste::laden(ordner)?);
        // Nur merken, wenn sich während des Lesens nichts geändert hat.
        if let Some(s) = vorher.filter(|s| stempel(ordner).as_ref() == Some(s)) {
            let mut m = merker();
            m.retain(|(p, _, _)| p != ordner);
            m.push((ordner.to_path_buf(), s, liste.clone()));
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

    /// Kopfdatei zur Liste `liste`; `summe` ersetzt die richtige Prüfsumme.
    fn kopf(liste: &str, summe: Option<&str>) -> String {
        let richtig = hex(&Sha256::digest(liste.as_bytes()));
        format!(
            "quelle = \"https://example.org/probe.zip\"\nabgerufen = 2026-11-26\nlizenz = \"CC BY 4.0\"\nsha256 = \"{}\"\nsha256_liste = \"{}\"\n",
            "ab".repeat(32),
            summe.unwrap_or(&richtig)
        )
    }

    /// Ein frischer Ordner mit Liste und (wenn gegeben) Kopfdatei.
    fn ordner_mit(name: &str, liste: Option<&str>, kopfdatei: Option<&str>) -> PathBuf {
        let o = std::env::temp_dir().join(format!("dashboard-ortsliste-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&o);
        std::fs::create_dir_all(&o).unwrap();
        if let Some(l) = liste {
            std::fs::write(o.join(LISTE), l).unwrap();
        }
        if let Some(k) = kopfdatei {
            std::fs::write(o.join(KOPF), k).unwrap();
        }
        o
    }

    #[test]
    fn laden_mit_kopf_und_pruefsumme() {
        let o = ordner_mit("gut", Some(PROBE), Some(&kopf(PROBE, None)));
        let l = Ortsliste::laden(&o).unwrap();
        assert_eq!(namen(&l.suchen("brumbach", 10)), ["Nebelfurt"]);
        // Gemerkt: zweimal dieselbe Liste; nach einer Änderung die neue.
        let a = Ortsliste::geladen(&o).unwrap();
        assert!(Arc::ptr_eq(&a, &Ortsliste::geladen(&o).unwrap()));
        let kurz = "Nebelfurt\t\tZZ\tA\t12.5\t-33.75\tEurope/Berlin\n";
        std::fs::write(o.join(LISTE), kurz).unwrap();
        std::fs::write(o.join(KOPF), kopf(kurz, None)).unwrap();
        assert_eq!(Ortsliste::geladen(&o).unwrap().alle().len(), 1);
        let _ = std::fs::remove_dir_all(&o);
    }

    #[test]
    fn laden_verweigert_ohne_werte() {
        let falsch = "cd".repeat(32);
        let faelle = [
            ("fehlt", None, None, SATZ_FEHLT),
            ("ohne-kopf", Some(PROBE), None, SATZ_OHNE_KOPF),
            ("summe", Some(PROBE), Some(kopf(PROBE, Some(&falsch))), SATZ_PRUEFSUMME),
            ("lizenz", Some(PROBE), Some(kopf(PROBE, None).replace("CC BY 4.0", "frei")), SATZ_KOPF),
            ("feld", Some(PROBE), Some(format!("{}extra = 1\n", kopf(PROBE, None))), SATZ_KOPF),
            ("datum", Some(PROBE), Some(kopf(PROBE, None).replace("2026-11-26", "\"gestern\"")), SATZ_KOPF),
        ];
        for (name, liste, k, satz) in faelle {
            let o = ordner_mit(name, liste, k.as_deref());
            assert_eq!(Ortsliste::laden(&o).unwrap_err(), satz, "{name}");
            assert_eq!(Ortsliste::geladen(&o).unwrap_err(), satz, "{name}");
            let _ = std::fs::remove_dir_all(&o);
        }
        // Die Sätze nennen keine Prüfsumme, keine Zahl und keinen Ort.
        for satz in [SATZ_FEHLT, SATZ_OHNE_KOPF, SATZ_PRUEFSUMME, SATZ_KOPF, SATZ_UNLESBAR] {
            assert!(!satz.replace("CC BY 4.0", "").chars().any(|c| c.is_ascii_digit()), "{satz}");
        }
        // Eine veränderte Liste mit alter Prüfsumme.
        let o = ordner_mit("veraendert", Some(&format!("{PROBE}Zusatzort\t\tZZ\tA\t1.5\t2.5\tUTC\n")), Some(&kopf(PROBE, None)));
        assert_eq!(Ortsliste::laden(&o).unwrap_err(), SATZ_PRUEFSUMME);
        let _ = std::fs::remove_dir_all(&o);
    }

    /// Etwa so groß wie die echte Liste (gut 30 000 Zeilen): Laden und Suchen messen.
    #[test]
    fn grosse_liste_laden_und_suchen() {
        let zonen = ["Europe/Berlin", "Asia/Tokyo", "America/Sao_Paulo", "UTC"];
        let mut liste = String::new();
        for i in 0..30_000 {
            let z = zonen[i % zonen.len()];
            liste.push_str(&format!("Probeort {i}\tProbe-{i},Örtchen {i}\tZZ\tRegion {}\t{}.25\t{}.5\t{z}\n", i % 50, i % 80, i % 170));
        }
        let o = ordner_mit("gross", Some(&liste), Some(&kopf(&liste, None)));
        let t = std::time::Instant::now();
        let l = Ortsliste::laden(&o).unwrap();
        let laden = t.elapsed();
        let t = std::time::Instant::now();
        let treffer = l.suchen("orTCHEN 2999", 10);
        let suchen = t.elapsed();
        eprintln!("30 000 Zeilen: laden {laden:?}, suchen {suchen:?}");
        assert_eq!(treffer[0].name, "Probeort 2999");
        assert_eq!(treffer.len(), 10);
        let _ = std::fs::remove_dir_all(&o);
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
