export const DEMO_ROUTES = [
  {
    id: "alpine",
    name: "Alpine training loop",
    area: "Grindelwald · generated course",
    lengthM: 18_200,
    climbM: 620,
    points: loopPoints(46.624, 8.041, 0.043, 0.026, 18, 730, 620),
  },
  {
    id: "lakeside",
    name: "Lakeside circuit",
    area: "Lake Lucerne · generated course",
    lengthM: 14_500,
    climbM: 240,
    points: loopPoints(46.997, 8.535, 0.034, 0.021, 16, 430, 240),
  },
  {
    id: "forest",
    name: "Forest valley ride",
    area: "Emmental · generated course",
    lengthM: 12_100,
    climbM: 310,
    points: loopPoints(46.923, 7.729, 0.031, 0.024, 17, 520, 310),
  },
];

export function loopPoints(lat, lon, latRadius, lonRadius, count, base, climb) {
  const points = Array.from({ length: count + 1 }, (_, index) => {
    const angle = (index / count) * Math.PI * 2;
    const wave =
      Math.sin(angle * 2.1 + 0.3) * 0.23 + Math.sin(angle * 4.2) * 0.1;
    return {
      lat: lat + Math.sin(angle) * latRadius + wave * latRadius,
      lon:
        lon +
        Math.cos(angle) * lonRadius +
        Math.sin(angle * 3) * lonRadius * 0.12,
      ele:
        base +
        climb *
          (0.5 + 0.34 * Math.sin(angle - 0.8) + 0.12 * Math.sin(angle * 2.7)),
    };
  });
  points[points.length - 1] = { ...points[0] };
  return points;
}

export function distanceMeters(a, b) {
  const radians = Math.PI / 180;
  const lat1 = a.lat * radians;
  const lat2 = b.lat * radians;
  const dLat = (b.lat - a.lat) * radians;
  const dLon = (b.lon - a.lon) * radians;
  const x = dLon * Math.cos((lat1 + lat2) / 2);
  return 6_371_000 * Math.hypot(x, dLat);
}

export function routeLength(points) {
  let total = 0;
  for (let index = 1; index < points.length; index += 1) {
    total += distanceMeters(points[index - 1], points[index]);
  }
  return total;
}

export function gradeAt(points, index) {
  const from = points[Math.max(0, index - 1)];
  const to = points[Math.min(points.length - 1, index + 1)];
  const distance = distanceMeters(from, to);
  if (!distance || from.ele == null || to.ele == null) return 0;
  return ((to.ele - from.ele) / distance) * 100;
}

export function stepRide(state, seconds, effort = 1) {
  const dt = Math.min(Math.max(seconds, 0), 0.1);
  const grade = Number(state.grade ?? 0);
  const targetPower = 110 + 190 * effort;
  const drag = 0.24 + Math.max(0, grade) * 0.004;
  const acceleration =
    targetPower / 78 - drag * state.speedMps - Math.max(0, grade) * 0.14;
  const speedMps = Math.min(
    20,
    Math.max(0, state.speedMps + acceleration * dt * 0.55),
  );
  return {
    ...state,
    speedMps,
    distanceM: state.distanceM + speedMps * dt,
    elapsedS: state.elapsedS + dt,
    powerW: Math.round(targetPower),
    cadenceRpm: Math.round(68 + effort * 28 + speedMps * 1.2),
  };
}

export function formatDuration(seconds) {
  const total = Math.floor(seconds);
  return `${String(Math.floor(total / 3600)).padStart(2, "0")}:${String(
    Math.floor((total % 3600) / 60),
  ).padStart(2, "0")}:${String(total % 60).padStart(2, "0")}`;
}

export function activityFromRide(state, route, now = new Date()) {
  return {
    id: crypto.randomUUID(),
    name: route.name,
    date: now.toISOString(),
    distanceM: state.distanceM,
    elapsedS: state.elapsedS,
    avgPowerW: state.elapsedS ? Math.round(state.powerW) : 0,
    climbM: route.climbM ?? 0,
    simulated: true,
    points: route.points,
  };
}

export function parseTrackXml(xmlText, fileName = "Imported route") {
  if (typeof DOMParser === "undefined") {
    throw new Error("Track import requires a browser XML parser.");
  }
  const document = new DOMParser().parseFromString(xmlText, "application/xml");
  if (document.querySelector("parsererror"))
    throw new Error("The selected file is not valid XML.");

  const trackPoints = [...document.getElementsByTagNameNS("*", "trkpt")];
  const gpxPoints =
    trackPoints.length > 0
      ? trackPoints
      : [...document.getElementsByTagNameNS("*", "rtept")];
  const points =
    gpxPoints.length > 0
      ? gpxPoints.map((point) => ({
          lat: Number(point.getAttribute("lat")),
          lon: Number(point.getAttribute("lon")),
          ele: optionalNumber(
            point.getElementsByTagNameNS("*", "ele")[0]?.textContent,
          ),
        }))
      : [...document.getElementsByTagNameNS("*", "Trackpoint")].map(
          (point) => ({
            lat: Number(
              point.getElementsByTagNameNS("*", "LatitudeDegrees")[0]
                ?.textContent,
            ),
            lon: Number(
              point.getElementsByTagNameNS("*", "LongitudeDegrees")[0]
                ?.textContent,
            ),
            ele: optionalNumber(
              point.getElementsByTagNameNS("*", "AltitudeMeters")[0]
                ?.textContent,
            ),
          }),
        );

  const valid = points.filter(
    (point) =>
      Number.isFinite(point.lat) &&
      Number.isFinite(point.lon) &&
      Math.abs(point.lat) <= 90 &&
      Math.abs(point.lon) <= 180,
  );
  if (valid.length < 2)
    throw new Error("No usable GPS track points were found.");
  if (valid.length > 100_000)
    throw new Error("This file exceeds the 100,000-point import limit.");
  const nameNode =
    document.getElementsByTagNameNS("*", "name")[0] ??
    document.getElementsByTagNameNS("*", "Activity")[0];
  const name =
    nameNode?.textContent?.trim() || fileName.replace(/\.(gpx|tcx)$/i, "");
  const distanceM = routeLength(valid);
  return {
    id: crypto.randomUUID(),
    name,
    area: "Imported GPS track",
    lengthM: distanceM,
    climbM: elevationGain(valid),
    points: valid,
    imported: true,
  };
}

export function elevationGain(points) {
  let gain = 0;
  for (let index = 1; index < points.length; index += 1) {
    const delta = points[index].ele - points[index - 1].ele;
    if (Number.isFinite(delta) && delta > 0) gain += delta;
  }
  return gain;
}

export function exportGpx(activity) {
  const points = activity.points ?? [];
  const track = points
    .map(
      (point) =>
        `<trkpt lat="${point.lat.toFixed(7)}" lon="${point.lon.toFixed(7)}">${
          Number.isFinite(point.ele) ? `<ele>${point.ele.toFixed(1)}</ele>` : ""
        }</trkpt>`,
    )
    .join("");
  return `<?xml version="1.0" encoding="UTF-8"?>\n<gpx version="1.1" creator="VELOCITY" xmlns="http://www.topografix.com/GPX/1/1"><trk><name>${escapeXml(activity.name)}</name><trkseg>${track}</trkseg></trk></gpx>`;
}

function optionalNumber(value) {
  if (value == null || value.trim() === "") return null;
  const number = Number(value);
  return Number.isFinite(number) ? number : null;
}

function escapeXml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&apos;");
}
