# Ambilight

Screen-synced lighting for your monitor. Ambilight reads the edges of your screen and mirrors them onto an LED strip stuck to the back of the display, for macOS and Windows.

<p align="center"><img src="icons/app-icon.png" width="128" alt="Ambilight icon"></p>

- **Lives in the menu bar / system tray.** One click opens a small panel: on/off, mode, brightness. No browser, no local web server.
- **Four modes.** **Screen** follows what's on the display, **Sound** follows the music or movie you're playing, **Color** holds one color (ten presets or any custom color), **Rainbow** cycles slowly.
- **Works with Netflix and other streaming.** Streaming sites black out their video while the screen is captured, so when a Netflix, Prime Video, Disney+ (and others) window is in front, Ambilight switches to Sound mode by itself: the video stays visible and the lights follow the soundtrack. YouTube and everything else keep screen sync. See [Streaming video](#streaming-video).
- **Fits any strip layout.** Set LEDs per side, which corner the strip starts at and which way it runs. A live preview shows the result, and **Identify LEDs** lights each side in its own color so you can check it on the real strip.
- **Ignores black bars.** Letterboxed and pillarboxed videos use the picture area, not the bars.
- **Recovers by itself.** Reconnects when the USB cable is unplugged and plugged back in, restarts capture after the computer sleeps, and turns the strip off when the app quits (and, on macOS, while the display sleeps).
- **Small and light.** About 7 MB. The screen is captured at low resolution (about 320 px wide on macOS, a GPU-downscaled frame on Windows), so it barely uses the CPU.

## What you need

- **An LED strip** with individually addressable LEDs (WS2812B, SK6812 and similar), stuck around the back of the screen.
- **A USB controller running Adalight firmware**: any board and sketch that speaks the Adalight serial protocol (`Ada` header, then RGB bytes). An Arduino Nano or ESP8266/ESP32 with a FastLED-based Adalight sketch is the usual choice. Set the sketch's LED count and baud rate to match your strip, and use the same baud rate in Ambilight.
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

Updating to a new version may require switching the permissions on again.

### First launch on Windows

Windows SmartScreen may warn about an unknown publisher: click **More info → Run anyway**. Most controllers use a CH340 or CP210x USB chip. Windows usually installs the driver automatically; if the controller doesn't show up, install the driver from the chip maker.

> Windows support is new and hasn't been tested with real hardware yet. Reports are very welcome.

## Set up your strip

1. Plug in the controller and power the strip. Ambilight picks the first USB serial device automatically.
2. Open **Settings…** from the panel (or right-click the tray icon).
3. Under **Connection**, set **Speed** to your firmware's baud rate.
4. Under **LED layout**, enter the LED count for each side, the corner where LED 1 sits (the end wired to the controller), and the direction the strip runs, seen from the front of the screen.
5. Click **Identify LEDs**. For 6 seconds LED 1 lights white, the right side red, the top green, the left side blue and the bottom yellow. If a color appears on the wrong side, adjust the corner or direction.

Settings are saved as JSON in `~/Library/Application Support/com.thepubdoc.ambilight/` on macOS and `%APPDATA%\com.thepubdoc.ambilight\` on Windows.

## Streaming video

Netflix, Prime Video, Disney+ and other streaming services protect their video: while any app captures the screen, the browser shows black instead. No screen-capture ambilight can read that picture, so Ambilight steps aside instead.

It recognises the service from the title of the window in front (browsers title their windows after the active tab, e.g. "Netflix" or "… | Prime Video"). While such a window is in front, Ambilight stops capturing the screen, so the video plays normally, and shows **Sound** mode instead: the lights follow the soundtrack, pulsing with music and flashing on loud moments. Switch to another tab or app and screen sync resumes within half a second. This works in any browser and in the services' own apps.

In **Settings → Streaming video** you can choose another fallback (**Color** or **Rainbow**) and turn individual services off. Supported: Netflix, Prime Video, Disney+, JioHotstar/JioCinema, Apple TV+, Hulu, Paramount+, Peacock, SonyLIV, ZEE5 and Crunchyroll.

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
  src/adalight.rs  Adalight protocol encoding
  src/serial.rs    USB serial port discovery and opening
  src/engine.rs    50 Hz loop: capture → smoothing → serial; reconnects, sleep handling, auto-switch
  src/capture/     Screen capture: ScreenCaptureKit (macOS), DXGI Desktop Duplication (Windows)
  src/audio/       System-audio loudness: Core Audio process tap (macOS), WASAPI loopback (Windows)
  src/sound.rs     Sound mode visuals
  src/foreground.rs  Front window title, for the streaming auto-switch
src-tauri/       Desktop app: tray icon, windows, settings storage, start at login
ui/              Popover and settings pages (plain HTML/CSS/JS, no build step)
tools/           Icon generation and local macOS signing helpers
```

To publish a release, push a tag like `v0.1.0`. The release workflow builds the macOS (Apple silicon and Intel) and Windows installers and attaches them to a draft GitHub release.

## License

[MIT](LICENSE) © 2026 Aayush Agrawal
