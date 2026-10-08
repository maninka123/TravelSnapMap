// Test environment: jsdom plus the browser APIs it lacks, the demo backend over Tauri's IPC mocks, and a MapLibre
// stand-in (jsdom has no WebGL). Each test gets a fresh sample library.
import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach, beforeEach, vi } from "vitest";
import { installDemoBackend } from "../dev/demoBackend";

if (!window.matchMedia) {
  window.matchMedia = (query: string) => ({
    matches: false, media: query, onchange: null, addListener() {}, removeListener() {}, addEventListener() {}, removeEventListener() {}, dispatchEvent: () => false,
  }) as MediaQueryList;
}
class NoopObserver { observe() {} unobserve() {} disconnect() {} takeRecords() { return []; } }
globalThis.ResizeObserver ??= NoopObserver as unknown as typeof ResizeObserver;
globalThis.IntersectionObserver ??= NoopObserver as unknown as typeof IntersectionObserver;
Element.prototype.scrollIntoView ??= () => {};
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.releasePointerCapture ??= () => {};

vi.mock("maplibre-gl", () => {
  class Evented { on() { return this; } once() { return this; } off() { return this; } }
  class Map extends Evented {
    touchZoomRotate = { disableRotation() {} };
    addControl() { return this; } remove() {} getBounds() { return { contains: () => true }; } getZoom() { return 2; }
    getCenter() { return { lng: 0, lat: 0 }; } easeTo() {} fitBounds() {} jumpTo() {} setStyle() {} getSource() { return undefined; }
    getLayer() { return undefined; } setFilter() {} triggerRepaint() {} getCanvas() { return { style: {} }; } cameraForBounds() { return { zoom: 3 }; }
  }
  class Popup extends Evented { setLngLat() { return this; } setHTML() { return this; } setDOMContent() { return this; } addTo() { return this; } remove() {} }
  class Marker { setLngLat() { return this; } addTo() { return this; } remove() {} }
  class LngLatBounds { extend() { return this; } contains() { return true; } }
  class NavigationControl {}
  return { default: { Map, Popup, Marker, LngLatBounds, NavigationControl }, Map, Popup, Marker, LngLatBounds, NavigationControl };
});

// The services module decides "inside the desktop app" when it loads, so the IPC mock must exist first.
installDemoBackend();

beforeEach(() => {
  localStorage.clear();
  localStorage.setItem("onboarding.done", "1");
});
afterEach(() => cleanup());
