import test from "node:test";
import assert from "node:assert/strict";

import {
  DEMO_ROUTES,
  distanceMeters,
  elevationGain,
  exportGpx,
  formatDuration,
  gradeAt,
  routeLength,
  stepRide,
} from "../web/model.js";

test("demo routes contain closed, geographically plausible courses", () => {
  assert.equal(DEMO_ROUTES.length, 3);
  for (const route of DEMO_ROUTES) {
    assert.equal(route.points[0].lat, route.points.at(-1).lat);
    assert.ok(routeLength(route.points) > 5_000);
    assert.ok(
      route.points.every((point) => point.lat >= 45 && point.lat <= 48),
    );
  }
});

test("haversine distance and route grades use metric coordinates", () => {
  assert.ok(
    Math.abs(
      distanceMeters({ lat: 46, lon: 7 }, { lat: 46, lon: 7.001 }) - 77,
    ) < 3,
  );
  assert.ok(
    gradeAt(
      [
        { lat: 46, lon: 7, ele: 100 },
        { lat: 46, lon: 7.001, ele: 120 },
      ],
      0,
    ) > 10,
  );
  assert.equal(
    elevationGain([{ ele: 20 }, { ele: 25 }, { ele: 18 }, { ele: 27 }]),
    14,
  );
});

test("ride physics advances without negative or unbounded values", () => {
  const next = stepRide(
    { speedMps: 0, distanceM: 0, elapsedS: 0, grade: 4 },
    0.05,
    1,
  );
  assert.ok(next.speedMps > 0);
  assert.ok(next.distanceM > 0);
  assert.equal(next.elapsedS, 0.05);
  assert.ok(
    stepRide({ speedMps: 30, distanceM: 0, elapsedS: 0 }, 1).speedMps <= 20,
  );
});

test("exports preserve route coordinates and produce valid XML text", () => {
  const gpx = exportGpx({
    name: "Rider & route",
    points: [
      { lat: 46.5, lon: 7.5, ele: 812.4 },
      { lat: 46.6, lon: 7.6, ele: null },
    ],
  });
  assert.match(gpx, /Rider &amp; route/);
  assert.match(gpx, /<ele>812\.4<\/ele>/);
  assert.match(gpx, /lat="46\.6000000" lon="7\.6000000"/);
  assert.equal(formatDuration(3661), "01:01:01");
});
