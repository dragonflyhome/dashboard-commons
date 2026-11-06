//! Befehle neben dem Server (Zielbild, Modulplan: `werkzeug/`). Aufruf als
//! erstes Argument, etwa `dashboard.exe pruefe --alle`; dann startet kein
//! Server. `pruefe` prüft Projekte. Der Scanner für
//! Svelte- und TS-Dateien (Etappe 4v) steht in `scanner`, die Stil-Prüfung
//! mit Stylelint (Etappe 3b) in `stil`, ESLint, svelte-check und Vitest je
//! Projektordner (Etappe 4b, für `pruefe <slug>` und den Baumeister) in
//! `projektpruefung`.

pub mod projektpruefung;
pub mod pruefe;
pub mod rechte;
pub mod scanner;
pub mod stil;

// ----- Die Fassung des Prüfers (Kleinkram 2) -----

/// Die Quellen des Prüfers: die Startdateien (Regeln, Scanner, Stil,
/// Projektprüfung) und alles in `src/werkzeug`, `src/inhalt` und
/// `src/vertrag`, was sie über Verweise erreichen (die Hülle). Der Test
/// `pruefer_quellen_vollstaendig` verlangt Liste == Hülle.
// shortcut: Code außerhalb von werkzeug/inhalt/vertrag (etwa oeffnen, quellen,
// app::data_file, app::quellen, datenschutz::verbergen, dienst::einstellungen,
// dienst::programme, pc) erfasst die Hülle nicht; aufrüsten, wenn die Prüfung
// von dort Regeln nutzt, die sich ändern. Ebenso nicht: relative Pfade aus
// mod.rs und `use super::*`; aufrüsten, wenn eine Startdatei so einbindet.
macro_rules! pruefer_quellen {
    ($($pfad:literal),* $(,)?) => {
        (&[$($pfad),*], &[$(include_bytes!($pfad) as &[u8]),*])
    };
}

const PRUEFER_QUELLEN: (&[&str], &[&[u8]]) = pruefer_quellen![
    "../werkzeug/mod.rs",
    "../werkzeug/projektpruefung.rs",
    "../werkzeug/scanner.rs",
    "../werkzeug/stil.rs",
    "../inhalt/assets.rs",
    "../inhalt/pruefung.rs",
    "../inhalt/seite.rs",
    "../vertrag/abgeleitet.rs",
    "../vertrag/anstehend.rs",
    "../vertrag/bedeutung.rs",
    "../vertrag/daten.rs",
    "../vertrag/fristen.rs",
    "../vertrag/gruppen.rs",
    "../vertrag/lesen.rs",
    "../vertrag/projekt.rs",
    "../vertrag/schwerpunkte.rs",
    "../vertrag/verlauf.rs",
];

/// Ein Fingerabdruck über Bytes, beim Übersetzen berechnet (FNV-artig, je
/// acht Bytes ein Schritt, der Rest einzeln).
const fn abdruck_der_quellen(teile: &[&[u8]]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut i = 0;
    while i < teile.len() {
        let mut rest: &[u8] = teile[i];
        while let [a, b, c, d, e, f, g, k, weiter @ ..] = rest {
            h ^= u64::from_le_bytes([*a, *b, *c, *d, *e, *f, *g, *k]);
            h = h.wrapping_mul(0x0100_0000_01b3).rotate_left(29);
            rest = weiter;
        }
        while let [a, weiter @ ..] = rest {
            h ^= *a as u64;
            h = h.wrapping_mul(0x0100_0000_01b3);
            rest = weiter;
        }
        // Trenner zwischen den Dateien.
        h ^= 0xff;
        h = h.wrapping_mul(0x0100_0000_01b3);
        i += 1;
    }
    h
}

/// Die Fassung des Prüfers in diesem Programm. Dienst und Bote, aus demselben
/// Stand gebaut, haben dieselbe; weicht sie ab, prüft der Bote mit anderen
/// Regeln, als der Dienst sie kennt (`/api/betrieb`, `stoerungen`). Sie geht
/// auch in den Umgebungs-Abdruck (`bote::bau::umgebung`): Ändert sich der
/// Prüfer, prüft der Bote beim nächsten Bau jedes Projekt neu, und kein altes
/// Urteil bleibt in `stand.json` kleben.
pub const PRUEFER_FASSUNG: u64 = abdruck_der_quellen(PRUEFER_QUELLEN.1);

/// [`PRUEFER_FASSUNG`] als Text (16 Hex-Zeichen), wie im Hallo des Boten.
pub fn pruefer_fassung() -> String {
    format!("{PRUEFER_FASSUNG:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Die Startdateien der Prüfung: Regeln, Scanner, Stil, Projektprüfung.
    const START: [&str; 7] = [
        "werkzeug/mod.rs",
        "werkzeug/projektpruefung.rs",
        "werkzeug/scanner.rs",
        "werkzeug/stil.rs",
        "inhalt/assets.rs",
        "inhalt/pruefung.rs",
        "inhalt/seite.rs",
    ];

    /// Die ersten Namen nach `a::`: `x` aus `x::…` oder `x`, aus `{x, y::z}` beide.
    fn erste_namen(rest: &str) -> Vec<&str> {
        fn name(t: &str) -> &str {
            let t = t.trim_start();
            &t[..t.find(|c: char| !(c.is_alphanumeric() || c == '_')).unwrap_or(t.len())]
        }
        let Some(gruppe) = rest.strip_prefix('{') else { return vec![name(rest)] };
        let (mut tiefe, mut anfang, mut aus) = (0, 0, Vec::new());
        for (i, c) in gruppe.char_indices() {
            match c {
                '{' => tiefe += 1,
                '}' if tiefe == 0 => {
                    aus.push(name(&gruppe[anfang..i]));
                    break;
                }
                '}' => tiefe -= 1,
                ',' if tiefe == 0 => {
                    aus.push(name(&gruppe[anfang..i]));
                    anfang = i + 1;
                }
                _ => {}
            }
        }
        aus
    }

    /// Die Dateien, auf die `datei` (etwa `inhalt/pruefung.rs`) verweist:
    /// `crate::{werkzeug,inhalt,vertrag}::X` (auch ohne `crate::` nach einem
    /// `use crate::vertrag;`) und `super::X`, auch in `use`. Ohne Kommentare
    /// und ohne das Testmodul am Ende; `X.rs`, sonst `mod.rs` des Ordners.
    fn verweise(src: &std::path::Path, datei: &str) -> Vec<String> {
        let (ordner, name) = datei.split_once('/').unwrap();
        let text = std::fs::read_to_string(src.join(datei)).unwrap().replace('\r', "");
        let ohne_tests = text.split("\n#[cfg(test)]\nmod tests").next().unwrap();
        let code: String = ohne_tests.lines().map(|z| z.split("//").next().unwrap()).collect::<Vec<_>>().join("\n");
        let mut aus = Vec::new();
        for (i, _) in code.match_indices("::") {
            let davor = &code[..i];
            let vorher = davor.trim_end_matches(|c: char| c.is_alphanumeric() || c == '_');
            let modul = match &davor[vorher.len()..] {
                m @ ("werkzeug" | "inhalt" | "vertrag") if !vorher.ends_with("::") || vorher.ends_with("crate::") => m,
                "super" if name != "mod.rs" && !vorher.ends_with("::") => ordner,
                _ => continue,
            };
            for x in erste_namen(&code[i + 2..]) {
                if x.is_empty() || x == "self" || x == "super" {
                    continue;
                }
                let f = format!("{modul}/{x}.rs");
                aus.push(if src.join(&f).is_file() { f } else { format!("{modul}/mod.rs") });
            }
        }
        aus
    }

    /// Hüllen-Test: Von den Startdateien aus jedem Verweis
    /// folgen; die Liste in `pruefer_quellen!` ist genau diese Hülle.
    #[test]
    fn pruefer_quellen_vollstaendig() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut huelle = std::collections::BTreeSet::new();
        let mut offen: Vec<String> = START.iter().map(|s| s.to_string()).collect();
        while let Some(datei) = offen.pop() {
            if huelle.insert(datei.clone()) {
                offen.extend(verweise(&src, &datei));
            }
        }
        let huelle: Vec<String> = huelle.into_iter().map(|d| format!("../{d}")).collect();
        let mut liste: Vec<String> = PRUEFER_QUELLEN.0.iter().map(|s| s.to_string()).collect();
        liste.sort();
        assert_eq!(liste, huelle, "src/werkzeug/mod.rs: pruefer_quellen! muss genau die Hülle nennen");
        assert_eq!(PRUEFER_QUELLEN.0.len(), PRUEFER_QUELLEN.1.len());
    }

    /// Die Hülle liest, was sie soll: Gruppen, `super`, Kommentare, Testmodul.
    #[test]
    fn huelle_liest_verweise() {
        assert_eq!(erste_namen("{api::Befund, daten::{a, b}, self} rest"), ["api", "daten", "self"]);
        assert_eq!(erste_namen("seite::is_valid_slug(x)"), ["seite"]);
        let src = std::env::temp_dir().join(format!("dashboard-huelle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&src);
        std::fs::create_dir_all(src.join("inhalt")).unwrap();
        for f in ["a.rs", "b.rs", "c.rs", "mod.rs"] {
            std::fs::write(src.join("inhalt").join(f), "").unwrap();
        }
        let text = "use super::{a, gibt_es_nicht};\nuse crate::vertrag::lesen;\nfn f() { bote::vertrag::x(); } // crate::inhalt::c\n\
                    #[cfg(test)]\nmod tests {\n    use super::b;\n}\n";
        std::fs::write(src.join("inhalt").join("x.rs"), text).unwrap();
        assert_eq!(verweise(&src, "inhalt/x.rs"), ["inhalt/a.rs", "inhalt/mod.rs", "vertrag/mod.rs"]);
        let _ = std::fs::remove_dir_all(&src);
    }

    /// Andere Bytes, andere Fassung; die Fassung ist 16 Hex-Zeichen lang.
    #[test]
    fn pruefer_fassung_haengt_an_den_quellen() {
        assert_ne!(abdruck_der_quellen(&[b"abc"]), abdruck_der_quellen(&[b"abd"]));
        assert_ne!(abdruck_der_quellen(&[b"ab", b"c"]), abdruck_der_quellen(&[b"a", b"bc"]));
        assert_ne!(abdruck_der_quellen(&[b"0123456789abcdef"]), abdruck_der_quellen(&[b"0123456789abcdeg"]));
        let t = pruefer_fassung();
        assert_eq!(t.len(), 16);
        assert!(t.bytes().all(|b| b.is_ascii_hexdigit()));
    }
}
