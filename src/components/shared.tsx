import { Check, ChevronRight, Clipboard } from "lucide-react";
import { useState } from "react";
import { t } from "../lib/i18n";
import { copyConnectionDetail } from "../lib/tauri";
export function NavItem({
  active,
  icon,
  label,
  onClick,
}: {
  active: boolean;
  icon: React.ReactNode;
  label: string;
  onClick: () => void;
}) {
  return (
    <button className={`nav-item ${active ? "active" : ""}`} onClick={onClick}>
      {icon}
      <span>{t(label)}</span>
      {active && <ChevronRight size={15} />}
    </button>
  );
}
export function StepHeading({
  number,
  title,
  done,
}: {
  number: string;
  title: string;
  done: boolean;
}) {
  return (
    <div className="step-heading">
      <span className={done ? "step-number done" : "step-number"}>
        {done ? <Check size={15} /> : number}
      </span>
      <h2>{t(title)}</h2>
      {done && <span className="done-label">{t("Complete")}</span>}
    </div>
  );
}
export function GuideStep({
  number,
  title,
  text,
}: {
  number: string;
  title: string;
  text: string;
}) {
  return (
    <div className="guide-step">
      <span className="guide-number">{number}</span>
      <div>
        <h2>{t(title)}</h2>
        <p>{t(text)}</p>
      </div>
    </div>
  );
}
export function CopyRow({
  label,
  value,
  icon,
  field,
  disabled,
  onError,
}: {
  label: string;
  value: string;
  icon: React.ReactNode;
  field: "url" | "token" | "model";
  disabled: boolean;
  onError: (message: string) => void;
}) {
  const [copied, setCopied] = useState(false);
  async function copy() {
    try {
      await copyConnectionDetail(field);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1800);
    } catch (error) {
      onError(String(error));
    }
  }
  return (
    <div className="copy-row">
      <div className="copy-label">
        {icon}
        <span>{t(label)}</span>
      </div>
      <span className="copy-value">{t(value)}</span>
      <button
        className="icon-button copy-button"
        onClick={() => void copy()}
        disabled={disabled}
        aria-label={"Copy " + label}
        title={disabled ? t("Connect first") : "Copy " + label}
      >
        {copied ? <Check size={15} /> : <Clipboard size={15} />}
      </button>
    </div>
  );
}
