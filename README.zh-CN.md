# Claude Office Gateway

仅面向 **macOS 与 Windows** 的桌面客户端，将 PowerPoint、Excel 和 Word 的 Claude 加载项连接到自己的模型供应商。

基于 Tauri 2、React、TypeScript 和 Rust 重构。架构参考 `cc-switch` 的桌面壳体、命令调用、单实例和托盘模式，网关能力迁移自原生 Swift 客户端。

## 使用

要求 macOS 13+，或安装了 WebView2 的 Windows 10/11。界面支持简体中文与英文。右上角提供「亮色／暗色／跟随系统」，会记住选择；跟随系统时实时响应系统外观变化。

1. 在「本地 HTTPS 证书」卡片安装证书，完成系统确认；检查和卸载也在同一区域。证书添加到当前用户，期限一年。证书信任按主机名校验，不能限定单个端口，也不等于信任所有 localhost 证书；[范围与平台差异](docs/CERTIFICATE_SCOPE.md)。
2. 填写 HTTPS API 基础地址、协议、供应商密钥和准确的模型 ID。「保存」仅保存；「保存并连接」会保存并发送两次简短检测请求，可能产生少量费用。
3. 在 Office 的 Claude 加载项选择 Gateway、Anthropic 格式、x-api-key 认证，复制客户端提供的地址、网关令牌和 Office 模型 ID。令牌无需 `Bearer` 前缀。

使用期间保持客户端运行。关闭窗口后仍继续服务；退出应用后停止。再次启动时点击连接即可，无需重新输入已保存密钥。

密钥保存在 macOS 钥匙串或 Windows 凭据管理器。应用没有遥测，也不保存请求正文或文档内容。

## 验证与构建

安装 Node.js 22.12+（或 24+）、Rust 1.88+、rustfmt/clippy。macOS 需要 Xcode Command Line Tools；Windows 需要 Visual Studio C++ Build Tools 和 Windows SDK。

```bash
npm ci
npm run dev
```

macOS：

```bash
./scripts/check.sh
./scripts/build-macos.sh
```

Windows PowerShell：

```powershell
./scripts/check.ps1
./scripts/build-windows.ps1
```

产物集中在 `artifacts/macos/`、`artifacts/windows/`。本地 macOS 包采用 ad-hoc 签名；公开发行需另行完成 Developer ID 签名和公证。

迁移旧版前先退出 Swift 客户端，释放 `15724` 端口。新版直接读取原配置和原钥匙串条目；第一次保存会备份 Swift 配置，不替换已信任的证书。旧 PKCS#12 私钥会在需要时本地转换为仅当前用户可读的 PEM。

[架构与阶段记录](docs/ARCHITECTURE.md) · [macOS 验证记录](docs/VALIDATION.md) · [Windows 验证步骤](docs/WINDOWS.md) · [English](README.md)
