import { useCallback, useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { enterLightweightMode, showMainWindow } from "../lib/tauri";

type Options = {
  dirty: boolean;
  busy: boolean;
  onError: (message: string) => void;
};
export function useLightweightMode(options: Options) {
  const current = useRef(options);
  current.current = options;
  const pending = useRef(false);
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const enter = useCallback(async () => {
    if (pending.current) return;
    if (current.current.busy) {
      current.current.onError(
        "Wait for the current operation before entering lightweight mode.",
      );
      void showMainWindow().catch(() => {});
      return;
    }
    pending.current = true;
    setConfirmDiscard(false);
    try {
      await enterLightweightMode();
    } catch (error) {
      current.current.onError(String(error));
      void showMainWindow().catch(() => {});
    } finally {
      pending.current = false;
    }
  }, []);
  const request = useCallback(() => {
    if (pending.current) return;
    if (current.current.dirty && !current.current.busy) {
      setConfirmDiscard(true);
      void showMainWindow().catch(() => {});
    } else {
      void enter();
    }
  }, [enter]);
  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen("request-lightweight-mode", request)
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [request]);
  return {
    request,
    enter,
    confirmDiscard,
    cancel: () => setConfirmDiscard(false),
  };
}
