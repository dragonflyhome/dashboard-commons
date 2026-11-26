//! Das Cookie mit dem Ausweis. Ein Schema für alle Plattformen: Der Grundname
//! ist `dashboard`, dazu immer HttpOnly, SameSite=Strict, Path=/, nie eine
//! Domain. Nie in der URL, nie in JavaScript lesbar.
//!
//! Was sich je Plattform unterscheidet, hängt an einer einzigen Frage: Gilt
//! das Cookie nur über eine sichere Verbindung (`SICHER`)?
//!
//! - Ja (Windows): Name `__Host-dashboard` und das Merkmal Secure. Edge
//!   erlaubt Secure auf http://localhost. Das Präfix `__Host-` sorgt dafür,
//!   dass der Browser es nur vom Dashboard selbst annimmt (ohne Domain, mit
//!   Path=/, Secure).
//! - Nein (außerhalb von Windows): Name `dashboard`, ohne Secure. Safari
//!   speichert ein Cookie mit Secure über http://localhost nicht, und
//!   `__Host-` verlangt Secure; also entfällt dort beides zusammen.
//!
//! Grenze auf beiden Wegen: Cookies sind nicht an den Port gebunden. Ein
//! anderes Programm auf einem anderen Port von localhost kann ein Cookie
//! gleichen Namens setzen; lesen kann es den Ausweis nicht, und ein fremder
//! Wert besteht die Prüfung der Signatur nicht.

use axum::http::{HeaderMap, HeaderValue, header};

/// Ob das Cookie nur über eine sichere Verbindung gilt (Präfix `__Host-` und Secure).
pub const SICHER: bool = cfg!(windows);

/// Der Name auf dieser Plattform; `lesen` nimmt nur ihn an.
pub const NAME: &str = name(SICHER);

/// Der Name nach dem Schema: Grundname `dashboard`, mit `__Host-` nur zusammen mit Secure.
const fn name(sicher: bool) -> &'static str {
    if sicher { "__Host-dashboard" } else { "dashboard" }
}

/// Die Merkmale nach dem Schema, ohne `Max-Age`.
const fn merkmale(sicher: bool) -> &'static str {
    if sicher { "Path=/; HttpOnly; Secure; SameSite=Strict" } else { "Path=/; HttpOnly; SameSite=Strict" }
}

/// `Set-Cookie` für einen Ausweis, der noch `sekunden` gilt.
pub fn setzen(token: &str, sekunden: i64) -> Option<HeaderValue> {
    setzen_als(SICHER, token, sekunden)
}

fn setzen_als(sicher: bool, token: &str, sekunden: i64) -> Option<HeaderValue> {
    HeaderValue::from_str(&format!("{}={token}; {}; Max-Age={}", name(sicher), merkmale(sicher), sekunden.max(0))).ok()
}

/// `Set-Cookie`, das den Ausweis im Browser löscht: derselbe Name, dieselben Merkmale.
pub fn loeschen() -> HeaderValue {
    loeschen_als(SICHER)
}

fn loeschen_als(sicher: bool) -> HeaderValue {
    if sicher {
        HeaderValue::from_static("__Host-dashboard=; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=0")
    } else {
        HeaderValue::from_static("dashboard=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0")
    }
}

/// Der Ausweis aus den `Cookie`-Kopfzeilen. Steht das Cookie mehrmals da
/// oder enthält es andere Zeichen als Base64url und Punkte, gilt keins.
pub fn lesen(kopf: &HeaderMap) -> Option<String> {
    lesen_als(SICHER, kopf)
}

/// Steht ein Cookie mit genau dem eigenen Namen da (gleich welcher Wert)? Fremde
/// Namen, die ihn nur enthalten (etwa `dashboard_layout`), zählen nicht.
pub fn vorhanden(kopf: &HeaderMap) -> bool {
    vorhanden_als(SICHER, kopf)
}

fn vorhanden_als(sicher: bool, kopf: &HeaderMap) -> bool {
    kopf.get_all(header::COOKIE)
        .iter()
        .filter_map(|w| w.to_str().ok())
        .flat_map(|t| t.split(';'))
        .any(|teil| teil.trim().split_once('=').is_some_and(|(n, _)| n == name(sicher)))
}

fn lesen_als(sicher: bool, kopf: &HeaderMap) -> Option<String> {
    let eigener = name(sicher);
    let mut gefunden: Option<&str> = None;
    for wert in kopf.get_all(header::COOKIE) {
        let Ok(text) = wert.to_str() else { continue };
        for teil in text.split(';') {
            let Some((name, wert)) = teil.trim().split_once('=') else { continue };
            if name != eigener {
                continue;
            }
            if gefunden.is_some() {
                return None;
            }
            gefunden = Some(wert);
        }
    }
    let wert = gefunden?;
    let sauber =
        !wert.is_empty() && wert.len() <= super::jwt::LAENGE_HOECHSTENS && wert.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'));
    sauber.then(|| wert.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kopf(werte: &[&str]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for w in werte {
            h.append(header::COOKIE, HeaderValue::from_str(w).unwrap());
        }
        h
    }

    #[test]
    fn setzen_hat_alle_merkmale() {
        let s = setzen_als(true, "a.b.c", 900).unwrap();
        assert_eq!(s.to_str().unwrap(), "__Host-dashboard=a.b.c; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=900");
        let o = setzen_als(false, "a.b.c", 900).unwrap();
        assert_eq!(o.to_str().unwrap(), "dashboard=a.b.c; Path=/; HttpOnly; SameSite=Strict; Max-Age=900");
        for sicher in [true, false] {
            let s = setzen_als(sicher, "a.b.c", -5).unwrap();
            let s = s.to_str().unwrap();
            assert!(s.ends_with("Max-Age=0") && !s.contains("Domain"), "{s}");
            assert_eq!(s.contains("Secure"), sicher, "Secure und das Präfix gehören zusammen: {s}");
            assert_eq!(s.starts_with("__Host-"), sicher, "{s}");
        }
    }

    #[test]
    fn loeschen_hat_dieselben_merkmale() {
        for sicher in [true, false] {
            let erwartet = format!("{}=; {}; Max-Age=0", name(sicher), merkmale(sicher));
            assert_eq!(loeschen_als(sicher).to_str().unwrap(), erwartet);
        }
    }

    #[test]
    fn lesen_nur_eindeutig_und_sauber() {
        for (sicher, n) in [(true, "__Host-dashboard"), (false, "dashboard")] {
            let l = |werte: &[String]| lesen_als(sicher, &kopf(&werte.iter().map(String::as_str).collect::<Vec<_>>()));
            assert_eq!(l(&[format!("x=1; {n}=a.b_c-d")]).as_deref(), Some("a.b_c-d"));
            assert_eq!(l(&["x=1".into(), format!("{n}=a.b.c")]).as_deref(), Some("a.b.c"));
            assert_eq!(l(&[format!("{n}=a; {n}=b")]), None, "doppelt");
            assert_eq!(l(&[format!("{n}=a<b")]), None);
            assert_eq!(l(&[format!("{n}=")]), None);
            assert_eq!(l(&[]), None);
        }
    }

    #[test]
    fn vorhanden_nur_mit_genauem_namen() {
        for (sicher, n) in [(true, "__Host-dashboard"), (false, "dashboard")] {
            assert!(vorhanden_als(sicher, &kopf(&[&format!("x=1; {n}=kaputt<")])), "{n}");
            assert!(!vorhanden_als(sicher, &kopf(&[&format!("{n}_layout=1; mein{n}=2")])), "{n}: fremde Namen");
            assert!(!vorhanden_als(sicher, &kopf(&[])));
        }
    }

    #[test]
    fn lesen_nimmt_nur_den_eigenen_namen() {
        assert_eq!(lesen_als(true, &kopf(&["dashboard=a.b.c"])), None, "ohne Präfix zählt nicht, wo es sicher sein muss");
        assert_eq!(lesen_als(false, &kopf(&["__Host-dashboard=a.b.c"])), None, "mit Präfix zählt nicht, wo es ohne gesetzt wird");
        assert_eq!(lesen_als(false, &kopf(&["__Host-dashboard=x.y.z; dashboard=a.b.c"])).as_deref(), Some("a.b.c"));
        assert_eq!(lesen_als(true, &kopf(&["dashboard=x.y.z; __Host-dashboard=a.b.c"])).as_deref(), Some("a.b.c"));
    }

    #[cfg(windows)]
    #[test]
    fn unter_windows_sicher_mit_praefix() {
        assert_eq!(NAME, "__Host-dashboard");
        assert_eq!(setzen("a.b.c", 900).unwrap().to_str().unwrap(), "__Host-dashboard=a.b.c; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=900");
        assert_eq!(loeschen().to_str().unwrap(), "__Host-dashboard=; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=0");
        assert_eq!(lesen(&kopf(&["dashboard=a.b.c"])), None);
        assert_eq!(lesen(&kopf(&["__Host-dashboard=a.b.c"])).as_deref(), Some("a.b.c"));
    }

    #[cfg(not(windows))]
    #[test]
    fn ausserhalb_von_windows_ohne_praefix_und_ohne_secure() {
        assert_eq!(NAME, "dashboard");
        assert_eq!(setzen("a.b.c", 900).unwrap().to_str().unwrap(), "dashboard=a.b.c; Path=/; HttpOnly; SameSite=Strict; Max-Age=900");
        assert_eq!(loeschen().to_str().unwrap(), "dashboard=; Path=/; HttpOnly; SameSite=Strict; Max-Age=0");
        assert_eq!(lesen(&kopf(&["__Host-dashboard=a.b.c"])), None);
        assert_eq!(lesen(&kopf(&["dashboard=a.b.c"])).as_deref(), Some("a.b.c"));
    }
}
