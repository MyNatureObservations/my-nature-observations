import { readFileSync } from "node:fs";
import ts from "typescript";
export const moduleUrl = code => `data:text/javascript;base64,${Buffer.from(code).toString("base64")}`;
export function resolveImports(code, local = {}) {
  return code.replace(/from (["'])([^"']+)\1/g, (_, quote, name) => `from ${quote}${local[name] ?? import.meta.resolve(name)}${quote}`);
}
function tsUrl(name, local = {}) {
  const source = readFileSync(new URL(`../src/lib/${name}.ts`, import.meta.url), "utf8");
  return moduleUrl(resolveImports(ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext } }).outputText, local));
}
export const photographsUrl = tsUrl("journal-photographs");
export const controlsUrl = tsUrl("photograph-controls");
export const imageViewUrl = tsUrl("journal-image-view", { "./journal-photographs": photographsUrl });
export const imageUrl = tsUrl("journal-image", { "./journal-image-view": imageViewUrl, "./photograph-controls": controlsUrl });
export const { JournalImage } = await import(imageUrl);
export const photographs = await import(photographsUrl);
