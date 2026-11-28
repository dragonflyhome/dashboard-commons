/**
 * Die beiden Namensregeln (src/stilregeln.js, Ticket 20, Teil 1), je Fall ein
 * kleines Stück CSS: Ein Name, den es nicht gibt, wird abgewiesen, auch im
 * Rückfallwert und in color-mix; ein alter Name ist ein Hinweis mit dem neuen
 * Namen. Dazu die Liste selbst (src/lib/rollen.js) gegen app.css.
 */
import { readFileSync } from 'node:fs';
import stylelint from 'stylelint';
import { describe, expect, it } from 'vitest';
import config from '../stylelint.config.js';
import {
	ALTE_NAMEN,
	FARBROLLEN,
	FARBROLLEN_JE_SCHEMA,
	UEBRIGE_NAMEN,
	VORSILBEN
} from './lib/rollen.js';
import { namenIn } from './stilregeln.js';

const BEKANNT = 'dashboard/bekannte-namen';
const ALT = 'dashboard/alte-namen';
const FORM = '--extern-<name>: <wert>; /* extern: <Herkunft> */';

interface Fund {
	regel: string;
	stufe: string;
	text: string;
}

async function funde(code: string, datei = 'probe.css', einstellung = config): Promise<Fund[]> {
	const { results } = await stylelint.lint({ code, codeFilename: datei, config: einstellung });
	return results.flatMap((r) =>
		r.warnings.map((w) => ({ regel: w.rule, stufe: w.severity, text: w.text }))
	);
}

/** Nur die Funde der beiden Namensregeln. */
async function namensfunde(code: string, datei = 'probe.css'): Promise<Fund[]> {
	return (await funde(code, datei)).filter((f) => f.regel === BEKANNT || f.regel === ALT);
}

describe('Namen in einem Wert', () => {
	it('findet jeden Namen, auch verschachtelt', () => {
		expect(namenIn('1px solid var(--linie)')).toEqual(['--linie']);
		expect(namenIn('var(--a, var(--b, VAR(--c)))')).toEqual(['--a', '--b', '--c']);
		expect(namenIn('color-mix(in srgb, var(--ton) 10%, var( --flaeche ))')).toEqual([
			'--ton',
			'--flaeche'
		]);
		expect(namenIn('calc(1rem + 2px)')).toEqual([]);
	});
});

describe('Regel dashboard/bekannte-namen', () => {
	it.each([
		['unbekannter Name', 'a { color: var(--gibt-es-nicht); }', '--gibt-es-nicht'],
		['Tippfehler bei einer Schriftgröße', 'a { color: var(--text-farbe); }', '--text-farbe'],
		['im Rückfallwert', 'a { color: var(--vordergrund, var(--danger)); }', '--danger'],
		[
			'in color-mix',
			'a { background: color-mix(in srgb, var(--falsch) 10%, transparent); }',
			'--falsch'
		],
		['in einer Kurzform', 'a { border: 1px solid var(--rand); }', '--rand'],
		['eigener Name aus einer anderen Datei', 'a { fill: var(--zeichen); }', '--zeichen'],
		[
			'eigener Name, dessen Wert einen unbekannten Namen nennt',
			'a { --zeichen: var(--falsch); fill: var(--zeichen); }',
			'--falsch'
		]
	])('weist ab: %s', async (_name, code, name) => {
		const f = await namensfunde(code);
		expect(f).toHaveLength(1);
		expect(f[0]?.regel).toBe(BEKANNT);
		expect(f[0]?.stufe).toBe('error');
		expect(f[0]?.text.startsWith('Stil: ')).toBe(true);
		expect(f[0]?.text).toContain(`„${name}“`);
	});

	it.each([
		['--b-ok', 'Klasse b-ok mit --ton.'],
		['--b-ok-soft', 'Klasse b-ok mit --ton-soft.'],
		['--b-wartet-text', 'Klasse b-wartet mit --ton-text.'],
		['--b-wartet-auf', 'Klasse b-wartet mit --ton-auf.']
	])('weist einen Ton der Bedeutungen ab und nennt den Ersatz: %s', async (name, ersatz) => {
		const f = await namensfunde(`a { color: var(${name}); }`);
		expect(f.map((x) => x.regel)).toEqual([BEKANNT]);
		expect(f[0]?.text).toContain(ersatz);
	});

	it('lässt jeden Namen der Liste und die Vorsilben durch', async () => {
		const zeilen = [...FARBROLLEN, ...UEBRIGE_NAMEN, '--reihe-3', '--bits-select-anchor-width']
			.map((n, i) => `.k${String(i)} { margin: var(${n}); }`)
			.join('\n');
		expect(await namensfunde(zeilen)).toEqual([]);
	});

	it('lässt eigene Namen durch, die in derselben Datei deklariert sind', async () => {
		expect(
			await funde(`.zeichen {
	--zeichen: var(--reihe-3);
	--zeichen-leise: color-mix(in srgb, var(--zeichen) 40%, var(--flaeche));
	fill: var(--zeichen);
	stroke: var(--zeichen-leise, var(--linie));
}`)
		).toEqual([]);
	});

	it('kennt eigene Namen aus dem Markup einer Svelte-Datei', async () => {
		const svelte = (stil: string): string =>
			`<table style:--tabelle-min="{breite}rem" style="--rand: 2px"></table>\n<style>\n${stil}\n</style>`;
		expect(
			await funde(
				svelte('table { min-width: var(--tabelle-min, 0); padding: var(--rand); }'),
				'x.svelte'
			)
		).toEqual([]);
		expect(
			(await namensfunde(svelte('table { min-width: var(--tabelle-max); }'), 'x.svelte')).map(
				(f) => f.regel
			)
		).toEqual([BEKANNT]);
	});

	it.each([
		['--b-ok', 'Klasse b-ok mit --ton.'],
		['--b-wartet-soft', 'Klasse b-wartet mit --ton-soft.'],
		['--ok', 'Klasse b-ok mit --ton.'],
		['--linie', 'einen eigenen Namen'],
		['--bg', 'einen eigenen Namen'],
		['--reihe-3', 'einen eigenen Namen']
	])('weist ab: eine Datei setzt %s neu', async (name, ersatz) => {
		const f = await namensfunde(`.karte { ${name}: var(--reihe-6); }`);
		expect(f.map((x) => x.regel)).toEqual([BEKANNT]);
		expect(f[0]?.text).toContain(`„${name}“ gehört zur Farbordnung`);
		expect(f[0]?.text).toContain(ersatz);
	});

	it('ein neu gesetzter Ton der Bedeutungen gilt nicht als eigener Name', async () => {
		const f = await namensfunde('.karte { --b-ok: var(--reihe-6); color: var(--b-ok); }');
		expect(f.map((x) => x.regel)).toEqual([BEKANNT, BEKANNT]);
	});

	it('einen Ton darf ein Baustein für sich setzen', async () => {
		expect(
			await funde(
				'.karte { --ton: var(--reihe-2); --ton-soft: var(--flaeche); color: var(--ton); }'
			)
		).toEqual([]);
	});

	it('ein eigener Name mit festem Farbwert bleibt ein fester Farbwert', async () => {
		const regeln = (await funde('a { --zeichen: #b8322a; fill: var(--zeichen); }')).map(
			(f) => f.regel
		);
		expect(regeln).toContain('color-no-hex');
		expect(regeln).not.toContain(BEKANNT);
	});

	describe('externe Vorgabe', () => {
		const mit = (deklaration: string, nutzung = 'fill: var(--extern-osten);'): string =>
			`.zeichen {\n\t${deklaration}\n\t${nutzung}\n}`;

		it('geht durch: in der Datei deklariert, Herkunft hinter dem Semikolon', async () => {
			const code = mit('--extern-osten: #b8322a; /* extern: Farbe des Ostens nach Quelle */');
			expect(await namensfunde(code)).toEqual([]);
			// Zwischenstand bis Teil 2: Den festen Wert weist color-no-hex weiter ab.
			expect((await funde(code)).map((f) => f.regel)).toContain('color-no-hex');
		});

		it('geht im Kern ganz durch, wenn der Wert die Farbreihe nennt', async () => {
			expect(
				await funde(
					mit('--extern-osten: var(--reihe-3); /* extern: Farbe des Ostens nach Quelle */')
				)
			).toEqual([]);
		});

		it('weist ab: das Markup setzt sie über style:, je Treffer einmal', async () => {
			const svelte =
				'<div class="k" style:--extern-osten={farbe}></div>\n<p style="margin: 0"></p>\n' +
				'<style>\n.k { fill: var(--extern-osten); }\n</style>\n<style>\n.l { margin: 0; }\n</style>';
			const f = await namensfunde(svelte, 'x.svelte');
			const markup = f.filter((x) => x.text.includes('per Klasse wählen'));
			expect(markup).toHaveLength(1);
			expect(markup[0]?.text.startsWith('Stil: ')).toBe(true);
			// Im Stil gilt der Name damit nicht als deklariert.
			expect(f.filter((x) => x.text.includes('nicht deklariert'))).toHaveLength(1);
			expect(f).toHaveLength(2);
			// Der Fund zeigt auf die Zeile im Markup, nicht auf den Stilblock.
			const { results } = await stylelint.lint({
				code: `<p></p>\n${svelte}`,
				codeFilename: 'x.svelte',
				config
			});
			const zeilen = results.flatMap((r) =>
				r.warnings.filter((w) => w.text.includes('per Klasse wählen')).map((w) => w.line)
			);
			expect(zeilen).toEqual([2]);
		});

		it('weist ab: das Markup deklariert sie in style="…"', async () => {
			const svelte =
				'<div class="k" style="--extern-osten: var(--reihe-3)"></div>\n<style>\n.k { fill: var(--extern-osten); }\n</style>';
			const f = await namensfunde(svelte, 'x.svelte');
			expect(f.filter((x) => x.text.includes('per Klasse wählen'))).toHaveLength(1);
			expect(f.filter((x) => x.text.includes('nicht deklariert'))).toHaveLength(1);
		});

		it.each([
			['ohne Kommentar', '--extern-osten: #b8322a;'],
			['mit leerem Kommentar', '--extern-osten: #b8322a; /* extern: */'],
			['mit anderem Kommentar', '--extern-osten: #b8322a; /* Farbe des Ostens */'],
			['Kommentar vor dem Semikolon', '--extern-osten: #b8322a /* extern: Quelle */;'],
			['Kommentar erst in der nächsten Zeile', '--extern-osten: #b8322a;\n\t/* extern: Quelle */']
		])('weist ab: Herkunft fehlt, %s', async (_name, deklaration) => {
			const f = await namensfunde(mit(deklaration));
			expect(f.map((x) => x.regel)).toEqual([BEKANNT]);
			expect(f[0]?.text).toContain('„--extern-osten“');
			expect(f[0]?.text).toContain(FORM);
		});

		it.each([
			['color', 'color: var(--extern-osten);'],
			['outline', 'outline: 2px solid var(--extern-osten);'],
			['outline-color', 'outline-color: var(--extern-osten);']
		])('weist ab: in %s', async (eigenschaft, nutzung) => {
			const f = await namensfunde(mit('--extern-osten: #b8322a; /* extern: Quelle */', nutzung));
			expect(f.map((x) => x.regel)).toEqual([BEKANNT]);
			expect(f[0]?.text).toContain(`„${eigenschaft}“`);
		});

		it('weist ab: in der Datei nicht deklariert', async () => {
			const f = await namensfunde('a { fill: var(--extern-westen); }');
			expect(f.map((x) => x.regel)).toEqual([BEKANNT]);
			expect(f[0]?.text).toContain('nicht deklariert');
			expect(f[0]?.text).toContain(FORM);
		});
	});

	it('nimmt app.css aus', async () => {
		expect(
			await namensfunde(':root { --ok: var(--b-ok); --x: var(--text); }', 'src/app.css')
		).toEqual([]);
	});
});

describe('Regel dashboard/alte-namen', () => {
	it.each(ALTE_NAMEN)('meldet %s als Hinweis mit dem Ersatz %s', async (name, ersatz) => {
		const f = await namensfunde(`a { border-color: var(${name}); }`);
		expect(f).toEqual([
			{ regel: ALT, stufe: 'warning', text: expect.stringContaining(`Neu: ${ersatz}.`) as string }
		]);
		expect(f[0]?.text.startsWith('Stil: ')).toBe(true);
	});

	it('wird mit der Einstellung zur Abweisung', async () => {
		const streng = { ...config, rules: { ...config.rules, [ALT]: true } };
		const f = await funde('a { color: var(--text); }', 'probe.css', streng);
		expect(f.map((x) => [x.regel, x.stufe])).toEqual([[ALT, 'error']]);
	});
});

describe('Liste der Namen', () => {
	const css = readFileSync('src/app.css', 'utf8');
	const inAppCss = new Set([...css.matchAll(/^\s*(--[a-z0-9-]+)\s*:/gm)].map((t) => t[1] ?? ''));
	const farbrollen: readonly string[] = FARBROLLEN;
	const uebrige: readonly string[] = UEBRIGE_NAMEN;
	const alte = ALTE_NAMEN.map(([name]): string => name);

	it('nennt keinen Namen zweimal und beginnt jeden mit --', () => {
		const alle = [...farbrollen, ...uebrige, ...alte];
		expect(new Set(alle).size).toBe(alle.length);
		for (const name of [...alle, ...VORSILBEN]) expect(name).toMatch(/^--[a-z0-9-]+$/);
	});

	it('kennt jeden Namen, den app.css deklariert', () => {
		const fremd = [...inAppCss].filter(
			(n) =>
				!farbrollen.includes(n) &&
				!uebrige.includes(n) &&
				!alte.includes(n) &&
				!n.startsWith('--b-') &&
				!VORSILBEN.some((v) => n.startsWith(v))
		);
		expect(fremd).toEqual([]);
	});

	it('nennt nur Namen, die app.css deklariert', () => {
		// --karte-oben setzt ein Baustein für die Karten darunter; app.css kennt ihn nicht.
		const ohne = [...farbrollen, ...uebrige, ...alte].filter(
			(n) => !inAppCss.has(n) && n !== '--karte-oben'
		);
		expect(ohne).toEqual([]);
	});

	it('führt die Farbrollen je Schema als Teil der Farbrollen', () => {
		for (const name of FARBROLLEN_JE_SCHEMA) expect(farbrollen).toContain(name);
	});

	it('jeder Ersatz eines alten Namens ist ein Name der Liste oder eine Klasse b-… mit --ton', () => {
		for (const [, ersatz] of ALTE_NAMEN) {
			const ton = /^Klasse b-[a-z]+ mit (--ton(?:-soft|-text|-auf)?)$/.exec(ersatz);
			expect(farbrollen).toContain(ton?.[1] ?? ersatz);
		}
	});
});
