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
  Ride page. Nearby SWISSIMAGE aerial tiles and regional elevation are loaded
  on demand; mapped surface features appear in the imagery, not as 3D assets.
- `web/model.js` contains track distance, simulation, import/export and activity
  helpers. GPX/TCX routes and JSON activity backups are stored on-device.
- IndexedDB is the primary store; localStorage is used if IndexedDB is
  unavailable. The app does not upload data.
- Hash navigation (`#/dashboard`, `#/ride`, `#/routes`, `#/activities`,
  `#/devices`, `#/settings`) works on static hosting and survives refreshes.
- A service worker caches the app shell and same-origin assets. A separate
  bounded Cache Storage cache retains up to 96 regional geodata tiles for
  offline reuse; uncached regions use generated scenery when offline.
- Web Bluetooth is feature-detected and requests permission only after a user
  action. Standard heart-rate and cycling-power notifications are read when
  exposed by the browser/device. FTMS resistance control and ANT+ are not
  implemented.

The Vite production base is `/velocity/`. The build checker validates generated
HTML asset references before the Pages artifact is uploaded.

## Data and rendering boundaries

Browser rides load a five-by-five 2.5 m/pixel SWISSIMAGE base patch and a
three-by-three 1 m/pixel detail overlay around its center from the official
swisstopo WMTS, plus three-by-three Mapzen Terrarium elevation tiles from the
public AWS Open Data bucket. The European elevation source is EU-DEM at roughly
25–30 m native resolution, not swissALTI3D. Both services support browser CORS
and need no key.
swisstopo's free-geodata terms permit reuse with attribution (`© swisstopo`);
Mapzen's terrain-tile documentation requires attribution to Mapzen and its
underlying sources, including Copernicus EU-DEM. See the [swisstopo terms](https://www.swisstopo.admin.ch/en/terms-of-use-free-geodata-and-geoservices),
[Mapzen attribution](https://github.com/tilezen/joerd/blob/master/docs/attribution.md)
and [Mapzen data sources](https://github.com/tilezen/joerd/blob/master/docs/data-sources.md).

The tile cache is capped at 96 entries and prunes the oldest cached requests.
It prefers cached tiles for 30 days, then refreshes online and uses the prior
copy if a network request fails. It never downloads Switzerland as a whole.
Regional data is loaded around the rider and refreshed when they leave the
loaded patch; unavailable or out-of-coverage areas retain generated scenery
with an explicit status.

The browser demo routes remain generated loops rather than surveyed roads.
The loaded imagery and terrain are real, but roads, buildings and vegetation
are only visible as pixels, not as vector or 3D features, and the 25–30 m
EU-DEM cannot reproduce SwissALTI3D's sub-meter terrain or buildings. Imported
GPX/TCX coordinates are preserved. Areas without loaded regional data use
generated scenery. Native elevation support remains independent of the browser
renderer.

Static hosting has no server-side database, computation or upload endpoint.
Large geodata packs are intentionally not bundled. GeoAdmin's public geoservice
is best-effort and may limit excessive traffic; the app requests only nearby
tiles with four concurrent requests and does not retry failed requests.

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
