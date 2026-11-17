import { expect, test } from '@playwright/test'

// The key footprint, as the host sees it. The library calls `preventDefault()` exactly when it acted
// on a key (and logs a `tilers keybind:` line when it recognized one), so a probe reading
// `defaultPrevented` tells "ours" from "the page's". A bare letter must stay un-prevented, or the
// suite would pass on a listener that swallows everything.

/** Boot and install the probe. It runs in the bubble phase, after the library's capture-phase
 * listener, so it observes the already-set `defaultPrevented`. */
const boot = async (page) => {
	await page.goto('/')
	// The keydown listener is installed when PackedArea mounts; the header proves the wasm booted.
	await page.waitForSelector('.tl-header', { timeout: 30_000 })
	await page.evaluate(() => {
		globalThis.__tlProbe = []
		window.addEventListener('keydown', (e) => globalThis.__tlProbe.push({ key: e.key, prevented: e.defaultPrevented }))
	})
}

const lastPrevented = (page) => page.evaluate(() => globalThis.__tlProbe.at(-1)?.prevented ?? null)

/** Grow the first tile, so the undo history has somewhere to go: undo with nothing to undo is,
 * correctly, left to the page. */
const edit = async (page) => {
	const grip = page.locator('.tl-tile .tl-resize-handle').first()
	const box = await grip.boundingBox()
	await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
	await page.mouse.down()
	await page.mouse.move(box.x + 160, box.y + 120, { steps: 8 })
	await page.mouse.up()
}

test.describe('default platform', () => {
	test.beforeEach(async ({ page }) => await boot(page))

	test('bare keys are the page’s', async ({ page }) => {
		for (const k of ['u', 'f', 'd', 'Shift+Slash', 'Backspace', 'Alt+s', 'j']) {
			await page.keyboard.press(k)
			expect(await lastPrevented(page), `${k} must reach the page untouched`).toBe(false)
		}
		await expect(page.locator('.tl-help')).toHaveCount(0)
	})

	test('the leader opens the scope, and u then undoes', async ({ page }) => {
		const logs = []
		page.on('console', (m) => m.text().startsWith('tilers keybind') && logs.push(m.text()))
		await edit(page)
		await page.keyboard.press('Alt+z')
		expect(await lastPrevented(page), 'the leader is ours').toBe(true)
		await page.keyboard.press('u')
		expect(await lastPrevented(page), 'leader u undoes').toBe(true)
		expect(logs.some((l) => /undo/i.test(l)), `recognition must log to console: ${logs}`).toBe(true)
	})

	test('Control+z is ours on the page and the browser’s inside an input', async ({ page }) => {
		await edit(page)
		await page.keyboard.press('Control+z')
		expect(await lastPrevented(page), 'undo on the page').toBe(true)
		await page.locator('.tl-render-overlay input').first().click()
		await page.keyboard.press('Control+z')
		expect(await lastPrevented(page), 'text undo stays native').toBe(false)
	})

	test('the hint reveals after its delay and closes on a leaf', async ({ page }) => {
		// A long delay, so the hidden phase outlasts any round-trip between keypress and assertion.
		await page.evaluate(() => document.documentElement.style.setProperty('--tl-hint-delay', '1500ms'))
		const hint = page.locator('.tl-hint')
		await page.keyboard.press('Alt+z')
		await expect(hint).toHaveCount(1)
		await expect(hint, 'a fast chord never flashes the hint').toBeHidden()
		await expect(hint).toBeVisible({ timeout: 5_000 })
		await expect(hint).toContainText('Undo')
		await page.keyboard.press('Escape')
		await expect(hint).toHaveCount(0)
	})

	test('a tile’s ? opens the help naming it', async ({ page }) => {
		const tile = page.locator('.tl-tile:not(.tl-shadow)').nth(1)
		const title = await tile.locator('.tl-tab.tl-active').innerText()
		await tile.locator('.tl-action[title="Keybinds"]').click()
		await expect(page.locator('.tl-help')).toBeVisible()
		await expect(page.locator('.tl-help-pane')).toHaveText(title)
		await expect(page.locator('.tl-help')).toContainText('Alt+z s')
	})
})

test.describe('macOS', () => {
	test.beforeEach(async ({ page }) => {
		await page.addInitScript(() => Object.defineProperty(Navigator.prototype, 'platform', { get: () => 'MacIntel' }))
		await boot(page)
	})

	test('Control+t is the leader and Alt+z is not', async ({ page }) => {
		await page.keyboard.press('Alt+z')
		expect(await lastPrevented(page), 'Alt+z composes on macOS, so it is not bound').toBe(false)
		await expect(page.locator('.tl-hint')).toHaveCount(0)
		await page.keyboard.press('Control+t')
		expect(await lastPrevented(page)).toBe(true)
		await expect(page.locator('.tl-hint')).toHaveCount(1)
	})
})
