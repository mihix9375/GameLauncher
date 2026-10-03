import { invoke } from "../core/tauri.js";

const originalWarn = console.warn.bind(console);
let initialized = false;
let warnedAboutLogFailure = false;

export function formatLogArguments(args) {
	return args.map(value => {
		if (value instanceof Error) return value.stack || `${value.name}: ${value.message}`;
		if (typeof value === "string") return value;
		try { return JSON.stringify(value) ?? String(value); }
		catch { return String(value); }
	}).join(" ").slice(0, 16_000);
}

export function writeLog(level, message, source = "ui") {
	if (!window.__TAURI__) return;
	// 保存は非同期。ログ保存失敗を再びログに送り、無限ループにしない。
	void invoke("write_client_log", { level, message: String(message).slice(0, 16_000), source }).catch(error => {
		if (warnedAboutLogFailure) return;
		warnedAboutLogFailure = true;
		originalWarn("ログファイルへ保存できませんでした:", error);
		const element = document.getElementById("log-text");
		if (element) element.textContent += "（ログファイルへ保存できませんでした）";
	});
}

export function initFileLogging() {
	if (initialized) return;
	initialized = true;
	for (const [method, level] of [["warn", "WARN"], ["error", "ERROR"], ["info", "INFO"], ["log", "INFO"]]) {
		const original = console[method].bind(console);
		console[method] = (...args) => {
			original(...args);
			writeLog(level, formatLogArguments(args), `console.${method}`);
		};
	}
	window.addEventListener("error", event => {
		const message = event.error ? formatLogArguments([event.error]) : `${event.message} (${event.filename}:${event.lineno}:${event.colno})`;
		setLogText(`エラー: ${message}`, "ERROR", "window.error");
	});
	window.addEventListener("unhandledrejection", event => {
		setLogText(`エラー: ${formatLogArguments([event.reason])}`, "ERROR", "unhandledrejection");
	});
}

export function setLogText(text, level = "INFO", source = "ui") {
	const logEl = document.getElementById("log-text");
	if (logEl) {
		logEl.textContent = text;
	}
	writeLog(level, text, source);
}
