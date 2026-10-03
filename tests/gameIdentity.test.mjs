import { test } from "node:test";
import assert from "node:assert/strict";
import { gameId, canonicalGameId } from "../src/js/core/gameIdentity.js";

test("API IDs preserve spelling and only remove a trailing executable suffix", () => {
	assert.equal(gameId({ id: " 日本語 Game.EXE ", game: "different.exe" }), "日本語 Game");
	assert.equal(gameId({ game: "Fallback.exe" }), "Fallback");
	assert.equal(gameId("my.exe.game"), "my.exe.game");
	assert.equal(gameId(null), "");
});

test("state comparisons use the same identity for string and metadata values", () => {
	assert.equal(canonicalGameId(" GAME.exe "), canonicalGameId({ id: "game" }));
	assert.notEqual(canonicalGameId("my.exe.game"), canonicalGameId("my.game"));
});
