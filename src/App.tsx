import { useEffect, useMemo, useState } from "react";
import {
  BookOpen,
  Check,
  ChevronRight,
  CircleAlert,
  CircleHelp,
  Clipboard,
  ExternalLink,
  FileKey2,
  Gauge,
  KeyRound,
  Laptop,
  Link2,
  LoaderCircle,
  LockKeyhole,
  Menu,
  Network,
  Play,
  RefreshCw,
  Settings2,
  ShieldCheck,
  SquareTerminal,
  StopCircle,
  Trash2,
  X,
} from "lucide-react";
import logo from "./assets/logo.svg";
import {
  appState,
  connectGateway,
  deleteCertificate,
  installCertificate,
  recheckGateway,
  saveSettings,
  stopGateway,
  type AppState,
  type GatewayConfig,
  type GatewayPhase,
  type Protocol,
} from "./lib/tauri";

type Page = "connection" | "guide" | "settings";

const defaultConfig: GatewayConfig = {
  port: 15724,
  base_url: "",
  upstream_protocol: "openai_chat",
  upstream_model: "",
  public_model: "claude-office-custom",
  display_name: "Custom model",
  auth_header: "authorization",
  max_tokens_field: "max_completion_tokens",
  timeout_seconds: 120,
  allowed_origins: ["https://pivot.claude.ai"],
};

const addIns = [
  { name: "PowerPoint", id: "WA200010001" },
  { name: "Excel", id: "WA200009404" },
  { name: "Word", id: "WA200010453" },
];

function initialState(): AppState {
  return {
    config: defaultConfig,
    has_upstream_key: false,
    gateway_token: "",
    certificate: { exists: false, trusted: false, subject: "Claude Office Gateway localhost" },
    runtime: { phase: "stopped", running: false },
    platform: navigator.userAgent.includes("Mac") ? "macos" : "windows",
  };
}

function phaseLabel(phase: GatewayPhase) {
  return {
    stopped: "Not connected",
    starting: "Starting local gateway",
    checking: "Checking provider",
    needs_trust: "Certificate needs attention",
    ready: "Ready for Office",
    failed: "Needs attention",
  }[phase];
}

function protocolLabel(protocol: Protocol) {
  return {
    anthropic: "Anthropic Messages",
    openai_chat: "OpenAI Chat Completions",
    openai_responses: "OpenAI Responses",
  }[protocol];
}

function App() {
  const [page, setPage] = useState<Page>("connection");
  const [state, setState] = useState<AppState>(initialState);
  const [draft, setDraft] = useState<GatewayConfig>(defaultConfig);
  const [upstreamKey, setUpstreamKey] = useState("");
  const [busy, setBusy] = useState<string | null>("loading");
  const [notice, setNotice] = useState<{ kind: "success" | "error"; text: string } | null>(null);
  const [mobileNav, setMobileNav] = useState(false);
  const [advancedOpen, setAdvancedOpen] = useState(false);

  useEffect(() => {
    void run("loading", async () => {
      const next = await appState();
      setState(next);
      setDraft(next.config);
      return next;
    });
  }, []);

  const busyLabel = busy ? busy : undefined;
  const runtime = state.runtime;
  const hasDraftChanges = JSON.stringify(draft) !== JSON.stringify(state.config) || upstreamKey.length > 0;
  const canConnect =
    state.certificate.trusted &&
    draft.base_url.trim().length > 0 &&
    draft.upstream_model.trim().length > 0 &&
    (state.has_upstream_key || upstreamKey.trim().length > 0) &&
    !busy;
  const statusTone = runtime.phase === "ready" ? "positive" : runtime.phase === "failed" ? "danger" : "neutral";

  async function run(label: string, action: () => Promise<AppState>): Promise<AppState | null> {
    setBusy(label);
    setNotice(null);
    try {
      const next = await action();
      setState(next);
      setDraft(next.config);
      if (next.runtime.phase === "failed") {
        setNotice({ kind: "error", text: next.runtime.error ?? "The gateway could not complete the operation." });
      } else if (label !== "loading") {
        setNotice({ kind: "success", text: successText(label) });
      }
      return next;
    } catch (error) {
      setNotice({ kind: "error", text: error instanceof Error ? error.message : String(error) });
      return null;
    } finally {
      setBusy(null);
    }
  }

  async function save(): Promise<boolean> {
    const next = await run("saving", () => saveSettings({ config: draft, upstream_key: upstreamKey }));
    if (next) setUpstreamKey("");
    return next !== null;
  }

  async function connect() {
    if (hasDraftChanges && !(await save())) return;
    await run("connecting", connectGateway);
  }

  const nav = (
    <aside className={mobileNav ? "sidebar sidebar-open" : "sidebar"}>
      <div className="sidebar-top" data-tauri-drag-region>
        <div className="brand-lockup">
          <img src={logo} alt="" />
          <div>
            <strong>Office Gateway</strong>
            <span>Private model bridge</span>
          </div>
        </div>
        <button className="icon-button mobile-close" aria-label="Close navigation" onClick={() => setMobileNav(false)}>
          <X size={17} />
        </button>
      </div>
      <nav className="nav-list" aria-label="Main navigation">
        <NavItem active={page === "connection"} icon={<Network size={18} />} label="Connection" onClick={() => { setPage("connection"); setMobileNav(false); }} />
        <NavItem active={page === "guide"} icon={<BookOpen size={18} />} label="Setup guide" onClick={() => { setPage("guide"); setMobileNav(false); }} />
        <NavItem active={page === "settings"} icon={<Settings2 size={18} />} label="Advanced settings" onClick={() => { setPage("settings"); setMobileNav(false); }} />
      </nav>
      <div className="sidebar-bottom">
        <div className="platform-pill"><Laptop size={15} /><span>{state.platform === "macos" ? "macOS" : "Windows"} client</span></div>
        <p>Office traffic stays on this device until it reaches the provider you choose.</p>
      </div>
    </aside>
  );

  return (
    <main className="app-shell">
      {nav}
      {mobileNav && <button className="scrim" aria-label="Close navigation" onClick={() => setMobileNav(false)} />}
      <section className="main-panel">
        <header className="window-header" data-tauri-drag-region>
          <button className="icon-button mobile-menu" aria-label="Open navigation" onClick={() => setMobileNav(true)}><Menu size={18} /></button>
          <div className="breadcrumbs"><span>Claude Office Gateway</span><ChevronRight size={15} /><strong>{pageTitle(page)}</strong></div>
          <div className="header-actions">
            <span className={`mini-status ${statusTone}`}><span />{phaseLabel(runtime.phase)}</span>
            <button className="icon-button" title="Refresh state" onClick={() => void run("refreshing", appState)} disabled={!!busy}><RefreshCw size={16} className={busy === "refreshing" ? "spin" : ""} /></button>
          </div>
        </header>
        <div className="page-scroll">
          {page === "connection" && (
            <ConnectionPage
              state={state}
              draft={draft}
              upstreamKey={upstreamKey}
              setDraft={setDraft}
              setUpstreamKey={setUpstreamKey}
              busy={busyLabel}
              notice={notice ?? (state.runtime.error ? { kind: "error", text: state.runtime.error } : null)}
              canConnect={canConnect}
              advancedOpen={advancedOpen}
              setAdvancedOpen={setAdvancedOpen}
              onInstall={() => void run("installing", installCertificate)}
              onDelete={() => void run("deleting", deleteCertificate)}
              onSave={save}
              onConnect={connect}
              onStop={() => void run("stopping", stopGateway)}
              onRecheck={() => void run("rechecking", recheckGateway)}
              onNavigateGuide={() => setPage("guide")}
            />
          )}
          {page === "guide" && <GuidePage onNavigate={() => setPage("connection")} />}
          {page === "settings" && <SettingsPage state={state} draft={draft} setDraft={setDraft} onSave={save} busy={!!busy} />}
        </div>
      </section>
    </main>
  );
}

function ConnectionPage(props: {
  state: AppState;
  draft: GatewayConfig;
  upstreamKey: string;
  setDraft: (config: GatewayConfig) => void;
  setUpstreamKey: (value: string) => void;
  busy: string | undefined;
  notice: { kind: "success" | "error"; text: string } | null;
  canConnect: boolean;
  advancedOpen: boolean;
  setAdvancedOpen: (value: boolean) => void;
  onInstall: () => void;
  onDelete: () => void;
  onSave: () => Promise<unknown>;
  onConnect: () => Promise<unknown>;
  onStop: () => void;
  onRecheck: () => void;
  onNavigateGuide: () => void;
}) {
  const { state, draft, upstreamKey, setDraft, setUpstreamKey, busy, notice } = props;
  const ready = state.runtime.phase === "ready";
  const saving = busy === "saving";
  const actionLabel = ready ? "Connected" : busy === "connecting" ? "Connecting…" : "Connect gateway";
  const connectionRows = useMemo(() => [
    { label: "Gateway URL", value: `https://localhost:${draft.port}`, icon: <Link2 size={15} /> },
    { label: "Gateway token", value: state.gateway_token ? "Stored securely" : "Generated on save", icon: <KeyRound size={15} /> },
    { label: "Office model ID", value: draft.public_model || "Not configured", icon: <Gauge size={15} /> },
  ], [draft.port, draft.public_model, state.gateway_token]);

  return <div className="content-wrap">
    <div className="page-intro">
      <div>
        <p className="eyebrow">Three steps to Office</p>
        <h1>Bring your model into PowerPoint, Excel, and Word.</h1>
        <p className="lede">A local HTTPS gateway translates the Claude Office add-in protocol into the provider API you already use.</p>
      </div>
      <div className={`status-orb ${ready ? "ready" : ""}`}><ShieldCheck size={30} /><span>{ready ? "Live" : "Local"}</span></div>
    </div>

    {notice && <div className={`notice ${notice.kind}`}><span>{notice.kind === "error" ? <CircleAlert size={17} /> : <Check size={17} />}</span><p>{notice.text}</p><button className="icon-button" onClick={() => undefined} aria-label="Dismiss"><X size={15} /></button></div>}

    <div className="status-banner">
      <div className="status-banner-icon"><LockKeyhole size={21} /></div>
      <div><strong>{state.certificate.trusted ? "Localhost is trusted" : "Secure localhost connection required"}</strong><p>{state.certificate.trusted ? "The gateway can accept Office requests over HTTPS." : "Install the per-user certificate before connecting a provider."}</p></div>
      <div className="status-banner-action">{state.certificate.trusted ? <span className="check-label"><Check size={15} /> Trusted</span> : <button className="button primary small" onClick={props.onInstall} disabled={!!busy}>{busy === "installing" ? "Installing…" : "Install certificate"}</button>}</div>
    </div>

    <div className="step-grid">
      <section className="card step-card provider-card">
        <StepHeading number="01" title="Connect a provider" done={state.has_upstream_key && !!draft.upstream_model && !!draft.base_url} />
        <p className="card-copy">Enter the API endpoint and model that should answer requests from Office.</p>
        <div className="form-grid">
          <label className="field wide"><span>API base URL</span><input value={draft.base_url} onChange={e => setDraft({ ...draft, base_url: e.target.value })} placeholder="https://api.example.com" /></label>
          <label className="field"><span>Provider protocol</span><select value={draft.upstream_protocol} onChange={e => setDraft({ ...draft, upstream_protocol: e.target.value as Protocol })}><option value="anthropic">Anthropic Messages</option><option value="openai_chat">OpenAI Chat Completions</option><option value="openai_responses">OpenAI Responses</option></select></label>
          <label className="field"><span>Provider model</span><input value={draft.upstream_model} onChange={e => setDraft({ ...draft, upstream_model: e.target.value })} placeholder="Exact model ID" /></label>
          <label className="field wide"><span>Provider API key</span><input type="password" value={upstreamKey} onChange={e => setUpstreamKey(e.target.value)} placeholder={state.has_upstream_key ? "Saved securely · enter a new key to replace" : "Paste the provider key"} /></label>
        </div>
        <div className="card-actions"><span className="muted-inline"><LockKeyhole size={14} /> Stored in the system credential vault</span><button className="button secondary" onClick={() => void props.onSave()} disabled={!!busy || !draft.base_url || !draft.upstream_model}>{saving ? "Saving…" : "Save provider"}</button></div>
      </section>

      <section className="card step-card office-card">
        <StepHeading number="02" title="Connect the Office add-in" done={ready} />
        <p className="card-copy">Copy these values into the Gateway sign-in panel in your Claude Office add-in.</p>
        <div className="connection-list">{connectionRows.map(row => <CopyRow key={row.label} {...row} disabled={!ready} />)}</div>
        <div className="hint-box"><CircleHelp size={16} /><span>Use <strong>Anthropic</strong> format and <strong>x-api-key</strong> authentication. The gateway token is separate from your provider key.</span></div>
        <div className="card-actions"><button className="text-button" onClick={props.onNavigateGuide}>Read the setup guide <ChevronRight size={15} /></button><div className="connection-actions">{ready ? <><button className="button secondary" onClick={props.onRecheck} disabled={!!busy}><RefreshCw size={15} /> Recheck</button><button className="button danger" onClick={props.onStop} disabled={!!busy}><StopCircle size={15} /> Stop</button></> : <button className="button primary" onClick={() => void props.onConnect()} disabled={!props.canConnect}>{actionLabel}<Play size={15} fill="currentColor" /></button>}</div></div>
      </section>
    </div>

    <section className="card advanced-card">
      <button className="disclosure" onClick={() => props.setAdvancedOpen(!props.advancedOpen)}><div><Settings2 size={17} /><span>Advanced gateway settings</span></div><ChevronRight size={17} className={props.advancedOpen ? "rotate-90" : ""} /></button>
      {props.advancedOpen && <div className="advanced-fields"><label className="field"><span>Office model ID</span><input value={draft.public_model} onChange={e => setDraft({ ...draft, public_model: e.target.value })} /></label><label className="field"><span>Display name</span><input value={draft.display_name} onChange={e => setDraft({ ...draft, display_name: e.target.value })} /></label><label className="field"><span>Local port</span><input type="number" min={1024} max={65535} value={draft.port} onChange={e => setDraft({ ...draft, port: Number(e.target.value) })} /></label><label className="field"><span>Timeout (seconds)</span><input type="number" min={1} max={600} value={draft.timeout_seconds} onChange={e => setDraft({ ...draft, timeout_seconds: Number(e.target.value) })} /></label></div>}
    </section>

    <div className="footer-note"><FileKey2 size={16} /><span>Requests go from Office to this device and then to <strong>{providerHost(draft.base_url)}</strong>. Document contents are not stored by this client.</span></div>
  </div>;
}

function SettingsPage({ state, draft, setDraft, onSave, busy }: { state: AppState; draft: GatewayConfig; setDraft: (config: GatewayConfig) => void; onSave: () => Promise<unknown>; busy: boolean }) {
  return <div className="content-wrap narrow"><div className="page-intro compact"><div><p className="eyebrow">Fine tune the bridge</p><h1>Advanced settings</h1><p className="lede">These values affect the local listener and how requests are authenticated upstream.</p></div></div><section className="card settings-card"><div className="settings-row"><div><strong>Upstream authentication</strong><p>Choose which header the provider expects.</p></div><select value={draft.auth_header} onChange={e => setDraft({ ...draft, auth_header: e.target.value as GatewayConfig["auth_header"] })}><option value="authorization">Authorization: Bearer</option><option value="x-api-key">x-api-key</option></select></div><div className="settings-row"><div><strong>OpenAI token field</strong><p>Some OpenAI-compatible providers still require the legacy field.</p></div><select value={draft.max_tokens_field} onChange={e => setDraft({ ...draft, max_tokens_field: e.target.value as GatewayConfig["max_tokens_field"] })}><option value="max_completion_tokens">max_completion_tokens</option><option value="max_tokens">max_tokens</option></select></div><div className="settings-row"><div><strong>Allowed Office origin</strong><p>Only this origin is accepted by the local CORS policy.</p></div><input className="settings-input" value={draft.allowed_origins[0] ?? ""} onChange={e => setDraft({ ...draft, allowed_origins: [e.target.value] })} /></div><div className="settings-row"><div><strong>Certificate identity</strong><p>{state.certificate.subject}</p></div><span className={state.certificate.trusted ? "check-label" : "muted-inline"}>{state.certificate.trusted ? <><Check size={15} /> Trusted</> : "Not trusted"}</span></div></section><div className="settings-actions"><button className="button primary" onClick={() => void onSave()} disabled={busy}>{busy ? "Saving…" : "Save advanced settings"}</button></div></div>;
}

function GuidePage({ onNavigate }: { onNavigate: () => void }) {
  return <div className="content-wrap narrow"><div className="page-intro compact"><div><p className="eyebrow">Setup once, then keep it running</p><h1>Office connection guide</h1><p className="lede">The client keeps a small trusted gateway on localhost while your Office add-in is in use.</p></div><div className="guide-mark"><BookOpen size={28} /></div></div><div className="guide-list"><GuideStep number="01" title="Install the local certificate" text="Install the certificate for your current user and localhost. macOS may show a Keychain confirmation; Windows uses the current user's certificate store." /><GuideStep number="02" title="Connect your model provider" text="Enter an HTTPS API base URL, select the provider protocol, paste your key, and use the exact model ID from your provider. The client sends two small checks when you connect." /><GuideStep number="03" title="Paste the gateway details into Office" text="Open the Claude add-in in PowerPoint, Excel, or Word, choose Gateway, and paste the URL, gateway token, and Office model ID shown on the Connection page." /></div><section className="card addin-card"><div><p className="eyebrow">Official add-ins</p><h2>Get the Office panel</h2><p className="card-copy">Install the compatible Claude add-in from Microsoft Marketplace.</p></div><div className="addin-list">{addIns.map(item => <a key={item.name} href={`https://marketplace.microsoft.com/en-us/product/${item.id}`} target="_blank" rel="noreferrer"><span>{item.name}</span><ExternalLink size={15} /></a>)}</div></section><section className="privacy-panel"><LockKeyhole size={18} /><div><strong>Private by default</strong><p>Provider keys live in the system credential vault. This app has no telemetry and does not log document contents.</p></div></section><button className="button primary" onClick={onNavigate}>Back to connection <ChevronRight size={16} /></button></div>;
}

function NavItem({ active, icon, label, onClick }: { active: boolean; icon: React.ReactNode; label: string; onClick: () => void }) { return <button className={`nav-item ${active ? "active" : ""}`} onClick={onClick}>{icon}<span>{label}</span>{active && <ChevronRight size={15} />}</button>; }
function StepHeading({ number, title, done }: { number: string; title: string; done: boolean }) { return <div className="step-heading"><span className={done ? "step-number done" : "step-number"}>{done ? <Check size={15} /> : number}</span><h2>{title}</h2>{done && <span className="done-label">Complete</span>}</div>; }
function GuideStep({ number, title, text }: { number: string; title: string; text: string }) { return <div className="guide-step"><span className="guide-number">{number}</span><div><h2>{title}</h2><p>{text}</p></div></div>; }
function CopyRow({ label, value, icon, disabled }: { label: string; value: string; icon: React.ReactNode; disabled: boolean }) { const [copied, setCopied] = useState(false); async function copy() { if (disabled) return; await navigator.clipboard?.writeText(value); setCopied(true); window.setTimeout(() => setCopied(false), 1800); } return <div className="copy-row"><div className="copy-label">{icon}<span>{label}</span></div><span className="copy-value">{value}</span><button className="icon-button copy-button" onClick={() => void copy()} disabled={disabled} title={disabled ? "Connect first" : `Copy ${label}`}>{copied ? <Check size={15} /> : <Clipboard size={15} />}</button></div>; }
function pageTitle(page: Page) { return page === "connection" ? "Connection" : page === "guide" ? "Setup guide" : "Advanced settings"; }
function successText(label: string) { return { installing: "The local certificate is ready. You can connect a provider now.", deleting: "The local certificate was removed.", saving: "Provider settings saved securely.", connecting: "The gateway is ready for Office.", stopping: "The local gateway stopped.", rechecking: "Provider check completed.", refreshing: "State refreshed." }[label as keyof Record<string, string>] ?? "Done."; }
function providerHost(value: string) { try { return value ? new URL(value).host : "your provider"; } catch { return "your provider"; } }

export default App;

