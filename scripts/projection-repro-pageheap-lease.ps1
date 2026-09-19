#Requires -Version 7.0
# This elevated helper changes only a new, uniquely named reproducer's IFEO key.
# It does not execute the reproducer, read conversations, or configure the Host.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$ImagePath,
    [Parameter(Mandatory)][ValidatePattern('^[A-Fa-f0-9]{64}$')][string]$Sha256,
    [ValidateRange(30,3900)][int]$LeaseSeconds = 300
)
$ErrorActionPreference = 'Stop'
$image = Get-Item -LiteralPath $ImagePath
if ($image.Name -notmatch '^xharness-projection-repro-[a-f0-9]{32}\.exe$') { throw 'Not a unique repro image' }
if ((Get-FileHash -LiteralPath $image.FullName).Hash -ne $Sha256) { throw 'Image hash mismatch' }
if (Test-Path -LiteralPath (Join-Path $image.DirectoryName 'pageheap.stop')) { return }
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
if (-not ([Security.Principal.WindowsPrincipal]::new($identity)).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) { throw 'PageHeap helper needs elevation' }
$root = [Microsoft.Win32.RegistryKey]::OpenBaseKey('LocalMachine', 'Registry64')
$parent = $root.OpenSubKey('SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options', $true)
$existing = $parent.OpenSubKey($image.Name)
if ($null -ne $existing) { $existing.Dispose(); throw 'Refusing to change an existing IFEO entry' }
$owned = $false
try {
    if (Test-Path -LiteralPath (Join-Path $image.DirectoryName 'pageheap.stop')) { return }
    $key = $parent.CreateSubKey($image.Name)
    $owned = $true
    $key.SetValue('ProjectionReproOwner', $Sha256, 'String')
    $key.SetValue('GlobalFlag', 0x02000000, 'DWord')
    $key.Flush()
    $key.Dispose()
    [IO.File]::WriteAllText((Join-Path $image.DirectoryName 'pageheap.ready'), $image.Name)
    $expires = [DateTime]::UtcNow.AddSeconds($LeaseSeconds)
    while ([DateTime]::UtcNow -lt $expires -and -not (Test-Path -LiteralPath (Join-Path $image.DirectoryName 'pageheap.stop'))) {
        Start-Sleep -Milliseconds 250
    }
} finally {
    if ($owned) {
        $key = $parent.OpenSubKey($image.Name)
        if ($null -ne $key) {
            $valid = $key.GetValue('ProjectionReproOwner') -eq $Sha256 -and $key.SubKeyCount -eq 0 -and
                @($key.GetValueNames() | Where-Object { $_ -notin 'GlobalFlag','ProjectionReproOwner' }).Count -eq 0
            $key.Dispose()
            if (-not $valid) { throw 'IFEO ownership changed; manual inspection required, not deleting' }
            $parent.DeleteSubKey($image.Name, $false)
        }
        [IO.File]::WriteAllText((Join-Path $image.DirectoryName 'pageheap.restored'), $image.Name)
    }
    $parent.Dispose()
    $root.Dispose()
}
