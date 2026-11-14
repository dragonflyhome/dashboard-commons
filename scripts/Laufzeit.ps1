<#
.SYNOPSIS
    Gemeinsamer Baustein: Laufzeit und Auslastung der Rechenkerne am Ende eines Skripts.
.DESCRIPTION
    Einbinden mit `. (Join-Path $PSScriptRoot 'Laufzeit.ps1')`, am Anfang
    `$uhr = UhrStart`, am Ende `UhrZeile $uhr`. Die Auslastung kommt aus
    GetSystemTimes (alle Kerne, alle Prozesse, auch Kindprozesse wie cargo):
    Anteil der Zeit, in der die Kerne nicht im Leerlauf waren.
#>
if (-not ('Dashboard.Systemzeit' -as [type])) {
    Add-Type -Namespace Dashboard -Name Systemzeit -MemberDefinition @'
[System.Runtime.InteropServices.DllImport("kernel32.dll", SetLastError = true)]
public static extern bool GetSystemTimes(out long leerlauf, out long kern, out long nutzer);
'@
}

function Systemzeiten {
    $l = [long]0; $k = [long]0; $n = [long]0
    if (-not [Dashboard.Systemzeit]::GetSystemTimes([ref]$l, [ref]$k, [ref]$n)) { return $null }
    # Die Kernzeit enthält den Leerlauf.
    return [pscustomobject]@{ Leerlauf = $l; Gesamt = $k + $n }
}

function UhrStart {
    return [pscustomobject]@{ Uhr = [Diagnostics.Stopwatch]::StartNew(); Zeiten = Systemzeiten }
}

# Der Satz allein (für Tests); UhrZeile gibt ihn aus.
function UhrSatz($uhr) {
    $s = $uhr.Uhr.Elapsed.TotalSeconds.ToString('0.0', [Globalization.CultureInfo]::GetCultureInfo('de-DE'))
    $kerne = [Environment]::ProcessorCount
    $jetzt = Systemzeiten
    if ($null -eq $uhr.Zeiten -or $null -eq $jetzt -or $jetzt.Gesamt -le $uhr.Zeiten.Gesamt) {
        return "Laufzeit $s s; Auslastung der $kerne Rechenkerne unbekannt."
    }
    $gesamt = $jetzt.Gesamt - $uhr.Zeiten.Gesamt
    $prozent = [math]::Round(100 * (1 - ($jetzt.Leerlauf - $uhr.Zeiten.Leerlauf) / $gesamt))
    return "Laufzeit $s s; Rechenkerne zu $([math]::Clamp($prozent, 0, 100)) % ausgelastet ($kerne Kerne)."
}

function UhrZeile($uhr) { Write-Host (UhrSatz $uhr) }
