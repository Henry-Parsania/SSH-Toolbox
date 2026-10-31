# Q2: memory used by SSH Toolbox with one terminal open, including its webview processes.
# Start the app, wait until the prompt shows, then run this from any PowerShell window.
#   powershell -File scripts\measure-memory.ps1 [-Name app]
# Uses "private working set", so memory shared between processes isn't counted twice.
param([string]$Name = "app")

$root = Get-Process -Name $Name -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $root) { Write-Error "No running process called '$Name'. Start the app first."; exit 1 }

$all = Get-CimInstance Win32_Process
$ids = New-Object System.Collections.Generic.HashSet[int]
$queue = New-Object System.Collections.Generic.Queue[int]
[void]$ids.Add($root.Id); $queue.Enqueue($root.Id)
while ($queue.Count -gt 0) {
    $parent = $queue.Dequeue()
    foreach ($p in $all | Where-Object { $_.ParentProcessId -eq $parent }) {
        if ($ids.Add([int]$p.ProcessId)) { $queue.Enqueue([int]$p.ProcessId) }
    }
}

$perf = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process |
    Where-Object { $ids.Contains([int]$_.IDProcess) }
$total = 0
foreach ($p in $perf) {
    $mb = [math]::Round($p.WorkingSetPrivate / 1MB, 1)
    $total += $p.WorkingSetPrivate
    "{0,8} MB  {1} (pid {2})" -f $mb, $p.Name, $p.IDProcess
}
"-------------------------------------"
"{0,8} MB  total for {1} processes (target: under 250 MB)" -f ([math]::Round($total / 1MB, 1)), $perf.Count
