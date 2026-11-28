/**
 * Anzeigezone (Kalender, Stufe 7b): die IANA-Zone des Blickorts aus dem Kopf
 * (GET /api/kalender/kopf). `null` heißt Gerätezone. Gespeicherte Zeitpunkte
 * bleiben, nur die Anzeige rechnet um. Den Wert setzt nur der gemeinsame
 * Himmel-Laden (himmel.svelte.ts); Abmelden und eine fehlgeschlagene Anfrage
 * setzen `null`. Die Formatierer lesen ihn bei jedem Aufruf, also reaktiv.
 */
let zone = $state<string | null>(null);

/** Die Zone, in der der Browser Uhrzeiten und Tage zeigt; `null`: Gerätezone. */
export function anzeigeZone(): string | null {
	return zone;
}

/** Setzt die Anzeigezone (nur der Himmel-Laden). */
export function zoneSetzen(z: string | null): void {
	zone = z;
}

/** Kalenderteile eines Zeitpunkts; `monat` 1 bis 12, `wochentag` 0 (Sonntag) bis 6. */
export interface ZeitTeile {
	jahr: number;
	monat: number;
	tag: number;
	stunde: number;
	minute: number;
	sekunde: number;
	wochentag: number;
}

// Ein Gedächtnis ohne Prototyp, an dem nichts zeichnet; so ist auch „constructor“ kein Treffer.
const formatierer = Object.create(null) as Record<string, Intl.DateTimeFormat | null | undefined>;

/** Der Formatierer der Anzeigezone; `null` für die Gerätezone oder eine Zone, die der Browser nicht kennt. */
function zonenFormatierer(): Intl.DateTimeFormat | null {
	const z = zone;
	if (z === null) return null;
	let f = formatierer[z];
	if (f === undefined) {
		try {
			f = new Intl.DateTimeFormat('en-US', {
				timeZone: z,
				hourCycle: 'h23',
				year: 'numeric',
				month: 'numeric',
				day: 'numeric',
				hour: 'numeric',
				minute: 'numeric',
				second: 'numeric'
			});
		} catch {
			// Eine Zone, die dieser Browser nicht kennt, darf keine Seite sprengen: dann Gerätezeit.
			f = null;
		}
		formatierer[z] = f;
	}
	return f;
}

/**
 * `{ timeZone }` für `toLocale…String`, leer in der Gerätezone. Liest die
 * Zone bei jedem Aufruf, also reaktiv.
 */
export function zoneOption(): { timeZone?: string } {
	const f = zonenFormatierer();
	return f === null ? {} : { timeZone: f.resolvedOptions().timeZone };
}

/** Die Kalenderteile von `d` in der Anzeigezone. */
export function teileInZone(d: Date): ZeitTeile {
	const f = zonenFormatierer();
	if (f === null) {
		return {
			jahr: d.getFullYear(),
			monat: d.getMonth() + 1,
			tag: d.getDate(),
			stunde: d.getHours(),
			minute: d.getMinutes(),
			sekunde: d.getSeconds(),
			wochentag: d.getDay()
		};
	}
	const teile = f.formatToParts(d);
	const teil = (typ: Intl.DateTimeFormatPartTypes): number =>
		Number(teile.find((p) => p.type === typ)?.value ?? Number.NaN);
	const jahr = teil('year');
	const monat = teil('month');
	const tag = teil('day');
	return {
		jahr,
		monat,
		tag,
		stunde: teil('hour'),
		minute: teil('minute'),
		sekunde: teil('second'),
		// Der 1.1.1970 war ein Donnerstag (4).
		wochentag: (((Math.floor(Date.UTC(jahr, monat - 1, tag) / 86_400_000) + 4) % 7) + 7) % 7
	};
}

function zwei(n: number): string {
	return String(n).padStart(2, '0');
}

/** Der Tag von `d` in der Anzeigezone als JJJJ-MM-TT (für die Anzeige; `tagText` bleibt Gerätetag). */
export function tagInZone(d: Date): string {
	const t = teileInZone(d);
	return `${String(t.jahr)}-${zwei(t.monat)}-${zwei(t.tag)}`;
}

/** Die Uhrzeit von `d` in der Anzeigezone, „09:05“. */
export function uhrInZone(d: Date): string {
	const t = teileInZone(d);
	return `${zwei(t.stunde)}:${zwei(t.minute)}`;
}
