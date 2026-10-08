import { test } from "node:test";
import assert from "node:assert/strict";
import { JSDOM } from "jsdom";

const dom = new JSDOM(`<div id="detail-modal"></div><div id="leaderboard-section"></div>
  <div id="leaderboard-list"></div><button id="tab-ranking" hidden></button>
  <button id="btn-jump-ranking" hidden></button>`);
globalThis.window = dom.window;
globalThis.document = dom.window.document;
let respond;
window.__TAURI__ = { core: { invoke: () => new Promise(resolve => { respond = resolve; }) } };
const { loadLeaderboards, stopLeaderboardAutoRefresh } = await import("../src/js/leaderboards/leaderboards.js");
const result = [{ id: "0", name: "Score", order: "high_score", enabled: true, entries: [] }];

test("late ranking response cannot revive UI after detail closes", async () => {
  const loading = loadLeaderboards({ id: "game" });
  document.dispatchEvent(new window.CustomEvent("modal:closed", { detail: { modalId: "detail-modal" } }));
  respond(result);
  await loading;
  assert.equal(document.getElementById("leaderboard-list").children.length, 0);
  assert.equal(document.getElementById("tab-ranking").hidden, true);
});

test("older game response cannot overwrite a newer detail view", async () => {
  const old = loadLeaderboards({ id: "old" });
  const respondOld = respond;
  const current = loadLeaderboards({ id: "current" });
  respond([{ ...result[0], name: "Current" }]);
  await current;
  respondOld(result);
  await old;
  assert.equal(document.querySelector(".leaderboard-card h5").textContent, "Current");
  stopLeaderboardAutoRefresh();
});
