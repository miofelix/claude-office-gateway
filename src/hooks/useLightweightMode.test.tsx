// @vitest-environment jsdom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useLightweightMode } from "./useLightweightMode";
const api = vi.hoisted(() => ({
  enterLightweightMode: vi.fn(),
  showMainWindow: vi.fn(),
  listen: vi.fn(),
  stop: vi.fn(),
}));
vi.mock("../lib/tauri", () => api);
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true }));
vi.mock("@tauri-apps/api/event", () => ({ listen: api.listen }));
beforeEach(() => {
  vi.resetAllMocks();
  api.enterLightweightMode.mockResolvedValue(undefined);
  api.showMainWindow.mockResolvedValue(undefined);
  api.listen.mockResolvedValue(api.stop);
});
afterEach(cleanup);
it("uses the latest dirty state for tray requests and keeps the draft on cancel", async () => {
  const onError = vi.fn();
  const { result, rerender } = renderHook(
    (dirty: boolean) => useLightweightMode({ dirty, busy: false, onError }),
    { initialProps: false },
  );
  await act(async () => {});
  rerender(true);
  const handler = api.listen.mock.calls[0][1];
  act(() => handler());
  expect(result.current.confirmDiscard).toBe(true);
  expect(api.enterLightweightMode).not.toHaveBeenCalled();
  act(() => result.current.cancel());
  expect(result.current.confirmDiscard).toBe(false);
  expect(api.enterLightweightMode).not.toHaveBeenCalled();
  act(() => handler());
  await act(() => result.current.enter());
  expect(api.enterLightweightMode).toHaveBeenCalledOnce();
});
it("blocks busy work and surfaces an explanation without destroying the view", async () => {
  const onError = vi.fn();
  const { result } = renderHook(() =>
    useLightweightMode({ dirty: false, busy: true, onError }),
  );
  await act(async () => result.current.request());
  expect(api.enterLightweightMode).not.toHaveBeenCalled();
  expect(onError).toHaveBeenCalledWith(
    "Wait for the current operation before entering lightweight mode.",
  );
  expect(api.showMainWindow).toHaveBeenCalledOnce();
});
it("deduplicates entry and leaves the window recoverable on native failure", async () => {
  let reject!: (message: string) => void;
  api.enterLightweightMode.mockImplementation(
    () =>
      new Promise((_, fail) => {
        reject = fail;
      }),
  );
  const onError = vi.fn();
  const { result } = renderHook(() =>
    useLightweightMode({ dirty: false, busy: false, onError }),
  );
  act(() => {
    result.current.request();
    result.current.request();
  });
  expect(api.enterLightweightMode).toHaveBeenCalledOnce();
  await act(async () => reject("Tray unavailable"));
  expect(onError).toHaveBeenCalledWith("Tray unavailable");
  expect(api.showMainWindow).toHaveBeenCalledOnce();
});
it("unsubscribes when the window is released, including a late listener registration", async () => {
  let resolve!: (stop: () => void) => void;
  api.listen.mockImplementation(
    () =>
      new Promise((done) => {
        resolve = done;
      }),
  );
  const { unmount } = renderHook(() =>
    useLightweightMode({ dirty: false, busy: false, onError: vi.fn() }),
  );
  unmount();
  await act(async () => resolve(api.stop));
  expect(api.stop).toHaveBeenCalledOnce();
});
