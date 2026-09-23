import { invoke } from "../core/tauri.js";
import { filterAndRenderGames } from "../games/filterGames.js";
import { loadGames } from "../games/loadGames.js";
import { openModal, closeModal } from "../ui/modal.js";
import { setLogText } from "../ui/log.js";
import { getSelectedGame, setGameUpdateFlag, getAllGames, removeGameById } from "../core/state.js";
import { launchGame } from "../games/launchGame.js";
import { renderGames } from "../games/renderGames.js";
import { submitComment } from "../comments/comments.js";
import { setCommunityTab } from "../ui/communityTabs.js";
import { updateGameCount } from "../ui/counter.js";

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
			setLogText("ローカル情報およびサーバーと同期中...");
			await loadGames();
			if (window.__TAURI__) {
				try {
					await invoke("sync_updates");
				} catch (e) {
					console.warn("sync_updates error:", e);
					setLogText("サーバーへの接続に失敗しました");
				}
			}
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
						const leaderboardUrlEl = document.getElementById("setting-leaderboard-url");
						if (leaderboardUrlEl) leaderboardUrlEl.value = cfg.leaderboard_url || "http://127.0.0.1:50052";
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
	document.getElementById("tab-comments")?.addEventListener("click", () => setCommunityTab("comments"));
	document.getElementById("tab-ranking")?.addEventListener("click", () => setCommunityTab("ranking"));
	document.getElementById("btn-jump-ranking")?.addEventListener("click", () => {
		setCommunityTab("ranking");
		document.querySelector("#detail-modal .detail-sidebar")?.scrollIntoView({ behavior: "smooth", block: "nearest" });
	});
	document.getElementById("btn-jump-comments")?.addEventListener("click", () => {
		setCommunityTab("comments");
		document.querySelector("#detail-modal .detail-sidebar")?.scrollIntoView({ behavior: "smooth", block: "nearest" });
	});
	
	document.getElementById("btn-close-settings")?.addEventListener("click", () => closeModal("settings-modal"));
	document.getElementById("settings-backdrop")?.addEventListener("click", () => closeModal("settings-modal"));
	document.getElementById("btn-save-settings")?.addEventListener("click", async () => {
		const server_url = document.getElementById("setting-server-url")?.value || "http://[::1]:50050";
		const leaderboard_url = document.getElementById("setting-leaderboard-url")?.value || "http://127.0.0.1:50052";
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
					config: { server_url, leaderboard_url, games_path, animations_enabled }
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

	const commentContent = document.getElementById("comment-content");
	const commentLength = document.getElementById("comment-length");
	commentContent?.addEventListener("input", () => {
		if (commentLength) commentLength.textContent = `${commentContent.value.length} / 1000`;
	});
	commentContent?.addEventListener("keydown", (event) => {
		if ((event.ctrlKey || event.metaKey) && event.key === "Enter") {
			event.preventDefault();
			submitComment();
		}
	});
	document.getElementById("btn-submit-comment")?.addEventListener("click", submitComment);

	if (window.__TAURI__ && window.__TAURI__.event) {
		const downloadQueue = [];
		let isDownloading = false;
		const pendingGameIds = new Set();
		const removedGameIds = new Set();

		async function processDownloadQueue() {
			if (isDownloading) return;
			isDownloading = true;
			while (downloadQueue.length > 0) {
				const item = downloadQueue.shift();
				if (removedGameIds.has(item.cleanId)) {
					pendingGameIds.delete(item.cleanId);
					continue;
				}
				const remaining = downloadQueue.length;
				setLogText(`[同期・ダウンロード中] ${item.game_id} (v${item.version}) を取得中... (残り${remaining}件)`);
				try {
					await invoke("download_game", { gameId: item.game_id, version: item.version });
					setLogText(`[完了] ${item.game_id} のダウンロード完了`);
					await loadGames();
				} catch (e) {
					console.error("Auto download error:", e);
					setLogText(`[エラー] ${item.game_id} のダウンロード失敗: ${e}`);
				} finally {
					pendingGameIds.delete(item.cleanId);
				}
			}
			isDownloading = false;
			setLogText("ゲーム情報の同期・更新がすべて完了しました");
		}

		const updateListener = window.__TAURI__.event.listen("update_notice", async (event) => {
			const payload = event.payload;
			if (payload && payload.game_id) {
				const cleanId = payload.game_id.replace(".exe", "");
				removedGameIds.delete(cleanId);
				const existing = getAllGames().find(g => g.id === payload.game_id || g.game === payload.game_id || g.id === cleanId);
				if (!existing || !existing.isInstalled || existing.version !== payload.version || existing.hasUpdate) {
					if (!pendingGameIds.has(cleanId)) {
						pendingGameIds.add(cleanId);
						setGameUpdateFlag(cleanId, payload.version);
						renderGames(getAllGames());
						downloadQueue.push({ game_id: payload.game_id, version: payload.version, cleanId });
						processDownloadQueue();
					}
				} else {
					setLogText(`[確認] ${payload.game_id} は既に最新バージョン (v${payload.version}) です`);
				}
			}
		});
		const deleteListener = window.__TAURI__.event.listen("game_delete_notice", async (event) => {
			const payload = event.payload;
			if (!payload?.game_id) return;
			const cleanId = payload.game_id.replace(".exe", "");
			removedGameIds.add(cleanId);
			pendingGameIds.delete(cleanId);
			if (payload.error) {
				console.error("Server deletion error:", payload.error);
				setLogText(`[エラー] ${cleanId} を削除できませんでした: ${payload.error}`);
				return;
			}
			const selectedGame = getSelectedGame();
			const selectedId = (selectedGame?.id || selectedGame?.game || "").replace(/\.exe$/i, "");
			removeGameById(cleanId);
			renderGames(getAllGames());
			updateGameCount(getAllGames().length);
			if (selectedId.toLocaleLowerCase() === cleanId.toLocaleLowerCase()) {
				closeModal("detail-modal");
			}
			await loadGames();
			setLogText(payload.deleted
				? `[削除] ${cleanId} はServerの配布終了により削除されました`
				: `[確認] ${cleanId} はServerで配布されていません`);
		});

		// リスナーの登録後に常駐ストリームを開始し、起動直後の通知欠落を防ぐ。
		Promise.all([updateListener, deleteListener])
			.then(() => invoke("sync_updates"))
			.catch((e) => {
				console.warn("initial sync_updates error:", e);
				setLogText("サーバーへの接続に失敗しました");
			});
	}
}
