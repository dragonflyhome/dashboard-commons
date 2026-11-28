// @ts-check
/**
 * Zwei eigene Stilregeln (Ticket 20, Teil 1): Jedes `var(--…)` nennt einen
 * Namen aus der Liste in src/lib/rollen.js, auch in `color-mix` und im
 * Rückfallwert. Ein unbekannter Name malt in CSS still nichts; darum fällt er
 * ohne diese Regeln erst am Bildschirm auf.
 *
 * `dashboard/bekannte-namen` weist ab:
 * - einen Namen, den es nicht gibt,
 * - einen Ton der Bedeutungen (`--b-…`) außerhalb von app.css,
 * - eine externe Vorgabe (`--extern-…`), die in der Datei nicht deklariert ist,
 *   deren Herkunft fehlt oder die Text oder Umriss färbt.
 *
 * Durch gehen: die Namen der Liste, die Vorsilben, und eigene Namen, die in
 * derselben Datei deklariert sind (im Stil oder im Markup über `style:--name`).
 * Was ein eigener Name als Wert nennt, wird wie jede andere Stelle geprüft.
 *
 * `dashboard/alte-namen` meldet einen alten Namen mit seinem Ersatz. Sie ist
 * eine eigene Regel, damit die Einstellung ihre Stufe für sich setzen kann:
 * erst Hinweis, später Abweisung.
 *
 * Jede Meldung beginnt mit „Stil“: Der Bau zeigt nur solche Zeilen.
 */
import wertLeser from 'postcss-value-parser';
import stylelint from 'stylelint';
import { ALTE_NAMEN, FARBROLLEN, UEBRIGE_NAMEN, VORSILBEN } from './lib/rollen.js';

const BEKANNT = 'dashboard/bekannte-namen';
const ALT = 'dashboard/alte-namen';

const EXTERN = '--extern-';
const FORM = '--extern-<name>: <wert>; /* extern: <Herkunft> */';

/** @type {ReadonlySet<string>} */
const bekannt = new Set([...FARBROLLEN, ...UEBRIGE_NAMEN]);
/** @type {ReadonlyMap<string, string>} */
const alt = new Map(ALTE_NAMEN);

const meldungen = stylelint.utils.ruleMessages(BEKANNT, {
	/** @param {string} name */
	unbekannt: (name) =>
		`Stil: Den Namen „${name}“ gibt es nicht. Erlaubt sind die Namen aus src/lib/rollen.js und eigene Namen, die in derselben Datei deklariert sind.`,
	/** @param {string} name @param {string} ersatz */
	bedeutung: (name, ersatz) =>
		`Stil: „${name}“ ist ein Ton der Bedeutungen und steht nur in app.css. Nimm stattdessen: ${ersatz}.`,
	/** @param {string} name */
	externFehlt: (name) =>
		`Stil: Die externe Vorgabe „${name}“ ist in dieser Datei nicht deklariert. Erwartet: ${FORM}`,
	/** @param {string} name */
	externHerkunft: (name) =>
		`Stil: Bei „${name}“ fehlt die Herkunft hinter dem Semikolon. Erwartet: ${FORM}`,
	/** @param {string} name @param {string} eigenschaft */
	externText: (name, eigenschaft) =>
		`Stil: Die externe Vorgabe „${name}“ steht in „${eigenschaft}“. Externe Farben tragen nie Text, Bedeutung oder Fokus.`
});

const meldungenAlt = stylelint.utils.ruleMessages(ALT, {
	/** @param {string} name @param {string} ersatz */
	alt: (name, ersatz) => `Stil: „${name}“ ist ein alter Name. Neu: ${ersatz}.`
});

/**
 * Alle Namen, die ein Wert über `var(--…)` nennt, auch verschachtelt in
 * `color-mix(…)` und im Rückfallwert.
 * @param {string} wert
 * @returns {string[]}
 */
export function namenIn(wert) {
	/** @type {string[]} */
	const namen = [];
	wertLeser(wert).walk((knoten) => {
		if (knoten.type !== 'function' || knoten.value.toLowerCase() !== 'var') return;
		const erster = knoten.nodes.find((n) => n.type === 'word');
		if (erster?.value.startsWith('--') === true) namen.push(erster.value);
	});
	return namen;
}

/**
 * Der Ersatz für einen Ton der Bedeutungen: `--b-ok-soft` wird zu
 * „Klasse b-ok mit --ton-soft“.
 * @param {string} name
 * @returns {string}
 */
function ersatzFuerBedeutung(name) {
	const teil = /^--b-(.+?)(-soft|-text|-auf)?$/.exec(name);
	return `Klasse b-${teil?.[1] ?? '…'} mit --ton${teil?.[2] ?? ''}`;
}

/**
 * Die eigenen Namen einer Datei: was ihr Stil deklariert und was ihr Markup
 * über `style:--name` setzt.
 * In einer Svelte-Datei ist jeder Stilblock eine eigene Wurzel; gemeint ist
 * darum immer die ganze Datei.
 * @param {import('postcss').Root} wurzel
 * @returns {Set<string>}
 */
function eigeneNamen(wurzel) {
	/** @type {Set<string>} */
	const eigene = new Set();
	// postcss-html hängt die ganze Datei als `document` an jede Wurzel.
	const mit = /** @type {{ document?: import('postcss').Document }} */ (wurzel);
	const datei = mit.document ?? wurzel;
	datei.walkDecls(/^--/, (d) => {
		eigene.add(d.prop);
	});
	const quelle = datei.source?.input.css ?? '';
	for (const treffer of quelle.matchAll(/\bstyle:(--[\w-]+)/g)) {
		if (treffer[1] !== undefined) eigene.add(treffer[1]);
	}
	return eigene;
}

/**
 * Ob hinter der Deklaration, in derselben Zeile, ihre Herkunft steht:
 * ein Kommentar `extern: …` mit Inhalt. Ein Kommentar vor dem Semikolon gehört zum Wert
 * und zählt darum nicht.
 * @param {import('postcss').Declaration} d
 * @returns {boolean}
 */
function hatHerkunft(d) {
	const danach = d.next();
	if (danach?.type !== 'comment') return false;
	if (danach.source?.start?.line !== d.source?.end?.line) return false;
	return /^extern:\s*\S/.test(danach.text);
}

/** @type {import('stylelint').Rule} */
const bekannteNamen = (an) => (wurzel, ergebnis) => {
	if (!stylelint.utils.validateOptions(ergebnis, BEKANNT, { actual: an, possible: [true] })) return;
	const eigene = eigeneNamen(wurzel);
	/** @param {import('postcss').Declaration} d @param {string} text */
	const melde = (d, text) => {
		stylelint.utils.report({ ruleName: BEKANNT, result: ergebnis, node: d, message: text });
	};
	wurzel.walkDecls((d) => {
		if (d.prop.startsWith(EXTERN) && !hatHerkunft(d)) melde(d, meldungen.externHerkunft(d.prop));
		const eigenschaft = d.prop.toLowerCase();
		for (const name of namenIn(d.value)) {
			if (bekannt.has(name) || alt.has(name)) continue;
			if (name.startsWith(EXTERN)) {
				if (!eigene.has(name)) melde(d, meldungen.externFehlt(name));
				else if (eigenschaft === 'color' || eigenschaft.startsWith('outline'))
					melde(d, meldungen.externText(name, d.prop));
				continue;
			}
			if (VORSILBEN.some((v) => name.startsWith(v)) || eigene.has(name)) continue;
			if (name.startsWith('--b-')) melde(d, meldungen.bedeutung(name, ersatzFuerBedeutung(name)));
			else melde(d, meldungen.unbekannt(name));
		}
	});
};
bekannteNamen.ruleName = BEKANNT;
bekannteNamen.messages = meldungen;

/** @type {import('stylelint').Rule} */
const alteNamen = (an) => (wurzel, ergebnis) => {
	if (!stylelint.utils.validateOptions(ergebnis, ALT, { actual: an, possible: [true] })) return;
	wurzel.walkDecls((d) => {
		for (const name of namenIn(d.value)) {
			const ersatz = alt.get(name);
			if (ersatz === undefined) continue;
			stylelint.utils.report({
				ruleName: ALT,
				result: ergebnis,
				node: d,
				message: meldungenAlt.alt(name, ersatz)
			});
		}
	});
};
alteNamen.ruleName = ALT;
alteNamen.messages = meldungenAlt;

/** Die beiden Regeln als Plugins für `plugins` in stylelint.config.js. */
export default [
	stylelint.createPlugin(BEKANNT, bekannteNamen),
	stylelint.createPlugin(ALT, alteNamen)
];
