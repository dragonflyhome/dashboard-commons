<#
.SYNOPSIS
    Nimmt den Dashboard-Dienst wieder heraus (als Administrator).
.DESCRIPTION
    Entfernt die Aufgabe des Boten, stoppt und löscht den Dienst und nimmt die
    Rechte des Dienstkontos im Repo wieder weg: am Repo-Ordner selbst und an
    projects, data, web, docs\entscheidungen, docs\zielbild und docs\anleitung (und, falls
    vergeben, das Durchlaufrecht auf den Ordnern über dem Repo). Ältere Regeln (vererbt auf dem ganzen Repo, eigene
    auf web\build) gehen dabei mit weg. Reste einer abgebrochenen Probe
    (Dienst <Dienstname>-Test und sein Ordner) räumt es mit weg. Den
    Testordner C:\ProgramData\<Dienstname>-Test löscht es nur, wenn er den
    Administratoren (oder SYSTEM) gehört und nichts darin ein Reparse-Punkt
    (Verknüpfung, Junction) ist; sonst bleibt er liegen, mit einem Satz.

    Administrator-Sitzung: Start am besten ohne Profil, mit vollem Pfad:
      C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe -NoProfile
        -NoExit -ExecutionPolicy Bypass -File <Repo>\scripts\Dienst-Entfernen.ps1
    (Windows-Taste+R, eintippen, Strg+Umschalt+Enter). Das Skript setzt
    PSModulePath auf die Systempfade, lädt ScheduledTasks mit vollem Pfad,
    arbeitet in System32 und ruft sc.exe und icacls.exe mit vollem Pfad auf.

    Der Ordner C:\ProgramData\<Dienstname> bleibt liegen, mit Datenspeicher,
    Sicherungen und Protokoll. Es wird nichts gelöscht und nichts
    zurückkopiert. Danach geht scripts\Start-Dashboard.ps1 wieder wie früher.

    Jeder Schritt prüft zuerst, ob es noch etwas zu tun gibt; ein zweiter Lauf
    schadet nicht.

    Exit-Codes:
      0  fertig
      1  ein Schritt ist gescheitert (der Satz nennt ihn)
      5  ohne Administrator-Rechte
.PARAMETER Dienstname
    Name des Dienstes (Standard Dashboard).
#>
[CmdletBinding()]
param(
    [ValidatePattern('^[A-Za-z][A-Za-z0-9-]{0,39}$')][string]$Dienstname = 'Dashboard'
)

$ErrorActionPreference = 'Stop'

$repo = Split-Path -Parent $PSScriptRoot
# Systemordner von Windows selbst, nicht aus Umgebungsvariablen (die kann das
# Konto als Benutzer-Variable überschreiben, auch für den erhöhten Lauf).
$sys32 = [Environment]::GetFolderPath([Environment+SpecialFolder]::System)
$programData = [Environment]::GetFolderPath([Environment+SpecialFolder]::CommonApplicationData)
$programFiles = [Environment]::GetFolderPath([Environment+SpecialFolder]::ProgramFiles)
$scPfad = Join-Path $sys32 'sc.exe'
$icaclsPfad = Join-Path $sys32 'icacls.exe'
$wurzel = Join-Path $programData $Dienstname
$sidSystem = 'S-1-5-18'
$sidAdmins = 'S-1-5-32-544'
$tomlPfad = Join-Path $wurzel 'dienst.toml'
$aufgabenOrdner = "\$Dienstname\"
$aufgabenName = "$Dienstname-Bote"
$script:schritt = 'Vorbereitung'

function Starte([string]$exe, [string]$argumente) {
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $exe
    $psi.Arguments = $argumente
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.CreateNoWindow = $true
    $psi.WorkingDirectory = $sys32
    try {
        $enc = [Text.Encoding]::GetEncoding([Globalization.CultureInfo]::CurrentCulture.TextInfo.OEMCodePage)
        $psi.StandardOutputEncoding = $enc
        $psi.StandardErrorEncoding = $enc
    } catch { }
    $p = [Diagnostics.Process]::Start($psi)
    $aus = $p.StandardOutput.ReadToEndAsync()
    $fehl = $p.StandardError.ReadToEnd()
    $p.WaitForExit()
    [pscustomobject]@{ Code = $p.ExitCode; Text = ($aus.Result + $fehl).Trim() }
}

function ScExe([string]$argumente) { Starte $scPfad $argumente }

function Icacls([string]$argumente) {
    $r = Starte $icaclsPfad $argumente
    if ($r.Code -ne 0) { throw "icacls $argumente meldet: $($r.Text)" }
}

$rp = [IO.FileAttributes]::ReparsePoint

# Erster Reparse-Punkt in $pfad selbst oder darunter; $null, wenn keiner.
# Folgt keinem (Get-ChildItem -Recurse folgt in 5.1 verlinkten Ordnern).
function Erste-Verknuepfung([string]$pfad) {
    $startEintrag = Get-Item -LiteralPath $pfad -Force
    if (($startEintrag.Attributes -band $rp) -ne 0) { return $startEintrag.FullName }
    if (-not $startEintrag.PSIsContainer) { return $null }
    $stapel = New-Object System.Collections.Generic.Stack[string]
    $stapel.Push($startEintrag.FullName)
    while ($stapel.Count -gt 0) {
        $d = New-Object IO.DirectoryInfo($stapel.Pop())
        foreach ($e in $d.EnumerateFileSystemInfos()) {
            if (($e.Attributes -band $rp) -ne 0) { return $e.FullName }
            if (($e.Attributes -band [IO.FileAttributes]::Directory) -ne 0) { $stapel.Push($e.FullName) }
        }
    }
    return $null
}

# Entfernt alle ausdrücklichen Einträge an $pfad, deren SID nicht in $erlaubt steht.
function Nur-Diese([string]$pfad, [string[]]$erlaubt) {
    $acl = Get-Acl -LiteralPath $pfad
    $fremd = @($acl.GetAccessRules($true, $false, [Security.Principal.SecurityIdentifier]) |
        ForEach-Object { $_.IdentityReference.Value } | Where-Object { $erlaubt -notcontains $_ } | Sort-Object -Unique)
    foreach ($s in $fremd) { Icacls ('"{0}" /remove *{1}' -f $pfad, $s) }
}

# Rest des Testordners löschen, ohne Verknüpfungen zu folgen. $null, wenn
# gelöscht; sonst der Satz, warum er liegen bleibt.
function Loesche-Testordner([string]$pfad) {
    $nachsatz = 'er bleibt liegen; bitte den Ordner prüfen und von Hand entfernen.'
    $i = Get-Item -LiteralPath $pfad -Force
    if (($i.Attributes -band $rp) -ne 0) { return "Testordner $pfad ist selbst eine Verknüpfung; $nachsatz" }
    $b = (Get-Acl -LiteralPath $pfad).GetOwner([Security.Principal.SecurityIdentifier]).Value
    if ($b -ne $sidAdmins -and $b -ne $sidSystem) { return "Testordner $pfad gehört nicht den Administratoren (Besitzer $b); $nachsatz" }
    # Erst allen außer SYSTEM und Administratoren die ausdrücklichen Einträge
    # entziehen (dann legt niemand sonst mehr etwas hinein), dann suchen.
    foreach ($o in @($pfad) + @('bin', 'daten', 'sicherung', 'protokoll', 'protokoll\bote', 'geheim' | ForEach-Object { Join-Path $pfad $_ })) {
        if ((Test-Path -LiteralPath $o -PathType Container) -and -not ((Get-Item -LiteralPath $o -Force).Attributes -band $rp)) {
            Nur-Diese $o @($sidSystem, $sidAdmins)
        }
    }
    $v = Erste-Verknuepfung $pfad
    if ($v) { return "Testordner $pfad enthält eine Verknüpfung ($v); $nachsatz" }
    Remove-Item -LiteralPath $pfad -Recurse -Force
    return $null
}

function DienstSid([string]$name) {
    $r = ScExe "showsid $name"
    if ($r.Text -match '(S-1-5-80(-\d+)+)') { return $Matches[1] }
    throw "Die SID von NT SERVICE\$name ließ sich nicht ermitteln: $($r.Text)"
}

function DienstStatus([string]$name) {
    $s = Get-Service -Name $name -ErrorAction SilentlyContinue
    if ($s) { return [string]$s.Status }
    return $null
}

function Gleicher-Pfad([string]$a, [string]$b) {
    if (-not $a -or -not $b) { return $false }
    (($a -replace '/', '\').TrimEnd('\')) -ieq (($b -replace '/', '\').TrimEnd('\'))
}

function Liegt-In([string]$pfad, [string]$ordner) {
    if (-not $pfad -or -not $ordner) { return $false }
    $o = ($ordner -replace '/', '\').TrimEnd('\') + '\'
    (($pfad -replace '/', '\').TrimEnd('\') + '\').StartsWith($o, [StringComparison]::OrdinalIgnoreCase)
}

function Vorfahren([string]$pfad, [string]$bis) {
    $liste = @()
    if (-not (Liegt-In $pfad $bis)) { return $liste }
    $p = Split-Path -Parent $pfad
    while ($p -and (Liegt-In $p $bis)) {
        $liste += $p
        if (Gleicher-Pfad $p $bis) { break }
        $p = Split-Path -Parent $p
    }
    return $liste
}

function TomlWert([string]$text, [string]$schluessel) {
    $m = [regex]::Match($text, '(?m)^\s*' + $schluessel + '\s*=\s*(?:"([^"]*)"|(\d+))')
    if (-not $m.Success) { return $null }
    if ($m.Groups[1].Success) { return $m.Groups[1].Value }
    return $m.Groups[2].Value
}

function EntferneDienst([string]$name) {
    $st = DienstStatus $name
    if (-not $st) { Write-Host "Dienst $name gibt es nicht (mehr)"; return }
    if ($st -ne 'Stopped') {
        $r = ScExe "stop $name"
        $ende = (Get-Date).AddSeconds(20)
        while ((DienstStatus $name) -ne 'Stopped' -and (Get-Date) -lt $ende) { Start-Sleep -Milliseconds 500 }
        if ((DienstStatus $name) -ne 'Stopped') { throw "Der Dienst $name hielt nicht binnen 20 s an ($($r.Text))." }
        Write-Host "Dienst $name angehalten"
    }
    $r = ScExe "delete $name"
    if ($r.Code -ne 0 -and $r.Code -ne 1072) { throw "sc delete $($name) meldet: $($r.Text)" }
    Write-Host "Dienst $name gelöscht"
}

# Orte, an denen Dienst-Einrichten.ps1 dem Dienstkonto Rechte gibt: das Repo
# selbst (lesen, ohne Vererbung; ältere Fassungen: vererbt), projects, data,
# web, docs\entscheidungen, docs\zielbild (vererbt; docs seit 4d),
# docs\anleitung (seit 6b), web\build (eigene Regel älterer Fassungen) und
# die Ordner über dem Repo (nur mit -Durchlaufrecht).
function RepoOrte([string]$profil) {
    $orte = @($repo) + @('projects', 'data', 'web', 'docs\entscheidungen', 'docs\zielbild', 'docs\anleitung', 'web\build' | ForEach-Object { Join-Path $repo $_ })
    # Umzug des Inhalts: die Ordner aus [inhalt] in dienst.toml (Quellen, daten, entscheidungen).
    $orte += @($script:inhaltOrte)
    if ($profil) { $orte += @(Vorfahren $repo $profil) }
    return $orte
}

function EntferneRechte([string]$name, [string]$profil) {
    $sid = DienstSid $name
    $orte = RepoOrte $profil
    $etwas = $false
    foreach ($p in $orte) {
        if (-not (Test-Path -LiteralPath $p)) { continue }
        $acl = Get-Acl -LiteralPath $p
        $da = @($acl.GetAccessRules($true, $false, [Security.Principal.SecurityIdentifier]) | Where-Object { $_.IdentityReference.Value -eq $sid })
        if ($da.Count -gt 0) {
            Icacls ('"{0}" /remove:g *{1}' -f $p, $sid)
            Write-Host "Rechte von NT SERVICE\$name an $p entfernt"
            $etwas = $true
        }
    }
    if (-not $etwas) { Write-Host "NT SERVICE\$name hat keine Rechte (mehr) im Repo" }
}

trap {
    Write-Host "FEHLER im Schritt '$($script:schritt)': $($_.Exception.Message)"
    Write-Host 'Das Skript hat hier aufgehört. Ein zweiter Lauf macht beim offenen Schritt weiter.'
    exit 1
}

$ich = [Security.Principal.WindowsIdentity]::GetCurrent()
if (-not (New-Object Security.Principal.WindowsPrincipal($ich)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Host 'Dienst-Entfernen.ps1 braucht Administrator-Rechte. Bitte ohne Profil starten: Windows-Taste+R, eintippen'
    Write-Host "  $sys32\WindowsPowerShell\v1.0\powershell.exe -NoProfile -NoExit -ExecutionPolicy Bypass -File `"$PSCommandPath`""
    Write-Host 'und mit Strg+Umschalt+Enter als Administrator starten (siehe LIESMICH-Dienst.txt).'
    exit 5
}

# Erhöhten Lauf härten: Module nur aus Systemordnern, ScheduledTasks mit
# vollem Pfad, Arbeitsordner System32 (für dieses Skript und alles, was es startet).
$env:PSModulePath = (@(
        (Join-Path $PSHOME 'Modules'),
        (Join-Path $programFiles 'WindowsPowerShell\Modules'),
        (Join-Path $sys32 'WindowsPowerShell\v1.0\Modules')
    ) | Select-Object -Unique) -join ';'
Import-Module -Name (Join-Path $sys32 'WindowsPowerShell\v1.0\Modules\ScheduledTasks\ScheduledTasks.psd1') -ErrorAction Stop
Set-Location -LiteralPath $sys32
[Environment]::CurrentDirectory = $sys32
$profilDateien = @($PROFILE.CurrentUserAllHosts, $PROFILE.CurrentUserCurrentHost) | Where-Object { $_ -and (Test-Path -LiteralPath $_) }
if ($profilDateien -and [Environment]::CommandLine -notmatch '(?i)(^|\s)-(nop|noprofile)(\s|$)') {
    Write-Host "WARNUNG: Diese Sitzung lief wohl mit Profil ($($profilDateien -join ', ')). Sicherer: neu starten mit powershell.exe -NoProfile (siehe LIESMICH-Dienst.txt)."
}

# Was steht in den Einstellungen? (Repo und Konto des Inhabers, für Rechte und Hinweise.)
$profil = $null
$script:inhaltOrte = @()
if (Test-Path -LiteralPath $tomlPfad) {
    $t = [IO.File]::ReadAllText($tomlPfad)
    $abschnitt = [regex]::Match($t, '(?ms)^\s*\[inhalt\]\s*$(.*?)(?=^\s*\[|\z)')
    if ($abschnitt.Success) {
        $script:inhaltOrte = @([regex]::Matches($abschnitt.Groups[1].Value, '"([^"]*)"') | ForEach-Object { $_.Groups[1].Value -replace '/', '\' })
    }
    $tRepo = TomlWert $t 'repo'
    if ($tRepo -and -not (Gleicher-Pfad $tRepo $repo)) {
        Write-Host "Hinweis: dienst.toml nennt das Repo $tRepo; die Rechte werden dort entfernt."
        $repo = $tRepo -replace '/', '\'
    }
    $sid = TomlWert $t 'inhaber_sid'
    if ($sid) {
        $eintrag = Get-ItemProperty -LiteralPath "HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\$sid" -ErrorAction SilentlyContinue
        if ($eintrag) { $profil = [Environment]::ExpandEnvironmentVariables($eintrag.ProfileImagePath) }
    }
}
if (-not $profil) { $profil = $env:USERPROFILE }

# 1. Bote.
$script:schritt = 'Aufgabe des Boten entfernen'
$aufgabe = Get-ScheduledTask -TaskPath $aufgabenOrdner -TaskName $aufgabenName -ErrorAction SilentlyContinue
if ($aufgabe) {
    if ($aufgabe.State -eq 'Running') { Stop-ScheduledTask -TaskPath $aufgabenOrdner -TaskName $aufgabenName }
    Unregister-ScheduledTask -TaskPath $aufgabenOrdner -TaskName $aufgabenName -Confirm:$false
    Write-Host "Aufgabe $aufgabenOrdner$aufgabenName entfernt"
} else {
    Write-Host "Aufgabe $aufgabenOrdner$aufgabenName gibt es nicht (mehr)"
}
# Leeren Ordner der Aufgabenplanung wegräumen.
try {
    $plan = New-Object -ComObject Schedule.Service
    $plan.Connect()
    $ordner = $plan.GetFolder($aufgabenOrdner.TrimEnd('\'))
    if ($ordner.GetTasks(1).Count -eq 0 -and $ordner.GetFolders(0).Count -eq 0) {
        $plan.GetFolder('\').DeleteFolder($Dienstname, 0)
        Write-Host "Leeren Ordner $aufgabenOrdner in der Aufgabenplanung entfernt"
    }
} catch { }
$boteExe = Join-Path $wurzel 'bin\dashboard-bote.exe'
# Wächter (--waechter) und Bote laufen mit demselben Programm; der Wächter
# zuerst, damit er den Boten nicht neu startet.
$beendet = 0
$ende = (Get-Date).AddSeconds(5)
do {
    $laufend = @(Get-CimInstance Win32_Process -Filter "Name='dashboard-bote.exe'" -ErrorAction SilentlyContinue |
        Where-Object { $_.ExecutablePath -and (Gleicher-Pfad $_.ExecutablePath $boteExe) } |
        Sort-Object @{ Expression = { if ([string]$_.CommandLine -match '(^|\s)--waechter(\s|$)') { 0 } else { 1 } } })
    foreach ($pr in $laufend) { try { Stop-Process -Id $pr.ProcessId -Force -ErrorAction Stop; $beendet++ } catch { } }
    if ($laufend.Count -eq 0) { break }
    Start-Sleep -Milliseconds 300
} while ((Get-Date) -lt $ende)
if ($beendet -gt 0) { Write-Host "Wächter und Bote beendet ($beendet Prozesse)" }

# 2. Dienst.
$script:schritt = "Dienst $Dienstname stoppen und löschen"
EntferneDienst $Dienstname

# 3. Rechte am Repo.
$script:schritt = 'Rechte am Repo entfernen'
EntferneRechte $Dienstname $profil

# 4. Reste einer abgebrochenen Probe.
$script:schritt = 'Reste der Probe wegräumen'
$tName = "$Dienstname-Test"
$tWurzel = Join-Path $programData $tName
if (Get-Service -Name $tName -ErrorAction SilentlyContinue) { EntferneDienst $tName }
$tSid = DienstSid $tName
foreach ($p in (RepoOrte $profil)) {
    if (-not (Test-Path -LiteralPath $p)) { continue }
    $acl = Get-Acl -LiteralPath $p
    if (@($acl.GetAccessRules($true, $false, [Security.Principal.SecurityIdentifier]) | Where-Object { $_.IdentityReference.Value -eq $tSid }).Count -gt 0) {
        Icacls ('"{0}" /remove:g *{1}' -f $p, $tSid)
        Write-Host "Rechte von NT SERVICE\$tName an $p entfernt"
    }
}
if (Test-Path -LiteralPath $tWurzel) {
    $testRest = Loesche-Testordner $tWurzel
    if ($testRest) { Write-Host "Achtung: $testRest" } else { Write-Host "Testordner $tWurzel entfernt" }
}

# 5. Hinweise.
$alteDatei = Join-Path $profil '.var\dashboard\dashboard.redb'
$dienstDatei = Join-Path $wurzel 'daten\dashboard.redb'
Write-Host ''
Write-Host "Fertig. Der Dienst $Dienstname und die Aufgabe des Boten sind weg."
Write-Host "Der Ordner $wurzel bleibt liegen: Datenspeicher (daten), Sicherungen (sicherung), Protokoll (protokoll)."
Write-Host 'Von Hand starten geht wieder: scripts\Start-Dashboard.ps1'
if (Test-Path -LiteralPath $dienstDatei) {
    Write-Host ''
    Write-Host "Achtung: Der Datenspeicher $dienstDatei ist jetzt der aktuellere."
    Write-Host "Start-Dashboard.ps1 nimmt aber $alteDatei (Stand vom Einrichten)."
    Write-Host 'Wer den neueren Stand behalten will, kopiert ihn vorher zurück, in diesem Fenster (als Administrator):'
    Write-Host "  Rename-Item -LiteralPath '$alteDatei' -NewName 'dashboard.vor-dienst.redb'"
    Write-Host "  Copy-Item -LiteralPath '$dienstDatei' -Destination '$alteDatei'"
}
exit 0
