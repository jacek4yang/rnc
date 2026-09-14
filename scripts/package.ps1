# Build and package the x64 MSVC release. Run scripts/verify.ps1 before distributing.
$ErrorActionPreference = 'Stop'
$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $projectRoot
try {
    & cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed' }
    $releaseRoot = Join-Path $projectRoot 'dist/nc-windows-x86_64'
    New-Item -ItemType Directory -Force -Path $releaseRoot | Out-Null
    Copy-Item -LiteralPath 'target/release/nc.exe' -Destination $releaseRoot -Force
    foreach ($name in @('README.md', 'README.zh-CN.md', 'CONTRIBUTING.md', 'SECURITY.md', 'CODE_OF_CONDUCT.md', 'CHANGELOG.md', 'LICENSE', 'THIRD-PARTY-NOTICES.md', 'benchmark-results.json')) {
        Copy-Item -LiteralPath $name -Destination $releaseRoot -Force
    }
    Copy-Item -LiteralPath 'docs' -Destination $releaseRoot -Recurse -Force
    $exeHash = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $releaseRoot 'nc.exe')).Hash.ToLowerInvariant()
    "$exeHash  nc.exe" | Set-Content -Encoding ascii -LiteralPath (Join-Path $releaseRoot 'SHA256SUMS')
    $archivePath = Join-Path $projectRoot 'dist/nc-windows-x86_64.zip'
    Compress-Archive -LiteralPath $releaseRoot -DestinationPath $archivePath -Force
    $archiveHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $archivePath).Hash.ToLowerInvariant()
    "$archiveHash  nc-windows-x86_64.zip" | Set-Content -Encoding ascii -LiteralPath (Join-Path $projectRoot 'dist/SHA256SUMS')
    Write-Output $archivePath
    Write-Output "nc.exe SHA256: $exeHash"
} finally {
    Pop-Location
}
