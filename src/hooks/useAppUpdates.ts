import { useCallback, useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import {
  checkForUpdates,
  getUpdateState,
  initialUpdateState,
  installAppUpdate,
  type UpdateSnapshot,
} from "../lib/updates";
const autoCheckKey = "office-gateway-auto-check-updates";
function readAutoCheck() {
  try {
    return window.localStorage?.getItem(autoCheckKey) !== "false";
  } catch {
    return true;
  }
}
export function useAppUpdates() {
  const [state, setState] = useState(initialUpdateState);
  const [autoCheck, setAutoCheck] = useState(readAutoCheck);
  const checked = useRef(false),
    pending = useRef(false),
    mounted = useRef(true),
    revision = useRef(0),
    refreshSequence = useRef(0);
  const refresh = useCallback(async () => {
    const generation = revision.current;
    const sequence = ++refreshSequence.current;
    try {
      const next = await getUpdateState();
      if (next.checked_at || next.phase !== "idle") checked.current = true;
      if (
        mounted.current &&
        generation === revision.current &&
        sequence === refreshSequence.current &&
        (!pending.current ||
          ["checking", "downloading", "installing", "restarting"].includes(
            next.phase,
          ))
      )
        setState(next);
    } catch {
      /* A manual check provides a visible retryable error. */
    }
  }, []);
  const run = useCallback(
    async (
      action: () => Promise<UpdateSnapshot>,
      phase: "checking" | "downloading",
    ) => {
      if (pending.current) return;
      pending.current = true;
      ++revision.current;
      checked.current = true;
      setState((old) => ({ ...old, phase, error: null }));
      try {
        const next = await action();
        if (mounted.current) setState(next);
      } catch (error) {
        if (mounted.current)
          setState((old) => ({
            ...old,
            phase: "error",
            error: error instanceof Error ? error.message : String(error),
          }));
      } finally {
        ++revision.current;
        pending.current = false;
      }
    },
    [],
  );
  const check = useCallback(() => run(checkForUpdates, "checking"), [run]);
  useEffect(() => {
    mounted.current = true;
    if (isTauri()) void refresh();
    return () => {
      mounted.current = false;
      ++revision.current;
    };
  }, [refresh]);
  useEffect(() => {
    try {
      window.localStorage?.setItem(autoCheckKey, String(autoCheck));
    } catch {}
    if (!autoCheck || !isTauri()) return;
    const timer = window.setTimeout(() => {
      if (!checked.current) void check();
    }, 1500);
    return () => clearTimeout(timer);
  }, [autoCheck, check]);
  const busy = ["checking", "downloading", "installing", "restarting"].includes(
    state.phase,
  );
  useEffect(() => {
    if (!busy || !isTauri()) return;
    const timer = window.setInterval(() => void refresh(), 700);
    return () => clearInterval(timer);
  }, [busy, refresh]);
  const install = useCallback(() => {
    const version = state.available?.version;
    if (!version || state.phase !== "available") return Promise.resolve();
    return run(() => installAppUpdate(version), "downloading");
  }, [state.available?.version, state.phase, run]);
  return {
    state,
    autoCheck,
    setAutoCheck,
    check,
    install,
    busy,
    hasUpdate: state.phase === "available" && state.available !== null,
  };
}
