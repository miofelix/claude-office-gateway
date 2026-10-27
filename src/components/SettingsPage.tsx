import { Check } from "lucide-react";
import { t } from "../lib/i18n";
import type { AppState, GatewayConfig } from "../lib/tauri";
export function SettingsPage({
  state,
  draft,
  setDraft,
  onSave,
  busy,
  onDelete,
  onInstall,
}: {
  state: AppState;
  draft: GatewayConfig;
  setDraft: (config: GatewayConfig) => void;
  onSave: () => Promise<unknown>;
  busy: boolean;
  onDelete: () => void;
  onInstall: () => void;
}) {
  return (
    <div className="content-wrap narrow">
      <div className="page-intro compact">
        <div>
          <p className="eyebrow">{t("Fine tune the bridge")}</p>
          <h1>{t("Advanced settings")}</h1>
          <p className="lede">
            {t(
              " These values affect the local listener and how requests are authenticated upstream. ",
            )}
          </p>
        </div>
      </div>
      <section className="card settings-card">
        <div className="settings-row">
          <div>
            <strong>{t("Upstream authentication")}</strong>
            <p>{t("Choose which header the provider expects.")}</p>
          </div>
          <select
            disabled={busy || state.runtime.running}
            value={draft.auth_header}
            onChange={(e) =>
              setDraft({
                ...draft,
                auth_header: e.target.value as GatewayConfig["auth_header"],
              })
            }
          >
            <option value="authorization">{t("Authorization: Bearer")}</option>
            <option value="x-api-key">{t("x-api-key")}</option>
          </select>
        </div>
        <div className="settings-row">
          <div>
            <strong>{t("OpenAI token field")}</strong>
            <p>
              {t(
                " Some OpenAI-compatible providers still require the legacy field. ",
              )}
            </p>
          </div>
          <select
            disabled={busy || state.runtime.running}
            value={draft.max_tokens_field}
            onChange={(e) =>
              setDraft({
                ...draft,
                max_tokens_field: e.target
                  .value as GatewayConfig["max_tokens_field"],
              })
            }
          >
            <option value="max_completion_tokens">
              {t("max_completion_tokens")}
            </option>
            <option value="max_tokens">{t("max_tokens")}</option>
          </select>
        </div>
        <div className="settings-row">
          <div>
            <strong>{t("Allowed Office origin")}</strong>
            <p>{t("Only this origin is accepted by the local CORS policy.")}</p>
          </div>
          <input
            className="settings-input"
            disabled={busy || state.runtime.running}
            value={draft.allowed_origins[0] ?? ""}
            onChange={(e) =>
              setDraft({ ...draft, allowed_origins: [e.target.value] })
            }
          />
        </div>
        <div className="settings-row">
          <div>
            <strong>{t("Certificate identity")}</strong>
            <p>{state.certificate.subject}</p>
          </div>
          <span
            className={
              state.certificate.trusted ? "check-label" : "muted-inline"
            }
          >
            {state.certificate.trusted ? (
              <>
                <Check size={15} />
                {t(" Trusted ")}
              </>
            ) : (
              "Not trusted"
            )}
          </span>
        </div>
      </section>
      <div className="settings-actions">
        <button
          className="button secondary"
          onClick={onInstall}
          disabled={busy || state.runtime.running}
        >
          {t("Install certificate")}
        </button>
        {state.certificate.exists && (
          <button className="button danger" onClick={onDelete} disabled={busy}>
            {t("Remove certificate…")}
          </button>
        )}
        <button
          className="button primary"
          onClick={() => void onSave()}
          disabled={busy}
        >
          {busy ? t("Saving…") : t("Save advanced settings")}
        </button>
      </div>
    </div>
  );
}
