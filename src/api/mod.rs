//! JSON-API für die Oberfläche. Nur lesend, bis auf den Schlüssel-POST
//! (`/schluessel/<name>`, mit Herkunftsprüfung), die Sicherung auf Knopfdruck
//! die Vorgänge (`/api/vorgaenge…`), die Aufgaben für den Inhaber (`/api/aufgaben…`)
//! und die Arbeiten der Chats (`/api/arbeiten…`), alle auch von Chats per curl,
//! die Einstufung des Inhabers (`/api/einstufung`) und seine Antworten auf Fragen
//! (`/api/antworten…`, beides nur aus dem Browser) und die Meldungen
//! (`/api/meldungen…`: Chats melden per curl, „gelesen“ nur aus dem Browser). Die
//! System-Seiten lesen `docs/entscheidungen`, `docs/zielbild` (Etappe 4d) und
//! `docs/anleitung` (Etappe 6b). Keine Route gibt
//! Schlüsselwerte zurück. Alle JSON-Typen stehen in `src/vertrag/` (hier als
//! `vertrag` erreichbar); `cargo test` erzeugt daraus die TypeScript-Typen in
//! `web/src/lib/vertrag/`, die Oberfläche importiert sie als `$vertrag`.

pub mod anmeldung;
pub mod anstehend;
pub mod antworten;
pub mod arbeiten;
pub mod aufgaben;
pub mod bauen;
pub mod daten;
pub mod events;
pub mod frei;
pub mod meldungen;
pub mod neue_form;
pub mod oeffnen;
pub mod passkey;
pub mod pc;
pub mod projekte;
pub mod pruefung;
pub mod schluessel;
pub mod sitzungen;
pub mod stand;
pub mod system_seiten;
pub mod vertrag;
pub mod vorgaenge;

use axum::Router;
use axum::routing::{get, post};

use crate::app::AppState;

/// Alle Routen der API und des Schlüssel-Formulars.
pub fn routes() -> Router<AppState> {
    #[allow(unused_mut)]
    let mut r = Router::new()
        .route("/api/projects", get(projekte::liste))
        .route("/api/projects/{slug}", get(projekte::eine))
        // Entscheidung archiv-status: die Archivansicht /projekte/archiviert.
        .route("/api/archiv", get(projekte::archiv))
        .route("/api/projekte/{slug}", get(neue_form::kopf))
        .route("/api/projekte/{slug}/daten/{name}", get(neue_form::daten))
        // Reiter „Hallo“: welches Projekt den Begrüßungstext trägt (kein Slug im Code).
        .route("/api/hallo", get(neue_form::hallo))
        .route("/projects/{slug}/assets/{file}", get(projekte::asset))
        // Etappe 4c: Bau-Stand mit Satz (nur der Inhaber, `sicherheit::geschuetzt`).
        .route("/api/bauen", get(bauen::liste))
        // Entscheidung lokal-oeffnen: nur POST, nur der Inhaber im eigenen Windows-Konto.
        .route("/api/oeffnen", post(oeffnen::oeffnen))
        .route("/api/pruefung", get(pruefung::alle))
        .route("/api/pruefung/{slug}", get(pruefung::eines))
        // Etappe 4d: System-Seiten /entscheidungen und /zielbild, aus docs/ (nur lesend).
        .route("/api/entscheidungen", get(system_seiten::entscheidungen))
        .route("/api/zielbild", get(system_seiten::zielbild))
        // Etappe 6b: /anleitung aus docs/anleitung im Code-Repo, geschützt wie /api/zielbild.
        .route("/api/anleitung", get(system_seiten::anleitung))
        .route("/api/pc", get(pc::bericht))
        .route("/api/pc/benutzer", get(pc::benutzer))
        .route("/api/pc/verlauf", get(pc::verlauf))
        // Hotfix 3a-1: geschützt, für die Oberfläche (immer mit Starts, Meldungen, Vorgängen).
        .route("/api/pc/verlauf/voll", get(pc::verlauf_voll))
        .route("/api/schluessel", get(schluessel::liste))
        .route("/schluessel/{name}", post(schluessel::speichern))
        .route("/api/daten", get(daten::bericht))
        .route("/api/daten/pc-verlauf", get(daten::pc_verlauf))
        .route("/api/daten/export", get(daten::export))
        .route("/api/daten/sichern", post(daten::sichern))
        .route("/api/vorgaenge", get(vorgaenge::liste).post(vorgaenge::anlegen))
        .route("/api/vorgaenge/uebersicht", get(vorgaenge::uebersicht))
        .route("/api/vorgaenge/warten", get(vorgaenge::warten))
        .route("/api/vorgaenge/{id}", get(vorgaenge::einer))
        .route("/api/vorgaenge/{id}/uebernommen", post(vorgaenge::uebernommen))
        .route("/api/vorgaenge/{id}/erledigt", post(vorgaenge::erledigt))
        .route("/api/aufgaben", get(aufgaben::liste).post(aufgaben::anlegen))
        .route("/api/aufgaben/anzahl", get(aufgaben::anzahl))
        .route("/api/aufgaben/{id}/zurueckgezogen", post(aufgaben::zurueckgezogen))
        // Etappe B2: Titel, Knöpfe oder Frist ändern, ohne neue Nummer (nur der Chat des Projekts).
        .route("/api/aufgaben/{id}", post(aufgaben::aendern))
        .route("/api/arbeiten", get(arbeiten::liste).post(arbeiten::anlegen))
        .route("/api/arbeiten/{id}", post(arbeiten::fortschreiben))
        // Zweig teilschritte (T3): Teilschritte mit Besitzer, nur für Chats (aus dem Browser 422).
        .route("/api/arbeiten/{id}/schritte", post(arbeiten::schritt_anlegen))
        .route("/api/arbeiten/{id}/schritte/{nr}", post(arbeiten::schritt_aendern))
        .route("/api/arbeiten/{id}/schritte/{nr}/uebernehmen", post(arbeiten::schritt_uebernehmen))
        // Herzschlag: offene Arbeit des Chats einer Sitzung (Stop-Hook `dashboard herzschlag`).
        .route("/api/sitzungen/{id}/offen", get(sitzungen::offen))
        .route("/api/einstufung", post(anstehend::einstufen))
        .route("/api/antworten", get(antworten::liste))
        .route("/api/meldungen", get(meldungen::liste).post(meldungen::anlegen))
        .route("/api/meldungen/{id}/gelesen", post(meldungen::gelesen))
        .route("/api/anstehend", get(anstehend::liste))
        .route("/api/projekte/{slug}/stand", get(anstehend::stand))
        .route("/api/projekte/{slug}/stand/voll", get(anstehend::stand_voll))
        // Etappe B2: das Backlog eines Projekts (geschützt wie /stand/voll).
        .route("/api/projekte/{slug}/backlog", get(anstehend::backlog))
        // Etappe B3: die Fristen aller sichtbaren Projekte (geschützt; die eines Projekts
        // stehen in `abgeleitet.fristen` von /api/projekte/<slug>/daten/fristen).
        .route("/api/fristen", get(anstehend::fristen))
        .route("/api/events", get(events::sse))
        // Kopfleiste, Fassung 2: der freie Kanal (Verbindung, Anmeldestand, schon freie Daten).
        .route("/api/events/frei", get(events::sse_frei))
        // Kopfleiste, Fassung 3: der gemeinsame Stand für die Aktualitätsprüfung (frei und geschützt).
        .route("/api/stand", get(stand::frei))
        .route("/api/stand/voll", get(stand::voll))
        .route("/api/anmeldung/probe", get(anmeldung::probe))
        .route("/api/anmeldung/wer", get(anmeldung::wer))
        // Etappe 7c: Passkey für andere Konten (frei, mit Windows-Anmeldung, nur als Inhaber).
        .route("/api/passkey/stand", get(passkey::stand))
        .route("/api/passkey/anmelden/beginn", post(passkey::anmelden_beginn))
        .route("/api/passkey/anmelden/ende", post(passkey::anmelden_ende))
        .route("/api/passkey/abmelden", post(passkey::abmelden))
        .route("/api/passkey/antrag", get(passkey::antrag_liste))
        .route("/api/passkey/antrag/beginn", post(passkey::antrag_beginn))
        .route("/api/passkey/antrag/ende", post(passkey::antrag_ende))
        .route("/api/schluessel/passkeys", get(passkey::liste))
        .route("/api/schluessel/passkeys/anzahl", get(passkey::anzahl))
        .route("/api/schluessel/passkeys/{id}/freigeben", post(passkey::freigeben))
        .route("/api/schluessel/passkeys/{id}/ablehnen", post(passkey::ablehnen))
        .route("/api/schluessel/passkeys/{id}/widerrufen", post(passkey::widerrufen))
        .route("/api/schluessel/sitzungen-beenden", post(passkey::sitzungen_beenden))
        .route("/healthz", get(|| async { "ok" }));
    #[cfg(test)]
    {
        r = r.merge(test_routes());
    }
    r
}

/// Die Wege, auf denen der Inhaber antwortet (Frage und Aufgabe). Sie stehen
/// für sich, weil sie bei „Etwas anderes …“ bis
/// `sicherheit::ANTWORT_KOERPER_HOECHSTENS` nehmen statt 16 KB
/// (`app::router` setzt die Grenze; `sicherheit::antwort_weg` kennt dieselben Pfade).
pub fn antwort_routes() -> Router<AppState> {
    Router::new()
        .route("/api/aufgaben/{id}/erledigt", post(aufgaben::erledigt))
        .route("/api/antworten/{slug}/{id}", get(antworten::eine).post(antworten::beantworten))
}

/// Nur in Tests: eine langsame Route (für das Zeitlimit) und eine, die abstürzt
/// (für den Absturzfänger).
#[cfg(test)]
pub fn test_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/_test/langsam",
            get(|| async {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                "ok"
            }),
        )
        .route(
            "/api/_test/panik",
            get(|| async {
                // `if true`, damit der Rückgabetyp `&str` bleibt (ohne Warnung).
                if true {
                    panic!("Absichtlicher Absturz für den Test");
                }
                "nie"
            }),
        )
}
