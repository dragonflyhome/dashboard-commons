/**
 * Schritt E2b: die Bauliste (`projekte.generated.ts`, alle Slugs mit eigener
 * Seite) als eigener Teil des Baus. Sie kommt nur dynamisch, damit sie nie mit
 * Code der Hülle in einem öffentlichen Teil liegt; der Dienst liefert sie nur
 * mit Anmeldung (`_app/oeffentlich.json`, scripts/oeffentlich.mjs). Einmal
 * geladen, bleibt sie; ein Fehler wird nicht gemerkt (der nächste Versuch lädt neu).
 */
import type { Component } from 'svelte';

type Laden = () => Promise<{ default: Component }>;

/**
 * Die drei Tabellen als eigener Typ, nicht aus dem erzeugten Modul abgeleitet: Eine
 * Arbeitskopie mit einer Liste alter Form (zwei Tabellen) besteht so die Typprüfung.
 */
export interface Liste {
	projektSeiten: Record<string, Laden>;
	projektUnterseiten: Record<string, Laden>;
	projektKopfEnde: Record<string, Laden>;
}

/** Was das Modul liefert: Eine Liste in alter Form (zwei Tabellen) hat keine Kopfteile. */
type Geladen = Omit<Liste, 'projektKopfEnde'> & Partial<Pick<Liste, 'projektKopfEnde'>>;

let geladen: Promise<Liste> | null = null;

export function bauliste(): Promise<Liste> {
	geladen ??= import('#lib/projekte.generated.ts').then(
		(m: Geladen) => ({
			projektSeiten: m.projektSeiten,
			projektUnterseiten: m.projektUnterseiten,
			projektKopfEnde: m.projektKopfEnde ?? {}
		}),
		(e: unknown) => {
			geladen = null;
			throw e;
		}
	);
	return geladen;
}
