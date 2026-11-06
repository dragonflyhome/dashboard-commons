/**
 * Bedeutungen aus dem Vertrag (`ok`, `laeuft`, `wartet`, `problem`, `info`,
 * `neutral`) in CSS-Klassen und Wörter übersetzen. Der Server schickt nur
 * Bedeutungen, die Farbe entscheidet die Oberfläche.
 */
import type { Bedeutung, FortschrittStufe, StandStufe, Status } from '$vertrag';

/** Alle sechs Bedeutungen, in fester Reihenfolge (für Legenden und Tests). */
export const BEDEUTUNGEN: readonly Bedeutung[] = [
	'ok',
	'laeuft',
	'wartet',
	'problem',
	'info',
	'neutral'
];

/** Klasse einer Bedeutung (app.css): `b-ok`, `b-laeuft`, … */
export type BedeutungKlasse = `b-${Bedeutung}`;

/**
 * Die eine Klasse je Bedeutung für Punkte, Lampen, Balken und Ringe (app.css).
 * Sie setzt den Ton `--ton`: ok grün (`--ok`), läuft Türkis (`--info`),
 * wartet gelb (`--warn`), Problem rot (`--bad`), Hinweis blau (`--done`),
 * neutral grau (`--plan`). Wer malt, nimmt `var(--ton, …)`.
 */
export function klasse(b: Bedeutung): BedeutungKlasse {
	return `b-${b}`;
}

/** Wort für Bildschirmleser, Tooltips und Filter. */
export function wort(b: Bedeutung): string {
	switch (b) {
		case 'ok':
			return 'in Ordnung';
		case 'laeuft':
			return 'läuft';
		case 'wartet':
			return 'wartet';
		case 'problem':
			return 'Problem';
		case 'info':
			return 'Hinweis';
		case 'neutral':
			return 'neutral';
	}
}

/**
 * Farbe eines Fortschritts aus der Stufe des Servers (Etappe 3a+; die Regel
 * ab 100 und ab 50 Prozent rechnet nur der Server). Ohne Stufe neutral.
 */
export function ausFortschrittStufe(stufe: FortschrittStufe | null | undefined): Bedeutung {
	switch (stufe) {
		case 'erreicht':
			return 'ok';
		case 'ab-haelfte':
			return 'laeuft';
		case 'unter-haelfte':
			return 'wartet';
		case null:
		case undefined:
			return 'neutral';
	}
}

/** Bedeutung des Status aus projekt.toml, wie der Server (`Bedeutung::aus_wort`). */
export function ausStatus(status: Status): Bedeutung {
	switch (status) {
		case 'aktiv':
			return 'laeuft';
		case 'pausiert':
			return 'wartet';
		// Entscheidung archiv-status: archiviert ist ruhig (keine Farbe).
		case 'geplant':
		case 'archiviert':
			return 'neutral';
		case 'fertig':
			return 'ok';
		case 'problem':
			return 'problem';
	}
}

/**
 * Etappe B1: Farbe eines Stand-Hinweises („Stand vom …“) aus der Stufe des
 * Servers. Alt ist gelb (wartet), frisch bleibt ruhig (neutral); das Alter
 * rechnet nur der Server.
 */
export function ausStandStufe(stufe: StandStufe | null | undefined): Bedeutung {
	return stufe === 'alt' ? 'wartet' : 'neutral';
}
