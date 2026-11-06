import { reuseOrOpen } from "./logic.js";

const bootstrap = chrome.runtime.getURL("start.html");
const status = document.getElementById("status");
const retry = document.getElementById("retry");
const wait = ms => new Promise(resolve => setTimeout(resolve, ms));

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
    retry.hidden = false;
  }
}
retry.addEventListener("click", () => void run());
void run();
