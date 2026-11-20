import { useEffect, useState } from "react";
import { Button, Card, Input } from "@proxyshard/shardx-ui-kit";
import { sequentialStart, sequentialStatus, sequentialStop, type SequentialConfig, type SequentialState } from "../../entities/automation";
import { useT } from "../../shared/i18n";
import { toast } from "../../shared/model/toast";

const initial: SequentialConfig = {
  start: 1,
  end: 1,
  url: "https://example.com/",
  wait_seconds: 15,
};

export function SequentialCard() {
  const t = useT();
  const [config, setConfig] = useState(initial);
  const [state, setState] = useState<SequentialState | null>(null);

  useEffect(() => {
    let alive = true;
    const read = () => { void sequentialStatus().then((next) => { if (alive) setState(next); }).catch(() => {}); };
    read();
    const timer = window.setInterval(read, 800);
    return () => { alive = false; window.clearInterval(timer); };
  }, []);

  const running = !!state?.running;
  const active = running || state?.current_id != null;
  const update = <K extends keyof SequentialConfig>(key: K, value: SequentialConfig[K]) =>
    setConfig((current) => ({ ...current, [key]: value }));

  const start = async () => {
    try {
      await sequentialStart(config);
      setState(await sequentialStatus());
    } catch (error) {
      toast.fail({ title: t("sequential.startFailed"), detail: error });
    }
  };

  const stop = async () => {
    try {
      await sequentialStop();
      setState(await sequentialStatus());
    } catch (error) {
      toast.fail({ title: t("sequential.stopFailed"), detail: error });
    }
  };

  return (
    <Card className="mb-3 p-4" data-sequential-card>
      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <h2 className="m-0 text-heading-3">{t("sequential.title")}</h2>
          <p className="m-0 mt-1 text-secondary text-text-soft-400">{t("sequential.help")}</p>
        </div>
        {active ? (
          <Button variant="neutral" mode="stroke" size="xsmall" onClick={() => void stop()}>{t("sequential.stop")}</Button>
        ) : (
          <Button variant="primary" mode="filled" size="xsmall" onClick={() => void start()}>{t("sequential.start")}</Button>
        )}
      </div>
      <div className="mt-3 grid grid-cols-4 gap-2">
        <Input inputSize="xsmall" type="number" min={1} label={t("sequential.startProfile")} value={String(config.start)} disabled={active} onChange={(e) => update("start", Number(e.target.value) || 1)} />
        <Input inputSize="xsmall" type="number" min={1} label={t("sequential.endProfile")} value={String(config.end)} disabled={active} onChange={(e) => update("end", Number(e.target.value) || 1)} />
        <Input inputSize="xsmall" type="number" min={1} max={86400} label={t("sequential.wait")} value={String(config.wait_seconds)} disabled={active} onChange={(e) => update("wait_seconds", Number(e.target.value) || 1)} />
        <Input inputSize="xsmall" label={t("sequential.url")} value={config.url} disabled={active} onChange={(e) => update("url", e.target.value)} />
      </div>
      {state && (
        <p className="m-0 mt-2 text-secondary text-text-soft-400" aria-live="polite">
          {state.phase}, {state.completed}/{state.total}, {state.failed} {t("sequential.failedCount")}
          {state.current ? `, ${state.current}` : ""}
        </p>
      )}
    </Card>
  );
}
