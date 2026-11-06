/**
 * Die gemeinsame Zeitachse (Etappe 3a-1) für Dichtestreifen und Gantt:
 * Zeit zu Position, Zoomstufen Tag, Woche, Monat und Jahr, Marken je Stufe
 * und Beschriftung ohne Überlappung. Reine Rechnung, ohne DOM.
 *
 * Einheit ist die Tageszahl (Tage seit dem 1.1.1970, siehe `tage.ts`); bei
 * Plänen ohne Datum (`plan`) sind es Plan-Monate (0, 1, 2 …).
 */
import { MONATE_KURZ, MS_TAG, tagAusZahl, tagKurz, tagZahl } from './tage.ts';

/** Ein Ausschnitt der Achse: von bis bis (bis ausschließlich). */
export interface Fenster {
	von: number;
	bis: number;
}

export type Stufe = 'tag' | 'woche' | 'monat' | 'jahr';

/** Von fein nach grob. */
export const STUFEN: readonly Stufe[] = ['tag', 'woche', 'monat', 'jahr'];

export const STUFEN_NAME: Record<Stufe, string> = {
	tag: 'Tag',
	woche: 'Woche',
	monat: 'Monat',
	jahr: 'Jahr'
};

/** Pixel je Tag, wenn eine Stufe gewählt ist; daraus folgt die Länge des Ausschnitts. */
export const PX_JE_TAG: Record<Stufe, number> = { tag: 44, woche: 12, monat: 3, jahr: 0.6 };

/** Bei Plan-Monaten: Pixel je Monat für die Stufen Monat und Jahr. */
export const PX_JE_PLANMONAT: Partial<Record<Stufe, number>> = { monat: 90, jahr: 24 };

export interface Marke {
	at: number;
	text: string;
	/** Hauptmarke (Monats- oder Jahreswechsel): kräftiger, wird zuerst beschriftet. */
	haupt: boolean;
}

export interface Beschriftung extends Marke {
	/** Position in Pixeln ab dem linken Rand. */
	x: number;
	/** Darf die Beschriftung stehen, ohne eine andere zu überdecken? */
	sichtbar: boolean;
}

export function spanne(f: Fenster): number {
	return Math.max(f.bis - f.von, 1e-9);
}

/** Zeit zu Position in Pixeln. */
export function position(f: Fenster, breite: number, at: number): number {
	return ((at - f.von) / spanne(f)) * breite;
}

/** Position in Pixeln zu Zeit. */
export function zeitpunkt(f: Fenster, breite: number, px: number): number {
	return f.von + (px / Math.max(breite, 1e-9)) * spanne(f);
}

/** Tageszahl eines Zeitpunkts (mit Bruchteil des Tages, Ortszeit). */
export function tagzahlAus(d: Date): number {
	const tag = tagZahl(
		`${String(d.getFullYear())}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
	);
	const seitMitternacht = d.getHours() * 3600_000 + d.getMinutes() * 60_000 + d.getSeconds() * 1000;
	return tag + seitMitternacht / MS_TAG;
}

/** Welche Stufe zum Ausschnitt passt: die feinste, deren Marken genug Platz haben. */
export function passendeStufe(f: Fenster, breite: number): Stufe {
	const pxTag = breite / spanne(f);
	if (pxTag >= 34) return 'tag';
	if (pxTag * 7 >= 44) return 'woche';
	if (pxTag * 30.4 >= 34) return 'monat';
	return 'jahr';
}

/** Der Ausschnitt einer Stufe um `mitte` (Tageszahl oder Plan-Monat). */
export function fensterFuerStufe(
	stufe: Stufe,
	breite: number,
	mitte: number,
	plan = false
): Fenster {
	const px = plan ? (PX_JE_PLANMONAT[stufe] ?? 24) : PX_JE_TAG[stufe];
	const s = Math.max(breite, 1) / px;
	return { von: mitte - s / 2, bis: mitte + s / 2 };
}

/** Wie weit ein Pfeil verschiebt: eine Einheit der Stufe. */
export function schrittFuer(stufe: Stufe, plan = false): number {
	if (plan) return stufe === 'jahr' ? 3 : 1;
	switch (stufe) {
		case 'tag':
			return 1;
		case 'woche':
			return 7;
		case 'monat':
			return 30;
		case 'jahr':
			return 365;
	}
}

/** Verschieben um `delta` Einheiten (Tage oder Monate). */
export function verschieben(f: Fenster, delta: number): Fenster {
	return { von: f.von + delta, bis: f.bis + delta };
}

/** Zoom um `faktor` (größer 1 vergrößert) um `anker` (0 bis 1). */
export function zoomen(f: Fenster, faktor: number, anker = 0.5): Fenster {
	const s = spanne(f) / faktor;
	const at = f.von + spanne(f) * anker;
	const von = at - s * anker;
	return { von, bis: von + s };
}

/** Grenzen mit etwas Luft links und rechts. */
export function gepolstert(g: Fenster, anteil = 0.03): Fenster {
	const p = spanne(g) * anteil;
	return { von: g.von - p, bis: g.bis + p };
}

/**
 * Den Ausschnitt in die Grenzen legen. Ist er länger als die Grenzen, gilt
 * das Ganze. Nicht endliche Werte gelten nicht: dann bleibt `vorher`.
 */
export function begrenzen(f: Fenster, grenzen: Fenster, vorher?: Fenster): Fenster {
	if (!Number.isFinite(f.von) || !Number.isFinite(f.bis) || f.bis <= f.von) {
		return vorher ?? { ...grenzen };
	}
	const s = spanne(f);
	if (s >= spanne(grenzen)) return { ...grenzen };
	let von = f.von;
	if (von < grenzen.von) von = grenzen.von;
	if (von + s > grenzen.bis) von = grenzen.bis - s;
	return { von, bis: von + s };
}

/** Sicherung gegen Endlosschleifen bei kaputten Werten. */
const HOECHSTENS = 5000;

function datumsTeile(zahl: number): { j: number; m: number; t: number } {
	const tag = tagAusZahl(zahl);
	return { j: Number(tag.slice(0, 4)), m: Number(tag.slice(5, 7)), t: Number(tag.slice(8, 10)) };
}

function zahlAus(j: number, m: number, t: number): number {
	return Math.round(Date.UTC(j, m - 1, t) / MS_TAG);
}

/** Marken einer Stufe im Ausschnitt. */
export function marken(f: Fenster, stufe: Stufe, plan = false): Marke[] {
	const aus: Marke[] = [];
	if (!Number.isFinite(f.von) || !Number.isFinite(f.bis) || f.bis <= f.von) return aus;
	if (plan) {
		const schritt = stufe === 'monat' ? 1 : stufe === 'jahr' ? 3 : 12;
		for (
			let m = Math.ceil(f.von / schritt) * schritt, n = 0;
			m <= f.bis && n < HOECHSTENS;
			m += schritt, n++
		) {
			aus.push({ at: m, text: `Monat ${String(m)}`, haupt: m % 12 === 0 });
		}
		return aus;
	}
	const start = Math.ceil(f.von);
	switch (stufe) {
		case 'tag':
			for (let d = start, n = 0; d <= f.bis && n < HOECHSTENS; d++, n++) {
				const { t, m } = datumsTeile(d);
				aus.push({
					at: d,
					text: t === 1 ? `1. ${MONATE_KURZ[m - 1] ?? ''}` : tagKurz(tagAusZahl(d)),
					haupt: t === 1
				});
			}
			return aus;
		case 'woche': {
			// Montage (der 5.1.1970 war ein Montag).
			let d = start + ((((1 - ((start + 4) % 7)) % 7) + 7) % 7);
			for (let n = 0; d <= f.bis && n < HOECHSTENS; d += 7, n++) {
				const { t, m } = datumsTeile(d);
				aus.push({ at: d, text: `${String(t)}.${String(m)}.`, haupt: t <= 7 });
			}
			return aus;
		}
		case 'monat': {
			let { j, m } = datumsTeile(start);
			for (let n = 0; n < HOECHSTENS; n++) {
				const d = zahlAus(j, m, 1);
				if (d > f.bis) break;
				if (d >= f.von) {
					aus.push({
						at: d,
						text: m === 1 ? String(j) : (MONATE_KURZ[m - 1] ?? ''),
						haupt: m === 1
					});
				}
				m += 1;
				if (m > 12) {
					m = 1;
					j += 1;
				}
			}
			return aus;
		}
		case 'jahr': {
			let { j } = datumsTeile(start);
			const breit = spanne(f) > 365 * 14;
			for (let n = 0; n < HOECHSTENS; n++, j++) {
				const d = zahlAus(j, 1, 1);
				if (d > f.bis) break;
				if (d >= f.von && (!breit || j % 5 === 0))
					aus.push({ at: d, text: String(j), haupt: true });
			}
			return aus;
		}
	}
}

/** Geschätzte Breite eines Texts in Pixeln (Ziffern und Buchstaben bei 12 px). */
export function textBreite(text: string, zeichenPx = 7.2): number {
	return Math.ceil(text.length * zeichenPx) + 4;
}

/**
 * Beschriftungen setzen, ohne dass sich zwei überdecken: erst die
 * Hauptmarken, dann die übrigen; jede steht links an ihrer Marke und braucht
 * `abstand` Pixel Luft zur nächsten. Was rechts aus der Achse ragen würde, fällt weg.
 */
export function beschriften(
	liste: Marke[],
	f: Fenster,
	breite: number,
	{ zeichenPx = 7.2, abstand = 8 }: { zeichenPx?: number; abstand?: number } = {}
): Beschriftung[] {
	const mitX = liste.map((m) => ({ ...m, x: position(f, breite, m.at), sichtbar: false }));
	const belegt: [number, number][] = [];
	const frei = (a: number, b: number): boolean =>
		belegt.every(([c, d]) => b + abstand <= c || a >= d + abstand);
	const reihe = [...mitX.filter((m) => m.haupt), ...mitX.filter((m) => !m.haupt)];
	for (const m of reihe) {
		const a = m.x + 3;
		const b = a + textBreite(m.text, zeichenPx);
		if (a < 0 || b > breite) continue;
		if (frei(a, b)) {
			belegt.push([a, b]);
			m.sichtbar = true;
		}
	}
	return mitX;
}

/** „03.11.2026“ aus einer Tageszahl. */
export function datumAusZahl(zahl: number): string {
	const t = tagAusZahl(zahl);
	return `${t.slice(8, 10)}.${t.slice(5, 7)}.${t.slice(0, 4)}`;
}
