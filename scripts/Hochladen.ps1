<#
.SYNOPSIS
    Der einzige Weg nach draußen: prüft main ganz und lädt nur dann hoch.
.DESCRIPTION
    Den pre-push-Hook (scripts\hooks\pre-push) kann man mit --no-verify oder
    aus einem anderen Klon umgehen. Darum wird nur über dieses Skript
    hochgeladen. Hinaus geht refs/heads/main nach refs/heads/main. Das Skript
      1. verlangt core.hooksPath = scripts/hooks (damit auch der Hook prüft),
      2. bricht ab, wenn main den Stand des fernen main nicht enthält (nur
         Fast-Forward),
      3. prüft mit scripts\Vor-Push.ps1 den ganzen Baum von main, alle
         Commits, die hinausgingen (fern..main; Autor, Committer, Nachricht),
         und dass main dabei nur Merge-Commits bekommt (First-Parent-Linie),
      4. lädt erst dann hoch, und nur refs/heads/main nach refs/heads/main
         (keine anderen Zweige, kein --force, kein --no-verify).
    Mit -Pruefen kein Schritt 4.

    Mit -Tag v0.MINOR.PATCH (Teil 3) nur dieser Tag: git push <Remote>
    refs/tags/<tag>:refs/tags/<tag>, ohne --no-verify; der Hook prüft (leicht,
    neu, auf der First-Parent-Linie von main). scripts/Freigeben.ps1 ruft
    -Tag auf (nach main), nicht von Hand. Nur leichte Tags (git tag ohne -a, -s, -m).

    Ohne Remote bricht das Skript mit einem Satz ab.

    Mit -Repo <pfad> ein Projekt-Repo (Overlay, ohne scripts/): core.hooksPath
    dort absolut auf <code-repo>/scripts/hooks; auch dort geht refs/heads/main
    nach refs/heads/main.
    Der Prüfer bekommt -Repo und die Push-Adresse des Remotes (git remote
    get-url --push --all, also pushurl, wenn gesetzt; mehrere verschiedene
    brechen ab), die Art kommt so aus repos.toml; darum braucht dort auch
    -Pruefen das Remote. Die Namenslisten nimmt der Prüfer ohne -Inhalt aus
    DASHBOARD_INHALT, sonst neben dem Code-Repo (nie neben dem Projekt-Repo).
    -Inhalt gilt nur für diese Vorprüfung; der Hook beim git push nimmt
    DASHBOARD_INHALT oder den Standard.

    Exit-Codes: 0 hochgeladen (oder mit -Pruefen: nichts gefunden);
    1 abgewiesen oder kein Remote; 2 der Prüfer konnte nicht prüfen.
.PARAMETER Repo
    Das Repo, das hinausgeht (Standard: das Code-Repo über scripts).
.PARAMETER Remote
    Name des Remotes (Standard origin).
.PARAMETER Inhalt
    Das Inhalts-Repo mit den Namenslisten (an Vor-Push.ps1 weiter).
.PARAMETER Pruefen
    Nur prüfen, was hinausginge, nicht hochladen.
.PARAMETER Wurzel
    Entfallen: bricht mit einem Satz ab (main geht direkt hinaus).
.PARAMETER Tag
    Lädt nur diesen leichten Tag hoch (etwa v0.1.0), geprüft vom Hook. Nicht
    zusammen mit -Pruefen (ein Tag lässt sich nicht zurücknehmen).
#>
[CmdletBinding()]
param(
    [string]$Repo,
    [string]$Remote = 'origin',
    [string]$Inhalt,
    [switch]$Pruefen,
    # Nur noch für den Abbruchsatz; ohne Parameter scheiterte der Aufruf ohne Erklärung.
    [switch]$Wurzel,
    [string]$Tag
)

$ErrorActionPreference = 'Stop'
$codeRepo = Split-Path -Parent $PSScriptRoot
# $repo ist zugleich der Parameter -Repo (PowerShell unterscheidet Groß/Klein nicht).
if (-not $repo) { $repo = $codeRepo }

function GitAus([string[]]$a) {
    $aus = & git -C $repo @a 2>&1
    if ($LASTEXITCODE -ne 0) { throw "git $($a -join ' '): $aus" }
    return ($aus | Out-String).Trim()
}

try {
    if ($Wurzel) { Write-Host 'Abbruch: -Wurzel gibt es nicht mehr; main geht direkt nach main hinaus (Hochladen.ps1 ohne -Wurzel, zum Probelauf -Pruefen). Es wurde nichts geändert.'; exit 1 }
    $repo = GitAus @('rev-parse', '--show-toplevel')
    $haken = (& git -C $repo config --get core.hooksPath) 2>$null
    $haken = "$haken"
    $codeHaken = Join-Path $codeRepo 'scripts\hooks'
    # Ein Projekt-Repo (Overlay) hat kein scripts/: Hooks und Prüfer kommen aus dem Code-Repo.
    $fremd = -not [string]::Equals([IO.Path]::GetFullPath($repo).TrimEnd('\', '/'), [IO.Path]::GetFullPath($codeRepo).TrimEnd('\', '/'), [StringComparison]::OrdinalIgnoreCase)
    if (-not $fremd -and $haken -ne 'scripts/hooks') {
        Write-Host 'Abbruch: core.hooksPath ist nicht scripts/hooks. Einmal setzen: git config core.hooksPath scripts/hooks'
        exit 1
    }
    if ($fremd -and -not ($haken -and [IO.Path]::IsPathRooted($haken) -and [string]::Equals([IO.Path]::GetFullPath($haken).TrimEnd('\', '/'), $codeHaken, [StringComparison]::OrdinalIgnoreCase))) {
        Write-Host "Abbruch: core.hooksPath dieses Repos zeigt nicht auf die Hooks des Code-Repos. Einmal setzen: git config core.hooksPath <code-repo>/scripts/hooks (absolut)"
        exit 1
    }
    $ps = if (Get-Command pwsh -ErrorAction SilentlyContinue) { 'pwsh' } else { 'powershell.exe' }
    if ($Tag) {
        # Ein Tag lässt sich nicht zurücknehmen (Verschieben und Löschen sind ein Fund): kein Push bei einem gemeinten Probelauf.
        if ($Pruefen) { Write-Host 'Abbruch: -Tag geht nicht zusammen mit -Pruefen; -Tag lädt den Tag immer hoch.'; exit 1 }
        if ($Tag -cnotmatch '^v0\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)$') { Write-Host 'Abbruch: -Tag hat nicht die Form v0.MINOR.PATCH (etwa v0.1.0).'; exit 1 }
        if (@((GitAus @('remote')) -split "`r?`n") -notcontains $Remote) { Write-Host "Abbruch: Es gibt kein Remote $Remote."; exit 1 }
        & git -C $repo push $Remote "refs/tags/$($Tag):refs/tags/$Tag"
        if ($LASTEXITCODE -ne 0) { Write-Host 'Der Tag ging nicht hinaus (siehe git und die Funde des Hooks).'; exit 1 }
        Write-Host "Hochgeladen: Tag $Tag auf $Remote."
        exit 0
    }
    $lokal = 'refs/heads/main'
    $main = GitAus @('rev-parse', '--verify', $lokal)
    $fern = $null
    $remotes = @((GitAus @('remote')) -split "`r?`n" | Where-Object { $_ })
    if ($remotes -contains $Remote) {
        $fern = (& git -C $repo rev-parse --verify --quiet "refs/remotes/$Remote/main") 2>$null
    } elseif ($fremd -or -not $Pruefen) {
        # Ohne Adresse prüfte der Prüfer ein Projekt-Repo als Basis: darum auch für -Pruefen ein Remote.
        if ($fremd) { Write-Host "Abbruch: Das Projekt-Repo hat kein Remote $Remote. Die Art kommt nur aus seiner Adresse: erst git remote add, dann erneut."; exit 1 }
        Write-Host "Abbruch: Es gibt kein Remote $Remote. Erst git remote add, dann erneut."
        exit 1
    }
    if ($fern) {
        # Nur Fast-Forward: git push wiese es ohnehin ab, -Pruefen soll es vorher sagen.
        & git -C $repo merge-base --is-ancestor $fern $main 2>$null
        if ($LASTEXITCODE -eq 1) { Write-Host "Abbruch: main enthält den Stand von $Remote/main nicht (kein Fast-Forward). Es wurde nichts hochgeladen."; exit 1 }
        if ($LASTEXITCODE -ne 0) { throw "git merge-base --is-ancestor endete mit $LASTEXITCODE" }
    }
    $bereich = if ($fern) { "$fern..$main" } else { $main }
    $argumente = @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $PSScriptRoot 'Vor-Push.ps1'), '-Repo', $repo, '-Baum', $lokal, '-Commits', $bereich, '-AufMain')
    # Die Art (Projekt) kommt nur aus der Adresse und repos.toml, nie aus diesem Skript.
    if ($fremd) {
        # Gepusht wird an die Push-Adressen (pushurl, bei mehreren an alle): genau die prüft er; verschiedene gehen nicht.
        $adressen = @((GitAus @('remote', 'get-url', '--push', '--all', $Remote)) -split "`r?`n" | Where-Object { $_ })
        if ($adressen.Count -eq 0 -or @($adressen | Where-Object { $_ -cne $adressen[0] }).Count -gt 0) {
            Write-Host "Abbruch: Das Remote $Remote hat mehrere verschiedene Push-Adressen (pushurl oder mehrere url). Die Art kommt aus genau einer Adresse: erst auf eine Push-Adresse zurücksetzen, dann erneut."
            exit 1
        }
        $argumente += @('-RemoteUrl', $adressen[0])
    }
    if ($Inhalt) { $argumente += @('-Inhalt', $Inhalt) }
    & $ps @argumente
    $code = $LASTEXITCODE
    if ($code -ne 0) {
        Write-Host "Abgewiesen (Exit $code). Es wurde nichts hochgeladen."
        exit $(if ($code -eq 2) { 2 } else { 1 })
    }
    if ($Pruefen) {
        Write-Host "Nur geprüft: $($lokal.Substring(11)) ginge so nach main hinaus. Hochgeladen wurde nichts."
        exit 0
    }
    & git -C $repo push $Remote "$($lokal):refs/heads/main"
    if ($LASTEXITCODE -ne 0) { Write-Host 'Das Hochladen ist gescheitert (siehe git). Der Hook hat dabei noch einmal geprüft.'; exit 1 }
    Write-Host "Hochgeladen: $($lokal.Substring(11)) nach main auf $Remote."
    exit 0
} catch {
    Write-Host "FEHLER $($_.Exception.Message). Es wurde nichts hochgeladen."
    exit 2
}
