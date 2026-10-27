// @vitest-environment jsdom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { initialUpdateState, type UpdateSnapshot } from "../lib/updates";
import { useAppUpdates } from "./useAppUpdates";
const api = vi.hoisted(() => ({ invoke: vi.fn(), isTauri: vi.fn(() => true) }));
vi.mock("@tauri-apps/api/core", () => api);
const available = (): UpdateSnapshot => ({
  ...initialUpdateState(),
  phase: "available",
  available: { version: "0.2.0-beta.2", notes: "A new release" },
});
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => {
    resolve = done;
  });
  return { promise, resolve };
}
beforeEach(() => {
  vi.useFakeTimers();
  vi.clearAllMocks();
  const stored = new Map<string, string>();
  Object.defineProperty(window, "localStorage", {
    configurable: true,
    value: {
      getItem: (key: string) => stored.get(key) ?? null,
      setItem: (key: string, value: string) => stored.set(key, value),
    },
  });
  api.invoke.mockResolvedValue(initialUpdateState());
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
});
describe("signed application updates", () => {
  it("shows the native version and channel", async () => {
    api.invoke.mockResolvedValue({
      ...initialUpdateState(),
      current_version: "0.3.0",
      channel: "stable",
    });
    const { result } = renderHook(useAppUpdates);
    await act(async () => {});
    expect(result.current.state.current_version).toBe("0.3.0");
    expect(result.current.state.channel).toBe("stable");
  });
  it("automatically checks once and never installs without a click", async () => {
    api.invoke.mockImplementation((command: string) =>
      Promise.resolve(
        command === "check_for_updates" ? available() : initialUpdateState(),
      ),
    );
    const { result } = renderHook(useAppUpdates);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000);
    });
    expect(result.current.hasUpdate).toBe(true);
    expect(
      api.invoke.mock.calls.filter(([name]) => name === "check_for_updates"),
    ).toHaveLength(1);
    expect(
      api.invoke.mock.calls.some(([name]) => name === "install_app_update"),
    ).toBe(false);
  });
  it("persists the disabled startup check preference", async () => {
    window.localStorage.setItem("office-gateway-auto-check-updates", "false");
    const { result } = renderHook(useAppUpdates);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000);
    });
    expect(result.current.autoCheck).toBe(false);
    expect(api.invoke).toHaveBeenCalledExactlyOnceWith("get_update_state");
    act(() => result.current.setAutoCheck(true));
    expect(
      window.localStorage.getItem("office-gateway-auto-check-updates"),
    ).toBe("true");
  });
  it("deduplicates checks and passes the reviewed version to install", async () => {
    const check = deferred<UpdateSnapshot>();
    api.invoke.mockImplementation((command: string) =>
      command === "check_for_updates"
        ? check.promise
        : Promise.resolve(initialUpdateState()),
    );
    const { result } = renderHook(useAppUpdates);
    await act(async () => {});
    let running!: Promise<void>;
    act(() => {
      running = result.current.check();
      void result.current.check();
    });
    await act(async () => {
      check.resolve(available());
      await running;
    });
    expect(
      api.invoke.mock.calls.filter(([name]) => name === "check_for_updates"),
    ).toHaveLength(1);
    await act(() => result.current.install());
    expect(api.invoke).toHaveBeenCalledWith("install_app_update", {
      expectedVersion: "0.2.0-beta.2",
    });
  });
  it("does not let a stale initial read overwrite a completed check", async () => {
    const initial = deferred<UpdateSnapshot>();
    api.invoke.mockImplementation((command: string) =>
      command === "get_update_state"
        ? initial.promise
        : Promise.resolve(available()),
    );
    const { result } = renderHook(useAppUpdates);
    await act(() => result.current.check());
    await act(async () => {
      initial.resolve(initialUpdateState());
    });
    expect(result.current.state.phase).toBe("available");
  });
  it("ignores an in-flight progress poll after the operation finishes", async () => {
    const progress = deferred<UpdateSnapshot>(),
      check = deferred<UpdateSnapshot>();
    api.invoke
      .mockResolvedValueOnce(initialUpdateState())
      .mockImplementation((command: string) =>
        command === "get_update_state" ? progress.promise : check.promise,
      );
    const { result } = renderHook(useAppUpdates);
    await act(async () => {});
    let running!: Promise<void>;
    act(() => {
      running = result.current.check();
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(700);
    });
    await act(async () => {
      check.resolve(available());
      await running;
    });
    await act(async () => {
      progress.resolve({ ...initialUpdateState(), phase: "checking" });
    });
    expect(result.current.state.phase).toBe("available");
  });
  it("exposes a retryable network or signature error", async () => {
    const { result } = renderHook(useAppUpdates);
    await act(async () => {});
    api.invoke.mockRejectedValueOnce("Signature verification failed");
    await act(() => result.current.check());
    expect(result.current.state.error).toBe("Signature verification failed");
    expect(result.current.busy).toBe(false);
    api.invoke.mockResolvedValueOnce(available());
    await act(() => result.current.check());
    expect(result.current.hasUpdate).toBe(true);
    expect(result.current.state.error).toBeNull();
  });
});
