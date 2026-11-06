//! Profilpfad und Benutzername verbergen, bevor ein Text den Server verlässt
//! (Auftrag der Architektur vor dem ersten Push: `/api/pc` ist ohne Anmeldung
//! erreichbar). Die eine Stelle dafür ist [`verbergen`]; die Oberfläche
//! (`web/src/lib/pfade.ts`, `profilVerbergen`) ist nur die zweite Sicherung.
//!
//! Ersetzt wird der Teil bis einschließlich des Profilordners durch
//! `%USERPROFILE%`, der Rest des Pfads bleibt, damit man den Ort erkennt:
//!
//! - `C:\Users\<name>`, `C:/Users/<name>`, auch „Benutzer“ und mit doppelten
//!   Trennzeichen (`C:\\Users\\<name>`, so steht es in Debug-Ausgaben);
//! - `/c/Users/<name>` (Git Bash), `/mnt/c/Users/<name>` (WSL),
//!   `/cygdrive/c/Users/<name>`;
//! - `\\rechner\c$\Users\<name>` (auch mit `/`);
//! - URL-kodiert (`%5C`, `%2F`, `%3A`, `%24`) und mit `file:` davor
//!   (`file:///C:/Users/<name>`, `file://rechner/c$/Users/<name>`);
//! - das tatsächliche Profil des Inhabers in allen diesen Formen, auch wenn
//!   es nicht unter `\Users` liegt (im Dienst aus seiner SID, siehe
//!   [`inhaber_setzen`] und `oeffnen::inhaber_profil`).
//!
//! Der Benutzername allein (etwa in `DOMÄNE\name` oder einem Satz) wird zu
//! `%USERNAME%`, wenn er bekannt ist und als ganzes Wort dasteht.
//!
//! Ohne bekannten Inhaber (Konsole, Tests) gelten das eigene Profil
//! (`USERPROFILE`) und der eigene Name (`USERNAME`): Dann läuft der Prozess
//! als Inhaber. Im Dienst setzt der Start beides aus der SID, nie das
//! Dienstkonto.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Serialize;
use serde_json::Value;

/// Platzhalter für das Profil (wie in `speicher::anzeige_pfad` und `pfade.ts`).
pub const PROFIL_PLATZHALTER: &str = "%USERPROFILE%";
/// Platzhalter für den Benutzernamen allein.
pub const NAME_PLATZHALTER: &str = "%USERNAME%";

/// Namen unter dieser Länge werden nicht allein ersetzt (sonst träfe es zu
/// viele gewöhnliche Wörter); im Pfad gilt jede Länge.
const NAME_MINDESTENS: usize = 3;

/// Was über den Inhaber bekannt ist: seine Profile (als Text) und Namen.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Bekannt {
    /// Profilordner als Text, etwa `C:\Users\beispiel` oder `D:\Profile\beispiel`.
    pub profile: Vec<String>,
    /// Benutzernamen (Konto, Name des Profilordners).
    pub namen: Vec<String>,
}

impl Bekannt {
    /// Aus einem Profil und einem Namen. Der Ordnername des Profils zählt
    /// auch als Name. Leeres und nicht Absolutes fällt weg.
    pub fn aus(profil: Option<&Path>, name: Option<&str>) -> Bekannt {
        let mut b = Bekannt::default();
        if let Some(p) = profil.filter(|p| p.is_absolute()) {
            let text = p.display().to_string().trim_end_matches(['\\', '/']).to_string();
            if text.len() > 3 {
                b.profile.push(text);
            }
            if let Some(n) = p.file_name().map(|n| n.to_string_lossy().to_string()) {
                b.name_dazu(&n);
            }
        }
        if let Some(n) = name {
            // „DOMÄNE\name“: der Teil nach dem letzten Rückstrich.
            b.name_dazu(n.rsplit('\\').next().unwrap_or(n));
        }
        b
    }

    fn name_dazu(&mut self, n: &str) {
        let n = n.trim();
        if n.chars().count() >= NAME_MINDESTENS && !self.namen.iter().any(|m| m.to_lowercase() == n.to_lowercase()) {
            self.namen.push(n.to_string());
        }
    }

    /// Aus dieser Umgebung (`USERPROFILE`, `USERNAME`): für Konsole und Tests.
    pub fn aus_umgebung() -> Bekannt {
        let profil = std::env::var_os("USERPROFILE").map(PathBuf::from);
        let name = std::env::var("USERNAME").ok();
        Bekannt::aus(profil.as_deref(), name.as_deref())
    }
}

static BEKANNT: OnceLock<Bekannt> = OnceLock::new();

/// Der Dienst setzt beim Start, was er über den Inhaber weiß (Profil und
/// Name aus der SID, siehe `oeffnen::inhaber_profil`). Nur das erste Setzen
/// zählt; ohne Setzen gilt [`Bekannt::aus_umgebung`]. Ein zweites Setzen geht
/// nie still durch: mit demselben Wert eine Warnung im Protokoll, mit einem
/// anderen (oder nachdem schon aus der Umgebung gelesen wurde) ein Fehler.
pub fn inhaber_setzen(b: Bekannt) -> Result<(), String> {
    setzen_in(&BEKANNT, b)
}

/// Wie [`inhaber_setzen`], in eine eigene Zelle (Tests).
fn setzen_in(zelle: &OnceLock<Bekannt>, b: Bekannt) -> Result<(), String> {
    match zelle.set(b) {
        Ok(()) => Ok(()),
        Err(neu) if zelle.get() == Some(&neu) => {
            tracing::warn!("datenschutz: Der Inhaber wurde ein zweites Mal gesetzt (derselbe Wert).");
            Ok(())
        }
        Err(_) => {
            let satz = "datenschutz: Der Inhaber war schon anders gesetzt; der neue Wert gilt nicht. Bitte den Dienst neu starten.".to_string();
            tracing::error!("{satz}");
            Err(satz)
        }
    }
}

/// Was gerade als Inhaber gilt.
pub fn bekannt() -> &'static Bekannt {
    BEKANNT.get_or_init(Bekannt::aus_umgebung)
}

/// Verbirgt Profilpfade und den Benutzernamen des Inhabers (siehe Kopf).
pub fn verbergen(text: &str) -> String {
    verbergen_mit(text, bekannt())
}

/// Wie [`verbergen`], mit ausdrücklich angegebenem Inhaber (Tests, Werkzeuge).
pub fn verbergen_mit(text: &str, b: &Bekannt) -> String {
    let ohne_pfade = pfade_ersetzen(text, b);
    namen_ersetzen(&ohne_pfade, &b.namen)
}

/// Jeder Text in einem JSON-Wert (auch Schlüssel) durch [`verbergen`].
pub fn json_verbergen(v: &mut Value) {
    json_verbergen_mit(v, bekannt());
}

/// Wie [`json_verbergen`], mit angegebenem Inhaber.
pub fn json_verbergen_mit(v: &mut Value, b: &Bekannt) {
    match v {
        Value::String(s) => {
            let neu = verbergen_mit(s, b);
            if neu != *s {
                *s = neu;
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| json_verbergen_mit(x, b)),
        Value::Object(o) => {
            let alt = std::mem::take(o);
            for (k, mut x) in alt {
                json_verbergen_mit(&mut x, b);
                o.insert(verbergen_mit(&k, b), x);
            }
        }
        _ => {}
    }
}

/// Eine Antwort als JSON, alle Texte verborgen. Für Wege, deren Inhalt aus
/// dem System kommt (PC, Betrieb, Verlauf), nicht für Seiteninhalte.
pub fn als_json<T: Serialize>(wert: &T) -> axum::Json<Value> {
    let mut v = serde_json::to_value(wert).unwrap_or(Value::Null);
    json_verbergen(&mut v);
    axum::Json(v)
}

/// Der Kopf eines Projekts für die freien Wege: Pfade der Repos und Adressen
/// der Links (ein lokaler Pfad ist dort erlaubt, `pfade.ts`) ohne Profil.
pub fn projekt_verbergen(p: &mut crate::vertrag::projekt::Projekt) {
    for l in &mut p.links {
        l.url = verbergen(&l.url);
    }
    for r in &mut p.repos {
        r.path = verbergen(&r.path);
    }
    for c in &mut p.checks {
        c.url = verbergen(&c.url);
    }
    for n in &mut p.processes {
        *n = verbergen(n);
    }
}

/// Der Git-Stand eines Repos für die freien Wege: Pfad und Fehlertext.
pub fn repo_verbergen(mut r: crate::git::RepoStatus) -> crate::git::RepoStatus {
    r.path = verbergen(&r.path);
    r.error = r.error.as_deref().map(verbergen);
    r
}

// ----- Erkennen -----

/// Ein URL-kodiertes Zeichen an `i`: `%XY` oder doppelt kodiert `%25XY`
/// (wie im Vor-Push-Prüfer). Gibt die Länge und `XY` klein.
fn prozent(t: &[u8], i: usize) -> Option<(usize, [u8; 2])> {
    if t.get(i) != Some(&b'%') {
        return None;
    }
    let hex = |k: usize| t.get(k).filter(|c| c.is_ascii_hexdigit()).map(u8::to_ascii_lowercase);
    if t.get(i + 1) == Some(&b'2')
        && t.get(i + 2) == Some(&b'5')
        && let (Some(a), Some(b)) = (hex(i + 3), hex(i + 4))
    {
        return Some((5, [a, b]));
    }
    Some((3, [hex(i + 1)?, hex(i + 2)?]))
}

/// Ein Trennzeichen an `i`: `\`, `/`, `%5C`, `%2F` (auch `%255C`, `%252F`); gibt seine Länge.
fn trenner(t: &[u8], i: usize) -> Option<usize> {
    match t.get(i)? {
        b'\\' | b'/' => Some(1),
        b'%' => prozent(t, i).filter(|(_, k)| matches!(k, b"5c" | b"2f")).map(|(n, _)| n),
        _ => None,
    }
}

/// Ein oder mehr Trennzeichen hintereinander; gibt das Ende.
fn trenner_folge(t: &[u8], mut i: usize) -> Option<usize> {
    let start = i;
    while let Some(n) = trenner(t, i) {
        i += n;
    }
    (i > start).then_some(i)
}

/// Doppelpunkt (`:`, `%3A` oder `%253A`) an `i`; gibt das Ende.
fn doppelpunkt(t: &[u8], i: usize) -> Option<usize> {
    match t.get(i)? {
        b':' => Some(i + 1),
        b'%' => prozent(t, i).filter(|(_, k)| k == b"3a").map(|(n, _)| i + n),
        _ => None,
    }
}

/// `$`, `%24` oder `%2524` an `i`; gibt das Ende.
fn dollar(t: &[u8], i: usize) -> Option<usize> {
    match t.get(i)? {
        b'$' => Some(i + 1),
        b'%' => prozent(t, i).filter(|(_, k)| k == b"24").map(|(n, _)| i + n),
        _ => None,
    }
}

/// Endet `t[..i]` auf ein kodiertes Trennzeichen oder einen kodierten
/// Doppelpunkt (`%5C`, `%2F`, `%3A`, auch doppelt kodiert)? Dann beginnt bei
/// `i` ein eigenes Wort, obwohl davor ein Buchstabe oder eine Ziffer steht.
fn nach_kodiertem_trenner(t: &[u8], i: usize) -> bool {
    [5usize, 3].iter().any(|&n| i >= n && prozent(t, i - n).is_some_and(|(laenge, k)| laenge == n && matches!(&k, b"5c" | b"2f" | b"3a")))
}

/// Davor steht nichts, was zu einem Wort gehört (Buchstabe, Ziffer, `_`, `.`).
fn wortgrenze_davor(t: &[u8], i: usize) -> bool {
    if i == 0 || nach_kodiertem_trenner(t, i) {
        return true;
    }
    // Das Zeichen davor (UTF-8: bis zu drei Folgebytes zurück).
    let mut k = i - 1;
    while k > 0 && i - k < 4 && t[k] & 0xC0 == 0x80 {
        k -= 1;
    }
    match std::str::from_utf8(&t[k..i]).ok().and_then(|s| s.chars().next()) {
        Some(c) => !(c.is_alphanumeric() || c == '_' || c == '.'),
        None => true,
    }
}

/// ASCII ohne Groß/Klein: steht `wort` an `i`?
fn wort_an(t: &[u8], i: usize, wort: &str) -> bool {
    t.len() >= i + wort.len() && t[i..i + wort.len()].eq_ignore_ascii_case(wort.as_bytes())
}

/// Ein Laufwerk an `i` in einer der Formen aus dem Kopf. Gibt den Buchstaben
/// (klein) und das Ende (nach dem Trennzeichen hinter dem Laufwerk) zurück.
fn laufwerk(t: &[u8], i: usize) -> Option<(u8, usize)> {
    // `C:` mit Trennzeichen danach.
    if t.get(i).is_some_and(u8::is_ascii_alphabetic)
        && wortgrenze_davor(t, i)
        && let Some(n) = doppelpunkt(t, i + 1)
        && let Some(e) = trenner_folge(t, n)
    {
        return Some((t[i].to_ascii_lowercase(), e));
    }
    // `\\rechner\c$\`.
    if trenner(t, i).is_some() && wortgrenze_davor(t, i) {
        let n = trenner(t, i)?;
        if let Some(m) = trenner(t, i + n) {
            let mut j = i + n + m;
            let rechner = j;
            while j < t.len() && trenner(t, j).is_none() && !stopp(t[j]) && !t[j].is_ascii_whitespace() {
                j += 1;
            }
            if j > rechner
                && let Some(s) = trenner(t, j)
            {
                let b = j + s;
                if t.get(b).is_some_and(u8::is_ascii_alphabetic)
                    && let Some(d) = dollar(t, b + 1)
                    && let Some(e) = trenner_folge(t, d)
                {
                    return Some((t[b].to_ascii_lowercase(), e));
                }
            }
        }
        // `/c/`, `/mnt/c/`, `/cygdrive/c/` (nur mit `/` oder `%2F`).
        let mut j = i + n;
        for vorsatz in ["mnt", "cygdrive"] {
            if wort_an(t, j, vorsatz)
                && let Some(s) = trenner(t, j + vorsatz.len())
            {
                j = j + vorsatz.len() + s;
                break;
            }
        }
        if t.get(j).is_some_and(u8::is_ascii_alphabetic)
            && let Some(e) = trenner_folge(t, j + 1)
        {
            let schraeg = |k: usize| t[k] == b'/' || t[k] == b'%';
            if schraeg(i) {
                return Some((t[j].to_ascii_lowercase(), e));
            }
        }
    }
    None
}

/// Zeichen, die in keinem Benutzernamen stehen dürfen (Windows), dazu
/// Anführungszeichen, Klammern und was eine Adresse beendet.
fn stopp(c: u8) -> bool {
    matches!(
        c,
        b'"' | b'\'' | b'`' | b'*' | b':' | b'<' | b'>' | b'?' | b'|' | b'[' | b']' | b';' | b'=' | b',' | b'+' | b'(' | b')' | b'#' | b'&' | b'{' | b'}'
    ) || c.is_ascii_control()
}

/// Ein Teil eines Pfads ab `i` bis zum nächsten Trennzeichen oder Stopp.
/// Mit Leerzeichen nur, wenn danach ein Trennzeichen kommt (`Max Muster\…`);
/// sonst endet er am ersten Leerzeichen (`…\beispiel ist kaputt`).
fn teil(text: &str, i: usize) -> usize {
    let t = text.as_bytes();
    let mut j = i;
    while j < t.len() && trenner(t, j).is_none() && !stopp(t[j]) {
        // Typografische Anführungszeichen beenden den Namen wie `"`.
        let c = text[j..].chars().next().unwrap_or(' ');
        if "„“”‚‘’«»‹›".contains(c) {
            break;
        }
        j += c.len_utf8();
    }
    if trenner(t, j).is_none()
        && let Some(leer) = t[i..j].iter().position(u8::is_ascii_whitespace)
    {
        j = i + leer;
    }
    // Leerzeichen am Ende gehören nicht dazu, ein Punkt (Satzende) auch nicht:
    // Windows-Namen enden nie auf einen Punkt.
    while j > i && (t[j - 1].is_ascii_whitespace() || t[j - 1] == b'.') {
        j -= 1;
    }
    j
}

/// Steht `komponente` (ohne Groß/Klein, auch nicht-ASCII) ganz an `i`? Gibt das Ende.
fn komponente_an(text: &str, i: usize, komponente: &str) -> Option<usize> {
    let rest = text.get(i..)?;
    let mut zeichen = rest.char_indices();
    for k in komponente.chars() {
        let (_, c) = zeichen.next()?;
        if !c.to_lowercase().eq(k.to_lowercase()) {
            return None;
        }
    }
    let ende = zeichen.next().map_or(text.len(), |(n, _)| i + n);
    let t = text.as_bytes();
    // Danach: Ende, Trennzeichen oder etwas, das nicht zum Namen gehört
    // (`beispiel2`, `beispiel-alt` und `beispiel.alt` sind andere Ordner).
    let ok = match text[ende..].chars().next() {
        None => true,
        Some('.') => !t.get(ende + 1).is_some_and(u8::is_ascii_alphanumeric),
        Some(c) => trenner(t, ende).is_some() || c.is_whitespace() || (c.is_ascii() && stopp(c as u8)) || "„“”‚‘’«»‹›".contains(c),
    };
    ok.then_some(ende)
}

/// Ein bekanntes Profil ab `i` (nach dem Laufwerk): die Teile ohne Laufwerk.
fn bekanntes_profil(text: &str, mut i: usize, teile: &[&str]) -> Option<usize> {
    let t = text.as_bytes();
    for (nr, k) in teile.iter().enumerate() {
        i = komponente_an(text, i, k)?;
        if nr + 1 < teile.len() {
            i = trenner_folge(t, i)?;
        }
    }
    Some(i)
}

/// Ein `file:` mit Schrägstrichen direkt vor `i`? Gibt seinen Anfang.
fn file_davor(t: &[u8], i: usize) -> Option<usize> {
    let mut j = i;
    while j > 0 && t[j - 1] == b'/' {
        j -= 1;
    }
    (j >= 5 && t[j - 5..j].eq_ignore_ascii_case(b"file:") && wortgrenze_davor(t, j - 5)).then(|| j - 5)
}

/// Die Laufwerksbuchstaben und Teile der bekannten Profile.
/// Ein bekanntes Profil, zerlegt: auf einem Laufwerk (Buchstabe klein, Teile
/// danach) oder als Netzwerkpfad (Rechner, Freigabe, Teile).
#[derive(Debug, Clone, PartialEq, Eq)]
enum ProfilForm {
    Laufwerk(u8, Vec<String>),
    Unc(Vec<String>),
}

/// Die bekannten Profile zerlegt. `\\?\C:\…` und `\\.\C:\…` gelten wie
/// `C:\…`, `\\?\UNC\rechner\…` wie `\\rechner\…` (so stehen Profile in der
/// Registry oder nach `canonicalize`).
fn profil_teile(b: &Bekannt) -> Vec<ProfilForm> {
    b.profile
        .iter()
        .filter_map(|p| {
            let p = p.replace('/', "\\");
            let p = match p.strip_prefix(r"\\?\UNC\").or_else(|| p.strip_prefix(r"\\.\UNC\")) {
                Some(rest) => format!(r"\\{rest}"),
                None => p.strip_prefix(r"\\?\").or_else(|| p.strip_prefix(r"\\.\")).map(str::to_string).unwrap_or(p),
            };
            let teile = |s: &str| -> Vec<String> { s.split('\\').filter(|s| !s.is_empty()).map(str::to_string).collect() };
            if let Some(rest) = p.strip_prefix(r"\\") {
                let t = teile(rest);
                // Rechner, Freigabe und mindestens ein Ordner.
                return (t.len() >= 3).then_some(ProfilForm::Unc(t));
            }
            let bytes = p.as_bytes();
            if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' {
                return None;
            }
            let t = teile(&p[2..]);
            (!t.is_empty()).then(|| ProfilForm::Laufwerk(bytes[0].to_ascii_lowercase(), t))
        })
        .collect()
}

/// Ein bekanntes Netzwerk-Profil ab `i` (`\\rechner\freigabe\…`, auch mit `/`
/// oder kodiert); gibt das Ende.
fn unc_profil(text: &str, i: usize, bekannte: &[ProfilForm]) -> Option<usize> {
    let t = text.as_bytes();
    if !wortgrenze_davor(t, i) {
        return None;
    }
    let n = trenner(t, i)?;
    let m = trenner(t, i + n)?;
    bekannte
        .iter()
        .filter_map(|p| match p {
            ProfilForm::Unc(teile) => bekanntes_profil(text, i + n + m, &teile.iter().map(String::as_str).collect::<Vec<_>>()),
            ProfilForm::Laufwerk(..) => None,
        })
        .max()
}

fn pfade_ersetzen(text: &str, b: &Bekannt) -> String {
    let t = text.as_bytes();
    // Schnell heraus: ohne Trenn- oder Prozentzeichen kein Pfad.
    if !t.iter().any(|c| matches!(c, b'\\' | b'/' | b'%')) {
        return text.to_string();
    }
    let bekannte = profil_teile(b);
    let mut aus = String::with_capacity(text.len());
    let mut kopiert = 0;
    let mut i = 0;
    while i < t.len() {
        // Nur an Zeichengrenzen anfangen (UTF-8).
        if !text.is_char_boundary(i) {
            i += 1;
            continue;
        }
        // Ein bekanntes Profil auf einer Freigabe (`\\rechner\profile$\<name>`).
        if let Some(e) = unc_profil(text, i, &bekannte) {
            let anfang = file_davor(t, i).filter(|a| *a >= kopiert).unwrap_or(i);
            aus.push_str(&text[kopiert..anfang]);
            aus.push_str(PROFIL_PLATZHALTER);
            kopiert = e;
            i = e;
            continue;
        }
        let Some((buchstabe, nach)) = laufwerk(t, i) else {
            i += 1;
            continue;
        };
        // Zuerst die bekannten Profile (auch außerhalb von \Users).
        let mut ende = bekannte
            .iter()
            .filter_map(|p| match p {
                ProfilForm::Laufwerk(l, teile) if *l == buchstabe => bekanntes_profil(text, nach, &teile.iter().map(String::as_str).collect::<Vec<_>>()),
                _ => None,
            })
            .max();
        // Sonst `Users\<name>` oder `Benutzer\<name>`.
        if ende.is_none() {
            for ordner in ["users", "benutzer"] {
                if let Some(e) = komponente_an(text, nach, ordner)
                    && let Some(n) = trenner_folge(t, e)
                {
                    let name_ende = teil(text, n);
                    if name_ende > n {
                        ende = Some(name_ende);
                    }
                }
            }
        }
        match ende {
            Some(e) => {
                let anfang = file_davor(t, i).filter(|a| *a >= kopiert).unwrap_or(i);
                aus.push_str(&text[kopiert..anfang]);
                aus.push_str(PROFIL_PLATZHALTER);
                kopiert = e;
                i = e;
            }
            None => i += 1,
        }
    }
    aus.push_str(&text[kopiert..]);
    aus
}

/// Jeder bekannte Name als ganzes Wort (ohne Groß/Klein) wird `%USERNAME%`.
fn namen_ersetzen(text: &str, namen: &[String]) -> String {
    let mut aus = text.to_string();
    for n in namen.iter().filter(|n| n.chars().count() >= NAME_MINDESTENS) {
        aus = ein_name(&aus, n);
    }
    aus
}

fn ein_name(text: &str, name: &str) -> String {
    let klein_name: String = name.to_lowercase();
    let erstes = klein_name.chars().next();
    let mut aus = String::with_capacity(text.len());
    let mut kopiert = 0;
    let wortzeichen = |c: char| c.is_alphanumeric();
    let it = text.char_indices().peekable();
    let mut vorher: Option<char> = None;
    for (i, c) in it {
        // Ein eigenes Wort: davor kein Buchstabe und keine Ziffer, oder ein
        // kodierter Trenner (`DOMAENE%5Cname`, `%2Fname`).
        let grenze = !vorher.is_some_and(wortzeichen) || nach_kodiertem_trenner(text.as_bytes(), i);
        if i >= kopiert
            && c.to_lowercase().next() == erstes
            && grenze
            && let Some(ende) = komponente_gleich(text, i, &klein_name)
        {
            let danach = text[ende..].chars().next();
            if !danach.is_some_and(wortzeichen) {
                aus.push_str(&text[kopiert..i]);
                aus.push_str(NAME_PLATZHALTER);
                kopiert = ende;
            }
        }
        vorher = Some(c);
    }
    aus.push_str(&text[kopiert..]);
    aus
}

/// Steht `klein` (schon klein) an `i`, ohne Groß/Klein? Gibt das Ende.
fn komponente_gleich(text: &str, i: usize, klein: &str) -> Option<usize> {
    let mut soll = klein.chars();
    for (n, c) in text[i..].char_indices() {
        for k in c.to_lowercase() {
            if soll.next() != Some(k) {
                return None;
            }
        }
        if soll.as_str().is_empty() {
            return Some(i + n + c.len_utf8());
        }
    }
    None
}

// ----- Konto aus der SID -----

/// Der Kontoname zu einer SID (ohne Domäne), über `LookupAccountSidW`.
pub fn name_aus_sid(sid: &str) -> Option<String> {
    konto_aus_sid(sid).map(|(_, name)| name)
}

/// Domäne (oder Rechner, `NT SERVICE` …) und Name des Kontos zu einer SID als
/// Text; `None`, wenn die SID keine ist oder Windows das Konto nicht kennt.
#[cfg(windows)]
pub fn konto_aus_sid(sid: &str) -> Option<(String, String)> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::ConvertStringSidToSidW;
    use windows_sys::Win32::Security::{LookupAccountSidW, PSID, SID_NAME_USE};
    let sid = sid.trim();
    if !crate::dienst::einstellungen::ist_sid(sid) {
        return None;
    }
    let breit: Vec<u16> = sid.encode_utf16().chain(std::iter::once(0)).collect();
    let mut psid: PSID = std::ptr::null_mut();
    // SAFETY: `breit` ist nullterminiert; `psid` gibt LocalFree unten frei.
    if unsafe { ConvertStringSidToSidW(breit.as_ptr(), &mut psid) } == 0 || psid.is_null() {
        return None;
    }
    let mut name = [0u16; 256];
    let mut domaene = [0u16; 256];
    let (mut n, mut d) = (name.len() as u32, domaene.len() as u32);
    let mut art: SID_NAME_USE = 0;
    // SAFETY: Puffer und Längen passen zusammen; `psid` ist gültig.
    let ok = unsafe { LookupAccountSidW(std::ptr::null(), psid, name.as_mut_ptr(), &mut n, domaene.as_mut_ptr(), &mut d, &mut art) };
    // SAFETY: von ConvertStringSidToSidW mit LocalAlloc angelegt.
    unsafe { LocalFree(psid as _) };
    if ok == 0 || n == 0 {
        return None;
    }
    let text = |b: &[u16], l: u32| String::from_utf16_lossy(&b[..(l as usize).min(b.len())]);
    Some((text(&domaene, d), text(&name, n)))
}

#[cfg(not(windows))]
pub fn konto_aus_sid(_sid: &str) -> Option<(String, String)> {
    None
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod endpunkte;
