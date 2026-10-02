import { invoke } from "../core/tauri.js";
import { getSelectedGame, setRunningGameId } from "../core/state.js";
import { canonicalGameId } from "../core/gameIdentity.js";
import { filterAndRenderGames } from "./filterGames.js";
import { updateDetailDownloadUi } from "../modals/detailModal.js";

let lastId = "";
let pending = false;
let timer = null;

export async function refreshGameSession() {
	if (!window.__TAURI__ || pending) return;
	pending = true;
	try {
		// プロセスの生存を確認する。通信失敗だけで終了したとは判定しない。
		const id = canonicalGameId(await invoke("get_running_game_id"));
		setRunningGameId(id);
		if (id !== lastId) {
			lastId = id;
			filterAndRenderGames(document.getElementById("search-input")?.value || "");
			updateDetailDownloadUi(getSelectedGame());
		}
	} catch (error) {
		console.warn("ゲームの実行状態を確認できませんでした:", error);
	} finally {
		pending = false;
	}
}

export function startGameSessionMonitoring() {
	if (!window.__TAURI__ || timer !== null) return;
	void refreshGameSession();
	timer = window.setInterval(refreshGameSession, 500);
	window.addEventListener("focus", refreshGameSession);
}
