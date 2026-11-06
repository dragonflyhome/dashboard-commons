# Vendor-Pruefen.ps1 - prüft die Vendor-Kopie des UI-Toolkits (web\vendor).
#
# Etappe 3a-3, Entscheidung ui-toolkit-als-vendor,
# Zielbild Abschnitt „Formsprache: vier Schichten“. Das Skript ändert nichts und
# lädt nichts herunter. Es zeigt je Paket unter web\vendor\<paket>\:
#
#   - Name, Version und Lizenz (aus HERKUNFT.toml und package.json)
#   - ob die Kopie unverändert ist: Prüfsumme über alle Dateien (inhalt_sha256)
#     neu gerechnet und mit HERKUNFT.toml verglichen
#   - ob keine Markdown-Datei mehr drin ist und eine Lizenzdatei da ist
#   - ob die Lizenz erlaubt ist (MIT, Apache-2.0, BSD, 0BSD, ISC)
#   - ob web\package.json und web\package-lock.json das Paket als
#     file:vendor\<paket> führen
#   - mit Netz (ohne -OhneNetz): ob die Registry für diese Version dieselbe
#     Prüfsumme des Tarballs meldet und ob es eine neuere Fassung gibt. Dafür
#     nur „npm view“ (Metadaten), kein Download
#
# Rechnung von inhalt_sha256 (gleich in Vendor-Holen.ps1): alle Dateien des
# Paket-Ordners außer HERKUNFT.toml, Pfad relativ mit „/“, sortiert nach
# Ordinalwert; je Datei eine Zeile "<pfad>:<sha256 hex klein>" mit "\n";
# über den UTF-8-Text dieser Zeilen sha256, hex klein.
#
# Aufruf (PowerShell 7 oder Windows PowerShell 5.1, kein Fenster):
#   pwsh -NoProfile -File scripts\Vendor-Pruefen.ps1
#   pwsh -NoProfile -File scripts\Vendor-Pruefen.ps1 -OhneNetz
#
# Exit-Codes:
#   0  Alles in Ordnung (neuere Fassungen sind nur ein Hinweis).
#   1  Mindestens ein Fund (Prüfsumme, Lizenz, Markdown, package.json, Lockfile).
#   2  web\vendor fehlt.
param(
    [switch]$OhneNetz,
    [string]$Repo = ''
)
$ErrorActionPreference = 'Stop'
# Windows PowerShell 5.1 kennt $PSScriptRoot im param-Block nicht immer.
if (-not $Repo) { $Repo = Split-Path -Parent $PSScriptRoot }
# Satz auf stderr und Ende mit Code (Write-Error endete mit Stop immer mit 1).
function Abbruch([string]$satz, [int]$code) { [Console]::Error.WriteLine($satz); exit $code }

$web = Join-Path $Repo 'web'
$vendor = Join-Path $web 'vendor'
if (-not (Test-Path -LiteralPath $vendor)) {
    Abbruch "web\vendor fehlt ($vendor)." 2
}

$ERLAUBT = @('MIT', 'Apache-2.0', 'BSD-2-Clause', 'BSD-3-Clause', '0BSD', 'ISC')

function Lies-Toml([string]$pfad) {
    # Nur die einfache Form aus HERKUNFT.toml: schluessel = "text" | zahl | [..]
    $werte = @{}
    foreach ($zeile in [IO.File]::ReadAllLines($pfad, [Text.Encoding]::UTF8)) {
        if ($zeile -match '^\s*#' -or $zeile -notmatch '^\s*([A-Za-z0-9_]+)\s*=\s*(.*)$') { continue }
        $k = $Matches[1]; $v = $Matches[2].Trim()
        if ($v -match '^"(.*)"$') { $werte[$k] = $Matches[1] }
        elseif ($v -match '^\[(.*)\]$') {
            $werte[$k] = @([regex]::Matches($Matches[1], '"([^"]*)"') | ForEach-Object { $_.Groups[1].Value })
        }
        else { $werte[$k] = $v }
    }
    return $werte
}

function Inhalt-Sha256([string]$ordner) {
    $basis = (Resolve-Path -LiteralPath $ordner).Path.TrimEnd('\')
    # [string[]]: sonst sortiert Array.Sort die Hüllen von PowerShell, nicht ordinal.
    [string[]]$rel = @(Get-ChildItem -LiteralPath $basis -Recurse -File -Force |
        ForEach-Object { $_.FullName.Substring($basis.Length + 1).Replace('\', '/') } |
        Where-Object { $_ -ne 'HERKUNFT.toml' })
    [Array]::Sort($rel, [StringComparer]::Ordinal)
    $sb = New-Object System.Text.StringBuilder
    foreach ($r in $rel) {
        $h = (Get-FileHash -LiteralPath (Join-Path $basis $r) -Algorithm SHA256).Hash.ToLowerInvariant()
        [void]$sb.Append("${r}:$h`n")
    }
    $sha = [Security.Cryptography.SHA256]::Create()
    try {
        $bytes = $sha.ComputeHash((New-Object Text.UTF8Encoding $false).GetBytes($sb.ToString()))
    }
    finally { $sha.Dispose() }
    return @{ summe = (($bytes | ForEach-Object { $_.ToString('x2') }) -join ''); dateien = $rel }
}

# npm ohne npm.cmd und ohne Fenster: node.exe mit npm-cli.js daneben.
function Npm-Ausgabe([string[]]$argumente, [int]$sekunden = 30) {
    $node = (Get-Command node -ErrorAction SilentlyContinue).Source
    if (-not $node) { return $null }
    $cli = Join-Path (Split-Path -Parent $node) 'node_modules\npm\bin\npm-cli.js'
    if (-not (Test-Path -LiteralPath $cli)) { return $null }
    $psi = New-Object Diagnostics.ProcessStartInfo
    $psi.FileName = $node
    $psi.Arguments = (@("`"$cli`"") + ($argumente | ForEach-Object { "`"$_`"" })) -join ' '
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.WorkingDirectory = $env:TEMP
    $p = [Diagnostics.Process]::Start($psi)
    $aus = $p.StandardOutput.ReadToEndAsync()
    [void]$p.StandardError.ReadToEndAsync()
    if (-not $p.WaitForExit($sekunden * 1000)) {
        try { $p.Kill() } catch { }
        return $null
    }
    if ($p.ExitCode -ne 0) { return $null }
    return $aus.Result
}

# Paket-Ordner: vendor\<name> und vendor\@scope\<name>
$ordner = @()
foreach ($d in Get-ChildItem -LiteralPath $vendor -Directory) {
    if ($d.Name.StartsWith('@')) {
        foreach ($u in Get-ChildItem -LiteralPath $d.FullName -Directory) { $ordner += $u }
    }
    else { $ordner += $d }
}

$pkg = Get-Content -LiteralPath (Join-Path $web 'package.json') -Raw -Encoding UTF8 | ConvertFrom-Json
# Das Lockfile hat einen Schlüssel "" (das Projekt selbst); den kann
# ConvertFrom-Json in PowerShell 5.1 nicht. Darum genügt hier der Text.
$lockText = Get-Content -LiteralPath (Join-Path $web 'package-lock.json') -Raw -Encoding UTF8
$alleAbh = @{}
foreach ($teil in @($pkg.dependencies, $pkg.devDependencies)) {
    if ($null -eq $teil) { continue }
    foreach ($e in $teil.PSObject.Properties) { $alleAbh[$e.Name] = [string]$e.Value }
}

$netz = -not $OhneNetz
$funde = 0
$neuer = 0
$zeilen = @()
foreach ($o in $ordner) {
    $name = if ($o.Parent.Name.StartsWith('@')) { "$($o.Parent.Name)/$($o.Name)" } else { $o.Name }
    $satz = @()
    $hkPfad = Join-Path $o.FullName 'HERKUNFT.toml'
    if (-not (Test-Path -LiteralPath $hkPfad)) {
        $zeilen += [pscustomobject]@{ Paket = $name; Version = '?'; Lizenz = '?'; Stand = 'HERKUNFT.toml fehlt' }
        $funde++
        continue
    }
    $hk = Lies-Toml $hkPfad
    $pj = Get-Content -LiteralPath (Join-Path $o.FullName 'package.json') -Raw -Encoding UTF8 | ConvertFrom-Json
    if ($pj.version -ne $hk.version) { $satz += "package.json sagt $($pj.version)"; $funde++ }
    if ($hk.paket -ne $name) { $satz += "HERKUNFT nennt $($hk.paket)"; $funde++ }

    $inhalt = Inhalt-Sha256 $o.FullName
    if ($inhalt.summe -ne $hk.inhalt_sha256) { $satz += 'Inhalt geändert (inhalt_sha256 passt nicht)'; $funde++ }
    if ($hk.dateien -and [int]$hk.dateien -ne $inhalt.dateien.Count) { $satz += "$($inhalt.dateien.Count) statt $($hk.dateien) Dateien"; $funde++ }
    $md = @($inhalt.dateien | Where-Object { $_ -match '\.md$' })
    if ($md.Count -gt 0) { $satz += "Markdown: $($md -join ', ')"; $funde++ }
    $lizDatei = @($inhalt.dateien | Where-Object { $_ -match '^(licen[cs]e|copying)(\.txt)?$' })
    if ($lizDatei.Count -eq 0) { $satz += 'keine Lizenzdatei'; $funde++ }
    $lizenz = [string]$hk.lizenz
    if ($ERLAUBT -notcontains $lizenz) { $satz += "Lizenz $lizenz nicht erlaubt"; $funde++ }

    $soll = "file:vendor/$name"
    if ($alleAbh[$name] -ne $soll) { $satz += "package.json: $($alleAbh[$name]) statt $soll"; $funde++ }
    $muster = '"node_modules/' + [regex]::Escape($name) + '":\s*\{[^{}]*?"resolved":\s*"' + [regex]::Escape($soll) + '"'
    if ($lockText -notmatch $muster) { $satz += 'Lockfile zeigt nicht auf vendor'; $funde++ }

    if ($netz) {
        $roh = Npm-Ausgabe @('view', "$name@$($hk.version)", 'dist.integrity', '--json')
        if ($null -eq $roh) {
            $netz = $false
            Write-Host 'Kein Netz oder npm fehlt: Registry-Vergleich und neuere Fassungen übersprungen.'
        }
        else {
            $reg = ($roh | ConvertFrom-Json)
            if ($reg -ne $hk.pruefsumme) { $satz += 'Registry meldet eine andere Prüfsumme!'; $funde++ }
            $neu = Npm-Ausgabe @('view', $name, 'version', '--json')
            if ($null -ne $neu) {
                $neueste = [string]($neu | ConvertFrom-Json)
                if ($neueste -and $neueste -ne $hk.version) { $satz += "neuer: $neueste"; $neuer++ }
            }
        }
    }
    $zeilen += [pscustomobject]@{
        Paket   = $name
        Version = $hk.version
        Lizenz  = $lizenz
        Stand   = if ($satz.Count -eq 0) { 'in Ordnung' } else { $satz -join '; ' }
    }
}

# Gegenrichtung: file:vendor-Einträge ohne Ordner
foreach ($k in $alleAbh.Keys) {
    if ($alleAbh[$k] -like 'file:vendor/*' -and -not (Test-Path -LiteralPath (Join-Path $vendor $k))) {
        $zeilen += [pscustomobject]@{ Paket = $k; Version = '?'; Lizenz = '?'; Stand = 'in package.json, aber kein Ordner in web\vendor' }
        $funde++
    }
}

$zeilen | Sort-Object Paket | Format-Table -AutoSize -Wrap | Out-String -Width 200 | Write-Host
Write-Host ("{0} Pakete, {1} Funde{2}." -f $ordner.Count, $funde, $(if ($neuer -gt 0) { ", $neuer mit neuerer Fassung (Vendor-Holen.ps1)" } else { '' }))
if ($funde -gt 0) { exit 1 }
exit 0
