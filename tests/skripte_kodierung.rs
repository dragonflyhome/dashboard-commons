//! Jedes Skript in `scripts\*.ps1` beginnt mit der UTF-8-BOM. Ohne sie liest
//! Windows PowerShell 5.1 die Datei als ANSI, und die Umlaute in Sätzen und
//! Kommentaren werden falsch (Auflage aus dem Hotfix Anmeldung ohne curl:
//! Dienst-Einrichten.ps1 läuft in 5.1 und bindet Anfrage.ps1 ein).

#[test]
fn jedes_skript_hat_die_utf8_bom() {
    let ordner = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts");
    let mut ohne = Vec::new();
    let mut zahl = 0;
    for eintrag in std::fs::read_dir(&ordner).expect("scripts lesbar") {
        let pfad = eintrag.unwrap().path();
        if pfad.extension().and_then(|e| e.to_str()) != Some("ps1") {
            continue;
        }
        zahl += 1;
        let bytes = std::fs::read(&pfad).unwrap();
        if !bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
            ohne.push(pfad.file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    assert!(zahl > 0, "keine Skripte in {}", ordner.display());
    assert!(ohne.is_empty(), "ohne UTF-8-BOM (in UTF-8 mit BOM speichern): {ohne:?}");
}
