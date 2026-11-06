<#
.SYNOPSIS
    Baut den frischen Wurzel-Commit als Entwurf in einem Wegwerf-Klon und prüft ihn ganz.
.DESCRIPTION
    Frischer Start ohne alte Historie: Der veröffentlichte Stand beginnt mit
    einem einzigen Wurzel-Commit ohne Eltern. Das Skript
      1. klont das Code-Repo in einen neuen Ordner (-Ziel, etwa in einem temporären Ordner)
         und entfernt dort das Remote des Klons (es gibt kein Remote),
      2. nimmt den Baum von -Stand (Standard HEAD), lässt die Orte des Inhalts
         weg (wie scripts\Inhalt-Umziehen.ps1: projects\ außer _vorlage, data\,
         docs\entscheidungen, docs\architektur-stand.md, erzeugte Typen) und
         schreibt ihn als einen Wurzel-Commit ohne Eltern als Ranke <ranke2@proton.me> auf den
         lokalen Zweig -Zweig (Standard entwurf/wurzel),
      3. prüft den Zweig mit scripts\Vor-Push.ps1 (ganzer Baum, Autor,
         Committer, Nachricht) mit den Namenslisten aus -Inhalt,
      4. schreibt die Liste „das geht raus“ nach -Bericht: Dateibaum (Ordner mit
         Zahl der Dateien, dazu jede Datei außer web\vendor) und das Ergebnis
         der Prüfung, nie die gefundenen Werte; bei Funden ohne Ordner und Dateien.
    Kein Remote, kein Push, kein gh. Das Original-Repo bleibt unverändert.
.PARAMETER Repo
    Das Code-Repo (Standard: der Ordner über scripts).
.PARAMETER Stand
    Welcher Stand die Grundlage ist (Standard HEAD).
.PARAMETER Ziel
    Neuer Ordner für den Klon (darf nicht da sein).
.PARAMETER Inhalt
    Das Inhalts-Repo mit pruefer\namen.toml.
.PARAMETER Bericht
    Die Markdown-Datei für die Liste „das geht raus“.
.PARAMETER Zweig
    Lokaler Zweig für den Entwurf (Standard entwurf/wurzel).
#>
[CmdletBinding()]
param(
    [string]$Repo,
    [string]$Stand = 'HEAD',
    [Parameter(Mandatory)][string]$Ziel,
    [string]$Inhalt,
    [Parameter(Mandatory)][string]$Bericht,
    [string]$Zweig = 'entwurf/wurzel'
)

$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'Inhalt-Orte.ps1')
$utf8 = New-Object System.Text.UTF8Encoding($false)
try { [Console]::OutputEncoding = $utf8 } catch { }

# Wie in Vor-Push.ps1: Die Konsole nennt das Benutzerprofil nur als %USERPROFILE%.
function OhneProfil([string]$t) {
    if ($null -eq $t) { return '' }
    if ($env:USERPROFILE) { $t = $t.Replace($env:USERPROFILE, '%USERPROFILE%') }
    return [regex]::Replace($t, '(?i)[a-z]:[\\/]+(?:users|benutzer)[\\/]+[^\\/"''<>|:*?\r\n]+', '%USERPROFILE%')
}

function GitIn([string]$wo, [string[]]$a, [hashtable]$umgebung = @{}) {
    $alt = @{}
    foreach ($k in $umgebung.Keys) { $alt[$k] = [Environment]::GetEnvironmentVariable($k); [Environment]::SetEnvironmentVariable($k, $umgebung[$k]) }
    try {
        $aus = & git -C $wo -c core.quotepath=off @a 2>&1
        if ($LASTEXITCODE -ne 0) { throw "git $($a -join ' ') endete mit $LASTEXITCODE ($(($aus | Out-String).Trim()))" }
        return (($aus | ForEach-Object { "$_" }) -join "`n")
    } finally {
        foreach ($k in $alt.Keys) { [Environment]::SetEnvironmentVariable($k, $alt[$k]) }
    }
}

try {
    if (-not $Repo) { $Repo = Split-Path -Parent $PSScriptRoot }
    $Repo = (Resolve-Path -LiteralPath $Repo).Path
    if (Test-Path -LiteralPath $Ziel) { throw "$Ziel gibt es schon; bitte einen neuen Ordner nennen." }
    $quelle = (GitIn $Repo @('rev-parse', $Stand)).Trim()
    Write-Host (OhneProfil "Klon von $Repo ($($quelle.Substring(0, 9))) nach $Ziel ...")
    & git clone -q --no-hardlinks --no-checkout $Repo $Ziel
    if ($LASTEXITCODE -ne 0) { throw 'git clone ist gescheitert.' }
    foreach ($r in @((GitIn $Ziel @('remote')) -split "`n" | Where-Object { $_ })) { GitIn $Ziel @('remote', 'remove', $r) | Out-Null }

    # Baum ohne die Orte des Inhalts, über einen eigenen Index (kein Checkout nötig).
    $index = Join-Path $Ziel '.git\entwurf-index'
    $env_ = @{ GIT_INDEX_FILE = $index }
    GitIn $Ziel @('read-tree', $quelle) $env_ | Out-Null
    $projekte = @(((GitIn $Ziel @('ls-files', '-z', '--', 'projects') $env_) -split "`0") | Where-Object { $_ -match '^projects/([^/]+)/' -and $Matches[1] -ne '_vorlage' } | ForEach-Object { 'projects/' + ($_ -split '/')[1] } | Sort-Object -Unique)
    $raus = @($projekte) + @($InhaltRaus | Where-Object { (GitIn $Ziel @('ls-files', '--', $_) $env_).Trim() })
    if ($raus.Count -gt 0) { GitIn $Ziel (@('rm', '-r', '-q', '--cached', '--') + $raus) $env_ | Out-Null }
    $baum = (GitIn $Ziel @('write-tree') $env_).Trim()
    $wer = @{ GIT_AUTHOR_NAME = 'Ranke'; GIT_AUTHOR_EMAIL = 'ranke2@proton.me'; GIT_COMMITTER_NAME = 'Ranke'; GIT_COMMITTER_EMAIL = 'ranke2@proton.me' }
    $nachricht = 'chore: erste öffentliche Fassung'
    $commit = (GitIn $Ziel @('commit-tree', '--no-gpg-sign', $baum, '-m', $nachricht) $wer).Trim()
    GitIn $Ziel @('update-ref', "refs/heads/$Zweig", $commit) | Out-Null
    Remove-Item -LiteralPath $index -Force
    Write-Host "Wurzel-Commit $($commit.Substring(0, 9)) auf $Zweig (ohne Eltern, als Ranke)."

    # Prüfung mit dem Pre-Push-Prüfer.
    $ps = if (Get-Command pwsh -ErrorAction SilentlyContinue) { 'pwsh' } else { 'powershell.exe' }
    $argumente = @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', (Join-Path $PSScriptRoot 'Vor-Push.ps1'), '-Repo', $Ziel, '-Baum', $Zweig, '-Commits', $Zweig, '-AufMain')
    if ($Inhalt) { $argumente += @('-Inhalt', $Inhalt) }
    $ergebnis = @(& $ps @argumente 2>&1 | ForEach-Object { "$_" })
    $code = $LASTEXITCODE

    # Liste „das geht raus“.
    $dateien = @(((GitIn $Ziel @('ls-tree', '-r', '-z', '--name-only', $Zweig)) -split "`0") | Where-Object { $_ })
    $ordner = $dateien | Group-Object { $t = $_ -split '/'; if ($t.Count -le 1) { '(Wurzel)' } elseif ($t[0] -eq 'web' -and $t.Count -gt 2) { "$($t[0])/$($t[1])" } else { $t[0] } } | Sort-Object Name
    $vendor = $dateien | Where-Object { $_ -like 'web/vendor/*' } | Group-Object { ($_ -split '/')[2] } | Sort-Object Name
    $urteil = switch ($code) { 0 { 'Nichts gefunden.' } 1 { 'Abgewiesen: Es gibt Funde (unten, ohne Werte).' } default { 'Der Prüfer konnte nicht prüfen (Liste fehlt, kaputt oder eigener Fehler).' } }
    $md = New-Object System.Text.StringBuilder
    [void]$md.AppendLine('# Das geht raus (Entwurf)')
    [void]$md.AppendLine('')
    [void]$md.AppendLine("Erzeugt von scripts\Wurzel-Entwurf.ps1 am $('{0:dd.MM.yyyy HH:mm}' -f (Get-Date)). Nur ein Entwurf: kein Remote, kein Push.")
    [void]$md.AppendLine('')
    [void]$md.AppendLine("- Grundlage: Stand $($quelle.Substring(0, 9)) des Code-Repos, ohne die Orte des Inhalts ($($raus.Count) Pfade weggelassen).")
    [void]$md.AppendLine("- Wurzel-Commit: $($commit.Substring(0, 9)) auf dem lokalen Zweig $Zweig, ohne Eltern, Autor und Committer Ranke.")
    [void]$md.AppendLine("- Dateien: $($dateien.Count).")
    [void]$md.AppendLine("- Prüfung (scripts\Vor-Push.ps1, Exit $code): $urteil")
    [void]$md.AppendLine('')
    [void]$md.AppendLine('## Prüfung')
    [void]$md.AppendLine('')
    [void]$md.AppendLine('Je Fund Datei, Zeile und Muster; die gefundenen Werte stehen absichtlich nicht hier.')
    [void]$md.AppendLine('')
    [void]$md.AppendLine('```')
    foreach ($z in $ergebnis) { [void]$md.AppendLine($z) }
    [void]$md.AppendLine('```')
    [void]$md.AppendLine('')
    # Mit Funden (auch in Pfaden) keine Namen von Ordnern und Dateien im Bericht.
    if ($code -ne 0) {
        [void]$md.AppendLine('Ordner und Dateien stehen hier nicht, solange die Prüfung Funde hat (ein Pfad kann selbst der Fund sein).')
        [IO.File]::WriteAllText($Bericht, $md.ToString(), $utf8)
        Write-Host (OhneProfil "Liste geschrieben (ohne Dateien, weil es Funde gibt): $Bericht")
        foreach ($z in $ergebnis | Select-Object -Last 3) { Write-Host "  $z" }
        exit $code
    }
    [void]$md.AppendLine('## Ordner')
    [void]$md.AppendLine('')
    [void]$md.AppendLine('| Ordner | Dateien |')
    [void]$md.AppendLine('|---|---|')
    foreach ($o in $ordner) { [void]$md.AppendLine("| $($o.Name) | $($o.Count) |") }
    [void]$md.AppendLine('')
    [void]$md.AppendLine('## web/vendor (fremde Pakete, je Paket)')
    [void]$md.AppendLine('')
    foreach ($v in $vendor) { [void]$md.AppendLine("- $($v.Name): $($v.Count) Dateien") }
    [void]$md.AppendLine('')
    [void]$md.AppendLine('## Alle Dateien außer web/vendor')
    [void]$md.AppendLine('')
    [void]$md.AppendLine('```')
    foreach ($d in ($dateien | Where-Object { $_ -notlike 'web/vendor/*' })) { [void]$md.AppendLine($d) }
    [void]$md.AppendLine('```')
    [IO.File]::WriteAllText($Bericht, $md.ToString(), $utf8)
    Write-Host (OhneProfil "Liste geschrieben: $Bericht")
    foreach ($z in $ergebnis | Select-Object -Last 3) { Write-Host "  $z" }
    exit $code
} catch {
    Write-Host (OhneProfil "FEHLER $($_.Exception.Message)")
    exit 2
}
