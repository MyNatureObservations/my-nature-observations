import type { Place, PlaceCoordinates } from "$lib/places";

export const PLACE_MAP = "mno:place-map";
export type MapRequest = {
  mode: "orientation" | "journal" | "default" | "observation";
  generation?: number;
  center?: PlaceCoordinates | null;
  selectedId?: string | null;
  currentId?: string | null;
  remove?: () => Promise<boolean>;
  select?: (place: Place) => Promise<boolean>;
};
export function createPlaceMap() {
  let request = $state.raw<MapRequest | null>(null);
  let busy = $state(false);
  let closeGuard: (() => boolean) | null = null;
  let returnFocus: HTMLElement | null = null;
  function close(force = false) {
    if (!force && (busy || (closeGuard && !closeGuard()))) return;
    request = null; busy = false; closeGuard = null;
    if (returnFocus?.isConnected) returnFocus.focus();
  }
  return {
    get request() { return request; },
    get busy() { return busy; },
    setCloseGuard(guard: () => boolean) { closeGuard = guard; return () => { if (closeGuard === guard) closeGuard = null; }; },
    setBusy(value: boolean) { busy = value; },
    open(next: MapRequest) {
      if (busy || (closeGuard && !closeGuard())) return () => {};
      closeGuard = null;
      returnFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      request = next;
      return () => { if (request === next) close(true); };
    },
    close,
  };
}
export type PlaceMapController = ReturnType<typeof createPlaceMap>;
