[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$CandidatePath,
    [Parameter(Mandatory)][string]$MetadataPath,
    [Parameter(Mandatory)][string]$OutputDirectory
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'lib/proxy-chain-build.ps1')
$candidate = [IO.Path]::GetFullPath($CandidatePath)
$output = [IO.Path]::GetFullPath($OutputDirectory)
if ($output.TrimEnd([IO.Path]::DirectorySeparatorChar) -eq $candidate.TrimEnd([IO.Path]::DirectorySeparatorChar) -or $output.StartsWith($candidate.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Keep verification evidence outside the candidate source tree.' }
New-Item -ItemType Directory -Force -Path $output | Out-Null
$meta = Get-Content -Raw -LiteralPath $MetadataPath | ConvertFrom-Json
function SourceIdentity {
    $head = (& git -C $candidate rev-parse HEAD).Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read candidate commit.' }
    $tree = (& git -C $candidate rev-parse 'HEAD^{tree}').Trim()
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read candidate tree.' }
    if ($meta.schema -ne 1 -or $meta.result -ne 'applied' -or $head -ne $meta.candidate_commit -or $tree -ne $meta.source_tree) { throw 'Candidate provenance does not match the patch application receipt.' }
    if (@(& git -C $candidate status --porcelain=v1).Count -ne 0) { throw 'Candidate source tree is dirty.' }
}
function Step([string]$Name, [string]$Command, [string[]]$Arguments) {
    Write-Output "Running $Name"
    $script:checks += Invoke-ProxyChainLoggedCommand -Name $Name -Command $Command -Arguments $Arguments -OutputDirectory $output
}
$checks = @()
SourceIdentity
if ($IsWindows -and [string]::IsNullOrWhiteSpace($env:LIBCLANG_PATH)) {
    $clang = @('C:/Program Files/LLVM/bin', 'C:/Program Files (x86)/LLVM/bin') | Where-Object { Test-Path -LiteralPath (Join-Path $_ 'libclang.dll') } | Select-Object -First 1
    if ($clang) { $env:LIBCLANG_PATH = $clang }
    else { throw 'libclang.dll is required for default automation features; install LLVM or set LIBCLANG_PATH explicitly.' }
}
foreach ($tool in @('git','node','npm','cargo','rustc','cmake','nasm','go')) {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw "Required native build tool is missing from PATH: $tool. Default automation features must not be disabled to bypass this dependency." }
}
if (-not (Test-Path -LiteralPath (Join-Path $candidate 'src-tauri/src/proxy_chain/mod.rs'))) { throw 'Proxy-chain Rust module is missing.' }
$uiTest = Join-Path $candidate 'e2e/proxy-chain-settings.spec.ts'
if (-not (Test-Path -LiteralPath $uiTest)) { throw 'Proxy-chain settings browser test is missing.' }
Push-Location -LiteralPath $candidate
try {
    Step 'pipeline-offline' 'pwsh' @('-NoProfile','-File',(Join-Path $PSScriptRoot 'test-proxy-chain-pipeline.ps1'),'-OutputDirectory',(Join-Path $output 'pipeline-offline'))
    Step 'npm-install' 'npm' @('ci', '--no-audit', '--no-fund')
    Step 'ui-kit' 'npm' @('run', 'build:ui-kit')
    Step 'ui-check' 'npm' @('run', 'check')
    # The kit is already built above; do not reinstall it for this production UI check.
    Step 'ui-production' 'npm' @('--ignore-scripts', 'run', 'build')
    Step 'rust-chain-list' 'cargo' @('test', '--manifest-path', 'src-tauri/Cargo.toml', '--locked', 'proxy_chain', '--', '--list')
    $listed = Get-Content -Raw -LiteralPath (Join-Path $output 'rust-chain-list.log')
    if ($listed -notmatch '(?m)^proxy_chain::[^\r\n]+: test\s*$') { throw 'Rust proxy-chain test filter matched no tests.' }
    Step 'rust-chain' 'cargo' @('test', '--manifest-path', 'src-tauri/Cargo.toml', '--locked', 'proxy_chain')
    Step 'rust-full' 'cargo' @('test', '--manifest-path', 'src-tauri/Cargo.toml', '--locked')
    Step 'playwright-install' 'npx' @('playwright', 'install', 'chromium')

    # Upstream's package scripts use POSIX env syntax. This config uses Playwright's
    # env map, which works on native Windows without a shell-specific assignment.
    $node = (Get-Command node).Source
    $vite = Join-Path $candidate 'node_modules/vite/bin/vite.js'
    $port = 43127
    $config = [ordered]@{
        testDir=(Join-Path $candidate 'e2e'); fullyParallel=$true; forbidOnly=$true; reporter='list'
        outputDir=(Join-Path $output 'playwright-results')
        timeout=30000
        use=@{ browserName='chromium'; baseURL="http://127.0.0.1:$port"; viewport=@{width=1280;height=800}; locale='en-US'; timezoneId='UTC'; reducedMotion='reduce' }
        projects=@(@{name='dev'})
        webServer=@{command=('"{0}" "{1}" --host 127.0.0.1 --port {2} --strictPort' -f $node,$vite,$port);url="http://127.0.0.1:$port";reuseExistingServer=$false;timeout=120000;env=@{VITE_MOCK='1'}}
    }
    $configPath = Join-Path $output 'playwright-proxy-chain.config.mjs'
    ('export default ' + ($config | ConvertTo-Json -Depth 12) + ';') | Set-Content -LiteralPath $configPath -Encoding utf8
    Step 'ui-browser' 'npx' @('playwright','test','--config',$configPath,'proxy-chain-settings.spec.ts','settings-save.spec.ts','settings-secret.spec.ts','--project=dev')
    SourceIdentity
    $receipt = [ordered]@{
        schema=1; kind='proxy-chain-verification'; result='passed'; completed_at_utc=[DateTime]::UtcNow.ToString('o')
        candidate_commit=$meta.candidate_commit; source_tree=$meta.source_tree; patch_sha256=$meta.patch_sha256
        checks=$checks
    }
    $receipt | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $output 'verification.json') -Encoding utf8
} finally { Pop-Location }
