# Die Orte des privaten Inhalts im Code-Repo (Umzug des Inhalts, Entscheidung
# github-trennung mit [docs_oeffentlich]). Eingebunden von
# Inhalt-Umziehen.ps1 und Dienst-Einrichten.ps1 (mit `. $PSScriptRoot\Inhalt-Orte.ps1`),
# damit beide dieselben Orte meinen. Ändert nichts, legt nur Werte fest.

# Was in den Inhalt zieht (relativ zum Code-Repo, mit /). projects/_vorlage bleibt.
$InhaltZiehtUm = @('projects', 'data', 'docs/entscheidungen', 'docs/architektur-stand.md')
# Was nach dem Umzug im Code-Repo nicht mehr eingecheckt ist (git rm).
# Dazu kommen alle Projekte unter projects/ außer _vorlage (siehe InhaltAlteProjekte).
$InhaltRaus = @('data', 'docs/entscheidungen', 'docs/architektur-stand.md', 'web/src/lib/vertrag/projekte')
# Zeilen, die nach dem Umzug in die .gitignore des Code-Repos kommen.
$InhaltIgnorieren = @(
    '# Umzug des Inhalts: privater Inhalt liegt in dashboard-inhalt, nie hier.',
    '/projects/*',
    '!/projects/_vorlage/',
    '/data/',
    '/docs/entscheidungen/',
    '/docs/architektur-stand.md',
    '/web/src/lib/vertrag/projekte/'
)

# Die eingecheckten Projekte unter projects/ außer _vorlage (Ordnernamen).
function InhaltAlteProjekte([string]$repo) {
    $liste = & git -C $repo ls-files -z -- projects
    if ($LASTEXITCODE -ne 0) { throw "git ls-files in $repo ist gescheitert." }
    @(($liste -join '') -split "`0" | Where-Object { $_ -match '^projects/([^/]+)/' -and $Matches[1] -ne '_vorlage' } |
        ForEach-Object { ($_ -split '/')[1] } | Sort-Object -Unique)
}

# Darf $pfad der Ordner des privaten Inhalts sein (Dienst-Einrichten.ps1
# -Inhalt und die Ordner aus [inhalt], Inhalt-Umziehen.ps1)? $null, wenn ja;
# sonst ein Satz. Verlangt: ein vollständiger Pfad mit Laufwerk, ohne . und ..,
# keine Laufwerkswurzel, im Profil $profil (aber nicht das Profil selbst),
# nicht das Code-Repo $repo, nicht darin und nicht darüber; und kein
# Reparse-Punkt (Junction, symbolischer Link) auf dem Weg vom Laufwerk bis zum
# Ordner, so weit es ihn schon gibt. Öffnet keinen Ordner: Es liest nur die
# Eckdaten der Ordner auf dem Weg (wie Wurzel-Unsicher, ohne hineinzusehen).
# Läuft in Windows PowerShell 5.1 und PowerShell 7.
function Inhalt-Unsicher([string]$pfad, [string]$profil, [string]$repo) {
    $unter = { param([string]$a, [string]$b) ($a + '\').StartsWith($b + '\', [StringComparison]::OrdinalIgnoreCase) }
    if (-not $pfad -or -not $pfad.Trim()) { return 'Der Inhalts-Ordner fehlt.' }
    $p = $pfad -replace '/', '\'
    if ($p -notmatch '^[A-Za-z]:\\') { return "Der Inhalts-Ordner '$pfad' ist kein vollständiger Pfad mit Laufwerk (etwa C:\Users\<name>\dev\dashboard-inhalt)." }
    $teile = @($p.Substring(3).Split('\') | Where-Object { $_ -ne '' })
    foreach ($t in $teile) {
        if ($t -eq '..' -or $t -eq '.') { return "Der Inhalts-Ordner '$pfad' enthält '$t'; bitte den Weg ohne . und .. angeben." }
        if ($t -match '[<>:"|?*~]' -or $t.TrimEnd(' ', '.') -ne $t) { return "Der Inhalts-Ordner '$pfad' hat einen ungewöhnlichen Teil '$t' (Sonderzeichen, Kurzname mit ~ oder Punkt am Ende)." }
    }
    if ($teile.Count -eq 0) { return "Der Inhalts-Ordner '$pfad' ist eine Laufwerkswurzel." }
    $voll = $p.Substring(0, 3) + ($teile -join '\')
    if (-not $profil) { return 'Das Profil des Kontos ist unbekannt; ohne es wird kein Inhalts-Ordner angenommen.' }
    $prof = ($profil -replace '/', '\').TrimEnd('\')
    if ($voll -ieq $prof) { return "Der Inhalts-Ordner '$pfad' ist das Profil selbst; bitte einen Ordner darin nehmen (etwa $prof\dev\dashboard-inhalt)." }
    if (-not (& $unter $voll $prof)) { return "Der Inhalts-Ordner '$pfad' liegt nicht im Profil $prof." }
    if ($repo) {
        $r = ($repo -replace '/', '\').TrimEnd('\')
        if ($voll -ieq $r -or (& $unter $voll $r)) { return "Der Inhalts-Ordner '$pfad' ist das Code-Repo oder liegt darin; der Inhalt liegt daneben." }
        if (& $unter $r $voll) { return "Im Inhalts-Ordner '$pfad' liegt das Code-Repo; der Inhalt liegt daneben." }
    }
    $weg = $voll.Substring(0, 3)
    foreach ($t in $teile) {
        $weg = Join-Path $weg $t
        if (-not (Test-Path -LiteralPath $weg)) { break }
        $e = Get-Item -LiteralPath $weg -Force
        if (($e.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { return "Auf dem Weg zum Inhalts-Ordner ist $weg eine Verknüpfung (Reparse-Punkt); der Inhalt liegt nur in echten Ordnern." }
        if (-not $e.PSIsContainer) { return "Auf dem Weg zum Inhalts-Ordner ist $weg eine Datei, kein Ordner." }
    }
    return $null
}
