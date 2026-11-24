import { expect, test } from '@playwright/test'

// A tile's header is the framework's: its tabs with their `×`, then the host's own buttons, then `? + ⧉ ✕`.
// The host adds; it never replaces or hides. A locked scope (insilico's top-bar lock) hides what would
// close, move or add a tile.

test.beforeEach(async ({ page }) => {
	await page.goto('/')
	await page.waitForSelector('.tl-header', { timeout: 30_000 })
})

const tileOf = (page, prefix) => page.locator(`.tl-tile:has(.tl-tab[data-panel^="${prefix}"])`)

test("the host's buttons come first, the framework's always follow", async ({ page }) => {
	const titles = await page.locator('.tl-tile').first().locator('.tl-actions button').evaluateAll((els) => els.map((e) => e.title))
	expect(titles).toEqual(['Maximize', 'Keybinds', 'Add a tab', 'Float', 'Close'])
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

test('a locked dock shows no close, float, add, grip or drag, and stays put until unlocked', async ({ page }) => {
	const lock = page.locator('.tl-lock')
	await expect(lock).toHaveAttribute('aria-pressed', 'false')
	await lock.click()
	await expect(lock).toHaveAttribute('aria-pressed', 'true')
	await expect(page.locator('.tl-tab-close')).toHaveCount(0)
	await expect(page.locator('.tl-resize-handle, .tl-resize-right, .tl-resize-bottom'), 'no grips').toHaveCount(0)
	for (const title of ['Float', 'Close', 'Add a tab']) await expect(page.locator(`.tl-actions [title="${title}"]`)).toHaveCount(0)
	const tile = tileOf(page, 'console')
	await expect(tile.locator('.tl-actions [title="Keybinds"]'), 'the rest of the header stays').toHaveCount(1)
	const before = await tile.boundingBox()
	const header = await tile.locator('.tl-header').boundingBox()
	await page.mouse.move(header.x + 4, header.y + header.height / 2)
	await page.mouse.down()
	await page.mouse.move(header.x + 300, header.y + 300, { steps: 8 })
	await expect(page.locator('.tl-ghost')).toHaveCount(0)
	await page.mouse.up()
	expect(await tile.boundingBox()).toEqual(before)

	await lock.click()
	await expect(tile.locator('.tl-actions [title="Close"]')).toHaveCount(1)
})

test('the maximize key takes the hovered tile, not the one last pressed', async ({ page }) => {
	const shown = () => page.locator('.tl-tab.tl-active').evaluateAll((els) => els.map((e) => e.dataset.panel.split('-')[0]))
	const all = (await shown()).length
	const trades = page.locator('.tl-render-overlay[data-tile]').nth(3)
	await tileOf(page, 'chart').locator('.tl-tab').click()
	await trades.hover()
	await page.keyboard.press('Alt+z')
	await page.keyboard.press('f')
	expect(await shown()).toEqual(['trades'])
	await page.keyboard.press('Alt+z')
	await page.keyboard.press('f')
	expect((await shown()).length).toBe(all)
	await page.mouse.move(400, 10) // the host's bar: over no tile
	await page.keyboard.press('Alt+z')
	await page.keyboard.press('f')
	expect(await shown()).toEqual(['trades'])
})
