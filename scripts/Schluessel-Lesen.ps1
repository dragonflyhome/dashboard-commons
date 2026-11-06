# Liest einen Wert aus dem Windows Credential Manager (Generic Credential)
# und gibt ihn auf stdout aus. Nur für Sitzungen/Skripte gedacht; der Inhaber trägt
# Werte über http://127.0.0.1:8787/schluessel ein.
#
#   .\scripts\Schluessel-Lesen.ps1 wetter-api             -> Wert von ranke/wetter-api
#   $pw = & .\scripts\Schluessel-Lesen.ps1 wetter-api
#
# Läuft in Windows PowerShell 5.1 und PowerShell 7, ohne Zusatzmodule.
param(
    [Parameter(Mandatory = $true)][string]$Name,
    [string]$Prefix = "ranke/"
)
$ErrorActionPreference = "Stop"

if (-not ("RankeCred" -as [type])) {
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class RankeCred {
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct CREDENTIAL {
        public uint Flags; public uint Type; public string TargetName; public string Comment;
        public System.Runtime.InteropServices.ComTypes.FILETIME LastWritten;
        public uint CredentialBlobSize; public IntPtr CredentialBlob; public uint Persist;
        public uint AttributeCount; public IntPtr Attributes; public string TargetAlias; public string UserName;
    }
    [DllImport("advapi32", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern bool CredReadW(string target, uint type, uint flags, out IntPtr cred);
    [DllImport("advapi32")] static extern void CredFree(IntPtr buf);
    public static string Read(string target) {
        IntPtr p;
        if (!CredReadW(target, 1, 0, out p)) return null;
        try {
            var c = (CREDENTIAL)Marshal.PtrToStructure(p, typeof(CREDENTIAL));
            return Marshal.PtrToStringUni(c.CredentialBlob, (int)c.CredentialBlobSize / 2);
        } finally { CredFree(p); }
    }
}
"@
}

$wert = [RankeCred]::Read("$Prefix$Name")
if ($null -eq $wert) {
    Write-Error "Kein Eintrag '$Prefix$Name' im Credential Manager. Bitte in data\schluessel.toml anfordern und den Inhaber auf http://127.0.0.1:8787/schluessel verweisen."
    exit 2
}
Write-Output $wert
