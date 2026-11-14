const SWISSTOPO_TILE_URL =
  "https://wmts.geo.admin.ch/1.0.0/ch.swisstopo.swissimage/default/current/21781";
const TERRARIUM_TILE_URL =
  "https://s3.amazonaws.com/elevation-tiles-prod/terrarium";
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
]);
const TILE_CONCURRENCY = 4;
const IMAGE_TILE_RADIUS = 2;
const IMAGE_TILE_MATRIX = 22;
const DETAIL_TILE_RADIUS = 1;
const DETAIL_TILE_MATRIX = 25;
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
  const elevationCenter = terrariumTileAddress(
    center.lat,
    center.lon,
    ELEVATION_ZOOM,
  );
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
  const elevationTiles = tilesAround(elevationCenter.x, elevationCenter.y);
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
    ...elevationTiles.map((tile) => ({ kind: "elevation", tile })),
  ];
  let completed = 0;
  let detailError = null;
  const loaded = await mapConcurrent(jobs, TILE_CONCURRENCY, async (job) => {
    try {
      if (job.kind === "image" || job.kind === "detail") {
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
      if (job.kind !== "detail" || signal?.aborted) throw error;
      detailError ??= error;
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
    if (
      !detailError &&
      loadedDetailTiles.length === (DETAIL_TILE_RADIUS * 2 + 1) ** 2
    ) {
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
  } catch (error) {
    closeImageTiles(loadedTiles);
    throw error;
  }
  if (detailError) {
    console.warn(
      "High-resolution SWISSIMAGE tiles are unavailable; using the regional base imagery.",
      detailError,
    );
  }

  const elevationData = new Map(
    loadedTiles
      .filter((tile) => tile.kind === "elevation")
      .map((tile) => [`${tile.tile.x}/${tile.tile.y}`, tile.pixels]),
  );
  const bounds = imageLayers[0].bounds;
  const detailLayer = imageLayers[1] ?? null;
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
    detail: matrix === DETAIL_TILE_MATRIX,
    bounds: {
      minX: minEasting - origin.easting,
      maxX: maxEasting - origin.easting,
      minZ: origin.northing - maxNorthing,
      maxZ: origin.northing - minNorthing,
    },
    textureCanvas: canvas,
  };
}

function closeImageTiles(tiles) {
  tiles.filter((tile) => tile.bitmap).forEach((tile) => tile.bitmap.close());
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
      .filter((tile) => tile?.kind === "image" || tile?.kind === "detail")
      .forEach((tile) => tile.bitmap.close());
    throw failure.reason;
  }
  return results;
}
