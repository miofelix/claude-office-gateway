import { useEffect, useRef, useState } from "react";
import {
  Github,
  RefreshCw,
  Sun,
  Moon,
  Monitor,
  LoaderCircle,
} from "lucide-react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { t, useLanguage } from "./lib/i18n";
import { useAppearance, type Appearance } from "./hooks/useAppearance";
import { useAppUpdates } from "./hooks/useAppUpdates";
import { UpdateDialog } from "./components/UpdateDialog";
import { useGateway } from "./hooks/useGateway";
import { useLightweightMode } from "./hooks/useLightweightMode";
import { LightweightDialog } from "./components/LightweightDialog";
import { ConnectionPage } from "./components/ConnectionPage";
import { setTrayLanguage } from "./lib/tauri";
import logo from "./assets/logo.svg";

export default function App() {
  const [language, setLanguage] = useLanguage();
  const { appearance, setAppearance } = useAppearance();
  const gateway = useGateway();
  const updates = useAppUpdates();
  const [updatesOpen, setUpdatesOpen] = useState(false);
  useEffect(() => {
    void setTrayLanguage(language).catch(() => {});
  }, [language]);
  const [confirm, setConfirm] = useState<"install" | "delete" | null>(null);
  const lightweight = useLightweightMode({
    dirty: gateway.dirty,
    busy: !!gateway.busy || updates.busy || confirm !== null || updatesOpen,
    onError: (text) => gateway.setNotice({ kind: "error", text }),
  });
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    if (confirm) dialog.current?.showModal();
    else dialog.current?.close();
  }, [confirm]);
  return (
    <div className={"app-shell platform-" + gateway.state.platform}>
      <header className="window-header" data-tauri-drag-region>
        <div className="brand" data-tauri-drag-region>
          <img src={logo} alt="" />
          <span className="brand-name">Claude Office Gateway</span>
          <button
            className="version-button"
            onClick={() => setUpdatesOpen(true)}
            title={t("Version and updates")}
            aria-label={t("Version and updates")}
          >
            <span>v{updates.state.current_version}</span>
            {updates.hasUpdate && <span className="update-dot" />}
            {updates.busy && <LoaderCircle size={11} className="spin" />}
          </button>
          {gateway.state.test_mode && (
            <span className="test-mode-label">{t("Test session")}</span>
          )}
        </div>
        <div className="header-actions">
          <label className="appearance-control" title={t("Appearance")}>
            {appearance === "system" ? (
              <Monitor size={14} />
            ) : appearance === "dark" ? (
              <Moon size={14} />
            ) : (
              <Sun size={14} />
            )}
            <select
              aria-label={t("Appearance")}
              value={appearance}
              onChange={(event) =>
                setAppearance(event.target.value as Appearance)
              }
            >
              <option value="light">{t("Light")}</option>
              <option value="dark">{t("Dark")}</option>
              <option value="system">{t("Follow system")}</option>
            </select>
          </label>
          <select
            className="language-select"
            aria-label="Language / 语言"
            value={language}
            onChange={(event) => setLanguage(event.target.value as "en" | "zh")}
          >
            <option value="zh">简体中文</option>
            <option value="en">English</option>
          </select>
          <button
            className="icon-button"
            aria-label={t("Refresh state")}
            title={t("Refresh state")}
            onClick={() => void gateway.refresh()}
            disabled={!!gateway.busy}
          >
            <RefreshCw
              size={16}
              className={gateway.busy === "refreshing" ? "spin" : ""}
            />
          </button>
          <a
            className="icon-button"
            href="https://github.com/miofelix/claude-office-gateway"
            target="_blank"
            rel="noreferrer"
            title={t("Open GitHub repository")}
            aria-label={t("Open GitHub repository")}
            onClick={(event) => {
              event.preventDefault();
              void openUrl(
                "https://github.com/miofelix/claude-office-gateway",
              ).catch((error) =>
                gateway.setNotice({ kind: "error", text: String(error) }),
              );
            }}
          >
            <Github size={18} />
          </a>
        </div>
      </header>
      <main className="page-scroll">
        <ConnectionPage
          gateway={gateway}
          onInstall={() => setConfirm("install")}
          onDelete={() => setConfirm("delete")}
          onLightweight={lightweight.request}
        />
      </main>
      <UpdateDialog
        open={updatesOpen}
        onClose={() => setUpdatesOpen(false)}
        updates={updates}
      />
      <LightweightDialog
        open={lightweight.confirmDiscard}
        onCancel={lightweight.cancel}
        onEnter={() => void lightweight.enter()}
      />
      <dialog
        ref={dialog}
        onCancel={() => setConfirm(null)}
        aria-labelledby="certificate-dialog-title"
      >
        <h2 id="certificate-dialog-title">
          {t(
            confirm === "delete"
              ? "Uninstall the local certificate?"
              : "Trust localhost for Office?",
          )}
        </h2>
        <p>
          {t(
            confirm === "delete"
              ? "This stops the gateway, removes only this app’s certificate from your user certificate store, and deletes its local identity files. Your provider settings and keys are kept."
              : "This installs this app’s one-year localhost certificate for your current user. Certificate checks match hostnames, not ports. Your system may ask you to confirm the trust change.",
          )}
        </p>
        {confirm === "install" && (
          <p className="certificate-platform-note">
            {t(
              gateway.state.platform === "macos"
                ? "macOS: user-level SSL trust for localhost in your login Keychain."
                : "Windows: this certificate is added to Current User / Trusted Root Certification Authorities.",
            )}
          </p>
        )}
        <div className="dialog-actions">
          <button
            autoFocus
            className="button secondary"
            onClick={() => setConfirm(null)}
          >
            {t("Cancel")}
          </button>
          <button
            className={
              "button " + (confirm === "delete" ? "danger" : "primary")
            }
            onClick={() => {
              const action = confirm;
              setConfirm(null);
              if (action === "delete") void gateway.remove();
              else void gateway.install();
            }}
          >
            {t(
              confirm === "delete"
                ? "Uninstall certificate"
                : "Install certificate",
            )}
          </button>
        </div>
      </dialog>
    </div>
  );
}
