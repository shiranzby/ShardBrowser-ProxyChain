# Proxy-chain 补丁维护流程

维护仓库：`shiranzby/ShardBrowser-ProxyChain`。官方仓库只作为上游来源，不向官方仓库推送。

## 版本和来源

补丁版本使用 `pc1`、`pc2` 递增；上游正式 tag 和上游 commit 单独记录。Portable 文件名为：

```text
ShardBrowser-v2.1.2-pc1-portable-win-x64.exe
```

补丁 manifest 在 `patches/manifest.json`。只有 `ready=true`、`proxy-chain.patch` 的 SHA256 与 manifest 一致、补丁提交 SHA 合法时，自动流程才会继续。补丁尚未准备好时，`check-upstream.ps1` 会失败，不会把失败误判为“没有上游更新”。

## 自动流程

1. `upstream-watch.yml` 每日检查官方正式 Release，忽略 draft/prerelease，并解析带注释 tag 到不可变 commit。
2. 新上游或新补丁触发 `upstream-rebase.yml`。它在临时 checkout 中创建 `pc-ci/<upstream-tag>-<patch-revision>` 临时分支，再使用 `git apply --3way` 应用补丁；冲突、未合并文件、空补丁或禁改路径会停止流程，并保留诊断 artifact。临时分支仅存在于 runner，不向官方仓库推送；固定父提交、补丁树、提交身份/时间使验证与构建 job 得到相同 candidate commit。
3. 通过 Rust、UI、生产构建和 Playwright 门槛后调用 `build-portable.yml`。构建 job 会重新应用同一上游 commit 和补丁，确认 candidate commit/tree 与测试收据一致，再运行 `npx tauri build --no-bundle --target x86_64-pc-windows-msvc`。它保留官方 Tauri/Rust/UI 构建路径，仅跳过本任务不需要的 WiX/NSIS 打包器，避免网络下载打包器阻断已经成功的 Portable 编译。
4. 原始 Cargo package launcher EXE 从 `src-tauri/target/x86_64-pc-windows-msvc/release` 收集；若设置 `CARGO_TARGET_DIR` 则按绝对值或相对候选源码目录解析。不会把 MSI/NSIS 安装包或上次遗留的 product-name EXE 改名为 Portable。产物附 `.sha256`、`build-info.json` 和测试日志。
5. 没有真实 Windows 11 引擎验收收据时，产物仅标为 candidate artifact。只有 `proxy-chain-engine-acceptance` 收据与同一 candidate、源码树、Portable SHA256、引擎版本/构建/hash 完全匹配，并通过所有 Fail Closed、出口、UDP/QUIC/WebRTC、数据兼容项时，才允许后续发布 job。

## 工具门槛

完整自动化 feature 默认启用。Windows job 明确检查 `cmake`、`nasm`、`go` 和 LLVM 的 `libclang.dll`；缺失时尝试安装 NASM，并发现标准安装位置的 `LIBCLANG_PATH`，仍缺失就失败。不得用 `--no-default-features` 绕过依赖后把结果当作正式 Portable。

## 冲突处理

冲突不会自动猜测解决。检查 `proxy-chain-verification` artifact 的 `candidate-info.json`、冲突文件和日志；维护者更新补丁、patch SHA 和 revision 后重新触发。开放 Issue 使用固定标题去重。

`patches/last-success.json` 仅在构建和 artifact 上传均成功后更新。相同上游 commit/补丁 hash/revision 不重复构建；Actions artifact 过期需要下载时，可手动勾选 `force_rebuild`。它记录“已成功生成候选产物”，不宣称真实引擎验收通过。公开可下载的 artifact 是本项目当前自动交付渠道；正式 Fork Release 仍需另行接入同一严格验收门槛。

## 本地演练

在 Windows PowerShell 中：

```powershell
./scripts/test-proxy-chain-pipeline.ps1
./scripts/check-upstream.ps1 -OutputPath $env:TEMP/discovery.json
./scripts/apply-patch.ps1 -UpstreamTag v2.1.2 -UpstreamCommit <40-hex-commit> `
  -CandidatePath $env:TEMP/shardbrowser-candidate `
  -OutputPath $env:TEMP/shardbrowser-proofs/candidate-info.json
./scripts/verify-patch.ps1 -CandidatePath $env:TEMP/shardbrowser-candidate `
  -MetadataPath $env:TEMP/shardbrowser-proofs/candidate-info.json `
  -OutputDirectory $env:TEMP/shardbrowser-proofs/verification
./scripts/build-portable.ps1 -CandidatePath $env:TEMP/shardbrowser-candidate `
  -MetadataPath $env:TEMP/shardbrowser-proofs/candidate-info.json `
  -VerificationPath $env:TEMP/shardbrowser-proofs/verification/verification.json `
  -OutputDirectory $env:TEMP/shardbrowser-proofs/artifact
```

离线脚本使用新建的合成 Git 仓库和模拟 Release 数据，验证稳定版选择、无更新、注释 tag、移动 tag、服务故障、补丁重放身份、冲突停止和缺失证明拒绝。它不访问真实代理，不生成假正式验收收据，也不构建 EXE。

`<40-hex-commit>` 必须来自同一次官方 Release 查询。不要把 API token、代理用户名/密码、认证头、真实 Profile 数据或 Cookie 放入日志、artifact、manifest 或仓库。
