import { svelteTesting } from '@testing-library/svelte/vite';
import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vitest/config';
import { grundbauFassung } from './scripts/grundbau.mjs';

/** Der Rust-Server, der Daten liefert und im Betrieb das Build ausliefert. */
const backend = 'http://127.0.0.1:8787';

/**
 * Etappe 4b: weitere Inhaltsquellen (DASHBOARD_PROJECTS_DIR, getrennt durch ;).
 * Die Bauliste nennt Projekte mit absolutem Pfad; vite darf sie lesen. Ohne die
 * Variable (der Fall heute) bleibt es bei ../projects.
 */
const quellen = (process.env.DASHBOARD_PROJECTS_DIR ?? '')
	.split(';')
	.map((q) => q.trim())
	.filter((q) => q !== '');

/** Ein Ordner unter web/ als absoluter Pfad (für die Aliase unten). */
const ordner = (p: string): string => fileURLToPath(new URL(p, import.meta.url));

export default defineConfig({
	plugins: [
		sveltekit({
			compilerOptions: {
				// Runes-Modus für das ganze Projekt (außer node_modules).
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},
			// Statisches Build nach web/build; der Rust-Server liefert es unter / aus.
			// fallback: Einzelseiten-App, damit /projects/<slug> ohne
			// vorgerenderte Liste funktioniert (die Seiten kommen zur Laufzeit).
			// Ein Bau über den Boten setzt DASHBOARD_BUILD_ZIEL auf web/builds/<ordner> (Etappe 4v).
			adapter: adapter({
				pages: process.env.DASHBOARD_BUILD_ZIEL ?? 'build',
				assets: process.env.DASHBOARD_BUILD_ZIEL ?? 'build',
				fallback: 'index.html',
				strict: true
			}),
			paths: { base: '' },
			// Grundbau und Bauzeit; der Dienst nimmt einen Bau des Boten nur bei
			// gleichem Grundbau wie die eingespielte Oberfläche an (scripts/grundbau.mjs).
			version: { name: grundbauFassung(ordner('./src')) }
		}),
		svelteTesting()
	],
	// Der Vertrag mit dem Rust-Server: von `cargo test` erzeugte Typen
	// (src/vertrag/export.rs). Import: `import type { X } from '$vertrag';`.
	// Etappe 4v: Projektseiten (projects/<slug>/Seite.svelte) importieren nur
	// aus $katalog (Bausteine), $projekt (ladeDaten) und $vertrag.
	// Subpath-Imports (#…) erlauben kein `$`. Etappe 3b-2: nicht mehr über
	// `alias` von Kit (veraltet, und dessen Prüfung der tsconfig meldete unter
	// Windows fälschlich tsconfig_invalid), sondern hier für Vite und als
	// `paths` in tsconfig.json für TypeScript; beide Listen gleich halten.
	resolve: {
		alias: {
			$vertrag: ordner('./src/lib/vertrag'),
			$katalog: ordner('./src/lib/katalog'),
			$projekt: ordner('./src/lib/projekt')
		}
	},
	server: {
		// Projektseiten liegen außerhalb von web/ (projects/<slug>/*.svelte);
		// eingebunden nur über web/src/lib/projekte.generated.ts. Etappe 4b: dazu die
		// letzten grünen Kopien (.projekte-gruen) und weitere Inhaltsquellen.
		fs: { allow: ['../projects', '.projekte-gruen', ...quellen] },
		// Entwicklung: vite dev auf 5173, Daten vom Rust-Server.
		proxy: {
			'/api': backend,
			'/schluessel': backend,
			'/projects': backend
		}
	},
	test: {
		expect: { requireAssertions: true },
		projects: [
			{
				extends: './vite.config.ts',
				test: {
					name: 'komponenten',
					environment: 'jsdom',
					include: ['src/**/*.svelte.{test,spec}.{js,ts}'],
					setupFiles: ['./vitest-setup.ts']
				}
			},
			{
				extends: './vite.config.ts',
				test: {
					name: 'module',
					environment: 'node',
					include: ['src/**/*.{test,spec}.{js,ts}'],
					exclude: ['src/**/*.svelte.{test,spec}.{js,ts}']
				}
			}
		]
	}
});
