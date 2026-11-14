import test from "node:test";
import assert from "node:assert/strict";

import {
  blendRegionalTerrainHeight,
  decodeTerrariumRgb,
  isSwissImageCoverage,
  swissGrid,
  swissImageTileAddress,
  terrariumTileAddress,
} from "../web/swiss-data.js";

test("Swiss-grid projection matches swisstopo coordinates in Grindelwald", () => {
  const grid = swissGrid(46.624, 8.067);
  assert.ok(Math.abs(grid.easting - 2_648_125.555) < 0.1);
  assert.ok(Math.abs(grid.northing - 1_163_831.074) < 0.1);
  assert.deepEqual(swissImageTileAddress(46.624, 8.067), {
    row: 290,
    col: 356,
  });
  assert.deepEqual(swissImageTileAddress(46.626922694843884, 8.067, 25), {
    row: 725,
    col: 891,
  });
  assert.deepEqual(swissImageTileAddress(46.626922694843884, 8.067, 27), {
    row: 2903,
    col: 3564,
  });
  assert.deepEqual(terrariumTileAddress(46.626922694843884, 8.067, 14), {
    x: 8559,
    y: 5787,
  });
});

test("Terrarium tiles use XYZ coordinates and decode signed elevations", () => {
  assert.deepEqual(terrariumTileAddress(46.624, 8.067, 15), {
    x: 17_118,
    y: 11_575,
  });
  assert.equal(decodeTerrariumRgb(128, 0, 0), 0);
  assert.equal(decodeTerrariumRgb(128, 1, 128), 1.5);
  assert.equal(decodeTerrariumRgb(127, 255, 0), -1);
});

test("regional terrain blends smoothly into the procedural boundary", () => {
  assert.equal(blendRegionalTerrainHeight(800, -40, 0, 3500), 800);
  assert.equal(blendRegionalTerrainHeight(800, -40, 3500, 3500), -40);
  const boundaryHeight = blendRegionalTerrainHeight(800, -40, 300, 3500);
  assert.ok(boundaryHeight < 800);
  assert.ok(boundaryHeight > 700);
});

test("Swiss imagery coverage is bounded to the published WMTS extent", () => {
  assert.equal(isSwissImageCoverage(46.624, 8.067), true);
  assert.equal(isSwissImageCoverage(40.7128, -74.006), false);
});
