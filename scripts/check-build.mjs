import assert from "node:assert/strict";
import { access, readFile, writeFile } from "node:fs/promises";
import path from "node:path";

const outputDirectory = path.resolve("dist");
const requiredFiles = [
  "index.html",
  "manifest.webmanifest",
  "icon.svg",
  "sw.js",
];
for (const filename of requiredFiles) {
  await access(path.join(outputDirectory, filename));
}

const html = await readFile(path.join(outputDirectory, "index.html"), "utf8");
assert.match(
  html,
  /\/velocity\//,
  "Production entrypoint must use the /velocity/ base path.",
);
assert.doesNotMatch(
  html,
  /(?:src|href)=["']\/(?!velocity\/)/,
  "A root-relative asset path was found.",
);

for (const match of html.matchAll(/\b(?:src|href)=["']([^"']+)["']/g)) {
  const resource = match[1];
  if (/^(?:[a-z]+:|#|data:)/i.test(resource)) continue;
  assert.ok(
    resource.startsWith("./") || resource.startsWith("/velocity/"),
    `Asset path is not relative or prefixed with /velocity/: ${resource}`,
  );
  const relativePath = resource.startsWith("/velocity/")
    ? resource.slice("/velocity/".length)
    : resource.slice(2);
  const resolvedPath = path.resolve(outputDirectory, relativePath);
  assert.ok(
    resolvedPath.startsWith(`${outputDirectory}${path.sep}`),
    `Asset resolves outside the deployment directory: ${resource}`,
  );
  await access(resolvedPath);
}

const bundledFiles = html.matchAll(
  /(?:src|href)=["']([^"']+\.(?:js|css))["']/g,
);
let bundleCount = 0;
for (const [, resource] of bundledFiles) {
  bundleCount += 1;
  assert.ok(
    resource.includes("/velocity/") || resource.startsWith("./"),
    `JavaScript or CSS is missing the project base path: ${resource}`,
  );
}
assert.ok(
  bundleCount >= 2,
  "Expected at least one JavaScript and one CSS bundle.",
);

await writeFile(path.join(outputDirectory, ".nojekyll"), "");
process.stdout.write(
  `Verified ${requiredFiles.length} required files and ${bundleCount} base-prefixed bundles.\n`,
);
