import { invoke } from "../core/tauri.js";
import { setCommunityTab } from "../ui/communityTabs.js";

let requestNumber = 0;
let refreshTimer = null;
const RefreshIntervalMilliseconds = 5000;

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

export async function loadLeaderboards(game, options = {}) {
	const reset = options.reset !== false;
	const current = ++requestNumber;
	const section = document.getElementById("leaderboard-section");
	const list = document.getElementById("leaderboard-list");
	const jumpButton = document.getElementById("btn-jump-ranking");
	const tabButton = document.getElementById("tab-ranking");
	if (!section || !list) {
		return;
	}
	if (reset) {
		section.hidden = true;
		if (jumpButton) jumpButton.hidden = true;
		if (tabButton) tabButton.hidden = true;
		list.replaceChildren();
	}
	if (!window.__TAURI__) {
		return;
	}
	try {
		const boards = await invoke("get_leaderboards", { gameId: gameId(game) });
		if (current !== requestNumber) {
			return;
		}
		if (!Array.isArray(boards) || boards.length === 0) {
			list.replaceChildren();
			if (jumpButton) jumpButton.hidden = true;
			if (tabButton) tabButton.hidden = true;
			setCommunityTab("comments");
			return;
		}

		const cards = boards.slice(0, 2).map(renderBoard);
		list.replaceChildren(...cards);
		if (jumpButton) jumpButton.hidden = false;
		if (tabButton) tabButton.hidden = false;
	} catch (error) {
		console.warn("get_leaderboards error:", error);
	}
}

export function startLeaderboardAutoRefresh(game) {
	stopLeaderboardAutoRefresh();
	refreshTimer = window.setInterval(() => {
		const modal = document.getElementById("detail-modal");
		if (!modal || modal.classList.contains("hidden")) {
			stopLeaderboardAutoRefresh();
			return;
		}
		if (document.visibilityState === "visible") {
			void loadLeaderboards(game, { reset: false });
		}
	}, RefreshIntervalMilliseconds);
}

export function stopLeaderboardAutoRefresh() {
	if (refreshTimer !== null) {
		window.clearInterval(refreshTimer);
		refreshTimer = null;
	}
}

document.addEventListener("modal:closed", (event) => {
	if (event.detail?.modalId === "detail-modal") {
		stopLeaderboardAutoRefresh();
	}
});
