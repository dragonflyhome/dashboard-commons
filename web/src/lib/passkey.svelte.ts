/**
 * Offene Passkey-Anträge für den Hinweis in der Kopfzeile (Etappe 7c), einmal
 * für alle Seiten. Die Zahl kommt aus `/api/schluessel/passkeys/anzahl`;
 * ohne Anmeldung als Inhaber ist sie 0. Jeder neue Antrag ist ein Vorgang, darum
 * zählt ein Ereignis `vorgang` (oder ein Nachholen) neu.
 */
import { live } from './live.svelte.ts';
import { ladeAnzahl } from './passkey.ts';
import { mitZugang } from './zugang.ts';

export class PasskeyAntraege {
	offen = $state(0);
	#zaehlung = 0;
	#fetch: typeof fetch;

	constructor(fetchFn: typeof fetch = mitZugang((...a) => fetch(...a))) {
		this.#fetch = fetchFn;
	}

	/** Lädt die Zahl neu; ältere Antworten zählen nicht mehr. */
	async zaehlen(): Promise<void> {
		const nummer = ++this.#zaehlung;
		const n = await ladeAnzahl(this.#fetch);
		if (nummer === this.#zaehlung) this.offen = n ?? 0;
	}

	/** Erste Zählung und Abo auf den Live-Kanal; gibt das Abmelden zurück. */
	start(): () => void {
		void this.zaehlen();
		const offVorgang = live.onVorgang(() => void this.zaehlen());
		const offNachholen = live.onNachholen(() => void this.zaehlen());
		return () => {
			offVorgang();
			offNachholen();
		};
	}
}

export const passkeyAntraege = new PasskeyAntraege();
