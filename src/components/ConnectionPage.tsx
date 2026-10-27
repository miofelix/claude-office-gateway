import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Check,
  ChevronDown,
  CircleAlert,
  KeyRound,
  Link2,
  LoaderCircle,
  Play,
  Settings2,
  ShieldCheck,
  StopCircle,
  X,
} from "lucide-react";
import { useGateway } from "../hooks/useGateway";
import { phaseLabel } from "../lib/config";
import { t } from "../lib/i18n";
import type { GatewayConfig, Protocol } from "../lib/tauri";
import { CopyRow } from "./shared";

type Gateway = ReturnType<typeof useGateway>;
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
  return (
    <div className="content-wrap">
      <div className="certificate-row">
        <ShieldCheck
          size={17}
          className={state.certificate.trusted ? "success-text" : "muted"}
        />
        <span>
          {t(
            state.certificate.trusted
              ? "Localhost is trusted"
              : "Secure localhost connection required",
          )}
        </span>
        <span className="certificate-scope">
          {t("Current user · localhost only")}
        </span>
        {!state.certificate.trusted && (
          <button
            className="button secondary small"
            onClick={onInstall}
            disabled={!!busy}
          >
            {t(busy === "installing" ? "Installing…" : "Install certificate")}
          </button>
        )}
      </div>
      {notice && (
        <div className={"notice " + notice.kind} role="status">
          {notice.kind === "error" ? (
            <CircleAlert size={16} />
          ) : (
            <Check size={16} />
          )}
          <p>{t(notice.text)}</p>
          <button
            className="icon-button"
            onClick={() => g.setNotice(null)}
            aria-label={t("Dismiss")}
          >
            <X size={14} />
          </button>
        </div>
      )}
      <section
        className="section provider-section"
        aria-labelledby="provider-title"
      >
        <h1 id="provider-title">{t("Model provider")}</h1>
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
                  ? "Saved securely · enter a new key to replace"
                  : "Paste the provider key",
              )}
              autoComplete="off"
              spellCheck={false}
            />
          </label>
        </fieldset>
        <div className="connection-controls">
          <div className="runtime-status" aria-live="polite">
            {pending ? (
              <LoaderCircle size={15} className="spin" />
            ) : (
              <span
                className={
                  "status-dot " +
                  (ready
                    ? "ready"
                    : state.runtime.phase === "failed"
                      ? "failed"
                      : "")
                }
              />
            )}
            <span>
              {t(
                pending ? "Checking provider" : phaseLabel(state.runtime.phase),
              )}
            </span>
            {ready && state.runtime.verification_ms != null && (
              <small>{state.runtime.verification_ms} ms</small>
            )}
          </div>
          <div className="button-row">
            <button
              className="button secondary"
              onClick={() => void g.save()}
              disabled={!g.editable || !!g.error}
            >
              {t(busy === "saving" ? "Saving…" : "Save provider")}
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
                title={g.error ? t(g.error) : undefined}
              >
                <Play size={13} />
                {t("Connect gateway")}
              </button>
            )}
          </div>
        </div>
      </section>
      <section
        className="section office-section"
        aria-labelledby="office-title"
      >
        <h2 id="office-title">{t("Office connection")}</h2>
        <div className="connection-list">
          <CopyRow
            field="url"
            label="Gateway URL"
            value={"https://localhost:" + state.config.port}
            icon={<Link2 size={14} />}
            disabled={!ready}
            onError={(text) => g.setNotice({ kind: "error", text })}
          />
          <CopyRow
            field="token"
            label="Gateway token"
            value={state.gateway_token ? "••••••••••••" : "—"}
            icon={<KeyRound size={14} />}
            disabled={!ready}
            onError={(text) => g.setNotice({ kind: "error", text })}
          />
          <CopyRow
            field="model"
            label="Office model ID"
            value={state.config.public_model}
            icon={null}
            disabled={!ready}
            onError={(text) => g.setNotice({ kind: "error", text })}
          />
        </div>
        <p className="help-text">
          {t(
            "In Office, choose Gateway → Anthropic → x-api-key. Paste the gateway token without Bearer.",
          )}
        </p>
      </section>
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
              onChange={(event) => patch({ public_model: event.target.value })}
            />
          </label>
          <label className="field">
            <span>{t("Display name")}</span>
            <input
              value={draft.display_name}
              onChange={(event) => patch({ display_name: event.target.value })}
            />
          </label>
          <label className="field">
            <span>{t("Local port")}</span>
            <input
              type="number"
              min={1024}
              max={65535}
              value={draft.port}
              onChange={(event) => patch({ port: Number(event.target.value) })}
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
          <label className="field origin-field">
            <span>{t("Allowed Office origin")}</span>
            <input
              value={draft.allowed_origins[0] ?? ""}
              onChange={(event) =>
                patch({ allowed_origins: [event.target.value] })
              }
            />
          </label>
        </fieldset>
        <div className="advanced-footer">
          <span>{t("Changes use the Save provider button above.")}</span>
          {state.certificate.exists && (
            <button
              className="text-button danger-text"
              onClick={onDelete}
              disabled={!!busy}
            >
              {t("Remove certificate…")}
            </button>
          )}
        </div>
      </details>
      <footer className="page-footer">
        <p>
          {t(
            "Connect sends two short provider checks. Your provider may bill these requests.",
          )}
        </p>
        <div>
          <span>{t("Official add-ins")}</span>
          {[
            { name: "PowerPoint", id: "WA200010001" },
            { name: "Excel", id: "WA200009404" },
            { name: "Word", id: "WA200010453" },
          ].map((item) => (
            <a
              key={item.id}
              href={
                "https://marketplace.microsoft.com/en-us/product/" + item.id
              }
              onClick={(event) => {
                event.preventDefault();
                void openUrl(
                  "https://marketplace.microsoft.com/en-us/product/" + item.id,
                ).catch((error) =>
                  g.setNotice({ kind: "error", text: String(error) }),
                );
              }}
            >
              {item.name}
            </a>
          ))}
        </div>
      </footer>
    </div>
  );
}
