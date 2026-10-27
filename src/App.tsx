import {
  BookOpen,
  ChevronRight,
  Laptop,
  Menu,
  Network,
  RefreshCw,
  Settings2,
  X,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import logo from "./assets/logo.svg";
import { ConnectionPage } from "./components/ConnectionPage";
import { GuidePage } from "./components/GuidePage";
import { SettingsPage } from "./components/SettingsPage";
import { NavItem } from "./components/shared";
import { useGateway } from "./hooks/useGateway";
import { phaseLabel } from "./lib/config";
import { t, useLanguage } from "./lib/i18n";

type Page = "connection" | "guide" | "settings";
export default function App() {
  const [language, setLanguage] = useLanguage();
  const gateway = useGateway();
  const { state, busy, notice } = gateway;
  const [page, setPage] = useState<Page>("connection"),
    [mobileNav, setMobileNav] = useState(false),
    [advancedOpen, setAdvancedOpen] = useState(false),
    [confirm, setConfirm] = useState<"install" | "delete" | null>(null);
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    if (confirm) dialog.current?.showModal();
    else dialog.current?.close();
  }, [confirm]);
  useEffect(() => {
    function shortcut(event: KeyboardEvent) {
      if ((event.metaKey || event.ctrlKey) && event.key === ",") {
        event.preventDefault();
        setPage("settings");
      }
    }
    window.addEventListener("keydown", shortcut);
    return () => window.removeEventListener("keydown", shortcut);
  }, []);
  const statusTone =
    state.runtime.phase === "ready"
      ? "positive"
      : state.runtime.phase === "failed"
        ? "danger"
        : "neutral";
  const choose = (page: Page) => {
    setPage(page);
    setMobileNav(false);
  };
  return (
    <main className={"app-shell platform-" + state.platform}>
      <aside className={"sidebar" + (mobileNav ? " sidebar-open" : "")}>
        <div className="sidebar-top" data-tauri-drag-region>
          <div className="brand-lockup">
            <img src={logo} alt="" />
            <div>
              <strong>{t("Office Gateway")}</strong>
              <span>{t("Private model bridge")}</span>
            </div>
          </div>
          <button
            className="icon-button mobile-close"
            aria-label={t("Close navigation")}
            onClick={() => setMobileNav(false)}
          >
            <X size={17} />
          </button>
        </div>
        <nav className="nav-list" aria-label={t("Main navigation")}>
          <NavItem
            active={page === "connection"}
            icon={<Network size={18} />}
            label={t("Connection")}
            onClick={() => choose("connection")}
          />
          <NavItem
            active={page === "guide"}
            icon={<BookOpen size={18} />}
            label={t("Setup guide")}
            onClick={() => choose("guide")}
          />
          <NavItem
            active={page === "settings"}
            icon={<Settings2 size={18} />}
            label={t("Advanced settings")}
            onClick={() => choose("settings")}
          />
        </nav>
        <div className="sidebar-bottom">
          <label className="language-picker">
            <span>Language / 语言</span>
            <select
              aria-label="Language / 语言"
              value={language}
              onChange={(event) =>
                setLanguage(event.target.value as "en" | "zh")
              }
            >
              <option value="zh">简体中文</option>
              <option value="en">English</option>
            </select>
          </label>
          <div className="platform-pill">
            <Laptop size={15} />
            <span>
              {state.platform === "macos" ? t("macOS") : t("Windows")}
              {t(" client")}
            </span>
          </div>
          <p>
            {t(
              "Closing this window keeps the gateway running. Quit from the tray or menu bar to stop it.",
            )}
          </p>
        </div>
      </aside>
      {mobileNav && (
        <button
          className="scrim"
          aria-label={t("Close navigation")}
          onClick={() => setMobileNav(false)}
        />
      )}
      <section className="main-panel">
        <header className="window-header" data-tauri-drag-region>
          <button
            className="icon-button mobile-menu"
            aria-label={t("Open navigation")}
            onClick={() => setMobileNav(true)}
          >
            <Menu size={18} />
          </button>
          <div className="breadcrumbs" data-tauri-drag-region>
            <span>{t("Claude Office Gateway")}</span>
            <ChevronRight size={15} />
            <strong>
              {page === "connection"
                ? t("Connection")
                : page === "guide"
                  ? t("Setup guide")
                  : t("Advanced settings")}
            </strong>
          </div>
          <div className="header-actions">
            <span className={"mini-status " + statusTone}>
              <span />
              {t(phaseLabel(state.runtime.phase))}
            </span>
            <button
              className="icon-button"
              aria-label={t("Refresh state")}
              title={t("Refresh state")}
              onClick={() => void gateway.refresh()}
              disabled={!!busy}
            >
              <RefreshCw
                size={16}
                className={busy === "refreshing" ? "spin" : ""}
              />
            </button>
          </div>
        </header>
        <div className="page-scroll">
          {page === "connection" && (
            <ConnectionPage
              state={state}
              draft={gateway.draft}
              upstreamKey={gateway.upstreamKey}
              setDraft={gateway.setDraft}
              setUpstreamKey={gateway.setUpstreamKey}
              busy={busy ?? undefined}
              notice={notice}
              onDismiss={() => gateway.setNotice(null)}
              onCopyError={(text) => gateway.setNotice({ kind: "error", text })}
              editable={gateway.editable}
              validationError={gateway.error}
              canConnect={gateway.canConnect}
              advancedOpen={advancedOpen}
              setAdvancedOpen={setAdvancedOpen}
              onInstall={() => setConfirm("install")}
              onDelete={() => setConfirm("delete")}
              onSave={gateway.save}
              onConnect={gateway.connect}
              onStop={() => void gateway.stop()}
              onRecheck={() => void gateway.recheck()}
              onNavigateGuide={() => choose("guide")}
            />
          )}
          {page !== "connection" && notice && (
            <div
              className={"global-notice notice " + notice.kind}
              role="status"
            >
              <p>{t(notice.text)}</p>
              <button
                className="icon-button"
                aria-label={t("Dismiss")}
                onClick={() => gateway.setNotice(null)}
              >
                <X size={15} />
              </button>
            </div>
          )}
          {page === "guide" && (
            <GuidePage onNavigate={() => choose("connection")} />
          )}
          {page === "settings" && (
            <SettingsPage
              state={state}
              draft={gateway.draft}
              setDraft={gateway.setDraft}
              busy={!!busy}
              onSave={gateway.save}
              onDelete={() => setConfirm("delete")}
              onInstall={() => setConfirm("install")}
            />
          )}
        </div>
      </section>
      <dialog
        ref={dialog}
        onCancel={() => setConfirm(null)}
        aria-labelledby="certificate-dialog-title"
      >
        <h2 id="certificate-dialog-title">
          {confirm === "delete"
            ? t("Remove the local certificate?")
            : t("Trust localhost for Office?")}
        </h2>
        <p>
          {confirm === "delete"
            ? t(
                "This stops the gateway, removes only this app’s certificate from your user certificate store, and deletes its local identity files. Your provider settings and keys are kept.",
              )
            : t(
                "This adds a one-year HTTPS leaf certificate for localhost to your current user certificate store. It cannot issue other certificates. Complete any password confirmation in the system dialog.",
              )}
        </p>
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
            {confirm === "delete"
              ? t("Remove certificate")
              : t("Install certificate")}
          </button>
        </div>
      </dialog>
    </main>
  );
}
