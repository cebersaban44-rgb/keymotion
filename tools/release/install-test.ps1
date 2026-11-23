#Requires -Version 7.2
<#
.SYNOPSIS
Installs Keymotion on a throwaway Windows machine and checks what installing,
connecting the coding agents, updating and uninstalling leave behind.

.DESCRIPTION
The release workflow's install-test job (.github/workflows/release.yml) runs
this on a fresh GitHub-hosted Windows runner, with the installer the run
built and the last published release's. It changes the signed-in user's
files, registry and PATH: it installs and uninstalls Keymotion, turns the
app's agent access on, and writes Claude Code and Codex settings in the
user's folder. So it refuses to run anywhere but on a GitHub-hosted runner.

The scenarios run in this order, each on what the one before left:

  a  Fresh silent install, then the app turns agent access on (PATH)
  b  keymotion-cli connect, beside the user's own settings and another copy's
  c  Silent uninstall
  d  Manual reinstall keeps the connections and PATH
  e  Update while a keymotion-cli runs
  f  Uninstall while Keymotion runs
  g  Upgrade from the last published release
  h  keymotion://background starts Keymotion without a window

Each prints PASS, FAIL or SKIP with what it saw, and the table goes to the
job's summary ($GITHUB_STEP_SUMMARY). The exit code is 1 when a scenario
failed. Logs and copies of the files the scenarios looked at go to -OutDir.

What the updater runs (tauri-plugin-updater 2.14.1, updater.rs
`updater_parameters`, with tauri.conf.json's windows.installMode "passive"):
the new setup with `/P /UPDATE`, and since the app restarts after an update,
`/R /ARGS` followed by the app's own arguments (none for a normal start).

.PARAMETER Installer
The setup this run built: Keymotion_<version>_x64-setup.exe.

.PARAMETER PreviousInstaller
The last published release's setup, for the upgrade (g). Without it, g is
skipped.

.PARAMETER OutDir
Where logs and copies of the checked files go.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)] [string] $Installer,
    [string] $PreviousInstaller,
    [string] $OutDir
)

# Only on a throwaway GitHub-hosted runner, never on someone's own computer.
if ($env:GITHUB_ACTIONS -ne 'true' -or $env:RUNNER_ENVIRONMENT -ne 'github-hosted') {
    [Console]::Error.WriteLine(
        'install-test.ps1 installs and uninstalls Keymotion, turns its agent access on, and changes this ' +
        "user's PATH, registry and Claude Code and Codex settings. It runs only on a throwaway GitHub-hosted " +
        'Actions runner (GITHUB_ACTIONS=true, RUNNER_ENVIRONMENT=github-hosted). Nothing was changed.')
    exit 2
}

Set-StrictMode -Version 3.0
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$Installer = (Resolve-Path -LiteralPath $Installer).Path
if ($PreviousInstaller) { $PreviousInstaller = (Resolve-Path -LiteralPath $PreviousInstaller).Path }
if (-not $OutDir) { $OutDir = Join-Path ($env:RUNNER_TEMP ?? [IO.Path]::GetTempPath()) 'install-test' }
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

function Get-SetupVersion([string]$Path) {
    $name = [IO.Path]::GetFileName($Path)
    if ($name -notmatch '^Keymotion_(?<v>.+)_x64-setup\.exe$') { throw "$name is not named Keymotion_<version>_x64-setup.exe" }
    $Matches['v']
}
$Version = Get-SetupVersion $Installer

# ------------------------------------------------------------- where things are

$HomeDir = $env:USERPROFILE
$InstallDir = Join-Path $env:LOCALAPPDATA 'Keymotion'
$AppExe = Join-Path $InstallDir 'keymotion.exe'
$Bin = Join-Path $InstallDir 'bin'
$CliExe = Join-Path $Bin 'keymotion-cli.exe'
$UninstallExe = Join-Path $InstallDir 'uninstall.exe'
$UninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Keymotion'
$RecordKey = 'HKCU:\Software\Keymotion\Keymotion'
$UserPathRecordKey = 'HKCU:\Software\Keymotion\UserPath'
$SchemeKey = 'HKCU:\Software\Classes\keymotion'
$SchemeCommandKey = 'HKCU:\Software\Classes\keymotion\shell\open\command'
$AumidKey = 'HKCU:\Software\Classes\AppUserModelId\dev.keymotion'
# The Microsoft Store copy's app execution alias: what its hooks name.
$Alias = Join-Path $env:LOCALAPPDATA 'Microsoft\WindowsApps\keymotion-cli.exe'
$AppData = Join-Path $env:APPDATA 'dev.keymotion'
$AppLogs = Join-Path $env:LOCALAPPDATA 'dev.keymotion\logs'
$CdpPort = 9223
# What tauri-plugin-updater runs the new setup with, for an app started
# without arguments.
$UpdaterArgs = @('/P', '/UPDATE', '/R', '/ARGS')
# An entry of the user's own on PATH, left unexpanded.
$SeedPathEntry = '%USERPROFILE%\km-install-test-tools'

$Claude = @{
    Dir = Join-Path $HomeDir '.claude'
    Settings = Join-Path $HomeDir '.claude\settings.json'
    SkillDir = Join-Path $HomeDir '.claude\skills\keymotion'
    UserSkill = Join-Path $HomeDir '.claude\skills\user-notes\SKILL.md'
}
$Codex = @{
    Dir = Join-Path $HomeDir '.codex'
    Hooks = Join-Path $HomeDir '.codex\hooks.json'
    Config = Join-Path $HomeDir '.codex\config.toml'
    RulesDir = Join-Path $HomeDir '.codex\rules'
    Rule = Join-Path $HomeDir '.codex\rules\keymotion.rules'
    UserRule = Join-Path $HomeDir '.codex\rules\default.rules'
    SkillDir = Join-Path $HomeDir '.agents\skills\keymotion'
    UserSkill = Join-Path $HomeDir '.agents\skills\user-helper\SKILL.md'
}

Add-Type -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class KmNative {
    delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);
    [DllImport("user32.dll")] static extern bool EnumWindows(EnumWindowsProc cb, IntPtr lParam);
    [DllImport("user32.dll")] static extern bool IsWindowVisible(IntPtr hWnd);
    [DllImport("user32.dll")] static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetWindowTextW(IntPtr hWnd, StringBuilder s, int n);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] static extern int GetClassNameW(IntPtr hWnd, StringBuilder s, int n);
    [DllImport("user32.dll")] static extern bool GetWindowRect(IntPtr hWnd, out RECT r);
    [DllImport("user32.dll")] public static extern IntPtr GetShellWindow();
    [StructLayout(LayoutKind.Sequential)] struct RECT { public int Left, Top, Right, Bottom; }
    [DllImport("ntdll.dll")] public static extern int NtSuspendProcess(IntPtr process);
    [DllImport("ntdll.dll")] public static extern int NtResumeProcess(IntPtr process);

    // The visible top-level windows of a process, as "class 'title' WxH".
    public static List<string> VisibleWindows(int pid) {
        var found = new List<string>();
        EnumWindows((h, l) => {
            uint owner;
            GetWindowThreadProcessId(h, out owner);
            if (owner == (uint)pid && IsWindowVisible(h)) {
                var title = new StringBuilder(256);
                GetWindowTextW(h, title, 256);
                var cls = new StringBuilder(256);
                GetClassNameW(h, cls, 256);
                RECT r;
                GetWindowRect(h, out r);
                found.Add(String.Format("{0} '{1}' {2}x{3}", cls, title, r.Right - r.Left, r.Bottom - r.Top));
            }
            return true;
        }, IntPtr.Zero);
        return found;
    }
}
'@

# ------------------------------------------------------------------- results

$Scenarios = [System.Collections.Generic.List[object]]::new()
$script:Checks = [System.Collections.Generic.List[object]]::new()
$script:Headline = $null

# One check of a scenario: printed at once, and the scenario fails if any
# of its checks does.
function Confirm-That($Ok, [string]$What, [string]$Evidence = '') {
    $ok = [bool]$Ok
    $script:Checks.Add([pscustomobject]@{ Ok = $ok; What = $What; Evidence = $Evidence })
    $mark = if ($ok) { 'ok  ' } else { 'FAIL' }
    $tail = if ($Evidence) { " -- $Evidence" } else { '' }
    Write-Host "  $mark $What$tail"
}

# What the scenario saw that is not a pass or a fail.
function Add-Note([string]$Text) {
    $script:Checks.Add([pscustomobject]@{ Ok = $null; What = $Text; Evidence = '' })
    Write-Host "  note $Text"
}

function Skip-Scenario([string]$Why) { throw "SKIP: $Why" }

function Invoke-Scenario([string]$Id, [string]$Title, [scriptblock]$Body) {
    $script:Checks = [System.Collections.Generic.List[object]]::new()
    $script:Headline = $null
    Write-Host "::group::$Id. $Title"
    $status = $null
    $reason = $null
    $clock = [Diagnostics.Stopwatch]::StartNew()
    try {
        & $Body
    } catch {
        $message = $_.Exception.Message
        if ($message.StartsWith('SKIP: ')) {
            $status = 'SKIP'
            $reason = $message.Substring(6)
        } else {
            $reason = "stopped at line $($_.InvocationInfo.ScriptLineNumber): $message"
            Write-Host "  ERROR $reason"
        }
    }
    $failed = @($script:Checks | Where-Object { $_.Ok -eq $false })
    $passed = @($script:Checks | Where-Object { $_.Ok -eq $true })
    if ($failed.Count -or ($reason -and $status -ne 'SKIP')) {
        $status = 'FAIL'
    } elseif (-not $status) {
        $status = if ($passed.Count) { 'PASS' } else { 'FAIL' }
    }
    $evidence = switch ($status) {
        'PASS' { if ($script:Headline) { $script:Headline } else { $passed[-1].What + ': ' + $passed[-1].Evidence } }
        'SKIP' { $reason }
        default {
            if ($failed.Count) { $failed[0].What + ': ' + $failed[0].Evidence } elseif ($reason) { $reason } else { 'no checks ran' }
        }
    }
    Write-Host "::endgroup::"
    Write-Host ("{0,-4} {1}. {2} ({3:n0} s) -- {4}" -f $status, $Id, $Title, $clock.Elapsed.TotalSeconds, $evidence)
    if ($status -eq 'FAIL') {
        Write-Host "::error title=Install test $Id. $Title::$($evidence -replace '[\r\n]+', ' ')"
    }
    $Scenarios.Add([pscustomobject]@{
            Id = $Id; Title = $Title; Status = $status; Evidence = $evidence; Error = $reason
            Seconds = [math]::Round($clock.Elapsed.TotalSeconds); Checks = @($script:Checks)
        })
}

# ------------------------------------------------------------------- helpers

function Wait-Until([int]$TimeoutSec, [scriptblock]$Condition, [int]$IntervalMs = 500) {
    $clock = [Diagnostics.Stopwatch]::StartNew()
    while ($true) {
        $ok = try { [bool](& $Condition) } catch { $false }
        if ($ok) { return $true }
        if ($clock.Elapsed.TotalSeconds -ge $TimeoutSec) { return $false }
        Start-Sleep -Milliseconds $IntervalMs
    }
}

# A console program: its exit code and output (stdin closed at once).
function Invoke-Exe([string]$FilePath, [string[]]$Arguments = @(), [hashtable]$Environment = @{}, [int]$TimeoutSec = 60) {
    $psi = [Diagnostics.ProcessStartInfo]::new($FilePath)
    foreach ($a in $Arguments) { $psi.ArgumentList.Add($a) }
    $psi.UseShellExecute = $false
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    foreach ($k in $Environment.Keys) { $psi.Environment[$k] = $Environment[$k] }
    $p = [Diagnostics.Process]::Start($psi)
    $p.StandardInput.Close()
    $out = $p.StandardOutput.ReadToEndAsync()
    $err = $p.StandardError.ReadToEndAsync()
    if (-not $p.WaitForExit($TimeoutSec * 1000)) {
        try { $p.Kill($true) } catch { }
        throw "$FilePath $($Arguments -join ' ') did not end within $TimeoutSec s"
    }
    $p.WaitForExit()
    [pscustomobject]@{ ExitCode = $p.ExitCode; Out = $out.Result.Trim(); Err = $err.Result.Trim() }
}

# A setup or uninstaller: its exit code. Not redirected, so nothing it
# starts holds our pipes.
function Invoke-Setup([string]$Path, [string[]]$Arguments, [int]$TimeoutSec = 600) {
    Write-Host "  run  $([IO.Path]::GetFileName($Path)) $($Arguments -join ' ')"
    $psi = [Diagnostics.ProcessStartInfo]::new($Path)
    foreach ($a in $Arguments) { $psi.ArgumentList.Add($a) }
    $psi.UseShellExecute = $false
    $p = [Diagnostics.Process]::Start($psi)
    if (-not $p.WaitForExit($TimeoutSec * 1000)) {
        try { $p.Kill($true) } catch { }
        throw "$([IO.Path]::GetFileName($Path)) $($Arguments -join ' ') did not end within $TimeoutSec s (a message box waiting?)"
    }
    $p.ExitCode
}

# NSIS's uninstaller copies itself to %TEMP%\~nsu*.tmp and runs from there,
# so the program started ends at once: wait for the copy too.
function Invoke-Uninstall([string[]]$Arguments = @('/S'), [int]$TimeoutSec = 300) {
    if (-not (Test-Path -LiteralPath $UninstallExe)) { throw "$UninstallExe is not there" }
    $code = Invoke-Setup $UninstallExe $Arguments
    $copies = { @(Get-Process | Where-Object { $_.Path -and $_.Path -match '\\~nsu[^\\]*\.tmp\\' }) }
    $done = Wait-Until $TimeoutSec { (& $copies).Count -eq 0 }
    if (-not $done) { throw "the uninstaller's copy in %TEMP% did not end within $TimeoutSec s" }
    $code
}

function Get-FileSha([string]$Path) {
    if (Test-Path -LiteralPath $Path -PathType Leaf) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash } else { 'missing' }
}

function Get-Short([string]$Hash) { if ($Hash.Length -gt 12) { $Hash.Substring(0, 12) } else { $Hash } }

function Write-Text([string]$Path, [string]$Text) {
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Path) | Out-Null
    [IO.File]::WriteAllText($Path, $Text, [Text.UTF8Encoding]::new($false))
}

function Read-Json([string]$Path) {
    if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return $null }
    $text = Get-Content -Raw -LiteralPath $Path
    if (-not $text) { return $null }
    $text | ConvertFrom-Json -AsHashtable -Depth 100
}

function Get-RegValue([string]$Key, [string]$Name) {
    if (-not (Test-Path -LiteralPath $Key)) { return $null }
    $item = Get-Item -LiteralPath $Key
    $item.GetValue($Name, $null, [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
}

# HKCU\Environment\Path as the registry holds it: unexpanded, with its type.
function Get-UserPath {
    $key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment')
    try {
        if (-not $key -or ($key.GetValueNames() -notcontains 'Path')) { return [pscustomobject]@{ Raw = ''; Kind = 'None' } }
        [pscustomobject]@{
            Raw = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
            Kind = $key.GetValueKind('Path').ToString()
        }
    } finally {
        if ($key) { $key.Dispose() }
    }
}

function Set-UserPath([string]$Raw) {
    $key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
    try { $key.SetValue('Path', $Raw, [Microsoft.Win32.RegistryValueKind]::ExpandString) } finally { $key.Dispose() }
}

function Get-PathEntries([string]$Raw) { @($Raw -split ';' | Where-Object { $_ -ne '' }) }

function Test-SameDir([string]$A, [string]$B) {
    $norm = { param($p) ([Environment]::ExpandEnvironmentVariables($p.Trim().Trim('"'))).Replace('/', '\').TrimEnd('\') }
    [string]::Equals((& $norm $A), (& $norm $B), [StringComparison]::OrdinalIgnoreCase)
}

function Get-BinEntries([string]$Raw) { @(Get-PathEntries $Raw | Where-Object { Test-SameDir $_ $Bin }) }

# The PATH a program started from now on gets: the machine's, then the
# user's, expanded (as Windows builds it for a new sign-in or terminal).
function Get-FreshPath {
    [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' + [Environment]::GetEnvironmentVariable('Path', 'User')
}

# A new process looks keymotion-cli up by its name.
function Find-CliOnPath {
    $fresh = @{ PATH = Get-FreshPath }
    $where = Invoke-Exe 'where.exe' @('keymotion-cli') -Environment $fresh
    $first = @($where.Out -split "`r?`n" | Where-Object { $_ })[0] ?? ''
    $version = Invoke-Exe 'cmd.exe' @('/d', '/c', 'keymotion-cli --version') -Environment $fresh
    [pscustomobject]@{ Where = $first; WhereCode = $where.ExitCode; Version = $version.Out; VersionCode = $version.ExitCode }
}

function Test-CliVersion([string]$Line) { $Line -like "keymotion-cli $Version (*" }

function Get-CliVersion([string]$Exe) {
    $r = Invoke-Exe $Exe @('--version')
    if ($r.ExitCode -ne 0) { return "exit $($r.ExitCode): $($r.Err)" }
    $r.Out
}

function Get-AppProcesses { @(Get-Process -Name keymotion -ErrorAction SilentlyContinue) }

# Keymotion closed, and the WebView2 processes it started with it.
function Stop-App {
    $procs = Get-AppProcesses
    if ($procs.Count) {
        $procs | Stop-Process -Force -ErrorAction SilentlyContinue
        Wait-Until 20 { (Get-AppProcesses).Count -eq 0 } | Out-Null
    }
    $webviews = { @(Get-CimInstance Win32_Process -Filter "Name = 'msedgewebview2.exe'" | Where-Object { $_.CommandLine -match 'dev\.keymotion' }) }
    if (-not (Wait-Until 15 { (& $webviews).Count -eq 0 })) {
        foreach ($w in & $webviews) { Stop-Process -Id $w.ProcessId -Force -ErrorAction SilentlyContinue }
    }
}

function Get-VisibleWindows([int]$ProcessId) { @([KmNative]::VisibleWindows($ProcessId)) }

function Get-SchemeCommand { Get-RegValue $SchemeCommandKey '' }

function Get-InstalledVersion { Get-RegValue $UninstallKey 'DisplayVersion' }

# ------------------------------------------------------- the app, over CDP

# Runtime.evaluate in Keymotion's page (WebView2's DevTools protocol).
function Invoke-Cdp([string]$Expression, [int]$TimeoutSec = 60) {
    $targets = Invoke-RestMethod -Uri "http://127.0.0.1:$CdpPort/json/list" -TimeoutSec 5
    $page = @($targets | Where-Object { $_.type -eq 'page' -and $_.url -match 'tauri\.localhost' }) | Select-Object -First 1
    if (-not $page) { throw "no Keymotion page among WebView2's targets: $(@($targets | ForEach-Object { $_.url }) -join ', ')" }
    $ws = [System.Net.WebSockets.ClientWebSocket]::new()
    $cts = [System.Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds($TimeoutSec))
    try {
        $ws.ConnectAsync([Uri]$page.webSocketDebuggerUrl, $cts.Token).GetAwaiter().GetResult()
        $request = @{ id = 1; method = 'Runtime.evaluate'; params = @{ expression = $Expression; awaitPromise = $true; returnByValue = $true } } |
            ConvertTo-Json -Depth 5 -Compress
        $bytes = [Text.Encoding]::UTF8.GetBytes($request)
        $ws.SendAsync([ArraySegment[byte]]::new($bytes), [System.Net.WebSockets.WebSocketMessageType]::Text, $true, $cts.Token).GetAwaiter().GetResult()
        $buffer = [byte[]]::new(65536)
        while ($true) {
            $stream = [IO.MemoryStream]::new()
            do {
                $got = $ws.ReceiveAsync([ArraySegment[byte]]::new($buffer), $cts.Token).GetAwaiter().GetResult()
                $stream.Write($buffer, 0, $got.Count)
            } until ($got.EndOfMessage)
            $message = [Text.Encoding]::UTF8.GetString($stream.ToArray()) | ConvertFrom-Json -AsHashtable -Depth 100
            if ($message.Contains('id') -and $message['id'] -eq 1) { break }
        }
    } finally {
        $ws.Dispose()
        $cts.Dispose()
    }
    if ($message.Contains('error')) { throw "CDP: $($message['error'] | ConvertTo-Json -Compress)" }
    $result = $message['result']
    if ($result.Contains('exceptionDetails')) { throw "the page threw: $($result['exceptionDetails'] | ConvertTo-Json -Compress -Depth 5)" }
    $result['result']['value']
}

# One of the app's Tauri commands, as its pages call them.
function Invoke-AppCommand([string]$Command, [hashtable]$Arguments = @{}) {
    $json = $Arguments | ConvertTo-Json -Depth 20 -Compress
    $expression = "(async () => { try { return JSON.stringify({ ok: true, value: await window.__TAURI_INTERNALS__.invoke('$Command', $json) }); } catch (e) { return JSON.stringify({ ok: false, error: String((e && e.message) || e) }); } })()"
    $answer = (Invoke-Cdp $expression) | ConvertFrom-Json -AsHashtable -Depth 100
    if (-not $answer['ok']) { throw "$Command failed in the app: $($answer['error'])" }
    $answer['value']
}

# Keymotion started as a user starts it, with WebView2's DevTools port open
# for this test.
function Start-AppWithCdp {
    Stop-App
    $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=$CdpPort"
    try {
        $p = Start-Process -FilePath $AppExe -PassThru
    } finally {
        Remove-Item Env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS -ErrorAction SilentlyContinue
    }
    $ready = Wait-Until 90 { (Invoke-Cdp 'typeof window.__TAURI_INTERNALS__?.invoke' -TimeoutSec 5) -eq 'function' } 1000
    if (-not $ready) {
        $alive = if ($p.HasExited) { "it ended with exit code $($p.ExitCode)" } else { 'it runs' }
        throw "Keymotion's page did not answer over WebView2's DevTools port within 90 s ($alive)"
    }
    $p
}

# Agent access on, as the AI agents page turns it on: the plan, then the
# user's approval of that plan.
function Enable-AgentAccess {
    $plan = Invoke-AppCommand 'agents_connect_plan'
    $outcome = Invoke-AppCommand 'agents_connect' @{ token = $plan['token'] }
    [pscustomobject]@{ Plan = $plan; Outcome = $outcome }
}

# ------------------------------------------------- the coding agents' files

$ClaudeSeed = @'
{
  "$schema": "https://json.schemastore.org/claude-code-settings.json",
  "model": "opus",
  "permissions": {
    "allow": ["Bash(git status)", "Read(~/notes/**)"],
    "deny": ["Bash(rm -rf:*)"]
  },
  "env": {
    "INSTALL_TEST_MARKER": "keep me"
  },
  "hooks": {
    "UserPromptSubmit": [
      { "hooks": [{ "type": "command", "command": "echo user-prompt-hook" }] }
    ],
    "PreToolUse": [
      { "matcher": "Bash", "hooks": [{ "type": "command", "command": "user-bash-guard.cmd", "timeout": 5 }] }
    ],
    "Stop": [
      { "hooks": [{ "type": "command", "command": "echo user-stop-hook" }] }
    ]
  },
  "statusLine": { "type": "command", "command": "echo user-status-line" }
}
'@

$CodexConfigSeed = @'
# Codex settings of the user's own (Keymotion's install test).
model = "gpt-5-codex"  # the model I use
approval_policy = "on-request"

[features]
# hooks on
codex_hooks = true

[projects.'C:\work\demo']
trust_level = "trusted"
'@

$CodexHooksSeed = @'
{
  "description": "My own Codex hooks",
  "hooks": {
    "Stop": [
      { "hooks": [{ "type": "command", "command": "Write-Output user-codex-stop", "timeout": 5 }] }
    ]
  }
}
'@

$CodexRuleSeed = @'
# A rule of the user's own.
prefix_rule(pattern = ["git", "status"], decision = "allow")
'@

$UserSkillSeed = @'
---
name: user-notes
description: A skill of the user's own; Keymotion leaves it alone.
---

Notes.
'@

function Write-AgentSeeds {
    Write-Text $Claude.Settings $ClaudeSeed
    Write-Text $Claude.UserSkill $UserSkillSeed
    Write-Text $Codex.Config $CodexConfigSeed
    Write-Text $Codex.Hooks $CodexHooksSeed
    Write-Text $Codex.UserRule $CodexRuleSeed
    Write-Text $Codex.UserSkill $UserSkillSeed.Replace('user-notes', 'user-helper')
}

function Test-KeymotionHandler($Handler) { ([string]$Handler['command']) -match 'keymotion-cli' }

# Keymotion's hook handlers in an agent's hooks file, whichever copy.
function Get-KeymotionHandlers([string]$File) {
    $root = Read-Json $File
    $out = [System.Collections.Generic.List[object]]::new()
    if ($null -eq $root -or -not $root.Contains('hooks')) { return @() }
    foreach ($event in $root['hooks'].Keys) {
        foreach ($group in @($root['hooks'][$event])) {
            $matcher = if ($group.Contains('matcher')) { [string]$group['matcher'] } else { '' }
            foreach ($h in @($group['hooks'])) {
                if (-not (Test-KeymotionHandler $h)) { continue }
                $handlerArgs = if ($h.Contains('args')) { @($h['args']) -join ' ' } else { '' }
                $out.Add([pscustomobject]@{ Event = $event; Matcher = $matcher; Command = [string]$h['command']; Args = $handlerArgs })
            }
        }
    }
    @($out)
}

# A hook handler runs the keymotion-cli at $Exe.
function Test-RunsCopy($Handler, [string]$Agent, [string]$Exe) {
    if ($Agent -eq 'claude-code') {
        [string]::Equals($Handler.Command, $Exe, [StringComparison]::OrdinalIgnoreCase) -and $Handler.Args -eq 'hook claude-code'
    } else {
        $Handler.Command.IndexOf("'$Exe'", [StringComparison]::OrdinalIgnoreCase) -ge 0 -and $Handler.Command.TrimEnd().EndsWith('hook codex }')
    }
}

# Our hooks in a file: how many, how many run $Exe, events with two.
function Get-HookFacts([string]$File, [string]$Agent, [string]$Exe) {
    $all = @(Get-KeymotionHandlers $File)
    $mine = @($all | Where-Object { Test-RunsCopy $_ $Agent $Exe })
    $doubled = @($all | Group-Object Event | Where-Object Count -gt 1 | ForEach-Object Name)
    $others = @($all | Where-Object { -not (Test-RunsCopy $_ $Agent $Exe) } | ForEach-Object Command | Select-Object -Unique)
    [pscustomobject]@{ Total = $all.Count; Mine = $mine.Count; Doubled = $doubled; Others = $others }
}

# Everything in a hooks file that is not Keymotion's, in order: the keys
# and each handler of the user's, with its event and matcher.
function Get-UserView([string]$File) {
    $root = Read-Json $File
    if ($null -eq $root) { return '<missing>' }
    $lines = [System.Collections.Generic.List[string]]::new()
    foreach ($k in $root.Keys) {
        if ($k -ceq 'hooks') { continue }
        $lines.Add("key $k = " + (ConvertTo-Json -InputObject $root[$k] -Compress -Depth 64))
    }
    if ($root.Contains('hooks')) {
        foreach ($event in $root['hooks'].Keys) {
            foreach ($group in @($root['hooks'][$event])) {
                $matcher = if ($group.Contains('matcher')) { [string]$group['matcher'] } else { '' }
                foreach ($h in @($group['hooks'])) {
                    if (Test-KeymotionHandler $h) { continue }
                    $lines.Add("hook $event [$matcher] " + (ConvertTo-Json -InputObject $h -Compress -Depth 64))
                }
            }
        }
    }
    $lines -join "`n"
}

function Get-EventNames([string]$File) {
    $root = Read-Json $File
    if ($null -eq $root -or -not $root.Contains('hooks')) { return '' }
    @($root['hooks'].Keys) -join ','
}

function Test-FileNames([string]$File, [string]$Text) {
    (Test-Path -LiteralPath $File -PathType Leaf) -and (Get-Content -Raw -LiteralPath $File).IndexOf($Text, [StringComparison]::OrdinalIgnoreCase) -ge 0
}

# The Codex rule names keymotion-cli as a Starlark string.
function Get-RuleText([string]$Exe) { $Exe.Replace('\', '\\') }

# keymotion-cli connections, as this copy sees them.
function Get-Connections {
    $r = Invoke-Exe $CliExe @('connections', '--json')
    $json = $r.Out | ConvertFrom-Json -AsHashtable -Depth 50
    $by = @{}
    foreach ($a in @($json['data']['agents'])) { $by[$a['agent']] = $a }
    $by
}

function Format-Connection($Status) {
    if ($null -eq $Status) { return 'not reported' }
    "$($Status['state']), $($Status['hooks']) of $($Status['hooks_wanted']) hooks, runs $($Status['runs'])"
}

# The files that make a connection: hashed, to tell that nothing changed.
function Get-ConnectionSnapshot {
    $files = @($Claude.Settings, (Join-Path $Claude.SkillDir 'SKILL.md'), $Codex.Hooks, (Join-Path $Codex.SkillDir 'SKILL.md'), $Codex.Rule, $Codex.Config)
    ($files | ForEach-Object { "$([IO.Path]::GetFileName($_))=$(Get-Short (Get-FileSha $_))" }) -join ' '
}

# Copies of what the scenarios looked at, for the job's artifact.
function Save-State([string]$Label) {
    try {
        $dest = Join-Path $OutDir "state-$Label"
        New-Item -ItemType Directory -Force -Path $dest | Out-Null
        foreach ($dir in @($Claude.Dir, $Codex.Dir, (Join-Path $HomeDir '.agents'))) {
            if (Test-Path -LiteralPath $dir) { Copy-Item -Recurse -Force -LiteralPath $dir -Destination (Join-Path $dest ([IO.Path]::GetFileName($dir))) }
        }
        $facts = [ordered]@{
            path = Get-UserPath
            pathRecord = Get-RegValue $UserPathRecordKey 'PathAdded'
            scheme = Get-SchemeCommand
            installed = Get-InstalledVersion
            files = if (Test-Path -LiteralPath $InstallDir) { @(Get-ChildItem -Recurse -File -LiteralPath $InstallDir | ForEach-Object { $_.FullName }) } else { @() }
        }
        $facts | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $dest 'facts.json')
    } catch {
        Write-Host "  (state $Label not saved: $($_.Exception.Message))"
    }
}

# ---------------------------------------------------------------- the runner

Write-Host "Keymotion $Version install test"
Write-Host "  installer: $Installer ($(Get-Short (Get-FileSha $Installer)))"
Write-Host "  previous:  $(if ($PreviousInstaller) { $PreviousInstaller } else { '(none)' })"
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$elevated = ([Security.Principal.WindowsPrincipal]$identity).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
$webview2 = Get-RegValue 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}' 'pv'
$explorer = @(Get-Process -Name explorer -ErrorAction SilentlyContinue).Count
$Session = (Get-Process -Id $PID).SessionId
$RunnerFacts = "Windows $([Environment]::OSVersion.Version), user $($identity.Name), session $Session, " +
"administrator $elevated, explorer.exe running $($explorer -gt 0), shell window $([KmNative]::GetShellWindow() -ne [IntPtr]::Zero), WebView2 $webview2"
Write-Host "  runner:    $RunnerFacts"

# A clean start: no Keymotion, no coding agents' folders, no agent folder
# variables; the user's PATH with an entry of their own that must stay
# unexpanded.
if (Test-Path -LiteralPath $InstallDir) { throw "Keymotion is installed on this runner already ($InstallDir): not a clean machine" }
foreach ($dir in @($Claude.Dir, $Codex.Dir, (Join-Path $HomeDir '.agents'))) {
    if (Test-Path -LiteralPath $dir) {
        $aside = "$dir.before-install-test"
        Write-Host "  moving $dir aside to $aside"
        Move-Item -LiteralPath $dir -Destination $aside
    }
}
Remove-Item Env:CLAUDE_CONFIG_DIR, Env:CODEX_HOME -ErrorAction SilentlyContinue
$original = Get-UserPath
$base = $original.Raw.TrimEnd(';')
Set-UserPath ($(if ($base) { "$base;$SeedPathEntry" } else { $SeedPathEntry }))
$PathSeed = Get-UserPath
Write-Host "  user PATH: $($PathSeed.Kind) $($PathSeed.Raw)"

$script:AppHash = $null
$script:Wanted = @{ 'claude-code' = 0; codex = 0 }
$script:NormalStartWindows = $null
# What b found the user's own content to be, for the later scenarios.
$script:ClaudeUser = $null
$script:ClaudeSeedEvents = 'UserPromptSubmit,PreToolUse,Stop'
$script:CodexUser = $null
$script:CodexSeedEvents = 'Stop'
$script:ConfigSha = $null
$script:UserRuleSha = $null
$script:UserSkillShas = $null

# ---------------------------------------------------------------- scenarios

Invoke-Scenario 'a' 'Fresh silent install' {
    $before = Get-UserPath
    $code = Invoke-Setup $Installer @('/S')
    Confirm-That ($code -eq 0) 'the setup ends with 0' "exit code $code"
    foreach ($f in @($AppExe, $UninstallExe, $CliExe)) {
        Confirm-That (Test-Path -LiteralPath $f -PathType Leaf) "$([IO.Path]::GetRelativePath($InstallDir, $f)) is installed" $f
    }
    $script:AppHash = Get-FileSha $AppExe
    $display = Get-InstalledVersion
    $uninstallString = Get-RegValue $UninstallKey 'UninstallString'
    Confirm-That ($display -eq $Version -and "$uninstallString" -like "*uninstall.exe*") 'the uninstall entry is registered' "DisplayVersion $display, UninstallString $uninstallString"
    $record = Get-RegValue $RecordKey ''
    Confirm-That ([string]::Equals("$record", $InstallDir, [StringComparison]::OrdinalIgnoreCase)) 'the installation record names the install folder' "$RecordKey = $record"
    $cliVersion = Get-CliVersion $CliExe
    Confirm-That (Test-CliVersion $cliVersion) 'bin\keymotion-cli.exe says this version' $cliVersion
    $after = Get-UserPath
    Confirm-That ($after.Raw -ceq $before.Raw -and $after.Kind -eq $before.Kind) 'installing leaves PATH alone (Keymotion adds bin when agent access is turned on)' "$($after.Kind), bin entries: $((Get-BinEntries $after.Raw).Count)"
    $scheme = Get-SchemeCommand
    Add-Note "keymotion: scheme right after installing: $(if ($scheme) { $scheme } else { 'not registered (the app registers it at its first start)' })"

    # The user opens Keymotion and turns agent access on (AI agents page).
    $app = Start-AppWithCdp
    $scheme = Get-SchemeCommand
    Confirm-That ("$scheme" -like "*$AppExe*") 'the keymotion: URL scheme opens the installed app (registered at its first start)' "$scheme"
    $aumid = Test-Path -LiteralPath $AumidKey
    Add-Note "notifications' AppUserModelId registered: $aumid"
    $access = Enable-AgentAccess
    $planDir = if ($access.Plan['path']) { $access.Plan['path']['dir'] } else { '(none)' }
    Confirm-That (Test-SameDir "$planDir" $Bin) 'the AI agents page offers the bin folder for PATH' "plan: $planDir; coding apps not found: $(@($access.Plan['notFound']) -join ', ')"
    Confirm-That ($access.Outcome['applied'] -and $access.Outcome['pathAdded']) 'turning agent access on puts bin on PATH' "applied $($access.Outcome['applied']), pathAdded $($access.Outcome['pathAdded']), pathError $($access.Outcome['pathError'])"
    Stop-App

    $after = Get-UserPath
    $ours = @(Get-BinEntries $after.Raw)
    Confirm-That ($ours.Count -eq 1) 'HKCU\Environment Path has the bin folder once' "$($ours.Count) entry: $($ours -join ', ')"
    Confirm-That ($after.Kind -eq 'ExpandString') 'Path keeps its REG_EXPAND_SZ type' $after.Kind
    Confirm-That ((Get-PathEntries $after.Raw) -ccontains $SeedPathEntry) "the user's $SeedPathEntry stays unexpanded" ''
    $prefix = "$($before.Raw);"
    $atEnd = $after.Raw.StartsWith($prefix, [StringComparison]::Ordinal) -and (Test-SameDir $after.Raw.Substring([math]::Min($prefix.Length, $after.Raw.Length)) $Bin)
    Confirm-That $atEnd "bin goes at the end; the user's entries stay as they were" $after.Raw
    $added = Get-RegValue $UserPathRecordKey 'PathAdded'
    Confirm-That (Test-SameDir "$added" $Bin) 'Keymotion records that it added the folder' "PathAdded = $added"
    $found = Find-CliOnPath
    Confirm-That ([string]::Equals($found.Where, $CliExe, [StringComparison]::OrdinalIgnoreCase)) 'a new process finds keymotion-cli on PATH' "where: $($found.Where)"
    Confirm-That (Test-CliVersion $found.Version) 'keymotion-cli --version by its name says this version' $found.Version
    $script:Headline = "installed $display; app made bin the last PATH entry (REG_EXPAND_SZ kept); new process: $($found.Version)"
    Save-State 'a'
}

Invoke-Scenario 'b' "Connect beside the user's settings and another copy's" {
    if (-not (Test-Path -LiteralPath $CliExe)) { throw 'keymotion-cli is not installed (a failed)' }
    Write-AgentSeeds
    $claudeUser = Get-UserView $Claude.Settings
    $codexUser = Get-UserView $Codex.Hooks
    $configSha = Get-FileSha $Codex.Config
    $ruleSha = Get-FileSha $Codex.UserRule
    $skillShas = "$(Get-FileSha $Claude.UserSkill) $(Get-FileSha $Codex.UserSkill)"

    # The Microsoft Store copy connected Claude Code first: its hooks, skill
    # and rule name its app execution alias. (A copy of this keymotion-cli at
    # the alias's path writes exactly those entries.)
    New-Item -ItemType Directory -Force -Path (Split-Path -Parent $Alias) | Out-Null
    Copy-Item -LiteralPath $CliExe -Destination $Alias -Force
    $r = Invoke-Exe $Alias @('connect', 'claude-code', '--yes')
    Confirm-That ($r.ExitCode -eq 0) 'the other copy (Store alias) connects Claude Code' "exit $($r.ExitCode) $($r.Err)"
    $facts = Get-HookFacts $Claude.Settings 'claude-code' $Alias
    Add-Note "Claude Code before: $($facts.Mine) hooks run $Alias"

    # This copy connects Claude Code: it takes the other copy's hooks over
    # (one Keymotion hook per event, kb-hooks merge_hooks).
    $r = Invoke-Exe $CliExe @('connect', 'claude-code', '--yes')
    Confirm-That ($r.ExitCode -eq 0) 'keymotion-cli connect claude-code --yes' "exit $($r.ExitCode) $($r.Err)"
    $conn = Get-Connections
    $script:Wanted['claude-code'] = [int]$conn['claude-code']['hooks_wanted']
    $facts = Get-HookFacts $Claude.Settings 'claude-code' $CliExe
    Confirm-That ($facts.Total -eq $script:Wanted['claude-code'] -and $facts.Mine -eq $facts.Total -and -not $facts.Doubled.Count) 'Claude Code: one Keymotion hook per event, each running this copy' "$($facts.Mine) of $($facts.Total) run bin (wanted $($script:Wanted['claude-code'])), doubled: $($facts.Doubled -join ',')"
    Confirm-That ($facts.Others.Count -eq 0) "Claude Code: the other copy's hooks were replaced, not doubled" "others left: $($facts.Others -join ', ')"
    Confirm-That ((Get-UserView $Claude.Settings) -ceq $claudeUser) "Claude Code: the user's keys and hooks are as they were, in order" ''
    Confirm-That (Test-FileNames (Join-Path $Claude.SkillDir 'SKILL.md') $CliExe) 'Claude Code: the keymotion skill names this copy' (Join-Path $Claude.SkillDir 'SKILL.md')
    Confirm-That (Test-Path -LiteralPath "$($Claude.Settings).before-keymotion") 'Claude Code: a copy of settings.json as it was is kept' "$($Claude.Settings).before-keymotion"
    Confirm-That ($conn['claude-code']['state'] -eq 'connected') 'keymotion-cli connections: Claude Code connected' (Format-Connection $conn['claude-code'])

    # This copy connects Codex.
    $r = Invoke-Exe $CliExe @('connect', 'codex', '--yes')
    Confirm-That ($r.ExitCode -eq 0) 'keymotion-cli connect codex --yes' "exit $($r.ExitCode) $($r.Err)"
    $conn = Get-Connections
    $script:Wanted['codex'] = [int]$conn['codex']['hooks_wanted']
    $facts = Get-HookFacts $Codex.Hooks 'codex' $CliExe
    Confirm-That ($facts.Total -eq $script:Wanted['codex'] -and $facts.Mine -eq $facts.Total -and -not $facts.Doubled.Count) 'Codex: one Keymotion hook per event, each running this copy' "$($facts.Mine) of $($facts.Total) run bin (wanted $($script:Wanted['codex']))"
    Confirm-That ((Get-UserView $Codex.Hooks) -ceq $codexUser) "Codex: the user's hooks.json entries are as they were" ''
    Confirm-That ((Get-FileSha $Codex.Config) -eq $configSha) 'Codex: config.toml (with its comments) is untouched' (Get-Short $configSha)
    Confirm-That ((Get-FileSha $Codex.UserRule) -eq $ruleSha) "Codex: the user's own rules file is untouched" $Codex.UserRule
    Confirm-That (Test-FileNames $Codex.Rule (Get-RuleText $CliExe)) 'Codex: the keymotion rule names this copy' $Codex.Rule
    Confirm-That (Test-FileNames (Join-Path $Codex.SkillDir 'SKILL.md') $CliExe) 'Codex: the keymotion skill (~/.agents) names this copy' (Join-Path $Codex.SkillDir 'SKILL.md')
    Confirm-That ($conn['codex']['state'] -in @('connected', 'needs_approval')) 'keymotion-cli connections: Codex connected (waiting for trust in Codex)' (Format-Connection $conn['codex'])

    # Then the Store copy connects Codex again: from now on Codex's
    # Keymotion entries are the other copy's, which uninstalling this copy
    # must leave alone (--if-this-copy).
    $r = Invoke-Exe $Alias @('connect', 'codex', '--yes')
    Confirm-That ($r.ExitCode -eq 0) 'the other copy (Store alias) connects Codex' "exit $($r.ExitCode) $($r.Err)"
    Remove-Item -LiteralPath $Alias -Force
    $facts = Get-HookFacts $Codex.Hooks 'codex' $Alias
    Confirm-That ($facts.Total -eq $script:Wanted['codex'] -and $facts.Mine -eq $facts.Total) "Codex: every Keymotion hook runs the other copy now" "$($facts.Mine) of $($facts.Total) run $Alias"
    Confirm-That ((Test-FileNames (Join-Path $Codex.SkillDir 'SKILL.md') $Alias) -and (Test-FileNames $Codex.Rule (Get-RuleText $Alias))) "Codex: the skill and the rule name the other copy" ''
    $conn = Get-Connections
    Confirm-That ($conn['codex']['state'] -eq 'outdated') 'keymotion-cli connections: Codex runs another copy' (Format-Connection $conn['codex'])
    Confirm-That ((Get-UserView $Codex.Hooks) -ceq $codexUser -and (Get-FileSha $Codex.Config) -eq $configSha) "Codex: the user's entries and config.toml still as they were" ''
    Confirm-That ("$(Get-FileSha $Claude.UserSkill) $(Get-FileSha $Codex.UserSkill)" -eq $skillShas) "the user's own skills are untouched" ''

    $script:ClaudeUser = $claudeUser
    $script:CodexUser = $codexUser
    $script:ConfigSha = $configSha
    $script:UserRuleSha = $ruleSha
    $script:UserSkillShas = $skillShas
    $script:Headline = "Claude Code: $($script:Wanted['claude-code']) hooks run bin (took the Store copy's over); Codex: $($script:Wanted['codex']) hooks, then handed to the Store copy; user's JSON keys/hooks, config.toml, rules, skills unchanged"
    Save-State 'b'
}

Invoke-Scenario 'c' 'Silent uninstall' {
    if (-not (Test-Path -LiteralPath $UninstallExe)) { throw 'Keymotion is not installed' }
    $code = Invoke-Uninstall @('/S')
    Confirm-That ($code -eq 0) 'uninstall.exe /S ends with 0' "exit code $code"
    Confirm-That (-not (Test-Path -LiteralPath $InstallDir)) 'the install folder is gone' $(if (Test-Path -LiteralPath $InstallDir) { (Get-ChildItem -Recurse -Name -LiteralPath $InstallDir) -join ', ' } else { $InstallDir })
    Confirm-That (-not (Test-Path -LiteralPath $UninstallKey)) 'the uninstall entry is gone' $UninstallKey
    $path = Get-UserPath
    Confirm-That ((Get-BinEntries $path.Raw).Count -eq 0) 'bin is off PATH' $path.Raw
    Confirm-That ($path.Raw -ceq $PathSeed.Raw -and $path.Kind -eq $PathSeed.Kind) "PATH is exactly the user's again (type and entries)" "$($path.Kind) $($path.Raw)"
    Confirm-That ($null -eq (Get-RegValue $UserPathRecordKey 'PathAdded') -and -not (Test-Path -LiteralPath $UserPathRecordKey)) "Keymotion's PATH record is gone" $UserPathRecordKey
    Confirm-That (-not (Test-Path -LiteralPath $SchemeKey) -and -not (Test-Path -LiteralPath $AumidKey)) 'the keymotion: scheme and the AppUserModelId are gone' ''

    $facts = Get-HookFacts $Claude.Settings 'claude-code' $CliExe
    Confirm-That ($facts.Total -eq 0) "Claude Code: Keymotion's hooks are gone" "$($facts.Total) left"
    Confirm-That (-not (Test-Path -LiteralPath $Claude.SkillDir)) 'Claude Code: the keymotion skill folder is gone' $Claude.SkillDir
    Confirm-That ((Get-UserView $Claude.Settings) -ceq $script:ClaudeUser) "Claude Code: the user's keys and hooks are as they were" ''
    $events = Get-EventNames $Claude.Settings
    Confirm-That ($events -eq $script:ClaudeSeedEvents) 'Claude Code: no empty hook events left behind' $events

    $facts = Get-HookFacts $Codex.Hooks 'codex' $Alias
    Confirm-That ($facts.Total -eq $script:Wanted['codex'] -and $facts.Mine -eq $facts.Total) "Codex: the other copy's hooks stay" "$($facts.Mine) of $($facts.Total) run $Alias"
    Confirm-That ((Test-FileNames (Join-Path $Codex.SkillDir 'SKILL.md') $Alias) -and (Test-FileNames $Codex.Rule (Get-RuleText $Alias))) "Codex: the other copy's skill and rule stay" ''
    Confirm-That ((Get-UserView $Codex.Hooks) -ceq $script:CodexUser) "Codex: the user's hooks.json entries are as they were" ''
    Confirm-That ((Get-FileSha $Codex.Config) -eq $script:ConfigSha -and (Get-FileSha $Codex.UserRule) -eq $script:UserRuleSha) "Codex: config.toml and the user's rules file are untouched" ''
    Confirm-That ("$(Get-FileSha $Claude.UserSkill) $(Get-FileSha $Codex.UserSkill)" -eq $script:UserSkillShas) "the user's own skills are untouched" ''
    $script:Headline = "files, uninstall entry, PATH entry + record, scheme, Claude Code hooks/skill gone; Store copy's Codex hooks/skill/rule and all user content kept"
    Save-State 'c'
}

Invoke-Scenario 'd' 'Manual reinstall keeps the connections and PATH' {
    $code = Invoke-Setup $Installer @('/S')
    Confirm-That ($code -eq 0) 'installed again (setup /S)' "exit code $code"
    Start-AppWithCdp | Out-Null
    $access = Enable-AgentAccess
    $connected = @($access.Outcome['connected'])
    Confirm-That ($connected.Count -eq 2 -and $access.Outcome['pathAdded']) 'the app connects Claude Code and Codex and puts bin on PATH' "connected: $($connected -join ', '); failed: $(@($access.Outcome['failed']).Count); pathAdded $($access.Outcome['pathAdded'])"
    Stop-App
    $conn = Get-Connections
    Confirm-That ($conn['claude-code']['state'] -eq 'connected' -and ([string]$conn['codex']['runs']).IndexOf("'$CliExe'", [StringComparison]::OrdinalIgnoreCase) -ge 0) 'both run this copy now' "Claude Code $(Format-Connection $conn['claude-code']); Codex $(Format-Connection $conn['codex'])"
    $snapshot = Get-ConnectionSnapshot
    $path = Get-UserPath
    $scheme = Get-SchemeCommand
    Confirm-That ($null -ne $scheme) 'the app registered the keymotion: scheme again' "$scheme"

    # The same setup run by hand over the installed copy (not an update): in
    # passive mode Tauri's "already installed" step runs the old
    # uninstaller first, without /UPDATE, for the same version.
    $code = Invoke-Setup $Installer @('/P')
    Confirm-That ($code -eq 0) 'setup /P over the installed copy ends with 0' "exit code $code"
    Confirm-That (-not (Test-Path -LiteralPath $SchemeKey)) 'the old uninstaller ran (its cleanup removed the keymotion: scheme)' $(if (Test-Path -LiteralPath $SchemeKey) { 'scheme still registered: the old uninstaller did not run, so KEYMOTION_SETUP_REPLACES was not exercised' } else { 'scheme removed' })
    Confirm-That ((Get-InstalledVersion) -eq $Version -and (Test-Path -LiteralPath $CliExe)) 'Keymotion is installed afterwards' "DisplayVersion $(Get-InstalledVersion)"
    $after = Get-ConnectionSnapshot
    Confirm-That ($after -eq $snapshot) 'the Claude Code and Codex files are as they were' $after
    $conn = Get-Connections
    Confirm-That ($conn['claude-code']['state'] -eq 'connected' -and $conn['codex']['state'] -in @('connected', 'needs_approval')) 'keymotion-cli connections: both still connected' "Claude Code $($conn['claude-code']['state']); Codex $($conn['codex']['state'])"
    $pathAfter = Get-UserPath
    Confirm-That ($pathAfter.Raw -ceq $path.Raw -and $pathAfter.Kind -eq $path.Kind -and (Get-BinEntries $pathAfter.Raw).Count -eq 1) 'PATH is as it was, bin on it' "$($pathAfter.Kind) $($pathAfter.Raw)"
    Confirm-That (Test-SameDir "$(Get-RegValue $UserPathRecordKey 'PathAdded')" $Bin) "Keymotion's PATH record stays (a later uninstall still takes bin off)" "PathAdded = $(Get-RegValue $UserPathRecordKey 'PathAdded')"
    $run = Get-RegValue 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run' 'Keymotion'
    Add-Note "start-with-Windows Run value after the reinstall: $(if ($run) { $run } else { 'none' })"
    $script:Headline = "setup /P ran the old uninstaller (scheme removed) yet hooks, skills, rule and PATH ($((Get-BinEntries $pathAfter.Raw).Count) bin entry, record kept) are unchanged"
    Save-State 'd'
}

Invoke-Scenario 'e' 'Update while a keymotion-cli runs' {
    if (-not (Test-Path -LiteralPath $CliExe)) { throw 'keymotion-cli is not installed (d failed)' }
    Stop-App
    $snapshot = Get-ConnectionSnapshot
    $path = Get-UserPath

    # A hook waiting on its agent's event: keymotion-cli in hook mode, its
    # stdin open, held (suspended) so it keeps running from its file.
    $psi = [Diagnostics.ProcessStartInfo]::new($CliExe)
    $psi.ArgumentList.Add('hook')
    $psi.ArgumentList.Add('claude-code')
    $psi.UseShellExecute = $false
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $holder = [Diagnostics.Process]::Start($psi)
    $suspended = [KmNative]::NtSuspendProcess($holder.Handle)
    Confirm-That ($suspended -eq 0 -and -not $holder.HasExited) 'a keymotion-cli runs from bin (hook mode, waiting on its stdin)' "pid $($holder.Id), $CliExe hook claude-code"
    $locked = $false
    $lockMessage = ''
    try {
        $fs = [IO.File]::Open($CliExe, [IO.FileMode]::Open, [IO.FileAccess]::ReadWrite, [IO.FileShare]::ReadWrite)
        $fs.Dispose()
    } catch {
        $locked = $true
        $lockMessage = $_.Exception.Message
    }
    Confirm-That $locked 'bin\keymotion-cli.exe cannot be written while it runs' $lockMessage

    try {
        $code = Invoke-Setup $Installer $UpdaterArgs
        Confirm-That ($code -eq 0) "the setup with the updater's arguments ends with 0" "$($UpdaterArgs -join ' '): exit code $code"
        $old = @(Get-ChildItem -LiteralPath $Bin -Filter 'keymotion-cli.old*.exe' -ErrorAction SilentlyContinue)
        Confirm-That ($old.Count -eq 1 -and $old[0].Name -match '^keymotion-cli\.old-\d+\.exe$') 'the running keymotion-cli was renamed to keymotion-cli.old-<n>.exe' "$(@($old | ForEach-Object Name) -join ', ')"
        $version = Get-CliVersion $CliExe
        Confirm-That (Test-CliVersion $version) 'the new bin\keymotion-cli.exe is in place and runs' $version
        Confirm-That (-not $holder.HasExited) 'the running copy was not stopped' "pid $($holder.Id) alive"
        Confirm-That ((Get-ConnectionSnapshot) -eq $snapshot) 'the update leaves the Claude Code and Codex files alone' ''
        $pathAfter = Get-UserPath
        Confirm-That ($pathAfter.Raw -ceq $path.Raw -and $pathAfter.Kind -eq $path.Kind) 'the update leaves PATH alone' $pathAfter.Kind
        Confirm-That ((Get-InstalledVersion) -eq $Version) 'the uninstall entry says this version' (Get-InstalledVersion)
        $restarted = Wait-Until 45 { (Get-AppProcesses).Count -gt 0 }
        Add-Note "/R started Keymotion after the update: $restarted"
    } finally {
        [KmNative]::NtResumeProcess($holder.Handle) | Out-Null
        try { $holder.StandardInput.Close() } catch { }
        if (-not $holder.WaitForExit(15000)) { $holder.Kill() }
    }
    Confirm-That ($holder.ExitCode -eq 0) 'the renamed copy runs on and ends with 0' "exit code $($holder.ExitCode)"
    Stop-App
    $old = @(Get-ChildItem -LiteralPath $Bin -Filter 'keymotion-cli.old*.exe' -ErrorAction SilentlyContinue)
    Confirm-That ($old.Count -eq 1) 'the renamed file stays once it has ended (no cleanup until the next setup or uninstall)' "$(@($old | ForEach-Object Name) -join ', ')"
    $code = Invoke-Setup $Installer @('/P', '/UPDATE')
    $old = @(Get-ChildItem -LiteralPath $Bin -Filter 'keymotion-cli.old*.exe' -ErrorAction SilentlyContinue)
    Confirm-That ($code -eq 0 -and $old.Count -eq 0) 'the next setup run deletes it' "exit code $code; left: $(@($old | ForEach-Object Name) -join ', ')"
    Stop-App
    $script:Headline = "a held keymotion-cli was renamed to keymotion-cli.old-<n>.exe, the new one runs ($version), connections and PATH untouched; the old file goes at the next setup run"
    Save-State 'e'
}

Invoke-Scenario 'f' 'Uninstall while Keymotion runs' {
    if (-not (Test-Path -LiteralPath $UninstallExe)) { throw 'Keymotion is not installed' }
    Stop-App
    $p = Start-Process -FilePath $AppExe -PassThru
    $null = $p.Handle # keeps the exit code readable
    $shown = Wait-Until 60 { $p.HasExited -or (Get-VisibleWindows $p.Id).Count -gt 0 }
    if ($p.HasExited) { Skip-Scenario "Keymotion did not keep running on this runner (exit code $($p.ExitCode))" }
    $windows = @(Get-VisibleWindows $p.Id)
    $script:NormalStartWindows = $windows
    Confirm-That (-not $p.HasExited) 'Keymotion runs' "pid $($p.Id); visible windows: $(if ($windows.Count) { $windows -join '; ' } else { 'none' })"
    if (-not $shown) { Add-Note 'no visible window within 60 s' }

    $code = Invoke-Uninstall @('/S')
    Confirm-That ($code -eq 0) 'uninstall.exe /S ends with 0' "exit code $code"
    $closed = Wait-Until 10 { (Get-AppProcesses).Count -eq 0 }
    Confirm-That $closed 'Keymotion was closed by the uninstaller' "running: $(@(Get-AppProcesses | ForEach-Object Id) -join ', ')"
    Confirm-That (-not (Test-Path -LiteralPath $InstallDir)) 'the install folder is gone' $(if (Test-Path -LiteralPath $InstallDir) { (Get-ChildItem -Recurse -Name -LiteralPath $InstallDir) -join ', ' } else { $InstallDir })
    Confirm-That (-not (Test-Path -LiteralPath $UninstallKey)) 'the uninstall entry is gone' ''
    $claude = Get-HookFacts $Claude.Settings 'claude-code' $CliExe
    $codex = Get-HookFacts $Codex.Hooks 'codex' $CliExe
    Confirm-That ($claude.Total -eq 0 -and $codex.Total -eq 0) "Keymotion's hooks are gone from both" "Claude Code $($claude.Total), Codex $($codex.Total) left"
    Confirm-That (-not (Test-Path -LiteralPath $Claude.SkillDir) -and -not (Test-Path -LiteralPath $Codex.SkillDir) -and -not (Test-Path -LiteralPath $Codex.Rule)) 'the skills and the Codex rule are gone' ''
    Confirm-That ((Get-UserView $Claude.Settings) -ceq $script:ClaudeUser -and (Get-EventNames $Claude.Settings) -eq $script:ClaudeSeedEvents) "Claude Code: the user's settings as they were" (Get-EventNames $Claude.Settings)
    Confirm-That ((Get-UserView $Codex.Hooks) -ceq $script:CodexUser -and (Get-EventNames $Codex.Hooks) -eq $script:CodexSeedEvents) "Codex: the user's hooks.json as it was" (Get-EventNames $Codex.Hooks)
    Confirm-That ((Get-FileSha $Codex.Config) -eq $script:ConfigSha -and (Get-FileSha $Codex.UserRule) -eq $script:UserRuleSha -and (Test-Path -LiteralPath $Codex.RulesDir)) "Codex: config.toml, the user's rules file and its folder stay" ''
    $path = Get-UserPath
    Confirm-That ($path.Raw -ceq $PathSeed.Raw -and $path.Kind -eq $PathSeed.Kind) "PATH is exactly the user's again" "$($path.Kind) $($path.Raw)"
    Confirm-That (-not (Test-Path -LiteralPath $UserPathRecordKey)) "Keymotion's PATH record is gone" ''
    $script:Headline = "Keymotion (visible window: $($windows.Count -gt 0)) closed by the uninstaller; files, hooks, skills, rule, PATH entry gone; user content kept"
    Save-State 'f'
}

Invoke-Scenario 'g' 'Upgrade from the last published release' {
    if (-not $PreviousInstaller) { Skip-Scenario 'no published release setup was given (-PreviousInstaller)' }
    $previous = Get-SetupVersion $PreviousInstaller
    Stop-App
    $code = Invoke-Setup $PreviousInstaller @('/S')
    Confirm-That ($code -eq 0 -and (Get-InstalledVersion) -eq $previous) "Keymotion $previous installed (setup /S)" "exit code $code, DisplayVersion $(Get-InstalledVersion)"
    $oldHash = Get-FileSha $AppExe
    Add-Note "keymotion-cli in $previous's installation: $(Test-Path -LiteralPath $CliExe)"
    $p = Start-Process -FilePath $AppExe -PassThru
    Wait-Until 30 { Test-Path -LiteralPath $AppData } | Out-Null
    Start-Sleep -Seconds 5
    Stop-App
    $marker = Join-Path $AppData 'install-test-marker.txt'
    Write-Text $marker "written while $previous was installed"
    $dataBefore = @(Get-ChildItem -Recurse -File -LiteralPath $AppData).Count
    $path = Get-UserPath

    $code = Invoke-Setup $Installer $UpdaterArgs
    Confirm-That ($code -eq 0) "the new setup with the updater's arguments ends with 0" "$($UpdaterArgs -join ' '): exit code $code"
    $newHash = Get-FileSha $AppExe
    Confirm-That ((Get-InstalledVersion) -eq $Version -and $newHash -eq $script:AppHash) "Keymotion $Version is installed (keymotion.exe is this build's)" "DisplayVersion $(Get-InstalledVersion); keymotion.exe $(Get-Short $oldHash) -> $(Get-Short $newHash)"
    if ($previous -eq $Version) { Add-Note "the published release has the same version number ($previous): told apart by keymotion.exe's SHA-256" }
    $version = if (Test-Path -LiteralPath $CliExe) { Get-CliVersion $CliExe } else { 'missing' }
    Confirm-That (Test-CliVersion $version) 'bin\keymotion-cli.exe is added and runs' $version
    Confirm-That ((Test-Path -LiteralPath $marker) -and @(Get-ChildItem -Recurse -File -LiteralPath $AppData).Count -ge $dataBefore) 'the app data folder is kept' "$AppData, $dataBefore files before"
    $pathAfter = Get-UserPath
    Confirm-That ($pathAfter.Raw -ceq $path.Raw -and (Get-BinEntries $pathAfter.Raw).Count -eq 0) 'the update leaves PATH alone (bin goes on it when agent access is turned on)' "$($pathAfter.Kind) $($pathAfter.Raw)"
    $restarted = Wait-Until 45 { (Get-AppProcesses).Count -gt 0 }
    Add-Note "/R started Keymotion after the update: $restarted"
    Stop-App
    $script:Headline = "$previous -> $Version with $($UpdaterArgs -join ' '): new keymotion.exe, bin\keymotion-cli.exe added ($version), app data kept, PATH untouched"
    Save-State 'g'
}

Invoke-Scenario 'h' 'keymotion://background starts Keymotion without a window' {
    Stop-App
    if (-not (Test-Path -LiteralPath $AppExe)) {
        $code = Invoke-Setup $Installer @('/S')
        Confirm-That ($code -eq 0) 'installed (setup /S)' "exit code $code"
    }
    if (-not ("$(Get-SchemeCommand)" -like "*$AppExe*")) {
        # The app registers the scheme when it starts.
        $first = Start-Process -FilePath $AppExe -PassThru
        Wait-Until 30 { "$(Get-SchemeCommand)" -like "*$AppExe*" } | Out-Null
        Stop-App
    }
    $scheme = Get-SchemeCommand
    Confirm-That ("$scheme" -like "*$AppExe*") 'keymotion: opens the installed app' "$scheme"
    $logsBefore = @(Get-ChildItem -LiteralPath $AppLogs -File -ErrorAction SilentlyContinue | ForEach-Object FullName)

    Start-Process 'keymotion://background'
    $started = Wait-Until 30 { (Get-AppProcesses).Count -gt 0 }
    Confirm-That $started 'Keymotion starts' "$(@(Get-AppProcesses | ForEach-Object { "pid $($_.Id)" }) -join ', ')"
    if (-not $started) { return }
    # Long enough for a window to show, as it does within seconds on a
    # normal start.
    Start-Sleep -Seconds 12
    $procs = Get-AppProcesses
    $windows = @($procs | ForEach-Object { Get-VisibleWindows $_.Id })
    $control = if ($null -ne $script:NormalStartWindows) { "a normal start (f) showed: $(if ($script:NormalStartWindows.Count) { $script:NormalStartWindows -join '; ' } else { 'none' })" } else { 'no normal start to compare' }
    Confirm-That ($procs.Count -gt 0 -and $windows.Count -eq 0) 'it shows no window' "visible windows: $(if ($windows.Count) { $windows -join '; ' } else { 'none' }); $control"
    $log = Get-ChildItem -LiteralPath $AppLogs -File -ErrorAction SilentlyContinue | Sort-Object LastWriteTime | Select-Object -Last 1
    if ($log) {
        $line = Select-String -LiteralPath $log.FullName -Pattern 'started in the background for an agent' -SimpleMatch | Select-Object -Last 1
        Add-Note "app log ($($log.Name)): $(if ($line) { $line.Line.Trim() } else { 'no background start line' })"
    }
    Stop-App
    $script:Headline = "Start-Process keymotion://background: Keymotion ran with no visible window; $control"
    Save-State 'h'
}

# ------------------------------------------------------------------ results

Stop-App
if (Test-Path -LiteralPath $AppLogs) { Copy-Item -Recurse -Force -LiteralPath $AppLogs -Destination (Join-Path $OutDir 'app-logs') }

$failedCount = @($Scenarios | Where-Object Status -eq 'FAIL').Count
Write-Host ''
Write-Host "Keymotion $Version install test: $(@($Scenarios | Where-Object Status -eq 'PASS').Count) passed, $failedCount failed, $(@($Scenarios | Where-Object Status -eq 'SKIP').Count) skipped"
foreach ($s in $Scenarios) { Write-Host ("  {0,-4} {1}. {2} -- {3}" -f $s.Status, $s.Id, $s.Title, $s.Evidence) }

$Scenarios | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $OutDir 'results.json')

function Format-Cell([string]$Text) { ($Text -replace '[\r\n]+', ' ' -replace '\|', '\|').Trim() }

$md = [System.Text.StringBuilder]::new()
[void]$md.AppendLine("### Install test: Keymotion $Version")
[void]$md.AppendLine()
[void]$md.AppendLine("Setup ``$([IO.Path]::GetFileName($Installer))``; upgrade from ``$(if ($PreviousInstaller) { [IO.Path]::GetFileName($PreviousInstaller) } else { 'none' })``. Updater arguments: ``$($UpdaterArgs -join ' ')``. Runner: $RunnerFacts.")
[void]$md.AppendLine()
[void]$md.AppendLine('| | Scenario | Result | Evidence |')
[void]$md.AppendLine('|---|---|---|---|')
foreach ($s in $Scenarios) {
    [void]$md.AppendLine("| $($s.Id) | $(Format-Cell $s.Title) | **$($s.Status)** | $(Format-Cell $s.Evidence) |")
}
[void]$md.AppendLine()
foreach ($s in $Scenarios) {
    [void]$md.AppendLine("<details><summary>$($s.Id). $(Format-Cell $s.Title): $($s.Status) ($($s.Seconds) s)</summary>")
    [void]$md.AppendLine()
    foreach ($c in $s.Checks) {
        $mark = if ($null -eq $c.Ok) { 'note' } elseif ($c.Ok) { 'ok' } else { '**FAIL**' }
        $tail = if ($c.Evidence) { ": $(Format-Cell $c.Evidence)" } else { '' }
        [void]$md.AppendLine("- $mark $(Format-Cell $c.What)$tail")
    }
    if ($s.Error -and $s.Status -ne 'SKIP') { [void]$md.AppendLine("- **stopped**: $(Format-Cell $s.Error)") }
    [void]$md.AppendLine()
    [void]$md.AppendLine('</details>')
    [void]$md.AppendLine()
}
$summary = $md.ToString()
Set-Content -LiteralPath (Join-Path $OutDir 'summary.md') -Value $summary
if ($env:GITHUB_STEP_SUMMARY) { Add-Content -LiteralPath $env:GITHUB_STEP_SUMMARY -Value $summary -Encoding utf8 }

exit $(if ($failedCount) { 1 } else { 0 })
