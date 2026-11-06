import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import type { Backup, BackupLage } from '$vertrag';
import BackupCard from './BackupCard.svelte';

const backup: Backup = {
	copies: 2,
	external_disk: false,
	cloud: 'Cloud-Speicher: 40,2 GB',
	cloud_stand: null,
	note: 'Papiere liegen doppelt.',
	plan: []
};

/** Was der Server für copies = 2, external_disk = false und Text in cloud schickt. */
const stand: BackupLage = {
	kopien: [
		{ ort: 'pc', stand: 'vorhanden', text: null },
		{ ort: 'zweites-medium', stand: 'fehlt', text: null },
		{ ort: 'anderer-ort', stand: 'teilweise', text: 'Cloud-Speicher: 40,2 GB' }
	],
	regel_321: 'teilweise'
};

function kopie(titel: string): HTMLElement {
	const el = screen.getByText(titel).closest<HTMLElement>('.copy');
	if (!el) throw new Error(`keine Kopie „${titel}“`);
	return el;
}

describe('BackupCard', () => {
	it('zeigt die Kopien nach dem Stand vom Server, nicht nach copies', () => {
		render(BackupCard, { backup, stand, projects: [], totalBytes: null });
		expect(screen.getByText('eine Teilkopie in der Cloud, Platte fehlt')).toBeTruthy();
		const pc = kopie('Kopie 1: dieser PC');
		const platte = kopie('Kopie 2: externe Platte');
		const cloud = kopie('Kopie 3: anderer Ort');
		expect([pc, platte, cloud].map((k) => k.querySelector('.state')?.textContent)).toEqual([
			'vorhanden',
			'fehlt',
			'teilweise'
		]);
		// copies = 2, aber keine Platte: die Platte gilt nicht als vorhanden.
		expect(platte.classList.contains('miss')).toBe(true);
		expect(platte.classList.contains('have')).toBe(false);
		expect(cloud.classList.contains('part')).toBe(true);
		expect(cloud.title).toBe('Cloud-Speicher: 40,2 GB');
	});

	it('ohne Stand vom Server: keine Kopien, Ampel neutral', () => {
		const { container } = render(BackupCard, {
			backup,
			stand: null,
			projects: [],
			totalBytes: null
		});
		expect(screen.getByText('Stand unbekannt')).toBeTruthy();
		expect(container.querySelectorAll('.copy')).toHaveLength(0);
	});
});
