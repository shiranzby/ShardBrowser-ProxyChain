import { Input, Select } from "@proxyshard/shardx-ui-kit";
import { useT } from "../../../shared/i18n";

type SessionMode = "sticky" | "rotating" | "per_request";

export function ProviderSessionFields<M extends Exclude<SessionMode, "sticky">>({
  alternate, mode, minutes, count, maxMinutes, disabled, onMode, onMinutes, onCount, invalidCount,
}: {
  alternate: M;
  mode: "sticky" | M;
  minutes: string;
  count: string;
  maxMinutes: number;
  disabled: boolean;
  onMode: (mode: "sticky" | M) => void;
  onMinutes: (minutes: string) => void;
  onCount: (count: string) => void;
  invalidCount?: boolean;
}) {
  const t = useT();
  const tnb = alternate === "rotating";
  const sticky = mode === "sticky";
  return (
    <>
      <Select label={tnb ? t("tnb.sessionType") : t("provider.sessionType")} size="xsmall" value={mode} disabled={disabled}
        onChange={(value) => onMode(value === "sticky" ? "sticky" : alternate)}
        options={[
          { value: "sticky", label: tnb ? t("tnb.stickySession") : t("provider.sessionSticky") },
          { value: alternate, label: tnb ? t("tnb.rotatingSession") : t("provider.sessionPerRequest") },
        ]} />
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        {sticky && <Input label={tnb ? t("tnb.ttl") : t("provider.sessionMinutes")}
          hint={tnb ? t("tnb.ttlHint") : undefined} inputSize="xsmall" type="number" min={1} max={maxMinutes}
          step={1} value={minutes} disabled={disabled} onChange={(e) => onMinutes(e.target.value)} />}
        <Input label={tnb ? t("tnb.count") : t("provider.generationCount")}
          hint={invalidCount ? t("provider.generationCountRange") : undefined} inputSize="xsmall"
          type="number" min={1} max={1000} step={1} value={sticky || tnb ? count : "1"}
          disabled={disabled || (!sticky && !tnb)} onChange={(e) => onCount(e.target.value)} />
      </div>
    </>
  );
}
