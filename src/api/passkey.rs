//! Passkey für andere Konten (Etappe 7c): Anmelden mit Passkey, Antrag aus
//! einem anderen Windows-Konto, Freigabe, Ablehnung, Widerruf.
//!
//! Frei (die Seite `/anmelden` muss jeder erreichen):
//! - `GET  /api/passkey/stand`: ist diese Sitzung angemeldet, und wie?
//! - `POST /api/passkey/anmelden/beginn` und `…/ende`: Anmeldung mit einem
//!   registrierten Passkey; danach der Ausweis im Cookie `__Host-dashboard`.
//! - `POST /api/passkey/abmelden`: Ausweis sperren und Cookie löschen.
//!
//! Mit Windows-Anmeldung, aber ohne die Inhaber-Pflicht (`konto_noetig`): Der
//! Antrag trägt so den echten Kontonamen, nicht einen eingetippten.
//! - `GET  /api/passkey/antrag`: die Passkeys des eigenen Kontos.
//! - `POST /api/passkey/antrag/beginn` und `…/ende`: Passkey anlegen. Im
//!   Hauptkonto sofort registriert, sonst ein Antrag (ausstehend).
//!
//! Nur als Inhaber (unter `/api/schluessel`, geschützt wie in 7b):
//! - `GET  /api/schluessel/passkeys` und `…/anzahl`
//! - `POST /api/schluessel/passkeys/<id>/freigeben|ablehnen|widerrufen`: im
//!   Hauptkonto, oder mit Passkey höchstens 5 Minuten nach der Anmeldung
//! - `POST /api/schluessel/sitzungen-beenden`: alle Ausweise ungültig
//!
//! Alle Antworten des Authenticators kommen als Base64url (ohne Füllzeichen)
//! im JSON. Ins Protokoll kommen nur kurze technische Gründe, nie Schlüssel,
//! Signaturen oder Ausweise.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use serde::de::DeserializeOwned;

use super::vorgaenge::json_kopf_pruefen;
use crate::anmeldung::{Inhaber, PasskeySitzung, WindowsKonto};
use crate::app::AppState;
use crate::fehler::AppError;
use crate::passkey::jwt::{Ausweis, JwtSchluessel};
use crate::passkey::webauthn::{self, Erwartung};
use crate::passkey::{self, Abweisung, ERNEUERN_NACH_SEKUNDEN, FRISCH_SEKUNDEN, RP_ID, RP_NAME, cookie};
use crate::speicher::Speicher;
use crate::speicher::aufgaben::{self, Zeit};
use crate::speicher::passkeys::{AufgabenArt, Entscheidung, Freigeber, PasskeyAufgabe, PasskeyFehler};
use crate::vertrag::api::Ereignis;
use crate::vertrag::passkey::{
    AnmeldeArt, AnmeldeOptionen, AnmeldenEnde, Anmeldewort, AntragBeginn, AntragEnde, AntragErgebnis, AntragOptionen, EigeneAntraege, Freigabe, PasskeyAnzahl,
    PasskeyListe, PasskeyStand, PasskeyZeile,
};

/// Zeitlimit für den Dialog im Browser (wie die Challenge: 5 Minuten).
const ZEITLIMIT_MS: u32 = 300_000;
/// Längster Name eines Geräts.
const NAME_HOECHSTENS: usize = 60;

/// Wege, die eine Windows-Anmeldung mit irgendeinem Konto brauchen (die
/// Anträge), Groß/Klein egal wie bei `sicherheit::geschuetzt`.
pub fn konto_noetig(pfad: &str) -> bool {
    let pfad = pfad.to_ascii_lowercase();
    pfad.strip_prefix("/api/passkey/antrag").is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

fn jetzt() -> Zeit {
    aufgaben::jetzt()
}

fn zeit_aus(sekunden: i64, wie: &Zeit) -> Option<String> {
    chrono::DateTime::from_timestamp(sekunden, 0).map(|z| z.with_timezone(&wie.timezone()).to_rfc3339_opts(chrono::SecondsFormat::Secs, false))
}

/// Der Ausweis einer Anfrage, wie ihn die Schleuse sieht.
// Kurzlebiger Rückgabewert je Anfrage; eine Box brächte nur Umbau an allen Stellen, die ihn bauen und lesen.
#[allow(clippy::large_enum_variant)]
pub enum AusweisStand {
    /// Kein Cookie.
    Keiner,
    /// Gültig; `erneuert`: ein neues Cookie, wenn der Ausweis älter als eine Minute war.
    Gueltig { sitzung: PasskeySitzung, erneuert: Option<HeaderValue> },
    /// Cookie da, aber ungültig (abgelaufen, gesperrt, widerrufen, falsch): wird gelöscht.
    Ungueltig,
}

/// Prüft das Cookie `__Host-dashboard`: Signatur und Inhalt
/// (`JwtSchluessel::pruefen`), dann im Datenspeicher Sperrliste und ob der
/// Passkey noch registriert ist.
pub async fn ausweis_aus(state: &AppState, kopf: &HeaderMap) -> AusweisStand {
    let Some(token) = cookie::lesen(kopf) else {
        return if kopf.get_all(header::COOKIE).iter().any(|c| c.to_str().is_ok_and(|c| c.contains(cookie::NAME))) {
            AusweisStand::Ungueltig
        } else {
            AusweisStand::Keiner
        };
    };
    let (Some(jwt), Some(speicher)) = (state.jwt.clone(), state.speicher.clone()) else {
        return AusweisStand::Ungueltig;
    };
    let jetzt = jetzt();
    let ausweis = match jwt.pruefen(&token, jetzt.timestamp()) {
        Ok(a) => a,
        Err(e) => {
            tracing::info!("Ausweis abgewiesen: {e}");
            return AusweisStand::Ungueltig;
        }
    };
    let (jti, cid, anmeldung) = (ausweis.jti.clone(), ausweis.cid.clone(), ausweis.auth_time);
    let passkey = match Speicher::im_hintergrund(speicher, move |s| s.sitzung_gilt(&jti, &cid, anmeldung)).await {
        Ok(Some(p)) => p,
        Ok(None) => {
            tracing::info!("Ausweis abgewiesen: gesperrt oder Passkey nicht mehr registriert");
            return AusweisStand::Ungueltig;
        }
        Err(e) => {
            tracing::error!("Ausweis: Datenspeicher: {e} ({})", e.einzelheiten());
            return AusweisStand::Ungueltig;
        }
    };
    let jetzt = jetzt.timestamp();
    let erneuert = (jetzt - ausweis.iat >= ERNEUERN_NACH_SEKUNDEN && ausweis.exp < ausweis.sitzung_ende())
        .then(|| ausweis.erneuert(jetzt))
        .and_then(|neu| cookie::setzen(&jwt.ausstellen(&neu), neu.exp - jetzt));
    AusweisStand::Gueltig { sitzung: PasskeySitzung { ausweis, name: passkey.name }, erneuert }
}

/// Fehler der Passkey-Schritte als Antwort; Einzelheiten nur ins Protokoll.
fn fehler(e: PasskeyFehler) -> AppError {
    match e {
        PasskeyFehler::Abgewiesen(a) => {
            tracing::warn!("Passkey abgewiesen: {a}");
            match a {
                Abweisung::ChallengeAbgelaufen | Abweisung::ChallengeUnbekannt => {
                    AppError::abgelehnt("Das hat zu lange gedauert oder war schon einmal da. Bitte noch einmal von vorn.")
                }
                Abweisung::Unbekannt | Abweisung::NichtRegistriert => {
                    AppError::abgelehnt("Diesen Passkey kennt das Dashboard nicht, oder er ist (noch) nicht freigegeben.")
                }
                Abweisung::GibtEsSchon => AppError::konflikt("Diesen Passkey gibt es schon."),
                Abweisung::Verfahren | Abweisung::Attestation => {
                    AppError::abgelehnt("Dieser Passkey wird nicht unterstützt (nur ES256 oder RS256, ohne Bescheinigung).")
                }
                _ => AppError::abgelehnt("Der Passkey wurde nicht angenommen. Bitte noch einmal versuchen."),
            }
        }
        PasskeyFehler::Grenze(satz) => AppError::zu_viele(satz),
        PasskeyFehler::NichtGefunden => AppError::nicht_gefunden("Diesen Passkey gibt es nicht."),
        PasskeyFehler::NichtFrisch => AppError::bestaetigung_noetig(),
        PasskeyFehler::Selbst => AppError::abgelehnt("Ein Passkey kann nicht über sich selbst entscheiden."),
        PasskeyFehler::Vergleichszahl => {
            AppError::konflikt("Die Vergleichszahl passt nicht zu diesem Antrag. Bitte die Zahl auf beiden Bildschirmen vergleichen.")
        }
        PasskeyFehler::Zustand(satz) => AppError::konflikt(satz),
        PasskeyFehler::Speicher(e) => {
            tracing::error!("Passkey: Datenspeicher: {e} ({})", e.einzelheiten());
            AppError::intern("Der Datenspeicher lässt sich gerade nicht lesen oder schreiben; Einzelheiten stehen im Protokoll.")
        }
    }
}

/// Wie `Speicher::im_hintergrund`, mit `PasskeyFehler`.
async fn hintergrund<T, F>(state: &AppState, f: F) -> Result<T, AppError>
where
    T: Send + 'static,
    F: FnOnce(&Speicher) -> Result<T, PasskeyFehler> + Send + 'static,
{
    let speicher = state.speicher()?;
    let ergebnis = tokio::task::spawn_blocking(move || f(&speicher))
        .await
        .map_err(|e| fehler(PasskeyFehler::Speicher(crate::speicher::SpeicherFehler::Hintergrund(e.to_string()))))?;
    ergebnis.map_err(fehler)
}

fn json_lesen<T: DeserializeOwned>(kopf: &HeaderMap, koerper: &[u8]) -> Result<T, AppError> {
    json_kopf_pruefen(kopf)?;
    serde_json::from_slice(koerper).map_err(|e| {
        tracing::debug!("Passkey: Körper unlesbar: {e}");
        AppError::unlesbar("Der Inhalt ist kein passendes JSON.")
    })
}

fn b64_feld(wert: &str, feld: &str) -> Result<Vec<u8>, AppError> {
    passkey::b64_lesen(wert).ok_or_else(|| AppError::unlesbar(format!("„{feld}“ ist kein Base64url.")))
}

fn schluessel(state: &AppState) -> Result<Arc<JwtSchluessel>, AppError> {
    state.jwt.clone().ok_or_else(|| {
        AppError::nicht_verfuegbar(
            "Passkeys sind noch nicht eingeschaltet: Ein einmaliger Schritt als Administrator fehlt. Er steht als Aufgabe unter „Für dich“ auf der Startseite.",
        )
    })
}

/// Live-Kanal und Warter für die Ereignisse der Vorgänge.
fn melden(state: &AppState, ereignisse: &[Ereignis]) {
    for e in ereignisse {
        super::vorgaenge::melden(state, e);
    }
}

fn herkunft(state: &AppState) -> String {
    passkey::herkunft(state.port)
}

/// Wie die Anfrage als Inhaber gilt (für Stand und Liste).
fn wie(inhaber: Option<&Inhaber>) -> AnmeldeArt {
    match inhaber {
        Some(Inhaber::Hauptkonto) => AnmeldeArt::Hauptkonto,
        Some(Inhaber::Passkey(_)) => AnmeldeArt::Passkey,
        None => AnmeldeArt::Keine,
    }
}

// ----- frei -----

/// `GET /api/passkey/stand`.
///
/// Frei: Die Windows-Anmeldung sieht er nur auf einer Verbindung, die schon
/// angemeldet ist (Edge meldet erst nach einer 401 an); sonst `wie: keine`,
/// auch im Hauptkonto. Die Seite `/anmelden` fragt darum zuerst den
/// geschützten `/api/passkey/antrag` (dessen `hauptkonto` ist sicher) und
/// erst danach diesen Stand (Auflage aus Hotfix 3a-1, behoben in 3a+).
pub async fn stand(State(state): State<AppState>, inhaber: Option<Extension<Inhaber>>) -> Json<PasskeyStand> {
    let inhaber = inhaber.map(|Extension(l)| l);
    let jetzt = jetzt();
    let (passkey, sitzung_bis, frisch_bis) = match &inhaber {
        Some(Inhaber::Passkey(s)) => {
            (Some(s.name.clone()), zeit_aus(s.ausweis.sitzung_ende(), &jetzt), zeit_aus(s.ausweis.auth_time + FRISCH_SEKUNDEN, &jetzt))
        }
        _ => (None, None, None),
    };
    Json(PasskeyStand { wie: wie(inhaber.as_ref()), passkey, sitzung_bis, frisch_bis, verfuegbar: state.jwt.is_some() })
}

/// `POST /api/passkey/anmelden/beginn`: eine Challenge zum Anmelden.
pub async fn anmelden_beginn(State(state): State<AppState>) -> Result<Json<AnmeldeOptionen>, AppError> {
    schluessel(&state)?;
    let jetzt = jetzt();
    let challenge = hintergrund(&state, move |s| s.passkey_aufgabe_anlegen(&PasskeyAufgabe::anmelden(&jetzt), &jetzt)).await?;
    Ok(Json(AnmeldeOptionen { challenge, rp_id: RP_ID.into(), zeitlimit_ms: ZEITLIMIT_MS }))
}

/// `POST /api/passkey/anmelden/ende`: Antwort prüfen, Ausweis ausstellen.
/// Eine laufende Sitzung mit Passkey wird dabei beendet (neuer Ausweis, neue Zeit).
pub async fn anmelden_ende(State(state): State<AppState>, inhaber: Option<Extension<Inhaber>>, kopf: HeaderMap, koerper: Bytes) -> Result<Response, AppError> {
    let jwt = schluessel(&state)?;
    let k: AnmeldenEnde = json_lesen(&kopf, &koerper)?;
    let id_roh = b64_feld(&k.id, "id")?;
    let client = b64_feld(&k.client_daten, "client_daten")?;
    let auth = b64_feld(&k.auth_daten, "auth_daten")?;
    let sig = b64_feld(&k.signatur, "signatur")?;
    let id = passkey::b64(&id_roh);
    let herkunft = herkunft(&state);
    let challenge = webauthn::challenge_aus(&client).unwrap_or_default();
    let jetzt = jetzt();
    let benutzer = k.benutzer.filter(|b| !b.is_empty());
    let p = hintergrund(&state, move |s| {
        // Erst die Challenge verbrauchen (wie beim Anlegen), dann Art und
        // Herkunft prüfen: auch eine Antwort mit falscher Art oder Herkunft
        // macht die Challenge ungültig.
        s.passkey_aufgabe_nehmen(&challenge, AufgabenArt::Anmelden, &jetzt)?;
        // Ausdrücklich: geprüft wird genau die Challenge, die eben verbraucht wurde.
        if webauthn::client_daten_pruefen(&client, "webauthn.get", &herkunft)? != challenge {
            return Err(Abweisung::ChallengeUnbekannt.into());
        }
        let e = Erwartung { herkunft: &herkunft, rp_id: RP_ID };
        s.passkey_anmelden(&id, benutzer.as_deref(), &client, &auth, &sig, e, &jetzt)
    })
    .await?;
    let ausweis = Ausweis::neu(&p.id, jetzt.timestamp());
    if let Some(Extension(Inhaber::Passkey(alt))) = &inhaber {
        let (jti, bis) = (alt.ausweis.jti.clone(), alt.ausweis.sitzung_ende());
        hintergrund(&state, move |s| Ok(s.ausweis_sperren(&jti, bis, &jetzt)?)).await?;
    }
    tracing::info!("Anmeldung mit Passkey: angemeldet");
    let stand = PasskeyStand {
        wie: AnmeldeArt::Passkey,
        passkey: Some(p.name.clone()),
        sitzung_bis: zeit_aus(ausweis.sitzung_ende(), &jetzt),
        frisch_bis: zeit_aus(ausweis.auth_time + FRISCH_SEKUNDEN, &jetzt),
        verfuegbar: true,
    };
    let mut res = Json(stand).into_response();
    if let Some(c) = cookie::setzen(&jwt.ausstellen(&ausweis), ausweis.exp - jetzt.timestamp()) {
        res.headers_mut().insert(header::SET_COOKIE, c);
    }
    Ok(res)
}

/// `POST /api/passkey/abmelden`: Ausweis sperren (bis er ohnehin abliefe) und Cookie löschen.
pub async fn abmelden(State(state): State<AppState>, inhaber: Option<Extension<Inhaber>>) -> Result<Response, AppError> {
    if let Some(Extension(Inhaber::Passkey(s))) = inhaber {
        let (jti, bis, jetzt) = (s.ausweis.jti.clone(), s.ausweis.sitzung_ende(), jetzt());
        hintergrund(&state, move |sp| Ok(sp.ausweis_sperren(&jti, bis, &jetzt)?)).await?;
        tracing::info!("Anmeldung mit Passkey: abgemeldet");
    }
    let mut res = StatusCode::NO_CONTENT.into_response();
    res.headers_mut().insert(header::SET_COOKIE, cookie::loeschen());
    Ok(res)
}

// ----- mit Windows-Anmeldung (irgendein Konto) -----

fn konto_aus(konto: Option<Extension<WindowsKonto>>) -> Result<WindowsKonto, AppError> {
    konto.map(|Extension(k)| k).ok_or_else(AppError::anmeldung_noetig)
}

/// `GET /api/passkey/antrag`: die Passkeys des eigenen Windows-Kontos,
/// dazu der Anmeldestand in einem Wort (Kopfleiste, Fassung 2).
pub async fn antrag_liste(
    State(state): State<AppState>,
    inhaber: Option<Extension<Inhaber>>,
    konto: Option<Extension<WindowsKonto>>,
) -> Result<Json<EigeneAntraege>, AppError> {
    let konto = konto_aus(konto)?;
    let sid = konto.sid.clone();
    let alle = hintergrund(&state, |s| Ok(s.passkeys()?)).await?;
    let anmeldung = wort_aus(inhaber.is_some(), Some(&konto), &alle);
    let passkeys = alle.iter().filter(|p| p.konto_sid.eq_ignore_ascii_case(&sid)).map(PasskeyZeile::from).collect();
    Ok(Json(EigeneAntraege { konto: konto.name, hauptkonto: konto.inhaber, passkeys, anmeldung }))
}

// ----- Anmeldestand in einem Wort (Kopfleiste, Fassung 2) -----

/// Der Anmeldestand aus dem, was die Schleuse an die Anfrage gehängt hat:
/// gilt sie als Inhaber (Windows-Anmeldung mit seinem Konto oder gültiger
/// Ausweis), `angemeldet`. Ein fremdes Windows-Konto (Negotiate fertig,
/// nicht der Inhaber) ohne registrierten Passkey ist `nicht_registriert`, mit
/// einem `nicht_angemeldet` (anmelden geht). Ohne Windows-Anmeldung auf der
/// Verbindung `nicht_angemeldet`. Ausstehende, abgelehnte und widerrufene
/// Passkeys zählen nicht als registriert.
pub fn wort_aus(inhaber: bool, konto: Option<&WindowsKonto>, alle: &[crate::vertrag::passkey::Passkey]) -> Anmeldewort {
    if inhaber || konto.is_some_and(|k| k.inhaber) {
        return Anmeldewort::Angemeldet;
    }
    match konto {
        Some(k) => {
            let registriert = alle.iter().any(|p| p.konto_sid.eq_ignore_ascii_case(&k.sid) && p.status == crate::vertrag::passkey::PasskeyStatus::Registriert);
            if registriert { Anmeldewort::NichtAngemeldet } else { Anmeldewort::NichtRegistriert }
        }
        None => Anmeldewort::NichtAngemeldet,
    }
}

/// Wie [`wort_aus`], liest die Passkeys selbst (nur bei einem fremden Konto).
/// Lässt sich der Datenspeicher nicht lesen, gilt `nicht_angemeldet`: Der
/// Weg zum Passkey bleibt offen, und die Seite dort sagt, was fehlt.
pub async fn anmeldewort(state: &AppState, inhaber: bool, konto: Option<WindowsKonto>) -> Anmeldewort {
    if inhaber || konto.as_ref().is_none_or(|k| k.inhaber) {
        return wort_aus(inhaber, konto.as_ref(), &[]);
    }
    let Some(speicher) = state.speicher.clone() else { return Anmeldewort::NichtAngemeldet };
    match Speicher::im_hintergrund(speicher, |s| s.passkeys()).await {
        Ok(alle) => wort_aus(false, konto.as_ref(), &alle),
        Err(e) => {
            tracing::error!("Anmeldestand: Datenspeicher: {e} ({})", e.einzelheiten());
            Anmeldewort::NichtAngemeldet
        }
    }
}

/// Name des Geräts: getrimmt, 1 bis 60 Zeichen, ohne Steuerzeichen.
fn name_pruefen(name: &str) -> Result<String, AppError> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > NAME_HOECHSTENS || name.chars().any(char::is_control) {
        return Err(AppError::unlesbar(format!("Bitte dem Gerät einen Namen geben (1 bis {NAME_HOECHSTENS} Zeichen), etwa „Handy“ oder „Laptop“.")));
    }
    Ok(name.to_string())
}

/// `POST /api/passkey/antrag/beginn`: Grenzen prüfen und eine Challenge zum Anlegen.
pub async fn antrag_beginn(
    State(state): State<AppState>,
    konto: Option<Extension<WindowsKonto>>,
    kopf: HeaderMap,
    koerper: Bytes,
) -> Result<Json<AntragOptionen>, AppError> {
    schluessel(&state)?;
    let konto = konto_aus(konto)?;
    let k: AntragBeginn = json_lesen(&kopf, &koerper)?;
    let name = name_pruefen(&k.name)?;
    let jetzt = jetzt();
    let aufgabe = PasskeyAufgabe::anlegen(&konto, &name, &jetzt);
    let benutzer_id = aufgabe.benutzer.clone().unwrap_or_default();
    let (sid, inhaber) = (konto.sid.clone(), konto.inhaber);
    let (challenge, ausschliessen, ereignisse) = hintergrund(&state, move |s| {
        let ereignisse = if inhaber { Vec::new() } else { s.passkey_antrag_pruefen(&sid, &jetzt)? };
        let challenge = s.passkey_aufgabe_anlegen(&aufgabe, &jetzt)?;
        let ausschliessen = s
            .passkeys()?
            .into_iter()
            .filter(|p| p.konto_sid.eq_ignore_ascii_case(&sid))
            .filter(|p| matches!(p.status, crate::vertrag::passkey::PasskeyStatus::Ausstehend | crate::vertrag::passkey::PasskeyStatus::Registriert))
            .map(|p| p.id)
            .collect();
        Ok((challenge, ausschliessen, ereignisse))
    })
    .await?;
    melden(&state, &ereignisse);
    Ok(Json(AntragOptionen {
        challenge,
        rp_id: RP_ID.into(),
        rp_name: RP_NAME.into(),
        benutzer_id,
        benutzer_name: konto.name,
        anzeige_name: format!("Dashboard ({name})"),
        verfahren: webauthn::VERFAHREN.to_vec(),
        ausschliessen,
        zeitlimit_ms: ZEITLIMIT_MS,
        hauptkonto: konto.inhaber,
    }))
}

/// `POST /api/passkey/antrag/ende`: Antwort prüfen und speichern. Die
/// Challenge muss zu diesem Windows-Konto gehören.
pub async fn antrag_ende(
    State(state): State<AppState>,
    konto: Option<Extension<WindowsKonto>>,
    kopf: HeaderMap,
    koerper: Bytes,
) -> Result<(StatusCode, Json<AntragErgebnis>), AppError> {
    let konto = konto_aus(konto)?;
    let k: AntragEnde = json_lesen(&kopf, &koerper)?;
    let id_roh = b64_feld(&k.id, "id")?;
    let client = b64_feld(&k.client_daten, "client_daten")?;
    let att = b64_feld(&k.attestation, "attestation")?;
    let herkunft = herkunft(&state);
    let e = Erwartung { herkunft: &herkunft, rp_id: RP_ID };
    let neu = webauthn::anlegen_pruefen(&client, &att, &id_roh, e);
    let challenge = webauthn::challenge_aus(&client).unwrap_or_default();
    let jetzt = jetzt();
    let (p, ereignisse) = hintergrund(&state, move |s| {
        // Die Challenge wird auch verbraucht, wenn die Prüfung scheitert.
        let aufgabe = s.passkey_aufgabe_nehmen(&challenge, AufgabenArt::Anlegen, &jetzt);
        let neu = neu?;
        let aufgabe = aufgabe?;
        let gleich = aufgabe.konto_sid.as_deref().is_some_and(|sid| sid.eq_ignore_ascii_case(&konto.sid));
        if !gleich || aufgabe.hauptkonto != konto.inhaber {
            return Err(Abweisung::ChallengeFremd.into());
        }
        s.passkey_anlegen(&neu, &aufgabe, &jetzt)
    })
    .await?;
    melden(&state, &ereignisse);
    tracing::info!("Passkey angelegt: {}", if p.hauptkonto { "im Hauptkonto, registriert" } else { "Antrag, ausstehend" });
    Ok((StatusCode::CREATED, Json(AntragErgebnis { status: p.status, vergleichszahl: p.vergleichszahl.clone(), ablauf: p.ablauf.clone() })))
}

// ----- nur als Inhaber (unter /api/schluessel) -----

fn inhaber_aus(inhaber: Option<Extension<Inhaber>>) -> Result<Inhaber, AppError> {
    // Die Schleuse lässt ohne den Inhaber gar nicht bis hierher; sicher ist sicher.
    inhaber.map(|Extension(l)| l).ok_or_else(AppError::anmeldung_noetig)
}

/// `GET /api/schluessel/passkeys`.
pub async fn liste(State(state): State<AppState>, inhaber: Option<Extension<Inhaber>>) -> Result<Json<PasskeyListe>, AppError> {
    let inhaber = inhaber_aus(inhaber)?;
    let jetzt = jetzt();
    let (alle, offen) = hintergrund(&state, move |s| Ok((s.passkeys()?, s.passkey_antraege_offen(&jetzt)?))).await?;
    let (frisch_bis, eigener) = match &inhaber {
        Inhaber::Passkey(s) => (zeit_aus(s.ausweis.auth_time + FRISCH_SEKUNDEN, &jetzt), Some(s.ausweis.cid.clone())),
        Inhaber::Hauptkonto => (None, None),
    };
    Ok(Json(PasskeyListe { passkeys: alle.iter().map(PasskeyZeile::from).collect(), offen, wie: wie(Some(&inhaber)), frisch_bis, eigener }))
}

/// `GET /api/schluessel/passkeys/anzahl`: offene Anträge (Hinweis in der Kopfzeile).
pub async fn anzahl(State(state): State<AppState>) -> Result<Json<PasskeyAnzahl>, AppError> {
    let jetzt = jetzt();
    let offen = hintergrund(&state, move |s| Ok(s.passkey_antraege_offen(&jetzt)?)).await?;
    Ok(Json(PasskeyAnzahl { offen }))
}

fn freigeber(inhaber: &Inhaber) -> Freigeber {
    match inhaber {
        Inhaber::Hauptkonto => Freigeber::Hauptkonto,
        Inhaber::Passkey(s) => Freigeber::Passkey { cid: s.ausweis.cid.clone(), name: s.name.clone(), anmeldung: s.ausweis.auth_time },
    }
}

async fn entscheiden(state: &AppState, inhaber: Option<Extension<Inhaber>>, id: String, was: Entscheidung) -> Result<Json<PasskeyZeile>, AppError> {
    let wer = freigeber(&inhaber_aus(inhaber)?);
    let jetzt = jetzt();
    let (p, ereignisse) = hintergrund(state, move |s| s.passkey_entscheiden(&id, &was, &wer, &jetzt)).await?;
    melden(state, &ereignisse);
    tracing::info!("Passkey entschieden: {:?}", p.status);
    Ok(Json(PasskeyZeile::from(&p)))
}

/// `POST /api/schluessel/passkeys/<id>/freigeben` mit `{vergleichszahl}`.
pub async fn freigeben(
    State(state): State<AppState>,
    inhaber: Option<Extension<Inhaber>>,
    Path(id): Path<String>,
    kopf: HeaderMap,
    koerper: Bytes,
) -> Result<Json<PasskeyZeile>, AppError> {
    let f: Freigabe = json_lesen(&kopf, &koerper)?;
    entscheiden(&state, inhaber, id, Entscheidung::Freigeben { vergleichszahl: f.vergleichszahl }).await
}

/// `POST /api/schluessel/passkeys/<id>/ablehnen`.
pub async fn ablehnen(State(state): State<AppState>, inhaber: Option<Extension<Inhaber>>, Path(id): Path<String>) -> Result<Json<PasskeyZeile>, AppError> {
    entscheiden(&state, inhaber, id, Entscheidung::Ablehnen).await
}

/// `POST /api/schluessel/passkeys/<id>/widerrufen`.
pub async fn widerrufen(State(state): State<AppState>, inhaber: Option<Extension<Inhaber>>, Path(id): Path<String>) -> Result<Json<PasskeyZeile>, AppError> {
    entscheiden(&state, inhaber, id, Entscheidung::Widerrufen).await
}

/// `POST /api/schluessel/sitzungen-beenden`: alle Ausweise bis jetzt gelten
/// nicht mehr (auch der eigene). Das Hauptkonto bleibt angemeldet.
pub async fn sitzungen_beenden(State(state): State<AppState>, inhaber: Option<Extension<Inhaber>>) -> Result<Response, AppError> {
    let inhaber = inhaber_aus(inhaber)?;
    let jetzt = jetzt();
    hintergrund(&state, move |s| Ok(s.alle_sitzungen_beenden(&jetzt)?)).await?;
    tracing::info!("Alle Sitzungen mit Passkey beendet");
    let mut res = StatusCode::NO_CONTENT.into_response();
    if matches!(inhaber, Inhaber::Passkey(_)) {
        res.headers_mut().insert(header::SET_COOKIE, cookie::loeschen());
    }
    Ok(res)
}

/// Jede Minute (aus `aufgaben::spawn_ablauf`): abgelaufene Anträge schließen,
/// alte Challenges und Sperren entfernen.
pub async fn ablauf_lauf(state: &AppState, jetzt: Zeit) -> Result<(), AppError> {
    let ereignisse = hintergrund(state, move |s| Ok(s.passkey_aufraeumen(&jetzt)?)).await?;
    if !ereignisse.is_empty() {
        melden(state, &ereignisse);
    }
    Ok(())
}

#[cfg(all(test, windows))]
pub(crate) mod tests {
    use axum::body::Body;
    use axum::extract::ConnectInfo;
    use axum::http::{Request, StatusCode, header};
    use axum::{Extension, Router};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use super::*;
    use crate::anmeldung::verbindung::{Verbindung, merken};
    use crate::app::{beispiel_fuer_tests, router, router_angemeldet};
    use crate::passkey::webauthn::pruef_authenticator::PruefAuthenticator;

    const HERKUNFT: &str = "http://localhost:8787";

    fn zustand() -> AppState {
        AppState::fuer_tests(&beispiel_fuer_tests())
    }

    /// Über eine Verbindung, die als fremdes Windows-Konto angemeldet ist.
    fn fremd(state: &AppState) -> Router {
        let v = Verbindung::neu();
        merken(v.nr, WindowsKonto::test(false));
        router(state.clone()).layer(Extension(ConnectInfo(v)))
    }

    /// Ohne jede Windows-Anmeldung (anderer Browser oder noch nicht ausgehandelt).
    fn ohne(state: &AppState) -> Router {
        router(state.clone()).layer(Extension(ConnectInfo(Verbindung::neu())))
    }

    fn post(pfad: &str, koerper: serde_json::Value, cookie: Option<&str>) -> Request<Body> {
        let mut r =
            Request::post(pfad).header(header::HOST, "localhost:8787").header(header::ORIGIN, HERKUNFT).header(header::CONTENT_TYPE, "application/json");
        if let Some(c) = cookie {
            r = r.header(header::COOKIE, format!("{}={c}", cookie::NAME));
        }
        r.body(Body::from(koerper.to_string())).unwrap()
    }

    fn get(pfad: &str, cookie: Option<&str>) -> Request<Body> {
        let mut r = Request::get(pfad).header(header::HOST, "localhost:8787");
        if let Some(c) = cookie {
            r = r.header(header::COOKIE, format!("{}={c}", cookie::NAME));
        }
        r.body(Body::empty()).unwrap()
    }

    async fn schicke(app: &Router, req: Request<Body>) -> (StatusCode, Option<String>, serde_json::Value) {
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let set = res.headers().get(header::SET_COOKIE).map(|v| v.to_str().unwrap().to_string());
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, set, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
    }

    /// Der Ausweis aus `Set-Cookie`.
    fn ausweis_aus_cookie(set: &str) -> String {
        set.strip_prefix(&format!("{}=", cookie::NAME)).unwrap().split(';').next().unwrap().to_string()
    }

    /// Passkey anlegen über die Wege: beginn, Authenticator, ende.
    pub(crate) async fn anlegen(app: &Router, a: &PruefAuthenticator) -> (StatusCode, serde_json::Value) {
        let (s, _, o) = schicke(app, post("/api/passkey/antrag/beginn", serde_json::json!({"name": "Handy"}), None)).await;
        if s != StatusCode::OK {
            return (s, o);
        }
        let (c, att) = a.anlegen(o["challenge"].as_str().unwrap(), HERKUNFT);
        let k = serde_json::json!({"id": a.id_b64(), "client_daten": passkey::b64(&c), "attestation": passkey::b64(&att)});
        let (s, _, j) = schicke(app, post("/api/passkey/antrag/ende", k, None)).await;
        (s, j)
    }

    fn anmelde_koerper(a: &mut PruefAuthenticator, challenge: &str, herkunft: &str) -> serde_json::Value {
        let (c, auth, sig) = a.anmelden(challenge, herkunft);
        serde_json::json!({
            "id": a.id_b64(), "client_daten": passkey::b64(&c), "auth_daten": passkey::b64(&auth), "signatur": passkey::b64(&sig)
        })
    }

    /// Mit Passkey anmelden: gibt Status und Ausweis zurück.
    pub(crate) async fn anmelden(app: &Router, a: &mut PruefAuthenticator, herkunft: &str) -> (StatusCode, Option<String>) {
        let (_, _, o) = schicke(app, post("/api/passkey/anmelden/beginn", serde_json::json!({}), None)).await;
        let k = anmelde_koerper(a, o["challenge"].as_str().unwrap(), herkunft);
        let (s, set, _) = schicke(app, post("/api/passkey/anmelden/ende", k, None)).await;
        (s, set.map(|s| ausweis_aus_cookie(&s)))
    }

    #[tokio::test]
    async fn hauptkonto_legt_an_und_meldet_mit_passkey_an() {
        let state = zustand();
        let haupt = router_angemeldet(state.clone());
        let mut a = PruefAuthenticator::es256();
        let (s, j) = anlegen(&haupt, &a).await;
        assert_eq!((s, j["status"].as_str()), (StatusCode::CREATED, Some("registriert")), "{j}");
        // Ohne Windows-Anmeldung, nur mit dem Passkey.
        let app = ohne(&state);
        let (s, _, _) = schicke(&app, get("/api/vorgaenge/uebersicht", None)).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        let (_, _, o) = schicke(&app, post("/api/passkey/anmelden/beginn", serde_json::json!({}), None)).await;
        assert_eq!(o["rp_id"], "localhost");
        let k = anmelde_koerper(&mut a, o["challenge"].as_str().unwrap(), HERKUNFT);
        let (s, set, j) = schicke(&app, post("/api/passkey/anmelden/ende", k.clone(), None)).await;
        assert_eq!(s, StatusCode::OK, "{j}");
        let set = set.unwrap();
        for teil in ["__Host-dashboard=", "HttpOnly", "Secure", "SameSite=Strict", "Path=/", "Max-Age=900"] {
            assert!(set.contains(teil), "{teil} fehlt: {set}");
        }
        let ausweis = ausweis_aus_cookie(&set);
        assert!(!j.to_string().contains(&ausweis), "Ausweis nie im Körper");
        let (s, _, _) = schicke(&app, get("/api/vorgaenge/uebersicht", Some(&ausweis))).await;
        assert_eq!(s, StatusCode::OK);
        let (_, _, j) = schicke(&app, get("/api/passkey/stand", Some(&ausweis))).await;
        assert_eq!(j["wie"], "passkey", "{j}");
        // Dieselbe Antwort noch einmal: Challenge verbraucht.
        let (s, set, _) = schicke(&app, post("/api/passkey/anmelden/ende", k, None)).await;
        assert_eq!((s, set), (StatusCode::FORBIDDEN, None));
        // Abmelden sperrt den Ausweis.
        let (s, set, _) = schicke(&app, post("/api/passkey/abmelden", serde_json::json!({}), Some(&ausweis))).await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        assert!(set.unwrap().contains("Max-Age=0"));
        let (s, set, _) = schicke(&app, get("/api/vorgaenge/uebersicht", Some(&ausweis))).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        assert!(set.unwrap().contains("Max-Age=0"), "ungültiges Cookie wird gelöscht");
    }

    /// Abnahme 7c: Eine Anmeldung mit falscher Art oder Herkunft verbraucht die
    /// Challenge; ein zweiter Versuch mit derselben Challenge (jetzt richtig)
    /// wird als unbekannt oder schon benutzt abgewiesen.
    #[tokio::test]
    async fn falsche_art_oder_herkunft_verbraucht_die_challenge() {
        let state = zustand();
        let mut a = PruefAuthenticator::es256();
        assert_eq!(anlegen(&router_angemeldet(state.clone()), &a).await.0, StatusCode::CREATED);
        let app = ohne(&state);
        for (art, herkunft) in [("webauthn.create", HERKUNFT), ("webauthn.get", "http://127.0.0.1:8787")] {
            let (_, _, o) = schicke(&app, post("/api/passkey/anmelden/beginn", serde_json::json!({}), None)).await;
            let ch = o["challenge"].as_str().unwrap().to_string();
            a.art_anmelden = art;
            let k = anmelde_koerper(&mut a, &ch, herkunft);
            let (s, set, _) = schicke(&app, post("/api/passkey/anmelden/ende", k, None)).await;
            assert_eq!((s, set), (StatusCode::FORBIDDEN, None), "{art} {herkunft}");
            a.art_anmelden = "webauthn.get";
            let k = anmelde_koerper(&mut a, &ch, HERKUNFT);
            let (s, set, j) = schicke(&app, post("/api/passkey/anmelden/ende", k, None)).await;
            assert_eq!((s, set), (StatusCode::FORBIDDEN, None), "{art} {herkunft}: Challenge muss verbraucht sein");
            assert!(j["fehler"].as_str().unwrap().contains("schon einmal da"), "{j}");
        }
        // Mit frischer Challenge geht es (der Passkey selbst ist in Ordnung).
        assert_eq!(anmelden(&app, &mut a, HERKUNFT).await.0, StatusCode::OK);
    }

    #[tokio::test]
    async fn falsche_herkunft_ueber_die_wege() {
        let state = zustand();
        let mut a = PruefAuthenticator::rs256();
        assert_eq!(anlegen(&router_angemeldet(state.clone()), &a).await.0, StatusCode::CREATED);
        let app = ohne(&state);
        let (s, ausweis) = anmelden(&app, &mut a, "http://127.0.0.1:8787").await;
        assert_eq!((s, ausweis), (StatusCode::FORBIDDEN, None));
        let (s, ausweis) = anmelden(&app, &mut a, HERKUNFT).await;
        assert_eq!(s, StatusCode::OK);
        assert!(ausweis.is_some(), "RS256 meldet an");
    }

    #[tokio::test]
    async fn antrag_aus_anderem_konto_freigabe_und_widerruf() {
        let state = zustand();
        let haupt = router_angemeldet(state.clone());
        let anders = fremd(&state);
        let mut a = PruefAuthenticator::es256();
        // Antrag: ausstehend, mit Vergleichszahl; berechtigt zu nichts.
        let (s, j) = anlegen(&anders, &a).await;
        assert_eq!((s, j["status"].as_str()), (StatusCode::CREATED, Some("ausstehend")), "{j}");
        let zahl = j["vergleichszahl"].as_str().unwrap().to_string();
        let (s, _, j) = schicke(&anders, get("/api/vorgaenge/uebersicht", None)).await;
        assert_eq!((s, j["art"].as_str()), (StatusCode::UNAUTHORIZED, Some("passkey")));
        let (s, ausweis) = anmelden(&anders, &mut a, HERKUNFT).await;
        assert_eq!((s, ausweis), (StatusCode::FORBIDDEN, None), "ausstehend: kein Ausweis");
        // Zweiter Antrag desselben Kontos: abgewiesen.
        let (s, j) = anlegen(&anders, &PruefAuthenticator::es256()).await;
        assert_eq!(s, StatusCode::TOO_MANY_REQUESTS, "{j}");
        // Im Hauptkonto: Liste mit Vergleichszahl, Vorgang, Hinweis-Zahl.
        let (_, _, liste) = schicke(&haupt, get("/api/schluessel/passkeys", None)).await;
        assert_eq!(liste["offen"], 1);
        assert_eq!(liste["passkeys"][0]["vergleichszahl"].as_str(), Some(zahl.as_str()));
        assert_eq!(liste["passkeys"][0]["konto"].as_str(), Some(r"PC\fremd"));
        assert!(!liste.to_string().contains("S-1-5-21"), "keine SID in der Liste");
        let (_, _, anzahl) = schicke(&haupt, get("/api/schluessel/passkeys/anzahl", None)).await;
        assert_eq!(anzahl["offen"], 1);
        let (_, _, u) = schicke(&haupt, get("/api/vorgaenge/uebersicht", None)).await;
        assert!(u["vorgaenge"].as_array().unwrap().iter().any(|v| v["art"] == "passkey-antrag"), "{u}");
        // Das andere Konto kommt nicht an die Verwaltung.
        let id = a.id_b64();
        let pfad = format!("/api/schluessel/passkeys/{id}/freigeben");
        let (s, _, _) = schicke(&anders, post(&pfad, serde_json::json!({"vergleichszahl": zahl}), None)).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        // Freigabe aus dem Hauptkonto.
        let (s, _, j) = schicke(&haupt, post(&pfad, serde_json::json!({"vergleichszahl": zahl}), None)).await;
        assert_eq!((s, j["status"].as_str()), (StatusCode::OK, Some("registriert")), "{j}");
        // Jetzt meldet das andere Konto mit dem Passkey an und sieht die Vorgänge.
        let (s, ausweis) = anmelden(&anders, &mut a, HERKUNFT).await;
        assert_eq!(s, StatusCode::OK);
        let ausweis = ausweis.unwrap();
        let (s, _, _) = schicke(&anders, get("/api/vorgaenge/uebersicht", Some(&ausweis))).await;
        assert_eq!(s, StatusCode::OK);
        // Widerruf aus dem Hauptkonto beendet die laufende Sitzung sofort.
        let (s, _, _) = schicke(&haupt, post(&format!("/api/schluessel/passkeys/{id}/widerrufen"), serde_json::json!({}), None)).await;
        assert_eq!(s, StatusCode::OK);
        let (s, _, j) = schicke(&anders, get("/api/vorgaenge/uebersicht", Some(&ausweis))).await;
        assert_eq!((s, j["art"].as_str()), (StatusCode::UNAUTHORIZED, Some("passkey")));
    }

    /// Das Wort des freien Kanals (`/api/events/frei`, Kopfleiste Fassung 2)
    /// und von `/api/passkey/antrag`.
    async fn wort(app: &Router, pfad: &str, cookie: Option<&str>) -> String {
        let res = app.clone().oneshot(get(pfad, cookie)).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK, "{pfad}");
        if pfad == "/api/events/frei" {
            let mut body = res.into_body();
            let teil = tokio::time::timeout(std::time::Duration::from_secs(3), body.frame()).await.unwrap().unwrap().unwrap();
            let text = String::from_utf8_lossy(&teil.into_data().unwrap()).to_string();
            let daten = text.strip_prefix("event: anmeldung\ndata: ").and_then(|t| t.lines().next()).unwrap_or_else(|| panic!("{text}"));
            serde_json::from_str::<serde_json::Value>(daten).unwrap()["stand"].as_str().unwrap().to_string()
        } else {
            let bytes = res.into_body().collect().await.unwrap().to_bytes();
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["anmeldung"].as_str().unwrap().to_string()
        }
    }

    /// Kopfleiste, Fassung 2: Ein fremdes Windows-Konto (Negotiate fertig,
    /// nicht der Inhaber) ohne Passkey ist `nicht_registriert`, auch mit einem
    /// ausstehenden Antrag; nach der Freigabe `nicht_angemeldet`, mit
    /// Ausweis `angemeldet`, nach dem Widerruf wieder `nicht_registriert`.
    /// Ohne Windows-Anmeldung `nicht_angemeldet`, im Hauptkonto `angemeldet`.
    #[tokio::test]
    async fn fremdes_konto_ergibt_nicht_registriert() {
        let state = zustand();
        let haupt = router_angemeldet(state.clone());
        let anders = fremd(&state);
        for pfad in ["/api/events/frei", "/api/passkey/antrag"] {
            assert_eq!(wort(&anders, pfad, None).await, "nicht_registriert", "{pfad}");
            assert_eq!(wort(&haupt, pfad, None).await, "angemeldet", "{pfad}");
        }
        assert_eq!(wort(&ohne(&state), "/api/events/frei", None).await, "nicht_angemeldet");
        let mut a = PruefAuthenticator::es256();
        let (s, j) = anlegen(&anders, &a).await;
        assert_eq!(s, StatusCode::CREATED, "{j}");
        let zahl = j["vergleichszahl"].as_str().unwrap().to_string();
        assert_eq!(wort(&anders, "/api/events/frei", None).await, "nicht_registriert", "ausstehend zählt nicht");
        let id = a.id_b64();
        let (s, _, _) = schicke(&haupt, post(&format!("/api/schluessel/passkeys/{id}/freigeben"), serde_json::json!({"vergleichszahl": zahl}), None)).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(wort(&anders, "/api/events/frei", None).await, "nicht_angemeldet");
        assert_eq!(wort(&anders, "/api/passkey/antrag", None).await, "nicht_angemeldet");
        let (_, ausweis) = anmelden(&anders, &mut a, HERKUNFT).await;
        let ausweis = ausweis.unwrap();
        assert_eq!(wort(&anders, "/api/events/frei", Some(&ausweis)).await, "angemeldet");
        assert_eq!(wort(&ohne(&state), "/api/events/frei", Some(&ausweis)).await, "angemeldet", "Passkey ohne Windows");
        let (s, _, _) = schicke(&haupt, post(&format!("/api/schluessel/passkeys/{id}/widerrufen"), serde_json::json!({}), None)).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(wort(&anders, "/api/events/frei", Some(&ausweis)).await, "nicht_registriert");
    }

    /// Den geschützten Kanal mit Ausweis öffnen und das erste Ereignis lesen.
    async fn kanal_auf(app: &Router, ausweis: &str) -> axum::body::Body {
        let res = app.clone().oneshot(get("/api/events", Some(ausweis))).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let mut body = res.into_body();
        let teil = tokio::time::timeout(std::time::Duration::from_secs(3), body.frame()).await.unwrap().unwrap().unwrap();
        let text = String::from_utf8_lossy(&teil.into_data().unwrap()).to_string();
        assert!(text.starts_with("event: anmeldung\ndata: {\"stand\":\"angemeldet\"}"), "{text}");
        body
    }

    /// Der Rest des Stroms; er muss binnen 3 s enden.
    async fn kanal_rest(body: axum::body::Body) -> String {
        let alles = tokio::time::timeout(std::time::Duration::from_secs(3), body.collect()).await.expect("der Strom endet nach dem Ende der Sitzung").unwrap();
        String::from_utf8_lossy(&alles.to_bytes()).to_string()
    }

    /// Kopfleiste, Fassung 2 (Nachbesserung): Widerruf und Abmelden beenden
    /// einen offenen geschützten Live-Kanal der Sitzung, zum Schluss mit
    /// `anmeldung` und `nicht_angemeldet`; ein neuer Versuch gibt 401.
    #[tokio::test]
    async fn widerruf_und_abmelden_beenden_den_geschuetzten_kanal() {
        let state = zustand();
        let haupt = router_angemeldet(state.clone());
        let anders = fremd(&state);
        let mut a = PruefAuthenticator::es256();
        let (_, j) = anlegen(&anders, &a).await;
        let zahl = j["vergleichszahl"].as_str().unwrap().to_string();
        let id = a.id_b64();
        let (s, _, _) = schicke(&haupt, post(&format!("/api/schluessel/passkeys/{id}/freigeben"), serde_json::json!({"vergleichszahl": zahl}), None)).await;
        assert_eq!(s, StatusCode::OK);

        // Abmelden: der Strom dieser Sitzung endet.
        let (_, ausweis) = anmelden(&anders, &mut a, HERKUNFT).await;
        let ausweis = ausweis.unwrap();
        let body = kanal_auf(&anders, &ausweis).await;
        let (s, _, _) = schicke(&anders, post("/api/passkey/abmelden", serde_json::json!({}), Some(&ausweis))).await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        let rest = kanal_rest(body).await;
        assert!(rest.trim_end().ends_with("event: anmeldung\ndata: {\"stand\":\"nicht_angemeldet\"}"), "{rest}");
        let res = anders.clone().oneshot(get("/api/events", Some(&ausweis))).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        // Widerruf: ebenso.
        let (_, ausweis) = anmelden(&anders, &mut a, HERKUNFT).await;
        let ausweis = ausweis.unwrap();
        let body = kanal_auf(&anders, &ausweis).await;
        let (s, _, _) = schicke(&haupt, post(&format!("/api/schluessel/passkeys/{id}/widerrufen"), serde_json::json!({}), None)).await;
        assert_eq!(s, StatusCode::OK);
        let rest = kanal_rest(body).await;
        assert!(rest.trim_end().ends_with("event: anmeldung\ndata: {\"stand\":\"nicht_angemeldet\"}"), "{rest}");
        let res = anders.clone().oneshot(get("/api/events", Some(&ausweis))).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn ablehnung_und_freigabe_nur_mit_frischer_bestaetigung() {
        let state = zustand();
        let haupt = router_angemeldet(state.clone());
        // Der Inhaber legt im Hauptkonto einen Passkey an (Handy) und meldet sich damit in einem anderen Konto an.
        let mut inhaber = PruefAuthenticator::es256();
        anlegen(&haupt, &inhaber).await;
        let anders = fremd(&state);
        let (_, ausweis) = anmelden(&anders, &mut inhaber, HERKUNFT).await;
        let ausweis = ausweis.unwrap();
        // Ein Antrag aus einem dritten Konto.
        let dritter = {
            let v = Verbindung::neu();
            merken(v.nr, WindowsKonto { sid: "S-1-5-21-7-7-7-1003".into(), name: r"PC\dritter".into(), inhaber: false });
            router(state.clone()).layer(Extension(ConnectInfo(v)))
        };
        let b = PruefAuthenticator::es256();
        let (_, j) = anlegen(&dritter, &b).await;
        let zahl = j["vergleichszahl"].as_str().unwrap().to_string();
        let id = b.id_b64();
        // Ein Ausweis mit Anmeldung vor 6 Minuten (richtig signiert, noch gültig).
        let jwt = state.jwt.clone().unwrap();
        let jetzt = aufgaben::jetzt().timestamp();
        let gueltig = jwt.pruefen(&ausweis, jetzt).unwrap();
        let alt = Ausweis { auth_time: jetzt - 6 * 60, iat: jetzt - 10, exp: jetzt + 600, jti: "alt-1".into(), ..gueltig };
        let alt = jwt.ausstellen(&alt);
        let pfad = format!("/api/schluessel/passkeys/{id}/freigeben");
        let (s, _, j) = schicke(&anders, post(&pfad, serde_json::json!({"vergleichszahl": zahl}), Some(&alt))).await;
        assert_eq!((s, j["art"].as_str()), (StatusCode::FORBIDDEN, Some("bestaetigung")), "{j}");
        let ablehnen = format!("/api/schluessel/passkeys/{id}/ablehnen");
        let (s, _, _) = schicke(&anders, post(&ablehnen, serde_json::json!({}), Some(&alt))).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
        // Mit der frischen Anmeldung: Ablehnung geht; der Eintrag bleibt als abgelehnt.
        let (s, _, j) = schicke(&anders, post(&ablehnen, serde_json::json!({}), Some(&ausweis))).await;
        assert_eq!((s, j["status"].as_str()), (StatusCode::OK, Some("abgelehnt")), "{j}");
        let (_, _, liste) = schicke(&haupt, get("/api/schluessel/passkeys", None)).await;
        assert!(liste["passkeys"].as_array().unwrap().iter().any(|p| p["id"] == id.as_str() && p["status"] == "abgelehnt"));
        // Selbstentscheidung: der eigene Passkey kann sich nicht widerrufen.
        let eigen = format!("/api/schluessel/passkeys/{}/widerrufen", inhaber.id_b64());
        let (s, _, _) = schicke(&anders, post(&eigen, serde_json::json!({}), Some(&ausweis))).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
    }

    /// Auflage aus dem Hotfix 3a-1 (Plan 3a-4): Eine Sitzung mit Passkey
    /// (fremdes Windows-Konto, Passkey des Inhabers) bekommt `/api/pc/verlauf/voll`
    /// und `/api/projekte/<slug>/stand/voll` vollständig: mit Starts und
    /// Vorgängen, ohne den Hinweis `ohne-anmeldung`, mit den Titeln der
    /// Aufgaben; dasselbe wie das Hauptkonto. Ohne Ausweis 401.
    #[tokio::test]
    async fn passkey_sitzung_bekommt_verlauf_und_stand_voll() {
        let state = zustand();
        let s = state.speicher.clone().unwrap();
        let jetzt = aufgaben::jetzt();
        let start = jetzt - chrono::Duration::days(2);
        s.pc_ereignisse_einspielen(&crate::pc::ereignisse::starts(&[start]), &jetzt).unwrap();
        s.vorgang_anlegen(crate::vertrag::api::VorgangsArt::Abnahme, "dashboard", "Abnahme Verlauf", None, "browser").unwrap();
        let neu = aufgaben::NeueAufgabe {
            projekt: "dashboard".into(),
            von: "test".into(),
            titel: "Titel nur mit Anmeldung".into(),
            link: None,
            knopf: None,
            frist: None,
            ablauf: None,
            prioritaet: None,
            frist_bezug: None,
        };
        s.aufgabe_anlegen(neu, &jetzt).unwrap();
        let haupt = router_angemeldet(state.clone());
        let mut a = PruefAuthenticator::es256();
        assert_eq!(anlegen(&haupt, &a).await.0, StatusCode::CREATED);
        let anders = fremd(&state);
        for pfad in ["/api/pc/verlauf/voll", "/api/projekte/dashboard/stand/voll"] {
            let (s, _, j) = schicke(&anders, get(pfad, None)).await;
            assert_eq!(s, StatusCode::UNAUTHORIZED, "{pfad} ohne Ausweis: {j}");
        }
        let (s, ausweis) = anmelden(&anders, &mut a, HERKUNFT).await;
        assert_eq!(s, StatusCode::OK);
        let ausweis = ausweis.unwrap();

        let kennungen = |v: &serde_json::Value| -> Vec<String> {
            let mut k: Vec<String> = v["ereignisse"].as_array().unwrap().iter().map(|e| e["kennung"].as_str().unwrap().to_string()).collect();
            k.sort();
            k
        };
        let (s, _, v) = schicke(&anders, get("/api/pc/verlauf/voll", Some(&ausweis))).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        let k = kennungen(&v);
        assert!(k.iter().any(|k| k.starts_with("start:")), "Starts mit Passkey: {k:?}");
        assert!(k.iter().any(|k| k.starts_with("vorgang:")), "Vorgänge mit Passkey: {k:?}");
        assert!(!v["hinweise"].as_array().unwrap().iter().any(|h| h["art"] == "ohne-anmeldung"), "{v}");
        let (_, _, voll_haupt) = schicke(&haupt, get("/api/pc/verlauf/voll", None)).await;
        assert_eq!(k, kennungen(&voll_haupt), "Passkey und Hauptkonto sehen denselben Verlauf");

        let (s, _, v) = schicke(&anders, get("/api/projekte/dashboard/stand/voll", Some(&ausweis))).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        let titel: Vec<&str> = v["eintraege"].as_array().unwrap().iter().filter(|e| e["art"] == "aufgabe").filter_map(|e| e["titel"].as_str()).collect();
        assert_eq!((v["sichtbar"].as_bool(), titel), (Some(true), vec!["Titel nur mit Anmeldung"]), "{v}");
        let (_, _, stand_haupt) = schicke(&haupt, get("/api/projekte/dashboard/stand/voll", None)).await;
        assert_eq!(v, stand_haupt, "Passkey und Hauptkonto sehen denselben Stand");
    }

    #[tokio::test]
    async fn ausweise_mit_falschem_verfahren_oder_abgelaufen_gelten_nicht() {
        let state = zustand();
        let mut a = PruefAuthenticator::es256();
        anlegen(&router_angemeldet(state.clone()), &a).await;
        let app = ohne(&state);
        let (_, ausweis) = anmelden(&app, &mut a, HERKUNFT).await;
        let ausweis = ausweis.unwrap();
        let teile: Vec<&str> = ausweis.split('.').collect();
        let none = format!("{}.{}.", passkey::b64(br#"{"alg":"none","typ":"JWT"}"#), teile[1]);
        let hs = format!("{}.{}.{}", passkey::b64(br#"{"alg":"HS256","typ":"JWT"}"#), teile[1], teile[2]);
        let jwt = state.jwt.clone().unwrap();
        let jetzt = aufgaben::jetzt().timestamp();
        let gueltig = jwt.pruefen(&ausweis, jetzt).unwrap();
        let abgelaufen = jwt.ausstellen(&Ausweis { iat: jetzt - 1000, exp: jetzt - 100, auth_time: jetzt - 1000, ..gueltig });
        for (was, t) in [("none", none), ("HS256", hs), ("abgelaufen", abgelaufen)] {
            let (s, set, _) = schicke(&app, get("/api/vorgaenge/uebersicht", Some(&t))).await;
            assert_eq!(s, StatusCode::UNAUTHORIZED, "{was}");
            assert!(set.is_some_and(|c| c.contains("Max-Age=0")), "{was}: Cookie gelöscht");
        }
        // „Alle Sitzungen beenden“ aus dem Hauptkonto.
        let (s, _, _) = schicke(&app, get("/api/vorgaenge/uebersicht", Some(&ausweis))).await;
        assert_eq!(s, StatusCode::OK);
        let haupt = router_angemeldet(state.clone());
        let (s, _, _) = schicke(&haupt, post("/api/schluessel/sitzungen-beenden", serde_json::json!({}), None)).await;
        assert_eq!(s, StatusCode::NO_CONTENT);
        let (s, _, _) = schicke(&app, get("/api/vorgaenge/uebersicht", Some(&ausweis))).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn antrag_braucht_windows_anmeldung() {
        let state = zustand();
        let app = ohne(&state);
        let (s, _, _) = schicke(&app, post("/api/passkey/antrag/beginn", serde_json::json!({"name": "Handy"}), None)).await;
        assert_eq!(s, StatusCode::UNAUTHORIZED);
        let res = app.clone().oneshot(get("/api/passkey/antrag", None)).await.unwrap();
        assert_eq!(res.headers().get(header::WWW_AUTHENTICATE).unwrap(), "Negotiate");
        // Name des Geräts muss stimmen.
        let anders = fremd(&state);
        let lang = "x".repeat(61);
        for name in ["", "   ", lang.as_str(), "a\nb"] {
            let (s, _, _) = schicke(&anders, post("/api/passkey/antrag/beginn", serde_json::json!({"name": name}), None)).await;
            assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{name:?}");
        }
        // Chats können keinen Vorgang passkey-antrag anlegen.
        let req = Request::post("/api/vorgaenge")
            .header(header::HOST, "127.0.0.1:8787")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(r#"{"art": "passkey-antrag", "slug": "passkey", "titel": "x"}"#))
            .unwrap();
        let (s, _, _) = schicke(&router_angemeldet(state.clone()), req).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    }
}
