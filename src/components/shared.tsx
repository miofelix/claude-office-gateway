import { useState } from "react";
import { Check, Clipboard } from "lucide-react";
import { copyConnectionDetail } from "../lib/tauri";
import { t } from "../lib/i18n";
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
      <span className="copy-label">
        {icon}
        {t(label)}
      </span>
      <div>
        <code className="copy-value">{value}</code>
        <button
          className="icon-button"
          onClick={() => void copy()}
          disabled={disabled}
          aria-label={"Copy " + label}
          title={
            copied
              ? t("Copied")
              : disabled
                ? t("Connect first")
                : "Copy " + label
          }
        >
          {copied ? <Check size={14} /> : <Clipboard size={14} />}
        </button>
        <span className="sr-only" role="status">
          {copied ? t("Copied") : ""}
        </span>
      </div>
    </div>
  );
}
