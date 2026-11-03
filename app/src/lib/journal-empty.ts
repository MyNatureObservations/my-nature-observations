// A conservative recognizer, not a document validator or normalizer.
// Unknown nodes, attributes, marks and malformed structures are never hidden.
const blocks = ["paragraph", "heading", "blockquote", "bulletList", "orderedList", "codeBlock", "horizontalRule"];
const inline = ["text", "hardBreak"];
const whitespace = /^[\t\n\v\f\r \u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000\ufeff]*$/u;
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === "object" && !Array.isArray(value);
const keys = (value: Record<string, unknown>, allowed: string[]) =>
  Object.keys(value).every(key => allowed.includes(key));

function emptyAttrs(node: Record<string, unknown>, type: string): boolean {
  if (!("attrs" in node)) return true;
  const attrs = node.attrs;
  if (!record(attrs)) return false;
  if (type === "heading") {
    return keys(attrs, ["level"]) && (!("level" in attrs) ||
      (Number.isInteger(attrs.level) && Number(attrs.level) >= 1 && Number(attrs.level) <= 6));
  }
  if (type === "orderedList") {
    return keys(attrs, ["start", "type"]) && (!("start" in attrs) ||
      (Number.isSafeInteger(attrs.start) && Number(attrs.start) >= 1)) &&
      (!("type" in attrs) || attrs.type === null || (typeof attrs.type === "string" && ["1", "a", "A", "i", "I"].includes(attrs.type)));
  }
  if (type === "codeBlock") {
    return keys(attrs, ["language"]) && (!("language" in attrs) || attrs.language === null || typeof attrs.language === "string");
  }
  return keys(attrs, []);
}

function emptyNode(value: unknown, allowed: string[], depth: number): boolean {
  if (depth > 64 || !record(value) || typeof value.type !== "string" || !allowed.includes(value.type)) return false;
  const type = value.type;
  if (type === "text") {
    if (!keys(value, ["type", "text", "marks"]) || typeof value.text !== "string" || !whitespace.test(value.text)) return false;
    if (!("marks" in value)) return true;
    return Array.isArray(value.marks) && value.marks.every(mark =>
      record(mark) && keys(mark, ["type", "attrs"]) &&
      typeof mark.type === "string" && ["bold", "italic", "strike", "underline", "code"].includes(mark.type) &&
      (!("attrs" in mark) || (record(mark.attrs) && keys(mark.attrs, []))));
  }
  if (type === "hardBreak") return keys(value, ["type"]);
  if (type === "horizontalRule" || !keys(value, ["type", "content", "attrs"]) || !emptyAttrs(value, type)) return false;
  const content = "content" in value ? value.content : [];
  if (!Array.isArray(content)) return false;
  if (["bulletList", "orderedList", "blockquote", "listItem"].includes(type) && content.length === 0) return false;
  if (type === "listItem" && (!record(content[0]) || content[0].type !== "paragraph")) return false;
  const children = type === "paragraph" || type === "heading" ? inline
    : type === "codeBlock" ? ["text"]
    : type === "bulletList" || type === "orderedList" ? ["listItem"] : blocks;
  return content.every(child => emptyNode(child, children, depth + 1));
}

export function isDefinitelyEmptyJournalDocument(document: unknown): boolean {
  return emptyNode(document, ["doc"], 0);
}

export function isDefinitelyEmptyJournalJson(contentJson: string): boolean {
  try { return isDefinitelyEmptyJournalDocument(JSON.parse(contentJson)); }
  catch { return false; }
}
