import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { processList, profileFocusWindow, useProfile } from "../entities/profile";
import { inTeam } from "../entities/space";
import { toast } from "../shared/model/toast";
import { t } from "../shared/i18n";

type ShortcutRequest = { id: string; silent_launcher: boolean };

/** A link asks the GUI to use the same password and launch path as Start. */
export function useProfileShortcuts() {
  const ready = useProfile((s) => s.status === "ready");
  useEffect(() => {
    if (!ready) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    let work = Promise.resolve();
    const consume = () => {
      work = work.then(async () => {
        if (disposed) return;
        const ids = await invoke<Array<string | ShortcutRequest>>("profile_shortcut_take_pending");
        for (const request of ids) {
          if (disposed) return;
          const id = typeof request === "string" ? request : request.id;
          const silentLauncher = typeof request !== "string" && request.silent_launcher;
          if (inTeam()) { await invoke("profile_shortcut_show_launcher"); toast.err(t("commands.shortcutPersonalOnly")); continue; }
          await useProfile.getState().reload();
          const state = useProfile.getState();
          const profile = state.profiles.find((p) => p.id === id);
          if (!profile) { await invoke("profile_shortcut_show_launcher"); toast.err(t("commands.shortcutMissing")); continue; }
          const running = await processList();
          if (running.some((p) => p.profile_id === id)) {
            await profileFocusWindow(id);
          } else if (!state.startBusy.has(id)) {
            // startStop keeps password prompts, permission checks and current
            // on-disk route resolution, without enabling a debugging port.
            // A protected/direct route can ask for confirmation. Reveal the
            // launcher only if that guarded path actually presents a dialog.
            const prompts = new MutationObserver(() => {
              if (document.querySelector('[role="dialog"], [role="alertdialog"]')) {
                prompts.disconnect();
                void invoke("profile_shortcut_show_launcher");
              }
            });
            if (silentLauncher) prompts.observe(document.body, { childList: true, subtree: true });
            try {
              const failure = await state.startStop(profile, { startOnly: true });
              if (failure && silentLauncher) await invoke("profile_shortcut_show_launcher");
            } finally { prompts.disconnect(); }
          }
        }
      }).catch((detail) => {
        void invoke("profile_shortcut_show_launcher");
        toast.fail({ title: t("commands.shortcutStartFailed"), detail });
      });
    };
    // Subscribe first, then drain: cold startup and a second instance cannot
    // lose a request between the native setup and the first webview render.
    void listen("profile-shortcut-requested", consume).then((fn) => {
      if (disposed) { fn(); return; }
      unlisten = fn;
      consume();
    });
    return () => { disposed = true; unlisten?.(); };
  }, [ready]);
}
