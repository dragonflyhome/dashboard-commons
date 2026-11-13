//! Zusammenbau: Zustand, Pfade, Router und die Schleusen (Zielbild, Kapitel c).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::Request;
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::get;
use tokio::sync::watch;
use tower::ServiceBuilder;
use tower_http::catch_panic::CatchPanicLayer;
use tower_http::csrf::{CsrfLayer, ProtectionError};
use tower_http::limit::RequestBodyLimitLayer;
use tracing::{error, info, warn};

use crate::api;
use crate::baumeister;
use crate::bote;
use crate::dienst::einstellungen::Einstellungen;
use crate::dienst::{self, Lauf, Modus};
use crate::fehler::AppError;
use crate::git;
use crate::inhalt::store::{Abgleich, ContentStore};
use crate::oberflaeche;
use crate::pc::{inventory, sysapps, system};
use crate::sicherheit;
use crate::speicher::{self, Speicher};

pub const DEFAULT_PORT: u16 = 8787;

#[derive(Clone)]
pub struct AppState {
    /// Der (erste) Projektordner; bei mehreren Inhaltsquellen der erste Ordner.
    pub projects_dir: Arc<PathBuf>,
    /// Etappe 4b: alle Inhaltsquellen (`DASHBOARD_PROJECTS_DIR` als Liste).
    /// Mit einer Quelle nur `projects_dir`.
    pub quellen: Arc<Vec<PathBuf>>,
    /// Die Entscheidungen (`/api/entscheidungen`): privater Inhalt, seit dem
    /// Umzug außerhalb des Code-Repos (`[inhalt].entscheidungen`).
    pub entscheidungen_dir: Arc<PathBuf>,
    pub data_file: Arc<PathBuf>,
    /// `data/schluessel.toml`: welche Passwörter die Sitzungen brauchen (nie Werte).
    pub keys_file: Arc<PathBuf>,
    /// Port, damit Adress- und Herkunftsprüfung wissen, wer „wir“ sind.
    pub port: u16,
    pub snapshot: system::SharedSnapshot,
    pub inventory: inventory::SharedInventory,
    pub sysapps: sysapps::SharedSysApps,
    /// Live-Kanal für Dateiänderungen (SSE).
    pub events: api::events::Sender,
    /// Live-Kanal für Schritte der Vorgänge (SSE-Ereignis `vorgang`).
    pub vorgang_events: api::events::VorgangSender,
    /// Wer gerade auf Vorgänge wartet (`/api/vorgaenge/warten`), je Slug.
    pub warteraum: Arc<api::vorgaenge::Warteraum>,
    /// Sitzung → Chat für den Herzschlag (`api::sitzungen`), nur im Arbeitsspeicher.
    pub sitzungen: Arc<api::sitzungen::Zuordnung>,
    /// Live-Kanal für den Bau der Oberfläche (SSE-Ereignis `bau`, Etappe 4v).
    pub bau_events: api::events::BauSender,
    /// Live-Kanal für neue Fassungen der Oberfläche (SSE-Ereignis `fassung`, Etappe 4b).
    pub fassung_events: api::events::FassungSender,
    /// Bau-Tempo, Zweig C: Stand des laufenden Baus (SSE-Ereignis `bau-fortschritt`, nur geschützt).
    pub bau_fortschritt: api::events::BauFortschrittSender,
    /// Live-Lücken: Ereignisse mit Bereich (`speicher`, `pc-benutzer`, `docs`),
    /// gebündelt. Der Datenspeicher meldet hier jedes Festschreiben.
    pub aenderungen: Arc<crate::aenderungen::Melder>,
    /// Nimmt Vorgänge der Art `bau` an und lässt den Boten bauen (`baumeister`).
    pub baumeister: Arc<baumeister::Baumeister>,
    /// Woher die Oberfläche kommt: `web/builds/<ordner>`, auf den der Zeiger
    /// `web/builds/aktuell` zeigt, sonst `web/build` (`WebOrt`).
    pub web: Arc<WebOrt>,
    /// Zeitlimit je Anfrage (Schleuse 5); Tests setzen es kürzer.
    pub zeitlimit: Duration,
    /// Content-Security-Policy der Oberfläche, aus den Prüfsummen der eingebetteten
    /// Skripte in index.html; wird neu berechnet, wenn sich die Datei ändert (Neubau).
    pub csp: Arc<CspCache>,
    /// Der Datenspeicher (Zielbild, Kapitel c3). `None`, wenn er sich beim
    /// Start nicht öffnen ließ; dann laufen Projekte, PC und Schlüssel weiter,
    /// und die Wege, die ihn brauchen, antworten mit 503 (`speicher()`).
    pub speicher: Option<Arc<Speicher>>,
    /// Warum der Datenspeicher fehlt: der Satz für den Inhaber aus `SpeicherFehler`.
    pub speicher_fehler: Option<Arc<str>>,
    /// Schema der Datei, wenn sie zu neu war (`SpeicherFehler::ZuNeu`); für
    /// `/api/betrieb` und Dienst.ps1 beim Zurückrollen.
    pub speicher_zu_neu: Option<u32>,
    /// Datei des Datenspeichers, damit /daten Ort und Sicherungsordner auch
    /// ohne offenen Speicher nennen kann.
    pub daten_datei: Arc<PathBuf>,
    /// Abschaltsignal: wird beim Beenden (Strg+C) `true`. Lange offene
    /// Antworten (Warter, SSE) enden dann sofort, sonst wartet das geordnete
    /// Beenden auf sie (ein Warter hält bis 1500 s, der SSE-Strom unbegrenzt).
    pub abschalten: watch::Receiver<bool>,
    /// Die Einstellungsdatei (`--konsole`/`--dienst --einstellungen`); `None`
    /// im Altweg mit Umgebungsvariablen und in Tests. Spätere Teile (Bote,
    /// 7b) lesen hier `inhaber_sid` und den Pfad des Boten.
    pub einstellungen: Option<Arc<Einstellungen>>,
    /// Der Bote (Etappe 7a1): `Some`, wenn Einstellungen da sind (Dienst und
    /// Konsole mit Einstellungen). Dann holen Benutzer-Daten der PC-Seite,
    /// Git-Stand und Tresor über ihn. `None` im Altweg und in Tests: alles
    /// direkt wie bisher.
    pub bote: Option<Arc<bote::client::Bote>>,
    /// Git-Stand aller Repos, im Hintergrund erneuert (über den Boten, wenn
    /// es ihn gibt); die Projekt-API liest nur von hier.
    pub git: Arc<git::Zwischenspeicher>,
    /// Inhalt aller Projekte und von `data/pc.toml` und `data/schwerpunkte.toml`
    /// im Arbeitsspeicher (Etappe 2d, `inhalt::store`). Die Lesewege der API,
    /// „Anstehend“, der Fristen-Lauf und die Backend-Module lesen nur von hier;
    /// der Beobachter hält ihn alle 2 s auf dem Stand der Platte.
    pub inhalt: Arc<ContentStore>,
    /// Wie und seit wann der Server läuft (`/api/betrieb`).
    pub lauf: Arc<Lauf>,
    /// Backend-Module der Projekte (Etappe 4b): Router, Hooks, Störungen.
    pub projekte: Arc<crate::projekte::Register>,
    /// Ausgang der Sitzungsabfrage für „Bote fehlt“ (Etappe A3, `/api/betrieb`).
    pub sitzung: crate::pc::sitzung::GeteilterWaechter,
    /// Schlüssel für die Ausweise nach Anmeldung mit Passkey (JWT, Etappe 7c).
    /// `None`, wenn sich die Schlüsseldatei nicht laden oder anlegen ließ; dann
    /// geht nur die Windows-Anmeldung (Grund im Protokoll).
    pub jwt: Option<Arc<crate::passkey::jwt::JwtSchluessel>>,
    /// Kopfleiste, Fassung 3: die Ereignisnummern für `/api/stand` (frei und
    /// geschützt); im Dienst zählt eine Aufgabe mit (`api::stand::zaehlen_starten`).
    pub zaehler: Arc<api::stand::Ereigniszaehler>,
    /// „Öffnen“ (Entscheidung lokal-oeffnen): die Grenze je Minute (in Tests auch Ersatz und Regeln).
    pub oeffnen: Arc<crate::oeffnen::Stand>,
    /// Der Sender dazu, damit Tests das Beenden auslösen können.
    #[cfg(test)]
    abschalter: Arc<watch::Sender<bool>>,
}

impl AppState {
    /// Zustand für den echten Betrieb: Pfade aus Umgebung oder Repo, Sammler starten.
    /// Lässt sich der Datenspeicher nicht öffnen (kaputt, zu neu, schon offen),
    /// steht der Grund im Protokoll und auf /daten; der Server startet trotzdem,
    /// nur ohne PC-Verlauf und ohne nächtliche Sicherung.
    ///
    /// Gibt den Sender des Abschaltsignals mit zurück: `main` sendet `true`,
    /// bevor es auf das Ende der offenen Antworten wartet.
    pub fn aus_umgebung(port: u16) -> anyhow::Result<(Self, watch::Sender<bool>)> {
        let alle = quellen();
        let projekte = alle.erste();
        Self::zusammenbauen(Quellen {
            quellen: alle.wurzeln(),
            port,
            daten: daten_datei(),
            sicherung: None,
            pc: data_file(&projekte),
            schluessel: keys_file(&projekte),
            entscheidungen: entscheidungen_dir(&projekte),
            projekte,
            web: web_dir(),
            web_kopie: None,
            einstellungen: None,
            modus: Modus::Umgebung,
        })
    }

    /// Zustand aus der Einstellungsdatei (`dienst.toml`): alle Pfade von dort,
    /// `data/pc.toml` und `data/schluessel.toml` unter `<repo>/data`. Modus
    /// `konsole`; der Dienst setzt ihn mit `mit_modus` um.
    pub fn aus_einstellungen(e: &Einstellungen) -> anyhow::Result<(Self, watch::Sender<bool>)> {
        // Umzug des Inhalts: alle Quellen aus [inhalt] (oder DASHBOARD_PROJECTS_DIR).
        let quellen = e.projekt_quellen();
        let projekte = crate::quellen::Quellen::aus_liste(quellen.clone()).erste();
        Self::zusammenbauen(Quellen {
            port: e.port,
            daten: e.pfade.daten.clone(),
            sicherung: Some(e.pfade.sicherung.clone()),
            pc: e.pc_datei(),
            schluessel: e.schluessel_datei(),
            entscheidungen: e.inhalt.entscheidungen.clone(),
            quellen,
            projekte,
            web: e.pfade.web.clone(),
            web_kopie: web_kopie_neben_programm(),
            einstellungen: Some(Arc::new(e.clone())),
            modus: Modus::Konsole,
        })
    }

    /// Derselbe Zustand mit anderem Modus (Startzeit bleibt).
    pub fn mit_modus(mut self, modus: Modus) -> Self {
        self.lauf = Arc::new(Lauf { modus, gestartet: self.lauf.gestartet });
        self
    }

    /// Gemeinsamer Zusammenbau beider Wege.
    fn zusammenbauen(q: Quellen) -> anyhow::Result<(Self, watch::Sender<bool>)> {
        let Quellen {
            port,
            daten,
            sicherung,
            pc: data_file,
            schluessel: keys_file,
            entscheidungen,
            projekte: dir,
            quellen: quellen_pfade,
            web,
            web_kopie,
            einstellungen,
            modus,
        } = q;
        let snapshot: system::SharedSnapshot = Default::default();
        system::spawn_collector(snapshot.clone());
        let geoeffnet = match &sicherung {
            Some(ordner) => Speicher::oeffnen_mit_sicherung(&daten, ordner),
            None => Speicher::oeffnen(&daten),
        };
        let aenderungen = crate::aenderungen::Melder::neu();
        let (speicher, speicher_fehler, speicher_zu_neu) = match geoeffnet {
            Ok(s) => {
                let s = Arc::new(s);
                s.melder_setzen(aenderungen.clone());
                info!("Datenspeicher: {} (Schema {})", daten.display(), s.schema().unwrap_or(0));
                speicher::pc_verlauf::spawn_sammler(s.clone(), snapshot.clone());
                speicher::sicherung::spawn_naechtlich(s.clone());
                (Some(s), None, None)
            }
            Err(e) => {
                error!("Datenspeicher {} nicht geöffnet: {e} ({}). Der Server läuft ohne ihn weiter.", daten.display(), e.einzelheiten());
                let zu_neu = match &e {
                    speicher::SpeicherFehler::ZuNeu { schema, .. } => Some(*schema),
                    _ => None,
                };
                (None, Some(Arc::from(e.to_string())), zu_neu)
            }
        };
        info!("Projektseiten aus {}", dir.display());
        let bote = einstellungen.as_ref().map(|e| Arc::new(bote::client::Bote::neu(e)));
        let inv: inventory::SharedInventory = Default::default();
        let apps: sysapps::SharedSysApps = Default::default();
        match &bote {
            Some(b) => {
                info!("Benutzer-Daten, Git-Stand und Tresor über den Boten (Pipe {})", einstellungen.as_ref().map(|e| e.bote.pipe.as_str()).unwrap_or(""));
                // Zwischenspeicher der Ordnergrößen neben dem Datenspeicher, nicht
                // im Profil des Dienstkontos (das hat keins, das jemand ansieht).
                let cache = einstellungen.as_ref().map(|e| e.inventar_datei()).unwrap_or_else(inventory::cache_file);
                bote::sammler::spawn_lebenszeichen(b.clone());
                let ausschluss = einstellungen.as_ref().map(|e| e.ausschluss_datei()).unwrap_or_default();
                bote::sammler::spawn_inventar(inv.clone(), b.clone(), cache, ausschluss, aenderungen.clone());
                bote::sammler::spawn_sysapps(apps.clone(), b.clone(), speicher.clone(), aenderungen.clone());
            }
            None => {
                // Die Ausschlussliste liegt im Daten-Ordner des Inhalts; fehlt
                // sie, zählt das Inventar nicht (geschlossen scheitern).
                let ausschluss = data_file.parent().map(Path::to_path_buf).unwrap_or_default().join(crate::dienst::einstellungen::AUSSCHLUSS_DATEI);
                inventory::spawn(inv.clone(), inventory::home_dir(), inventory::cache_file(), ausschluss, aenderungen.clone());
                sysapps::spawn(apps.clone(), speicher.clone(), aenderungen.clone());
            }
        }
        info!("PC-Eckdaten aus {}", data_file.display());
        info!("Schlüssel-Anforderungen aus {}", keys_file.display());
        let web = Arc::new(WebOrt::neu(web).mit_kopie(web_kopie));
        let web_jetzt = web.ordner();
        if web_jetzt.join("index.html").is_file() {
            info!("Oberfläche aus {}", web_jetzt.display());
        } else {
            warn!("Oberfläche fehlt ({}): erst in web\\ `npm run build` ausführen", web_jetzt.display());
        }
        let (abschalter, abschalten) = watch::channel(false);
        // Inhalt (Etappe 2d): alle Projekte einmal lesen, dann hält der
        // Beobachter ihn auf dem Stand.
        let schwerpunkte = keys_file.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("data")).join("schwerpunkte.toml");
        let alle_quellen =
            if quellen_pfade.len() > 1 { crate::quellen::Quellen::aus_liste(quellen_pfade.clone()) } else { crate::quellen::Quellen::ein_ordner(dir.clone()) };
        if !alle_quellen.einzeln() {
            info!("Inhaltsquellen: {}", alle_quellen.als_text());
        }
        for satz in alle_quellen.unlesbar_saetze().iter().chain(&alle_quellen.version) {
            warn!("Inhaltsquellen: {satz}");
        }
        let inhalt = Arc::new(ContentStore::laden_aus(&alle_quellen, data_file.clone(), schwerpunkte, Abgleich::Beobachter));
        // Git-Stand: erster Lauf sofort, dann alle 60 s und nach Änderungen;
        // ein geänderter Stand geht an den ContentStore (Ereignis `projekt`
        // oder, bei vielen zugleich, `inhalt`).
        let events = api::events::channel();
        let git = Arc::new(
            git::Zwischenspeicher::neu(alle_quellen.wurzeln(), bote.clone())
                .mit_inhalt(inhalt.clone())
                .mit_melder(api::events::git_melder(inhalt.clone()))
                .mit_abschaltung(abschalten.clone()),
        );
        git.clone().starten();
        if let Some(b) = &bote {
            // Beim Dienststart ist der Bote noch nicht da: sobald er erreichbar
            // wird (bis dahin fragt es je Takt selbst mit Hallo), alle Repos
            // sofort erneuern statt erst beim nächsten Takt.
            git.clone().bote_beobachten(b.clone());
        }
        let aus = {
            let mut rx = abschalten.clone();
            async move {
                if rx.wait_for(|&aus| aus).await.is_err() {
                    std::future::pending::<()>().await;
                }
            }
        };
        // Live-Lücken: die Entscheidungen (privat), das Zielbild (Code-Repo)
        // und assets/ aller Quellen.
        let weitere = api::events::WeitereWachen {
            melder: aenderungen.clone(),
            entscheidungen: entscheidungen.clone(),
            zielbild: api::system_seiten::docs_ordner_aus(einstellungen.as_deref(), &dir, web.build()).join("zielbild"),
            quellen: if quellen_pfade.len() > 1 { quellen_pfade.clone() } else { vec![dir.clone()] },
        };
        api::events::spawn_watcher(events.clone(), inhalt.clone(), git.clone(), bote.clone(), weitere, aus);
        // Etappe 7c: Schlüssel der Ausweise; beim ersten Start mit enger Freigabe angelegt.
        let jwt_datei = match &einstellungen {
            Some(e) => e.jwt_schluessel_datei(),
            None => jwt_schluessel_datei(&daten),
        };
        let jwt = match crate::passkey::geheim::laden_oder_anlegen(&jwt_datei) {
            Ok(k) => {
                info!("Schlüssel für Ausweise (Passkey): {}", jwt_datei.display());
                Some(Arc::new(k))
            }
            Err(e) => {
                error!("Anmeldung mit Passkey aus: {e}");
                None
            }
        };
        let csp = Arc::new(CspCache::neu(web.clone()));
        let mut state = Self {
            quellen: Arc::new(if quellen_pfade.len() > 1 { quellen_pfade } else { vec![dir.clone()] }),
            projects_dir: Arc::new(dir),
            entscheidungen_dir: Arc::new(entscheidungen),
            data_file: Arc::new(data_file),
            keys_file: Arc::new(keys_file),
            port,
            snapshot,
            inventory: inv,
            sysapps: apps,
            events,
            vorgang_events: api::events::vorgang_kanal(),
            bau_events: api::events::bau_kanal(),
            fassung_events: api::events::fassung_kanal(),
            bau_fortschritt: api::events::bau_fortschritt_kanal(),
            aenderungen,
            baumeister: Arc::new(baumeister::Baumeister::neu()),
            warteraum: Default::default(),
            sitzungen: Default::default(),
            web,
            zeitlimit: Duration::from_secs(10),
            csp,
            speicher,
            speicher_fehler,
            speicher_zu_neu,
            daten_datei: Arc::new(daten),
            abschalten,
            einstellungen,
            bote,
            git,
            inhalt,
            lauf: Arc::new(Lauf::jetzt(modus)),
            sitzung: Default::default(),
            jwt,
            zaehler: api::stand::neu(),
            oeffnen: Default::default(),
            projekte: Arc::new(crate::projekte::Register::leer()),
            #[cfg(test)]
            abschalter: Arc::new(abschalter.clone()),
        };
        // Backend-Module der Projekte (Etappe 4b): Kontext, Tabellen, Hooks.
        let register = crate::projekte::Register::fuer_zustand(&state, crate::projekte::module());
        if let Some(s) = &state.speicher {
            register.tabellen_einrichten(s);
        }
        state.projekte = Arc::new(register);
        state.projekte.starten(&state);
        // [zusammenbau]: Rechenteile ohne angemeldetes Modul (allgemeines
        // dashboard.exe) nennt die Prüfung je Projekt; hier ein Satz im Protokoll.
        let ohne = rechenteile_ohne_modul(&state);
        if !ohne.is_empty() {
            warn!("Rechenteile nicht eingebaut ({}): {}", ohne.join(", "), crate::projekte::SATZ_OHNE_ZUSAMMENBAU);
        }
        // Kopfleiste, Fassung 3: Ereignisnummern für die Aktualitätsprüfung (/api/stand).
        api::stand::zaehlen_starten(&state);
        // Live-Lücken: Bote da oder weg als `pc-benutzer` mit Bereich `bote`;
        // der Tresor-Stand jede Minute (in der Anmeldeinformationsverwaltung
        // gelöschte Schlüssel). Beide enden beim Abschaltsignal.
        if let Some(b) = &state.bote {
            crate::aenderungen::spawn_bote_wache(b.clone(), state.aenderungen.clone(), state.abschaltung());
        }
        api::schluessel::spawn_tresor_wache(state.clone());
        // Etappe 4b: der Baumeister (eine Warteschlange für Beobachter und
        // Bau-Vorgänge, auch solche, die vor einem Neustart liegen blieben).
        // Gebaut wird über den Boten; ohne Boten nur ohne Dienst (Testkopie oder
        // lokaler Start, nie im Dienst) mit DASHBOARD_BAUMEISTER=direkt (im
        // eigenen Prozess). Ein Neustart baut nicht: Die Oberfläche kommt aus web/builds/aktuell (`WebOrt`).
        let direkt = state.bote.is_none() && baumeister::direkt_gewollt();
        let bauen = if direkt {
            let web = state.web.build().parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("web"));
            let repo = web.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
            info!("Baumeister: baut direkt (ohne Boten) in {}", repo.display());
            let ort = crate::bote::bau::BauOrt { repo, projekte: state.projects_dir.to_path_buf(), quellen: state.quellen.to_vec() };
            Some(baumeister::direkt(ort))
        } else if state.bote.is_some() || state.speicher.is_some() {
            Some(baumeister::ueber_boten(state.bote.clone()))
        } else {
            None
        };
        if let Some(b) = bauen {
            baumeister::starten(state.clone(), b);
        }
        if state.bote.is_some() || direkt {
            baumeister::beobachter_starten(state.clone());
        }
        if state.speicher.is_some() {
            // Aufgaben für den Inhaber: jede Minute abgelaufene Links löschen (Etappe 5a).
            api::aufgaben::spawn_ablauf(state.clone());
            // Meldungen aus Ereignissen (Etappe A3): Prüfung, Fristen, Bote.
            crate::meldungen::spawn(state.clone());
        }
        Ok((state, abschalter))
    }

    /// `data/schwerpunkte.toml` (Etappe A1): im selben Ordner wie
    /// `schluessel.toml`, also `<repo>/data/` (im Dienst aus `repo` in
    /// dienst.toml, in Tests `tests/beispiel/data/`). Nur gelesen.
    pub fn schwerpunkte_datei(&self) -> PathBuf {
        self.keys_file.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("data")).join("schwerpunkte.toml")
    }

    /// Sicherungsordner: aus den Einstellungen, sonst `sicherung\` neben der Datei.
    pub fn sicherungs_ordner(&self) -> PathBuf {
        match &self.einstellungen {
            Some(e) => e.pfade.sicherung.clone(),
            None => speicher::sicherungs_ordner_fuer(&self.daten_datei),
        }
    }

    /// Wird fertig, sobald das Abschaltsignal gesendet ist (sofort, wenn es
    /// schon gesendet war). Ist der Sender weg, ohne gesendet zu haben, wird
    /// sie nie fertig.
    pub fn abschaltung(&self) -> impl std::future::Future<Output = ()> + Send + 'static + use<> {
        let mut rx = self.abschalten.clone();
        async move {
            if rx.wait_for(|&aus| aus).await.is_err() {
                std::future::pending::<()>().await;
            }
        }
    }

    /// Für Tests: derselbe Zustand mit einer eigenen gebauten Oberfläche im
    /// Ordner `build` (ohne `builds/` daneben), unabhängig von `web/` des Repos.
    #[cfg(test)]
    pub fn mit_web(mut self, build: PathBuf) -> Self {
        self.web = Arc::new(WebOrt::neu(build));
        self.csp = Arc::new(CspCache::neu(self.web.clone()));
        self
    }

    /// Für Tests: derselbe Zustand mit einem anderen Projektordner (und einem
    /// eigenen ContentStore dafür, der bei jedem Zugriff abgleicht).
    #[cfg(test)]
    pub fn mit_projekten(mut self, projekte: PathBuf) -> Self {
        self.inhalt = Arc::new(ContentStore::neu(projekte.clone(), self.data_file.to_path_buf(), self.schwerpunkte_datei(), Abgleich::BeiJedemZugriff));
        self.quellen = Arc::new(vec![projekte.clone()]);
        self.projects_dir = Arc::new(projekte);
        self
    }

    /// Für Tests: derselbe Zustand mit mehreren Inhaltsquellen (ContentStore
    /// über alle, der bei jedem Zugriff abgleicht).
    #[cfg(test)]
    pub fn mit_quellen(mut self, quellen: Vec<PathBuf>) -> Self {
        let q = crate::quellen::Quellen::aus_liste(quellen.clone());
        self.inhalt = Arc::new(ContentStore::laden_aus(&q, self.data_file.to_path_buf(), self.schwerpunkte_datei(), Abgleich::BeiJedemZugriff));
        self.projects_dir = Arc::new(q.erste());
        self.quellen = Arc::new(quellen);
        self
    }

    /// Die Inhaltsquellen, frisch von der Platte aufgelöst (Etappe 4b).
    pub fn alle_quellen(&self) -> crate::quellen::Quellen {
        crate::quellen::Quellen::aus_liste(self.quellen.to_vec())
    }

    /// Der Ordner mit `pc.toml`, `schluessel.toml` und der Ausschlussliste
    /// des Inventars (Daten-Ordner des Inhalts).
    pub fn daten_ordner(&self) -> PathBuf {
        self.data_file.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("data"))
    }

    /// Die Slugs der archivierten Projekte (Entscheidung archiv-status),
    /// aus dem ContentStore. Ihre Einträge im Datenspeicher fehlen in den Listen
    /// ohne `projekt` und in allen Zählern.
    pub fn archivierte(&self) -> std::collections::BTreeSet<String> {
        self.inhalt.bestand().archivierte()
    }

    /// Der Elternordner eines Projekts: bei einer Quelle `projects_dir`, sonst
    /// die Quelle, in der es (zuerst) liegt.
    pub fn projekt_eltern(&self, slug: &str) -> PathBuf {
        if self.quellen.len() <= 1 {
            return self.projects_dir.to_path_buf();
        }
        self.alle_quellen().eltern(slug).map(Path::to_path_buf).unwrap_or_else(|| self.projects_dir.to_path_buf())
    }

    /// Für Tests: das Beenden auslösen, als wäre Strg+C gedrückt.
    #[cfg(test)]
    pub fn abschalten_ausloesen(&self) {
        self.abschalter.send_replace(true);
    }

    /// Der offene Datenspeicher, sonst 503 mit dem Satz aus `speicher_satz()`.
    pub fn speicher(&self) -> Result<Arc<Speicher>, AppError> {
        self.speicher.clone().ok_or_else(|| AppError::nicht_verfuegbar(self.speicher_satz()))
    }

    /// Satz für den Inhaber, wenn der Datenspeicher fehlt: nennt den Sicherungsordner
    /// (als `%USERPROFILE%\…`) und den ursprünglichen Grund.
    pub fn speicher_satz(&self) -> String {
        let ordner = speicher::anzeige_pfad(&self.sicherungs_ordner());
        let grund = self.speicher_fehler.as_deref().unwrap_or("Er wurde beim Start nicht geöffnet.");
        format!("Der Datenspeicher lässt sich nicht öffnen. Die letzten Sicherungen liegen in {ordner}. {grund}")
    }

    /// Zustand für Tests: ein fester Beispiel-Ordner, keine Sammler, keine echten `projects/`.
    #[cfg(test)]
    pub fn fuer_tests(beispiel: &Path) -> Self {
        let speicher = Arc::new(Speicher::temporaer());
        let ohne = Self::fuer_tests_ohne_speicher(beispiel, "");
        speicher.melder_setzen(ohne.aenderungen.clone());
        Self { daten_datei: Arc::new(speicher.pfad().to_path_buf()), speicher: Some(speicher), speicher_fehler: None, ..ohne }
    }

    /// Wie `fuer_tests`, mit eigenem Port (für die Adressprüfung) und dem
    /// Sender des Abschaltsignals, für `dienst::betreiben` in Tests.
    #[cfg(test)]
    pub fn fuer_tests_mit_abschalter(beispiel: &Path, port: u16) -> (Self, watch::Sender<bool>) {
        let state = Self { port, ..Self::fuer_tests(beispiel) };
        let sender = (*state.abschalter).clone();
        (state, sender)
    }

    /// Zustand für Tests ohne Datenspeicher, als hätte er sich beim Start mit
    /// dem Satz `satz` nicht öffnen lassen. Die Datei liegt im Temp-Ordner und
    /// wird nie angelegt.
    #[cfg(test)]
    pub fn fuer_tests_ohne_speicher(beispiel: &Path, satz: &str) -> Self {
        let projects = beispiel.join("projects");
        let web = Arc::new(WebOrt::neu(web_dir()));
        let daten = crate::speicher::test_pfad();
        let (abschalter, abschalten) = watch::channel(false);
        Self {
            data_file: Arc::new(beispiel.join("data").join("pc.toml")),
            keys_file: Arc::new(beispiel.join("data").join("schluessel.toml")),
            port: DEFAULT_PORT,
            quellen: Arc::new(vec![projects.clone()]),
            projects_dir: Arc::new(projects),
            entscheidungen_dir: Arc::new(beispiel.join("docs").join("entscheidungen")),
            snapshot: Default::default(),
            inventory: Default::default(),
            sysapps: Default::default(),
            events: api::events::channel(),
            vorgang_events: api::events::vorgang_kanal(),
            bau_events: api::events::bau_kanal(),
            fassung_events: api::events::fassung_kanal(),
            bau_fortschritt: api::events::bau_fortschritt_kanal(),
            aenderungen: crate::aenderungen::Melder::neu(),
            baumeister: Arc::new(baumeister::Baumeister::neu()),
            warteraum: Default::default(),
            sitzungen: Default::default(),
            csp: Arc::new(CspCache::neu(web.clone())),
            web,
            zeitlimit: Duration::from_secs(10),
            speicher: None,
            speicher_fehler: Some(Arc::from(satz)),
            speicher_zu_neu: None,
            daten_datei: Arc::new(daten),
            abschalten,
            einstellungen: None,
            bote: None,
            // Leer und ohne Hintergrundaufgabe; Tests füllen ihn selbst.
            git: Arc::new(git::Zwischenspeicher::neu(vec![beispiel.join("projects")], None)),
            // Ohne Beobachter: Jeder Zugriff gleicht erst mit der Platte ab.
            inhalt: Arc::new(ContentStore::neu(
                beispiel.join("projects"),
                beispiel.join("data").join("pc.toml"),
                beispiel.join("data").join("schwerpunkte.toml"),
                Abgleich::BeiJedemZugriff,
            )),
            lauf: Arc::new(Lauf::jetzt(Modus::Umgebung)),
            sitzung: Default::default(),
            jwt: Some(Arc::new(crate::passkey::jwt::JwtSchluessel::zufaellig())),
            zaehler: api::stand::neu(),
            oeffnen: Default::default(),
            projekte: Arc::new(crate::projekte::Register::leer()),
            abschalter: Arc::new(abschalter),
        }
    }
}

/// Das Beispiel-Repo `tests/beispiel` (für Tests in anderen Modulen).
#[cfg(test)]
pub fn beispiel_fuer_tests() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel")
}

/// Für Tests: der Router, als käme jede Anfrage über eine Verbindung, die
/// schon als Inhaber angemeldet ist (eigene Verbindungsnummer, im Vorrat als
/// Inhaber gemerkt). Für alle Tests, die nicht die Anmeldung selbst prüfen.
#[cfg(test)]
pub fn router_angemeldet(state: AppState) -> Router {
    let v = crate::anmeldung::verbindung::Verbindung::neu();
    #[cfg(windows)]
    crate::anmeldung::verbindung::merken(v.nr, crate::anmeldung::WindowsKonto::test(true));
    router(state).layer(axum::Extension(axum::extract::ConnectInfo(v)))
}

/// Woher `zusammenbauen` seine Pfade nimmt.
struct Quellen {
    port: u16,
    daten: PathBuf,
    /// `None`: `sicherung\` neben der Datei des Datenspeichers.
    sicherung: Option<PathBuf>,
    pc: PathBuf,
    schluessel: PathBuf,
    /// Die Entscheidungen (privater Inhalt).
    entscheidungen: PathBuf,
    projekte: PathBuf,
    /// Alle Inhaltsquellen (Etappe 4b); mit einer Quelle nur `projekte`.
    quellen: Vec<PathBuf>,
    web: PathBuf,
    /// Die eingespielte Kopie der Oberfläche (im Dienst `bin\web`), sonst None.
    web_kopie: Option<PathBuf>,
    einstellungen: Option<Arc<Einstellungen>>,
    modus: Modus,
}

/// Woher die Oberfläche kommt (Etappe 4v). Ein Bau über den Boten legt einen
/// neuen Ordner `web/builds/<JJJJMMTT-hhmmss>/` an und schreibt dessen Namen
/// in die Textdatei `web/builds/aktuell` (`bote::bau`). Gilt der Zeiger,
/// liefert der Dienst aus diesem Ordner aus, sonst aus `web/build` wie bisher.
///
/// Gültig heißt: nur ein Name ohne Pfadteile, der Ordner existiert, ist keine
/// Verknüpfung und enthält index.html. Geprüft wird nur, wenn sich die Datei
/// `aktuell` ändert (Änderungszeit und Größe, so wie `CspCache` es mit
/// index.html macht); sonst kostet eine Anfrage nur einen Blick auf ihre
/// Eckdaten. So gilt ein grüner Bau sofort, ohne Neustart des Dienstes.
///
/// Ohne gültigen Zeiger kommt die Oberfläche aus der Kopie (`kopie`, im Dienst
/// `bin\web` neben dem Programm, siehe [`web_kopie_neben_programm`]), die
/// Dienst.ps1 -Neustart zusammen mit den Programmen einspielt; nur wenn sie
/// fehlt, aus `web/build`. So ändert ein `npm run build` im Repo die laufende
/// Seite nicht.
///
/// Mit Kopie gilt ein Zeiger nur, wenn sein Bau denselben Grundbau hat wie
/// die Kopie ([`grundbau`]): Der Bote baut aus dem aktuellen `web/src` des
/// Repos, und ein neuerer Grundbau kann Wege brauchen, die der laufende Dienst
/// noch nicht kennt. Er wartet dann auf Dienst.ps1 -Neustart ([`WebOrt::wartender_bau`]);
/// bis dahin bleibt der letzte gültige Bau des Boten, ohne ihn die Kopie.
pub struct WebOrt {
    /// `web/build`: Rückfall ohne Kopie; daneben liegen `builds` und die grünen Kopien.
    build: PathBuf,
    /// `web/builds`: Bau-Ordner und Zeiger.
    builds: PathBuf,
    /// Die eingespielte Kopie der Oberfläche (`None`: keine, etwa beim Start aus dem Repo).
    kopie: Option<PathBuf>,
    stand: std::sync::RwLock<Option<ZeigerStand>>,
}

/// Zuletzt gesehener Zeiger: sein Inhalt (`None`: fehlt oder zu groß) und
/// der gewählte Ordner.
struct ZeigerStand {
    stempel: Option<Vec<u8>>,
    ordner: PathBuf,
}

/// Höchstgröße der Datei `aktuell`; was größer ist, gilt nicht.
const ZEIGER_HOECHSTENS: u64 = 256;

impl WebOrt {
    /// Zu `web/build`; die Bau-Ordner liegen daneben in `web/builds`.
    pub fn neu(build: PathBuf) -> Self {
        let builds = build.parent().map(|p| p.join("builds")).unwrap_or_else(|| PathBuf::from("builds"));
        Self::mit_builds(build, builds)
    }

    pub fn mit_builds(build: PathBuf, builds: PathBuf) -> Self {
        Self { build, builds, kopie: None, stand: std::sync::RwLock::new(None) }
    }

    /// Derselbe Ort mit der eingespielten Kopie `kopie` (ohne gültigen Zeiger
    /// wird aus ihr ausgeliefert, solange sie index.html hat).
    pub fn mit_kopie(self, kopie: Option<PathBuf>) -> Self {
        Self { kopie, ..self }
    }

    /// `web/build`.
    pub fn build(&self) -> &Path {
        &self.build
    }

    /// Die eingespielte Kopie, wenn sie gesetzt ist und index.html hat.
    pub fn kopie(&self) -> Option<&Path> {
        self.kopie.as_deref().filter(|k| k.join("index.html").is_file())
    }

    /// Ohne gültigen Zeiger: die Kopie, sonst `web/build`.
    fn ruheort(&self) -> PathBuf {
        if let Some(k) = self.kopie() {
            return k.to_path_buf();
        }
        if let Some(k) = &self.kopie {
            warn!("Die Kopie der Oberfläche fehlt ({}); die Oberfläche kommt aus {}", k.display(), self.build.display());
        }
        self.build.clone()
    }

    /// Ein gültiger Bau-Ordner `ordner` gilt nur mit demselben Grundbau wie die
    /// Kopie; ohne Kopie immer. Fehlt die Kennung auf einer Seite, gilt er nicht.
    fn passt_zur_kopie(&self, ordner: &Path) -> Result<(), String> {
        let Some(k) = self.kopie() else { return Ok(()) };
        match (grundbau(k), grundbau(ordner)) {
            (Some(a), Some(b)) if a == b => Ok(()),
            (a, b) => Err(format!(
                "der Bau hat einen anderen Grundbau ({}) als die eingespielte Oberfläche ({})",
                b.as_deref().unwrap_or("ohne Kennung"),
                a.as_deref().unwrap_or("ohne Kennung")
            )),
        }
    }

    /// Der zuletzt ausgelieferte Bau des Boten, solange er noch index.html hat
    /// und zur Kopie passt; sonst None. Nach einem Start des Dienstes ist
    /// keiner gemerkt (dann die Kopie).
    fn letzter_bau(&self) -> Option<PathBuf> {
        let s = self.stand.read().ok()?;
        let o = &s.as_ref()?.ordner;
        let gilt = o.parent() == Some(self.builds.as_path()) && o.join("index.html").is_file() && self.passt_zur_kopie(o).is_ok();
        gilt.then(|| o.clone())
    }

    /// Satz für die Störungen in `/api/betrieb`: Ein gültiger Bau des Boten
    /// gilt nicht, weil sein Grundbau nicht zur eingespielten Oberfläche passt.
    pub fn wartender_bau(&self) -> Option<String> {
        let ordner = zeiger_ordner(&self.builds).ok()?;
        self.passt_zur_kopie(&ordner).err().map(|_| SATZ_BAU_WARTET.to_string())
    }

    /// Der Inhalt der Datei `aktuell` (höchstens `ZEIGER_HOECHSTENS` + 1 Bytes
    /// gelesen). Änderungszeit und Größe reichten nicht: Die Ordnernamen haben
    /// immer dieselbe Länge, und zwei Umstellungen kurz nacheinander können
    /// dieselbe Zeit tragen. Die Datei ist winzig, das Lesen je Anfrage kostet
    /// kaum mehr als ihre Eckdaten.
    fn stempel(&self) -> Option<Vec<u8>> {
        use std::io::Read;
        let f = std::fs::File::open(self.builds.join(bote::bau::ZEIGER)).ok()?;
        let mut inhalt = Vec::new();
        f.take(ZEIGER_HOECHSTENS + 1).read_to_end(&mut inhalt).ok()?;
        Some(inhalt)
    }

    /// Etappe 4b: Der Baumeister hat grün gebaut und die Datei `aktuell`
    /// umgestellt (über den Boten); hier stellt er den Zeiger im Speicher um,
    /// sofort und in einem Schritt. Der Name muss dasselbe sein, was in der
    /// Datei steht, und der Ordner gültig (wie bei `zeiger_ordner`); sonst
    /// bleibt alles, wie es war, und der Grund kommt zurück.
    pub fn umstellen(&self, name: &str) -> Result<PathBuf, String> {
        let ordner = zeiger_ordner(&self.builds)?;
        if ordner.file_name().and_then(|n| n.to_str()) != Some(name) {
            return Err(format!("web\\builds\\aktuell nennt nicht {name}"));
        }
        self.passt_zur_kopie(&ordner)?;
        let stempel = self.stempel();
        let mut s = self.stand.write().map_err(|_| "Sperre vergiftet".to_string())?;
        *s = Some(ZeigerStand { stempel, ordner: ordner.clone() });
        Ok(ordner)
    }

    /// Der Ordner, aus dem jetzt ausgeliefert wird.
    pub fn ordner(&self) -> PathBuf {
        let stempel = self.stempel();
        // Der gemerkte Ordner kann ohne Zeigerwechsel verschwinden (etwa der letzte gültige Bau beim Aufräumen).
        if let Ok(s) = self.stand.read()
            && let Some(s) = s.as_ref().filter(|s| s.stempel == stempel && s.ordner.join("index.html").is_file())
        {
            return s.ordner.clone();
        }
        let ordner = match stempel {
            Some(_) => match zeiger_ordner(&self.builds) {
                Ok(o) => self.passt_zur_kopie(&o).map(|()| o).unwrap_or_else(|grund| {
                    // Wartet ein neuerer Bau auf den Neustart, bleibt der letzte gültige des Boten.
                    let ort = self.letzter_bau().unwrap_or_else(|| self.ruheort());
                    warn!("web\\builds\\aktuell gilt noch nicht ({grund}); die Oberfläche kommt aus {}", ort.display());
                    ort
                }),
                Err(grund) => {
                    let ort = self.ruheort();
                    warn!("web\\builds\\aktuell gilt nicht ({grund}); die Oberfläche kommt aus {}", ort.display());
                    ort
                }
            },
            None => self.ruheort(),
        };
        if let Ok(mut s) = self.stand.write() {
            if s.as_ref().is_some_and(|alt| alt.ordner != ordner) {
                info!("Oberfläche jetzt aus {}", ordner.display());
            }
            *s = Some(ZeigerStand { stempel, ordner: ordner.clone() });
        }
        ordner
    }
}

/// Störung, solange ein Bau des Boten auf den Neustart wartet.
pub const SATZ_BAU_WARTET: &str = "Ein neuerer Bau der Oberfläche passt nicht zum laufenden Dienst und gilt erst nach Dienst.ps1 -Neustart; bis dahin bleibt die zuletzt gültige Oberfläche, auch für geänderte Projektseiten.";

/// Die Kennung des Grundbaus eines Bau-Ordners: in `_app/version.json` der
/// Teil vor dem ersten `-`. web/vite.config.ts setzt die Fassung auf
/// `<Abdruck von web/src ohne Bauliste>-<Bauzeit>`; ältere Bauten haben nur
/// die Bauzeit und keinen `-`, also keine Kennung.
pub fn grundbau(ordner: &Path) -> Option<String> {
    let text = std::fs::read_to_string(ordner.join("_app").join("version.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let (kennung, _) = v.get("version")?.as_str()?.split_once('-')?;
    (!kennung.is_empty()).then(|| kennung.to_string())
}

/// Liest `<builds>/aktuell` und prüft den Ordner darin; sonst der Grund.
pub fn zeiger_ordner(builds: &Path) -> Result<PathBuf, String> {
    use std::io::Read;
    let datei = builds.join(bote::bau::ZEIGER);
    let mut text = String::new();
    std::fs::File::open(&datei).and_then(|f| f.take(ZEIGER_HOECHSTENS + 1).read_to_string(&mut text)).map_err(|e| format!("nicht lesbar: {e}"))?;
    if text.len() as u64 > ZEIGER_HOECHSTENS {
        return Err("die Datei ist zu groß".into());
    }
    let name = text.trim();
    let einfach = !name.is_empty()
        && !name.starts_with('.')
        && name.len() <= 100
        && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_' || c == b'.');
    if !einfach {
        return Err(format!("„{}“ ist kein einfacher Ordnername", name.chars().take(40).collect::<String>()));
    }
    let ordner = builds.join(name);
    match std::fs::symlink_metadata(&ordner) {
        Ok(m) if m.file_type().is_symlink() => return Err(format!("{name} ist eine Verknüpfung")),
        Ok(m) if m.is_dir() => {}
        Ok(_) => return Err(format!("{name} ist kein Ordner")),
        Err(_) => return Err(format!("den Ordner {name} gibt es nicht")),
    }
    if crate::inhalt::seite::ist_verknuepfung(&ordner) {
        return Err(format!("{name} ist eine Verknüpfung"));
    }
    if !ordner.join("index.html").is_file() {
        return Err(format!("in {name} fehlt index.html"));
    }
    Ok(ordner)
}

/// CSP der Oberfläche, gemerkt je index.html (Ordner und Änderungszeit). Ein
/// Neubau bei laufendem Server ändert das Startskript, ein Bau über den Boten
/// sogar den Ordner (`WebOrt`); ohne Neuberechnung bliebe die Seite weiß.
pub struct CspCache {
    web: Arc<WebOrt>,
    stand: std::sync::RwLock<CspStand>,
}

/// index.html (Ordner und Änderungszeit), für die die CSP gilt, und die CSP.
type CspStand = (Option<(PathBuf, std::time::SystemTime)>, Arc<str>);

impl CspCache {
    pub fn neu(web: Arc<WebOrt>) -> Self {
        let s = Self { web, stand: std::sync::RwLock::new((None, Arc::from(""))) };
        s.aktuell();
        s
    }

    /// Die passende CSP; liest index.html nur neu, wenn sich Ordner oder Änderungszeit geändert haben.
    pub fn aktuell(&self) -> Arc<str> {
        let datei = self.web.ordner().join("index.html");
        let jetzt = std::fs::metadata(&datei).and_then(|m| m.modified()).ok().map(|t| (datei.clone(), t));
        if let Ok(s) = self.stand.read()
            && s.0 == jetzt
            && !s.1.is_empty()
        {
            return s.1.clone();
        }
        let html = std::fs::read_to_string(&datei).unwrap_or_default();
        let hashes = sicherheit::inline_skript_hashes(&html);
        if hashes.is_empty() {
            warn!("Kein eingebettetes Startskript in {} gefunden; CSP erlaubt nur eigene Dateien", datei.display());
        }
        let csp: Arc<str> = Arc::from(sicherheit::csp_oberflaeche(&hashes));
        if let Ok(mut s) = self.stand.write() {
            *s = (jetzt, csp.clone());
        }
        csp
    }
}

/// Die Kopie der Oberfläche neben dem laufenden Programm (`bin\web`): Dorthin
/// legt scripts\Dienst.ps1 -Neustart `web\build` zusammen mit den Programmen.
pub fn web_kopie_neben_programm() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join(WEB_KOPIE)))
}

/// Name des Ordners der Oberfläche neben den Programmen (Dienst.ps1: `bin\web`).
pub const WEB_KOPIE: &str = "web";

/// `web/build`: Umgebungsvariable, sonst im Repo.
pub fn web_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("DASHBOARD_WEB_DIR") {
        return PathBuf::from(p);
    }
    let local = PathBuf::from("web").join("build");
    if local.is_dir() {
        return local;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web").join("build")
}

/// Datei des Datenspeichers: Umgebungsvariable `DASHBOARD_DATEN` (Dateipfad,
/// z. B. für den Testserver), sonst `%USERPROFILE%\.var\dashboard\dashboard.redb`
/// (außerhalb des Repos, nicht in Git; Rückfall `HOME`).
pub fn daten_datei() -> PathBuf {
    if let Some(p) = std::env::var_os("DASHBOARD_DATEN") {
        return PathBuf::from(p);
    }
    let heim = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from);
    heim.unwrap_or_else(|| PathBuf::from(".")).join(".var").join("dashboard").join("dashboard.redb")
}

/// Schlüssel der Ausweise (Etappe 7c) ohne Einstellungsdatei: Umgebungsvariable
/// `DASHBOARD_JWT_SCHLUESSEL` (Dateipfad, etwa für den Testserver), sonst
/// `geheim\jwt-schluessel` neben der Datei des Datenspeichers.
pub fn jwt_schluessel_datei(daten: &Path) -> PathBuf {
    if let Some(p) = std::env::var_os("DASHBOARD_JWT_SCHLUESSEL") {
        return PathBuf::from(p);
    }
    daten.parent().map(Path::to_path_buf).unwrap_or_default().join("geheim").join("jwt-schluessel")
}

/// `data/pc.toml`: neben dem Projektordner, sonst im Repo.
pub fn data_file(projects_dir: &Path) -> PathBuf {
    if let Some(p) = std::env::var_os("DASHBOARD_PC_DATA") {
        return PathBuf::from(p);
    }
    let beside = projects_dir.parent().map(|p| p.join("data").join("pc.toml"));
    match beside {
        Some(p) if p.is_file() => p,
        _ => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data").join("pc.toml"),
    }
}

/// `data/schluessel.toml`, gleiche Suche wie `data_file`.
pub fn keys_file(projects_dir: &Path) -> PathBuf {
    if let Some(p) = std::env::var_os("DASHBOARD_KEYS_DATA") {
        return PathBuf::from(p);
    }
    let beside = projects_dir.parent().map(|p| p.join("data").join("schluessel.toml"));
    match beside {
        Some(p) if p.is_file() => p,
        _ => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data").join("schluessel.toml"),
    }
}

/// Die Entscheidungen ohne Einstellungsdatei: `DASHBOARD_ENTSCHEIDUNGEN_DIR`,
/// sonst `docs/entscheidungen` neben dem (ersten) Projektordner. Nach dem
/// Umzug zeigt `DASHBOARD_PROJECTS_DIR` auf `dashboard-inhalt/projects`, dann
/// ist das `dashboard-inhalt/docs/entscheidungen`.
pub fn entscheidungen_dir(projects_dir: &Path) -> PathBuf {
    if let Some(p) = std::env::var_os("DASHBOARD_ENTSCHEIDUNGEN_DIR") {
        return PathBuf::from(p);
    }
    projects_dir.parent().map(Path::to_path_buf).unwrap_or_default().join("docs").join("entscheidungen")
}

/// Projekte mit Rechenteil (`server/`), deren Modul in diesem Programm nicht
/// angemeldet ist ([zusammenbau]). Projekte aus weiteren Quellen mit eigenem
/// Hinweis (`quellen::SATZ_RECHENTEIL`) zählen hier nicht. Liest die Platte.
pub fn rechenteile_ohne_modul(state: &AppState) -> Vec<String> {
    let q = state.alle_quellen();
    let angemeldet = state.projekte.slugs();
    q.projekte()
        .iter()
        .filter(|(s, e)| e.join(s).join("server").is_dir())
        .filter(|(s, _)| !angemeldet.contains(&s.as_str()) && !q.rechenteile.iter().any(|(r, _)| r == *s))
        .map(|(s, _)| s.clone())
        .collect()
}

/// Die Inhaltsquellen (Etappe 4b): `DASHBOARD_PROJECTS_DIR` als Liste, getrennt
/// durch `;` (je Eintrag ein Ordner mit Projekten oder ein einzelnes Projekt),
/// sonst `projects/` im Repo.
pub fn quellen() -> crate::quellen::Quellen {
    match std::env::var_os("DASHBOARD_PROJECTS_DIR") {
        Some(p) => crate::quellen::Quellen::aus_text(&p.to_string_lossy()),
        None => crate::quellen::Quellen::ein_ordner(projects_dir_standard()),
    }
}

/// Ordner mit den Projektseiten: der erste Ordner aus `DASHBOARD_PROJECTS_DIR`
/// (mit einem Eintrag wie bisher genau dieser), sonst `projects/` im Repo.
pub fn projects_dir() -> PathBuf {
    if std::env::var_os("DASHBOARD_PROJECTS_DIR").is_some() {
        return quellen().erste();
    }
    projects_dir_standard()
}

fn projects_dir_standard() -> PathBuf {
    let local = PathBuf::from("projects");
    if local.is_dir() {
        return local;
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("projects")
}

/// Nach der Herkunftsprüfung von tower-http: deren leere 403 bekommt unsere Form.
async fn csrf_antwort(req: Request, next: Next) -> Response {
    let res = next.run(req).await;
    if res.extensions().get::<ProtectionError>().is_some() {
        return sicherheit::abgelehnt();
    }
    res
}

pub fn router(state: AppState) -> Router {
    let mut csrf = CsrfLayer::new();
    for herkunft in sicherheit::eigene_herkuenfte(state.port) {
        csrf = csrf.add_trusted_origin(&herkunft).expect("eigene Herkunft ist eine gültige Adresse");
    }
    // Von außen nach innen. Kopfzeilen und Nummer zuerst, damit auch die
    // Antworten der Schleusen selbst (421, 403, 408, 413, 500) sie tragen.
    let schleusen = ServiceBuilder::new()
        .layer(middleware::from_fn(sicherheit::zwischenspeicher)) // 8
        .layer(middleware::from_fn_with_state(state.clone(), sicherheit::kopfzeilen)) // 7
        .layer(middleware::from_fn_with_state(sicherheit::Zaehler::default(), sicherheit::nummerieren)) // 2
        .layer(middleware::from_fn(sicherheit::fehlerform)) // JSON oder Satz
        .layer(sicherheit::protokoll()) // 2: Nummer, Dauer, Ergebnis
        .layer(CatchPanicLayer::custom(sicherheit::absturz)) // 1
        .layer(middleware::from_fn_with_state(state.clone(), sicherheit::nur_eigene_adresse)) // 3
        // 3a (Hotfix Prozent): `%` im Pfad unter /api → 400, vor Anmeldung und Routing.
        .layer(middleware::from_fn(sicherheit::prozent_im_pfad)) // 3a
        .layer(middleware::from_fn_with_state(state.clone(), sicherheit::herkunft_pflicht)) // 4
        .layer(middleware::from_fn(csrf_antwort))
        .layer(csrf) // 4
        // 4b: vor dem Zeitlimit, damit auch `warten` und der Live-Kanal (ohne Zeitlimit) anmelden müssen.
        .layer(middleware::from_fn_with_state(state.clone(), sicherheit::anmeldung_pflicht)) // 4b
        .layer(middleware::from_fn_with_state(state.clone(), sicherheit::zeitlimit)) // 5
        .layer(middleware::from_fn(sicherheit::groesse)) // 6
        // 6: außen die Grenze der Modul-Wege (64 KiB); alle anderen Wege
        // haben darunter ihre 16 KB (siehe unten), wie bisher.
        .layer(RequestBodyLimitLayer::new(dashboard_projekt::rahmen::KOERPER_HOECHSTENS)); // 6
    Router::new()
        .merge(api::routes())
        // Herzschlag: Sitzung → Chat bei schreibenden Aufrufen mit Kopfzeile (nur die Wege der API).
        .layer(middleware::from_fn_with_state(state.clone(), api::sitzungen::merken))
        // Lebenszeichen mit Angaben zum Lauf (Etappe 7a), neben /healthz.
        .route("/api/betrieb", get(dienst::betrieb::betrieb))
        // Kopfleiste, Fassung 2: Pfade und der Anmeldestand des Inhabers nur geschützt.
        .route("/api/betrieb/voll", get(dienst::betrieb::betrieb_voll))
        // Alte Adresse der Oberfläche (bis Etappe d): Lesezeichen weiterleiten.
        .route("/neu", get(oberflaeche::neu_redirect))
        .route("/neu/", get(oberflaeche::neu_redirect))
        .route("/neu/{*path}", get(oberflaeche::neu_redirect))
        // Alles andere ist die Oberfläche: Dateien aus web/build, sonst index.html.
        .fallback(oberflaeche::web)
        .layer(RequestBodyLimitLayer::new(sicherheit::MAX_KOERPER))
        // Antworten des Inhabers (Frage, Aufgabe): bis 64 KiB für „Etwas anderes …“. Die Grenze
        // setzt die äußere Schleuse 6 (ANTWORT_KOERPER_HOECHSTENS ist dieselbe Zahl); eine
        // zweite gleich große Schicht hier gäbe ohne Längenangabe 400 statt 413.
        .merge(api::antwort_routes())
        // Etappe 4b: Endpunkte der Backend-Module unter /api/projekte/<slug>/x/,
        // je im Rahmen des SDK (Absturzfänger, 64 KiB, Fehlerform).
        .merge(state.projekte.router())
        .layer(schleusen)
        .with_state(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode, header};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    /// Das Beispiel-Repo: feste Seiten, die kein anderer Chat anfasst.
    pub fn beispiel() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests").join("beispiel")
    }

    fn app() -> Router {
        router_angemeldet(AppState::fuer_tests(&beispiel()))
    }

    fn get(path: &str) -> Request<Body> {
        Request::get(path).header("host", "127.0.0.1:8787").body(Body::empty()).unwrap()
    }

    async fn get_page(path: &str) -> (StatusCode, String) {
        let res = app().oneshot(get(path)).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8_lossy(&bytes).to_string())
    }

    fn is_app_or_hint(status: StatusCode, body: &str) -> bool {
        (status == StatusCode::OK || status == StatusCode::SERVICE_UNAVAILABLE) && (body.contains("<html") || body.contains("<!doctype"))
    }

    #[tokio::test]
    async fn app_is_served_on_every_page_route() {
        // Projektseiten direkt unter /projects/<slug> (die Weiterleitung /neu prüft
        // neu_leitet_dauerhaft_zur_projektseite).
        for path in [
            "/",
            "/pc",
            "/schluessel",
            "/projects/dashboard",
            "/projects/neu-form",
            "/projects/neu-form/plan",
            "/katalog/entwuerfe/logo",
            "/anleitung",
            "/zielbild",
            "/entscheidungen",
            "/projekte/archiviert",
        ] {
            let (status, body) = get_page(path).await;
            assert!(is_app_or_hint(status, &body), "{path}: {status} {body}");
        }
    }

    /// Etappe 3b (Fund des Prüfers „Fehlerseite 200 statt 404“): Adressen, die
    /// keine Seite sind, bekommen die Oberfläche mit 404; der Browser zeigt die
    /// Fehlerseite. Ohne gebaute Oberfläche bleibt es beim Hinweis (503).
    #[tokio::test]
    async fn unbekannte_seiten_sind_404_mit_oberflaeche() {
        for path in [
            "/irgendwas",
            "/pc/gibt-es-nicht",
            "/projects/gibt-es-nicht",
            "/projects/gibt-es-nicht/neu",
            "/projects/neu-form/etwas",
            // Etappe 4d: unter den System-Seiten gibt es nichts.
            "/anleitung/x",
            "/zielbild/abschnitt",
            "/entscheidungen/2026-10-31-plan-4.toml",
            // Entscheidung archiv-status: unter /projekte nur die Archivansicht.
            "/projekte",
            "/projekte/archiviert/x",
        ] {
            let (status, body) = get_page(path).await;
            assert!(
                matches!(status, StatusCode::NOT_FOUND | StatusCode::SERVICE_UNAVAILABLE) && (body.contains("<html") || body.contains("<!doctype")),
                "{path}: {status} {body}"
            );
            assert!(!body.contains("cargo") && !body.contains("npm "), "{path}: keine Befehle in der Antwort");
        }
    }

    /// Ein Projekt hat seine Seite unter /projects/<slug> und die Unterseiten
    /// unter /projects/<slug>/<name>. Seit Etappe 6a leiten /neu und
    /// /neu/<name> für jedes Projekt dauerhaft dorthin weiter (mit Suche); eine
    /// unbekannte Unterseite ist erst am Ziel 404, ein unbekanntes Projekt gleich.
    #[tokio::test]
    async fn neu_leitet_dauerhaft_zur_projektseite() {
        for (pfad, ziel) in [
            ("/projects/neu-form/neu", "/projects/neu-form"),
            ("/projects/neu-form/neu/", "/projects/neu-form"),
            ("/projects/neu-form/neu/plan", "/projects/neu-form/plan"),
            ("/projects/neu-form/neu/plan?x=1", "/projects/neu-form/plan?x=1"),
            ("/projects/dashboard/neu", "/projects/dashboard"),
            ("/projects/dashboard/neu/gibt-es-nicht", "/projects/dashboard/gibt-es-nicht"),
            ("/projects/neu-kaputt/neu", "/projects/neu-kaputt"),
        ] {
            let res = app().oneshot(get(pfad)).await.unwrap();
            assert_eq!(res.status(), StatusCode::TEMPORARY_REDIRECT, "{pfad}");
            assert_eq!(res.headers()[header::LOCATION], ziel, "{pfad}");
        }
        for pfad in ["/projects/neu-form", "/projects/neu-form/plan", "/projects/dashboard", "/projects/schaufenster/vorschau"] {
            let (status, body) = get_page(pfad).await;
            assert!(is_app_or_hint(status, &body), "{pfad}: {status}");
        }
        // Unbekannte Unterseite und unbekanntes Projekt: 404, auch unter /neu.
        for pfad in ["/projects/neu-form/gibt-es-nicht", "/projects/dashboard/plan", "/projects/gibt-es-nicht/neu", "/projects/gibt-es-nicht/neu/plan"] {
            let (status, _) = get_page(pfad).await;
            assert!(matches!(status, StatusCode::NOT_FOUND | StatusCode::SERVICE_UNAVAILABLE), "{pfad}: {status}");
        }
        let b = AppState::fuer_tests(&beispiel()).inhalt.bestand();
        assert_eq!(oberflaeche::weiter_zur_hauptseite("projects/gibt-es-nicht/neu", &b), None);
        assert!(!oberflaeche::ist_seite("projects/neu-form/neu", &b));
        assert!(!oberflaeche::ist_unterseite("projects/neu-form/neu", &b));
        assert!(!oberflaeche::ist_unterseite("projects/neu-form/vorschau", &b));
    }

    /// Jede Route in `web/src/routes` ist für den Server eine Seite (200),
    /// sonst gäbe die neue Route 404 (`oberflaeche::SEITEN`, `ist_seite`).
    #[test]
    fn jede_route_der_oberflaeche_ist_eine_seite() {
        fn routen(ordner: &std::path::Path, vorn: &str, aus: &mut Vec<String>) {
            for e in std::fs::read_dir(ordner).unwrap().flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if e.path().is_dir() {
                    routen(&e.path(), &format!("{vorn}/{name}"), aus);
                } else if name == "+page.svelte" {
                    aus.push(vorn.to_string());
                }
            }
        }
        let wurzel = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("web").join("src").join("routes");
        let mut gefunden = Vec::new();
        routen(&wurzel, "", &mut gefunden);
        assert!(gefunden.len() >= 10, "{gefunden:?}");
        let b = AppState::fuer_tests(&beispiel()).inhalt.bestand();
        for r in &gefunden {
            // Gruppenseiten gibt es nur mit data/gruppen.toml (die Beispiele haben keine):
            // geprüft in gruppen_test::gruppenseite_nur_fuer_gruppen_mit_teilen.
            if r.starts_with("/gruppen/") {
                assert_eq!(r, "/gruppen/[slug]");
                continue;
            }
            let pfad = r.replace("[slug]", "neu-form").replace("[name]", "plan");
            let rel = pfad.trim_start_matches('/');
            // Etappe 6a: /projects/[slug]/neu… gibt es in der Oberfläche nur als Weiterleitung.
            if r.starts_with("/projects/[slug]/neu") {
                assert!(oberflaeche::weiter_zur_hauptseite(rel, &b).is_some(), "Route {r} ({rel}) leitet nicht weiter");
                continue;
            }
            let seite = oberflaeche::ist_seite(rel, &b) || oberflaeche::ist_unterseite(rel, &b);
            assert!(seite, "Route {r} ({rel}) fehlt in oberflaeche::SEITEN oder ist_seite");
        }
        for unbekannt in [
            "irgendwas",
            "pc/x",
            "projects/gibt-es-nicht",
            "projects/dashboard/neu",
            "projects/neu-form/neu/x",
            "anleitung/x",
            "zielbild/x",
            "entscheidungen/x",
            "gruppen",
            "gruppen/neu-form",
        ] {
            assert!(!oberflaeche::ist_seite(unbekannt, &b), "{unbekannt}");
        }
    }

    #[tokio::test]
    async fn app_never_escapes_web_dir_and_missing_assets_are_404() {
        let (status, body) = get_page("/../Cargo.toml").await;
        assert!(!body.contains("[package]"), "{status}: darf nicht aus web/build ausbrechen");
        let (_, body) = get_page("/..%2FCargo.toml").await;
        assert!(!body.contains("[package]"));
        assert_eq!(get_page("/.git/config").await.0, StatusCode::NOT_FOUND);
        assert_eq!(get_page("/_app/immutable/gibt-es-nicht.js").await.0, StatusCode::NOT_FOUND);
    }

    /// Etappe 3b-2 (Auflage aus der Abnahme 3b): Unter /katalog gibt es nur die
    /// bekannten Seiten (`oberflaeche::KATALOG_SEITEN`); jede andere Adresse
    /// ist 404 mit der Oberfläche, wie überall sonst.
    #[tokio::test]
    async fn unbekannte_katalogseiten_sind_404() {
        for path in ["/katalog", "/katalog/", "/katalog/entwuerfe/logo", "/katalog/entwuerfe/logo/"] {
            let (status, body) = get_page(path).await;
            assert!(is_app_or_hint(status, &body), "{path}: {status}");
        }
        for path in ["/katalog/gibtsnicht", "/katalog/entwuerfe", "/katalog/entwuerfe/gibtsnicht", "/katalog/entwuerfe/logo/mehr"] {
            let (status, body) = get_page(path).await;
            assert!(
                matches!(status, StatusCode::NOT_FOUND | StatusCode::SERVICE_UNAVAILABLE) && (body.contains("<html") || body.contains("<!doctype")),
                "{path}: {status}"
            );
        }
        let b = AppState::fuer_tests(&beispiel()).inhalt.bestand();
        assert!(oberflaeche::ist_seite("katalog", &b) && oberflaeche::ist_seite("katalog/entwuerfe/logo", &b));
        assert!(!oberflaeche::ist_seite("katalog/gibtsnicht", &b) && !oberflaeche::ist_seite("katalog/entwuerfe/gibtsnicht", &b));
    }

    /// Unter Windows ersetzte `join` den ganzen Pfad, wenn die
    /// Adresse ein Laufwerk enthielt; `/C:/Windows/win.ini` gab die Datei aus.
    /// Jede Form eines Pfads außerhalb von web/build ist jetzt 404; normale
    /// Dateien bleiben 200, Seitenwege bekommen index.html.
    #[tokio::test]
    async fn adressen_mit_laufwerk_wurzel_oder_strom_sind_404() {
        let web = std::env::temp_dir().join(format!("dashboard-web-pfade-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&web);
        std::fs::create_dir_all(web.join("_app").join("immutable")).unwrap();
        std::fs::write(web.join("index.html"), "<!doctype html><title>App</title>").unwrap();
        std::fs::write(web.join("_app").join("immutable").join("start.js"), "export {};").unwrap();
        std::fs::write(web.join("favicon.png"), b"png").unwrap();
        let ort = Arc::new(WebOrt::neu(web.clone()));
        let state = AppState { csp: Arc::new(CspCache::neu(ort.clone())), web: ort, ..AppState::fuer_tests(&beispiel()) };
        let app = router_angemeldet(state);
        let abrufen = |pfad: &'static str| {
            let app = app.clone();
            async move {
                let res = app.oneshot(get(pfad)).await.unwrap();
                let status = res.status();
                let body = String::from_utf8_lossy(&res.into_body().collect().await.unwrap().to_bytes()).to_string();
                (status, body)
            }
        };
        let fremd = std::fs::read_to_string("C:/Windows/win.ini").unwrap_or_default();
        for pfad in [
            "/C:/Windows/win.ini",
            "/c:/windows/win.ini",
            "/C:%5CWindows%5Cwin.ini",
            "/C:",
            "//server/share/datei.txt",
            "//?/C:/Windows/win.ini",
            "/%5C%5C%3F%5CC:%5CWindows%5Cwin.ini",
            "/favicon.png:strom",
            "/index.html::$DATA",
            "/%2e%2e/Cargo.toml",
            "/_app/../index.html",
            "/_app/./immutable/start.js",
            "/../Cargo.toml",
            "/.git/config",
            "/a//b",
        ] {
            let (status, body) = abrufen(pfad).await;
            assert_eq!(status, StatusCode::NOT_FOUND, "{pfad}: {body}");
            assert!(fremd.trim().is_empty() || !body.contains(fremd.trim()), "{pfad} gab eine fremde Datei aus");
        }
        assert_eq!(abrufen("/favicon.png").await, (StatusCode::OK, "png".to_string()));
        assert_eq!(abrufen("/_app/immutable/start.js").await.0, StatusCode::OK);
        for seite in ["/", "/pc", "/projects/dashboard", "/projects/neu-form", "/projects/neu-form/plan"] {
            let (status, body) = abrufen(seite).await;
            assert_eq!(status, StatusCode::OK, "{seite}");
            assert!(body.contains("<title>App</title>"), "{seite}: {body}");
        }
        let _ = std::fs::remove_dir_all(&web);
    }

    /// Das Symbol der angehefteten Edge-App: Manifest, PNG und ico mit
    /// der richtigen Art.
    #[test]
    fn manifest_und_symbole_haben_die_richtige_art() {
        use crate::oberflaeche::mime_for;
        assert_eq!(mime_for(Path::new("manifest.webmanifest")), "application/manifest+json");
        assert_eq!(mime_for(Path::new("icons/inhaber-192.png")), "image/png");
        assert_eq!(mime_for(Path::new("favicon.ico")), "image/x-icon");
        assert!(sicherer_teilpfad_gilt("/icons/inhaber-256.png") && sicherer_teilpfad_gilt("/manifest.webmanifest"));
    }

    fn sicherer_teilpfad_gilt(p: &str) -> bool {
        crate::oberflaeche::sicherer_teilpfad(p).is_some()
    }

    #[test]
    fn sicherer_teilpfad_nur_normale_segmente() {
        use crate::oberflaeche::sicherer_teilpfad;
        assert_eq!(sicherer_teilpfad("/"), Some(PathBuf::new()));
        assert_eq!(sicherer_teilpfad("/_app/immutable/a-1_b.js"), Some(PathBuf::from("_app").join("immutable").join("a-1_b.js")));
        assert_eq!(sicherer_teilpfad("/projects/x/"), Some(PathBuf::from("projects").join("x")));
        for schlecht in ["/C:/x", "/c:", "//h/s", "/a\\b", "/a:b", "/%2e%2e", "/..", "/.", "/a/../b", "/.versteckt", "/a//b", "/ä"] {
            assert_eq!(sicherer_teilpfad(schlecht), None, "{schlecht}");
        }
    }

    #[tokio::test]
    async fn old_neu_address_redirects_to_root() {
        let res = app().oneshot(get("/neu/pc?x=1")).await.unwrap();
        assert_eq!(res.status(), StatusCode::PERMANENT_REDIRECT);
        assert_eq!(res.headers()[header::LOCATION], "/pc?x=1");
    }

    #[tokio::test]
    async fn assets_only_with_plain_names_and_html_gets_own_csp() {
        assert_eq!(get_page("/projects/schaufenster/assets/..%2Fpage.md").await.0, StatusCode::NOT_FOUND);
        assert_eq!(get_page("/projects/../assets/x.png").await.0, StatusCode::NOT_FOUND);
        assert_eq!(get_page("/projects/schaufenster/assets/.versteckt").await.0, StatusCode::NOT_FOUND);
        let res = app().oneshot(get("/projects/schaufenster/assets/vorschau.html")).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let csp = res.headers()[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
        assert!(csp.contains("default-src 'none'") && csp.contains("sandbox allow-scripts"), "{csp}");
        let res = app().oneshot(get("/projects/schaufenster/assets/bild.svg")).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(res.headers()[header::CONTENT_TYPE], "image/svg+xml");
        // SVG hat eine eigene strenge CSP, nicht die der Oberfläche.
        let csp = res.headers()[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
        assert_eq!(csp, crate::api::projekte::ASSET_SVG_CSP);
        assert_eq!(get_page("/projects/schaufenster/assets/con.svg").await.0, StatusCode::NOT_FOUND);
        assert_eq!(get_page("/projects/schaufenster/assets/NUL.txt").await.0, StatusCode::NOT_FOUND);
    }

    /// Etappe 4b: ein einzelnes Projekt als zweite Quelle (Overlay-Repo) über
    /// den Router: Seite, Kopf, Daten und Assets aus seinem Ordner. Wird die
    /// Quelle unlesbar, bleibt die Seite, und die Prüfung nennt einen Satz
    /// ohne Pfad; wieder lesbar, ist der Satz weg.
    #[tokio::test]
    async fn zweite_quelle_ueber_den_router() {
        let basis = std::env::temp_dir().join(format!("dash-router-quelle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&basis);
        let solo = basis.join("overlay/projects/fremd-solo");
        std::fs::create_dir_all(solo.join("assets")).unwrap();
        std::fs::create_dir_all(solo.join("daten")).unwrap();
        std::fs::write(solo.join("projekt.toml"), "schema = 3\ntitle = \"Fremd Solo\"\nstatus = \"aktiv\"\n").unwrap();
        std::fs::write(solo.join("assets/bild.png"), "PNG").unwrap();
        std::fs::write(solo.join("daten/fortschritt.toml"), "[[zeile]]\ntitel = \"F\"\nprozent = 40\n").unwrap();
        let state = AppState::fuer_tests_ohne_speicher(&beispiel(), "").mit_quellen(vec![beispiel().join("projects"), solo.clone()]);
        let app = router_angemeldet(state);
        let hole = |pfad: &'static str| {
            let app = app.clone();
            async move {
                let res = app.oneshot(get(pfad)).await.unwrap();
                let status = res.status();
                (status, String::from_utf8_lossy(&res.into_body().collect().await.unwrap().to_bytes()).to_string())
            }
        };
        let (status, body) = hole("/api/projects/fremd-solo").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(body.contains("\"title\":\"Fremd Solo\""), "{body}");
        let (status, body) = hole("/api/projekte/fremd-solo").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(body.contains("\"fortschritt\""), "{body}");
        let (status, body) = hole("/api/projekte/fremd-solo/daten/fortschritt").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(body.contains("\"prozent\":40"), "{body}");
        assert_eq!(hole("/projects/fremd-solo/assets/bild.png").await, (StatusCode::OK, "PNG".to_string()));
        assert_eq!(hole("/projects/fremd-solo/assets/fehlt.png").await.0, StatusCode::NOT_FOUND);
        let satz = crate::quellen::satz_unlesbar(1);
        assert!(!hole("/api/pruefung").await.1.contains(&satz));

        std::fs::rename(basis.join("overlay"), basis.join("overlay-weg")).unwrap();
        let (status, body) = hole("/api/projects/fremd-solo").await;
        assert_eq!(status, StatusCode::OK, "der letzte gute Stand bleibt: {body}");
        let (_, bericht) = hole("/api/pruefung").await;
        assert!(bericht.contains(&satz), "{bericht}");
        assert!(!bericht.contains("overlay"), "Satz ohne Pfad: {bericht}");
        std::fs::rename(basis.join("overlay-weg"), basis.join("overlay")).unwrap();
        assert!(!hole("/api/pruefung").await.1.contains(&satz));
        assert_eq!(hole("/projects/fremd-solo/assets/bild.png").await.0, StatusCode::OK);
        let _ = std::fs::remove_dir_all(&basis);
    }

    #[tokio::test]
    async fn keys_post_needs_origin_and_known_name() {
        let res = app()
            .oneshot(
                Request::post("/schluessel/beispiel-test")
                    .header("host", "127.0.0.1:8787")
                    .header("origin", "https://boese.example")
                    .body(Body::from("wert=x"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::FORBIDDEN);
        assert!(res.headers()[header::CONTENT_TYPE].to_str().unwrap().starts_with("text/plain"));
        let body = res.into_body().collect().await.unwrap().to_bytes();
        assert!(String::from_utf8_lossy(&body).contains("Abgelehnt"));
        let res = app()
            .oneshot(
                Request::post("/schluessel/gibt-es-nicht")
                    .header("host", "127.0.0.1:8787")
                    .header("origin", "http://127.0.0.1:8787")
                    .body(Body::from("wert=geheim123"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::SEE_OTHER, "zurück zur Seite mit Fehlermeldung");
        let loc = res.headers()[header::LOCATION].to_str().unwrap().to_string();
        assert!(loc.starts_with("/schluessel?fehler="), "{loc}");
        assert!(!loc.contains("geheim123"), "nie den Wert zurückgeben: {loc}");
        let res = app()
            .oneshot(
                Request::post("/schluessel/beispiel-test")
                    .header("host", "127.0.0.1:8787")
                    .header("referer", "http://127.0.0.1:8787/schluessel")
                    .body(Body::from(vec![b'a'; 5000]))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn api_projects_and_detail_are_json() {
        let (status, body) = get_page("/api/projects").await;
        assert_eq!(status, StatusCode::OK);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        let list = v["projects"].as_array().unwrap();
        let slugs: Vec<&str> = list.iter().filter_map(|p| p["slug"].as_str()).collect();
        assert_eq!(slugs, ["dashboard", "schaufenster", "geflecht", "neu-form", "neu-daten"], "Reihenfolge nach order: {body}");
        let broken = v["broken"].as_array().unwrap();
        let kaputt: Vec<&str> = broken.iter().filter_map(|b| b["slug"].as_str()).collect();
        assert_eq!(kaputt, ["kaputt", "neu-kaputt"], "genau die absichtlich kaputten Seiten: {body}");
        assert!(broken[1]["error"].as_str().unwrap().starts_with("projects/neu-kaputt/projekt.toml, Zeile 4: Bei `status` steht „aktief“."), "{body}");
        let (status, body) = get_page("/api/projects/dashboard").await;
        assert_eq!(status, StatusCode::OK);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["slug"], "dashboard");
        assert!(v.get("form").is_none(), "Etappe 6d: kein Feld form: {body}");
        assert_eq!(v["bedeutung"], "laeuft", "aktiv wird laeuft: {body}");
        assert!(v.get("status_class").is_none(), "keine CSS-Klassen mehr: {body}");
        assert!(v.get("segments").is_none(), "keine alte Form mehr: {body}");
        assert_eq!(v["daten"]["gantt"]["balken"][0]["name"], "Kompost ansetzen", "{body}");
        assert_eq!(v["vorschau"], serde_json::Value::Null);
        assert!(v["repos"].is_array(), "Git-Stand fehlt: {body}");
        let (status, body) = get_page("/api/projects/neu-form").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["status"], "aktiv");
        assert_eq!(v["backup"], "grün");
        assert_eq!(v["backup_bedeutung"], "ok");
        assert_eq!(v["vorschau"], "/projects/neu-form/assets/vorschau.html");
        assert_eq!(v["seiten"][0]["name"], "beet-a", "nach order: {body}");
        assert_eq!(v["daten"]["kennzahlen"]["kennzahl"][0]["farbe"], "ok", "{body}");
        assert_eq!(v["daten"]["zeitstrahl"]["teil"]["plan"]["eintrag"][0]["titel"], "Pilot");
        assert_eq!(v["daten"]["fragen"]["frage"][0]["art"], "wahl");
        assert_eq!(v["daten"]["eigen"]["notizen"]["notiz"][0]["wichtig"], true);
        assert_eq!(v["checks"][0]["ok"], false);
        assert_eq!(v["processes"][0]["running"], false);
        let (status, body) = get_page("/api/projects/neu-kaputt").await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(v["fehler"].as_str().unwrap().contains("Erlaubt sind: aktiv, pausiert, geplant, fertig, problem, archiviert."), "{body}");
        let (status, body) = get_page("/api/projects/schaufenster").await;
        assert_eq!(status, StatusCode::OK);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["vorschau"], "/projects/schaufenster/assets/vorschau.html");
        let (status, body) = get_page("/api/projects/gibt-es-nicht").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["art"], "nicht-gefunden");
        let (status, body) = get_page("/api/projects/kaputt").await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["art"], "unlesbar");
        assert!(v["fehler"].as_str().unwrap().starts_with("projects/kaputt/projekt.toml"), "{body}");
        assert_eq!(get_page("/api/projects/..").await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn api_pc_and_keys_never_leak_values() {
        let (status, body) = get_page("/api/pc/benutzer").await;
        assert_eq!(status, StatusCode::OK);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        let s = &v["schluessel"];
        assert_eq!(s["gespeichert"].as_u64().unwrap() + s["fehlen"].as_u64().unwrap(), 1);
        assert_ne!(s["vollstaendig"], "unbekannt", "ohne Boten direkt aus dem Tresor: {body}");
        assert_eq!(s["grund"], serde_json::Value::Null, "{body}");
        assert_eq!(v["bote"], "da", "angemeldet: {body}");
        let (status, body) = get_page("/api/pc").await;
        assert_eq!(status, StatusCode::OK);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(v.get("keys").is_none() && v.get("schluessel").is_none(), "Schlüssel nur unter /api/pc/benutzer: {body}");
        assert!(v["snapshot"]["process_names"].is_null(), "Prozessliste bleibt intern");
        assert_eq!(v["data"]["steps"].as_array().unwrap().len(), 3, "Fahrplan aus dem Beispiel: {body}");
        let backups = v["project_backups"].as_array().unwrap();
        assert_eq!(backups.len(), 3, "schaufenster, geflecht, neu-form: {body}");
        assert!(backups.iter().all(|b| b["bedeutung"].is_string() && b.get("backup_class").is_none()), "{body}");
        let (status, body) = get_page("/api/schluessel").await;
        assert_eq!(status, StatusCode::OK);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        let k = &v["keys"][0];
        assert_eq!(k["name"], "beispiel-test");
        assert!(k.get("wert").is_none() && k.get("value").is_none());
        assert!(!body.contains("benutzer"), "auch der Benutzername bleibt intern");
    }

    /// Auflage aus der Abnahme 2a: ohne Datenspeicher laufen Projekte, PC und
    /// Schlüssel weiter.
    #[tokio::test]
    async fn ohne_speicher_laeuft_der_rest_weiter() {
        let state = AppState::fuer_tests_ohne_speicher(&beispiel(), "Ein Eintrag im Datenspeicher ist beschädigt.");
        assert!(state.speicher().is_err());
        let app = router_angemeldet(state);
        for pfad in ["/api/projects", "/api/projects/dashboard", "/api/pc", "/api/schluessel", "/api/daten", "/healthz"] {
            let res = app.clone().oneshot(get(pfad)).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK, "{pfad}");
        }
        for pfad in ["/", "/daten", "/pc"] {
            let res = app.clone().oneshot(get(pfad)).await.unwrap();
            let status = res.status();
            let bytes = res.into_body().collect().await.unwrap().to_bytes();
            assert!(is_app_or_hint(status, &String::from_utf8_lossy(&bytes)), "{pfad}: {status}");
        }
    }

    /// Messung der Etappe 2d (von Hand, nicht im normalen Testlauf): mit einer
    /// Kopie des echten Projektordners, nie mit dem laufenden Dienst.
    /// `DASHBOARD_MESSEN=<Ordner mit projects\ und data\> cargo test --lib
    /// messung_startseite -- --ignored --nocapture`. Misst das erste volle
    /// Lesen (Start), die erste Anfrage jedes Wegs der Startseite und den
    /// Median aus 50 weiteren; dann eine geänderte Datei bis zum Ereignis.
    #[tokio::test]
    #[ignore]
    async fn messung_startseite() {
        use std::time::{Duration, Instant};
        let Some(basis) = std::env::var_os("DASHBOARD_MESSEN").map(PathBuf::from) else {
            eprintln!("DASHBOARD_MESSEN ist nicht gesetzt; nichts gemessen.");
            return;
        };
        let projekte = basis.join("projects");
        let start = Instant::now();
        let inhalt = Arc::new(ContentStore::laden(
            projekte.clone(),
            basis.join("data").join("pc.toml"),
            basis.join("data").join("schwerpunkte.toml"),
            Abgleich::Beobachter,
        ));
        let laden = start.elapsed();
        let b = inhalt.bestand();
        eprintln!("Start: {} Projekte ({} kaputt) in {laden:?} gelesen", b.liste().0.len(), b.liste().1.len());
        let state = AppState {
            projects_dir: Arc::new(projekte.clone()),
            data_file: Arc::new(basis.join("data").join("pc.toml")),
            keys_file: Arc::new(basis.join("data").join("schluessel.toml")),
            inhalt: inhalt.clone(),
            ..AppState::fuer_tests(&beispiel())
        };
        let app = router_angemeldet(state);
        async fn messen(app: &Router, pfad: &str) -> Duration {
            let start = Instant::now();
            let res = app.clone().oneshot(get(pfad)).await.unwrap();
            assert!(res.status().is_success(), "{pfad}: {}", res.status());
            let _ = res.into_body().collect().await.unwrap();
            start.elapsed()
        }
        let pfade = ["/api/projects", "/api/pc", "/api/anstehend", "/api/projects/dashboard", "/api/projekte/dashboard/stand"];
        let mut summe_erste = Duration::ZERO;
        for pfad in pfade {
            let erste = messen(&app, pfad).await;
            summe_erste += erste;
            let mut zeiten: Vec<Duration> = Vec::new();
            for _ in 0..50 {
                zeiten.push(messen(&app, pfad).await);
            }
            zeiten.sort();
            eprintln!("{pfad}: erste {erste:?}, Median {:?}, höchstens {:?}", zeiten[25], zeiten[49]);
            assert!(zeiten[25] < Duration::from_millis(50), "{pfad}: {:?}", zeiten[25]);
        }
        eprintln!("Startseite kalt (Lesen + erste Anfragen): {:?}", laden + summe_erste);
        assert!(laden + summe_erste < Duration::from_millis(500));

        // Eine Datei ändern: nur dieses Projekt neu, ein Ereignis.
        let mut rx = inhalt.abonnieren();
        let datei = projekte.join("dashboard").join("projekt.toml");
        let text = std::fs::read_to_string(&datei).unwrap();
        std::fs::write(&datei, format!("{text}\n")).unwrap();
        let start = Instant::now();
        let a = inhalt.abgleichen();
        eprintln!(
            "Abgleich nach einer Änderung: {:?}, Projekte {:?}, Ereignisse {}",
            start.elapsed(),
            a.projekte,
            std::iter::from_fn(|| rx.try_recv().ok()).count()
        );
        std::fs::write(&datei, text).unwrap();
        let start = Instant::now();
        inhalt.abgleichen();
        eprintln!("Abgleich nach dem Zurückschreiben: {:?}", start.elapsed());
        let start = Instant::now();
        inhalt.abgleichen();
        eprintln!("Abgleich ohne Änderung (Leerlauf, alle 2 s): {:?}", start.elapsed());
    }

    /// Vorgabe 6 des Git-Zwischenspeichers: /api/projects startet kein git.
    /// Ein Probe-Projekt nennt dieses Repo und eines, dessen git hängt (früher
    /// 3 s Budget); die Liste kommt trotzdem in unter 50 ms, leer („wird
    /// ermittelt“) und nach dem Füllen mit Stand.
    #[tokio::test]
    async fn projektliste_liest_nur_den_git_zwischenspeicher() {
        let basis = std::env::temp_dir().join(format!("dash-gitzw-{}", std::process::id()));
        let projekte = basis.join("projects");
        let _ = std::fs::remove_dir_all(&basis);
        std::fs::create_dir_all(projekte.join("probe")).unwrap();
        let hier = env!("CARGO_MANIFEST_DIR").replace('\\', "/");
        let seite = format!(
            "schema = 3\ntitle = \"Probe\"\nstatus = \"aktiv\"\nsummary = \"Git-Zwischenspeicher\"\n\n\
             [[repos]]\npath = \"{hier}\"\nlabel = \"hier\"\n\n\
             [[repos]]\npath = \"{hier}/tests/beispiel/haengt\"\nlabel = \"haengt\"\n"
        );
        std::fs::write(projekte.join("probe/projekt.toml"), seite).unwrap();
        let git = Arc::new(git::Zwischenspeicher::neu(vec![projekte.clone()], None));
        let state = AppState { git: git.clone(), ..AppState::fuer_tests(&beispiel()).mit_projekten(projekte.clone()) };
        let app = router(state);

        async fn messen(app: &Router, pfad: &str) -> (std::time::Duration, serde_json::Value) {
            let start = std::time::Instant::now();
            let res = app.clone().oneshot(get(pfad)).await.unwrap();
            assert_eq!(res.status(), StatusCode::OK, "{pfad}");
            let bytes = res.into_body().collect().await.unwrap().to_bytes();
            (start.elapsed(), serde_json::from_slice(&bytes).unwrap())
        }
        // 50 ms sind das Ziel; die Grenze hat Luft für einen langsamen Testlauf
        // (Auflage aus der Abnahme des Git-Zwischenspeichers), im vollen Lauf
        // mehr (392 ms gesehen, während alle Tests zugleich liefen).
        let grenze = crate::zeitgrenze(std::time::Duration::from_millis(150), std::time::Duration::from_millis(1000));

        // Leer: kein Stand, kein Fehler, kein Zweig.
        let (dauer, v) = messen(&app, "/api/projects").await;
        assert!(dauer < grenze, "leer: {dauer:?}");
        let repos = v["projects"][0]["repos"].as_array().unwrap().clone();
        assert_eq!(repos.len(), 2, "{v}");
        for r in &repos {
            assert!(r["stand"].is_null() && r["error"].is_null() && r["branch"].is_null(), "{r}");
        }
        let (_, d) = messen(&app, "/api/projects/probe").await;
        assert!(d["repos"][0]["stand"].is_null(), "{d}");

        // Gefüllt (das hängende Repo kostet hier einmal sein Budget).
        assert_eq!(git.alle_erneuern().await, 2);
        let (dauer, v) = messen(&app, "/api/projects").await;
        assert!(dauer < grenze, "gefüllt: {dauer:?}");
        let repos = v["projects"][0]["repos"].as_array().unwrap();
        assert_eq!(repos[0]["label"], "hier");
        assert!(repos[0]["branch"].is_string() && repos[0]["error"].is_null(), "{v}");
        assert_eq!(repos[1]["error"], "git antwortet nicht (Zeitlimit)");
        for r in repos {
            assert!(chrono::DateTime::parse_from_rfc3339(r["stand"].as_str().unwrap()).is_ok(), "{r}");
        }
        let (dauer, d) = messen(&app, "/api/projects/probe").await;
        assert!(dauer < grenze, "Einzelseite: {dauer:?}");
        assert!(d["repos"][0]["stand"].is_string(), "{d}");
        let _ = std::fs::remove_dir_all(&basis);
    }

    /// Ein `web/` im Temp-Ordner: `build/` und zwei Bau-Ordner, jeder mit
    /// eigenem Startskript (also eigener CSP) und eigener Datei unter `_app/`.
    struct WebProbe(PathBuf);

    impl WebProbe {
        fn neu(name: &str) -> WebProbe {
            let w = std::env::temp_dir().join(format!("dashboard-webort-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&w);
            for (ordner, inhalt) in [("build", "web-build"), ("builds/20261027-100000", "bau-1"), ("builds/20261027-110000", "bau-2")] {
                let o = w.join(ordner);
                std::fs::create_dir_all(o.join("_app")).unwrap();
                let html = format!("<!doctype html><title>{inhalt}</title><script>start('{inhalt}')</script>");
                std::fs::write(o.join("index.html"), html).unwrap();
                std::fs::write(o.join("_app").join("teil.js"), format!("// {inhalt}")).unwrap();
            }
            WebProbe(w)
        }

        fn zeiger(&self, text: &str) {
            crate::bote::bau::zeiger_schreiben(&self.0.join("builds"), text).unwrap();
        }

        fn state(&self) -> AppState {
            self.state_mit_kopie(None)
        }

        fn state_mit_kopie(&self, kopie: Option<PathBuf>) -> AppState {
            let web = Arc::new(WebOrt::neu(self.0.join("build")).mit_kopie(kopie));
            AppState { csp: Arc::new(CspCache::neu(web.clone())), web, ..AppState::fuer_tests(&beispiel()) }
        }
    }

    /// Mit eingespielter Kopie (im Dienst `bin\web`) kommt die Oberfläche aus
    /// ihr, nicht aus web/build: Ein `npm run build` im Repo ändert die
    /// laufende Seite nicht. Ein Bau über den Boten (Zeiger) gilt weiter
    /// sofort; ohne Zeiger wieder die Kopie. Gegenprobe: Ohne Kopie (nicht
    /// gesetzt oder ohne index.html) folgt die Seite web/build wie bisher.
    #[tokio::test]
    async fn auslieferung_aus_der_kopie_nicht_aus_web_build() {
        let w = WebProbe::neu("kopie");
        let kopie = w.0.join("bin-web");
        std::fs::create_dir_all(kopie.join("_app")).unwrap();
        std::fs::write(kopie.join("index.html"), "<!doctype html><title>kopie</title><script>start('kopie')</script>").unwrap();
        std::fs::write(kopie.join("_app").join("teil.js"), "// kopie").unwrap();
        let state = w.state_mit_kopie(Some(kopie.clone()));
        let app = router_angemeldet(state.clone());
        let (seite, teil, csp) = ausgeliefert(&app).await;
        assert!(seite.contains("kopie") && teil.contains("kopie"), "aus der Kopie: {seite} {teil}");
        assert_eq!(csp, csp_von("kopie"));
        assert_eq!(crate::dienst::betrieb::fassung_name(&state), "build", "die Kopie ist der Grundbau");
        // Ein Bau im Repo (npm run build schreibt web/build neu) ändert nichts.
        std::fs::write(w.0.join("build").join("index.html"), "<!doctype html><title>neu-im-repo</title>").unwrap();
        std::fs::write(w.0.join("build").join("_app").join("teil.js"), "// neu-im-repo").unwrap();
        let (seite, teil, _) = ausgeliefert(&app).await;
        assert!(seite.contains("kopie") && teil.contains("kopie") && !seite.contains("neu-im-repo"), "{seite} {teil}");
        // Ein grüner Bau über den Boten mit demselben Grundbau gilt sofort, ohne Zeiger wieder die Kopie.
        for (ordner, fassung) in [("bin-web", "abc-1"), ("builds/20261027-110000", "abc-2")] {
            std::fs::write(w.0.join(ordner).join("_app").join("version.json"), format!("{{\"version\":\"{fassung}\"}}")).unwrap();
        }
        w.zeiger("20261027-110000");
        assert!(ausgeliefert(&app).await.0.contains("bau-2"));
        assert_eq!(state.web.wartender_bau(), None);
        std::fs::remove_file(w.0.join("builds").join("aktuell")).unwrap();
        assert!(ausgeliefert(&app).await.0.contains("kopie"));
        // Gegenprobe: ohne Kopie web/build (auch wenn sie gesetzt ist, aber index.html fehlt).
        for ohne in [None, Some(w.0.join("gibt-es-nicht"))] {
            let app = router_angemeldet(w.state_mit_kopie(ohne.clone()));
            assert!(ausgeliefert_seite(&app).await.contains("neu-im-repo"), "{ohne:?}");
        }
    }

    /// Mit Kopie gilt ein Bau des Boten nur bei gleichem Grundbau: Ein Bau aus
    /// einem neueren `web/src` (anderer Abdruck) oder ohne Kennung wartet auf
    /// den Neustart, die Kopie bleibt, `umstellen` lehnt ab und die Störung
    /// nennt es. Gegenprobe: Ohne Kopie gilt derselbe Bau sofort.
    #[tokio::test]
    async fn bau_mit_anderem_grundbau_wartet_auf_den_neustart() {
        let w = WebProbe::neu("grundbau");
        let kopie = w.0.join("bin-web");
        std::fs::create_dir_all(kopie.join("_app")).unwrap();
        std::fs::write(kopie.join("index.html"), "<!doctype html><title>kopie</title><script>start('kopie')</script>").unwrap();
        std::fs::write(kopie.join("_app").join("teil.js"), "// kopie").unwrap();
        std::fs::write(kopie.join("_app").join("version.json"), r#"{"version":"abc-1"}"#).unwrap();
        let bau = w.0.join("builds").join("20261027-110000");
        let state = w.state_mit_kopie(Some(kopie.clone()));
        let app = router_angemeldet(state.clone());
        w.zeiger("20261027-110000");
        for fassung in [Some(r#"{"version":"def-2"}"#), Some(r#"{"version":"1794553248560"}"#), None] {
            let datei = bau.join("_app").join("version.json");
            match fassung {
                Some(f) => std::fs::write(&datei, f).unwrap(),
                None => {
                    let _ = std::fs::remove_file(&datei);
                }
            }
            let (seite, teil, csp) = ausgeliefert(&app).await;
            assert!(seite.contains("kopie") && teil.contains("kopie"), "{fassung:?}: {seite}");
            assert_eq!(csp, csp_von("kopie"));
            assert!(state.web.umstellen("20261027-110000").unwrap_err().contains("Grundbau"), "{fassung:?}");
            assert_eq!(state.web.wartender_bau().as_deref(), Some(SATZ_BAU_WARTET));
            assert!(crate::dienst::betrieb::stoerungen(&state).iter().any(|s| s == SATZ_BAU_WARTET));
        }
        // Gegenprobe: ohne Kopie gilt der Bau sofort, keine Störung.
        let ohne = w.state_mit_kopie(None);
        assert!(ausgeliefert(&router_angemeldet(ohne.clone())).await.0.contains("bau-2"));
        assert!(ohne.web.umstellen("20261027-110000").is_ok());
        assert_eq!(ohne.web.wartender_bau(), None);
        assert_eq!(grundbau(&kopie).as_deref(), Some("abc"));
    }

    /// Wartet ein Bau des Boten auf den Neustart, bleibt der letzte gültige
    /// Bau des Boten, nicht die ältere Kopie. Gegenprobe: Ist keiner gemerkt
    /// (frischer Start) oder fehlt seine index.html, kommt die Kopie.
    #[tokio::test]
    async fn wartender_bau_laesst_den_letzten_gueltigen_bau() {
        let w = WebProbe::neu("letzter");
        let kopie = w.0.join("bin-web");
        std::fs::create_dir_all(kopie.join("_app")).unwrap();
        std::fs::write(kopie.join("index.html"), "<!doctype html><title>kopie</title><script>start('kopie')</script>").unwrap();
        std::fs::write(kopie.join("_app").join("teil.js"), "// kopie").unwrap();
        for (ordner, fassung) in [("bin-web", "abc-1"), ("builds/20261027-100000", "abc-2"), ("builds/20261027-110000", "def-3")] {
            std::fs::write(w.0.join(ordner).join("_app").join("version.json"), format!("{{\"version\":\"{fassung}\"}}")).unwrap();
        }
        let state = w.state_mit_kopie(Some(kopie.clone()));
        let app = router_angemeldet(state.clone());
        w.zeiger("20261027-100000");
        assert!(ausgeliefert(&app).await.0.contains("bau-1"));
        w.zeiger("20261027-110000");
        let (seite, teil, csp) = ausgeliefert(&app).await;
        assert!(seite.contains("bau-1") && teil.contains("bau-1"), "der letzte gültige Bau bleibt: {seite}");
        assert_eq!(csp, csp_von("bau-1"));
        assert_eq!(state.web.wartender_bau().as_deref(), Some(SATZ_BAU_WARTET));
        // Gegenprobe: frisch gestartet ist keiner gemerkt.
        let frisch = router_angemeldet(w.state_mit_kopie(Some(kopie.clone())));
        assert!(ausgeliefert(&frisch).await.0.contains("kopie"));
        // Ein zweiter wartender Bau: Das Aufräumen des Boten lässt den zuletzt gültigen Bau stehen.
        let builds = w.0.join("builds");
        let dritter = builds.join("20261027-120000");
        std::fs::create_dir_all(dritter.join("_app")).unwrap();
        std::fs::write(dritter.join("index.html"), "<!doctype html><title>bau-3</title>").unwrap();
        std::fs::write(dritter.join("_app").join("version.json"), r#"{"version":"def-4"}"#).unwrap();
        w.zeiger("20261027-120000");
        assert_eq!(crate::bote::bau::aufraeumen(&builds, crate::bote::bau::BEHALTEN, "20261027-120000", Some(&kopie)), 0);
        let (seite, teil, _) = ausgeliefert(&app).await;
        assert!(seite.contains("bau-1") && teil.contains("bau-1"), "nach zwei wartenden Bauten: {seite}");
        // Gegenprobe: Der gemerkte Bau verschwindet ohne Zeigerwechsel (derselbe
        // Stempel, also der Treffer-Zweig): dann die Kopie, nie ein toter Ordner.
        std::fs::remove_file(builds.join("20261027-100000").join("index.html")).unwrap();
        let (seite, teil, csp) = ausgeliefert(&app).await;
        assert!(seite.contains("kopie") && teil.contains("kopie"), "{seite} {teil}");
        assert_eq!(csp, csp_von("kopie"));
    }

    /// Im Dienst (aus dienst.toml) liegt die Kopie neben dem Programm
    /// (`bin\web`); ohne diese Verdrahtung läse der Dienst wieder web/build.
    #[cfg(windows)]
    #[tokio::test]
    async fn aus_einstellungen_nimmt_die_kopie_neben_dem_programm() {
        let ordner = std::env::temp_dir().join(format!("dashboard-webkopie-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&ordner);
        std::fs::create_dir_all(&ordner).unwrap();
        let o = ordner.to_string_lossy().replace('\\', "/");
        let text = format!(
            "schema = 1\nrepo = \"{}\"\nport = 8793\n[pfade]\ndaten = \"{o}/d.redb\"\nsicherung = \"{o}/s\"\nprotokoll = \"{o}/p\"\n[konten]\ninhaber_sid = \"\"\ndienst_sid = \"\"\n[bote]\npipe = \"dashboard-bote-gibt-es-nicht-{}\"\nprogramm = \"{o}/dashboard-bote.exe\"\n",
            env!("CARGO_MANIFEST_DIR").replace('\\', "/"),
            std::process::id()
        );
        let e = crate::dienst::einstellungen::Einstellungen::aus_text(&ordner.join("dienst.toml"), &text).unwrap();
        let (state, abschalter) = AppState::aus_einstellungen(&e).unwrap();
        let erwartet = std::env::current_exe().unwrap().parent().unwrap().join(WEB_KOPIE);
        assert_eq!(state.web.kopie.as_deref(), Some(erwartet.as_path()));
        abschalter.send_replace(true);
        drop(state);
        let _ = std::fs::remove_dir_all(&ordner);
    }

    /// Nur die Seite (ohne CSP-Vergleich, für eine index.html ohne Skript).
    async fn ausgeliefert_seite(app: &Router) -> String {
        let res = app.clone().oneshot(get("/projects/dashboard")).await.unwrap();
        String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap()
    }

    impl Drop for WebProbe {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Seite, Datei unter `_app/` und CSP, wie sie gerade ausgeliefert werden.
    async fn ausgeliefert(app: &Router) -> (String, String, String) {
        let res = app.clone().oneshot(get("/projects/dashboard")).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let csp = res.headers()[header::CONTENT_SECURITY_POLICY].to_str().unwrap().to_string();
        let seite = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
        let res = app.clone().oneshot(get("/_app/teil.js")).await.unwrap();
        let teil = String::from_utf8(res.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
        (seite, teil, csp)
    }

    fn csp_von(inhalt: &str) -> String {
        let html = format!("<!doctype html><title>{inhalt}</title><script>start('{inhalt}')</script>");
        sicherheit::csp_oberflaeche(&sicherheit::inline_skript_hashes(&html))
    }

    /// Nach einem Bau gilt der neue Ordner sofort, ohne Neustart: Zeiger
    /// umschreiben, die Antwort ändert sich (Seite, Dateien, CSP).
    #[tokio::test]
    async fn auslieferung_folgt_dem_zeiger_ohne_neustart() {
        let w = WebProbe::neu("folgt");
        let app = router_angemeldet(w.state());
        let (seite, teil, csp) = ausgeliefert(&app).await;
        assert!(seite.contains("web-build") && teil.contains("web-build"), "ohne Zeiger web/build: {seite}");
        assert_eq!(csp, csp_von("web-build"));
        for bau in ["bau-1", "bau-2", "bau-1"] {
            w.zeiger(if bau == "bau-1" { "20261027-100000" } else { "20261027-110000" });
            let (seite, teil, csp) = ausgeliefert(&app).await;
            assert!(seite.contains(bau) && teil.contains(bau), "{bau}: {seite} {teil}");
            assert_eq!(csp, csp_von(bau), "die CSP folgt dem Ordner");
        }
        // Zeiger weg: wieder web/build.
        std::fs::remove_file(w.0.join("builds").join("aktuell")).unwrap();
        assert!(ausgeliefert(&app).await.0.contains("web-build"));
    }

    /// Ein Zeiger, der nicht gilt, führt zu web/build: Pfadteile, fehlender
    /// Ordner, Ordner ohne index.html, Datei statt Ordner, leer, Verknüpfung.
    #[tokio::test]
    async fn ungueltiger_zeiger_nimmt_web_build() {
        let w = WebProbe::neu("ungueltig");
        let builds = w.0.join("builds");
        std::fs::create_dir_all(builds.join("ohne-index")).unwrap();
        std::fs::write(builds.join("eine-datei"), "x").unwrap();
        let app = router_angemeldet(w.state());
        let falsch = [
            "../build",
            "..",
            "20261027-100000/..",
            "20261027-100000\\..\\..\\build",
            "C:\\Windows",
            "/20261027-100000",
            "gibt-es-nicht",
            "ohne-index",
            "eine-datei",
            "",
            ".versteckt",
        ];
        for text in falsch {
            w.zeiger("20261027-110000");
            assert!(ausgeliefert(&app).await.0.contains("bau-2"), "erst gültig");
            w.zeiger(text);
            let (seite, _, csp) = ausgeliefert(&app).await;
            assert!(seite.contains("web-build"), "„{text}“: {seite}");
            assert_eq!(csp, csp_von("web-build"), "„{text}“");
            assert!(zeiger_ordner(&builds).is_err(), "„{text}“");
        }
        // Zu groß.
        std::fs::write(builds.join("aktuell"), "2".repeat(300)).unwrap();
        assert!(ausgeliefert(&app).await.0.contains("web-build"));
        // Eine Verknüpfung (Junction) auf einen gültigen Ordner zählt nicht.
        #[cfg(windows)]
        {
            let ziel = builds.join("20261027-110000");
            let verknuepfung = builds.join("verknuepft");
            let out = crate::dienst::programme::befehl(crate::dienst::programme::Werkzeug::Cmd)
                .unwrap()
                .args(["/c", "mklink", "/J"])
                .arg(&verknuepfung)
                .arg(&ziel)
                .output()
                .unwrap();
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
            w.zeiger("verknuepft");
            assert!(ausgeliefert(&app).await.0.contains("web-build"), "Junction");
            assert!(zeiger_ordner(&builds).unwrap_err().contains("Verknüpfung"));
            std::fs::remove_dir(&verknuepfung).unwrap();
        }
        // Und wieder gültig.
        w.zeiger(" 20261027-100000\r\n");
        assert!(ausgeliefert(&app).await.0.contains("bau-1"));
    }

    #[tokio::test]
    async fn health() {
        assert_eq!(get_page("/healthz").await.1, "ok");
    }

    #[tokio::test]
    async fn betrieb_nennt_modus_und_fassung() {
        let (status, body) = get_page("/api/betrieb").await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["modus"], "umgebung");
        assert_eq!(v["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(v["einstellungen"], serde_json::Value::Null);
        assert_eq!(v["bote"], "nicht vorgesehen");
        assert_eq!(v["bote_seit"], serde_json::Value::Null);
        assert!(chrono::DateTime::parse_from_rfc3339(v["gestartet"].as_str().unwrap()).is_ok(), "{body}");
        let state = AppState::fuer_tests(&beispiel()).mit_modus(Modus::Dienst);
        let res = router(state).oneshot(get("/api/betrieb")).await.unwrap();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(v["modus"], "dienst");
    }
}
