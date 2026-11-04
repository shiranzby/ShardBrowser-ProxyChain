[CmdletBinding()]
param(
    [string]$ManifestPath = (Join-Path $PSScriptRoot '../patches/manifest.json'),
    [string]$CompletedStatePath = (Join-Path $PSScriptRoot '../patches/last-success.json'),
    [Parameter(Mandatory)][string]$OutputPath,
    [switch]$ForceBuild,
    # Offline fixtures are explicit; scheduled workflows never set this option.
    [string]$FixtureDirectory
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

function Api([string]$Path) {
    $headers = @{ Accept = 'application/vnd.github+json'; 'User-Agent' = 'ShardBrowser-ProxyChain'; 'X-GitHub-Api-Version' = '2022-11-28' }
    if ($env:GH_TOKEN) { $headers.Authorization = "Bearer $env:GH_TOKEN" }
    try { Invoke-RestMethod -Uri "https://api.github.com/$Path" -Headers $headers -TimeoutSec 30 }
    catch { throw "GitHub request failed for $Path; no update decision was made." }
}

$manifest = Get-Content -Raw -LiteralPath $ManifestPath | ConvertFrom-Json
if ($manifest.schema -ne 1 -or $manifest.upstream_repository -ne 'ProxyShard/ShardBrowser') { throw 'Unsupported patch manifest.' }
if ($manifest.patch_revision -notmatch '^pc[1-9][0-9]*$') { throw 'Invalid patch revision.' }
if (-not $manifest.ready -or $manifest.patch_sha256 -notmatch '^[0-9a-f]{64}$') { throw 'The functional patch is not ready; finish and hash it before enabling upstream builds.' }
$patchPath = Join-Path (Split-Path -Parent $ManifestPath) $manifest.patch_file
if ($manifest.patch_file -ne 'proxy-chain.patch' -or -not (Test-Path -LiteralPath $patchPath -PathType Leaf)) { throw 'Missing functional patch.' }
if ((Get-FileHash -LiteralPath $patchPath -Algorithm SHA256).Hash.ToLowerInvariant() -ne $manifest.patch_sha256) { throw 'The functional patch hash does not match the manifest.' }

if ($FixtureDirectory) {
    if (Test-Path -LiteralPath (Join-Path $FixtureDirectory 'api-error.json')) { throw 'GitHub request failed (offline service-failure fixture); no update decision was made.' }
    $releases = @(Get-Content -Raw -LiteralPath (Join-Path $FixtureDirectory 'releases.json') | ConvertFrom-Json)
} else {
    $releases = @()
    # Do not let a large prerelease history hide a stable release.
    for ($page = 1; $page -le 100; $page++) {
        $batch = @(Api "repos/$($manifest.upstream_repository)/releases?per_page=100&page=$page")
        $releases += $batch
        if ($batch.Count -lt 100) { break }
        if ($page -eq 100) { throw 'Release pagination exceeded its bound; no update decision was made.' }
    }
}
$stable = @($releases | Where-Object { -not $_.draft -and -not $_.prerelease -and $_.tag_name -match '^v[0-9]+\.[0-9]+\.[0-9]+$' } | Sort-Object { [version]$_.tag_name.Substring(1) } -Descending)
if ($stable.Count -eq 0) { throw 'No formal stable release found; no update decision was made.' }
$tag = [string]$stable[0].tag_name
if ($FixtureDirectory) {
    $ref = Get-Content -Raw -LiteralPath (Join-Path $FixtureDirectory 'tag-ref.json') | ConvertFrom-Json
} else { $ref = Api "repos/$($manifest.upstream_repository)/git/ref/tags/$tag" }
if ($ref.ref -ne "refs/tags/$tag") { throw 'GitHub returned a different tag reference.' }
$object = $ref.object
$depth = 0
while ($object.type -eq 'tag') {
    if (++$depth -gt 8 -or $object.sha -notmatch '^[0-9a-f]{40}$') { throw 'Invalid annotated tag chain.' }
    $object = if ($FixtureDirectory) {
        (Get-Content -Raw -LiteralPath (Join-Path $FixtureDirectory "tag-$($object.sha).json") | ConvertFrom-Json).object
    } else { (Api "repos/$($manifest.upstream_repository)/git/tags/$($object.sha)").object }
}
if ($object.type -ne 'commit' -or $object.sha -notmatch '^[0-9a-f]{40}$') { throw 'Stable tag did not resolve to a commit.' }

$shouldBuild = $true
$reason = 'new upstream or patch revision'
if (Test-Path -LiteralPath $CompletedStatePath -PathType Leaf) {
    $previous = Get-Content -Raw -LiteralPath $CompletedStatePath | ConvertFrom-Json
    if ($previous.schema -ne 1 -or $previous.result -ne 'success' -or $previous.portable_sha256 -notmatch '^[0-9a-f]{64}$') { throw 'Successful build state is malformed.' }
    if ($previous.upstream_tag -eq $tag -and $previous.upstream_commit -ne $object.sha) { throw 'The upstream stable tag moved after a successful build; investigate instead of publishing automatically.' }
    if ($previous.upstream_tag -eq $tag -and $previous.upstream_commit -eq $object.sha -and $previous.patch_sha256 -eq $manifest.patch_sha256 -and $previous.patch_revision -eq $manifest.patch_revision) {
        $shouldBuild = $false
        $reason = 'same upstream commit and patch already built successfully'
    }
}
if ($ForceBuild) { $shouldBuild = $true; $reason = 'explicit rebuild requested' }
$result = [ordered]@{
    schema = 1; source = $(if ($FixtureDirectory) { 'fixture' } else { 'github' })
    checked_at_utc = [DateTime]::UtcNow.ToString('o'); upstream_repository = $manifest.upstream_repository
    upstream_tag = $tag; upstream_commit = $object.sha; patch_revision = $manifest.patch_revision
    patch_sha256 = $manifest.patch_sha256; should_build = $shouldBuild; reason = $reason
}
$result | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $OutputPath -Encoding utf8
Write-Output "$tag $($object.sha): $reason"
