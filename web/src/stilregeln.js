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
 *   deren Herkunft fehlt, die Text oder Umriss färbt oder die das Markup setzt
 *   (`style:--extern-…` oder `style="--extern-…: …"`): Dort stünde sie ohne
 *   Herkunft, darum wählt das Markup eine externe Farbe nur per Klasse,
 * - einen Namen der Farbordnung, den eine Datei außerhalb von app.css neu setzt
 *   (`.karte { --b-ok: … }` würde alle Bedeutungen darunter still umfärben).
 *
 * Durch gehen: die Namen der Liste, die Vorsilben, und eigene Namen, die in
 * derselben Datei deklariert sind (im Stil oder im Markup über `style:--name`).
 * Ein eigener Name ist nur, was nicht schon der Farbordnung gehört.
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
/** Die Töne setzt eine Klasse `b-…`; ein Baustein darf sie für sich neu setzen. */
const TON = /^--ton(-soft|-text|-auf)?$/;
const farbrollen = /** @type {readonly string[]} */ (FARBROLLEN);

/**
 * Ob ein Name der Farbordnung gehört und darum kein eigener Name einer Datei
 * sein kann: ein Ton der Bedeutungen, eine Farbrolle außer `--ton…`, ein alter
 * Name oder ein Name mit Vorsilbe.
 * @param {string} name
 * @returns {boolean}
 */
function gehoertDerOrdnung(name) {
	return (
		name.startsWith('--b-') ||
		(farbrollen.includes(name) && !TON.test(name)) ||
		alt.has(name) ||
		VORSILBEN.some((v) => name.startsWith(v))
	);
}

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
	/** @param {string} name */
	externMarkup: (name) =>
		`Stil: Die externe Vorgabe „${name}“ setzt das Markup. Dort stünde sie ohne Herkunft: im Stil des Bausteins deklarieren (${FORM}) und im Markup per Klasse wählen.`,
	/** @param {string} name @param {string} ersatz */
	ordnung: (name, ersatz) =>
		`Stil: „${name}“ gehört zur Farbordnung und wird nur in app.css gesetzt. Nimm stattdessen: ${ersatz}.`,
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
		// Eine externe Vorgabe zählt nur aus einem Stilblock, nie aus `style="…"`.
		if (d.prop.startsWith(EXTERN) ? !imMarkup(d) : !gehoertDerOrdnung(d.prop)) eigene.add(d.prop);
	});
	for (const { name } of direktiven(wurzel)) {
		if (!gehoertDerOrdnung(name)) eigene.add(name);
	}
	return eigene;
}

/**
 * Ob eine Deklaration in einem `style="…"` des Markups steht.
 * @param {import('postcss').Declaration} d
 * @returns {boolean}
 */
function imMarkup(d) {
	const wurzel = /** @type {{ source?: { inline?: boolean } }} */ (d.root());
	return wurzel.source?.inline === true;
}

/**
 * Die Namen, die das Markup der Datei über `style:--name` setzt, je mit ihrer
 * Zeile in der Datei.
 * @param {import('postcss').Root} wurzel
 * @returns {{ name: string, zeile: number }[]}
 */
function direktiven(wurzel) {
	const mit = /** @type {{ document?: import('postcss').Document }} */ (wurzel);
	const quelle = (mit.document ?? wurzel).source?.input.css ?? '';
	/** @type {{ name: string, zeile: number }[]} */
	const treffer = [];
	for (const t of quelle.matchAll(/\bstyle:(--[\w-]+)/g)) {
		if (t[1] === undefined) continue;
		treffer.push({ name: t[1], zeile: quelle.slice(0, t.index).split('\n').length });
	}
	return treffer;
}

/**
 * Ob diese Wurzel die erste der Datei ist. Was die ganze Datei betrifft, wird
 * nur dort gemeldet, sonst stünde es je Stilblock einmal da.
 * @param {import('postcss').Root} wurzel
 * @returns {boolean}
 */
function istErste(wurzel) {
	const mit = /** @type {{ document?: import('postcss').Document }} */ (wurzel);
	return mit.document === undefined || mit.document.first === wurzel;
}

/**
 * Was statt eines Namens der Farbordnung zu nehmen ist.
 * @param {string} name
 * @returns {string}
 */
function ersatzFuerOrdnung(name) {
	if (name.startsWith('--b-')) return ersatzFuerBedeutung(name);
	const neu = alt.get(name);
	if (neu?.startsWith('Klasse') === true) return neu;
	return 'einen eigenen Namen, dessen Wert die Farbordnung nennt';
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
	// Was das Markup über `style:--extern-…` setzt, je Treffer einmal.
	if (istErste(wurzel)) {
		for (const { name, zeile } of direktiven(wurzel)) {
			if (!name.startsWith(EXTERN)) continue;
			stylelint.utils.report({
				ruleName: BEKANNT,
				result: ergebnis,
				node: wurzel,
				line: zeile,
				message: meldungen.externMarkup(name)
			});
		}
	}
	wurzel.walkDecls((d) => {
		if (d.prop.startsWith(EXTERN)) {
			if (imMarkup(d)) melde(d, meldungen.externMarkup(d.prop));
			else if (!hatHerkunft(d)) melde(d, meldungen.externHerkunft(d.prop));
		} else if (d.prop.startsWith('--') && gehoertDerOrdnung(d.prop)) {
			melde(d, meldungen.ordnung(d.prop, ersatzFuerOrdnung(d.prop)));
		}
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
