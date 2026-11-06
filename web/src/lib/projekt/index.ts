/**
 * `$projekt`: Daten für Projektseiten in neuer Form.
 *
 *   import { ladeDaten } from '$projekt';
 *   const plan = ladeDaten('gantt'); // Typ folgt aus dem Namen
 *   const notizen = ladeDaten<Notizen>('eigen-notizen'); // eigener Typ
 *
 * Das Projekt kommt aus dem Kontext der Route; die Seite nennt nur den
 * Dateinamen in `daten/` ohne `.toml`.
 */
export { ladeDaten, ladeKopf } from './laden.svelte.ts';
export type { DatenTypen, Geladen, Zustand } from './laden.svelte.ts';
