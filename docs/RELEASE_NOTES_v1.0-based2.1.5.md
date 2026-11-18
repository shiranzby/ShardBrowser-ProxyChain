# v1.0-based2.1.5 更新说明

这是 ShardBrowser-X 的第一个实际使用交付版本，基于官方 ShardBrowser `v2.1.5` 构建。产品界面显示版本为 `2.1.5-pc17`，GitHub 发布标签为 `v1.0-based2.1.5`。

## 本版本新增

- Windows 代理链统一配置，支持系统代理、已保存代理、手动 HTTP、HTTPS、SOCKS5 第一层与第二层组合。
- 检测和启动复用准备好的隧道，网页正文验证要求 HTTP 成功且正文非空。
- TNBProxy、1024Proxy、自定义供应商横向切换和默认配置保存。
- TNB API 提取凭据与代理出口凭据分开保存，白名单错误提供明确提示。
- 1024Proxy 国家代码搜索，输入 `EG` 可选择埃及。
- Profile 桌面快捷方式和静默启动参数。
- 新建 Profile 时扩展多选。
- 首页主题、代理状态和代理链快捷控件。
- 代理列表展示国家缩写、地区、测试结果、出口 IP 和 UDP 状态。
- 移除启动时的 100 MB 高级住宅流量营销弹窗。
- 默认关闭 Web Store 扩展更新检查。
- 设置中的关于页指向 ShardBrowser-X 仓库。

## 兼容与边界

当前发布能力包括 HTTP proxy、HTTPS CONNECT over TLS 和明文 SOCKS5。SOCKS5 over TLS、HY2、VLESS、REALITY 和 QUIC outbound 不属于本版本已实现功能。

HK_NAT_VPS 的 IPv4/NAT 线路不稳定，不作为单层或第一层稳定测试节点。它仍可在链式代理中作为第二层候选。

## 来源

本版本基于官方仓库的 `v2.1.5`，本分支额外改动通过 `pc17` 补丁维护。完整来源、构建身份和 SHA256 见发布附件中的 `build-info.json`、`artifact-index.json` 和校验文件。
