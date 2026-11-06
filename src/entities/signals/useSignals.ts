import { useEffect, useMemo } from "react";
import { create } from "zustand";
import {
  allViews, emptyFilters, folderNamesOf, makeProfileFilter, useFolderMeta, useProfile, useProfileUi, useViews, viewQuery,
  type ProfileFilters, type ProfileMeta, type ProfileView,
} from "../profile";
import { useProxy, type ProxyEntry } from "../proxy";
import { planTierOf, usePsPlans } from "../proxyshard";
import { tagsOf, useTags, type Tag } from "../tag";
import { useMinuteClock } from "../../shared/lib/useNow";
import { routeExitProxyId } from "../../shared/lib/profileRoute";
import { computeSignals, testedCountry, type Signal, type SignalInput, type SignalKind } from "./signals";
import { isHidden, pruneHidden, useHiddenSignals } from "./hidden";
import { readLocales, useProfileLocales } from "./locales";

type Stage = "idle" | "loading" | "done";
/** How far the Profiles page's own reads have come (each its first pass):
 *  the last tests of the proxies profiles use, the timezone and language of
 *  the profiles those tests can judge, and the ProxyShard plans. */
export const useSignalSources = create<{ tests: Stage; locales: Stage; plans: Stage }>(() => ({ tests: "idle", locales: "idle", plans: "idle" }));

/** The proxies the profiles use, in the list's order: the ones whose last test a card can need. */
function usedProxyIds(): string[] {
  const { profiles, proxies } = useProfile.getState();
  const known = new Set(proxies.map((x) => x.id));
  const used = new Set<string>();
  for (const p of profiles) { const id = routeExitProxyId(p); if (id && known.has(id)) used.add(id); }
  return [...used];
}

type IdleWindow = { requestIdleCallback?: (run: () => void, opts?: { timeout: number }) => number };
/** Once the page is idle: never in the way of the list's first paint. A
 *  timer where there is no idle callback (WebKit). */
function whenIdle(run: () => void) {
  const idle = (window as unknown as IdleWindow).requestIdleCallback;
  if (idle) idle(run, { timeout: 500 });
  else setTimeout(run, 0);
}

/** The profiles a proxy test can judge for a leak: on a proxy whose last test passed. */
function judgedIds(): string[] {
  const { profiles, proxies } = useProfile.getState();
  const known = new Set(proxies.map((x) => x.id));
  const snapshots = useProxy.getState().snapshots;
  return profiles.filter((p) => { const id = routeExitProxyId(p); return !p.proxy_route && id && known.has(id) && testedCountry(snapshots[id]) !== null; }).map((p) => p.id);
}

/** The residential tiers the profiles' proxies draw on. */
function usedTiers() {
  const { profiles, proxies } = useProfile.getState();
  const used = new Set(profiles.map(routeExitProxyId));
  return [...new Set(proxies.filter((x) => used.has(x.id)).map(planTierOf).filter((x) => x !== null))];
}

async function readTestsThenLocales() {
  await useProxy.getState().loadLastTests(usedProxyIds());
  useSignalSources.setState({ tests: "done", locales: "loading" });
  await readLocales(judgedIds());
  useSignalSources.setState({ locales: "done" });
}

async function readPlans() {
  await usePsPlans.getState().load(usedTiers());
  useSignalSources.setState({ plans: "done" });
}

let started = false;

/** A plan read this long ago is read again when the window comes back. */
const PLANS_STALE_MS = 5 * 60_000;

/// What the problem cards read beyond the list, once per app session when
/// the Profiles page opens, after the list is in and the page is idle, and
/// never blocking it:
/// - the last test of each proxy a profile uses (the proxy store's own when
///   the Proxies page read them; the rest at most four at a time, each once),
///   and again for a proxy that comes into use;
/// - then the timezone and language of each profile on a proxy that passed
///   its test (four at a time, each once), and of a profile that joins them;
/// - the ProxyShard plans those proxies draw on, quietly (the ProxyShard
///   page says what fails), and again in place when the window comes back
///   and the last read is five minutes old or more (a plan changed on the
///   website or another device).
/// Called from the page's mount effect, which runs after its first paint.
export function loadSignalSources(): void {
  if (started) return;
  started = true;
  const go = () => whenIdle(() => {
    useSignalSources.setState({ tests: "loading", plans: "loading" });
    void readTestsThenLocales();
    void readPlans();
    useProfile.subscribe((s, prev) => {
      if (s.profiles !== prev.profiles || s.proxies !== prev.proxies) {
        whenIdle(() => void useProxy.getState().loadLastTests(usedProxyIds()).then(() => readLocales(judgedIds())));
      }
    });
    useProxy.subscribe((s, prev) => {
      if (s.snapshots !== prev.snapshots) whenIdle(() => void readLocales(judgedIds()));
    });
    window.addEventListener("focus", () => void usePsPlans.getState().refresh(PLANS_STALE_MS));
  });
  if (useProfile.getState().status === "ready") { go(); return; }
  const stop = useProfile.subscribe((s) => {
    if (s.status !== "ready") return;
    stop();
    go();
  });
}

/** The query the cards count: the Folder, or a View's own saved query
 *  (a Folder, tag or proxy it names that is gone stops narrowing). */
type ScopeQuery = { folder: string; filters: ProfileFilters };

function scopeQuery(
  viewId: string | null, folder: string, views: ProfileView[],
  profiles: ProfileMeta[], registry: string[], tags: Tag[], proxies: ProxyEntry[],
): ScopeQuery {
  const view = viewId ? allViews(views).find((v) => v.id === viewId) ?? null : null;
  if (!view) return { folder, filters: emptyFilters() };
  const known = { folders: folderNamesOf(profiles, registry), tagIds: tags.map((x) => x.id), proxyIds: proxies.map((x) => x.id) };
  const q = viewQuery(view, view.query.columns, known);
  return { folder: q.folder, filters: q.filters };
}

export type SignalState = {
  /** The true signals the user has not hidden, in risk order. */
  signals: Signal[];
  /** The scope hides are kept for: "folder:<name>" or "view:<id>". */
  scopeKey: string;
  /** What they were computed from, for the widget's own lines. */
  input: SignalInput;
};

/// What needs the user in the current Folder or View. Outside a View the
/// cards count the Folder (filters a person sets narrow the list, not what
/// needs them); inside one, the View's own saved query. A search being
/// typed never changes them. The ProxyShard plans are the account's, in
/// every scope.
export function useSignalState(): SignalState {
  const parents = useFolderMeta((s) => s.parents);
  const profiles = useProfile((s) => s.profiles);
  const proxies = useProfile((s) => s.proxies);
  const running = useProfile((s) => s.running);
  const registry = useProfile((s) => s.folderRegistry);
  const folder = useProfileUi((s) => s.folder);
  const viewId = useProfileUi((s) => s.viewId);
  const views = useViews((s) => s.views);
  const tags = useTags((s) => s.tags);
  const byProfile = useTags((s) => s.byProfile);
  const snapshots = useProxy((s) => s.snapshots);
  const locales = useProfileLocales((s) => s.byId);
  const startFailed = useProfile((s) => s.startFailed);
  const failReasons = useProfile((s) => s.failReasons);
  const plans = usePsPlans((s) => s.plans);
  const hidden = useHiddenSignals((s) => s.hidden);
  const now = useMinuteClock((n) => n);

  const query = useMemo(
    () => scopeQuery(viewId, folder, views, profiles, registry, tags, proxies),
    [viewId, folder, views, profiles, registry, tags, proxies],
  );

  const scopeIds = useMemo(() => {
    const keep = makeProfileFilter(proxies, "", query.folder, query.filters, running, (id) => tagsOf(id, { tags, byProfile }), now, undefined, profiles, parents);
    return new Set(profiles.filter(keep).map((p) => p.id));
  }, [profiles, proxies, query, running, tags, byProfile, now, parents]);

  const input = useMemo<SignalInput>(
    () => ({ profiles, scopeIds, running, proxies, snapshots, locales, startFailed, failReasons, plans, now }),
    [profiles, scopeIds, running, proxies, snapshots, locales, startFailed, failReasons, plans, now],
  );
  const scopeKey = viewId ? `view:${viewId}` : `folder:${folder}`;
  const all = useMemo(() => computeSignals(input), [input]);
  const signals = useMemo(() => all.filter((s) => !isHidden(s, scopeKey, hidden)), [all, scopeKey, hidden]);

  // A hide whose card is no longer true here, or whose scope is gone, goes
  // (L3 review, N3). Only on what is read: the tests and the timezones for a
  // leak, the tests for a failure, the plans for the balance.
  const listIn = useProfile((s) => s.status === "ready");
  const testsIn = useSignalSources((s) => s.tests === "done");
  const localesIn = useSignalSources((s) => s.locales === "done");
  const plansIn = useSignalSources((s) => s.plans === "done");
  useEffect(() => {
    if (!listIn) return;
    const settled = new Set<SignalKind>();
    if (testsIn) settled.add("failedToday");
    if (localesIn) settled.add("locationLeak");
    if (plansIn) settled.add("proxyBalance");
    const folders = new Set(folderNamesOf(profiles, registry));
    const viewIds = new Set(allViews(views).map((v) => v.id));
    const exists = (scope: string) => scope === "folder:all"
      || (scope.startsWith("folder:") && folders.has(scope.slice("folder:".length)))
      || (scope.startsWith("view:") && viewIds.has(scope.slice("view:".length)));
    pruneHidden(scopeKey, settled, new Set(all.map((s) => s.kind)), exists);
  }, [listIn, testsIn, localesIn, plansIn, all, scopeKey, profiles, registry, views]);
  return { signals, scopeKey, input };
}

/** The visible signals for the current scope, minus the hidden ones. */
export function useSignals(): Signal[] {
  return useSignalState().signals;
}
