import { readFileSync } from 'node:fs'

import { expect, test } from '@playwright/test'

// The Grafana plugin over `nix run .#dev -- grafana`'s testdata fixture, a twin of devops'
// `ev-deployments`. Every testdata query carries `alias: q-<panel id>`, which is how a request is
// attributed to its panel.

const DOCK = 'a/evmain-tilers-app/d/tl-fixture'
const PANELS = 9
const FIXTURE = JSON.parse(readFileSync(new URL('../tilers_grafana/plugin/provisioning/fixtures/tl-fixture.json', import.meta.url)))
/** Panel ids by their `dock` key. */
const AUTHORED = FIXTURE.panels.filter((p) => p.dock?.minHRem).map((p) => [p.id, p.dock.minHRem])
const COMPILED = FIXTURE.panels.filter((p) => p.dock?.precompile).map((p) => p.id)
const compiled = (page, id) =>
	page.waitForFunction((k) => JSON.parse(localStorage.getItem(k) ?? 'null'), `dv-grafana-min-tl-fixture-panel-${id}`, { timeout: 30_000 }).then((h) => h.jsonValue())

const boot = async (page, path = DOCK) => {
	await page.goto(path)
	await page.waitForFunction((n) => document.querySelectorAll('.tl-grafana-slot').length === n, PANELS, { timeout: 30_000 })
	// variables resolve, so titles carry `$deployment`'s value
	await expect(page.locator('.tl-tab').first()).toHaveText(/· AA$/)
}

const tab = (page, title) => page.locator('.tl-tab', { hasText: new RegExp(`^${title} ·`) })
/** The overlay box holding panel `id`'s slot. */
const overlay = (page, id) => page.locator(`.tl-render-overlay:has(> .tl-grafana-slot[data-panel="panel-${id}"])`)
const geometry = (page) => page.$$eval('.tl-tile', (els) => els.map((e) => e.getAttribute('style')))

/** Drag `from`'s tab onto `onto`'s header, joining them as tabs. */
const join = async (page, from, onto) => {
	const src = await tab(page, from).boundingBox()
	const dst = await tab(page, onto).boundingBox()
	await page.mouse.move(src.x + src.width / 2, src.y + src.height / 2)
	await page.mouse.down()
	await page.mouse.move(dst.x + dst.width + 30, dst.y + dst.height / 2, { steps: 12 })
	await page.mouse.up()
}

test.beforeEach(async ({ page }) => await boot(page))

test('every panel renders natively inside a tile', async ({ page }) => {
	expect(await page.locator('.tl-tile').count()).toBe(PANELS)
	await Promise.all(COMPILED.map((id) => compiled(page, id))) // a compile probes its slot past the tile
	const inside = await page.$$eval('.tl-grafana-slot', (slots) => {
		const tiles = [...document.querySelectorAll('.tl-tile')].map((t) => t.getBoundingClientRect())
		return slots.map((s) => {
			const r = s.getBoundingClientRect()
			return tiles.some((t) => r.left >= t.left - 1 && r.right <= t.right + 1 && r.top >= t.top - 1 && r.bottom <= t.bottom + 1)
		})
	})
	expect(inside, 'each slot sits within some tile').toEqual(Array(PANELS).fill(true))
	await expect(overlay(page, 11).locator('[data-testid="data-testid panel content"]')).toBeVisible()
})

test('a tab-join switches panels, and the inactive tab stays quiet on refresh', async ({ page }) => {
	await join(page, 'Flux', 'Switch')
	const header = page.locator('.tl-header', { has: tab(page, 'Switch') })
	await expect(header.locator('.tl-tab')).toHaveCount(2)
	await expect(overlay(page, 9)).toBeVisible()
	await expect(overlay(page, 2)).toBeHidden()

	await tab(page, 'Switch').click()
	await expect(overlay(page, 2)).toBeVisible()
	await expect(overlay(page, 9)).toBeHidden()

	const asked = []
	page.on('request', (r) => r.url().includes('/api/ds/query') && asked.push(r.postData() ?? ''))
	await page.getByRole('button', { name: 'Refresh', exact: true }).click()
	await page.waitForLoadState('networkidle')
	expect(asked.some((b) => b.includes('"q-2"')), 'the active tab refreshes').toBe(true)
	expect(asked.some((b) => b.includes('"q-9"')), 'the inactive tab must not query').toBe(false)
})

test('a closed panel reopens from a tile’s +', async ({ page }) => {
	const tile = (title) => page.locator('.tl-tile', { has: tab(page, title) })
	await tile('Flux').locator('button[title="Close"]').click()
	await expect(page.locator('.tl-tile')).toHaveCount(PANELS - 1)
	await tile('Switch').locator('button[title="Add window as a tab"]').click()
	await expect(page.getByRole('menuitem')).toHaveText(['Flux · AA'])
	await page.getByRole('menuitem', { name: 'Flux · AA' }).click()
	await expect(tile('Switch').locator('.tl-tab')).toHaveText(['Switch · AA', 'Flux · AA'])
	await expect(overlay(page, 9).locator('[data-testid="data-testid panel content"]')).toBeVisible()
})

test('a container-only resize re-measures', async ({ page }) => {
	const before = await page.locator('.tl-tile').first().boundingBox()
	await page.evaluate(() => (document.querySelector('.tl-packed').parentElement.style.right = '400px'))
	await expect.poll(async () => (await page.locator('.tl-tile').first().boundingBox()).width).toBeLessThan(before.width - 50)
})

test('leader s then a reload restores the layout', async ({ page }) => {
	await join(page, 'Flux', 'Switch')
	const edited = await geometry(page)
	for (const k of ['Alt+z', 's']) await page.keyboard.press(k)
	await page.reload()
	await boot(page, page.url())
	expect(await geometry(page)).toEqual(edited)
})

test('? is Grafana’s shortcuts modal; leader ? is ours', async ({ page }) => {
	await page.keyboard.press('Shift+Slash')
	await expect(page.getByRole('dialog')).toBeVisible()
	await expect(page.locator('.tl-help')).toHaveCount(0)
	await page.keyboard.press('Escape')
	await page.keyboard.press('Alt+z')
	await page.keyboard.press('Shift+Slash')
	await expect(page.locator('.tl-help')).toBeVisible()
})

test('Grafana’s own keys pass through the dock', async ({ page }) => {
	// Registered after the dock's capture-phase listener and before any of Grafana's document ones,
	// so it reads what *we* did to the event.
	await page.evaluate(() => {
		globalThis.__tlProbe = []
		window.addEventListener('keydown', (e) => globalThis.__tlProbe.push(e.defaultPrevented), true)
	})
	// An edit first, so an unclaimed Ctrl+Z is a choice rather than an empty history.
	await join(page, 'Flux', 'Switch')
	for (const k of ['Control+z', 'd']) {
		await page.keyboard.press(k)
		expect(await page.evaluate(() => globalThis.__tlProbe.at(-1)), `${k} is Grafana’s`).toBe(false)
		await page.keyboard.press('Escape')
	}
})

test('an idle dock does no work beyond a native dashboard', async ({ page }) => {
	await page.waitForLoadState('networkidle')
	const idle = () =>
		page.evaluate(
			() =>
				new Promise((done) => {
					let mutations = 0
					let longTasks = 0
					const root = document.querySelector('.tl-packed')
					const mo = root
						? new MutationObserver((ms) => (mutations += ms.filter((m) => !m.target.closest?.('.tl-grafana-slot') && !m.target.parentElement?.closest('.tl-grafana-slot')).length))
						: undefined
					mo?.observe(root, { subtree: true, childList: true, attributes: true, characterData: true })
					const po = new PerformanceObserver((l) => (longTasks += l.getEntries().length))
					po.observe({ type: 'longtask' })
					setTimeout(() => {
						mo?.disconnect()
						po.disconnect()
						done({ mutations, longTasks })
					}, 5000)
				})
		)
	const dock = await idle()
	expect(dock.mutations, 'the dock chrome is static while nothing moves').toBe(0)

	await page.goto('d/tl-fixture')
	await page.waitForLoadState('networkidle')
	const native = await idle()
	expect(dock.longTasks).toBeLessThanOrEqual(native.longTasks + 1)
})

test('declared and compiled mins hold when the window halves', async ({ page }) => {
	expect(AUTHORED.length && COMPILED.length, 'the fixture declares both kinds').toBeTruthy()
	const sizes = await Promise.all(COMPILED.map((id) => compiled(page, id)))
	/** Px each clipping box in panel `id`'s slot clips, kept per element; `more` = boxes clipping past the first call's. */
	const clip = (id) =>
		page.$eval(`.tl-grafana-slot[data-panel="panel-${id}"]`, (s) => {
			const now = new Map()
			for (const e of [s, ...s.querySelectorAll('*')]) {
				const c = getComputedStyle(e)
				const px = Math.max(e.scrollHeight - e.clientHeight, e.scrollWidth - e.clientWidth) - 1
				if (px > 0 && (e === s || c.overflowX !== 'visible' || c.overflowY !== 'visible')) now.set(e, px)
			}
			window.dvClip ??= new Map()
			const base = window.dvClip.get(s) ?? (window.dvClip.set(s, now), now)
			return [...now].filter(([e, px]) => px > (base.get(e) ?? 0)).map(([e, px]) => `${e.className} +${px}`)
		})
	for (const id of COMPILED) {
		await page.waitForTimeout(500) // the compiled floor's reflow lands
		expect(await clip(id), `panel ${id} clips nothing at full size beyond its own widgets`).toEqual([])
	}
	await page.setViewportSize({ width: 1600, height: 500 })
	const slotH = (id) => page.$eval(`.tl-grafana-slot[data-panel="panel-${id}"]`, (s) => s.clientHeight)
	const rem = await page.evaluate(() => parseFloat(getComputedStyle(document.documentElement).fontSize))
	for (const [id, minHRem] of AUTHORED) {
		await expect.poll(() => slotH(id), `panel ${id} keeps its ${minHRem}rem`).toBeGreaterThanOrEqual(minHRem * rem)
	}
	for (const [i, id] of COMPILED.entries()) {
		await expect.poll(() => slotH(id), `panel ${id} keeps its compiled ${sizes[i].h}px`).toBeGreaterThanOrEqual(sizes[i].h)
		expect(await clip(id), `panel ${id} clips no more at half height`).toEqual([])
	}
})

test('a second load takes compiled sizes from the cache, without flexing', async ({ page }) => {
	const first = await Promise.all(COMPILED.map((id) => compiled(page, id)))
	await page.addInitScript(() => {
		window.tlFlexed = 0
		new MutationObserver((ms) => (window.tlFlexed += ms.filter((m) => m.target.matches?.('.tl-grafana-slot')).length)).observe(document, {
			subtree: true,
			attributes: true,
			attributeFilter: ['style'],
		})
	})
	await page.reload()
	await boot(page, page.url())
	await page.waitForLoadState('networkidle')
	await page.waitForTimeout(2000)
	expect(await page.evaluate(() => window.tlFlexed), 'no slot was probed').toBe(0)
	expect(await Promise.all(COMPILED.map((id) => compiled(page, id)))).toEqual(first)
})

test('an unsupported dashboard names what it refuses', async ({ page }) => {
	await page.goto('a/evmain-tilers-app/d/tl-fixture-repeat')
	await expect(page.getByText('panel 1 "Memory · $deployment" uses repeat')).toBeVisible()
})
