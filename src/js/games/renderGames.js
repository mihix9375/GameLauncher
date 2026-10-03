import { getMockMetadata, isGameRunning } from "../core/state.js";
import { openDetailModal } from "../modals/detailModal.js";
import { canonicalGameId } from "../core/gameIdentity.js";
import { downloadProgress, updateProgressBar } from "../ui/downloadProgress.js";

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
		card.dataset.gameId = canonicalGameId(game);

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
		if (isGameRunning(game)) {
			actionBtn.className += " play-btn";
			actionBtn.innerHTML = `<span class="btn-icon">▶</span><span class="btn-text">起動中</span>`;
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

		card.appendChild(bannerWrap);
		card.appendChild(content);

		card.addEventListener("click", () => {
			openDetailModal(game, meta);
		});

		container.appendChild(card);
		updateGameCardDownloadUi(game, card);
	});
}

export function updateGameCardDownloadUi(game, existingCard = null) {
	const progress = downloadProgress(game);
	if (!progress.downloading) return false;
	const gameId = canonicalGameId(game);
	const card = existingCard || [...document.querySelectorAll(".game-card")].find(element => element.dataset.gameId === gameId);
	if (!card) return false;
	card.classList.add("is-downloading");
	const actionButton = card.querySelector(".card-action-btn");
	if (actionButton) {
		actionButton.className = "card-action-btn downloading-btn";
		actionButton.disabled = true;
		actionButton.innerHTML = `<span class="btn-icon">⏳</span><span class="btn-text">${progress.label}</span>`;
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
	updateProgressBar(progressWrap, progressWrap.querySelector("span"), progress);
	return true;
}
