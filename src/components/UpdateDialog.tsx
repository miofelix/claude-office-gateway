import { useEffect, useRef } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  Check,
  Download,
  ExternalLink,
  Info,
  LoaderCircle,
  RefreshCw,
  X,
} from "lucide-react";
import type { useAppUpdates } from "../hooks/useAppUpdates";
import { RELEASES_URL, releaseNotesUrl } from "../lib/updates";
import { t } from "../lib/i18n";
import logo from "../assets/logo.svg";
export function UpdateDialog({
  open,
  onClose,
  updates,
}: {
  open: boolean;
  onClose: () => void;
  updates: ReturnType<typeof useAppUpdates>;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const { state, busy, hasUpdate } = updates;
  useEffect(() => {
    if (open) dialog.current?.showModal();
    else dialog.current?.close();
  }, [open]);
  const installing = ["downloading", "installing", "restarting"].includes(
    state.phase,
  );
  const percent =
    state.total && state.total > 0
      ? Math.min(100, (state.downloaded / state.total) * 100)
      : undefined;
  const status =
    state.phase === "checking"
      ? "Checking for updates…"
      : state.phase === "up_to_date"
        ? "You are up to date."
        : state.phase === "available"
          ? "A new version is available."
          : state.phase === "downloading"
            ? "Downloading and verifying update…"
            : state.phase === "installing"
              ? "Installing update…"
              : state.phase === "restarting"
                ? "Restarting…"
                : state.phase === "error"
                  ? (state.error ??
                    "Could not check for updates. Check your network and try again.")
                  : "Updates are verified with a signing key before installation.";
  const notesVersion = state.available?.version ?? state.current_version;
  return (
    <dialog
      ref={dialog}
      className="update-dialog"
      onCancel={onClose}
      aria-labelledby="update-dialog-title"
    >
      <div className="update-dialog-heading">
        <img src={logo} alt="" />
        <div>
          <h2 id="update-dialog-title">Claude Office Gateway</h2>
          <p className="update-version">
            v{state.current_version}
            <span className="scope-chip">
              {t(state.channel === "beta" ? "Beta channel" : "Stable channel")}
            </span>
          </p>
        </div>
        <button
          className="icon-button"
          aria-label={t("Close")}
          onClick={onClose}
        >
          <X size={16} />
        </button>
      </div>
      <div
        className={"update-status " + (state.phase === "error" ? "error" : "")}
        role="status"
      >
        {busy ? (
          <LoaderCircle size={17} className="spin" />
        ) : state.phase === "up_to_date" ? (
          <Check size={17} />
        ) : hasUpdate ? (
          <Download size={17} />
        ) : (
          <Info size={17} />
        )}
        <span>{t(status)}</span>
      </div>
      {state.available && (
        <div className="update-release">
          <strong>
            {t("Available version")}: v{state.available.version}
          </strong>
          {state.available.notes && (
            <div className="release-notes">{state.available.notes}</div>
          )}
        </div>
      )}
      {state.phase === "downloading" && (
        <div className="update-progress">
          <progress max={100} value={percent} />
          <span>
            {(state.downloaded / 1048576).toFixed(1)} MB
            {state.total
              ? " / " + (state.total / 1048576).toFixed(1) + " MB"
              : ""}
          </span>
        </div>
      )}
      {state.phase === "up_to_date" && state.checked_at && (
        <p className="update-checked-at">
          {t("Last checked")}: {new Date(state.checked_at).toLocaleString()}
        </p>
      )}
      <label className="auto-update-check">
        <input
          type="checkbox"
          checked={updates.autoCheck}
          onChange={(event) => updates.setAutoCheck(event.target.checked)}
          disabled={installing}
        />
        <span>{t("Check for updates on startup")}</span>
      </label>
      {(hasUpdate || installing) && (
        <p className="update-install-note">
          {t(
            "Installing an update restarts this app and pauses the local gateway.",
          )}
        </p>
      )}
      <div className="update-dialog-actions">
        <button
          className="text-button"
          onClick={() => void openUrl(releaseNotesUrl(notesVersion))}
        >
          {t("Release notes")}
          <ExternalLink size={13} />
        </button>
        <div className="button-row">
          {state.phase === "error" && (
            <button
              className="button secondary"
              onClick={() => void openUrl(RELEASES_URL)}
            >
              {t("Download from GitHub")}
            </button>
          )}
          <button
            className={"button " + (hasUpdate ? "primary" : "secondary")}
            disabled={busy}
            onClick={() =>
              void (hasUpdate ? updates.install() : updates.check())
            }
          >
            {busy ? (
              <LoaderCircle size={14} className="spin" />
            ) : hasUpdate ? (
              <Download size={14} />
            ) : (
              <RefreshCw size={14} />
            )}
            <span>
              {t(
                installing
                  ? "Updating…"
                  : state.phase === "checking"
                    ? "Checking…"
                    : hasUpdate
                      ? "Install and restart"
                      : "Check for updates",
              )}
            </span>
          </button>
        </div>
      </div>
    </dialog>
  );
}
