# 0.2.0-beta.1 验证记录

日期：2026-10-27。发布源代码提交：`5da8bb6f96d249c9aee46d288c95ecc30ec063b4`，对应 `v0.2.0-beta.1` 标签。初期 UI / 外观验证的历史记录见 [VALIDATION.md](VALIDATION.md)。

## 本机自动检查

`./scripts/check.sh` 通过：19 项前端测试、18 项 Rust 测试、10 项发布脚本测试，以及 Prettier、TypeScript、Vite 生产构建、rustfmt、clippy（warnings 视为错误）。

更新测试覆盖原生版本和通道、启动检查偏好、检查去重、只检查不自动安装、安装时固定已确认版本、网络错误重试，以及过期初始化／轮询结果不能覆盖已完成操作。

发布器测试覆盖 feed 的版本排序、不降级、已发布版本不可修改、首次创建独立 updates 分支、正式版同时推进 beta/stable，并且不强推分支。

## PowerPoint 实际连接修复

用户在 macOS PowerPoint 报告 `Load failed (localhost:15724)`，bootstrap 尚未调用。对正在运行的网关最小复现：

| 请求 | 修复前 | 修复后 |
| --- | --- | --- |
| HTTPS HTTP/1.1 `/health` | 200 | 支持保留 |
| HTTPS HTTP/2 `/health` | 403，Host is not allowed | 200 |
| HTTPS HTTP/2 Office 跨域 OPTIONS | 403 | 204，返回允许的 Origin / 方法 / 请求头 |
| macOS URLSession `/health` | 403，Host is not allowed | 200 |

原因是原校验仅读取 HTTP/1.1 `Host`，HTTP/2 的 `:authority` 位于请求 URI，未被识别。修复后验证 URI authority 与 Host，二者同时出现时均须合法且一致；仍拒绝外部域名、错误端口、缺失主机与冲突主机。

新增回归测试先在旧实现上稳定失败（403 != 200），修复后通过。三种协议的真实 TLS / SSE 集成测试现在强制协商 HTTP/2，并断言响应的实际协议版本，避免再次只覆盖 HTTP/1.1。

本机启动修复后的正式 app，系统已信任的证书无需重装。用户在正式客户端重新复制令牌后，已确认 PowerPoint **连接成功**。没有记录用户令牌、供应商密钥、文档内容或请求正文。

## DeepSeek 真实密钥验证

用户报告连接成功后对话显示 `Claude is temporarily unavailable`，详情为 HTTP 422。在独立空白 PowerPoint 文稿通过官方加载项复现：Office 函数工具的 `type: custom` 被 DeepSeek 原生接口拒绝。使用获授权的真实密钥，只移除该字段，同一合成请求由 422 变为 200。另确认思考模式下的强制工具选择与缺少思考历史的工具结果也会被拒绝。对照结果和官方资料见 [DEEPSEEK.md](DEEPSEEK.md)。

新增真实 TLS 回归测试先后得到预期 HTTP 400、422，针对性修复后通过。覆盖 Anthropic 九种 / Chat 八种情况，包括 Office 的 custom 工具、token 计数、工具结果不携带工具定义，以及其他域名与相似域名保留原行为。

重新签名构建并启动 macOS 正式客户端，沿用钥匙串中的凭据和已信任证书。通过本机实际 HTTPS 网关，保留 Office 的 adaptive/high 输入参数，分别验证：

- 非流式：强制指定幻灯片计数工具 → 回传合成结果 3 → 自动工具模式下返回正确答案，HTTP 200。
- 流式：强制指定同一工具 → 回传合成结果且省略 tools → 返回正确答案与 message_stop，HTTP 200。
- 原生界面显示 DeepSeek 工具会话使用非思考模式的中英文提示；运行状态已恢复为 Office 连接就绪。

随后在 PowerPoint 新建独立空白文稿，通过官方 Claude 加载项验证：

- 同一文本请求修复前返回 422，修复后正常显示 `HELLO`。
- 加载项执行读取幻灯片数量的工具，正确返回 1。
- 加载项调用多个 PowerPoint 工具，成功新增标题为 `Gateway validation` 的第 2 页，原第 1 页保持不变。

未读取用户原有 Office 文档，只对本轮创建的空白测试文稿执行工具操作。密钥和令牌只从系统凭据读取至进程内存，未写入测试脚本、项目文件或输出。临时诊断仅输出参数结构与状态，已从发布源码清除。

合成验证文稿保存在本机 `artifacts/validation/DeepSeek-Office-Validation.pptx`（未纳入 Git），检查确认共 2 页且第 2 页包含预期标题。

清除诊断代码后的最终签名构建再次通过非流式／流式 custom 工具往返，以及携带 custom 工具的 token 计数（HTTP 200，返回有效整数计数）。

## macOS 本机构建

`python3 scripts/build-signed-release.py --target aarch64-apple-darwin` 通过。生成 Apple Silicon app、DMG、更新归档和 `.sig`。

- `codesign --verify --deep --strict` 通过，采用 ad-hoc 签名。
- DMG `hdiutil verify` 通过。
- 更新归档通过公钥 Minisign 验证。
- 向归档副本追加字节后，验签按预期失败。
- 原生 WKWebView 已确认版本徽标、更新弹窗、beta 通道和失败重试反馈。
- 从本次 Actions 下载的 Apple Silicon DMG 在本机通过完整性校验、只读挂载及包内应用的严格代码签名验证；确认 ARM64、版本 `0.2.0-beta.1`、最低 macOS 13.0。其资产 SHA-256 与构建记录一致，更新归档也通过本机验签。

未进行 Apple Developer ID 签名或公证。未为了更新测试替换用户正在工作的客户端；完整跨版本安装与重启仍需后续版本验证。

## 发布验证

[发布流水线 40461824098](https://github.com/miofelix/claude-office-gateway/actions/runs/40461824098) 的 Apple Silicon macOS、Intel macOS、Windows x64 检查、原生构建及签名验证全部通过，publish 作业成功。

[v0.2.0-beta.1](https://github.com/miofelix/claude-office-gateway/releases/tag/v0.2.0-beta.1) 已于 2026-10-27 06:55 UTC 发布为预发布版本。标签精确指向 `5da8bb6f96d249c9aee46d288c95ecc30ec063b4`。

- 共 12 个公开资产：两个 macOS 架构各 DMG / 更新归档 / 签名，Windows EXE / MSI 及各自签名，加 `latest.json`、`SHA256SUMS`。
- 发布后重新下载全部资产，10 个分发文件的 SHA-256 全部匹配，4 个不同更新包的 Minisign 验证全部通过。
- 公开版 ARM / Intel 两个 DMG 均在本机完成完整性校验、只读挂载及包内严格代码签名验证，版本、架构和最低系统要求符合预期；Intel 应用未在 Intel 实机运行。
- HTTPS beta feed 返回 200，内容与该 Release 的 `latest.json` 完全一致，包含两个 macOS 架构和 Windows 通用 / NSIS / MSI 平台键。
- 最终 macOS 客户端手动检查正式 beta 源，原生窗口显示 `0.2.0-beta.1`、测试通道、当前已是最新版本，并显示检查时间。
- 普通 Desktop checks 也在 macOS / Windows 上通过，运行号 `40461823440`。

本次没有通过发布一个更高虚拟版本测试安装替换；跨版本完整升级与重启需下一版本继续验证。Windows 原生构建和签名验证已完成，真实设备安装、Office 交互与更新替换仍交由用户在 Windows 电脑验证。
