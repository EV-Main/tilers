import { expect, test } from '@playwright/test'

// A tile's header is the framework's: its tabs with their `×`, then the host's own buttons, then `? + ⧉ ✕`.
// The host adds; it never replaces or hides. A locked panel (insilico's console) hides what would close
// or move it.

test.beforeEach(async ({ page }) => {
	await page.goto('/')
	await page.waitForSelector('.tl-header', { timeout: 30_000 })
})

const tileOf = (page, prefix) => page.locator(`.tl-tile:has(.tl-tab[data-panel^="${prefix}"])`)

test("the host's buttons come first, the framework's always follow", async ({ page }) => {
	const titles = await page.locator('.tl-tile').first().locator('.tl-actions button').evaluateAll((els) => els.map((e) => e.title))
	expect(titles).toEqual(['Maximize', 'Collapse', 'Keybinds', 'Add a tab', 'Float', 'Close'])
})

test("a tab's × closes that tab alone, the header's ✕ the whole tile", async ({ page }) => {
	const tile = tileOf(page, 'chart')
	await tile.locator('.tl-actions [title="Add a tab"]').click()
	await page.getByRole('button', { name: 'News', exact: true }).click()
	const tabs = tile.locator('.tl-tab')
	await expect(tabs).toHaveCount(2)
	const added = await tabs.nth(1).getAttribute('data-panel')
	await expect(tabs.nth(1), 'the added tab shows').toHaveClass(/tl-active/)

	await tabs.nth(0).hover()
	await tabs.nth(0).locator('.tl-tab-close').click()
	await expect(page.locator('.tl-tab[data-panel^="chart"]'), 'the closed tab is gone').toHaveCount(0)
	await expect(page.locator('.tl-ghost'), 'and no drag started').toHaveCount(0)

	const count = await page.locator('.tl-tile').count()
	await page.locator(`.tl-tile:has(.tl-tab[data-panel="${added}"]) .tl-actions [title="Close"]`).click()
	await expect(page.locator('.tl-tile')).toHaveCount(count - 1)
})

test('a locked panel shows no close, float or drag, and stays put', async ({ page }) => {
	const tile = tileOf(page, 'console')
	await expect(tile.locator('.tl-tab-close')).toHaveCount(0)
	await expect(tile.locator('.tl-actions [title="Float"]')).toHaveCount(0)
	await expect(tile.locator('.tl-actions [title="Close"]')).toHaveCount(0)
	await expect(tile.locator('.tl-actions [title="Keybinds"]'), 'the rest of the header stays').toHaveCount(1)
	const before = await tile.boundingBox()
	const header = await tile.locator('.tl-header').boundingBox()
	await page.mouse.move(header.x + 4, header.y + header.height / 2)
	await page.mouse.down()
	await page.mouse.move(header.x + 300, header.y + 300, { steps: 8 })
	await expect(page.locator('.tl-ghost')).toHaveCount(0)
	await page.mouse.up()
	expect(await tile.boundingBox()).toEqual(before)
})

test('a collapsed tile is a spine as wide as the title bar is tall, and a click opens it', async ({ page }) => {
	const tile = page.locator('.tl-tile').first()
	const panel = await tile.locator('.tl-tab.tl-active').getAttribute('data-panel')
	const open = await tile.boundingBox()
	const titleH = (await tile.locator('.tl-header').boundingBox()).height
	const step = await page.evaluate(() => document.querySelector('.tl-packed').clientWidth / 64)

	await tile.locator('.tl-actions [title="Collapse"]').click()
	const spine = page.locator(`.tl-tile.tl-collapsed:has(.tl-tab[data-panel="${panel}"])`)
	await expect(spine).toHaveCount(1)
	const box = await spine.boundingBox()
	expect(box.width, 'the spine fits the title bar on its side').toBeGreaterThanOrEqual(titleH - 1)
	expect(box.width, 'and no more than a step past it').toBeLessThan(titleH + step + 2)
	expect(box.height, 'a spine keeps its height').toBeCloseTo(open.height, 0)

	const tab = await spine.locator('.tl-tab').boundingBox()
	await page.mouse.click(tab.x + tab.width / 2, tab.y + 8)
	const reopened = page.locator(`.tl-tile:has(.tl-tab[data-panel="${panel}"])`)
	await expect(reopened).not.toHaveClass(/tl-collapsed/)
	expect((await reopened.boundingBox()).width, 'it opens to the width it kept').toBeCloseTo(open.width, 0)
})
