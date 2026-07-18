# Fan a task out to an installed agent CLI, logging to .agents/runs/.
# See .agents/README.md for the workflow and ground rules.
#
#   .\.agents\dispatch.ps1 -Agent devin -Task "..."               # wait, print result
#   .\.agents\dispatch.ps1 -Agent agy   -Task "..." -Background   # fire-and-forget
param(
    [Parameter(Mandatory = $true)][ValidateSet('devin', 'agy')][string]$Agent,
    [Parameter(Mandatory = $true)][string]$Task,
    [switch]$Background,
    [string]$TimeoutMinutes = '15'
)

$repo = Split-Path $PSScriptRoot -Parent
$runs = Join-Path $PSScriptRoot 'runs'
New-Item -ItemType Directory -Force $runs | Out-Null
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$log = Join-Path $runs "$stamp-$Agent.log"

# Every brief gets the repo ground rules prepended so one-shot agents behave.
$preamble = "You are working in the git repo at $repo. Read AGENTS.md and follow it. " +
    "Only create or edit the files your task names. Do not commit. Task: "
$prompt = $preamble + $Task

switch ($Agent) {
    'devin' { $exe = 'devin'; $cliArgs = @('-p', '--permission-mode', 'accept-edits', '--', $prompt) }
    'agy' { $exe = 'agy'; $cliArgs = @('-p', $prompt, '--mode', 'accept-edits', '--print-timeout', "${TimeoutMinutes}m") }
}

"[$stamp] $Agent task: $Task" | Set-Content $log
if ($Background) {
    Start-Process -FilePath $exe -ArgumentList $cliArgs -WorkingDirectory $repo `
        -RedirectStandardOutput "$log.out" -RedirectStandardError "$log.err" -NoNewWindow
    Write-Host "dispatched $Agent in background; logs: $log.out"
}
else {
    & $exe @cliArgs 2>&1 | Tee-Object -Append $log
}
