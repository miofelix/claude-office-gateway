# Windows 验证交接

Windows 版本与 macOS 共用 React/Rust 核心，包含 Windows 凭据管理器、当前用户证书存储、原生标题栏、AppUserModelID、单实例、托盘和 NSIS/MSI 配置。当前机器是 macOS；Windows 原生自动检查和安装器构建通过 GitHub Actions 的 Windows runner 进行，结果见 [公测版验证记录](RELEASE_VALIDATION.md)。真实 Windows 安装和 Office 交互仍需以下人工验证。

## 环境

- Windows 10/11，推荐 x64。
- Node.js 22.12+ 或 24+，使用 npm。
- Rust stable/MSVC 工具链，包含 rustfmt、clippy。
- Visual Studio Build Tools 的「使用 C++ 的桌面开发」和 Windows SDK。
- WebView2 Runtime；安装器包含官方 bootstrapper 的安装流程。

## 自动检查与安装包

复制或克隆当前仓库；不用复制 macOS 的 `node_modules`、`src-tauri/target` 或 `.app`。

```powershell
npm ci
./scripts/check.ps1
./scripts/build-windows.ps1
```

安装包输出到 `artifacts/windows/`，原始输出在 `src-tauri/target/release/bundle/nsis/` 和 `msi/`。

## 原生验证

1. 安装 NSIS 或 MSI 包，检查程序名称、图标、标题栏、引导式单页和中文/英文切换；验证「亮色／暗色／跟随系统」三档与重启后记忆，跟随系统时改变 Windows 外观应实时更新；高级设置仅有一个折叠入口，GitHub 图标跳转到项目仓库。
2. 在「本地 HTTPS 证书」卡片安装证书，确认只进入当前用户证书存储，状态变为「已信任」。不要安装到本地计算机证书存储。
3. 配置真实供应商，分别按需要验证 Anthropic Messages、OpenAI Chat Completions、OpenAI Responses。确认保存不调用模型，连接进行两次短检测。
4. 在 Office 加载项选择 Gateway、Anthropic 格式、x-api-key 认证；粘贴三个连接参数。分别检查文本增量输出、工具调用与结果、包含截图的任务。
5. 关闭主窗口后确认请求继续；从托盘恢复。检查菜单中的连接状态、连接/检测、停止、复制地址/令牌/模型 ID、GitHub 和退出；复制与检测操作应按状态启用。重复启动应用应聚焦已有窗口。退出应用应释放端口。
6. 重启后直接使用已保存密钥连接。输入错误密钥、占用端口、检测中停止时，检查错误提示与状态恢复。
7. 在同一证书卡片卸载证书，确认网关停止、仅对应指纹的用户证书被移除、配置与供应商密钥保留。重新安装后可恢复。
8. 点击标题旁版本号，确认原生版本和 beta 通道。检查启动自动检查开关、手动检查和更新说明链接；有更新时验证下载进度、签名校验和安装重启，并分别验证 EXE 与 MSI 安装的更新路径。MSI 数值版本与应用 SemVer 的映射见 [发布流程](RELEASING.md)。
9. 分别检查安装、升级与卸载流程；用户数据与信任的清理由应用设置中的显式操作完成，卸载器不会静默删除用户凭据。
10. beta.2：从托盘或高级设置进入「轻量模式」，确认任务栏窗口消失、Office 请求继续。取消勾选、托盘打开、重复启动分别验证按需恢复且只有一个窗口；未保存修改的取消／丢弃、更新期间阻止进入、轻量模式下托盘停止／连接／复制／退出也应正常。完全退出再启动应回到普通模式。[轻量模式记录](LIGHTWEIGHT_MODE.md)。
11. beta.2 证书闪窗修复：安装证书后观察主界面至少 30 秒，再关闭窗口／进入轻量模式观察至少 30 秒，确认 PowerShell 和 certutil 控制台不再反复出现。检查、安装、卸载及失败反馈仍应正常；不会为了隐藏窗口取消证书信任检查。[原因与回归](WINDOWS_CERTIFICATE_CONSOLE.md)。

如需隔离日常配置，可在独立测试会话设置 `OFFICE_GATEWAY_DATA_DIR`。需要验证实际 Windows Credential Manager 时不要启用 `OFFICE_GATEWAY_TEST_MODE`。
