#Requires -Version 7.0
[CmdletBinding(DefaultParameterSetName='Synthetic')]
param(
    [Parameter(Mandatory)][string]$Binary,
    [Parameter(Mandatory)][string]$Manifest,
    [Parameter(Mandatory)][string]$OutputRoot,
    [Parameter(Mandatory,ParameterSetName='Journal')][string]$Journal,
    [Parameter(ParameterSetName='Synthetic')][switch]$Synthetic,
    [ValidateRange(1,10000)][int]$Rounds = 100,
    [ValidateRange(1,4)][int]$Workers = 1,
    [ValidateRange(1,3600)][int]$Seconds = 120,
    [ValidateRange(256,8192)][int]$MemoryLimitMiB = 2048,
    [switch]$PageHeap,
    [string]$Debugger,
    [string]$SymbolDirectory
)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows) { throw 'This supervisor is Windows-only' }
$binaryFile = Get-Item -LiteralPath $Binary
$expected = Get-Content -LiteralPath $Manifest -Raw | ConvertFrom-Json
$hash = (Get-FileHash -LiteralPath $binaryFile.FullName).Hash
if ($expected.mode -ne 'offline-no-host-no-models-no-tools' -or $hash -ne $expected.sha256) { throw 'Offline artifact manifest/hash mismatch' }
if (-not [IO.Path]::IsPathFullyQualified($OutputRoot) -or -not (Test-Path -LiteralPath $OutputRoot -PathType Container)) { throw 'OutputRoot must be an existing absolute directory' }
if ($PageHeap -and (-not $Debugger -or -not (Test-Path -LiteralPath $Debugger -PathType Leaf))) { throw 'PageHeap requires a local cdb.exe path' }
if ($SymbolDirectory -and -not (Test-Path -LiteralPath $SymbolDirectory -PathType Container)) { throw 'Symbols must be a local directory' }
if ($PSCmdlet.ParameterSetName -eq 'Journal') { $Journal = (Get-Item -LiteralPath $Journal).FullName }
$runDir = Join-Path $OutputRoot ('projection-repro-' + [guid]::NewGuid().ToString('N'))
# CDB command files have their own language: prohibit command/quote delimiters.
foreach ($path in @($runDir, $binaryFile.DirectoryName)) {
    if ($path -match '[";\r\n]') { throw 'Path contains unsupported debugger delimiters' }
}
New-Item -ItemType Directory -Path $runDir -ErrorAction Stop | Out-Null
$imageName = 'xharness-projection-repro-' + [guid]::NewGuid().ToString('N') + '.exe'
$imagePath = Join-Path $runDir $imageName
Copy-Item -LiteralPath $binaryFile.FullName -Destination $imagePath
Copy-Item -LiteralPath $Manifest -Destination (Join-Path $runDir 'build-manifest.json')
$payload = Join-Path $runDir 'payload'
$arguments = @('--output', $payload, '--rounds', "$Rounds", '--workers', "$Workers", '--seconds', "$Seconds")
if ($PSCmdlet.ParameterSetName -eq 'Journal') { $arguments += @('--journal', $Journal) } else { $arguments += '--synthetic' }
$process = $null
$lease = $null
$reason = $null
$exitCode = $null
$peak = 0L
$restored = -not $PageHeap
$verified = $false
$stopFile = Join-Path $runDir 'pageheap.stop'
try {
    if ($PageHeap) {
        $helper = Join-Path $PSScriptRoot 'projection-repro-pageheap-lease.ps1'
        # Start-Process joins arguments; reject embedded quotes and quote paths explicitly.
        foreach ($path in @($helper,$imagePath)) { if ($path -match '["\r\n]') { throw 'Unsafe helper path' } }
        $helperArgs = '-NoProfile -NonInteractive -File "{0}" -ImagePath "{1}" -Sha256 {2} -LeaseSeconds {3}' -f $helper,$imagePath,$hash,($Seconds+180)
        $lease = Start-Process -FilePath (Join-Path $PSHOME 'pwsh.exe') -ArgumentList $helperArgs -Verb RunAs -WindowStyle Hidden -PassThru
        $readyDeadline = [DateTime]::UtcNow.AddSeconds(30)
        while (-not (Test-Path -LiteralPath (Join-Path $runDir 'pageheap.ready'))) {
            if ($lease.HasExited -or [DateTime]::UtcNow -ge $readyDeadline) { throw 'PageHeap helper did not become ready' }
            Start-Sleep -Milliseconds 200
        }
        $dumpPath = (Join-Path $runDir 'first-fault.dmp').Replace('\','/')
        $capture = '.echo PROJECTION_NATIVE_FAULT; .exr -1; .ecxr; kv; .dump /ma \"' + $dumpPath + '\"; q'
        $commands = @('!gflag', '!heap -s')
        foreach ($event in @('av','0xc0000374','0xc0000409','bpe')) { $commands += ('sxe -c "' + $capture + '" ' + $event) }
        $commands += 'g'
        $commandPath = Join-Path $runDir 'debugger.commands'
        [IO.File]::WriteAllLines($commandPath, $commands)
    }
    $start = [Diagnostics.ProcessStartInfo]::new()
    $start.UseShellExecute = $false
    $start.CreateNoWindow = $true
    $start.RedirectStandardOutput = $true
    $start.RedirectStandardError = $true
    if ($PageHeap) {
        $start.FileName = (Get-Item -LiteralPath $Debugger).FullName
        $symbols = $binaryFile.DirectoryName
        if ($SymbolDirectory) { $symbols += ';' + (Get-Item -LiteralPath $SymbolDirectory).FullName }
        foreach ($arg in @('-y',$symbols,'-logo',(Join-Path $runDir 'debugger.log'),'-cf',$commandPath,$imagePath)) { $start.ArgumentList.Add($arg) }
    } else { $start.FileName = $imagePath }
    foreach ($arg in $arguments) { $start.ArgumentList.Add($arg) }
    $process = [Diagnostics.Process]::Start($start)
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    $deadline = [DateTime]::UtcNow.AddSeconds($Seconds+30)
    $samples = [IO.StreamWriter]::new((Join-Path $runDir 'resources.jsonl'),$false)
    try {
        while (-not $process.WaitForExit(250)) {
            # Unique image name; do not inspect or kill installed Host processes.
            $children = @(Get-Process -Name ([IO.Path]::GetFileNameWithoutExtension($imageName)) -ErrorAction SilentlyContinue)
            $private = 0L
            foreach ($child in $children) { $private += $child.PrivateMemorySize64; $child.Dispose() }
            $peak = [Math]::Max($peak,$private)
            $samples.WriteLine((@{utc=[DateTime]::UtcNow.ToString('o');privateBytes=$private} | ConvertTo-Json -Compress))
            $samples.Flush()
            if ($private -gt ($MemoryLimitMiB * 1MB)) { $reason = 'sampled-memory-budget'; break }
            if ([DateTime]::UtcNow -ge $deadline) { $reason = 'supervisor-timeout'; break }
        }
        if ($reason -and -not $process.HasExited) { $process.Kill($true); $process.WaitForExit() }
        $exitCode = $process.ExitCode
    } finally { $samples.Dispose() }
    [IO.File]::WriteAllText((Join-Path $runDir 'stdout.log'),$stdout.GetAwaiter().GetResult())
    [IO.File]::WriteAllText((Join-Path $runDir 'stderr.log'),$stderr.GetAwaiter().GetResult())
    if ($PageHeap) {
        $log = Get-Content -LiteralPath (Join-Path $runDir 'debugger.log') -Raw
        $verified = $log -match '02000000' -and $log -match 'Page heap has been enabled'
        if ($log -match '(?m)^PROJECTION_NATIVE_FAULT\r?$') { $reason = 'native-fault' }
        if (-not $verified -and -not $reason) { $reason = 'pageheap-not-verified' }
    }
    if (-not $reason) {
        $resultFile = Join-Path $payload 'result.json'
        if (-not (Test-Path -LiteralPath $resultFile)) { $reason = 'missing-result' }
        else {
            $result = Get-Content -LiteralPath $resultFile -Raw | ConvertFrom-Json
            if ($result.failed -or $result.timedOut -or $result.completedRounds -ne ($Rounds*$Workers) -or $exitCode -ne 0) { $reason = 'workload-failed' }
        }
    }
} catch {
    $reason = 'setup-or-supervisor-error'
    [IO.File]::WriteAllText((Join-Path $runDir 'supervisor-error.txt'), $_.Exception.Message)
    Write-Warning 'Supervisor failed; inspect local evidence and PageHeap cleanup status.'
} finally {
    if ($process -and -not $process.HasExited) { $process.Kill($true); $process.WaitForExit() }
    if ($PageHeap) {
        [IO.File]::WriteAllText($stopFile,'stop')
        if ($lease) { $null = $lease.WaitForExit(10000) }
        $root = [Microsoft.Win32.RegistryKey]::OpenBaseKey('LocalMachine','Registry64')
        $key = $root.OpenSubKey('SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options\' + $imageName)
        $restored = $null -eq $key
        if ($key) { $key.Dispose() }
        $root.Dispose()
    }
    $summary = @{reason=$reason;exitCode=$exitCode;pageHeapRequested=[bool]$PageHeap;pageHeapVerified=$verified;settingsRestored=$restored;peakSampledPrivateBytes=$peak;runDirectory=$runDir;nativeCrashReproduced=($reason -eq 'native-fault')}
    $summary | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $runDir 'supervisor-result.json')
    $summary | ConvertTo-Json
    if ($process) { $process.Dispose() }
    if ($lease) { $lease.Dispose() }
}
if (-not $restored) { throw "PageHeap cleanup unverified: inspect exact IFEO entry $imageName" }
if ($reason) { throw "Isolated run did not pass: $reason. Evidence: $runDir" }
