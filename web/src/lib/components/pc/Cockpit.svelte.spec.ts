import { render } from '@testing-library/svelte';
import { flushSync } from 'svelte';
import { describe, expect, it } from 'vitest';
import type { PcBenutzer, PcReport, PcTick } from '$vertrag';
import { freierKanal } from '#lib/freikanal.svelte.ts';
import { live } from '#lib/live.svelte.ts';
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
	it('ohne Anmeldung: nennt das Betriebssystem, aber nie den Rechner', () => {
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

	it('nennt angemeldet den Rechner und die Platte aus /api/pc/benutzer', () => {
		const benutzer = {
			rechner: 'beispiel-rechner',
			platten: [
				{
					name: 'Beispiel Platte',
					mount: '/Volumes/Beispiel Platte',
					total: 1_000,
					used: 400,
					belegung: 'normal'
				}
			],
			inventory: { computed_at: null, entries: [], hinweis: null },
			schluessel: { vollstaendig: 'ja', gespeichert: 1, fehlen: 0, grund: null },
			bote: 'da'
		} as unknown as PcBenutzer;
		const { container } = render(Cockpit, {
			report: bericht('%COMPUTERNAME%'),
			ladung: { benutzer, grund: null }
		});
		expect(container.querySelector('.head .muted')?.textContent.trim()).toMatch(
			/^beispiel-rechner · Beispiel-System 1/
		);
		// macOS: der Ort /Volumes/<Name> wiederholt den Namen; er steht nur einmal da.
		expect(container.textContent).toContain('Platte Beispiel Platte');
		expect(container.textContent).not.toContain('(/Volumes/');
		expect(container.textContent).not.toContain('%COMPUTERNAME%');
	});

	it('macOS: ein Ort außerhalb von /Volumes steht in Klammern dahinter', () => {
		const benutzer = {
			rechner: 'beispiel-rechner',
			platten: [{ name: 'Beispiel HD', mount: '/', total: 1_000, used: 400, belegung: 'normal' }],
			inventory: { computed_at: null, entries: [], hinweis: null },
			schluessel: { vollstaendig: 'ja', gespeichert: 1, fehlen: 0, grund: null },
			bote: 'da'
		} as unknown as PcBenutzer;
		const { container } = render(Cockpit, {
			report: bericht('%COMPUTERNAME%'),
			ladung: { benutzer, grund: null }
		});
		expect(container.textContent).toContain('Platte Beispiel HD (/)');
	});

	it('angemeldet: der freie Takt überschreibt die Platte nicht, der geschützte schon', () => {
		const platte = (name: string) => ({
			name,
			mount: `/Volumes/${name}`,
			total: 1_000,
			used: 400,
			belegung: 'normal' as const
		});
		const takt = (name: string) =>
			({
				at: name,
				cpu_percent: 1,
				mem_used: 1,
				mem_total: 2,
				disks: [platte(name)],
				uptime_secs: 1,
				cpu_belegung: 'normal',
				mem_belegung: 'normal'
			}) as unknown as PcTick;
		const benutzer = {
			rechner: 'beispiel-rechner',
			platten: [platte('Beispiel Platte')],
			inventory: { computed_at: null, entries: [], hinweis: null },
			schluessel: { vollstaendig: 'ja', gespeichert: 1, fehlen: 0, grund: null },
			bote: 'da'
		} as unknown as PcBenutzer;
		try {
			freierKanal.aktiv = 'geschuetzt';
			const { container } = render(Cockpit, {
				report: bericht('%COMPUTERNAME%'),
				ladung: { benutzer, grund: null }
			});
			freierKanal.pc = takt('%VOLUME%');
			flushSync();
			expect(container.textContent).toContain('Platte Beispiel Platte');
			live.pc = takt('Andere Platte');
			flushSync();
			expect(container.textContent).toContain('Platte Andere Platte');
		} finally {
			freierKanal.aktiv = null;
			freierKanal.pc = null;
			live.pc = null;
		}
	});

	it('abgemeldet, der geschützte Takt steht noch: seine Platte zählt nicht, nur der freie', () => {
		const takt = (name: string, mount: string) =>
			({
				at: name,
				cpu_percent: 1,
				mem_used: 1,
				mem_total: 2,
				disks: [{ name, mount, total: 1_000, used: 400, belegung: 'normal' }],
				uptime_secs: 1,
				cpu_belegung: 'normal',
				mem_belegung: 'normal'
			}) as unknown as PcTick;
		try {
			freierKanal.aktiv = 'frei';
			live.pc = takt('Beispiel Platte', 'C:\\');
			freierKanal.pc = takt('%VOLUME%', 'C:\\');
			const { container } = render(Cockpit, {
				report: bericht('%COMPUTERNAME%'),
				ladung: { benutzer: null, grund: 'anmeldung' }
			});
			expect(container.textContent).not.toContain('Beispiel Platte');
			expect(container.textContent).toContain('Platte C: 600 B frei');
			// Gegenprobe: gilt der geschützte, steht sein Name da.
			freierKanal.aktiv = 'geschuetzt';
			flushSync();
			expect(container.textContent).toContain('Platte Beispiel Platte (C:)');
		} finally {
			freierKanal.aktiv = null;
			freierKanal.pc = null;
			live.pc = null;
		}
	});

	it('ohne Anmeldung: die Platte kommt aus dem freien Takt', () => {
		try {
			const { container } = render(Cockpit, { report: bericht('%COMPUTERNAME%') });
			freierKanal.pc = {
				at: 'frei',
				cpu_percent: 1,
				mem_used: 1,
				mem_total: 2,
				disks: [{ name: '%VOLUME%', mount: 'X:\\', total: 1_000, used: 400, belegung: 'normal' }],
				uptime_secs: 1,
				cpu_belegung: 'normal',
				mem_belegung: 'normal'
			} as unknown as PcTick;
			flushSync();
			expect(container.textContent).toContain('Platte X: 600 B frei');
		} finally {
			freierKanal.pc = null;
		}
	});

	/** Ein Datenträger unter Windows: der Einhängepunkt ist nur der Laufwerksbuchstabe. */
	const windowsPlatte = (name: string, used = 400) => ({
		name,
		mount: 'C:\\',
		total: 1_000,
		used,
		belegung: 'normal' as const
	});
	const freierTakt = (used: number) =>
		({
			at: `frei-${used}`,
			cpu_percent: 1,
			mem_used: 1,
			mem_total: 2,
			disks: [windowsPlatte('%VOLUME%', used)],
			uptime_secs: 1,
			cpu_belegung: 'normal',
			mem_belegung: 'normal'
		}) as unknown as PcTick;
	const benutzerMit = (platten: unknown[]) =>
		({
			rechner: 'beispiel-rechner',
			platten,
			inventory: { computed_at: null, entries: [], hinweis: null },
			schluessel: { vollstaendig: 'ja', gespeichert: 1, fehlen: 0, grund: null },
			bote: 'da'
		}) as unknown as PcBenutzer;

	it('Windows: angemeldet der Name des Datenträgers mit dem Laufwerk, frei nur das Laufwerk', () => {
		try {
			const angemeldet = render(Cockpit, {
				report: bericht('%COMPUTERNAME%'),
				ladung: { benutzer: benutzerMit([windowsPlatte('Beispiel Platte')]), grund: null }
			});
			expect(angemeldet.container.textContent).toContain('Platte Beispiel Platte (C:)');
			angemeldet.unmount();
			// Ein leerer Name: nur das Laufwerk, ohne leere Klammer.
			const ohneName = render(Cockpit, {
				report: bericht('%COMPUTERNAME%'),
				ladung: { benutzer: benutzerMit([windowsPlatte('')]), grund: null }
			});
			expect(ohneName.container.textContent).toContain('Platte C: 600 B frei');
			ohneName.unmount();
			// Ohne Anmeldung (die echte Ladung): der freie Takt trägt den Platzhalter, er erscheint nicht.
			const frei = render(Cockpit, {
				report: bericht('%COMPUTERNAME%'),
				ladung: { benutzer: null, grund: 'anmeldung' }
			});
			freierKanal.pc = freierTakt(400);
			flushSync();
			expect(frei.container.textContent).toContain('Platte C: 600 B frei');
			expect(frei.container.textContent).not.toContain('%VOLUME%');
			expect(frei.container.textContent).not.toContain('Beispiel Platte');
		} finally {
			freierKanal.pc = null;
		}
	});

	it('nach dem Abmelden (neue Ladung ohne Benutzer): kein Rechnername, die Platte folgt dem freien Takt', async () => {
		try {
			const { container, rerender } = render(Cockpit, {
				report: bericht('%COMPUTERNAME%'),
				ladung: { benutzer: benutzerMit([windowsPlatte('Beispiel Platte')]), grund: null }
			});
			freierKanal.pc = freierTakt(100);
			flushSync();
			expect(container.textContent).toContain('beispiel-rechner');
			expect(container.textContent).toContain('Platte Beispiel Platte (C:)');
			// Das Layout lädt /api/pc/benutzer beim Wechsel geschützt → frei neu: jetzt 401.
			await rerender({ ladung: { benutzer: null, grund: 'anmeldung' } });
			freierKanal.pc = freierTakt(700);
			flushSync();
			expect(container.textContent).not.toContain('beispiel-rechner');
			expect(container.textContent).not.toContain('Beispiel Platte');
			expect(container.textContent).toContain('Platte C: 300 B frei');
		} finally {
			freierKanal.pc = null;
		}
	});
});
