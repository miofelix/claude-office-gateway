# 更新与分发研究笔记

核对日期：2026-10-27。依据为官方文档、对应版本源码和项目文件的只读快照；**本任务未执行 macOS／Windows 原生构建、安装或升级测试**，下述配置建议不代表验证通过。

读取时：`package.json`／`package-lock.json` 锁定 Tauri CLI `2.10.1`；`src-tauri/Cargo.lock` 为 Tauri `2.10.3`、tauri-utils `2.8.3`、aws-lc-rs `1.15.3`、aws-lc-sys `0.36.0`、cmake crate `0.1.57`。应用版本仍为 `0.1.0`，Rust 最低声明为 `1.88.0`；锁文件尚无 updater 插件。macOS 配置为 `signingIdentity: "-"`，Windows 配置启用 MSI、NSIS。以上是读取时状态，主代理后续修改后需重新核对。

## 1. `v0.2.0-beta.1` 与 Windows 安装器

Git 标签可用 `v0.2.0-beta.1`，应用和更新清单统一使用 `0.2.0-beta.1`。CLI `2.10.1` 的两个 bundler 行为不同：[1][2]

| 目标 | 对 `0.2.0-beta.1` 的处理 | 是否需要数值覆盖 |
| --- | --- | --- |
| MSI / WiX | `convert_version` 尝试把整个 `beta.1` 解析为整数，失败并报错 | **需要**，用 `bundle.windows.wix.version`，例如 `"0.2.0"` |
| NSIS | `try_add_numeric_build_number` 忽略预发布部分，将 `VIProductVersion` 生成为 `0.2.0.0`；保留应用版本字符串 | **无需**另外覆盖 |

因此可保留应用的完整 SemVer，只在 Windows 平台配置增加以下片段；它是建议，未在本任务应用：[1][3]

```json
{ "bundle": { "windows": { "wix": { "version": "0.2.0" } } } }
```

WiX 字段支持 `major.minor.patch[.build]`，前两段最大 255，后两段最大 65535。但 Windows Installer 比较 `ProductVersion` **只看前三段**，所以 `0.2.0.1`、`0.2.0.2` 不能仅凭第四段保证升级排序。**推断／建议：**后续 beta 和正式版需单独设计 MSI 数值版本映射，并原生验证升级；updater 的 SemVer 比较不等同于 MSI 的版本比较。[3][4]

## 2. Tauri v2 更新产物和平台键

使用 `bundle.createUpdaterArtifacts: true` 并提供 updater 签名材料。官网列出的 v2 原生产物如下；`.sig` 与下载负载逐一配对。`"v1Compatible"` 是兼容旧客户端的另一种模式，会产生 Windows ZIP，不应把它当作 v2 默认格式。[5]

| Rust 构建目标 | 静态清单 `platforms` 基础键 | 更新负载 | 签名文件 |
| --- | --- | --- | --- |
| `aarch64-apple-darwin` | `darwin-aarch64` | `bundle/macos/<product>.app.tar.gz` | 同路径加 `.sig` |
| `x86_64-apple-darwin` | `darwin-x86_64` | `bundle/macos/<product>.app.tar.gz` | 同路径加 `.sig` |
| `x86_64-pc-windows-msvc`，NSIS | `windows-x86_64` | `bundle/nsis/*.exe` | 对应 `.exe.sig` |
| `x86_64-pc-windows-msvc`，WiX | `windows-x86_64` | `bundle/msi/*.msi` | 对应 `.msi.sig` |

表中路径相对于相应 release 输出目录；显式 `--target` 时通常为 `src-tauri/target/<triple>/release/`。DMG 是手动分发包，macOS updater 下载的是包含 `.app` 的 tar.gz。[5]

清单顶层使用 `version: "0.2.0-beta.1"`、`platforms`，可附 `notes`、RFC 3339 `pub_date`；每个平台条目包含 HTTPS `url` 和 `signature`。**`signature` 是对应 `.sig` 文件的文本内容，不是签名文件 URL。**文件名中的 `x64`／`arm64` 不改变平台键使用的 `x86_64`／`aarch64`。[5][6]

当前稳定版 updater 经 crates.io 元数据核对为 **`2.14.0`**。其源码按 `{os}-{arch}-{installer}`、`{os}-{arch}` 顺序匹配，因此支持 `darwin-aarch64-app`、`darwin-x86_64-app`、`windows-x86_64-nsis`、`windows-x86_64-msi`，并回退到表中基础键。**建议：**同时维护两类 Windows 安装器时，各放一个后缀键，并保留 `windows-x86_64` 作为明确选择的兼容默认项；一个基础键不能同时表示两个下载地址。此后缀匹配结论对应 `2.14.0`，若选用更老插件须复核。[6][7]

macOS bundler 从 `.app` 路径直接派生 `.app.tar.gz`，两个架构可能同名。**推断／建议：**上传 Release 前为资产名加架构，更新清单引用最终名称；仅重命名不改变已签名字节，重新打包则须重新签名。[5][8]

GitHub 的 “latest release” 排除 prerelease。**推断／建议：**仅发布 beta 时，不依赖 `/releases/latest/download/latest.json` 自动选中它；显式使用 `/releases/download/v0.2.0-beta.1/latest.json`，或由主代理维护专门的 beta 清单地址。设置 GitHub `prerelease: true` 不会自动建立客户端 beta 通道。[5][9]

## 3. GitHub runner 与原生工具链

截至核对日，GitHub 标准 hosted runner 的明确搭配为：[10]

| 构建目标 | 建议 `runs-on` | runner 架构 |
| --- | --- | --- |
| macOS ARM64 | `macos-15` | ARM64 |
| macOS Intel | `macos-15-intel` | x64 |
| Windows x64 | `windows-2022` | x64 |

当前文档也列出 ARM64 的 `macos-latest`、`macos-14`、`macos-26`；不要按旧经验把 `macos-15` 当 Intel。官方镜像公告称 macOS 14 将于 **2026-11-02** 完全结束支持，因此不建议新流水线依赖它。`-latest` 会迁移，固定 OS 标签也不固定镜像内所有软件版本。[10][11]

- **macOS：**需要 Xcode／Command Line Tools 和对应 Rust target。Tauri 支持同一 macOS 主机跨 Intel／ARM 编译；采用架构匹配 runner 是减少本地 C／汇编依赖跨编译变量的建议，并非必须分别拥有两种主机。项目的 macOS 13 最低版本声明仍需在依赖与实际目标系统上验证。[12]
- **aws-lc-sys `0.36.0`：**会编译本地 C／汇编；`get_builder` 在条件满足时优先 `cc`，否则回退 CMake，也可被 `AWS_LC_SYS_CMAKE_BUILDER` 强制选择。因此 Cargo.lock 含 cmake crate 不等于每次构建都必需执行 CMake。Windows MSVC 还需可用的 C 工具链与 NASM，预编译 NASM 对象属于另行选择的路径。[13][14]
- **CMake：**官方 macOS 15 镜像清单列有 3.31.5／4.1.2，不能假设 PATH 永远指向某个版本。CMake 4 移除了低于 3.5 的策略兼容；锁定 aws-lc-sys 的外层声明为 `3.5...3.31`。**推断／建议：**记录实际 `cmake --version`；若发生 policy 错误，按具体出错的 CMakeLists 排查，必要时固定 3.31.x，不能仅据依赖名称就断言必须降级或原生构建一定成功。[11][13][15]
- **Windows：**Tauri MSI 使用 WiX v3，需在 Windows 构建，并满足 MSVC C++ Build Tools／Windows SDK／WebView2 前提；GitHub `windows-2022` 镜像列有 Visual Studio 2022、CMake 和 WiX，但应核对实际工具路径。[16][17]
- **Rust 版本：**当前 updater `2.14.0` 最低 Rust 为 **1.90**，高于项目读取时声明的 1.88；若采用该版本，CI 工具链和最低版本声明必须一起核对。[7]

Windows 的“UTF-8 问题”应按错误区分：[18][19][20]

- 若错误是输出流无法解析为 UTF-8：CLI 对应 bundler 用 `BufRead::lines()` 读取子进程输出，而 WiX v3 向 Console 输出消息。**推断／未实测排障建议：**在启动构建工具前、同一 Windows 控制台设置 `chcp 65001`；它改变后启动程序的控制台代码页，不等于改了 MSI 数据库编码。
- 若是 `LGHT0311`／字符串不能用数据库代码页表示：Tauri WiX 模板的 `Product Codepage` 来自语言映射，`en-US` 为 1252，`zh-CN` 为 936。应使安装器语言／本地化文件／产品文本匹配；不能把改终端 UTF-8 当作修复 MSI 数据库字符集。[19][21]

## 4. macOS ad-hoc 与 updater 签名

`bundle.macOS.signingIdentity: "-"` 表示 **ad-hoc 代码签名**，不提供 Developer ID 身份，也不代表 Apple 公证。Tauri 官方将 Developer ID 签名、公证用于官网等渠道分发；ad-hoc 分发仍会触发面向用户的系统信任处理，不能据此承诺下载后无提示运行。[22]

**updater 签名是独立且强制的机制**：构建使用 `TAURI_SIGNING_PRIVATE_KEY`／按需的密码生成 `.sig`，客户端通过 `plugins.updater.pubkey` 验证所下载的产物；官方明确不能禁用。Apple 签名／公证、GitHub Release 或 SHA-256 清单均不替代 `.sig`。反过来，正确 updater 签名也不授予 Apple Developer ID 或公证状态。**推断：**ad-hoc `.app` 可以作为 updater 签名归档的内容，但实际 Gatekeeper 行为、替换安装及重启仍需原生验证。[5][22]

## 官方来源

- [1] [Tauri CLI 2.10.1：WiX `convert_version`、配置覆盖](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.10.1/crates/tauri-bundler/src/bundle/windows/msi/mod.rs#L311)
- [2] [Tauri CLI 2.10.1：NSIS `try_add_numeric_build_number`](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.10.1/crates/tauri-bundler/src/bundle/windows/nsis/mod.rs#L150)
- [3] [Tauri v2 配置：WixConfig](https://v2.tauri.app/reference/config/#wixconfig)
- [4] [Microsoft：MSI ProductVersion，仅前三段参与比较](https://learn.microsoft.com/en-us/windows/win32/msi/productversion)
- [5] [Tauri v2 updater：产物、签名、静态清单](https://v2.tauri.app/plugin/updater/)
- [6] [updater 2.14.0：平台／安装器匹配及 OS、架构名称](https://github.com/tauri-apps/plugins-workspace/blob/updater-v2.14.0/plugins/updater/src/updater.rs#L743)
- [7] [crates.io：updater 官方发布元数据，含版本、最低 Rust](https://crates.io/api/v1/crates/tauri-plugin-updater)
- [8] [Tauri CLI 2.10.1：macOS 更新归档命名](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.10.1/crates/tauri-bundler/src/bundle/updater_bundle.rs#L47)
- [9] [GitHub：Get the latest release](https://docs.github.com/en/rest/releases/releases#get-the-latest-release)
- [10] [GitHub：标准 hosted runner 标签与架构](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)
- [11] [官方 macOS 15 ARM 镜像清单及 macOS 14 退役公告，固定快照](https://github.com/actions/runner-images/blob/6d35e16176367288d91febefafbd221cb2da6ca1/images/macos/macos-15-arm64-Readme.md)；[Intel 镜像清单](https://github.com/actions/runner-images/blob/6d35e16176367288d91febefafbd221cb2da6ca1/images/macos/macos-15-Readme.md)
- [12] [Tauri：macOS 原生与跨架构构建](https://v2.tauri.app/distribute/macos-application-bundle/)；[平台构建前提](https://v2.tauri.app/start/prerequisites/)
- [13] [aws-lc-sys 0.36.0 对应提交：builder 选择](https://github.com/aws/aws-lc-rs/blob/efaf569ec6c3a3a1d427a5101e363a3ee8f52891/aws-lc-sys/builder/main.rs#L422)；[CMake 声明](https://github.com/aws/aws-lc-rs/blob/efaf569ec6c3a3a1d427a5101e363a3ee8f52891/aws-lc-sys/CMakeLists.txt#L4)
- [14] [AWS：Windows 构建工具与 NASM](https://aws.github.io/aws-lc-rs/requirements/windows.html)
- [15] [CMake：最低版本与 4.0 策略兼容变化](https://cmake.org/cmake/help/latest/command/cmake_minimum_required.html)
- [16] [Tauri：Windows 安装器及 WiX 要求](https://v2.tauri.app/distribute/windows-installer/)
- [17] [官方 Windows 2022 镜像清单，固定快照](https://github.com/actions/runner-images/blob/6d35e16176367288d91febefafbd221cb2da6ca1/images/windows/Windows2022-Readme.md)
- [18] [Tauri CLI 2.10.1：子进程输出读取](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.10.1/crates/tauri-bundler/src/utils/mod.rs)；[WiX v3 Console 输出](https://github.com/wixtoolset/wix3/blob/develop/src/tools/wix/ConsoleMessageHandler.cs)
- [19] [Tauri CLI 2.10.1：WiX 模板 Codepage](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.10.1/crates/tauri-bundler/src/bundle/windows/msi/main.wxs)；[语言代码页映射](https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.10.1/crates/tauri-bundler/src/bundle/windows/msi/languages.json)
- [20] [Microsoft：chcp 的作用与后启动程序](https://learn.microsoft.com/en-us/windows-server/administration/windows-commands/chcp)
- [21] [WiX 官方：数据库及本地化代码页](https://docs.firegiant.com/wix3/overview/codepage/)
- [22] [Tauri：macOS 签名、公证及 ad-hoc](https://v2.tauri.app/distribute/sign/macos/)
