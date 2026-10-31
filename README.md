# Ambilight

Screen-synced lighting for your monitor. Ambilight reads the edges of your screen and mirrors them onto an LED strip stuck to the back of the display, for macOS and Windows.

<p align="center"><img src="icons/app-icon.png" width="128" alt="Ambilight icon"></p>

- **Lives in the menu bar / system tray.** One click opens a small panel: on/off, mode, brightness. No browser, no local web server.
- **Five modes.** **Screen** follows what's on the display. **Mix** uses the screen's colors but pulses with the audio, which is great for games and music videos. **Sound** follows the music or movie you're playing. **Color** holds one color (ten presets or any custom color). **Effect** plays an animation: Rainbow, Spectrum (bass glows along the bottom, treble across the top), Breathing, Candle or Aurora.
- **Works with Netflix and other streaming.** Streaming sites black out their video while the screen is captured, so when a Netflix, Prime Video, Disney+ (and others) window is in front, Ambilight switches to Sound mode by itself: the video stays visible and the lights follow the soundtrack. YouTube and everything else keep screen sync. See [Streaming video](#streaming-video).
- **Fits any strip layout.** Set LEDs per side, which corner the strip starts at and which way it runs. A live preview shows the result, and **Identify LEDs** lights each side in its own color so you can check it on the real strip.
- **Ignores black bars.** Letterboxed and pillarboxed videos use the picture area, not the bars.
- **Looks right on cheap strips.** White balance sliders fix a blue or green tint, and the edge depth slider sets how far into the picture each LED looks.
- **Protects your power supply.** Enter the adapter's rating and bright scenes are dimmed just enough to stay under 90% of it.
- **Keyboard shortcuts** work in any app: ⌃⌥L / Ctrl+Alt+L toggles the lights, ⌃⌥M / Ctrl+Alt+M cycles modes, ⌃⌥↑↓ / Ctrl+Alt+↑↓ changes brightness.
- **Night schedule.** Turn the lights off or dim them every night between two times.
- **USB or Wi-Fi.** Drives an Adalight controller over USB, or a [WLED](https://kno.wled.ge) controller over the network.
- **Multiple displays.** Pick which monitor to follow.
- **Updates itself.** Checks GitHub for new releases and installs them in one click.
- **Recovers by itself.** Reconnects when the USB cable is unplugged and plugged back in, restarts capture after the computer sleeps, and turns the strip off when the app quits (and, on macOS, while the display sleeps).
- **Small and light.** About 7 MB. The screen is captured at low resolution (about 320 px wide on macOS, a GPU-downscaled frame on Windows), so it barely uses the CPU.

## What you need

- **An LED strip** with individually addressable LEDs (WS2812B, SK6812 and similar), stuck around the back of the screen.
- **A controller**, either:
  - **USB with Adalight firmware**: any board and sketch that speaks the Adalight serial protocol (`Ada` header, then RGB bytes). An Arduino Nano or ESP8266/ESP32 with a FastLED-based Adalight sketch is the usual choice. Set the sketch's LED count and baud rate to match your strip, and use the same baud rate in Ambilight.
  - **Wi-Fi with [WLED](https://kno.wled.ge)** on an ESP32/ESP8266. Set its LED count to match your layout, then choose **Wi-Fi (WLED)** under **Settings → Connection** and enter its IP address or name. Ambilight uses WLED's realtime UDP protocol (port 21324), so no extra setup is needed in WLED.
- **A separate 5 V power supply for the strip.** USB can't power more than a handful of LEDs. Budget up to 60 mA per LED at full white (139 LEDs ≈ 8 A), less if you keep brightness lower. Connect the supply's ground to the controller's ground.

Wiring: controller data pin → strip **DIN** (the end where the printed arrows point away from), 5 V and GND from the power supply to the strip, and a shared ground between supply and controller.

## Install

Download the latest installer from [Releases](https://github.com/ThePubDoc/ambilight/releases):

- **macOS 14.2 or later**: `Ambilight_<version>_aarch64.dmg` for Apple silicon, `Ambilight_<version>_x64.dmg` for Intel Macs. Open the DMG and drag Ambilight to Applications.
- **Windows 10/11**: `Ambilight_<version>_x64-setup.exe`.

### First launch on macOS

The app isn't notarized by Apple, so:

1. **Open it:** the first time, right-click Ambilight in Applications and choose **Open**, then **Open** again. If macOS refuses, go to **System Settings → Privacy & Security** and click **Open Anyway**.
2. **Allow screen recording:** open **System Settings → Privacy & Security → Screen & System Audio Recording**. If Ambilight is listed, switch it on. If it isn't (common for apps without an Apple Developer ID), click **+** and add it from Applications. Ambilight notices within a few seconds; no restart needed.
3. **For Sound mode, allow system audio:** the first time Sound mode runs, macOS asks whether Ambilight may record system audio; click **Allow**. If you missed it, the same settings page has a **System Audio Recording Only** list where you can switch Ambilight on (or add it with **+**). Ambilight only measures how loud the audio is; it never records or stores it, and it doesn't use the microphone.

Updating to a new version (including through the built-in updater) may require switching the permissions on again.

### First launch on Windows

Windows SmartScreen may warn about an unknown publisher: click **More info → Run anyway**. Most controllers use a CH340 or CP210x USB chip. Windows usually installs the driver automatically; if the controller doesn't show up, install the driver from the chip maker.

> Windows support is new and hasn't been tested with real hardware yet. Reports are very welcome.

## Set up your strip

1. Plug in the controller and power the strip. Ambilight picks the first USB serial device automatically.
2. Open **Settings…** from the panel (or right-click the tray icon).
3. Under **Connection**, set **Speed** to your firmware's baud rate.
4. Under **LED layout**, enter the LED count for each side, the corner where LED 1 sits (the end wired to the controller), and the direction the strip runs, seen from the front of the screen.
5. Click **Identify LEDs**. For 8 seconds LED 1 lights white, the right side red, the top green, the left side blue and the bottom yellow. If a color appears on the wrong side, adjust the corner or direction. If LED 1 isn't at a corner, set **LED 1 offset**; if the bottom edge has no LEDs around the monitor stand, set **Gap at the bottom**.
6. Optional but recommended: under **White balance**, click **Show white** and lower any color that makes white look tinted; under **Power**, enter your adapter's amps.

Settings are saved as JSON in `~/Library/Application Support/com.thepubdoc.ambilight/` on macOS and `%APPDATA%\com.thepubdoc.ambilight\` on Windows.

## Streaming video

Netflix, Prime Video, Disney+ and other streaming services protect their video: while any app captures the screen, the browser shows black instead. No screen-capture ambilight can read that picture, so Ambilight steps aside instead.

It recognises the service from the title of the window in front (browsers title their windows after the active tab, e.g. "Netflix" or "… | Prime Video"). While such a window is in front, Ambilight stops capturing the screen, so the video plays normally, and shows **Sound** mode instead: the lights follow the soundtrack, pulsing with music and flashing on loud moments. Switch to another tab or app and screen sync resumes within half a second. This works in any browser and in the services' own apps.

In **Settings → Streaming video** you can choose another fallback (**Color** or **Effect**) and turn individual services off. This applies to both Screen and Mix modes. Supported: Netflix, Prime Video, Disney+, JioHotstar/JioCinema, Apple TV+, Hulu, Paramount+, Peacock, SonyLIV, ZEE5 and Crunchyroll.

Sound mode also works on its own: pick **Sound** in the panel to make the strip follow music.

## Troubleshooting

| Symptom | Likely cause |
|---|---|
| Status says **USB port is in use by another app** | Another LED app (Hyperion, Prismatik…) has the port. Quit it. |
| Status says **LED controller not found** | Check the USB cable, or pick the port manually under **Connection**. On Windows, check the USB driver. |
| Connected, but the strip stays dark | The strip's 5 V supply is unplugged or switched off, or the data wire is on the wrong end of the strip. |
| Only part of the strip lights | The LED counts add up to fewer LEDs than the strip (or the firmware) has. |
| Colors flicker or look random | Baud rate doesn't match the firmware, or the controller and supply don't share a ground. |
| Wrong colors (red shows as green…) | The firmware's color order doesn't match the strip. Fix it in the firmware sketch (e.g. `GRB` for WS2812B). |
| macOS: stuck on **Needs Screen Recording permission** | Add Ambilight with **+** in Screen & System Audio Recording (see above). |
| Netflix (or another streaming site) goes black | Check the service is switched on under **Settings → Streaming video**. If its tab title doesn't contain the service's name, the window can't be recognised; please open an issue with the title. |
| Sound mode only shows a dim glow | Nothing is playing, or (macOS) Ambilight isn't allowed under **System Audio Recording Only**. |
| Bright scenes look dimmer than expected | The **Power** limit is doing its job. Raise it only if your adapter is rated higher. |
| Shortcuts don't work | Another app already uses the same keys, or they're switched off under **Settings → Keyboard shortcuts**. |
| WLED: connected but nothing changes | Check the address, and that WLED's LED count matches the layout. WLED returns to its own effect 2 seconds after Ambilight stops sending. |

Logs are written to `~/Library/Logs/com.thepubdoc.ambilight/ambilight.log` on macOS and `%LOCALAPPDATA%\com.thepubdoc.ambilight\logs\ambilight.log` on Windows. Please attach the log when you open an issue.

## Build from source

You need [Rust](https://rustup.rs) and Node.js 18 or later, plus:

- **macOS:** Xcode or the Command Line Tools (`xcode-select --install`).
- **Windows:** [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the C++ workload. WebView2 ships with Windows 10/11.

```sh
npm install
npm run dev     # run a development build
npm run build   # installers in target/release/bundle/
cargo test -p ambilight-core
```

**macOS tip:** builds are ad-hoc signed, so macOS treats every rebuild as a new app and forgets the screen recording permission. Run `tools/setup-signing.sh` once to create a local self-signed certificate (in its own keychain), then `tools/sign-macos.sh` after each `npm run build`. The permission then survives rebuilds.

### Project layout

```
core/            Platform-independent engine (Rust library, unit-tested)
  src/layout.rs    LED positions from per-side counts, start corner and direction
  src/frame.rs     Edge sampling and black-bar detection
  src/pixels.rs    Gamma, brightness, white balance and the power limiter
  src/adalight.rs  Adalight protocol framing
  src/output.rs    USB (Adalight) and Wi-Fi (WLED UDP) outputs
  src/serial.rs    USB serial port discovery and opening
  src/effects.rs   Rainbow, Spectrum, Breathing, Candle and Aurora
  src/engine.rs    50 Hz loop: capture → smoothing → output; reconnects, sleep, schedule, auto-switch
  src/capture/     Screen capture: ScreenCaptureKit (macOS), DXGI Desktop Duplication (Windows)
  src/audio/       System-audio loudness: Core Audio process tap (macOS), WASAPI loopback (Windows)
  src/sound.rs     Sound mode visuals
  src/foreground.rs  Front window title, for the streaming auto-switch
src-tauri/       Desktop app: tray icon, windows, settings storage, start at login, shortcuts, updater
ui/              Popover and settings pages (plain HTML/CSS/JS, no build step)
tools/           Icon generation and local macOS signing helpers
```

### Releasing

Bump the version in `Cargo.toml`, `package.json` and `src-tauri/tauri.conf.json`, then push a tag like `v0.2.0`. The release workflow builds the macOS (Apple silicon and Intel) and Windows installers, plus signed update archives and `latest.json` for the in-app updater, and attaches them to a draft GitHub release. Publish the draft to ship it.

The updater archives are signed with the key in the `TAURI_SIGNING_PRIVATE_KEY` repository secret; its public half is in `tauri.conf.json`. Forks that publish their own builds need their own key (`npx tauri signer generate`).

## License

[MIT](LICENSE) © 2026 Aayush Agrawal
