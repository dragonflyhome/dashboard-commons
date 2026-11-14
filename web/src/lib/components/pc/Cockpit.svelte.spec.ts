import { render } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import type { PcReport } from '$vertrag';
import Cockpit from './Cockpit.svelte';

/** Ein Bericht mit erfundenem Rechner; nur was das Cockpit liest. */
function bericht(host: string): PcReport {
	return {
		snapshot: {
			taken_at: '2026-10-26T08:00:00+01:00',
			host,
			os: 'Beispiel-System 1',
			cpu_percent: 12.5,
			cpu_count: 8,
			mem_used: 8_000_000_000,
			mem_total: 16_000_000_000,
			disks: [{ name: '%VOLUME%', mount: '/', total: 1_000, used: 400, belegung: 'normal' }],
			uptime_secs: 3600,
			cpu_belegung: 'normal',
			mem_belegung: 'normal',
			belegung_schwellen: { hoch_ab: 70, kritisch_ab: 90 },
			neustart: { neustart: 'nicht-noetig', gruende: [] },
			defender: {
				checked_at: null,
				antivirus_enabled: true,
				realtime_enabled: true,
				signature_version: null,
				signature_updated: null,
				signature_age_days: 1,
				last_quick_scan: null,
				error: null,
				befund: 'in-ordnung',
				gruende: [],
				signatur_schwelle_tage: 3
			},
			history: [{ at: '2026-10-26T08:00:00+01:00', cpu: 12, mem: 50 }]
		},
		data: { backup: { note: '' } },
		backup_stand: null
	} as unknown as PcReport;
}

describe('Cockpit', () => {
	it('nennt das Betriebssystem, aber nie den Rechner', () => {
		// Der Server liefert statt des Namens den Platzhalter; auch der steht nicht im Bild.
		for (const host of ['%COMPUTERNAME%', 'beispiel-rechner', '']) {
			const { container, unmount } = render(Cockpit, { report: bericht(host) });
			const text = container.textContent;
			expect(text, host).toContain('Beispiel-System 1');
			expect(text, host).not.toContain('beispiel-rechner');
			expect(text, host).not.toContain('%COMPUTERNAME%');
			// Kein verwaister Trenner vor dem Betriebssystem.
			expect(container.querySelector('.head .muted')?.textContent.trim(), host).toMatch(
				/^Beispiel-System 1/
			);
			unmount();
		}
	});
});
