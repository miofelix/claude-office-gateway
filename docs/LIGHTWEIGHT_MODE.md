# 轻量模式与 beta.2 验证

版本：`0.2.0-beta.2`。日期：2026-10-27。

## 使用

从 macOS 状态栏／Windows 托盘菜单勾选「轻量模式」，或在单页的「高级设置」点击「进入轻量模式」。主窗口和 WebView 会被销毁，网关、凭据状态、证书检查和托盘继续运行；macOS 同时隐藏 Dock 图标，Windows 不保留任务栏窗口。

从托盘点击「打开 Claude Office Gateway」、取消「轻量模式」勾选，或再次启动应用，会按需重建同一个主窗口。窗口恢复保存的连接状态、外观和语言偏好，以及本次会话中的窗口位置／尺寸。macOS 恢复 Dock 图标。

轻量模式只对当前进程生效，完全退出后再次启动会显示正常窗口。普通关闭按钮仍是隐藏窗口；退出托盘菜单会结束应用并停止网关。

进入时若有未保存的供应商修改，先显示确认，可选择继续编辑或明确丢弃；不会把尚未保存的密钥写入磁盘以便恢复草稿。保存、连接、证书或更新操作进行中时暂缓进入。窗口重建不会重复已完成的启动更新检查。

## 实现

参考本机 `cc-switch/src-tauri/src/lightweight.rs` 和其托盘入口，使用 Tauri 的 `destroy()` 与 `WebviewWindowBuilder::from_config()`。`lightweight.rs` 集中管理进入、销毁、恢复四种状态，窗口操作在原生主线程串行执行；销毁尚未完成时收到唤起请求，会在 Destroyed 事件后恢复。

只有运行时因没有窗口而发起的退出被阻止。显式退出与更新重启沿用原有退出流程，不会被托盘驻留逻辑阻断。更新和网关状态保留在 Rust，窗口仅重新挂载前端。

## 本机验证

- 自动检查：24 项前端测试、20 项 Rust 测试、10 项发布脚本测试，格式、类型检查、生产构建、clippy 通过。
- 窗口生命周期测试覆盖销毁期间重复唤起、重建去重、失败后重试和冷启动状态。
- 前端测试覆盖托盘事件读取最新草稿、取消保留、明确丢弃、操作中阻止、原生失败重试与销毁后解除事件监听。
- macOS 原生客户端进入轻量模式后，同一进程继续监听 localhost；HTTPS `/health` 返回 200 和 beta.2 版本，经已有真实供应商的合成请求返回 200 / end_turn。未读取 Office 文档。
- 再次打开应用按需重建窗口，原生状态保持 Office 已连接，外观／语言偏好保留。
- 原生界面验证未保存修改先弹确认，取消后草稿保留；确认丢弃并重建后恢复已保存配置。临时草稿未保存。
- 同版修复 Windows 证书检查反复弹出 PowerShell 窗口，原生回归先红后绿，见 [证书控制台记录](WINDOWS_CERTIFICATE_CONSOLE.md)。

此模式释放窗口和其 WebView，不承诺固定内存降幅；WebKit／WebView2 可能保留进程缓存，常驻 Rust 网关也仍使用内存。

## 发布验证

发布源提交：`026cc7cbfb278257980de2c9331bb95e2bf1c52a`。[Actions run 40469550524](https://github.com/miofelix/claude-office-gateway/actions/runs/40469550524) 的 Apple Silicon、Intel macOS 和 Windows x64 构建及 publish 全部成功。[beta.2](https://github.com/miofelix/claude-office-gateway/releases/tag/v0.2.0-beta.2) 于 2026-10-27 08:15:49 UTC 发布。

- 共 12 个公开资产全部重新下载，10 个分发文件 SHA-256 与清单一致，4 份不同更新归档／安装器的 Minisign 验证通过。
- ARM / Intel DMG 均在本机完成完整性校验、只读挂载、原生版本与架构核对、严格 codesign 验证。Intel 应用未在 Intel 实机运行。
- HTTPS beta 更新源与 Release 的 `latest.json` 内容一致，包含 macOS 两种架构和 Windows 通用／NSIS／MSI 平台键。篡改归档副本的验签失败符合预期。
- Windows 原生检查通过 23 项 Rust、24 项前端、10 项发布脚本测试；证书控制台测试先红后绿。EXE / MSI 构建和更新签名均通过。

## macOS 跨版本升级

在独立目录解包已验证的公开 beta.1，使用隔离配置与内存凭据运行。通过其原生更新弹窗从正式 beta feed 发现 beta.2，手动执行「安装并重启」。确认下载进度、强制签名验证、实际替换与自动重启完成。

重启后的原生弹窗显示 `0.2.0-beta.2` 和「当前已是最新版本」。更新副本通过严格 codesign 验证，程序 SHA-256 与公开 beta.2 签名归档内程序完全一致。没有安装测试证书、读取日常钥匙串密钥或替换日常配置。

Windows 原生构建已验证，实际托盘、任务栏、Office 连续性、可见窗口闪现与跨版本安装仍需 Windows 设备验收。
