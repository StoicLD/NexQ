param([ValidateSet('dev', 'setup', 'doctor', 'check', 'build')][string]$Action = 'dev')
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
Set-Location -LiteralPath $root
$nodeVersion = (Get-Content -LiteralPath '.node-version' -Raw).Trim()
$nodeDir = if ($env:NVM_HOME) { Join-Path $env:NVM_HOME ("v" + $nodeVersion) }
if (-not $nodeDir -or -not (Test-Path -LiteralPath (Join-Path $nodeDir 'node.exe'))) {
    throw "Install Node $nodeVersion with nvm-windows (nvm install $nodeVersion)."
}
$cargoBin = if ($env:CARGO_HOME) { Join-Path $env:CARGO_HOME 'bin' } else { Join-Path $env:USERPROFILE '.cargo\bin' }
$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
$vs = & $vswhere -latest -version '[17.0,18.0)' -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $vs) { throw 'Install the Visual Studio 2022 C++ Build Tools.' }
Import-Module (Join-Path $vs 'Common7\Tools\Microsoft.VisualStudio.DevShell.dll')
Enter-VsDevShell -VsInstallPath $vs -SkipAutomaticLocation -DevCmdArguments '-arch=x64 -host_arch=x64 -vcvars_ver=14.44.35207 -winsdk=10.0.26100.0' | Out-Null

$env:LIBCLANG_PATH = Join-Path $env:ProgramFiles 'LLVM\bin'
$env:Path = (@($nodeDir, $cargoBin, (Join-Path $env:ProgramFiles 'CMake\bin'),
    (Join-Path $env:LOCALAPPDATA 'Microsoft\WinGet\Links'), $env:LIBCLANG_PATH, $env:Path) -join ';')
# The desktop app's CLI may be absent from Explorer's PATH when launched by double-click.
if (-not (Get-Command codex.exe -ErrorAction SilentlyContinue)) {
    $codexPackage = Get-AppxPackage -Name OpenAI.Codex -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($codexPackage) {
        $codexDir = Join-Path $codexPackage.InstallLocation 'app\resources'
        if (Test-Path -LiteralPath (Join-Path $codexDir 'codex.exe')) { $env:Path += ";$codexDir" }
    }
}
$env:CARGO_TARGET_DIR = Join-Path $root '.dev\t'
$env:CMAKE_GENERATOR = 'Ninja'
$env:CMAKE_POLICY_VERSION_MINIMUM = '3.5'
$env:CARGO_BUILD_JOBS = '8'
$env:VSLANG = '1033'
# Prefer the repository toolchain over an override inherited from a parent shell.
Remove-Item Env:RUSTUP_TOOLCHAIN -ErrorAction SilentlyContinue
$config = Join-Path $root 'src-tauri\tauri.win-dev.conf.json'
function Run([string]$Exe, [string[]]$Arguments) {
    & $Exe @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$Exe $Arguments exited with $LASTEXITCODE" }
}
if ($env:VCToolsVersion.TrimEnd('\') -ne '14.44.35207' -or $env:WindowsSDKVersion.TrimEnd('\') -ne '10.0.26100.0') {
    throw 'Required x64 MSVC 14.44.35207 and SDK 10.0.26100.0 were not activated.'
}
if ((node --version) -ne ('v' + $nodeVersion) -or (npm.cmd --version) -ne '12.2.0') {
    throw "Expected Node $nodeVersion / npm 12.2.0 in $nodeDir."
}
if (-not (Test-Path -LiteralPath (Join-Path $env:LIBCLANG_PATH 'libclang.dll'))) { throw 'Missing libclang.dll.' }
switch ($Action) {
    'doctor' {
        Run node @('--version')
        Run npm.cmd @('--version')
        Run rustup @('show', 'active-toolchain')
        Run cargo @('--version')
        if (Get-Command codex.exe -ErrorAction SilentlyContinue) { Run codex.exe @('--version') }
        Run cmake @('--version')
        Run ninja @('--version')
        Run clang @('--version')
        Write-Host "MSVC $env:VCToolsVersion / SDK $env:WindowsSDKVersion / target $env:CARGO_TARGET_DIR"
    }
    'setup' {
        Run rustc @('--version')
        Run npm.cmd @('ci','--no-audit','--no-fund')
        Run node @('scripts/fetch-onnxruntime.mjs')
        Push-Location src-tauri
        try { Run cargo @('fetch','--locked','--target','x86_64-pc-windows-msvc') } finally { Pop-Location }
    }
    'check' {
        Run npm.cmd @('run','build')
        Run npm.cmd @('test')
        Run node @('scripts/fetch-onnxruntime.mjs')
        Push-Location src-tauri
        try { Run cargo @('check','--locked','--features','windows-dev-profile') } finally { Pop-Location }
    }
    'build' {
        Run npm.cmd @('run','tauri','--','build','--debug','--no-bundle','--config',$config)
        $debug = Join-Path $env:CARGO_TARGET_DIR 'debug'
        $out = New-Item -ItemType Directory -Path (Join-Path $root '.dev\app') -Force
        Copy-Item -LiteralPath (Join-Path $debug 'nexq.exe') -Destination $out.FullName
        Get-ChildItem -LiteralPath $debug -File -Filter '*.dll' | Copy-Item -Destination $out.FullName
        Write-Host "Standalone app: $($out.FullName)\nexq.exe"
    }
    'dev'   { Run npm.cmd @('run','tauri','--','dev','--config',$config) }
}
