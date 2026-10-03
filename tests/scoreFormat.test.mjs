import { test } from "node:test";
import assert from "node:assert/strict";
import { formatScore } from "../src/js/leaderboards/scoreFormat.js";
test("formats ordinary scores without rounding", () => {
 assert.equal(formatScore("12345"), "12,345"); assert.equal(formatScore("-12.5000"), "-12.5");
 assert.equal(formatScore("1.23456789e3"), "1,234.56789"); assert.equal(formatScore("0.0001"), "0.0001");
});
test("large and tiny scores use scientific display without Number overflow", () => {
 assert.equal(formatScore("123456789012345678901234567890"), "1.235e29");
 assert.equal(formatScore("1.25e1000"), "1.25e1000"); assert.equal(formatScore("9.99999e999"), "1e1000");
 assert.equal(formatScore("-1e-1000"), "-1e-1000"); assert.equal(formatScore("0"), "0");
});
