// @vitest-environment jsdom
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { defaultConfig, initialState } from "../lib/config";
import type { AppState } from "../lib/tauri";
import { useGateway } from "./useGateway";
const api = vi.hoisted(() => ({
  appState: vi.fn(),
  saveSettings: vi.fn(),
  connectGateway: vi.fn(),
  stopGateway: vi.fn(),
  recheckGateway: vi.fn(),
  installCertificate: vi.fn(),
  deleteCertificate: vi.fn(),
}));
vi.mock("../lib/tauri", () => api);
function saved(): AppState {
  return {
    ...initialState(),
    config: {
      ...defaultConfig,
      base_url: "https://provider.example",
      upstream_model: "real-model",
    },
    has_upstream_key: true,
    certificate: { exists: true, trusted: true, subject: "localhost" },
  };
}
beforeEach(() => {
  vi.resetAllMocks();
  api.appState.mockResolvedValue(saved());
});
afterEach(() => {
  cleanup();
});
describe("gateway editing and operation lifecycle", () => {
  it("does not start a listener if cancelled while the draft is saving", async () => {
    let finish!: (state: AppState) => void;
    api.saveSettings.mockImplementation(
      () =>
        new Promise<AppState>((resolve) => {
          finish = resolve;
        }),
    );
    api.stopGateway.mockResolvedValue(saved());
    const { result } = renderHook(useGateway);
    await waitFor(() => expect(result.current.busy).toBeNull());
    act(() => result.current.setUpstreamKey("pending-key"));
    let connecting!: Promise<void>;
    act(() => {
      connecting = result.current.connect();
    });
    await waitFor(() => expect(api.saveSettings).toHaveBeenCalledOnce());
    await act(() => result.current.stop());
    await act(async () => {
      finish(saved());
      await connecting;
    });
    expect(api.connectGateway).not.toHaveBeenCalled();
    expect(result.current.state.runtime.phase).toBe("stopped");
  });

  it("reconnects with the stored credential without requesting a new key", async () => {
    api.connectGateway.mockResolvedValue({
      ...saved(),
      runtime: { phase: "ready", running: true },
    });
    const { result } = renderHook(useGateway);
    await waitFor(() => expect(result.current.busy).toBeNull());
    expect(result.current.canConnect).toBe(true);
    await act(() => result.current.connect());
    expect(api.saveSettings).not.toHaveBeenCalled();
    expect(api.connectGateway).toHaveBeenCalledOnce();
  });
  it("preserves an unsaved provider draft when refreshing runtime state", async () => {
    const { result } = renderHook(useGateway);
    await waitFor(() => expect(result.current.busy).toBeNull());
    act(() =>
      result.current.setDraft({
        ...saved().config,
        upstream_model: "new-draft",
      }),
    );
    await act(() => result.current.refresh());
    expect(result.current.draft.upstream_model).toBe("new-draft");
  });
  it("stops after a save error and keeps the entered key for correction", async () => {
    api.saveSettings.mockRejectedValue("Keychain access denied");
    const { result } = renderHook(useGateway);
    await waitFor(() => expect(result.current.busy).toBeNull());
    act(() => result.current.setUpstreamKey("new-key"));
    await act(() => result.current.connect());
    expect(api.connectGateway).not.toHaveBeenCalled();
    expect(result.current.upstreamKey).toBe("new-key");
    expect(result.current.notice?.kind).toBe("error");
  });
  it("does not overwrite a stopped state when an old connect result resolves", async () => {
    let finish!: (state: AppState) => void;
    api.connectGateway.mockImplementation(
      () =>
        new Promise<AppState>((resolve) => {
          finish = resolve;
        }),
    );
    api.stopGateway.mockResolvedValue(saved());
    const { result } = renderHook(useGateway);
    await waitFor(() => expect(result.current.busy).toBeNull());
    let connecting!: Promise<void>;
    act(() => {
      connecting = result.current.connect();
    });
    await waitFor(() => expect(api.connectGateway).toHaveBeenCalledOnce());
    await act(() => result.current.stop());
    await act(async () => {
      finish({ ...saved(), runtime: { phase: "ready", running: true } });
      await connecting;
    });
    expect(result.current.state.runtime.phase).toBe("stopped");
    expect(result.current.busy).toBeNull();
  });
  it("shows a failed provider verification as an error", async () => {
    api.connectGateway.mockResolvedValue({
      ...saved(),
      runtime: {
        phase: "failed",
        running: false,
        error: "The provider rejected the key.",
      },
    });
    const { result } = renderHook(useGateway);
    await waitFor(() => expect(result.current.busy).toBeNull());
    await act(() => result.current.connect());
    expect(result.current.notice).toEqual({
      kind: "error",
      text: "The provider rejected the key.",
    });
  });
});
