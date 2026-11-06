# 新建配置默认值

设置 → 配置默认值新增独立的创建设置：启动网址、页面语言、浏览器界面语言、时区、WebRTC 和六项噪声。首次缺省值为 `https://www.browserscan.net/zh/`、`zh-CN`、界面跟随配置；时区/WebRTC 保持自动，噪声保持真实。

`settings.json.profile_defaults` 只在创建时复制。普通新建、库指纹创建、API `get-new-fingerprint` / `POST /profiles` / temporary 及 runner 共用这组默认。API 提供的显式值优先，空 `startup_url` 表示不额外打开网址。导入和克隆保留来源值；修改默认设置不重写旧配置。

每个配置的 `startup_url` 单独保存，并可在编辑配置的基本页修改。启动器读取该值、校验 HTTP/HTTPS、`about:blank` 或空值，然后作为单个网址参数传给 Engine；不拼进 `extra_args`，不传入指纹 JSON。旧配置缺少字段时不补网址。语言仍使用现有 `navigator.language`、`accept_language`、`languages`、`icu_locale` 和 `ui_language` 机制。

定向验证：Rust 创建/显式覆盖、设置旧 JSON 缺省、克隆/导入不覆盖与启动 argv；Playwright 新建默认、修改保存重载、旧配置编辑保留。原生 Engine 页面导航由候选产物验收另行确认。
