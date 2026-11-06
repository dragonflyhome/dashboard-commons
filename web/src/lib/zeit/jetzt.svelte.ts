/**
 * „Jetzt“ für die Zeit-Bausteine (Verlauf, Dichtestreifen), das über
 * Mitternacht weiterläuft (Auflage aus 3a-1: Heute wurde beim Öffnen
 * festgelegt; eine über Mitternacht offene Seite zeigte den alten Tag als
 * Heute). Um Mitternacht und beim Wiederkehren der Seite an einem neuen Tag
 * (`beiTageswechsel`) wird `wert` neu gestellt (SvelteDate, reaktiv); was
 * daraus rechnet (Heute, Gruppen), rechnet neu.
 *
 * Muss beim Anlegen einer Komponente entstehen (legt einen `$effect` an,
 * der beim Abbau aufräumt).
 */
import { SvelteDate } from 'svelte/reactivity';
import { beiTageswechsel } from '../tageswechsel.ts';

export class LaufendesJetzt {
	// Date.now(), nicht new SvelteDate() ohne Wert: so gilt auch eine gestellte Uhr (Tests).
	readonly wert = new SvelteDate(Date.now());

	constructor() {
		$effect(() =>
			beiTageswechsel(() => {
				this.wert.setTime(Date.now());
			})
		);
	}
}
