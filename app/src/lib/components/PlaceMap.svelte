<script lang="ts">
  import { onMount } from "svelte";
  import "leaflet/dist/leaflet.css";
  import type * as Leaflet from "leaflet";
  import { groupMappedPlaces, mapDraftCoordinates, type Place, type PlaceCoordinates } from "$lib/places";
  import * as m from "$lib/paraglide/messages.js";

  let {
    latitude,
    longitude,
    places = [],
    journalCenter = null,
    defaultCenter = null,
    locale = "sv",
    onCoordinates,
    onCreateHere,
    focusCenter = null,
    selectedPlaceId = null,
    onSelectPlace,
  }: {
    latitude: string;
    longitude: string;
    places?: Place[];
    journalCenter?: PlaceCoordinates | null;
    defaultCenter?: PlaceCoordinates | null;
    locale?: "sv" | "en";
    onCoordinates: (latitude: number, longitude: number) => void;
    onCreateHere?: (latitude: number, longitude: number, screen?: { x: number; y: number }) => void;
    focusCenter?: PlaceCoordinates | null;
    selectedPlaceId?: string | null;
    onSelectPlace?: (place: Place) => void;
  } = $props();

  let mapElement: HTMLDivElement;
  let map: Leaflet.Map | null = null;
  let marker: Leaflet.Marker | null = null;
  let savedLayer: Leaflet.LayerGroup | null = null;
  let leaflet: typeof Leaflet | null = null;
  let tilesUnavailable = $state(false);
  let retrying = $state(false);
  let tileLayer: Leaflet.TileLayer | null = null;
  let disposed = false;
  let reloading = false;
  let retryNeedsTiles = false;
  let cameraReady = false;
  let worldUntouched = false;
  let movingCamera = false;
  let lastWorldSize = "";
  // Identity distinguishes a replaced tile from a late event at the same coordinates.
  const tiles = new Map<HTMLElement, { coords: Leaflet.Coords; state: "loading" | "loaded" | "failed" }>();
  function usablePoint(point: PlaceCoordinates | null): [number, number] | null {
    return point ? mapDraftCoordinates(String(point.latitude), String(point.longitude)) : null;
  }
  function initialCoordinates(): [number, number] | null {
    return parsedCoordinates() ?? usablePoint(focusCenter) ?? usablePoint(journalCenter) ?? usablePoint(defaultCenter);
  }
  function moveCamera(point: [number, number], zoom: number) {
    if (!map || disposed) return;
    const current = map.getCenter();
    if (current.lat === point[0] && current.lng === point[1] && map.getZoom() === zoom) return;
    movingCamera = true;
    try { map.setView(point, zoom, { animate: false }); }
    finally { movingCamera = false; }
  }
  function moveToPlace(point: [number, number]) {
    worldUntouched = false;
    moveCamera(point, Math.max(map?.getZoom() ?? 0, 12));
  }
  function refreshTileStatus() {
    if (disposed || !map || !tileLayer) return;
    const nativeZoom = Math.max(0, Math.round(map.getZoom()));
    const size = tileLayer.getTileSize();
    // Removal is synchronous; do not interpret its temporarily empty set as recovery.
    if (reloading) return;
    const replacementTiles = [...tiles.values()];
    if (retryNeedsTiles && replacementTiles.length > 0 && !replacementTiles.some(tile => tile.state === "loading")) {
      retryNeedsTiles = false;
      retrying = false;
    } else if (retrying && replacementTiles.length === 0) {
      retrying = false; // No requests started: retain the warning and allow another attempt.
    }
    tilesUnavailable = retryNeedsTiles || replacementTiles.some(({ coords, state }) => {
      if (state !== "failed" || coords.z !== nativeZoom) return false;
      const bounds = map!.getPixelBounds();
      if (!bounds.min || !bounds.max) return false;
      const scale = map!.getZoomScale(coords.z, map!.getZoom());
      return coords.x * size.x < bounds.max.x * scale && (coords.x + 1) * size.x > bounds.min.x * scale
        && coords.y * size.y < bounds.max.y * scale && (coords.y + 1) * size.y > bounds.min.y * scale;
    });
  }
  function tileStarted(event: Leaflet.TileEvent) {
    if (disposed) return;
    tiles.set(event.tile, { coords: event.coords, state: "loading" });
    refreshTileStatus();
  }
  function tileFinished(event: Leaflet.TileEvent, failed: boolean) {
    if (disposed) return;
    const tile = tiles.get(event.tile);
    if (!tile) return;
    tile.state = failed ? "failed" : "loaded";
    refreshTileStatus();
  }
  function tileRemoved(event: Leaflet.TileEvent) {
    if (disposed) return;
    tiles.delete(event.tile);
    refreshTileStatus();
  }
  function retryTiles() {
    if (disposed || !map || !tileLayer || retrying || !tilesUnavailable) return;
    retrying = true; reloading = true; retryNeedsTiles = true;
    try {
      // redraw() bypasses Leaflet 1.9.4's normal integer tile-zoom initialization.
      tileLayer.remove();
      tiles.clear();
      tileLayer.addTo(map);
    } finally { reloading = false; refreshTileStatus(); }
  }
  function updateViewSize() {
    if (!map || !leaflet || disposed) return;
    map.invalidateSize({ pan: false });
    const size = map.getSize();
    if (size.x <= 0 || size.y <= 0) return;
    if (!cameraReady) {
      const point = initialCoordinates();
      cameraReady = true;
      worldUntouched = point === null;
      if (point) moveToPlace(point);
      updateMarker();
      updateSavedPlaces();
    }
    if (worldUntouched) {
      const measured = `${size.x}:${size.y}`;
      if (measured !== lastWorldSize) {
        const mercatorLimit = 85.0511287798;
        const world = leaflet.latLngBounds([-mercatorLimit, -180], [mercatorLimit, 180]);
        lastWorldSize = measured;
        moveCamera([0, 0], map.getBoundsZoom(world));
      }
    }
    refreshTileStatus();
  }
  let initializedCoordinates = false;
  const contextTargets = new WeakMap<Element, () => Leaflet.LatLng>();

  function showSavedChoices(group: Place[], point: Leaflet.LatLng) {
    if (!leaflet || !map || !onSelectPlace) return;
    if (group.length === 1) { map.closePopup(); onSelectPlace(group[0]); return; }
    const choices = document.createElement("div");
    for (const place of group) {
      const button = document.createElement("button");
      button.type = "button";
      button.textContent = `${place.id === selectedPlaceId ? "✓ " : ""}${place.name}`;
      button.setAttribute("aria-pressed", String(place.id === selectedPlaceId));
      button.addEventListener("click", (event) => { event.stopPropagation(); map?.closePopup(); onSelectPlace?.(place); });
      choices.append(button);
    }
    leaflet.DomEvent.disableClickPropagation(choices);
    leaflet.popup().setLatLng(point).setContent(choices).openOn(map);
    choices.querySelector("button")?.focus();
  }

  function nativeContextMenu(event: MouseEvent) {
    if (!map || !onCreateHere || (event.target instanceof Element && event.target.closest(".leaflet-control"))) return;
    event.preventDefault();
    event.stopPropagation();
    let target = event.target instanceof Element ? event.target : null;
    let point: Leaflet.LatLng | null = null;
    while (target && target !== mapElement) {
      const locate = contextTargets.get(target);
      if (locate) { point = locate(); break; }
      target = target.parentElement;
    }
    point ??= map.mouseEventToLatLng(event);
    onCreateHere(point.lat, point.lng, { x: event.clientX, y: event.clientY });
  }

  function parsedCoordinates(): [number, number] | null {
    return mapDraftCoordinates(latitude, longitude);
  }

  function publish(point: Leaflet.LatLng) {
    worldUntouched = false;
    onCoordinates(Number(point.lat.toFixed(6)), Number(point.lng.toFixed(6)));
  }

  function updateMarker() {
    if (!leaflet || !map) return;
    const coordinates = parsedCoordinates();
    if (!coordinates) {
      marker?.remove();
      marker = null;
      return;
    }
    if (!marker) {
      marker = leaflet.marker(coordinates, {
        draggable: true,
        bubblingMouseEvents: false,
        keyboard: true,
        icon: leaflet.divIcon({
          className: "mno-place-marker",
          html: '<span aria-hidden="true"></span>',
          iconSize: [24, 24],
          iconAnchor: [12, 12],
        }),
        title: m.place_map_marker({}, { locale }),
      });
      marker.on("dragend", () => marker && publish(marker.getLatLng()));
      marker.on("click", () => {
        if (!marker) return;
        const point = marker.getLatLng();
        const group = groupMappedPlaces(places).find(group => group[0].latitude === point.lat && group[0].longitude === point.lng);
        if (group) showSavedChoices(group, point);
      });
      marker.addTo(map);
      const element = marker.getElement();
      if (element) contextTargets.set(element, () => marker!.getLatLng());
    } else {
      marker.setLatLng(coordinates);
    }
    if (cameraReady && !initializedCoordinates) {
      moveToPlace(coordinates);
      initializedCoordinates = true;
    }
  }

  function updateSavedPlaces() {
    if (!leaflet || !map || !savedLayer) return;
    savedLayer.clearLayers();
    for (const group of groupMappedPlaces(places)) {
      const place = group[0];
      const point = leaflet.circleMarker([place.latitude!, place.longitude!], {
        radius: group.some(place => place.id === selectedPlaceId) ? 8 : 6,
        bubblingMouseEvents: false,
        color: "#607763",
        weight: 2,
        fillColor: "#f8f7f3",
        fillOpacity: 0.9,
      }).addTo(savedLayer);
      const label = document.createElement("span");
      label.textContent = `${group.map(place => place.name).join(" · ")} · ${place.latitude}, ${place.longitude}`;
      point.bindTooltip(label);
      point.on("click", () => showSavedChoices(group, point.getLatLng()));
      const element = point.getElement();
      if (element) {
        contextTargets.set(element, () => point.getLatLng());
        element.setAttribute("tabindex", "0");
        element.setAttribute("role", "button");
        element.setAttribute("aria-label", label.textContent);
        element.addEventListener("keydown", (event) => { const key = (event as KeyboardEvent).key; if (key === "Enter" || key === " ") { event.preventDefault(); event.stopPropagation(); showSavedChoices(group, point.getLatLng()); } });
      }
    }
  }

  $effect(() => {
    latitude;
    longitude;
    updateMarker();
  });

  $effect(() => {
    places;
    selectedPlaceId;
    updateSavedPlaces();
  });
  function focusSelectedPlace() {
    const point = usablePoint(focusCenter);
    if (cameraReady && point) moveToPlace(point);
  }
  $effect(() => { focusCenter; focusSelectedPlace(); });

  onMount(() => {
    let resizeFrame = 0;
    let resizeObserver: ResizeObserver | null = null;
    void (async () => {
      const loaded = await import("leaflet");
      if (disposed) return;
      leaflet = loaded;
      const start = initialCoordinates();
      map = loaded.map(mapElement, {
        center: start ?? [0, 0],
        zoom: start ? 12 : 0,
        // Zoom below native level zero keeps the whole world visible in low containers.
        minZoom: -4,
        zoomSnap: 0.1,
        trackResize: false,
        keyboard: true,
        attributionControl: true,
      });
      map.getContainer().setAttribute("aria-label", m.place_map_label({}, { locale }));
      map.on("movestart zoomstart", () => { if (!movingCamera) worldUntouched = false; });
      map.on("moveend zoomend", refreshTileStatus);
      map.on("click", (event: Leaflet.LeafletMouseEvent) => publish(event.latlng));
      mapElement.addEventListener("contextmenu", nativeContextMenu, true);
      savedLayer = loaded.layerGroup().addTo(map);

      tileLayer = loaded.tileLayer("https://tile.openstreetmap.org/{z}/{x}/{y}.png", {
        minZoom: -4,
        minNativeZoom: 0,
        maxZoom: 19,
        noWrap: true,
        attribution: '&copy; <a href="https://www.openstreetmap.org/copyright" target="_blank" rel="noopener noreferrer">OpenStreetMap contributors</a>',
      });
      tileLayer.on("tileloadstart", tileStarted)
        .on("tileload", (event: Leaflet.TileEvent) => tileFinished(event, false))
        .on("tileerror", (event: Leaflet.TileErrorEvent) => tileFinished(event, true))
        .on("tileunload", tileRemoved)
        .on("load", refreshTileStatus)
        .addTo(map);

      resizeObserver = new ResizeObserver(() => {
        cancelAnimationFrame(resizeFrame);
        resizeFrame = requestAnimationFrame(updateViewSize);
      });
      resizeObserver.observe(mapElement);
      resizeFrame = requestAnimationFrame(updateViewSize);
    })();
    return () => {
      disposed = true;
      mapElement.removeEventListener("contextmenu", nativeContextMenu, true);
      resizeObserver?.disconnect();
      cancelAnimationFrame(resizeFrame);
      tileLayer?.off();
      tiles.clear();
      map?.remove();
      tileLayer = null;
      map = null;
      marker = null;
      savedLayer = null;
      leaflet = null;
    };
  });
</script>

<div class="map-shell">
  <div class="map" bind:this={mapElement}></div>
  {#if tilesUnavailable || retrying}
    <div class="tile-warning">
      <p role="status">{m.place_map_tiles_unavailable({}, { locale })}</p>
      <button type="button" disabled={retrying} onclick={retryTiles}>{m.place_map_retry({}, { locale })}</button>
    </div>
  {/if}
</div>
<p class="map-help">{m.place_map_help({}, { locale })}</p>

<style>
  .map-shell { position: relative; margin-top: 14px; }
  .map { height: clamp(280px, 55vh, 560px); min-height: 190px; border: 1px solid #cccec7; border-radius: 10px; background: #e9ebe5; overflow: hidden; }
  .tile-warning { position: absolute; left: 10px; bottom: 22px; z-index: 500; max-width: calc(100% - 20px); margin: 0; padding: 6px 9px; border-radius: 6px; background: rgba(248,247,243,.92); color: #696d65; font: 11px system-ui,sans-serif; }
  .tile-warning p { margin: 0; }
  .tile-warning button { margin-top: 4px; border: 1px solid #d9d8d1; border-radius: 4px; padding: 4px 8px; background: #f8f7f3; color: inherit; font: inherit; cursor: pointer; }
  .tile-warning button:disabled { opacity: .55; cursor: default; }
  .tile-warning button:focus-visible { outline: 2px solid #788e7b; outline-offset: 2px; }
  .map-help { margin: 6px 0 0; color: var(--text-muted, #686d63); font: 11px system-ui,sans-serif; }
  :global(.mno-place-marker) { border: 0; background: transparent; }
  :global(.mno-place-marker span) { display: block; width: 22px; height: 22px; border: 3px solid #f8f7f3; border-radius: 50% 50% 50% 0; background: #607763; box-shadow: 0 2px 7px rgba(42,49,42,.28); transform: rotate(-45deg); }
  :global(.leaflet-control-attribution) { font: 10px system-ui,sans-serif; }
  :global(.leaflet-popup-content button) { display: block; width: 100%; padding: 8px; text-align: left; border: 1px solid #d9d8d1; background: #f8f7f3; color: #435b47; cursor: pointer; }
  @media (max-width: 560px), (max-height: 680px) { .map { height: clamp(190px, 43vh, 360px); } }
</style>
