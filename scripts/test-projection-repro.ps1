#Requires -Version 7.0
[CmdletBinding()]
param([string]$Binary,[string]$Manifest)
$ErrorActionPreference = 'Stop'
foreach ($name in @('run-projection-repro.ps1','projection-repro-pageheap-lease.ps1')) {
    $tokens=$null; $errors=$null
    [void][Management.Automation.Language.Parser]::ParseFile((Join-Path $PSScriptRoot $name),[ref]$tokens,[ref]$errors)
    if ($errors.Count) { throw "PowerShell parse failed: $name" }
}
if ($Binary) {
    if (-not $Manifest) { throw 'Manifest required with binary' }
    & (Join-Path $PSScriptRoot 'run-projection-repro.ps1') -Binary $Binary -Manifest $Manifest -OutputRoot ([IO.Path]::GetTempPath()) -Synthetic -Rounds 2 -Workers 2 -Seconds 60
    $blocked = $false
    try {
        & (Join-Path $PSScriptRoot 'run-projection-repro.ps1') -Binary $Binary -Manifest $Manifest -OutputRoot 'relative' -Synthetic
    } catch { $blocked = $true }
    if (-not $blocked) { throw 'Relative output root was not rejected' }
}
Write-Output 'Projection supervisor checks passed (PageHeap requires separate elevated native verification).'
