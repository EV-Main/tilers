import { expect, test } from '@playwright/test'

// The plugin framed by an embedding page (`?_tilers.host`) over `nix run .#dev -- grafana`, which
// allows embedding. The parent is a same-origin page served by `page.route`; it records what the
// frame posts and answers per `cfg`. `tl-fixture-tabs` seeds [Alpha] | [Beta, Gamma]: the last two
// share one `gridPos`.

const DOCK = '/a/evmain-tilers-app/d/tl-fixture-tabs'
const PARENT = `<body style="margin:0"><iframe id="f" style="width:1500px;height:900px;border:0"></iframe><script>
window.received = []
const f = document.getElementById('f')
addEventListener('message', (e) => {
	if (e.source !== f.contentWindow || !e.data?.tilers) return // Grafana posts its own 'GrafanaAppInit'
	received.push(e.data)
	const reply = e.data.tilers === 'ready' ? cfg.onReady : e.data.tilers === 'publish' ? cfg.onPublish : null
	if (reply) f.contentWindow.postMessage(reply, '*')
})
window.answer = (m) => f.contentWindow.postMessage(m, '*')
f.src = new URLSearchParams(location.search).get('src')
</script></body>`

const HOST = (published = {}, publish = false) => ({ tilers: 'host', published, publish })

/** Frame `src` in the parent page; `cfg.onReady` / `cfg.onPublish` are the messages it answers with. */
const frame = async (page, src, cfg = {}) => {
	await page.route('**/tilers-host-parent.html*', (route) => route.fulfill({ contentType: 'text/html', body: PARENT }))
	await page.addInitScript((c) => (window.cfg = c), cfg)
	await page.goto(`/tilers-host-parent.html?src=${encodeURIComponent(src)}`)
	return page.frameLocator('#f')
}
const received = (page) => page.evaluate(() => window.received)
const tiles = (f) => f.locator('.tl-tile')
const seeded = async (f) => {
	await expect(f.locator('.tl-grafana-slot')).toHaveCount(3, { timeout: 30_000 })
	await expect(tiles(f)).toHaveCount(2)
}
const tab = (f, title) => f.locator('.tl-tab', { hasText: new RegExp(`^${title}$`) })
const overlay = (f, id) => f.locator(`.tl-render-overlay:has(> .tl-grafana-slot[data-panel="panel-${id}"])`)
/** Keys go to the frame once something inside it has focus. */
const chord = async (page, f, ...keys) => {
	await f.locator('.tl-header').first().click()
	for (const k of keys) await page.keyboard.press(k)
}

/** A layout this plugin produced, distinguishable from the seed: Alpha joined into Beta's tile, cached under `xl`. */
const layoutOf = async (browser) => {
	const ctx = await browser.newContext({ baseURL: test.info().project.use.baseURL, viewport: { width: 1600, height: 1000 } })
	const page = await ctx.newPage()
	await page.goto(DOCK)
	await expect(page.locator('.tl-grafana-slot')).toHaveCount(3, { timeout: 30_000 })
	const src = await tab(page, 'Alpha').boundingBox()
	const dst = await tab(page, 'Gamma').boundingBox()
	await page.mouse.move(src.x + src.width / 2, src.y + src.height / 2)
	await page.mouse.down()
	await page.mouse.move(dst.x + dst.width + 30, dst.y + dst.height / 2, { steps: 12 })
	await page.mouse.up()
	await expect(page.locator('.tl-tile')).toHaveCount(1)
	await page.keyboard.press('Alt+z')
	await page.keyboard.press('s')
	const json = await page.evaluate(() => localStorage.getItem('dv-grafana-tl-fixture-tabs-xl'))
	await ctx.close()
	expect(json, 'leader s wrote the xl layout').not.toBe(null)
	return json
}

test('panels sharing a gridPos open as the tabs of one tile, the first active', async ({ page }) => {
	await page.goto(DOCK)
	await expect(page.locator('.tl-grafana-slot')).toHaveCount(3, { timeout: 30_000 })
	await expect(page.locator('.tl-tile')).toHaveCount(2)
	const shared = page.locator('.tl-header', { has: tab(page, 'Beta') })
	await expect(shared.locator('.tl-tab')).toHaveText(['Beta', 'Gamma'])
	await expect(overlay(page, 2)).toBeVisible()
	await expect(overlay(page, 3)).toBeHidden()
})

test('nothing mounts before the parent answers; an empty answer seeds', async ({ page }) => {
	const f = await frame(page, `${DOCK}?_tilers.host`)
	await expect(f.getByText('Waiting for the embedding page…')).toBeVisible({ timeout: 30_000 })
	await expect(tiles(f)).toHaveCount(0)
	expect(await received(page)).toEqual([{ tilers: 'ready' }])
	await page.evaluate((m) => window.answer(m), HOST())
	await seeded(f)
})

test('a fresh browser opens the published layout', async ({ page, browser }) => {
	const layout = await layoutOf(browser)
	const f = await frame(page, `${DOCK}?_tilers.host`, { onReady: HOST({ xl: layout }) })
	await expect(f.locator('.tl-grafana-slot')).toHaveCount(3, { timeout: 30_000 })
	await expect(tiles(f)).toHaveCount(1)
	await expect(f.locator('.tl-tab')).toHaveText(['Beta', 'Gamma', 'Alpha'])
})

test('the browser\'s own cache beats the published layout', async ({ page, browser }) => {
	const layout = await layoutOf(browser)
	const f = await frame(page, `${DOCK}?_tilers.host`, { onReady: HOST() })
	await seeded(f)
	await chord(page, f, 'Alt+z', 's')
	await expect(f.locator('.tl-notice')).toContainText('cached')
	await page.addInitScript((l) => (window.cfg = { onReady: { tilers: 'host', published: { xl: l }, publish: false } }), layout) // runs after frame()'s, so it wins on reload
	await page.reload()
	await seeded(f)
})

test('leader S posts the layout when publishing is allowed, and the parent\'s notify shows', async ({ page }) => {
	const text = "Published as everyone's default (xl)"
	const f = await frame(page, `${DOCK}?_tilers.host`, { onReady: HOST({}, true), onPublish: { tilers: 'notify', text } })
	await seeded(f)
	await chord(page, f, 'Alt+z', 'Shift+S')
	await expect(f.locator('.tl-notice')).toContainText(text)
	const [ready, publish] = await received(page)
	expect(ready).toEqual({ tilers: 'ready' })
	expect(publish).toMatchObject({ tilers: 'publish', band: 'xl' })
	expect(JSON.parse(publish.layout), 'the layout is the versioned payload').toHaveProperty('grid')
})

test('leader S is unbound when publishing is not allowed', async ({ page }) => {
	const f = await frame(page, `${DOCK}?_tilers.host`, { onReady: HOST({}, false) })
	await seeded(f)
	await chord(page, f, 'Alt+z', 'Shift+S')
	await page.keyboard.press('Alt+z')
	await page.keyboard.press('s') // the barrier: once its notice shows, an earlier publish would have posted
	await expect(f.locator('.tl-notice')).toContainText('cached')
	expect(await received(page)).toEqual([{ tilers: 'ready' }])
})

test('framed without the flag, the plugin posts nothing', async ({ page }) => {
	const f = await frame(page, DOCK)
	await seeded(f)
	expect(await received(page)).toEqual([])
})
