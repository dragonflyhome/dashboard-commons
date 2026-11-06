/**
 * Die letzte Minute Prozessorlast für das PC-Symbol oben rechts (Kopfleiste,
 * Fassung 3). Die Werte kommen über den einen Kanal der Seite (angemeldet der
 * geschützte, sonst der freie; Prozessorlast ist ein freier PC-Wert), alle
 * 5 s als `pc`. Gemerkt werden die letzten `PLAETZE` Werte (12 × 5 s =
 * 60 s); ein Wert mit derselben Zeit wie der letzte zählt nicht doppelt
 * (beim Wechsel des Kanals kommen kurz beide). Die Stufe (normal, hoch,
 * kritisch) rechnet der Server; die Farbe wählt die Oberfläche.
 */
import type { Belegung, PcTick } from '$vertrag';

/** So viele Werte: etwa 60 s bei einem Wert alle 5 s. */
export const PLAETZE = 12;

export class PcMinute {
	werte = $state<number[]>([]);
	/** Die Stufe des letzten Werts vom Server; vor dem ersten Wert `unbekannt`. */
	belegung = $state<Belegung>('unbekannt');
	#zuletzt: string | null = null;

	/** Der letzte Wert in Prozent, gerundet; `null` vor dem ersten. */
	get jetzt(): number | null {
		const v = this.werte.at(-1);
		return v === undefined ? null : Math.round(v);
	}

	/** Ein Wert vom Kanal. Unbekannte Belegung (Messung fehlt) zählt nicht. */
	aufnehmen(t: PcTick | null): void {
		if (t === null || t.cpu_belegung === 'unbekannt') return;
		const zeit = t.at ?? null;
		if (zeit !== null && zeit === this.#zuletzt) return;
		this.#zuletzt = zeit;
		this.werte = [...this.werte, t.cpu_percent].slice(-PLAETZE);
		this.belegung = t.cpu_belegung;
	}

	/** Für Tests. */
	vergessen(): void {
		this.werte = [];
		this.belegung = 'unbekannt';
		this.#zuletzt = null;
	}
}

/** Eine für die ganze Seite. */
export const pcMinute = new PcMinute();

/** Der Name für Vorlese-Programme und der Titel: „PC: Prozessor 23 %“. */
export function pcName(jetzt: number | null): string {
	return jetzt === null ? 'PC: Prozessor noch ohne Wert' : `PC: Prozessor ${String(jetzt)} %`;
}
