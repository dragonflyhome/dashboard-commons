/**
 * Etappe 3b-2: Das Logo ist eine einzige Datei, web/static/logo.svg (gewählt:
 * Entwurf 1 „Blüte“, LogoBluete.svelte). Kopfleiste, Browser-Reiter,
 * Manifest und die Seite der Logo-Entwürfe zeigen genau diese Datei; das
 * echte Logo ist darum ein Tausch dieser einen Datei (Anleitung, „das Logo
 * tauschen“). Nach den Asset-Regeln: ohne Skript, ohne Verweise nach außen.
 */
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

const SRC = import.meta.dirname;
const WEB = join(SRC, '..');
const lies = (...teile: string[]): string => readFileSync(join(WEB, ...teile), 'utf8');

describe('Logo als eine Datei (Etappe 3b-2)', () => {
	it('static/logo.svg ist ein SVG ohne Skript und ohne Verweise nach außen', () => {
		const svg = lies('static', 'logo.svg');
		expect(svg).toContain('<svg');
		expect(svg).not.toMatch(/<script|on[a-z]+\s*=|<foreignObject/i);
		// Nur der SVG-Namensraum und interne Verweise (url(#…)), keine Adressen.
		const adressen = [...svg.matchAll(/(?:href|src)\s*=\s*"([^"]*)"|url\(([^)]*)\)/g)]
			.map((m) => m[1] ?? m[2] ?? '')
			.filter((a) => !a.startsWith('#'));
		expect(adressen).toEqual([]);
	});

	it('feste Farben nur auf eigener Fläche oder mit prefers-color-scheme', () => {
		// Als img, Favicon und App-Symbol wirken keine Token (Anleitung,
		// „das Logo tauschen“): Das Logo bringt seine Fläche mit oder wechselt
		// seine Farben selbst.
		const svg = lies('static', 'logo.svg');
		const box = /viewBox\s*=\s*"\s*0\s+0\s+([\d.]+)\s+([\d.]+)\s*"/.exec(svg);
		const flaeche = [...svg.matchAll(/<rect\b([^>]*)\/?>/g)].some(([, a = '']) => {
			const wert = (n: string): string | undefined =>
				new RegExp(`\\b${n}\\s*=\\s*"([^"]*)"`).exec(a)?.[1];
			const fill = wert('fill');
			return (
				box !== null &&
				(wert('x') ?? '0') === '0' &&
				(wert('y') ?? '0') === '0' &&
				wert('width') === box[1] &&
				wert('height') === box[2] &&
				fill !== undefined &&
				fill !== 'none' &&
				fill !== 'transparent'
			);
		});
		const wechselt = /@media\s*\(\s*prefers-color-scheme\s*:\s*dark\s*\)/.test(svg);
		expect(flaeche || wechselt).toBe(true);
	});

	it('Favicon, Manifest, Kopfleiste und Logo-Entwürfe zeigen diese Datei', () => {
		expect(lies('src', 'app.html')).toContain(
			'<link rel="icon" href="/logo.svg" type="image/svg+xml" />'
		);
		// favicon.ico bleibt als Rückfall.
		expect(lies('src', 'app.html')).toContain('href="/favicon.ico"');
		const manifest = JSON.parse(lies('static', 'manifest.webmanifest')) as {
			icons: { src: string; sizes: string; type: string }[];
		};
		expect(manifest.icons).toContainEqual({
			src: '/logo.svg',
			sizes: 'any',
			type: 'image/svg+xml'
		});
		// Kopfleiste, Fassung 3: das Symbol ist die Wurzel der Brotkrumen.
		expect(lies('src', 'lib', 'components', 'kopf', 'Brotkrumen.svelte')).toContain(
			"asset('logo.svg')"
		);
		expect(lies('src', 'routes', 'katalog', 'entwuerfe', 'logo', '+page.svelte')).toContain(
			"asset('logo.svg')"
		);
	});

	it('logo.svg ist der gewählte Entwurf 1 „Blüte“: dieselben Formen wie LogoBluete.svelte', () => {
		// Formen samt Klassen in Reihenfolge; Farben (fill) zählen hier nicht.
		const formen = (text: string): string[] =>
			[...text.matchAll(/<(rect|ellipse|path|circle)\b([^>]*?)\/?>/g)].map(([, tag, a = '']) => {
				const attrs = [...a.matchAll(/([a-z-]+)\s*=\s*"([^"]*)"/g)]
					.filter(([, n]) => n !== 'fill')
					.map(([, n, w]) => `${n}=${w}`)
					.sort();
				return `${tag} ${attrs.join(' ')}`;
			});
		const entwurf = lies('src', 'routes', 'katalog', 'entwuerfe', 'logo', 'LogoBluete.svelte');
		const svg = lies('static', 'logo.svg');
		expect(formen(svg).length).toBe(8);
		expect(formen(svg)).toEqual(formen(entwurf));
		const box = (text: string): string | undefined => /viewBox="([^"]*)"/.exec(text)?.[1];
		expect(box(svg)).toBe(box(entwurf));
	});

	it('logo.svg trägt die Werte von --accent, --on-accent und --warn, hell und dunkel', () => {
		const css = lies('src', 'app.css');
		const token = (block: string, name: string): string | undefined =>
			new RegExp(`--${name}:\\s*(#[0-9a-f]{6})\\s*;`, 'i').exec(block)?.[1];
		// Der erste Block mit :root (Selektorliste) trägt die hellen Werte.
		const hell = /:root\b[^{]*\{([\s\S]*?)\n\}/.exec(css)?.[1] ?? '';
		const dunkel = /:root\[data-theme='dunkel'\][^{]*\{([\s\S]*?)\n\}/.exec(css)?.[1] ?? '';
		expect(token(hell, 'accent')).toBeDefined();
		expect(token(dunkel, 'accent')).toBeDefined();
		const svg = lies('static', 'logo.svg');
		const [stilHell = '', stilDunkel = ''] = svg.split(
			/@media\s*\(\s*prefers-color-scheme\s*:\s*dark\s*\)/
		);
		const fuell = (stil: string, klasse: string): string | undefined =>
			new RegExp(`\\.${klasse}\\b[^{]*\\{[^}]*fill:\\s*(#[0-9a-f]{6})`, 'i').exec(stil)?.[1];
		for (const [stil, block] of [
			[stilHell, hell],
			[stilDunkel, dunkel]
		] as const) {
			expect(fuell(stil, 'grund')).toBe(token(block, 'accent'));
			expect(fuell(stil, 'blatt')).toBe(token(block, 'on-accent'));
			expect(fuell(stil, 'wasser')).toBe(token(block, 'on-accent'));
			expect(fuell(stil, 'kern')).toBe(token(block, 'warn'));
		}
	});

	it('logo.svg ist gültiges XML: keine zwei Bindestriche in Kommentaren', () => {
		// Sonst bleibt das Bild im Browser leer (XML verbietet „--“ in Kommentaren).
		const svg = lies('static', 'logo.svg');
		for (const [, inhalt = ''] of svg.matchAll(/<!--([\s\S]*?)-->/g)) {
			expect(inhalt).not.toContain('--');
		}
	});

	it('PNG-Rückfälle und favicon.ico: richtige Größen, keine Metadaten', () => {
		const bloecke = (png: Buffer): string[] => {
			const typen: string[] = [];
			for (let i = 8; i + 8 <= png.length; i += 12 + png.readUInt32BE(i)) {
				typen.push(png.toString('ascii', i + 4, i + 8));
			}
			return typen;
		};
		const pruefen = (png: Buffer, groesse: number): void => {
			expect(png.subarray(1, 4).toString('ascii')).toBe('PNG');
			expect(png.readUInt32BE(16)).toBe(groesse);
			expect(png.readUInt32BE(20)).toBe(groesse);
			expect(bloecke(png).filter((t) => /^(tEXt|iTXt|zTXt|eXIf|tIME)$/.test(t))).toEqual([]);
		};
		const bild = (...teile: string[]): Buffer => readFileSync(join(WEB, 'static', ...teile));
		pruefen(bild('icons', 'symbol-192.png'), 192);
		const s256 = bild('icons', 'symbol-256.png');
		pruefen(s256, 256);
		const ico = bild('favicon.ico');
		const groessen: number[] = [];
		for (let n = 0; n < ico.readUInt16LE(4); n++) {
			const o = 6 + 16 * n;
			const png = ico.subarray(
				ico.readUInt32LE(o + 12),
				ico.readUInt32LE(o + 12) + ico.readUInt32LE(o + 8)
			);
			// 0 steht im ICO für 256 Pixel.
			const groesse = ico[o] === 0 ? 256 : (ico[o] ?? 0);
			pruefen(png, groesse);
			groessen.push(groesse);
			// Das 256er im ICO ist dasselbe Bild wie das App-Symbol.
			if (groesse === 256) expect(png.equals(s256)).toBe(true);
		}
		expect(groessen).toEqual([16, 24, 32, 48, 64, 128, 256]);
	});

	it('keine zweite Logo-Datei mehr in der Oberfläche', () => {
		expect(existsSync(join(SRC, 'lib', 'assets', 'favicon.svg'))).toBe(false);
	});
});
