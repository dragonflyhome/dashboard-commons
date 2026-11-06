//! Die Fehlerform aller Wege eines Moduls, wie im ganzen Dashboard:
//! `{ "fehler": "…ein Satz in einfachem Deutsch…", "art": "nicht-gefunden" }`.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use ts_rs::TS;

/// Ein Fehler mit Satz für den Inhaber. Der Status geht nicht ins JSON.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ModulFehler")]
pub struct Fehler {
    #[serde(skip)]
    #[ts(skip)]
    pub status: u16,
    /// Satz für den Inhaber.
    pub fehler: String,
    /// Kurzes, festes Wort: `nicht-gefunden`, `unlesbar`, `vertrag`,
    /// `abgelehnt`, `zu-gross`, `zu-lange`, `intern`, `speicher-fehlt`.
    pub art: String,
}

impl Fehler {
    pub fn neu(status: StatusCode, art: &str, satz: impl Into<String>) -> Self {
        Self { status: status.as_u16(), fehler: satz.into(), art: art.to_string() }
    }

    /// 404: gibt es nicht.
    pub fn nicht_gefunden(satz: impl Into<String>) -> Self {
        Self::neu(StatusCode::NOT_FOUND, "nicht-gefunden", satz)
    }

    /// 422: eine Eingabe oder Datei passt nicht.
    pub fn unlesbar(satz: impl Into<String>) -> Self {
        Self::neu(StatusCode::UNPROCESSABLE_ENTITY, "unlesbar", satz)
    }

    /// 422: eine Datei bricht den Vertrag (Satz mit Datei und Zeile).
    pub fn vertrag(satz: impl Into<String>) -> Self {
        Self::neu(StatusCode::UNPROCESSABLE_ENTITY, "vertrag", satz)
    }

    /// 403: nicht erlaubt.
    pub fn abgelehnt(satz: impl Into<String>) -> Self {
        Self::neu(StatusCode::FORBIDDEN, "abgelehnt", satz)
    }

    /// 413: zu viele Daten.
    pub fn zu_gross(satz: impl Into<String>) -> Self {
        Self::neu(StatusCode::PAYLOAD_TOO_LARGE, "zu-gross", satz)
    }

    /// 408 bzw. Zeitlimit eines Hooks.
    pub fn zu_lange(satz: impl Into<String>) -> Self {
        Self::neu(StatusCode::REQUEST_TIMEOUT, "zu-lange", satz)
    }

    /// 500: im Modul ist etwas schiefgegangen.
    pub fn intern(satz: impl Into<String>) -> Self {
        Self::neu(StatusCode::INTERNAL_SERVER_ERROR, "intern", satz)
    }

    /// 503: der Datenspeicher ist nicht offen.
    pub fn speicher_fehlt(satz: impl Into<String>) -> Self {
        Self::neu(StatusCode::SERVICE_UNAVAILABLE, "speicher-fehlt", satz)
    }

    /// Der HTTP-Status (500, wenn die Zahl ungültig ist).
    pub fn status(&self) -> StatusCode {
        StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
    }
}

impl IntoResponse for Fehler {
    fn into_response(self) -> Response {
        (self.status(), Json(self)).into_response()
    }
}

impl std::fmt::Display for Fehler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.fehler, self.art)
    }
}

impl std::error::Error for Fehler {}

/// Grund eines Absturzes als Text (für Protokoll und Prüfung).
pub fn absturz_grund(e: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = e.downcast_ref::<String>() {
        s.clone()
    } else if let Some(s) = e.downcast_ref::<&str>() {
        (*s).to_string()
    } else {
        "unbekannt".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_und_status() {
        let f = Fehler::nicht_gefunden("Gibt es nicht.");
        assert_eq!(f.status(), StatusCode::NOT_FOUND);
        let json = serde_json::to_value(&f).unwrap();
        assert_eq!(json, serde_json::json!({ "fehler": "Gibt es nicht.", "art": "nicht-gefunden" }));
        assert_eq!(Fehler::intern("x").status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(Fehler::vertrag("x").art, "vertrag");
        assert_eq!(Fehler { status: 1, fehler: "x".into(), art: "y".into() }.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
