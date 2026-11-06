//! Vertragstypen der Module (kein Wächter: Der Wächter liegt im Code-Repo,
//! `dashboard.exe pruefe-zusammenbau`, und läuft vor jedem cargo-Aufruf hier;
//! Dienst.ps1 ruft diesen Test nicht).

/// Vertragstypen aller Module aktuell (web/src/lib/vertrag/projekte/<slug>/
/// im Code-Repo, dort nicht eingecheckt); sonst neu geschrieben und rot.
#[test]
fn vertrag_der_module_aktuell() {
    for m in dashboard_lokal::module() {
        dashboard_projekt::vertrag::vertrag_pruefen(&*m.modul);
    }
}
