//! Das Cookie mit dem Ausweis: `__Host-dashboard`, HttpOnly, Secure (Edge
//! erlaubt Secure auf http://localhost), SameSite=Strict, Path=/. Nie in der
//! URL, nie in JavaScript lesbar. Das Präfix `__Host-` sorgt dafür, dass der
//! Browser es nur vom Dashboard selbst annimmt (ohne Domain, mit Path=/,
//! Secure).

use axum::http::{HeaderMap, HeaderValue, header};

pub const NAME: &str = "__Host-dashboard";

/// `Set-Cookie` für einen Ausweis, der noch `sekunden` gilt.
pub fn setzen(token: &str, sekunden: i64) -> Option<HeaderValue> {
    HeaderValue::from_str(&format!("{NAME}={token}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age={}", sekunden.max(0))).ok()
}

/// `Set-Cookie`, das den Ausweis im Browser löscht.
pub fn loeschen() -> HeaderValue {
    HeaderValue::from_static("__Host-dashboard=; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=0")
}

/// Der Ausweis aus den `Cookie`-Kopfzeilen. Steht das Cookie mehrmals da
/// oder enthält es andere Zeichen als Base64url und Punkte, gilt keins.
pub fn lesen(kopf: &HeaderMap) -> Option<String> {
    let mut gefunden: Option<&str> = None;
    for wert in kopf.get_all(header::COOKIE) {
        let Ok(text) = wert.to_str() else { continue };
        for teil in text.split(';') {
            let Some((name, wert)) = teil.trim().split_once('=') else { continue };
            if name != NAME {
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
        let v = setzen("a.b.c", 900).unwrap();
        let s = v.to_str().unwrap();
        for teil in ["__Host-dashboard=a.b.c", "Path=/", "HttpOnly", "Secure", "SameSite=Strict", "Max-Age=900"] {
            assert!(s.contains(teil), "{teil} fehlt in {s}");
        }
        assert!(!s.contains("Domain"));
        assert!(loeschen().to_str().unwrap().contains("Max-Age=0"));
    }

    #[test]
    fn lesen_nur_eindeutig_und_sauber() {
        assert_eq!(lesen(&kopf(&["x=1; __Host-dashboard=a.b_c-d"])).as_deref(), Some("a.b_c-d"));
        assert_eq!(lesen(&kopf(&["x=1", "__Host-dashboard=a.b.c"])).as_deref(), Some("a.b.c"));
        assert_eq!(lesen(&kopf(&["__Host-dashboard=a; __Host-dashboard=b"])), None, "doppelt");
        assert_eq!(lesen(&kopf(&["__Host-dashboard=a<b"])), None);
        assert_eq!(lesen(&kopf(&["__Host-dashboard="])), None);
        assert_eq!(lesen(&kopf(&["dashboard=a.b.c"])), None, "ohne Präfix zählt nicht");
        assert_eq!(lesen(&kopf(&[])), None);
    }
}
