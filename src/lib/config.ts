import type { AppState, GatewayConfig, GatewayPhase } from "./tauri";
export const defaultConfig: GatewayConfig = {
  port: 15724,
  base_url: "",
  upstream_protocol: "openai_chat",
  upstream_model: "",
  public_model: "claude-office-custom",
  display_name: "Custom model",
  auth_header: "authorization",
  max_tokens_field: "max_completion_tokens",
  timeout_seconds: 120,
  allowed_origins: ["https://pivot.claude.ai"],
  probe_cache_seconds: 300,
  max_request_bytes: 32 * 1024 * 1024,
};
export const initialState = (): AppState => ({
  active_provider_id: "default",
  providers: [
    {
      id: "default",
      name: "Custom model",
      base_url: "",
      upstream_model: "",
      public_model: defaultConfig.public_model,
    },
  ],
  config: defaultConfig,
  has_upstream_key: false,
  gateway_token: "",
  certificate: {
    exists: false,
    trusted: false,
    subject: "Claude Office Gateway localhost",
  },
  runtime: { phase: "stopped", running: false },
  platform: navigator.userAgent.includes("Mac") ? "macos" : "windows",
});
export function phaseLabel(phase: GatewayPhase) {
  return {
    stopped: "Not connected",
    starting: "Starting local gateway",
    checking: "Checking provider",
    needs_trust: "Certificate needs attention",
    ready: "Ready for Office",
    failed: "Needs attention",
  }[phase];
}
export function providerHost(value: string) {
  try {
    return value ? new URL(value).host : "your provider";
  } catch {
    return "your provider";
  }
}
export function usesDeepSeekToolCompatibility(config: GatewayConfig) {
  try {
    return (
      new URL(config.base_url).hostname === "api.deepseek.com" &&
      config.upstream_protocol !== "openai_responses"
    );
  } catch {
    return false;
  }
}
export function draftError(draft: GatewayConfig, hasKey: boolean) {
  try {
    const url = new URL(draft.base_url);
    if (
      url.protocol !== "https:" ||
      url.username ||
      url.password ||
      url.search ||
      url.hash
    )
      return "Enter an HTTPS API base URL without credentials or query parameters.";
  } catch {
    return "Enter an HTTPS API base URL.";
  }
  if (!draft.upstream_model.trim()) return "Enter the exact provider model ID.";
  if (!hasKey) return "Enter a provider API key.";
  if (!/^[a-zA-Z0-9][a-zA-Z0-9._-]{0,127}$/.test(draft.public_model))
    return "Enter a valid Office model ID.";
  if (!Number.isInteger(draft.port) || draft.port < 1024 || draft.port > 65535)
    return "Choose a local port between 1024 and 65535.";
  if (
    !Number.isInteger(draft.timeout_seconds) ||
    draft.timeout_seconds < 1 ||
    draft.timeout_seconds > 600
  )
    return "Provider timeout must be between 1 and 600 seconds.";
  return null;
}
