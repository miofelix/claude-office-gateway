import { openUrl } from "@tauri-apps/plugin-opener";
import {
  BookOpen,
  ChevronRight,
  ExternalLink,
  LockKeyhole,
} from "lucide-react";
import { t } from "../lib/i18n";
import { GuideStep } from "./shared";
const addIns = [
  { name: "PowerPoint", id: "WA200010001" },
  { name: "Excel", id: "WA200009404" },
  { name: "Word", id: "WA200010453" },
];
export function GuidePage({ onNavigate }: { onNavigate: () => void }) {
  return (
    <div className="content-wrap narrow">
      <div className="page-intro compact">
        <div>
          <p className="eyebrow">{t("Setup once, then keep it running")}</p>
          <h1>{t("Office connection guide")}</h1>
          <p className="lede">
            {t(
              " The client keeps a small trusted gateway on localhost while your Office add-in is in use. ",
            )}
          </p>
        </div>
        <div className="guide-mark">
          <BookOpen size={28} />
        </div>
      </div>
      <div className="guide-list">
        <GuideStep
          number="01"
          title={t("Install the local certificate")}
          text={t(
            "Install the certificate for your current user and localhost. macOS may show a Keychain confirmation; Windows uses the current user's certificate store.",
          )}
        />
        <GuideStep
          number="02"
          title={t("Connect your model provider")}
          text={t(
            "Enter an HTTPS API base URL, select the provider protocol, paste your key, and use the exact model ID from your provider. The client sends two small checks when you connect.",
          )}
        />
        <GuideStep
          number="03"
          title={t("Paste the gateway details into Office")}
          text={t(
            "Open the Claude add-in in PowerPoint, Excel, or Word, choose Gateway, and paste the URL, gateway token, and Office model ID shown on the Connection page.",
          )}
        />
      </div>
      <section className="card addin-card">
        <div>
          <p className="eyebrow">{t("Official add-ins")}</p>
          <h2>{t("Get the Office panel")}</h2>
          <p className="card-copy">
            {t(
              " Install the compatible Claude add-in from Microsoft Marketplace. ",
            )}
          </p>
        </div>
        <div className="addin-list">
          {addIns.map((item) => (
            <a
              onClick={(event) => {
                event.preventDefault();
                void openUrl(
                  "https://marketplace.microsoft.com/en-us/product/" + item.id,
                );
              }}
              key={item.name}
              href={`https://marketplace.microsoft.com/en-us/product/${item.id}`}
              target="_blank"
              rel="noreferrer"
            >
              <span>{item.name}</span>
              <ExternalLink size={15} />
            </a>
          ))}
        </div>
      </section>
      <section className="privacy-panel">
        <LockKeyhole size={18} />
        <div>
          <strong>{t("Private by default")}</strong>
          <p>
            {t(
              " Provider keys live in the system credential vault. This app has no telemetry and does not log document contents. ",
            )}
          </p>
        </div>
      </section>
      <button className="button primary" onClick={onNavigate}>
        {t(" Back to connection ")}
        <ChevronRight size={16} />
      </button>
    </div>
  );
}
