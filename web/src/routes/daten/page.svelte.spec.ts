import { render, screen } from '@testing-library/svelte';
import type { ComponentProps } from 'svelte';
import { describe, expect, it, vi } from 'vitest';
import type { DatenBericht } from '$vertrag';

vi.mock('$app/navigation', () => ({ invalidate: vi.fn() }));
vi.mock('$app/paths', () => ({ resolve: (p: string) => p }));

const { default: Page } = await import('./+page.svelte');

const bericht: DatenBericht = {
	pfad: '%USERPROFILE%\\dashboard\\data\\dashboard.redb',
	groesse_bytes: 0,
	schema: 0,
	letzte_sicherung: null,
	sicherung_klasse: 'bad',
	sicherungs_ordner: '%USERPROFILE%\\dashboard\\data\\sicherung',
	tabellen: [],
	pc_verlauf: { erster: null, letzter: null, eintraege: 0 },
	fehler: 'Der Datenspeicher ließ sich nicht öffnen. Hinweis: Sicherung liegt im Ordner.'
};

function zeige(b: DatenBericht): void {
	const props = { data: { bericht: b, verlauf: { werte: [] } } };
	render(Page, props as unknown as ComponentProps<typeof Page>);
}

describe('Seite Daten', () => {
	it('zeigt ohne Datenspeicher den roten Kasten und sperrt das Sichern', () => {
		zeige(bericht);
		const kasten = screen.getByRole('alert');
		expect(kasten).toHaveClass('block-error');
		expect(kasten).toHaveTextContent('Der Datenspeicher ließ sich nicht öffnen.');
		expect(screen.getByRole('button', { name: 'Jetzt sichern' })).toBeDisabled();
		// Ort und Ordner bleiben sichtbar.
		expect(screen.getByText(bericht.pfad)).toBeInTheDocument();
		expect(screen.getByText(bericht.sicherungs_ordner)).toBeInTheDocument();
		// Tabellen, Verlauf und Export: je ein kurzer Satz statt Inhalt.
		expect(screen.getAllByText('Ohne Datenspeicher nicht verfügbar.')).toHaveLength(3);
		expect(screen.queryByText(/Als TOML ansehen/)).not.toBeInTheDocument();
		expect(screen.queryByText(/Nummer 0/)).not.toBeInTheDocument();
		expect(screen.getByText('unbekannt')).toBeInTheDocument();
	});
	it('zeigt mit Datenspeicher keinen Kasten', () => {
		zeige({ ...bericht, fehler: null, schema: 1 });
		expect(screen.queryByRole('alert')).not.toBeInTheDocument();
		expect(screen.getByText('Nummer 1')).toBeInTheDocument();
		expect(screen.getByRole('button', { name: 'Jetzt sichern' })).toBeEnabled();
		expect(screen.queryByText('Ohne Datenspeicher nicht verfügbar.')).not.toBeInTheDocument();
	});
});
