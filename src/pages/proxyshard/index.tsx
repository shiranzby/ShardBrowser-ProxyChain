import { useEffect, useState } from "react";
import { Button } from "@proxyshard/shardx-ui-kit";
import { HeaderMenu, PageBody, PageHeader } from "../../widgets/PageHeader";
import { ProviderDefaultsPanel } from "../../features/manage-proxies/ui/ProviderDefaultsPanel";
import { useNav } from "../../shared/model/navigation";
import { useProxy } from "../../entities/proxy";
import { usePsAccount } from "../../entities/proxyshard";
import { OpenDashboardButton, PsBuyDialog, PsKeyDialog, PsKeySetup } from "../../features/proxyshard";
import { PsHeaderFacts } from "../../widgets/ProxyShard/PsHeaderFacts";
import { PsManagementPanels } from "../../widgets/ProxyShard/PsManagementPanels";
import { KeyIcon, RefreshIcon } from "../../shared/icons";
import { useDialogLife } from "../../shared/lib/useDialogLife";
import { useT } from "../../shared/i18n";

type View = "panels" | "setup";

/// Provider defaults are the primary page. Existing ProxyShard account tools
/// remain available on demand without loading its account on every visit.
export function ProxyShardPage() {
  const t = useT();
  const init = usePsAccount((s) => s.init);
  const key = usePsAccount((s) => s.key);
  const status = usePsAccount((s) => s.status);
  const refreshMe = usePsAccount((s) => s.refreshMe);
  const [accountOpen, setAccountOpen] = useState(false);
  const [keyOpen, setKeyOpen] = useState(false);
  const keyLife = useDialogLife(keyOpen);
  const [buying, setBuying] = useState(false);
  const buyLife = useDialogLife(buying);
  useEffect(() => { if (accountOpen) init(); }, [accountOpen, init]);
  // While a key is being read or checked, the body keeps what it showed (the
  // header says "Checking your key…"): empty before the first answer, so the
  // setup card never flashes, and no swap during Check again or a new key.
  const now: View | null = status === "ok" ? "panels" : status === "err" || key === "" ? "setup" : null;
  const [last, setLast] = useState<View | null>(null);
  if (now && now !== last) setLast(now);
  const view = now ?? last;

  return (
    <>
      <PageHeader
        title={t("provider.managementTitle")}
        help={{ title: t("provider.managementTitle"), body: t("provider.managementHelp") }}
        actions={<Button variant="primary" mode="filled" size="xsmall" onClick={() => { void useNav.getState().setSection("proxies").then((moved) => { if (moved) useProxy.getState().setBulkOpen(true); }); }}>{t("proxyBulkImporter.title")}</Button>}
      />
      <PageBody>
        <div className="flex flex-col gap-6" data-provider-management>
          <ProviderDefaultsPanel />
          <div className="border-t border-stroke-soft-200 pt-4">
            <Button variant="neutral" mode="ghost" size="xsmall" aria-expanded={accountOpen} onClick={() => setAccountOpen((open) => !open)}>{t("provider.legacyAccount")}</Button>
            {accountOpen && <div className="flex flex-col gap-4 pt-3">
              <h3 className="m-0 text-section-label text-text-strong-950">{t("pageHelp.proxyshard.title")}</h3>
              <p className="m-0 text-help text-text-sub-600">{t("pageHelp.proxyshard.body")}</p>
              <PsHeaderFacts />
              <div className="flex items-center gap-2">
                {view === "panels" && <Button variant="neutral" mode="stroke" size="xsmall" onClick={() => setBuying(true)}>{t("psBuyCard.open")}</Button>}
                <OpenDashboardButton />
                {view === "panels" && <HeaderMenu items={[
                  { label: t("psKey.changeItem"), icon: <KeyIcon className="size-3.5" />, onSelect: () => setKeyOpen(true) },
                  { label: t("psKey.checkItem"), icon: <RefreshIcon className="size-3.5" />, onSelect: () => void refreshMe() },
                ]} />}
              </div>
              {view === "panels" ? <PsManagementPanels /> : view === "setup" ? <PsKeySetup /> : null}
            </div>}
          </div>
        </div>
      </PageBody>
      {buyLife.mounted && <PsBuyDialog key={buyLife.session} open={buying} onClose={() => setBuying(false)} />}
      {keyLife.mounted && <PsKeyDialog key={keyLife.session} open={keyOpen} onClose={() => setKeyOpen(false)} />}
    </>
  );
}
