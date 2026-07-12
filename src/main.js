import { initClock } from "./js/ui/clock.js";
import { setupEventListeners } from "./js/events/setupEventListeners.js";
import { loadGames } from "./js/games/loadGames.js";

document.addEventListener("DOMContentLoaded", () => {
	initClock();
	setupEventListeners();
	loadGames();
});
