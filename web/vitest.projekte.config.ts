/**
 * Vitest für die eigenen Tests eines Projekts (Etappe 4b): `*.spec.ts` im
 * Projektordner, den die Umgebungsvariable DASHBOARD_VITEST_PROJEKT nennt
 * (absolut). Rust ruft es auf (src/werkzeug/projektpruefung.rs, für
 * `dashboard pruefe <slug>` und den Baumeister); von Hand:
 *
 *   $env:DASHBOARD_VITEST_PROJEKT = 'C:\…\projects\<slug>'; npx vitest run --config vitest.projekte.config.ts
 *
 * Gleiche Aliase und Umgebung wie die Komponenten-Tests in vite.config.ts
 * (jsdom, Testing Library), ohne SvelteKit.
 */
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { svelteTesting } from '@testing-library/svelte/vite';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';

const ordner = (p: string): string => fileURLToPath(new URL(p, import.meta.url));
const projekt = process.env.DASHBOARD_VITEST_PROJEKT ?? ordner('./.baumeister/kein-projekt');

export default defineConfig({
	plugins: [svelte({ compilerOptions: { runes: true } }), svelteTesting()],
	resolve: {
		alias: {
			$vertrag: ordner('./src/lib/vertrag'),
			$katalog: ordner('./src/lib/katalog'),
			$projekt: ordner('./src/lib/projekt')
		}
	},
	server: { fs: { allow: ['.', projekt] } },
	test: {
		dir: projekt,
		include: ['**/*.spec.ts'],
		exclude: ['**/node_modules/**', '**/server/**'],
		environment: 'jsdom',
		setupFiles: [ordner('./vitest-setup.ts')],
		expect: { requireAssertions: true }
	}
});
