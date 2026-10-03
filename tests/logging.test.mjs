import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { JSDOM } from "jsdom";

const dom = new JSDOM('<div id="log-text"></div>');
const records = [];
let rejectWrites = false;
globalThis.window = dom.window;
globalThis.document = dom.window.document;
window.__TAURI__ = { core: { invoke: async (command, payload) => {
	if (rejectWrites) throw new Error("log storage unavailable");
	records.push({ command, ...payload });
} } };
const { setLogText, initFileLogging, formatLogArguments } = await import("../src/js/ui/log.js");

test("UI notification does not block and is forwarded for file logging", () => {
	setLogText("ダウンロード失敗", "ERROR");
	assert.equal(document.getElementById("log-text").textContent, "ダウンロード失敗");
	assert.equal(records.at(-1).command, "write_client_log");
	assert.equal(records.at(-1).level, "ERROR");
	assert.equal(records.at(-1).message, "ダウンロード失敗");
});

test("error serialization includes stacks and handles cycles and large messages", () => {
	assert.match(formatLogArguments([new Error("test failure")]), /test failure/);
	const circular = {}; circular.self = circular;
	assert.doesNotThrow(() => formatLogArguments([circular]));
	assert.equal(formatLogArguments(["x".repeat(20_000)]).length, 16_000);
});

test("console and unhandled errors are logged; initialization is idempotent", () => {
	const originals = Object.fromEntries(["warn", "error", "log", "info"].map(key => [key, console[key]]));
	try {
		initFileLogging(); initFileLogging();
		const before = records.length;
		console.warn("通信切断");
		assert.equal(records.length, before + 1);
		assert.equal(records.at(-1).source, "console.warn");
		assert.equal(records.at(-1).level, "WARN");
		window.dispatchEvent(new dom.window.ErrorEvent("error", { message: "runtime failure" }));
		assert.equal(records.at(-1).source, "window.error");
		assert.match(document.getElementById("log-text").textContent, /runtime failure/);
		const rejection = new dom.window.Event("unhandledrejection"); rejection.reason = new Error("rejected");
		window.dispatchEvent(rejection);
		assert.equal(records.at(-1).source, "unhandledrejection");
	} finally { Object.assign(console, originals); }
});

test("game loading and launching no longer use blocking browser dialogs", () => {
	for (const file of ["loadGames.js", "launchGame.js"]) {
		const source = readFileSync(new URL(`../src/js/games/${file}`, import.meta.url), "utf8");
		assert.doesNotMatch(source, /\b(?:alert|confirm|prompt)\s*\(/);
	}
});

test("log saving failure does not interrupt operations or recurse", async () => {
	rejectWrites = true;
	setLogText("操作は継続できます");
	await Promise.resolve();
	assert.match(document.getElementById("log-text").textContent, /操作は継続できます/);
	assert.match(document.getElementById("log-text").textContent, /ログファイルへ保存できませんでした/);
	rejectWrites = false;
	setLogText("再び操作できます");
	assert.equal(records.at(-1).message, "再び操作できます");
});
