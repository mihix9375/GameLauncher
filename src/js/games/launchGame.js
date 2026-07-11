import { invoke } from "../core/tauri.js";
import { setLogText } from "../ui/log.js";

export async function launchGame(game) {
	try {
		setLogText(`${game.title} を起動中...`);
		const launchBtn = document.getElementById("btn-launch-game");
		const originalText = launchBtn.innerHTML;
		
		if (launchBtn) {
			launchBtn.innerHTML = `<span class="btn-icon">⏳</span><span class="btn-text">起動処理中...</span>`;
			launchBtn.style.opacity = "0.7";
			launchBtn.style.pointerEvents = "none";
		}

		if (window.__TAURI__) {
			let needsDownload = game._needsUpdate;
			if (needsDownload === undefined) {
				try {
					setLogText(`${game.title} の更新情報を確認中...`);
					const res = await invoke("check_version", { version: game.version || "v1.0.0", id: game.id });
					needsDownload = res && res.is_update_available;
					if (res && res.latest_version) {
						game._latestVersion = res.latest_version;
					}
				} catch (e) {
					needsDownload = true;
				}
			}

			if (needsDownload) {
				setLogText(`${game.title} をダウンロード/更新中...`);
				await invoke("download_game", { gameId: game.id, version: game._latestVersion || game.version });
				game._needsUpdate = false;
				if (game._latestVersion) {
					game.version = game._latestVersion;
				}
				const statusEl = document.querySelector(".launch-status");
				if (statusEl) {
					statusEl.textContent = "起動可能 (最新)";
					statusEl.className = "launch-status ready";
				}
			}

			setLogText(`${game.title} を起動中...`);
			await invoke("launch", { gameId: game.id });
		} else {
			await new Promise(resolve => setTimeout(resolve, 1200));
			alert(`[テストモード] ゲーム「${game.title}」を起動しました！`);
		}

		setLogText(`${game.title} を実行中`);
		
		setTimeout(() => {
			if (launchBtn) {
				launchBtn.innerHTML = originalText;
				launchBtn.style.opacity = "1";
				launchBtn.style.pointerEvents = "auto";
			}
		}, 2000);

	} catch (error) {
		console.error("Launch error:", error);
		setLogText(`エラー: ${game.title} の起動に失敗しました`);
		alert(`ゲームの起動時にエラーが発生しました。\n詳細: ${error}`);
		
		const launchBtn = document.getElementById("btn-launch-game");
		if (launchBtn) {
			launchBtn.innerHTML = `<span class="btn-icon">▶</span><span class="btn-text">起動する (Play)</span>`;
			launchBtn.style.opacity = "1";
			launchBtn.style.pointerEvents = "auto";
		}
	}
}
