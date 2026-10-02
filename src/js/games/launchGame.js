import { invoke } from "../core/tauri.js";
import { setLogText } from "../ui/log.js";
import { formatError } from "../core/errors.js";
import { getAllGames, setGameDownloadProgress } from "../core/state.js";
import { renderGames } from "./renderGames.js";
import { updateDetailDownloadUi } from "../modals/detailModal.js";
import { refreshGameCard } from "./loadGames.js";
import { gameId } from "../core/gameIdentity.js";
import { refreshGameSession } from "./gameSession.js";

let launchRequestInProgress = false;

export async function launchGame(game) {
	if (launchRequestInProgress) {
		setLogText("別のゲームを起動処理中です");
		return;
	}
	launchRequestInProgress = true;
	let downloadAttempted = false;

	try {
		if (window.__TAURI__ && await invoke("is_game_running")) {
			const message = "別のゲームが既に起動しています。終了してから起動してください。";
			setLogText(message);
			alert(message);
			return;
		}

		setLogText(`${game.title} を起動中...`);
		const launchBtn = document.getElementById("btn-launch-game");

		if (launchBtn) {
			launchBtn.innerHTML = `<span class="btn-icon">⏳</span><span class="btn-text">起動処理中...</span>`;
			launchBtn.style.opacity = "0.7";
			launchBtn.style.pointerEvents = "none";
		}

		if (window.__TAURI__) {
			const targetId = gameId(game);
			let needsDownload = game.isInstalled === false ? true : game._needsUpdate;
			if (needsDownload === undefined) {
				try {
					setLogText(`${game.title} の更新情報を確認中...`);
					const res = await invoke("check_version", { version: game.version || "v1.0.0", gameId: targetId });
					needsDownload = res && res.is_update_available;
					if (res && res.latest_version) {
						game._latestVersion = res.latest_version;
					}
				} catch (error) {
					console.warn("Version check failed:", error);
					needsDownload = game.isInstalled === false;
				}
			}

			if (needsDownload) {
				downloadAttempted = true;
				setLogText(`${game.title} をダウンロード/更新中...`);
				setGameDownloadProgress(targetId, {
					game_id: targetId,
					stage: "queued",
					received_bytes: 0,
					total_bytes: 0,
				});
				renderGames(getAllGames());
				updateDetailDownloadUi(game);
				await invoke("download_game", { gameId: targetId, version: game._latestVersion || game.version });
				game._needsUpdate = false;
				game.hasUpdate = false;
				game.isInstalled = true;
				setGameDownloadProgress(targetId, null);
				if (game._latestVersion) {
					game.version = game._latestVersion;
				}
				const statusEl = document.querySelector(".launch-status");
				if (statusEl) {
					statusEl.textContent = "起動可能 (最新)";
					statusEl.className = "launch-status ready";
				}
				const refreshedGame = await refreshGameCard(targetId);
				updateDetailDownloadUi(refreshedGame || game);
				setLogText(`${game.title} のダウンロードが完了しました`);
				return;
			}

			setLogText(`${game.title} を起動中...`);
			await invoke("launch", { gameId: targetId });
			await refreshGameSession();
		} else {
			await new Promise(resolve => setTimeout(resolve, 1200));
			alert(`[テストモード] ゲーム「${game.title}」を起動しました！`);
		}

		setLogText(`${game.title} を実行中`);
		// 詳細画面がまだ同じゲームを表示している場合だけUIを戻す。
		// 閉じた画面や、後から開いた別ゲームのボタンを古いタイマーで変更しない。
		updateDetailDownloadUi(game);

	} catch (error) {
		console.error("Launch error:", error);
		const wasRemoteOnly = game.isInstalled === false;
		const detail = formatError(
			error,
			downloadAttempted || wasRemoteOnly
				? "サーバーからゲームを取得できませんでした。配布が終了している可能性があります。"
				: "ゲームを起動できませんでした。",
		);
		setGameDownloadProgress(game.id || game.game, { stage: "error", error: String(error) });
		renderGames(getAllGames());
		updateDetailDownloadUi(game);
		const operation = downloadAttempted || wasRemoteOnly ? "ダウンロード" : "起動";
		setLogText(`エラー: ${game.title} の${operation}に失敗しました (${detail})`);
		alert(`ゲームの${operation}時にエラーが発生しました。\n詳細: ${detail}`);
	} finally {
		launchRequestInProgress = false;
	}
}
