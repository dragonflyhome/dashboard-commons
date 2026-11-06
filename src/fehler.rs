//! Ein Fehlertyp für alle Routen. Jede Fehlerantwort hat dieselbe Form:
//! `{ "fehler": "…in einfachem Deutsch…", "art": "nicht-gefunden" }`.
//! Auf Browser-Wegen (nicht `/api/*`, kein `Accept: application/json`) macht
//! die Schleuse `sicherheit::fehlerform` daraus denselben Satz als Text; dafür
//! hängt jede Fehlerantwort den Satz als Erweiterung `FehlerSatz` an.
//! Technische Einzelheiten gehen ins Protokoll, nicht zum Inhaber.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppError {
    pub status: StatusCode,
    /// Kurzes, festes Wort für die Oberfläche: `nicht-gefunden`, `unlesbar`,
    /// `abgelehnt`, `zu-gross`, `zu-lange`, `zu-viele`, `konflikt`, `falsche-adresse`, `intern`, `speicher-fehlt`,
    /// `anmeldung`, `fremdes-konto`, `passkey`, `bestaetigung`; aus der Schleuse
    /// `fehlerform` für Antworten ohne eigenen Satz auch `methode`, `fehlerhaft`.
    pub art: &'static str,
    /// Satz für den Inhaber.
    pub fehler: String,
}

/// Der Satz einer Fehlerantwort, als Erweiterung an der Antwort (für `fehlerform`).
#[derive(Debug, Clone)]
pub struct FehlerSatz(pub String);

#[derive(Serialize)]
struct Koerper<'a> {
    fehler: &'a str,
    art: &'a str,
}

impl AppError {
    pub fn nicht_gefunden(was: impl Into<String>) -> Self {
        Self { status: StatusCode::NOT_FOUND, art: "nicht-gefunden", fehler: was.into() }
    }

    /// Eine Datei ist da, lässt sich aber nicht lesen (Kopf fehlerhaft o. ä.).
    pub fn unlesbar(was: impl Into<String>) -> Self {
        Self { status: StatusCode::UNPROCESSABLE_ENTITY, art: "unlesbar", fehler: was.into() }
    }

    /// Der Körper kommt ohne oder mit falschem `Content-Type` (415, Art `unlesbar`).
    pub fn falsches_format(warum: impl Into<String>) -> Self {
        Self { status: StatusCode::UNSUPPORTED_MEDIA_TYPE, art: "unlesbar", fehler: warum.into() }
    }

    /// 400, Art `fehlerhaft`: die Anfrage selbst ist falsch gebaut (etwa ein
    /// Prozentzeichen im Pfad unter `/api`, Schleuse `sicherheit::prozent_im_pfad`).
    pub fn fehlerhaft(warum: impl Into<String>) -> Self {
        Self { status: StatusCode::BAD_REQUEST, art: "fehlerhaft", fehler: warum.into() }
    }

    pub fn abgelehnt(warum: impl Into<String>) -> Self {
        Self { status: StatusCode::FORBIDDEN, art: "abgelehnt", fehler: warum.into() }
    }

    pub fn zu_gross(warum: impl Into<String>) -> Self {
        Self { status: StatusCode::PAYLOAD_TOO_LARGE, art: "zu-gross", fehler: warum.into() }
    }

    /// 409: passt nicht zum jetzigen Stand (etwa eine Frage, die schon anders
    /// beantwortet ist, oder inzwischen neu gestellt wurde).
    pub fn konflikt(warum: impl Into<String>) -> Self {
        Self { status: StatusCode::CONFLICT, art: "konflikt", fehler: warum.into() }
    }

    /// 429: eine Obergrenze ist erreicht (etwa offene Aufgaben je Projekt).
    pub fn zu_viele(warum: impl Into<String>) -> Self {
        Self { status: StatusCode::TOO_MANY_REQUESTS, art: "zu-viele", fehler: warum.into() }
    }

    pub fn zu_lange(warum: impl Into<String>) -> Self {
        Self { status: StatusCode::REQUEST_TIMEOUT, art: "zu-lange", fehler: warum.into() }
    }

    /// Kein (gültiger) Windows-Nachweis auf einem geschützten Weg (401). Die
    /// Kopfzeile `WWW-Authenticate: Negotiate` setzt der Aufrufer dazu.
    pub fn anmeldung_noetig() -> Self {
        Self { status: StatusCode::UNAUTHORIZED, art: "anmeldung", fehler: "Bitte mit dem Windows-Konto anmelden.".to_string() }
    }

    /// Etappe 7c: ein anderes Windows-Konto ohne gültigen Ausweis (401, Art
    /// `passkey`); die Oberfläche schickt zur Anmeldung mit Passkey.
    pub fn passkey_noetig() -> Self {
        Self { status: StatusCode::UNAUTHORIZED, art: "passkey", fehler: "Bitte mit dem Passkey anmelden (Seite /anmelden).".to_string() }
    }

    /// Etappe 7c: Freigeben, Ablehnen und Widerrufen mit Passkey brauchen eine
    /// frische Anmeldung (höchstens 5 Minuten alt); 403, Art `bestaetigung`.
    pub fn bestaetigung_noetig() -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            art: "bestaetigung",
            fehler: "Bitte zuerst noch einmal mit dem Passkey bestätigen (höchstens 5 Minuten alt).".to_string(),
        }
    }

    /// Angemeldet, aber nicht mit dem Konto des Inhabers (403).
    pub fn fremdes_konto() -> Self {
        Self { status: StatusCode::FORBIDDEN, art: "fremdes-konto", fehler: "Dieses Windows-Konto darf hier nicht.".to_string() }
    }

    /// Anfrage kam nicht über 127.0.0.1/localhost (DNS-Rebinding).
    pub fn falsche_adresse(warum: impl Into<String>) -> Self {
        Self { status: StatusCode::MISDIRECTED_REQUEST, art: "falsche-adresse", fehler: warum.into() }
    }

    pub fn intern(was: impl Into<String>) -> Self {
        Self { status: StatusCode::INTERNAL_SERVER_ERROR, art: "intern", fehler: was.into() }
    }

    /// Der Datenspeicher ließ sich beim Start nicht öffnen; der Rest läuft weiter.
    pub fn nicht_verfuegbar(warum: impl Into<String>) -> Self {
        Self { status: StatusCode::SERVICE_UNAVAILABLE, art: "speicher-fehlt", fehler: warum.into() }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // Kein Fehlersatz nennt das Profil oder den Benutzernamen (`datenschutz`),
        // auch wenn er einen Pfad aus einem Lesefehler mitbringt.
        let fehler = crate::datenschutz::verbergen(&self.fehler);
        let mut res = (self.status, Json(Koerper { fehler: &fehler, art: self.art })).into_response();
        res.extensions_mut().insert(FehlerSatz(fehler));
        res
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.fehler, self.art)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    #[tokio::test]
    async fn antwort_hat_status_und_feste_form() {
        let res = AppError::nicht_gefunden("Kein Projekt „x“").into_response();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        assert_eq!(res.extensions().get::<FehlerSatz>().unwrap().0, "Kein Projekt „x“");
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["art"], "nicht-gefunden");
        assert_eq!(v["fehler"], "Kein Projekt „x“");
        assert!(v.get("error").is_none());
    }

    /// Auftrag der Architektur: Ein Fehlersatz mit Pfad (etwa aus einem
    /// Lesefehler) nennt weder Profil noch Benutzernamen, im Körper wie im
    /// Satz für die Browser-Wege.
    #[tokio::test]
    async fn fehlersatz_ohne_profil() {
        let res = AppError::intern(r"C:\Users\beispiel\data\pc.toml: Zugriff verweigert").into_response();
        assert_eq!(res.extensions().get::<FehlerSatz>().unwrap().0, r"%USERPROFILE%\data\pc.toml: Zugriff verweigert");
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(v["fehler"], r"%USERPROFILE%\data\pc.toml: Zugriff verweigert");
    }

    #[test]
    fn alle_arten_haben_den_passenden_status() {
        assert_eq!(AppError::zu_lange("x").status, StatusCode::REQUEST_TIMEOUT);
        assert_eq!(AppError::zu_viele("x").status, StatusCode::TOO_MANY_REQUESTS);
        assert_eq!((AppError::konflikt("x").status, AppError::konflikt("x").art), (StatusCode::CONFLICT, "konflikt"));
        assert_eq!(AppError::falsche_adresse("x").status, StatusCode::MISDIRECTED_REQUEST);
        assert_eq!(AppError::intern("x").status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(AppError::zu_gross("x").art, "zu-gross");
        assert_eq!(AppError::falsches_format("x").status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
        assert_eq!(AppError::falsches_format("x").art, "unlesbar");
        assert_eq!(AppError::nicht_verfuegbar("x").status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(AppError::nicht_verfuegbar("x").art, "speicher-fehlt");
        assert_eq!(AppError::anmeldung_noetig().status, StatusCode::UNAUTHORIZED);
        assert_eq!(AppError::anmeldung_noetig().art, "anmeldung");
        assert_eq!(AppError::fremdes_konto().status, StatusCode::FORBIDDEN);
        assert_eq!(AppError::fremdes_konto().art, "fremdes-konto");
    }
}
