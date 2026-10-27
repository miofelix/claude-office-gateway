// Run with an existing ego-browser Page; see docs/LAYOUT_VALIDATION.md.
export async function testLayout(page, options = {}) {
  const baseURL = options.baseURL ?? "http://127.0.0.1:1420";
  const failures = [];
  let checks = 0;
  async function inspect(label) {
    const result = await page.evaluate(() => window.inspectLayout());
    checks++;
    for (const failure of result.failures) {
      failures.push(`${label} ${result.width}x${result.height}: ${failure}`);
    }
  }
  async function size(width, height = 560) {
    await page.cdp("Emulation.setDeviceMetricsOverride", {
      width,
      height,
      deviceScaleFactor: 1,
      mobile: false,
    });
  }
  async function load(params) {
    await page.goto(baseURL + "/tests/layout/?" + new URLSearchParams(params));
    await page.waitForFunction(
      () =>
        window.inspectLayout &&
        document.querySelector(".header-actions button")?.disabled === false,
    );
  }
  for (const platform of ["macos", "windows"]) {
    for (const language of ["en", "zh"]) {
      for (const theme of ["light", "dark"]) {
        const label = `${platform}/${language}/${theme}`;
        await load({ platform, language, theme });
        for (const width of [360, 480, 620, 780, 920, 921, 1060, 1440]) {
          await size(width);
          await inspect(label);
        }
        await size(1060, 800);
        await inspect(label);
        await page.click(".advanced-settings > summary");
        await page.click(".trust-details > summary");
        for (const width of [360, 780, 921, 1060]) {
          await size(width);
          await inspect(label + "/expanded");
        }
      }
      for (const scenario of ["ready", "error", "update", "update-error"]) {
        await load({ platform, language, theme: "light", scenario });
        if (scenario.startsWith("update")) {
          await page.click(".version-button");
          await page.waitForSelector("dialog[open]");
        }
        for (const width of [360, 780, 1060]) {
          await size(width);
          await inspect(`${platform}/${language}/${scenario}`);
        }
        if (scenario === "ready") {
          await page.click('.copy-button[aria-label="Copy Gateway token"]');
          await size(780);
          await inspect(`${platform}/${language}/copied`);
        }
      }
      await load({ platform, language, theme: "dark" });
      await page.click(".certificate-actions .button.secondary");
      await page.waitForSelector("dialog[open]");
      for (const width of [360, 780]) {
        await size(width);
        await inspect(`${platform}/${language}/certificate-dialog`);
      }
    }
  }
  console.log(JSON.stringify({ checks, failures }, null, 2));
  if (failures.length)
    throw new Error(
      `${failures.length} layout regressions across ${checks} checks`,
    );
  return checks;
}
