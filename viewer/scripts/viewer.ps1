#Requires -Version 7.0
<#
.SYNOPSIS
    Install, run and test the Clio viewers (PowerShell 7; Windows, also Linux/macOS).

.DESCRIPTION
    scripts/viewer.ps1 install [rust|python|all] [--jira]
    scripts/viewer.ps1 run rust|python [viewer args...]
    scripts/viewer.ps1 test [rust|python|all]

    `run` installs first if needed and, without --file or --jira, opens the
    sample plan. Relative paths in viewer arguments are resolved from the directory
    you run the script in. Set CLIO_PYTHON to choose a Python interpreter.

.EXAMPLE
    ./scripts/viewer.ps1 install
    ./scripts/viewer.ps1 run rust
    ./scripts/viewer.ps1 run python --file C:\plans\tasks.json --theme mocha
#>
# No param() block on purpose: every argument arrives in $args as a plain
# string, so viewer options such as --file or --theme pass through untouched
# instead of being parsed as PowerShell parameters.
$Command = if ($args.Count -ge 1) { [string] $args[0] } else { 'help' }
$Viewer = if ($args.Count -ge 2) { [string] $args[1] } else { $null }
$rest = if ($args.Count -ge 3) { @($args[2..($args.Count - 1)] | ForEach-Object { [string] $_ }) } else { @() }
$Jira = $false
$ViewerArgs = @()
foreach ($a in $rest) {
    if ($Command -eq 'install' -and $a -in '--jira', '-Jira') { $Jira = $true } else { $ViewerArgs += $a }
}
if ($Command -eq 'install' -and $Viewer -in '--jira', '-Jira') { $Jira = $true; $Viewer = $null }

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false  # we check $LASTEXITCODE ourselves

$ViewerDir = Split-Path -Parent $PSScriptRoot
$RustDir = Join-Path $ViewerDir 'rust'
$PyDir = Join-Path $ViewerDir 'python'
$Venv = Join-Path $PyDir '.venv'
$Exe = if ($IsWindows) { '.exe' } else { '' }
$VenvBin = if ($IsWindows) { Join-Path $Venv 'Scripts' } else { Join-Path $Venv 'bin' }
$VenvPython = Join-Path $VenvBin "python$Exe"
$RustBin = Join-Path $RustDir 'target' 'release' "clio-viewer$Exe"
$PyBin = Join-Path $VenvBin "clio-viewer$Exe"
$Sample = Join-Path $ViewerDir 'fixtures' 'sample.json'

function Say([string] $Message) { Write-Host "==> $Message" -ForegroundColor Blue }

function Fail([string] $Message) {
    Write-Host "error: $Message" -ForegroundColor Red
    exit 1
}

# Runs a native command and stops on a non-zero exit code.
function Invoke-Checked([string] $What, [scriptblock] $Block) {
    & $Block
    if ($LASTEXITCODE -ne 0) { Fail "$What failed (exit code $LASTEXITCODE)" }
}

function Assert-Cargo {
    if (Get-Command cargo -ErrorAction SilentlyContinue) { return }
    if ($IsWindows) {
        Fail "cargo not found. Install Rust: 'winget install Rustlang.Rustup' (it also needs the Visual Studio C++ Build Tools: 'winget install Microsoft.VisualStudio.2022.BuildTools'), then open a new terminal."
    }
    Fail "cargo not found. Install Rust from https://rustup.rs"
}

# Returns the command (as an array: program + arguments) of the first Python >= 3.10.
function Find-Python {
    $candidates = @()
    if ($env:CLIO_PYTHON) { $candidates += , @($env:CLIO_PYTHON) }
    if ($IsWindows) { $candidates += , @('py', '-3') }
    foreach ($name in 'python3', 'python') { $candidates += , @($name) }
    foreach ($c in $candidates) {
        if (-not (Get-Command $c[0] -ErrorAction SilentlyContinue)) { continue }
        $cmdArgs = @($c | Select-Object -Skip 1)
        # The Windows Store "python" stub fails this check instead of opening the Store.
        & $c[0] @cmdArgs -c 'import sys; sys.exit(sys.version_info < (3, 10))' 2>$null
        if ($LASTEXITCODE -eq 0) { return , $c }
    }
    if ($IsWindows) {
        Fail "Python 3.10+ not found. Install it: 'winget install Python.Python.3.13', then open a new terminal (or set CLIO_PYTHON)."
    }
    Fail "Python 3.10+ not found. Install it with your package manager, or set CLIO_PYTHON."
}

function Install-Rust {
    Assert-Cargo
    $features = if ($Jira) { @('--features', 'jira') } else { @() }
    Say ("Building the Rust viewer (release" + $(if ($Jira) { ', with Jira support' } else { '' }) + ")")
    Push-Location $RustDir
    try { Invoke-Checked 'cargo build' { cargo build --release @features } }
    finally { Pop-Location }
    Say "Rust viewer: $RustBin"
}

function Install-Python {
    $py = Find-Python
    if (-not (Test-Path $VenvPython)) {
        $pyArgs = @($py | Select-Object -Skip 1)
        Say "Creating $Venv with $(& $py[0] @pyArgs --version 2>&1)"
        Invoke-Checked 'creating the virtual environment' { & $py[0] @pyArgs -m venv $Venv }
    }
    Say 'Installing the Python viewer and its dependencies into the venv'
    Invoke-Checked 'pip upgrade' { & $VenvPython -m pip install --quiet --upgrade pip }
    # PYTHONWARNINGS hides setuptools' editable-install banner.
    $env:PYTHONWARNINGS = 'ignore'
    try { Invoke-Checked 'pip install' { & $VenvPython -m pip install --quiet -e "$PyDir[test]" } }
    finally { Remove-Item Env:PYTHONWARNINGS -ErrorAction SilentlyContinue }
    Say "Python viewer: $PyBin"
}

function Invoke-Install {
    $which = if ($Viewer) { $Viewer } else { 'all' }
    switch ($which) {
        'rust' { Install-Rust }
        'python' { Install-Python }
        'all' { Install-Rust; Install-Python }
        default { Fail "install: expected rust, python or all, not '$which'" }
    }
}

function Invoke-Run {
    switch ($Viewer) {
        'rust' { $bin = $RustBin; if (-not (Test-Path $bin)) { Install-Rust } }
        'python' { $bin = $PyBin; if (-not (Test-Path $bin)) { Install-Python } }
        default { Fail "run: say which viewer: 'run rust' or 'run python'" }
    }
    $runArgs = @($ViewerArgs | Where-Object { $_ -ne '--' })
    # No data source given: show the sample plan.
    $hasSource = @($runArgs | Where-Object { $_ -in '--file', '--jira' -or $_ -like '--file=*' }).Count -gt 0
    if (-not $hasSource) { $runArgs = @('--file', $Sample) + $runArgs }
    # Box-drawing characters in captured output (e.g. --snapshot | Out-File).
    $saved = [Console]::OutputEncoding
    [Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
    try { & $bin @runArgs }
    finally { [Console]::OutputEncoding = $saved }
    exit $LASTEXITCODE
}

function Invoke-Test {
    $which = if ($Viewer) { $Viewer } else { 'all' }
    if ($which -notin 'rust', 'python', 'all') { Fail "test: expected rust, python or all, not '$which'" }
    if ($which -ne 'python') {
        Assert-Cargo
        Say 'Rust tests'
        Push-Location $RustDir
        try { Invoke-Checked 'cargo test' { cargo test } }
        finally { Pop-Location }
    }
    if ($which -ne 'rust') {
        if (-not (Test-Path $VenvPython)) { Install-Python }
        if (Get-Command cargo -ErrorAction SilentlyContinue) {
            # The parity tests compare against the Rust debug build.
            Push-Location $RustDir
            try { Invoke-Checked 'cargo build' { cargo build --quiet } }
            finally { Pop-Location }
        }
        else {
            Say 'cargo not found: Rust/Python parity tests will be skipped'
        }
        Say 'Python tests'
        Push-Location $PyDir
        try { Invoke-Checked 'pytest' { & $VenvPython -m pytest -q } }
        finally { Pop-Location }
    }
}

switch ($Command) {
    'install' {
        if ($ViewerArgs.Count) { Fail "install: unknown argument '$($ViewerArgs[0])'" }
        Invoke-Install
    }
    'run' { Invoke-Run }
    'test' { Invoke-Test }
    { $_ -in 'help', '-h', '--help', '-?' } { Get-Help $PSCommandPath -Detailed; exit 0 }
    default { Fail "unknown command '$Command' (expected install, run or test)" }
}
