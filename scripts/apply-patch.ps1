[CmdletBinding()]
param(
    [string]$ManifestPath = (Join-Path $PSScriptRoot '../patches/manifest.json'),
    [Parameter(Mandatory)][string]$UpstreamTag,
    [Parameter(Mandatory)][string]$UpstreamCommit,
    [Parameter(Mandatory)][string]$CandidatePath,
    [Parameter(Mandatory)][string]$OutputPath,
    # A local mirror is useful for offline conflict drills; CI uses the default.
    [string]$SourceRepository = 'https://github.com/ProxyShard/ShardBrowser.git'
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
function Invoke-Git([Parameter(ValueFromRemainingArguments=$true)][string[]]$Arguments) {
    $result = & git @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) { throw "git $($Arguments[0]) failed: $($result -join "`n")" }
    $result
}
$manifest = Get-Content -Raw -LiteralPath $ManifestPath | ConvertFrom-Json
if ($manifest.schema -ne 1 -or $manifest.upstream_repository -ne 'ProxyShard/ShardBrowser' -or -not $manifest.ready) { throw 'The functional patch manifest is not ready.' }
if ($UpstreamTag -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+$' -or $UpstreamCommit -notmatch '^[0-9a-f]{40}$') { throw 'Invalid stable tag or commit.' }
if ($manifest.base_commit -notmatch '^[0-9a-f]{40}$' -or $manifest.patch_revision -notmatch '^pc[1-9][0-9]*$' -or $manifest.patch_commit -notmatch '^[0-9a-f]{40}$') { throw 'Invalid patch provenance.' }
if ($manifest.patch_file -ne 'proxy-chain.patch' -or $manifest.patch_sha256 -notmatch '^[0-9a-f]{64}$') { throw 'Invalid patch artifact.' }
$patchPath = [IO.Path]::GetFullPath((Join-Path (Split-Path -Parent $ManifestPath) $manifest.patch_file))
if ((Get-FileHash -LiteralPath $patchPath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $manifest.patch_sha256) { throw 'Patch hash mismatch.' }
$candidate = [IO.Path]::GetFullPath($CandidatePath)
$output = [IO.Path]::GetFullPath($OutputPath)
if ($output.TrimEnd([IO.Path]::DirectorySeparatorChar) -eq $candidate.TrimEnd([IO.Path]::DirectorySeparatorChar) -or $output.StartsWith($candidate.TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) { throw 'Keep build receipts outside the candidate source tree.' }
if (Test-Path -LiteralPath $candidate) { throw 'Candidate path already exists; use a new isolated directory.' }
New-Item -ItemType Directory -Path $candidate | Out-Null
Invoke-Git @('init', $candidate) | Out-Null
Invoke-Git @('-C', $candidate, 'fetch', '--no-tags', '--depth=1', $SourceRepository, "refs/tags/$UpstreamTag") | Out-Null
$actual = [string](Invoke-Git @('-C', $candidate, 'rev-parse', 'FETCH_HEAD^{commit}'))
if ($actual -ne $UpstreamCommit) { throw 'Fetched stable tag does not match the discovery commit.' }
Invoke-Git @('-C', $candidate, 'checkout', '--detach', $actual) | Out-Null
$candidateBranch = "pc-ci/$UpstreamTag-$($manifest.patch_revision)"
Invoke-Git @('-C', $candidate, 'switch', '-c', $candidateBranch) | Out-Null
# Three-way application requires the baseline blobs even when the new tag changed them.
Invoke-Git @('-C', $candidate, 'fetch', '--no-tags', '--depth=1', $SourceRepository, $manifest.base_commit) | Out-Null
$applyOutput = & git -C $candidate apply --3way --index -- $patchPath 2>&1
$applyExit = $LASTEXITCODE
$unmerged = @(& git -C $candidate diff --name-only --diff-filter=U)
if ($applyExit -ne 0 -or $unmerged.Count -gt 0) {
    # Keep the checkout and conflict paths for diagnosis. Never resolve conflicts here.
    $failure = [ordered]@{ schema=1; result='patch_conflict'; upstream_tag=$UpstreamTag; upstream_commit=$actual; patch_sha256=$manifest.patch_sha256; conflict_files=$unmerged; details=@($applyOutput | ForEach-Object { "$_" }) }
    $failure | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath $output -Encoding utf8
    throw 'The patch did not apply cleanly. No tests, build, or release are allowed.'
}
Invoke-Git @('-C', $candidate, 'diff', '--cached', '--check') | Out-Null
$paths = @(Invoke-Git @('-C', $candidate, 'diff', '--cached', '--name-only'))
if ($paths.Count -eq 0) { throw 'The patch changed no files.' }
foreach ($path in $paths) {
    if ($path -match '(^|/)(\.git|\.github|patches|node_modules|target)(/|$)' -or $path -match '(^|/)(api\.txt|\.env[^/]*)$') { throw "Functional patch changes a forbidden path: $path" }
}
$oldAuthorDate = $env:GIT_AUTHOR_DATE
$oldCommitterDate = $env:GIT_COMMITTER_DATE
try {
    # Identical upstream + patch must reproduce the tested commit in the build job.
    $env:GIT_AUTHOR_DATE = '2000-01-01T00:00:00+00:00'
    $env:GIT_COMMITTER_DATE = '2000-01-01T00:00:00+00:00'
    Invoke-Git @('-C', $candidate, '-c', 'user.name=ProxyChain Builder', '-c', 'user.email=proxy-chain@users.noreply.github.com', '-c', 'commit.gpgsign=false', 'commit', '-m', "Apply $($manifest.patch_revision) to $UpstreamTag") | Out-Null
} finally {
    $env:GIT_AUTHOR_DATE = $oldAuthorDate
    $env:GIT_COMMITTER_DATE = $oldCommitterDate
}
$candidateCommit = [string](Invoke-Git @('-C', $candidate, 'rev-parse', 'HEAD'))
$tree = [string](Invoke-Git @('-C', $candidate, 'rev-parse', 'HEAD^{tree}'))
$maintenance = [string](Invoke-Git @('-C', (Join-Path $PSScriptRoot '..'), 'rev-parse', 'HEAD'))
$metadata = [ordered]@{
    schema=1; result='applied'; upstream_repository=$manifest.upstream_repository; upstream_tag=$UpstreamTag; upstream_commit=$actual
    patch_revision=$manifest.patch_revision; patch_sha256=$manifest.patch_sha256; patch_commit=$manifest.patch_commit
    candidate_commit=$candidateCommit; candidate_branch=$candidateBranch; source_tree=$tree; maintenance_commit=$maintenance; changed_files=$paths
}
$metadata | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $output -Encoding utf8
Write-Output "Patch applied: $candidateCommit ($tree)"
