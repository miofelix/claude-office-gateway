import { invoke } from "@tauri-apps/api/core";

export type Protocol = "anthropic" | "openai_chat" | "openai_responses";
export type GatewayPhase =
  | "stopped"
  | "starting"
  | "checking"
  | "needs_trust"
  | "ready"
  | "failed";

export interface GatewayConfig {
  port: number;
  base_url: string;
  upstream_protocol: Protocol;
  upstream_model: string;
  public_model: string;
  display_name: string;
  auth_header: "authorization" | "x-api-key";
  max_tokens_field: "max_tokens" | "max_completion_tokens";
  timeout_seconds: number;
  allowed_origins: string[];
}

export interface CertificateState {
  exists: boolean;
  trusted: boolean;
  subject: string;
}

export interface RuntimeState {
  phase: GatewayPhase;
  running: boolean;
  verified_at?: string;
  verification_ms?: number;
  error?: string;
}

export interface AppState {
  config: GatewayConfig;
  has_upstream_key: boolean;
  gateway_token: string;
  certificate: CertificateState;
  runtime: RuntimeState;
  platform: "macos" | "windows";
}

export async function appState(): Promise<AppState> {
  return invoke<AppState>("get_app_state");
}

export async function installCertificate(): Promise<AppState> {
  return invoke<AppState>("install_certificate");
}

export async function deleteCertificate(): Promise<AppState> {
  return invoke<AppState>("delete_certificate");
}

export async function saveSettings(input: {
  config: GatewayConfig;
  upstream_key: string;
}): Promise<AppState> {
  return invoke<AppState>("save_settings", { input });
}

export async function connectGateway(): Promise<AppState> {
  return invoke<AppState>("connect_gateway");
}

export async function stopGateway(): Promise<AppState> {
  return invoke<AppState>("stop_gateway");
}

export async function recheckGateway(): Promise<AppState> {
  return invoke<AppState>("recheck_gateway");
}

