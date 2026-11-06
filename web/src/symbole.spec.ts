/**
 * Die reinen Teile von scripts/symbole-erzeugen.mjs, ohne Edge: der Filter,
 * der nur IHDR, IDAT und IEND behält, und der ICO-Packer. Das Rendern selbst
 * braucht Edge und läuft nur von Hand (Anleitung, „das Logo tauschen“);
 * was dabei herauskommt, prüft logo.spec.ts an den Dateien in static/.
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';
import { ICO_GROESSEN, icoPacken, nurBildBloecke } from '../scripts/symbole-erzeugen.mjs';

const STATIC = join(import.meta.dirname, '..', 'static');
const SIGNATUR = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

/** Ein Block: Länge, Typ, Daten, Prüfsumme (hier beliebig, der Filter rechnet sie nicht). */
function block(typ: string, daten: Buffer, pruef = 0x12345678): Buffer {
	const kopf = Buffer.alloc(8);
	kopf.writeUInt32BE(daten.length, 0);
	kopf.write(typ, 4, 'latin1');
	const ende = Buffer.alloc(4);
	ende.writeUInt32BE(pruef, 0);
	return Buffer.concat([kopf, daten, ende]);
}

/** IHDR mit Größe und Farbtyp (6 = RGBA, 3 = Palette). */
function ihdr(groesse: number, farbtyp = 6): Buffer {
	const d = Buffer.alloc(13);
	d.writeUInt32BE(groesse, 0);
	d.writeUInt32BE(groesse, 4);
	d[8] = 8;
	d[9] = farbtyp;
	return block('IHDR', d);
}

function typen(png: Buffer): string[] {
	const t: string[] = [];
	for (let i = 8; i + 8 <= png.length; i += 12 + png.readUInt32BE(i)) {
		t.push(png.toString('latin1', i + 4, i + 8));
	}
	return t;
}

/** Ein kleines PNG mit Metadaten, wie es ein Browser liefern könnte. */
function mitMetadaten(groesse: number): Buffer {
	return Buffer.concat([
		SIGNATUR,
		ihdr(groesse),
		block('sRGB', Buffer.from([0])),
		block('pHYs', Buffer.alloc(9)),
		block('IDAT', Buffer.from(`bild-${String(groesse)}-a`)),
		block('tEXt', Buffer.from('Software\0Edge')),
		block('IDAT', Buffer.from(`bild-${String(groesse)}-b`)),
		block('tIME', Buffer.alloc(7)),
		block('IEND', Buffer.alloc(0))
	]);
}

describe('symbole-erzeugen: nur Bildblöcke', () => {
	it('behält Signatur, IHDR, alle IDAT und IEND in ihrer Reihenfolge, unverändert', () => {
		const roh = mitMetadaten(16);
		const sauber = nurBildBloecke(roh);
		expect(sauber.subarray(0, 8).equals(SIGNATUR)).toBe(true);
		expect(typen(roh)).toEqual(['IHDR', 'sRGB', 'pHYs', 'IDAT', 'tEXt', 'IDAT', 'tIME', 'IEND']);
		expect(typen(sauber)).toEqual(['IHDR', 'IDAT', 'IDAT', 'IEND']);
		expect(sauber.includes(Buffer.from('bild-16-a'))).toBe(true);
		expect(sauber.includes(Buffer.from('bild-16-b'))).toBe(true);
		expect(sauber.includes(Buffer.from('Software'))).toBe(false);
		// Blöcke samt Prüfsumme bleiben byte-gleich.
		expect(sauber.includes(ihdr(16))).toBe(true);
	});

	it('lässt ein sauberes PNG genau so, wie es ist', () => {
		const png = readFileSync(join(STATIC, 'icons', 'symbol-192.png'));
		expect(nurBildBloecke(png).equals(png)).toBe(true);
		expect(new Set(typen(png))).toEqual(new Set(['IHDR', 'IDAT', 'IEND']));
	});

	it('lehnt Nicht-PNG, abgeschnittene Blöcke und Paletten ab', () => {
		expect(() => nurBildBloecke(Buffer.from('kein Bild'))).toThrow('kein PNG');
		const ganz = mitMetadaten(16);
		expect(() => nurBildBloecke(ganz.subarray(0, ganz.length - 3))).toThrow('abgeschnitten');
		const palette = Buffer.concat([SIGNATUR, ihdr(16, 3), block('IEND', Buffer.alloc(0))]);
		expect(() => nurBildBloecke(palette)).toThrow('Palette');
	});
});

describe('symbole-erzeugen: ICO packen', () => {
	it('schreibt Kopf, je Bild einen Eintrag und die PNGs dahinter', () => {
		const bilder = [16, 256].map((groesse) => ({
			groesse,
			png: nurBildBloecke(mitMetadaten(groesse))
		}));
		const ico = icoPacken(bilder);
		expect([ico.readUInt16LE(0), ico.readUInt16LE(2), ico.readUInt16LE(4)]).toEqual([0, 1, 2]);
		let versatz = 6 + 16 * 2;
		bilder.forEach(({ groesse, png }, n) => {
			const o = 6 + 16 * n;
			// 256 steht als 0; Ebenen 1, 32 Bit.
			expect([ico[o], ico[o + 1]]).toEqual(groesse === 256 ? [0, 0] : [groesse, groesse]);
			expect([ico.readUInt16LE(o + 4), ico.readUInt16LE(o + 6)]).toEqual([1, 32]);
			expect(ico.readUInt32LE(o + 8)).toBe(png.length);
			expect(ico.readUInt32LE(o + 12)).toBe(versatz);
			expect(ico.subarray(versatz, versatz + png.length).equals(png)).toBe(true);
			versatz += png.length;
		});
		expect(ico.length).toBe(versatz);
	});

	it('packt die Bilder aus static/favicon.ico wieder genau zu dieser Datei', () => {
		const ico = readFileSync(join(STATIC, 'favicon.ico'));
		const bilder = Array.from({ length: ico.readUInt16LE(4) }, (_, n) => {
			const o = 6 + 16 * n;
			const start = ico.readUInt32LE(o + 12);
			return {
				groesse: ico[o] === 0 ? 256 : (ico[o] ?? 0),
				png: ico.subarray(start, start + ico.readUInt32LE(o + 8))
			};
		});
		expect(bilder.map((b) => b.groesse)).toEqual(ICO_GROESSEN);
		expect(icoPacken(bilder).equals(ico)).toBe(true);
	});

	it('lehnt Größen ab, die ein ICO nicht fassen kann', () => {
		const png = nurBildBloecke(mitMetadaten(16));
		expect(() => icoPacken([{ groesse: 512, png }])).toThrow('passt nicht');
		expect(() => icoPacken([{ groesse: 0, png }])).toThrow('passt nicht');
	});
});
