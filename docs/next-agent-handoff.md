# VELOCITY handoff: real 360° street imagery

## Goal

Improve the browser ride visuals using genuine street-level imagery rather
than trying to make aerial orthophotos look like Street View. Keep the Swiss
ride as the default. Add an explicitly optional street-view ride in a place
with real coverage; Paris is currently verified.

## Verified source

Panoramax is a public, open street-imagery platform. A verified Paris item:

- Item: `cba46ce6-49af-41ca-83f8-66e2dd67a511`
- API metadata:
  <https://api.panoramax.xyz/api/collections/c357d9bb-0b14-4f90-8fb6-8f52201a53b2/items/cba46ce6-49af-41ca-83f8-66e2dd67a511>
- Original image:
  <https://panoramax.openstreetmap.fr/images/cb/a4/6c/e6/49af-41ca-83f8-66e2dd67a511.jpg>
- Sequence collection: `c357d9bb-0b14-4f90-8fb6-8f52201a53b2`
- Position: longitude `2.319994731`, latitude `48.855030563` (Paris)
- Captured: `2025-07-01T09:14:50Z`; this is recorded imagery, not live video.
- Resolution: **5760 × 2880**, 360° field of view, equirectangular projection.
  The API reports a GoPro Max and confirms `Xmp.GPano.ProjectionType =
  equirectangular`.
- Download size observed: about 3.0 MB.
- License: **CC BY-SA 4.0**. API producer: `benoitdd`; source instance:
  Panoramax OpenStreetMap France. Preserve attribution, link the item and
  license, and meet share-alike requirements for adapted imagery.
- Public API and original image worked without an API key. Image server
  returned HTTP 200 and `Access-Control-Allow-Origin: *`, allowing WebGL
  texture use from the browser at the time checked.
- Alternate close-up sample:
  <https://panoramax-storage-public-fast.s3.gra.perf.cloud.ovh.net/main-pictures/71/3f/0c/67/5e1a-4b98-8750-33d9624bf007.jpg>
  is 4096 × 2160, but only a 95° perspective frame. Prefer the equirectangular
  item above for an immersive viewer.

Search API used:

```text
https://api.panoramax.xyz/api/search?bbox=2.318,48.854,2.323,48.857&limit=100
```

The query found multiple images in the same 360° collection, including ranks
344–352, captured two seconds apart and spaced along the road. Each observed
frame reported 5760 × 2880 and 360° FOV. The item metadata also provides
`prev`/`next` links for walking the sequence. A query near the existing
Grindelwald route returned no imagery, so do not imply this dataset covers
the Swiss route.

License reference: <https://creativecommons.org/licenses/by-sa/4.0/>

## Current project/deployment state

- Repository: <https://github.com/Catrick-cpu/velocity>
- Live app: <https://catrick-cpu.github.io/velocity/>
- Browser stack: Vite, JavaScript, and Three.js; static GitHub Pages hosting
  under `/velocity/`.
- Latest deployed commit at the time of research: `39411d4` (10 cm SWISSIMAGE
  close-range aerial imagery). The Swiss scene uses aerial imagery, hillshade,
  OSM buildings, and EU-DEM terrain; it is not street-level photography.
- Prior report: the latest deploy passed GitHub Actions and browser checks.
  Do not claim a new deployment or integration based on this source research.
- No Panoramax application code has been added yet.

## Recommended implementation

1. Inspect `web/app.js`, `web/world.js`, `web/swiss-data.js`, and existing
   styles/tests before editing. Preserve current Swiss functionality and
   hash/base-path routing.
2. Add a clearly optional “Paris · Panoramax 360°” ride/view. Identify it as
   captured street imagery with its capture date, not live footage or a
   photorealistic 3D world. Do not silently relocate the default Swiss ride.
3. Use the collection's `next`/`prev` links and item positions to traverse
   frames. Load the initial image, then prefetch only adjacent frames; do not
   download the whole collection or commit its images to this repository.
4. Render the equirectangular image in the existing Three.js renderer (or an
   equivalent viewer) with camera yaw/pitch controls. Check the image's
   heading/orientation metadata and test that the forward direction matches
   the road. In an optional ride mode, advance between captured camera
   positions using activity speed and frame positions; be transparent that
   movement is discrete and cannot reproduce continuous parallax or dynamic
   traffic.
5. Show measured loading/error states. If Panoramax/API/image hosting fails,
   explain the unavailable data and offer the existing Swiss ride; do not
   present a blank scene or pretend the imagery loaded. Consider the 2048 px
   `sd` derivative for constrained connections and use `hd` only when needed.
6. Put visible attribution in the viewer (Panoramax, producer `benoitdd`, item
   link, CC BY-SA 4.0); document the license and any changes. Recheck the
   source's current license, attribution metadata, CORS, and service limits
   before release. It is an external dependency, not a guaranteed SLA.
7. Add focused tests for API parsing, adjacent-frame navigation, base-path
   correctness, failure/offline behavior, and image loading. Run the existing
   test/lint/build checks, then browser-test with network and console errors
   recorded and capture a screenshot.
8. If deploying, use the existing GitHub Actions workflow and verify the live
   `/velocity/` URL. Do not commit unrelated user changes.

## Important worktree note

At handoff, `git status` showed pre-existing modified native Rust files:

- `crates/velocity-app/src/world/dem.rs`
- `crates/velocity-core/Cargo.toml`
- `crates/velocity-core/src/db.rs`
- `crates/velocity-sensors/Cargo.toml`
- `crates/velocity-sensors/src/lib.rs`

They are unrelated to this imagery research. Leave them intact and do not
stage or commit them unless their owner explicitly asks.
