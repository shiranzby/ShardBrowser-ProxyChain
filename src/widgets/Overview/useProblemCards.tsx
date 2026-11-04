import { useMemo } from "react";
import type { BadgeColor } from "@proxyshard/shardx-ui-kit";
import { may, useProfile, useProfileUi } from "../../entities/profile";
import { useProxy } from "../../entities/proxy";
import { isLifetime, type PlanUsage } from "../../entities/proxyshard";
import { hideSignal, type Signal, type SignalInput, type SignalKind } from "../../entities/signals";
import { tierLabel } from "../../features/proxyshard";
import { AUTO_LANG, AUTO_TZ, LOCALES } from "../../shared/constants";
import { intlLocale, translate, useLang, type Lang } from "../../shared/i18n";
import { countryWord } from "../../shared/lib/geo";
import { proxyChainErrorFromText, proxyChainErrorText } from "../../shared/lib/proxyChainError";
import { useNav } from "../../shared/model/navigation";
import { toast } from "../../shared/model/toast";

type T = (key: string, vars?: Record<string, string | number>) => string;

/** One thing a card does: a verb, and what it runs. */
export type CardAction = { id: string; label: string; run: () => unknown };

/** A profile this person may change: always outside a team (its rights on the row). */
const editable = (id: string) => {
  const p = useProfile.getState().profiles.find((x) => x.id === id);
  return !p || may(p, "edit");
};

/// One problem card, ready to draw: a title with its severity pill, what is
/// wrong in one plain sentence, why it matters in one more, one detail line
/// (or the plan's usage), one fix, and a quiet count at the right. The card's
/// click shows the profiles it names.
export type CardModel = {
  kind: SignalKind | "automation";
  label: string;
  pill: { color: BadgeColor; text: string };
  what: string;
  /** Why it matters: what happens if it is left, in one plain sentence. */
  why: string;
  /** One line; a plan's usage draws its bar instead. */
  detail: string;
  usage: { share: number; used: string; ends: string | null } | null;
  fix: CardAction | null;
  /** "2 more", "6 profiles use it"; null when there is nothing more. */
  more: string | null;
  /** The card's click: the profiles it names, in the list. */
  show: () => void;
  /** Hides the card in this scope until what it is about changes. */
  hide: (() => void) | null;
  /** An account at risk: the folded Problems to fix header names it. */
  risk: boolean;
};

/** `useT`, but one function per language, so the memo below can depend on it. */
function useLangT(): { t: T; lang: Lang } {
  const lang = useLang((s) => s.lang);
  return useMemo(() => ({ t: (key: string, vars?: Record<string, string | number>) => translate(lang, key, vars), lang }), [lang]);
}

const DAY = 86_400_000;

/** "1.2 GB", "10 GB": one decimal at most. */
function gb(bytes: number, lang: Lang, unit = true): string {
  const n = new Intl.NumberFormat(intlLocale(lang), { maximumFractionDigits: 1 }).format(bytes / 1024 ** 3);
  return unit ? `${n} GB` : n;
}
const clock = (ms: number, lang: Lang) => new Intl.DateTimeFormat(intlLocale(lang), { hour: "numeric", minute: "2-digit" }).format(ms);
const day = (ms: number, lang: Lang) => new Intl.DateTimeFormat(intlLocale(lang), { month: "short", day: "numeric" }).format(ms);

/** A backend's reason in plain words, where it is a known one. */
function reasonOf(raw: string, t: T): string {
  if (!raw.trim()) return t("overview.whyUnknown");
  const chainError = proxyChainErrorFromText(raw);
  if (chainError) return proxyChainErrorText(t, chainError) ?? raw;
  if (/timed?\s*out|timeout/i.test(raw)) return t("overview.whyTimeout");
  if (/\b407\b|auth|credential|password/i.test(raw)) return t("overview.whyLogin");
  if (/refused/i.test(raw)) return t("overview.whyRefused");
  if (/port in use|address in use/i.test(raw)) return t("overview.whyPort");
  // The backend's own words, as the end of a sentence.
  const words = raw.replace(/^Error:\s*/, "").replace(/[.\s]+$/, "");
  return words.charAt(0).toLowerCase() + words.slice(1);
}

/** The list shows exactly these profiles, under a removable chip. */
function showOnly(label: string, ids: string[]) {
  const ui = useProfileUi.getState();
  ui.setSearch("");
  ui.setOnly({ label, ids });
}

/** Timezone and language back to Auto (from proxy): each profile then takes
 *  both from its proxy's location at its next start. */
async function matchProxy(ids: string[], names: Map<string, string>, t: T) {
  const store = useProfile.getState();
  let done = 0;
  for (const id of ids) {
    try {
      const form = await store.loadForm(id);
      if ((await store.saveProfile({ ...form, timezone: AUTO_TZ, language: AUTO_LANG })) === "ok") done++;
      // The store leaves a failed save to its caller (WRITE-15): say which.
      else toast.fail({ title: t("useProfile.saveFailed", { name: names.get(id) ?? "" }), subject: names.get(id) });
    } catch {
      // The read failed (the profile is gone): nothing changed.
    }
  }
  if (done === 1 && ids.length === 1) toast.ok(t("overview.matchedOne", { name: names.get(ids[0]) ?? "" }));
  else if (done > 0) toast.ok(t("overview.matchedMany", { n: done }));
}

/** Retests the failing proxies (a progress toast, then the result). The proxy
 *  store may not have its list yet: the Profiles store's is the same. */
function retest(ids: string[]) {
  const px = useProxy.getState();
  if (px.proxies.length === 0) px.setProxies(useProfile.getState().proxies);
  return useProxy.getState().bulkTest(ids);
}

/** The cards for the true signals, in risk order. `topUp` opens the Top up
 *  dialog for a plan. */
export function useProblemCards(signals: Signal[], input: SignalInput, scopeKey: string, topUp: (plan: PlanUsage) => void): CardModel[] {
  const { t, lang } = useLangT();
  return useMemo(
    () => signals.map((s) => cardOf(s, input, t, lang, () => hideSignal(s, scopeKey), topUp)),
    [signals, input, scopeKey, t, lang, topUp],
  );
}

function cardOf(s: Signal, input: SignalInput, t: T, lang: Lang, hide: () => void, topUp: (plan: PlanUsage) => void): CardModel {
  const profiles = (n: number) => (n === 1 ? t("overview.profilesOne", { n }) : t("overview.profilesMany", { n }));
  const more = (n: number) => (n > 0 ? t("overview.more", { n }) : null);
  const names = new Map(input.profiles.map((p) => [p.id, p.name]));
  switch (s.kind) {
    case "locationLeak": {
      const { direct, mismatch } = s.detail;
      const label = t("overview.leak");
      const base = { kind: s.kind, label, pill: { color: "error" as const, text: profiles(s.count) }, usage: null, hide, risk: true, show: () => showOnly(label, s.ids) };
      // (a) first: a profile on no proxy leaks on every start.
      if (direct.length > 0) {
        const named = direct.slice(0, 3);
        // The pill counts every profile on the card; when others are here for a
        // mismatch, the sentence says how many of those start on the real IP.
        const total = direct.length + mismatch.length;
        const n = direct.length;
        const realIp = mismatch.length > 0
          ? n === 1 ? t("overview.leakRealIpOfOne", { n, total }) : t("overview.leakRealIpOfMany", { n, total })
          : n === 1 ? t("overview.leakRealIpOne", { n }) : t("overview.leakRealIpMany", { n });
        return {
          ...base,
          what: realIp,
          why: t("overview.mattersLeakRealIp"),
          detail: t("overview.leakNoProxy", { names: named.map((p) => p.name).join(", ") }),
          // The Assign dialog for exactly these profiles; the selection stays.
          // A team's profiles this person may not edit are left out; with none left, no fix.
          fix: direct.some((p) => editable(p.id))
            ? { id: "assign", label: t("overview.assignProxy"), run: () => useProfileUi.getState().openAssign(direct.filter((p) => editable(p.id)).map((p) => p.id)) }
            : null,
          more: more(direct.length - named.length + mismatch.length),
        };
      }
      const first = mismatch[0];
      const tz = mismatch.some((m) => m.tz);
      const language = mismatch.some((m) => m.lang);
      const ip = countryWord(first.ip, lang);
      const languageName = first.lang ? LOCALES.find((l) => l.code === first.lang)?.label ?? first.lang : "";
      return {
        ...base,
        what: tz && language ? t("overview.leakBoth") : tz ? t("overview.leakTimezone") : t("overview.leakLanguage"),
        why: t("overview.mattersLeakMismatch"),
        detail: first.tz
          ? t("overview.leakTimeOn", { name: first.name, zone: countryWord(first.tz, lang), ip })
          : t("overview.leakLanguageOn", { name: first.name, language: languageName, ip }),
        fix: { id: "match", label: t("overview.matchProxy"), run: () => matchProxy(mismatch.map((m) => m.id), names, t) },
        more: more(mismatch.length - 1),
      };
    }
    case "proxyBalance": {
      const { plan, low, daysLeft } = s.detail;
      const label = t("overview.balance");
      const name = t("overview.planName", { tier: tierLabel(t, plan.tier) });
      const now = input.now;
      const endsIn = plan.endsAt !== null ? Math.ceil((plan.endsAt - now) / DAY) : null;
      const pill = low && plan.remain !== null
        ? t("overview.balanceLeft", { amount: gb(plan.remain, lang) })
        : endsIn === null || endsIn < 0 ? t("overview.balanceEnded")
        : endsIn === 0 ? t("overview.balanceEndsToday")
        : endsIn === 1 ? t("overview.balanceEndsTomorrow")
        : t("overview.balanceEndsIn", { n: endsIn });
      let what: string;
      if (low) {
        what = daysLeft === null ? t("overview.balanceAlmostOut", { plan: name })
          : daysLeft <= 1 ? t("overview.balanceRunsOutDay", { plan: name })
          : t("overview.balanceRunsOut", { plan: name, n: daysLeft });
      } else {
        const date = plan.endsAt !== null ? day(plan.endsAt, lang) : "";
        what = plan.endsAt !== null && plan.endsAt < now ? t("overview.balanceEndedOn", { plan: name, date }) : t("overview.balanceEndsOn", { plan: name, date });
      }
      // Running low, ended, or ending soon: what the profiles on the plan then lose.
      const why = low ? t("overview.mattersBalanceLow") : plan.endsAt !== null && plan.endsAt < now ? t("overview.mattersBalanceEnded") : t("overview.mattersBalanceEnds");
      const usage = plan.total !== null && plan.spent !== null
        ? {
          share: Math.min(1, Math.max(0, plan.spent / plan.total)),
          used: t("overview.balanceUsed", { used: gb(plan.spent, lang, false), total: gb(plan.total, lang) }),
          // A plan with no end (residential traffic; the API's 2125) has no date to print.
          ends: plan.endsAt !== null && !isLifetime(plan, now) ? t("overview.balanceEnds", { date: day(plan.endsAt, lang) }) : null,
        }
        : null;
      const fix: CardAction = low && plan.orderId !== null
        ? { id: "topUp", label: t("overview.topUp"), run: () => topUp(plan) }
        : { id: "open", label: t("overview.openProxyShard"), run: () => useNav.getState().setSection("proxyshard") };
      return {
        kind: s.kind, label, pill: { color: "warning", text: pill }, what, why, usage,
        detail: usage ? `${usage.used}${usage.ends ? `, ${usage.ends}` : ""}` : "",
        fix,
        more: s.count === 0 ? null : s.count === 1 ? t("overview.usedByOne", { n: 1 }) : t("overview.usedByMany", { n: s.count }),
        // No profile here uses it: the plan's own page.
        show: s.ids.length > 0 ? () => showOnly(label, s.ids) : () => useNav.getState().setSection("proxyshard"),
        hide, risk: false,
      };
    }
    case "failedToday": {
      const { starts, proxies } = s.detail;
      const label = t("overview.failed");
      const start = starts[0];
      const failingIds = proxies.map((x) => x.id);
      let what: string;
      let why: string;
      let detail: string;
      let fix: CardAction;
      if (start) {
        const onFailing = start.proxyId !== null ? proxies.find((x) => x.id === start.proxyId) : undefined;
        what = t("overview.failedStart", { name: start.name, why: reasonOf(onFailing && !start.reason ? onFailing.error : start.reason, t) });
        why = t("overview.mattersFailedStart");
        detail = onFailing
          ? t("overview.failedLastTryProxy", { time: clock(start.at, lang), proxy: onFailing.name })
          : t("overview.failedLastTry", { time: clock(start.at, lang) });
        fix = onFailing
          ? { id: "retest", label: failingIds.length === 1 ? t("overview.retestProxy") : t("overview.retestProxies"), run: () => retest(failingIds) }
          : {
            id: "again", label: t("overview.tryAgain"),
            run: () => { const p = useProfile.getState().profiles.find((x) => x.id === start.id); return p ? useProfile.getState().startStop(p) : undefined; },
          };
      } else {
        const px = proxies[0];
        what = t("overview.failedProxy", { proxy: px.name, why: reasonOf(px.error, t) });
        why = t("overview.mattersFailedProxy");
        const time = px.lastSeen !== null ? clock(px.lastSeen, lang) : "";
        detail = px.usedBy === 1 ? t("overview.failedTestedOne", { time }) : t("overview.failedTestedMany", { time, n: px.usedBy });
        fix = { id: "retest", label: failingIds.length === 1 ? t("overview.retestProxy") : t("overview.retestProxies"), run: () => retest(failingIds) };
      }
      return {
        kind: s.kind, label, pill: { color: "error", text: profiles(s.count) }, what, why, detail, usage: null, fix,
        more: more(s.count - 1), show: () => showOnly(label, s.ids), hide, risk: false,
      };
    }
  }
}

/// Automation runs (the mock's fourth card) need the run history from the
/// backend (ledger: backend item). Until it exists the capability is off and
/// the card never shows; the card is built here so that turning it on is a
/// data change, not a design one.
export const AUTOMATION_RUN_HISTORY = false;

export type FailedRun = { project: string; step: number; reason: string; nextAt: number | null; profiles: number };

export function useAutomationCard(run: FailedRun | null): CardModel | null {
  const { t, lang } = useLangT();
  return useMemo(() => {
    if (!AUTOMATION_RUN_HISTORY || !run) return null;
    return {
      kind: "automation", label: t("overview.automation"), pill: { color: "error", text: t("overview.automationFailed", { n: 1 }) },
      what: t("overview.automationStopped", { project: run.project, step: run.step, why: reasonOf(run.reason, t) }),
      why: t("overview.mattersAutomation"),
      detail: run.nextAt !== null ? t("overview.automationNext", { time: clock(run.nextAt, lang), n: run.profiles }) : "",
      usage: null,
      fix: { id: "openRun", label: t("overview.openRun"), run: () => useNav.getState().setSection("automation") },
      more: null, show: () => useNav.getState().setSection("automation"), hide: null, risk: false,
    };
  }, [run, t, lang]);
}
