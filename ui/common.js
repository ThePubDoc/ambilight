// Shared between the popover and the settings window.

export const { invoke } = window.__TAURI__.core;
export const { listen } = window.__TAURI__.event;
export const isMac = navigator.userAgent.includes("Mac");

export const $ = (id) => document.getElementById(id);

export const ledCount = (c) => c.top + c.right + c.bottom + c.left;

/** Saves config, coalescing rapid changes (slider drags) into one write per ~50 ms. */
let pending = null;
let timer = null;
export function saveConfig(config) {
  pending = config;
  if (timer) return;
  timer = setTimeout(() => {
    timer = null;
    invoke("set_config", { config: pending });
  }, 50);
}

/** Dot color and one-line text describing the current state. */
export function summarize(config, status) {
  const leds = `${ledCount(config)} LEDs`;
  if (status.linkError) return { tone: "orange", text: status.linkError };
  if (!status.port) return { tone: "gray", text: "Connecting…" };
  if (!config.enabled) return { tone: "gray", text: `Off · ${leds}` };
  if (config.mode === "screen") {
    const capture = status.capture;
    if (capture.state === "needsPermission") {
      return { tone: "orange", text: "Needs Screen Recording permission", needsPermission: true };
    }
    if (capture.state === "failed") return { tone: "orange", text: `Screen capture: ${capture.message}` };
    if (capture.state !== "running") return { tone: "yellow", text: "Starting screen capture…" };
  }
  return { tone: "green", text: `Connected to ${status.port} · ${leds}` };
}

export const toHex = ({ r, g, b }) =>
  "#" + [r, g, b].map((v) => Math.round(Math.min(1, Math.max(0, v)) * 255).toString(16).padStart(2, "0")).join("");

export const fromHex = (hex) => ({
  r: parseInt(hex.slice(1, 3), 16) / 255,
  g: parseInt(hex.slice(3, 5), 16) / 255,
  b: parseInt(hex.slice(5, 7), 16) / 255,
});
