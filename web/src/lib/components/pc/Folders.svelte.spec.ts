import { render } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import type { Inventory } from '$vertrag';
import Folders from './Folders.svelte';

describe('Ordner (Schritt E1)', () => {
	it('zeigt den Benutzerordner angemeldet im Klartext', () => {
		// Nur von /api/pc/benutzer (geschützt); erfundenes Profil.
		const inv: Inventory = {
			root: 'C:\\Users\\beispiel',
			computed_at: '2026-10-26T08:00:00+01:00',
			duration_secs: 1,
			entries: [],
			face: [],
			hinweis: null
		};
		const { container } = render(Folders, { inv, diskTotal: 0 });
		expect(container.textContent).toContain('C:\\Users\\beispiel');
		expect(container.textContent).not.toContain('%USERPROFILE%');
	});
});
