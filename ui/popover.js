import { $, invoke, isMac, listen, permissionHint, saveConfig, summarize } from "./common.js";

const SWATCHES = [
  { r: 1, g: 0.55, b: 0.2 }, // warm white
  { r: 1, g: 1, b: 1 },
  { r: 1, g: 0, b: 0 },
  { r: 1, g: 0.35, b: 0 },
  { r: 1, g: 0.85, b: 0 },
  { r: 0, g: 1, b: 0.2 },
  { r: 0, g: 0.85, b: 1 },
  { r: 0, g: 0.2, b: 1 },
  { r: 0.6, g: 0, b: 1 },
  { r: 1, g: 0, b: 0.6 },
];

let config;
let status;
let available = null; // update info, when a newer release exists

const sameColor = (a, b) => Math.abs(a.r - b.r) + Math.abs(a.g - b.g) + Math.abs(a.b - b.b) < 0.01;

function update(patch) {
  config = { ...config, ...patch };
  render();
  saveConfig(config);
}

function render() {
  $("enabled").checked = config.enabled;
  for (const b of $("mode").children) {
    b.classList.toggle("selected", b.dataset.mode === config.mode);
    b.disabled = !config.enabled;
  }
  const effectMode = config.mode === "effect";
  $("effect").classList.toggle("hidden", !effectMode);
  for (const b of $("effect").children) {
    b.classList.toggle("selected", b.dataset.effect === config.effect);
    b.disabled = !config.enabled;
  }
  // Color mode and the Breathing effect both use the chosen color.
  const usesColor = config.mode === "color" || (effectMode && config.effect === "breathing");
  $("swatches").classList.toggle("hidden", !usesColor);
  for (const [i, b] of [...$("swatches").children].entries()) {
    b.classList.toggle("selected", sameColor(SWATCHES[i], config.color));
    b.disabled = !config.enabled;
  }
  const brightness = $("brightness");
  if (document.activeElement !== brightness) brightness.value = Math.round(config.brightness * 100);
  brightness.disabled = !config.enabled;
  $("shortcutHint").textContent = config.shortcuts ? (isMac ? "⌃⌥L on/off" : "Ctrl+Alt+L on/off") : "";
  $("update").classList.toggle("hidden", !available);
  if (available && !$("installUpdate").disabled) $("updateText").textContent = `Ambilight ${available.version} is available`;
  renderStatus();
}

function renderStatus() {
  const s = summarize(config, status);
  $("dot").className = `dot ${s.tone}`;
  $("statusText").textContent = s.text;
  $("permission").classList.toggle("hidden", !s.needsPermission);
  $("permissionHint").textContent = permissionHint(s.needsPermission);
  fit();
}

function fit() {
  requestAnimationFrame(() => invoke("fit_popover", { height: Math.ceil($("main").getBoundingClientRect().height) }));
}

function bind() {
  $("enabled").addEventListener("change", (e) => update({ enabled: e.target.checked }));
  for (const b of $("mode").children) b.addEventListener("click", () => update({ mode: b.dataset.mode }));
  for (const b of $("effect").children) b.addEventListener("click", () => update({ effect: b.dataset.effect }));
  for (const color of SWATCHES) {
    const b = document.createElement("button");
    b.style.background = `rgb(${color.r * 255}, ${color.g * 255}, ${color.b * 255})`;
    b.setAttribute("aria-label", "Color");
    b.addEventListener("click", () => update({ color }));
    $("swatches").append(b);
  }
  $("brightness").addEventListener("input", (e) => update({ brightness: e.target.value / 100 }));
  $("permissionButton").addEventListener("click", () => invoke("open_permission_settings"));
  $("installUpdate").addEventListener("click", async (e) => {
    e.target.disabled = true;
    $("updateText").textContent = `Installing ${available.version}…`;
    try {
      await invoke("install_update"); // restarts the app when done
    } catch (err) {
      $("updateText").textContent = `Update failed: ${err}`;
      e.target.disabled = false;
    }
  });
  $("settings").addEventListener("click", () => invoke("open_settings"));
  $("quit").addEventListener("click", () => invoke("quit"));
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") window.__TAURI__.window.getCurrentWindow().hide();
  });
}

async function init() {
  ({ config, status, update: available } = await invoke("get_state"));
  bind();
  render();
  listen("status", (e) => { status = e.payload; renderStatus(); });
  listen("config", (e) => { config = e.payload; render(); });
  listen("update", (e) => { available = e.payload; render(); });
}

init();
