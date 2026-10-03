import { Marked } from "../vendor/marked.js";
import DOMPurify from "../vendor/dompurify.js";

const escapeHtml = text => text.replace(/[&<>"']/g, character => ({
  "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
}[character]));

const parser = new Marked({
  gfm: true,
  breaks: true,
  // Description authors write Markdown; raw HTML stays visible as text.
  renderer: { html: token => escapeHtml(token.text) },
});

export function renderMarkdown(element, text) {
  const fragment = DOMPurify.sanitize(parser.parse(String(text || "")), {
    RETURN_DOM_FRAGMENT: true,
    ALLOWED_TAGS: ["p", "br", "h1", "h2", "h3", "h4", "h5", "h6", "ul", "ol", "li",
      "blockquote", "pre", "code", "em", "strong", "del", "hr", "table", "thead", "tbody",
      "tr", "th", "td", "a", "img", "input"],
    ALLOWED_ATTR: ["href", "src", "alt", "title", "start", "align", "type", "checked", "disabled"],
    ALLOW_DATA_ATTR: false,
    ALLOW_ARIA_ATTR: false,
  });
  // Only external web links/mail and fragment anchors are meaningful in a description.
  for (const link of fragment.querySelectorAll("a")) {
    const href = link.getAttribute("href") || "";
    if (!/^(https?:\/\/|mailto:|#)/i.test(href)) link.removeAttribute("href");
    else if (!href.startsWith("#")) {
      link.target = "_blank";
      link.rel = "noopener noreferrer";
    }
  }
  // Local files are not exposed to the webview; remote images use explicit HTTPS URLs.
  for (const image of fragment.querySelectorAll("img")) {
    if (!/^https:\/\//i.test(image.getAttribute("src") || "")) {
      image.replaceWith(document.createTextNode(image.alt));
    } else {
      image.loading = "lazy";
      image.referrerPolicy = "no-referrer";
    }
  }
  for (const input of fragment.querySelectorAll("input")) {
    if (input.type !== "checkbox") input.remove();
    else input.disabled = true;
  }
  element.replaceChildren(fragment);
}
