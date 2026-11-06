//! SDK für Backend-Module der Projekte (Etappe 4b, `[projekt_backend]`).
//!
//! Ein Projekt darf ein eigenes Rust-Modul haben: eine Kiste in
//! `projects/<slug>/server/`, die nur von dieser Kiste abhängt (nie von der
//! Kiste `dashboard`). Sie läuft im Dienst mit: eigene Endpunkte unter
//! `/api/projekte/<slug>/x/`, Hooks an festen Stellen und eigene Tabellen im
//! Datenspeicher.
//!
//! Was ein Modul bekommt, steht in [`ProjektKontext`], und nur das. Der
//! Dienst füllt den Kontext über den Trait [`Anbindung`]; in Tests füllt ihn
//! der Prüfstand im Arbeitsspeicher (`pruefstand`, nur mit dem Feature
//! `pruefstand` für Tests; `ProjektKontext::neu` nur mit `dienst`).
//!
//! Teile: `fehler` (Fehlerform `{ fehler, art }`), `kontext` (Kontext,
//! Tabellen, Git-Stand), `tabellen` (Tabellen mit Fassung und Umbau), `hooks`,
//! `rahmen` (Router mit Absturzfänger, Körpergrenze, Fehlerform), `vertrag`
//! (TypeScript-Typen nach `web/src/lib/vertrag/projekte/<slug>/`) und
//! `pruefstand` (Test-Rahmen).

pub mod fehler;
pub mod hooks;
pub mod kontext;
#[cfg(any(test, feature = "pruefstand"))]
pub mod pruefstand;
pub mod rahmen;
pub mod tabellen;
pub mod vertrag;

pub use fehler::Fehler;
pub use hooks::{BauErgebnis, Hooks, VorgangInfo, Zeitplan};
pub use kontext::{Anbindung, GitStand, ProjektKontext, Tabelle};
pub use tabellen::TabellenDef;
pub use vertrag::Vertrag;

/// Woher ein Modul stammt, zur Bauzeit in der Modul-Kiste festgehalten
/// (mit [`herkunft!`]): Kistenname und die letzten Ordner der Kiste. Das
/// Register prüft damit, dass `slug()` zu `projects/<slug>/server` und zur
/// Kiste `projekt-<slug>` passt, ohne zur Laufzeit Dateien zu lesen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Herkunft {
    kiste: &'static str,
    ordner: &'static str,
}

impl Herkunft {
    /// Nur für [`herkunft!`].
    #[doc(hidden)]
    pub const fn __aus_bau(kiste: &'static str, ordner: &'static str) -> Self {
        Self { kiste, ordner }
    }

    /// Name der Kiste aus Cargo.toml (`projekt-<slug>`).
    pub fn kiste(&self) -> &'static str {
        self.kiste
    }

    /// Die letzten drei Ordner der Kiste, mit `/` getrennt (`projects/<slug>/server`).
    pub fn ordner(&self) -> String {
        let teile: Vec<&str> = self.ordner.split(['/', '\\']).filter(|t| !t.is_empty()).collect();
        teile[teile.len().saturating_sub(3)..].join("/")
    }
}

/// Die Herkunft der Kiste, in der das Makro steht (zur Bauzeit):
/// `fn herkunft(&self) -> Herkunft { dashboard_projekt::herkunft!() }`.
#[macro_export]
macro_rules! herkunft {
    () => {
        $crate::Herkunft::__aus_bau(env!("CARGO_PKG_NAME"), env!("CARGO_MANIFEST_DIR"))
    };
}

/// Was ein Modul für Router und Antworten braucht, ohne eigene Abhängigkeit.
pub use axum;
pub use chrono;

/// Ein Backend-Modul eines Projekts.
pub trait Modul: Send + Sync + 'static {
    /// Der Slug des Projekts (Ordnername unter `projects/`).
    fn slug(&self) -> &'static str;

    /// Die Endpunkte. Der Dienst hängt sie unter `/api/projekte/<slug>/x/` ein;
    /// das Präfix setzt das Register, hier stehen nur Wege wie `/stufen`.
    /// Seit Etappe B2 sind alle Wege geschützt (Windows-Anmeldung, ohne sie
    /// 401), auch GET; frei lesbar ist nur, was [`Modul::freie_wege`]
    /// ausdrücklich nennt. Schreibende Wege dazu nur aus dem Dashboard selbst
    /// (Herkunftsprüfung), Körper höchstens 64 KiB.
    fn router(&self) -> axum::Router<ProjektKontext>;

    /// Etappe B2: Wege (wie in [`Modul::router`], etwa `/stufen` oder
    /// `/stufen/{id}`), die GET ohne Anmeldung lesen darf. Standard: keiner.
    /// Ein freier Weg gibt nie persönlichen Text heraus (Fragen, Titel,
    /// Notizen, Befunde); jeder freie Weg wird von der Architektur abgenommen.
    fn freie_wege(&self) -> &'static [&'static str] {
        &[]
    }

    /// Woher das Modul stammt; immer `dashboard_projekt::herkunft!()`.
    fn herkunft(&self) -> Herkunft;

    /// Eigene Tabellen im Datenspeicher (`projekt/<slug>/<name>`).
    fn tabellen(&self) -> Vec<TabellenDef> {
        Vec::new()
    }

    /// Hooks an festen Stellen des Dienstes.
    fn hooks(&self) -> Hooks {
        Hooks::default()
    }

    /// Die Antworttypen für TypeScript (`v.typ::<MeineAntwort>()`).
    fn vertrag(&self, _v: &mut Vertrag) {}
}

#[cfg(test)]
mod tests {
    use axum::extract::State;
    use axum::http::StatusCode;
    use axum::routing::{get, post};
    use axum::{Json, Router};
    use serde_json::{Value, json};

    use super::*;
    use crate::pruefstand::Pruefstand;

    struct Probe;

    fn panik_hook(_: &ProjektKontext) -> Result<(), Fehler> {
        panic!("Absicht im Hook")
    }

    fn zaehlen(k: &ProjektKontext) -> Result<(), Fehler> {
        let t = k.tabelle("zaehler")?;
        let n: u64 = t.lesen("n")?.unwrap_or(0);
        t.schreiben("n", &(n + 1))
    }

    impl Modul for Probe {
        fn herkunft(&self) -> crate::Herkunft {
            crate::herkunft!()
        }
        fn slug(&self) -> &'static str {
            "probe"
        }
        fn router(&self) -> Router<ProjektKontext> {
            Router::new()
                .route("/gut", get(|State(k): State<ProjektKontext>| async move { Json(json!({ "slug": k.slug() })) }))
                .route(
                    "/panik",
                    get(|| async {
                        if true {
                            panic!("Absicht im Endpunkt");
                        }
                        "nie"
                    }),
                )
                .route("/echo", post(|Json(v): Json<Value>| async move { Json(v) }))
                .route(
                    "/daten",
                    get(|State(k): State<ProjektKontext>| async move {
                        k.im_hintergrund(|k| k.daten("fortschritt")?.ok_or_else(|| Fehler::nicht_gefunden("fehlt"))).await.map(Json)
                    }),
                )
        }
        fn tabellen(&self) -> Vec<TabellenDef> {
            vec![TabellenDef::neu("zaehler", "Zählt Läufe")]
        }
        fn hooks(&self) -> Hooks {
            Hooks { zeitplan: Some(Zeitplan { alle: std::time::Duration::from_secs(1), lauf: zaehlen }), taeglich: Some(panik_hook), ..Hooks::default() }
        }
    }

    #[tokio::test]
    async fn rahmen_faengt_absturz_und_hat_fehlerform() {
        let p = Pruefstand::neu(&Probe).datei("fortschritt", "[[zeile]]\ntitel = \"A\"\nprozent = 50\n");
        assert_eq!(p.get("/gut").await, (StatusCode::OK, json!({ "slug": "probe" })));
        let (status, v) = p.get("/panik").await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(v["art"], "intern");
        assert!(v["fehler"].as_str().unwrap().contains("abgestürzt"), "{v}");
        assert!(p.befunde().iter().any(|(wo, s)| wo == "endpunkt" && s.contains("Absicht im Endpunkt")));
        // Danach läuft alles weiter.
        assert_eq!(p.get("/gut").await.0, StatusCode::OK);
        let (status, v) = p.get("/gibt-es-nicht").await;
        assert_eq!((status, v["art"].as_str()), (StatusCode::NOT_FOUND, Some("nicht-gefunden")));
        let (status, v) = p.anfrage(axum::http::Method::DELETE, "/gut", None).await;
        assert_eq!((status, v["art"].as_str()), (StatusCode::METHOD_NOT_ALLOWED, Some("methode")));
        let gross = Value::String("x".repeat(rahmen::KOERPER_HOECHSTENS + 1));
        let (status, v) = p.post("/echo", gross).await;
        assert_eq!((status, v["art"].as_str()), (StatusCode::PAYLOAD_TOO_LARGE, Some("zu-gross")));
        assert_eq!(p.post("/echo", json!({ "a": 1 })).await, (StatusCode::OK, json!({ "a": 1 })));
        let (status, v) = p.get("/daten").await;
        assert_eq!((status, &v["zeile"][0]["prozent"]), (StatusCode::OK, &json!(50)));
    }

    #[test]
    fn hooks_und_tabellen() {
        let p = Pruefstand::neu(&Probe);
        p.zeitplan().unwrap();
        p.zeitplan().unwrap();
        assert_eq!(p.kontext().tabelle("zaehler").unwrap().lesen::<u64>("n").unwrap(), Some(2));
        assert!(p.kontext().tabelle("fremd").is_err(), "nur Tabellen aus tabellen()");
        let e = p.taeglich().unwrap_err();
        assert!(e.fehler.contains("Absicht im Hook"), "{e}");
        assert!(p.befunde().iter().any(|(wo, _)| wo == "taeglich"));
        assert!(p.nach_bau(BauErgebnis::Gruen).is_ok(), "ohne Hook: nichts zu tun");
        assert_eq!(Probe.hooks().zeitplan_abstand(), Some(hooks::ZEITPLAN_MINDESTENS));
    }

    #[test]
    fn umbau_beim_einrichten_und_zu_neu() {
        fn umbau(von: u32, mut w: Value) -> Result<Value, String> {
            if von == 1 {
                w["neu"] = json!(true);
            }
            Ok(w)
        }
        let a = pruefstand::TestAnbindung::default();
        a.einrichten("p", &[TabellenDef::neu("t", "x")]).unwrap();
        a.eintrag_schreiben("p", "t", 1, "k", br#"{"alt":1}"#).unwrap();
        a.einrichten("p", &[TabellenDef::neu("t", "x").mit_fassung(2, umbau)]).unwrap();
        let w: Value = serde_json::from_slice(&a.eintrag_lesen("p", "t", 2, "k").unwrap().unwrap()).unwrap();
        assert_eq!(w, json!({ "alt": 1, "neu": true }));
        let e = a.einrichten("p", &[TabellenDef::neu("t", "x")]).unwrap_err();
        assert!(e.contains("Fassung 2"), "{e}");
        // Abgelehnt: für ein Modul mit Fassung 1 bleibt die Tabelle gesperrt.
        assert!(a.eintrag_lesen("p", "t", 1, "k").is_err() && a.anzahl("p", "t", 1).is_err());
        assert!(a.eintrag_schreiben("p", "t", 1, "k", b"1").is_err());
    }
}
