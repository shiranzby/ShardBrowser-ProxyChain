import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { processList, profileFocusWindow, useProfile } from "../entities/profile";
import { inTeam } from "../entities/space";
import { toast } from "../shared/model/toast";
import { t } from "../shared/i18n";

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
        const ids = await invoke<string[]>("profile_shortcut_take_pending");
        for (const id of ids) {
          if (disposed) return;
          if (inTeam()) { toast.err(t("commands.shortcutPersonalOnly")); continue; }
          await useProfile.getState().reload();
          const state = useProfile.getState();
          const profile = state.profiles.find((p) => p.id === id);
          if (!profile) { toast.err(t("commands.shortcutMissing")); continue; }
          const running = await processList();
          if (running.some((p) => p.profile_id === id)) {
            await profileFocusWindow(id);
          } else if (!state.startBusy.has(id)) {
            // startStop keeps password prompts, permission checks and current
            // on-disk route resolution, without enabling a debugging port.
            await state.startStop(profile, { startOnly: true });
          }
        }
      }).catch((detail) => { toast.fail({ title: t("commands.shortcutStartFailed"), detail }); });
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
