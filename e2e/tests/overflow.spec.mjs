import { expect, test } from '@playwright/test'

// The packed model guarantees x + w ≤ cols for every tile (see the Rust `refit_clamps_into_narrower_band`
// test), so the grid must NEVER overflow horizontally. It used to anyway: the per-step size was derived
// from getBoundingClientRect width, which includes the vertical scrollbar's gutter (and tiles from their
// border box), so once a tall layout scrolled vertically the rightmost column spilled past the content
// area and grew a horizontal scrollbar. `scrollWidth > clientWidth` is exactly that — content pushed out
// of the horizontal view — and is the overlay-scrollbar-agnostic way to detect it.

// Headless Chromium defaults to overlay (zero-width) scrollbars, under which the gutter never steals
// width and the bug can't reproduce. Force classic, space-consuming scrollbars so the guard has teeth.
const CLASSIC_SCROLLBARS = '::-webkit-scrollbar { width: 15px; height: 15px; } ::-webkit-scrollbar-thumb { background: #555; }'

test.beforeEach(async ({ page }) => {
	await page.goto('/')
	await page.waitForSelector('.tl-header', { timeout: 30_000 })
	await page.addStyleTag({ content: CLASSIC_SCROLLBARS })
})

// A wide desktop (no overflow expected), then short/narrow viewports that force a vertical scrollbar and
// reflow a right-edge tile — the exact "arranged wide, viewed small" case the user hit.
for (const [w, h] of [
	[1280, 720],
	[520, 360],
	[400, 300],
	[360, 700],
]) {
	test(`no horizontal overflow at ${w}x${h}`, async ({ page }) => {
		await page.setViewportSize({ width: 1280, height: 720 })
		await page.waitForTimeout(300)
		await page.setViewportSize({ width: w, height: h })
		await page.waitForTimeout(400) // let the onresize effect refit + re-measure
		const { scrollWidth, clientWidth } = await page
			.locator('.tl-packed')
			.evaluate((el) => ({ scrollWidth: el.scrollWidth, clientWidth: el.clientWidth }))
		expect(scrollWidth, `grid content (${scrollWidth}px) spilled past the view (${clientWidth}px)`).toBeLessThanOrEqual(clientWidth + 1)
	})
}
