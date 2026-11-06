/**
 * Vendor-Kopie unverändert (Auflage aus der Abnahme 3a-3): Die Einträge
 * `file:vendor/…` im Lockfile haben kein `integrity`, `npm ci` prüft ihren
 * Inhalt also nicht. Dieser Test rechnet darum in `npm run verify` je Paket
 * `inhalt_sha256` aus `web/vendor/<paket>/HERKUNFT.toml` nach, genau wie
 * `scripts/Vendor-Pruefen.ps1 -OhneNetz`:
 * alle Dateien des Paket-Ordners außer HERKUNFT.toml, Pfad relativ mit „/“,
 * ordinal sortiert; je Datei eine Zeile "<pfad>:<sha256 hex klein>\n"; über
 * den UTF-8-Text dieser Zeilen sha256, hex klein.
 */
import { createHash } from 'node:crypto';
import { readdirSync, readFileSync } from 'node:fs';
import { join, relative, sep } from 'node:path';
import { describe, expect, it } from 'vitest';

const VENDOR = join(import.meta.dirname, '..', 'vendor');

/** Paket-Ordner: vendor/<name> und vendor/@scope/<name>, wie im Skript. */
function pakete(): string[] {
	const namen: string[] = [];
	for (const d of readdirSync(VENDOR, { withFileTypes: true })) {
		if (!d.isDirectory()) continue;
		if (d.name.startsWith('@')) {
			for (const u of readdirSync(join(VENDOR, d.name), { withFileTypes: true })) {
				if (u.isDirectory()) namen.push(`${d.name}/${u.name}`);
			}
		} else {
			namen.push(d.name);
		}
	}
	return namen.sort();
}

/** Alle Dateien unter `basis`, relativ mit „/“ (auch versteckte). */
function dateien(basis: string): string[] {
	return readdirSync(basis, { recursive: true, withFileTypes: true })
		.filter((e) => e.isFile())
		.map((e) => relative(basis, join(e.parentPath, e.name)).split(sep).join('/'));
}

/** Rechnung wie Inhalt-Sha256 in scripts/Vendor-Pruefen.ps1. */
function inhaltSha256(ordner: string): { summe: string; anzahl: number } {
	// Das Skript vergleicht mit -ne, also ohne Groß- und Kleinschreibung.
	const rel = dateien(ordner).filter((r) => r.toLowerCase() !== 'herkunft.toml');
	// Ordinal wie [StringComparer]::Ordinal: UTF-16-Einheiten, nicht localeCompare.
	rel.sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
	let text = '';
	for (const r of rel) {
		const h = createHash('sha256')
			.update(readFileSync(join(ordner, r)))
			.digest('hex');
		text += `${r}:${h}\n`;
	}
	return { summe: createHash('sha256').update(text, 'utf8').digest('hex'), anzahl: rel.length };
}

/** Ein Wert der einfachen Form `schluessel = "text"` oder `= zahl` aus HERKUNFT.toml. */
function wert(toml: string, schluessel: string): string | undefined {
	const m = new RegExp(`^${schluessel}[ \\t]*=[ \\t]*(?:"([^"]*)"|([0-9]+))[ \\t\\r]*$`, 'm').exec(
		toml
	);
	return m?.[1] ?? m?.[2];
}

const liste = pakete();

describe('Vendor-Kopie (web/vendor)', () => {
	it('jedes file:vendor-Paket aus package.json hat einen Ordner und umgekehrt', () => {
		const pkg = JSON.parse(readFileSync(join(VENDOR, '..', 'package.json'), 'utf8')) as {
			devDependencies?: Record<string, string>;
			dependencies?: Record<string, string>;
		};
		const ausPkg = Object.entries({ ...pkg.dependencies, ...pkg.devDependencies })
			.filter(([, v]) => v.startsWith('file:vendor/'))
			.map(([k, v]) => {
				expect(v, k).toBe(`file:vendor/${k}`);
				return k;
			})
			.sort();
		expect(ausPkg).toEqual(liste);
		expect(liste.length).toBeGreaterThan(0);
	});

	// Zeitlimit 60 s nur hier: Der Test liest und hasht jede Datei eines Pakets;
	// große Pakete (etwa @swc/helpers) brauchen unter Last (volles verify, Bau
	// nebenher) mehr als die 5 s von vitest.
	it.each(liste)(
		'%s: inhalt_sha256 aus HERKUNFT.toml passt zum Inhalt',
		(paket) => {
			const ordner = join(VENDOR, ...paket.split('/'));
			const toml = readFileSync(join(ordner, 'HERKUNFT.toml'), 'utf8');
			expect(wert(toml, 'paket')).toBe(paket);
			const soll = wert(toml, 'inhalt_sha256');
			expect(soll, 'inhalt_sha256 fehlt').toMatch(/^[0-9a-f]{64}$/);
			const ist = inhaltSha256(ordner);
			expect(String(ist.anzahl), 'Zahl der Dateien').toBe(wert(toml, 'dateien'));
			expect(ist.summe, 'Inhalt geändert (inhalt_sha256 passt nicht)').toBe(soll);
			const version = (
				JSON.parse(readFileSync(join(ordner, 'package.json'), 'utf8')) as { version: string }
			).version;
			expect(version).toBe(wert(toml, 'version'));
		},
		60_000
	);
});
