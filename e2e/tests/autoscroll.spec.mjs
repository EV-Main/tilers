import { expect, test } from '@playwright/test'

// A held drag near the root's edge scrolls it with no further pointer moves, and the drop resolves
// against the scrolled view, not the one the drag started in.

test.beforeEach(async ({ page, browserName }) => {
	// Firefox routes the drag's pointermoves to the pressed header, not the capture surface, so no
	// drag ever arms there; plain drags included.
	test.fixme(browserName === 'firefox', 'drag never arms in Firefox')
	await page.setViewportSize({ width: 1280, height: 200 })
	await page.goto('/')
	await page.waitForSelector('.tl-header', { timeout: 30_000 })
})

async function check(page, dir) {
	const root = page.locator('.tl-packed')
	// Up: the lowest header, scrolled to mid-view. Down: the highest, from the top. Either way the trip
	// to the edge we hold at crosses the view.
	const view = await root.evaluate((el, dir) => {
		const r = el.getBoundingClientRect()
		const tabs = [...el.querySelectorAll('.tl-header .tl-tab.tl-active')].map((t) => ({ t, y: t.getBoundingClientRect().top - r.top + el.scrollTop }))
		tabs.sort((a, b) => (dir === 'up' ? b.y - a.y : a.y - b.y))
		el.scrollTop = dir === 'up' ? tabs[0].y - el.clientHeight / 2 : 0
		return { top: r.top, bottom: r.top + el.clientHeight, max: el.scrollHeight - el.clientHeight, scroll: el.scrollTop, panel: tabs[0].t.dataset.panel }
	}, dir)
	expect(view.max, 'the layout must overflow the short viewport').toBeGreaterThan(0)
	if (dir === 'up') expect(view.scroll).toBeGreaterThan(0)
	await page.waitForTimeout(100)
	const tab = page.locator(`.tl-tab[data-panel="${view.panel}"]`)
	// Content px, so the scroll doesn't enter.
	const top = () => tab.evaluate((t) => t.closest('.tl-tile').offsetTop)
	const start = await top()
	const box = await tab.boundingBox()
	const [x, y] = [box.x + box.width / 2, box.y + box.height / 2]

	await page.mouse.move(x, y)
	await page.mouse.down()
	await page.mouse.move(x + 12, y + 12, { steps: 3 }) // past the arm threshold
	const edge = dir === 'up' ? view.top - 20 : view.bottom - 2 // up: over the host chrome, past the root
	await page.mouse.move(x + 12, edge, { steps: 10 })
	await expect
		.poll(() => root.evaluate((el, dir) => (dir === 'up' ? el.scrollTop === 0 : el.scrollTop >= el.scrollHeight - el.clientHeight - 1), dir), {
			timeout: 5_000,
		})
		.toBe(true)
	await page.mouse.up()

	// Settling may lift a tile dropped low, never sink one dropped high.
	if (dir === 'up') await expect.poll(top).toBe(0)
	else await expect.poll(top).toBeGreaterThan(start)
}

test('holding a drag above the root scrolls it up', async ({ page }) => check(page, 'up'))
test('holding a drag below the root scrolls it down', async ({ page }) => check(page, 'down'))
