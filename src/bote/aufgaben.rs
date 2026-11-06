//! Was der Bote auf eine Anfrage tut. Jede Aufgabe hat ein Zeitlimit wie im
//! Dienst bisher (Kindprozesse mit `kill_on_drop`, höchstens 300 s). Fehler
//! kommen als `Antwort::Fehler` mit einem Satz zurück, nie als Absturz.
//! Der Wert beim Tresor-Schreiben geht nur an `cred_write` und wird danach
//! überschrieben (`Wert` in `vertrag::bote`).

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::git;
use crate::pc::{inventory, sysapps};
use crate::schluessel;
use crate::vertrag::bote::{Anfrage, Antwort};

/// Höchste Dauer einer Aufgabe.
pub const HOECHSTENS: Duration = Duration::from_secs(300);
/// Höchste Dauer eines Baus: das Zeitlimit für npm (120 s, Etappe 4b) und die
/// Prüfungen davor (`bau::PRUEFUNG_HOECHSTENS`).
pub const BAU_HOECHSTENS: Duration = Duration::from_secs(super::bau::ZEITLIMIT.as_secs() + super::bau::PRUEFUNG_HOECHSTENS.as_secs());

/// Was der Bote über sich weiß.
#[derive(Debug, Clone)]
pub struct Kontext {
    /// SID des Kontos, unter dem der Bote läuft.
    pub konto_sid: String,
    /// Startzeit, RFC 3339.
    pub gestartet: String,
    /// Profil des Boten (Wurzel der Ordnergrößen).
    pub profil: PathBuf,
    /// Die Inhaltsquellen (`[inhalt].projekte` aus dienst.toml, Standard
    /// `<repo>/projects`; Umzug des Inhalts): Git-Stand gibt es nur für Repos,
    /// die eine Projektseite einer Quelle nennt. Leer: für keins.
    pub quellen: Vec<PathBuf>,
    /// Die Ausschlussliste des Inventars (`inventar-ausschluss.toml` im
    /// Daten-Ordner des Inhalts). `None` oder unlesbar: Das Inventar zählt
    /// nicht (geschlossen scheitern).
    pub ausschluss: Option<PathBuf>,
    /// Das Repo (`repo` aus dienst.toml): nur dort baut der Bote die
    /// Oberfläche (`Anfrage::Bau`). `None`: kein Bau.
    pub repo: Option<PathBuf>,
    /// Zeitlimit für `npm run build` (Tests setzen es kürzer).
    pub bau_zeitlimit: Duration,
    /// Ordner des Dienstes, die „Öffnen“ nie aufmacht (`oeffnen::ordner_des_dienstes`).
    pub verboten: Vec<PathBuf>,
    /// Wie „Öffnen“ öffnet; Tests setzen `bote::oeffnen::Aufnahme` ein.
    pub oeffner: std::sync::Arc<dyn super::oeffnen::Oeffner>,
    /// SID des Dienstkontos (`konten.dienst_sid`): nur dieser Client darf „Öffnen“ schicken. `None`: niemand.
    pub dienst_sid: Option<String>,
    /// Die eigene Grenze des Boten für „Öffnen“ (10 je Minute).
    pub oeffnungen: std::sync::Arc<crate::oeffnen::Grenze>,
}

impl Kontext {
    /// Für diesen Prozess: eigene SID, Startzeit jetzt, Profil aus `USERPROFILE`,
    /// ohne Projektordner.
    pub fn jetzt() -> Kontext {
        #[cfg(windows)]
        let konto_sid = super::win::eigene_sid().unwrap_or_default();
        #[cfg(not(windows))]
        let konto_sid = String::new();
        Kontext {
            konto_sid,
            gestartet: chrono::Local::now().to_rfc3339(),
            profil: inventory::home_dir(),
            quellen: Vec::new(),
            ausschluss: None,
            repo: None,
            bau_zeitlimit: super::bau::ZEITLIMIT,
            verboten: crate::oeffnen::ordner_des_dienstes(None),
            oeffner: standard_oeffner(),
            dienst_sid: None,
            oeffnungen: Default::default(),
        }
    }

    /// Mit einem Projektordner.
    pub fn mit_projekten(self, projekte: PathBuf) -> Kontext {
        Kontext { quellen: vec![projekte], ..self }
    }

    /// Mit Inhaltsquellen, Ausschlussliste und Repo aus den Einstellungen (so läuft der Bote).
    pub fn mit_einstellungen(self, e: &crate::dienst::einstellungen::Einstellungen) -> Kontext {
        Kontext {
            quellen: e.projekt_quellen(),
            ausschluss: Some(e.ausschluss_datei()),
            repo: Some(e.repo.clone()),
            verboten: crate::oeffnen::ordner_des_dienstes(Some(e)),
            dienst_sid: Some(e.konten.dienst_sid.clone()).filter(|s| !s.trim().is_empty()),
            ..self
        }
    }
}

/// Im Betrieb öffnet der Bote wirklich; in den Unit-Tests nie (`Verweigerer`).
fn standard_oeffner() -> std::sync::Arc<dyn super::oeffnen::Oeffner> {
    #[cfg(test)]
    return std::sync::Arc::new(super::oeffnen::Verweigerer);
    #[cfg(not(test))]
    std::sync::Arc::new(super::oeffnen::WindowsOeffner)
}

/// Alle Projektseiten aller Quellen (Umzug des Inhalts: auch außerhalb des Repos).
fn seiten_aller(quellen: &[PathBuf]) -> Vec<crate::inhalt::seite::Project> {
    match quellen {
        [eine] => crate::inhalt::seite::load_all(eine).0,
        mehrere => crate::quellen::Quellen::aus_liste(mehrere.to_vec()).seiten(),
    }
}

/// Git-Stand nur für Repos, die in einer Projektseite (`projekt.toml`)
/// unter `repos` stehen; Vergleich kanonisch, Groß/Klein egal.
/// Die Liste wird je Anfrage gelesen, damit neue Projekte sofort gelten.
// Antwort ist der Vertragstyp des Boten und wird gleich gesendet; eine Box nur fürs Err brächte nichts.
#[allow(clippy::result_large_err)]
fn repo_pruefen(pfad: &str, quellen: &[PathBuf]) -> Result<(), Antwort> {
    let abgelehnt = || Antwort::fehler("Den Git-Stand liefert der Bote nur für Repos, die eine Projektseite nennt.");
    if quellen.is_empty() {
        return Err(abgelehnt());
    }
    let gesucht = super::pfad_zum_vergleich(Path::new(pfad));
    let liste = seiten_aller(quellen);
    let bekannt = liste.iter().flat_map(|p| p.meta.repos.iter()).any(|r| super::pfad_zum_vergleich(Path::new(&r.path)) == gesucht);
    if bekannt {
        Ok(())
    } else {
        tracing::warn!("Git-Stand für einen Pfad abgelehnt, den keine Projektseite nennt.");
        Err(abgelehnt())
    }
}

/// Die Repos aller Projektseiten (zum Vergleich aufbereitet), höchstens
/// `BEKANNTE_FRISCH` alt; ein unbekannter Pfad liest sie neu, aber nicht öfter
/// als alle `BEKANNTE_NEU_AB`. So liest der Bote nicht alle 2 s alle Seiten.
fn bekannte_repos(projekte: &[PathBuf], gesucht: &[String]) -> Vec<String> {
    use std::sync::Mutex;
    use std::time::Instant;
    /// Gelesen um, Projektordner, Repos.
    type Karte = (Instant, Vec<PathBuf>, Vec<String>);
    static KARTE: Mutex<Option<Karte>> = Mutex::new(None);
    let mut g = KARTE.lock().unwrap_or_else(|e| {
        tracing::warn!("Bote: Sperre der bekannten Repos war vergiftet; es geht weiter.");
        e.into_inner()
    });
    let veraltet = match &*g {
        None => true,
        Some((zeit, ordner, liste)) => {
            ordner != projekte || zeit.elapsed() > BEKANNTE_FRISCH || (zeit.elapsed() > BEKANNTE_NEU_AB && gesucht.iter().any(|p| !liste.contains(p)))
        }
    };
    if veraltet {
        let liste = seiten_aller(projekte);
        let repos = liste.iter().flat_map(|p| p.meta.repos.iter()).map(|r| super::pfad_zum_vergleich(Path::new(&r.path))).collect();
        *g = Some((Instant::now(), projekte.to_vec(), repos));
    }
    g.as_ref().map(|(_, _, l)| l.clone()).unwrap_or_default()
}

/// So lange gilt die Liste der bekannten Repos.
const BEKANNTE_FRISCH: Duration = Duration::from_secs(30);
/// Frühestens so viel später liest ein unbekannter Pfad sie neu.
const BEKANNTE_NEU_AB: Duration = Duration::from_secs(5);

/// Fingerabdrücke (Etappe 2d) für Repos, die eine Projektseite nennt; andere
/// fehlen in der Antwort. Nur Zeitstempel und Größen, kein git, darum ohne
/// Besitzerprüfung. Höchstens `STEMPEL_HOECHSTENS` Repos.
async fn repo_stempel(pfade: Vec<String>, tief: bool, quellen: &[PathBuf]) -> Antwort {
    use crate::vertrag::bote::{RepoFingerabdruck, STEMPEL_HOECHSTENS};
    if quellen.is_empty() {
        return Antwort::fehler("Fingerabdrücke liefert der Bote nur für Repos, die eine Projektseite nennt.");
    }
    let projekte = quellen.to_vec();
    if pfade.len() > STEMPEL_HOECHSTENS {
        return Antwort::fehler(format!("Höchstens {STEMPEL_HOECHSTENS} Repos je Anfrage."));
    }
    let ergebnis = tokio::task::spawn_blocking(move || {
        let gesucht: Vec<String> = pfade.iter().map(|p| super::pfad_zum_vergleich(Path::new(p))).collect();
        let bekannt = bekannte_repos(&projekte, &gesucht);
        pfade
            .into_iter()
            .zip(gesucht)
            .filter(|(_, g)| bekannt.contains(g))
            .map(|(pfad, _)| {
                let stempel = crate::api::events::fingerabdruck(Path::new(&pfad), tief);
                RepoFingerabdruck { pfad, stempel }
            })
            .collect()
    })
    .await;
    match ergebnis {
        Ok(liste) => Antwort::RepoStempel(liste),
        Err(e) => Antwort::fehler(format!("Fingerabdrücke abgebrochen: {e}")),
    }
}

/// Satz, wenn der Repo-Ordner einem anderen Konto gehört.
pub const FREMDES_REPO: &str = "Das Repo gehört nicht diesem Konto.";

/// Git nur in Repo-Ordnern, die dem Konto des Boten gehören (Auflage aus der
/// Abnahme 7a1): Ein fremd angelegtes Repo könnte über seine Einstellungen
/// (etwa einen Filter-Treiber) Programme als Inhaber starten. Verglichen wird
/// der Besitzer des Ordners (`GetNamedSecurityInfoW`, OWNER) mit der SID des
/// Boten. Gibt es den Ordner nicht, bleibt es beim Satz von `git::inspect`
/// („Ordner nicht gefunden“, dort läuft kein git).
// Antwort ist der Vertragstyp des Boten und wird gleich gesendet; eine Box nur fürs Err brächte nichts.
#[allow(clippy::result_large_err)]
fn besitzer_pruefen(pfad: &str, eigene_sid: &str) -> Result<(), Antwort> {
    if !Path::new(pfad).is_dir() {
        return Ok(());
    }
    #[cfg(windows)]
    let besitzer = ordner_besitzer(pfad);
    #[cfg(not(windows))]
    let besitzer: Result<String, String> = Err("nur unter Windows".to_string());
    match besitzer {
        Ok(sid) if crate::anmeldung::ist_inhaber(&sid, eigene_sid) => Ok(()),
        Ok(_) => {
            tracing::warn!("Git-Stand abgelehnt: der Repo-Ordner gehört einem anderen Konto.");
            Err(Antwort::fehler(FREMDES_REPO))
        }
        Err(e) => {
            tracing::warn!("Git-Stand abgelehnt: Besitzer des Repo-Ordners nicht lesbar ({e}).");
            Err(Antwort::fehler(FREMDES_REPO))
        }
    }
}

/// SID des Besitzers eines Ordners als Text.
#[cfg(windows)]
fn ordner_besitzer(pfad: &str) -> Result<String, String> {
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::{ERROR_SUCCESS, LocalFree};
    use windows_sys::Win32::Security::Authorization::{ConvertSidToStringSidW, GetNamedSecurityInfoW, SE_FILE_OBJECT};
    use windows_sys::Win32::Security::{OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID};

    let breit: Vec<u16> = pfad.encode_utf16().chain(std::iter::once(0)).collect();
    let mut besitzer: PSID = null_mut();
    let mut sd: PSECURITY_DESCRIPTOR = null_mut();
    // SAFETY: nullterminierter Pfad; `besitzer` zeigt danach in `sd`, das mit LocalFree freigegeben wird.
    let r = unsafe {
        GetNamedSecurityInfoW(breit.as_ptr(), SE_FILE_OBJECT, OWNER_SECURITY_INFORMATION, &mut besitzer, null_mut(), null_mut(), null_mut(), &mut sd)
    };
    if r != ERROR_SUCCESS {
        return Err(format!("GetNamedSecurityInfoW: {r}"));
    }
    let mut text: *mut u16 = null_mut();
    // SAFETY: `besitzer` liegt in `sd`; Text und Beschreibung werden danach mit LocalFree freigegeben.
    unsafe {
        let ergebnis = if besitzer.is_null() || ConvertSidToStringSidW(besitzer, &mut text) == 0 || text.is_null() {
            Err("ConvertSidToStringSidW".to_string())
        } else {
            let mut n = 0;
            while *text.add(n) != 0 {
                n += 1;
            }
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(text, n));
            LocalFree(text as *mut _);
            Ok(s)
        };
        LocalFree(sd as *mut _);
        ergebnis
    }
}

/// Nur Ziele `ranke/<name>` mit gültigem Namen.
// Antwort ist der Vertragstyp des Boten und wird gleich gesendet; eine Box nur fürs Err brächte nichts.
#[allow(clippy::result_large_err)]
fn ziel_pruefen(ziel: &str) -> Result<(), Antwort> {
    match ziel.strip_prefix(schluessel::TARGET_PREFIX) {
        Some(name) if schluessel::is_valid_name(name) => Ok(()),
        _ => Err(Antwort::fehler(format!("Das Ziel „{ziel}“ nimmt der Bote nicht an; erlaubt ist nur {}<name>.", schluessel::TARGET_PREFIX))),
    }
}

/// Wurzel der Ordnergrößen: das Profil oder ein Ordner darin.
// Antwort ist der Vertragstyp des Boten und wird gleich gesendet; eine Box nur fürs Err brächte nichts.
#[allow(clippy::result_large_err)]
fn wurzel_pruefen(wurzel: Option<&str>, profil: &Path) -> Result<PathBuf, Antwort> {
    let Some(w) = wurzel else { return Ok(profil.to_path_buf()) };
    let w = PathBuf::from(w);
    let innen = super::pfad_zum_vergleich(&w);
    let profil_text = super::pfad_zum_vergleich(profil);
    if innen == profil_text || innen.starts_with(&format!("{profil_text}\\")) {
        Ok(w)
    } else {
        Err(Antwort::fehler("Ordnergrößen zählt der Bote nur im eigenen Profil."))
    }
}

/// Führt eine Anfrage aus, ohne bekannten Client (etwa `--einmal`): „Öffnen“
/// geht dann nie (nur vom Dienst, siehe [`ausfuehren_von`]).
pub async fn ausfuehren(anfrage: Anfrage, k: &Kontext) -> Antwort {
    ausfuehren_von(anfrage, k, None).await
}

/// Führt eine Anfrage aus; `client_sid`: das Konto des Clients an der Pipe
/// (die Schleife des Boten liest es per Impersonation, nur für „Öffnen“).
pub async fn ausfuehren_von(anfrage: Anfrage, k: &Kontext, client_sid: Option<String>) -> Antwort {
    let frist = match anfrage {
        Anfrage::Bau => k.bau_zeitlimit + Duration::from_secs(60),
        _ => HOECHSTENS,
    };
    match tokio::time::timeout(frist, ausfuehren_ohne_frist(anfrage, k, client_sid)).await {
        Ok(a) => a,
        Err(_) => Antwort::fehler(format!("Die Aufgabe hat länger als {} s gedauert und wurde abgebrochen.", frist.as_secs())),
    }
}

async fn ausfuehren_ohne_frist(anfrage: Anfrage, k: &Kontext, client_sid: Option<String>) -> Antwort {
    match anfrage {
        Anfrage::Hallo => Antwort::Hallo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            konto_sid: k.konto_sid.clone(),
            gestartet: k.gestartet.clone(),
            // Bei jedem Hallo neu (Lebenszeichen alle 60 s): Nummer und
            // Zustand der eigenen Sitzung, ohne SID und Namen.
            sitzung: crate::pc::sitzung::eigene_sitzung(),
            // Kleinkram 2: mit welchen Regeln dieser Bote prüft und baut.
            pruefer: Some(crate::werkzeug::pruefer_fassung()),
        },
        Anfrage::TresorInfo { ziel } => {
            if let Err(a) = ziel_pruefen(&ziel) {
                return a;
            }
            match schluessel::cred_info(&ziel) {
                Some(mut i) => Antwort::TresorInfo { vorhanden: true, gespeichert_am: i.saved_at.map(|t| t.to_rfc3339()), benutzer: i.username.take() },
                None => Antwort::TresorInfo { vorhanden: false, gespeichert_am: None, benutzer: None },
            }
        }
        Anfrage::TresorSchreiben { ziel, benutzer, wert } => {
            if let Err(a) = ziel_pruefen(&ziel) {
                return a;
            }
            let ergebnis = schluessel::cred_write(&ziel, &benutzer, wert.als_str().trim());
            drop(wert);
            match ergebnis {
                Ok(()) => Antwort::Ok,
                Err(e) if e.to_string().contains("zu lang") => Antwort::fehler("Der Wert ist zu lang."),
                Err(e) => {
                    tracing::warn!("Tresor: {ziel} nicht geschrieben: {e:#}");
                    Antwort::fehler("Der Windows-Tresor hat den Wert nicht angenommen. Einzelheiten stehen im Protokoll des Boten.")
                }
            }
        }
        Anfrage::Winget => match sysapps::winget_liste().await {
            Ok(l) => Antwort::Winget(l),
            Err(e) => Antwort::fehler(e),
        },
        Anfrage::Autostart => match tokio::task::spawn_blocking(sysapps::autostart).await {
            Ok(l) => Antwort::Autostart(l),
            Err(e) => Antwort::fehler(format!("Autostart nicht gelesen: {e}")),
        },
        Anfrage::Inventar { wurzel } => {
            let w = match wurzel_pruefen(wurzel.as_deref(), &k.profil) {
                Ok(w) => w,
                Err(a) => return a,
            };
            // Die Ausschlussliste kommt aus dem Inhalt; ohne sie zählt das
            // Inventar nicht (geschlossen scheitern, Auflage Archiv).
            let liste = k.ausschluss.clone();
            match tokio::task::spawn_blocking(move || inventory::compute_mit(&w, &inventory::ausschluss_laden(liste.as_deref()))).await {
                Ok(inv) => Antwort::Inventar(inv),
                Err(e) => Antwort::fehler(format!("Zählen abgebrochen: {e}")),
            }
        }
        Anfrage::Werkzeuge => Antwort::Werkzeuge(sysapps::werkzeuge().await),
        Anfrage::GitStatus { pfad, privat } => {
            if let Err(a) = repo_pruefen(&pfad, &k.quellen) {
                return a;
            }
            if let Err(a) = besitzer_pruefen(&pfad, &k.konto_sid) {
                return a;
            }
            Antwort::GitStatus(git::inspect(&pfad, None, privat).await)
        }
        Anfrage::RepoStempel { pfade, tief } => repo_stempel(pfade, tief, &k.quellen).await,
        Anfrage::Bau => {
            let Some(repo) = k.repo.clone().filter(|_| !k.quellen.is_empty()) else {
                return Antwort::fehler("Der Bote kennt kein Repo (ohne Einstellungen baut er nicht).");
            };
            // Umzug des Inhalts: alle Quellen (der erste Ordner für Wege ohne Slug).
            let erste = crate::quellen::Quellen::aus_liste(k.quellen.clone()).erste();
            let ort = super::bau::BauOrt { repo, projekte: erste, quellen: k.quellen.clone() };
            Antwort::Bau(super::bau::bauen(&ort, k.bau_zeitlimit).await)
        }
        // Zweig C: nur Zahlen, über eine zweite Verbindung neben dem Bau.
        Anfrage::Fortschritt => Antwort::Fortschritt(super::bau::FORTSCHRITT.jetzt()),
        Anfrage::Oeffnen { pfad } => {
            let k = k.clone();
            match tokio::task::spawn_blocking(move || super::oeffnen::oeffnen(&pfad, &k, client_sid.as_deref())).await {
                Ok(a) => a,
                Err(_) => Antwort::fehler("Öffnen wurde abgebrochen."),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kontext() -> Kontext {
        Kontext {
            konto_sid: "S-1-5-21-9".into(),
            gestartet: "jetzt".into(),
            profil: PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            quellen: vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel").join("projects")],
            ausschluss: None,
            repo: None,
            bau_zeitlimit: super::super::bau::ZEITLIMIT,
            verboten: Vec::new(),
            oeffner: std::sync::Arc::new(crate::bote::oeffnen::Verweigerer),
            dienst_sid: None,
            oeffnungen: Default::default(),
        }
    }

    /// Git-Stand nur für Repos aus den Projektseiten (im Beispiel nur
    /// `C:/gibt-es-nicht/beispiel-repo`, anders geschrieben); alles andere abgelehnt.
    #[tokio::test]
    async fn git_nur_fuer_repos_der_projektseiten() {
        for fremd in [env!("CARGO_MANIFEST_DIR"), "C:/Windows", "C:/gibt-es-nicht/beispiel-repo/.."] {
            let a = ausfuehren(Anfrage::GitStatus { pfad: fremd.into(), privat: false }, &kontext()).await;
            assert!(matches!(&a, Antwort::Fehler { satz } if satz.contains("nur für Repos")), "{fremd}: {a:?}");
        }
        let a = ausfuehren(Anfrage::GitStatus { pfad: r"c:\GIBT-ES-NICHT\beispiel-repo".into(), privat: false }, &kontext()).await;
        assert!(matches!(&a, Antwort::GitStatus(s) if s.error.as_deref() == Some("Ordner nicht gefunden")), "{a:?}");
        // Ohne Projektordner: keins.
        let ohne = Kontext { quellen: Vec::new(), ..kontext() };
        let a = ausfuehren(Anfrage::GitStatus { pfad: "C:/gibt-es-nicht/beispiel-repo".into(), privat: false }, &ohne).await;
        assert!(matches!(a, Antwort::Fehler { .. }), "{a:?}");
        // Umzug des Inhalts: Das Repo steht nur in einer zweiten Quelle; der
        // Bote kennt es trotzdem (vorher kannte er nur den ersten Ordner).
        let extra = std::env::temp_dir().join(format!("dashboard-bote-quellen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&extra);
        std::fs::create_dir_all(extra.join("beispiel-b")).unwrap();
        std::fs::write(
            extra.join("beispiel-b/projekt.toml"),
            "schema = 3\ntitle = \"B\"\nstatus = \"aktiv\"\nsummary = \"x\"\n\n[[repos]]\npath = \"C:/gibt-es-nicht/zweite-quelle\"\n",
        )
        .unwrap();
        let zwei = Kontext { quellen: vec![kontext().quellen[0].clone(), extra.clone()], ..kontext() };
        let a = ausfuehren(Anfrage::GitStatus { pfad: "C:/gibt-es-nicht/zweite-quelle".into(), privat: false }, &zwei).await;
        assert!(matches!(&a, Antwort::GitStatus(s) if s.error.as_deref() == Some("Ordner nicht gefunden")), "{a:?}");
        let a = ausfuehren(Anfrage::GitStatus { pfad: "C:/gibt-es-nicht/zweite-quelle".into(), privat: false }, &kontext()).await;
        assert!(matches!(&a, Antwort::Fehler { .. }), "nur eine Quelle: unbekannt: {a:?}");
        let _ = std::fs::remove_dir_all(&extra);
    }

    /// Das Inventar im Boten ohne Ausschlussliste: zählt nicht (geschlossen
    /// scheitern), nur ein Satz.
    #[tokio::test]
    async fn inventar_ohne_liste_zaehlt_nicht() {
        let a = ausfuehren(Anfrage::Inventar { wurzel: None }, &kontext()).await;
        match a {
            Antwort::Inventar(inv) => {
                assert!(inv.entries.is_empty() && inv.face.is_empty(), "{inv:?}");
                assert!(inv.hinweis.as_deref().is_some_and(|s| s.contains("Ausschlussliste")), "{inv:?}");
            }
            a => panic!("{a:?}"),
        }
    }

    /// Fingerabdrücke (Etappe 2d) nur für Repos aus den Projektseiten; andere
    /// fehlen in der Antwort, ohne Projektordner ein Fehler, zu viele auch.
    #[tokio::test]
    async fn fingerabdruecke_nur_fuer_repos_der_projektseiten() {
        let pfade = vec![r"c:\GIBT-ES-NICHT\beispiel-repo".to_string(), env!("CARGO_MANIFEST_DIR").to_string()];
        let a = ausfuehren(Anfrage::RepoStempel { pfade: pfade.clone(), tief: false }, &kontext()).await;
        match a {
            Antwort::RepoStempel(l) => {
                assert_eq!(l.len(), 1, "{l:?}");
                assert_eq!(l[0].pfad, pfade[0]);
            }
            a => panic!("{a:?}"),
        }
        let ohne = Kontext { quellen: Vec::new(), ..kontext() };
        assert!(matches!(ausfuehren(Anfrage::RepoStempel { pfade: pfade.clone(), tief: true }, &ohne).await, Antwort::Fehler { .. }));
        let viele = vec![pfade[0].clone(); crate::vertrag::bote::STEMPEL_HOECHSTENS + 1];
        assert!(matches!(ausfuehren(Anfrage::RepoStempel { pfade: viele, tief: false }, &kontext()).await, Antwort::Fehler { .. }));
    }

    /// Git nur in Ordnern, die dem Konto des Boten gehören: ein eigener
    /// Temp-Ordner ja, C:\Windows (TrustedInstaller) nicht, mit einer fremden
    /// SID auch der eigene Ordner nicht.
    #[cfg(windows)]
    #[test]
    fn git_nur_in_eigenen_ordnern() {
        let ich = crate::bote::win::eigene_sid().unwrap();
        let ordner = std::env::temp_dir().join(format!("dashboard-besitzer-{}", std::process::id()));
        std::fs::create_dir_all(&ordner).unwrap();
        let eigen = ordner.display().to_string();
        assert!(besitzer_pruefen(&eigen, &ich).is_ok(), "eigener Temp-Ordner");
        let fremd = besitzer_pruefen(&eigen, "S-1-5-21-1111111111-2222222222-3333333333-1001");
        assert!(matches!(&fremd, Err(Antwort::Fehler { satz }) if satz == FREMDES_REPO), "{fremd:?}");
        let windows = besitzer_pruefen(r"C:\Windows", &ich);
        assert!(matches!(&windows, Err(Antwort::Fehler { satz }) if satz == FREMDES_REPO), "{windows:?}");
        assert!(besitzer_pruefen(r"C:\gibt-es-nicht\beispiel-repo", &ich).is_ok(), "fehlt: Satz von git::inspect");
        let _ = std::fs::remove_dir(&ordner);
    }

    #[tokio::test]
    async fn hallo_und_fremde_ziele() {
        match ausfuehren(Anfrage::Hallo, &kontext()).await {
            Antwort::Hallo { konto_sid, sitzung, pruefer, .. } => {
                assert_eq!(konto_sid, "S-1-5-21-9");
                assert_eq!(pruefer, Some(crate::werkzeug::pruefer_fassung()), "Kleinkram 2: die Fassung des Prüfers");
                // Wer die Tests ausführt, läuft in einer Sitzung (unter Windows).
                #[cfg(windows)]
                assert!(sitzung.is_some(), "Sitzung des Boten fehlt");
                let _ = sitzung;
            }
            a => panic!("{a:?}"),
        }
        for ziel in ["fremd/x", "ranke/../x", "ranke/"] {
            let a = ausfuehren(Anfrage::TresorInfo { ziel: ziel.into() }, &kontext()).await;
            assert!(matches!(a, Antwort::Fehler { .. }), "{ziel}: {a:?}");
        }
        let a = ausfuehren(Anfrage::TresorInfo { ziel: "ranke/test-gibts-nicht-bote".into() }, &kontext()).await;
        assert!(matches!(a, Antwort::TresorInfo { vorhanden: false, .. }), "{a:?}");
    }

    #[tokio::test]
    async fn bau_nur_mit_repo() {
        let a = ausfuehren(Anfrage::Bau, &kontext()).await;
        assert!(matches!(&a, Antwort::Fehler { satz } if satz.contains("kennt kein Repo")), "{a:?}");
        assert_eq!(BAU_HOECHSTENS, Duration::from_secs(360));
    }

    #[tokio::test]
    async fn inventar_nur_im_profil() {
        let a = ausfuehren(Anfrage::Inventar { wurzel: Some("C:/Windows".into()) }, &kontext()).await;
        assert!(matches!(a, Antwort::Fehler { .. }), "{a:?}");
        let tests = format!("{}/tests/beispiel/data", env!("CARGO_MANIFEST_DIR"));
        let a = ausfuehren(Anfrage::Inventar { wurzel: Some(tests) }, &kontext()).await;
        assert!(matches!(a, Antwort::Inventar(_)), "{a:?}");
    }
}
