/**
 * Bau-Stand der Seiten im Browser (Etappe 4c), einmal für alle Seiten.
 *
 * Drei Quellen, ausgewertet nur in `projekte.ts` (`bauAnzeige`):
 *
 * - **frei:** `bau` je Projekt aus `/api/projects` (Zustand und Zeit der
 *   letzten guten Fassung). Das Layout reicht die Liste mit `liste()` durch;
 *   jede neue Liste ist der Stand des Servers und ersetzt, was live kam.
 * - **live:** das Ereignis `bau` im geschützten Kanal (nur angemeldet). Es
 *   überbrückt die Zeit bis zur nächsten Liste; bei `gruen` und `rot` lädt
 *   das Layout die Liste neu (`api:projects`), hier der Satz.
 * - **geschützt:** `GET /api/bauen` mit dem Satz bei rot. Ohne Anmeldung
 *   fragt die Seite gar nicht erst (`mitZugang`).
 * - **Fortschritt** (Bau-Tempo, Zweig C): das Ereignis `bau-fortschritt` im
 *   geschützten Kanal, nur Zahlen. Ohne Anmeldung kommt keins, also kein
 *   Schrittzähler. Er ist nur sichtbar, nie angesagt (`BauKopf.svelte`).
 *
 * Die Oberfläche bietet keinen
 * Knopf zum Bauen mehr an; sie baut von selbst (Chats nehmen den Vorgang
 * `bau`). `POST /api/bauen/<slug>` gibt es seit Etappe 6d nicht mehr.
 */
import { API } from './api/client';
import { live } from './live.svelte.ts';
import { mitZugang } from './zugang.ts';
import { bauAnzeige, kopfMitStand, type BauAnzeige } from './projekte.ts';
import type { BauEvent, BauFortschrittEvent, BauListe, ProjectSummary, ProjektBau } from '$vertrag';

/** Zweig C: der Schritt des laufenden Baus als kurzer Text; `null`: keiner. */
export function schrittText(f: BauFortschrittEvent | null): string | null {
	if (f === null) return null;
	switch (f.schritt) {
		case 'pruefung':
			return f.gesamt > 0
				? `prüft Projekt ${String(Math.min(f.fertig + 1, f.gesamt))} von ${String(f.gesamt)}`
				: null;
		case 'sammelpruefung':
			return `prüft ${String(f.gesamt)} Seiten`;
		case 'gesamtpruefung':
			return 'prüft alles zusammen und baut';
		case 'vite_build':
			return 'baut noch einmal';
	}
}

export class BauLage {
	/** Frei: Bau-Stand je Slug aus `/api/projects`. */
	frei = $state.raw<Record<string, ProjektBau>>({});
	/** Live: das letzte Ereignis `bau` je Slug, bis die Liste neu da ist. */
	live = $state.raw<Record<string, BauEvent>>({});
	/** Geschützt: der Satz bei rot je Slug (`/api/bauen`). */
	saetze = $state.raw<Record<string, string>>({});
	/**
	 * Ist die Karte „Stand“ des Projekts rot? Das Unter-Dashboard meldet es
	 * (`standMelden`); der Kopf sagt dann nicht „Seite in Ordnung“.
	 */
	standRot = $state.raw<Record<string, boolean>>({});
	/** Zweig C: der letzte Fortschritt des laufenden Baus (nur angemeldet), bis er endet. */
	fortschritt = $state.raw<BauFortschrittEvent | null>(null);

	#fetch: typeof fetch;

	constructor(fetchFn: typeof fetch = mitZugang((...a) => fetch(...a))) {
		this.#fetch = fetchFn;
	}

	/** Die Projektliste ist (neu) da: Sie ist der Stand des Servers. */
	liste(projekte: readonly ProjectSummary[] | null): void {
		const frei: Record<string, ProjektBau> = {};
		for (const p of projekte ?? []) if (p.bau) frei[p.slug] = p.bau;
		this.frei = frei;
		this.live = {};
		// Laut Server läuft kein Bau: Ein Zähler, dessen Ende verpasst wurde, gilt nicht mehr.
		if (!Object.values(frei).some((b) => b.zustand === 'laeuft')) this.fortschritt = null;
	}

	/** Ein Ereignis `bau`. */
	ereignis(b: BauEvent): void {
		this.live = { ...this.live, [b.slug]: b };
		// `laeuft` kommt nur am Anfang einer Runde, `gruen`/`rot` am Ende: Ein Zähler
		// von davor gilt nie mehr, auch einer, der nach dem Ende ankam (zwei Kanäle).
		this.fortschritt = null;
	}

	/** Zweig C: der Schritt als Text, solange ein Bau läuft; sonst `null`. */
	get schritt(): string | null {
		return this.laeuft ? schrittText(this.fortschritt) : null;
	}

	/**
	 * Läuft gerade ein Bau? Ein Ereignis `bau` mit `laeuft` (bis `gruen` oder
	 * `rot` kommt), sonst der Stand der Liste. Für Zustand 3 am Live-Punkt
	 * (`fassung.svelte.ts`), angemeldet.
	 */
	get laeuft(): boolean {
		if (Object.values(this.live).some((b) => b.zustand === 'laeuft')) return true;
		return Object.entries(this.frei).some(([s, b]) => b.zustand === 'laeuft' && !(s in this.live));
	}

	/**
	 * Läuft laut Projektliste ein Bau (nur ja/nein, ohne Slug)? Für Zustand 3
	 * ohne Anmeldung; aktuell hält die Liste dann `FassungWache.svelte`.
	 */
	get freiLaeuft(): boolean {
		return Object.values(this.frei).some((b) => b.zustand === 'laeuft');
	}

	/** Was Kopf, Leiste und Karte zum Bau dieser Seite zeigen (`null`: ohne eigene Seite). */
	anzeige(slug: string): BauAnzeige | null {
		return bauAnzeige(this.frei[slug], this.live[slug] ?? null, this.saetze[slug] ?? null);
	}

	/** Das Unter-Dashboard hat den Stand geladen: rot oder nicht. */
	standMelden(slug: string, rot: boolean): void {
		if ((this.standRot[slug] === true) !== rot) this.standRot = { ...this.standRot, [slug]: rot };
	}

	/** Die Zeile im Kopf der Projektseite: wie `anzeige`, dazu der Stand des Projekts. */
	kopf(slug: string): BauAnzeige | null {
		return kopfMitStand(this.anzeige(slug), this.standRot[slug] === true);
	}

	/** Lädt die Sätze (geschützt). Ohne Anmeldung: keine Sätze. Wirft nie. */
	async laden(): Promise<void> {
		try {
			const res = await this.#fetch(`${API}/bauen`, {
				credentials: 'same-origin',
				headers: { accept: 'application/json' }
			});
			if (!res.ok) {
				this.saetze = {};
				return;
			}
			const l = (await res.json()) as BauListe;
			const saetze: Record<string, string> = {};
			for (const p of l.projekte) if (p.satz !== null) saetze[p.slug] = p.satz;
			this.saetze = saetze;
		} catch {
			// Netzfehler (Server startet neu): beim nächsten Anlass wieder.
		}
	}

	/** Live-Kanal und erste Abfrage; gibt die Abmeldung zurück. */
	start(): () => void {
		void this.laden();
		const offBau = live.onBau((b) => {
			this.ereignis(b);
			if (b.zustand !== 'laeuft') void this.laden();
		});
		const offFortschritt = live.onBauFortschritt((f) => {
			this.fortschritt = f;
		});
		// Geschützter Kanal offen (angemeldet): die Sätze holen.
		const offKanal = live.onKanal((m) => {
			if (m.art === 'offen') void this.laden();
		});
		const offNachholen = live.onNachholen(() => {
			void this.laden();
		});
		return () => {
			offBau();
			offFortschritt();
			offKanal();
			offNachholen();
		};
	}
}

export const bauLage = new BauLage();
