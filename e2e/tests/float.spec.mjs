import { expect, test } from '@playwright/test'

// A float is a layer above the grid: lifting a tile out lets the grid fill in, the panel's content is
// the same node (never remounted), it stays put while the dock scrolls, its header moves it, `⊟` docks
// it where it shows, and dragging its tab docks it where it drops.

test.beforeEach(async ({ page }) => {
	await page.goto('/')
	await page.waitForSelector('.tl-header', { timeout: 30_000 })
})

const floats = (page) => page.locator('.tl-floats .tl-tile.tl-float')
const tileOf = (page, prefix) => page.locator(`.tl-tile:has(.tl-tab[data-panel^="${prefix}"])`)

/** Float the tile showing `prefix`, marking its panel's content node first. */
const lift = async (page, prefix) => {
	const panel = await tileOf(page, prefix).locator('.tl-tab.tl-active').getAttribute('data-panel')
	await tileOf(page, prefix).locator('.tl-actions [title="Float"]').click()
	await expect(floats(page)).toHaveCount(1)
	return panel
}

test('⧉ lifts a tile into the float layer and the grid fills in, its content kept', async ({ page }) => {
	const grid = await page.locator('.tl-packed > .tl-tile').count()
	const nodes = await page.locator('.tl-render-overlay').count()
	await page.evaluate(() => document.querySelectorAll('.tl-render-overlay').forEach((e, i) => (e.__tl = i)))
	const panel = await lift(page, 'chart')
	await expect(floats(page).locator(`.tl-tab[data-panel="${panel}"]`)).toHaveCount(1)
	await expect(page.locator('.tl-packed > .tl-tile')).toHaveCount(grid - 1)
	const kept = await page.evaluate(() => [...document.querySelectorAll('.tl-render-overlay')].every((e, i) => e.__tl === i))
	expect(kept, 'every content node is the one it was').toBe(true)
	expect(await page.locator('.tl-render-overlay').count()).toBe(nodes)

	const shell = await floats(page).boundingBox()
	const shown = await page.evaluate(() => {
		const r = [...document.querySelectorAll('.tl-render-overlay')].find((e) => getComputedStyle(e).position === 'sticky').getBoundingClientRect()
		return { x: r.x, y: r.y, width: r.width, height: r.height }
	})
	expect(shown.x).toBeCloseTo(shell.x, 0)
	expect(shown.width).toBeCloseTo(shell.width, 0)
	expect(shown.y + shown.height).toBeCloseTo(shell.y + shell.height, 0)

	// Its frame paints its border and grips over the content, so its body must let the content through.
	const hit = await page.evaluate(({ x, y }) => {
		const el = document.elementFromPoint(x, y)
		return { content: el.closest('.tl-render-overlay') !== null, frame: el.closest('.tl-float') !== null }
	}, { x: shell.x + shell.width / 2, y: shell.y + shell.height / 2 })
	expect(hit, "a float's body hits its content, not its frame").toEqual({ content: true, frame: false })
	const paint = await floats(page).evaluate((f) => [getComputedStyle(f).backgroundColor, getComputedStyle(f.querySelector('.tl-group')).backgroundColor])
	expect(paint, 'its frame is see-through over the content').toEqual(['rgba(0, 0, 0, 0)', 'rgba(0, 0, 0, 0)'])
})

test('a float stays put while the dock scrolls under it', async ({ page }) => {
	await page.setViewportSize({ width: 1400, height: 420 })
	await page.waitForTimeout(300)
	await lift(page, 'chart')
	const at = await floats(page).boundingBox()
	const scrolled = await page.evaluate(() => {
		const root = document.querySelector('.tl-packed')
		root.scrollTop = 200
		return root.scrollTop
	})
	expect(scrolled, 'the dock scrolls at this height').toBeGreaterThan(0)
	await page.waitForTimeout(100)
	expect(await floats(page).boundingBox()).toEqual(at)
})

test('its header moves it, and ⊟ docks it where it shows', async ({ page }) => {
	await lift(page, 'chart')
	const at = await floats(page).boundingBox()
	await page.mouse.move(at.x + 4, at.y + 10)
	await page.mouse.down()
	await page.mouse.move(at.x + 104, at.y + 90, { steps: 8 })
	await page.mouse.up()
	const moved = await floats(page).boundingBox()
	expect(moved.x).toBeGreaterThan(at.x + 50)
	expect(moved.y).toBeGreaterThan(at.y + 40)
	await expect(page.locator('.tl-ghost')).toHaveCount(0)

	await floats(page).locator('.tl-actions [title="Dock"]').click()
	await expect(floats(page)).toHaveCount(0)
	await expect(tileOf(page, 'chart')).toHaveCount(1)
})

test("dragging a float's tab onto a tile's header joins it", async ({ page }) => {
	const panel = await lift(page, 'chart')
	const tab = await floats(page).locator('.tl-tab').first().boundingBox()
	const target = await tileOf(page, 'trades').locator('.tl-header').boundingBox()
	await page.mouse.move(tab.x + tab.width / 2, tab.y + tab.height / 2)
	await page.mouse.down()
	await page.mouse.move(target.x + target.width - 30, target.y + target.height / 2, { steps: 12 })
	await page.mouse.up()
	await expect(floats(page)).toHaveCount(0)
	await expect(tileOf(page, 'trades').locator(`.tl-tab[data-panel="${panel}"]`)).toHaveCount(1)
})

test('a host opens a panel straight into the float layer', async ({ page }) => {
	const grid = await page.locator('.tl-packed > .tl-tile').count()
	await page.getByTitle('Open a floating window').click()
	await expect(floats(page)).toHaveCount(1)
	await expect(floats(page).locator('.tl-tab[data-panel^="news"]')).toHaveCount(1)
	await expect(page.locator('.tl-packed > .tl-tile')).toHaveCount(grid)
})
