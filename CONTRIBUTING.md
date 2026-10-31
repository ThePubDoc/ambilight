# Contributing to Ambilight

Thanks for helping! Bug reports, hardware reports and pull requests are all welcome.

## Ways to help without writing code

- **Hardware report:** tell us your strip, controller and OS work (or don't) with the [hardware report form](https://github.com/ThePubDoc/ambilight/issues/new?template=hardware_report.yml). This helps the next person choosing parts.
- **Windows testing:** Windows support compiles and builds in CI but has had little real-world use. Running it with your strip and reporting what happens is very valuable.
- **Bug reports:** use the [bug report form](https://github.com/ThePubDoc/ambilight/issues/new?template=bug_report.yml) and attach the log (the form says where to find it).

## Development setup

You need [Rust](https://rustup.rs), Node.js 18+, and:

- **macOS:** Xcode or the Command Line Tools (`xcode-select --install`).
- **Windows:** Visual Studio Build Tools with the C++ workload.

```sh
npm install
npm run dev                      # run the app in development mode
cargo test -p ambilight-core     # unit tests (no hardware needed)
```

On macOS, run `tools/setup-signing.sh` once and `tools/sign-macos.sh` after each `npm run build`, so the Screen Recording permission survives rebuilds (see the README).

### Where things live

- `core/`: the engine, as a plain Rust library with unit tests: LED layout, screen sampling, beat tracking, effects, the Adalight and WLED outputs, and screen and audio capture per platform.
- `src-tauri/`: the desktop app: tray icon, windows, settings storage, shortcuts, updater.
- `ui/`: the panel and settings pages, plain HTML/CSS/JS with no build step.

Most features are a change in `core/` plus a control in `ui/settings.html` and `ui/settings.js`. New config fields go in `core/src/config.rs` with `#[serde(default)]`, so older config files keep working.

## Before opening a pull request

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p ambilight-core
```

CI runs the same checks on macOS, Windows and Linux, and builds the app on macOS and Windows.

- Keep pull requests focused on one change.
- Add or update tests for logic in `core/` (layout, sampling, beat tracking and effects are all testable without hardware).
- Say how you tested it, and on which OS and hardware if it touches capture or output.
- If you change the UI, run `tools/screenshots/capture.sh` and include the updated screenshots.

## Good first issues

Look for the [good first issue](https://github.com/ThePubDoc/ambilight/labels/good%20first%20issue) label. Each one says which files to look at.

By contributing you agree that your contributions are licensed under the [MIT License](LICENSE).
