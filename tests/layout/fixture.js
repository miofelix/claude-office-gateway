import { mockIPC, mockWindows } from "@tauri-apps/api/mocks";
import { defaultConfig } from "../../src/lib/config";
import { version } from "../../package.json";
import { inspectLayout } from "./inspect.js";

// Exercise the production React tree and styles with synthetic IPC only.
const params = new URLSearchParams(location.search);
const scenario = params.get("scenario") ?? "stopped";
const language = params.get("language") ?? "en";
localStorage.setItem("office-gateway-language", language);
localStorage.setItem(
  "office-gateway-appearance",
  params.get("theme") ?? "light",
);
localStorage.setItem("office-gateway-auto-check-updates", "false");
const longText = "https://provider.example/" + "diagnosticsegment".repeat(32);
const state = {
  active_provider_id: "default",
  providers: [
    {
      id: "default",
      name: "Fixture provider",
      base_url: "https://provider.example",
      upstream_model: "example-model",
      public_model: "claude-" + "long-model-alias".repeat(8),
    },
  ],
  config: {
    ...defaultConfig,
    base_url: "https://provider.example",
    upstream_model: "example-model",
    public_model: "claude-" + "long-model-alias".repeat(8),
  },
  has_upstream_key: true,
  gateway_token: "synthetic-gateway-token",
  certificate: { exists: true, trusted: false, subject: "localhost" },
  runtime: {
    phase:
      scenario === "ready"
        ? "ready"
        : scenario === "error"
          ? "failed"
          : "stopped",
    running: scenario === "ready",
    ...(scenario === "error" ? { error: longText } : {}),
  },
  platform: params.get("platform") ?? "macos",
  test_mode: true,
};
const updates = {
  current_version: version,
  channel: "beta",
  phase:
    scenario === "update"
      ? "available"
      : scenario === "update-error"
        ? "error"
        : "idle",
  available:
    scenario === "update"
      ? {
          version: "0.3.0-beta.1",
          notes: ("Release notes: " + longText + "\n").repeat(14),
        }
      : null,
  downloaded: 0,
  total: null,
  checked_at: null,
  error: scenario === "update-error" ? longText : null,
};
window.isTauri = true;
mockWindows("main");
mockIPC(
  (command) => {
    if (command === "get_app_state") return structuredClone(state);
    if (command === "get_update_state" || command === "check_for_updates")
      return structuredClone(updates);
    if (
      command === "copy_connection_detail" ||
      command === "set_tray_language" ||
      command === "show_main_window" ||
      command === "plugin:window|set_theme"
    )
      return;
    throw new Error("Unexpected layout fixture IPC: " + command);
  },
  { shouldMockEvents: true },
);
window.inspectLayout = inspectLayout;
await import("../../src/main.tsx");
