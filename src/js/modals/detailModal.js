import { invoke } from "../core/tauri.js";
import { setSelectedGame } from "../core/state.js";
import { openModal } from "../ui/modal.js";
import { setLogText } from "../ui/log.js";
import { loadComments } from "../comments/comments.js";
import { loadLeaderboards } from "../leaderboards/leaderboards.js";
import { setCommunityTab } from "../ui/communityTabs.js";

function updateBannerImageMode(banner, image, backdrop) {
	if (!image.naturalWidth || !image.naturalHeight) return;
	const ratio = image.naturalWidth / image.naturalHeight;
	const isWidescreen = Math.abs(ratio - (16 / 9)) < 0.01;
	banner.classList.toggle("is-widescreen", isWidescreen);
	if (backdrop) backdrop.hidden = isWidescreen;
	updateBannerPalette(banner, image);
}

function updateBannerPalette(banner, image) {
	try {
		const canvas = document.createElement("canvas");
		canvas.width = 48;
		canvas.height = 6;
		const context = canvas.getContext("2d", { willReadFrequently: true });
		if (!context) return;

		const sampleHeight = Math.max(1, Math.round(image.naturalHeight * 0.08));
		context.drawImage(
			image,
			0,
			image.naturalHeight - sampleHeight,
			image.naturalWidth,
			sampleHeight,
			0,
			0,
			canvas.width,
			canvas.height,
		);

		const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
		let red = 0;
		let green = 0;
		let blue = 0;
		let samples = 0;
		for (let index = 0; index < pixels.length; index += 4) {
			if (pixels[index + 3] < 32) continue;
			red += pixels[index];
			green += pixels[index + 1];
			blue += pixels[index + 2];
			samples += 1;
		}
		if (!samples) return;

		red = Math.round(red / samples);
		green = Math.round(green / samples);
		blue = Math.round(blue / samples);
		const luminance = red * 0.2126 + green * 0.7152 + blue * 0.0722;
		const edgeScale = Math.min(1, 110 / Math.max(luminance, 1));
		const edge = [red, green, blue].map(channel => Math.round(channel * edgeScale));
		const panel = edge.map(channel => Math.round(channel * 0.32));
		banner.style.setProperty("--banner-edge-color", `rgb(${edge.join(", ")})`);
		banner.style.setProperty("--banner-panel-color", `rgb(${panel.join(", ")})`);
	} catch {
		// Canvas access can be blocked for remote images; CSS defaults remain usable.
	}
}

export async function openDetailModal(game, meta) {
	setSelectedGame(game);
	const merged = Object.assign({}, meta || {}, game);
	const sidebar = document.querySelector("#detail-modal .detail-sidebar");
	const summary = document.querySelector("#detail-modal .detail-summary");
	if (sidebar) sidebar.scrollTop = 0;
	if (summary) summary.scrollTop = 0;
	setCommunityTab("comments");

	document.getElementById("modal-title").textContent = merged.title || game.title;
	document.getElementById("modal-version").textContent = merged.version || game.version || "v1.0.0";
	document.getElementById("modal-author").textContent = merged.author || "ゲーム開発研究部";
	document.getElementById("modal-date").textContent = merged.latest_update || merged.latestUpdate || merged.lastUpdate || "2026/07/07";
	document.getElementById("modal-description").textContent = merged.description || "説明文はありません。";

	const bannerEl = document.getElementById("modal-banner");
	const bannerImage = document.getElementById("modal-banner-image");
	const bannerBackdrop = document.getElementById("modal-banner-backdrop");
	if (bannerEl) {
		bannerEl.style.removeProperty("--banner-edge-color");
		bannerEl.style.removeProperty("--banner-panel-color");
		if (game.image && game.image.length > 5) {
			bannerEl.style.backgroundImage = "";
			bannerEl.classList.remove("is-widescreen");
			if (bannerBackdrop) {
				bannerBackdrop.src = game.image;
				bannerBackdrop.hidden = false;
			}
			if (bannerImage) {
				bannerImage.onload = () => updateBannerImageMode(bannerEl, bannerImage, bannerBackdrop);
				bannerImage.onerror = () => {
					bannerEl.classList.remove("is-widescreen");
					bannerImage.hidden = true;
					if (bannerBackdrop) bannerBackdrop.hidden = true;
				};
				bannerImage.src = game.image;
				bannerImage.alt = `${merged.title || game.title || "ゲーム"}のサムネイル`;
				bannerImage.hidden = false;
				if (bannerImage.complete) updateBannerImageMode(bannerEl, bannerImage, bannerBackdrop);
			}
		} else {
			bannerEl.style.backgroundImage = "";
			bannerEl.classList.remove("is-widescreen");
			if (bannerImage) {
				bannerImage.onload = null;
				bannerImage.onerror = null;
				bannerImage.removeAttribute("src");
				bannerImage.hidden = true;
			}
			if (bannerBackdrop) {
				bannerBackdrop.removeAttribute("src");
				bannerBackdrop.hidden = true;
			}
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
  loadLeaderboards(game);
  loadComments(game);
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
