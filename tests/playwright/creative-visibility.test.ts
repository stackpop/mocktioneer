import { test, expect } from '@playwright/test';

/**
 * Test that mocktioneer creative text is visible and not clipped.
 *
 * The creative displays "{width}×{height}" text centered in the SVG.
 * This test verifies the text fits within the creative bounds and is readable.
 */

interface AdSize {
  width: number;
  height: number;
  cpm: number;
}

// Fetched from /_/sizes endpoint before tests run
let AD_SIZES: AdSize[] = [];

test.beforeAll(async ({ request }) => {
  const response = await request.get('/_/sizes');
  expect(response.ok()).toBe(true);
  const data = await response.json();
  AD_SIZES = data.sizes;
  expect(AD_SIZES.length).toBeGreaterThan(0);
});

test.describe('Creative visibility tests', () => {
  test('sizes endpoint returns valid data', async ({ request }) => {
    const response = await request.get('/_/sizes');
    expect(response.ok()).toBe(true);
    const data = await response.json();
    expect(data.sizes).toBeInstanceOf(Array);
    expect(data.sizes.length).toBe(13);
    for (const size of data.sizes) {
      expect(typeof size.width).toBe('number');
      expect(typeof size.height).toBe('number');
      expect(typeof size.cpm).toBe('number');
    }
  });

  for (const size of AD_SIZES) {
    test(`${size.width}x${size.height} - dimension text is visible`, async ({ page }) => {
      // Load the SVG creative directly
      await page.goto(`/static/img/${size.width}x${size.height}.svg`);

      // Get the SVG element
      const svg = page.locator('svg');
      await expect(svg).toBeVisible();

      // Find the main dimension text (e.g., "300×250")
      const expectedText = `${size.width}×${size.height}`;
      const mainText = svg.locator(`text:has-text("${expectedText}")`);
      await expect(mainText).toBeVisible();

      // Verify the text bounding box is within the SVG bounds
      const svgBox = await svg.boundingBox();
      const textBox = await mainText.boundingBox();

      expect(svgBox).not.toBeNull();
      expect(textBox).not.toBeNull();

      if (svgBox && textBox) {
        // Text should be fully within SVG bounds (with small tolerance for anti-aliasing)
        const tolerance = 2;
        expect(textBox.x).toBeGreaterThanOrEqual(svgBox.x - tolerance);
        expect(textBox.y).toBeGreaterThanOrEqual(svgBox.y - tolerance);
        expect(textBox.x + textBox.width).toBeLessThanOrEqual(svgBox.x + svgBox.width + tolerance);
        expect(textBox.y + textBox.height).toBeLessThanOrEqual(svgBox.y + svgBox.height + tolerance);

        // Log dimensions for debugging
        console.log(`${size.width}x${size.height}: SVG=${svgBox.width}x${svgBox.height}, Text=${Math.round(textBox.width)}x${Math.round(textBox.height)}`);
      }
    });

    test(`${size.width}x${size.height} - caption text is visible`, async ({ page }) => {
      await page.goto(`/static/img/${size.width}x${size.height}.svg`);

      const svg = page.locator('svg');
      await expect(svg).toBeVisible();

      // Find the caption text "mocktioneer banner"
      const caption = svg.locator('text:has-text("mocktioneer banner")');
      await expect(caption).toBeVisible();

      // Verify caption is within bounds
      const svgBox = await svg.boundingBox();
      const captionBox = await caption.boundingBox();

      expect(svgBox).not.toBeNull();
      expect(captionBox).not.toBeNull();

      if (svgBox && captionBox) {
        const tolerance = 2;
        expect(captionBox.x).toBeGreaterThanOrEqual(svgBox.x - tolerance);
        expect(captionBox.y).toBeGreaterThanOrEqual(svgBox.y - tolerance);
        expect(captionBox.x + captionBox.width).toBeLessThanOrEqual(svgBox.x + svgBox.width + tolerance);
        expect(captionBox.y + captionBox.height).toBeLessThanOrEqual(svgBox.y + svgBox.height + tolerance);
      }
    });
  }
});

test.describe('Creative HTML wrapper tests', () => {
  for (const size of AD_SIZES) {
    test(`${size.width}x${size.height} - HTML creative renders correctly`, async ({ page }) => {
      // Load the HTML creative wrapper
      await page.goto(`/static/creatives/${size.width}x${size.height}.html`);

      // The creative should contain an img pointing to the SVG
      const img = page.locator('img#creative-img');
      await expect(img).toBeVisible();

      // Verify the image loaded successfully (not broken)
      const naturalWidth = await img.evaluate((el: HTMLImageElement) => el.naturalWidth);
      expect(naturalWidth).toBeGreaterThan(0);
    });
  }
});

test.describe('Creative with bid display', () => {
  test('SVG shows bid amount when provided', async ({ page }) => {
    // The SVG endpoint should accept a bid query param
    await page.goto('/static/img/300x250.svg?bid=2.50');

    const svg = page.locator('svg');
    await expect(svg).toBeVisible();

    // Should show the bid amount in the caption
    const bidText = svg.locator('text:has-text("$2.50")');
    await expect(bidText).toBeVisible();
  });
});
