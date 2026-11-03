import { fileURLToPath } from "node:url";
import { resolve } from "node:path";

export const developmentConfig = fileURLToPath(new URL("../src-tauri/tauri.dev.conf.json", import.meta.url));

export function tauriArguments(args) {
  const boundary = args.indexOf("--");
  const cli = boundary < 0 ? args : args.slice(0, boundary);
  const development = cli[0] === "dev" || (cli[0] === "build" && cli.includes("--debug"));
  // Apply our identity last, before any arguments forwarded to the application.
  return development
    ? [...cli, "--config", developmentConfig, ...(boundary < 0 ? [] : args.slice(boundary))]
    : args;
}

if (resolve(process.argv[1] ?? "") === fileURLToPath(import.meta.url)) {
  const { run, logError } = await import("@tauri-apps/cli");
  try { await run(tauriArguments(process.argv.slice(2)), "npm run tauri"); }
  catch (error) { logError(String(error)); process.exitCode = 1; }
}
