/** Usage statistics are disabled in this fork. */
export function initAnalytics(): Promise<void> {
  try {
    localStorage.removeItem("shardx-analytics-client-id");
    localStorage.removeItem("shardx-analytics-session-count");
    localStorage.removeItem("shardx.analytics");
  } catch { /* Restricted storage does not affect startup. */ }
  return Promise.resolve();
}

export async function trackSection(_section: string): Promise<void> {}
