<#
.SYNOPSIS
    Gibt eine neue Fassung des Code-Repos heraus: ein Aufruf, ohne Rückfrage,
    in fester Folge. Jeder rote Schritt bricht ab, bevor etwas hinausgeht.
.DESCRIPTION
    Folge:
      0. Wiederaufnahme: Liegt ein Zweig freigabe/v0.x.y ohne Tag v0.x.y da,
         Abbruch mit einem Satz. Steht main genau auf einem Merge
         chore(release): v0.x.y mit Tag v0.x.y, geht es bei der Lock-Datei
         des Inhalts (Ende von 4) weiter; beide Pushes sind wiederholbar.
      1. Vorbedingungen: git fetch origin (ohne Verbindung Abbruch), auf main,
         sauberer Arbeitsbaum, main enthält origin/main (nur Fast-Forward),
         die Lock-Datei der Zusammenbau-Kiste im Inhalt ist unverändert.
      3. Version und Changelog: Fassung-Festschreiben.ps1 -Pruefen (Version
         aus den Commits seit dem letzten Tag, Zählregel, künftiger Tag, auch
         auf origin). Der Stand von main wird hier gemerkt; Schritt 4 schreibt
         genau ihn fest und bricht ab, wenn main sich bewegt hat. Es
         läuft vor 2, weil es Sekunden dauert und 2 Minuten.
      2. Prüfen: cargo fmt --check, clippy -D warnings, cargo test,
         npm run verify, dashboard pruefe --system für docs/anleitung und
         docs/zielbild (0 Fehler, 0 Hinweise), Hochladen.ps1 -Pruefen (der
         Prüfer über das, was hinausginge).
      4. Fassung-Festschreiben.ps1: Merge chore(release): v0.x.y auf main und
         leichter Tag, lokal. Dann im Inhalt dashboard-lokal/Cargo.lock mit
         cargo update -p dashboard -p dashboard-projekt --offline nachziehen
         und als eigenen Commit nur mit diesem Pfad festschreiben (sonst
         scheitert der Bau mit --locked). Ohne dashboard-lokal entfällt das.
      5. Hinaus: Hochladen.ps1 (main), dann Hochladen.ps1 -Tag v0.x.y.
      6. Nur wenn der Arbeitsbaum sauber auf main beim Tag steht (Dienst.ps1
         baut aus ihm): Dienst.ps1 -Neustart; danach liest das Skript version von
         /api/betrieb (Port aus dienst.toml wie Dienst.ps1) und nennt, welche
         Fassung läuft, auch wenn der Neustart rot endet.
    Was draußen ist, wird nie zurückgenommen. Ist hochgeladen, aber der Dienst
    rot, wiederholt ein zweiter Aufruf nur 5 (ohne Wirkung) und 6.
    Kein Schalter überspringt eine Prüfung.

    Exit-Codes: 0 freigegeben und die Fassung läuft (mit -Pruefen: ginge so
    hinaus); 1 abgewiesen oder ein Schritt rot (der Satz sagt, was schon
    geschehen ist); 2 Fehler.
.PARAMETER Pruefen
    Nur 1 bis 3: sagt, welche Fassung es würde. Ändert nichts.
.PARAMETER Inhalt
    Das Inhalts-Repo (Namenslisten, dashboard-lokal). Standard:
    DASHBOARD_INHALT, sonst der Nachbarordner dashboard-inhalt des Code-Repos.
    Gilt auch für die Hooks beim Push.
.PARAMETER Dienst
    Das Skript für Schritt 6 (Standard scripts\Dienst.ps1); bekommt -Neustart.
.PARAMETER Einstellungen
    Die dienst.toml mit dem Port (Standard: wie Dienst.ps1 aus dem Dienst
    Dashboard).
#>
[CmdletBinding()]
param(
    [switch]$Pruefen,
    [string]$Inhalt,
    [string]$Dienst,
    [string]$Einstellungen
)

$ErrorActionPreference = 'Stop'
$utf8 = New-Object System.Text.UTF8Encoding($false)
try { [Console]::OutputEncoding = $utf8 } catch { }
$repo = Split-Path -Parent $PSScriptRoot
if (-not $Inhalt) { $Inhalt = if ($env:DASHBOARD_INHALT) { $env:DASHBOARD_INHALT } else { Join-Path (Split-Path -Parent $repo) 'dashboard-inhalt' } }
# Auch die Hooks beim Push (über Hochladen.ps1) prüfen gegen diesen Inhalt.
$env:DASHBOARD_INHALT = $Inhalt
if (-not $Dienst) { $Dienst = Join-Path $PSScriptRoot 'Dienst.ps1' }
# Kinder (cargo, Hooks, Fassung-Festschreiben.ps1) erben denselben Bauordner.
if (-not $env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR = Join-Path $env:LOCALAPPDATA 'Dashboard\cache\freigabe' }
$ps = if (Get-Command pwsh -ErrorAction SilentlyContinue) { 'pwsh' } else { 'powershell.exe' }
$TAG_FORM = '^v0\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)$'
$lokal = Join-Path $Inhalt 'dashboard-lokal'
$NICHTS = 'Es ging nichts hinaus.'

function Ende([int]$code, [string]$satz) { Write-Host $satz; exit $code }

function GitLauf([string]$wo, [string[]]$a, [switch]$Still) {
    $alt = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { $aus = & git -C $wo @a 2>&1 } finally { $ErrorActionPreference = $alt }
    $code = $LASTEXITCODE
    $text = ((@($aus) | ForEach-Object { "$_" }) -join "`n").Trim()
    if ($code -ne 0 -and -not $Still) { throw "git $($a -join ' '): $text" }
    return [pscustomobject]@{ Code = $code; Text = $text }
}

# Ein Programm in einem Ordner; die Ausgabe läuft eingerückt mit und kommt als Text zurück.
function Aufruf([string]$wo, [string]$exe, [string[]]$a) {
    $zeilen = New-Object Collections.Generic.List[string]
    $alt = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    Push-Location -LiteralPath $wo
    try {
        & $exe @a 2>&1 | ForEach-Object { $t = "$_"; $zeilen.Add($t); Write-Host "  $t" }
        $code = $LASTEXITCODE
    } finally { Pop-Location; $ErrorActionPreference = $alt }
    return [pscustomobject]@{ Code = $code; Text = ($zeilen -join "`n") }
}

function Skript([string]$datei, [string[]]$a) {
    return (Aufruf $repo $ps (@('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $datei) + $a))
}

function LockSauber {
    if (-not (Test-Path -LiteralPath (Join-Path $lokal 'Cargo.toml') -PathType Leaf)) { return }
    $s = GitLauf $Inhalt @('status', '--porcelain', '--untracked-files=no', '--', 'dashboard-lokal') -Still
    if ($s.Code -ne 0) { Ende 1 "Abbruch: Der Inhalt $Inhalt ist kein Git-Repo (git status: $($s.Text)). $NICHTS" }
    if ($s.Text) { Ende 1 "Abbruch: dashboard-lokal im Inhalt (Cargo.toml oder Cargo.lock) hat offene Änderungen; erst committen oder verwerfen. $NICHTS" }
}

# Ende von 4: Lock-Datei der Zusammenbau-Kiste auf die neue Fassung; wiederholbar.
function LockNachziehen([string]$version) {
    if (-not (Test-Path -LiteralPath (Join-Path $lokal 'Cargo.toml') -PathType Leaf)) {
        Write-Host "Hinweis: Neben dem Inhalt gibt es keine Zusammenbau-Kiste dashboard-lokal; keine Lock-Datei nachzuziehen."
        return
    }
    LockSauber
    $weiter = "main trägt Fassung $version lokal (Merge und Tag). $NICHTS Nach der Behebung erneut aufrufen; es geht bei der Lock-Datei weiter."
    $u = Aufruf $lokal 'cargo' @('update', '-p', 'dashboard', '-p', 'dashboard-projekt', '--offline')
    if ($u.Code -ne 0) { Ende 1 "Abbruch: cargo update in dashboard-lokal ist gescheitert (siehe oben). $weiter" }
    $lock = [IO.File]::ReadAllText((Join-Path $lokal 'Cargo.lock'))
    if ($lock -notmatch "(?m)^name = `"dashboard`"\r?\nversion = `"$([regex]::Escape($version))`"") { Ende 1 "Abbruch: dashboard-lokal/Cargo.lock trägt dashboard nicht mit $version. $weiter" }
    if (-not (GitLauf $Inhalt @('status', '--porcelain', '--', 'dashboard-lokal/Cargo.lock')).Text) {
        Write-Host "dashboard-lokal/Cargo.lock steht schon auf $version."
        return
    }
    # commit -- <pfad>: nur dieser Pfad, auch wenn im Inhalt anderes vorgemerkt ist.
    $c = GitLauf $Inhalt @('commit', '-q', '-m', "chore(dashboard-lokal): Cargo.lock auf Fassung $version", '--', 'dashboard-lokal/Cargo.lock') -Still
    if ($c.Code -ne 0) { Ende 1 "Abbruch: Der Commit der Lock-Datei im Inhalt ging nicht durch: $($c.Text) $weiter" }
    Write-Host "Inhalt: dashboard-lokal/Cargo.lock auf $version festgeschrieben."
}

# Der Stand einer Ref auf origin ('' ohne sie); ohne Verbindung Abbruch.
function Fern([string]$ref) {
    $l = GitLauf $repo @('ls-remote', 'origin', $ref) -Still
    if ($l.Code -ne 0) { Ende 1 "Abbruch: origin ist nicht erreichbar (git ls-remote): $($l.Text) Lokal trägt main Fassung $version; nach der Behebung erneut aufrufen." }
    return (("$($l.Text)" -split "`n" | Select-Object -First 1) -split '\s+')[0]
}
# Port wie Dienst.ps1: dienst.toml aus dem Aufruf des Dienstes, sonst 8787.
function BetriebAdresse {
    $toml = $Einstellungen
    if (-not $toml) {
        $w = Get-CimInstance Win32_Service -Filter "Name='Dashboard'" -ErrorAction SilentlyContinue
        if (-not $w) { return $null }
        $m = [regex]::Match([string]$w.PathName, '--einstellungen\s+"?([^"]+?)"?(\s|$)')
        $toml = if ($m.Success) { $m.Groups[1].Value } else {
            $e = [regex]::Match([string]$w.PathName, '^\s*"?([^"]+?\.exe)"?')
            Join-Path (Split-Path -Parent (Split-Path -Parent $e.Groups[1].Value)) 'dienst.toml'
        }
    }
    $port = 8787
    try {
        $m = [regex]::Match([IO.File]::ReadAllText($toml), '(?m)^\s*port\s*=\s*(\d+)')
        if ($m.Success) { $port = [int]$m.Groups[1].Value }
    } catch { }
    return "http://127.0.0.1:$port/api/betrieb"
}

function LaufendeFassung {
    $url = BetriebAdresse
    if (-not $url) { return $null }
    . (Join-Path $PSScriptRoot 'Anfrage.ps1')
    for ($i = 0; $i -lt 10; $i++) {
        $r = WindowsAnfrage -Url $url -Sekunden 3
        if ($r.Code -eq '200') {
            try { $v = ($r.Text | ConvertFrom-Json).version; if ($v) { return [string]$v } } catch { }
        }
        Start-Sleep -Seconds 1
    }
    return $null
}

try {
    # 0. Wiederaufnahme.
    foreach ($z in @((GitLauf $repo @('for-each-ref', '--format=%(refname:short)', 'refs/heads/freigabe/')).Text -split "`n" | Where-Object { $_ })) {
        $t = $z.Substring('freigabe/'.Length)
        if ((GitLauf $repo @('rev-parse', '--verify', '--quiet', "refs/tags/$t") -Still).Code -ne 0) {
            Ende 1 "Abbruch: Der Zweig $z liegt ohne Tag $t da (eine abgebrochene Freigabe). Steht main auf seinem Merge chore(release): $($t), fehlt nur der Tag: git tag $t main, git branch -d $z, dann erneut. Sonst den Zweig klären. $NICHTS"
        }
    }
    $tag = @((GitLauf $repo @('tag', '--points-at', 'refs/heads/main', '--list', 'v*')).Text -split "`n" | Where-Object { $_ -cmatch $TAG_FORM }) | Select-Object -First 1
    $kopf = (GitLauf $repo @('log', '-1', '--format=%P%n%s', 'refs/heads/main')).Text -split "`n"
    $wieder = -not $Pruefen -and $tag -and $kopf[1] -ceq "chore(release): $tag" -and @($kopf[0] -split ' ' | Where-Object { $_ }).Count -eq 2
    if ($wieder) {
        $version = $tag.Substring(1)
        Write-Host "Wiederaufnahme: main steht auf dem Merge chore(release): $tag mit Tag; weiter bei der Lock-Datei und Schritt 5."
    } else {
        # 1. Vorbedingungen.
        $f = GitLauf $repo @('fetch', '-q', 'origin') -Still
        if ($f.Code -ne 0) { Ende 1 "Abbruch: git fetch origin ging nicht (keine Verbindung oder kein Remote origin): $($f.Text) $NICHTS" }
        if ((GitLauf $repo @('symbolic-ref', '--quiet', '--short', 'HEAD') -Still).Text -cne 'main') { Ende 1 "Abbruch: Der Arbeitsbaum steht nicht auf main. $NICHTS" }
        if ((GitLauf $repo @('status', '--porcelain')).Text) { Ende 1 "Abbruch: Der Arbeitsbaum ist nicht sauber (git status). $NICHTS" }
        if ((GitLauf $repo @('rev-parse', '--verify', '--quiet', 'refs/remotes/origin/main') -Still).Code -eq 0 -and
            (GitLauf $repo @('merge-base', '--is-ancestor', 'refs/remotes/origin/main', 'refs/heads/main') -Still).Code -ne 0) {
            Ende 1 "Abbruch: main enthält origin/main nicht (kein Fast-Forward); erst origin/main einbringen. $NICHTS"
        }
        LockSauber

        # Genau dieser Stand wird geprüft und festgeschrieben; bewegt sich main bis dahin, bricht 4 ab.
        $basis = (GitLauf $repo @('rev-parse', 'refs/heads/main')).Text

        # 3. Version und Changelog (vor 2: Sekunden statt Minuten).
        Write-Host 'Version und Changelog (Fassung-Festschreiben.ps1 -Pruefen) ...'
        $p = Skript (Join-Path $PSScriptRoot 'Fassung-Festschreiben.ps1') @('-Pruefen', '-Basis', $basis)
        if ($p.Code -ne 0) { Ende $(if ($p.Code -eq 1) { 1 } else { 2 }) "Abbruch bei Version und Changelog (siehe oben). $NICHTS" }
        $m = [regex]::Match($p.Text, 'Fassung: (0\.\d+\.\d+) ')
        if (-not $m.Success) { throw "Fassung-Festschreiben.ps1 -Pruefen nannte keine Fassung: $($p.Text)" }
        $version = $m.Groups[1].Value
        # fetch holt nur Tags auf geholten Commits; der Hook weist jeden fern schon vorhandenen Tag ab.
        $l = GitLauf $repo @('ls-remote', '--tags', 'origin', "refs/tags/v$version") -Still
        if ($l.Code -ne 0) { Ende 1 "Abbruch: origin ist nicht erreichbar (git ls-remote): $($l.Text) $NICHTS" }
        if ($l.Text) { Ende 1 "Abbruch: Den Tag v$version gibt es auf origin schon (kein Verschieben). $NICHTS" }

        # 2. Prüfen.
        $web = Join-Path $repo 'web'
        foreach ($s in @(
                @('cargo fmt --check', $repo, 'cargo', @('fmt', '--all', '--check')),
                @('cargo clippy -D warnings', $repo, 'cargo', @('clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings')),
                @('cargo test', $repo, 'cargo', @('test', '--workspace', '--locked')),
                @('npm run verify', $web, 'npm', @('run', 'verify')),
                @('pruefe --system docs/anleitung', $repo, 'cargo', @('run', '-q', '--locked', '--bin', 'dashboard', '--', 'pruefe', '--system', 'docs/anleitung')),
                @('pruefe --system docs/zielbild', $repo, 'cargo', @('run', '-q', '--locked', '--bin', 'dashboard', '--', 'pruefe', '--system', 'docs/zielbild'))
            )) {
            Write-Host "$($s[0]) ..."
            $r = Aufruf $s[1] $s[2] $s[3]
            # pruefe ist erst mit 0 Fehlern und 0 Hinweisen grün.
            if ($r.Code -ne 0 -or ($s[0] -like 'pruefe*' -and $r.Text -notmatch '(?m)^0 Fehler, 0 Hinweise\s*$')) { Ende 1 "Abbruch: $($s[0]) ist rot (siehe oben). $NICHTS" }
        }
        Write-Host 'Prüfer über das, was hinausginge (Hochladen.ps1 -Pruefen) ...'
        $h = Skript (Join-Path $PSScriptRoot 'Hochladen.ps1') @('-Pruefen', '-Inhalt', $Inhalt)
        if ($h.Code -ne 0) { Ende 1 "Abbruch: Hochladen.ps1 -Pruefen ist rot (siehe oben). $NICHTS" }
        if ($Pruefen) { Ende 0 "Probe: Fassung $version ginge so hinaus. Geändert wurde nichts." }

        # 4. Lokal festschreiben.
        Write-Host "Fassung $version festschreiben (Fassung-Festschreiben.ps1) ..."
        $r = Skript (Join-Path $PSScriptRoot 'Fassung-Festschreiben.ps1') @('-Basis', $basis)
        if ($r.Code -ne 0) { Ende $(if ($r.Code -eq 1) { 1 } else { 2 }) "Abbruch beim Festschreiben (siehe oben). $NICHTS" }
        $tag = "v$version"
        if ((GitLauf $repo @('rev-parse', "refs/tags/$tag^{commit}")).Text -ne (GitLauf $repo @('rev-parse', 'refs/heads/main')).Text) { throw "Der Tag $tag steht nicht auf main." }
    }
    LockNachziehen $version

    # 5. Hinaus. Was origin schon hat, geht nicht noch einmal hinaus: Der Hook
    # bekäme dann keine Ref-Zeilen und wiese den Push ab.
    $kopf = (GitLauf $repo @('rev-parse', 'refs/heads/main')).Text
    if ((Fern 'refs/heads/main') -ceq $kopf) { Write-Host 'main ist schon draußen.' } else {
        Write-Host 'main hinaus (Hochladen.ps1) ...'
        $h = Skript (Join-Path $PSScriptRoot 'Hochladen.ps1') @('-Inhalt', $Inhalt)
        if ($h.Code -ne 0) { Ende 1 "Abbruch: main ging nicht hinaus (siehe oben); der Tag auch nicht. Lokal trägt main Fassung $version. Nach der Behebung erneut aufrufen." }
    }
    if ((Fern "refs/tags/$tag") -ceq $kopf) { Write-Host "Tag $tag ist schon draußen." } else {
        Write-Host "Tag $tag hinaus (Hochladen.ps1 -Tag) ..."
        $h = Skript (Join-Path $PSScriptRoot 'Hochladen.ps1') @('-Tag', $tag)
        if ($h.Code -ne 0) { Ende 1 "Abbruch: main ist draußen, der Tag $tag nicht (siehe oben). Nach der Behebung erneut aufrufen." }
    }

    # 6. Dienst. Dienst.ps1 baut aus dem Arbeitsbaum: Er muss sauber auf dem Tag stehen.
    if ((GitLauf $repo @('symbolic-ref', '--quiet', '--short', 'HEAD') -Still).Text -cne 'main' -or (GitLauf $repo @('status', '--porcelain')).Text -or
        (GitLauf $repo @('rev-parse', 'HEAD')).Text -cne (GitLauf $repo @('rev-parse', "refs/tags/$tag^{commit}")).Text) {
        Ende 1 "Fassung $version ist draußen (main und Tag $tag); der Dienst wurde nicht neu gestartet, weil der Arbeitsbaum nicht sauber auf main beim Tag steht. Danach erneut aufrufen."
    }
    Write-Host 'Dienst neu starten (Dienst.ps1 -Neustart) ...'
    $d = Aufruf $repo $ps @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $Dienst, '-Neustart')
    $laeuft = LaufendeFassung
    $satz = if (-not $laeuft) { 'Welche Fassung läuft, ist unklar: /api/betrieb nennt keine.' }
    elseif ($laeuft -ceq $version) { "Es läuft Fassung $laeuft." }
    else { "Es läuft Fassung $laeuft, nicht $version." }
    if ($d.Code -eq 0 -and $laeuft -ceq $version) { Ende 0 "Freigegeben: Fassung $version ist draußen (main und Tag $tag). $satz" }
    Ende 1 "Fassung $version ist draußen (main und Tag $tag), aber der Dienst ist rot (Dienst.ps1 Exit $($d.Code)). $satz Ein zweiter Aufruf wiederholt nur das Hochladen (ohne Wirkung) und den Dienst."
} catch {
    Write-Host "FEHLER $($_.Exception.Message)"
    exit 2
}
