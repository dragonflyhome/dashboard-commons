<#
.SYNOPSIS
    Leitet die nächste Fassung ab, prüft das Changelog und schreibt Fassung und
    Tag lokal fest. Nichts geht hinaus.
.DESCRIPTION
    Schritte 3 und 4 der Freigabe (ohne Lock-Datei des Inhalts):
      3. Version: alle Commits zwischen dem letzten Tag v0.x.y auf main und main
         (auch die eingebrachter Zweige). Ein feat oder ein ! (Bruch) hebt die
         mittlere Zahl (0.2.0 wird 0.3.0), alles andere die letzte (0.2.1). Die
         erste Zahl bleibt.
         Changelog (Zählregel, aus CHANGELOG.txt auf main): Unreleased ist nicht
         leer; gibt es ein !, steht unter Geändert oder Entfernt eine Zeile
         „- Bruch: …“; gibt es ein feat, sind Hinzugefügt oder Geändert nicht
         leer; gibt es ein fix, ist Behoben nicht leer. Nur Pflege (kein feat,
         fix, perf oder !) darf als einzige Zeile „- Pflege ohne sichtbare
         Änderung.“ unter Geändert haben; sonst zählt diese Zeile nicht.
         Der künftige Tag: Form v0.MINOR.PATCH, neu, kein Zweig freigabe/<tag>.
      4. In einem vorübergehenden Worktree ein Zweig freigabe/v0.x.y mit einem
         Commit chore(release): v0.x.y: Unreleased wird „0.x.y - JJJJ-MM-TT“,
         darüber ein neues leeres Unreleased; version in Cargo.toml ([package])
         und Cargo.lock (cargo update --workspace --offline, nur die eigenen Kisten); npm version <v>
         --no-git-tag-version in web (package.json und package-lock.json).
         Dann auf main git merge --no-ff, Worktree weg, leichter Tag v0.x.y auf
         dem Merge-Commit, Zweig weg.
    Mit -Pruefen nur Schritt 3, ohne Wirkung. Die Zeilen im Changelog schreibt,
    wer den Zweig einbringt, nie dieses Skript.
    Geht Schritt 4 vor dem Merge schief, verschwinden Worktree und Zweig wieder;
    main bleibt, wie es war. Fehlt nach dem Merge der Tag, bleibt der Zweig
    stehen, und der Satz sagt, was fehlt.

    Exit-Codes: 0 festgeschrieben (mit -Pruefen: passt); 1 abgewiesen;
    2 Fehler (siehe Satz).
.PARAMETER Repo
    Das Repo (Standard: das Code-Repo über scripts).
.PARAMETER Pruefen
    Nur ableiten und prüfen, nichts ändern.
.PARAMETER Basis
    Der geprüfte Stand von main (Standard: main beim Start). Abgeleitet,
    geprüft und festgeschrieben wird genau dieser Stand; steht main vor dem
    Merge nicht mehr darauf (oder der Arbeitsbaum nicht auf main), Abbruch mit
    Rücknahme.
#>
[CmdletBinding()]
param(
    [string]$Repo,
    [switch]$Pruefen,
    [string]$Basis
)

$ErrorActionPreference = 'Stop'
$utf8 = New-Object System.Text.UTF8Encoding($false)
try { [Console]::OutputEncoding = $utf8 } catch { }
if (-not $Repo) { $Repo = Split-Path -Parent $PSScriptRoot }
$TAG_FORM = '^v0\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)$'
$PFLEGE = '- Pflege ohne sichtbare Änderung.'

# git ohne Abbruch über stderr (Windows PowerShell 5.1 machte daraus einen Fehler).
function GitLauf([string]$wo, [string[]]$a, [switch]$Still) {
    $alt = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try { $aus = & git -C $wo @a 2>&1 } finally { $ErrorActionPreference = $alt }
    $code = $LASTEXITCODE
    $text = (@($aus) | ForEach-Object { "$_" }) -join "`n"
    if ($code -ne 0 -and -not $Still) { throw "git $($a -join ' '): $text" }
    return [pscustomobject]@{ Code = $code; Text = $text.Trim() }
}

function Lesen([string]$pfad) {
    $bytes = [IO.File]::ReadAllBytes($pfad)
    $bom = $bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF
    $text = $utf8.GetString($bytes).TrimStart([char]0xFEFF)
    return [pscustomobject]@{ Text = $text; Bom = $bom }
}

function Schreiben([string]$pfad, [string]$text, [bool]$bom) {
    [IO.File]::WriteAllText($pfad, $text, (New-Object System.Text.UTF8Encoding($bom)))
}

# Index der Überschrift Unreleased (Zeile, darunter nur Striche) oder -1.
function UnreleasedZeile([string[]]$z) {
    for ($i = 0; $i -lt $z.Count - 1; $i++) {
        if ($z[$i].TrimEnd() -ceq 'Unreleased' -and $z[$i + 1].TrimEnd() -match '^-{3,}$') { return $i }
    }
    return -1
}

$wt = $null
$zweig = $null
$angelegt = $false
$gemergt = $false
$getaggt = $false
try {
    $Repo = (GitLauf $Repo @('rev-parse', '--show-toplevel')).Text
    $zweigJetzt = (GitLauf $Repo @('symbolic-ref', '--quiet', '--short', 'HEAD') -Still).Text
    if ($zweigJetzt -cne 'main') { Write-Host 'Abbruch: Der Arbeitsbaum steht nicht auf main. Es wurde nichts geändert.'; exit 1 }
    $Basis = (GitLauf $Repo @('rev-parse', '--verify', "$(if ($Basis) { $Basis } else { 'refs/heads/main' })^{commit}")).Text

    # 3. Version ableiten.
    $letzter = @((GitLauf $Repo @('tag', '--merged', $Basis, '--list', 'v*', '--sort=-v:refname')).Text -split "`n" | Where-Object { $_ -cmatch $TAG_FORM }) | Select-Object -First 1
    if (-not $letzter) { Write-Host 'Abbruch: Auf main gibt es keinen Tag v0.x.y; die erste Fassung setzt der Inhaber fest. Es wurde nichts geändert.'; exit 1 }
    $betreffe = @((GitLauf $Repo @('log', '--format=%s', "refs/tags/$letzter..$Basis")).Text -split "`n" | Where-Object { $_ })
    if ($betreffe.Count -eq 0) { Write-Host "Abbruch: Seit $letzter gibt es keinen Commit auf main. Es wurde nichts geändert."; exit 1 }
    $feat = @(); $fix = @(); $bruch = @(); $sichtbar = $false
    foreach ($b in $betreffe) {
        if ($b -cnotmatch '^(?<typ>[a-z]+)(?:\([^()]*\))?(?<bruch>!)?: ') { continue }
        if ($Matches['bruch']) { $bruch += $b }
        if ($Matches['typ'] -ceq 'feat') { $feat += $b }
        if ($Matches['typ'] -ceq 'fix') { $fix += $b }
        if ($Matches['bruch'] -or @('feat', 'fix', 'perf') -ccontains $Matches['typ']) { $sichtbar = $true }
    }
    $teile = $letzter.Substring(1).Split('.') | ForEach-Object { [int64]$_ }
    $version = if ($feat.Count -or $bruch.Count) { "0.$($teile[1] + 1).0" } else { "0.$($teile[1]).$($teile[2] + 1)" }
    $tag = "v$version"
    $zweig = "freigabe/$tag"

    # 3. Changelog prüfen (wie es auf main steht).
    $alt = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
    try { $roh = & git -C $Repo show "$($Basis):CHANGELOG.txt" 2>$null } finally { $ErrorActionPreference = $alt }
    if ($LASTEXITCODE -ne 0) { Write-Host 'Abbruch: CHANGELOG.txt fehlt auf main. Es wurde nichts geändert.'; exit 1 }
    $zeilen = @($roh | ForEach-Object { "$_".TrimStart([char]0xFEFF) })
    $u = UnreleasedZeile $zeilen
    if ($u -lt 0) { Write-Host 'Abbruch: CHANGELOG.txt hat keinen Abschnitt Unreleased (Zeile Unreleased, darunter ----------). Es wurde nichts geändert.'; exit 1 }
    $eintraege = @()
    $unter = ''
    for ($i = $u + 2; $i -lt $zeilen.Count; $i++) {
        if ($i + 1 -lt $zeilen.Count -and $zeilen[$i].Trim() -and $zeilen[$i + 1].TrimEnd() -match '^-{3,}$') { break }
        $z = $zeilen[$i].TrimEnd()
        if (@('Hinzugefügt', 'Geändert', 'Veraltet', 'Entfernt', 'Behoben', 'Sicherheit') -ccontains $z) { $unter = $z; continue }
        if ($z.StartsWith('- ')) { $eintraege += [pscustomobject]@{ Unter = $unter; Text = $z } }
    }
    # Die Pflege-Zeile zählt nur bei reiner Pflege und nur unter Geändert.
    $eintraege = @($eintraege | Where-Object { $_.Text -cne $PFLEGE -or (-not $sichtbar -and $_.Unter -ceq 'Geändert') })
    $funde = @()
    $liste = { param($c) ($c | ForEach-Object { '„' + $_ + '“' }) -join ', ' }
    if ($eintraege.Count -eq 0) { $funde += 'Unreleased in CHANGELOG.txt ist leer (bei reiner Pflege genügt unter Geändert die eine Zeile „- Pflege ohne sichtbare Änderung.“).' }
    if ($bruch.Count -and -not @($eintraege | Where-Object { @('Geändert', 'Entfernt') -ccontains $_.Unter -and $_.Text.StartsWith('- Bruch: ') }).Count) {
        $funde += 'Ein Bruch ohne Zeile „- Bruch: …“ unter Geändert oder Entfernt; ausgelöst von ' + (& $liste $bruch) + '.'
    }
    if ($feat.Count -and -not @($eintraege | Where-Object { @('Hinzugefügt', 'Geändert') -ccontains $_.Unter }).Count) {
        $funde += "Ein feat ohne Zeile unter Hinzugefügt oder Geändert; ausgelöst von $(& $liste $feat)."
    }
    if ($fix.Count -and -not @($eintraege | Where-Object { $_.Unter -ceq 'Behoben' }).Count) {
        $funde += "Ein fix ohne Zeile unter Behoben; ausgelöst von $(& $liste $fix)."
    }
    # 3. Der künftige Tag wie beim Hook: Form, neu; auf der Hauptlinie liegt er durch den Merge.
    if ($tag -cnotmatch $TAG_FORM) { $funde += "Der Tag $tag hat nicht die Form v0.MINOR.PATCH." }
    if ((GitLauf $Repo @('rev-parse', '--verify', '--quiet', "refs/tags/$tag") -Still).Code -eq 0) { $funde += "Den Tag $tag gibt es schon (kein Verschieben)." }
    if ((GitLauf $Repo @('rev-parse', '--verify', '--quiet', "refs/heads/$zweig") -Still).Code -eq 0) { $funde += "Der Zweig $zweig liegt schon da (eine abgebrochene Freigabe?); erst klären, dann erneut." }
    # git kennt refs/heads/freigabe und refs/heads/freigabe/* nicht nebeneinander.
    if ((GitLauf $Repo @('rev-parse', '--verify', '--quiet', 'refs/heads/freigabe') -Still).Code -eq 0) { $funde += "Der Zweig freigabe verhindert den Zweig $zweig; erst entfernen." }
    $zahlen = "seit $($letzter): $($betreffe.Count) Commits, davon $($feat.Count) feat, $($bruch.Count) mit !, $($fix.Count) fix"
    if ($funde.Count) {
        $funde | ForEach-Object { Write-Host "Abbruch: $_" }
        Write-Host "Fassung $version ($zahlen) nicht festgeschrieben. Es wurde nichts geändert."
        exit 1
    }
    if ($Pruefen) {
        Write-Host "Nächste Fassung: $version ($zahlen). Changelog passt. Nur geprüft, nichts geändert."
        exit 0
    }

    # 4. Lokal festschreiben, ohne den Arbeitsbaum umzuschalten.
    if ((GitLauf $Repo @('status', '--porcelain')).Text) { Write-Host 'Abbruch: Der Arbeitsbaum ist nicht sauber (git status). Es wurde nichts geändert.'; exit 1 }
    $wt = Join-Path ([IO.Path]::GetTempPath()) ("freigabe-$version-" + [guid]::NewGuid().ToString('N').Substring(0, 8))
    [void](GitLauf $Repo @('worktree', 'add', '-q', '-b', $zweig, $wt, $Basis))
    $angelegt = $true
    # Der pre-commit-Hook prüft web/ mit Prettier aus node_modules: Junction auf das des Repos.
    $nm = Join-Path $Repo 'web\node_modules'
    if (Test-Path -LiteralPath $nm) { [void](New-Item -ItemType Junction -Path (Join-Path $wt 'web\node_modules') -Target $nm) }
    if (-not $env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR = Join-Path $env:LOCALAPPDATA 'Dashboard\cache\freigabe' }
    $datum = (Get-Date).ToString('yyyy-MM-dd', [Globalization.CultureInfo]::InvariantCulture)

    $cl = Lesen (Join-Path $wt 'CHANGELOG.txt')
    $nl = if ($cl.Text.Contains("`r`n")) { "`r`n" } else { "`n" }
    $z = [Collections.Generic.List[string]]($cl.Text -split [regex]::Escape($nl))
    $u = UnreleasedZeile $z.ToArray()
    $titel = "$version - $datum"
    $z[$u] = $titel
    $z[$u + 1] = '-' * $titel.Length
    $z.InsertRange($u, [string[]]@('Unreleased', '----------', ''))
    Schreiben (Join-Path $wt 'CHANGELOG.txt') ($z -join $nl) $cl.Bom

    $ct = Lesen (Join-Path $wt 'Cargo.toml')
    $nl = if ($ct.Text.Contains("`r`n")) { "`r`n" } else { "`n" }
    $z = $ct.Text -split [regex]::Escape($nl)
    $name = $null; $treffer = @(); $imPaket = $false
    for ($i = 0; $i -lt $z.Count; $i++) {
        if ($z[$i] -match '^\s*\[') { $imPaket = $z[$i].Trim() -ceq '[package]'; continue }
        if (-not $imPaket) { continue }
        if ($z[$i] -match '^\s*version\s*=\s*"[^"]*"\s*$') { $treffer += $i }
        if ($z[$i] -match '^\s*name\s*=\s*"([^"]+)"\s*$') { $name = $Matches[1] }
    }
    if ($treffer.Count -ne 1 -or -not $name) { throw 'Cargo.toml braucht unter [package] genau eine Zeile version = "…" und einen name.' }
    $z[$treffer[0]] = "version = `"$version`""
    Schreiben (Join-Path $wt 'Cargo.toml') ($z -join $nl) $ct.Bom

    $alt = $ErrorActionPreference; $ErrorActionPreference = 'Continue'
    try {
        & cargo update --workspace --offline --manifest-path (Join-Path $wt 'Cargo.toml') 2>&1 | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'cargo update --workspace --offline hat Cargo.lock nicht nachgezogen.' }
        Push-Location (Join-Path $wt 'web')
        try { & npm version $version --no-git-tag-version 2>&1 | ForEach-Object { Write-Host "  $_" }; $code = $LASTEXITCODE } finally { Pop-Location }
        if ($code -ne 0) { throw "npm version $version --no-git-tag-version ist gescheitert." }
    } finally { $ErrorActionPreference = $alt }
    if ((Lesen (Join-Path $wt 'Cargo.lock')).Text -notmatch "(?m)^name = `"$([regex]::Escape($name))`"\r?\nversion = `"$([regex]::Escape($version))`"") { throw "Cargo.lock trägt $name nicht mit $version." }

    $nachricht = Join-Path ([IO.Path]::GetTempPath()) ("freigabe-nachricht-" + [guid]::NewGuid().ToString('N') + '.txt')
    Schreiben $nachricht "chore(release): $tag`n" $false
    try {
        $pfade = @('CHANGELOG.txt', 'Cargo.toml', 'Cargo.lock', 'web/package.json', 'web/package-lock.json') | Where-Object { Test-Path -LiteralPath (Join-Path $wt $_) }
        [void](GitLauf $wt (@('add', '--') + $pfade))
        $rest = (GitLauf $wt @('status', '--porcelain', '--untracked-files=no')).Text -split "`n" | Where-Object { $_ -and $_ -notmatch '^M  ' }
        if ($rest) { throw "Im Worktree hat sich mehr geändert als erwartet: $($rest -join '; ')" }
        $c = GitLauf $wt @('commit', '-q', '-F', $nachricht) -Still
        if ($c.Code -ne 0) { throw "Der Commit chore(release): $tag ging nicht durch (Hooks?): $($c.Text)" }
        $j = Join-Path $wt 'web\node_modules'
        if (Test-Path -LiteralPath $j) { [IO.Directory]::Delete($j, $false) }
        [void](GitLauf $Repo @('worktree', 'remove', $wt))
        $wt = $null
        # Die Hooks dauern Minuten: Nur der geprüfte Stand bekommt den Merge.
        if ((GitLauf $Repo @('symbolic-ref', '--quiet', 'HEAD') -Still).Text -cne 'refs/heads/main' -or (GitLauf $Repo @('rev-parse', 'refs/heads/main')).Text -cne $Basis -or (GitLauf $Repo @('status', '--porcelain')).Text) {
            Write-Host "Abbruch: Während des Festschreibens hat sich main bewegt, der Arbeitsbaum steht nicht mehr auf main oder ist nicht sauber; geprüft war $Basis."
            exit 1
        }
        $m = GitLauf $Repo @('merge', '--no-ff', '-q', '-F', $nachricht, $zweig) -Still
        if ($m.Code -ne 0) {
            [void](GitLauf $Repo @('merge', '--abort') -Still)
            throw "Der Merge nach main ging nicht durch: $($m.Text)"
        }
    } finally { Remove-Item -LiteralPath $nachricht -Force -ErrorAction SilentlyContinue }
    if ((GitLauf $Repo @('rev-parse', 'refs/heads/main^1')).Text -cne $Basis) { throw "Der Merge liegt nicht auf main über $Basis." }
    $gemergt = $true
    [void](GitLauf $Repo @('tag', $tag, 'refs/heads/main'))
    $getaggt = $true
    [void](GitLauf $Repo @('branch', '-d', $zweig))
    $angelegt = $false
    Write-Host "Festgeschrieben: Fassung $version ($zahlen) als Merge chore(release): $tag auf main, leichter Tag $tag darauf. Hinaus ging nichts."
    exit 0
} catch {
    Write-Host "FEHLER $($_.Exception.Message)"
    if ($getaggt) { Write-Host "main hat den Merge chore(release): $tag und den Tag; nur der Zweig $zweig blieb stehen. Nichts ging hinaus." }
    elseif ($gemergt) { Write-Host "main hat den Merge chore(release): $tag, aber der Tag fehlt; der Zweig $zweig bleibt stehen. Nichts ging hinaus." }
    exit 2
} finally {
    if ($wt -and (Test-Path -LiteralPath $wt)) {
        $j = Join-Path $wt 'web\node_modules'
        # Erst die Junction lösen, damit das Aufräumen nie in ihr Ziel greift.
        if (Test-Path -LiteralPath $j) { try { [IO.Directory]::Delete($j, $false) } catch { } }
        if (-not (Test-Path -LiteralPath $j)) { [void](GitLauf $Repo @('worktree', 'remove', '--force', $wt) -Still) }
    }
    if ($angelegt -and -not $gemergt) {
        if ((GitLauf $Repo @('branch', '-D', $zweig) -Still).Code -eq 0) { Write-Host "Zurückgenommen: Worktree und Zweig $zweig; main ist unverändert. Nichts ging hinaus." }
        else { Write-Host "main ist unverändert, aber der Zweig $zweig blieb stehen (git worktree list); erst entfernen. Nichts ging hinaus." }
    }
}
