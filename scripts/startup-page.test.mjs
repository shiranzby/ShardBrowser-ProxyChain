import test from 'node:test';
import assert from 'node:assert/strict';
import { canonical, existingTab, reuseOrOpen } from '../src-tauri/src/startup_page/logic.js';
const BOOT = 'chrome-extension://cdaadhlkdbidpoihgpljlkjkhhpccbih/start.html';
function apiFor(tabs) {
  const rows = structuredClone(tabs), calls = [];
  return { rows, calls,
    get: async id => { const tab = rows.find(t => t.id === id); if (!tab) throw Error('closed'); return { ...tab }; },
    query: async () => rows.map(t => ({ ...t })),
    update: async (id, patch) => { calls.push(['update', id, patch]); Object.assign(rows.find(t => t.id === id), patch); },
    remove: async id => { calls.push(['remove', id]); rows.splice(rows.findIndex(t => t.id === id), 1); },
  };
}
test('startup matching preserves query and fragment and supports pending URLs', () => {
  assert.equal(canonical('https://EXAMPLE.com:443'), 'https://example.com/');
  const tabs = [{ id: 2, url: 'about:blank', pendingUrl: 'https://example.com/?a=1#section' }];
  assert.equal(existingTab(tabs, 'https://example.com/?a=1#section', 1)?.id, 2);
  assert.equal(existingTab([{ id: 2, url: 'https://example.com/', pendingUrl: 'https://other.example/' }], 'https://example.com/', 1), undefined);
  for (const other of ['http://example.com/?a=1#section', 'https://example.com/other', 'https://example.com/?a=2#section', 'https://example.com/?a=1#other']) assert.equal(existingTab(tabs, other, 1), undefined);
  assert.equal(existingTab([{ ...tabs[0], incognito: true }], tabs[0].pendingUrl, 1), undefined);
});
test('a restored target is reused and only the bootstrap is closed', async () => {
  const tabs = [{ id: 1, url: BOOT, active: true }, { id: 2, url: 'https://example.com/' }, { id: 3, url: 'https://example.com/other' }, { id: 4, url: 'https://example.com/' }];
  const api = apiFor(tabs);
  assert.equal(await reuseOrOpen(api, 1, 'https://example.com/', BOOT), 'reused');
  assert.deepEqual(api.rows.map(t => [t.id, t.url]), tabs.slice(1).map(t => [t.id, t.url]));
  assert.deepEqual(api.calls, [['update', 2, { active: true }], ['remove', 1]]);
});
test('missing target replaces its bootstrap and the next serialized bootstrap reuses it', async () => {
  const api = apiFor([{ id: 1, url: BOOT }, { id: 2, url: BOOT }, { id: 3, url: 'https://other.example/' }]);
  assert.equal(await reuseOrOpen(api, 1, 'https://example.com/', BOOT), 'opened');
  assert.equal(await reuseOrOpen(api, 2, 'https://example.com/', BOOT), 'reused');
  assert.deepEqual(api.rows.map(t => t.url), ['https://example.com/', 'https://other.example/']);
});

test('clearing the preference retires a restored bootstrap without reopening its old target', async () => {
  const api = apiFor([{ id: 1, url: BOOT }, { id: 2, url: 'https://other.example/' }]);
  assert.equal(await reuseOrOpen(api, 1, null, BOOT), 'cleared');
  assert.deepEqual(api.calls, [['update', 1, { url: 'about:blank' }]]);
  assert.deepEqual(api.rows.map(t => t.url), ['about:blank', 'https://other.example/']);
});
test('navigation by the user and lookup failures do not overwrite or remove their tab', async () => {
  for (const tab of [{ id: 1, url: 'https://user.example/' }, { id: 1, url: BOOT, pendingUrl: 'https://user.example/' }]) {
    const api = apiFor([tab]);
    assert.equal(await reuseOrOpen(api, 1, 'https://example.com/', BOOT), 'user_navigated');
    assert.equal(api.calls.length, 0);
  }
  const api = apiFor([{ id: 1, url: BOOT }]);
  api.query = async () => { throw Error('unavailable'); };
  await assert.rejects(() => reuseOrOpen(api, 1, 'https://example.com/', BOOT));
  assert.equal(api.calls.length, 0);
});
test('a target that closes during selection leaves the bootstrap recoverable', async () => {
  const api = apiFor([{ id: 1, url: BOOT }, { id: 2, url: 'https://example.com/' }]);
  const get = api.get;
  api.get = async id => { if (id === 2) throw Error('closed'); return get(id); };
  await assert.rejects(() => reuseOrOpen(api, 1, 'https://example.com/', BOOT));
  assert.equal(api.calls.length, 0);
});

test('navigation during the async lookup leaves the bootstrap recoverable', async () => {
  const api = apiFor([{ id: 1, url: BOOT }, { id: 2, url: 'https://example.com/' }]);
  const query = api.query;
  let first = true;
  api.query = async (...args) => {
    const result = await query(...args);
    if (first) { first = false; api.rows[0].pendingUrl = 'https://user.example/'; }
    return result;
  };
  assert.equal(await reuseOrOpen(api, 1, 'https://example.com/', BOOT), 'user_navigated');
  assert.equal(api.rows[0].url, BOOT);
  assert.equal(api.rows[0].pendingUrl, 'https://user.example/');
  assert.equal(api.calls.length, 0);
});
