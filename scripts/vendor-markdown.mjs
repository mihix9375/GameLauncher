// Keep the browser libraries local so Markdown also works on the club LAN without Internet.
import { copyFile, mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";

const root = new URL("../", import.meta.url);
const target = new URL("src/js/vendor/", root);
await mkdir(target, { recursive: true });
for (const [source, name] of [
  ["node_modules/marked/lib/marked.esm.js", "marked.js"],
  ["node_modules/marked/LICENSE", "marked-LICENSE.txt"],
  ["node_modules/dompurify/dist/purify.es.mjs", "dompurify.js"],
  ["node_modules/dompurify/LICENSE", "dompurify-LICENSE.txt"],
]) {
  await copyFile(fileURLToPath(new URL(source, root)), fileURLToPath(new URL(name, target)));
}
