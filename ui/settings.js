import { $, EFFECT_NAMES, fromHex, invoke, isMac, ledCount, listen, MODE_NAMES, permissionHint, saveConfig, summarize, toHex } from "./common.js";

const BAUD_RATES = [115200, 230400, 460800, 500000, 921600, 1000000, 2000000];

let config;
let status;
let ports = [];
let displays = [];
let services = [];
let available = null; // update info
let apps = []; // running apps that can get a profile

function update(patch) {
  config = { ...config, ...patch };
  render();
  saveConfig(config);
}

const minutesToTime = (m) => `${String(Math.floor(m / 60)).padStart(2, "0")}:${String(m % 60).padStart(2, "0")}`;
const timeToMinutes = (t) => {
  const [h, m] = t.split(":").map(Number);
  return h * 60 + m;
};

function render() {
  for (const el of document.querySelectorAll(".stepper input")) {
    if (document.activeElement !== el) el.value = config[el.dataset.field];
  }
  $("total").textContent = `${ledCount(config)} LEDs`;
  $("start").value = config.start;
  $("direction").value = config.direction;

  renderDisplays();
  $("smoothing").value = config.smoothingMs;
  $("smoothingValue").textContent = `${Math.round(config.smoothingMs)} ms`;
  $("edgeDepth").value = config.edgeDepth;
  $("edgeDepthValue").textContent = `${Math.round(config.edgeDepth)}%`;
  $("blackBars").checked = config.ignoreBlackBars;
  $("beatSensitivity").value = config.beatSensitivity;
  $("color").value = toHex(config.color);

  for (const input of document.querySelectorAll(".wb input")) {
    const v = Math.round(config.whiteBalance[input.dataset.channel] * 100);
    if (document.activeElement !== input) input.value = v;
    input.nextElementSibling.textContent = `${v}%`;
  }

  if (document.activeElement !== $("maxCurrent")) $("maxCurrent").value = config.maxCurrentA;
  const fullWhite = (ledCount(config) * 0.061).toFixed(1);
  $("powerEstimate").textContent = config.maxCurrentA > 0
    ? `Full white would draw about ${fullWhite} A; Ambilight keeps it under ${(config.maxCurrentA * 0.9).toFixed(1)} A.`
    : `Full white would draw about ${fullWhite} A at 100% brightness.`;

  renderProfiles();
  $("autoSwitchMode").value = config.autoSwitchMode;
  for (const box of $("services").querySelectorAll("input")) box.checked = config.autoSwitchServices.includes(box.value);

  $("scheduleOn").checked = config.schedule.enabled;
  $("scheduleFrom").value = minutesToTime(config.schedule.from);
  $("scheduleUntil").value = minutesToTime(config.schedule.until);
  $("scheduleDim").value = String(config.schedule.dimTo);
  for (const id of ["scheduleFrom", "scheduleUntil", "scheduleDim"]) $(id).disabled = !config.schedule.enabled;

  $("output").value = config.output;
  for (const el of document.querySelectorAll(".serial-only")) el.classList.toggle("hidden", config.output !== "serial");
  for (const el of document.querySelectorAll(".wled-only")) el.classList.toggle("hidden", config.output !== "wled");
  renderPorts();
  $("baud").value = String(config.baud);
  if (document.activeElement !== $("wledHost")) $("wledHost").value = config.wledHost;

  $("shortcuts").checked = config.shortcuts;
  $("autoUpdate").checked = config.autoUpdate;
  renderStatus();
  drawPreview();
}

/** Mode options for a profile: every mode, with Effect split into its effects. */
const PROFILE_CHOICES = [
  ...["screen", "screenSound", "sound", "color"].map((mode) => ({ value: mode, label: MODE_NAMES[mode], mode })),
  ...Object.entries(EFFECT_NAMES).map(([effect, label]) => ({ value: `effect:${effect}`, label: `Effect: ${label}`, mode: "effect", effect })),
];

function renderProfiles() {
  const list = $("profiles");
  if (list.contains(document.activeElement)) return; // don't rebuild under the user's hand
  list.replaceChildren(
    ...config.appProfiles.map((profile, i) => {
      const row = document.createElement("div");
      row.className = "row";
      const name = Object.assign(document.createElement("span"), { className: "label", textContent: profile.name });
      const select = document.createElement("select");
      select.setAttribute("aria-label", `Mode for ${profile.name}`);
      select.replaceChildren(...PROFILE_CHOICES.map((c) => new Option(c.label, c.value)));
      select.value = profile.mode === "effect" ? `effect:${profile.effect}` : profile.mode;
      select.addEventListener("change", () => {
        const choice = PROFILE_CHOICES.find((c) => c.value === select.value);
        const profiles = config.appProfiles.map((p, j) =>
          j === i ? { ...p, mode: choice.mode, effect: choice.effect ?? p.effect } : p);
        update({ appProfiles: profiles });
      });
      const remove = Object.assign(document.createElement("button"), { className: "plain", textContent: "Remove" });
      remove.addEventListener("click", () => update({ appProfiles: config.appProfiles.filter((_, j) => j !== i) }));
      const controls = document.createElement("div");
      controls.className = "controls";
      controls.append(select, remove);
      row.append(name, controls);
      return row;
    }),
  );
  renderAddApp();
}

function renderAddApp() {
  const taken = new Set(config.appProfiles.map((p) => p.app.toLowerCase()));
  const choices = apps.filter((a) => !taken.has(a.id.toLowerCase()));
  const select = $("addApp");
  select.replaceChildren(
    choices.length ? new Option("Choose a running app…", "") : new Option("No other apps running", ""),
    ...choices.map((a) => new Option(a.name, a.id)),
  );
  $("addProfile").disabled = !choices.length;
}

function renderDisplays() {
  const select = $("display");
  const options = [new Option("Main display", "0"), ...displays.map((d) => new Option(d.label + (d.main ? " — main" : ""), String(d.id)))];
  select.replaceChildren(...options);
  select.value = displays.some((d) => d.id === config.display) ? String(config.display) : "0";
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
  $("permissionTitle").textContent = s.needsPermission === "audio" ? "System Audio Recording permission" : "Screen Recording permission";
  $("permissionHint").textContent = `${permissionHint(s.needsPermission)} No restart needed.`;
}

function renderUpdate(text) {
  $("updateStatus").textContent = text ?? (available ? `Ambilight ${available.version} is available.` : "");
  $("installUpdate").classList.toggle("hidden", !available);
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
    const field = el.dataset.field;
    const min = Number(el.dataset.min ?? 0);
    const minus = Object.assign(document.createElement("button"), { textContent: "−", ariaLabel: "Less" });
    const plus = Object.assign(document.createElement("button"), { textContent: "+", ariaLabel: "More" });
    const input = Object.assign(document.createElement("input"), { type: "number", min, max: 500 });
    input.dataset.field = field;
    input.setAttribute("aria-label", field);
    const set = (v) => update({ [field]: Math.max(min, Math.min(500, Math.round(Number(v) || 0))) });
    minus.addEventListener("click", () => set(config[field] - 1));
    plus.addEventListener("click", () => set(config[field] + 1));
    input.addEventListener("change", () => set(input.value));
    el.append(minus, input, plus);
  }
}

function buildServices() {
  $("services").replaceChildren(
    ...services.map((service) => {
      const row = document.createElement("div");
      row.className = "row";
      const id = `service-${service.key}`;
      const label = Object.assign(document.createElement("label"), { textContent: service.label, htmlFor: id });
      const sw = document.createElement("label");
      sw.className = "switch";
      const box = Object.assign(document.createElement("input"), { type: "checkbox", id, value: service.key });
      box.addEventListener("change", () => {
        const keys = new Set(config.autoSwitchServices);
        if (box.checked) keys.add(service.key); else keys.delete(service.key);
        update({ autoSwitchServices: [...keys] });
      });
      sw.append(box, document.createElement("span"));
      row.append(label, sw);
      return row;
    }),
  );
}

function bind() {
  buildSteppers();
  buildServices();
  const prefix = isMac ? "⌃⌥" : "Ctrl+Alt+";
  for (const el of document.querySelectorAll(".kbd")) el.textContent = prefix + el.dataset.key;

  $("start").addEventListener("change", (e) => update({ start: e.target.value }));
  $("direction").addEventListener("change", (e) => update({ direction: e.target.value }));
  $("identify").addEventListener("click", () => invoke("identify"));

  $("display").addEventListener("change", (e) => update({ display: Number(e.target.value) }));
  $("smoothing").addEventListener("input", (e) => update({ smoothingMs: Number(e.target.value) }));
  $("edgeDepth").addEventListener("input", (e) => update({ edgeDepth: Number(e.target.value) }));
  $("blackBars").addEventListener("change", (e) => update({ ignoreBlackBars: e.target.checked }));
  $("beatSensitivity").addEventListener("change", (e) => update({ beatSensitivity: e.target.value }));
  $("color").addEventListener("input", (e) => update({ color: fromHex(e.target.value) }));

  for (const input of document.querySelectorAll(".wb input")) {
    input.addEventListener("input", () =>
      update({ whiteBalance: { ...config.whiteBalance, [input.dataset.channel]: Number(input.value) / 100 } }));
  }
  $("wbReset").addEventListener("click", () => update({ whiteBalance: { r: 1, g: 1, b: 1 } }));
  $("testWhite").addEventListener("click", () => invoke("test_white"));

  $("maxCurrent").addEventListener("change", (e) => update({ maxCurrentA: Math.max(0, Number(e.target.value) || 0) }));

  $("autoSwitchMode").addEventListener("change", (e) => update({ autoSwitchMode: e.target.value }));

  $("refreshApps").addEventListener("click", refreshApps);
  $("addApp").addEventListener("focus", refreshApps);
  $("addProfile").addEventListener("click", () => {
    const app = apps.find((a) => a.id === $("addApp").value);
    if (!app) return;
    update({ appProfiles: [...config.appProfiles, { app: app.id, name: app.name, mode: "screenSound", effect: "rainbow" }] });
  });

  const schedule = (patch) => update({ schedule: { ...config.schedule, ...patch } });
  $("scheduleOn").addEventListener("change", (e) => schedule({ enabled: e.target.checked }));
  $("scheduleFrom").addEventListener("change", (e) => e.target.value && schedule({ from: timeToMinutes(e.target.value) }));
  $("scheduleUntil").addEventListener("change", (e) => e.target.value && schedule({ until: timeToMinutes(e.target.value) }));
  $("scheduleDim").addEventListener("change", (e) => schedule({ dimTo: Number(e.target.value) }));

  $("output").addEventListener("change", (e) => update({ output: e.target.value }));
  $("baud").replaceChildren(...BAUD_RATES.map((b) => new Option(`${b.toLocaleString()} baud`, String(b))));
  $("port").addEventListener("change", (e) => update({ port: e.target.value }));
  $("refresh").addEventListener("click", refreshPorts);
  $("baud").addEventListener("change", (e) => update({ baud: Number(e.target.value) }));
  $("wledHost").addEventListener("change", (e) => update({ wledHost: e.target.value.trim() }));
  $("reconnect").addEventListener("click", () => invoke("reconnect"));
  $("permission").addEventListener("click", () => invoke("open_permission_settings"));

  $("shortcuts").addEventListener("change", (e) => update({ shortcuts: e.target.checked }));
  $("autostart").addEventListener("change", async (e) => {
    e.target.checked = await invoke("set_autostart", { enabled: e.target.checked });
  });
  $("autoUpdate").addEventListener("change", (e) => update({ autoUpdate: e.target.checked }));
  $("checkUpdate").addEventListener("click", async (e) => {
    e.target.disabled = true;
    renderUpdate("Checking…");
    try {
      available = await invoke("check_update");
      renderUpdate(available ? undefined : "You're up to date.");
    } catch (err) {
      renderUpdate(`Couldn't check for updates: ${err}`);
    }
    e.target.disabled = false;
  });
  $("installUpdate").addEventListener("click", async (e) => {
    e.target.disabled = true;
    renderUpdate(`Installing ${available.version}… Ambilight restarts when it's done.`);
    try {
      await invoke("install_update");
    } catch (err) {
      renderUpdate(`Update failed: ${err}`);
      e.target.disabled = false;
    }
  });

  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", drawPreview);
  window.addEventListener("resize", drawPreview);
}

async function refreshApps() {
  apps = await invoke("running_apps");
  const selected = $("addApp").value;
  renderAddApp();
  $("addApp").value = selected;
}

async function refreshPorts() {
  ports = await invoke("list_ports");
  renderPorts();
}

async function init() {
  const state = await invoke("get_state");
  ({ config, status, update: available } = state);
  services = await invoke("streaming_services");
  displays = await invoke("list_displays");
  ports = await invoke("list_ports");
  apps = await invoke("running_apps");
  $("footer").textContent = `Ambilight ${state.version} · MIT License · github.com/ThePubDoc/ambilight`;
  bind();
  $("autostart").checked = await invoke("get_autostart");
  render();
  renderUpdate();
  listen("status", (e) => { status = e.payload; renderStatus(); });
  listen("config", (e) => { config = e.payload; render(); });
  listen("update", (e) => { available = e.payload; renderUpdate(); });
}

init();
