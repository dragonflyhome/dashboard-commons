#Requires -Version 7.7
<#
.SYNOPSIS
    Probe: übersetzt die Bibliothek für macOS (aarch64-apple-darwin), ohne Abbruch.
.DESCRIPTION
    cargo check --lib --profile test --target aarch64-apple-darwin. Das Profil
    test, weil die Bibliothek außerhalb von Windows ohne cfg(test) absichtlich
    nicht baut (Verschlüsselung nur mit DPAPI).

    Bei --target gelten die rustflags aus .cargo/config.toml nicht für die
    Abhängigkeiten des Hosts; windows-sys (raw-dylib) ruft dann das dlltool
    des self-contained-Ordners, dem der Assembler fehlt. Darum liegt
    llvm-dlltool.exe als dlltool.exe in einem eigenen Ordner im Anwendungscache
    und steht nur für diesen Lauf vorn im PATH. Quelle ist die erste gefundene:
    %USERPROFILE%\.cargo\bin, sonst der sysroot der aktiven Toolchain
    (lib\rustlib\<host>\bin).

    Eine Probe: Das Ergebnis steht in einem Satz da, der Exit-Code ist immer 0,
    auch bei einer Ausnahme (rustup fehlt, Kopieren scheitert, cargo startet
    nicht). Fehlt das Ziel oder llvm-dlltool, sagt sie es (rustup target add
    aarch64-apple-darwin; rustup component add llvm-tools).
.PARAMETER Ziel
    Das Ziel (Standard aarch64-apple-darwin).
#>
[CmdletBinding()]
param([string]$Ziel = 'aarch64-apple-darwin')

$ErrorActionPreference = 'Stop'
# Die Sätze mit Umlauten kommen auch umgeleitet (Freigeben.ps1, Tests) als UTF-8 an.
try { [Console]::OutputEncoding = [Text.UTF8Encoding]::new($false) } catch { }
$uhr = $null
$start = [Diagnostics.Stopwatch]::StartNew()
$repo = Split-Path -Parent $PSScriptRoot

function Fertig([string]$satz) {
    Write-Host $satz
    # Fehlt Laufzeit.ps1 oder wirft sie, kommt eine Ersatzzeile statt eines zweiten Satzes über den catch.
    try { UhrZeile $uhr } catch {
        Write-Host "Laufzeit $($start.Elapsed.TotalSeconds.ToString('0.0', [Globalization.CultureInfo]::GetCultureInfo('de-DE'))) s; Auslastung der $([Environment]::ProcessorCount) Rechenkerne unbekannt."
    }
    exit 0
}

# Sätze ohne den echten Benutzerordner.
function Ohne([string]$pfad) {
    if (-not $env:USERPROFILE) { return $pfad }
    return $pfad.Replace($env:USERPROFILE, '%USERPROFILE%', [StringComparison]::OrdinalIgnoreCase)
}

try {
    . (Join-Path $PSScriptRoot 'Laufzeit.ps1')
    $uhr = UhrStart

    # Im Repo, damit ein Toolchain-Override dort für rustup und rustc gleich gilt.
    Push-Location -LiteralPath $repo
    try {
        $ziele = @(& rustup target list --installed 2>$null)
        $rc = $LASTEXITCODE
    } finally { Pop-Location; $global:LASTEXITCODE = 0 }
    if ($rc -ne 0) { Fertig "Mac-Probe rot (nur Probe, kein Abbruch): rustup target list --installed endete mit $rc." }
    if ($ziele -notcontains $Ziel) { Fertig "Mac-Probe übersprungen: Das Ziel $Ziel ist nicht installiert (rustup target add $Ziel)." }

    $orte = @(Join-Path $env:USERPROFILE '.cargo\bin')
    $quelle = Join-Path $orte[0] 'llvm-dlltool.exe'
    if (-not (Test-Path -LiteralPath $quelle -PathType Leaf)) {
        Push-Location -LiteralPath $repo
        try {
            $sysroot = "$(& rustc --print sysroot 2>$null)".Trim()
            $wirt = "$(@(& rustc -vV 2>$null) -match '^host: ')" -replace '^host: ', ''
        } finally { Pop-Location; $global:LASTEXITCODE = 0 }
        $orte += if ($sysroot -and $wirt) { Join-Path $sysroot "lib\rustlib\$($wirt.Trim())\bin" } else { 'dem sysroot der Toolchain (lib\rustlib\<host>\bin)' }
        $quelle = Join-Path $orte[1] 'llvm-dlltool.exe'
        if (-not (Test-Path -LiteralPath $quelle -PathType Leaf)) {
            Fertig "Mac-Probe übersprungen: llvm-dlltool.exe fehlt in $(Ohne $orte[0]) und in $(Ohne $orte[1]) (rustup component add llvm-tools)."
        }
    }

    $ordner = Join-Path $env:LOCALAPPDATA 'Dashboard\cache\mac-dlltool'
    $dlltool = Join-Path $ordner 'dlltool.exe'
    if (-not (Test-Path -LiteralPath $dlltool) -or (Get-Item -LiteralPath $quelle).Length -ne (Get-Item -LiteralPath $dlltool).Length) {
        [void][IO.Directory]::CreateDirectory($ordner)
        Copy-Item -LiteralPath $quelle -Destination $dlltool -Force
    }
    $env:PATH = "$ordner;$env:PATH"

    $zeilen = New-Object Collections.Generic.List[string]
    Push-Location -LiteralPath $repo
    try {
        $ErrorActionPreference = 'Continue'
        # Kein Wert statt 0: Ein cargo, das keinen Exit-Code setzt, gilt nicht als grün.
        $global:LASTEXITCODE = $null
        & cargo check --lib --profile test --locked --target $Ziel 2>&1 | ForEach-Object { $zeilen.Add("$_") }
        $code = $LASTEXITCODE
    } finally { Pop-Location; $ErrorActionPreference = 'Stop' }

    $warnungen = @($zeilen | Where-Object { $_ -match '^warning: ' -and $_ -notmatch 'generated \d+ warning' }).Count
    $fehler = @($zeilen | Where-Object { $_ -match '^error(\[E\d+\])?: ' }).Count
    if ($code -eq 0) {
        Fertig "Mac-Probe grün: cargo check für $Ziel ohne Fehler, $warnungen Warnungen."
    }
    $zeilen | Select-Object -Last 40 | ForEach-Object { Write-Host "  $_" }
    $wie = if ($null -eq $code) { 'endete ohne Exit-Code' } else { "endete mit $code" }
    Fertig "Mac-Probe rot (nur Probe, kein Abbruch): cargo check für $Ziel $wie, $fehler Fehler, $warnungen Warnungen."
} catch {
    Fertig "Mac-Probe rot (nur Probe, kein Abbruch): $(Ohne ($_.Exception.Message -replace '\s+', ' ').Trim())"
}
