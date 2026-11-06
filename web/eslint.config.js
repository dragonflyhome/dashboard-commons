import prettier from 'eslint-config-prettier';
import path from 'node:path';
import js from '@eslint/js';
import svelte from 'eslint-plugin-svelte';
import { defineConfig, includeIgnoreFile } from 'eslint/config';
import globals from 'globals';
import ts from 'typescript-eslint';

const gitignorePath = path.resolve(import.meta.dirname, '.gitignore');

/**
 * Das Toolkit in web/vendor (Bits UI und alle Helfer, auch die indirekten).
 * Ein neues Vendor-Paket kommt hier dazu; src/grenze.spec.ts hält die Liste
 * gleich mit den Ordnern in web/vendor. Die Pakete unter @floating-ui/ sperrt
 * das Muster als ganzen Scope, sie stehen darum nicht einzeln hier.
 */
export const TOOLKIT = [
	'bits-ui',
	'runed',
	'svelte-toolbelt',
	'tabbable',
	'@internationalized/date',
	'@swc/helpers',
	'clsx',
	'dequal',
	'esm-env',
	'lz-string',
	'style-to-object',
	'inline-style-parser',
	'tslib'
];
const TOOLKIT_SATZ =
	'Das Toolkit (Bits UI) nur im Katalog einbinden; hier die Bausteine aus $katalog nehmen.';
/** Scopes, die als Ganzes zum Toolkit gehören. */
export const TOOLKIT_SCOPES = ['@floating-ui'];
const toolkitMuster = `^((${TOOLKIT.join('|')})(/|$)|${TOOLKIT_SCOPES.map((s) => `${s}/`).join('|')})`;
// esquery (Selektoren von no-restricted-syntax) erlaubt kein / im Regex;
// das Regex-Escape \u002F ist derselbe Schrägstrich.
const toolkitSelektor = toolkitMuster.replaceAll('/', '\\u002F');

/**
 * Schichten (Zielbild „Formsprache: vier Schichten“, Etappe 3a-3): Das
 * Toolkit (Bits UI und seine Helfer in web/vendor) bindet nur der Katalog
 * ein. Alle anderen nehmen die Bausteine aus $katalog; so lässt sich das
 * Toolkit tauschen, ohne Seiten anzufassen. Statische Importe sperrt
 * no-restricted-imports, dynamisches import('…') no-restricted-syntax.
 */
/** @type {import('eslint').Linter.Config} */
export const grenze = {
	files: ['**/*.ts', '**/*.js', '**/*.svelte'],
	ignores: ['src/lib/katalog/**'],
	rules: {
		'no-restricted-imports': [
			'error',
			{ patterns: [{ regex: toolkitMuster, message: TOOLKIT_SATZ }] }
		],
		'no-restricted-syntax': [
			'error',
			{
				selector: `ImportExpression[source.value=/${toolkitSelektor}/]`,
				message: TOOLKIT_SATZ
			},
			{
				selector: `ImportExpression[source.type="TemplateLiteral"][source.quasis.0.value.raw=/${toolkitSelektor}/]`,
				message: TOOLKIT_SATZ
			}
		]
	}
};

export default defineConfig(
	includeIgnoreFile(gitignorePath),
	// Erzeugt von `cargo test` (src/vertrag/export.rs), nicht von Hand ändern.
	{ ignores: ['src/lib/vertrag/**'] },
	// Erzeugt von `dashboard pruefe --bauliste` (Rust), nicht von Hand ändern.
	{ ignores: ['src/lib/projekte.generated.ts'] },
	// Vendor-Kopie des Toolkits (Etappe 3a-3): unverändert aus der Registry.
	{ ignores: ['vendor/**'] },
	js.configs.recommended,
	// Pedantisch: typisierte Regeln, kein any, keine schwebenden Promises.
	ts.configs.strictTypeChecked,
	ts.configs.stylisticTypeChecked,
	svelte.configs.recommended,
	prettier,
	svelte.configs.prettier,
	{
		languageOptions: {
			globals: { ...globals.browser, ...globals.node },
			parserOptions: { projectService: true, tsconfigRootDir: import.meta.dirname }
		},
		rules: {
			// typescript-eslint rät von no-undef in TS-Projekten ab.
			'no-undef': 'off',
			'@typescript-eslint/no-explicit-any': 'error',
			// Nur mit Begründung im Kommentar erlaubt (Anleitung, „erlaubte Abschaltungen“).
			'@typescript-eslint/no-non-null-assertion': 'error',
			'@typescript-eslint/consistent-type-imports': ['error', { prefer: 'type-imports' }],
			'@typescript-eslint/switch-exhaustiveness-check': 'error',
			'@typescript-eslint/restrict-template-expressions': ['error', { allowNumber: true }]
		}
	},
	{
		files: ['**/*.svelte', '**/*.svelte.ts', '**/*.svelte.js'],
		languageOptions: {
			parserOptions: {
				projectService: true,
				extraFileExtensions: ['.svelte'],
				parser: ts.parser
			}
		}
	},
	grenze,
	{
		files: ['*.config.js', '*.config.ts', 'vitest-setup.ts', 'scripts/*.mjs'],
		extends: [ts.configs.disableTypeChecked]
	}
);
