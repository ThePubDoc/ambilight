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

/** The mode actually shown, after app profiles and the streaming switch (reported by the engine). */
export const effectiveMode = (config, status) => status.effectiveMode ?? config.mode;
const effectiveEffect = (config, status) => status.effectiveEffect ?? config.effect;

export const MODE_NAMES = { screen: "Screen", screenSound: "Screen + Sound", sound: "Sound", color: "Color", effect: "Effect" };
export const EFFECT_NAMES = { rainbow: "Rainbow", spectrum: "Spectrum", breathing: "Breathing", candle: "Candle", aurora: "Aurora" };

const capturesScreen = (mode) => mode === "screen" || mode === "screenSound";
const needsAudio = (config, status, mode) =>
  mode === "sound" || mode === "screenSound" || (mode === "effect" && effectiveEffect(config, status) === "spectrum");

/** Dot color and one-line text describing the current state. */
export function summarize(config, status) {
  const leds = `${ledCount(config)} LEDs`;
  if (status.linkError) return { tone: "orange", text: status.linkError };
  if (!status.port) return { tone: "gray", text: "Connecting…" };
  if (!config.enabled) return { tone: "gray", text: `Off · ${leds}` };
  if (status.scheduled && config.schedule.dimTo === 0) return { tone: "gray", text: "Off for the night schedule" };

  const mode = effectiveMode(config, status);
  if (capturesScreen(mode)) {
    const capture = status.capture;
    if (capture.state === "needsPermission") {
      return { tone: "orange", text: "Needs Screen Recording permission", needsPermission: "screen" };
    }
    if (capture.state === "failed") return { tone: "orange", text: `Screen capture: ${capture.message}` };
    if (capture.state !== "running") return { tone: "yellow", text: "Starting screen capture…" };
  }
  if (needsAudio(config, status, mode)) {
    const audio = status.audio;
    if (audio.state === "needsPermission") {
      return { tone: "orange", text: "Needs System Audio Recording permission", needsPermission: "audio" };
    }
    if (audio.state === "failed") return { tone: "orange", text: `Sound: ${audio.message}` };
    if (audio.state !== "running") return { tone: "yellow", text: "Starting sound capture…" };
  }
  const name = mode === "effect" ? EFFECT_NAMES[effectiveEffect(config, status)] : MODE_NAMES[mode];
  if (status.autoSwitchedFor) {
    return { tone: "green", text: `${name} while ${status.autoSwitchedFor} is in front` };
  }
  if (status.profileFor) return { tone: "green", text: `${name} for ${status.profileFor}` };
  if (status.scheduled) return { tone: "green", text: `Dimmed for the night schedule · ${leds}` };
  return { tone: "green", text: `Connected to ${status.port} · ${leds}` };
}

/** Where to switch on the missing permission (both live in the same Privacy pane on macOS). */
export function permissionHint(kind) {
  const list = kind === "audio" ? "System Audio Recording Only" : "Screen & System Audio Recording";
  return `Switch Ambilight on under ${list}. If it isn't listed, click + and add it from Applications.`;
}

export const toHex = ({ r, g, b }) =>
  "#" + [r, g, b].map((v) => Math.round(Math.min(1, Math.max(0, v)) * 255).toString(16).padStart(2, "0")).join("");

export const fromHex = (hex) => ({
  r: parseInt(hex.slice(1, 3), 16) / 255,
  g: parseInt(hex.slice(3, 5), 16) / 255,
  b: parseInt(hex.slice(5, 7), 16) / 255,
});
