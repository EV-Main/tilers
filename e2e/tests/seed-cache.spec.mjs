import { expect, test } from '@playwright/test'

// The framework's built-in client-side layout cache, exercised through the insilico example (which
// carries zero persistence code of its own — that is the point). `Alt+z s` writes the live layout to
// `localStorage` under `<storage_key>-<band>`; a reload restores it instead of re-seeding; a
// viewport change into another band uses a different key; `Alt+z S` also calls the host hook. Both
// say what happened in the dock's own `.tl-notice`.

const KEY = 'insilico-layout'

/** Boot and wait for the dock. Playwright gives each test its own context, so the cache starts empty
 * — and stays that way across a `reload`, which an init-script wipe would not. */
const boot = async (page) => {
	await page.goto('/')
	await page.waitForSelector('.tl-header', { timeout: 30_000 })
}

/** A chord sequence: each keydown in turn, as a user types it. */
const chord = async (page, ...keys) => {
	for (const k of keys) await page.keyboard.press(k)
}
const CACHE = ['Alt+z', 's']
const PUBLISH = ['Alt+z', 'Shift+S']

const cache = (page, band) => page.evaluate((k) => localStorage.getItem(k), `${KEY}-${band}`)
/** Tile geometry, in the skeleton's own order — the layout's observable signature. */
const geometry = (page) => page.$$eval('.tl-tile', (els) => els.map((e) => e.getAttribute('style')))

test.beforeEach(async ({ page }) => await boot(page))

test('leader s caches the current band under its own key', async ({ page }) => {
	expect(await cache(page, 'xl'), 'a fresh profile starts with no cache').toBe(null)
	await chord(page, ...CACHE)
	const json = await cache(page, 'xl')
	expect(json, 'leader s must write the xl entry').not.toBe(null)
	expect(JSON.parse(json), 'the cached payload is the versioned layout').toHaveProperty('grid')
	await expect(page.locator('.tl-notice'), 'caching says so without any host code').toContainText('xl')
})

test('a cached layout survives a reload', async ({ page }) => {
	// Grow the first tile so the restored layout is distinguishable from a fresh seed.
	const grip = page.locator('.tl-tile .tl-resize-handle').first()
	const box = await grip.boundingBox()
	await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
	await page.mouse.down()
	await page.mouse.move(box.x + 160, box.y + 120, { steps: 8 })
	await page.mouse.up()

	const edited = await geometry(page)
	await chord(page, ...CACHE)
	await page.reload()
	await page.waitForSelector('.tl-header', { timeout: 30_000 })
	expect(await geometry(page), 'the restored layout must match what was cached').toEqual(edited)
})

test('each band caches under its own key', async ({ page }) => {
	await chord(page, ...CACHE)
	const xl = await cache(page, 'xl')
	expect(xl).not.toBe(null)

	await page.setViewportSize({ width: 700, height: 800 })
	// The band change re-seeds; wait for the dock to settle into the narrow arrangement.
	await page.waitForFunction(() => document.querySelectorAll('.tl-tile').length > 0)
	await chord(page, ...CACHE)

	expect(await cache(page, 'sm'), 'the narrow band writes the sm entry').not.toBe(null)
	expect(await cache(page, 'xl'), 'the xl entry must be untouched by an sm save').toBe(xl)
})

test('leader S caches, then calls the host hook', async ({ page }) => {
	await chord(page, ...PUBLISH)
	await expect(page.locator('.tl-notice'), "the example's publish hook reports through the dock").toContainText('publish')
	expect(await cache(page, 'xl'), 'publishing caches first').not.toBe(null)
})
