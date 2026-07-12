import { invoke } from "../core/tauri.js";
import { filterAndRenderGames } from "../games/filterGames.js";
import { loadGames } from "../games/loadGames.js";
import { openModal, closeModal } from "../ui/modal.js";
import { setLogText } from "../ui/log.js";
import { getSelectedGame, setGameUpdateFlag, getAllGames } from "../core/state.js";
import { launchGame } from "../games/launchGame.js";
import { renderGames } from "../games/renderGames.js";

export function setupEventListeners() {
	const searchInput = document.getElementById("search-input");
	if (searchInput) {
		searchInput.addEventListener("input", (e) => {
			filterAndRenderGames(e.target.value);
		});
	}

	if (window.__TAURI__) {
		invoke("get_client_config").then(cfg => {
			if (cfg && cfg.animations_enabled === false) {
				document.body.classList.add("no-animations");
			}
		}).catch(() => {});
	}

	const btnRefresh = document.getElementById("btn-refresh");
	if (btnRefresh) {
		btnRefresh.addEventListener("click", async () => {
			setLogText("同期・更新中...");
			await loadGames();
			if (window.__TAURI__) {
				try {
					await invoke("sync_updates");
				} catch (e) {
					console.warn("sync_updates error:", e);
				}
			}
			setLogText("更新完了");
		});
	}

	const btnSettings = document.getElementById("btn-settings");
	if (btnSettings) {
		btnSettings.addEventListener("click", async () => {
			if (window.__TAURI__) {
				try {
					const cfg = await invoke("get_client_config");
					if (cfg) {
						const urlEl = document.getElementById("setting-server-url");
						if (urlEl) urlEl.value = cfg.server_url || "http://[::1]:50050";
						const pathEl = document.getElementById("setting-games-path");
						if (pathEl) pathEl.value = cfg.games_path || "";
						const animEl = document.getElementById("anim-toggle");
						if (animEl) animEl.checked = cfg.animations_enabled ?? true;
					}
				} catch (e) {
					console.warn("get_client_config error:", e);
				}
			}
			openModal("settings-modal");
		});
	}

	document.getElementById("btn-close-modal")?.addEventListener("click", () => closeModal("detail-modal"));
	document.getElementById("modal-backdrop")?.addEventListener("click", () => closeModal("detail-modal"));
	
	document.getElementById("btn-close-settings")?.addEventListener("click", () => closeModal("settings-modal"));
	document.getElementById("settings-backdrop")?.addEventListener("click", () => closeModal("settings-modal"));
	document.getElementById("btn-save-settings")?.addEventListener("click", async () => {
		const server_url = document.getElementById("setting-server-url")?.value || "http://[::1]:50050";
		const games_path = document.getElementById("setting-games-path")?.value || "";
		const animations_enabled = document.getElementById("anim-toggle")?.checked ?? true;

		if (animations_enabled) {
			document.body.classList.remove("no-animations");
		} else {
			document.body.classList.add("no-animations");
		}

		if (window.__TAURI__) {
			try {
				await invoke("save_client_config", {
					config: { server_url, games_path, animations_enabled }
				});
				setLogText("設定を保存し適用しました");
				await loadGames();
			} catch (e) {
				console.warn("save_client_config error:", e);
				setLogText("設定の保存に失敗しました");
			}
		} else {
			setLogText("設定を保存しました");
		}
		closeModal("settings-modal");
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

	if (window.__TAURI__ && window.__TAURI__.event) {
		window.__TAURI__.event.listen("update_notice", async (event) => {
			const payload = event.payload;
			if (payload && payload.game_id) {
				const cleanId = payload.game_id.replace(".exe", "");
				const existing = getAllGames().find(g => g.id === payload.game_id || g.game === payload.game_id || g.id === cleanId);
				if (!existing || !existing.isInstalled || existing.version !== payload.version || existing.hasUpdate) {
					setLogText(`[更新通知] ${cleanId} (v${payload.version}) を検出。自動ダウンロード開始...`);
					setGameUpdateFlag(cleanId, payload.version);
					renderGames(getAllGames());
					try {
						await invoke("download_game", { gameId: payload.game_id, version: payload.version });
						setLogText(`[完了] ${payload.game_id} の自動ダウンロード完了。自動リフレッシュ中...`);
						await loadGames();
					} catch (e) {
						console.error("Auto download error:", e);
						setLogText(`[エラー] ${payload.game_id} の自動ダウンロード失敗: ${e}`);
					}
				} else {
					setLogText(`[確認] ${payload.game_id} は既に最新バージョン (v${payload.version}) です`);
				}
			}
		});
	}
}
