import { useMemo, useState } from "react";
import { Badge, DialogModal, Textarea } from "@proxyshard/shardx-ui-kit";
import { Field } from "../../../shared/ui/Field";
import { toast } from "../../../shared/model/toast";
import { fingerprintImport, useFingerprint } from "../../../entities/fingerprint";
import { useT } from "../../../shared/i18n";

type CandidateStatus = "ready" | "duplicate" | "incomplete" | "invalid";
type Candidate = { value: Record<string, unknown> | null; status: CandidateStatus; detail?: string };

function stableStringify(value: unknown): string {
  if (Array.isArray(value)) return `[${value.map(stableStringify).join(",")}]`;
  if (value && typeof value === "object") {
    return `{${Object.entries(value as Record<string, unknown>)
      .sort(([a], [b]) => a.localeCompare(b))
      .map(([key, item]) => `${JSON.stringify(key)}:${stableStringify(item)}`)
      .join(",")}}`;
  }
  return JSON.stringify(value);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === "object" && !Array.isArray(value);
}

function validatePayload(payload: Record<string, unknown>): { status: CandidateStatus; detail?: string } {
  const objectFields = ["navigator", "screen", "webgl", "client_hints"] as const;
  const invalidObjects = objectFields.filter((key) => payload[key] !== undefined && !isRecord(payload[key]));
  if (invalidObjects.length) {
    return { status: "invalid", detail: invalidObjects.join(", ") };
  }

  const missing: string[] = [];
  for (const key of objectFields) {
    if (!isRecord(payload[key])) {
      missing.push(key);
    }
  }

  const navigator = payload.navigator as Record<string, unknown> | undefined;
  for (const key of ["user_agent", "platform"] as const) {
    if (navigator?.[key] !== undefined && typeof navigator[key] !== "string") {
      return { status: "invalid", detail: `navigator.${key}` };
    }
    if (navigator?.[key] === undefined) missing.push(`navigator.${key}`);
  }

  const screen = payload.screen as Record<string, unknown> | undefined;
  for (const key of ["width", "height", "avail_width", "avail_height", "device_pixel_ratio"] as const) {
    if (screen?.[key] !== undefined && (typeof screen[key] !== "number" || !Number.isFinite(screen[key]))) {
      return { status: "invalid", detail: `screen.${key}` };
    }
    if (screen?.[key] === undefined) missing.push(`screen.${key}`);
  }

  const webgl = payload.webgl as Record<string, unknown> | undefined;
  for (const key of ["vendor", "renderer"] as const) {
    if (webgl?.[key] !== undefined && typeof webgl[key] !== "string") {
      return { status: "invalid", detail: `webgl.${key}` };
    }
    if (webgl?.[key] === undefined) missing.push(`webgl.${key}`);
  }

  const clientHints = payload.client_hints as Record<string, unknown> | undefined;
  if (clientHints?.platform_version !== undefined && typeof clientHints.platform_version !== "string") {
    return { status: "invalid", detail: "client_hints.platform_version" };
  }
  if (clientHints?.platform_version === undefined) missing.push("client_hints.platform_version");

  return missing.length ? { status: "incomplete", detail: missing.join(", ") } : { status: "ready" };
}

function parseCandidates(text: string, existing: unknown[]): Candidate[] {
  const source = text.trim();
  if (!source) return [];
  const values: unknown[] = [];
  try {
    const parsed = JSON.parse(source);
    values.push(...(Array.isArray(parsed) ? parsed : [parsed]));
  } catch {
    for (const line of source.split(/\r?\n/).map((item) => item.trim()).filter(Boolean)) {
      try { values.push(JSON.parse(line)); } catch { values.push(null); }
    }
  }
  const existingPayloads = new Set(existing.map((entry) => stableStringify((entry as { payload?: unknown }).payload)));
  return values.map((value) => {
    if (!value || typeof value !== "object" || Array.isArray(value)) {
      return { value: null, status: "invalid", detail: "invalidJson" };
    }
    const payload = value as Record<string, unknown>;
    const identity = stableStringify(payload);
    if (existingPayloads.has(identity)) {
      return { value: payload, status: "duplicate", detail: "duplicate" };
    }
    const validation = validatePayload(payload);
    if (validation.status !== "invalid") existingPayloads.add(identity);
    return { value: payload, ...validation };
  });
}

export function FingerprintImporter({ open, onClose }: { open: boolean; onClose: () => void }) {
  const t = useT();
  const [text, setText] = useState("");
  const [name, setName] = useState("");
  const existing = useFingerprint((s) => s.items);
  const candidates = useMemo(() => parseCandidates(text, existing), [text, existing]);
  const save = async () => {
    const importable = candidates.flatMap(({ value, status }) =>
      value && (status === "ready" || status === "incomplete") ? [value] : [],
    );
    if (!importable.length) {
      toast.fail({ title: t("fingerprintImporter.noImportable") });
      return;
    }
    let imported = 0;
    let failed = 0;
    for (const candidate of importable) {
      try {
        const payload = { ...candidate };
        if (importable.length === 1 && name.trim()) payload.name = name.trim();
        await fingerprintImport(JSON.stringify(payload), null);
        imported += 1;
      } catch {
        failed += 1;
      }
    }
    if (imported) {
      if (imported === 1 && failed === 0) {
        toast.ok(t("fingerprintImporter.imported", { name: name.trim() || String(importable[0].name ?? t("fingerprintImporter.title")) }));
      } else {
        toast.ok(t("fingerprintImporter.importedMany", { count: imported, failed }));
      }
      onClose();
    } else {
      toast.fail({ title: t("fingerprintImporter.failed") });
    }
  };
  return (
    <DialogModal
      open={open}
      onClose={onClose}
      title={t("fingerprintImporter.title")}
      dirty={text.trim() !== "" || name.trim() !== ""}
      confirmLabel={t("fingerprintImporter.confirm")}
      onConfirm={save}
      cancelLabel={t("fingerprintImporter.cancel")}
      onCancel={onClose}
    >
      <div className="flex flex-col gap-3 py-4">
        <Field label={t("fingerprintImporter.nameLabel")} value={name} onChange={setName} placeholder={t("fingerprintImporter.namePlaceholder")} />
        <Textarea
          label={t("fingerprintImporter.jsonLabel")}
          rows={14}
          className="mono w-full"
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder='{ "name": "...", "navigator": { ... }, "webgl": { ... }, ... }'
        />
        {candidates.length > 0 && (
          <div className="flex flex-col gap-2 rounded-control bg-bg-weak-50 p-3" data-testid="fingerprint-import-preview">
            <div className="flex items-center justify-between gap-3">
              <span className="text-label-medium text-text-strong-950">{t("fingerprintImporter.preview")}</span>
              <span className="text-label-small text-text-sub-600">{t("fingerprintImporter.previewCount", { count: candidates.length })}</span>
            </div>
            <div className="flex flex-wrap gap-2">
              <Badge color="success" size="small">{t("fingerprintImporter.ready", { count: candidates.filter((item) => item.status === "ready").length })}</Badge>
              <Badge color="gray" size="small">{t("fingerprintImporter.incomplete", { count: candidates.filter((item) => item.status === "incomplete").length })}</Badge>
              <Badge color="warning" size="small">{t("fingerprintImporter.duplicate", { count: candidates.filter((item) => item.status === "duplicate").length })}</Badge>
              <Badge color="error" size="small">{t("fingerprintImporter.invalid", { count: candidates.filter((item) => item.status === "invalid").length })}</Badge>
            </div>
            <div className="max-h-28 overflow-auto text-label-small text-text-sub-600">
              {candidates.map((candidate, index) => (
                <div key={index} className="flex gap-2 py-0.5">
                  <span>{index + 1}.</span>
                  <span>{candidate.status === "ready" ? t("fingerprintImporter.statusReady") : candidate.status === "incomplete" ? t("fingerprintImporter.statusIncomplete", { fields: candidate.detail ?? "" }) : candidate.status === "duplicate" ? t("fingerprintImporter.statusDuplicate") : t("fingerprintImporter.statusInvalid")}</span>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>
    </DialogModal>
  );
}
