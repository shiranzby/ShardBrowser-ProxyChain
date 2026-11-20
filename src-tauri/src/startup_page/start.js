import { reuseOrOpen } from "./logic.js";

const bootstrap = chrome.runtime.getURL("start.html");
const status = document.getElementById("status");
const retry = document.getElementById("retry");
const wait = ms => new Promise(resolve => setTimeout(resolve, ms));

async function checkPage(probe) {
  const target = await chrome.tabs.create({ url: globalThis.SHARD_STARTUP_URL, active: true });
  const until = Date.now() + 50_000;
  while (Date.now() < until) {
    const tab = await chrome.tabs.get(target.id);
    if (tab.status === "complete") {
      if (!tab.url || new URL(tab.url).origin !== probe.origin) return "unexpected_page_origin";
      const [result] = await chrome.scripting.executeScript({
        target: { tabId: target.id },
        args: [probe.origin],
        func: origin => {
          if (location.origin !== origin || document.readyState !== "complete") return false;
          const status = performance.getEntriesByType("navigation")[0]?.responseStatus;
          if (status && (status < 200 || status >= 300)) return false;
          return Boolean(document.body?.innerText.trim());
        },
      });
      if (result?.result === true) {
        return "ok";
      }
    }
    await wait(200);
  }
  return "unreadable_page";
}

async function runProbe(ownId, probe) {
  let outcome;
  try {
    outcome = await checkPage(probe);
  } catch {
    outcome = "page_check_failed";
  }
  const response = await fetch(`${probe.report}?outcome=${outcome}`, { method: "POST" });
  if (!response.ok || outcome !== "ok") throw new Error(outcome);
  await chrome.tabs.remove(ownId);
}

async function settleRestore() {
  let last = "", stable = 0;
  // Chrome constructs restored tabs before command-line tabs. Also tolerate
  // late creation without waiting for network loads or lazy tabs to finish.
  for (let i = 0; i < 12; i++) {
    const tabs = await chrome.tabs.query({});
    const signature = JSON.stringify(tabs.map(t => [t.id, t.pendingUrl || t.url]));
    stable = signature === last ? stable + 1 : 0;
    if (i >= 3 && stable >= 3) return;
    last = signature;
    await wait(200);
  }
}

async function run() {
  retry.hidden = true;
  try {
    const own = await chrome.tabs.getCurrent();
    if (!own?.id || own.incognito) throw new Error("startup_tab_unavailable");
    const probe = globalThis.SHARD_STARTUP_PROBE;
    if (location.hash) {
      if (location.hash === `#${probe?.nonce}`) {
        await runProbe(own.id, probe);
      } else {
        await chrome.tabs.update(own.id, { url: "about:blank" });
      }
      return;
    }
    if (probe) {
      await chrome.tabs.update(own.id, { url: "about:blank" });
      return;
    }
    await settleRestore();
    // Crash-restored bootstrap tabs and this launch's entry share one origin.
    // Serialize their check/update so they cannot both create the target.
    await navigator.locks.request("shardbrowser-startup-tab", async () => {
      for (let i = 0; i < 2; i++) {
        if (await reuseOrOpen(chrome.tabs, own.id, globalThis.SHARD_STARTUP_URL, bootstrap) !== "retry") return;
      }
      throw new Error("startup_target_changed");
    });
  } catch {
    status.textContent = "启动标签检查未完成，其他标签保持不变。 / Startup tab check could not finish. Other tabs were kept.";
    retry.hidden = Boolean(globalThis.SHARD_STARTUP_PROBE);
  }
}
retry.addEventListener("click", () => void run());
void run();
