//! Nur für Tests: einen Test allein in einem eigenen Prozess ausführen.
//!
//! Gebraucht von den Tests, die das Protokoll mitschreiben. `tracing` merkt
//! sich je Stelle im Code prozessweit, ob jemand zuhört, und entscheidet das,
//! wenn die Stelle zum ersten Mal erreicht wird oder sich die Zuhörer ändern.
//! Eine Mitschrift, die nur für einen Thread gilt, verliert darum Zeilen,
//! sobald andere Tests im selben Prozess dieselben Stellen erreichen: Der
//! Test bestünde dann, ohne etwas zu belegen. Allein im Prozess gibt es
//! diesen Wettlauf nicht.
//!
//! Ein solcher Test ruft zuerst [`im_eigenen_prozess`]. Im gewöhnlichen Lauf
//! startet das dasselbe Testprogramm noch einmal, nur mit diesem einen Test,
//! verlangt, dass er dort besteht, und gibt `false` zurück: Der Test ist
//! fertig. Im eigenen Prozess gibt es `true` zurück, und der Test tut seine
//! eigentliche Arbeit.

/// Gesetzt im Prozess, der nur den einen Test ausführt.
const UMGEBUNG: &str = "DASHBOARD_TEST_ALLEIN";

/// Siehe Modulkommentar. `name` ist der volle Name des Tests, wie `cargo test` ihn zeigt.
pub fn im_eigenen_prozess(name: &str) -> bool {
    if std::env::var_os(UMGEBUNG).is_some() {
        return true;
    }
    let programm = std::env::current_exe().expect("Pfad des Testprogramms");
    let aus = crate::dienst::programme::eigener_befehl(programm)
        .args([name, "--exact", "--test-threads=1", "--nocapture"])
        .env(UMGEBUNG, "1")
        .output()
        .expect("das Testprogramm startet");
    let text = format!("{}\n{}", String::from_utf8_lossy(&aus.stdout), String::from_utf8_lossy(&aus.stderr));
    // Genau ein Test muss gelaufen sein: Mit falschem Namen bestünde der Lauf leer.
    assert!(aus.status.success() && text.contains("test result: ok. 1 passed"), "{name} besteht allein im eigenen Prozess nicht:\n{text}");
    false
}
