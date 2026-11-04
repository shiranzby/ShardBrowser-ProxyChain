[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$CandidatePath,
    [Parameter(Mandatory)][string]$MetadataPath,
    [Parameter(Mandatory)][string]$VerificationPath,
    [Parameter(Mandatory)][string]$OutputDirectory,
    # Optional post-build acceptance evidence. Missing evidence produces a candidate,
    # never an accepted Release. No external path is read from an environment variable.
    [string]$AcceptancePath,
    [switch]$ValidateExisting
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'lib/proxy-chain-build.ps1')
$candidate = [IO.Path]::GetFullPath($CandidatePath)
$output = [IO.Path]::GetFullPath($OutputDirectory)
if ($output.TrimEnd([IO.Path]::DirectorySeparatorChar) -eq $candidate.TrimEnd([IO.Path]::DirectorySeparatorChar) -or $output.StartsWith($candidate.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Keep Portable artifacts outside the candidate source tree.' }
New-Item -ItemType Directory -Force -Path $output | Out-Null
$meta = Get-Content -Raw -LiteralPath $MetadataPath | ConvertFrom-Json
$verified = Get-Content -Raw -LiteralPath $VerificationPath | ConvertFrom-Json
if ($meta.schema -ne 1 -or $meta.result -ne 'applied' -or $meta.candidate_commit -notmatch '^[0-9a-f]{40}$' -or $meta.source_tree -notmatch '^[0-9a-f]{40}$' -or $meta.patch_sha256 -notmatch '^[0-9a-f]{64}$' -or $meta.upstream_tag -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+$' -or $meta.patch_revision -notmatch '^pc[1-9][0-9]*$') { throw 'Invalid candidate provenance.' }
if ($verified.schema -ne 1 -or $verified.kind -ne 'proxy-chain-verification' -or $verified.result -ne 'passed' -or $verified.candidate_commit -ne $meta.candidate_commit -or $verified.source_tree -ne $meta.source_tree -or $verified.patch_sha256 -ne $meta.patch_sha256) { throw 'Verification does not cover this candidate.' }

function EvidencePath([string]$Base, [string]$Relative) {
    if ([IO.Path]::IsPathRooted($Relative) -or $Relative -match '(^|[\\/])\.\.([\\/]|$)' -or $Relative -match ':') { throw 'Evidence must use a safe relative file path.' }
    $basePath = [IO.Path]::GetFullPath($Base).TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
    $p = [IO.Path]::GetFullPath((Join-Path $basePath $Relative))
    if (-not $p.StartsWith($basePath, [StringComparison]::OrdinalIgnoreCase) -or -not (Test-Path -LiteralPath $p -PathType Leaf)) { throw 'Evidence file is missing or outside its proof directory.' }
    # Do not follow a symlink out of the proof directory.
    $item = Get-Item -LiteralPath $p
    while ($item.FullName.Length -ge $basePath.TrimEnd([IO.Path]::DirectorySeparatorChar).Length) {
        if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Evidence symlinks are not accepted.' }
        if (-not $item.PSIsContainer) { $item = $item.Directory } else { $item = $item.Parent }
        if ($null -eq $item) { break }
    }
    $p
}
function CheckFile([string]$Base, [string]$Relative, [string]$Hash) {
    if ($Hash -notmatch '^[0-9a-f]{64}$') { throw 'Evidence hash is invalid.' }
    $p = EvidencePath $Base $Relative
    if ((Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLowerInvariant() -ne $Hash) { throw 'Evidence file hash mismatch.' }
}
$required = @('pipeline-offline','npm-install','ui-kit','ui-check','ui-production','rust-chain-list','rust-chain','rust-full','playwright-install','ui-browser')
$proofBase = Split-Path -Parent ([IO.Path]::GetFullPath($VerificationPath))
foreach ($name in $required) {
    $check = @($verified.checks | Where-Object { $_.id -eq $name })
    if ($check.Count -ne 1 -or $check[0].status -ne 'passed') { throw "Missing verification gate: $name" }
    CheckFile $proofBase $check[0].log $check[0].sha256
}
$head = (& git -C $candidate rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $head -ne $meta.candidate_commit) { throw 'Candidate commit changed after verification.' }
$tree = (& git -C $candidate rev-parse 'HEAD^{tree}').Trim()
if ($LASTEXITCODE -ne 0 -or $tree -ne $meta.source_tree) { throw 'Candidate source changed after verification.' }
Assert-ProxyChainCleanSource $candidate
$filename = "ShardBrowser-$($meta.upstream_tag)-$($meta.patch_revision)-portable-win-x64.exe"
$portable = Join-Path $output $filename
$buildLog = Join-Path $output 'portable-build.log'
if ($ValidateExisting) {
    $previousInfo = Get-Content -Raw -LiteralPath (Join-Path $output 'build-info.json') | ConvertFrom-Json
    if ($previousInfo.result -ne 'built' -or $previousInfo.candidate_commit -ne $head -or $previousInfo.source_tree -ne $tree -or $previousInfo.patch_sha256 -ne $meta.patch_sha256 -or $previousInfo.portable_filename -ne $filename -or $previousInfo.portable_sha256 -ne (Get-FileHash -LiteralPath $portable -Algorithm SHA256).Hash.ToLowerInvariant()) { throw 'Existing Portable binary does not match its original build receipt.' }
}
if (-not $ValidateExisting) {
    if (-not $IsWindows) { throw 'Portable compilation requires a native Windows runner.' }
    if (-not $env:LIBCLANG_PATH) {
        $clang = @('C:/Program Files/LLVM/bin','C:/Program Files (x86)/LLVM/bin') | Where-Object { Test-Path -LiteralPath (Join-Path $_ 'libclang.dll') } | Select-Object -First 1
        if (-not $clang) { throw 'LLVM libclang.dll is unavailable for the default automation features.' }
        $env:LIBCLANG_PATH = $clang
    }
    foreach ($tool in @('node','npm','cargo','cmake','nasm','go')) {
        if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) { throw "Required native build tool is missing from PATH: $tool" }
    }
    Push-Location -LiteralPath $candidate
    $oldPatchRevision = $env:SHARDBROWSER_PATCH_REVISION
    $oldTargetDirectory = $env:CARGO_TARGET_DIR
    $release = Get-ProxyChainReleaseDirectory $candidate $oldTargetDirectory
    try {
        $env:SHARDBROWSER_PATCH_REVISION = $meta.patch_revision
        # Tauri starts Cargo in its Rust folder. Normalize to an absolute path
        # before invoking it so a relative override has one documented meaning.
        $env:CARGO_TARGET_DIR = Split-Path -Parent (Split-Path -Parent $release)
        # Keep the upstream Tauri build + beforeBuildCommand, but Portable needs
        # only the raw EXE. Do not download or run WiX/NSIS installer bundlers.
        & npx tauri build --no-bundle --target x86_64-pc-windows-msvc 2>&1 | Tee-Object -LiteralPath $buildLog | Write-Output
        if ($LASTEXITCODE -ne 0) { throw 'Windows Portable compilation failed.' }
    } finally { $env:SHARDBROWSER_PATCH_REVISION = $oldPatchRevision; $env:CARGO_TARGET_DIR = $oldTargetDirectory; Pop-Location }
    $cargo = Get-Content -Raw -LiteralPath (Join-Path $candidate 'src-tauri/Cargo.toml')
    if ($cargo -notmatch '(?ms)^\[package\]\s*\r?\nname\s*=\s*"([A-Za-z0-9_-]+)"') { throw 'Cannot identify the upstream Cargo package.' }
    $package = $Matches[1]
    # The non-bundled Cargo package executable is authoritative. A product-name
    # binary left from a prior installer build must not win over this build.
    $raw = Join-Path $release "$package.exe"
    if (-not (Test-Path -LiteralPath $raw -PathType Leaf)) { $raw = $null }
    if (-not $raw) { throw 'No raw launcher binary produced; refusing to rename an installer.' }
    Copy-Item -LiteralPath $raw -Destination $portable
}
if ((& git -C $candidate rev-parse HEAD).Trim() -ne $head -or $LASTEXITCODE -ne 0) { throw 'Candidate source changed while building.' }
Assert-ProxyChainCleanSource $candidate
if (-not (Test-Path -LiteralPath $portable -PathType Leaf)) { throw 'Portable binary is missing.' }
$bytes = [IO.File]::OpenRead($portable)
try { if ($bytes.ReadByte() -ne 0x4d -or $bytes.ReadByte() -ne 0x5a) { throw 'Portable artifact is not a Windows PE executable.' } } finally { $bytes.Dispose() }
$sha = (Get-FileHash -LiteralPath $portable -Algorithm SHA256).Hash.ToLowerInvariant()
"$sha  $filename" | Set-Content -LiteralPath (Join-Path $output "$filename.sha256") -Encoding ascii

$publishReady = $false
$acceptanceState = 'not_run'
if ($AcceptancePath) {
    if (-not (Test-Path -LiteralPath $AcceptancePath -PathType Leaf)) { throw 'An explicit acceptance receipt was requested but is missing.' }
    $acceptance = Get-Content -Raw -LiteralPath $AcceptancePath | ConvertFrom-Json
    if ($acceptance.schema -ne 1 -or $acceptance.kind -ne 'proxy-chain-engine-acceptance' -or $acceptance.candidate_commit -ne $head -or $acceptance.source_tree -ne $tree -or $acceptance.portable_sha256 -ne $sha) { throw 'Acceptance does not cover this exact Portable binary and source.' }
    if ($acceptance.platform.os -ne 'windows' -or $acceptance.platform.major -ne 11 -or $acceptance.platform.architecture -ne 'x64' -or $acceptance.engine.sha256 -notmatch '^[0-9a-f]{64}$' -or [string]::IsNullOrWhiteSpace($acceptance.engine.version) -or [string]::IsNullOrWhiteSpace($acceptance.engine.build)) { throw 'Acceptance engine/platform identity is incomplete.' }
    $ids = @('engine_socks5_auth','two_layer_http','two_layer_socks5','final_exit_is_profile','fail_closed_system_down','fail_closed_system_auth','fail_closed_profile_auth','fail_closed_midstream','three_profile_isolation','listener_cleanup','quic_disabled','webrtc_no_direct_udp','dns_via_chain','profile_data_preserved','portable_upgrade_preserved','settings_compatibility','thirty_profiles_saved')
    $base = Split-Path -Parent ([IO.Path]::GetFullPath($AcceptancePath))
    $allPassed = $true
    foreach ($id in $ids) {
        $one = @($acceptance.checks | Where-Object { $_.id -eq $id })
        if ($one.Count -ne 1 -or $one[0].status -notin @('passed','failed','not_run')) { throw "Acceptance check is missing or invalid: $id" }
        if ($one[0].status -ne 'passed') { $allPassed = $false; continue }
        if (@($one[0].evidence).Count -eq 0) { throw "A passed acceptance check has no evidence: $id" }
        foreach ($file in $one[0].evidence) { CheckFile $base $file.path $file.sha256 }
    }
    if ($allPassed) {
        foreach ($field in @('system_exit_ip','profile_exit_ip','observed_exit_ip')) {
            $parsed = $null
            if (-not [Net.IPAddress]::TryParse([string]$acceptance.route.$field, [ref]$parsed)) { throw 'Acceptance route IP is invalid.' }
        }
        if ($acceptance.route.observed_exit_ip -ne $acceptance.route.profile_exit_ip -or $acceptance.route.observed_exit_ip -eq $acceptance.route.system_exit_ip) { throw 'Actual exit IP does not prove the second proxy as the final exit.' }
        $publishReady = $true
        $acceptanceState = 'passed'
    } else { $acceptanceState = 'incomplete' }
}
$info = [ordered]@{
    schema=1; result='built'; built_at_utc=[DateTime]::UtcNow.ToString('o')
    upstream_repository=$meta.upstream_repository; upstream_tag=$meta.upstream_tag; upstream_commit=$meta.upstream_commit
    patch_revision=$meta.patch_revision; patch_commit=$meta.patch_commit; patch_sha256=$meta.patch_sha256
    candidate_commit=$head; source_tree=$tree; maintenance_commit=$meta.maintenance_commit
    portable_filename=$filename; portable_sha256=$sha; verification_sha256=(Get-FileHash -LiteralPath $VerificationPath -Algorithm SHA256).Hash.ToLowerInvariant()
    acceptance=$acceptanceState; publish_ready=$publishReady; artifact_kind=$(if($publishReady){'accepted_portable'}else{'candidate_portable'})
    build_command='npx tauri build --no-bundle --target x86_64-pc-windows-msvc'; node_version=((& node --version) -join ''); rust_version=((& rustc --version) -join '')
    run_url=$(if($env:GITHUB_RUN_ID){"https://github.com/$env:GITHUB_REPOSITORY/actions/runs/$env:GITHUB_RUN_ID"}else{$null})
}
if ($AcceptancePath) { $info.acceptance_receipt_sha256 = (Get-FileHash -LiteralPath $AcceptancePath -Algorithm SHA256).Hash.ToLowerInvariant(); $info.engine = $acceptance.engine }
$info | ConvertTo-Json -Depth 8 | Set-Content -LiteralPath (Join-Path $output 'build-info.json') -Encoding utf8
Write-Output "$filename SHA256=$sha; acceptance=$acceptanceState; publish_ready=$publishReady"
