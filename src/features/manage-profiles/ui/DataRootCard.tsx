import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { Button, ProgressBar, Tooltip } from "@proxyshard/shardx-ui-kit";
import { FolderIcon } from "../../../shared/icons";
import { toast } from "../../../shared/model/toast";
import { confirmModal } from "../../../shared/lib/confirm";
import { middleEllipsis } from "../../../shared/lib/middleEllipsis";
import { t, useT } from "../../../shared/i18n";
import {
  dataRootGet, dataRootMigrate,
  teamRecoveryCopies,
  snapshotCreate, snapshotInspect, snapshotPreview, snapshotProfilePayloads,
  type DataRootInfo, type MigrationProgress,
} from "../../../entities/settings";
import { profileImport } from "../../../entities/profile";

// Getters: the map is built at import time, so the language has to be read
// when the phase is shown, not here.
const PHASE_LABEL: Record<MigrationProgress["phase"], string> = {
  get scan() { return t("dataRootCard.phaseScan"); },
  get copy() { return t("dataRootCard.phaseCopy"); },
  get verify() { return t("dataRootCard.phaseVerify"); },
  get cleanup() { return t("dataRootCard.phaseCleanup"); },
  get done() { return t("dataRootCard.phaseDone"); },
};

/** Where profiles, user-data, extensions and the trash live. The move copies,
 *  verifies, then deletes; the backend refuses launches while it runs. */
export function DataRootCard() {
  const t = useT();
  const [info, setInfo] = useState<DataRootInfo | null>(null);
  const [progress, setProgress] = useState<MigrationProgress | null>(null);

  const refresh = () => dataRootGet().then(setInfo).catch(() => {});
  useEffect(() => { refresh(); }, []);

  useEffect(() => {
    let un: (() => void) | undefined;
    let disposed = false;
    listen<MigrationProgress>("data-migration", (e) => {
      setProgress(e.payload.phase === "done" ? null : e.payload);
    }).then((fn) => { if (disposed) fn(); else un = fn; });
    return () => { disposed = true; un?.(); };
  }, []);

  const running = progress !== null;

  const move = async () => {
    const dir = await open({ directory: true, title: t("dataRootCard.pickFolderTitle") });
    if (typeof dir !== "string") return;
    const ok = await confirmModal({
      // The destination is in the title; the body says what can go wrong (G2).
      title: t("dataRootCard.confirmTitle", { dir }),
      message: t("dataRootCard.confirmMessage"),
      buttons: [
        { label: t("dataRootCard.cancel"), value: false },
        { label: t("dataRootCard.moveAction"), value: true, primary: true },
      ],
    });
    if (ok !== true) return;
    setProgress({ phase: "scan", done: 0, total: 0, percent: 0, current: "" });
    try {
      const n = await dataRootMigrate(dir);
      toast.ok(n === 1 ? t("dataRootCard.movedOne") : t("dataRootCard.movedMany", { n }));
    } catch (e) {
      toast.fail({ title: t("dataRootCard.moveFailed"), detail: e });
    } finally {
      setProgress(null);
      refresh();
    }
  };

  return (
    <div className="flex flex-col gap-3">
      <p className="m-0 text-help text-text-sub-600">
        {info?.portable ? t("dataRootCard.portableIntro") : t("dataRootCard.intro")}
      </p>
      <div className="flex flex-col gap-1.5">
        <span className="text-secondary font-medium text-text-sub-600">
          {info && !info.custom ? t.rich("dataRootCard.currentLocationDefault", { muted: (s) => <span className="text-text-soft-400">{s}</span> }) : t("dataRootCard.currentLocation")}
        </span>
        <div className="flex items-center gap-2">
          {/* Cut in the middle, so the folder at its end stays (TYPE-07); whole on hover and to screen readers.
              50 characters fit beside "Open folder" and "Change" (G3 made the first button longer). */}
          <Tooltip content={info?.path ?? ""}>
            <span
              data-data-path
              aria-label={info?.path}
              className="min-w-0 flex-1 truncate rounded-control bg-bg-weak-50 px-[11px] py-[9px] text-mono text-text-sub-600"
            >
              {info ? middleEllipsis(info.path, 50) : "…"}
            </span>
          </Tooltip>
          <Button
            variant="neutral" mode="stroke" size="xsmall" disabled={!info || running}
            onClick={() => info && revealItemInDir(info.path).catch(() => toast.err(t("dataRootCard.openFailed")))}
          >
            {t("dataRootCard.reveal")}
          </Button>
          {!info?.portable && <Button
            variant="primary" mode="stroke" size="xsmall" disabled={running}
            leftIcon={<FolderIcon className="size-3.5" />}
            onClick={move}
          >
            {t("dataRootCard.change")}
          </Button>}
        </div>
      </div>

      {progress && (
        <div className="flex flex-col gap-1.5 tile p-3">
          <div className="flex items-baseline justify-between gap-3">
            <span className="text-secondary font-medium text-text-strong-950">
              {PHASE_LABEL[progress.phase]}
            </span>
            <span className="text-mono text-text-soft-400">
              {progress.total > 0 ? `${progress.done} / ${progress.total}` : ""}
            </span>
          </div>
          <ProgressBar aria-label={t("dataRootCard.moveBar")} value={progress.percent} max={100} />
          {progress.current && (
            <span className="truncate text-mono text-text-disabled-300">
              {middleEllipsis(progress.current, 64)}
            </span>
          )}
        </div>
      )}
    </div>
  );
}

/** Read-only list of team conflict copies kept after a stale session.
 * Restoring requires an explicit profile import, so this surface only reveals
 * the saved browser-data folder and never overwrites a live profile. */
export function TeamRecoveryCopies() {
  const t = useT();
  const [copies, setCopies] = useState<Awaited<ReturnType<typeof teamRecoveryCopies>>>([]);
  useEffect(() => { teamRecoveryCopies().then(setCopies).catch(() => {}); }, []);
  if (copies.length === 0) return <p className="m-0 text-help text-text-soft-400">{t("teamRecovery.noCopies")}</p>;
  return (
    <div className="flex flex-col gap-2">
      {copies.map((copy) => (
        <div key={`${copy.team}/${copy.profile_id}/${copy.attempt}`} className="flex items-center gap-3 rounded-control bg-bg-weak-50 px-3 py-2">
          <div className="min-w-0 flex-1">
            <div className="truncate text-secondary font-medium text-text-strong-950">
              {copy.team} / {copy.profile_id}
            </div>
            <div className="truncate text-help text-text-soft-400">
              {t("teamRecovery.attempt", { attempt: copy.attempt })}, {copy.bytes.toLocaleString()} B
            </div>
          </div>
          <Button
            variant="neutral" mode="stroke" size="xsmall"
            onClick={() => revealItemInDir(copy.path).catch(() => toast.err(t("dataRootCard.openFailed")))}
          >
            {t("teamRecovery.open")}
          </Button>
        </div>
      ))}
    </div>
  );
}

/** A complete local snapshot with a per-file manifest. Restoring is kept as an
 * explicit follow-up operation so an accidental click cannot overwrite a
 * profile or its encrypted credentials. */
export function SnapshotCard() {
  const t = useT();
  const [preview, setPreview] = useState<Awaited<ReturnType<typeof snapshotPreview>> | null>(null);
  const [busy, setBusy] = useState(false);
  const [inspection, setInspection] = useState<Awaited<ReturnType<typeof snapshotInspect>> | null>(null);

  useEffect(() => { snapshotPreview().then(setPreview).catch(() => {}); }, []);

  const create = async () => {
    const dir = await open({ directory: true, title: t("snapshot.pickDestination") });
    if (typeof dir !== "string") return;
    setBusy(true);
    try {
      const result = await snapshotCreate(dir);
      toast.ok(t("snapshot.created", { files: result.files, bytes: result.bytes.toLocaleString() }));
    } catch (e) {
      toast.fail({ title: t("snapshot.createFailed"), detail: e });
    } finally {
      setBusy(false);
    }
  };

  const inspect = async () => {
    const selected = await open({ directory: true, multiple: false, title: t("snapshot.pickExisting") });
    if (typeof selected !== "string") return;
    setBusy(true);
    try {
      setInspection(await snapshotInspect(selected));
    } catch (e) {
      toast.fail({ title: t("snapshot.inspectFailed"), detail: e });
    } finally {
      setBusy(false);
    }
  };

  const restore = async () => {
    const selected = await open({ directory: true, multiple: false, title: t("snapshot.pickExisting") });
    if (typeof selected !== "string") return;
    setBusy(true);
    try {
      const payloads = await snapshotProfilePayloads(selected);
      if (payloads.length === 0) {
        toast.fail({ title: t("snapshot.restoreEmpty") });
        return;
      }
      const ok = await confirmModal({
        title: t("snapshot.restoreTitle", { count: payloads.length }),
        message: t("snapshot.restoreMessage"),
        buttons: [
          { label: t("snapshot.cancel"), value: false },
          { label: t("snapshot.restoreAction"), value: true, primary: true },
        ],
      });
      if (ok !== true) return;
      const count = await profileImport(payloads);
      toast.ok(t("snapshot.restored", { count }));
    } catch (e) {
      toast.fail({ title: t("snapshot.restoreFailed"), detail: e });
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2">
        <Button variant="primary" mode="stroke" size="xsmall" disabled={busy} onClick={() => void create()}>
          {t("snapshot.create")}
        </Button>
        <Button variant="neutral" mode="stroke" size="xsmall" disabled={busy} onClick={() => void inspect()}>
          {t("snapshot.inspect")}
        </Button>
        <Button variant="neutral" mode="stroke" size="xsmall" disabled={busy} onClick={() => void restore()}>
          {t("snapshot.restore")}
        </Button>
      </div>
      {preview && (
        <p className="m-0 text-help text-text-soft-400">
          {t("snapshot.preview", { files: preview.entries.length, bytes: preview.entries.reduce((n, e) => n + e.bytes, 0).toLocaleString() })}
        </p>
      )}
      {inspection && (
        <p role="status" className="m-0 text-help text-text-sub-600">
          {t("snapshot.inspection", {
            verified: inspection.verified,
            files: inspection.files,
            missing: inspection.missing,
            changed: inspection.changed,
          })}
        </p>
      )}
      <p className="m-0 text-help text-text-soft-400">{t("snapshot.help")}</p>
    </div>
  );
}
