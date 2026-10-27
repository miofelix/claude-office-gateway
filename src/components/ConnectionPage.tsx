import {
  Check,
  ChevronRight,
  CircleAlert,
  CircleHelp,
  FileKey2,
  Gauge,
  KeyRound,
  Link2,
  LockKeyhole,
  Play,
  RefreshCw,
  Settings2,
  ShieldCheck,
  StopCircle,
  X,
} from "lucide-react";
import { useMemo } from "react";
import { providerHost } from "../lib/config";
import { t } from "../lib/i18n";
import type { AppState, GatewayConfig, Protocol } from "../lib/tauri";
import { CopyRow, StepHeading } from "./shared";
export function ConnectionPage(props: {
  state: AppState;
  draft: GatewayConfig;
  upstreamKey: string;
  setDraft: (config: GatewayConfig) => void;
  setUpstreamKey: (value: string) => void;
  busy: string | undefined;
  notice: { kind: "success" | "error"; text: string } | null;
  onDismiss: () => void;
  onCopyError: (text: string) => void;
  editable: boolean;
  validationError: string | null;
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
  const { state, draft, upstreamKey, setDraft, setUpstreamKey, busy, notice } =
    props;
  const ready = state.runtime.phase === "ready";
  const saving = busy === "saving";
  const actionLabel = ready
    ? t("Connected")
    : busy === "connecting"
      ? t("Connecting…")
      : t("Connect gateway");
  const connectionRows = useMemo(
    () => [
      {
        label: "Gateway URL",
        field: "url" as const,
        value: `https://localhost:${state.config.port}`,
        icon: <Link2 size={15} />,
      },
      {
        label: "Gateway token",
        field: "token" as const,
        value: state.gateway_token
          ? t("Stored securely")
          : t("Generated on save"),
        icon: <KeyRound size={15} />,
      },
      {
        label: "Office model ID",
        field: "model" as const,
        value: state.config.public_model || "Not configured",
        icon: <Gauge size={15} />,
      },
    ],
    [state.config.port, state.config.public_model, state.gateway_token],
  );

  return (
    <div className="content-wrap">
      <div className="page-intro">
        <div>
          <p className="eyebrow">{t("Three steps to Office")}</p>
          <h1>{t("Bring your model into PowerPoint, Excel, and Word.")}</h1>
          <p className="lede">
            {t(
              " A local HTTPS gateway translates the Claude Office add-in protocol into the provider API you already use. ",
            )}
          </p>
        </div>
        <div className={`status-orb ${ready ? "ready" : ""}`}>
          <ShieldCheck size={30} />
          <span>{ready ? t("Live") : t("Local")}</span>
        </div>
      </div>

      {notice && (
        <div className={`notice ${notice.kind}`}>
          <span>
            {notice.kind === "error" ? (
              <CircleAlert size={17} />
            ) : (
              <Check size={17} />
            )}
          </span>
          <p>{t(notice.text)}</p>
          <button
            className="icon-button"
            onClick={props.onDismiss}
            aria-label={t("Dismiss")}
          >
            <X size={15} />
          </button>
        </div>
      )}

      <div className="status-banner">
        <div className="status-banner-icon">
          <LockKeyhole size={21} />
        </div>
        <div>
          <strong>
            {state.certificate.trusted
              ? t("Localhost is trusted")
              : t("Secure localhost connection required")}
          </strong>
          <p>
            {state.certificate.trusted
              ? t("The gateway can accept Office requests over HTTPS.")
              : t(
                  "Install the per-user certificate before connecting a provider.",
                )}
          </p>
        </div>
        <div className="status-banner-action">
          {state.certificate.trusted ? (
            <span className="check-label">
              <Check size={15} />
              {t(" Trusted ")}
            </span>
          ) : (
            <button
              className="button primary small"
              onClick={props.onInstall}
              disabled={!!busy}
            >
              {busy === "installing"
                ? t("Installing…")
                : t("Install certificate")}
            </button>
          )}
        </div>
      </div>

      <div className="step-grid">
        <section className="card step-card provider-card">
          <StepHeading
            number="02"
            title={t("Connect a provider")}
            done={
              state.has_upstream_key &&
              !!draft.upstream_model &&
              !!draft.base_url
            }
          />
          <p className="card-copy">
            {t(
              " Enter the API endpoint and model that should answer requests from Office. ",
            )}
          </p>
          <fieldset className="form-grid" disabled={!props.editable}>
            <label className="field wide">
              <span>{t("API base URL")}</span>
              <input
                value={draft.base_url}
                onChange={(e) =>
                  setDraft({ ...draft, base_url: e.target.value })
                }
                placeholder={t("https://api.example.com")}
              />
            </label>
            <label className="field">
              <span>{t("Provider protocol")}</span>
              <select
                value={draft.upstream_protocol}
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    upstream_protocol: e.target.value as Protocol,
                    auth_header:
                      e.target.value === "anthropic"
                        ? "x-api-key"
                        : "authorization",
                  })
                }
              >
                <option value="anthropic">{t("Anthropic Messages")}</option>
                <option value="openai_chat">
                  {t("OpenAI Chat Completions")}
                </option>
                <option value="openai_responses">
                  {t("OpenAI Responses")}
                </option>
              </select>
            </label>
            <label className="field">
              <span>{t("Provider model")}</span>
              <input
                value={draft.upstream_model}
                onChange={(e) =>
                  setDraft({ ...draft, upstream_model: e.target.value })
                }
                placeholder={t("Exact model ID")}
              />
            </label>
            <label className="field wide">
              <span>{t("Provider API key")}</span>
              <input
                type="password"
                value={upstreamKey}
                onChange={(e) => setUpstreamKey(e.target.value)}
                placeholder={
                  state.has_upstream_key
                    ? t("Saved securely · enter a new key to replace")
                    : t("Paste the provider key")
                }
              />
            </label>
          </fieldset>
          <div className="card-actions">
            <span className="muted-inline">
              <LockKeyhole size={14} />
              {t(" Stored in the system credential vault ")}
            </span>
            <button
              className="button secondary"
              onClick={() => void props.onSave()}
              disabled={!props.editable || !!props.validationError}
            >
              {saving ? t("Saving…") : t("Save provider")}
            </button>
          </div>
        </section>

        <section className="card step-card office-card">
          <StepHeading
            number="03"
            title={t("Connect the Office add-in")}
            done={ready}
          />
          <p className="card-copy">
            {t(
              " Copy these values into the Gateway sign-in panel in your Claude Office add-in. ",
            )}
          </p>
          <div className="connection-list">
            {connectionRows.map((row) => (
              <CopyRow
                key={row.label}
                {...row}
                disabled={!ready}
                onError={props.onCopyError}
              />
            ))}
          </div>
          <div className="hint-box">
            <CircleHelp size={16} />
            <span>
              {t(" Use ")}
              <strong>{t("Anthropic")}</strong>
              {t(" format and")} <strong>{t("x-api-key")}</strong>
              {t(
                " authentication. The gateway token is separate from your provider key. ",
              )}
            </span>
          </div>
          <div className="card-actions">
            <button className="text-button" onClick={props.onNavigateGuide}>
              {t(" Read the setup guide ")}
              <ChevronRight size={15} />
            </button>
            <div className="connection-actions">
              {state.runtime.running ? (
                <>
                  <button
                    className="button secondary"
                    onClick={props.onRecheck}
                    disabled={!!busy}
                  >
                    <RefreshCw size={15} />
                    {t(" Recheck ")}
                  </button>
                  <button
                    className="button danger"
                    onClick={props.onStop}
                    disabled={busy === "stopping"}
                  >
                    <StopCircle size={15} />
                    {t(" Stop ")}
                  </button>
                </>
              ) : busy === "connecting" ? (
                <button className="button secondary" onClick={props.onStop}>
                  {t("Cancel connection")}
                </button>
              ) : (
                <button
                  className="button primary"
                  onClick={() => void props.onConnect()}
                  disabled={!props.canConnect}
                >
                  {actionLabel}
                  <Play size={15} fill="currentColor" />
                </button>
              )}
            </div>
          </div>
        </section>
      </div>

      <section className="card advanced-card">
        <button
          className="disclosure"
          onClick={() => props.setAdvancedOpen(!props.advancedOpen)}
        >
          <div>
            <Settings2 size={17} />
            <span>{t("Advanced gateway settings")}</span>
          </div>
          <ChevronRight
            size={17}
            className={props.advancedOpen ? "rotate-90" : ""}
          />
        </button>
        {props.advancedOpen && (
          <fieldset className="advanced-fields" disabled={!props.editable}>
            <label className="field">
              <span>{t("Office model ID")}</span>
              <input
                value={draft.public_model}
                onChange={(e) =>
                  setDraft({ ...draft, public_model: e.target.value })
                }
              />
            </label>
            <label className="field">
              <span>{t("Display name")}</span>
              <input
                value={draft.display_name}
                onChange={(e) =>
                  setDraft({ ...draft, display_name: e.target.value })
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
                onChange={(e) =>
                  setDraft({ ...draft, port: Number(e.target.value) })
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
                onChange={(e) =>
                  setDraft({
                    ...draft,
                    timeout_seconds: Number(e.target.value),
                  })
                }
              />
            </label>
          </fieldset>
        )}
      </section>

      <p className="validation-note">
        {(props.validationError ? t(props.validationError) : null) ??
          (!state.certificate.trusted
            ? t("Install the localhost certificate to enable connection.")
            : t(
                "Connect sends two short provider checks. Your provider may bill these requests.",
              ))}
      </p>
      <div className="footer-note">
        <FileKey2 size={16} />
        <span>
          {t(" Requests go from Office to this device and then to")}{" "}
          <strong>{t(providerHost(draft.base_url))}</strong>
          {t(". Document contents are not stored by this client. ")}
        </span>
      </div>
    </div>
  );
}
