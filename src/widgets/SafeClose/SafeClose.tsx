import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { DialogModal } from "@proxyshard/shardx-ui-kit";
import { useT } from "../../shared/i18n";

type Update = { remaining: number } | {
  failure: "busy" | "cancelled" | "timed_out" | "save_failed" | "unsealed";
};

export function SafeClose() {
  const t = useT();
  const [status, setStatus] = useState<Update | null>(null);

  useEffect(() => {
    let mounted = true;
    const subscription = listen<Update>("app://safe-close", ({ payload }) => {
      if (mounted) setStatus("failure" in payload && payload.failure === "cancelled" ? null : payload);
    });
    void subscription.catch(() => {});
    return () => {
      mounted = false;
      void subscription.then((unlisten) => unlisten()).catch(() => {});
    };
  }, []);

  if (!status) return null;
  const waiting = "remaining" in status;
  const message = waiting
    ? (status.remaining > 0 ? t("safeClose.remaining", { count: status.remaining }) : t("safeClose.saving"))
    : status.failure === "busy" ? t("safeClose.busy")
    : status.failure === "timed_out" ? t("safeClose.timed_out")
    : status.failure === "save_failed" ? t("safeClose.save_failed")
    : t("safeClose.unsealed");
  const close = () => {
    if (!waiting) {
      setStatus(null);
      return;
    }
    void invoke("app_safe_close_cancel").catch(() => {});
  };
  return (
    <DialogModal
      open
      size="sm"
      title={waiting ? t("safeClose.title") : t("safeClose.failureTitle")}
      subtitle={<span role="status">{message}</span>}
      cancelLabel={waiting ? t("safeClose.cancel") : t("common.close")}
      footerNote={waiting ? t("safeClose.cancelNote") : undefined}
      onClose={close}
    />
  );
}
