import { ExternalLinkIcon } from "../../../shared/icons";
import { useT } from "../../../shared/i18n";
import { HeaderMenu } from "../../../widgets/PageHeader/HeaderMenu";
import { menuItemsFor } from "../../commands";
import { buyProxies } from "../lib/buyProxies";
import { useProxyShardVisibility } from "../../../shared/model/proxyShardVisibility";
import { useState } from "react";
import { ProxyFolderManager } from "./ProxyFolderManager";

/// The Proxies header's `…` menu: the page's commands from the registry
/// (E4), and ProxyShard's shop (owner default: "Buy proxies" lives here and
/// in the empty state, CRAFT-25), a link out rather than a command.
export function ProxiesMoreMenu() {
  const t = useT();
  const [foldersOpen, setFoldersOpen] = useState(false);
  const hideProxyShard = useProxyShardVisibility((s) => s.hidden);
  return (
    <>
      <HeaderMenu
        items={[
          ...menuItemsFor("pageMenu", "proxies", { ids: [] }),
          { label: t("proxyFolder.manageTitle"), onSelect: () => setFoldersOpen(true) },
          ...(!hideProxyShard ? [{ label: t("proxiesMoreMenu.buy"), icon: <ExternalLinkIcon className="size-3.5" />, onSelect: buyProxies }] : []),
        ]}
      />
      <ProxyFolderManager open={foldersOpen} onClose={() => setFoldersOpen(false)} />
    </>
  );
}
