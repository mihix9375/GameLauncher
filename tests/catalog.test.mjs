import { test } from "node:test";
import assert from "node:assert/strict";
import { JSDOM } from "jsdom";
import { setAllGames, setSelectedGame, setGameUpdateFlag, findGameById, applyRemoteGameMetadata, removeGameById, setRunningGameId } from "../src/js/core/state.js";

const dom = new JSDOM(`<!doctype html><div id="game-list"></div><div id="modal-title"></div>
  <div id="modal-version"></div><div id="modal-author"></div><div id="modal-date"></div>
  <div id="modal-description"></div><div id="modal-tags"></div>
  <div id="detail-modal"><span class="launch-status"></span></div>
  <button id="btn-launch-game"></button><div id="detail-download-progress"></div>
  <span id="detail-download-progress-bar"></span><span id="detail-download-progress-label"></span>`);
globalThis.window = dom.window;
globalThis.document = dom.window.document;
const { renderGames, updateGameCardDownloadUi } = await import("../src/js/games/renderGames.js");
const { updateDetailMetadataUi, updateDetailDownloadUi } = await import("../src/js/modals/detailModal.js");

const metadata = { id: "internal-id", title: "実際のゲームタイトル", game: "Game.exe", version: "1.0.0", author: "作者", description: "# 遊び方\n\n- 移動", tags: ["Unity"], latestUpdate: "2026/10/02" };

test("running game stays marked after metadata replacement and restores buttons on exit", () => {
	const game = { ...metadata, isInstalled: true };
	setRunningGameId(game.id);
	setSelectedGame(game);
	renderGames([game]);
	updateDetailDownloadUi(game);
	assert.equal(document.querySelector(".card-action-btn .btn-text").textContent, "起動中");
	assert.equal(document.querySelector("#btn-launch-game .btn-text").textContent, "起動中");
	assert.equal(document.getElementById("btn-launch-game").disabled, true);
	const refreshed = { ...game };
	setSelectedGame(refreshed);
	renderGames([refreshed]);
	updateDetailDownloadUi(refreshed);
	assert.equal(document.querySelector("#btn-launch-game .btn-text").textContent, "起動中");
	setRunningGameId(null);
	renderGames([refreshed]);
	updateDetailDownloadUi(refreshed);
	assert.equal(document.querySelector(".card-action-btn .btn-text").textContent, "起動する (Play)");
	assert.equal(document.getElementById("btn-launch-game").disabled, false);
	assert.equal(document.getElementById("btn-launch-game").style.pointerEvents, "auto");
});

test("undownloaded card and an already open detail view show metadata title", () => {
  setAllGames([]);
  setGameUpdateFlag(metadata.id, metadata.version);
  const game = findGameById(metadata.id);
  assert.notEqual(game.title, metadata.id);
  game._isDownloading = true;
  game._downloadProgress = { stage: "downloading", received_bytes: 10, total_bytes: 100 };
  setSelectedGame(game);
  assert.equal(applyRemoteGameMetadata(metadata.id, metadata), game);
  renderGames([game]);
  updateDetailMetadataUi(game);
  assert.equal(document.querySelector(".card-title").textContent, metadata.title);
  assert.equal(document.getElementById("modal-title").textContent, metadata.title);
  assert.equal(document.getElementById("modal-description").querySelector("h1").textContent, "遊び方");
  assert.equal(game.isInstalled, false);
  assert.equal(game._isDownloading, true);
  assert.equal(game._downloadProgress.received_bytes, 10);
});

test("late metadata cannot resurrect a deleted game or overwrite an installed game", () => {
  setAllGames([]);
  setGameUpdateFlag(metadata.id, metadata.version);
  removeGameById(metadata.id);
  assert.equal(applyRemoteGameMetadata(metadata.id, metadata), null);
  const installed = { id: metadata.id, title: "ローカルタイトル", isInstalled: true };
  setAllGames([installed]);
  assert.equal(applyRemoteGameMetadata(metadata.id, metadata), null);
  assert.equal(installed.title, "ローカルタイトル");
});

test("card creation and updates agree with detail progress, including unknown totals", () => {
	const game = { ...metadata, isInstalled: false, _isDownloading: true,
		_downloadProgress: { stage: "downloading", received_bytes: 37, total_bytes: 100 } };
	setSelectedGame(game);
	renderGames([game]);
	updateDetailDownloadUi(game);
	assert.equal(document.querySelector(".card-download-progress").getAttribute("aria-valuenow"), "37");
	assert.equal(document.getElementById("detail-download-progress").getAttribute("aria-valuenow"), "37");
	game._downloadProgress = { stage: "extracting", received_bytes: 0, total_bytes: 0 };
	updateGameCardDownloadUi(game);
	updateDetailDownloadUi(game);
	assert.equal(document.querySelector(".card-action-btn .btn-text").textContent, "展開中");
	assert.equal(document.getElementById("detail-download-progress-label").textContent, "展開中");
	assert.equal(document.querySelector(".card-download-progress").hasAttribute("aria-valuenow"), false);
	assert.equal(document.getElementById("detail-download-progress").hasAttribute("aria-valuenow"), false);
});
