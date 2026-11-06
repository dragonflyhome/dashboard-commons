/**
 * System-Seiten /entscheidungen und /zielbild (Etappe 4d): die Felder aus
 * `docs/…/*.toml`, wie der Server sie liefert (`Feld`, `FeldWert`), als
 * einfache Zeilen für die Bausteine `Text` und `Liste` aus dem Katalog. Kein
 * Markdown: Text bleibt Text.
 */
import type { Entscheidung, Feld, FeldWert } from '$vertrag';

/** Ein Feld zum Zeigen: Name und entweder Text oder Aufzählung. */
export interface Zeile {
	name: string;
	text?: string;
	punkte?: string[];
}

/** `auflagen_spaeter` → „Auflagen spaeter“ (Schlüssel bleiben sonst, wie sie sind). */
export function feldName(name: string): string {
	const t = name.replaceAll('_', ' ').trim();
	return t === '' ? name : t.charAt(0).toUpperCase() + t.slice(1);
}

/** Ein Wert in einer Zeile: Listen mit Komma, Gruppen als „Name: Wert; …“. */
export function einzeilig(w: FeldWert): string {
	switch (w.art) {
		case 'text':
			return w.inhalt;
		case 'liste':
			return w.inhalt.map(einzeilig).join(', ');
		case 'gruppe':
			return w.inhalt.map((f) => `${feldName(f.name)}: ${einzeilig(f.wert)}`).join('; ');
	}
}

/**
 * Felder als flache Zeilen. Gruppen (Tabellen wie `[nachbesserung_1]`)
 * werden zu Zeilen mit „Gruppe · Feld“, Listen zu Aufzählungen.
 */
export function zeilen(felder: readonly Feld[], vorn = ''): Zeile[] {
	const aus: Zeile[] = [];
	for (const f of felder) {
		const name = vorn === '' ? feldName(f.name) : `${vorn} · ${feldName(f.name)}`;
		const w = f.wert;
		if (w.art === 'text') aus.push({ name, text: w.inhalt });
		else if (w.art === 'liste') aus.push({ name, punkte: w.inhalt.map(einzeilig) });
		else aus.push(...zeilen(w.inhalt, name));
	}
	return aus;
}

/** Die kurze Zeile unter dem Thema: Datum, Etappe, Status, von wem. */
export function eckdaten(e: Entscheidung): string {
	return [
		e.datum,
		e.etappe === null ? null : `Etappe ${e.etappe}`,
		e.status,
		e.von === null ? null : `von ${e.von}`
	]
		.filter((t): t is string => t !== null && t.trim() !== '')
		.join(' · ');
}

/**
 * Suche über Thema, Etappe, Status, von, Urteil, was und Dateiname
 * (Groß/Klein egal). Leere Suche: alles. „etappe 4“ und „4“ finden Etappe 4.
 */
export function filtern(liste: readonly Entscheidung[], suche: string): Entscheidung[] {
	const s = suche.trim().toLowerCase();
	if (s === '') return [...liste];
	return liste.filter((e) =>
		[
			e.thema,
			e.etappe === null ? null : `etappe ${e.etappe}`,
			e.status,
			e.von,
			e.urteil,
			e.was,
			e.datum,
			e.datei
		].some((t) => t?.toLowerCase().includes(s))
	);
}
