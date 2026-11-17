import { expect, test } from '@playwright/test'

// A native drag (a host's card, a file) held over an inactive tab opens it, so the tab's content can
// take the drop.

test('a native drag entering an inactive tab opens it', async ({ page }) => {
	await page.goto('/')
	await page.waitForSelector('.tl-header', { timeout: 30_000 })
	const header = page.locator('.tl-header').first()
	await header.getByTitle('Add window as a tab').click()
	await page.getByRole('button', { name: 'Console', exact: true }).click()
	await expect(header.locator('.tl-tab')).toHaveCount(2)
	const tab = header.locator('.tl-tab:not(.tl-active)')
	const panel = await tab.getAttribute('data-panel')
	// The overlay showing at the tile body's centre is the active tab's content.
	const shown = () =>
		header.evaluate((h) => {
			const r = h.closest('.tl-tile').getBoundingClientRect()
			const o = document.elementFromPoint(r.left + r.width / 2, r.top + (r.height + h.offsetHeight) / 2).closest('.tl-render-overlay')
			return [...o.parentElement.children].indexOf(o)
		})
	const before = await shown()

	const dataTransfer = await page.evaluateHandle(() => new DataTransfer())
	await tab.dispatchEvent('dragenter', { dataTransfer })

	await expect(page.locator(`.tl-tab[data-panel="${panel}"]`)).toHaveClass(/tl-active/)
	await expect.poll(shown).not.toBe(before)
})
