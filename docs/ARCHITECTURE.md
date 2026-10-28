# 架构与迁移

当前项目从空目录独立建立；原 Swift 仓库保留，参考目录仅用于架构分析与恢复本地依赖。仅支持 macOS、Windows，不包含 Linux、Android 或 iOS 入口。

## 分层

| 模块 | 职责 |
| --- | --- |
| `src/App.tsx`、`src/components/` | 单页连接表单、唯一高级设置折叠区、证书确认、中英文 UI |
| `src/hooks/useAppearance.ts` | 亮／暗／跟随系统三档偏好、本地记忆、系统外观监听与原生窗口外观同步 |
| `src/hooks/useGateway.ts` | 草稿、保存/连接编排、状态刷新、取消与过期结果隔离 |
| `src/lib/tauri.ts` | 类型明确的前端命令接口 |
| `src-tauri/src/lib.rs` | Tauri 命令、单实例、窗口驻留、平台入口 |
| `tray.rs` | 动态状态栏菜单：连接、检测、停止、复制、GitHub；状态同步与中英文标签 |
| `lightweight.rs`、`useLightweightMode.ts` | 仅托盘模式、窗口销毁／重建、唤起去重、草稿与进行中操作保护 |
| `runtime.rs` | 监听器、启动/验证/停止状态、取消、端口错误、信任失效后的停止 |
| `gateway.rs` | localhost HTTPS 路由、认证、Host/Origin/CORS、请求限制、上游传输、探针缓存 |
| `transport.rs` | 脱敏错误分类、请求编号、连接阶段的受限恢复、跨重试的总期限 |
| `upstream_errors.rs` | 将 HTTP／SSE 上游错误映射为固定提示与安全类别，保留单向指纹用于关联 |
| `protocol.rs`、`stream.rs` | Anthropic ↔ OpenAI 转换、工具/图片/推理参数、SSE 增量转换 |
| `providers.rs` | 供应商请求兼容：DeepSeek 工具会话保持非思考模式；Kimi 官方指定型号的受限工具选择与结果校验 |
| `config.rs`、`paths.rs` | 多供应商 profile、活动 profile 切换、配置验证、原子写入、文件权限、旧版 Swift 字段兼容 |
| `credentials.rs`、`certificate.rs` | 系统凭据、用户级证书信任、证书生成与删除 |

借鉴 `cc-switch` 的模块分层与原生桌面生命周期，不引入其无关的 CLI 管理、用量数据库、同步和技能管理功能。

## 行为

- 编辑草稿独立于运行状态。刷新不会丢失草稿；运行时禁止修改配置，复制始终基于已保存配置。
- 供应商以 profile 保存在 `settings.json` 中；每个 profile 有独立名称、API 配置和系统凭据库条目。旧版单供应商配置自动迁移为 `default` profile。停止网关后才能切换 profile；切换不会把未保存草稿带到另一个 profile。
- 保存不会检测上游；连接显式执行两次短请求。原有 `x`、`.` 非流式探针仅在真实成功后缓存，普通 Office 请求不缓存。
- 密钥留空表示沿用系统中已保存的密钥；复制隐藏的网关令牌由 Rust 直接写入剪贴板。
- 停止既关闭监听器，也取消当前检测。前端与 Rust 均隔离旧结果；保存尚未完成时取消也不会随后启动新监听器。
- 流式回复边接收边转换。正文在请求生命周期内暂存，不写磁盘；工具参数保持对象类型，截图会保留到 OpenAI 的多模态输入。
- 流式供应商超时按连续无数据的读取间隔计时，正常长回复不受总时长截断；连接阶段最多等待 30 秒，非流式请求及检测仍保留配置的总时限。每 10 秒向 Office 发送标准 Anthropic `ping`，避免供应商只有注释／推理事件时下游长时间无响应；下游心跳不重置上游读取超时。传输失败日志只记录阶段、耗时和是否超时，不含 URL、认证或正文。
- Host、Origin、认证、预检方法和请求头均校验。上游重定向不自动跟随，错误文本不包含上游密钥或返回正文。
- 只有明确处于 HTTP 请求发送前的临时建连失败才延迟 250 毫秒重试一次；证书、代理、超时、HTTP 拒绝、已发送请求与响应流中断均不重放。非流式总期限不会随重试重置。日志只输出固定错误类别、系统错误码和自生成请求编号；`x-office-gateway-request-id` 可与错误体关联。上游错误响应仍保留 `Retry-After` 和供应商请求编号。`/health` 的 `transport_revision` 用于辨别实际运行构建。
- 上游 SSE 错误（包括 `response.failed`）和 HTTP 错误只通过固定提示返回，不透传供应商错误正文。HTTP 诊断正文最多读取 16 KiB、等待两秒，异常或超限时仍保留原 HTTP 状态和重试头。失败日志包含安全类别、单向指纹、请求字节数、历史轮数、工具数和输出上限，不保存请求／错误正文。错误提示本身附带网关请求编号，保证 Office 仅复制 `error.message` 时也能关联。
- 安装／检查／卸载集中在同一证书卡片；右侧引导按「安装加载项 → 选择 Gateway 和 API 格式／认证 → 粘贴三个参数」完成接入。
- 外观偏好以 `office-gateway-appearance` 保存在当前客户端，默认跟随系统；CSS 配色与 Tauri 原生窗口主题一起切换，不改动操作系统全局外观。
- 轻量模式只释放窗口和 WebView，Rust 网关与托盘保持运行；统一恢复入口处理托盘、macOS Reopen、重复启动，窗口销毁中到达的恢复请求排到 Destroyed 后。仅无窗口触发的隐式退出被阻止，显式退出和更新重启保持原有清理流程。
- localhost 证书为 `CA:FALSE`、ServerAuth、SAN localhost/127.0.0.1/::1，有效期一年。macOS 信任限定 SSL localhost；Windows 使用当前用户证书存储并按指纹删除。证书不能按端口限定；`CA:FALSE` 也不能作为跨平台信任锚约束的绝对保证，详见 `CERTIFICATE_SCOPE.md`。

## 提交阶段

1. `e77687c`：建立 Tauri/React 桌面骨架与初版交互。
2. `aee773d`：迁移 Rust 配置、凭据、证书和网关核心。
3. `a4ec331`：完善协议兼容、增量流、生命周期、独立依赖、中英文交互和回归测试。
4. `f232bf1`：按反馈改为精简单页，移除导航和重复高级设置入口，增加 GitHub 图标与动态托盘操作。
5. `01a0c34`：macOS 发布包验证、构建脚本及 Windows 验证步骤。
6. `41d6013`：证书不能按端口限定的调研，及跨端口／无关证书的真实 TLS 回归验证。
7. `529513d`：引导式单页、清晰的 Office 安装与接入步骤、集中的证书管理。
8. `239d1f1`：亮／暗／跟随系统三档外观、偏好持久化、系统动态响应、原生主题同步。

## 旧版数据

macOS 沿用原 `Claude Office Gateway/settings.json`、`TLS/` 和 `com.miofelix.office-gateway` 钥匙串服务。配置反序列化接受旧 camelCase 字段；首次写入前保留 Swift 配置备份。原 PKCS#12 证书身份保留，启动时按需提取 PEM 私钥，目录与私钥分别使用 `0700`、`0600` 权限。

## 测试隔离

`OFFICE_GATEWAY_DATA_DIR` 可指定独立数据目录。仅在同时设置 `OFFICE_GATEWAY_TEST_MODE=1` 时，桌面测试使用内存凭据，避免访问日常使用的钥匙串。此模式仍要求真实证书信任才能启动 UI 网关，不绕过 TLS 或认证。

Rust 集成测试另行生成临时证书，客户端显式信任该测试证书，使用真实 localhost TLS 连接和本地模拟上游；不会安装用户证书或调用收费服务。
