import { render } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import type { PcBenutzer, SysAppsRechner } from '$vertrag';
import SystemApps from './SystemApps.svelte';

describe('Systemanwendungen (Schritt E1)', () => {
	it('zeigt den Autostart-Befehl angemeldet im Klartext', () => {
		const apps = {
			refreshed_at: '2026-10-26T08:00:00+01:00',
			services_running: [],
			services_total: 0,
			errors: []
		} as unknown as SysAppsRechner;
		// Nur von /api/pc/benutzer (geschützt); erfundenes Profil.
		const benutzer = {
			sysapps: {
				autostart: [
					{
						name: 'Werkzeug',
						command: '"C:\\Users\\beispiel\\AppData\\Local\\werkzeug.exe" --leise',
						source: 'HKCU Run'
					}
				],
				tools: [],
				winget: [],
				winget_error: null
			},
			stand: null
		} as unknown as PcBenutzer;
		const { container } = render(SystemApps, { apps, benutzer });
		expect(container.textContent).toContain('C:\\Users\\beispiel\\AppData\\Local\\werkzeug.exe');
		expect(container.textContent).not.toContain('%USERPROFILE%');
	});
});
