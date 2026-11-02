param(
    [Parameter(Mandatory = $true)]
    [int] $RootProcessId,
    [ValidateRange(2, 30)]
    [int] $DurationSeconds = 6,
    [string] $Phase = 'window-open'
)

$ErrorActionPreference = 'Stop'

function Get-ProcessTreeSample {
    $inventory = @(Get-CimInstance Win32_Process | Select-Object ProcessId, ParentProcessId)
    $ids = [System.Collections.Generic.HashSet[int]]::new()
    $null = $ids.Add($RootProcessId)
    do {
        $changed = $false
        foreach ($item in $inventory) {
            if ($ids.Contains([int]$item.ParentProcessId) -and $ids.Add([int]$item.ProcessId)) {
                $changed = $true
            }
        }
    } while ($changed)
    $items = @()
    foreach ($processKey in $ids) {
        $instance = Get-Process -Id $processKey -ErrorAction SilentlyContinue
        if ($null -ne $instance) {
            $items += [pscustomobject]@{
                id = $instance.Id
                name = $instance.ProcessName
                workingSetBytes = $instance.WorkingSet64
                privateBytes = $instance.PrivateMemorySize64
                cpuSeconds = $instance.TotalProcessorTime.TotalSeconds
            }
        }
    }
    [pscustomobject]@{
        timeUtc = [DateTime]::UtcNow.ToString('o')
        processes = $items
        workingSetBytes = ($items | Measure-Object workingSetBytes -Sum).Sum
        privateBytes = ($items | Measure-Object privateBytes -Sum).Sum
    }
}

if ($null -eq (Get-Process -Id $RootProcessId -ErrorAction SilentlyContinue)) {
    throw "Root process is not running: $RootProcessId"
}
$before = Get-ProcessTreeSample
$timer = [Diagnostics.Stopwatch]::StartNew()
Start-Sleep -Seconds $DurationSeconds
$after = Get-ProcessTreeSample
$timer.Stop()
$deltaCpu = 0.0
foreach ($current in $after.processes) {
    $previous = $before.processes | Where-Object id -EQ $current.id
    if ($null -ne $previous) {
        $deltaCpu += [Math]::Max(0, $current.cpuSeconds - $previous.cpuSeconds)
    }
}
[pscustomobject]@{
    phase = $Phase
    elapsedSeconds = $timer.Elapsed.TotalSeconds
    logicalProcessors = [Environment]::ProcessorCount
    stableProcessCpuSeconds = $deltaCpu
    stableProcessCpuPercentOfOneCore = 100 * $deltaCpu / $timer.Elapsed.TotalSeconds
    note = 'Working sets include shared pages; CPU delta covers processes present in both samples. This is a bounded local smoke measurement, not a minimum-memory guarantee.'
    before = $before
    after = $after
} | ConvertTo-Json -Depth 5
