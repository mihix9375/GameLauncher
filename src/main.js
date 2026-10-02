import { initClock } from "./js/ui/clock.js";
import { setupEventListeners } from "./js/events/setupEventListeners.js";
import { loadGames } from "./js/games/loadGames.js";
import { setupScrollHint } from "./js/ui/scrollHint.js";
import { setupFullscreenToggle } from "./js/ui/fullscreen.js";

// WebView2は最小化・非表示状態でタイマーやページ処理を休止する場合がある。
// 未解決のWeb Lockを保持し、起動直後に最小化されても初期処理を継続させる。
function keepRendererAliveWhileMinimized() {
	if (!navigator.locks?.request) return;
	void navigator.locks.request(
		"gamelauncher-background-runtime",
		{ mode: "shared" },
		() => new Promise(() => {}),
	).catch(error => console.warn("Background runtime lock was not available:", error));
}

document.addEventListener("DOMContentLoaded", () => {
	keepRendererAliveWhileMinimized();
	initClock();
	setupEventListeners();
	setupFullscreenToggle();
	setupScrollHint(
		document.getElementById("main-scroll"),
		document.getElementById("game-list-scroll-hint"),
		{
			minStep: 240,
			stepRatio: 0.78,
			observedElements: [document.getElementById("game-list")],
		},
	);
	loadGames();
});
