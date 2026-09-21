import { invoke } from "../core/tauri.js";
import { getSelectedGame } from "../core/state.js";
import { setLogText } from "../ui/log.js";

let loadSequence = 0;

function getGameId(game) {
	return (game?.id || game?.game || "").replace(/\.exe$/i, "");
}

function formatDate(unixSeconds) {
	const date = new Date(Number(unixSeconds) * 1000);
	return Number.isNaN(date.getTime()) ? "" : date.toLocaleString("ja-JP");
}

function renderMessage(message, isError = false) {
	const list = document.getElementById("comment-list");
	if (!list) return;
	list.replaceChildren();
	const element = document.createElement("p");
	element.className = `comment-message${isError ? " error" : ""}`;
	element.textContent = message;
	list.appendChild(element);
}

function renderComments(comments) {
	const list = document.getElementById("comment-list");
	const count = document.getElementById("comment-count");
	if (count) count.textContent = `${comments.length}件`;
	if (!list) return;
	list.replaceChildren();
	if (comments.length === 0) {
		renderMessage("まだコメントはありません。最初のコメントをどうぞ！");
		return;
	}

	for (const comment of comments) {
		const item = document.createElement("article");
		item.className = "comment-item";
		const meta = document.createElement("div");
		meta.className = "comment-meta";
		const author = document.createElement("span");
		author.className = "comment-author";
		author.textContent = comment.author || "匿名";
		const date = document.createElement("time");
		date.className = "comment-date";
		date.textContent = formatDate(comment.created_at);
		const content = document.createElement("p");
		content.className = "comment-content";
		content.textContent = comment.content;
		meta.append(author, date);
		item.append(meta, content);
		list.appendChild(item);
	}
	list.scrollTop = list.scrollHeight;
}

export async function loadComments(game) {
	const gameId = getGameId(game);
	const sequence = ++loadSequence;
	const count = document.getElementById("comment-count");
	if (count) count.textContent = "--";
	renderMessage("コメントを読み込んでいます...");
	if (!window.__TAURI__) {
		renderComments([]);
		return;
	}

	try {
		const comments = await invoke("list_comments", { gameId });
		if (sequence === loadSequence && getGameId(getSelectedGame()) === gameId) {
			renderComments(Array.isArray(comments) ? comments : []);
		}
	} catch (error) {
		if (sequence === loadSequence) {
			renderComments([]);
			setLogText(`コメントの取得に失敗しました: ${error}`);
		}
	}
}

export async function submitComment() {
	const game = getSelectedGame();
	const gameId = getGameId(game);
	const authorInput = document.getElementById("comment-author");
	const contentInput = document.getElementById("comment-content");
	const button = document.getElementById("btn-submit-comment");
	const content = contentInput?.value.trim() || "";
	if (!content) {
		setLogText("コメントを入力してください");
		contentInput?.focus();
		return;
	}
	if (!window.__TAURI__) return;

	button.disabled = true;
	try {
		await invoke("add_comment", {
			gameId,
			author: authorInput?.value || "",
			content,
		});
		contentInput.value = "";
		const length = document.getElementById("comment-length");
		if (length) length.textContent = "0 / 1000";
		setLogText(`${game?.title || gameId} にコメントを投稿しました`);
		await loadComments(game);
	} catch (error) {
		setLogText(`コメントの投稿に失敗しました: ${error}`);
	} finally {
		button.disabled = false;
	}
}
