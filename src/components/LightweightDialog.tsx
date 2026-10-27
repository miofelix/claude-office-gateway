import { useEffect, useRef } from "react";
import { t } from "../lib/i18n";

export function LightweightDialog({
  open,
  onCancel,
  onEnter,
}: {
  open: boolean;
  onCancel: () => void;
  onEnter: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    if (open) dialog.current?.showModal();
    else dialog.current?.close();
  }, [open]);
  return (
    <dialog
      ref={dialog}
      onCancel={onCancel}
      aria-labelledby="lightweight-dialog-title"
    >
      <h2 id="lightweight-dialog-title">{t("Enter lightweight mode?")}</h2>
      <p>
        {t(
          "Your unsaved provider changes will be discarded. Save them first if you want to keep them.",
        )}
      </p>
      <p>
        {t(
          "The main window and web view will be released. The gateway and tray keep running. Open the app from the tray to return.",
        )}
      </p>
      <div className="dialog-actions">
        <button autoFocus className="button secondary" onClick={onCancel}>
          {t("Keep editing")}
        </button>
        <button className="button primary" onClick={onEnter}>
          {t("Discard and enter")}
        </button>
      </div>
    </dialog>
  );
}
