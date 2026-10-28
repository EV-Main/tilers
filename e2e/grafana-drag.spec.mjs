import { expect, test } from '@playwright/test'

// A native (HTML5) drag held over an inactive tab opens it, so that panel's content can take the drop.
// `tl-fixture-tabs` seeds [Alpha] | [Beta, Gamma] | [Delta]; the drag source and the drop target are test-injected
// into Alpha's and Gamma's slots, as a dynamictext panel's own elements would be.

const DOCK = '/a/evmain-tilers-app/d/tl-fixture-tabs'

test('a native drag over an inactive tab opens it, and its content takes the drop', async ({ page }) => {
	await page.goto(DOCK)
	await expect(page.locator('.tl-grafana-slot')).toHaveCount(4, { timeout: 30_000 })
	await page.evaluate(() => {
		const slot = (id) => document.querySelector(`.tl-grafana-slot[data-panel="panel-${id}"]`)
		const src = Object.assign(document.createElement('div'), { id: 'src', draggable: true, textContent: 'card' })
		src.style.cssText = 'position:absolute;inset:8px auto auto 8px;width:80px;height:40px;background:#888;z-index:10'
		src.addEventListener('dragstart', (e) => e.dataTransfer.setData('text/plain', 'card'))
		slot(1).append(src)
		const dst = Object.assign(document.createElement('div'), { id: 'dst' })
		dst.style.cssText = 'position:absolute;inset:0;z-index:10'
		dst.addEventListener('dragover', (e) => e.preventDefault())
		dst.addEventListener('drop', (e) => {
			e.preventDefault()
			dst.dataset.dropped = e.dataTransfer.getData('text/plain')
		})
		slot(3).append(dst)
	})
	const gamma = page.locator('.tl-tab', { hasText: /^Gamma$/ })
	const from = await page.locator('#src').boundingBox()
	const over = await gamma.boundingBox()
	await page.mouse.move(from.x + 20, from.y + 20)
	await page.mouse.down()
	await page.mouse.move(over.x + over.width / 2, over.y + over.height / 2, { steps: 12 })
	await expect(page.locator('.tl-render-overlay:has(> .tl-grafana-slot[data-panel="panel-3"])')).toBeVisible()
	const body = await page.locator('#dst').boundingBox()
	await page.mouse.move(body.x + body.width / 2, body.y + body.height / 2, { steps: 8 })
	await page.mouse.up()
	await expect(page.locator('#dst')).toHaveAttribute('data-dropped', 'card')
})
