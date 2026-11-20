import { useEffect, useRef, useState, type ReactNode } from "react";
import { Button, Input, Textarea, usePresence, useLatched, type MenuItem } from "@proxyshard/shardx-ui-kit";
import { useGpuCompat, useGpuCompatReady, verdictsLoaded } from "../../shared/model/gpuCompat";
import { useNav } from "../../shared/model/navigation";
import { useT } from "../../shared/i18n";
import { useDialogLife } from "../../shared/lib/useDialogLife";
import { pageTitle, refocusIfLost } from "../../shared/lib/focus";
import { rowActionIcons } from "../../shared/ui/rowActionIcons";
import { PeekBody, PeekHeader, PeekPanel, PeekSection } from "../../shared/ui/peek";
import { fingerprintOs, useFingerprint, type FingerprintEntry } from "../../entities/fingerprint";
import { IncompatibleExplainer, IncompatibleWarningModal } from "../../features/gpu-compat";
import { openProfileForm } from "../../features/profile-form";
import { toast } from "../../shared/model/toast";

/// Puts the keyboard back on a fingerprint's card: its grid item, the element
/// that takes focus (F27). A card a filter hid is not there to return to: the
/// grid's one Tab stop takes it, else the page title (PC-080).
export function focusFingerprintCard(id: string) {
  requestAnimationFrame(() => {
    const card = document.querySelector<HTMLElement>(`[data-fingerprint-id="${CSS.escape(id)}"]`)?.closest<HTMLElement>("[data-row-id]");
    (card ?? document.querySelector<HTMLElement>("main [data-row-id][tabindex='0']") ?? pageTitle())?.focus();
  });
}

/** The card after this one in the library as shown, or the one before. */
function neighbourCard(id: string): string | null {
  const ids = [...document.querySelectorAll<HTMLElement>("[data-fingerprint-id]")].map((el) => el.dataset.fingerprintId!);
  const at = ids.indexOf(id);
  return at < 0 ? null : ids[at + 1] ?? ids[at - 1] ?? null;
}

/// The side panel for one library fingerprint: what it claims, whether this
/// machine can back it, and "New profile from this" in its header. The shell
/// (frame, header, sections, the way in and out) is every Peek's (F1).
export function FingerprintPeek() {
  const t = useT();
  const peekId = useFingerprint((s) => s.peekId);
  const closePeek = useFingerprint((s) => s.closePeek);
  const entry = useFingerprint((s) => (peekId ? s.items.find((e) => e.id === peekId) : undefined));
  const [editing, setEditing] = useState(false);

  // The fingerprint left the library (removed): Peek goes with it.
  const shownId = useRef<string | null>(null);
  useEffect(() => {
    if (entry) shownId.current = entry.id;
    else if (peekId && shownId.current === peekId) closePeek();
  }, [peekId, entry, closePeek]);
  useEffect(() => setEditing(false), [peekId]);

  // Kept through the exit: the panel leaves showing what it showed (FEEL-02).
  const open = peekId && entry ? entry : null;
  const presence = usePresence(open?.id ?? null, { maxExitMs: 250 });
  const shown = useLatched(open);
  if (!presence.mounted || !shown) return null;
  const close = () => { closePeek(); focusFingerprintCard(shown.id); };

  return (
    <PeekPanel kind="fingerprint" label={t("fingerprintPeek.label", { name: shown.label })} presence={presence}>
      {/* Keyed: a wait or a warning for one fingerprint never carries to the next. */}
      <FingerprintPeekHeader key={shown.id} entry={shown} onClose={close} />
      <PeekBody>
        <PeekSection id="details" title={t("fingerprintPeek.details")}>
          {editing ? <PayloadEditor entry={shown} onDone={() => setEditing(false)} /> : <Details entry={shown} onEdit={() => setEditing(true)} />}
        </PeekSection>
        <PeekSection id="tags" title={t("fingerprintPeek.tags")}><Tags entry={shown} /></PeekSection>
        <PeekSection id="field-checks" title={t("fingerprintPeek.fieldChecks")}><FieldChecks entry={shown} /></PeekSection>
        <PeekSection id="compat" title={t("fingerprintPeek.compat")}><Compat entry={shown} /></PeekSection>
      </PeekBody>
    </PeekPanel>
  );
}

function FieldChecks({ entry }: { entry: FingerprintEntry }) {
  const t = useT();
  const fields = [
    ["webgl", t("fingerprintPeek.gpu")],
    ["webrtc", "WebRTC"],
    ["canvas", "Canvas"],
    ["fonts", t("fingerprintPeek.fieldFonts")],
    ["navigator", t("fingerprintPeek.fieldLanguage")],
    ["timezone", t("fingerprintPeek.fieldTimezone")],
    ["screen", t("fingerprintPeek.screen")],
  ] as const;
  const labels = {
    missing: t("fingerprintPeek.fieldMissing"),
    structure_valid: t("fingerprintPeek.fieldValid"),
    invalid: t("fingerprintPeek.fieldInvalid"),
  };
  const sourceLabels = {
    imported: t("fingerprintPeek.fieldImportedSource"),
    unavailable: t("fingerprintPeek.fieldUnavailableSource"),
  };
  return (
    <>
      <p className="m-0 mb-2 text-secondary text-text-soft-400">{t("fingerprintPeek.fieldSource")}</p>
      <dl className="m-0 grid grid-cols-[minmax(0,1fr)_auto] gap-x-3 gap-y-1 text-secondary">
        {fields.map(([field, label]) => {
          const status = entry.field_checks?.[field];
          const source = entry.field_sources?.[field] ?? "unavailable";
          return (
            <div key={field} className="contents">
              <dt>{label}</dt>
              <dd className="m-0">
                {status ? labels[status] : t("fingerprintPeek.notChecked")}
                <span className="ms-1 text-text-soft-400">({sourceLabels[source]})</span>
              </dd>
            </div>
          );
        })}
      </dl>
    </>
  );
}

function Tags({ entry }: { entry: FingerprintEntry }) {
  const t = useT();
  const saveTags = useFingerprint((s) => s.saveTags);
  const [value, setValue] = useState((entry.tags ?? []).join(", "));
  const [saving, setSaving] = useState(false);
  useEffect(() => setValue((entry.tags ?? []).join(", ")), [entry.id, entry.tags]);
  const save = async () => {
    const tags = value.split(",").map((tag) => tag.trim()).filter(Boolean);
    setSaving(true);
    try { await saveTags(entry.id, tags); } finally { setSaving(false); }
  };
  return (
    <div className="flex flex-col gap-2">
      <Input
        label={t("fingerprintPeek.tagsInput")}
        value={value}
        placeholder={t("fingerprintPeek.tagsPlaceholder")}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => { if (e.key === "Enter") void save(); }}
      />
      <Button variant="neutral" mode="stroke" size="xsmall" isLoading={saving} onClick={() => void save()}>
        {t("fingerprintPeek.saveTags")}
      </Button>
    </div>
  );
}

function FingerprintPeekHeader({ entry, onClose }: { entry: FingerprintEntry; onClose: () => void }) {
  const t = useT();
  const remove = useFingerprint((s) => s.remove);
  const copyJson = useFingerprint((s) => s.copyJson);
  const { start, busy, warning } = useNewProfileFrom(entry);
  // The Fingerprint Peek's own menu (R9): a built-in one says why it can't go.
  const menuItems: MenuItem[] = [
    { label: t("fingerprintLibrary.export"), onSelect: () => void copyJson(entry) },
    entry.builtin
      ? { label: t("fingerprintPeek.removeItem"), icon: rowActionIcons.trash({ className: "size-3.5" }), disabled: true, hint: t("fingerprintPeek.builtin"), onSelect: () => {} }
      : {
          label: t("fingerprintPeek.removeItem"), icon: rowActionIcons.trash({ className: "size-3.5" }), danger: true,
          // Removed, its card and its Peek go: the keyboard goes to the next
          // card, else the one before, not to the page (Pair 18 review,
          // M12); the last one gone, to the page title (PC-080).
          onSelect: () => {
            const next = neighbourCard(entry.id);
            void remove(entry.id).then((gone) => {
              if (!gone) return;
              if (next) focusFingerprintCard(next);
              else refocusIfLost(pageTitle);
            });
          },
        },
  ];
  return (
    <>
      <PeekHeader
        subject={entry.id}
        title={<h2 className="m-0 min-w-0 truncate text-panel-title text-text-strong-950">{entry.label}</h2>}
        primary={
          // Short beside the title, so a long fingerprint name stays whole;
          // the spoken name says what it starts from (G3, WCAG 2.5.3).
          <Button variant="primary" mode="lighter" size="3xsmall" isLoading={busy} onClick={() => void start()} aria-label={t("fingerprintPeek.newProfile")}>
            {t("fingerprintPeek.newProfileShort")}
          </Button>
        }
        menuItems={menuItems}
        menuLabel={t("pageHeader.more")}
        closeLabel={t("peek.close")}
        onClose={onClose}
      />
      {warning}
    </>
  );
}

/// "New profile from this": waits for this machine's GPU check, asks first
/// when the machine can't back the fingerprint, then opens the new-profile
/// form on the Profiles page with this fingerprint chosen. Nothing is made
/// until Create profile (manual save). A card's menu runs it too.
export function useNewProfileFrom(entry: FingerprintEntry) {
  const closePeek = useFingerprint((s) => s.closePeek);
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);
  // One opening per press, even while this machine's GPU check still runs.
  const running = useRef(false);
  const compat = useGpuCompat((s) => s.byId[entry.id]);
  // The question stays mounted, with the verdict it showed, through its exit (FEEL-01).
  const askLife = useDialogLife(asking);
  const compatShown = useLatched(compat);

  const create = async () => {
    // The Profiles page, as New profile anywhere goes there; an open editor with unsaved changes may keep us here.
    const there = await useNav.getState().setSection("profiles");
    running.current = false;
    setBusy(false);
    if (!there) return;
    closePeek();
    openProfileForm({ mode: "create", tab: "basics", preset: { gpu_preset_id: entry.id } });
  };
  // Decided on this machine's verdict if it comes within a second; a hung check does not hold the form.
  const start = async () => {
    if (running.current) return;
    running.current = true;
    setBusy(true);
    await verdictsLoaded();
    const { byId, suppressed } = useGpuCompat.getState();
    const verdict = byId[entry.id];
    if (verdict && !verdict.compatible && !suppressed) {
      running.current = false;
      setBusy(false);
      setAsking(true);
      return;
    }
    await create();
  };
  const warning = askLife.mounted && compatShown ? (
    <IncompatibleWarningModal
      key={askLife.session}
      open={asking}
      compat={compatShown}
      onKeep={() => { setAsking(false); running.current = true; void create(); }}
      onCancel={() => setAsking(false)}
    />
  ) : null;
  return { start, busy, warning };
}

function Details({ entry, onEdit }: { entry: FingerprintEntry; onEdit: () => void }) {
  const t = useT();
  const nav = entry.payload?.navigator ?? {};
  const screen = entry.payload?.screen ?? {};
  const rows: [string, ReactNode][] = [
    [t("fingerprintPeek.os"), fingerprintOs(entry)],
    [t("fingerprintPeek.chrome"), entry.chrome || "—"],
    [t("fingerprintPeek.gpu"), entry.gpu || "—"],
    [t("fingerprintPeek.screen"), screen.width && screen.height
      // device_pixel_ratio is the FingerprintConfig field.
      ? t("fingerprintPeek.screenValue", { w: screen.width, h: screen.height, r: screen.device_pixel_ratio ?? screen.pixel_ratio ?? 1 })
      : "—"],
    [t("fingerprintPeek.cores"), nav.hardware_concurrency ?? "—"],
    [t("fingerprintPeek.memory"), nav.device_memory ? t("fingerprintPeek.memoryValue", { n: nav.device_memory }) : "—"],
  ];
  return (
    <>
      {/* Soft 12 px labels, strong 13 px values, 14 px apart (owner, "more air"). */}
      <dl className="m-0 grid grid-cols-[auto_1fr] items-baseline gap-x-4 gap-y-[14px]">
        {rows.map(([k, v]) => (
          <div key={k} className="contents">
            <dt className="text-secondary text-text-soft-400">{k}</dt>
            <dd data-selectable className="m-0 min-w-0 break-words text-ui text-text-strong-950">{v}</dd>
          </div>
        ))}
      </dl>
      {nav.user_agent && (
        <div className="flex flex-col gap-[6px]">
          <span className="text-secondary text-text-soft-400">{t("fingerprintPeek.userAgent")}</span>
          <p data-selectable className="m-0 [overflow-wrap:anywhere] rounded-control bg-bg-weak-50 p-2 text-mono leading-[18px] text-text-sub-600">{nav.user_agent}</p>
        </div>
      )}
      <Button
        variant="neutral"
        mode="stroke"
        size="xsmall"
        disabled={entry.builtin}
        title={entry.builtin ? t("fingerprintPeek.builtinEdit") : undefined}
        onClick={onEdit}
      >
        {t("fingerprintPeek.editJson")}
      </Button>
    </>
  );
}

function PayloadEditor({ entry, onDone }: { entry: FingerprintEntry; onDone: () => void }) {
  const t = useT();
  const updatePayload = useFingerprint((s) => s.updatePayload);
  const rollbackPayload = useFingerprint((s) => s.rollbackPayload);
  const [value, setValue] = useState(() => JSON.stringify(entry.payload, null, 2));
  const [saving, setSaving] = useState(false);
  const save = async () => {
    setSaving(true);
    try {
      JSON.parse(value);
      await updatePayload(entry.id, value);
      onDone();
    } catch (error) {
      // Parse errors and backend validation errors both stay in the editor so
      // the user can correct the payload without losing their draft.
      if (error instanceof SyntaxError) {
        toast.fail({ title: t("fingerprintPeek.invalidJson") });
      }
    } finally {
      setSaving(false);
    }
  };
  const rollback = async () => {
    setSaving(true);
    try {
      await rollbackPayload(entry.id);
      onDone();
    } catch {
      // The store reports the backend error. Keep the editor open so the
      // current payload is still available to the user.
    } finally {
      setSaving(false);
    }
  };
  return (
    <div className="flex flex-col gap-2">
      <Textarea
        label={t("fingerprintPeek.editJson")}
        rows={14}
        className="mono w-full"
        value={value}
        onChange={(e) => setValue(e.target.value)}
      />
      <div className="flex gap-2">
        <Button variant="primary" size="xsmall" isLoading={saving} onClick={() => void save()}>{t("fingerprintPeek.saveJson")}</Button>
        <Button variant="neutral" mode="stroke" size="xsmall" disabled={saving} onClick={() => void rollback()}>{t("fingerprintPeek.rollbackJson")}</Button>
        <Button variant="neutral" mode="stroke" size="xsmall" disabled={saving} onClick={onDone}>{t("fingerprintPeek.cancelEdit")}</Button>
      </div>
    </div>
  );
}

function Compat({ entry }: { entry: FingerprintEntry }) {
  const t = useT();
  useGpuCompatReady();
  const loaded = useGpuCompat((s) => s.loaded);
  const compat = useGpuCompat((s) => s.byId[entry.id]);
  // No verdict is not a clean one: say so rather than "can run it".
  if (!loaded || !compat) return <p className="m-0 text-secondary text-text-soft-400">{t("fingerprintPeek.notChecked")}</p>;
  if (compat.compatible) return <p className="m-0 text-secondary text-success-text">{t("fingerprintPeek.compatible")}</p>;
  return <IncompatibleExplainer compat={compat} />;
}
