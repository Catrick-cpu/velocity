# VELOCITY

VELOCITY is a local-first, open-source cycling application built in Rust. It combines a native 3D riding experience, smart-trainer/sensor integration, route and activity handling, and training analysis without requiring an account or cloud service.

> **Status:** active development. The native Rust desktop app remains the full-featured version. A separate, playable browser app is published on GitHub Pages; its graphics and device support are not at native parity.

## Highlights

- **3D virtual world (native):** real-time route-based riding view rendered with OpenGL (egui/eframe + glow), including terrain chunks, road geometry, vegetation/villages/water, animated cyclist, camera modes, day/cloud/wet atmosphere controls, shadows/post-processing, and adaptive quality presets (`Auto`, `Ultra Low`, `Low`, `Medium`, `High`, `Ultra`).
- **Playable browser app:** Three.js/WebGL 2 3D training scene, route/camera/weather controls, local ride recording, GPX/TCX import, GPX/JSON export, and a light/dark theme at <https://catrick-cpu.github.io/velocity/>.
- **Real elevation data (when available):** streams/caches Mapzen Terrarium elevation tiles (SRTM/Copernicus/GMTED sources, CC BY 4.0, AWS Open Data) for geographically grounded terrain; falls back gracefully to route/profile terrain when offline or for bundled demo routes.
- **Routes & activities:** GPX route support, demo loop for trying the app without hardware, ride/session telemetry pipeline, local SQLite storage, and import/export/analysis foundations in `velocity-core`.
- **Sensors & trainer:** BLE/smart-trainer integration via `velocity-sensors` (Linux-focused; no system dbus headers required thanks to vendored libdbus), plus simulated telemetry for indoor/demo use.
- **Privacy & local-first:** data is stored locally. Do not commit personal activities, device identifiers, tokens, or credentials.

## Repository layout

- `crates/velocity-core` — domain model, physics, routes, elevation/profile logic, analytics, storage, formats
- `crates/velocity-sensors` — BLE sensors and smart trainer integration
- `crates/velocity-app` — native application, UI, charts, and 3D world renderer
- `assets/` — fonts and static assets
- `web/` — standalone JavaScript browser application and PWA shell
- `docs/` — architecture and deployment notes
- `scripts/` — build output validation

## Build (native, Linux)

### Prerequisites
- Rust toolchain compatible with the workspace (`rust-version` in `Cargo.toml`, currently 1.95+)
- A working C toolchain for native dependencies (Rust crates compile bundled/vendored deps where configured)
- An OpenGL-capable graphics stack/driver for the 3D world

No `sudo`, system package installs, or root privileges are required by this repository’s workflow.

### Run
```bash
cargo run -p velocity-app
```

Useful options (see `crates/velocity-app/src/main.rs`):
```bash
cargo run -p velocity-app -- --help
cargo run -p velocity-app -- --demo
cargo run -p velocity-app -- --offline
cargo run -p velocity-app -- --world-quality ultra
cargo run -p velocity-app -- --screenshot /tmp/velocity.png --screenshot-delay 8
```

### Check & test

```bash
cargo check -p velocity-app
cargo test --locked --workspace
cargo fmt --all -- --check
```

### Browser app

Requires Node.js 24+ and npm:

```bash
npm ci
npm run dev
```

The browser app is an independent implementation, not a Rust/WASM build. For a
production build and browser tests:

```bash
npm run lint
npm run format:check
npm test
npm run build
npm run check:build
npx playwright install chromium
npm run test:e2e
```

## Graphics & performance notes

- Use **Quality** presets to balance fidelity and frame pacing. **Ultra** is a manual maximum-fidelity preset (higher view distance/density/shadows) and may run slowly on integrated or software GPUs.
- The renderer uses chunk streaming around the rider, instancing, LOD/density scaling, frustum-aware drawing, and adaptive resolution options to stay usable on modest hardware (e.g. Intel HD-class graphics), but results depend on driver and scene.
- Weather/time controls (hour, cloud, wet) are visual simulation inputs and work offline.

## Browser / GitHub Pages

The browser app is deployed by `.github/workflows/deploy.yml` at
<https://catrick-cpu.github.io/velocity/>. Vite's production `base` is
`/velocity/`; the app uses hash routing (`/#/ride`, `/#/settings`) for static
hosting. The workflow verifies asset paths and runs automated browser tests
before deployment.

The browser's world is procedurally generated and is not a real map of
Switzerland. Imported GPS tracks are followed in the scene, but no real terrain
or building data is streamed. WebGL 2 / Three.js is supported; WebGPU is not
implemented. Bluetooth availability depends on the browser and device; FTMS
trainer control, ANT+ and FIT import remain native-only. See
[`docs/architecture.md`](docs/architecture.md) and
[`docs/deployment.md`](docs/deployment.md) for boundaries and the build path.

## Data sources & attribution

- **Elevation tiles:** Mapzen Terrain Tiles (SRTM, Copernicus, GMTED), **CC BY 4.0**, hosted on AWS Open Data (`https://s3.amazonaws.com/elevation-tiles-prod/terrarium`). Attribution must be preserved where elevation data is displayed/used.
- **Routes/maps (planned/integration-dependent):** OpenStreetMap and related open datasets may be used in future world-building; comply with their licenses and attribution requirements for any integrated data/assets.

Only free/open datasets, libraries, and assets compatible with open-source distribution should be added. Verify licenses before including third-party content.

## Security & privacy

- Never commit API keys, passwords, tokens, private exports, personal GPS activities, or device identifiers.
- Review staged files before pushing. Use `.gitignore` to keep local data and secrets out of the repository.

## License

MIT — see [`LICENSE`](LICENSE).
