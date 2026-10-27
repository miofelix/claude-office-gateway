// @vitest-environment jsdom
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useAppearance } from "./useAppearance";
const native = vi.hoisted(() => ({
  setTheme: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => native }));
let stored: Map<string, string>;
let dark = false;
let listeners: Set<(event: MediaQueryListEvent) => void>;
function systemTheme(next: boolean) {
  dark = next;
  for (const listener of listeners)
    listener({ matches: next } as MediaQueryListEvent);
}
beforeEach(() => {
  dark = false;
  stored = new Map();
  listeners = new Set();
  native.setTheme.mockClear();
  Object.defineProperty(window, "localStorage", {
    configurable: true,
    value: {
      getItem: (key: string) => stored.get(key) ?? null,
      setItem: (key: string, value: string) => stored.set(key, value),
    },
  });
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: () => ({
      get matches() {
        return dark;
      },
      addEventListener: (
        _event: string,
        listener: (event: MediaQueryListEvent) => void,
      ) => listeners.add(listener),
      removeEventListener: (
        _event: string,
        listener: (event: MediaQueryListEvent) => void,
      ) => listeners.delete(listener),
    }),
  });
});
afterEach(() => {
  cleanup();
  delete document.documentElement.dataset.theme;
});
describe("appearance preferences", () => {
  it("defaults to following system appearance and responds to system changes", () => {
    const { result } = renderHook(useAppearance);
    expect(result.current.appearance).toBe("system");
    expect(result.current.theme).toBe("light");
    expect(native.setTheme).toHaveBeenLastCalledWith(null);
    act(() => systemTheme(true));
    expect(result.current.theme).toBe("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
  });
  it("keeps a chosen light appearance when system appearance changes", () => {
    dark = true;
    const { result } = renderHook(useAppearance);
    act(() => result.current.setAppearance("light"));
    act(() => systemTheme(false));
    act(() => systemTheme(true));
    expect(result.current.theme).toBe("light");
    expect(native.setTheme).toHaveBeenLastCalledWith("light");
    expect(stored.get("office-gateway-appearance")).toBe("light");
  });
  it("restores the saved dark preference after remounting", () => {
    const first = renderHook(useAppearance);
    act(() => first.result.current.setAppearance("dark"));
    first.unmount();
    const second = renderHook(useAppearance);
    expect(second.result.current.appearance).toBe("dark");
    expect(second.result.current.theme).toBe("dark");
    expect(native.setTheme).toHaveBeenLastCalledWith("dark");
  });
  it("resets the native override when switching back to follow system", () => {
    const { result } = renderHook(useAppearance);
    act(() => result.current.setAppearance("dark"));
    act(() => result.current.setAppearance("system"));
    expect(native.setTheme).toHaveBeenLastCalledWith(null);
    expect(result.current.theme).toBe("light");
    act(() => systemTheme(true));
    expect(result.current.theme).toBe("dark");
  });
});
