# Claude Office Gateway

A macOS and Windows desktop client that connects the Claude add-ins for **PowerPoint, Excel, and Word** to your own model provider through a localhost HTTPS gateway.

[简体中文](README.zh-CN.md) · [Architecture and migration](docs/ARCHITECTURE.md) · [macOS validation](docs/VALIDATION.md) · [Windows validation](docs/WINDOWS.md)

Supports Anthropic Messages, OpenAI Chat Completions, and OpenAI Responses, including streamed text, client function tools, tool result screenshots, model aliases, and the Office connection probes. Exact token counting requires an Anthropic upstream. Unsupported hosted tools and content are rejected locally.

## Beta download and updates

Current version: **0.2.0-beta.3**. [Download the beta](https://github.com/miofelix/claude-office-gateway/releases/tag/v0.2.0-beta.3): Apple Silicon DMG and Windows x64 EXE / MSI. macOS Intel builds ended with beta.2.

Click the version beside the app title for update checks, release notes and download progress. Startup checks are enabled by default and can be disabled. Installation is manual, verifies the update signature, then restarts the app. Prereleases use the beta channel; stable builds use stable. Beta also receives newer stable releases.

This beta uses ad-hoc macOS signing without Apple notarization. Windows installers do not have Authenticode signing. Initial installation may show operating-system trust prompts. Update payloads independently require Minisign verification. See [release and signing operations](docs/RELEASING.md).

## Use

Requires macOS 13+ or Windows 10/11 with WebView2. The interface supports English and Simplified Chinese. The top-right appearance selector offers Light, Dark, and Follow system. The choice persists across launches, and system mode responds to live appearance changes.

1. Install the certificate from the **Local HTTPS certificate** card and complete the system confirmation. Check and uninstall actions are in the same card. It is a one-year, self-signed localhost certificate installed for your current user. Certificate verification cannot be restricted to one port; see [certificate scope and platform differences](docs/CERTIFICATE_SCOPE.md).
2. Choose or create a saved provider profile, then enter its HTTPS API base URL, protocol, provider key, and exact model ID. **Save** saves without sending requests; **Save and connect** saves and sends two small checks that your provider may bill. Stop the gateway before switching profiles.
3. In the Claude Office add-in, choose **Gateway**, **Anthropic** format, and **x-api-key** authentication. Copy the URL, gateway token, and Office model ID from the client. Paste the token without `Bearer`.

The Office model ID must start with `claude`, for example `claude-office-custom`. It is a gateway alias mapped to your actual provider model; keep the provider’s exact ID in **Provider model**. If Office reports “Enter a Claude model name or a gateway alias that references one”, check this alias.

Keep the client running while using Office. Closing its window keeps the connection alive; quitting stops it. Reopen from the Dock or tray. After relaunch, choose the saved profile and click **Connect gateway**; saved provider keys do not need to be re-entered.

**Lightweight mode** releases the main window and web view while keeping the gateway and tray running. Enable it from the tray menu or **Advanced settings**. Open from the tray or launch the app again to rebuild the window. Unsaved changes require a choice before entry; the mode lasts for the current session only. [Behavior and validation](docs/LIGHTWEIGHT_MODE.md).

For the official DeepSeek API, native function tool definitions are normalized and tool conversations use non-thinking mode with Anthropic Messages or OpenAI Chat Completions. This avoids rejected Office tool definitions, forced tool calls and missing thinking history on tool results. Plain text requests and other providers keep their original behavior. See [DeepSeek compatibility and validation](docs/DEEPSEEK.md).

Keys use macOS Keychain or Windows Credential Manager. This client has no telemetry and does not store request bodies or document contents. Requests reach your configured provider; Office and that provider have their own data policies.

## Build and verify

Install Node.js 22.12+ (or 24+), Rust 1.88+ with rustfmt/clippy, and the native desktop build tools. On Windows, use Visual Studio Build Tools with the C++ workload and Windows SDK. On macOS, install Xcode Command Line Tools.

```bash
npm ci
npm run dev
```

On macOS:

```bash
./scripts/check.sh
./scripts/build-macos.sh
```

On Windows, in PowerShell:

```powershell
./scripts/check.ps1
./scripts/build-windows.ps1
```

Collected installers and application bundles are written to `artifacts/macos/` or `artifacts/windows/`. Dependencies are locked in `package-lock.json` and `src-tauri/Cargo.lock`. Local builds need no release private key and do not generate official signed update payloads. Beta macOS packages use ad-hoc signing; Developer ID signing and notarization are still pending.

## Upgrading the Swift client

Quit the Swift client first so it releases port `15724`. The macOS client reuses `~/Library/Application Support/Claude Office Gateway/settings.json` and the original Keychain service. Camel-case Swift settings are read directly; the first save retains a `settings.swift-backup.json`. The existing PKCS#12 identity is converted locally to a private PEM key when needed, retaining the certificate and its trust.

An independent community project, not affiliated with Anthropic or Microsoft. [MIT License](LICENSE).
