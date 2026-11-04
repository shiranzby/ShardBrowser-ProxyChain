# The build script resolves relative overrides against CandidatePath, then passes
# an absolute CARGO_TARGET_DIR to Tauri/Cargo so child working directories agree.
function Get-ProxyChainReleaseDirectory([string]$CandidatePath, [string]$CargoTargetDirectory) {
    $candidate = [IO.Path]::GetFullPath($CandidatePath)
    $target = if ([string]::IsNullOrWhiteSpace($CargoTargetDirectory)) {
        Join-Path $candidate 'src-tauri/target'
    } elseif ([IO.Path]::IsPathRooted($CargoTargetDirectory)) {
        [IO.Path]::GetFullPath($CargoTargetDirectory)
    } else {
        [IO.Path]::GetFullPath((Join-Path $candidate $CargoTargetDirectory))
    }
    Join-Path $target 'x86_64-pc-windows-msvc/release'
}

# A successful native tool may print nothing. Create its evidence file before
# starting it; Tee-Object otherwise never opens a file for an empty pipeline.
function Invoke-ProxyChainLoggedCommand([string]$Name, [string]$Command, [string[]]$Arguments, [string]$OutputDirectory) {
    $log = Join-Path $OutputDirectory "$Name.log"
    [IO.File]::WriteAllText($log, '')
    & $Command @Arguments 2>&1 | Tee-Object -LiteralPath $log | ForEach-Object { Write-Host $_ }
    $code = $LASTEXITCODE
    if ($code -ne 0) { throw "$Name failed with exit code $code. Publication is blocked." }
    [pscustomobject]@{ id=$Name; status='passed'; log=(Split-Path -Leaf $log); sha256=(Get-FileHash -LiteralPath $log -Algorithm SHA256).Hash.ToLowerInvariant() }
}
