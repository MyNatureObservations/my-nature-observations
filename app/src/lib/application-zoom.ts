export const ZOOM_LEVELS = [1, 1.25, 1.5, 1.75, 2] as const;
export const ZOOM_STORAGE_KEY = "mno.application-zoom.v1";
export type ZoomAction = "in" | "out" | "reset";
export type ZoomStorage = Pick<Storage, "getItem" | "setItem">;

export function zoomAction(event: Pick<KeyboardEvent, "key" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey" | "isComposing">, mac: boolean): ZoomAction | null {
  if (event.isComposing || event.altKey || (mac ? !event.metaKey || event.ctrlKey : !event.ctrlKey || event.metaKey)) return null;
  if (event.key === "+" || event.key === "=") return "in";
  if (event.key === "-") return "out";
  if (event.key === "0" && !event.shiftKey) return "reset";
  return null;
}

export function routeZoomKey(event: KeyboardEvent, mac: boolean, request: (action: ZoomAction) => unknown) {
  const action = zoomAction(event, mac);
  if (!action) return;
  event.preventDefault();
  event.stopImmediatePropagation();
  if (!event.repeat) request(action);
}

export function createZoomController(options: {
  setZoom: (level: number) => Promise<void>;
  storage: () => ZoomStorage;
  onFailure: () => void;
  onApplied: () => void;
}) {
  let level = 1;
  let initialized = false;
  let queue = Promise.resolve();
  function save() {
    try { options.storage().setItem(ZOOM_STORAGE_KEY, String(level)); } catch { /* Session zoom still works. */ }
  }
  async function apply(next: number) {
    try {
      await options.setZoom(next);
      level = next;
      save();
      options.onApplied();
    } catch { options.onFailure(); }
  }
  function initialize() {
    if (!initialized) {
      initialized = true;
      let saved = 1;
      try {
        const value = options.storage().getItem(ZOOM_STORAGE_KEY);
        if (value !== null && ZOOM_LEVELS.some(level => String(level) === value)) saved = Number(value);
      } catch { /* Missing storage defaults to 100%. */ }
      queue = queue.then(() => apply(saved));
    }
    return queue;
  }
  function request(action: ZoomAction) {
    initialize();
    queue = queue.then(async () => {
      const index = ZOOM_LEVELS.findIndex(value => value === level);
      const next = action === "reset" ? 1 : ZOOM_LEVELS[Math.max(0, Math.min(ZOOM_LEVELS.length - 1, index + (action === "in" ? 1 : -1)))];
      if (next !== level || action === "reset") await apply(next);
    });
    return queue;
  }
  return { initialize, request, get level() { return level; } };
}
