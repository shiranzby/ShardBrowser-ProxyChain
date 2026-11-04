[CmdletBinding()]
param([string]$OutputDirectory)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot 'lib/proxy-chain-build.ps1')
if (-not $OutputDirectory) { $OutputDirectory = Join-Path ([IO.Path]::GetTempPath()) ("proxy-chain-pipeline-test-" + [Guid]::NewGuid().ToString('N')) }
$root = [IO.Path]::GetFullPath($OutputDirectory)
if (Test-Path -LiteralPath $root) { throw 'Offline test output must be a new directory.' }
New-Item -ItemType Directory -Path $root | Out-Null
$results = [Collections.Generic.List[object]]::new()
function Invoke-Git([Parameter(ValueFromRemainingArguments=$true)][string[]]$Arguments) {
    $out = & git @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) { throw "Synthetic git operation failed: $($out -join "`n")" }
    $out
}
function Json([string]$Path, $Value) { $Value | ConvertTo-Json -Depth 15 | Set-Content -LiteralPath $Path -Encoding utf8 }
function Assert([bool]$Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
function Case([string]$Name, [scriptblock]$Body) {
    try { & $Body; $results.Add(@{id=$Name;status='passed'}); Write-Output "PASS $Name" }
    catch { $results.Add(@{id=$Name;status='failed';message=$_.Exception.Message}); throw }
}
function Refused([scriptblock]$Body, [string]$Pattern) {
    $rejected = $false
    try { & $Body | Out-Null } catch { $rejected = $true; Assert ($_.Exception.Message -match $Pattern) "Unexpected refusal: $($_.Exception.Message)" }
    Assert $rejected 'Operation was not refused.'
}
$upstream = Join-Path $root 'synthetic-upstream'
$fixtures = Join-Path $root 'fixtures'
$patchDir = Join-Path $root 'patches'
New-Item -ItemType Directory -Path $upstream,$fixtures,$patchDir | Out-Null
Invoke-Git init $upstream | Out-Null
Invoke-Git -C $upstream config user.name 'Synthetic test' | Out-Null
Invoke-Git -C $upstream config user.email 'synthetic@example.invalid' | Out-Null
New-Item -ItemType Directory -Path (Join-Path $upstream 'src-tauri') | Out-Null
"[package]`nname = `"synthetic-launcher`"`nversion = `"2.1.2`"" | Set-Content (Join-Path $upstream 'src-tauri/Cargo.toml') -Encoding utf8
"original`nsecond line" | Set-Content (Join-Path $upstream 'feature.txt') -Encoding utf8
Invoke-Git -C $upstream add . | Out-Null
Invoke-Git -C $upstream -c commit.gpgsign=false commit -m baseline | Out-Null
$base = [string](Invoke-Git -C $upstream rev-parse HEAD)
Invoke-Git -C $upstream tag v2.1.2 | Out-Null
"patched`nsecond line" | Set-Content (Join-Path $upstream 'feature.txt') -Encoding utf8
Invoke-Git -C $upstream add . | Out-Null
Invoke-Git -C $upstream -c commit.gpgsign=false commit -m patch | Out-Null
$patchCommit = [string](Invoke-Git -C $upstream rev-parse HEAD)
$patch = Join-Path $patchDir 'proxy-chain.patch'
Invoke-Git -C $upstream diff --binary $base $patchCommit "--output=$patch" | Out-Null
$hash = (Get-FileHash -LiteralPath $patch -Algorithm SHA256).Hash.ToLowerInvariant()
$manifest = @{schema=1;upstream_repository='ProxyShard/ShardBrowser';patch_revision='pc1';base_tag='v2.1.2';base_commit=$base;patch_file='proxy-chain.patch';patch_sha256=$hash;patch_commit=$patchCommit;ready=$true}
$manifestPath = Join-Path $patchDir 'manifest.json'
Json $manifestPath $manifest
$state = Join-Path $root 'success.json'
$discovery = Join-Path $root 'discovery.json'
function Fixtures([string]$Tag, [string]$Commit, [bool]$Annotated=$false) {
    Json (Join-Path $fixtures 'releases.json') @(@{tag_name='v999.0.0-beta';draft=$false;prerelease=$true},@{tag_name='v999.0.0';draft=$true;prerelease=$false},@{tag_name=$Tag;draft=$false;prerelease=$false})
    if ($Annotated) {
        $tagObject = '1234567890123456789012345678901234567890'
        Json (Join-Path $fixtures 'tag-ref.json') @{ref="refs/tags/$Tag";object=@{type='tag';sha=$tagObject}}
        Json (Join-Path $fixtures "tag-$tagObject.json") @{object=@{type='commit';sha=$Commit}}
    } else { Json (Join-Path $fixtures 'tag-ref.json') @{ref="refs/tags/$Tag";object=@{type='commit';sha=$Commit}} }
}
function Discover {
    & (Join-Path $PSScriptRoot 'check-upstream.ps1') -ManifestPath $manifestPath -CompletedStatePath $state -FixtureDirectory $fixtures -OutputPath $discovery
}
try {
    Fixtures 'v2.1.2' $base
    Case 'new-stable-is-built-and-prerelease-is-ignored' {
        Discover
        $d = Get-Content -Raw $discovery | ConvertFrom-Json
        Assert ($d.should_build -and $d.upstream_tag -eq 'v2.1.2' -and $d.upstream_commit -eq $base) 'Wrong stable selection.'
    }
    Json $state @{schema=1;result='success';upstream_tag='v2.1.2';upstream_commit=$base;patch_revision='pc1';patch_sha256=$hash;portable_sha256=('a'*64)}
    Case 'no-update-does-not-rebuild' {
        Discover
        Assert (-not (Get-Content -Raw $discovery | ConvertFrom-Json).should_build) 'Same successful tuple was rebuilt.'
    }
    Case 'annotated-tag-resolves-to-commit' {
        Fixtures 'v2.2.0' $base $true
        Discover
        $d = Get-Content -Raw $discovery | ConvertFrom-Json
        Assert ($d.should_build -and $d.upstream_tag -eq 'v2.2.0' -and $d.upstream_commit -eq $base) 'Annotated tag not resolved.'
    }
    Case 'moved-tag-is-refused' {
        Fixtures 'v2.1.2' $patchCommit
        Refused { Discover } 'tag moved'
    }
    Case 'api-failure-is-not-no-update' {
        Json (Join-Path $fixtures 'api-error.json') @{status=503}
        Refused { Discover } 'no update decision was made'
        # Rename only a test-owned fixture; retain the failure input as evidence.
        Move-Item -LiteralPath (Join-Path $fixtures 'api-error.json') -Destination (Join-Path $fixtures 'api-error.used.json')
    }
    Case 'functional-patch-not-ready-is-refused' {
        $notReady = $manifest.Clone(); $notReady.ready=$false
        $p=Join-Path $patchDir 'not-ready.json'; Json $p $notReady
        Refused { & (Join-Path $PSScriptRoot 'check-upstream.ps1') -ManifestPath $p -FixtureDirectory $fixtures -OutputPath $discovery } 'not ready'
    }
    $candidate1=Join-Path $root 'candidate-1'; $candidate2=Join-Path $root 'candidate-2'
    $meta1=Join-Path $root 'candidate-1.json'; $meta2=Join-Path $root 'candidate-2.json'
    Case 'patch-applies-to-isolated-upstream' {
        & (Join-Path $PSScriptRoot 'apply-patch.ps1') -ManifestPath $manifestPath -UpstreamTag v2.1.2 -UpstreamCommit $base -CandidatePath $candidate1 -OutputPath $meta1 -SourceRepository $upstream
        Assert ((Get-Content -Raw (Join-Path $candidate1 'feature.txt')) -match '^patched') 'Patch content not applied.'
        Assert (@(& git -C $candidate1 status --porcelain=v1).Count -eq 0) 'Applied candidate not clean.'
        $receipt=Get-Content -Raw $meta1 | ConvertFrom-Json
        $branch=([string](& git -C $candidate1 branch --show-current)).Trim()
        Assert ($branch -eq 'pc-ci/v2.1.2-pc1' -and $receipt.candidate_branch -eq $branch) 'The isolated build did not create its deterministic temporary branch.'
    }
    Case 'patch-replay-has-identical-commit-and-tree' {
        & (Join-Path $PSScriptRoot 'apply-patch.ps1') -ManifestPath $manifestPath -UpstreamTag v2.1.2 -UpstreamCommit $base -CandidatePath $candidate2 -OutputPath $meta2 -SourceRepository $upstream
        $a=Get-Content -Raw $meta1 | ConvertFrom-Json; $b=Get-Content -Raw $meta2 | ConvertFrom-Json
        Assert ($a.candidate_commit -eq $b.candidate_commit -and $a.source_tree -eq $b.source_tree) 'Replayed candidate identity changed.'
        Assert ($a.candidate_branch -eq $b.candidate_branch) 'Replayed candidate branch identity changed.'
    }
    Case 'patch-conflict-stops-with-conflict-evidence' {
        Invoke-Git -C $upstream checkout --detach $base | Out-Null
        "upstream changed the same line`nsecond line" | Set-Content (Join-Path $upstream 'feature.txt') -Encoding utf8
        Invoke-Git -C $upstream add . | Out-Null
        Invoke-Git -C $upstream -c commit.gpgsign=false commit -m conflict | Out-Null
        Invoke-Git -C $upstream tag v2.2.0 | Out-Null
        $sha=[string](Invoke-Git -C $upstream rev-parse HEAD)
        $conflict=Join-Path $root 'conflict.json'
        Refused { & (Join-Path $PSScriptRoot 'apply-patch.ps1') -ManifestPath $manifestPath -UpstreamTag v2.2.0 -UpstreamCommit $sha -CandidatePath (Join-Path $root 'conflict') -OutputPath $conflict -SourceRepository $upstream } 'did not apply cleanly'
        $c=Get-Content -Raw $conflict | ConvertFrom-Json
        Assert ($c.result -eq 'patch_conflict' -and 'feature.txt' -in $c.conflict_files) 'Conflict file evidence missing.'
    }
    Case 'portable-build-rejects-incomplete-test-receipt' {
        $m=Get-Content -Raw $meta1 | ConvertFrom-Json
        $incomplete=Join-Path $root 'incomplete-verification.json'
        Json $incomplete @{schema=1;kind='proxy-chain-verification';result='passed';candidate_commit=$m.candidate_commit;source_tree=$m.source_tree;patch_sha256=$hash;checks=@()}
        Refused { & (Join-Path $PSScriptRoot 'build-portable.ps1') -CandidatePath $candidate1 -MetadataPath $meta1 -VerificationPath $incomplete -OutputDirectory (Join-Path $root 'refused-portable') } 'Missing verification gate'
    }
    Case 'portable-build-rejects-test-source-mismatch' {
        $m=Get-Content -Raw $meta1 | ConvertFrom-Json
        $wrong=Join-Path $root 'mismatched-verification.json'
        Json $wrong @{schema=1;kind='proxy-chain-verification';result='passed';candidate_commit=('b'*40);source_tree=$m.source_tree;patch_sha256=$hash;checks=@()}
        Refused { & (Join-Path $PSScriptRoot 'build-portable.ps1') -CandidatePath $candidate1 -MetadataPath $meta1 -VerificationPath $wrong -OutputDirectory (Join-Path $root 'refused-source') } 'does not cover this candidate'
    }
    Case 'verification-output-cannot-equal-active-source-root' {
        Refused { & (Join-Path $PSScriptRoot 'verify-patch.ps1') -CandidatePath $candidate1 -MetadataPath $meta1 -OutputDirectory $candidate1 } 'outside the candidate source tree'
        Assert (@(& git -C $candidate1 status --porcelain=v1).Count -eq 0) 'Verification wrote into active source.'
    }
    Case 'portable-output-cannot-equal-active-source-root' {
        Refused { & (Join-Path $PSScriptRoot 'build-portable.ps1') -CandidatePath $candidate1 -MetadataPath $meta1 -VerificationPath (Join-Path $root 'incomplete-verification.json') -OutputDirectory $candidate1 } 'outside the candidate source tree'
        Assert (@(& git -C $candidate1 status --porcelain=v1).Count -eq 0) 'Portable build wrote into active source.'
    }
    Case 'patch-receipt-cannot-equal-source-root' {
        $target=Join-Path $root 'refused-receipt-source'
        Refused { & (Join-Path $PSScriptRoot 'apply-patch.ps1') -ManifestPath $manifestPath -UpstreamTag v2.1.2 -UpstreamCommit $base -CandidatePath $target -OutputPath $target -SourceRepository $upstream } 'outside the candidate source tree'
        Assert (-not (Test-Path -LiteralPath $target)) 'Patch script created a rejected source path.'
    }
    Case 'artifact-label-follows-the-actual-patch-revision' {
        # The workflow expression is a contract: GitHub resolves this field from
        # the actual build-info step, not a hard-coded pc1 label.
        $workflow=Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot '../.github/workflows/build-portable.yml')
        Assert ($workflow.Contains('name: ShardBrowser-${{ inputs.upstream_tag }}-${{ steps.meta.outputs.patch_revision }}-portable-win-x64-candidate')) 'Artifact name does not use the built patch revision.'
        Assert ($workflow.Contains('"patch_revision=$($r.patch_revision)" >> $env:GITHUB_OUTPUT')) 'Artifact revision is not emitted from build-info.'
    }
    Case 'portable-build-never-enters-an-installer-bundler' {
        $script=Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'build-portable.ps1')
        Assert ($script.Contains('& npx tauri build --no-bundle --target x86_64-pc-windows-msvc')) 'Portable compilation still invokes installer bundling.'
        Assert ($script.Contains("build_command='npx tauri build --no-bundle --target x86_64-pc-windows-msvc'")) 'Build receipt omits the actual no-bundle command.'
    }
    Case 'cargo-target-directory-default-relative-and-absolute' {
        $suffix='x86_64-pc-windows-msvc/release'
        Assert ((Get-ProxyChainReleaseDirectory $candidate1 '') -eq (Join-Path $candidate1 "src-tauri/target/$suffix")) 'Default Cargo target path changed.'
        Assert ((Get-ProxyChainReleaseDirectory $candidate1 'build-output') -eq (Join-Path $candidate1 "build-output/$suffix")) 'Relative Cargo target directory is not relative to the candidate working directory.'
        $outside=Join-Path $root 'shared-target'
        Assert ((Get-ProxyChainReleaseDirectory $candidate1 $outside) -eq (Join-Path $outside $suffix)) 'Absolute CARGO_TARGET_DIR was ignored.'
    }
    Case 'silent-success-still-has-a-hashed-step-log' {
        $receipt = Invoke-ProxyChainLoggedCommand -Name 'silent-success' -Command 'pwsh' -Arguments @('-NoProfile','-NonInteractive','-Command','exit 0') -OutputDirectory $root
        $log = Join-Path $root $receipt.log
        Assert ($receipt.status -eq 'passed' -and (Test-Path -LiteralPath $log -PathType Leaf)) 'A successful silent tool has no step evidence.'
        Assert ((Get-Item -LiteralPath $log).Length -eq 0) 'Silent tool log was populated with fabricated output.'
        Assert ($receipt.sha256 -eq (Get-FileHash -LiteralPath $log -Algorithm SHA256).Hash.ToLowerInvariant()) 'Silent tool log hash did not match its actual bytes.'
    }
    Case 'silent-failure-preserves-log-and-blocks-the-gate' {
        Refused { Invoke-ProxyChainLoggedCommand -Name 'silent-failure' -Command 'pwsh' -Arguments @('-NoProfile','-NonInteractive','-Command','exit 7') -OutputDirectory $root } 'failed with exit code 7'
        Assert (Test-Path -LiteralPath (Join-Path $root 'silent-failure.log') -PathType Leaf) 'Failure log was not preserved.'
    }
} finally {
    Json (Join-Path $root 'offline-test-results.json') @{schema=1;kind='proxy-chain-pipeline-offline-test';completed_at_utc=[DateTime]::UtcNow.ToString('o');fixture_only=$true;results=@($results.ToArray())}
    Write-Output "Offline evidence: $root"
}
