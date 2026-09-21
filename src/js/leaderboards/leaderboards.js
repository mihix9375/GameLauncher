import { invoke } from "../core/tauri.js";

let requestNumber = 0;

function gameId(game) {
	return (game.id || game.game || "").replace(/\.exe$/i, "");
}

function renderBoard(board) {
	const card = document.createElement("article");
	card.className = "leaderboard-card";
	const heading = document.createElement("div");
	heading.className = "leaderboard-card-heading";

	const title = document.createElement("h5");
	title.textContent = board.name;
	const order = document.createElement("span");
	order.textContent = board.order === "low_score" ? "LOW → HIGH" : "HIGH → LOW";
	heading.append(title, order);
	card.append(heading);

	if (!board.entries?.length) {
		const empty = document.createElement("p");
		empty.className = "leaderboard-empty";
		empty.textContent = "まだ記録がありません";
		card.append(empty);
		return card;
	}

	const list = document.createElement("ol");
	list.className = "leaderboard-entries";
	for (const entry of board.entries) {
		const row = document.createElement("li");
		const rank = document.createElement("strong");
		rank.textContent = entry.rank;
		const name = document.createElement("span");
		name.textContent = entry.player_name;
		const score = document.createElement("b");
		score.textContent = Number(entry.score).toLocaleString("ja-JP");
		row.append(rank, name, score);
		list.append(row);
	}
	card.append(list);
	return card;
}

export async function loadLeaderboards(game) {
	const current = ++requestNumber;
	const section = document.getElementById("leaderboard-section");
	const list = document.getElementById("leaderboard-list");
	if (!section || !list) {
		return;
	}
	section.hidden = true;
	list.replaceChildren();
	if (!window.__TAURI__) {
		return;
	}
	try {
		const boards = await invoke("get_leaderboards", { gameId: gameId(game) });
		if (current !== requestNumber || !Array.isArray(boards) || boards.length === 0) {
			return;
		}
		for (const board of boards.slice(0, 2)) {
			list.append(renderBoard(board));
		}
		section.hidden = false;
	} catch (error) {
		console.warn("get_leaderboards error:", error);
	}
}
