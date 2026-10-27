# macOS 本机验证

日期：2026-10-27。以下为 `239d1f1` 阶段的历史记录，包含引导式单页和三档外观。新增版本更新、HTTP/2 修复及公测发布验证见 [0.2.0-beta.1 验证记录](RELEASE_VALIDATION.md)。

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

`./scripts/check.sh` 完整执行通过：

- Prettier、TypeScript 严格检查、生产前端打包通过。
- 12 项前端测试通过：已有的 8 项保存、取消、过期结果、草稿、重连、错误与复制回归；新增 4 项外观测试，覆盖跟随系统、手动覆盖、持久化恢复和恢复系统外观。
- 14 项 Rust 测试通过：三种协议的真实 localhost TLS 往返、SSE 增量、认证与 CORS、工具调用与截图、非法输入与未完成流、Swift 配置兼容、端口冲突、检测中停止，以及证书身份与端口的区别。
- `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings` 通过。

新增证书测试在不同本地端口使用同一临时证书，TLS 验证均成功；换成另一张同名 localhost 自签证书则失败。该测试使用显式信任测试证书的 Rustls 客户端，不代表验证了每个平台、每个 Office 客户端的信任锚实现。标准与平台区别见 `CERTIFICATE_SCOPE.md`。

所有上游由本地模拟服务提供。增量测试先等待真实文本到达，再释放上游结束事件，验证并非整段缓冲后返回。代理未修改系统证书信任或调用收费模型。

## 浏览器交互与视觉检查

使用与生产相同的前端，测试专用 IPC fixture 只在代理拥有的浏览器页面中注入，未加入生产代码：

- 1060 × 800 下检查中文、英文、亮色、暗色布局；780 × 800 下检查单列适配，没有横向溢出。
- 页面没有导航栏，只有一个高级设置入口。
- 证书安装、检查、卸载集中在同一卡片；取消安装不提交操作，模拟卸载后连接状态与复制按钮恢复正确状态。
- 三个清晰的 Office 安装入口均分发到原有正确 Marketplace URL；Gateway、Anthropic、x-api-key 与三个连接参数可直接在页面查看。
- 网关令牌复制调用发送 `field=token`，不复制遮罩文本。
- 外观选择恰为亮色、暗色、跟随系统；刷新保留手动选择，跟随系统实时响应模拟系统变化，手动亮色不被系统变化覆盖。
- 检查了暗色卡片、表单、Office 图标、状态、复制按钮和确认弹窗的背景与文字。

截图为模拟连接数据的视觉预览：

- `artifacts/previews/office-gateway-light.png`
- `artifacts/previews/office-gateway-dark.png`

## 原生窗口

以独立 bundle identifier 启动 `Office Gateway UI Preview.app`，保留原客户端：

- 原生 WKWebView 正常显示引导式单页、集中证书操作、较大的 Office 入口、三个参数及右上角外观选择。
- 实际截图观察到「跟随系统」选中时的暗色界面，含原生窗口装饰与控件。
- 原生窗口主题使用 Tauri `Window.setTheme` 同步；显式亮／暗使用相应值，跟随系统传入 `null`，并配置对应窗口权限。
- 预览窗口使用独立数据目录和内存测试凭据，并明确显示「隔离测试」；其证书操作与真实收费供应商连接未由代理执行。

## 发布包

构建与收集命令：

```bash
npm run tauri -- build --bundles app,dmg
python3 scripts/collect-macos.py --output artifacts/macos-redesign
```

本次单独输出到新目录，保留旧版应用及其正在运行的窗口：

- `artifacts/macos-redesign/Claude Office Gateway.app`，约 13 MiB，arm64。
- `artifacts/macos-redesign/Claude Office Gateway_0.1.0_aarch64.dmg`，约 5.1 MiB。
- `artifacts/macos-redesign/SHA256SUMS`。

应用经 `codesign --verify --deep --strict` 通过，标识符为 `com.miofelix.office-gateway`，采用 ad-hoc 签名。DMG 经 `hdiutil verify` 通过。未进行 Developer ID 签名或 Apple 公证。

DMG SHA-256：

```text
a010eddcd38bfa5ac69bfbd4bb3ab379437a383c89fff955219c211adb876e39
```

## 验证范围

本机完成自动检查、真实本地 TLS 模拟上游测试、浏览器交互与视觉检查、原生窗口检查、发布构建、签名与 DMG 完整性验证。真实 Office 文档和真实供应商的端到端使用仍需用户自己的 Office 加载项与供应商账户。Windows 编译、安装器和原生功能验证留待 Windows 机器，步骤见 `WINDOWS.md`。
