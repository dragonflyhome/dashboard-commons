//! Keine Funktion in `scripts\*.ps1` verdeckt einen Parameter des Skripts.
//! PowerShell unterscheidet bei Variablen nicht nach Groß- und Kleinschreibung
//! und sucht sie dynamisch: Legt eine Funktion `$konto` an, sieht jede
//! Funktion, die sie aufruft, dort statt `-Konto` diesen Wert. So prüfte
//! Dienst-Einrichten.ps1 -NurLeserecht -Inhalt im Umzug (Schritt 5) gegen das
//! Profil von NT SERVICE\Dashboard statt gegen das vom Konto des Inhabers.

use std::path::{Path, PathBuf};

use dashboard::dienst::programme::{self, Werkzeug};

/// Liest jede Datei mit dem Parser von PowerShell und nennt je Fund eine
/// Zeile „<datei>: <funktion>: $<name>“: eine Zuweisung, ein Parameter oder
/// eine foreach-Variable in einer Funktion, die wie ein Skript-Parameter heißt
/// (ohne `$script:` und `$global:`).
const SUCHE: &str = r#"
foreach ($datei in $args) {
  $fehler = $null
  $ast = [System.Management.Automation.Language.Parser]::ParseFile($datei, [ref]$null, [ref]$fehler)
  if ($fehler) { "${datei}: nicht lesbar"; continue }
  if (-not $ast.ParamBlock) { continue }
  $namen = @($ast.ParamBlock.Parameters | ForEach-Object { $_.Name.VariablePath.UserPath })
  $funktionen = $ast.FindAll({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] }, $true)
  foreach ($f in $funktionen) {
    $innen = @()
    $innen += $f.FindAll({ param($n) $n -is [System.Management.Automation.Language.AssignmentStatementAst] -and $n.Left -is [System.Management.Automation.Language.VariableExpressionAst] }, $true) | ForEach-Object { $_.Left.VariablePath }
    $innen += $f.FindAll({ param($n) $n -is [System.Management.Automation.Language.ParameterAst] }, $true) | ForEach-Object { $_.Name.VariablePath }
    $innen += $f.FindAll({ param($n) $n -is [System.Management.Automation.Language.ForEachStatementAst] }, $true) | ForEach-Object { $_.Variable.VariablePath }
    foreach ($v in $innen) {
      if (-not $v.IsScript -and -not $v.IsGlobal -and ($namen -contains $v.UserPath)) { "${datei}: $($f.Name): `$$($v.UserPath)" }
    }
  }
}
'FERTIG'
"#;

/// Funde je Datei; `None`, wenn keine PowerShell da ist.
fn funde(dateien: &[PathBuf]) -> Option<Vec<String>> {
    for w in [Werkzeug::PowerShell7, Werkzeug::WindowsPowerShell] {
        let Ok(mut c) = programme::befehl(w) else { continue };
        // Je Aufruf ein eigener Name: beide Tests laufen im selben Prozess
        // gleichzeitig, und einer löschte sonst das Skript des anderen.
        static NUMMER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NUMMER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let skript = std::env::temp_dir().join(format!("dashboard-schatten-{}-{n}.ps1", std::process::id()));
        std::fs::write(&skript, SUCHE).unwrap();
        let o = c.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]).arg(&skript).args(dateien).output();
        let _ = std::fs::remove_file(&skript);
        let o = o.ok()?;
        let aus = String::from_utf8_lossy(&o.stdout).into_owned();
        assert!(aus.lines().any(|z| z.trim() == "FERTIG"), "Suche lief nicht zu Ende:\n{aus}\n{}", String::from_utf8_lossy(&o.stderr));
        return Some(aus.lines().map(str::trim).filter(|z| !z.is_empty() && *z != "FERTIG").map(String::from).collect());
    }
    None
}

fn skripte(ordner: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(ordner)
        .expect("Ordner lesbar")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("ps1"))
        .collect();
    v.sort();
    v
}

#[test]
fn keine_funktion_verdeckt_einen_skript_parameter() {
    let wurzel = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut dateien = skripte(&wurzel.join("scripts"));
    dateien.extend(skripte(&wurzel.join("tests").join("skripte")));
    assert!(dateien.len() > 10, "zu wenige Skripte gefunden: {dateien:?}");
    let Some(f) = funde(&dateien) else {
        dashboard::ueberspringen("keine PowerShell gefunden");
        return;
    };
    assert!(f.is_empty(), "Funktionen verdecken einen Parameter des Skripts (bitte umbenennen, etwa $dienstKonto):\n{}", f.join("\n"));
}

#[test]
fn die_suche_findet_verdeckte_parameter() {
    let ordner = std::env::temp_dir().join(format!("dashboard-schatten-probe-{}", std::process::id()));
    std::fs::create_dir_all(&ordner).unwrap();
    let probe = ordner.join("probe.ps1");
    std::fs::write(
        &probe,
        "param([string]$Konto, [string]$Inhalt, [switch]$AufMain)\n\
         function Lesen { $konto = 'NT SERVICE\\Dashboard'; Profil }\n\
         function Profil { $Konto }\n\
         function Schreiben { $inhalt = @('a'); foreach ($x in $inhalt) { $x } }\n\
         function Pruefen([bool]$aufMain) { $aufMain }\n\
         function Gut { $script:Konto = 'x'; $dienstKonto = 'y'; $dienstKonto }\n",
    )
    .unwrap();
    let f = funde(std::slice::from_ref(&probe));
    let _ = std::fs::remove_dir_all(&ordner);
    let Some(f) = f else {
        dashboard::ueberspringen("keine PowerShell gefunden");
        return;
    };
    let hat = |fn_name: &str, var: &str| f.iter().any(|z| z.ends_with(&format!(": {fn_name}: ${var}")));
    assert!(hat("Lesen", "konto"), "{f:?}");
    assert!(hat("Schreiben", "inhalt"), "{f:?}");
    assert!(hat("Pruefen", "aufMain"), "{f:?}");
    assert!(!f.iter().any(|z| z.contains(": Gut: ") || z.contains(": Profil: ")), "{f:?}");
    assert_eq!(f.len(), 3, "{f:?}");
}
