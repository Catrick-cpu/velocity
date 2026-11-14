# Architecture

VELOCITY has two implementations with a shared product goal, not a shared runtime.

## Native application

The Cargo workspace is a native Rust desktop application:

- `crates/velocity-core` contains route and activity models, simulation/analytics,
  GPX/FIT/TCX handling, and SQLite persistence.
- `crates/velocity-sensors` contains native BLE scanning and trainer/sensor
  integrations. ANT+ and operating-system Bluetooth support are platform-bound.
- `crates/velocity-app` uses egui/eframe with an OpenGL renderer for the desktop
  UI and streamed 3D terrain. Its Mapzen Terrarium elevation cache is optional.

The current native binary is not a WebAssembly application. The eframe target is
configured for X11/Wayland, the world renderer is OpenGL-specific, SQLite and
Bluetooth dependencies are native-only, and the repository has no wasm entry
point. Those Rust modules are not compiled into Pages.

## Browser application

The static browser app is an independent implementation under `web/`:

- Vanilla JavaScript and Three.js build a WebGL 2 scene when the rider opens the
  Ride page. The terrain, road surface, trees, a few buildings, rider and camera
  are generated locally; no graphics service, API key, server or map download
  is required.
- `web/model.js` contains track distance, simulation, import/export and activity
  helpers. GPX/TCX routes and JSON activity backups are stored on-device.
- IndexedDB is the primary store; localStorage is used if IndexedDB is
  unavailable. The app does not upload data.
- Hash navigation (`#/dashboard`, `#/ride`, `#/routes`, `#/activities`,
  `#/devices`, `#/settings`) works on static hosting and survives refreshes.
- A service worker caches the app shell and same-origin assets after they have
  been visited. Cached UI and local records are available offline; no offline
  external map tiles are implied.
- Web Bluetooth is feature-detected and requests permission only after a user
  action. Standard heart-rate and cycling-power notifications are read when
  exposed by the browser/device. FTMS resistance control and ANT+ are not
  implemented.

The Vite production base is `/velocity/`. The build checker validates generated
HTML asset references before the Pages artifact is uploaded.

## Data and rendering boundaries

The browser demo routes are generated loops around named Swiss regions. They are
not surveyed roads, navigation routes, a height model, or a geospatially accurate
copy of Switzerland. A user-imported GPX/TCX route retains its coordinates and
can be followed in the generated scene, but the browser does not fetch
terrain/building/road datasets. Native elevation tiles use Mapzen Terrain Tiles
from AWS Open Data (SRTM/Copernicus/GMTED), licensed CC BY 4.0; this browser
build does not contact that source.

Static hosting has no server-side database, computation or upload endpoint.
Large regional data packs are intentionally not bundled. If real 3D map data is
added later, it needs independently hosted, chunked assets, documented license
and attribution, CORS/cache validation, and an offline fallback before it can
replace generated scenery.

## Local development and checks

```sh
npm ci
npm run dev
npm run lint
npm run format:check
npm test
npm run build
npm run check:build
npx playwright install chromium
npm run test:e2e

cargo fmt --all -- --check
cargo test --locked --workspace
cargo run -p velocity-app -- --demo --offline
```

See [deployment](deployment.md) for the automated Pages pipeline.
