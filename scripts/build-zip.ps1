$ErrorActionPreference = "Stop"

$repoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$packageJsonPath = Join-Path $repoRoot "package.json"
$packageJson = Get-Content $packageJsonPath -Raw | ConvertFrom-Json
$version = $packageJson.version

$exePath = Join-Path $repoRoot "src-tauri\\target\\release\\ai-quota-monitor.exe"
if (-not (Test-Path $exePath)) {
  throw "Release binary not found: $exePath. Run 'npm run build:tauri' first."
}

$outDir = Join-Path $repoRoot "src-tauri\\target\\release\\bundle\\zip"
New-Item -Path $outDir -ItemType Directory -Force | Out-Null

$zipName = "AI-Quota-Monitor-$version-windows-x64.zip"
$zipPath = Join-Path $outDir $zipName
if (Test-Path $zipPath) {
  Remove-Item $zipPath -Force
}

$stagingDir = Join-Path $env:TEMP ("ai-quota-monitor-zip-" + [guid]::NewGuid().ToString("N"))
New-Item -Path $stagingDir -ItemType Directory -Force | Out-Null

Copy-Item $exePath (Join-Path $stagingDir "AI Quota Monitor.exe")
Compress-Archive -Path (Join-Path $stagingDir "*") -DestinationPath $zipPath -CompressionLevel Optimal

Remove-Item $stagingDir -Recurse -Force
Write-Host "Created zip package: $zipPath"
