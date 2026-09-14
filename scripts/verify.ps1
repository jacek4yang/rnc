# Run from the repository root. Fail immediately on any unsuccessful native command.
$ErrorActionPreference = 'Stop'
function Invoke-Checked {
    param([string]$File, [string[]]$Arguments)
    & $File @Arguments
    if ($LASTEXITCODE -ne 0) { throw "$File failed with exit code $LASTEXITCODE" }
}
Invoke-Checked cargo @('fmt', '--check')
Invoke-Checked cargo @('clippy', '--all-targets', '--all-features', '--', '-D', 'warnings')
Invoke-Checked cargo @('test')
Invoke-Checked cargo @('build', '--release')
Invoke-Checked python @('scripts/windows_console_test.py', '--exe', 'target/release/nc.exe')
Invoke-Checked python @('scripts/windows_io_test.py', '--exe', 'target/release/nc.exe')
