import { VectorTile } from "@mapbox/vector-tile";
import { PbfReader } from "pbf";

const SWISSTOPO_TILE_URL =
  "https://wmts.geo.admin.ch/1.0.0/ch.swisstopo.swissimage/default/current/21781";
const SWISSTOPO_HILLSHADE_URL =
  "https://wmts.geo.admin.ch/1.0.0/ch.swisstopo.swissalti3d-reliefschattierung/default/current/21781";
const TERRARIUM_TILE_URL =
  "https://s3.amazonaws.com/elevation-tiles-prod/terrarium";
const OPENFREEMAP_TILEJSON_URL = "https://tiles.openfreemap.org/planet";
const OPENFREEMAP_ORIGIN = new URL(OPENFREEMAP_TILEJSON_URL).origin;
const CACHE_NAME = "velocity-geodata-v1";
const CACHE_LIMIT = 96;
const CACHE_MAX_AGE = 30 * 24 * 60 * 60 * 1000;
const TILE_SIZE = 256;
const WMTS_ORIGIN = { easting: 420_000, northing: 350_000 };
const WMTS_RESOLUTIONS = new Map([
  [22, 8928.57142857143 * 0.00028],
  [23, 7142.857142857143 * 0.00028],
  [24, 5357.142857142858 * 0.00028],
  [25, 3571.4285714285716 * 0.00028],
  [26, 0.5],
  [27, 0.25],
  [28, 0.1],
]);
const TILE_CONCURRENCY = 4;
const IMAGE_TILE_RADIUS = 2;
const IMAGE_TILE_MATRIX = 22;
const DETAIL_TILE_RADIUS = 1;
const DETAIL_TILE_MATRIX = 25;
const ULTRA_DETAIL_TILE_RADIUS = 2;
const ULTRA_DETAIL_TILE_MATRIX = 28;
const HILLSHADE_TILE_MATRIX = 25;
const VECTOR_TILE_ZOOM = 14;
const VECTOR_TILE_RADIUS = 1;
const ELEVATION_ZOOM = 14;
const EARTH_RADIUS = 6_378_137;

export function swissGrid(lat, lon) {
  const latAux = (lat * 3600 - 169_028.66) / 10_000;
  const lonAux = (lon * 3600 - 26_782.5) / 10_000;
  const easting =
    2_000_000 +
    600_072.37 +
    211_455.93 * lonAux -
    10_938.51 * lonAux * latAux -
    0.36 * lonAux * latAux ** 2 -
    44.54 * lonAux ** 3;
  const northing =
    1_000_000 +
    200_147.07 +
    308_807.95 * latAux +
    3745.25 * lonAux ** 2 +
    76.63 * latAux ** 2 -
    194.56 * lonAux ** 2 * latAux +
    119.79 * latAux ** 3;
  return { easting, northing };
}

export function swissImageTileAddress(lat, lon, matrix = IMAGE_TILE_MATRIX) {
  const resolution = WMTS_RESOLUTIONS.get(matrix);
  if (!resolution) {
    throw new RangeError(`Unsupported SWISSIMAGE tile matrix: ${matrix}`);
  }
  const { easting, northing } = swissGrid(lat, lon);
  const lv03Easting = easting - 2_000_000;
  const lv03Northing = northing - 1_000_000;
  const tileSpan = resolution * TILE_SIZE;
  return {
    col: Math.floor((lv03Easting - WMTS_ORIGIN.easting) / tileSpan),
    row: Math.floor((WMTS_ORIGIN.northing - lv03Northing) / tileSpan),
  };
}

export function terrariumTileAddress(lat, lon, zoom = 15) {
  const count = 2 ** zoom;
  const latitude = (lat * Math.PI) / 180;
  return {
    x: Math.floor(((lon + 180) / 360) * count),
    y: Math.floor(((1 - Math.asinh(Math.tan(latitude)) / Math.PI) / 2) * count),
  };
}

export function decodeTerrariumRgb(red, green, blue) {
  return red * 256 + green + blue / 256 - 32_768;
}

export function blendRegionalTerrainHeight(
  regionalHeight,
  fallbackHeight,
  distance,
  transitionDistance,
) {
  const progress = Math.max(0, Math.min(distance / transitionDistance, 1));
  const blend = progress * progress * (3 - 2 * progress);
  return regionalHeight + (fallbackHeight - regionalHeight) * blend;
}

export function isSwissImageCoverage(lat, lon) {
  return (
    lat >= 45.398181 && lat <= 48.230651 && lon >= 5.140242 && lon <= 11.47757
  );
}

export function swissImageRegionKey(lat, lon) {
  const { row, col } = swissImageTileAddress(lat, lon);
  return `${row}/${col}`;
}

export async function loadSwissRegion({ center, origin, signal, onProgress }) {
  if (!isSwissImageCoverage(center.lat, center.lon)) {
    throw new Error(
      "The selected location is outside the SWISSIMAGE coverage.",
    );
  }

  const imageCenter = swissImageTileAddress(center.lat, center.lon);
  const detailCenter = swissImageTileAddress(
    center.lat,
    center.lon,
    DETAIL_TILE_MATRIX,
  );
  const ultraDetailCenter = swissImageTileAddress(
    center.lat,
    center.lon,
    ULTRA_DETAIL_TILE_MATRIX,
  );
  const hillshadeCenter = swissImageTileAddress(
    center.lat,
    center.lon,
    HILLSHADE_TILE_MATRIX,
  );
  const elevationCenter = terrariumTileAddress(
    center.lat,
    center.lon,
    ELEVATION_ZOOM,
  );
  const buildingCenter = terrariumTileAddress(
    center.lat,
    center.lon,
    VECTOR_TILE_ZOOM,
  );
  let buildingTileTemplate = null;
  let buildingError = null;
  try {
    const tileJson = await fetchTile(OPENFREEMAP_TILEJSON_URL, signal);
    const metadata = JSON.parse(await tileJson.blob.text());
    if (metadata.vector_layers?.some((layer) => layer.id === "building")) {
      buildingTileTemplate = metadata.tiles?.[0] ?? null;
    }
  } catch (error) {
    if (signal?.aborted) throw error;
    buildingError = error;
  }
  const imageTiles = tilesAround(
    imageCenter.col,
    imageCenter.row,
    IMAGE_TILE_RADIUS,
  );
  const detailTiles = tilesAround(
    detailCenter.col,
    detailCenter.row,
    DETAIL_TILE_RADIUS,
  );
  const ultraDetailTiles = tilesAround(
    ultraDetailCenter.col,
    ultraDetailCenter.row,
    ULTRA_DETAIL_TILE_RADIUS,
  );
  const hillshadeTiles = tilesAround(
    hillshadeCenter.col,
    hillshadeCenter.row,
    DETAIL_TILE_RADIUS,
  );
  const elevationTiles = tilesAround(elevationCenter.x, elevationCenter.y);
  const buildingTiles = buildingTileTemplate
    ? tilesAround(buildingCenter.x, buildingCenter.y, VECTOR_TILE_RADIUS)
    : [];
  const jobs = [
    ...imageTiles.map((tile) => ({
      kind: "image",
      matrix: IMAGE_TILE_MATRIX,
      tile,
    })),
    ...detailTiles.map((tile) => ({
      kind: "detail",
      matrix: DETAIL_TILE_MATRIX,
      tile,
    })),
    ...ultraDetailTiles.map((tile) => ({
      kind: "ultra",
      matrix: ULTRA_DETAIL_TILE_MATRIX,
      tile,
    })),
    ...hillshadeTiles.map((tile) => ({
      kind: "hillshade",
      matrix: HILLSHADE_TILE_MATRIX,
      tile,
    })),
    ...elevationTiles.map((tile) => ({ kind: "elevation", tile })),
    ...buildingTiles.map((tile) => ({ kind: "building", tile })),
  ];
  let completed = 0;
  const optionalErrors = {};
  const loaded = await mapConcurrent(jobs, TILE_CONCURRENCY, async (job) => {
    try {
      if (["image", "detail", "ultra", "hillshade"].includes(job.kind)) {
        if (job.kind === "hillshade") {
          const url = `${SWISSTOPO_HILLSHADE_URL}/${job.matrix}/${job.tile.row}/${job.tile.col}.png`;
          const response = await fetchTile(url, signal);
          const tile = {
            kind: job.kind,
            tile: job.tile,
            bitmap: await globalThis.createImageBitmap(response.blob),
            fromCache: response.fromCache,
          };
          completed += 1;
          onProgress?.(completed, jobs.length);
          return tile;
        }
        const url = `${SWISSTOPO_TILE_URL}/${job.matrix}/${job.tile.row}/${job.tile.col}.jpeg`;
        const response = await fetchTile(url, signal);
        const tile = {
          kind: job.kind,
          tile: job.tile,
          bitmap: await globalThis.createImageBitmap(response.blob),
          fromCache: response.fromCache,
        };
        completed += 1;
        onProgress?.(completed, jobs.length);
        return tile;
      }

      if (job.kind === "building") {
        const url = openFreeMapTileUrl(
          buildingTileTemplate,
          job.tile.x,
          job.tile.y,
          VECTOR_TILE_ZOOM,
        );
        const response = await fetchTile(url, signal);
        completed += 1;
        onProgress?.(completed, jobs.length);
        return {
          kind: job.kind,
          tile: job.tile,
          blob: response.blob,
          fromCache: response.fromCache,
        };
      }

      const url = `${TERRARIUM_TILE_URL}/${ELEVATION_ZOOM}/${job.tile.x}/${job.tile.y}.png`;
      const response = await fetchTile(url, signal);
      const bitmap = await globalThis.createImageBitmap(response.blob);
      const canvas = document.createElement("canvas");
      canvas.width = TILE_SIZE;
      canvas.height = TILE_SIZE;
      const context = canvas.getContext("2d", { willReadFrequently: true });
      if (!context) {
        bitmap.close();
        throw new Error(
          "Canvas pixel access is unavailable for elevation data.",
        );
      }
      context.drawImage(bitmap, 0, 0);
      bitmap.close();
      const tile = {
        kind: "elevation",
        tile: job.tile,
        pixels: context.getImageData(0, 0, TILE_SIZE, TILE_SIZE).data,
        source: response.headers.get("x-amz-meta-x-imagery-sources"),
        fromCache: response.fromCache,
      };
      completed += 1;
      onProgress?.(completed, jobs.length);
      return tile;
    } catch (error) {
      if (
        !["detail", "ultra", "hillshade", "building"].includes(job.kind) ||
        signal?.aborted
      ) {
        throw error;
      }
      optionalErrors[job.kind] ??= error;
      completed += 1;
      onProgress?.(completed, jobs.length);
      return null;
    }
  });

  const loadedTiles = loaded.filter(Boolean);
  const originGrid = swissGrid(origin.lat, origin.lon);
  const imageLayers = [];
  try {
    imageLayers.push(
      createImageLayer(
        loadedTiles.filter((tile) => tile.kind === "image"),
        imageCenter,
        originGrid,
        IMAGE_TILE_MATRIX,
        IMAGE_TILE_RADIUS,
      ),
    );
    const loadedDetailTiles = loadedTiles.filter(
      (tile) => tile.kind === "detail",
    );
    const layerTileCount = (DETAIL_TILE_RADIUS * 2 + 1) ** 2;
    const completeDetailTiles =
      !optionalErrors.detail && loadedDetailTiles.length === layerTileCount;
    if (completeDetailTiles) {
      imageLayers.push(
        createImageLayer(
          loadedDetailTiles,
          detailCenter,
          originGrid,
          DETAIL_TILE_MATRIX,
          DETAIL_TILE_RADIUS,
        ),
      );
    } else {
      closeImageTiles(loadedDetailTiles);
    }
    const loadedUltraTiles = loadedTiles.filter(
      (tile) => tile.kind === "ultra",
    );
    const ultraLayerTileCount = (ULTRA_DETAIL_TILE_RADIUS * 2 + 1) ** 2;
    const completeUltraTiles =
      !optionalErrors.ultra && loadedUltraTiles.length === ultraLayerTileCount;
    if (completeUltraTiles) {
      imageLayers.push(
        createImageLayer(
          loadedUltraTiles,
          ultraDetailCenter,
          originGrid,
          ULTRA_DETAIL_TILE_MATRIX,
          ULTRA_DETAIL_TILE_RADIUS,
        ),
      );
    } else {
      closeImageTiles(loadedUltraTiles);
    }

    const loadedHillshadeTiles = loadedTiles.filter(
      (tile) => tile.kind === "hillshade",
    );
    if (
      !optionalErrors.hillshade &&
      loadedHillshadeTiles.length === layerTileCount
    ) {
      const hillshade = createImageLayer(
        loadedHillshadeTiles,
        hillshadeCenter,
        originGrid,
        HILLSHADE_TILE_MATRIX,
        DETAIL_TILE_RADIUS,
      );
      imageLayers
        .filter((layer) => layer.matrix > IMAGE_TILE_MATRIX)
        .forEach((layer) => blendHillshade(layer, hillshade));
    } else {
      closeImageTiles(loadedHillshadeTiles);
    }
  } catch (error) {
    closeImageTiles(loadedTiles);
    throw error;
  }
  if (optionalErrors.detail || optionalErrors.ultra) {
    console.warn(
      "Some high-resolution SWISSIMAGE tiles are unavailable; using the best available regional imagery.",
      optionalErrors.detail ?? optionalErrors.ultra,
    );
  }
  if (optionalErrors.hillshade) {
    console.warn(
      "SwissALTI3D hillshade is unavailable; using the aerial imagery without terrain shading.",
      optionalErrors.hillshade,
    );
  }
  if (buildingError || optionalErrors.building) {
    console.warn(
      "OpenFreeMap building tiles are unavailable; continuing without 3D buildings.",
      buildingError ?? optionalErrors.building,
    );
  }

  let buildings = [];
  try {
    buildings = await decodeBuildingTiles(
      loadedTiles.filter((tile) => tile.kind === "building"),
      originGrid,
    );
  } catch (error) {
    console.warn(
      "OpenFreeMap building geometry could not be decoded; continuing without 3D buildings.",
      error,
    );
  }

  const elevationData = new Map(
    loadedTiles
      .filter((tile) => tile.kind === "elevation")
      .map((tile) => [`${tile.tile.x}/${tile.tile.y}`, tile.pixels]),
  );
  const bounds = imageLayers[0].bounds;
  const detailLayer =
    imageLayers.filter((layer) => layer.matrix > IMAGE_TILE_MATRIX).at(-1) ??
    null;
  const terrainBounds = getTerrainBounds(
    elevationCenter,
    origin,
    ELEVATION_ZOOM,
  );

  const heightAtGeo = (lat, lon) => sampleElevation(elevationData, lat, lon);
  const referenceElevation = heightAtGeo(center.lat, center.lon);
  if (!Number.isFinite(referenceElevation)) {
    throw new Error("No usable elevation sample was returned for this region.");
  }

  const originLatitudeRadians = (origin.lat * Math.PI) / 180;
  const heightAtWorld = (x, z) => {
    const lat = origin.lat - z / 110_574;
    const lon = origin.lon + x / (111_320 * Math.cos(originLatitudeRadians));
    return heightAtGeo(lat, lon);
  };

  return {
    key: `${imageCenter.row}/${imageCenter.col}`,
    bounds,
    detailBounds: detailLayer?.bounds ?? null,
    terrainBounds,
    imageLayers,
    buildings,
    heightAtGeo,
    heightAtWorld,
    referenceElevation,
    containsWorld: (x, z) =>
      x >= bounds.minX &&
      x <= bounds.maxX &&
      z >= bounds.minZ &&
      z <= bounds.maxZ,
    tileCount: jobs.length,
    cachedTileCount: loadedTiles.filter((tile) => tile.fromCache).length,
    source: [
      ...new Set(
        loadedTiles
          .filter((tile) => tile.kind === "elevation" && tile.source)
          .flatMap((tile) => tile.source.split(",").map((item) => item.trim())),
      ),
    ],
  };
}

function createImageLayer(tiles, center, origin, matrix, radius) {
  const side = radius * 2 + 1;
  const canvas = document.createElement("canvas");
  canvas.width = TILE_SIZE * side;
  canvas.height = TILE_SIZE * side;
  const context = canvas.getContext("2d");
  if (!context) {
    closeImageTiles(tiles);
    throw new Error("Canvas rendering is unavailable for Swiss map tiles.");
  }
  for (const tile of tiles) {
    context.drawImage(
      tile.bitmap,
      (tile.tile.col - center.col + radius) * TILE_SIZE,
      (tile.tile.row - center.row + radius) * TILE_SIZE,
    );
    tile.bitmap.close();
  }

  const resolution = WMTS_RESOLUTIONS.get(matrix);
  if (!resolution) {
    throw new RangeError(`Unsupported SWISSIMAGE tile matrix: ${matrix}`);
  }
  const tileSpan = resolution * TILE_SIZE;
  const minCol = center.col - radius;
  const minRow = center.row - radius;
  const minEasting = WMTS_ORIGIN.easting + minCol * tileSpan + 2_000_000;
  const maxEasting = minEasting + TILE_SIZE * side * resolution;
  const maxNorthing = WMTS_ORIGIN.northing - minRow * tileSpan + 1_000_000;
  const minNorthing = maxNorthing - TILE_SIZE * side * resolution;
  return {
    detail: matrix > IMAGE_TILE_MATRIX,
    matrix,
    lift:
      matrix === ULTRA_DETAIL_TILE_MATRIX
        ? 0.36
        : matrix > IMAGE_TILE_MATRIX
          ? 0.12
          : 0,
    bounds: {
      minX: minEasting - origin.easting,
      maxX: maxEasting - origin.easting,
      minZ: origin.northing - maxNorthing,
      maxZ: origin.northing - minNorthing,
    },
    textureCanvas: canvas,
  };
}

function blendHillshade(imageLayer, hillshadeLayer) {
  const image = imageLayer.bounds;
  const shade = hillshadeLayer.bounds;
  if (
    image.minX < shade.minX ||
    image.maxX > shade.maxX ||
    image.minZ < shade.minZ ||
    image.maxZ > shade.maxZ
  ) {
    return;
  }
  const resolution =
    (shade.maxX - shade.minX) / hillshadeLayer.textureCanvas.width;
  const sourceX = (image.minX - shade.minX) / resolution;
  const sourceY = (shade.maxZ - image.maxZ) / resolution;
  const sourceSize = (image.maxX - image.minX) / resolution;
  const context = imageLayer.textureCanvas.getContext("2d");
  if (!context) return;
  context.save();
  context.globalAlpha = 0.18;
  context.globalCompositeOperation = "multiply";
  context.drawImage(
    hillshadeLayer.textureCanvas,
    sourceX,
    sourceY,
    sourceSize,
    sourceSize,
    0,
    0,
    imageLayer.textureCanvas.width,
    imageLayer.textureCanvas.height,
  );
  context.restore();
  imageLayer.texture?.dispose();
  imageLayer.texture = null;
}

function openFreeMapTileUrl(template, x, y, zoom) {
  const url = new URL(
    template
      .replace("{z}", String(zoom))
      .replace("{x}", String(x))
      .replace("{y}", String(y)),
  );
  if (url.origin !== OPENFREEMAP_ORIGIN) {
    throw new Error("OpenFreeMap returned an unexpected vector tile host.");
  }
  return url.href;
}

async function decodeBuildingTiles(tiles, origin) {
  const buildings = [];
  const seen = new Set();
  for (const tile of tiles) {
    const vectorTile = new VectorTile(
      new PbfReader(new Uint8Array(await tile.blob.arrayBuffer())),
    );
    const layer = vectorTile.layers.building;
    if (!layer) continue;
    for (let index = 0; index < layer.length; index += 1) {
      const feature = layer.feature(index);
      if (feature.type !== 3) continue;
      const geometry = feature.toGeoJSON(
        tile.tile.x,
        tile.tile.y,
        VECTOR_TILE_ZOOM,
      ).geometry;
      const polygons =
        geometry.type === "Polygon"
          ? [geometry.coordinates]
          : geometry.type === "MultiPolygon"
            ? geometry.coordinates
            : [];
      for (const polygon of polygons) {
        const rings = polygon.map((sourceRing) =>
          sourceRing.slice(0, -1).map(([lon, lat]) => {
            const grid = swissGrid(lat, lon);
            return {
              x: grid.easting - origin.easting,
              z: origin.northing - grid.northing,
            };
          }),
        );
        const footprint = rings[0];
        if (!footprint || footprint.length < 3) continue;
        const centerX =
          footprint.reduce((sum, point) => sum + point.x, 0) / footprint.length;
        const centerZ =
          footprint.reduce((sum, point) => sum + point.z, 0) / footprint.length;
        const key = `${feature.id ?? "building"}:${Math.round(centerX)}:${Math.round(centerZ)}`;
        if (seen.has(key)) continue;
        seen.add(key);
        const height = Number(feature.properties.render_height);
        const totalHeight = Number.isFinite(height)
          ? Math.max(3, Math.min(height, 100))
          : 5;
        const minHeight = Number(feature.properties.render_min_height);
        buildings.push({
          id: key,
          footprint,
          holes: rings.slice(1).filter((ring) => ring.length >= 3),
          height: totalHeight,
          minHeight: Number.isFinite(minHeight)
            ? Math.max(0, Math.min(minHeight, totalHeight - 1))
            : 0,
          color:
            typeof feature.properties.colour === "string"
              ? feature.properties.colour
              : null,
        });
      }
    }
  }
  return buildings;
}

function closeImageTiles(tiles) {
  tiles.filter((tile) => tile?.bitmap).forEach((tile) => tile.bitmap.close());
}

function getTerrainBounds(center, origin, zoom) {
  const tileCount = 2 ** zoom;
  const tileSpan = (2 * Math.PI * EARTH_RADIUS) / tileCount;
  const worldHalf = Math.PI * EARTH_RADIUS;
  const originX = (origin.lon * Math.PI * EARTH_RADIUS) / 180;
  const cosLatitude = Math.cos((origin.lat * Math.PI) / 180);
  const minWorldX = (center.x - 1) * tileSpan - worldHalf;
  const maxWorldX = (center.x + 2) * tileSpan - worldHalf;
  const northLatitude = tileYToLatitude(center.y - 1, tileCount);
  const southLatitude = tileYToLatitude(center.y + 2, tileCount);
  return {
    minX: (minWorldX - originX) * cosLatitude,
    maxX: (maxWorldX - originX) * cosLatitude,
    minZ: -(northLatitude - origin.lat) * 110_574,
    maxZ: -(southLatitude - origin.lat) * 110_574,
  };
}

function tileYToLatitude(y, tileCount) {
  return (
    (Math.atan(Math.sinh(Math.PI * (1 - (2 * y) / tileCount))) * 180) / Math.PI
  );
}

function tilesAround(x, y, radius = 1) {
  const side = radius * 2 + 1;
  return Array.from({ length: side * side }, (_, index) => ({
    x: x + (index % side) - radius,
    y: y + Math.floor(index / side) - radius,
    col: x + (index % side) - radius,
    row: y + Math.floor(index / side) - radius,
  }));
}

function sampleElevation(tiles, lat, lon) {
  const count = 2 ** ELEVATION_ZOOM;
  const latitude = (lat * Math.PI) / 180;
  const pixelX = ((lon + 180) / 360) * count * TILE_SIZE;
  const pixelY =
    ((1 - Math.asinh(Math.tan(latitude)) / Math.PI) / 2) * count * TILE_SIZE;
  const x0 = Math.floor(pixelX);
  const y0 = Math.floor(pixelY);
  const xFraction = pixelX - x0;
  const yFraction = pixelY - y0;
  const topLeft = elevationPixel(tiles, x0, y0);
  const topRight = elevationPixel(tiles, x0 + 1, y0);
  const bottomLeft = elevationPixel(tiles, x0, y0 + 1);
  const bottomRight = elevationPixel(tiles, x0 + 1, y0 + 1);
  if (![topLeft, topRight, bottomLeft, bottomRight].every(Number.isFinite)) {
    return null;
  }
  const top = topLeft + (topRight - topLeft) * xFraction;
  const bottom = bottomLeft + (bottomRight - bottomLeft) * xFraction;
  return top + (bottom - top) * yFraction;
}

function elevationPixel(tiles, pixelX, pixelY) {
  const tileX = Math.floor(pixelX / TILE_SIZE);
  const tileY = Math.floor(pixelY / TILE_SIZE);
  const tileKey = `${tileX}/${tileY}`;
  const pixels = tiles.get(tileKey);
  if (!pixels) return null;
  const x = pixelX - tileX * TILE_SIZE;
  const y = pixelY - tileY * TILE_SIZE;
  const index = (y * TILE_SIZE + x) * 4;
  return decodeTerrariumRgb(
    pixels[index],
    pixels[index + 1],
    pixels[index + 2],
  );
}

async function fetchTile(url, signal) {
  let cache = null;
  let cached = null;
  if ("caches" in globalThis) {
    try {
      cache = await globalThis.caches.open(CACHE_NAME);
      cached = await cache.match(url);
    } catch (error) {
      console.warn("Could not access the geodata tile cache:", error);
    }
  }
  const cachedAt = Number(cached?.headers.get("x-velocity-cached-at"));
  if (cached && Date.now() - cachedAt < CACHE_MAX_AGE) {
    return cachedTile(cached);
  }

  let response;
  try {
    response = await globalThis.fetch(url, { mode: "cors", signal });
  } catch (error) {
    if (signal?.aborted) throw error;
    if (cached) return cachedTile(cached);
    throw error;
  }
  if (!response.ok) {
    if (cached && (response.status === 429 || response.status >= 500)) {
      return cachedTile(cached);
    }
    throw new Error(`Map tile request failed (${response.status}): ${url}`);
  }
  const blob = await response.blob();
  if (cache) {
    try {
      const headers = new globalThis.Headers(response.headers);
      headers.set("x-velocity-cached-at", String(Date.now()));
      await cache.put(
        url,
        new globalThis.Response(blob, { status: response.status, headers }),
      );
      await trimCache(cache);
    } catch (error) {
      console.warn("Could not cache a geodata tile for offline use:", error);
    }
  }
  return { blob, headers: response.headers, fromCache: false };
}

async function cachedTile(response) {
  return {
    blob: await response.blob(),
    headers: response.headers,
    fromCache: true,
  };
}

async function trimCache(cache) {
  const keys = await cache.keys();
  const excess = keys.length - CACHE_LIMIT;
  if (excess > 0) {
    await Promise.all(keys.slice(0, excess).map((key) => cache.delete(key)));
  }
}

async function mapConcurrent(items, limit, worker) {
  const results = new Array(items.length);
  let nextIndex = 0;
  const workers = await Promise.allSettled(
    Array.from({ length: Math.min(limit, items.length) }, async () => {
      while (nextIndex < items.length) {
        const index = nextIndex;
        nextIndex += 1;
        results[index] = await worker(items[index]);
      }
    }),
  );
  const failure = workers.find((result) => result.status === "rejected");
  if (failure) {
    results
      .filter((tile) => tile?.bitmap)
      .forEach((tile) => tile.bitmap.close());
    throw failure.reason;
  }
  return results;
}
