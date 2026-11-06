/**
 * ESLint für die Projektseiten (Etappe 4b): `projects/<slug>/**` (oder ein
 * Projekt aus einer anderen Inhaltsquelle). Rust ruft es je Projektordner mit
 * `--cache` auf (src/werkzeug/eslint.rs, für `dashboard pruefe <slug>` und
 * den Baumeister); von Hand aus dem Ordner über dem Projekt:
 *
 *   node <web>/node_modules/eslint/bin/eslint.js --config <web>/eslint.projekte.config.js <slug>
 *
 * Ohne Typprüfung (die macht svelte-check), darum schnell und ohne
 * tsconfig im Projekt. Die Regeln des Zielbilds („Prüfung je Projekt“):
 * kein fetch, EventSource, WebSocket, localStorage, sessionStorage,
 * indexedDB, document.cookie, kein {@html}; Bausteine nur über $katalog
 * (die Grenze zum Toolkit wie in eslint.config.js). Die meisten Regeln prüft
 * auch der Scanner in Rust; ESLint fängt dazu, was ein Compiler sieht
 * (svelte/valid-compile, unbenutzte Variablen, Fehler im Ablauf).
 */
import js from '@eslint/js';
import prettier from 'eslint-config-prettier';
import svelte from 'eslint-plugin-svelte';
import { defineConfig } from 'eslint/config';
import globals from 'globals';
import ts from 'typescript-eslint';
import { grenze } from './eslint.config.js';

const DATEN = 'Daten kommen nur über `ladeDaten` aus $projekt.';
const SPEICHER = 'Was bleiben soll, gehört in daten/*.toml oder über die API in den Datenspeicher.';

export default defineConfig(
	// Rechenteile (Rust) und fremde Ordner prüft ESLint nicht.
	{
		ignores: ['**/server/**', '**/node_modules/**', '**/target/**', '**/assets/**', '**/daten/**']
	},
	js.configs.recommended,
	ts.configs.recommended,
	svelte.configs.recommended,
	prettier,
	svelte.configs.prettier,
	{
		languageOptions: { globals: { ...globals.browser } },
		rules: {
			'no-undef': 'off',
			'no-restricted-globals': [
				'error',
				{ name: 'fetch', message: DATEN },
				{ name: 'EventSource', message: DATEN },
				{ name: 'WebSocket', message: DATEN },
				{ name: 'localStorage', message: SPEICHER },
				{ name: 'sessionStorage', message: SPEICHER },
				{ name: 'indexedDB', message: SPEICHER }
			],
			'no-restricted-properties': [
				'error',
				{ object: 'window', property: 'fetch', message: DATEN },
				{ object: 'window', property: 'localStorage', message: SPEICHER },
				{ object: 'window', property: 'sessionStorage', message: SPEICHER },
				{ object: 'window', property: 'indexedDB', message: SPEICHER },
				{
					object: 'document',
					property: 'cookie',
					message: 'Die Anmeldung gehört dem Dashboard, nicht der Seite.'
				}
			],
			'svelte/no-at-html-tags': 'error'
		}
	},
	{
		files: ['**/*.svelte', '**/*.svelte.ts'],
		languageOptions: {
			parserOptions: { parser: ts.parser, extraFileExtensions: ['.svelte'] }
		}
	},
	{ files: ['**/*.ts', '**/*.svelte'], rules: grenze.rules }
);
