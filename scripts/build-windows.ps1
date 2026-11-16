# Builds the video editor on this Windows computer and installs it.
# Run in PowerShell as administrator:
#   powershell -ExecutionPolicy Bypass -File scripts\build-windows.ps1
# Or, with nothing downloaded yet:
#   irm https://raw.githubusercontent.com/shahergamal/arabic-video-editor/main/scripts/build-windows.ps1 | iex
#
# The first run takes about an hour: it installs the build tools (Git, Node,
# Rust, Visual Studio Build Tools, CMake, LLVM) and compiles everything.
# Later runs only update and rebuild.
param(
  [string]$Branch = "main",
  [string]$Folder = "$env:USERPROFILE\arabic-video-editor"
)
$ErrorActionPreference = "Stop"

function Step($text) { Write-Host "`n==> $text" -ForegroundColor Cyan }

function Refresh-Path {
  $env:Path = [Environment]::GetEnvironmentVariable("Path", "Machine") + ";" +
              [Environment]::GetEnvironmentVariable("Path", "User") + ";" +
              "$env:USERPROFILE\.cargo\bin"
}

function Need($id, $command, $extra = @()) {
  if ($command -and (Get-Command $command -ErrorAction SilentlyContinue)) { return }
  Step "Installing $id"
  winget install --id $id -e --silent --accept-package-agreements --accept-source-agreements @extra
  if ($LASTEXITCODE -ne 0 -and $LASTEXITCODE -ne -1978335189) { throw "winget could not install $id" }
  Refresh-Path
}

if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
  throw "winget is missing. Install 'App Installer' from the Microsoft Store, then run this again."
}

Step "Build tools"
Need "Git.Git" "git"
Need "OpenJS.NodeJS.LTS" "node"
Need "Rustlang.Rustup" "cargo"
Need "Kitware.CMake" "cmake"
Need "LLVM.LLVM" "clang"
$vswhere = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe"
$hasVC = (Test-Path $vswhere) -and (& $vswhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath)
if (-not $hasVC) {
  Need "Microsoft.VisualStudio.2022.BuildTools" $null @("--override", "--quiet --wait --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended")
}
Refresh-Path
if (-not $env:LIBCLANG_PATH -and (Test-Path "$env:ProgramFiles\LLVM\bin")) { $env:LIBCLANG_PATH = "$env:ProgramFiles\LLVM\bin" }
rustup default stable | Out-Null

Step "Getting the code ($Branch)"
if (Test-Path "$Folder\.git") {
  git -C $Folder fetch origin $Branch
  git -C $Folder checkout $Branch
  git -C $Folder reset --hard "origin/$Branch"
} else {
  git clone --branch $Branch https://github.com/shahergamal/arabic-video-editor.git $Folder
}
Set-Location $Folder

Step "Interface packages"
Push-Location app
npm ci
if ($LASTEXITCODE -ne 0) { throw "npm ci failed" }
Pop-Location

Step "ffmpeg"
node scripts/fetch-ffmpeg.mjs x86_64-pc-windows-msvc
if ($LASTEXITCODE -ne 0) { throw "ffmpeg download failed" }

Step "Building the installer (this is the long part)"
Push-Location app
npx tauri build --bundles nsis
if ($LASTEXITCODE -ne 0) { throw "build failed" }
Pop-Location

$setup = Get-ChildItem "$Folder\target\release\bundle\nsis\*-setup.exe", "$Folder\app\src-tauri\target\release\bundle\nsis\*-setup.exe" -ErrorAction SilentlyContinue |
  Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $setup) { throw "the installer was not found after the build" }

Step "Installing $($setup.Name)"
Start-Process $setup.FullName -ArgumentList "/S" -Wait
Copy-Item $setup.FullName "$env:USERPROFILE\Desktop\" -Force
Write-Host "`nDone. Open the editor from the Start menu. A copy of the installer is on the Desktop." -ForegroundColor Green
