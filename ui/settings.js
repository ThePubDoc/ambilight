import { $, fromHex, invoke, ledCount, listen, saveConfig, summarize, toHex } from "./common.js";

const BAUD_RATES = [115200, 230400, 460800, 500000, 921600, 1000000, 2000000];
const SIDES = ["top", "right", "bottom", "left"];

let config;
let status;
let ports = [];

function update(patch) {
  config = { ...config, ...patch };
  render();
  saveConfig(config);
}

function render() {
  for (const side of SIDES) {
    const input = document.querySelector(`[data-side="${side}"] input`);
    if (document.activeElement !== input) input.value = config[side];
  }
  $("total").textContent = `${ledCount(config)} LEDs`;
  $("start").value = config.start;
  $("direction").value = config.direction;
  $("smoothing").value = config.smoothingMs;
  $("smoothingValue").textContent = `${Math.round(config.smoothingMs)} ms`;
  $("blackBars").checked = config.ignoreBlackBars;
  $("color").value = toHex(config.color);
  renderPorts();
  $("baud").value = String(config.baud);
  renderStatus();
  drawPreview();
}

function renderPorts() {
  const select = $("port");
  const options = [{ path: "", label: "Automatic" }, ...ports];
  if (config.port && !ports.some((p) => p.path === config.port)) {
    options.push({ path: config.port, label: `${config.port} (not connected)` });
  }
  select.replaceChildren(...options.map((p) => new Option(p.label, p.path)));
  select.value = config.port;
}

function renderStatus() {
  const s = summarize(config, status);
  $("dot").className = `dot ${s.tone}`;
  $("statusText").textContent = s.text;
  $("permissionRow").classList.toggle("hidden", !s.needsPermission);
}

/** Screen outline with one dot per LED, hue following wiring order, LED 1 ringed. */
async function drawPreview() {
  const dots = await invoke("layout_preview", { config });
  const canvas = $("preview");
  const dpr = window.devicePixelRatio || 1;
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  canvas.width = w * dpr;
  canvas.height = h * dpr;
  const ctx = canvas.getContext("2d");
  ctx.scale(dpr, dpr);

  const margin = 14;
  const aspect = 21 / 9;
  let sw = w - margin * 2;
  let sh = sw / aspect;
  if (sh > h - margin * 2) { sh = h - margin * 2; sw = sh * aspect; }
  const x0 = (w - sw) / 2;
  const y0 = (h - sh) / 2;

  const dark = matchMedia("(prefers-color-scheme: dark)").matches;
  ctx.fillStyle = dark ? "#111" : "#e5e5ea";
  ctx.strokeStyle = dark ? "#48484a" : "#c7c7cc";
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.roundRect(x0, y0, sw, sh, 6);
  ctx.fill();
  ctx.stroke();

  const n = dots.length;
  const r = Math.max(1.5, Math.min(3.5, (sw * 2 + sh * 2) / Math.max(n, 1) / 3));
  const out = 7;
  const pos = (d) => [
    x0 + d.x * sw + (d.x === 0 ? -out : d.x === 1 ? out : 0),
    y0 + d.y * sh + (d.y === 0 ? -out : d.y === 1 ? out : 0),
  ];
  dots.forEach((d, i) => {
    const [x, y] = pos(d);
    ctx.fillStyle = `hsl(${(i / n) * 300}, 85%, 55%)`;
    ctx.beginPath();
    ctx.arc(x, y, r, 0, Math.PI * 2);
    ctx.fill();
  });
  if (n) {
    const [x, y] = pos(dots[0]);
    ctx.strokeStyle = dark ? "#fff" : "#1d1d1f";
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.arc(x, y, r + 3.5, 0, Math.PI * 2);
    ctx.stroke();
  }
}

function buildSteppers() {
  for (const el of document.querySelectorAll(".stepper")) {
    const side = el.dataset.side;
    const minus = Object.assign(document.createElement("button"), { textContent: "−", ariaLabel: `Fewer ${side} LEDs` });
    const plus = Object.assign(document.createElement("button"), { textContent: "+", ariaLabel: `More ${side} LEDs` });
    const input = Object.assign(document.createElement("input"), { type: "number", min: 0, max: 500 });
    input.setAttribute("aria-label", `${side} LEDs`);
    const set = (v) => update({ [side]: Math.max(0, Math.min(500, Math.round(Number(v) || 0))) });
    minus.addEventListener("click", () => set(config[side] - 1));
    plus.addEventListener("click", () => set(config[side] + 1));
    input.addEventListener("change", () => set(input.value));
    el.append(minus, input, plus);
  }
}

function bind() {
  buildSteppers();
  $("baud").replaceChildren(...BAUD_RATES.map((b) => new Option(`${b.toLocaleString()} baud`, String(b))));
  $("start").addEventListener("change", (e) => update({ start: e.target.value }));
  $("direction").addEventListener("change", (e) => update({ direction: e.target.value }));
  $("identify").addEventListener("click", () => invoke("identify"));
  $("smoothing").addEventListener("input", (e) => update({ smoothingMs: Number(e.target.value) }));
  $("blackBars").addEventListener("change", (e) => update({ ignoreBlackBars: e.target.checked }));
  $("color").addEventListener("input", (e) => update({ color: fromHex(e.target.value) }));
  $("port").addEventListener("change", (e) => update({ port: e.target.value }));
  $("refresh").addEventListener("click", refreshPorts);
  $("baud").addEventListener("change", (e) => update({ baud: Number(e.target.value) }));
  $("reconnect").addEventListener("click", () => invoke("reconnect"));
  $("permission").addEventListener("click", () => invoke("open_permission_settings"));
  $("autostart").addEventListener("change", async (e) => {
    e.target.checked = await invoke("set_autostart", { enabled: e.target.checked });
  });
  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", drawPreview);
  window.addEventListener("resize", drawPreview);
}

async function refreshPorts() {
  ports = await invoke("list_ports");
  renderPorts();
}

async function init() {
  const state = await invoke("get_state");
  ({ config, status } = state);
  $("footer").textContent = `Ambilight ${state.version} · MIT License · github.com/ThePubDoc/ambilight`;
  bind();
  ports = await invoke("list_ports");
  $("autostart").checked = await invoke("get_autostart");
  render();
  listen("status", (e) => { status = e.payload; renderStatus(); });
  listen("config", (e) => { config = e.payload; render(); });
}

init();
