import assert from "node:assert/strict";
import test from "node:test";
import { readFileSync } from "node:fs";
import { developmentConfig, tauriArguments } from "../scripts/tauri.mjs";
const read = file => readFileSync(new URL(file,import.meta.url),"utf8");
const production=JSON.parse(read("../src-tauri/tauri.conf.json"));
const development=JSON.parse(read("../src-tauri/tauri.dev.conf.json"));

test("normal dev commands always apply the explicit DEV identity last",()=>{
  for(const args of [["dev"],["dev","--release"],["build","--debug"],["dev","--config","other.json"]]) {
    assert.deepEqual(tauriArguments(args),[...args,"--config",developmentConfig]);
  }
  assert.deepEqual(tauriArguments(["dev","--","application-argument"]),["dev","--config",developmentConfig,"--","application-argument"]);
  assert.equal(JSON.parse(read("../package.json")).scripts.tauri,"node scripts/tauri.mjs");
});

test("ordinary production builds keep their configuration and identity",()=>{
  assert.deepEqual(tauriArguments(["build"]),["build"]);
  assert.deepEqual(tauriArguments(["build","--bundles","app"]),["build","--bundles","app"]);
  assert.deepEqual(tauriArguments(["info"]),["info"]);
  assert.equal(production.identifier,"org.mynatureobservations.app");assert.equal(production.productName,"My Nature Observations");
  assert.equal(development.identifier,"org.mynatureobservations.app.dev");assert.match(development.productName,/DEV/);
  assert.equal(development.app,undefined); // Preserve production window geometry in the overlay.
});

test("identity validation precedes single-instance registration and database startup",()=>{
  const source=read("../src-tauri/src/lib.rs");
  assert.ok(source.indexOf("environment::validate_identity")<source.indexOf("tauri::Builder::default()"));
  assert.ok(source.indexOf("environment::validate_identity")<source.indexOf("tauri_plugin_single_instance::init"));
});

// Tile images use the webview HTTP stack, not Rust's weather client.
test("packaged webviews identify MNO to the public tile service", () => {
  const expected = `MyNatureObservations/${production.version} (+https://github.com/MyNatureObservations/my-nature-observations)`;
  assert.ok(production.app.windows.length > 0);
  for (const window of production.app.windows) assert.equal(window.userAgent, expected);
  assert.equal(development.app, undefined, "DEV inherits the identifying webview configuration");
});
