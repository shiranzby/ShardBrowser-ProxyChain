import { useT } from "../i18n";
import { SelectField } from "./SelectField";

type Policy = "inherit" | "enabled" | "disabled";
export function ChainPolicyField({ value, onChange, profile = false }: { value?: Policy; onChange: (value: Policy) => void; profile?: boolean }) {
  const t = useT();
  return <div className="flex flex-col gap-2" data-chain-policy>
    <SelectField label={t("proxyChain.policyLabel")} value={value ?? "inherit"} onChange={onChange} options={[
      { value: "inherit", label: profile ? t("proxyChain.policyInheritProfile") : t("proxyChain.policyInherit") },
      { value: "enabled", label: t("proxyChain.policyEnabled") },
      { value: "disabled", label: t("proxyChain.policyDisabled") },
    ]} />
    <p className="m-0 text-help text-text-sub-600">{t("proxyChain.policyHelp")}</p>
  </div>;
}
