<#
.SYNOPSIS
    Prüft das Einspielen der Oberfläche in scripts\Dienst.ps1 an einer Testwurzel.
.DESCRIPTION
    Der Dienst liefert die Oberfläche aus bin\web, einer Kopie, die nur
    Dienst.ps1 -Neustart zusammen mit den Programmen wechselt. Dieses Skript
    liest Dienst.ps1 als AST, holt nur OberflaecheBereitlegen, TauscheProgramme
    und OberflaecheZurueck heraus (der Rest des Skripts läuft nicht, es fasst
    keinen Dienst an) und ruft sie gegen eine Testwurzel im Temp-Ordner auf
    (bin mit erfundenen Programmen, ein Repo-web\build). Geprüft: erster
    Neustart ohne bin\web, zweiter mit bin\web.alt, Rückweg, gescheiterter
    Austausch (Programm fehlt, Oberfläche fehlt) bringt Programme und
    Oberfläche zurück, ein Bau ohne index.html wird nicht bereitgelegt, ein
    Bau im Repo nach dem Einspielen ändert bin\web nicht. Die Testwurzel
    verschwindet danach wieder.

    Ohne Pester, ohne Administrator, ohne Netz. Aufruf aus dem Repo, in beiden Shells:

        pwsh -NoProfile -File tests\skripte\Dienst-Oberflaeche.ps1
        powershell.exe -NoProfile -ExecutionPolicy Bypass -File tests\skripte\Dienst-Oberflaeche.ps1
.NOTES
    Exit-Codes: 0 alles gut, 1 mindestens eine Prüfung gescheitert.
#>
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$skript = Join-Path (Join-Path $repo 'scripts') 'Dienst.ps1'

$token = $null; $parseFehler = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($skript, [ref]$token, [ref]$parseFehler)
if ($parseFehler.Count -gt 0) {
    foreach ($f in $parseFehler) { Write-Host ("FEHLER Dienst.ps1:{0}: {1}" -f $f.Extent.StartLineNumber, $f.Message) }
    exit 1
}
$gesucht = @('OberflaecheBereitlegen', 'TauscheProgramme', 'OberflaecheZurueck')
$funktionen = $ast.FindAll({ param($n) $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $gesucht -contains $n.Name }, $true)
foreach ($name in $gesucht) {
    $f = @($funktionen | Where-Object { $_.Name -eq $name })
    if ($f.Count -ne 1) { Write-Host "FEHLER Dienst.ps1: Funktion $name fehlt (oder steht doppelt)"; exit 1 }
    . ([ScriptBlock]::Create($f[0].Extent.Text))
}

$script:fehler = 0
function Pruefe([string]$was, [bool]$ok, [string]$sonst = '') {
    if ($ok) { Write-Host "ok     $was" }
    else { Write-Host "FEHLER $was $sonst"; $script:fehler++ }
}

# Testwurzel: bin (wie unter ProgramData), ein web\build des Repos, zwei Bau-Ordner der Programme.
$wurzel = Join-Path ([IO.Path]::GetTempPath()) ("dashboard-dienst-oberflaeche-{0}" -f [guid]::NewGuid().ToString('N'))
$script:bin = Join-Path $wurzel 'bin'
$build = Join-Path $wurzel 'repo\web\build'
$programme = @('dashboard.exe', 'dashboard-bote.exe', 'dashboard-herzschlag.exe')

function Inhalt([string]$datei) { if (Test-Path -LiteralPath $datei) { [IO.File]::ReadAllText($datei) } else { '<fehlt>' } }
function Seite([string]$ordner) { Inhalt (Join-Path $ordner 'index.html') }
function Bau([string]$text) {
    if (Test-Path -LiteralPath $build) { Remove-Item -LiteralPath $build -Recurse -Force }
    New-Item -ItemType Directory -Force -Path (Join-Path $build '_app') | Out-Null
    [IO.File]::WriteAllText((Join-Path $build 'index.html'), $text)
    [IO.File]::WriteAllText((Join-Path $build '_app\teil.js'), "// $text")
}
function Programme([string]$ordner, [string]$text, [string[]]$ohne = @()) {
    New-Item -ItemType Directory -Force -Path $ordner | Out-Null
    foreach ($n in $programme) { if ($ohne -notcontains $n) { [IO.File]::WriteAllText((Join-Path $ordner $n), $text) } }
    $ordner
}
$web = Join-Path $script:bin 'web'
$webAlt = Join-Path $script:bin 'web.alt'
$webNeu = Join-Path $script:bin 'web.neu'

try {
    $null = Programme $script:bin 'p0'

    # 1. Erster Neustart: bin\web gibt es noch nicht.
    Bau 'u1'
    $f = OberflaecheBereitlegen $build
    Pruefe '1 bereitgelegt' (-not $f -and (Seite $webNeu) -eq 'u1' -and (Inhalt (Join-Path $webNeu '_app\teil.js')) -eq '// u1') "$f"
    $f = TauscheProgramme (Programme (Join-Path $wurzel 'q1') 'p1') $webNeu
    Pruefe '1 getauscht' (-not $f -and $script:oberflaecheGetauscht) "$f"
    Pruefe '1 bin\web ist u1, kein web.alt, kein web.neu' ((Seite $web) -eq 'u1' -and -not (Test-Path $webAlt) -and -not (Test-Path $webNeu))
    Pruefe '1 Programme p1, vorige p0 als .alt.exe' ((Inhalt (Join-Path $script:bin 'dashboard.exe')) -eq 'p1' -and (Inhalt (Join-Path $script:bin 'dashboard.alt.exe')) -eq 'p0')

    # Ein Bau im Repo danach ändert bin\web nicht.
    Bau 'im-repo'
    Pruefe '1 Bau im Repo ändert bin\web nicht' ((Seite $web) -eq 'u1')

    # 2. Zweiter Neustart: die vorige Oberfläche wird bin\web.alt.
    Bau 'u2'
    $f = OberflaecheBereitlegen $build
    $f = if ($f) { $f } else { TauscheProgramme (Programme (Join-Path $wurzel 'q2') 'p2') $webNeu }
    Pruefe '2 getauscht' (-not $f -and $script:oberflaecheGetauscht) "$f"
    Pruefe '2 bin\web u2, web.alt u1' ((Seite $web) -eq 'u2' -and (Seite $webAlt) -eq 'u1')

    # 3. Rückweg: Programme (wie Dienst.ps1, Move-Item -Force) und Oberfläche gemeinsam.
    foreach ($n in $programme) { Move-Item -LiteralPath (Join-Path $script:bin ($n -replace '\.exe$', '.alt.exe')) -Destination (Join-Path $script:bin $n) -Force }
    $f = OberflaecheZurueck
    Pruefe '3 zurück' (-not $f) "$f"
    Pruefe '3 bin\web u1, dashboard.exe p1, kein web.alt, kein web.verworfen' ((Seite $web) -eq 'u1' -and (Inhalt (Join-Path $script:bin 'dashboard.exe')) -eq 'p1' -and -not (Test-Path $webAlt) -and -not (Test-Path (Join-Path $script:bin 'web.verworfen')))
    $f = OberflaecheZurueck
    Pruefe '3 ohne web.alt: Satz, bin\web bleibt' ($f -and (Seite $web) -eq 'u1') "$f"

    # 4. Austausch scheitert am Programm: Programme und Oberfläche wie vorher.
    Bau 'u3'
    $null = OberflaecheBereitlegen $build
    $f = TauscheProgramme (Programme (Join-Path $wurzel 'q3') 'p3' @('dashboard-herzschlag.exe')) $webNeu
    Pruefe '4 Satz' ($f -match 'gescheitert' -and -not $script:oberflaecheGetauscht) "$f"
    Pruefe '4 Programme p1, bin\web u1' ((Inhalt (Join-Path $script:bin 'dashboard.exe')) -eq 'p1' -and (Inhalt (Join-Path $script:bin 'dashboard-herzschlag.exe')) -eq 'p1' -and (Seite $web) -eq 'u1')

    # 5. Austausch scheitert an der Oberfläche (web.neu fehlt): bin\web und Programme zurück.
    Remove-Item -LiteralPath $webNeu -Recurse -Force
    $f = TauscheProgramme (Programme (Join-Path $wurzel 'q4') 'p4') $webNeu
    Pruefe '5 Satz' ($f -match 'gescheitert' -and -not $script:oberflaecheGetauscht) "$f"
    Pruefe '5 Programme p1, bin\web u1, kein web.alt' ((Inhalt (Join-Path $script:bin 'dashboard.exe')) -eq 'p1' -and (Seite $web) -eq 'u1' -and -not (Test-Path $webAlt))

    # 6. Ein Bau ohne index.html wird nicht bereitgelegt; ein altes web.neu fällt weg.
    New-Item -ItemType Directory -Force -Path $webNeu | Out-Null
    Remove-Item -LiteralPath (Join-Path $build 'index.html')
    $f = OberflaecheBereitlegen $build
    Pruefe '6 Satz, kein web.neu' ($f -match 'index.html' -and -not (Test-Path $webNeu)) "$f"

    # 7. Ohne Oberfläche (kein web\package.json): nur die Programme, bin\web bleibt.
    $f = TauscheProgramme (Programme (Join-Path $wurzel 'q5') 'p5') $null
    Pruefe '7 nur Programme' (-not $f -and -not $script:oberflaecheGetauscht -and (Seite $web) -eq 'u1' -and (Inhalt (Join-Path $script:bin 'dashboard.exe')) -eq 'p5') "$f"
} finally {
    Remove-Item -LiteralPath $wurzel -Recurse -Force -ErrorAction SilentlyContinue
}

if ($script:fehler -gt 0) { Write-Host "$($script:fehler) Prüfung(en) gescheitert."; exit 1 }
Write-Host 'Alle Prüfungen gut.'
exit 0
