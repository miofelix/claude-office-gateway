# Claude Office Gateway

A macOS and Windows desktop client that connects the Claude add-ins for **PowerPoint, Excel, and Word** to your own model provider through a localhost HTTPS gateway.

[简体中文](README.zh-CN.md) · [Architecture and migration](docs/ARCHITECTURE.md) · [macOS validation](docs/VALIDATION.md) · [Windows validation](docs/WINDOWS.md)

Supports Anthropic Messages, OpenAI Chat Completions, and OpenAI Responses, including streamed text, client function tools, tool result screenshots, model aliases, and the Office connection probes. Exact token counting requires an Anthropic upstream. Unsupported hosted tools and content are rejected locally.

## Use

Requires macOS 13+ or Windows 10/11 with WebView2. The interface supports English and Simplified Chinese. The top-right appearance selector offers Light, Dark, and Follow system. The choice persists across launches, and system mode responds to live appearance changes.

1. Install the certificate from the **Local HTTPS certificate** card and complete the system confirmation. Check and uninstall actions are in the same card. It is a one-year, self-signed localhost certificate installed for your current user. Certificate verification cannot be restricted to one port; see [certificate scope and platform differences](docs/CERTIFICATE_SCOPE.md).
2. Enter an HTTPS API base URL, protocol, provider key, and exact model ID. **Save** saves without sending requests; **Save and connect** saves and sends two small checks that your provider may bill.
3. In the Claude Office add-in, choose **Gateway**, **Anthropic** format, and **x-api-key** authentication. Copy the URL, gateway token, and Office model ID from the client. Paste the token without `Bearer`.

Keep the client running while using Office. Closing its window keeps the connection alive; quitting stops it. Reopen from the Dock or tray. After relaunch, click **Connect gateway**; a saved provider key does not need to be re-entered.

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

Collected installers and application bundles are written to `artifacts/macos/` or `artifacts/windows/`. Dependencies are locked in `package-lock.json` and `src-tauri/Cargo.lock`. The macOS build uses ad-hoc signing for local use; public distribution requires Developer ID signing and notarization.

## Upgrading the Swift client

Quit the Swift client first so it releases port `15724`. The macOS client reuses `~/Library/Application Support/Claude Office Gateway/settings.json` and the original Keychain service. Camel-case Swift settings are read directly; the first save retains a `settings.swift-backup.json`. The existing PKCS#12 identity is converted locally to a private PEM key when needed, retaining the certificate and its trust.

An independent community project, not affiliated with Anthropic or Microsoft. [MIT License](LICENSE).
