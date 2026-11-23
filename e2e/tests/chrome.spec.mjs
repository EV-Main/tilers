import { expect, test } from '@playwright/test'

// A tile's chrome is the host's: insilico gives every tab a label with its own `×`, and its header
// buttons call the dock's api. The framework keeps the gestures around them.

test.beforeEach(async ({ page }) => {
	await page.goto('/')
	await page.waitForSelector('.tl-header', { timeout: 30_000 })
})

test("a tab's label is the host's, and its × closes that tab alone", async ({ page }) => {
	const tile = page.locator('.tl-tile').first()
	await tile.locator('.tl-actions [title="Add window as a tab"]').click()
	await page.getByRole('button', { name: 'News', exact: true }).click()
	const tabs = tile.locator('.tl-tab')
	await expect(tabs).toHaveCount(2)
	const [first, added] = await tabs.evaluateAll((els) => els.map((e) => e.dataset.panel))
	await expect(tabs.nth(1), 'the added tab shows').toHaveClass(/tl-active/)

	await tabs.nth(0).locator('button[title="Close"]').click()
	await expect(tabs).toHaveCount(1)
	await expect(tabs.nth(0)).toHaveAttribute('data-panel', added)
	await expect(page.locator(`[data-panel="${first}"]`), 'the closed tab is gone, and no drag started').toHaveCount(0)
	await expect(page.locator('.tl-ghost')).toHaveCount(0)
})

test('a tile names the panel its strip shows', async ({ page }) => {
	const pairs = await page.$$eval('.tl-tile', (tiles) => tiles.map((t) => [t.dataset.panel, t.querySelector('.tl-tab.tl-active').dataset.panel]))
	expect(pairs.length).toBeGreaterThan(1)
	for (const [tile, tab] of pairs) expect(tile).toBe(tab)
})

test('a collapsed tile is a spine as wide as the title bar is tall, and a click opens it', async ({ page }) => {
	const tile = page.locator('.tl-tile').first()
	const panel = await tile.getAttribute('data-panel')
	const open = await tile.boundingBox()
	const titleH = (await tile.locator('.tl-header').boundingBox()).height
	const step = await page.evaluate(() => document.querySelector('.tl-packed').clientWidth / 64)

	await tile.locator('.tl-actions [title="Collapse"]').click()
	const spine = page.locator(`.tl-tile.tl-collapsed[data-panel="${panel}"]`)
	await expect(spine).toHaveCount(1)
	const box = await spine.boundingBox()
	expect(box.width, 'the spine fits the title bar on its side').toBeGreaterThanOrEqual(titleH - 1)
	expect(box.width, 'and no more than a step past it').toBeLessThan(titleH + step + 2)
	expect(box.height, 'a spine keeps its height').toBeCloseTo(open.height, 0)

	const tab = await spine.locator('.tl-tab').boundingBox()
	await page.mouse.click(tab.x + tab.width / 2, tab.y + tab.height - 4)
	const reopened = page.locator(`.tl-tile[data-panel="${panel}"]`)
	await expect(reopened).not.toHaveClass(/tl-collapsed/)
	expect((await reopened.boundingBox()).width, 'it opens to the width it kept').toBeCloseTo(open.width, 0)
})
