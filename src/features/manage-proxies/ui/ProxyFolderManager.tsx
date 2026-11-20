import { useEffect, useMemo, useState } from "react";
import { Button, DialogModal, Input } from "@proxyshard/shardx-ui-kit";
import { SelectField } from "../../../shared/ui/SelectField";
import { useT } from "../../../shared/i18n";
import { toast } from "../../../shared/model/toast";
import {
  proxyFolderAdd,
  proxyFolderDelete,
  proxyFolderRename,
  proxyFolderSetParent,
  proxyFoldersList,
  type ProxyFolder,
} from "../../../entities/proxy/model/api";

function optionsFor(folders: ProxyFolder[], current = "") {
  const byParent = new Map<string, ProxyFolder[]>();
  for (const folder of folders) {
    const list = byParent.get(folder.parent) ?? [];
    list.push(folder);
    byParent.set(folder.parent, list);
  }
  const options: { value: string; label: string }[] = [];
  const blocked = new Set<string>();
  if (current) {
    const markDescendants = (parent: string) => {
      for (const folder of folders) {
        if (folder.parent === parent && !blocked.has(folder.name)) {
          blocked.add(folder.name);
          markDescendants(folder.name);
        }
      }
    };
    blocked.add(current);
    markDescendants(current);
  }
  const visit = (parent: string, depth: number, seen: Set<string>) => {
    for (const folder of (byParent.get(parent) ?? []).sort((a, b) => a.name.localeCompare(b.name))) {
      if (seen.has(folder.name) || blocked.has(folder.name)) continue;
      const next = new Set(seen);
      next.add(folder.name);
      options.push({ value: folder.name, label: `${"　".repeat(depth)}${folder.name}` });
      visit(folder.name, depth + 1, next);
    }
  };
  visit("", 0, new Set());
  return options;
}

function errorDetail(error: unknown, fallback: string) {
  const code = String(error);
  if (code.includes("already exists")) return code;
  if (code.includes("cannot contain itself")) return code;
  if (code.includes("parent proxy folder does not exist")) return code;
  return fallback;
}

export function ProxyFolderManager({ open, onClose }: { open: boolean; onClose: () => void }) {
  const t = useT();
  const [folders, setFolders] = useState<ProxyFolder[]>([]);
  const [newName, setNewName] = useState("");
  const [newParent, setNewParent] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const [editName, setEditName] = useState("");
  const [editParent, setEditParent] = useState("");
  const [busy, setBusy] = useState(false);

  const reload = async () => {
    try {
      setFolders((await proxyFoldersList()).folders);
    } catch (error) {
      toast.fail({ title: t("proxyFolder.loadFailed"), detail: errorDetail(error, t("proxyFolder.loadFailed")) });
    }
  };
  useEffect(() => {
    if (open) void reload();
  }, [open]);

  const parentOptions = useMemo(() => [
    { value: "", label: t("proxyFolder.root") },
    ...optionsFor(folders),
  ], [folders, t]);

  const add = async () => {
    const name = newName.trim();
    if (!name || busy) return;
    setBusy(true);
    try {
      setFolders((await proxyFolderAdd(name, newParent)).folders);
      setNewName("");
      setNewParent("");
      toast.saved(t("proxyFolder.created", { name }));
    } catch (error) {
      toast.fail({ title: t("proxyFolder.saveFailed"), detail: errorDetail(error, t("proxyFolder.saveFailed")) });
    } finally {
      setBusy(false);
    }
  };

  const saveEdit = async () => {
    if (!editing || busy) return;
    const name = editName.trim();
    if (!name) return;
    setBusy(true);
    try {
      let next = (await proxyFolderRename(editing, name)).folders;
      next = (await proxyFolderSetParent(name, editParent)).folders;
      setFolders(next);
      setEditing(null);
      toast.saved(t("proxyFolder.updated", { name }));
    } catch (error) {
      toast.fail({ title: t("proxyFolder.saveFailed"), detail: errorDetail(error, t("proxyFolder.saveFailed")) });
    } finally {
      setBusy(false);
    }
  };

  const remove = async (name: string) => {
    if (busy || !window.confirm(t("proxyFolder.deleteConfirm", { name }))) return;
    setBusy(true);
    try {
      setFolders((await proxyFolderDelete(name)).folders);
      if (editing === name) setEditing(null);
      toast.saved(t("proxyFolder.deleted", { name }));
    } catch (error) {
      toast.fail({ title: t("proxyFolder.saveFailed"), detail: errorDetail(error, t("proxyFolder.saveFailed")) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <DialogModal
      open={open}
      onClose={() => { if (!busy) onClose(); }}
      showClose={!busy}
      hideCancel={busy}
      title={t("proxyFolder.manageTitle")}
      cancelLabel={t("common.close")}
      onCancel={() => { if (!busy) onClose(); }}
      onConfirm={onClose}
      confirmLabel={t("common.close")}
      isDisabled={busy}
    >
      <div className="flex min-w-0 flex-col gap-4 py-3">
        <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] items-end gap-2">
          <Input label={t("proxyFolder.name")} inputSize="xsmall" value={newName} onChange={(e) => setNewName(e.target.value)} />
          <SelectField label={t("proxyFolder.parent")} value={newParent} options={parentOptions} onChange={setNewParent} />
          <Button size="3xsmall" variant="primary" mode="filled" isLoading={busy} disabled={!newName.trim()} onClick={() => void add()}>
            {t("proxyFolder.create")}
          </Button>
        </div>
        <div className="flex min-h-0 flex-col gap-1.5">
          {folders.length === 0 && <p className="text-help text-text-sub-600">{t("proxyFolder.empty")}</p>}
          {folders.map((folder) => {
            const isEditing = editing === folder.name;
            return (
              <div key={folder.name} className="flex min-w-0 items-center gap-2 rounded-control border border-border-weak px-2 py-1.5">
                {isEditing ? (
                  <>
                    <Input aria-label={t("proxyFolder.name")} inputSize="xsmall" value={editName} onChange={(e) => setEditName(e.target.value)} />
                    <SelectField aria-label={t("proxyFolder.parent")} value={editParent} options={parentOptions} onChange={setEditParent} />
                    <Button size="3xsmall" variant="primary" mode="filled" isLoading={busy} onClick={() => void saveEdit()}>{t("commands.saveSettings")}</Button>
                    <Button size="3xsmall" variant="neutral" mode="stroke" disabled={busy} onClick={() => setEditing(null)}>{t("confirm.cancel")}</Button>
                  </>
                ) : (
                  <>
                    <span className="min-w-0 flex-1 truncate">{folder.parent ? `${folder.parent} / ` : ""}{folder.name}</span>
                    <Button size="3xsmall" variant="neutral" mode="stroke" disabled={busy} onClick={() => { setEditing(folder.name); setEditName(folder.name); setEditParent(folder.parent); }}>{t("commands.edit")}</Button>
                    <Button size="3xsmall" variant="neutral" mode="stroke" disabled={busy} onClick={() => void remove(folder.name)}>{t("commands.delete")}</Button>
                  </>
                )}
              </div>
            );
          })}
        </div>
        <p className="text-help text-text-sub-600">{t("proxyFolder.deleteHelp")}</p>
      </div>
    </DialogModal>
  );
}
