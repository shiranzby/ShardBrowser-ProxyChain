import { useEffect, useState } from "react";
import { automationFleet, lastRunOf, type RunState, type WorkerState } from "../../entities/automation";
import { useT } from "../../shared/i18n";

const TONE: Record<WorkerState["status"], string> = {
  queued: "text-text-soft-400",
  starting: "text-warning-base",
  running: "text-success-base",
  done: "text-text-sub-600",
  failed: "text-error-base",
  stopped: "text-text-soft-400",
};

/** A second window: every browser in every run, one row each. */
export function FleetMonitor() {
  const t = useT();
  const [runs, setRuns] = useState<RunState[] | null>(null);
  const [unavailable, setUnavailable] = useState(false);

  useEffect(() => {
    let alive = true;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let last = "";
    let failed = false;
    const tick = () => {
      automationFleet()
        .then((r) => {
          if (!alive) return;
          if (failed) {
            failed = false;
            setUnavailable(false);
          }
          // Unchanged: nothing to draw (PERF-23).
          const json = JSON.stringify(r);
          if (json !== last) { last = json; setRuns(r); }
          // Every 0.7 s while a run is going, every 3 s while none is.
          timer = setTimeout(tick, r.some((x) => x.running) ? 700 : 3000);
        })
        .catch(() => {
          if (!alive) return;
          if (!failed) {
            failed = true;
            setUnavailable(true);
          }
          timer = setTimeout(tick, 3000);
        });
    };
    tick();
    return () => { alive = false; clearTimeout(timer); };
  }, []);

  const active = runs?.filter((r) => r.running).length ?? 0;
  const status = {
    queued: t("fleetMonitor.queued"),
    starting: t("status.starting"),
    running: t("status.running"),
    done: t("fleetMonitor.finished"),
    finished: t("fleetMonitor.finished"),
    failed: t("fleetMonitor.failed"),
    stopped: t("fleetMonitor.stopped"),
  };

  return (
    <div className="flex h-screen flex-col gap-2 bg-bg-white-0 p-3 ring-1 ring-inset ring-stroke-soft-200">
      <div className="text-ui text-text-strong-950">
        {t("fleetMonitor.title")} {active > 0 && <span className="text-text-soft-400">({t("fleetMonitor.activeCount", { n: active })})</span>}
      </div>

      {unavailable && (
        <p role="status" className="m-0 text-secondary text-text-sub-600">
          {runs?.length ? t("fleetMonitor.stale") : t("fleetMonitor.unavailable")}
        </p>
      )}
      {runs === null ? (
        !unavailable && <p role="status" className="m-0 mt-6 text-center text-body text-text-soft-400">{t("listStates.loading")}</p>
      ) : runs.length === 0 ? (
        !unavailable &&
        <p className="m-0 mt-6 text-center text-balance text-body text-text-soft-400">
          {t("fleetMonitor.emptyState")}
        </p>
      ) : (
        <div className="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto">
          {runs.map((r) => (
            <div key={r.project_id} className="tile">
              <div className="flex items-center justify-between px-2.5 py-1.5">
                <span className="truncate text-secondary font-medium text-text-strong-950">{r.project_name}</span>
                <span className="text-secondary text-text-soft-400">
                  {r.running ? status.running : status[lastRunOf(r).outcome]}
                </span>
              </div>
              {/* Workers are the run's thread slots, in order; profile_id is empty until a slot takes one. */}
              {r.workers.map((w, i) => (
                <div
                  key={i}
                  className="grid grid-cols-[1fr_64px_76px] items-center gap-2 px-2.5 py-1.5"
                >
                  <div className="min-w-0">
                    <div className="truncate text-secondary text-text-strong-950">
                      {w.profile_name}
                    </div>
                    {w.note && (
                      <div className="truncate text-secondary text-text-soft-400">{w.note}</div>
                    )}
                  </div>
                  <div className="text-secondary text-text-soft-400">
                    {w.steps_total > 0 ? `${w.step}/${w.steps_total}` : "—"}
                  </div>
                  <div className={`text-right text-secondary ${TONE[w.status]}`}>
                    {status[w.status]}
                    {w.pass > 0 && <span className="text-text-soft-400"> {t("fleetMonitor.pass", { n: w.pass })}</span>}
                  </div>
                </div>
              ))}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
