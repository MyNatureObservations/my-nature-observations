import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";
import ts from "typescript";

const component = readFileSync(new URL("../src/lib/components/AppNavigation.svelte", import.meta.url), "utf8");
const source = component.match(/<script lang="ts">([\s\S]*?)<\/script>/)[1].replace(/^  import .*;\n/gm, "");
const script = ts.transpileModule(source, {
  compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.None },
}).outputText;
const translations = Object.fromEntries(["sv", "en"].map(locale => [locale,
  JSON.parse(readFileSync(new URL(`../messages/${locale}.json`, import.meta.url), "utf8")),
]));

const readerSource = readFileSync(new URL("../src/lib/journal-data.ts", import.meta.url), "utf8")
  .replace(/^import .*;\n/gm, "").replace(/^export /gm, "");
const readerScript = ts.transpileModule(readerSource, { compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.None } }).outputText;

function harness() {
  const calls = [];
  const context = vm.createContext({
    PLACE_MAP: 'mno:place-map', getContext: () => ({ close() {} }),
    $state: value => value,
    $derived: value => value,
    $effect: () => {},
    afterNavigate: () => {},
    onMount: () => {},
    language: { current: "sv" },
    m: new Proxy({}, { get: (_, key) => (_args, options) => translations[options.locale][key] }),
    invoke: (command, args) => {
      assert.equal(command, "create_manual_backup");
      return new Promise((resolve, reject) => calls.push({ args, resolve, reject }));
    },
  });
  vm.runInContext(readerScript, context);
  vm.runInContext(script, context);
  return { calls, run: code => vm.runInContext(code, context) };
}

test("manual backup shows working and verified-success status and blocks double clicks", async () => {
  const h = harness();
  const operation = h.run("createManualBackup()");
  assert.equal(h.run("isCreatingBackup"), true);
  assert.equal(h.run("backupStatusMessage()"), "Skapar säkerhetskopia…");
  await h.run("createManualBackup()");
  assert.equal(h.calls.length, 1);
  assert.match(h.calls[0].args.suggestedName, /^MNO-backup-\d{4}-\d{2}-\d{2}_\d{4}\.mno-backup$/);
  assert.equal(h.calls[0].args.dialogTitle, "Skapa säkerhetskopia…");
  h.calls[0].resolve("created");
  await operation;
  assert.equal(h.run("isCreatingBackup"), false);
  assert.equal(h.run("backupStatusMessage()"), "Säkerhetskopian är skapad.");
  h.run('language.current = "en"');
  assert.equal(h.run("backupStatusMessage()"), "The backup has been created.");
});

test("cancelling the save dialog returns silently to idle and allows another attempt", async () => {
  const h = harness();
  const operation = h.run("createManualBackup()");
  h.calls[0].resolve("cancelled");
  await operation;
  assert.equal(h.run("backupStatus"), "idle");
  assert.equal(h.run("backupStatusMessage()"), "");
  assert.equal(h.run("isCreatingBackup"), false);
  const next = h.run("createManualBackup()");
  assert.equal(h.calls.length, 2);
  h.calls[1].resolve("cancelled");
  await next;
});

for (const code of ["already_exists", "invalid_destination", "busy", "failed", "unexpected error"]) {
  test(`backup failure ${code} gives understandable feedback and permits retry`, async () => {
    const h = harness();
    const operation = h.run("createManualBackup()");
    h.calls[0].reject(code);
    await operation;
    const expectedStatus = code === "unexpected error" ? "failed" : code;
    assert.equal(h.run("backupStatus"), expectedStatus);
    assert.equal(h.run("isCreatingBackup"), false);
    assert.equal(h.run("backupStatusMessage()"), translations.sv[`backup_${expectedStatus}`]);
    const next = h.run("createManualBackup()");
    h.calls[1].resolve("created");
    await next;
    assert.equal(h.run("backupStatus"), "created");
  });
}

test("suggested filename uses the local date and time with zero padding", () => {
  const h = harness();
  assert.equal(h.run("suggestedBackupFilename(new Date(2026, 9, 17, 14, 30))"), "MNO-backup-2026-10-17_1430.mno-backup");
  assert.equal(h.run("suggestedBackupFilename(new Date(2026, 0, 2, 3, 4))"), "MNO-backup-2026-01-02_0304.mno-backup");
});


test("new backup clears prior export success and blocks concurrent export", async () => {
  const h = harness();
  h.run('exportStatus = "created"; backupStatus = "created"');
  const operation = h.run('createManualBackup()');
  assert.equal(h.run('exportStatus'), 'idle'); assert.equal(h.run('backupStatus'), 'creating');
  await h.run('createOpenExport()'); assert.equal(h.calls.length, 1);
  h.calls[0].resolve('created'); await operation;
});
