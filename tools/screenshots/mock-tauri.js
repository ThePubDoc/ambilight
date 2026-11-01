// Stands in for the Tauri bridge so the UI pages can be rendered in a plain browser for
// screenshots. Loaded before the page's own scripts by tools/screenshots/capture.sh.
(() => {
  const params = new URLSearchParams(location.search);
  const mode = params.get("mode") ?? "screenSound";

  const config = {
    enabled: true,
    mode,
    effect: params.get("effect") ?? "aurora",
    brightness: 0.8,
    color: { r: 1, g: 0.55, b: 0.2 },
    smoothingMs: 150,
    ignoreBlackBars: true,
    edgeDepth: 8,
    whiteBalance: { r: 1, g: 0.92, b: 0.85 },
    maxCurrentA: 6,
    beatSensitivity: "low",
    soundProfile: "glow",
    top: 49, right: 21, bottom: 48, left: 21,
    start: "bottomRight",
    direction: "anticlockwise",
    offset: 0,
    bottomGap: 0,
    display: 0,
    output: "serial",
    port: "",
    baud: 1000000,
    wledHost: "",
    autoSwitchServices: ["netflix", "prime", "disney", "hotstar", "appletv", "hulu"],
    autoSwitchMode: "sound",
    appProfiles: [
      { app: "com.valvesoftware.steam", name: "Steam", mode: "screenSound", effect: "rainbow" },
      { app: "com.spotify.client", name: "Spotify", mode: "sound", effect: "rainbow" },
    ],
    schedule: { enabled: true, from: 1410, until: 420, dimTo: 0 },
    shortcuts: true,
    autoUpdate: true,
  };
  const status = {
    port: "cu.usbserial-1110",
    linkError: null,
    capture: { state: "running" },
    audio: { state: "running" },
    autoSwitchedFor: params.get("streaming"),
    profileFor: params.get("profile"),
    effectiveMode: params.get("streaming") ? "sound" : mode,
    effectiveEffect: config.effect,
    scheduled: false,
  };

  // Same algorithm as core/src/layout.rs, for the settings preview.
  function layoutPreview(c) {
    const cw = [];
    for (let i = 0; i < c.top; i++) cw.push({ x: (i + 0.5) / c.top, y: 0 });
    for (let i = 0; i < c.right; i++) cw.push({ x: 1, y: (i + 0.5) / c.right });
    for (let i = c.bottom - 1; i >= 0; i--) cw.push({ x: (i + 0.5) / c.bottom, y: 1 });
    for (let i = c.left - 1; i >= 0; i--) cw.push({ x: 0, y: (i + 0.5) / c.left });
    const corner = { topLeft: 0, topRight: c.top, bottomRight: c.top + c.right, bottomLeft: c.top + c.right + c.bottom }[c.start];
    const rotated = cw.slice(corner).concat(cw.slice(0, corner));
    return c.direction === "anticlockwise" ? rotated.reverse() : rotated;
  }

  const responses = {
    get_state: () => ({ config, status, version: "0.2.1", update: null, platform: "macos" }),
    layout_preview: (args) => layoutPreview(args.config),
    streaming_services: () => [
      ["netflix", "Netflix"], ["prime", "Prime Video"], ["disney", "Disney+"], ["hotstar", "JioHotstar"],
      ["appletv", "Apple TV+"], ["hulu", "Hulu"],
    ].map(([key, label]) => ({ key, label })),
    list_displays: () => [{ id: 1, label: "External display 1 (3440 × 1440)", main: true }],
    list_ports: () => [{ path: "/dev/cu.usbserial-1110", label: "cu.usbserial-1110 (USB Serial)" }],
    running_apps: () => [{ id: "com.apple.Music", name: "Music" }],
    get_autostart: () => true,
  };

  window.__TAURI__ = {
    core: { invoke: async (cmd, args) => (responses[cmd] ? responses[cmd](args) : null) },
    event: { listen: async () => () => {} },
    window: { getCurrentWindow: () => ({ hide() {} }) },
  };
})();
