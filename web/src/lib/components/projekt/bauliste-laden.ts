/**
 * Schritt E2b: die Bauliste (`projekte.generated.ts`, alle Slugs mit eigener
 * Seite) als eigener Teil des Baus. Sie kommt nur dynamisch, damit sie nie mit
 * Code der Hülle in einem öffentlichen Teil liegt; der Dienst liefert sie nur
 * mit Anmeldung (`_app/oeffentlich.json`, scripts/oeffentlich.mjs). Einmal
 * geladen, bleibt sie; ein Fehler wird nicht gemerkt (der nächste Versuch lädt neu).
 */
import type * as Modul from '#lib/projekte.generated.ts';
/** Die beiden Tabellen, ohne den Modul-Namensraum (den nähme $state.raw nicht als Wert). */
export type Liste = Pick<typeof Modul, 'projektSeiten' | 'projektUnterseiten'>;

let geladen: Promise<Liste> | null = null;

export function bauliste(): Promise<Liste> {
	geladen ??= import('#lib/projekte.generated.ts').then(
		({ projektSeiten, projektUnterseiten }) => ({ projektSeiten, projektUnterseiten }),
		(e: unknown) => {
			geladen = null;
			throw e;
		}
	);
	return geladen;
}
