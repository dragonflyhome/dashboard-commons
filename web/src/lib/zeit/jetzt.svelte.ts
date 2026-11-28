/**
 * „Jetzt“ für die Zeit-Bausteine (Verlauf, Dichtestreifen), das über
 * Mitternacht weiterläuft (Auflage aus 3a-1: Heute wurde beim Öffnen
 * festgelegt; eine über Mitternacht offene Seite zeigte den alten Tag als
 * Heute). Um Mitternacht am Blickort (Anzeigezone, Stufe 7b) und beim
 * Wiederkehren der Seite an einem neuen Tag wird `wert` neu gestellt
 * (SvelteDate, reaktiv); was daraus rechnet (Heute, Gruppen), rechnet neu.
 * Wechselt die Zone, stellt es `wert` gleich neu und den Zeitgeber auf die
 * Mitternacht der neuen Zone.
 *
 * Muss beim Anlegen einer Komponente entstehen (legt einen `$effect` an,
 * der beim Abbau aufräumt).
 */
import { untrack } from 'svelte';
import { SvelteDate } from 'svelte/reactivity';
import { anzeigeZone } from './zone.svelte.ts';
import { beiTageswechselInZone } from './zonentag.ts';

export class LaufendesJetzt {
	// Date.now(), nicht new SvelteDate() ohne Wert: so gilt auch eine gestellte Uhr (Tests).
	readonly wert = new SvelteDate(Date.now());

	constructor() {
		$effect(() => {
			anzeigeZone();
			// untrack: `setTime` darf den Effekt nicht von `wert` abhängig machen.
			untrack(() => this.wert.setTime(Date.now()));
			return untrack(() =>
				beiTageswechselInZone(() => {
					this.wert.setTime(Date.now());
				})
			);
		});
	}
}
