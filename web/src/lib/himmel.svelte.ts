/**
 * Kalender, Stufe 6 und 7b: der Himmel von heute am Blickort
 * (`GET /api/kalender/kopf`, nur angemeldet). Ein gemeinsamer Laden (`himmel`)
 * für die Kopfleiste (`kopf/Himmel.svelte`), den Bildschirm /heute und die
 * Kalenderseite: Jeder Nutzer meldet sich mit `nutzen()` an; der erste startet,
 * der letzte beendet und leert. /heute verlangt „häufig“ (alle 10 Minuten neu,
 * für die Sonnenhöhe); die Kopfleiste allein lädt nicht so oft.
 *
 * Neu geladen wird zum Beginn des nächsten Tags am Blickort (`naechster_tag`,
 * auch nach Rückkehr in die Seite; ohne Kopf zur lokalen Mitternacht), bei
 * einem neuen Blickort (Bereich `blickort`, auch aus einem anderen Tab), bei
 * einer Änderung im Projekt `kalender` (oder einem Sammelereignis), nach
 * verpassten Ereignissen und beim Anmelden ohne Neuladen; ohne IANA-Zone am Blickort
 * zusätzlich stündlich. Beim Abmelden ist der Zustand sofort leer, der Fehler sagt
 * „anmelden“. Jede Antwort setzt die Anzeigezone (`zeit/zone.svelte.ts`),
 * jeder Fehler setzt sie zurück. Nichts wird im Browser gespeichert.
 */
import { API, AnmeldungNoetig, getJson, senden } from './api/client';
import { freierKanal } from './freikanal.svelte.ts';
import { live } from './live.svelte.ts';
import { beiTageswechsel, beiZeitpunkt } from './tageswechsel.ts';
import { zoneSetzen } from './zeit/zone.svelte.ts';
import { mitZugang } from './zugang.ts';
import type { KalenderBlickort, KalenderKopf } from '$vertrag';

/** Solange /heute offen ist, so oft neu (die Sonnenhöhe ändert sich langsam). */
export const HAEUFIG_MS = 10 * 60_000;
/** Ohne IANA-Zone (`naechster_tag` leer) so oft neu, damit der Vortag höchstens so lange steht. */
export const OHNE_ZONE_MS = 60 * 60_000;

export class HimmelLaden {
	/** Die letzte 200-Antwort; ohne Anmeldung, bei Fehler oder ohne Netz `null`. */
	kopf = $state<KalenderKopf | null>(null);
	/** Der Fehler der letzten Anfrage (`ApiError` oder Netz), sonst `null`; /heute unterscheidet danach. */
	fehler = $state<unknown>(null);
	#wahlen = $state(0);
	#nr = 0;
	#fetch: typeof fetch;
	#nutzer = 0;
	#haeufig = 0;
	#takt: ReturnType<typeof setInterval> | undefined;
	#aus: (() => void)[] = [];
	#tag: (() => void) | null = null;

	constructor(fetchFn: typeof fetch = mitZugang((...a) => fetch(...a))) {
		this.#fetch = fetchFn;
	}

	/** Eine Wahl des Blickorts läuft; die Auswahl ist so lange gesperrt. */
	get waehlt(): boolean {
		return this.#wahlen > 0;
	}

	/** Lädt neu; nur die letzte Anfrage (auch eine Wahl) zählt. */
	async laden(): Promise<void> {
		const nr = ++this.#nr;
		try {
			const k = await getJson<KalenderKopf>(`${API}/kalender/kopf`, this.#fetch);
			if (nr === this.#nr) this.#uebernehmen(k);
		} catch (e) {
			if (nr !== this.#nr) return;
			this.kopf = null;
			this.fehler = e;
			zoneSetzen(null);
			this.#tagStellen();
		}
	}

	/**
	 * Wählt den Blickort (`null`: folgt dem Heimatort) und übernimmt den neuen Kopf
	 * aus der Antwort. Zählt wie ein eigenes Laden; ein Fehler fliegt zum Aufrufer,
	 * der Kopf bleibt dann, wie er war.
	 */
	async waehlen(ort: string | null): Promise<void> {
		const nr = ++this.#nr;
		this.#wahlen += 1;
		try {
			const koerper: KalenderBlickort = { ort };
			const k = await senden<KalenderKopf>(`${API}/kalender/blickort`, koerper, this.#fetch);
			if (nr === this.#nr) this.#uebernehmen(k);
		} finally {
			this.#wahlen -= 1;
		}
	}

	/** Sofort leer; eine noch laufende Anfrage zählt nicht mehr. */
	leeren(): void {
		this.#nr += 1;
		this.kopf = null;
		this.fehler = null;
		zoneSetzen(null);
	}

	/**
	 * Meldet einen Nutzer an; gibt die Abmeldung zurück. Der erste startet Laden und
	 * Anlässe, der letzte beendet sie und leert. `haeufig`: alle 10 Minuten neu, solange
	 * mindestens ein solcher Nutzer da ist; kommt er zu einem schon geladenen Kopf, lädt
	 * er sofort neu (die Höhe darin kann Stunden alt sein).
	 */
	nutzen(haeufig = false): () => void {
		this.#nutzer += 1;
		if (this.#nutzer === 1) this.#starten();
		else if (haeufig) void this.laden();
		if (haeufig && ++this.#haeufig === 1)
			this.#takt = setInterval(() => void this.laden(), HAEUFIG_MS);
		let ab = false;
		return () => {
			if (ab) return;
			ab = true;
			if (haeufig && --this.#haeufig === 0) clearInterval(this.#takt);
			if (--this.#nutzer === 0) this.#beenden();
		};
	}

	#uebernehmen(k: KalenderKopf): void {
		this.kopf = k;
		this.fehler = null;
		zoneSetzen(k.zone);
		this.#tagStellen();
	}

	/**
	 * Zeitgeber auf den nächsten Tag am Blickort; ohne Kopf auf die lokale Mitternacht.
	 * Ohne IANA-Zone kennt der Browser den Tageswechsel am Ort nicht: dann zusätzlich
	 * eine Stunde nach dem letzten Laden neu.
	 */
	#tagStellen(): void {
		if (this.#nutzer === 0) return;
		this.#tag?.();
		const neu = (): void => void this.laden();
		const ziel = this.kopf === null ? NaN : Date.parse(this.kopf.naechster_tag);
		// Ein Zeitpunkt in der Vergangenheit (Uhr des Geräts vor der des Servers) liefe sonst im Kreis.
		const tag = ziel > Date.now() ? beiZeitpunkt(ziel, neu) : beiTageswechsel(neu);
		if (this.kopf?.naechster_tag !== '') {
			this.#tag = tag;
			return;
		}
		const stunde = setTimeout(neu, OHNE_ZONE_MS);
		this.#tag = () => {
			tag();
			clearTimeout(stunde);
		};
	}

	#starten(): void {
		const neu = (): void => void this.laden();
		this.#aus = [
			live.onBereich((b) => {
				if (b === 'blickort') neu();
			}),
			live.onInhalt((a) => {
				if (a.sammel || a.projekte.includes('kalender')) neu();
			}),
			live.onNachholen(neu),
			freierKanal.onBereit((k) => {
				// Nur wenn noch nichts da ist: angemeldet lädt schon der Start.
				if (k === 'geschuetzt' && this.kopf === null) neu();
			}),
			freierKanal.onAbgemeldet(() => {
				this.leeren();
				// Sonst stünde auf /heute bis zum nächsten Takt „wird geladen“.
				this.fehler = new AnmeldungNoetig();
			})
		];
		this.#tagStellen();
		neu();
	}

	#beenden(): void {
		for (const a of this.#aus) a();
		this.#aus = [];
		this.#tag?.();
		this.#tag = null;
		this.leeren();
	}
}

/** Der eine Laden für Kopfleiste, /heute und /kalender. */
export const himmel = new HimmelLaden();

/**
 * Der Satz für Name und Titel des Links in der Kopfleiste, etwa „Mond: abnehmend.
 * Sonne heute 07:53 bis 16:18. Ort: …. Zu Heute“; Ort ist der Blickort.
 * Am Übergangstag zu Polartag oder -nacht steht die eine Zeit, die es gibt, vor dem Satz des Servers.
 */
export function himmelSatz(k: KalenderKopf): string {
	const sonne =
		k.aufgang !== null && k.untergang !== null
			? `Sonne heute ${k.aufgang} bis ${k.untergang}.`
			: [
					k.aufgang !== null ? `Sonnenaufgang heute ${k.aufgang}.` : '',
					k.untergang !== null ? `Sonnenuntergang heute ${k.untergang}.` : '',
					k.text ?? ''
				].join(' ');
	return `Mond: ${k.phase}. ${sonne} Ort: ${k.ort}. Zu Heute`.replace(/\s+/g, ' ');
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
