import { test } from "node:test";
import assert from "node:assert/strict";
import { JSDOM } from "jsdom";

const dom = new JSDOM("<!doctype html><div id='description'></div>");
globalThis.window = dom.window;
globalThis.document = dom.window.document;
const { renderMarkdown } = await import("../src/js/ui/markdown.js");
const element = document.getElementById("description");

test("renders headings, lists, emphasis, tables, and code", () => {
  renderMarkdown(element, "# 遊び方\n\n- **移動**: WASD\n- ジャンプ: `Space`\n\n| 操作 | キー |\n| --- | --- |\n| 戻る | Esc |\n\n```js\nalert('<test>');\n```");
  assert.equal(element.querySelector("h1").textContent, "遊び方");
  assert.equal(element.querySelectorAll("li").length, 2);
  assert.equal(element.querySelector("strong").textContent, "移動");
  assert.equal(element.querySelector("table td").textContent, "戻る");
  assert.equal(element.querySelector("pre code").textContent.trim(), "alert('<test>');");
});

test("preserves ordinary line breaks and separate paragraphs", () => {
  renderMarkdown(element, "一行目\n二行目\n\n次の段落");
  assert.equal(element.querySelectorAll("br").length, 1);
  assert.equal(element.querySelectorAll("p").length, 2);
});

test("raw HTML and dangerous URLs cannot execute", () => {
  renderMarkdown(element, '<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>\n\n[危険](javascript:alert(1))\n\n[ローカル](file:///C:/secret)');
  assert.equal(element.querySelector("script, img, iframe, [onerror]"), null);
  assert.equal(element.querySelectorAll("a[href]").length, 0);
  assert.ok(element.textContent.includes("<script>"));
});

test("external links are isolated and local image paths are not loaded", () => {
  renderMarkdown(element, "[サイト](https://example.com)\n\n![ローカル](secret.png)\n\n![画像](https://example.com/image.png)");
  assert.equal(element.querySelector("a").rel, "noopener noreferrer");
  assert.equal(element.querySelectorAll("img").length, 1);
  assert.equal(element.querySelector("img").referrerPolicy, "no-referrer");
});
