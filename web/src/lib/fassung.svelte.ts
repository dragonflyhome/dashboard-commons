/**
 * Neue Fassung der Oberfläche im Tab (Etappe 4b, Nachtrag):
 * der Zustand am Live-Punkt und das Neuladen. Die
 * Regeln stehen in `fassung.ts`.
 *
 * Woher die drei Zustände kommen (`lage`, Vorrang 1 vor 3 vor 2):
 *
 * 1. Keine Verbindung: der Live-Kanal (frei oder geschützt,
 *    `freikanal.svelte.ts`) ist weg oder kam nie zustande, oder ein Abruf der
 *    API bekam keine Antwort (`apiStand`, bis wieder eine kommt).
 * 3. Bau läuft: angemeldet das Ereignis `bau` mit `laeuft` im geschützten
 *    Kanal, bis `gruen` oder `rot` kommt (`bauLage.laeuft`). Ohne Anmeldung
 *    (der freie Kanal trägt kein `bau`) nur ja/nein aus `bau.zustand` in
 *    `/api/projects` (`bauLage.freiLaeuft`, ohne Slug). Die Liste holt die
 *    Wache dann selbst nach (`FassungWache.svelte`). Den Schrittzähler
 *    (Bau-Tempo, Zweig C) zeigt angemeldet nur der Kopf der Projektseite.
 * 2. App veraltet: Das Ereignis `fassung` (beide Kanäle) nennt den Ordner
 *    des neuen Baus; `ereignis` prüft, ob er neuer ist als die Fassung, mit
 *    der der Tab geladen hat (`neuere`). Gleich oder älter: nichts.
 *
 * In Zustand 2 lädt die Seite nach 30 s von selbst (`selbstLaden`, mit
 * Schutz vor Schleifen, `FassungWache.svelte`); ein Klick lädt sofort
 * (`jetztLaden`), aber nie, solange etwas offen ist (`neuLadenAufschieben`).
 * In 1 lädt nichts neu, auch nicht nach einem fehlenden Teil des Baus
 * (`chunkFehler`); in 3 nur nach einem solchen, statt den Fehler zu zeigen.
 * Nie, solange etwas offen ist (auch nicht nach einem fehlenden Teil).
 *
 * Neu geladen wird ganz (`location.reload()`), nicht über den Router: Die
 * Teile des alten Baus gibt es nach dem Umstellen nicht mehr, ein neuer
 * Start mit index.html aus dem neuen Ordner ist der einzige sichere Weg.
 */
import { version } from '$app/env';
import { apiStand } from './api/antwortet.svelte.ts';
import { bauLage } from './bau.svelte.ts';
import {
	einmalNeuLaden,
	lageVon,
	neuere,
	neuLadenAufschieben,
	ungesichert,
	umgebung,
	type Lage
} from './fassung.ts';
import { freierKanal } from './freikanal.svelte.ts';
import { istLive } from './verbindung.ts';

/** Das Nötigste aus SvelteKits `beforeNavigate`. */
export interface Navigation {
	from: { route: { id: string | null } } | null;
	to: { url: URL; route: { id: string | null } } | null;
	willUnload: boolean;
	type: string;
	cancel: () => void;
}

/** Woher Verbindung und Bau kommen; in Tests ersetzbar. */
export interface Umfeld {
	/** Steht der Live-Kanal, und antwortet die API? `null`, solange der erste Versuch läuft. */
	verbunden: () => boolean | null;
	/** Läuft ein Bau? Angemeldet live, sonst aus der Projektliste. */
	baut: () => boolean;
}

export const umfeld: Umfeld = {
	verbunden: () => {
		const z = freierKanal.zustand;
		return z === null ? null : istLive(z) && apiStand.antwortet;
	},
	baut: () => (freierKanal.geschuetztOk ? bauLage.laeuft : bauLage.freiLaeuft)
};

export class Fassung {
	/** Der Server hat eine neuere Fassung als die dieses Tabs (Ereignis `fassung`). */
	veraltet = $state(false);
	/** Ein Sperrgrund hält das fällige Neuladen gerade auf (`selbstNeuLaden`). */
	aufgehalten = $state(false);
	/** Läuft ein Bau? */
	readonly baut: boolean = $derived.by(() => this.#umfeld.baut());
	/** Der Zustand am Live-Punkt; `null`: keiner. */
	readonly lage: Lage | null = $derived.by(() =>
		lageVon(this.#umfeld.verbunden(), this.baut, this.veraltet)
	);

	readonly #version: string;
	readonly #umfeld: Umfeld;

	/** `version`: die Fassung dieses Tabs (SvelteKit, `$app/env`). */
	constructor(version: string, u: Umfeld = umfeld) {
		this.#version = version;
		this.#umfeld = u;
	}

	/** Das Ereignis `fassung` nennt den Ordner eines Baus: veraltet nur, wenn er neuer ist. */
	ereignis(ordner: string): void {
		if (neuere(ordner, this.#version)) this.veraltet = true;
	}

	/**
	 * Von selbst neu laden (nach der Verzögerung, nichts offen). `false`:
	 * gerade nicht erlaubt (Schutz vor Schleifen, `darfNeuLaden`); der
	 * Hinweis bleibt, und es wird später noch einmal versucht.
	 */
	selbstLaden(): boolean {
		return einmalNeuLaden(this.#version);
	}

	/**
	 * Ein Teil des Baus ließ sich nicht laden: einmal neu laden (`true`) statt
	 * des Fehlers. `false`: gerade schon neu geladen, oder keine Verbindung
	 * (Zustand 1: nie neu laden; Chrome und Edge melden einen Teil, der wegen
	 * fehlender Verbindung nicht kam, mit demselben Satz), oder etwas ist offen
	 * (`neuLadenAufschieben`; dann lädt die Wache, sobald es vorbei ist); dann der Fehler.
	 */
	chunkFehler(doc: Document = document): boolean {
		if (this.lage === 'keine_verbindung' || neuLadenAufschieben(doc)) return false;
		return einmalNeuLaden(this.#version);
	}

	/**
	 * Klick auf den Hinweis (Abkürzung): sofort, aber nicht, solange etwas
	 * offen ist (`neuLadenAufschieben`; der Klick nimmt einem Feld den Fokus,
	 * sein Text zählt trotzdem). Dann bleibt es bei „aufgehalten“, und der
	 * Fokus geht zurück auf das ungesicherte Feld.
	 * shortcut: „aufgehalten“ endet erst mit der Uhr der Wache (spätestens 30 s), nicht sofort nach dem Ende des Sperrgrunds, aufrüsten, wenn das stört
	 */
	jetztLaden(doc: Document = document): void {
		if (neuLadenAufschieben(doc)) {
			this.aufgehalten = true;
			const feld = ungesichert(doc);
			if (feld instanceof HTMLTextAreaElement || feld instanceof HTMLInputElement) feld.focus();
			return;
		}
		umgebung.neuLaden();
	}

	/**
	 * Vor einer Navigation im Router: Ist die App veraltet (Zustand 2) und
	 * wechselt die Route, geht der Klick am Router vorbei zum Ziel (volle
	 * Navigation, lädt die neue Fassung). Nicht ohne Verbindung, während
	 * eines Baus, solange etwas offen ist (`neuLadenAufschieben`), innerhalb
	 * derselben Route (etwa `goto` mit `?ansicht=` auf /anstehend) und nicht bei
	 * Zurück/Vor (`popstate`: abbrechen und neu gehen verdoppelte den Verlauf).
	 * Verlassen der Seite (Tab zu, fremde Adresse) bleibt, wie es ist.
	 */
	vorNavigation(nav: Navigation, doc: Document = document): void {
		if (this.lage !== 'veraltet' || nav.willUnload || nav.to === null) return;
		if (nav.type === 'leave' || nav.type === 'popstate') return;
		if (nav.from !== null && nav.from.route.id === nav.to.route.id) return;
		if (neuLadenAufschieben(doc)) return;
		nav.cancel();
		umgebung.gehe(nav.to.url.href);
	}
}

export const fassung = new Fassung(version);
