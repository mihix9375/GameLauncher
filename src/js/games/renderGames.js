import { getMockMetadata } from "../core/state.js";
import { openDetailModal } from "../modals/detailModal.js";

export function renderGames(games) {
	const container = document.getElementById("game-list");
	if (!container) return;
	container.innerHTML = "";

	if (games.length === 0) {
		container.innerHTML = `<div style="grid-column: 1/-1; text-align: center; padding: 60px; color: var(--text-muted);">該当するゲームが見つかりませんでした。</div>`;
		return;
	}

	const MOCK_METADATA = getMockMetadata();

	games.forEach(game => {
		const mockMeta = MOCK_METADATA[game.id] || MOCK_METADATA.default;
		const meta = Object.assign({}, mockMeta, game);

		const card = document.createElement("div");
		card.className = "game-card";
		card.dataset.gameId = String(game.id || game.game || "").replace(/\.exe$/i, "").toLocaleLowerCase();

		const bannerWrap = document.createElement("div");
		bannerWrap.className = "card-banner-wrap";

		if (game.image && game.image.length > 5) {
			const img = document.createElement("img");
			img.className = "card-banner-img";
			img.src = game.image;
			img.alt = game.title;
			bannerWrap.appendChild(img);
		} else {
			const placeholder = document.createElement("div");
			placeholder.className = "card-banner-placeholder";
			placeholder.textContent = game.image === "1" ? "⚔️" : game.image === "2" ? "🚀" : "🎮";
			bannerWrap.appendChild(placeholder);
		}

		const versionBadge = document.createElement("span");
		versionBadge.className = "card-version-badge";
		versionBadge.textContent = game.version || meta.version || "v1.0.0";
		bannerWrap.appendChild(versionBadge);

		const content = document.createElement("div");
		content.className = "card-content";

		const title = document.createElement("h3");
		title.className = "card-title";
		title.textContent = game.title;

		const tagsContainer = document.createElement("div");
		tagsContainer.className = "card-tags";
		const tagsList = (Array.isArray(game.tags) && game.tags.length > 0) ? game.tags : (meta.tags || ["ゲーム"]);
		tagsList.forEach(tagText => {
			const tag = document.createElement("span");
			tag.className = "tag-pill";
			tag.textContent = tagText;
			tagsContainer.appendChild(tag);
		});

		content.appendChild(title);
		content.appendChild(tagsContainer);

		const actionWrap = document.createElement("div");
		actionWrap.className = "card-actions-wrap";
		const actionBtn = document.createElement("button");
		actionBtn.className = "card-action-btn";
		const progress = game._downloadProgress;
		const downloading = Boolean(game._isDownloading && progress);
		const hasMeasuredProgress = progress?.total_bytes > 0;
		const percentage = hasMeasuredProgress
			? Math.min(100, Math.round((progress.received_bytes / progress.total_bytes) * 100))
			: 0;
		const progressLabel = progress?.stage === "queued" ? "ダウンロード待機中"
			: progress?.stage === "preparing" ? "準備中"
			: progress?.stage === "extracting" ? "展開中"
			: progress?.stage === "installing" ? "適用中"
			: `${percentage}% ダウンロード中`;

		if (downloading) {
			card.classList.add("is-downloading");
			actionBtn.className += " downloading-btn";
			actionBtn.disabled = true;
			actionBtn.innerHTML = `<span class="btn-icon">⏳</span><span class="btn-text">${progressLabel}</span>`;
		} else if (game.isInstalled === false) {
			actionBtn.className += " download-btn";
			actionBtn.innerHTML = `<span class="btn-icon">⬇</span><span class="btn-text">ダウンロードする (Download)</span>`;
		} else if (game.hasUpdate || game._needsUpdate) {
			actionBtn.className += " update-btn";
			actionBtn.innerHTML = `<span class="btn-icon">🔄</span><span class="btn-text">更新する (Update)</span>`;
		} else {
			actionBtn.className += " play-btn";
			actionBtn.innerHTML = `<span class="btn-icon">▶</span><span class="btn-text">起動する (Play)</span>`;
		}

		actionBtn.addEventListener("click", (e) => {
			e.stopPropagation();
			openDetailModal(game, meta);
		});
		actionWrap.appendChild(actionBtn);
		content.appendChild(actionWrap);
		if (downloading) {
			const progressWrap = document.createElement("div");
			progressWrap.className = "card-download-progress";
			progressWrap.setAttribute("role", "progressbar");
			progressWrap.setAttribute("aria-valuemin", "0");
			progressWrap.setAttribute("aria-valuemax", "100");
			if (hasMeasuredProgress) progressWrap.setAttribute("aria-valuenow", String(percentage));
			else progressWrap.classList.add("indeterminate");
			const progressBar = document.createElement("span");
			progressBar.style.width = `${percentage}%`;
			progressWrap.appendChild(progressBar);
			content.appendChild(progressWrap);
		}

		card.appendChild(bannerWrap);
		card.appendChild(content);

		card.addEventListener("click", () => {
			openDetailModal(game, meta);
		});

		container.appendChild(card);
	});
}

export function updateGameCardDownloadUi(game) {
	if (!game?._isDownloading || !game._downloadProgress) return false;
	const gameId = String(game.id || game.game || "").replace(/\.exe$/i, "").toLocaleLowerCase();
	const card = [...document.querySelectorAll(".game-card")].find(element => element.dataset.gameId === gameId);
	if (!card) return false;

	const progress = game._downloadProgress;
	const measured = progress.total_bytes > 0;
	const percentage = measured
		? Math.min(100, Math.round((progress.received_bytes / progress.total_bytes) * 100))
		: 0;
	const label = progress.stage === "queued" ? "ダウンロード待機中"
		: progress.stage === "preparing" ? "準備中"
		: progress.stage === "extracting" ? "展開中"
		: progress.stage === "installing" ? "適用中"
		: `${percentage}% ダウンロード中`;
	card.classList.add("is-downloading");
	const actionButton = card.querySelector(".card-action-btn");
	if (actionButton) {
		actionButton.className = "card-action-btn downloading-btn";
		actionButton.disabled = true;
		actionButton.innerHTML = `<span class="btn-icon">⏳</span><span class="btn-text">${label}</span>`;
	}
	let progressWrap = card.querySelector(".card-download-progress");
	if (!progressWrap) {
		progressWrap = document.createElement("div");
		progressWrap.className = "card-download-progress";
		progressWrap.setAttribute("role", "progressbar");
		progressWrap.setAttribute("aria-valuemin", "0");
		progressWrap.setAttribute("aria-valuemax", "100");
		progressWrap.appendChild(document.createElement("span"));
		card.querySelector(".card-content")?.appendChild(progressWrap);
	}
	progressWrap.classList.toggle("indeterminate", !measured);
	if (measured) progressWrap.setAttribute("aria-valuenow", String(percentage));
	else progressWrap.removeAttribute("aria-valuenow");
	const bar = progressWrap.querySelector("span");
	if (bar) bar.style.width = `${percentage}%`;
	return true;
}
