import * as THREE from "three";
import { Sky } from "three/addons/objects/Sky.js";
import { routeLength } from "./model.js";

const PRESETS = {
  "Ultra Low": { pixelRatio: 0.65, trees: 140, shadows: false },
  Low: { pixelRatio: 0.8, trees: 260, shadows: false },
  Auto: { pixelRatio: 0.9, trees: 360, shadows: false },
  Medium: { pixelRatio: 1, trees: 520, shadows: true },
  High: { pixelRatio: 1.25, trees: 760, shadows: true },
  Ultra: { pixelRatio: 1.5, trees: 1000, shadows: true },
};

export class WorldRenderer {
  constructor(canvas, onStatus) {
    this.canvas = canvas;
    this.onStatus = onStatus;
    this.scene = new THREE.Scene();
    this.scene.background = new THREE.Color("#b5d4e4");
    this.scene.fog = new THREE.Fog("#b5d4e4", 900, 5200);
    this.camera = new THREE.PerspectiveCamera(60, 1, 0.1, 9000);
    this.renderer = new THREE.WebGLRenderer({
      canvas,
      antialias: true,
      powerPreference: "high-performance",
      alpha: false,
    });
    this.renderer.outputColorSpace = THREE.SRGBColorSpace;
    this.renderer.toneMapping = THREE.ACESFilmicToneMapping;
    this.renderer.toneMappingExposure = 1.08;
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 1.25));
    this.renderer.shadowMap.enabled = true;
    this.renderer.shadowMap.type = THREE.PCFSoftShadowMap;
    this.clock = new THREE.Clock();
    this.world = new THREE.Group();
    this.scene.add(this.world);
    this.sky = new Sky();
    this.sky.scale.setScalar(8500);
    this.sky.material.uniforms.turbidity.value = 7.5;
    this.sky.material.uniforms.rayleigh.value = 2.4;
    this.sky.material.uniforms.mieCoefficient.value = 0.004;
    this.sky.material.uniforms.mieDirectionalG.value = 0.82;
    this.scene.add(this.sky);
    this.gradientSky = createGradientSky();
    this.scene.add(this.gradientSky);
    this.cloudLayer = new THREE.Group();
    this.scene.add(this.cloudLayer);
    this.clouds = createClouds();
    this.cloudLayer.add(this.clouds);
    this.hemiLight = new THREE.HemisphereLight("#e8f4ff", "#4b5c42", 1.8);
    this.sun = new THREE.DirectionalLight("#fff0d3", 3.4);
    this.sun.position.set(-450, 850, 300);
    this.scene.add(this.hemiLight, this.sun);
    this.sunDisc = new THREE.Mesh(
      new THREE.SphereGeometry(34, 20, 16),
      new THREE.MeshBasicMaterial({ color: "#fff0c8", toneMapped: false }),
    );
    this.scene.add(this.sunDisc);
    this.sun.castShadow = true;
    this.sun.shadow.mapSize.set(1024, 1024);
    this.sun.shadow.camera.left = -1800;
    this.sun.shadow.camera.right = 1800;
    this.sun.shadow.camera.top = 1800;
    this.sun.shadow.camera.bottom = -1800;
    this.sun.shadow.camera.near = 10;
    this.sun.shadow.camera.far = 4500;
    this.sun.shadow.camera.updateProjectionMatrix();
    this.sun.shadow.bias = -0.00015;
    this.sun.shadow.normalBias = 0.035;
    this.sun.shadow.radius = 3;
    this.terrainTexture = createTerrainTexture();
    this.roadTexture = createRoadTexture();
    this.routeCurve = null;
    this.routeSamples = [];
    this.route = null;
    this.rider = createRider();
    this.scene.add(this.rider);
    this.preset = "Auto";
    this.elapsed = 0;
    this.frameCount = 0;
    this.fpsTime = 0;
    this.resizeObserver = new ResizeObserver(() => this.resize());
    this.resizeObserver.observe(canvas);
    this.resize();
  }

  setRoute(route) {
    this.route = route;
    this.clearWorld();
    const projected = projectRoute(route.points);
    const curvePoints = projected.map(
      (point) => new THREE.Vector3(point.x, 0, point.z),
    );
    const closed =
      Math.abs(route.points[0].lat - route.points.at(-1).lat) < 1e-8 &&
      Math.abs(route.points[0].lon - route.points.at(-1).lon) < 1e-8;
    this.routeCurve = new THREE.CatmullRomCurve3(
      curvePoints,
      closed,
      "centripetal",
      0.35,
    );
    this.routeSamples = buildRouteSamples(this.routeCurve, route, closed);
    this.buildTerrain(this.routeSamples, closed);
    this.buildMountains(this.routeSamples);
    this.buildRoad(this.routeSamples, closed);
    this.buildVegetation();
  }

  setQuality(preset) {
    const previousPreset = this.preset;
    const context = this.renderer.getContext();
    const debugInfo = context.getExtension("WEBGL_debug_renderer_info");
    const rendererName = debugInfo
      ? context.getParameter(debugInfo.UNMASKED_RENDERER_WEBGL)
      : "";
    const softwareRenderer =
      /swiftshader|llvmpipe|softpipe|software rasterizer/i.test(rendererName);
    const selected =
      preset === "Auto"
        ? softwareRenderer
          ? "Ultra Low"
          : navigator.hardwareConcurrency <= 4
            ? "Low"
            : "Medium"
        : preset;
    this.preset = PRESETS[selected] ? selected : "Auto";
    const quality = PRESETS[this.preset];
    this.renderer.setPixelRatio(
      Math.min(window.devicePixelRatio || 1, quality.pixelRatio),
    );
    this.sun.castShadow = quality.shadows;
    const shadowSize =
      selected === "Ultra" ? 2048 : selected === "High" ? 1536 : 1024;
    this.sun.shadow.mapSize.set(shadowSize, shadowSize);
    this.renderer.shadowMap.enabled = quality.shadows;
    this.renderer.shadowMap.type = THREE.PCFSoftShadowMap;
    this.sky.visible = !["Low", "Ultra Low"].includes(this.preset);
    this.gradientSky.visible = !this.sky.visible;
    this.requestedQuality = preset;
    if (previousPreset !== this.preset && this.route) this.setRoute(this.route);
    this.onStatus?.(`WebGL 2 · Three.js · ${this.preset}`);
    this.resize();
  }

  setWeather(hour, cloud, rain, fog) {
    const daylight = Math.sin(((hour - 6) / 12) * Math.PI);
    const storm = rain / 100;
    const cloudMix = cloud / 100;
    const sunDirection = new THREE.Vector3().setFromSphericalCoords(
      1,
      Math.acos(THREE.MathUtils.clamp(daylight, -0.2, 1)),
      (hour / 24) * Math.PI * 2 - Math.PI / 2,
    );
    const sky = new THREE.Color().setRGB(
      0.22 + Math.max(0, daylight) * 0.45 - storm * 0.14 - cloudMix * 0.1,
      0.3 + Math.max(0, daylight) * 0.48 - storm * 0.16 - cloudMix * 0.08,
      0.42 + Math.max(0, daylight) * 0.48 - storm * 0.18 - cloudMix * 0.06,
    );
    this.scene.background.copy(sky);
    this.scene.fog.color.copy(sky);
    this.gradientSky.material.uniforms.horizon.value.copy(sky);
    this.gradientSky.material.uniforms.zenith.value.setRGB(
      sky.r * 0.52,
      sky.g * 0.72,
      sky.b * 0.96,
    );
    this.scene.fog.near = 450 + (100 - fog) * 9;
    this.scene.fog.far = Math.max(this.scene.fog.near + 250, 5700 - fog * 33);
    this.sky.material.uniforms.sunPosition.value.copy(sunDirection);
    this.sun.position.copy(sunDirection).multiplyScalar(900);
    this.sunDisc.position.copy(sunDirection).multiplyScalar(7600);
    this.sunDisc.visible = daylight > 0.04;
    this.hemiLight.intensity =
      0.65 + Math.max(0, daylight) * 1.4 - storm * 0.35;
    this.sun.intensity =
      0.2 + Math.max(0, daylight) * 3 - storm * 1.5 - cloudMix * 0.65;
    this.sun.color.set(storm > 0.3 ? "#c4d2e0" : "#fff0d3");
    this.clouds.material.opacity = 0.035 + cloudMix * 0.38;
    this.cloudLayer.position.x = Math.sin(this.elapsed * 0.008) * 120;
    this.cloudLayer.position.z = Math.cos(this.elapsed * 0.006) * 90;
    if (this.roadMaterial) {
      this.roadMaterial.roughness = 0.9 - storm * 0.58;
      this.roadMaterial.metalness = storm * 0.12;
    }
  }

  render({ distanceM, lengthM, cameraMode, hour, cloud, rain, fog, quality }) {
    const delta = Math.min(this.clock.getDelta(), 0.08);
    this.elapsed += delta;
    this.frameCount += 1;
    this.fpsTime += delta;
    this.setWeather(hour, cloud, rain, fog);
    if (quality !== this.requestedQuality) {
      this.requestedQuality = quality;
      this.setQuality(quality);
    }

    if (this.routeSamples.length) {
      const progress = lengthM > 0 ? (distanceM % lengthM) / lengthM : 0;
      const riderState = pointAtProgress(this.routeSamples, progress);
      this.rider.position.copy(riderState.position);
      this.rider.rotation.y = Math.atan2(
        -riderState.tangent.z,
        riderState.tangent.x,
      );
      const cameraConfig = {
        chase: [12, 6, 6, 2.4],
        close: [6, 3.6, 3, 1.6],
        hood: [1, 1.4, 3, 0],
        drone: [150, 125, 105, 0],
      }[cameraMode] ?? [8, 4.2, 3.5, 2.6];
      const direction = riderState.tangent;
      const side = new THREE.Vector3(direction.z, 0, -direction.x);
      const cameraPosition = riderState.position
        .clone()
        .addScaledVector(direction, -cameraConfig[0])
        .addScaledVector(side, cameraConfig[3])
        .add(new THREE.Vector3(0, cameraConfig[1], 0));
      const target = riderState.position
        .clone()
        .addScaledVector(direction, cameraConfig[2])
        .add(new THREE.Vector3(0, 1.2, 0));
      this.camera.position.lerp(cameraPosition, 1 - Math.exp(-delta * 5));
      this.camera.lookAt(target);
    }

    this.renderer.render(this.scene, this.camera);
    if (this.fpsTime >= 0.75) {
      this.onFrame?.(Math.round(this.frameCount / this.fpsTime));
      this.frameCount = 0;
      this.fpsTime = 0;
    }
  }

  onFps(callback) {
    this.onFrame = callback;
  }

  resize() {
    const { clientWidth, clientHeight } = this.canvas;
    if (clientWidth < 1 || clientHeight < 1) return;
    this.renderer.setSize(clientWidth, clientHeight, false);
    this.camera.aspect = clientWidth / clientHeight;
    this.camera.updateProjectionMatrix();
  }

  dispose() {
    this.resizeObserver.disconnect();
    this.renderer.dispose();
    this.clearWorld();
    this.terrainTexture.dispose();
    this.roadTexture.dispose();
    this.sky.material.dispose();
    this.gradientSky.geometry.dispose();
    this.gradientSky.material.dispose();
    this.clouds.geometry.dispose();
    this.clouds.material.dispose();
    this.sunDisc.geometry.dispose();
    this.sunDisc.material.dispose();
  }

  clearWorld() {
    for (const child of [...this.world.children]) {
      this.world.remove(child);
      child.traverse((object) => {
        object.geometry?.dispose();
        if (Array.isArray(object.material))
          object.material.forEach((material) => material.dispose());
        else object.material?.dispose();
      });
    }
  }

  buildTerrain(samples, closed) {
    const bounds = routeBounds(samples);
    const spanX = Math.max(600, bounds.maxX - bounds.minX);
    const spanZ = Math.max(600, bounds.maxZ - bounds.minZ);
    const margin = Math.max(spanX, spanZ) * 0.8 + 240;
    const minX = bounds.minX - margin;
    const maxX = bounds.maxX + margin;
    const minZ = bounds.minZ - margin;
    const maxZ = bounds.maxZ + margin;
    const divisions =
      {
        "Ultra Low": 80,
        Low: 96,
        Auto: 112,
        Medium: 128,
        High: 152,
        Ultra: 184,
      }[this.preset] ?? 112;
    const vertices = [];
    const colors = [];
    const uvs = [];
    const indices = [];
    for (let z = 0; z <= divisions; z += 1) {
      for (let x = 0; x <= divisions; x += 1) {
        const worldX = minX + ((maxX - minX) * x) / divisions;
        const worldZ = minZ + ((maxZ - minZ) * z) / divisions;
        const height = terrainHeight(worldX, worldZ);
        vertices.push(worldX, height, worldZ);
        const color = terrainColor(height, worldX, worldZ);
        colors.push(color.r, color.g, color.b);
        uvs.push((x / divisions) * 28, (z / divisions) * 28);
        if (x < divisions && z < divisions) {
          const a = z * (divisions + 1) + x;
          const b = a + divisions + 1;
          indices.push(a, b, a + 1, b, b + 1, a + 1);
        }
      }
    }
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute(
      "position",
      new THREE.Float32BufferAttribute(vertices, 3),
    );
    geometry.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
    geometry.setAttribute("uv", new THREE.Float32BufferAttribute(uvs, 2));
    geometry.setIndex(indices);
    geometry.computeVertexNormals();
    const terrain = new THREE.Mesh(
      geometry,
      new THREE.MeshStandardMaterial({
        map: this.terrainTexture,
        vertexColors: true,
        roughness: 0.96,
      }),
    );
    terrain.receiveShadow = true;
    terrain.castShadow = false;
    this.world.add(terrain);

    if (closed && samples.length > 10) this.buildBuildings(samples, bounds);
  }

  buildRoad(samples, closed) {
    const vertices = [];
    const uvs = [];
    const indices = [];
    const count = samples.length;
    const segmentCount = closed ? count : count - 1;
    const roadWidth = 2.2;
    let distance = 0;
    for (let index = 0; index < count; index += 1) {
      const current = samples[index].position;
      const previous =
        samples[closed ? (index - 1 + count) % count : Math.max(0, index - 1)]
          .position;
      const following =
        samples[closed ? (index + 1) % count : Math.min(count - 1, index + 1)]
          .position;
      if (index > 0) distance += current.distanceTo(previous);
      const tangent = following.clone().sub(previous).normalize();
      const side = new THREE.Vector3(tangent.z, 0, -tangent.x).normalize();
      const lift = new THREE.Vector3(0, 0.12, 0);
      vertices.push(
        ...current
          .clone()
          .addScaledVector(side, -roadWidth)
          .add(lift)
          .toArray(),
        ...current.clone().addScaledVector(side, roadWidth).add(lift).toArray(),
      );
      const textureDistance = distance * 0.08;
      uvs.push(0, textureDistance, 1, textureDistance);
    }
    for (let index = 0; index < segmentCount; index += 1) {
      const next = (index + 1) % count;
      const start = index * 2;
      const end = next * 2;
      indices.push(start, end, start + 1, start + 1, end, end + 1);
    }
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute(
      "position",
      new THREE.Float32BufferAttribute(vertices, 3),
    );
    geometry.setAttribute("uv", new THREE.Float32BufferAttribute(uvs, 2));
    geometry.setIndex(indices);
    geometry.computeVertexNormals();
    this.roadMaterial = new THREE.MeshStandardMaterial({
      map: this.roadTexture,
      color: "#deddd2",
      roughness: 0.9,
      side: THREE.DoubleSide,
    });
    const road = new THREE.Mesh(geometry, this.roadMaterial);
    road.receiveShadow = true;
    this.world.add(road);

    const center = [];
    const edges = [];
    for (let index = 0; index < segmentCount; index += 1) {
      const next = samples[(index + 1) % count].position;
      const current = samples[index].position;
      const previous =
        samples[closed ? (index - 1 + count) % count : Math.max(0, index - 1)]
          .position;
      const tangent = next.clone().sub(previous).normalize();
      const side = new THREE.Vector3(tangent.z, 0, -tangent.x).normalize();
      const lineLift = new THREE.Vector3(0, 0.18, 0);
      edges.push(
        current
          .clone()
          .addScaledVector(side, roadWidth - 0.18)
          .add(lineLift),
        next
          .clone()
          .addScaledVector(side, roadWidth - 0.18)
          .add(lineLift),
        current
          .clone()
          .addScaledVector(side, -roadWidth + 0.18)
          .add(lineLift),
        next
          .clone()
          .addScaledVector(side, -roadWidth + 0.18)
          .add(lineLift),
      );
      if (index % 16 < 5) {
        center.push(current.clone().add(lineLift), next.clone().add(lineLift));
      }
    }
    const centerLine = new THREE.LineSegments(
      new THREE.BufferGeometry().setFromPoints(center),
      new THREE.LineBasicMaterial({
        color: "#e9e0be",
        transparent: true,
        opacity: 0.85,
      }),
    );
    const edgeLines = new THREE.LineSegments(
      new THREE.BufferGeometry().setFromPoints(edges),
      new THREE.LineBasicMaterial({
        color: "#e1e2d9",
        transparent: true,
        opacity: 0.78,
      }),
    );
    this.world.add(centerLine, edgeLines);
  }

  buildMountains(samples) {
    const bounds = routeBounds(samples);
    const centerX = (bounds.minX + bounds.maxX) / 2;
    const centerZ = (bounds.minZ + bounds.maxZ) / 2;
    const peaks = 10;
    const sides = 18;
    const levels = [
      [0, 1],
      [0.12, 0.91],
      [0.28, 0.76],
      [0.46, 0.59],
      [0.64, 0.42],
      [0.79, 0.28],
      [0.91, 0.15],
      [0.98, 0.06],
      [1, 0.012],
    ];
    const lowerRock = new THREE.Color("#455663");
    const upperRock = new THREE.Color("#8a9ba2");
    const snow = new THREE.Color("#e5e9e7");

    for (let peak = 0; peak < peaks; peak += 1) {
      const direction = (peak / peaks) * Math.PI * 2 + 0.12;
      const distance = 3200 + pseudoRandom(peak * 19 + 3) * 950;
      const width = 620 + pseudoRandom(peak * 29 + 7) * 520;
      const height = 620 + pseudoRandom(peak * 41 + 11) * 520;
      const center = new THREE.Vector3(
        centerX + Math.cos(direction) * distance,
        0,
        centerZ + Math.sin(direction) * distance,
      );
      const base = terrainHeight(center.x, center.z);
      const vertices = [];
      const colors = [];
      const indices = [];

      for (const [elevation, radius] of levels) {
        for (let side = 0; side < sides; side += 1) {
          const angle = (side / sides) * Math.PI * 2;
          const ridge = 0.84 + pseudoRandom(peak * 97 + side * 13 + 5) * 0.34;
          const radial = width * radius * ridge;
          const y = base + height * elevation + Math.sin(angle * 3 + peak) * 16;
          vertices.push(
            center.x + Math.cos(angle) * radial,
            y,
            center.z + Math.sin(angle) * radial,
          );
          const color =
            elevation > 0.79
              ? snow
                  .clone()
                  .lerp(upperRock, Math.max(0, (0.94 - elevation) * 2))
              : lowerRock.clone().lerp(upperRock, elevation * 0.86);
          colors.push(color.r, color.g, color.b);
        }
      }

      for (let level = 0; level < levels.length - 1; level += 1) {
        for (let side = 0; side < sides; side += 1) {
          const a = level * sides + side;
          const b = level * sides + ((side + 1) % sides);
          const c = (level + 1) * sides + side;
          const d = (level + 1) * sides + ((side + 1) % sides);
          indices.push(a, c, b, b, c, d);
        }
      }

      const geometry = new THREE.BufferGeometry();
      geometry.setAttribute(
        "position",
        new THREE.Float32BufferAttribute(vertices, 3),
      );
      geometry.setAttribute(
        "color",
        new THREE.Float32BufferAttribute(colors, 3),
      );
      geometry.setIndex(indices);
      geometry.computeVertexNormals();
      const mountain = new THREE.Mesh(
        geometry,
        new THREE.MeshStandardMaterial({
          vertexColors: true,
          flatShading: true,
          roughness: 1,
        }),
      );
      mountain.frustumCulled = true;
      this.world.add(mountain);
    }
  }

  buildVegetation() {
    const count = PRESETS[this.preset]?.trees ?? PRESETS.Auto.trees;
    const treeGeometry = new THREE.ConeGeometry(3.2, 12, 8);
    const canopyMeshes = ["#315b43", "#3a704f", "#4b805b"].map((color) => {
      const mesh = new THREE.InstancedMesh(
        treeGeometry,
        new THREE.MeshStandardMaterial({ color, roughness: 0.95 }),
        count,
      );
      mesh.userData.vegetation = true;
      mesh.castShadow = true;
      mesh.receiveShadow = true;
      return mesh;
    });
    const trunkGeometry = new THREE.CylinderGeometry(0.35, 0.55, 4.5, 5);
    const trunkMaterial = new THREE.MeshStandardMaterial({
      color: "#75533a",
      roughness: 1,
    });
    const trunks = new THREE.InstancedMesh(trunkGeometry, trunkMaterial, count);
    trunks.userData.vegetation = true;
    trunks.castShadow = true;
    const dummy = new THREE.Object3D();
    const samples = this.routeSamples;
    let visible = 0;
    for (let index = 0; index < count; index += 1) {
      const sampleIndex = Math.floor((index / count) * samples.length);
      const sample = samples[sampleIndex].position;
      const previous =
        samples[(sampleIndex - 1 + samples.length) % samples.length].position;
      const next = samples[(sampleIndex + 1) % samples.length].position;
      const tangent = next.clone().sub(previous).normalize();
      const side = new THREE.Vector3(tangent.z, 0, -tangent.x);
      const distance = 240 + pseudoRandom(index * 29 + 7) * 210;
      const sign = pseudoRandom(index * 23 + 13) > 0.5 ? 1 : -1;
      const x = sample.x + side.x * distance * sign;
      const z = sample.z + side.z * distance * sign;
      const height = terrainHeight(x, z);
      const size = 0.6 + pseudoRandom(index * 47 + 11) * 0.8;
      const canopies = [
        { scale: 1, height: 5.2 },
        { scale: 0.78, height: 8.6 },
        { scale: 0.56, height: 11.6 },
      ];
      for (const [layer, mesh] of canopyMeshes.entries()) {
        const canopy = canopies[layer];
        dummy.position.set(x, height + canopy.height * size, z);
        dummy.scale.set(size * canopy.scale, size * 0.46, size * canopy.scale);
        dummy.rotation.y = pseudoRandom(index * 3) * Math.PI;
        dummy.updateMatrix();
        mesh.setMatrixAt(visible, dummy.matrix);
      }
      dummy.position.y = height + 2.25 * size;
      dummy.scale.set(size, size, size);
      dummy.updateMatrix();
      trunks.setMatrixAt(visible, dummy.matrix);
      visible += 1;
    }
    canopyMeshes.forEach((mesh) => {
      mesh.count = visible;
    });
    trunks.count = visible;
    for (const mesh of [...canopyMeshes, trunks]) {
      mesh.instanceMatrix.needsUpdate = true;
      mesh.computeBoundingSphere();
      mesh.castShadow = this.preset !== "Low" && this.preset !== "Ultra Low";
    }
    this.world.add(...canopyMeshes, trunks);
  }

  buildBuildings(samples, bounds) {
    const geometry = new THREE.BoxGeometry(15, 9, 12);
    const material = new THREE.MeshStandardMaterial({
      color: "#ded4b8",
      roughness: 0.9,
    });
    const houses = new THREE.InstancedMesh(geometry, material, 18);
    const dummy = new THREE.Object3D();
    for (let index = 0; index < houses.count; index += 1) {
      const x =
        bounds.minX +
        pseudoRandom(index * 13 + 19) * (bounds.maxX - bounds.minX);
      const z =
        bounds.minZ +
        pseudoRandom(index * 23 + 31) * (bounds.maxZ - bounds.minZ);
      if (nearestRouteDistance(x, z, samples) < 38) {
        dummy.position.set(x + 40, terrainHeight(x + 40, z) + 4.5, z);
      } else {
        dummy.position.set(x, terrainHeight(x, z) + 4.5, z);
      }
      dummy.rotation.y = pseudoRandom(index * 37) * Math.PI * 2;
      dummy.updateMatrix();
      houses.setMatrixAt(index, dummy.matrix);
    }
    const roofGeometry = new THREE.ConeGeometry(10.5, 6.5, 4);
    roofGeometry.rotateY(Math.PI / 4);
    const roofs = new THREE.InstancedMesh(
      roofGeometry,
      new THREE.MeshStandardMaterial({ color: "#795848", roughness: 0.85 }),
      houses.count,
    );
    for (let index = 0; index < houses.count; index += 1) {
      houses.getMatrixAt(index, dummy.matrix);
      dummy.position.setFromMatrixPosition(dummy.matrix);
      dummy.quaternion.setFromRotationMatrix(dummy.matrix);
      dummy.scale.set(1, 1, 1);
      dummy.position.y += 7.5;
      dummy.updateMatrix();
      roofs.setMatrixAt(index, dummy.matrix);
    }
    houses.castShadow = true;
    houses.receiveShadow = true;
    roofs.castShadow = true;
    this.world.add(houses, roofs);
  }
}

function projectRoute(points) {
  const centerLat =
    points.reduce((sum, point) => sum + point.lat, 0) / points.length;
  const centerLon =
    points.reduce((sum, point) => sum + point.lon, 0) / points.length;
  const cosLat = Math.cos((centerLat * Math.PI) / 180);
  const projected = points.map((point) => ({
    x: (point.lon - centerLon) * 111_320 * cosLat,
    z: -(point.lat - centerLat) * 110_540,
    ele: point.ele,
  }));
  const maxRange = Math.max(
    ...projected.map((point) => Math.hypot(point.x, point.z)),
    1,
  );
  const scale = Math.min(1, 1250 / maxRange);
  return projected.map((point) => ({
    ...point,
    x: point.x * scale,
    z: point.z * scale,
  }));
}

function buildRouteSamples(curve, route, closed) {
  const count = Math.min(480, Math.max(120, route.points.length * 3));
  const positions = curve.getPoints(count);
  const referenceLength = routeLength(route.points) || route.lengthM || 1;
  return positions
    .slice(0, closed ? count : count + 1)
    .map((position, index) => {
      const t = index / count;
      const terrain = terrainHeight(position.x, position.z);
      return {
        position: new THREE.Vector3(position.x, terrain, position.z),
        distanceM: t * referenceLength,
      };
    });
}

function pointAtProgress(samples, progress) {
  const scaled = progress * samples.length;
  const index = Math.floor(scaled) % samples.length;
  const next = samples[(index + 1) % samples.length];
  const current = samples[index];
  const position = current.position
    .clone()
    .lerp(next.position, scaled - Math.floor(scaled));
  const tangent = next.position.clone().sub(current.position).normalize();
  return { position, tangent };
}

function terrainHeight(x, z) {
  const broad =
    Math.sin(x * 0.00055 + Math.sin(z * 0.0008)) * 78 +
    Math.cos(z * 0.00072 + Math.sin(x * 0.0006)) * 84;
  const ridges =
    Math.sin(x * 0.0015 + Math.sin(z * 0.001) * 1.2) * 48 +
    Math.sin(z * 0.0017 + 1.3) * 39 +
    Math.cos(x * 0.0024 + z * 0.0017) * 25;
  const peakA =
    Math.exp(-(((x - 1900) / 850) ** 2 + ((z + 2200) / 1100) ** 2)) * 510;
  const peakB =
    Math.exp(-(((x + 2400) / 950) ** 2 + ((z - 1700) / 800) ** 2)) * 390;
  const peakC =
    Math.exp(-(((x - 300) / 1300) ** 2 + ((z - 2800) / 1000) ** 2)) * 270;
  return Math.max(4, 75 + broad + ridges + peakA + peakB + peakC);
}

function terrainColor(height, x, z) {
  const tint = Math.sin(x * 0.005 + z * 0.003) * 0.025;
  if (height > 420) return new THREE.Color("#e0e0d9").offsetHSL(0, 0, tint);
  if (height > 300) return new THREE.Color("#8a8b7d").offsetHSL(0, 0, tint);
  if (height > 190) return new THREE.Color("#73816a").offsetHSL(0, 0, tint);
  if (height < 48) return new THREE.Color("#758466").offsetHSL(0, 0, tint);
  return new THREE.Color("#64815b").offsetHSL(0, 0, tint);
}

function createTerrainTexture() {
  const size = 256;
  const data = new Uint8Array(size * size * 4);
  for (let y = 0; y < size; y += 1) {
    for (let x = 0; x < size; x += 1) {
      const index = (y * size + x) * 4;
      const grain = (pseudoRandom(x * 71 + y * 137) - 0.5) * 30;
      const fleck = Math.sin(x * 0.31 + Math.cos(y * 0.27)) * 4;
      const value = 218 + grain + fleck;
      data[index] = value;
      data[index + 1] = value + 1;
      data[index + 2] = value - 5;
      data[index + 3] = 255;
    }
  }
  const texture = new THREE.DataTexture(data, size, size, THREE.RGBAFormat);
  texture.wrapS = THREE.RepeatWrapping;
  texture.wrapT = THREE.RepeatWrapping;
  texture.magFilter = THREE.LinearFilter;
  texture.minFilter = THREE.LinearMipmapLinearFilter;
  texture.generateMipmaps = true;
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.anisotropy = 8;
  texture.needsUpdate = true;
  return texture;
}

function createRoadTexture() {
  const size = 128;
  const data = new Uint8Array(size * size * 4);
  for (let y = 0; y < size; y += 1) {
    for (let x = 0; x < size; x += 1) {
      const index = (y * size + x) * 4;
      const grain = (pseudoRandom(x * 31 + y * 97) - 0.5) * 28;
      const value = 130 + grain + Math.sin(x * 0.18 + y * 0.07) * 3;
      data[index] = value;
      data[index + 1] = value + 2;
      data[index + 2] = value + 4;
      data[index + 3] = 255;
    }
  }
  const texture = new THREE.DataTexture(data, size, size, THREE.RGBAFormat);
  texture.wrapS = THREE.RepeatWrapping;
  texture.wrapT = THREE.RepeatWrapping;
  texture.magFilter = THREE.LinearFilter;
  texture.minFilter = THREE.LinearMipmapLinearFilter;
  texture.generateMipmaps = true;
  texture.colorSpace = THREE.SRGBColorSpace;
  texture.needsUpdate = true;
  return texture;
}

function createGradientSky() {
  const geometry = new THREE.SphereGeometry(8500, 32, 18);
  const material = new THREE.ShaderMaterial({
    uniforms: {
      horizon: { value: new THREE.Color("#c7e0e9") },
      zenith: { value: new THREE.Color("#4a83b1") },
    },
    vertexShader: `
      varying float vHeight;
      void main() {
        vHeight = normalize(position).y;
        gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
      }
    `,
    fragmentShader: `
      uniform vec3 horizon;
      uniform vec3 zenith;
      varying float vHeight;
      void main() {
        float gradient = smoothstep(-0.12, 0.78, vHeight);
        vec3 color = mix(horizon, zenith, gradient);
        gl_FragColor = vec4(color, 1.0);
      }
    `,
    side: THREE.BackSide,
    depthWrite: false,
    depthTest: false,
    toneMapped: false,
  });
  const sky = new THREE.Mesh(geometry, material);
  sky.renderOrder = -1;
  return sky;
}

function createClouds() {
  const count = 44;
  const clouds = new THREE.InstancedMesh(
    new THREE.SphereGeometry(1, 8, 6),
    new THREE.MeshBasicMaterial({
      color: "#fffefa",
      transparent: true,
      opacity: 0.12,
      depthWrite: false,
    }),
    count,
  );
  const dummy = new THREE.Object3D();
  for (let index = 0; index < count; index += 1) {
    const x = pseudoRandom(index * 19 + 3) * 7200 - 3600;
    const z = pseudoRandom(index * 31 + 7) * 7200 - 3600;
    dummy.position.set(x, 330 + pseudoRandom(index * 43 + 11) * 260, z);
    dummy.scale.set(
      80 + pseudoRandom(index * 17 + 29) * 140,
      15 + pseudoRandom(index * 23 + 5) * 22,
      45 + pseudoRandom(index * 41 + 13) * 90,
    );
    dummy.updateMatrix();
    clouds.setMatrixAt(index, dummy.matrix);
  }
  return clouds;
}

function routeBounds(samples) {
  return samples.reduce(
    (bounds, sample) => ({
      minX: Math.min(bounds.minX, sample.position.x),
      maxX: Math.max(bounds.maxX, sample.position.x),
      minZ: Math.min(bounds.minZ, sample.position.z),
      maxZ: Math.max(bounds.maxZ, sample.position.z),
    }),
    { minX: Infinity, maxX: -Infinity, minZ: Infinity, maxZ: -Infinity },
  );
}

function nearestRouteDistance(x, z, samples) {
  let minimum = Infinity;
  for (let index = 0; index < samples.length; index += 5) {
    const point = samples[index].position;
    minimum = Math.min(minimum, Math.hypot(point.x - x, point.z - z));
  }
  return minimum;
}

function pseudoRandom(seed) {
  const value = Math.sin(seed * 78.233) * 43_758.5453;
  return value - Math.floor(value);
}

function createRider() {
  const group = new THREE.Group();
  const frame = new THREE.MeshStandardMaterial({
    color: "#4bc5a4",
    roughness: 0.52,
  });
  const dark = new THREE.MeshStandardMaterial({
    color: "#202a2d",
    roughness: 0.8,
  });
  const kit = new THREE.MeshStandardMaterial({
    color: "#e8f4ee",
    roughness: 0.75,
  });
  const skin = new THREE.MeshStandardMaterial({
    color: "#d3a37b",
    roughness: 0.8,
  });

  const rear = new THREE.Vector3(-0.74, 0.38, 0);
  const front = new THREE.Vector3(0.74, 0.38, 0);
  const crank = new THREE.Vector3(-0.04, 0.42, 0);
  const seat = new THREE.Vector3(-0.36, 0.92, 0);
  const head = new THREE.Vector3(0.36, 0.91, 0);
  addCylinderBetween(group, rear, front, dark, 0.035);
  addCylinderBetween(group, rear, crank, frame, 0.035);
  addCylinderBetween(group, crank, seat, frame, 0.035);
  addCylinderBetween(group, seat, rear, frame, 0.035);
  addCylinderBetween(group, seat, head, frame, 0.035);
  addCylinderBetween(group, head, crank, frame, 0.035);
  addCylinderBetween(group, head, front, dark, 0.03);
  addCylinderBetween(
    group,
    seat,
    head.clone().add(new THREE.Vector3(-0.1, 0.32, 0)),
    dark,
    0.035,
  );
  addCylinderBetween(
    group,
    head,
    head.clone().add(new THREE.Vector3(0.22, 0, 0)),
    dark,
    0.03,
  );

  const wheel = new THREE.Mesh(
    new THREE.TorusGeometry(0.37, 0.035, 8, 24),
    dark,
  );
  wheel.rotation.y = Math.PI / 2;
  wheel.position.copy(rear);
  group.add(wheel);
  const frontWheel = wheel.clone();
  frontWheel.position.copy(front);
  group.add(frontWheel);

  const torso = new THREE.Mesh(
    new THREE.CapsuleGeometry(0.12, 0.45, 4, 8),
    kit,
  );
  torso.position.set(0.03, 1.25, 0);
  torso.rotation.z = -0.75;
  group.add(torso);
  const headMesh = new THREE.Mesh(new THREE.SphereGeometry(0.14, 12, 10), skin);
  headMesh.position.set(0.34, 1.58, 0);
  group.add(headMesh);
  addCylinderBetween(
    group,
    new THREE.Vector3(0.23, 1.4, 0),
    new THREE.Vector3(0.6, 1.1, 0),
    kit,
    0.055,
  );
  addCylinderBetween(
    group,
    new THREE.Vector3(0.6, 1.1, 0),
    head.clone(),
    kit,
    0.05,
  );
  addCylinderBetween(group, seat, crank, kit, 0.055);
  addCylinderBetween(group, crank, rear, kit, 0.055);
  group.scale.setScalar(1.5);
  group.castShadow = true;
  return group;
}

function addCylinderBetween(group, from, to, material, radius) {
  const direction = new THREE.Vector3().subVectors(to, from);
  const mesh = new THREE.Mesh(
    new THREE.CylinderGeometry(radius, radius, direction.length(), 7),
    material,
  );
  mesh.position.copy(from).add(to).multiplyScalar(0.5);
  mesh.quaternion.setFromUnitVectors(
    new THREE.Vector3(0, 1, 0),
    direction.normalize(),
  );
  mesh.castShadow = true;
  group.add(mesh);
}
