import { invoke } from "@tauri-apps/api/core";
import { version } from "../../package.json";
export const BUILD_VERSION = version;
export const RELEASES_URL =
  "https://github.com/miofelix/claude-office-gateway/releases";
export type UpdatePhase =
  | "idle"
  | "checking"
  | "up_to_date"
  | "available"
  | "downloading"
  | "installing"
  | "restarting"
  | "error";
export interface UpdateRelease {
  version: string;
  notes?: string | null;
  published_at?: string | null;
}
export interface UpdateSnapshot {
  current_version: string;
  channel: "beta" | "stable";
  phase: UpdatePhase;
  available: UpdateRelease | null;
  downloaded: number;
  total: number | null;
  checked_at: string | null;
  error: string | null;
}
export function initialUpdateState(): UpdateSnapshot {
  return {
    current_version: BUILD_VERSION,
    channel: BUILD_VERSION.includes("-") ? "beta" : "stable",
    phase: "idle",
    available: null,
    downloaded: 0,
    total: null,
    checked_at: null,
    error: null,
  };
}
export const getUpdateState = () => invoke<UpdateSnapshot>("get_update_state");
export const checkForUpdates = () =>
  invoke<UpdateSnapshot>("check_for_updates");
export const installAppUpdate = (expectedVersion: string) =>
  invoke<UpdateSnapshot>("install_app_update", { expectedVersion });
export const releaseNotesUrl = (version: string) =>
  RELEASES_URL + "/tag/v" + encodeURIComponent(version);
