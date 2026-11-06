import { useT } from "../i18n";
import { SelectField } from "./SelectField";
import type { ChainPolicy } from "../model/proxyChain";

export function ChainPolicyField({ value, onChange, profile = false }: { value?: ChainPolicy; onChange: (value: ChainPolicy) => void; profile?: boolean }) {
  const t = useT();
  return <div className="flex flex-col gap-2" data-chain-policy>
    <SelectField label={t("proxyChain.policyLabel")} value={value ?? "inherit"} onChange={onChange} options={[
      { value: "inherit", label: profile ? t("proxyChain.policyInheritProfile") : t("proxyChain.policyInherit") },
      { value: "enabled", label: t("proxyChain.policyEnabled") },
      { value: "disabled", label: t("proxyChain.policyDisabled") },
      { value: "prefer_first_hop", label: t("proxyChain.policyPreferFirstHop") },
    ]} />
    <p className="m-0 text-help text-text-sub-600">{t("proxyChain.policyHelp")}</p>
    {value === "prefer_first_hop" && <p data-chain-prefer-help className="m-0 text-help text-text-sub-600">{t("proxyChain.policyPreferHelp")}</p>}
  </div>;
}
