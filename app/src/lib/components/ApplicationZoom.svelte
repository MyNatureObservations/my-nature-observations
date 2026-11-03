<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import { createZoomController, routeZoomKey } from "$lib/application-zoom";
  import { nativeMenuLabels } from "$lib/native-menu-labels";
  import { language } from "$lib/language.svelte";
  import * as m from "$lib/paraglide/messages.js";

  let failed = $state(false);
  let menuFailed = $state(false);
  let menuReady = $state(false);
  let menuUpdates = Promise.resolve();
  $effect(() => {
    const labels = nativeMenuLabels(language.current);
    if (menuReady) {
      menuUpdates = menuUpdates.then(() => invoke<void>("update_native_menu_labels", { labels }))
        .then(() => { menuFailed = false; }).catch(() => { menuFailed = true; });
    }
  });

  onMount(() => {
    let disposed = false;
    let stopListening: (() => void) | undefined;
    const controller = createZoomController({
      setZoom: level => getCurrentWebview().setZoom(level),
      storage: () => localStorage,
      onFailure: () => { failed = true; },
      onApplied: () => { failed = false; }
    });
    void controller.initialize();
    const mac = /Mac|iPhone|iPad/.test(navigator.platform);
    const keydown = (event: KeyboardEvent) => routeZoomKey(event, mac, action => controller.request(action));
    // Own shortcuts once, before editor/dialog handlers. Native menu items have no accelerators.
    window.addEventListener("keydown", keydown, true);
    const listener = listen<string>("mno:zoom", event => {
      if (!disposed && (event.payload === "in" || event.payload === "out" || event.payload === "reset")) {
        void controller.request(event.payload);
      }
    });
    void listener.then(stop => {
      if (disposed) stop();
      else { stopListening = stop; menuReady = true; }
    }).catch(() => { if (!disposed) failed = true; });
    return () => {
      disposed = true;
      menuReady = false;
      window.removeEventListener("keydown", keydown, true);
      stopListening?.();
    };
  });
</script>

{#if menuFailed}<p class="zoom-error" role="status">{m.native_menu_failed({}, { locale: language.current })}</p>{/if}
{#if failed}<p class="zoom-error" role="status">{m.zoom_failed({}, { locale: language.current })}</p>{/if}

<style>
  .zoom-error { margin: 12px auto; max-width: 640px; padding: 0 28px; color: #9a403b; font: 13px/1.5 system-ui,sans-serif; }
</style>
