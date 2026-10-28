import { defineConfig, devices } from '@playwright/test'

// On NixOS the browser binaries are nix-patched, so Playwright's glibc/ldd preflight would
// wrongly fail; the nix store path is already correct. Harmless elsewhere.
process.env.PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS ??= '1'

// Two browsers, one spec: the keybind listener regression must hold in both engines (it was the
// Firefox-only break that motivated this test). Point at a running insilico dev server:
//   dx serve --example insilico --package tilers_dioxus --platform web --port 8111
// Override with TILERS_URL to run against any host that mounts PackedArea (e.g. the REA MFE).
export default defineConfig({
	testDir: '.',
	use: { baseURL: process.env.TILERS_URL ?? 'http://127.0.0.1:8111/' },
	projects: [
		{ name: 'chromium', testIgnore: 'grafana*.spec.mjs', use: { ...devices['Desktop Chrome'] } },
		{ name: 'firefox', testIgnore: 'grafana*.spec.mjs', use: { ...devices['Desktop Firefox'] } },
		// `nix run .#dev -- grafana`
		{
			name: 'grafana',
			testMatch: 'grafana*.spec.mjs',
			use: { ...devices['Desktop Chrome'], viewport: { width: 1600, height: 1000 }, baseURL: process.env.GRAFANA_URL ?? 'http://127.0.0.1:54581/' },
		},
	],
})
