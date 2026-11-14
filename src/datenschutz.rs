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
//!   [`inhaber_setzen`] und `oeffnen::inhaber_profil`);
//! - die Formen von macOS und Linux, auf jeder Plattform (ein Text kann von
//!   einem anderen Rechner stammen): `/Users/<name>`, `/home/<name>`, auch
//!   hinter `/System/Volumes/Data` und `/Volumes/<x>`, `/root` und
//!   `/var/root`, mit `file://` davor und URL-kodiert (`%2F`). Dazu drei
//!   Orte, die kein Profil sind, aber zum Konto oder Rechner gehören:
//!   `/Volumes/<name>` wird `/Volumes/%VOLUME%`, `/run/user/<n>` wird
//!   `/run/user/%UID%`, der Temp-Ordner je Konto `/var/folders/<xx>/<kennung>`
//!   (auch unter `/private`) wird `%TMPDIR%`. Ein solcher Pfad beginnt an
//!   einer Wortgrenze mit genau einem Schrägstrich: `https://…/users/x`,
//!   `ssh://host/Users/x` und `host:/Users/x` bleiben, wie sie sind. Er
//!   beginnt auch nach dem Doppelpunkt einer Liste von Pfaden
//!   (`PATH=/usr/bin:/home/<name>/bin`, `$PATH:/home/<name>/bin`), nach
//!   `file://localhost`, nach einem Schalter wie `-I` und mit `\/` wie in
//!   JSON (`"\/Users\/<name>"`).
//!
//! Jeder Text wird nur einmal gelesen: Keine Prüfung sucht weiter zurück als
//! eine feste Zahl von Zeichen (Test `langer_text_ohne_leerraum`).
//!
//! Der Benutzername allein (etwa in `DOMÄNE\name` oder einem Satz) wird zu
//! `%USERNAME%`, wenn er bekannt ist und als ganzes Wort dasteht.
//!
//! Der Name des Rechners wird zu `%COMPUTERNAME%`: Er trägt oft den Namen der
//! Person (etwa `<name>s-MacBook`). Das Feld `host` der Rechner-Werte bekommt
//! fest den Platzhalter ([`rechner_verbergen`]), und in jedem anderen Text
//! wird der bekannte Name als ganzes Wort ersetzt (etwa `\\<rechner>\…`). Der
//! Name eines Datenträgers (`DiskInfo.name`) wird zu `%VOLUME%`, wenn er
//! nicht leer ist: Er ist auf jeder Plattform frei vergeben.
//!
//! Ohne bekannten Inhaber (Konsole, Tests) gelten das eigene Profil
//! (`USERPROFILE`, sonst `HOME`) und der eigene Name (`USERNAME`, sonst
//! `USER` und `LOGNAME`): Dann läuft der Prozess als Inhaber. Im Dienst setzt der Start beides aus der SID, nie das
//! Dienstkonto.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::Serialize;
use serde_json::Value;

/// Platzhalter für das Profil (wie in `speicher::anzeige_pfad` und `pfade.ts`).
pub const PROFIL_PLATZHALTER: &str = "%USERPROFILE%";
/// Platzhalter für den Benutzernamen allein.
pub const NAME_PLATZHALTER: &str = "%USERNAME%";
/// Platzhalter für den Namen des Rechners.
pub const RECHNER_PLATZHALTER: &str = "%COMPUTERNAME%";
/// Platzhalter für den Namen eines Datenträgers (unter `/Volumes` und in `DiskInfo.name`).
pub const VOLUME_PLATZHALTER: &str = "%VOLUME%";
/// Platzhalter für die Benutzernummer in `/run/user/<n>`.
pub const UID_PLATZHALTER: &str = "%UID%";
/// Platzhalter für den Temp-Ordner je Konto (`/var/folders/<xx>/<kennung>`).
pub const TMPDIR_PLATZHALTER: &str = "%TMPDIR%";

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
    /// Namen des Rechners (ganz und ohne den Teil ab dem ersten Punkt).
    pub rechner: Vec<String>,
}

impl Bekannt {
    /// Aus einem Profil und einem Namen. Der Ordnername des Profils zählt
    /// auch als Name. Leeres und nicht Absolutes fällt weg. Ob ein Profil
    /// absolut ist, entscheidet sein Text ([`absolut_als_text`]), nicht die
    /// Plattform: `C:\Users\x` gilt auch unter macOS, `/Users/x` auch unter
    /// Windows (dort ist es für `Path::is_absolute` keins).
    pub fn aus(profil: Option<&Path>, name: Option<&str>) -> Bekannt {
        Bekannt::aus_mit(profil, name, true)
    }

    /// Wie [`Bekannt::aus`]; `ordnername` sagt, ob der Ordnername des Profils
    /// als Name zählt.
    fn aus_mit(profil: Option<&Path>, name: Option<&str>, ordnername: bool) -> Bekannt {
        let mut b = Bekannt::default();
        if let Some(roh) = profil.map(|p| p.display().to_string()).filter(|t| absolut_als_text(t)) {
            let text = roh.trim_end_matches(['\\', '/']).to_string();
            if text.len() > 3 {
                b.profile.push(text.clone());
            }
            // Der letzte Teil, nach `\` oder `/` getrennt (auf jeder Plattform gleich).
            if ordnername && let Some(n) = text.rsplit(['\\', '/']).next().filter(|n| !n.is_empty() && !n.ends_with(':')) {
                b.name_dazu(n);
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

    /// Dazu der Name des Rechners: ganz und, wenn er Punkte hat, auch der Teil
    /// vor dem ersten (`<name>.local`, `<name>.beispiel.example`). Leeres und
    /// Namen unter drei Zeichen fallen weg, wie beim Benutzernamen.
    pub fn mit_rechner(mut self, name: Option<&str>) -> Bekannt {
        let Some(ganz) = name.map(str::trim).filter(|n| !n.is_empty()) else {
            return self;
        };
        // `localhost` ist kein Name dieses Rechners, sondern jedes; ihn zu ersetzen verstümmelte Links.
        if ganz.split('.').next().is_some_and(|n| n.eq_ignore_ascii_case("localhost")) {
            return self;
        }
        for n in [Some(ganz), ganz.split('.').next()].into_iter().flatten() {
            if n.chars().count() >= NAME_MINDESTENS && !self.rechner.iter().any(|m| m.to_lowercase() == n.to_lowercase()) {
                self.rechner.push(n.to_string());
            }
        }
        self
    }

    /// Aus dieser Umgebung: für Konsole und Tests. Siehe [`Bekannt::aus_werten`].
    pub fn aus_umgebung() -> Bekannt {
        let pfad = |n: &str| std::env::var_os(n).map(PathBuf::from);
        let text = |n: &str| std::env::var(n).ok();
        Bekannt::aus_werten(pfad("USERPROFILE"), pfad("HOME"), text("USERNAME"), &[text("USER"), text("LOGNAME")]).mit_rechner(rechnername().as_deref())
    }

    /// Aus den Werten der Umgebung. Unter Windows stehen Profil und Name in
    /// `USERPROFILE` und `USERNAME`; sind sie gesetzt, gelten nur sie (wie
    /// bisher). Fehlt `USERPROFILE`, gilt `HOME`; fehlt `USERNAME`, gelten
    /// `USER` und `LOGNAME` (macOS, Linux). Leere Werte zählen als fehlend.
    /// Der Ordnername von `HOME` zählt nur als Name, wenn `HOME` direkt unter
    /// `/Users` oder `/home` liegt: Bei einem Dienstkonto (`/var/lib/<dienst>`)
    /// wäre er kein Kontoname und würde überall ersetzt. Bei root kommt der
    /// Name weiter aus `USER` und `LOGNAME`.
    pub fn aus_werten(userprofile: Option<PathBuf>, home: Option<PathBuf>, username: Option<String>, weitere: &[Option<String>]) -> Bekannt {
        let gesetzt = |p: &PathBuf| !p.as_os_str().is_empty();
        let username = username.filter(|n| !n.trim().is_empty());
        let mut b = match userprofile.filter(gesetzt) {
            Some(p) => Bekannt::aus(Some(&p), username.as_deref()),
            None => {
                let home = home.filter(gesetzt);
                let konto = home.as_deref().is_some_and(heim_eines_kontos);
                Bekannt::aus_mit(home.as_deref(), username.as_deref(), konto)
            }
        };
        if username.is_none() {
            for n in weitere.iter().flatten() {
                b.name_dazu(n);
            }
        }
        b
    }
}

/// Was der Dienst zum Verbergen kennt: Profil und Name des Inhabers aus der
/// SID und der Name dieses Rechners (`oeffnen::dienst_einrichten`).
pub fn bekannt_fuer_dienst(profil: Option<&Path>, name: Option<&str>) -> Bekannt {
    Bekannt::aus(profil, name).mit_rechner(rechnername().as_deref())
}

/// Der Name dieses Rechners, wie ihn das System nennt; `None`, wenn es keinen nennt.
pub fn rechnername() -> Option<String> {
    sysinfo::System::host_name().filter(|n| !n.trim().is_empty())
}

/// Liegt `home` direkt unter `/Users` oder `/home` (`/Users/<name>`)?
fn heim_eines_kontos(home: &Path) -> bool {
    let text = home.display().to_string();
    let mut teile = text.trim_end_matches('/').split('/');
    teile.next() == Some("") && matches!(teile.next(), Some("Users" | "home")) && teile.next().is_some_and(|n| !n.is_empty()) && teile.next().is_none()
}

/// Ist `text` ein absoluter Pfad in einer der bekannten Schreibweisen? Als
/// Text geprüft, damit das Urteil auf jeder Plattform gleich ausfällt:
/// Laufwerk (`C:\…`, `C:/…`), Netzwerk- oder Gerätepfad (`\\…`, `//…`) oder
/// POSIX (`/…`).
pub fn absolut_als_text(text: &str) -> bool {
    let t = text.as_bytes();
    let laufwerk = t.len() >= 3 && t[0].is_ascii_alphabetic() && t[1] == b':' && matches!(t[2], b'\\' | b'/');
    laufwerk || text.starts_with('/') || text.starts_with(r"\\")
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

/// Verbirgt Profilpfade, den Benutzernamen des Inhabers und den Namen des
/// Rechners (siehe Kopf).
pub fn verbergen(text: &str) -> String {
    verbergen_mit(text, bekannt())
}

/// Wie [`verbergen`], mit ausdrücklich angegebenem Inhaber (Tests, Werkzeuge).
pub fn verbergen_mit(text: &str, b: &Bekannt) -> String {
    let ohne_pfade = pfade_ersetzen(text, b);
    // Zuerst der Rechner: Sein Name kann den Benutzernamen als Wort enthalten (`<name>-rechner`).
    let ohne_rechner = rechner_ersetzen(&ohne_pfade, &b.rechner);
    namen_ersetzen(&ohne_rechner, &b.namen)
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

/// Die Rechner-Werte für jede Antwort: Der Name des Rechners wird fest der
/// Platzhalter, auf jeder Plattform und auch mit Anmeldung (wie Profil und
/// Kontoname), die Namen der Datenträger nach [`platten_verbergen`].
pub fn rechner_verbergen(s: &mut crate::pc::system::Snapshot) {
    s.host = RECHNER_PLATZHALTER.to_string();
    platten_verbergen(&mut s.disks);
}

/// Der Name jedes Datenträgers wird der Platzhalter. Ein leerer Name bleibt
/// leer, damit nichts vorgetäuscht wird. `mount` bleibt und läuft wie jeder
/// Text durch [`verbergen`]; nur ein Einhängepunkt direkt unter `/Volumes`
/// wird hier schon ganz ersetzt, weil der Name dort Leerzeichen tragen kann
/// (`/Volumes/<zwei Wörter>`) und [`verbergen`] einen Namen am Leerraum beendet.
pub fn platten_verbergen(platten: &mut [crate::pc::system::DiskInfo]) {
    const VOLUMES: &str = "/Volumes/";
    for p in platten.iter_mut() {
        if !p.name.is_empty() {
            p.name = VOLUME_PLATZHALTER.to_string();
        }
        let unter_volumes = p.mount.get(..VOLUMES.len()).is_some_and(|v| v.eq_ignore_ascii_case(VOLUMES));
        if unter_volumes && !p.mount[VOLUMES.len()..].trim_end_matches('/').contains('/') && p.mount.len() > VOLUMES.len() {
            p.mount = format!("{}{VOLUME_PLATZHALTER}", &p.mount[..VOLUMES.len()]);
        }
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

/// Endet `t[..i]` auf ein Escape (`\n`, `\t`, `\u0022`, `\x22`, oktal
/// `\042`), auf ein kodiertes Zeichen (`%22`, doppelt kodiert `%2522`) oder
/// auf einen Farbcode (`ESC[34m`)? Dahinter kann ein Laufwerk beginnen,
/// obwohl davor ein Buchstabe oder eine Ziffer steht. Jede Form hat eine
/// feste größte Länge: Die Prüfung liest nie weiter als 20 Zeichen zurück.
fn nach_escape_oder_kodiert(t: &[u8], i: usize) -> bool {
    let hex = |n: usize| i >= n && t[i - n..i].iter().all(u8::is_ascii_hexdigit);
    let rueck = |n: usize| i > n && t[i - n - 1] == b'\\';
    let buchstabe = |n: usize, c: u8| i > n && t[i - n - 1].eq_ignore_ascii_case(&c);
    // `\n`, `\t`.
    if i >= 2 && t[i - 1].is_ascii_alphabetic() && t[i - 2] == b'\\' {
        return true;
    }
    // `\uXXXX`, `\xXX`.
    if (hex(4) && buchstabe(4, b'u') && rueck(5)) || (hex(2) && buchstabe(2, b'x') && rueck(3)) {
        return true;
    }
    // Oktal: ein bis drei Ziffern hinter dem Rückstrich.
    if (1..=3).any(|n| i >= n && t[i - n..i].iter().all(|c| (b'0'..=b'7').contains(c)) && rueck(n)) {
        return true;
    }
    // `%XY`, `%25XY`.
    if [3usize, 5].iter().any(|&n| i >= n && prozent(t, i - n).is_some_and(|(laenge, _)| laenge == n)) {
        return true;
    }
    // `ESC[…m` mit höchstens 16 Ziffern und Strichpunkten.
    if i >= 3 && t[i - 1] == b'm' {
        let mut k = i - 1;
        while k > 0 && i - k <= 17 && (t[k - 1].is_ascii_digit() || t[k - 1] == b';') {
            k -= 1;
        }
        return k >= 2 && t[k - 1] == b'[' && t[k - 2] == 0x1b;
    }
    false
}

/// Steht direkt vor `i` ein Schalter wie `-I`, `-L` oder `-isystem`
/// (`-I/Users/<name>`, `-LC:\Users\<name>`)? Ein oder zwei Bindestriche an
/// einer Wortgrenze, dann bis zu 16 Buchstaben. Ein Rechnername mit
/// Bindestrich (`mein-host`) ist keiner.
fn schalter_davor(t: &[u8], i: usize) -> bool {
    let mut k = i;
    while k > 0 && i - k < 16 && t[k - 1].is_ascii_alphabetic() {
        k -= 1;
    }
    if k == i || k == 0 || t[k - 1] != b'-' {
        return false;
    }
    k -= 1;
    if k > 0 && t[k - 1] == b'-' {
        k -= 1;
    }
    k == 0 || !(t[k - 1].is_ascii_alphanumeric() || matches!(t[k - 1], b'_' | b'.' | b'@' | b'-') || t[k - 1] >= 0x80)
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
        && (wortgrenze_davor(t, i) || nach_escape_oder_kodiert(t, i) || schalter_davor(t, i))
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

/// Beginnt an `j` ein Laufwerk (Buchstabe an einer Wortgrenze, Doppelpunkt,
/// Trenner)? Dann endet dort ein Name (`/Users/C:\Users\<name>`, `max-D:\…`).
fn laufwerk_beginnt(t: &[u8], j: usize) -> bool {
    t[j].is_ascii_alphabetic() && wortgrenze_davor(t, j) && doppelpunkt(t, j + 1).is_some_and(|n| trenner_folge(t, n).is_some())
}

/// Ein Teil eines Pfads ab `i` bis zum nächsten Trennzeichen oder Stopp.
/// Mit Leerzeichen nur, wenn danach ein Trennzeichen kommt (`Max Muster\…`);
/// sonst endet er am ersten Leerzeichen (`…\beispiel ist kaputt`).
fn teil(text: &str, i: usize) -> usize {
    let t = text.as_bytes();
    let mut j = i;
    while j < t.len() && trenner(t, j).is_none() && !stopp(t[j]) && !(t[j] == b'%' && doppelpunkt(t, j).is_some()) && !laufwerk_beginnt(t, j) {
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
    /// Ohne Laufwerk, ab der Wurzel (`/Users/beispiel`, `/Volumes/x/heim`).
    Posix(Vec<String>),
}

/// Die bekannten Profile zerlegt. `\\?\C:\…` und `\\.\C:\…` gelten wie
/// `C:\…`, `\\?\UNC\rechner\…` wie `\\rechner\…` (so stehen Profile in der
/// Registry oder nach `canonicalize`).
fn profil_teile(b: &Bekannt) -> Vec<ProfilForm> {
    b.profile
        .iter()
        .filter_map(|p| {
            // POSIX: genau ein Schrägstrich vorn (`//rechner/…` ist ein Netzwerkpfad).
            if p.starts_with('/') && !p.starts_with("//") {
                let t: Vec<String> = p.split('/').filter(|s| !s.is_empty()).map(str::to_string).collect();
                return (!t.is_empty()).then_some(ProfilForm::Posix(t));
            }
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
            ProfilForm::Laufwerk(..) | ProfilForm::Posix(..) => None,
        })
        .max()
}

// ----- POSIX (macOS, Linux) -----

/// Ein Schrägstrich an `i` (`/` oder `%2F`, auch `%252F`, und `\/` wie in
/// JSON), kein Rückstrich allein; gibt seine Länge.
fn schraeg(t: &[u8], i: usize) -> Option<usize> {
    match t.get(i)? {
        b'/' => Some(1),
        b'\\' if t.get(i + 1) == Some(&b'/') => Some(2),
        b'%' => prozent(t, i).filter(|(_, k)| k == b"2f").map(|(n, _)| n),
        _ => None,
    }
}

/// Endet `t[..i]` auf `file:` (auch mit kodiertem Doppelpunkt)?
fn file_direkt_davor(t: &[u8], i: usize) -> bool {
    let wort = |ende: usize| ende >= 4 && t[ende - 4..ende].eq_ignore_ascii_case(b"file") && wortgrenze_davor(t, ende - 4);
    (i >= 1 && t[i - 1] == b':' && wort(i - 1))
        || [5usize, 3].iter().any(|&n| i >= n && prozent(t, i - n).is_some_and(|(l, k)| l == n && &k == b"3a") && wort(i - n))
}

/// Liest einen Text einmal vorwärts mit und weiß an jedem Doppelpunkt, ob
/// er der Trenner einer Liste von Pfaden ist (`PATH=/usr/bin:/home/<name>/bin`,
/// `$PATH:/home/<name>/bin`): Ja, wenn das Glied davor (seit dem letzten
/// Leerraum, `=` oder `:`) einen `/`, `$`, `~`, `%` oder `{` enthält. In
/// `[konto@]host` vor einem Ziel in der scp-Form kommt keins davon vor.
/// Ein `:` mit `//` dahinter (Schema einer Adresse) beginnt kein Glied.
/// Jedes Glied wird so nur einmal gelesen, wie lang der Text auch ist.
#[derive(Debug, Default)]
struct Glied {
    /// Bis hierher ist der Text gelesen.
    bis: usize,
    /// Das laufende Glied enthält ein Zeichen einer Liste.
    liste: bool,
    /// Das laufende Glied folgt auf das Schema einer Adresse (`https://…`).
    adresse: bool,
}

impl Glied {
    /// Ist der Doppelpunkt an `k` der Trenner einer Liste? Die Fragen kommen
    /// mit wachsendem `k`; eine Frage weiter vorn liest von vorn.
    fn liste_vor(&mut self, t: &[u8], k: usize) -> bool {
        if k < self.bis {
            *self = Glied::default();
        }
        for n in self.bis..k {
            match t[n] {
                b':' => {
                    self.liste = false;
                    self.adresse = t.get(n + 1) == Some(&b'/') && t.get(n + 2) == Some(&b'/');
                }
                c if c.is_ascii_whitespace() || c == b'=' => {
                    self.liste = false;
                    self.adresse = false;
                }
                b'/' | b'$' | b'~' | b'%' | b'{' if !self.adresse => self.liste = true,
                _ => {}
            }
        }
        self.bis = k;
        self.liste
    }
}

/// Endet `t[..i]` auf `file://localhost` (ohne Groß/Klein, die Schrägstriche
/// auch als `\/` wie in JSON)?
fn file_localhost_davor(t: &[u8], i: usize) -> bool {
    const HOST: &[u8] = b"localhost";
    if i < HOST.len() || !t[i - HOST.len()..i].eq_ignore_ascii_case(HOST) {
        return false;
    }
    let mut k = i - HOST.len();
    for _ in 0..2 {
        if k == 0 || t[k - 1] != b'/' {
            return false;
        }
        k -= 1;
        if k > 0 && t[k - 1] == b'\\' {
            k -= 1;
        }
    }
    file_direkt_davor(t, k)
}

/// Der Anfang eines POSIX-Pfads an `i`; gibt die Stelle nach dem führenden
/// Schrägstrich und wo der Pfad selbst beginnt (sein letzter führender
/// Schrägstrich; nur hinter einem kodierten `file%3A` ist das nicht `i`).
/// Er steht an einer Wortgrenze und allein: Nach `…org/` oder einem zweiten
/// Schrägstrich (`https://host/…`) beginnt keiner, nach `host:` (Git-Remote
/// in der scp-Form) auch nicht. Nach dem Doppelpunkt einer Liste von Pfaden
/// beginnt einer ([`Glied`]), nach `file://localhost` und nach einem Schalter
/// wie `-I` auch. Mehrere
/// Schrägstriche gelten nur direkt hinter `file:` (`file:///Users/<name>`).
fn posix_anfang(t: &[u8], i: usize, glied: &mut Glied) -> Option<(usize, usize)> {
    let n = schraeg(t, i)?;
    if file_localhost_davor(t, i) {
        return Some((i + n, i));
    }
    if !(wortgrenze_davor(t, i) || schalter_davor(t, i)) || (i > 0 && matches!(t[i - 1], b'/' | b'\\')) {
        return None;
    }
    // Direkt hinter einem kodierten Trenner (`%2F%2FUsers`) beginnt kein eigener Pfad.
    if [5usize, 3].iter().any(|&l| i >= l && prozent(t, i - l).is_some_and(|(m, k)| m == l && matches!(&k, b"5c" | b"2f"))) {
        return None;
    }
    let file = file_direkt_davor(t, i);
    // `host:/pfad`: ein Wort mit Doppelpunkt davor, das weder `file` noch ein Pfad in einer Liste ist.
    // Ein einzelner Laufwerksbuchstabe vor `:` ist kein Trenner einer Liste (`C:/bin:C:/Users/<name>`).
    if !file && i >= 2 && t[i - 1] == b':' && (t[i - 2].is_ascii_alphanumeric() || t[i - 2] == b']') {
        let laufwerk = t[i - 2].is_ascii_alphabetic() && (i < 3 || !t[i - 3].is_ascii_alphabetic());
        if laufwerk || !glied.liste_vor(t, i - 1) {
            return None;
        }
    }
    let mut j = i + n;
    let mut beginn = i;
    if schraeg(t, j).is_some() {
        if !file {
            return None;
        }
        while let Some(m) = schraeg(t, j) {
            beginn = j;
            j += m;
        }
        // Hinter `file:` im Klartext fällt alles ab `file:` weg (wie bei den
        // Laufwerken); hinter der kodierten Form bleibt, was vor dem Pfad steht.
        if i > 0 && t[i - 1] == b':' {
            beginn = i;
        }
    }
    Some((j, beginn))
}

/// Die Ordner `teile` ab `i`, je mit Trennzeichen danach; gibt die Stelle hinter dem letzten Trenner.
fn ordner_folge(text: &str, mut i: usize, teile: &[&str]) -> Option<usize> {
    for k in teile {
        i = trenner_folge(text.as_bytes(), komponente_an(text, i, k)?)?;
    }
    Some(i)
}

/// Eine Stelle in POSIX-Form ab `i`: was zu ersetzen ist (von, bis) und
/// womit. `von == i` heißt: der ganze Pfad von seinem Anfang an (dann fällt
/// auch ein `file:` im Klartext davor weg, wie bei den Laufwerken).
fn posix_stelle(text: &str, i: usize, bekannte: &[ProfilForm], glied: &mut Glied) -> Option<(usize, usize, &'static str)> {
    let t = text.as_bytes();
    let (j, beginn) = posix_anfang(t, i, glied)?;
    // Zuerst die bekannten Profile ohne Laufwerk.
    let bekannt = bekannte
        .iter()
        .filter_map(|p| match p {
            ProfilForm::Posix(teile) => bekanntes_profil(text, j, &teile.iter().map(String::as_str).collect::<Vec<_>>()),
            _ => None,
        })
        .max();
    if let Some(e) = bekannt {
        return Some((beginn, e, PROFIL_PLATZHALTER));
    }
    // Vorsätze, hinter denen wieder `Users/<name>` stehen kann.
    let mut k = j;
    let mut volume = None;
    if let Some(n) = ordner_folge(text, j, &["System", "Volumes", "Data"]) {
        k = n;
    } else if let Some(n) = ordner_folge(text, j, &["Volumes"]) {
        let ende = teil(text, n);
        if ende > n {
            volume = Some((n, ende));
            k = trenner_folge(t, ende).unwrap_or(ende);
        }
    }
    for ordner in ["Users", "home"] {
        if let Some(n) = ordner_folge(text, k, &[ordner]) {
            let ende = teil(text, n);
            if ende > n {
                return Some((beginn, ende, PROFIL_PLATZHALTER));
            }
        }
    }
    if let Some((von, bis)) = volume {
        return Some((von, bis, VOLUME_PLATZHALTER));
    }
    // Das Profil von root.
    for teile in [&["root"][..], &["var", "root"], &["private", "var", "root"]] {
        let (letzter, davor) = teile.split_last()?;
        if let Some(n) = ordner_folge(text, j, davor)
            && let Some(e) = komponente_an(text, n, letzter)
        {
            return Some((beginn, e, PROFIL_PLATZHALTER));
        }
    }
    // `/run/user/<n>`: nur die Nummer.
    if let Some(n) = ordner_folge(text, j, &["run", "user"]) {
        let ende = n + t[n..].iter().take_while(|c| c.is_ascii_digit()).count();
        if ende > n && !t.get(ende).is_some_and(u8::is_ascii_alphanumeric) {
            return Some((n, ende, UID_PLATZHALTER));
        }
    }
    // Temp-Ordner je Konto unter macOS: `/var/folders/<xx>/<kennung>`, auch unter `/private`.
    for vor in [&["var", "folders"][..], &["private", "var", "folders"]] {
        if let Some(n) = ordner_folge(text, j, vor) {
            let xx = teil(text, n);
            if xx > n
                && let Some(m) = trenner_folge(t, xx)
            {
                let kennung = teil(text, m);
                if kennung > m {
                    return Some((beginn, kennung, TMPDIR_PLATZHALTER));
                }
            }
        }
    }
    None
}

fn pfade_ersetzen(text: &str, b: &Bekannt) -> String {
    let t = text.as_bytes();
    // Schnell heraus: ohne Trenn- oder Prozentzeichen kein Pfad.
    if !t.iter().any(|c| matches!(c, b'\\' | b'/' | b'%')) {
        return text.to_string();
    }
    let bekannte = profil_teile(b);
    let mut glied = Glied::default();
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
            // Kein Laufwerk: ein Pfad in der Form von macOS oder Linux?
            match posix_stelle(text, i, &bekannte, &mut glied) {
                Some((von, bis, ersatz)) => {
                    let anfang = if von == i { file_davor(t, i).filter(|a| *a >= kopiert).unwrap_or(i) } else { von };
                    aus.push_str(&text[kopiert..anfang]);
                    aus.push_str(ersatz);
                    kopiert = bis;
                    i = bis;
                }
                None => i += 1,
            }
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
        aus = ein_name(&aus, n, NAME_PLATZHALTER, char::is_alphanumeric);
    }
    aus
}

/// Ersetzt `name` durch `platzhalter`, wo er als ganzes Wort dasteht (ohne
/// Groß/Klein). Was zu einem Wort gehört, sagt `wortzeichen`.
fn ein_name(text: &str, name: &str, platzhalter: &str, wortzeichen: fn(char) -> bool) -> String {
    let klein_name: String = name.to_lowercase();
    let erstes = klein_name.chars().next();
    let mut aus = String::with_capacity(text.len());
    let mut kopiert = 0;
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
                aus.push_str(platzhalter);
                kopiert = ende;
            }
        }
        vorher = Some(c);
    }
    aus.push_str(&text[kopiert..]);
    aus
}

/// Jeder bekannte Name des Rechners als ganzes Wort (ohne Groß/Klein) wird
/// `%COMPUTERNAME%`. Ein Bindestrich gehört hier zum Wort: `<name>-alt` ist
/// ein anderer Rechner.
fn rechner_ersetzen(text: &str, rechner: &[String]) -> String {
    let wortzeichen: fn(char) -> bool = |c| c.is_alphanumeric() || c == '-';
    let mut aus = text.to_string();
    // Der längere Name zuerst (`<name>.local` vor `<name>`).
    let mut namen: Vec<&String> = rechner.iter().filter(|n| n.chars().count() >= NAME_MINDESTENS).collect();
    namen.sort_by_key(|n| std::cmp::Reverse(n.len()));
    for n in namen {
        aus = ein_name(&aus, n, RECHNER_PLATZHALTER, wortzeichen);
    }
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
