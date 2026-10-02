import { invoke } from "../core/tauri.js";
import { filterAndRenderGames } from "../games/filterGames.js";
import { loadGames, refreshGameCard, sortGames } from "../games/loadGames.js";
import { openModal, closeModal } from "../ui/modal.js";
import { setLogText } from "../ui/log.js";
import { findGameById, getSelectedGame, setSelectedGame, setGameDownloadProgress, setGameUpdateFlag, getAllGames, removeGameById, applyRemoteGameMetadata } from "../core/state.js";
import { launchGame } from "../games/launchGame.js";
import { renderGames, updateGameCardDownloadUi } from "../games/renderGames.js";
import { submitComment } from "../comments/comments.js";
import { setCommunityTab } from "../ui/communityTabs.js";
import { updateGameCount } from "../ui/counter.js";
import { formatError } from "../core/errors.js";
import { updateDetailDownloadUi, updateDetailMetadataUi } from "../modals/detailModal.js";
import { gameId } from "../core/gameIdentity.js";
import { startGameSessionMonitoring } from "../games/gameSession.js";

const pendingMetadata = new Map();

function refreshRemoteMetadata(gameId) {
	if (pendingMetadata.has(gameId)) return pendingMetadata.get(gameId);
	const request = invoke("get_game_metadata", { gameId }).then(metadata => {
		const game = applyRemoteGameMetadata(gameId, metadata);
		if (!game) return;
		sortGames(getAllGames());
		filterAndRenderGames(document.getElementById("search-input")?.value || "");
		updateDetailMetadataUi(game);
	}).catch(error => {
		const game = findGameById(gameId);
		if (game?.isInstalled === false && game.title === "ゲーム情報を取得中…") {
			game.title = "ゲーム情報を取得できませんでした";
			filterAndRenderGames(document.getElementById("search-input")?.value || "");
			updateDetailMetadataUi(game);
		}
		console.warn("ゲーム情報の取得に失敗しました:", error);
	}).finally(() => pendingMetadata.delete(gameId));
	pendingMetadata.set(gameId, request);
	return request;
}

export function setupEventListeners() {
	startGameSessionMonitoring();
	let requestDownloadAll = () => {};
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
			setLogText("ゲーム情報を更新中...");
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

	const btnDownloadAll = document.getElementById("btn-download-all");
	if (btnDownloadAll) {
		btnDownloadAll.addEventListener("click", async () => {
			setLogText("全ゲームを確認・ダウンロード中...");
			await loadGames();
			if (window.__TAURI__) {
				try {
					requestDownloadAll();
					await invoke("sync_updates");
				} catch (e) {
					console.warn("download all sync error:", e);
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
	document.getElementById("detail-modal")?.addEventListener("modal:closed", () => setSelectedGame(null));
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
		const maxConcurrentDownloads = 4;
		const downloadQueue = [];
		let activeDownloads = 0;
		const pendingGameIds = new Set();
		const removedGameIds = new Set();
		let downloadAllRequested = false;
		let downloadAllTimer = null;

		requestDownloadAll = () => {
			downloadAllRequested = true;
			window.clearTimeout(downloadAllTimer);
			downloadAllTimer = window.setTimeout(() => { downloadAllRequested = false; }, 10000);
		};

		function noteDownloadAllNotice() {
			if (!downloadAllRequested) return;
			window.clearTimeout(downloadAllTimer);
			downloadAllTimer = window.setTimeout(() => { downloadAllRequested = false; }, 750);
		}

		function enqueueDownload(gameId, version, cleanId) {
			if (pendingGameIds.has(cleanId)) return;
			pendingGameIds.add(cleanId);
			setGameDownloadProgress(cleanId, {
				game_id: cleanId,
				stage: "queued",
				received_bytes: 0,
				total_bytes: 0,
			});
			renderGames(getAllGames());
			downloadQueue.push({ game_id: gameId, version, cleanId });
			pumpDownloadQueue();
		}

		function updateDownloadSummary() {
			const total = activeDownloads + downloadQueue.length;
			if (total > 0) {
				setLogText(`[同時ダウンロード中] ${activeDownloads}件を処理中 / 待機${downloadQueue.length}件`);
			} else {
				setLogText("ゲーム情報の同期・更新がすべて完了しました");
			}
		}

		async function downloadQueuedGame(item) {
			try {
				await invoke("download_game", { gameId: item.game_id, version: item.version });
				const completedGame = findGameById(item.cleanId);
				if (completedGame) {
					completedGame.isInstalled = true;
					completedGame.version = item.version;
					completedGame.hasUpdate = false;
					completedGame._needsUpdate = false;
					setGameDownloadProgress(item.cleanId, null);
				}
				const refreshedGame = await refreshGameCard(item.cleanId);
				if (refreshedGame) updateDetailDownloadUi(refreshedGame);
			} catch (e) {
				console.error("Auto download error:", e);
				setGameDownloadProgress(item.cleanId, { stage: "error", error: String(e) });
				renderGames(getAllGames());
				const failedGame = findGameById(item.cleanId);
				if (failedGame) updateDetailDownloadUi(failedGame);
				setLogText(`[エラー] ${item.game_id} のダウンロード失敗: ${formatError(e, "サーバーで配布されていない可能性があります")}`);
			} finally {
				pendingGameIds.delete(item.cleanId);
				activeDownloads -= 1;
				pumpDownloadQueue();
				updateDownloadSummary();
			}
		}

		function pumpDownloadQueue() {
			while (activeDownloads < maxConcurrentDownloads && downloadQueue.length > 0) {
				const item = downloadQueue.shift();
				if (removedGameIds.has(item.cleanId)) {
					pendingGameIds.delete(item.cleanId);
					continue;
				}
				activeDownloads += 1;
				void downloadQueuedGame(item);
			}
			updateDownloadSummary();
		}

		const updateListener = window.__TAURI__.event.listen("update_notice", async (event) => {
			const payload = event.payload;
			if (payload && payload.game_id) {
				const cleanId = gameId(payload.game_id);
				noteDownloadAllNotice();
				removedGameIds.delete(cleanId);
				const existing = findGameById(cleanId);
				const isInstalled = Boolean(existing && existing.isInstalled !== false);
				if (!existing || !existing.isInstalled || existing.version !== payload.version || existing.hasUpdate) {
					setGameUpdateFlag(cleanId, payload.version);
					renderGames(getAllGames());
					if (!isInstalled) void refreshRemoteMetadata(cleanId);
					if (isInstalled || downloadAllRequested) {
						enqueueDownload(payload.game_id, payload.version, cleanId);
					} else {
						setLogText(`[新着] ${payload.game_id} をダウンロードできます`);
					}
				} else {
					setLogText(`[確認] ${payload.game_id} は既に最新バージョン (v${payload.version}) です`);
				}
			}
		});
		const progressListener = window.__TAURI__.event.listen("download_progress", (event) => {
			const progress = event.payload;
			if (!progress?.game_id) return;
			if (setGameDownloadProgress(progress.game_id, progress)) {
				const game = findGameById(progress.game_id);
				if (game) {
					if (!updateGameCardDownloadUi(game)) renderGames(getAllGames());
					updateDetailDownloadUi(game);
				}
			}
		});
		const deleteListener = window.__TAURI__.event.listen("game_delete_notice", async (event) => {
			const payload = event.payload;
			if (!payload?.game_id) return;
			const cleanId = gameId(payload.game_id);
			removedGameIds.add(cleanId);
			pendingGameIds.delete(cleanId);
			if (payload.error) {
				console.error("Server deletion error:", payload.error);
				setLogText(`[エラー] ${cleanId} を削除できませんでした: ${payload.error}`);
				return;
			}
			const selectedGame = getSelectedGame();
			const selectedId = gameId(selectedGame);
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
		Promise.all([updateListener, deleteListener, progressListener])
			.then(() => invoke("sync_updates"))
			.catch((e) => {
				console.warn("initial sync_updates error:", e);
				setLogText("サーバーへの接続に失敗しました");
			});
	}
}
