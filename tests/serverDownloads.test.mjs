import { test } from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { JSDOM } from "jsdom";

test("server requests download uninstalled games in parallel without launching; ordinary notices do not", async () => {
  const html = await readFile(new URL("../src/index.html", import.meta.url), "utf8");
  const dom = new JSDOM(html, { url: "http://localhost" });
  globalThis.window = dom.window;
  globalThis.document = dom.window.document;
  globalThis.requestAnimationFrame = (fn) => dom.window.setTimeout(fn, 0);
  const listeners = new Map();
  const downloads = [];
  const calls = [];
  dom.window.__TAURI__ = {
    core: { invoke: async (command, args) => {
      calls.push(command);
      if (command === "download_game") return new Promise(resolve => downloads.push({ ...args, resolve }));
      if (command === "get_game_metadata") return { title: args.gameId };
      if (command === "get_client_config") return {};
      if (command === "refresh") return [];
      return null;
    } },
    event: { listen: async (name, callback) => { listeners.set(name, callback); return () => {}; } }
  };
  const { setAllGames } = await import("../src/js/core/state.js");
  const { setupEventListeners } = await import("../src/js/events/setupEventListeners.js");
  setAllGames(Array.from({ length: 6 }, (_, i) => ({ id: `game${i}`, title: `Game ${i}`, isInstalled: false, version: "" }))
    .concat({ id: "latest", title: "Latest", isInstalled: true, version: "1" }));
  try {
    setupEventListeners();
    await new Promise(resolve => setTimeout(resolve, 10));
    const notify = payload => listeners.get("update_notice")({ payload });
    await notify({ game_id: "game0", version: "1" });
    assert.equal(downloads.length, 0);
    for (let i = 0; i < 6; i++) await notify({ game_id: `game${i}`, version: "1", download_requested: true });
    assert.equal(downloads.length, 4);
    await notify({ game_id: "game0", version: "1", download_requested: true });
    await notify({ game_id: "latest", version: "1", download_requested: true });
    assert.equal(downloads.length, 4);
    downloads[0].resolve();
    await new Promise(resolve => setTimeout(resolve, 30));
    assert.equal(downloads.length, 5);
    downloads[1].resolve();
    await new Promise(resolve => setTimeout(resolve, 30));
    assert.equal(downloads.length, 6);
    assert.equal(new Set(downloads.map(item => item.gameId)).size, 6);
    assert.equal(calls.includes("launch_game"), false);
  } finally { dom.window.close(); }
});
