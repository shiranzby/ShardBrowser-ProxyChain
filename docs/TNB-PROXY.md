# TNBProxy 提取

新增 `tnbproxy_extract` IPC，对应用户提供的 IP 白名单提取接口 `GET /gen`。默认服务器为 `http://usdata.adv.tnbproxy.com:9090`，套餐为 `advanced_traffic`；不使用 DevToken 或另一套 REST API。

参数包含 `plan_name`、`count`（1–1000）、`geo`（国家_州省_城市）、`ttl`（0 轮换，1–180 粘性）、可选 `pool` / `isp` / `asn` / `ip` / `sid`，以及可选 `pre=pre`。固定请求 `format=txt&split=rn`，协议选择决定预览的 HTTP / SOCKS5 类型；响应有显式不同协议时拒绝。服务器允许自定义 HTTP/HTTPS origin 或 `/gen`，拒绝用户信息、query、fragment 和其他路径。

出口必须明确选择：

- `direct`：禁用 reqwest 的系统和环境代理，直接请求；应将该公网出口加入 TNB 白名单。
- `first_hop`：按已保存第一层配置解析该服务器目标。此选择明确启用本次第一层路线，不依赖环境启动的全局链开关。经现有固定目标 CONNECT/SOCKS5 透明隧道；自动模式若拒绝或解析失败，提取失败，不回落直连。应将第一层访问该服务器时的实际公网出口加入白名单。

提取不探测公网出口、不测试代理、不保存代理，也不调用 change/clean。成功返回现有 `ProxyEntry[]` 供预览，再由已有 `proxy_bulk_save` 保存。notes 保留 TNB 标记、server、plan_name、sid、ttl、route，供后续管理定位；不猜造管理 URL。

单次总超时 25 秒、响应上限 512 KiB、最多两个并发请求。禁止 redirect；只接受逐行完整代理，错误页/JSON/部分非法结果整体拒绝。错误只返回固定 `tnb_*` code，不回显响应正文或请求 URL。HTTPS 第一层路线按原服务器主机验证 TLS，无证书跳过或明文降级。

提取测试只使用 loopback HTTP/CONNECT/TLS 夹具，覆盖编码、数量、响应上限、超时、重定向、错误响应、预览不落盘、CONNECT 拒绝不直连、HTTPS 不信任证书拒绝。未自动调用真实 TNB 账户提取、换 IP 或清理接口；真实白名单、余额、分配效果需用户操作验收。

## 公开地区目录

`tnbproxy_locations` 使用固定的官方 `POST https://server.tnbproxy.com/api/regions/list`，无需认证、Cookie、设备标识或账号读取。国家传 `{}`，州省传 `{"region":"EG"}`，城市传 `{"region":"EG","state":"Cairo"}`。不访问需要认证的 `domains/list`，也不调用 `/gen`。

请求类型为 `{server, route, level: "country" | "state" | "city", country?, state?}`，返回 `{items: [{code, label}], supported, source}`。`server` 仅用于识别 `tnbproxy.com` 或其子域；不会把它当作地区 API 地址。其他自定义服务器返回不支持，不套用官方地区列表。国家名称由界面按 ISO 代码显示中英名称；州省/城市保留提供商原码与别名，不合并 Cairo 与 AlQahirah。空代码由界面的“不限”选项表达，生成 `geo` 时省略，官方代码中的连字符按官网规则转换为下划线。

地区查询遵循本次 `route`，第一层失败不改走直连。响应上限 512 KiB，最多 10000 条原始记录，20 秒超时；HTTP 401/403 明确返回认证受限错误，不尝试取得浏览器认证。目录是官方通用分类，不代表某套餐当前库存；公开源码确认 `advanced_traffic` 与 `data` 复用该目录，未宣称所有其他套餐覆盖。

普通回归使用 loopback metadata 夹具。显式运行 `tnb_locations_public_egypt_contract_live` 只核验公开国家、埃及州省、Cairo 城市目录，默认忽略，不分配代理、不消耗提取端口。
