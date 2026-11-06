/**
 * Abschnitte einer Seite der neuen Form für das Inhaltsverzeichnis
 * (`AbschnittVerzeichnis.svelte`): je sichtbarem `Abschnitt` aus dem Katalog
 * (`section.abschnitt` mit h2) die Kennung der Überschrift und ihr Text.
 */

export interface AbschnittEintrag {
	/** `id` der Überschrift (Ziel des Sprunglinks). */
	id: string;
	titel: string;
}

/** Die sichtbaren Abschnitte unter `wurzel`, in der Reihenfolge der Seite. */
export function abschnitteLesen(wurzel: ParentNode): AbschnittEintrag[] {
	const aus: AbschnittEintrag[] = [];
	for (const s of wurzel.querySelectorAll<HTMLElement>('section.abschnitt')) {
		// Versteckt (leer, oder in einem anderen Reiter): nicht im Verzeichnis.
		if (s.closest('[hidden]') !== null) continue;
		const h = s.querySelector<HTMLElement>(':scope > h2');
		const titel = h?.textContent.trim() ?? '';
		if (h === null || h.id === '' || titel === '') continue;
		aus.push({ id: h.id, titel });
	}
	return aus;
}
