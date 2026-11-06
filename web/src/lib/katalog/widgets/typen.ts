/** Typen der Widgets (Etappe 3a-3, A4). */
import type { AnstehendEintrag } from '$vertrag';

/** Eine Gruppe in „Arbeiten“: Wort, Einträge und wie viele noch (ohne Anmeldung). */
export interface ArbeitsGruppeAnsicht {
	wort: string;
	eintraege: readonly AnstehendEintrag[];
	weitere: number;
}
