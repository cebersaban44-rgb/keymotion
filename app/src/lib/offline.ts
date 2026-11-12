// What the app shows while no keyboard is connected: the model it supports,
// where the list of keyboards lives, and a stand-in keyboard for the pages
// that work without one (tools and the screen library).

import type { GeometryInfo } from "../bindings/GeometryInfo";
import type { KeyboardInfo } from "../bindings/KeyboardInfo";
// Generated from Rust by `cargo test` (dto.rs export_mock_geometry).
import ak980Geometry from "../bindings/geometry-ajazz-ak980-pro.json";

/** The keyboard the app supports today (docs/destek-haritasi.md). */
export const SUPPORTED_MODEL = "AJAZZ AK980 PRO";

/** The supported keyboards and what works in each connection mode. */
export const SUPPORT_LIST_URL = "https://github.com/cebersaban44-rgb/keymotion/blob/develop/docs/destek-haritasi.md";

/**
 * Stand-in for the supported model while nothing is connected. No channel
 * is open and no feature is usable, so every page that gets it shows its
 * "connect the cable" state and sends nothing; the drawing and the screen
 * library (stored per model) still work.
 */
export const OFFLINE_KEYBOARD: KeyboardInfo = {
  key: "offline",
  modelId: "ajazz-ak980-pro",
  modelName: SUPPORTED_MODEL,
  vendor: "AJAZZ",
  layout: "98",
  keyCount: 97,
  hasKnob: true,
  hasSideLight: true,
  screen: { width: 240, height: 135, verified: false },
  connection: "usb",
  product: null,
  firmware: null,
  features: [],
  modes: [],
  hasCommandChannel: false,
  hasDisplayChannel: false,
  bleName: null,
  bleAddress: null,
  battery: null,
  geometry: ak980Geometry as GeometryInfo,
};
