import { openUrl } from "@tauri-apps/plugin-opener";
import {
  ArrowDown,
  ArrowUpRight,
  Check,
  ChevronDown,
  CircleAlert,
  Info,
  KeyRound,
  Link2,
  LoaderCircle,
  Play,
  RefreshCw,
  Settings2,
  ShieldCheck,
  StopCircle,
  Trash2,
  X,
} from "lucide-react";
import type { ReactNode } from "react";
import { useGateway } from "../hooks/useGateway";
import { phaseLabel } from "../lib/config";
import { t } from "../lib/i18n";
import type { GatewayConfig, Protocol } from "../lib/tauri";
import { CopyRow } from "./shared";

type Gateway = ReturnType<typeof useGateway>;
const addIns = [
  { name: "PowerPoint", id: "WA200010001", letter: "P", color: "powerpoint" },
  { name: "Excel", id: "WA200009404", letter: "E", color: "excel" },
  { name: "Word", id: "WA200010453", letter: "W", color: "word" },
];
function SectionHeading({
  step,
  title,
  description,
  done,
  extra,
}: {
  step: string;
  title: string;
  description: string;
  done?: boolean;
  extra?: ReactNode;
}) {
  return (
    <div className="section-heading">
      <span className={"step-number" + (done ? " complete" : "")}>
        {done ? <Check size={15} /> : step}
      </span>
      <div>
        <h2
          id={
            step === "1"
              ? "certificate-title"
              : step === "2"
                ? "provider-title"
                : "office-title"
          }
        >
          {t(title)}
        </h2>
        <p>{t(description)}</p>
      </div>
      {extra && <div className="section-heading-extra">{extra}</div>}
    </div>
  );
}
export function ConnectionPage({
  gateway: g,
  onInstall,
  onDelete,
}: {
  gateway: Gateway;
  onInstall: () => void;
  onDelete: () => void;
}) {
  const { state, draft, setDraft, busy, notice } = g;
  const ready = state.runtime.phase === "ready";
  const pending =
    busy === "connecting" ||
    busy === "rechecking" ||
    ["starting", "checking"].includes(state.runtime.phase);
  const patch = (value: Partial<GatewayConfig>) =>
    setDraft({ ...draft, ...value });
  const endpoint =
    "https://localhost:" +
    (state.runtime.running ? state.config.port : draft.port);
  const showError = (error: unknown) =>
    g.setNotice({ kind: "error", text: String(error) });
  const certificateLabel = state.certificate.trusted
    ? "Trusted"
    : state.certificate.exists
      ? "Not yet trusted"
      : "Not installed";
  return (
    <div className="content-wrap">
      <div className="page-intro">
        <div>
          <h1>{t("Connect your model to Office")}</h1>
          <p>
            {t(
              "Trust this device, connect a provider, then add the gateway in Office.",
            )}
          </p>
        </div>
        <span className={"connection-badge " + (ready ? "connected" : "")}>
          <span />
          {t(phaseLabel(state.runtime.phase))}
        </span>
      </div>
      {notice && (
        <div className={"notice " + notice.kind} role="status">
          {notice.kind === "error" ? (
            <CircleAlert size={17} />
          ) : (
            <Check size={17} />
          )}
          <p>{t(notice.text)}</p>
          <button
            className="icon-button"
            onClick={() => g.setNotice(null)}
            aria-label={t("Dismiss")}
          >
            <X size={15} />
          </button>
        </div>
      )}
      <div className="workspace-grid">
        <div className="configuration-column">
          <section
            className="section certificate-section"
            aria-labelledby="certificate-title"
          >
            <SectionHeading
              step="1"
              title="Local HTTPS certificate"
              description="Lets the Office add-in reach your local gateway securely."
              done={state.certificate.trusted}
              extra={
                <span
                  className={
                    "status-label " +
                    (state.certificate.trusted ? "trusted" : "")
                  }
                >
                  {t(certificateLabel)}
                </span>
              }
            />
            <div className="certificate-endpoint">
              <ShieldCheck size={17} />
              <div>
                <span>{t("Local gateway")}</span>
                <code>{endpoint}</code>
              </div>
              <span className="scope-chip">{t("Current user")}</span>
            </div>
            <div className="certificate-bottom">
              <details className="trust-details">
                <summary>
                  <Info size={14} />
                  {t("What is trusted?")}
                  <ChevronDown size={12} />
                </summary>
                <div className="trust-explanation">
                  <strong>
                    {t("Certificates identify hosts, not ports.")}
                  </strong>
                  <p>
                    {t(
                      "The certificate matches localhost and loopback addresses. It cannot be restricted to one port. Trusting this certificate does not automatically trust unrelated localhost certificates.",
                    )}
                  </p>
                  <p>
                    {t(
                      "The gateway separately checks the local port, allowed Office origin, and gateway token.",
                    )}
                  </p>
                </div>
              </details>
              <div className="certificate-actions">
                {!state.certificate.trusted && (
                  <button
                    className="button secondary"
                    onClick={onInstall}
                    disabled={!!busy || state.runtime.running}
                  >
                    <ShieldCheck size={14} />
                    {t(
                      busy === "installing"
                        ? "Installing…"
                        : "Install certificate",
                    )}
                  </button>
                )}
                <button
                  className="button quiet"
                  onClick={() => void g.refresh()}
                  disabled={!!busy}
                  title={t("Check certificate")}
                >
                  <RefreshCw size={14} />
                  {t("Check")}
                </button>
                <button
                  className="button quiet danger-text"
                  onClick={onDelete}
                  disabled={!!busy || !state.certificate.exists}
                >
                  <Trash2 size={14} />
                  {t("Uninstall certificate")}
                </button>
              </div>
            </div>
          </section>
          <section
            className="section provider-section"
            aria-labelledby="provider-title"
          >
            <SectionHeading
              step="2"
              title="Model provider"
              description="Use the API URL, key, and model from your provider."
              done={ready}
            />
            <fieldset className="form-grid" disabled={!g.editable}>
              <label className="field">
                <span>{t("API base URL")}</span>
                <input
                  type="url"
                  value={draft.base_url}
                  onChange={(event) => patch({ base_url: event.target.value })}
                  placeholder="https://api.example.com"
                  spellCheck={false}
                  autoCapitalize="off"
                />
              </label>
              <label className="field">
                <span>{t("Provider protocol")}</span>
                <select
                  value={draft.upstream_protocol}
                  onChange={(event) =>
                    patch({
                      upstream_protocol: event.target.value as Protocol,
                      auth_header:
                        event.target.value === "anthropic"
                          ? "x-api-key"
                          : "authorization",
                    })
                  }
                >
                  <option value="anthropic">Anthropic Messages</option>
                  <option value="openai_chat">OpenAI Chat Completions</option>
                  <option value="openai_responses">OpenAI Responses</option>
                </select>
              </label>
              <label className="field">
                <span>{t("Provider model")}</span>
                <input
                  value={draft.upstream_model}
                  onChange={(event) =>
                    patch({ upstream_model: event.target.value })
                  }
                  placeholder={t("Exact model ID")}
                  spellCheck={false}
                />
              </label>
              <label className="field">
                <span>{t("Provider API key")}</span>
                <input
                  type="password"
                  value={g.upstreamKey}
                  onChange={(event) => g.setUpstreamKey(event.target.value)}
                  placeholder={t(
                    state.has_upstream_key
                      ? "Saved key · leave blank to keep"
                      : "Paste the provider key",
                  )}
                  autoComplete="off"
                  spellCheck={false}
                />
              </label>
            </fieldset>
            <div className="provider-actions">
              <div className="provider-hint">
                {pending ? (
                  <>
                    <LoaderCircle size={15} className="spin" />
                    <span>{t("Checking provider")}</span>
                  </>
                ) : ready ? (
                  <>
                    <Check size={15} />
                    <span>
                      {t("Provider verified")}
                      {state.runtime.verification_ms != null
                        ? " · " + state.runtime.verification_ms + " ms"
                        : ""}
                    </span>
                  </>
                ) : (
                  <span>
                    {t("Connect saves your settings and checks the provider.")}
                  </span>
                )}
              </div>
              <div className="button-row">
                <button
                  className="button secondary"
                  onClick={() => void g.save()}
                  disabled={!g.editable || !!g.error}
                >
                  {t(busy === "saving" ? "Saving…" : "Save")}
                </button>
                {state.runtime.running || busy === "connecting" ? (
                  <>
                    <button
                      className="button secondary"
                      onClick={() => void g.recheck()}
                      disabled={!!busy || pending}
                    >
                      {t("Recheck")}
                    </button>
                    <button
                      className="button secondary"
                      onClick={() => void g.stop()}
                      disabled={busy === "stopping"}
                    >
                      <StopCircle size={14} />
                      {t(pending ? "Cancel" : "Stop")}
                    </button>
                  </>
                ) : (
                  <button
                    className="button primary"
                    onClick={() => void g.connect()}
                    disabled={!g.canConnect}
                    title={
                      g.error
                        ? t(g.error)
                        : !state.certificate.trusted
                          ? t(
                              "Install the localhost certificate to enable connection.",
                            )
                          : undefined
                    }
                  >
                    <Play size={13} />
                    {t("Save and connect")}
                  </button>
                )}
              </div>
            </div>
            <details className="advanced-settings">
              <summary>
                <Settings2 size={15} />
                <span>{t("Advanced settings")}</span>
                <ChevronDown size={14} />
              </summary>
              <fieldset className="advanced-grid" disabled={!g.editable}>
                <label className="field">
                  <span>{t("Office model ID")}</span>
                  <input
                    value={draft.public_model}
                    aria-describedby="office-model-alias-help"
                    placeholder="claude-office-custom"
                    spellCheck={false}
                    onChange={(event) =>
                      patch({ public_model: event.target.value })
                    }
                  />
                  <small id="office-model-alias-help" className="field-hint">
                    {t(
                      'Office requires a model ID starting with "claude", such as "claude-office-custom". This gateway alias maps to your provider model.',
                    )}
                  </small>
                </label>
                <label className="field">
                  <span>{t("Display name")}</span>
                  <input
                    value={draft.display_name}
                    onChange={(event) =>
                      patch({ display_name: event.target.value })
                    }
                  />
                </label>
                <label className="field">
                  <span>{t("Local port")}</span>
                  <input
                    type="number"
                    min={1024}
                    max={65535}
                    value={draft.port}
                    onChange={(event) =>
                      patch({ port: Number(event.target.value) })
                    }
                  />
                </label>
                <label className="field">
                  <span>{t("Timeout (seconds)")}</span>
                  <input
                    type="number"
                    min={1}
                    max={600}
                    value={draft.timeout_seconds}
                    onChange={(event) =>
                      patch({ timeout_seconds: Number(event.target.value) })
                    }
                  />
                </label>
                <label className="field">
                  <span>{t("Upstream authentication")}</span>
                  <select
                    value={draft.auth_header}
                    onChange={(event) =>
                      patch({
                        auth_header: event.target
                          .value as GatewayConfig["auth_header"],
                      })
                    }
                  >
                    <option value="authorization">Authorization: Bearer</option>
                    <option value="x-api-key">x-api-key</option>
                  </select>
                </label>
                <label className="field">
                  <span>{t("OpenAI token field")}</span>
                  <select
                    value={draft.max_tokens_field}
                    onChange={(event) =>
                      patch({
                        max_tokens_field: event.target
                          .value as GatewayConfig["max_tokens_field"],
                      })
                    }
                  >
                    <option value="max_completion_tokens">
                      max_completion_tokens
                    </option>
                    <option value="max_tokens">max_tokens</option>
                  </select>
                </label>
                <label className="field full-width">
                  <span>{t("Allowed Office origin")}</span>
                  <input
                    value={draft.allowed_origins[0] ?? ""}
                    onChange={(event) =>
                      patch({ allowed_origins: [event.target.value] })
                    }
                  />
                </label>
              </fieldset>
              <p className="advanced-save-hint">
                {t(
                  "Changes use the Save button above. Stop the gateway before editing.",
                )}
              </p>
            </details>
          </section>
          <p className="local-privacy-note">
            <KeyRound size={14} />
            {t(
              state.test_mode
                ? "Test session: keys are kept in memory and are not saved to your credential vault."
                : "Keys stay in your system credential vault. Document contents are not saved.",
            )}
          </p>
        </div>
        <section
          className="section office-section"
          aria-labelledby="office-title"
        >
          <SectionHeading
            step="3"
            title="Set up the Office add-in"
            description="Choose an Office app, then enter the connection details."
            done={ready}
          />
          <div className="office-instructions">
            <h3>{t("Install the official Claude add-in")}</h3>
            <div className="addin-list">
              {addIns.map((item) => (
                <a
                  key={item.id}
                  className={"addin-link " + item.color}
                  href={
                    "https://marketplace.microsoft.com/en-us/product/" + item.id
                  }
                  onClick={(event) => {
                    event.preventDefault();
                    void openUrl(
                      "https://marketplace.microsoft.com/en-us/product/" +
                        item.id,
                    ).catch(showError);
                  }}
                >
                  <span className="office-app-mark" aria-hidden="true">
                    {item.letter}
                  </span>
                  <span className="addin-name">{item.name}</span>
                  <span className="addin-action">
                    {t("Install")}
                    <ArrowUpRight size={14} />
                  </span>
                </a>
              ))}
            </div>
            <div className="instruction-divider">
              <ArrowDown size={14} />
            </div>
            <h3>{t("In the Claude add-in, choose Gateway")}</h3>
            <dl className="office-api-settings">
              <div>
                <dt>{t("API format")}</dt>
                <dd>Anthropic</dd>
              </div>
              <div>
                <dt>{t("Authentication")}</dt>
                <dd>x-api-key</dd>
              </div>
            </dl>
            <div className="office-details-heading">
              <h3>{t("Paste these three values")}</h3>
              {ready && (
                <span className="status-label trusted">{t("Ready")}</span>
              )}
            </div>
            <div className="connection-list">
              <CopyRow
                field="url"
                label="Gateway URL"
                value={"https://localhost:" + state.config.port}
                icon={<Link2 size={14} />}
                disabled={!ready}
                onError={showError}
              />
              <CopyRow
                field="token"
                label="Gateway token"
                value={state.gateway_token ? "••••••••••••" : "—"}
                icon={<KeyRound size={14} />}
                disabled={!ready}
                onError={showError}
              />
              <CopyRow
                field="model"
                label="Office model ID"
                value={state.config.public_model}
                icon={null}
                disabled={!ready}
                onError={showError}
              />
            </div>
            <p className="model-alias-hint">
              <Info size={14} />
              <span>
                {t(
                  'Office requires the model ID to start with "claude", such as "claude-office-custom". Use this gateway alias in Office; keep the actual model ID in Provider model.',
                )}
              </span>
            </p>
            <p className="token-hint">
              <Info size={14} />
              <span>
                {t(
                  ready
                    ? "The gateway token is different from your provider key. Paste it without Bearer."
                    : "Connect the provider first to enable copying. The gateway token is separate from your provider key.",
                )}
              </span>
            </p>
          </div>
        </section>
      </div>
      <footer className="page-footer">
        <span>{t("Keep this app running while using Office.")}</span>
        <span>{t("Provider checks may incur a small charge.")}</span>
      </footer>
    </div>
  );
}
