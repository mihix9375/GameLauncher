import { invoke } from "../core/tauri.js";
import { getSelectedGame, setSelectedGame, isGameRunning } from "../core/state.js";
import { openModal } from "../ui/modal.js";
import { setLogText } from "../ui/log.js";
import { loadComments } from "../comments/comments.js";
import { loadLeaderboards, startLeaderboardAutoRefresh } from "../leaderboards/leaderboards.js";
import { setCommunityTab } from "../ui/communityTabs.js";
import { setupScrollHint } from "../ui/scrollHint.js";
import { renderMarkdown } from "../ui/markdown.js";
import { gameId, canonicalGameId } from "../core/gameIdentity.js";
import { downloadProgress, updateProgressBar } from "../ui/downloadProgress.js";

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

export function updateDetailDownloadUi(game) {
	if (!game || canonicalGameId(getSelectedGame()) !== canonicalGameId(game)) return;
	const button = document.getElementById("btn-launch-game");
	const status = document.querySelector("#detail-modal .launch-status");
	const progressWrap = document.getElementById("detail-download-progress");
	const progressBar = document.getElementById("detail-download-progress-bar");
	const progressLabel = document.getElementById("detail-download-progress-label");
	const progress = downloadProgress(game);

	if (isGameRunning(game)) {
		if (progressWrap) progressWrap.hidden = true;
		if (button) {
			button.disabled = true;
			button.innerHTML = `<span class="btn-icon">▶</span><span class="btn-text">起動中</span>`;
			button.style.opacity = "0.7";
			button.style.pointerEvents = "none";
		}
		if (status) {
			status.textContent = "ゲームを実行中";
			status.className = "launch-status ready";
		}
		return;
	}
	if (progress.downloading) {
		if (button) {
			button.disabled = true;
			button.innerHTML = `<span class="btn-icon">⏳</span><span class="btn-text">${progress.label}</span>`;
			button.style.opacity = "0.7";
			button.style.pointerEvents = "none";
		}
		if (status) {
			status.textContent = "ダウンロード中";
			status.className = "launch-status update";
		}
		if (progressWrap) {
			progressWrap.hidden = false;
		}
		updateProgressBar(progressWrap, progressBar, progress);
		if (progressLabel) progressLabel.textContent = progress.label;
		return;
	}

	if (progressWrap) progressWrap.hidden = true;
	if (button) {
		button.disabled = false;
		button.style.opacity = "1";
		button.style.pointerEvents = "auto";
	}
	if (game.isInstalled === false) {
		if (button) button.innerHTML = `<span class="btn-icon">⬇</span><span class="btn-text">ダウンロードする (Download)</span>`;
		if (status) {
			status.textContent = game._downloadError ? "ダウンロード失敗" : "未ダウンロード";
			status.className = "launch-status update";
		}
	} else if (game.hasUpdate || game._needsUpdate) {
		if (button) button.innerHTML = `<span class="btn-icon">🔄</span><span class="btn-text">更新する (Update)</span>`;
		if (status) {
			status.textContent = game._downloadError ? "更新失敗" : `更新可能${game._latestVersion ? ` (v${game._latestVersion})` : ""}`;
			status.className = "launch-status update";
		}
	} else {
		if (button) button.innerHTML = `<span class="btn-icon">▶</span><span class="btn-text">起動する (Play)</span>`;
		if (status) {
			status.textContent = "起動可能 (最新)";
			status.className = "launch-status ready";
		}
	}
}

export function updateDetailMetadataUi(game) {
	if (!game || canonicalGameId(getSelectedGame()) !== canonicalGameId(game)) return;
	const title = game.title || "ゲームタイトル";
	const titleElement = document.getElementById("modal-title");
	titleElement.textContent = title;
	titleElement.classList.toggle("long-title", title.length > 24);
	titleElement.classList.toggle("very-long-title", title.length > 44);
	document.getElementById("modal-version").textContent = game.version || "v1.0.0";
	document.getElementById("modal-author").textContent = game.author || "ゲーム開発研究部";
	document.getElementById("modal-date").textContent = game.latest_update || game.latestUpdate || game.lastUpdate || "";
	renderMarkdown(document.getElementById("modal-description"), game.description || "説明文はありません。");
	const tagContainer = document.getElementById("modal-tags");
	if (tagContainer && Array.isArray(game.tags)) {
		tagContainer.replaceChildren(...game.tags.map(text => {
			const tag = document.createElement("span");
			tag.className = "tag-pill";
			tag.textContent = text;
			return tag;
		}));
	}
}

export async function openDetailModal(game, meta) {
	setSelectedGame(game);
	const merged = Object.assign({}, meta || {}, game);
	const sidebar = document.querySelector("#detail-modal .detail-sidebar");
	const detailMain = document.querySelector("#detail-modal .detail-main");
	if (sidebar) sidebar.scrollTop = 0;
	if (detailMain) detailMain.scrollTop = 0;
	setupScrollHint(detailMain, document.getElementById("detail-scroll-hint"), {
		observedElements: [...document.querySelectorAll(".modal-header-banner, .detail-summary")],
	});
	setCommunityTab("comments");

	updateDetailMetadataUi(merged);
	updateDetailDownloadUi(game);

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

  openModal("detail-modal");
  loadLeaderboards(game);
  startLeaderboardAutoRefresh(game);
  loadComments(game);
  setLogText(`${game.title} の詳細を開きました`);

	if (window.__TAURI__) {
		try {
			setLogText(`${game.title} の更新をチェック中...`);
			const res = await invoke("check_version", { version: game.version || "0.0.0", gameId: gameId(game) });
			const needsInitialDownload = game.isInstalled === false;
			if (needsInitialDownload || (res && res.is_update_available)) {
				game._needsUpdate = true;
				game._latestVersion = res?.latest_version || game.version;
				setLogText(needsInitialDownload
					? `${game.title} をダウンロードできます`
					: `${game.title} に新しいバージョン (${res.latest_version}) があります`);
				updateDetailDownloadUi(game);
			} else {
				game._needsUpdate = false;
				game._latestVersion = res ? res.latest_version : game.version;
				setLogText(`${game.title} は最新版です`);
				updateDetailDownloadUi(game);
			}
		} catch (e) {
			console.warn("check_version error:", e);
			setLogText(`${game.title} の詳細を開きました`);
		}
	}
}
