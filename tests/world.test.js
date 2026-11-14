import test from "node:test";
import assert from "node:assert/strict";

import * as THREE from "three";
import { createRider, raiseCameraAboveTerrain } from "../web/world.js";

test("bike wheels face the travel plane and rotate around their axles", () => {
  const rider = createRider();
  const wheels = rider.children.filter((child) => child.userData.rollingWheel);
  assert.equal(wheels.length, 2);

  for (const wheel of wheels) {
    wheel.geometry.computeBoundingBox();
    const size = new THREE.Vector3();
    wheel.geometry.boundingBox.getSize(size);
    assert.ok(size.x > size.z * 5);
    assert.ok(size.y > size.z * 5);
  }

  rider.traverse((object) => {
    object.geometry?.dispose();
    if (Array.isArray(object.material)) {
      object.material.forEach((material) => material.dispose());
    } else {
      object.material?.dispose();
    }
  });
});

test("follow cameras rise above terrain blocking their sightline", () => {
  const camera = new THREE.Vector3(0, 10, 0);
  const target = new THREE.Vector3(10, 0, 0);

  raiseCameraAboveTerrain(camera, target, (x) => (x === 5 ? 8 : 0));

  assert.ok(camera.y >= 18);
});
