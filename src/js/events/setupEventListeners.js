import { filterAndRenderGames } from "../games/filterGames.js";
import { loadGames } from "../games/loadGames.js";
import { openModal, closeModal } from "../ui/modal.js";
import { setLogText } from "../ui/log.js";
import { getSelectedGame } from "../core/state.js";
import { launchGame } from "../games/launchGame.js";

export function setupEventListeners() {
	const searchInput = document.getElementById("search-input");
	if (searchInput) {
		searchInput.addEventListener("input", (e) => {
			filterAndRenderGames(e.target.value);
		});
	}

	const btnRefresh = document.getElementById("btn-refresh");
	if (btnRefresh) {
		btnRefresh.addEventListener("click", async () => {
			setLogText("更新中...");
			await loadGames();
			setLogText("更新完了");
		});
	}

	const btnSettings = document.getElementById("btn-settings");
	if (btnSettings) {
		btnSettings.addEventListener("click", () => {
			openModal("settings-modal");
		});
	}

	document.getElementById("btn-close-modal")?.addEventListener("click", () => closeModal("detail-modal"));
	document.getElementById("modal-backdrop")?.addEventListener("click", () => closeModal("detail-modal"));
	
	document.getElementById("btn-close-settings")?.addEventListener("click", () => closeModal("settings-modal"));
	document.getElementById("settings-backdrop")?.addEventListener("click", () => closeModal("settings-modal"));
	document.getElementById("btn-save-settings")?.addEventListener("click", () => {
		closeModal("settings-modal");
		setLogText("設定を保存しました");
	});

	const btnLaunch = document.getElementById("btn-launch-game");
	if (btnLaunch) {
		btnLaunch.addEventListener("click", async () => {
			const selectedGame = getSelectedGame();
			if (selectedGame) {
				await launchGame(selectedGame);
			}
		});
	}
}
