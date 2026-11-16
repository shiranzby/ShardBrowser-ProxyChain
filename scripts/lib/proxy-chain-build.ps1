# The build script resolves relative overrides against CandidatePath, then passes
# an absolute CARGO_TARGET_DIR to Tauri/Cargo so child working directories agree.
function Assert-ProxyChainCleanSource([string]$CandidatePath) {
    # Compare Git-normalized content, not stat/line-ending hints in porcelain.
    # A generator may write LF on Windows while preserving the indexed blob.
    & git -C $CandidatePath diff --quiet
    if ($LASTEXITCODE -ne 0) { throw 'Candidate source has unstaged changes or could not be compared.' }
    & git -C $CandidatePath diff --cached --quiet
    if ($LASTEXITCODE -ne 0) { throw 'Candidate source has staged changes or could not be compared.' }
    $untracked = @(& git -C $CandidatePath ls-files --others --exclude-standard)
    if ($LASTEXITCODE -ne 0) { throw 'Candidate untracked files could not be checked.' }
    if ($untracked.Count -ne 0) { throw 'Candidate source has untracked files.' }
}

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

# Run a browser suite behind an external watchdog. Playwright's per-test timeout does
# not cover a wedged Node/Vite child after a suite loses its last browser event.
function Invoke-ProxyChainTimedCommand([string]$Name, [string]$Command, [string[]]$Arguments, [string]$WorkingDirectory, [string]$OutputDirectory, [int]$TimeoutSeconds = 900) {
    $log = Join-Path $OutputDirectory "$Name.log"
    $stdout = Join-Path $OutputDirectory "$Name.stdout.tmp"
    $stderr = Join-Path $OutputDirectory "$Name.stderr.tmp"
    foreach ($path in @($log, $stdout, $stderr)) { [IO.File]::WriteAllText($path, '') }
    $proc = Start-Process -FilePath $Command -ArgumentList $Arguments -WorkingDirectory $WorkingDirectory -RedirectStandardOutput $stdout -RedirectStandardError $stderr -PassThru
    try {
        if (-not $proc.WaitForExit($TimeoutSeconds * 1000)) {
            & taskkill.exe /PID $proc.Id /T /F 2>$null | Out-Null
            throw "$Name exceeded the ${TimeoutSeconds}s browser-suite watchdog."
        }
        $proc.Refresh()
        $code = $proc.ExitCode
        $combined = ((Get-Content -Raw -LiteralPath $stdout) + (Get-Content -Raw -LiteralPath $stderr))
        [IO.File]::WriteAllText($log, $combined)
        Write-Host $combined
        if ($code -ne 0) { throw "$Name failed with exit code $code. Publication is blocked." }
        [pscustomobject]@{ id=$Name; status='passed'; log=(Split-Path -Leaf $log); sha256=(Get-FileHash -LiteralPath $log -Algorithm SHA256).Hash.ToLowerInvariant() }
    } finally {
        foreach ($path in @($stdout, $stderr)) { Remove-Item -LiteralPath $path -Force -ErrorAction SilentlyContinue }
    }
}

function New-ProxyChainPlaywrightConfig([string]$CandidatePath, [string]$OutputDirectory, [string]$NodeExecutable, [int]$Port) {
    $candidate = [IO.Path]::GetFullPath($CandidatePath)
    $output = [IO.Path]::GetFullPath($OutputDirectory)
    $vite = Join-Path $candidate 'node_modules/vite/bin/vite.js'
    [ordered]@{
        testDir=(Join-Path $candidate 'e2e'); fullyParallel=$false; workers=1; forbidOnly=$true; reporter='list'
        outputDir=(Join-Path $output 'playwright-results'); timeout=30000
        use=@{browserName='chromium';baseURL="http://127.0.0.1:$Port";viewport=@{width=1280;height=800};locale='en-US';timezoneId='UTC';reducedMotion='reduce'}
        projects=@(@{name='dev'})
        # The config lives with proofs, but Vite must resolve index.html and
        # vite.config.ts from the candidate source directory.
        webServer=@{cwd=$candidate;command=('"{0}" "{1}" --host 127.0.0.1 --port {2} --strictPort' -f $NodeExecutable,$vite,$Port);url="http://127.0.0.1:$Port";reuseExistingServer=$false;timeout=120000;env=@{VITE_MOCK='1'}}
    }
}
