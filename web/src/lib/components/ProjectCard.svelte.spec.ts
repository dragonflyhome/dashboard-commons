import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import ProjectCard from './ProjectCard.svelte';
import type { ProjectSummary } from '$vertrag';

const project: ProjectSummary = {
	slug: 'schaufenster',
	title: 'Beispiel',
	status: 'aktiv',
	bedeutung: 'laeuft',
	last_change: '2026-10-25',
	summary: 'Beispielseite im Schaufenster.',
	chat: 'Beispiel-Chat Schaufenster',
	backup: 'gelb',
	backup_bedeutung: 'wartet',
	backup_note: 'nur Git',
	vorschau: null,
	order: 10,
	open: ['Foto', 'Text'],
	repos: [
		{
			path: 'x',
			label: 'schaufenster',
			private: false,
			branch: 'main',
			commit_hash: 'abc',
			commit_date: '2026-10-25',
			commit_subject: 's',
			dirty_files: 3,
			error: null,
			stand: '2026-10-25T12:00:00+01:00'
		}
	],
	file_modified: null
};

describe('ProjectCard', () => {
	it('Etappe B2: ohne Anmeldung nur die Zahl der offenen Punkte (open_zahl), kein Text', () => {
		render(ProjectCard, { project: { ...project, open: [], open_zahl: 2 } });
		expect(screen.getByText('2 offene Punkte')).toBeInTheDocument();
		expect(screen.queryByText('Foto')).toBeNull();
	});

	it('zeigt Titel, Chat, Backup und ungesicherte Dateien', () => {
		render(ProjectCard, { project });
		expect(screen.getByRole('link').getAttribute('href')).toMatch(/\/projects\/schaufenster$/);
		expect(screen.getByText('Beispiel')).toBeInTheDocument();
		expect(screen.getByText(/Beispiel-Chat Schaufenster/)).toBeInTheDocument();
		expect(screen.getByText(/Backup: gelb/)).toBeInTheDocument();
		expect(screen.getByText('✎ 3 ungesicherte Dateien')).toBeInTheDocument();
		expect(screen.getByText('2 offene Punkte')).toBeInTheDocument();
	});
	it('zeigt das Alter des Git-Stands, wenn er älter als fünf Minuten ist', () => {
		const frisch = new Date('2026-10-25T12:03:00+01:00');
		const spaeter = new Date('2026-10-25T12:30:00+01:00');
		const { unmount } = render(ProjectCard, { project, jetzt: frisch });
		expect(screen.queryByText(/Git-Stand von/)).not.toBeInTheDocument();
		unmount();
		render(ProjectCard, { project, jetzt: spaeter });
		expect(screen.getByText(/Git-Stand von \d\d:\d\d/)).toBeInTheDocument();
	});
	it('lässt Zeilen weg, wenn Felder fehlen', () => {
		render(ProjectCard, {
			project: { ...project, chat: null, backup: null, backup_bedeutung: null, open: [], repos: [] }
		});
		expect(screen.queryByText(/💬/)).not.toBeInTheDocument();
		expect(screen.queryByText(/Backup/)).not.toBeInTheDocument();
		expect(screen.queryByText(/offene Punkte/)).not.toBeInTheDocument();
	});
	it('zeigt kein Etikett „neue Form“ mehr (Etappe 6a: es gibt nur noch die eine Form)', () => {
		render(ProjectCard, { project });
		expect(screen.queryByText('neue Form')).not.toBeInTheDocument();
		expect(screen.getByRole('img', { name: 'aktiv' })).toHaveClass('b-laeuft');
	});
	it('sagt „wird ermittelt“, solange ein Repo noch keinen Git-Stand hat', () => {
		const repos = project.repos.map((r) => ({
			...r,
			stand: null,
			dirty_files: null,
			branch: null
		}));
		render(ProjectCard, { project: { ...project, repos } });
		expect(screen.getByText('Git-Stand wird ermittelt')).toBeInTheDocument();
		expect(screen.queryByText(/alles gesichert/)).not.toBeInTheDocument();
	});
});

describe('ProjectCard: Bau-Stand der Seite (Etappe 4c)', () => {
	it('rot mit „zeigt die letzte gute Fassung von …“, im Bau mit Wort, grün ohne Pille', () => {
		const { unmount } = render(ProjectCard, {
			project,
			bau: {
				zustand: 'rot',
				bedeutung: 'problem',
				wort: 'Seite mit Fehler',
				zeile: 'Seite mit Fehler: zeigt die letzte gute Fassung von Sa 31.10., 14:05 Uhr'
			}
		});
		expect(
			screen.getByText('Seite mit Fehler: zeigt die letzte gute Fassung von Sa 31.10., 14:05 Uhr')
		).toBeInTheDocument();
		unmount();
		const zwei = render(ProjectCard, {
			project,
			bau: { zustand: 'laeuft', bedeutung: 'laeuft', wort: 'Seite wird gebaut', zeile: 'x' }
		});
		expect(screen.getByText('Seite wird gebaut')).toBeInTheDocument();
		zwei.unmount();
		render(ProjectCard, {
			project,
			bau: { zustand: 'gruen', bedeutung: 'ok', wort: 'Seite in Ordnung', zeile: 'x' }
		});
		expect(document.querySelector('.pill.bau')).toBeNull();
	});
});
