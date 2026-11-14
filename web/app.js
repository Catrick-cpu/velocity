import {
  DEMO_ROUTES,
  activityFromRide,
  elevationGain,
  exportGpx,
  formatDuration,
  gradeAt,
  parseTrackXml,
  routeLength,
  stepRide,
} from "./model.js";

const app = document.querySelector("#app");
const toast = document.querySelector("#toast");
const DEFAULT_SETTINGS = {
  theme: "light",
  quality: "Auto",
  timeOfDay: 9.5,
  cloud: 18,
  rain: 0,
  fog: 12,
  ftp: 250,
  weight: 75,
};
const state = {
  page: pageFromHash(),
  routeId: DEMO_ROUTES[0].id,
  routes: [...DEMO_ROUTES],
  activities: [],
  settings: { ...DEFAULT_SETTINGS },
  ride: {
    running: false,
    speedMps: 0,
    distanceM: 0,
    elapsedS: 0,
    powerW: 0,
    cadenceRpm: 0,
    heartRateBpm: null,
    grade: 0,
    effort: 1,
  },
  camera: "chase",
  world: null,
  frame: 0,
  previousFrame: 0,
  recordAccumulator: 0,
  tracePoints: [],
  bluetoothDevice: null,
  bluetoothCharacteristics: [],
  toastTimer: 0,
  storage: null,
  worldConstructor: null,
  worldLoadToken: 0,
  preferenceTimer: 0,
  importTarget: "route",
};

function pageFromHash() {
  const page = location.hash.replace(/^#\/?/, "").split("/")[0];
  return [
    "dashboard",
    "ride",
    "routes",
    "activities",
    "devices",
    "settings",
  ].includes(page)
    ? page
    : "dashboard";
}

function selectedRoute() {
  return (
    state.routes.find((route) => route.id === state.routeId) ?? state.routes[0]
  );
}

function notify(message, type = "info", duration = 4200) {
  clearTimeout(state.toastTimer);
  toast.textContent = message;
  toast.className = `toast toast-${type} visible`;
  state.toastTimer = setTimeout(
    () => toast.classList.remove("visible"),
    duration,
  );
}

function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, (character) => {
    const entities = {
      "&": "&amp;",
      "<": "&lt;",
      ">": "&gt;",
      '"': "&quot;",
      "'": "&#39;",
    };
    return entities[character];
  });
}

function metric(value, label, unit = "") {
  return `<div class="metric-card"><span>${label}</span><strong>${value}<small>${unit}</small></strong></div>`;
}

function layout(content) {
  app.innerHTML = `
    <header class="topbar">
      <a class="brand" href="#/dashboard" aria-label="VELOCITY dashboard">
        <span class="brand-mark">V</span><span>VELOCITY</span>
      </a>
      <div class="topbar-right">
        <span class="local-badge"><span></span> LOCAL-FIRST</span>
        <button class="icon-button" data-action="theme" aria-label="Toggle color theme" title="Toggle light and dark mode">
          ${state.settings.theme === "light" ? "☾" : "☀"}
        </button>
        <a class="avatar" href="#/settings" aria-label="Settings">R</a>
      </div>
    </header>
    <div class="shell">
      <aside class="sidebar">
        <div class="nav-group">
          <div class="nav-label">TRAIN</div>
          ${navLink("dashboard", "Overview", "⌂")}
          ${navLink("ride", "Ride", "↗")}
          ${navLink("routes", "Routes", "⌖")}
          ${navLink("activities", "Activities", "▤")}
        </div>
        <div class="nav-group">
          <div class="nav-label">SYSTEM</div>
          ${navLink("devices", "Devices", "⌁")}
          ${navLink("settings", "Settings", "⚙")}
        </div>
        <div class="sidebar-bottom">
          <div class="privacy-card"><span class="privacy-icon">◈</span><strong>Your data stays yours</strong><small>Stored in this browser. Never uploaded.</small></div>
          <div class="version-label">VELOCITY WEB · 0.1.0</div>
        </div>
      </aside>
      <main class="main-content" id="page-content">${content}</main>
    </div>
    <input id="file-import" class="visually-hidden" type="file" accept=".gpx,.tcx,.json,.fit,application/gpx+xml,application/json" />
  `;
}

function navLink(page, label, icon) {
  const active = state.page === page ? " active" : "";
  return `<a class="nav-link${active}" href="#/${page}" ${state.page === page ? 'aria-current="page"' : ""}>
    <span class="nav-icon" aria-hidden="true">${icon}</span><span>${label}</span>
  </a>`;
}

function renderDashboard() {
  const totalDistance = state.activities.reduce(
    (sum, item) => sum + (item.distanceM ?? 0),
    0,
  );
  const totalTime = state.activities.reduce(
    (sum, item) => sum + (item.elapsedS ?? 0),
    0,
  );
  const current = selectedRoute();
  return `
    <section class="page-heading">
      <div><span class="eyebrow">YOUR CYCLING SPACE</span><h1>Good ride,<br /><em>great day.</em></h1>
      <p>Your training data, routes and rides — private, local, and all yours.</p></div>
      <button class="button button-primary" data-action="go-ride"><span>↗</span> Start a ride</button>
    </section>
    <section class="overview-grid">
      <article class="welcome-card">
        <div class="welcome-copy"><span class="eyebrow">READY WHEN YOU ARE</span><h2>Find your<br />next rhythm.</h2>
        <p>Explore real Swiss aerial imagery and regional elevation while keeping every activity on this device. Demo courses remain generated training routes, not mapped roads.</p>
        <button class="button button-dark" data-action="go-ride">Enter the world <span>→</span></button></div>
        <div class="mountain-art" aria-hidden="true"><div class="sun-disc"></div><div class="mountain mountain-back"></div><div class="mountain mountain-front"></div><div class="mountain mountain-low"></div></div>
        <div class="welcome-note">REGIONAL MAP DATA · NETWORK OR CACHE</div>
      </article>
      <article class="quote-card"><span class="quote-mark">“</span><p>Consistency beats intensity. Show up, find your pace, and enjoy the ride.</p><span class="quote-caption">A NOTE FROM YOUR TRAINING SPACE</span></article>
    </section>
    <section class="section-block">
      <div class="section-heading"><div><span class="eyebrow">YOUR TRAINING</span><h2>At a glance</h2></div><a class="text-link" href="#/activities">View activity log <span>→</span></a></div>
      <div class="metrics-grid">
        ${metric(state.activities.length.toString(), "ACTIVITIES", "")}
        ${metric((totalDistance / 1000).toFixed(1), "DISTANCE", "km")}
        ${metric(formatDuration(totalTime), "RIDE TIME", "")}
        ${metric(String(state.settings.ftp), "FTP", "W")}
      </div>
    </section>
    <section class="section-block">
      <div class="section-heading"><div><span class="eyebrow">PICK A COURSE</span><h2>Ready to ride</h2></div><a class="text-link" href="#/routes">Route library <span>→</span></a></div>
      <div class="route-cards">
        ${state.routes
          .slice(0, 3)
          .map(
            (route, index) => `
          <button class="route-card route-card-${index + 1}" data-action="select-route" data-id="${escapeHtml(route.id)}">
            <span class="route-card-art" aria-hidden="true"><span class="route-sun"></span><span class="route-ridge"></span></span>
            <span class="route-card-copy"><strong>${escapeHtml(route.name)}</strong><small>${escapeHtml(route.area)}</small><span>${(route.lengthM / 1000).toFixed(1)} km <i>·</i> ${Math.round(route.climbM)} m climb</span></span>
          </button>`,
          )
          .join("")}
      </div>
      <p class="data-disclosure">Demo courses are generated for indoor training and are not mapped roads. Import a GPX track to ride your own route.</p>
    </section>
    <footer class="page-footer"><span>Built for riders. Designed for privacy.</span><span>Real aerial tiles · regional DEM · <a href="#/settings">About this build</a></span></footer>
    <span class="sr-only" id="current-route">${escapeHtml(current.name)}</span>`;
}

function renderRide() {
  const route = selectedRoute();
  const powerLabel = state.sensorPowerW == null ? "SIM WATTS" : "LIVE WATTS";
  const heartRateLabel = state.heartRateBpm == null ? "BPM" : "LIVE BPM";
  return `
    <section class="ride-header"><div><span class="eyebrow">VIRTUAL RIDE · ${powerLabel}</span><h1>${escapeHtml(route.name)}</h1>
      <p>${escapeHtml(route.area)} <span class="subtle-separator">·</span> Generated course · not for navigation</p></div>
      <label class="select-wrap"><span>COURSE</span><select id="ride-route">${state.routes.map((item) => `<option value="${escapeHtml(item.id)}" ${item.id === route.id ? "selected" : ""}>${escapeHtml(item.name)}</option>`).join("")}</select></label>
    </section>
    <section class="ride-layout">
      <div class="ride-stage">
        <canvas id="world" aria-label="Interactive three-dimensional cycling world"></canvas>
        <div id="world-fallback" class="world-fallback hidden"><strong>3D graphics unavailable</strong><span>This browser could not initialize WebGL 2. Ride records, routes and settings are still available.</span></div>
        <div class="world-topline"><span class="live-pill"><i></i><b id="ride-status">READY</b></span><span id="renderer-label">Checking graphics…</span></div>
        <div class="world-route-label"><span class="route-symbol">⌖</span><div><strong>${escapeHtml(route.name)}</strong><small>${escapeHtml(route.area)}</small></div></div>
        <div class="world-hud">
          <div class="speed-readout"><strong id="speed">0</strong><span>KM/H</span></div>
          <div class="hud-stats"><span><strong id="power">0</strong><small>${powerLabel}</small></span><span><strong id="cadence">0</strong><small>SIM RPM</small></span><span><strong id="heart-rate">--</strong><small>${heartRateLabel}</small></span></div>
          <div class="distance-readout"><strong id="distance">0.00</strong><span>KM</span><small id="elapsed">00:00:00</small></div>
        </div>
        <div class="world-caption"><span id="world-data-status" role="status" aria-live="polite">Swiss regional data loads on demand</span><span class="world-attribution"><a href="https://www.swisstopo.admin.ch/en/home.html" target="_blank" rel="noopener noreferrer">© swisstopo · SwissALTI3D</a> · <a href="https://openfreemap.org/quick_start/" target="_blank" rel="noopener noreferrer">© OpenMapTiles</a> · <a href="https://www.openstreetmap.org/copyright" target="_blank" rel="noopener noreferrer">© OpenStreetMap contributors</a> · <a href="https://github.com/tilezen/joerd/blob/master/docs/attribution.md" target="_blank" rel="noopener noreferrer">Mapzen / Copernicus EU-DEM</a></span></div>
      </div>
      <aside class="ride-side">
        <div class="ride-controls">
          <div class="control-heading"><div><span class="eyebrow">SESSION</span><h2>Ride controls</h2></div><span class="grade-badge" id="grade">0.0%</span></div>
          <div class="ride-buttons"><button class="button button-primary" id="ride-toggle" data-action="toggle-ride">Start ride</button><button class="button button-outline" data-action="finish-ride">Finish</button></div>
          <button class="button button-quiet button-full" data-action="reset-ride">Reset session</button>
          <div class="ride-divider"></div>
          <div class="setting-row"><span>Camera</span><div class="segmented" id="camera-controls">${cameraButton("chase", "Chase")}${cameraButton("close", "Close")}${cameraButton("hood", "Onboard")}${cameraButton("drone", "Aerial")}</div></div>
          <label class="range-control"><span>Effort <b id="effort-value">${Math.round(state.ride.effort * 100)}%</b></span><input id="effort" type="range" min="30" max="160" value="${Math.round(state.ride.effort * 100)}" /></label>
          <div class="ride-divider"></div>
          <div class="mini-heading"><span>WORLD CONDITIONS</span><span>VISUAL ONLY</span></div>
          ${rangeControl("time-of-day", "Time of day", 0, 24, 0.1, state.settings.timeOfDay, `${formatHour(state.settings.timeOfDay)}`)}
          ${rangeControl("cloud-cover", "Cloud cover", 0, 100, 1, state.settings.cloud, `${state.settings.cloud}%`)}
          ${rangeControl("rain", "Rain", 0, 100, 1, state.settings.rain, `${state.settings.rain}%`)}
          <div class="ride-divider"></div>
          <div class="session-note"><span class="sim-badge">${state.sensorPowerW == null ? "SIM" : "LIVE"}</span><span>Power: ${state.sensorPowerW == null ? "simulated" : "Bluetooth sensor"}. Speed and cadence are simulated. Heart rate: ${state.heartRateBpm == null ? "not connected" : "Bluetooth sensor"}.</span></div>
          <div class="ride-foot-stats"><span>FPS <b id="fps">--</b></span><span>Route <b>${(route.lengthM / 1000).toFixed(1)} km</b></span><span>Climb <b>${Math.round(route.climbM)} m</b></span></div>
        </div>
      </aside>
    </section>`;
}

function cameraButton(mode, label) {
  return `<button class="${state.camera === mode ? "selected" : ""}" data-action="camera" data-id="${mode}" aria-pressed="${state.camera === mode}">${label}</button>`;
}

function rangeControl(id, label, min, max, step, value, output) {
  return `<label class="range-control"><span>${label} <b id="${id}-value">${output}</b></span><input id="${id}" type="range" min="${min}" max="${max}" step="${step}" value="${value}" /></label>`;
}

function renderRoutes() {
  return `
    ${pageTitle("ROUTE LIBRARY", "Routes", "Choose a course for your next indoor session, or import a GPS track from your device.")}
    <section class="content-card import-card"><div class="import-icon">↥</div><div><strong>Bring your own route</strong><p>Import a GPX or TCX track. GPS coordinates stay in this browser; nothing is uploaded.</p></div><button class="button button-primary" data-action="import-route">Import route</button></section>
    <section class="section-block route-library"><div class="section-heading"><div><span class="eyebrow">AVAILABLE COURSES</span><h2>${state.routes.length} routes</h2></div><span class="local-tag">LOCAL</span></div>
      <div class="route-list">${state.routes
        .map(
          (route, index) => `
        <article class="route-row">
          <div class="route-thumb route-thumb-${index % 3}" aria-hidden="true"><span></span></div>
          <div class="route-info"><strong>${escapeHtml(route.name)}</strong><small>${escapeHtml(route.area)}${route.imported ? " · Imported GPS" : " · Procedural demo"}</small><div class="route-tags"><span>${(route.lengthM / 1000).toFixed(1)} km</span><span>${Math.round(route.climbM)} m climb</span><span>${route.points.length} track points</span></div></div>
          <div class="route-actions"><button class="button button-outline" data-action="select-route" data-id="${escapeHtml(route.id)}">${route.id === state.routeId ? "Selected" : "Select"}</button>${route.imported ? `<button class="button button-quiet" data-action="delete-route" data-id="${escapeHtml(route.id)}" aria-label="Delete ${escapeHtml(route.name)}">Delete</button>` : ""}</div>
        </article>`,
        )
        .join("")}</div>
    </section>
    <div class="notice-card"><strong>Map and terrain data</strong><p>The ride scene loads 2.5 m/pixel SWISSIMAGE imagery, a 1 m detail layer and a five-by-five 0.1 m/pixel close-range patch (about 128 m across), plus a SwissALTI3D hillshade texture. This is high-resolution aerial photography, not live street-level imagery; free continuous Street View-style coverage is not available. Elevation still comes from EU-DEM at roughly 25–30 m source resolution. Nearby OpenFreeMap/OpenMapTiles data derived from OpenStreetMap provides approximate 3D building footprints and heights; attribution is shown in the scene. The map services receive the requested area, but GPS tracks and ride data are not uploaded. Data is cached locally with a bounded 96-tile cache. Demo courses are generated, not mapped roads, and roads/vegetation are not 3D features. External data can be unavailable or incomplete; the ride remains playable with imagery or generated scenery.</p></div>`;
}

function renderActivities() {
  return `
    ${pageTitle("TRAINING LOG", "Activities", "Your rides are saved locally in this browser. Export them any time.")}
    <section class="activity-summary metrics-grid">
      ${metric(state.activities.length.toString(), "ACTIVITIES", "")}
      ${metric((state.activities.reduce((sum, item) => sum + (item.distanceM ?? 0), 0) / 1000).toFixed(1), "TOTAL DISTANCE", "km")}
      ${metric(formatDuration(state.activities.reduce((sum, item) => sum + (item.elapsedS ?? 0), 0)), "TOTAL TIME", "")}
      ${metric(String(state.activities.filter((item) => item.simulated).length), "SIMULATED", "")}
    </section>
    <section class="section-block">
      <div class="section-heading"><div><span class="eyebrow">ON THIS DEVICE</span><h2>Ride history</h2></div><button class="button button-outline" data-action="import-activity">Import file</button></div>
      ${
        state.activities.length
          ? `<div class="activity-list">${state.activities
              .slice()
              .sort((a, b) => b.date.localeCompare(a.date))
              .map((item) => activityRow(item))
              .join("")}</div>`
          : `<div class="empty-state"><div class="empty-icon">◷</div><strong>No rides recorded yet</strong><p>Start a ride, or import a GPX, TCX or VELOCITY JSON activity file.</p><button class="button button-primary" data-action="go-ride">Start your first ride</button></div>`
      }
    </section>
    <div class="notice-card"><strong>About simulated rides</strong><p>Without a connected sensor, indoor power, speed and cadence are estimates, clearly marked SIM. Imported GPX and TCX activities do not contain power unless supplied by the file. FIT import is not available in the browser build; use the native app for FIT files.</p></div>`;
}

function activityRow(item) {
  const date = Number.isFinite(Date.parse(item.date))
    ? new Date(item.date).toLocaleDateString()
    : "Imported";
  const sourceLabel = item.simulated
    ? item.powerSource === "device"
      ? "PARTIAL SENSOR"
      : "SIMULATED"
    : "IMPORTED";
  return `<article class="activity-row"><div class="activity-icon">↗</div><div class="activity-details"><strong>${escapeHtml(item.name || "Ride")}</strong><small>${date} · ${sourceLabel}</small></div><div class="activity-value"><strong>${((item.distanceM ?? 0) / 1000).toFixed(1)} km</strong><small>${formatDuration(item.elapsedS ?? 0)}</small></div><div class="activity-value"><strong>${Math.round(item.avgPowerW ?? 0)} W</strong><small>${Math.round(item.climbM ?? 0)} m climb</small></div><button class="icon-button" data-action="export-activity" data-id="${escapeHtml(item.id)}" title="Export activity">↓</button><button class="button button-quiet" data-action="delete-activity" data-id="${escapeHtml(item.id)}">Delete</button></article>`;
}

function renderDevices() {
  const bluetoothAvailable = "bluetooth" in navigator && window.isSecureContext;
  return `
    ${pageTitle("SENSOR CONNECTIONS", "Devices", "Connect compatible Bluetooth sensors with explicit browser permission.")}
    <section class="device-status-card ${state.bluetoothDevice ? "device-connected" : ""}">
      <div class="device-orbit"><span>⌁</span><i></i><i></i></div>
      <div class="device-status-copy"><span class="eyebrow">${state.bluetoothDevice ? "CONNECTED" : "DEVICE STATUS"}</span><h2>${state.bluetoothDevice ? escapeHtml(state.bluetoothDevice.name || "Bluetooth sensor") : "Your ride, your data."}</h2><p id="device-summary">${state.bluetoothDevice ? "Live readings are coming directly from the selected Bluetooth device." : "Choose a supported sensor to share its readings with VELOCITY. Connection begins only after your permission."}</p></div>
      <button class="button ${state.bluetoothDevice ? "button-outline" : "button-primary"}" data-action="${state.bluetoothDevice ? "disconnect-device" : "connect-device"}" ${bluetoothAvailable ? "" : "disabled"}>${state.bluetoothDevice ? "Disconnect" : "Connect a device"}</button>
    </section>
    ${bluetoothAvailable ? "" : `<div class="notice-card notice-warning"><strong>Web Bluetooth is unavailable in this browser or context.</strong><p>Web Bluetooth requires a supported Chromium-based browser and a secure context (HTTPS or localhost). Safari, Firefox and many mobile browsers do not expose this API. The native app remains the option for broader BLE support.</p></div>`}
    <section class="section-block"><div class="section-heading"><div><span class="eyebrow">WEB BLUETOOTH</span><h2>What can connect</h2></div></div>
      <div class="device-grid">
        ${deviceCard("Heart rate monitors", "Heart Rate Service · 0x180D", "Supported where Web Bluetooth is available. Heart rate is read only after you select and authorize the device.", state.bluetoothDevice ? "Connected" : "Browser-dependent")}
        ${deviceCard("Cycling power meters", "Cycling Power Service · 0x1818", "Power measurement notifications are read when the device exposes the standard service.", "Browser-dependent")}
        ${deviceCard("Wahoo KICKR / smart trainers", "FTMS · 0x1826", "FTMS readings may be available. Controlling trainer resistance is not implemented in this browser build.", "Read-only, if exposed")}
        ${deviceCard("Garmin Forerunner 245", "BLE / ANT+", "ANT+ is not supported by browsers. Garmin watch data is not directly integrated here; import an exported activity instead.", "Not directly supported")}
      </div>
    </section>
    <div class="notice-card"><strong>No simulated connections</strong><p>Sensor cards describe standard service compatibility, not a verified connection. Connected devices are named only after the browser reports a successful GATT connection. Power and cadence shown during a ride remain labeled simulated unless a live measurement is received.</p></div>`;
}

function deviceCard(title, protocol, description, status) {
  return `<article class="device-card"><div class="device-card-top"><span class="device-icon">⌁</span><span class="device-compat">${escapeHtml(status)}</span></div><h3>${title}</h3><span class="device-protocol">${protocol}</span><p>${description}</p></article>`;
}

function renderSettings() {
  return `
    ${pageTitle("PREFERENCES", "Settings", "Customize this browser build. Preferences and activities stay on this device.")}
    <section class="settings-grid">
      <article class="content-card settings-card"><span class="eyebrow">APPEARANCE</span><h2>Make it yours</h2>
        <label class="settings-field"><span>Color theme</span><select id="theme-setting"><option value="light" ${state.settings.theme === "light" ? "selected" : ""}>Light · default</option><option value="dark" ${state.settings.theme === "dark" ? "selected" : ""}>Dark</option></select></label>
        <label class="settings-field"><span>Graphics quality</span><select id="quality-setting">${["Auto", "Ultra Low", "Low", "Medium", "High", "Ultra"].map((quality) => `<option ${quality === state.settings.quality ? "selected" : ""}>${quality}</option>`).join("")}</select></label>
        <p class="field-help">WebGL 2 is used for 3D rendering. Quality adjusts render resolution, shadows and scene detail. WebGPU is not implemented by this renderer.</p>
      </article>
      <article class="content-card settings-card"><span class="eyebrow">TRAINING PROFILE</span><h2>Your targets</h2>
        <label class="settings-field"><span>Functional threshold power</span><div class="input-with-unit"><input id="ftp-setting" type="number" min="50" max="600" step="1" value="${state.settings.ftp}" /><span>W</span></div></label>
        <label class="settings-field"><span>Rider weight</span><div class="input-with-unit"><input id="weight-setting" type="number" min="30" max="200" step="0.5" value="${state.settings.weight}" /><span>kg</span></div></label>
        <p class="field-help">Profile values are stored locally. Weight is not used to estimate health or calories in this version.</p>
      </article>
      <article class="content-card settings-card"><span class="eyebrow">LOCAL STORAGE</span><h2>Your data</h2><div class="storage-facts"><span>Activities saved <b>${state.activities.length}</b></span><span>Routes saved <b>${state.routes.filter((item) => item.imported).length}</b></span><span>Storage method <b>${state.storage?.mode === "indexeddb" ? "IndexedDB" : "Browser storage"}</b></span></div>
        <button class="button button-outline" data-action="export-backup">Export data backup</button><p class="field-help">Clearing browser data deletes locally stored activities and settings. Export a backup before clearing site data.</p>
      </article>
      <article class="content-card settings-card"><span class="eyebrow">ABOUT THIS BUILD</span><h2>Native + browser</h2><p class="about-copy">The macOS/Linux desktop app is Rust, egui and OpenGL, with SQLite, GPX/FIT/TCX support and native BLE integrations. This Pages app is a separate JavaScript/Three.js implementation: WebGL 2 rendering, local browser storage and GPX/TCX import. Desktop Rust modules do not run in this browser build.</p><div class="about-links"><a href="https://github.com/Catrick-cpu/velocity" target="_blank" rel="noreferrer">Source on GitHub ↗</a><a href="#/devices">Device compatibility →</a></div></article>
    </section>
    <footer class="page-footer"><span>VELOCITY 0.1.0 · MIT License</span><span>Local-first by design.</span></footer>`;
}

function pageTitle(eyebrow, title, description) {
  return `<section class="page-heading compact"><div><span class="eyebrow">${eyebrow}</span><h1>${title}</h1><p>${description}</p></div></section>`;
}

function formatHour(hour) {
  const hours = Math.floor(hour) % 24;
  const minutes = Math.round((hour - Math.floor(hour)) * 60);
  return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}`;
}

function render() {
  stopWorldLoop();
  state.page = pageFromHash();
  document.documentElement.dataset.theme = state.settings.theme;
  const pages = {
    dashboard: renderDashboard,
    ride: renderRide,
    routes: renderRoutes,
    activities: renderActivities,
    devices: renderDevices,
    settings: renderSettings,
  };
  layout(pages[state.page]());
  if (state.page === "ride") startWorldLoop();
  document.title = `VELOCITY · ${state.page[0].toUpperCase()}${state.page.slice(1)}`;
}

function stopWorldLoop() {
  state.worldLoadToken += 1;
  if (state.frame) cancelAnimationFrame(state.frame);
  state.frame = 0;
  state.previousFrame = 0;
  state.world?.dispose();
  state.world = null;
}

async function startWorldLoop() {
  const canvas = document.querySelector("#world");
  if (!canvas) return;
  const token = ++state.worldLoadToken;
  try {
    if (!state.worldConstructor) {
      state.worldConstructor = (await import("./world.js")).WorldRenderer;
    }
    if (token !== state.worldLoadToken || state.page !== "ride") return;
    if (!state.world) {
      state.world = new state.worldConstructor(canvas, (status) => {
        const label = document.querySelector("#renderer-label");
        if (label) label.textContent = status;
      });
      state.world.onRegionStatus = (status) => {
        const label = document.querySelector("#world-data-status");
        if (label) label.textContent = status;
      };
      state.world.onFps((fps) => {
        const output = document.querySelector("#fps");
        if (output) output.textContent = String(fps);
      });
    }
    state.world.setQuality(state.settings.quality);
    state.world.setRoute(selectedRoute());
    updateRideControls();
  } catch (error) {
    if (token !== state.worldLoadToken) return;
    console.error("Could not initialize the 3D world:", error);
    document.querySelector("#world-fallback")?.classList.remove("hidden");
    const label = document.querySelector("#renderer-label");
    if (label) label.textContent = "WebGL 2 unavailable";
  }
  if (token === state.worldLoadToken)
    state.frame = requestAnimationFrame(worldFrame);
}

function worldFrame(timestamp) {
  if (state.page !== "ride") {
    state.frame = 0;
    return;
  }
  const elapsed = state.previousFrame
    ? Math.min(Math.max((timestamp - state.previousFrame) / 1000, 0), 0.5)
    : 0;
  state.previousFrame = timestamp;
  if (state.ride.running) {
    const route = selectedRoute();
    const progress = state.ride.distanceM / Math.max(1, route.lengthM);
    const pointIndex = Math.min(
      route.points.length - 1,
      Math.floor(progress * (route.points.length - 1)),
    );
    state.ride.grade = gradeAt(route.points, pointIndex);
    const steps = Math.max(1, Math.ceil(elapsed / 0.05));
    for (let index = 0; index < steps; index += 1) {
      state.ride = stepRide(state.ride, elapsed / steps, state.ride.effort);
    }
    state.recordAccumulator += elapsed;
    if (state.recordAccumulator >= 1) {
      const currentIndex = Math.min(
        route.points.length - 1,
        Math.floor(
          (state.ride.distanceM / route.lengthM) * (route.points.length - 1),
        ),
      );
      const point = route.points[currentIndex];
      if (point && state.tracePoints.at(-1) !== point)
        state.tracePoints.push(point);
      state.recordAccumulator = 0;
    }
    if (state.bluetoothDevice) {
      if (state.sensorPowerW != null) state.ride.powerW = state.sensorPowerW;
      if (state.heartRateBpm != null)
        state.ride.heartRateBpm = state.heartRateBpm;
    }
    updateRideHud();
  }

  if (state.world) {
    try {
      state.world.render({
        distanceM: state.ride.distanceM,
        lengthM: selectedRoute().lengthM,
        cameraMode: state.camera,
        hour: state.settings.timeOfDay,
        cloud: state.settings.cloud,
        rain: state.settings.rain,
        fog: state.settings.fog,
        quality: state.settings.quality,
      });
    } catch (error) {
      console.error("3D world rendering stopped:", error);
      document.querySelector("#world-fallback")?.classList.remove("hidden");
      document.querySelector("#renderer-label").textContent = "Rendering error";
      stopWorldLoop();
      return;
    }
  }
  state.frame = requestAnimationFrame(worldFrame);
}

function updateRideHud() {
  const ride = state.ride;
  setText("#speed", String(Math.round(ride.speedMps * 3.6)));
  setText("#power", String(Math.round(ride.powerW)));
  setText("#cadence", String(Math.round(ride.cadenceRpm)));
  setText(
    "#heart-rate",
    ride.heartRateBpm == null ? "--" : String(ride.heartRateBpm),
  );
  setText("#distance", (ride.distanceM / 1000).toFixed(2));
  setText("#elapsed", formatDuration(ride.elapsedS));
  setText("#grade", `${ride.grade.toFixed(1)}%`);
  setText(
    "#ride-status",
    ride.running ? "RECORDING" : ride.elapsedS > 0 ? "PAUSED" : "READY",
  );
  const toggle = document.querySelector("#ride-toggle");
  if (toggle)
    toggle.textContent = ride.running
      ? "Pause ride"
      : ride.elapsedS > 0
        ? "Resume ride"
        : "Start ride";
}

function updateRideControls() {
  updateRideHud();
  const button = document.querySelector("#ride-toggle");
  if (button)
    button.textContent = state.ride.running
      ? "Pause ride"
      : state.ride.elapsedS > 0
        ? "Resume ride"
        : "Start ride";
}

function setText(selector, text) {
  const element = document.querySelector(selector);
  if (element) element.textContent = text;
}

function startRide() {
  if (!state.ride.running) {
    state.ride.running = true;
    if (state.tracePoints.length === 0)
      state.tracePoints.push(selectedRoute().points[0]);
    notify(
      state.bluetoothDevice
        ? "Ride started. Sensor data is live where available."
        : "Ride started. Power and cadence are simulated.",
      "success",
    );
  } else {
    state.ride.running = false;
  }
  updateRideControls();
}

async function finishRide() {
  state.ride.running = false;
  if (state.ride.elapsedS < 1 || state.ride.distanceM < 1) {
    updateRideControls();
    notify("Ride ended before any activity could be recorded.");
    return;
  }
  const activity = activityFromRide(state.ride, selectedRoute());
  activity.simulated = true;
  activity.powerSource = state.sensorPowerW == null ? "simulation" : "device";
  activity.heartRateSource = state.heartRateBpm == null ? "none" : "device";
  activity.points =
    state.tracePoints.length > 1
      ? state.tracePoints
      : [selectedRoute().points[0], selectedRoute().points.at(-1)];
  activity.climbM = elevationGain(activity.points);
  state.activities.unshift(activity);
  await state.storage.put("activities", activity);
  state.ride = createEmptyRide();
  state.tracePoints = [];
  state.recordAccumulator = 0;
  updateRideControls();
  notify("Ride saved locally on this device.", "success");
}

function createEmptyRide() {
  return {
    running: false,
    speedMps: 0,
    distanceM: 0,
    elapsedS: 0,
    powerW: 0,
    cadenceRpm: 0,
    heartRateBpm: null,
    grade: 0,
    effort: state.ride.effort,
  };
}

function downloadFile(filename, content, mimeType) {
  const url = URL.createObjectURL(new Blob([content], { type: mimeType }));
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  URL.revokeObjectURL(url);
}

async function importFile(file) {
  if (!file) return;
  if (file.size > 20 * 1024 * 1024) {
    notify("Files larger than 20 MB are not accepted.", "error");
    return;
  }
  const extension = file.name.split(".").at(-1).toLowerCase();
  try {
    if (extension === "fit")
      throw new Error(
        "FIT import is not supported in the browser build. Use the native app to import FIT files.",
      );
    if (extension === "json") {
      const imported = JSON.parse(await file.text());
      if (
        imported &&
        typeof imported === "object" &&
        imported.version === 1 &&
        Array.isArray(imported.activities)
      ) {
        if (
          imported.activities.length > 1000 ||
          (imported.routes?.length ?? 0) > 100
        ) {
          throw new Error(
            "This backup contains too many activities or routes.",
          );
        }
        const activities = imported.activities.map(normalizeImportedActivity);
        const routes = (imported.routes ?? []).map(normalizeImportedRoute);
        for (const activity of activities)
          await state.storage.put("activities", activity);
        for (const route of routes) await state.storage.put("routes", route);
        state.activities.unshift(...activities);
        state.routes.push(...routes);
        if (imported.settings && typeof imported.settings === "object") {
          state.settings = normalizeSettings(imported.settings);
          await state.storage.put("settings", {
            id: "preferences",
            ...state.settings,
          });
        }
        render();
        notify("VELOCITY backup imported and saved locally.", "success");
        return;
      }
      const activity = normalizeImportedActivity(imported, file.name);
      state.activities.unshift(activity);
      await state.storage.put("activities", activity);
      render();
      notify("Activity imported and saved locally.", "success");
      return;
    }
    if (!["gpx", "tcx"].includes(extension))
      throw new Error("Choose a GPX, TCX or VELOCITY JSON file.");
    const route = parseTrackXml(await file.text(), file.name);
    if (state.importTarget === "activity") {
      const activity = {
        id: crypto.randomUUID(),
        name: route.name,
        date: new Date().toISOString(),
        distanceM: route.lengthM,
        elapsedS: 0,
        avgPowerW: 0,
        climbM: route.climbM,
        simulated: false,
        points: route.points,
      };
      state.activities.unshift(activity);
      await state.storage.put("activities", activity);
      render();
      notify(
        "GPS activity imported. Time and power are only available if present in the source file.",
        "success",
      );
    } else {
      state.routes.push(route);
      state.routeId = route.id;
      await state.storage.put("routes", route);
      render();
      notify("Route imported and saved locally.", "success");
    }
  } catch (error) {
    console.error("File import failed:", error);
    notify(error.message || "Could not import that file.", "error", 6500);
  }
}

function normalizeImportedActivity(value, fallbackName = "Imported activity") {
  if (!value || typeof value !== "object" || !Array.isArray(value.points)) {
    throw new Error("The JSON file is not a VELOCITY activity or backup.");
  }
  const points = normalizeImportedPoints(value.points);
  return {
    id: crypto.randomUUID(),
    name: String(value.name || fallbackName).slice(0, 100),
    date: Number.isFinite(Date.parse(value.date))
      ? new Date(value.date).toISOString()
      : new Date().toISOString(),
    distanceM: Math.max(0, Number(value.distanceM) || routeLength(points)),
    elapsedS: Math.max(0, Number(value.elapsedS) || 0),
    avgPowerW: Math.max(0, Number(value.avgPowerW) || 0),
    climbM: Math.max(0, Number(value.climbM) || elevationGain(points)),
    simulated: Boolean(value.simulated),
    powerSource: value.powerSource === "device" ? "device" : "simulation",
    points,
  };
}

function normalizeImportedRoute(value) {
  if (!value || typeof value !== "object" || !Array.isArray(value.points)) {
    throw new Error("The backup contains an invalid route.");
  }
  const points = normalizeImportedPoints(value.points);
  return {
    id: crypto.randomUUID(),
    name: String(value.name || "Imported route").slice(0, 100),
    area: "Imported GPS track",
    lengthM: Math.max(1, Number(value.lengthM) || routeLength(points)),
    climbM: Math.max(0, Number(value.climbM) || elevationGain(points)),
    points,
    imported: true,
  };
}

function normalizeImportedPoints(points) {
  if (points.length < 2 || points.length > 100_000) {
    throw new Error(
      "Activity tracks must contain between 2 and 100,000 points.",
    );
  }
  return points.map((point) => {
    const lat = Number(point?.lat);
    const lon = Number(point?.lon);
    const ele = point?.ele == null ? null : Number(point.ele);
    if (
      !Number.isFinite(lat) ||
      !Number.isFinite(lon) ||
      Math.abs(lat) > 90 ||
      Math.abs(lon) > 180 ||
      (ele != null && !Number.isFinite(ele))
    ) {
      throw new Error(
        "The file contains invalid GPS coordinates or elevation values.",
      );
    }
    return { lat, lon, ele };
  });
}

function normalizeSettings(value) {
  const qualityOptions = [
    "Auto",
    "Ultra Low",
    "Low",
    "Medium",
    "High",
    "Ultra",
  ];
  return {
    theme: value.theme === "dark" ? "dark" : "light",
    quality: qualityOptions.includes(value.quality)
      ? value.quality
      : DEFAULT_SETTINGS.quality,
    timeOfDay: clampNumber(value.timeOfDay, 0, 24, DEFAULT_SETTINGS.timeOfDay),
    cloud: clampNumber(value.cloud, 0, 100, DEFAULT_SETTINGS.cloud),
    rain: clampNumber(value.rain, 0, 100, DEFAULT_SETTINGS.rain),
    fog: clampNumber(value.fog, 0, 100, DEFAULT_SETTINGS.fog),
    ftp: clampNumber(value.ftp, 50, 600, DEFAULT_SETTINGS.ftp),
    weight: clampNumber(value.weight, 30, 200, DEFAULT_SETTINGS.weight),
  };
}

function clampNumber(value, min, max, fallback) {
  const number = Number(value);
  return Number.isFinite(number)
    ? Math.min(max, Math.max(min, number))
    : fallback;
}

function persistPreferences() {
  clearTimeout(state.preferenceTimer);
  state.preferenceTimer = setTimeout(async () => {
    try {
      await state.storage.put("settings", {
        id: "preferences",
        ...state.settings,
      });
    } catch (error) {
      console.error("Could not save preferences:", error);
      notify(`Settings were not saved: ${error.message}`, "error");
    }
  }, 250);
}

async function connectDevice() {
  if (!("bluetooth" in navigator) || !window.isSecureContext) {
    notify("Web Bluetooth is unavailable in this browser or context.", "error");
    return;
  }
  try {
    const device = await navigator.bluetooth.requestDevice({
      acceptAllDevices: true,
      optionalServices: ["heart_rate", "cycling_power", "fitness_machine"],
    });
    const server = await device.gatt.connect();
    state.bluetoothDevice = device;
    device.addEventListener("gattserverdisconnected", onDeviceDisconnected);
    let characteristics = 0;
    for (const [serviceUuid, characteristicUuid, kind] of [
      ["heart_rate", "heart_rate_measurement", "heart"],
      ["cycling_power", "cycling_power_measurement", "power"],
    ]) {
      try {
        const service = await server.getPrimaryService(serviceUuid);
        const characteristic =
          await service.getCharacteristic(characteristicUuid);
        await characteristic.startNotifications();
        characteristic.addEventListener(
          "characteristicvaluechanged",
          (event) => {
            const value = event.target.value;
            if (kind === "heart") {
              const is16Bit = (value.getUint8(0) & 1) === 1;
              state.heartRateBpm = is16Bit
                ? value.getUint16(1, true)
                : value.getUint8(1);
              state.ride.heartRateBpm = state.heartRateBpm;
            } else if (value.byteLength >= 4) {
              state.sensorPowerW = Math.max(0, value.getInt16(2, true));
            }
          },
        );
        state.bluetoothCharacteristics.push(characteristic);
        characteristics += 1;
      } catch (error) {
        if (error.name !== "NotFoundError")
          console.warn(`Optional ${kind} service unavailable:`, error);
      }
    }
    render();
    notify(
      characteristics
        ? `Connected to ${device.name || "Bluetooth sensor"}.`
        : `Connected to ${device.name || "device"}; no supported sensor service was found.`,
      characteristics ? "success" : "warning",
      6000,
    );
  } catch (error) {
    if (error.name === "NotFoundError" || error.name === "NotAllowedError") {
      notify("No device was selected. No connection was made.");
    } else {
      console.error("Bluetooth connection failed:", error);
      notify(`Could not connect: ${error.message}`, "error", 6500);
    }
  }
}

function onDeviceDisconnected() {
  state.bluetoothDevice = null;
  state.bluetoothCharacteristics = [];
  state.heartRateBpm = null;
  state.sensorPowerW = null;
  if (state.page === "devices") render();
  notify("Bluetooth device disconnected.", "warning");
}

async function disconnectDevice() {
  if (state.bluetoothDevice?.gatt?.connected)
    state.bluetoothDevice.gatt.disconnect();
  state.bluetoothDevice = null;
  state.bluetoothCharacteristics = [];
  state.heartRateBpm = null;
  state.sensorPowerW = null;
  render();
  notify("Device disconnected.");
}

function createStorage() {
  const stores = ["activities", "routes", "settings"];
  let database;
  let mode = "indexeddb";
  let warned = false;
  const fallbackKey = "velocity-web-data";

  async function fallbackRead(store) {
    let data;
    try {
      data = JSON.parse(localStorage.getItem(fallbackKey) || "{}");
    } catch (error) {
      throw new Error(
        `Local browser data could not be read: ${error.message}`,
        {
          cause: error,
        },
      );
    }
    return data[store] ?? [];
  }

  async function fallbackWrite(store, value) {
    let data;
    try {
      data = JSON.parse(localStorage.getItem(fallbackKey) || "{}");
      const entries = Array.isArray(data[store]) ? data[store] : [];
      const index = entries.findIndex((entry) => entry.id === value.id);
      if (index >= 0) entries[index] = value;
      else entries.push(value);
      data[store] = entries;
      localStorage.setItem(fallbackKey, JSON.stringify(data));
    } catch (error) {
      throw new Error(`Could not save local browser data: ${error.message}`, {
        cause: error,
      });
    }
  }

  async function fallbackDelete(store, id) {
    const data = JSON.parse(localStorage.getItem(fallbackKey) || "{}");
    data[store] = (data[store] ?? []).filter((entry) => entry.id !== id);
    localStorage.setItem(fallbackKey, JSON.stringify(data));
  }

  function open() {
    if (mode === "local")
      return Promise.reject(new Error("Using browser storage fallback."));
    if (database) return Promise.resolve(database);
    return new Promise((resolve, reject) => {
      if (!("indexedDB" in window)) {
        mode = "local";
        reject(new Error("IndexedDB is unavailable."));
        return;
      }
      const request = indexedDB.open("velocity-web", 1);
      request.onupgradeneeded = () => {
        for (const store of stores) {
          if (!request.result.objectStoreNames.contains(store)) {
            request.result.createObjectStore(store, { keyPath: "id" });
          }
        }
      };
      request.onsuccess = () => {
        database = request.result;
        resolve(database);
      };
      request.onerror = () =>
        reject(request.error ?? new Error("IndexedDB could not be opened."));
      request.onblocked = () =>
        reject(new Error("IndexedDB upgrade is blocked by another tab."));
    });
  }

  async function run(store, action, value) {
    try {
      const db = await open();
      return await new Promise((resolve, reject) => {
        const transaction = db.transaction(store, "readwrite");
        const request =
          action === "all"
            ? transaction.objectStore(store).getAll()
            : action === "put"
              ? transaction.objectStore(store).put(value)
              : transaction.objectStore(store).delete(value);
        request.onsuccess = () => resolve(request.result);
        request.onerror = () =>
          reject(request.error ?? new Error("Local storage operation failed."));
      });
    } catch (error) {
      if (mode !== "local" && !warned) {
        mode = "local";
        warned = true;
        notify(
          `IndexedDB failed (${error.message}); using local browser storage instead.`,
          "warning",
          7000,
        );
      }
      if (action === "all") return fallbackRead(store);
      if (action === "put") return fallbackWrite(store, value);
      return fallbackDelete(store, value);
    }
  }

  return {
    get mode() {
      return mode;
    },
    getAll: (store) => run(store, "all"),
    put: (store, value) => run(store, "put", value),
    delete: (store, id) => run(store, "delete", id),
  };
}

app.addEventListener("click", async (event) => {
  const actionElement = event.target.closest("[data-action]");
  if (!actionElement) return;
  const { action, id } = actionElement.dataset;
  switch (action) {
    case "go-ride":
      location.hash = "#/ride";
      break;
    case "theme":
      state.settings.theme =
        state.settings.theme === "light" ? "dark" : "light";
      persistPreferences();
      render();
      break;
    case "toggle-ride":
      startRide();
      break;
    case "finish-ride":
      await finishRide();
      break;
    case "reset-ride":
      state.ride = createEmptyRide();
      state.tracePoints = [];
      state.recordAccumulator = 0;
      updateRideControls();
      notify("Ride session reset.");
      break;
    case "select-route":
      state.routeId = id;
      if (state.page === "ride") state.world?.setRoute(selectedRoute());
      else notify(`${selectedRoute().name} selected.`);
      if (state.page !== "ride") location.hash = "#/ride";
      else render();
      break;
    case "camera":
      state.camera = id;
      document.querySelectorAll("#camera-controls button").forEach((button) => {
        const selected = button.dataset.id === id;
        button.classList.toggle("selected", selected);
        button.setAttribute("aria-pressed", String(selected));
      });
      break;
    case "import-route":
      state.importTarget = "route";
      document.querySelector("#file-import").click();
      break;
    case "import-activity":
      state.importTarget = "activity";
      document.querySelector("#file-import").click();
      break;
    case "export-activity": {
      const activity = state.activities.find((item) => item.id === id);
      if (activity)
        downloadFile(
          `${safeFilename(activity.name)}.gpx`,
          exportGpx(activity),
          "application/gpx+xml",
        );
      break;
    }
    case "delete-activity":
      state.activities = state.activities.filter((item) => item.id !== id);
      await state.storage.delete("activities", id);
      render();
      notify("Activity deleted from this browser.");
      break;
    case "delete-route":
      state.routes = state.routes.filter((item) => item.id !== id);
      await state.storage.delete("routes", id);
      if (state.routeId === id) state.routeId = DEMO_ROUTES[0].id;
      render();
      notify("Imported route deleted from this browser.");
      break;
    case "connect-device":
      await connectDevice();
      break;
    case "disconnect-device":
      await disconnectDevice();
      break;
    case "export-backup": {
      const backup = {
        version: 1,
        exportedAt: new Date().toISOString(),
        settings: state.settings,
        activities: state.activities,
        routes: state.routes.filter((route) => route.imported),
      };
      downloadFile(
        "velocity-backup.json",
        JSON.stringify(backup, null, 2),
        "application/json",
      );
      break;
    }
  }
});

app.addEventListener("change", async (event) => {
  const { id, value, files } = event.target;
  if (id === "ride-route") {
    state.routeId = value;
    state.world?.setRoute(selectedRoute());
    render();
  } else if (id === "file-import") {
    await importFile(files?.[0]);
    event.target.value = "";
  } else if (id === "theme-setting") {
    state.settings.theme = value;
    persistPreferences();
    render();
  } else if (id === "quality-setting") {
    state.settings.quality = value;
    persistPreferences();
    state.world?.setQuality(value);
  } else if (id === "ftp-setting" || id === "weight-setting") {
    const number = Number(value);
    if (!Number.isFinite(number) || number <= 0) {
      notify("Enter a valid positive number.", "error");
      return;
    }
    state.settings[id === "ftp-setting" ? "ftp" : "weight"] = number;
    persistPreferences();
  }
});

app.addEventListener("input", (event) => {
  const { id, value } = event.target;
  const settingsById = {
    "time-of-day": "timeOfDay",
    "cloud-cover": "cloud",
    rain: "rain",
  };
  if (id === "effort") {
    state.ride.effort = Number(value) / 100;
    setText("#effort-value", `${value}%`);
  } else if (settingsById[id]) {
    state.settings[settingsById[id]] = Number(value);
    const output = document.querySelector(`#${id}-value`);
    if (output)
      output.textContent =
        id === "time-of-day" ? formatHour(Number(value)) : `${value}%`;
    persistPreferences();
  }
});

window.addEventListener("hashchange", render);
window.addEventListener("online", () => notify("Back online.", "success"));
window.addEventListener("offline", () =>
  notify(
    "Offline. Local rides and settings remain available.",
    "warning",
    6500,
  ),
);

async function initialize() {
  state.storage = createStorage();
  try {
    const [savedActivities, savedRoutes, savedSettings] = await Promise.all([
      state.storage.getAll("activities"),
      state.storage.getAll("routes"),
      state.storage.getAll("settings"),
    ]);
    state.activities = savedActivities;
    state.routes.push(
      ...savedRoutes.filter(
        (route) => !state.routes.some((item) => item.id === route.id),
      ),
    );
    const preferences = savedSettings.find((item) => item.id === "preferences");
    if (preferences) state.settings = { ...DEFAULT_SETTINGS, ...preferences };
  } catch (error) {
    console.error("Could not load local VELOCITY data:", error);
    notify(`Local data could not be loaded: ${error.message}`, "error", 7000);
  }
  render();

  if ("serviceWorker" in navigator && import.meta.env.PROD) {
    navigator.serviceWorker
      .register(`${import.meta.env.BASE_URL}sw.js`, {
        scope: import.meta.env.BASE_URL,
      })
      .catch((error) => {
        console.warn("Offline app caching could not be enabled:", error);
      });
  }
}

function safeFilename(value) {
  return (
    String(value || "activity")
      .normalize("NFKD")
      .replace(/[^a-zA-Z0-9_-]+/g, "-")
      .replace(/^-|-$/g, "")
      .slice(0, 60) || "activity"
  );
}

initialize();
