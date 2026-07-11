import { invoke } from "../core/tauri.js";
import { setAllGames, getAllGames } from "../core/state.js";
import { renderGames } from "./renderGames.js";
import { updateGameCount } from "../ui/counter.js";
import { setLogText } from "../ui/log.js";

export async function loadGames() {
	try {
		setLogText("ゲーム情報を取得中...");
		let allGames;
		
		if (window.__TAURI__) {
			allGames = await invoke("refresh");
		} else {
			allGames = [
				{ id: "test1", title: "Project Alpha: Overdrive", version: "v1.1.1", image: "1" },
				{ id: "test2", title: "Cyber Horizon 2026", version: "v1.2.3", image: "2" },
				{ id: "test3", title: "Dungeon Chronicle", version: "v0.9.5", image: "1" },
				{ id: "test4", title: "Skyward Wings", version: "v2.0.0", image: "2" },
				{ id: "test5", title: "Mech Arena: Zero", version: "v1.0.4", image: "1" },
				{ id: "test6", title: "Rhythm Master Pro", version: "v3.1.0", image: "2" }
			];
		}

		setAllGames(allGames);
		renderGames(allGames);
		updateGameCount(allGames.length);
		setLogText(`準備完了 - ${allGames.length}件のゲームを読み込みました`);
	} catch (error) {
		console.error("Failed to load games:", error);
		setLogText(`エラー: ゲーム情報の取得に失敗しました (${error})`);
		alert(`ゲーム情報の表示に失敗しました。\n詳細: ${error}`);
	}
}
