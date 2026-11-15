/**
 * Schritt E2b: Ein Teil des Baus ließ sich nicht laden (`istChunkFehler`).
 *
 * Die Teile eigener Projektseiten und die Bauliste sind geschützt: Ohne
 * Anmeldung (Sitzung abgelaufen, Passkey widerrufen) antwortet der Dienst 401.
 * Der Browser meldet 401 und 404 mit demselben Satz („Failed to fetch
 * dynamically imported module“). Darum fragt die Seite erst, ob die Anmeldung
 * noch gilt (`zugang`, frisch statt des gemerkten Stands): Fehlt sie, geht es
 * zur Anmeldung mit dem Weg zurück (`anmeldenAdresse`), nie zum Neuladen; sonst
 * wie bisher ein Teil eines alten Baus (`fassung.chunkFehler`, mit Schutz vor
 * Schleifen). Nie, solange etwas offen ist (`eingabeDa`, strenger als beim
 * Neuladen der Fassung): Dann heißt es „abgelaufen“ (die Seite bleibt mit dem
 * ungesicherten Text stehen, der Aufrufer zeigt den Hinweis
 * `AnmeldungAbgelaufen`: Anmeldung in neuem Tab, danach neu versuchen) oder,
 * mit gültiger Anmeldung, „fehler“ ohne Neuladen. Ist ein fremdes Windows-Konto
 * angemeldet, heißt es statt „abgelaufen“ „fremd“ (der Hinweis nennt das Konto).
 */
import { tick } from 'svelte';
import { istChunkFehler, neuLadenAufschieben } from './fassung.ts';
import { fassung } from './fassung.svelte.ts';
import { anmeldenAdresse, zugang, type Zugang } from './zugang.ts';

export interface Hilfen {
	zugang: Pick<Zugang, 'vergessen' | 'pruefen'>;
	/** Volle Navigation zur Anmeldung (der neue Anmeldestand gilt danach von vorn). */
	zurAnmeldung: (adresse: string) => void;
	/** Ein Teil eines alten Baus: einmal neu laden (true, wenn es neu lädt). */
	neuLaden: () => boolean;
	/** Ist etwas offen, das ein Wegnavigieren verlöre (`eingabeDa`)? */
	offen: () => boolean;
	ort: () => Pick<URL, 'pathname' | 'search'>;
}

/** Felder ohne Eingabe zum Schreiben (Knöpfe, versteckte Felder). */
const KEIN_FELD = new Set(['button', 'submit', 'reset', 'image', 'hidden']);

/**
 * Ein Feld irgendwo auf der Seite weicht von seinem Startwert ab, auch
 * außerhalb eines Formulars (`input`, auch eine gewählte Datei, `textarea`,
 * `select`), oder ein Bereich mit `contenteditable` hat Text. Strenger als `ungesichert` (dort zählen
 * Suchfelder ohne Formular nicht), weil hier die Navigation zur Anmeldung den
 * Text verwürfe. Mit `bind:value` vorbelegte Felder gelten als geändert; dann
 * kommt der Hinweis statt der Navigation, nie umgekehrt.
 */
export function eingabeGeaendert(doc: Document): boolean {
	for (const f of doc.querySelectorAll<HTMLInputElement | HTMLTextAreaElement>('input, textarea')) {
		if (f.readOnly || f.disabled || KEIN_FELD.has(f.type.toLowerCase())) continue;
		const auswahl = f instanceof HTMLInputElement && (f.type === 'checkbox' || f.type === 'radio');
		if (auswahl && f.checked !== f.defaultChecked) return true;
		// Eine gewählte Datei: `value` ist dann ein Pfad, `files` sicher.
		if (f instanceof HTMLInputElement && f.type === 'file') {
			if ((f.files?.length ?? 0) > 0) return true;
		} else if (!auswahl && f.value !== f.defaultValue) return true;
	}
	for (const s of doc.querySelectorAll('select')) {
		const o = [...s.options];
		if (o.length === 0 || s.disabled) continue;
		// Ohne `selected` im Markup wählt der Browser bei einfacher Auswahl die erste Option.
		const start = s.multiple || o.some((x) => x.defaultSelected);
		if (o.some((x, i) => x.selected !== (start ? x.defaultSelected : i === 0))) return true;
	}
	for (const c of doc.querySelectorAll('[contenteditable]:not([contenteditable="false"])'))
		if (c.textContent.trim() !== '') return true;
	return false;
}

/**
 * Erscheint der haftende Hinweis oder das Fehlerbild: nach dem Zeichnen das
 * Element mit dem Fokus ins Bild holen (scroll-padding hält es über dem
 * Hinweis), ohne den Fokus zu bewegen. Ohne Fokus (body) rollt nichts: Mit
 * scroll-padding risse body.scrollIntoView die Seite nach oben. jsdom kennt
 * scrollIntoView nicht.
 */
export async function fokusZeigen(): Promise<void> {
	await tick();
	const el = document.activeElement;
	if (!el || el === document.body || el === document.documentElement) return;
	if (typeof el.scrollIntoView === 'function') el.scrollIntoView({ block: 'nearest' });
}

/** Offen für den Fall einer fehlenden Anmeldung: wie beim Neuladen oder ein geändertes Feld. */
export function eingabeDa(doc: Document): boolean {
	return neuLadenAufschieben(doc) || eingabeGeaendert(doc);
}

const HILFEN: Hilfen = {
	zugang,
	zurAnmeldung: (adresse) => {
		window.location.assign(adresse);
	},
	neuLaden: () => fassung.chunkFehler(),
	offen: () => eingabeDa(document),
	ort: () => window.location
};

/**
 * „weiter“: Die Seite geht selbst weiter (zur Anmeldung oder Neuladen), der
 * Aufrufer zeigt nichts. „abgelaufen“: Anmeldung weg, aber etwas offen; die
 * Seite bleibt, der Aufrufer zeigt `AnmeldungAbgelaufen`; „fremd“ ebenso, mit
 * dem Satz für ein fremdes Konto. „fehler“: das Fehlerbild; ist etwas offen,
 * ohne Neuladen (der Aufrufer lässt die Seite dann stehen, `eingabeDa`).
 */
export async function teilFehlt(
	e: unknown,
	h: Hilfen = HILFEN
): Promise<'weiter' | 'abgelaufen' | 'fremd' | 'fehler'> {
	if (!istChunkFehler(e)) return 'fehler';
	h.zugang.vergessen();
	const stand = await h.zugang.pruefen();
	if (stand === 'fehlt' || stand === 'fremd') {
		if (h.offen()) return stand === 'fremd' ? 'fremd' : 'abgelaufen';
		h.zurAnmeldung(anmeldenAdresse(h.ort()));
		return 'weiter';
	}
	if (h.offen()) return 'fehler';
	return h.neuLaden() ? 'weiter' : 'fehler';
}
