# 发布与应用更新

版本：`0.2.0-beta.2`。仓库：<https://github.com/miofelix/claude-office-gateway>。

## 版本与通道

应用标题旁显示原生版本号，点击打开更新窗口。启动检查默认为开，偏好保存在当前用户的 WebView 存储中；自动检查不会自动安装。下载、签名校验、安装、重启由用户手动触发。

原生编译版本含预发布标记时读取 `updates` 分支的 `beta.json`，否则读取 `stable.json`。发布器为所有新版本推进 beta，为正式版另外推进 stable；SemVer 比较阻止通道降级。发布版本资产不可变，修改已发布字节须提升版本号。

Windows MSI 使用数值映射，避免 WiX 不接受 `beta.1` 和 Windows Installer 忽略第四段的问题：

- 应用 `major.minor.patch-beta.N` → MSI `major.minor.(patch*256+N)`。
- `rc.N` 的槽位为 `127+N`，正式版为 `255`。
- `major`、`minor`、`patch` 最大 255，N 为 1..127。
- 本次 `0.2.0-beta.2` 的 MSI ProductVersion 为 `0.2.2`；应用和更新仍显示完整的 `0.2.0-beta.2`。

`check-release.py` 检查 npm、Cargo、Tauri 版本一致以及 MSI 映射。选定 updater `2.10.0`，与当前 Tauri / Rust 最低版本保持兼容；已核对它支持 Windows 安装器专用平台键。

## 构建和发布

1. 修改各版本文件、Windows MSI 数值版本，以及 `docs/releases/v<version>.md`。
2. 提交并推送 main。Desktop checks 在 macOS / Windows 原生执行检查。
3. 运行 **Publish desktop release** 的 workflow_dispatch，`version` 填完整版本。三个平台全部完成检查、原生构建、更新签名验证后，publish 作业才创建草稿 Release、上传资产并发布；Git 标签指向该次构建的精确提交。
4. 发布器再次校验资产 SHA-256 和更新签名，然后推进独立的 `updates` 分支。feed 推进失败可重跑 publish 作业；它验证已发布版本与现有资产相同后恢复操作，不替换已发布资产。

构建使用 `macos-15`（ARM64）、`macos-15-intel`、`windows-2022`。产物名含版本和架构，macOS 归档只重命名，不重新打包。Windows 同时发布 NSIS / MSI，各有独立更新地址及签名。`latest.json` 属于具体版本，beta 不依赖 GitHub 的 latest-release 规则。

本地正式构建：

```bash
python3 scripts/build-signed-release.py --target aarch64-apple-darwin
```

普通本地构建使用 `scripts/build-macos.sh` / `build-windows.ps1`，通过 `tauri.local.conf.json` 关闭更新产物生成，无需发布私钥。不能将这种产物当作官方更新包上传。

## 签名材料

更新公钥在 `tauri.conf.json`；加密私钥存放在发布维护者 macOS 用户目录下的 `Library/Application Support/Claude Office Gateway Release/updater.key`，目录权限 0700，文件 0600。密码位于登录钥匙串的 `com.miofelix.office-gateway.release` 服务、`updater-key-password` 账户。

GitHub Actions 使用仓库加密 secrets `TAURI_SIGNING_PRIVATE_KEY` 与 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`，仅注入签名构建步骤。私钥、密码和系统凭据不写入仓库或日志。私钥需要维护者单独离线备份；遗失后，现有客户端无法验证新密钥签名的更新。

更新签名独立于操作系统代码签名。macOS 当前为 ad-hoc，未使用 Apple Developer ID 或公证；Windows 当前没有 Authenticode 签名。不能把更新签名通过视为已通过系统发行信任。

## 验证边界

`tools/update-verifier` 使用与客户端相同的 Minisign 验证库验证归档或安装器，发布构建缺签名或验签失败即中止。完整安装替换和重启仍需对应系统实测；Windows 的 Office UI、系统信任和安装升级验证交由实际 Windows 设备完成。
