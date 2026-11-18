# ShyShard

ShyShard 是基于官方 ShardBrowser `v2.1.5` 的 Windows 指纹浏览器启动器分支。它保留上游 Profile、指纹、扩展、Local API、MCP 和浏览器引擎能力，并加入面向实际 Windows 使用的代理链、供应商配置和启动管理能力。

[English README](README.en.md) · [首个正式发布 v1.0-based2.1.5](https://github.com/shiranzby/ShardBrowser-X/releases/tag/v1.0-based2.1.5) · [更新说明](docs/RELEASE_NOTES_v1.0-based2.1.5.md)

## 相比上游增加的能力

- **Windows 代理链**：环境可选择系统代理、已保存代理或手动 HTTP、HTTPS、SOCKS5 第一层，并与第二层代理统一生成路由计划。
- **真实网页正文验证**：代理检测要求读取成功的 HTTP 正文，不能只凭 TCP、出口 IP 或地区信息判定可用。
- **稳定启动路径**：检测与启动复用已准备好的隧道，失败时保持 fail-closed，不静默降级为直连。
- **供应商配置**：TNBProxy、1024Proxy 和自定义供应商可横向切换，API 提取凭据与代理出口凭据分开保存，并使用 Windows 当前用户 DPAPI 保护。
- **地区选择**：国家支持 ISO 两字母代码和本地化名称搜索，例如输入 `EG` 可选择埃及。供应商未提供可信州/城市库存时允许填写原始值，不伪造官方列表。
- **Profile 快捷方式**：从 Profile 菜单创建桌面快捷方式，快捷方式只保存 Profile ID，并通过正常单实例启动路径静默启动。
- **扩展多选**：新建 Profile 时可在扩展页一次勾选多个已安装扩展。
- **界面调整**：首页主题快捷控件、代理状态快捷控件、通用“代理供应商”入口、代理国家/地区和测试状态展示、启动营销弹窗移除。
- **上游同步机制**：补丁从指定上游基线生成，Portable 构建、来源身份和 SHA256 都留有可复核收据。

## 协议边界

当前实际支持 HTTP proxy、HTTPS CONNECT over TLS 和明文 SOCKS5。SOCKS5 over TLS、Hysteria2/HY2、VLESS、REALITY 和 QUIC outbound 尚未作为已实现能力发布，不会用端口号推断协议。

HK_NAT_VPS 存在 IPv4/NAT 线路限制，稳定测试中不作为单层或第一层基准，仅保留为链式第二层候选。

## 首个实际交付

- 发布标签：`v1.0-based2.1.5`
- 上游基线：`v2.1.5`
- 产品显示版本：`2.1.5-pc17`
- 目标平台：Windows x64 Portable
- 发布内容：Portable 可执行文件、SHA256、构建身份和中文更新说明

## 使用与问题反馈

请先阅读 [首个版本更新说明](docs/RELEASE_NOTES_v1.0-based2.1.5.md)。问题反馈和后续版本在 [GitHub 仓库](https://github.com/shiranzby/ShardBrowser-X) 提交。

上游项目和许可证信息请查看 [English README](README.en.md) 与 [LICENSE](LICENSE)。
