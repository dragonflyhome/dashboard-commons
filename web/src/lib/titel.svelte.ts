/**
 * Der Titel des Browser-Tabs (Etappe A3, [benachrichtigungen]): vorn die
 * Zahl der Einträge der Stufe Jetzt, etwa „(2) Anstehend · Dashboard“, damit
 * der Inhaber sie auch sieht, wenn der Tab im Hintergrund ist. Die Zahl kommt aus
 * dem gemeinsamen Speicher `anstehend` (dieselbe wie am Reiter) und folgt
 * dem Live-Kanal ohne Neuladen; kein Fenster, kein Ton.
 *
 * Jede Seite schreibt `<title>{seitenTitel('Name')}</title>`; weil die
 * Funktion die Zahl im Zeichnen liest, ändert sich der Titel von selbst.
 */
import { anstehend } from './anstehend.svelte.ts';
import { titelMitZahl } from './anstehend.ts';

export function seitenTitel(name: string): string {
	return titelMitZahl(name, anstehend.jetzt);
}
