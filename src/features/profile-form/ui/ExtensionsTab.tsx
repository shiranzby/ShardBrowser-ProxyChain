import { useState } from "react";
import { Button, Input } from "@proxyshard/shardx-ui-kit";
import { extensionLinkAllowed, isStoreLink, normalizeExtensionLink, useExtensions } from "../../../entities/extension";
import { useT } from "../../../shared/i18n";
import { AddExtensionMenu } from "../../manage-extensions";
import { ExtensionPicker } from "../../manage-profiles";
import { patchForm, type ProfileFormDraft } from "../model/useProfileForm";

/// The library and its import actions stay inside the Profile's Extensions tab.
export function ExtensionsTab({ draft }: { draft: ProfileFormDraft }) {
  const t = useT();
  const [addingLink, setAddingLink] = useState(false);
  const [url, setUrl] = useState("");
  const busy = useExtensions((s) => s.busy);
  const libraryCount = useExtensions((s) => s.items.length);
  const importUrl = useExtensions((s) => s.importUrl);
  const link = normalizeExtensionLink(url);
  const allowed = link !== "" && extensionLinkAllowed(link);
  const addLink = async () => {
    // The existing store owns import errors, duplicate handling and library reload.
    const added = await importUrl(link);
    if (added) {
      setAddingLink(false);
      setUrl("");
    }
  };
  return (
    <div data-tour="form-extensions" className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-3">
        <p className="m-0 text-secondary text-text-soft-400">{libraryCount === 0 ? t("extensionPicker.emptyLibrary") : ""}</p>
        <AddExtensionMenu onAddFromLink={() => setAddingLink(true)} />
      </div>
      {addingLink && (
        <div data-extension-import className="tile flex flex-col gap-3 p-3">
          <Input
            label={t("extensions.linkFieldLabel")}
            inputSize="xsmall"
            className="mono"
            value={url}
            disabled={busy}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="https://chromewebstore.google.com/detail/…"
            error={link !== "" && !allowed ? t("extensions.linkHttp") : undefined}
            hint={allowed && !isStoreLink(link) ? t("extensions.linkOffStore") : undefined}
          />
          <p className="m-0 text-secondary text-text-soft-400">{t("extensions.linkHelp")}</p>
          <div className="flex justify-end gap-2">
            <Button variant="neutral" mode="stroke" size="xsmall" disabled={busy} onClick={() => { setAddingLink(false); setUrl(""); }}>
              {t("extensions.cancel")}
            </Button>
            <Button size="xsmall" disabled={busy || !allowed} onClick={() => void addLink()}>
              {busy ? t("extensions.downloading") : t("extensions.download")}
            </Button>
          </div>
        </div>
      )}
      <ExtensionPicker listOnly value={draft.form.extensions} onChange={(extensions) => patchForm({ extensions })} onAdd={() => setAddingLink(true)} />
    </div>
  );
}
