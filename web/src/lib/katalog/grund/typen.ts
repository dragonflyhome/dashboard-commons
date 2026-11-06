/** Typen der Grundteile (Etappe 3a-3), damit Seiten sie ohne Bits UI kennen. */

/** Eine Wahl in `<Auswahl>`. */
export interface Option {
	wert: string;
	text: string;
	/** Nicht wählbar (bleibt sichtbar, grau). */
	gesperrt?: boolean;
}

/** Eine Spalte in `<Tabelle>` (Form mit Daten). */
export interface Spalte {
	/** Schlüssel in jeder Zeile. */
	schluessel: string;
	titel: string;
	/**
	 * `mono`: Pfade und Kennungen, bricht nicht um. `zahl`: rechtsbündig.
	 * `marke` (Etappe B1): das Wort als `<Marke>`; die Bedeutung je Zeile
	 * sagt der Server (`abgeleitet.listen.<liste>.marken`), sonst neutral.
	 */
	art?: 'text' | 'mono' | 'zahl' | 'marke';
	/**
	 * Etappe B1: Schlüssel der Zeile, in dem die Adresse steht; die Zelle
	 * wird dann ein Link. Nur https (oder http://localhost, öffnet in neuem
	 * Tab) und Pfade dieser App (`/projekte/…`); alles andere bleibt Text.
	 */
	link?: string;
	/**
	 * Etappe B1: in der Summenzeile die Summe dieser Spalte. Sie rechnet der
	 * Server (`abgeleitet.listen.<liste>.summe`, nur Zahlenspalten); die
	 * Tabelle rechnet nie selbst.
	 */
	summe?: boolean;
}

/** Eine Zeile in `<Tabelle>`: Wert je Schlüssel der Spalten. */
export type Zeile = Readonly<Record<string, string | number | boolean | null | undefined>>;

/** Ein Punkt in `<Menue>` (Etappe 3b): ein Link. */
export interface MenuePunkt {
	text: string;
	/** Fertige Adresse (mit `resolve()` gebaut). */
	href: string;
	/** Die Seite, auf der man gerade ist. */
	aktiv?: boolean;
	/** Kleine Zahl in der Farbe für „Achtung“, mit Text für Vorlese-Programme. */
	zahl?: { n: number; text: string };
}
