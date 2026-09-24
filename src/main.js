import { initClock } from "./js/ui/clock.js";
import { setupEventListeners } from "./js/events/setupEventListeners.js";
import { loadGames } from "./js/games/loadGames.js";
import { setupScrollHint } from "./js/ui/scrollHint.js";

document.addEventListener("DOMContentLoaded", () => {
	initClock();
	setupEventListeners();
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
