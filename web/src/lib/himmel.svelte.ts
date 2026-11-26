/**
 * Kalender, Stufe 6: der Himmel von heute am Heimatort für die Kopfleiste
 * (`GET /api/kalender/kopf`, nur angemeldet). Der Baustein `kopf/Himmel.svelte`
 * startet den Laden in seinem eigenen `onMount`; kein anderes Teil teilt die Daten.
 *
 * Neu geladen wird nur bei Tageswechsel (auch nach Rückkehr in die Seite),
 * bei einer Änderung im Projekt `kalender` (oder einem Sammelereignis), nach
 * verpassten Ereignissen und beim Anmelden ohne Neuladen. Beim Abmelden ist der
 * Zustand sofort leer. Nichts wird im Browser gespeichert.
 */
import { API, getJson } from './api/client';
import { freierKanal } from './freikanal.svelte.ts';
import { live } from './live.svelte.ts';
import { beiTageswechsel } from './tageswechsel.ts';
import { mitZugang } from './zugang.ts';
import type { KalenderKopf } from '$vertrag';

export class HimmelLaden {
	/** Die letzte 200-Antwort; ohne Anmeldung, bei Fehler oder ohne Netz `null`. */
	kopf = $state<KalenderKopf | null>(null);
	#nr = 0;
	#fetch: typeof fetch;

	constructor(fetchFn: typeof fetch = mitZugang((...a) => fetch(...a))) {
		this.#fetch = fetchFn;
	}

	/** Lädt neu; nur die letzte Anfrage zählt. */
	async laden(): Promise<void> {
		const nr = ++this.#nr;
		const k = await getJson<KalenderKopf>(`${API}/kalender/kopf`, this.#fetch).catch(() => null);
		if (nr === this.#nr) this.kopf = k;
	}

	/** Sofort leer; eine noch laufende Anfrage zählt nicht mehr. */
	leeren(): void {
		this.#nr += 1;
		this.kopf = null;
	}

	/** Erstes Laden und die Anlässe zum Neuladen; gibt das Ende zurück (danach leer). */
	start(): () => void {
		void this.laden();
		const neu = (): void => void this.laden();
		const aus = [
			beiTageswechsel(neu),
			live.onInhalt((a) => {
				if (a.sammel || a.projekte.includes('kalender')) neu();
			}),
			live.onNachholen(neu),
			freierKanal.onBereit((k) => {
				if (k === 'geschuetzt') neu();
			}),
			freierKanal.onAbgemeldet(() => {
				this.leeren();
			})
		];
		return () => {
			for (const a of aus) a();
			this.leeren();
		};
	}
}

/** Der Satz für Name und Titel des Links, etwa „Mond: abnehmend. Sonne heute 07:53 bis 16:18. Ort: …. Zum Kalender“. */
export function himmelSatz(k: KalenderKopf): string {
	const sonne =
		k.aufgang !== null && k.untergang !== null
			? `Sonne heute ${k.aufgang} bis ${k.untergang}.`
			: (k.text ?? '');
	return `Mond: ${k.phase}. ${sonne} Ort: ${k.ort}. Zum Kalender`.replace(/\s+/g, ' ');
}

/**
 * Der beleuchtete Teil des Monds als Pfad im Kreis um 0,0 mit Halbmesser `r`,
 * hell rechts (zunehmend im Norden); `spiegeln`: hell links. Der Rand des Lichts
 * ist eine halbe Ellipse mit der Breite r·|cos(Phase)|.
 */
export function mondPfad(phaseGrad: number, r: number): { d: string; spiegeln: boolean } {
	const p = ((phaseGrad % 360) + 360) % 360;
	const zunehmend = p <= 180;
	const k = Math.cos(((zunehmend ? p : 360 - p) * Math.PI) / 180);
	const rx = Math.abs(r * k).toFixed(2);
	const d = `M 0 ${String(-r)} A ${String(r)} ${String(r)} 0 0 1 0 ${String(r)} A ${rx} ${String(r)} 0 0 ${k > 0 ? 0 : 1} 0 ${String(-r)} Z`;
	return { d, spiegeln: !zunehmend };
}
