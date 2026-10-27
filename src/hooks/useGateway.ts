import { useEffect, useRef, useState } from "react";
import { defaultConfig, draftError, initialState } from "../lib/config";
import {
  appState,
  connectGateway,
  deleteCertificate,
  installCertificate,
  recheckGateway,
  saveSettings,
  selectProvider,
  stopGateway,
  type AppState,
} from "../lib/tauri";
export type Notice = { kind: "success" | "error"; text: string };
export function useGateway() {
  const [state, setState] = useState(initialState);
  const [draft, setDraft] = useState(defaultConfig);
  const [draftProviderId, setDraftProviderId] = useState<string | null>(
    "default",
  );
  const [draftProviderName, setDraftProviderName] = useState("Custom model");
  const [upstreamKey, setUpstreamKey] = useState("");
  const [busy, setBusy] = useState<string | null>("loading");
  const [notice, setNotice] = useState<Notice | null>(null);
  const operation = useRef(0),
    working = useRef(false),
    initialized = useRef(false),
    dirty = useRef(false);
  function providerName(next: AppState) {
    return (
      next.providers.find((provider) => provider.id === next.active_provider_id)
        ?.name ?? "Custom model"
    );
  }
  function replaceDraft(next: AppState) {
    setDraft(next.config);
    setDraftProviderId(next.active_provider_id);
    setDraftProviderName(providerName(next));
    setUpstreamKey("");
  }
  dirty.current =
    draftProviderId !== state.active_provider_id ||
    draftProviderName !== providerName(state) ||
    JSON.stringify(draft) !== JSON.stringify(state.config) ||
    upstreamKey.length > 0;
  const hasDraftSavedKey =
    draftProviderId === state.active_provider_id && state.has_upstream_key;
  const error = !draftProviderName.trim()
    ? "Enter a provider name."
    : draftError(draft, hasDraftSavedKey || upstreamKey.trim().length > 0);
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
        replaceDraft(next);
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
        if (!dirty.current && !working.current) replaceDraft(next);
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
      () =>
        saveSettings({
          provider_id: draftProviderId,
          name: draftProviderName,
          config: draft,
          upstream_key: upstreamKey,
        }),
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
            provider_id: draftProviderId,
            name: draftProviderName,
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
  function newProvider() {
    if (!editable) {
      setNotice({
        kind: "error",
        text: "Stop the gateway before editing provider settings.",
      });
      return false;
    }
    setNotice(null);
    setDraft(defaultConfig);
    setDraftProviderId(null);
    setDraftProviderName("");
    setUpstreamKey("");
    return true;
  }
  async function switchProvider(id: string) {
    if (id === "__new__") return newProvider();
    if (dirty.current) {
      setNotice({
        kind: "error",
        text: "Save or discard the current provider changes before switching.",
      });
      return false;
    }
    return run("switching", () => selectProvider(id), { replaceDraft: true });
  }
  function discard() {
    replaceDraft(state);
    setNotice(null);
  }
  return {
    state,
    draft,
    setDraft,
    draftProviderId,
    draftProviderName,
    setDraftProviderName,
    hasDraftSavedKey,
    upstreamKey,
    setUpstreamKey,
    busy,
    notice,
    setNotice,
    error,
    editable,
    dirty: dirty.current,
    newProvider,
    switchProvider,
    discard,
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
