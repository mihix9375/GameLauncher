import { invoke } from "../core/tauri.js";
import { setSelectedGame } from "../core/state.js";
import { openModal } from "../ui/modal.js";
import { setLogText } from "../ui/log.js";

export async function openDetailModal(game, meta) {
	setSelectedGame(game);
	const merged = Object.assign({}, meta || {}, game);

	document.getElementById("modal-title").textContent = merged.title || game.title;
	document.getElementById("modal-version").textContent = merged.version || game.version || "v1.0.0";
	document.getElementById("modal-author").textContent = merged.author || "ゲーム開発研究部";
	document.getElementById("modal-date").textContent = merged.latest_update || merged.latestUpdate || merged.lastUpdate || "2026/07/07";
	document.getElementById("modal-description").textContent = merged.description || "説明文はありません。";

	const bannerEl = document.getElementById("modal-banner");
	if (bannerEl) {
		if (game.image && game.image.length > 5) {
			bannerEl.style.backgroundImage = `url("${game.image}")`;
			bannerEl.style.backgroundSize = "cover";
			bannerEl.style.backgroundPosition = "center";
		} else {
			bannerEl.style.backgroundImage = "";
		}
	}

	const modalTags = document.getElementById("modal-tags");
	modalTags.innerHTML = "";
	const tagsList = (Array.isArray(merged.tags) && merged.tags.length > 0) ? merged.tags : ["ゲーム"];
	tagsList.forEach(tagText => {
		const tag = document.createElement("span");
		tag.className = "tag-pill";
		tag.textContent = tagText;
		modalTags.appendChild(tag);
	});

	openModal("detail-modal");
	setLogText(`${game.title} の詳細を開きました`);

	if (window.__TAURI__) {
		try {
			setLogText(`${game.title} の更新をチェック中...`);
			const res = await invoke("check_version", { version: game.version || "0.0.0", gameId: (game.id || game.game || "").replace(".exe", "") });
			if (res && res.is_update_available) {
				game._needsUpdate = true;
				game._latestVersion = res.latest_version;
				setLogText(`${game.title} に新しいバージョン (${res.latest_version}) があります`);
				const statusEl = document.querySelector(".launch-status");
				if (statusEl) {
					statusEl.textContent = `更新可能 (v${res.latest_version})`;
					statusEl.className = "launch-status update";
				}
				const launchBtn = document.getElementById("btn-launch-game");
				if (launchBtn) {
					if (!game.isInstalled) {
						launchBtn.innerHTML = `<span class="btn-icon">⬇</span><span class="btn-text">ダウンロード (Download)</span>`;
					} else {
						launchBtn.innerHTML = `<span class="btn-icon">🔄</span><span class="btn-text">更新する (Update)</span>`;
					}
				}
			} else {
				game._needsUpdate = false;
				game._latestVersion = res ? res.latest_version : game.version;
				setLogText(`${game.title} は最新版です`);
				const statusEl = document.querySelector(".launch-status");
				if (statusEl) {
					statusEl.textContent = "起動可能 (最新)";
					statusEl.className = "launch-status ready";
				}
				const launchBtn = document.getElementById("btn-launch-game");
				if (launchBtn) {
					launchBtn.innerHTML = `<span class="btn-icon">▶</span><span class="btn-text">起動する (Play)</span>`;
				}
			}
		} catch (e) {
			console.warn("check_version error:", e);
			setLogText(`${game.title} の詳細を開きました`);
		}
	}
}
