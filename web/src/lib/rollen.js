// @ts-check
/**
 * Die eine Liste der Namen, die ein `var(--…)` nennen darf (Ticket 20, Teil 1).
 * Die Stilregel `dashboard/bekannte-namen` (src/stilregeln.js) hält jede Stelle
 * dagegen; ein Name, der hier fehlt, malt nichts und wird darum abgewiesen.
 *
 * Nur einfache Listen aus Zeichenketten, ohne berechnete Einträge: Dieselbe
 * Datei wird später auch aus Rust gelesen. Die Farbwerte selbst stehen allein
 * in src/app.css.
 */

/**
 * Farbrollen: die Farbreihe des Schemas und die Töne, die eine Klasse `b-…`
 * setzt. Die nummerierten Reihen (`--reihe-1` …) kommen über VORSILBEN.
 * Die Töne der Bedeutungen (`--b-…`) fehlen mit Absicht: Außerhalb von app.css
 * erscheinen sie nur über die Klasse `b-…` und `--ton…`.
 */
export const FARBROLLEN = /** @type {const} */ ([
	'--hintergrund',
	'--flaeche',
	'--papier',
	'--vordergrund',
	'--vordergrund-leise',
	'--linie',
	'--grundfarbe',
	'--grundfarbe-soft',
	'--grundfarbe-text',
	'--auf-grundfarbe',
	'--fokus',
	'--ton',
	'--ton-soft',
	'--ton-text',
	'--ton-auf',
	'--overlay',
	'--code-bg'
]);

/** Die Farbrollen, die jedes Schema selbst setzen muss; die anderen folgen aus ihnen. */
export const FARBROLLEN_JE_SCHEMA = /** @type {const} */ ([
	'--hintergrund',
	'--flaeche',
	'--vordergrund',
	'--vordergrund-leise',
	'--linie',
	'--grundfarbe',
	'--grundfarbe-soft',
	'--grundfarbe-text',
	'--auf-grundfarbe'
]);

/**
 * Übrige Namen: Schatten und Ringe, Schrift, Maße. Die Schriftgrößen stehen
 * ausgeschrieben da und nicht als Vorsilbe `--text-`, damit ein Tippfehler
 * wie `--text-farbe` auffällt.
 */
export const UEBRIGE_NAMEN = /** @type {const} */ ([
	'--shadow',
	'--shadow-hover',
	'--shadow-gross',
	'--shadow-bild',
	'--ring-akzent',
	'--ring-hof',
	'--puls',
	'--puls-weit',
	'--schatten',
	'--schatten-l',
	'--schatten-r',
	'--font',
	'--mono',
	'--text-2xs',
	'--text-xs',
	'--text-sm',
	'--text-md',
	'--text-base',
	'--text-lg',
	'--text-xl',
	'--text-2xl',
	'--text-3xl',
	'--text-4xl',
	'--text-5xl',
	'--text-em-sm',
	'--text-em-md',
	'--text-em-lg',
	'--text-grafik',
	'--text-grafik-klein',
	'--text-h1',
	'--text-riesig',
	'--text-wurzel',
	'--radius',
	'--karte-oben'
]);

/**
 * Vorsilben, unter denen jeder Name gilt: die nummerierten Reihen, die Maße
 * des Toolkits und die externen Vorgaben (für die gelten eigene Auflagen,
 * siehe src/stilregeln.js). Bewusst keine Vorsilbe `--text-`.
 */
export const VORSILBEN = /** @type {const} */ (['--reihe-', '--bits-', '--extern-']);

/**
 * Alte Namen aus der Zeit vor der Farbordnung, je mit ihrem Ersatz. In app.css
 * stehen sie noch als Verweise; die Regel `dashboard/alte-namen` meldet jede
 * Stelle mit dem neuen Namen. Die alten Töne der Bedeutungen haben keinen
 * neuen Namen: An ihre Stelle tritt die Klasse `b-…` mit `--ton…`.
 */
export const ALTE_NAMEN = /** @type {const} */ ([
	['--bg', '--hintergrund'],
	['--card', '--flaeche'],
	['--text', '--vordergrund'],
	['--muted', '--vordergrund-leise'],
	['--line', '--linie'],
	['--accent', '--grundfarbe'],
	['--accent-soft', '--grundfarbe-soft'],
	['--accent-text', '--grundfarbe-text'],
	['--on-accent', '--auf-grundfarbe'],
	['--ok', 'Klasse b-ok mit --ton'],
	['--info', 'Klasse b-laeuft mit --ton'],
	['--warn', 'Klasse b-wartet mit --ton'],
	['--bad', 'Klasse b-problem mit --ton'],
	['--done', 'Klasse b-info mit --ton'],
	['--plan', 'Klasse b-neutral mit --ton'],
	['--on-warn', 'Klasse b-wartet mit --ton-auf'],
	['--ok-text', 'Klasse b-ok mit --ton-text'],
	['--info-text', 'Klasse b-laeuft mit --ton-text'],
	['--warn-text', 'Klasse b-wartet mit --ton-text'],
	['--bad-text', 'Klasse b-problem mit --ton-text'],
	['--ok-soft', 'Klasse b-ok mit --ton-soft'],
	['--info-soft', 'Klasse b-laeuft mit --ton-soft'],
	['--warn-soft', 'Klasse b-wartet mit --ton-soft'],
	['--bad-soft', 'Klasse b-problem mit --ton-soft'],
	['--done-soft', 'Klasse b-info mit --ton-soft'],
	['--plan-soft', 'Klasse b-neutral mit --ton-soft']
]);
