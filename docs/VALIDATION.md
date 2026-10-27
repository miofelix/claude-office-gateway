# macOS 本机验证

日期：2026-10-27。客户端代码提交：`f232bf1`。

## 环境

| 项目 | 值 |
| --- | --- |
| 系统 | macOS 26.4.1 |
| 架构 | Apple Silicon / arm64 |
| Rust | 1.94.1 |
| Node.js / npm | 26.12.0 / 11.20.0 |
| 最低构建 Rust | 1.88（匹配锁定依赖） |
| 桌面配置 | macOS 13+；Tauri 2；原生 WKWebView |

## 自动检查

`./scripts/check.sh` 已完整执行通过：

- Prettier、TypeScript 严格检查、生产前端打包通过。
- 8 项前端回归测试通过：保存失败后不连接、保存中取消、旧结果隔离、刷新保留草稿、已有密钥重连、错误状态、真实令牌复制路径及剪贴板错误。
- 13 项 Rust 测试通过：三种协议的真实 localhost TLS 往返、SSE 增量输出、认证与 CORS、工具调用与截图、非法输入与未完成流、Swift 配置兼容、端口冲突与检测中停止。
- `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings` 通过。

TLS 集成测试生成临时证书，客户端只显式信任该测试证书，所有上游由本地模拟服务提供。增量测试先等待真实文本到达，再释放上游结束事件，验证并非整段缓冲后返回。

## 原生窗口

实际启动了发布版 `.app`，检查了原生可访问性状态和截图：

- 中文单页正常渲染；没有侧边导航或独立设置页面。
- 高级设置仅有一个折叠入口，展开后提供认证、token 字段、Office 模型 ID、端口与超时。
- GitHub 图标入口位于右上角，目标为 `https://github.com/miofelix/claude-office-gateway`。
- 未配置供应商时，保存/连接和复制按钮按状态禁用。
- 证书确认窗口可显示；在用户手动操作后，页面能反映「已受信任」及再次移除后的状态。
- 状态栏菜单的动态动作、启用条件与中英文同步已构建进发布版；未替用户执行收费供应商连接或复制真实密钥。

本次可见验证窗口使用 `OFFICE_GATEWAY_DATA_DIR=/tmp/claude-office-gateway-final-ui-check` 和内存测试凭据。它与日常配置独立；正式使用前先退出测试窗口，再直接打开交付的 `.app`。

## 发布包

构建命令：

```bash
npm run tauri -- build --bundles app,dmg
python3 scripts/collect-macos.py
```

产物：

- `artifacts/macos/Claude Office Gateway.app`，约 13 MiB，arm64。
- `artifacts/macos/Claude Office Gateway_0.1.0_aarch64.dmg`，约 5.1 MiB。
- `artifacts/macos/SHA256SUMS`。

应用经 `codesign --verify --deep --strict` 验证通过，标识符为 `com.miofelix.office-gateway`，采用 ad-hoc 签名。DMG 经 `hdiutil verify` 检查通过。未进行 Developer ID 签名或 Apple 公证。

DMG SHA-256：

```text
b2f16b3fe04da2bbedb2ac0a942b1dc1945f3f914022ffb40f76a93ed6493acb
```

## 验证范围

本机完成了自动检查、真实本地 TLS 模拟上游测试、原生窗口检查、发布构建、签名与 DMG 完整性验证。真实 Office 文档和真实供应商的端到端使用仍需用户自己的 Office 加载项与供应商账户；该部分不冒充为已经完成。Windows 的编译、安装器和原生功能验证留待 Windows 机器，步骤见 `WINDOWS.md`。
