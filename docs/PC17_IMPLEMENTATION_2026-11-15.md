# PC17 实施记录（2026-11-15）

## 范围

本轮按 `docs/STABLE_VERSION_DESIGN_2026-11-15.md` 开始实施，优先完成网页正文验证和 1024Proxy 地区选择。HK_NAT_VPS 不作为单层或第一层稳定性基准；没有修改 VPS、Sparkle 或系统代理。

## 已实施

### 网页正文验证

- 代理链 readiness verifier 对 HTTPS 目标读取有界正文，要求 HTTP 成功且正文非空。
- 读取上限保持 64 KiB，避免测试或异常响应无限增长。
- 准备好的代理链复用同一 Tunnel 做正文验证，不把固定目标 Tunnel 当作普通 HTTP proxy 再次握手。
- `PreparedProxy` 保存实际读取字节数，代理快照通过 `content.bytes` 返回真实值。
- 失败时保持 fail closed，不用 TCP、Geo 或出口 IP 单独冒充网页可用。

### 1024Proxy 地区选择

- 国家选择器支持完整 ISO 两字母代码和本地化名称搜索，输入 `EG` 可定位埃及。
- 选择国家后清空州/省，避免沿用失效的父级选择。
- 州/省与城市未伪造供应商库存；在供应商没有可靠字典时保留手填原始值，并在界面中说明。
- 后端保留供应商国家代码校验、地区用户名模板和独立凭据配置。

## 验证

- Rust 无默认 feature 完整回归：`639 passed; 0 failed; 14 ignored`。
- readiness 定向测试：`9 passed; 0 failed`。
- 前端单元测试：`128 passed; 0 failed`。
- TypeScript 编译：通过。
- Vite 已完成 7043 个模块转换，但 Windows esbuild 删除临时文件时返回 `Access is denied`；这属于当前构建环境门禁，不能记为前端构建成功，也未发现源码编译错误。
- 默认 feature Rust 构建仍受环境缺少 CMake 阻塞，尚未生成 PC17 稳定发布包。

## 仍未宣称完成

正式稳定包、默认 feature 构建、真实新 EXE 原生验收和发布前全包秘密扫描仍需单独完成。当前提交是 PC17 实施阶段，不等同于稳定版交付。
