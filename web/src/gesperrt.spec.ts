/**
 * Gesperrte Knöpfe (Etappe B2, Auftrag der Architektur): eine gemeinsame
 * Regel in app.css statt Deckkraft je Baustein. Gesperrt heißt gestrichelter
 * Rand, ohne Deckkraft, damit der Text lesbar bleibt (Vorlage
 * `.vorschlag:disabled` in EintragZeile; kontrast.spec.ts misst die Farben).
 */
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { describe, expect, it } from 'vitest';

const SRC = join(import.meta.dirname, '.');

/** Die Bausteine, die vorher bei gesperrt eine eigene Deckkraft hatten. */
const BAUSTEINE = [
	'lib/components/anstehend/FrageAntwort.svelte',
	'lib/components/aufgaben/AufgabenKasten.svelte',
	'lib/components/EtwasAnderes.svelte',
	'lib/katalog/grund/Knopf.svelte',
	'lib/katalog/grund/Schalter.svelte',
	'lib/katalog/grund/Wahl.svelte'
];

/** Alle Regeln eines Stils, deren Selektor `:disabled` oder `aria-disabled` nennt. */
function gesperrtRegeln(text: string): string[] {
	const stil = /<style[^>]*>([\s\S]*?)<\/style>/.exec(text)?.[1] ?? text;
	return [...stil.matchAll(/([^{}]+)\{([^{}]*)\}/g)]
		.filter((m) => /:disabled|aria-disabled/.test(m[1] ?? ''))
		.map((m) => `${(m[1] ?? '').trim()} {${m[2] ?? ''}}`);
}

describe('Gesperrte Knöpfe: eine Regel, ohne Deckkraft', () => {
	it('app.css sperrt mit gestricheltem Rand und ohne Deckkraft', () => {
		const css = readFileSync(join(SRC, 'app.css'), 'utf8');
		const regeln = gesperrtRegeln(css);
		const gemeinsam = regeln.find((r) =>
			r.includes(':is(button, .button, textarea, select):disabled')
		);
		expect(gemeinsam).toBeDefined();
		expect(gemeinsam).toContain('border-style: dashed');
		expect(gemeinsam).toContain('cursor: not-allowed');
		expect(regeln.join('\n')).not.toMatch(/opacity/);
	});

	it.each(BAUSTEINE)('%s setzt bei gesperrt keine Deckkraft', (datei) => {
		const text = readFileSync(join(SRC, datei), 'utf8');
		expect(gesperrtRegeln(text).join('\n')).not.toMatch(/opacity/);
	});
});
