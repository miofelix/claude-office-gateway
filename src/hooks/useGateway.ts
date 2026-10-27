import { useEffect, useRef, useState } from "react";
import { defaultConfig, draftError, initialState } from "../lib/config";
import {
  appState,
  connectGateway,
  deleteCertificate,
  installCertificate,
  recheckGateway,
  saveSettings,
  stopGateway,
  type AppState,
} from "../lib/tauri";
export type Notice = { kind: "success" | "error"; text: string };
export function useGateway() {
  const [state, setState] = useState(initialState);
  const [draft, setDraft] = useState(defaultConfig);
  const [upstreamKey, setUpstreamKey] = useState("");
  const [busy, setBusy] = useState<string | null>("loading");
  const [notice, setNotice] = useState<Notice | null>(null);
  const operation = useRef(0),
    working = useRef(false),
    initialized = useRef(false),
    dirty = useRef(false);
  dirty.current =
    JSON.stringify(draft) !== JSON.stringify(state.config) ||
    upstreamKey.length > 0;
  const error = draftError(
    draft,
    state.has_upstream_key || upstreamKey.trim().length > 0,
  );
  const editable =
    !busy &&
    !state.runtime.running &&
    !["starting", "checking"].includes(state.runtime.phase);
  async function run(
    label: string,
    action: (current: () => boolean) => Promise<AppState>,
    options: { replaceDraft?: boolean; interrupt?: boolean } = {},
  ) {
    if (working.current && !options.interrupt) return false;
    working.current = true;
    const id = ++operation.current;
    setBusy(label);
    setNotice(null);
    try {
      const next = await action(() => id === operation.current);
      if (id !== operation.current) return false;
      setState(next);
      if (options.replaceDraft) {
        setDraft(next.config);
        setUpstreamKey("");
      }
      if (next.runtime.error)
        setNotice({ kind: "error", text: next.runtime.error });
      else if (label !== "loading" && label !== "refreshing")
        setNotice({ kind: "success", text: successText(label) });
      return true;
    } catch (error) {
      if (id === operation.current)
        setNotice({
          kind: "error",
          text: error instanceof Error ? error.message : String(error),
        });
      return false;
    } finally {
      if (id === operation.current) {
        working.current = false;
        setBusy(null);
      }
    }
  }
  useEffect(() => {
    let mounted = true;
    if (!initialized.current) {
      initialized.current = true;
      void run("loading", appState, { replaceDraft: true });
    }
    async function poll() {
      const id = operation.current;
      try {
        const next = await appState();
        if (!mounted || id !== operation.current) return;
        setState(next);
        if (!dirty.current && !working.current) setDraft(next.config);
      } catch {
        /* Explicit refresh exposes failures without repeatedly interrupting editing. */
      }
    }
    const timer = window.setInterval(() => {
      void poll();
    }, 5000);
    window.addEventListener("focus", poll);
    return () => {
      mounted = false;
      clearInterval(timer);
      window.removeEventListener("focus", poll);
    };
  }, []);
  async function save() {
    if (error) {
      setNotice({ kind: "error", text: error });
      return false;
    }
    return run(
      "saving",
      () => saveSettings({ config: draft, upstream_key: upstreamKey }),
      { replaceDraft: true },
    );
  }
  async function connect() {
    if (error) {
      setNotice({ kind: "error", text: error });
      return;
    }
    await run(
      "connecting",
      async (current) => {
        let saved = state;
        if (dirty.current) {
          saved = await saveSettings({
            config: draft,
            upstream_key: upstreamKey,
          });
        }
        if (!current()) return saved;
        return connectGateway();
      },
      { replaceDraft: true },
    );
  }
  return {
    state,
    draft,
    setDraft,
    upstreamKey,
    setUpstreamKey,
    busy,
    notice,
    setNotice,
    error,
    editable,
    dirty: dirty.current,
    canConnect:
      !error && state.certificate.trusted && !busy && !state.runtime.running,
    save,
    connect,
    refresh: () => run("refreshing", appState),
    stop: () => run("stopping", stopGateway, { interrupt: true }),
    recheck: () => run("rechecking", recheckGateway),
    install: () => run("installing", installCertificate),
    remove: () => run("deleting", deleteCertificate),
  };
}
function successText(label: string) {
  return (
    {
      saving: "Provider settings saved securely.",
      connecting: "The gateway is ready for Office.",
      stopping: "The local gateway stopped.",
      rechecking: "Provider check completed.",
      installing: "The localhost certificate is trusted.",
      deleting: "The local certificate was removed.",
    }[label] ?? "Done."
  );
}
