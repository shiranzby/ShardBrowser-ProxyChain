import { useT } from "../../../shared/i18n";
import type { SyncKinds } from "../../../entities/sync";

type Props = {
  kinds: SyncKinds;
};

/** Keeps the sync boundary visible beside the switches. */
export function SyncScopeSummary({ kinds }: Props) {
  const t = useT();
  const enabled = [
    kinds.profiles === "all" ? t("sync.scopeProfilesAll") : t("sync.scopeProfilesSelected"),
    kinds.proxies === "all" ? t("sync.scopeProxiesAll") : t("sync.scopeProxiesBound"),
    kinds.folders_tags && t("sync.kindFolders"),
    kinds.bookmarks && t("sync.kindBookmarks"),
    kinds.extensions && t("sync.kindExtensions"),
    kinds.automation && t("sync.kindAutomation"),
  ].filter((value): value is string => Boolean(value));

  return (
    <section data-sync-scope className="flex flex-col gap-2">
      <div>
        <h3 className="m-0 text-body font-medium text-text-strong-950">{t("sync.scopeTitle")}</h3>
        <p className="m-0 text-help text-text-sub-600">{t("sync.scopeHelp")}</p>
      </div>
      <div className="flex flex-wrap gap-1.5" aria-label={t("sync.scopeIncluded")}>
        {enabled.map((item) => (
          <span key={item} className="rounded-control bg-bg-weak-50 px-2 py-1 text-help text-text-strong-950">
            {item}
          </span>
        ))}
      </div>
      <p className="m-0 text-help text-text-sub-600">{t("sync.scopeLocalOnly")}</p>
    </section>
  );
}
