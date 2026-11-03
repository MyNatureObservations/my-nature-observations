// Center within the app, retaining a small margin on narrow or low windows.
export function weatherPanelPosition(_anchor: { right: number; top: number; bottom: number }, width: number, height: number, viewport: { width: number; height: number }) {
  const margin = 16;
  const left = Math.max(margin, (viewport.width - width) / 2);
  const top = Math.max(margin, Math.min(80, viewport.height - height - margin));
  return { left, top };
}
