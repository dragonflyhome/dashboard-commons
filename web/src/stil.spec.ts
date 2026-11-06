/**
 * Die Stil-Prüfung selbst (stylelint.config.js): lehnt ab, was der Plan
 * verbietet, und lässt Token durch. So fällt auf, wenn eine Regel beim
 * Aktualisieren von Stylelint still wegfällt.
 */
import stylelint from 'stylelint';
import { describe, expect, it } from 'vitest';
import config from '../stylelint.config.js';

async function funde(code: string, datei = 'probe.css'): Promise<string[]> {
	const { results } = await stylelint.lint({ code, codeFilename: datei, config });
	return results.flatMap((r) => r.warnings.map((w) => w.rule));
}

describe('Stil-Prüfung', () => {
	it.each([
		['Hex', 'a { color: #fff; }', 'color-no-hex'],
		['rgb()', 'a { background: rgb(0 0 0); }', 'function-disallowed-list'],
		['hsl() im Schatten', 'a { box-shadow: 0 0 2px hsl(0 0% 0%); }', 'function-disallowed-list'],
		['Farbname', 'a { border: 1px solid red; }', 'color-named'],
		['RGB() groß geschrieben', 'a { background: RGB(0 0 0); }', 'function-disallowed-list'],
		['color()', 'a { color: color(srgb 1 0 0); }', 'function-disallowed-list'],
		['device-cmyk()', 'a { color: device-cmyk(0 0 0 1); }', 'function-disallowed-list'],
		[
			'initial statt Token (im dunklen Thema schwarz)',
			'a { color: initial; }',
			'scale-unlimited/declaration-strict-value'
		],
		['unset statt Token', 'a { border-color: unset; }', 'scale-unlimited/declaration-strict-value'],
		['none als Textfarbe', 'a { color: none; }', 'scale-unlimited/declaration-strict-value'],
		[
			'Kurzform font mit eigener Schrift',
			'a { font: 14px/1.2 Arial; }',
			'scale-unlimited/declaration-strict-value'
		],
		['Schriftgröße', 'a { font-size: 14px; }', 'scale-unlimited/declaration-strict-value'],
		[
			'Schriftgröße mit clamp',
			'a { font-size: clamp(1rem, 2vw, 2rem); }',
			'scale-unlimited/declaration-strict-value'
		],
		[
			'Schriftart',
			"a { font-family: 'Comic Sans MS'; }",
			'scale-unlimited/declaration-strict-value'
		],
		[
			'Schatten ohne Token',
			'a { box-shadow: 0 1px 2px var(--line); }',
			'scale-unlimited/declaration-strict-value'
		],
		['@import', "@import 'x.css';", 'at-rule-disallowed-list'],
		[
			'url(http…)',
			'a { background-image: url(https://example.org/x.png); }',
			'function-url-scheme-disallowed-list'
		],
		[
			'url(//…)',
			'a { background-image: url(//example.org/x.png); }',
			'function-url-no-scheme-relative'
		],
		[':global', ':global(a) { margin: 0; }', 'selector-pseudo-class-disallowed-list'],
		['!important', 'a { margin: 0 !important; }', 'declaration-no-important'],
		[
			'Abschalten ohne Begründung',
			'/* stylelint-disable-next-line color-no-hex */\na { color: #fff; }',
			'--report-descriptionless-disables'
		],
		// Auflagen aus 3a-2: jede Regel der Konfiguration hat einen Fall.
		[
			'!important in @keyframes',
			'@keyframes k { from { opacity: 0 !important; } }',
			'keyframe-declaration-no-important'
		],
		[
			'unnötiges Abschalten (die Zeile hat keinen Fund)',
			'/* stylelint-disable-next-line color-no-hex -- Beispiel mit Grund */\na { color: var(--text); }',
			'--report-needless-disables'
		],
		['fill mit Farbname', 'path { fill: red; }', 'scale-unlimited/declaration-strict-value'],
		['fill mit Hex', 'path { fill: #123456; }', 'scale-unlimited/declaration-strict-value'],
		['stroke mit Hex', 'path { stroke: #000; }', 'scale-unlimited/declaration-strict-value'],
		['stroke mit Farbname', 'path { stroke: white; }', 'color-named'],
		['fill mit initial', 'path { fill: initial; }', 'scale-unlimited/declaration-strict-value']
	])('lehnt ab: %s', async (_name, code, regel) => {
		expect(await funde(code)).toContain(regel);
	});

	it('lässt Token, color-mix mit Token und erlaubte Wörter durch', async () => {
		expect(
			await funde(`a {
	color: var(--text);
	background: color-mix(in srgb, var(--ok) 10%, transparent);
	border: 1px solid var(--line);
	border-left: 4px solid currentColor;
	border-right-color: CURRENTCOLOR;
	caret-color: Transparent;
	outline: 3px solid var(--fokus);
	box-shadow: var(--shadow);
	font: inherit;
	font-family: var(--mono);
	font-size: var(--text-sm);
	fill: none;
	stroke: inherit;
}
b {
	box-shadow: none;
}`)
		).toEqual([]);
	});

	it('prüft auch <style> in Svelte und erlaubt Abschalten mit Begründung', async () => {
		expect(await funde('<p>x</p>\n<style>\n\tp { color: #000; }\n</style>', 'x.svelte')).toContain(
			'color-no-hex'
		);
		expect(
			await funde(
				'/* stylelint-disable-next-line color-no-hex, scale-unlimited/declaration-strict-value -- Beispiel mit Grund */\na { color: #fff; }'
			)
		).toEqual([]);
	});
});
