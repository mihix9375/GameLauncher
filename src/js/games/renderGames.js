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
		const meta = MOCK_METADATA[game.id] || MOCK_METADATA.default;
		
		const card = document.createElement("div");
		card.className = "game-card";
		
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
		versionBadge.textContent = game.version || "v1.0.0";
		bannerWrap.appendChild(versionBadge);

		const content = document.createElement("div");
		content.className = "card-content";

		const title = document.createElement("h3");
		title.className = "card-title";
		title.textContent = game.title;

		const tagsContainer = document.createElement("div");
		tagsContainer.className = "card-tags";
		meta.tags.forEach(tagText => {
			const tag = document.createElement("span");
			tag.className = "tag-pill";
			tag.textContent = tagText;
			tagsContainer.appendChild(tag);
		});

		content.appendChild(title);
		content.appendChild(tagsContainer);

		card.appendChild(bannerWrap);
		card.appendChild(content);

		card.addEventListener("click", () => {
			openDetailModal(game, meta);
		});

		container.appendChild(card);
	});
}
