/**
 * Neue Fassung der Oberfläche (Ereignis `fassung`, Etappe 4b): die Regeln,
 * ohne Svelte und ohne `$app`, damit sie sich einzeln prüfen lassen. Den
 * Zustand und den Hinweis hält `fassung.svelte.ts`.
 *
 * Nach jedem grünen Bau liefert der Dienst aus einem neuen Ordner
 * `web/builds/<ordner>` aus. Die Teile des alten Baus (`/_app/immutable/…`)
 * gibt es dann nicht mehr (404). Ein offener Tab lädt darum ganz neu
 * (Grundsatz: Niemand muss etwas anklicken; die App aktualisiert
 * sich selbst). Am Live-Punkt steht
 * dazu höchstens einer von drei Zuständen (`lageVon`, Vorrang 1 vor 3 vor 2),
 * jeder als Information, nie als Aufforderung, ohne Warnfarbe:
 *
 * 1. Keine Verbindung (Live-Kanal getrennt): „Keine Verbindung zum Dienst,
 *    Daten können veraltet sein“. Nie neu laden, auch nicht von selbst.
 * 3. Neue Fassung wird gebaut (Ereignis `bau` mit `laeuft`, ohne Anmeldung
 *    `bau.zustand` aus `/api/projects`):
 *    „Daten live, neue Fassung wird gebaut“. Kein Neuladen.
 * 2. App veraltet (Ereignis `fassung` mit neuerer Bau-Kennung als beim
 *    Laden, `neuere`): „Daten live, App wird gleich aktualisiert“. Nach
 *    `SELBST_NEU_MS` lädt die Seite von selbst neu (`selbstNeuLaden`, der
 *    Regelweg); ein Klick lädt sofort (Abkürzung), aber nie über einen
 *    Sperrgrund hinweg. Ist etwas offen, das
 *    verloren ginge (`neuLadenAufschieben`), heißt es „App wird
 *    aktualisiert, sobald du fertig bist“, und es wird danach neu versucht.
 *    Bis dahin lädt der nächste Klick auf einen Link zu einer anderen Route
 *    die neue Fassung (volle Navigation zum Ziel).
 *
 * Nur TOML geändert: kein Bau, keine Fassung, also kein Zustand.
 *
 * Schlägt trotzdem ein Teil des alten Baus fehl (`istChunkFehler`), lädt die
 * Seite einmal neu statt eine Fehlerseite zu zeigen. Damit daraus keine
 * Schleife wird, merkt sie sich das in sessionStorage (`darfNeuLaden`).
 */

/** Schlüssel in sessionStorage (je Tab). */
export const MERKER = 'dashboard:fassung-neu-geladen';
/** So lange nach einem Neuladen gibt es kein weiteres (gegen Schleifen). */
export const SPERRE_MS = 10_000;
/** Aus derselben Fassung noch einmal neu laden erst nach dieser Zeit. */
export const WIEDER_MS = 5 * 60_000;

/** Wann und aus welcher Fassung (SvelteKit-`version`) zuletzt neu geladen wurde. */
export interface Merker {
	zeit: number;
	von: string;
}

/**
 * Darf die Seite jetzt von selbst neu laden?
 *
 * - noch nie in diesem Tab: ja;
 * - vor weniger als `SPERRE_MS`: nein (was gleich nach dem Laden wieder
 *   scheitert, hilft kein zweites Neuladen);
 * - aus derselben Fassung schon einmal: erst nach `WIEDER_MS` wieder
 *   (Neuladen brachte keine neue Fassung; dann zeigt die Seite den Fehler);
 * - sonst ja (eine neuere Fassung ist inzwischen wieder veraltet).
 */
export function darfNeuLaden(merker: Merker | null, jetzt: number, version: string): boolean {
	if (merker === null) return true;
	const her = jetzt - merker.zeit;
	if (her >= 0 && her < SPERRE_MS) return false;
	if (merker.von === version && her >= 0 && her < WIEDER_MS) return false;
	return true;
}

/** Liest den Merker; Unlesbares (oder kein Speicher) gilt als keiner. */
export function merkerLesen(speicher: Pick<Storage, 'getItem'> | null): Merker | null {
	try {
		const roh = speicher?.getItem(MERKER) ?? null;
		if (roh === null) return null;
		const m = JSON.parse(roh) as Partial<Merker> | null;
		return typeof m?.zeit === 'number' && typeof m.von === 'string'
			? { zeit: m.zeit, von: m.von }
			: null;
	} catch {
		return null;
	}
}

/**
 * Ein Teil des Baus ließ sich nicht laden (dynamischer Import, Modul-Skript
 * mit 404, CSS eines Teils). Die Sätze der Browser: Chrome und Edge „Failed
 * to fetch dynamically imported module“, Firefox „error loading dynamically
 * imported module“, Safari „Importing a module script failed“, Vite „Unable
 * to preload CSS“. Ein gewöhnlicher Netzfehler („Failed to fetch“) zählt
 * nicht; den meldet die Seite als „keine Verbindung“.
 */
export function istChunkFehler(e: unknown): boolean {
	const satz =
		e instanceof Error
			? e.message
			: typeof e === 'object' && e !== null && 'message' in e && typeof e.message === 'string'
				? e.message
				: typeof e === 'string'
					? e
					: '';
	return /dynamically imported module|importing a module script failed|unable to preload css/i.test(
		satz
	);
}

/** Eingabefelder, in denen man schreibt (nicht Knöpfe, Häkchen oder Regler). */
const KEIN_SCHREIBFELD = new Set([
	'button',
	'submit',
	'reset',
	'checkbox',
	'radio',
	'range',
	'color',
	'file',
	'image',
	'hidden'
]);

function istSchreibfeld(el: Element): boolean {
	const name = el.tagName.toLowerCase();
	if (name === 'textarea' || name === 'select') return true;
	if (name === 'input') return !KEIN_SCHREIBFELD.has((el as HTMLInputElement).type.toLowerCase());
	return el.closest('[contenteditable]:not([contenteditable="false"])') !== null;
}

/** Felder in einem offenen Dialog oder Popover, die auf Eingabe hindeuten. */
const FORMULAR = 'form, input, textarea, select, [contenteditable]:not([contenteditable="false"])';
/** Offene Dialoge (auch die von Bits UI, ein `div` mit Rolle) und Popover. */
const DIALOGE = ['dialog[open]', '[role="dialog"]', '[role="alertdialog"]', ':popover-open'];

/**
 * Ist gerade eine Eingabe offen? Dann lädt die Seite nicht von selbst neu.
 *
 * - ein Schreibfeld hat den Fokus (input, textarea, select, contenteditable);
 * - ein offener Dialog oder Popover enthält ein Formular oder Felder.
 */
export function eingabeOffen(doc: Document): boolean {
	const aktiv = doc.activeElement;
	if (aktiv !== null && aktiv !== doc.body && istSchreibfeld(aktiv)) return true;
	for (const auswahl of DIALOGE) {
		let offen: NodeListOf<Element>;
		try {
			offen = doc.querySelectorAll(auswahl);
		} catch {
			// Ältere Engines (und jsdom) kennen :popover-open nicht.
			continue;
		}
		for (const d of offen) if (d.querySelector(FORMULAR) !== null) return true;
	}
	return false;
}

/** Offene Dialoge, Popover oder Schubladen: auch ohne Felder darin nicht wegladen. */
function dialogOffen(doc: Document): boolean {
	for (const auswahl of DIALOGE) {
		try {
			if (doc.querySelector(auswahl) !== null) return true;
		} catch {
			// Ältere Engines (und jsdom) kennen :popover-open nicht.
		}
	}
	return false;
}

/**
 * Etwas wird gerade gesendet oder bestätigt: `aria-busy` an Formular,
 * Antwortgruppe oder Knopf (etwa eine laufende Passkey-Abfrage auf
 * /anmelden), oder eine offene Rückfrage mit `data-sperrt-neuladen` (etwa
 * „Freigabe bestätigen“ in `PasskeyVerwaltung.svelte`).
 */
const SENDET =
	'form[aria-busy="true"], [role="group"][aria-busy="true"], button[aria-busy="true"], [data-sperrt-neuladen]';

/**
 * Das erste ungesicherte Element, oder `null`. Ein Textfeld mit Text
 * (Antwort, Anmerkung, „Etwas anderes …“), ein ausgefülltes Feld in einem
 * Formular (etwa der Name des Geräts beim Passkey) oder etwas, das gerade
 * gesendet oder bestätigt wird (`SENDET`). Suchfelder außerhalb von
 * Formularen zählen nicht: Eine Suche ist kein Verlust. Ebenso wenig ein
 * Feld mit `data-entwurf-gesichert`: Sein Entwurf steht schon in
 * sessionStorage und überlebt das Neuladen („Etwas anderes …“).
 */
export function ungesichert(doc: Document): HTMLElement | null {
	const sendet = doc.querySelector<HTMLElement>(SENDET);
	if (sendet !== null) return sendet;
	for (const t of doc.querySelectorAll<HTMLTextAreaElement>(
		'textarea:not([data-entwurf-gesichert])'
	))
		if (t.value.trim() !== '') return t;
	for (const f of doc.querySelectorAll<HTMLInputElement>(
		'form input:not([data-entwurf-gesichert])'
	)) {
		if (f.readOnly || f.disabled || KEIN_SCHREIBFELD.has(f.type.toLowerCase())) continue;
		if (f.value !== '') return f;
	}
	return null;
}

/** Steht irgendwo etwas Ungesichertes (`ungesichert`)? */
export function ungesichertOffen(doc: Document): boolean {
	return ungesichert(doc) !== null;
}

/**
 * Soll das Neuladen von selbst warten? Ja, solange der Benutzer schreibt
 * (`eingabeOffen`), ein Dialog offen ist oder etwas Ungesichertes dasteht.
 */
export function neuLadenAufschieben(doc: Document): boolean {
	return eingabeOffen(doc) || dialogOffen(doc) || ungesichertOffen(doc);
}

/** Die drei Zustände am Live-Punkt; `null`: keiner, der Live-Punkt wie sonst. */
export type Lage = 'keine_verbindung' | 'baut' | 'veraltet';

/** Was am Live-Punkt steht (und einmal angesagt wird). Information, keine Aufforderung. */
export const LAGE_TEXT: Record<Lage, string> = {
	keine_verbindung: 'Keine Verbindung zum Dienst, Daten können veraltet sein',
	baut: 'Daten live, neue Fassung wird gebaut',
	veraltet: 'Daten live, App wird gleich aktualisiert'
};
/** Zustand 2, solange ein Sperrgrund das Neuladen aufhält (`neuLadenAufschieben`). */
export const TEXT_AUFGEHALTEN = 'App wird aktualisiert, sobald du fertig bist';
/** Tooltip am Knopf in Zustand 2: sagt, was geschieht, fordert nichts. */
export const TITEL_APP_VERALTET = 'Die neue Fassung lädt von selbst; ein Klick lädt sie sofort.';

/**
 * Welcher Zustand gilt? Vorrang 1 (keine Verbindung) vor 3 (Bau läuft) vor 2
 * (App veraltet): Ohne Verbindung stimmt „Daten live“ nicht, und mitten in
 * einer Folge von Bauten soll nichts neu laden. `verbunden` ist `null`,
 * solange der erste Versuch läuft (dann keiner, nichts wäre sicher).
 */
export function lageVon(verbunden: boolean | null, baut: boolean, veraltet: boolean): Lage | null {
	if (verbunden === null) return null;
	if (!verbunden) return 'keine_verbindung';
	if (baut) return 'baut';
	return veraltet ? 'veraltet' : null;
}

/**
 * Zeitpunkt einer Bau-Kennung in Millisekunden, oder `null`, wenn sie keine
 * kennt. Zwei Formen: der Ordner des Boten (`web/builds/<JJJJMMTT-hhmmss>`,
 * bei mehreren in derselben Sekunde mit `-2`, `-3` …; Ortszeit, beim Start
 * des Baus) aus dem Ereignis `fassung`, und SvelteKits `version` des Tabs
 * (Millisekunden, wenn Vite den Bau beginnt, also nach dem Namen des Ordners),
 * mit dem Abdruck des Grundbaus davor (`<16 Hex>-<ms>`, scripts/grundbau.mjs).
 */
export function kennungZeit(k: string): number | null {
	const o = /^(\d{4})(\d{2})(\d{2})-(\d{2})(\d{2})(\d{2})(?:-(\d{1,3}))?$/.exec(k);
	if (o !== null) {
		const [j, mo, t, h, mi, s, n] = o.slice(1).map(Number);
		// shortcut: Ortszeit, in der doppelten Herbststunde bis zu 1 h kein Hinweis; aufrüsten mit UTC oder Vite-version im Ereignis fassung
		const ms = new Date(j ?? 0, (mo ?? 1) - 1, t, h, mi, s).getTime();
		// Die Nummer ordnet Ordner derselben Sekunde (höchstens 999, `bote::bau::neuer_name`).
		return Number.isNaN(ms) ? null : ms + (Number.isNaN(n) ? 0 : (n ?? 1) - 1);
	}
	const v = /^(?:[0-9a-f]{16}-)?(\d{10,})$/.exec(k);
	return v === null ? null : Number(v[1]);
}

/**
 * Ist `kennung` (aus dem Ereignis `fassung`) neuer als `eigene` (die Fassung
 * beim Laden)? Gleich, älter oder unlesbar: nein. Eine nur „andere“ Kennung
 * reicht nicht (darum nicht `updated.check()` von SvelteKit).
 */
export function neuere(kennung: string, eigene: string): boolean {
	const neu = kennungZeit(kennung);
	const alt = kennungZeit(eigene);
	return neu !== null && alt !== null && neu > alt;
}

/** So lange nach dem Erkennen lädt die Seite von selbst neu. */
export const SELBST_NEU_MS = 30_000;
/**
 * So lange nach dem Ende eines Sperrgrunds („sobald du fertig bist“): kurz,
 * aber nicht sofort, damit etwa der Wechsel von einem Feld zum nächsten
 * nicht schon neu lädt.
 */
export const NACH_SPERRE_MS = 5_000;
/** Ohne Antwort der API bei stehendem Kanal: so oft fragt die Seite nach (`FassungWache.svelte`). */
export const NACHFRAGE_MS = 5_000;
/** So lange wartet die Wache auf `/api/stand`; danach gilt: keine Antwort (Zustand 1). */
export const STAND_FRIST_MS = 5_000;
/** So oft schaut die Seite nach, ob die Eingabe vorbei ist. */
export const PRUEF_MS = 1_000;

/** Was `selbstNeuLaden` braucht; in Tests ersetzbar. */
export interface SelbstNeuLaden {
	/** Etwas ist offen, das verloren ginge (`neuLadenAufschieben`). */
	aufschieben: () => boolean;
	/** Der Tab ist im Hintergrund. */
	verborgen: () => boolean;
	/** Meldet einmal, dass der Tab wieder sichtbar ist; gibt die Abmeldung zurück. */
	beiSichtbar: (f: () => void) => () => void;
	/**
	 * Lädt neu; `false`, wenn es gerade nicht darf (Schutz vor Schleifen).
	 * Als Promise, wenn es vorher fragt, ob der Dienst antwortet.
	 */
	laden: () => boolean | Promise<boolean>;
	/** Verzögerung; sonst `SELBST_NEU_MS`. */
	warten?: number;
	/** Meldet, ob ein Sperrgrund das fällige Neuladen gerade aufhält (für den Text). */
	aufgehalten?: (ja: boolean) => void;
}

/**
 * Lädt nach `warten` (30 s) von selbst neu. Ist dann etwas offen
 * (`aufschieben`), meldet es `aufgehalten(true)`, schaut jede Sekunde nach
 * und lädt, wenn es vorbei ist (`aufgehalten(false)`), nach `NACH_SPERRE_MS`
 * neu (vorher noch einmal geprüft). Im Hintergrund wartet es, bis der Tab wieder
 * sichtbar ist, und lädt dann gleich (wenn nichts offen ist). Darf es gerade
 * nicht (`laden` gibt `false`), versucht es nach `warten` noch einmal.
 * Gibt die Abmeldung zurück (räumt Uhr und Hörer auf).
 */
export function selbstNeuLaden(o: SelbstNeuLaden): () => void {
	const warten = o.warten ?? SELBST_NEU_MS;
	let uhr: ReturnType<typeof setTimeout> | undefined;
	let offSichtbar: (() => void) | null = null;
	let aus = false;

	const stellen = (ms: number, f: () => void): void => {
		clearTimeout(uhr);
		uhr = setTimeout(f, ms);
	};
	const verzoegern = (): void => {
		stellen(warten, faellig);
	};
	/** Etwas war offen: nachschauen, bis es vorbei ist, dann kurz warten. */
	const vorbei = (): void => {
		if (aus) return;
		if (o.aufschieben()) {
			stellen(PRUEF_MS, vorbei);
			return;
		}
		o.aufgehalten?.(false);
		stellen(NACH_SPERRE_MS, faellig);
	};
	const aufhalten = (): void => {
		o.aufgehalten?.(true);
		stellen(PRUEF_MS, vorbei);
	};
	/**
	 * Nicht geladen: Ist inzwischen etwas offen (während `laden` fragte), aufgehalten,
	 * sonst später (dann hält nichts auf, auch nicht ein Klick vorher, `jetztLaden`).
	 */
	const nichtGeladen = (): void => {
		if (aus) return;
		if (o.aufschieben()) {
			aufhalten();
			return;
		}
		o.aufgehalten?.(false);
		verzoegern();
	};
	const jetzt = (): void => {
		if (o.aufschieben()) {
			aufhalten();
			return;
		}
		const r = o.laden();
		if (r === false) nichtGeladen();
		else if (r !== true)
			void r.then((ok) => {
				if (!ok) nichtGeladen();
			});
	};
	function faellig(): void {
		if (aus) return;
		if (!o.verborgen()) {
			jetzt();
			return;
		}
		offSichtbar ??= o.beiSichtbar(() => {
			offSichtbar?.();
			offSichtbar = null;
			if (!aus) jetzt();
		});
	}

	verzoegern();
	return () => {
		aus = true;
		clearTimeout(uhr);
		offSichtbar?.();
		offSichtbar = null;
	};
}

/** Schlüssel in sessionStorage: wo der Fokus vor dem Neuladen stand (je Tab, einmal). */
export const FOKUS_MERKER = 'dashboard:fassung-fokus';
/** Merker, wenn der Fokus am Live-Punkt stand (`data-live-punkt` in `LiveBadge.svelte`). */
export const LIVE_PUNKT = 'Live-Punkt';
type Speicher = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;

/**
 * Vor dem Neuladen: wo der Fokus steht. Am Live-Punkt `LIVE_PUNKT`, sonst
 * die ID des aktiven Elements, ohne ID ein leerer Merker (danach `main`).
 */
export function fokusMerken(doc: Document, speicher: Speicher | null): void {
	const a = doc.activeElement;
	const wert =
		a === null || a === doc.body ? '' : a.closest('[data-live-punkt]') !== null ? LIVE_PUNKT : a.id;
	try {
		speicher?.setItem(FOKUS_MERKER, wert);
	} catch {
		// Ohne Speicher steht der Fokus nach dem Neuladen, wo der Browser ihn hinsetzt.
	}
}

/**
 * Nach dem Laden, einmal: Gab es einen Merker (`fokusMerken`), Fokus dorthin,
 * sonst auf `main`. Ohne Merker (gewöhnliches Laden) nichts. Sagt nichts an.
 */
export function fokusZurueck(doc: Document, speicher: Speicher | null): void {
	let wert: string | null;
	try {
		wert = speicher?.getItem(FOKUS_MERKER) ?? null;
		speicher?.removeItem(FOKUS_MERKER);
	} catch {
		return;
	}
	if (wert === null) return;
	const ziel =
		wert === LIVE_PUNKT
			? doc.querySelector<HTMLElement>('[data-live-punkt] .live')
			: wert === ''
				? null
				: doc.getElementById(wert);
	(ziel ?? doc.querySelector<HTMLElement>('main'))?.focus();
}

/** Was das Neuladen braucht; in Tests ersetzbar. */
export interface Umgebung {
	speicher: () => Speicher | null;
	jetzt: () => number;
	/** Die Seite ganz neu laden. */
	neuLaden: () => void;
	/** Volle Navigation (am Router vorbei) zu `href`. */
	gehe: (href: string) => void;
}

export const umgebung: Umgebung = {
	speicher: () => {
		try {
			return typeof sessionStorage === 'undefined' ? null : sessionStorage;
		} catch {
			return null;
		}
	},
	jetzt: () => Date.now(),
	neuLaden: () => {
		fokusMerken(document, umgebung.speicher());
		location.reload();
	},
	gehe: (href) => {
		location.assign(href);
	}
};

/** Läuft das Neuladen schon (in dieser Seite)? Dann gilt jeder weitere Ruf als erledigt. */
let laeuft = false;

/**
 * Lädt die Seite neu, wenn `darfNeuLaden` es erlaubt, und merkt sich das.
 * `true`: lädt neu (oder lud schon), die Seite soll keinen Fehler zeigen.
 * `false`: nicht erlaubt; die Seite zeigt, was sie sonst zeigen würde.
 */
export function einmalNeuLaden(version: string, u: Umgebung = umgebung): boolean {
	if (laeuft) return true;
	const speicher = u.speicher();
	const jetzt = u.jetzt();
	if (!darfNeuLaden(merkerLesen(speicher), jetzt, version)) return false;
	try {
		speicher?.setItem(MERKER, JSON.stringify({ zeit: jetzt, von: version } satisfies Merker));
	} catch {
		// Ohne Speicher kein Schutz über das Neuladen hinweg; dann lieber nicht.
		return false;
	}
	if (speicher === null) return false;
	laeuft = true;
	u.neuLaden();
	return true;
}

/** Nur für Tests: als wäre die Seite frisch geladen. */
export function zuruecksetzen(): void {
	laeuft = false;
}
