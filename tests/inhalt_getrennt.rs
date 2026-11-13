//! Umzug des Inhalts (Entscheidung `github-trennung`,
//! [docs_oeffentlich]): Nach dem Umzug liegt im Code-Repo kein privater
//! Inhalt mehr. Eingecheckt sein dürfen dann unter `projects/` nur
//! `_vorlage`, dazu kein `data/`, kein `docs/entscheidungen/`, kein
//! `docs/architektur-stand.md` und keine erzeugten Typen einzelner Projekte
//! (`web/src/lib/vertrag/projekte/`).
//!
//! Vor dem Umzug ist alles davon noch eingecheckt. Ein halber Umzug ist der
//! Fehler: Ist einer der alten Orte schon leer, müssen alle leer sein. (Der
//! Pre-Push-Prüfer weist die Orte ohnehin ab.)

use dashboard::dienst::programme::{self, Werkzeug};

/// Die eingecheckten Dateien unter `pfad` (leer ohne git oder außerhalb eines Repos).
fn eingecheckt(pfad: &str) -> Option<Vec<String>> {
    let aus = programme::befehl(Werkzeug::Git).ok()?.args(["-C", env!("CARGO_MANIFEST_DIR"), "ls-files", "-z", "--", pfad]).output().ok()?;
    if !aus.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&aus.stdout).split('\0').filter(|s| !s.is_empty()).map(String::from).collect())
}

#[test]
fn alte_orte_ganz_oder_gar_nicht() {
    let Some(projekte) = eingecheckt("projects") else {
        assert!(dashboard::ueberspringen("kein Git-Repo (etwa ein Paket ohne .git)"));
        return;
    };
    let fremde_projekte: Vec<&String> = projekte.iter().filter(|p| !p.starts_with("projects/_vorlage/")).collect();
    let orte = [
        ("projects/<slug> (außer _vorlage)", !fremde_projekte.is_empty()),
        ("data/", !eingecheckt("data").unwrap_or_default().is_empty()),
        ("docs/entscheidungen/", !eingecheckt("docs/entscheidungen").unwrap_or_default().is_empty()),
        ("docs/architektur-stand.md", !eingecheckt("docs/architektur-stand.md").unwrap_or_default().is_empty()),
    ];
    let noch: Vec<&str> = orte.iter().filter(|(_, da)| *da).map(|(n, _)| *n).collect();
    let umgezogen = !orte.iter().any(|(n, da)| *n == "docs/entscheidungen/" && *da);
    if umgezogen {
        assert!(noch.is_empty(), "Der Inhalt ist umgezogen, aber im Code-Repo ist noch eingecheckt: {}", noch.join(", "));
        let typen = eingecheckt("web/src/lib/vertrag/projekte").unwrap_or_default();
        assert!(typen.is_empty(), "Erzeugte Typen einzelner Projekte gehören nicht ins Code-Repo: {} Dateien", typen.len());
        assert!(projekte.iter().all(|p| p.starts_with("projects/_vorlage/")));
    } else {
        assert_eq!(noch.len(), orte.len(), "Halber Umzug: Nur ein Teil des Inhalts ist noch im Code-Repo ({}).", noch.join(", "));
    }
}

/// Bindet der Text erzeugte Typen eines einzelnen Projekts ein
/// (`$vertrag/projekte/<slug>`)? Die liegen nach dem Umzug nicht mehr im
/// Code-Repo; ein frischer Klon ohne Inhalt bekäme sonst einen Fehler in
/// svelte-check, und der Projektname stünde im Basis-Repo.
fn bindet_projekt_typen_ein(text: &str) -> bool {
    text.contains("$vertrag/projekte/") || text.contains("lib/vertrag/projekte/")
}

#[test]
fn die_suche_nach_projekt_typen_greift() {
    assert!(bindet_projekt_typen_ein("import type { Stufen } from '$vertrag/projekte/beispiel';"));
    assert!(bindet_projekt_typen_ein("import x from '../lib/vertrag/projekte/beispiel/index';"));
    assert!(!bindet_projekt_typen_ein("import type { Block } from '$vertrag';"));
    assert!(!bindet_projekt_typen_ein("import { projekte } from '$lib/projekte.generated';"));
}

/// Keine eingecheckte Datei unter `web/` bindet die Typen eines einzelnen
/// Projekts ein (Abnahme des Umzugs, Schritt 8): Das Basis-Repo muss ohne den
/// Inhalt bauen und prüfen.
#[test]
fn web_bindet_keine_projekt_typen_ein() {
    let Some(dateien) = eingecheckt("web") else {
        assert!(dashboard::ueberspringen("kein Git-Repo (etwa ein Paket ohne .git)"));
        return;
    };
    if !eingecheckt("docs/entscheidungen").unwrap_or_default().is_empty() {
        // Vor dem Umzug liegen die Typen noch im Code-Repo.
        return;
    }
    let wurzel = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let funde: Vec<&String> = dateien
        .iter()
        .filter(|d| [".ts", ".js", ".svelte", ".mjs"].iter().any(|e| d.ends_with(e)))
        .filter(|d| std::fs::read_to_string(wurzel.join(d)).is_ok_and(|t| bindet_projekt_typen_ein(&t)))
        .collect();
    assert!(funde.is_empty(), "Diese Dateien binden Typen eines einzelnen Projekts ein (gehört ins Projekt im Inhalt): {funde:?}");
}
