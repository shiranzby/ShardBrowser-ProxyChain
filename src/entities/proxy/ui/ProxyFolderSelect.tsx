import { useEffect, useMemo, useState } from "react";
import { Button, Input } from "@proxyshard/shardx-ui-kit";
import { SelectField } from "../../../shared/ui/SelectField";
import { useT } from "../../../shared/i18n";
import { proxyFolderAdd, proxyFoldersList, type ProxyFolder } from "../model/api";

function treeOptions(folders: ProxyFolder[]) {
  const byParent = new Map<string, ProxyFolder[]>();
  for (const folder of folders) {
    const list = byParent.get(folder.parent) ?? [];
    list.push(folder);
    byParent.set(folder.parent, list);
  }
  const out: { value: string; label: string }[] = [];
  const visit = (parent: string, depth: number, seen: Set<string>) => {
    for (const folder of (byParent.get(parent) ?? []).sort((a, b) => a.name.localeCompare(b.name))) {
      if (seen.has(folder.name)) continue;
      const next = new Set(seen);
      next.add(folder.name);
      out.push({ value: folder.name, label: `${"　".repeat(depth)}${folder.name}` });
      visit(folder.name, depth + 1, next);
    }
  };
  visit("", 0, new Set());
  return out;
}

export function ProxyFolderSelect({
  value,
  onChange,
  label,
  disabled,
  allowCreate = true,
}: {
  value: string;
  onChange: (value: string) => void;
  label?: string;
  disabled?: boolean;
  allowCreate?: boolean;
}) {
  const t = useT();
  const [folders, setFolders] = useState<ProxyFolder[]>([]);
  const [adding, setAdding] = useState(false);
  const [name, setName] = useState("");
  const [parent, setParent] = useState("");
  const [busy, setBusy] = useState(false);

  const reload = async () => {
    try {
      const state = await proxyFoldersList();
      setFolders(state.folders);
      return state.folders;
    } catch {
      return [];
    }
  };
  useEffect(() => { void reload(); }, []);

  const options = useMemo(() => [
    { value: "", label: t("proxyFolder.noFolder") },
    ...treeOptions(folders),
  ], [folders, t]);

  const add = async () => {
    const next = name.trim();
    if (!next || busy) return;
    setBusy(true);
    try {
      const state = await proxyFolderAdd(next, parent);
      setFolders(state.folders);
      onChange(next);
      setName("");
      setParent("");
      setAdding(false);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex min-w-0 flex-col gap-1.5">
      <SelectField
        label={label ?? t("provider.folder")}
        value={value}
        options={options}
        isSearchable={folders.length > 8}
        onChange={onChange}
        disabled={disabled}
      />
      {allowCreate && !disabled && (
        adding ? (
          <div className="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] items-end gap-1.5">
            <Input inputSize="xsmall" label={t("proxyFolder.newName")} value={name} onChange={(e) => setName(e.target.value)} />
            <SelectField label={t("proxyFolder.parent")} value={parent} options={[
              { value: "", label: t("proxyFolder.root") },
              ...treeOptions(folders).filter((option) => option.value !== value),
            ]} onChange={setParent} />
            <div className="flex gap-1.5">
              <Button size="3xsmall" variant="neutral" mode="stroke" onClick={() => setAdding(false)}>{t("kit.cancel")}</Button>
              <Button size="3xsmall" variant="primary" mode="filled" isLoading={busy} disabled={!name.trim()} onClick={() => void add()}>{t("proxyFolder.create")}</Button>
            </div>
          </div>
        ) : (
          <Button size="3xsmall" variant="neutral" mode="stroke" onClick={() => setAdding(true)}>{t("proxyFolder.add")}</Button>
        )
      )}
    </div>
  );
}
