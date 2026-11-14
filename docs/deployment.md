# GitHub Pages deployment

The public browser app is built from `web/` with Vite and published at
<https://catrick-cpu.github.io/velocity/>.

Vite's `base` is deliberately set to `/velocity/` in `vite.config.js`. The
generated app entry, CSS and JavaScript bundles use that prefix; the production
build check rejects root-relative assets and missing files. Client-side pages
use hash URLs (for example `/#/ride`), so direct links and browser refreshes do
not require server rewrite rules.

`.github/workflows/deploy.yml` runs on pushes to `master` and `main`, and on
manual dispatch. It verifies the npm/Cargo lockfiles, formatting, lint, browser
unit tests, native Rust tests, generated Pages paths, and Chromium browser
tests before uploading `dist/` as a Pages artifact. The deploy job uses the
official GitHub Pages Actions and only receives `pages: write` and
`id-token: write`; the build job only has `contents: read`.

For a repository Pages site, enable **Settings → Pages → Build and deployment →
GitHub Actions** once. The existing repository is already public and its Pages
site is configured for workflow deployment. No custom domain, secret, API key,
or server is required.

The application depends only on same-origin build assets and browser storage at
startup. The system fonts are used without a remote font service. Web Bluetooth
requires a secure context and a supporting browser; the app remains usable
without it.

The Ride page optionally requests nearby SWISSIMAGE tiles from swisstopo's
public WMTS and Mapzen Terrarium elevation tiles from AWS Open Data. No token or
paid API is required. The browser caches a limited number of successful map
tiles and falls back to generated scenery if a source is unavailable. Reuse
requires attribution: `© swisstopo`, plus Mapzen and the underlying terrain
data sources; see [architecture](architecture.md) for source and license
details. The elevation data is EU-DEM at roughly 25–30 m resolution, not
swissALTI3D.
