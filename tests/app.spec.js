import { expect, test } from "@playwright/test";

test("Pages base loads the dashboard and all application assets", async ({
  page,
}) => {
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  page.on("console", (message) => {
    if (message.type() === "error") errors.push(message.text());
  });
  const response = await page.goto("./");
  expect(response?.ok()).toBeTruthy();
  await expect(page.getByRole("heading", { name: /Good ride/ })).toBeVisible();
  await expect(page.getByText("Your data stays yours")).toBeVisible();
  expect(errors).toEqual([]);
  await page
    .locator(".sidebar")
    .getByRole("link", { name: "Settings" })
    .click();
  await expect(
    page.getByRole("heading", { name: "Settings", exact: true }),
  ).toBeVisible();
  expect(new URL(page.url()).pathname).toBe("/velocity/");
  expect(new URL(page.url()).hash).toBe("#/settings");
});

test("ride starts in a real WebGL2 scene and records a local simulated activity", async ({
  page,
}) => {
  const errors = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.goto("./#/ride");
  await expect(page.locator("#renderer-label")).toContainText(
    /WebGL 2|unavailable/,
  );
  const hasWebGl = await page.locator("#world").evaluate((canvas) => {
    return Boolean(canvas.getContext("webgl2"));
  });
  expect(hasWebGl).toBeTruthy();
  await page.getByRole("button", { name: "Start ride" }).click();
  await expect(page.locator("#elapsed")).not.toHaveText("00:00:00", {
    timeout: 25_000,
  });
  await expect(page.locator("#distance")).not.toHaveText("0.00", {
    timeout: 25_000,
  });
  await page.getByRole("button", { name: "Finish" }).click();
  await page
    .locator(".sidebar")
    .getByRole("link", { name: "Activities" })
    .click();
  await expect(
    page.locator(".activity-row .activity-details strong"),
  ).toHaveText("Alpine training loop");
  await expect(
    page.locator(".activity-row .activity-details small"),
  ).toBeVisible();
  await expect(
    page.locator(".activity-row .activity-details small"),
  ).toContainText("SIMULATED");
  expect(errors).toEqual([]);
});

test("GPX route import and theme preference work without external services", async ({
  page,
}) => {
  await page.goto("./#/routes");
  await page.getByRole("button", { name: "Import route" }).click();
  await page.locator("#file-import").setInputFiles({
    name: "test-loop.gpx",
    mimeType: "application/gpx+xml",
    buffer: Buffer.from(
      `<?xml version="1.0"?><gpx xmlns="http://www.topografix.com/GPX/1/1"><trk><name>Test loop</name><trkseg><trkpt lat="46.1" lon="7.1"><ele>500</ele></trkpt><trkpt lat="46.101" lon="7.101"><ele>520</ele></trkpt></trkseg></trk></gpx>`,
    ),
  });
  await expect(page.getByText("Test loop")).toBeVisible();
  await page
    .locator(".sidebar")
    .getByRole("link", { name: "Settings" })
    .click();
  await page.locator("#theme-setting").selectOption("dark");
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
});
