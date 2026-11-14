/**
 * Lokale Pfade sind keine Links, und der Kontoname steht
 * nirgends im Bild (Links-Kästen, /pc). Erfundene Namen.
 */
import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import {
	PROFIL_PLATZHALTER,
	SATZ_LOKALER_PFAD,
	VOLUME_PLATZHALTER,
	istLokalerPfad,
	profilVerbergen
} from './pfade.ts';
import LinksListe from './components/projekt/LinksListe.svelte';

describe('istLokalerPfad', () => {
	it('erkennt Laufwerk, file: und Netzwerkpfad', () => {
		for (const p of [
			'C:\\Users\\beispiel\\Downloads\\x.zip',
			'C:/Users/beispiel/Projekte/bericht',
			'd:\\Ablage',
			'file:///C:/Ablage/x.pdf',
			'FILE://rechner/x',
			'\\\\rechner\\freigabe\\x.pdf',
			'  C:\\mit-leerzeichen-vorn'
		]) {
			expect(istLokalerPfad(p), p).toBe(true);
		}
	});
	it('Web-Adressen, mailto und Wege im Dashboard bleiben Links', () => {
		for (const p of [
			'https://example.org/C:/x',
			'http://localhost:8787/',
			'mailto:a@example.org',
			'/projects/x',
			'C:'
		]) {
			expect(istLokalerPfad(p), p).toBe(false);
		}
	});
});

describe('istLokalerPfad unter macOS und Linux', () => {
	it('erkennt Profil- und Systemformen', () => {
		for (const p of [
			'/Users/beispiel/Downloads/x.zip',
			'/home/beispiel/dev',
			'/Volumes/Platte/Ablage',
			'/root',
			'/usr/local/bin/git',
			'/opt/homebrew/bin/node',
			'/private/var/folders/ab/x/T',
			'/Applications',
			'  /Users/beispiel'
		]) {
			expect(istLokalerPfad(p), p).toBe(true);
		}
	});
	it('ein Weg im Dashboard mit / vorn ist nie lokal', () => {
		for (const p of [
			'/projekte/x',
			'/projects/x',
			'/pc',
			'/daten',
			'/anstehend',
			'/',
			'/Userspace/x',
			'/homepage',
			'/users/beispiel',
			'https://example.org/Users/beispiel',
			'ssh://host/Users/beispiel/repo.git'
		]) {
			expect(istLokalerPfad(p), p).toBe(false);
		}
	});
});

describe('profilVerbergen unter macOS und Linux', () => {
	it('ersetzt das Profil, behält den Rest', () => {
		expect(profilVerbergen('/Users/beispiel/Downloads/x.zip')).toBe(
			`${PROFIL_PLATZHALTER}/Downloads/x.zip`
		);
		expect(profilVerbergen('/home/beispiel')).toBe(PROFIL_PLATZHALTER);
		expect(profilVerbergen('/System/Volumes/Data/Users/beispiel/x')).toBe(
			`${PROFIL_PLATZHALTER}/x`
		);
		expect(profilVerbergen('/Volumes/Platte/Users/beispiel/x')).toBe(`${PROFIL_PLATZHALTER}/x`);
		expect(profilVerbergen('file:///Users/beispiel/x.pdf')).toBe(`${PROFIL_PLATZHALTER}/x.pdf`);
		expect(profilVerbergen('/root/.cargo/bin')).toBe(`${PROFIL_PLATZHALTER}/.cargo/bin`);
		expect(profilVerbergen('/var/root')).toBe(PROFIL_PLATZHALTER);
	});
	it('mehrere Stellen in einem Befehl, Anführungszeichen bleiben', () => {
		expect(profilVerbergen('"/Users/beispiel/bin/a" --ort /home/beispiel/b')).toBe(
			`"${PROFIL_PLATZHALTER}/bin/a" --ort ${PROFIL_PLATZHALTER}/b`
		);
	});
	it('der Name eines Datenträgers wird ersetzt', () => {
		expect(profilVerbergen('/Volumes/Beispiels-Platte/Fotos')).toBe(
			`/Volumes/${VOLUME_PLATZHALTER}/Fotos`
		);
	});
	it('zwei Pfade hintereinander bleiben zwei Pfade', () => {
		expect(profilVerbergen('ls /Users/beispiel /Users/beispiel2')).toBe(
			`ls ${PROFIL_PLATZHALTER} ${PROFIL_PLATZHALTER}`
		);
		expect(profilVerbergen('/home/a;/home/b')).toBe(`${PROFIL_PLATZHALTER};${PROFIL_PLATZHALTER}`);
		expect(profilVerbergen('/Users/beispiel, /home/beispiel2/x')).toBe(
			`${PROFIL_PLATZHALTER}, ${PROFIL_PLATZHALTER}/x`
		);
		// Ein Name mit Leerzeichen gilt nur, wenn danach ein Trenner kommt.
		expect(profilVerbergen('/Users/Beispiel Konto/x')).toBe(`${PROFIL_PLATZHALTER}/x`);
		expect(profilVerbergen('/Users/beispiel ist kaputt')).toBe(`${PROFIL_PLATZHALTER} ist kaputt`);
	});
	it('eine Liste mit Doppelpunkt (PATH) wird je Pfad geschwärzt', () => {
		expect(profilVerbergen('PATH=/usr/bin:/Users/beispiel/.cargo/bin')).toBe(
			`PATH=/usr/bin:${PROFIL_PLATZHALTER}/.cargo/bin`
		);
		expect(profilVerbergen('/usr/bin:/home/beispiel/bin')).toBe(
			`/usr/bin:${PROFIL_PLATZHALTER}/bin`
		);
		expect(profilVerbergen('PATH=/Users/beispiel/bin:/usr/bin:/home/beispiel2/bin:/root/bin')).toBe(
			`PATH=${PROFIL_PLATZHALTER}/bin:/usr/bin:${PROFIL_PLATZHALTER}/bin:${PROFIL_PLATZHALTER}/bin`
		);
		expect(profilVerbergen('/usr/bin:/Volumes/Platte/x')).toBe(
			`/usr/bin:/Volumes/${VOLUME_PLATZHALTER}/x`
		);
		// Das Glied vor dem Doppelpunkt ist eine Variable.
		expect(profilVerbergen('export PATH=$PATH:/Users/beispiel/.cargo/bin')).toBe(
			`export PATH=$PATH:${PROFIL_PLATZHALTER}/.cargo/bin`
		);
		expect(profilVerbergen('PATH=${PATH}:/home/beispiel/bin:~/bin:/root/bin')).toBe(
			`PATH=\${PATH}:${PROFIL_PLATZHALTER}/bin:~/bin:${PROFIL_PLATZHALTER}/bin`
		);
		expect(profilVerbergen('%PATH%:/Users/beispiel/bin')).toBe(`%PATH%:${PROFIL_PLATZHALTER}/bin`);
		// Eine Adresse später im selben Wort ändert nichts (kompaktes JSON).
		expect(
			profilVerbergen('{"path":"/usr/bin:/Users/beispiel2/bin","url":"https://example.org"}')
		).toBe(`{"path":"/usr/bin:${PROFIL_PLATZHALTER}/bin","url":"https://example.org"}`);
		// Laufwerke in einer Liste: das Laufwerk gehört zum Profil.
		expect(profilVerbergen('PATH=C:/bin:C:/Users/beispiel/bin')).toBe(
			`PATH=C:/bin:${PROFIL_PLATZHALTER}/bin`
		);
	});
	it('file: mit einem, zwei oder drei Schrägstrichen und mit localhost', () => {
		expect(profilVerbergen('file:/Users/beispiel/x.pdf')).toBe(`${PROFIL_PLATZHALTER}/x.pdf`);
		expect(profilVerbergen('file://Users/beispiel/x.pdf')).toBe(`${PROFIL_PLATZHALTER}/x.pdf`);
		expect(profilVerbergen('file:///home/beispiel')).toBe(PROFIL_PLATZHALTER);
		expect(profilVerbergen('file:/root/x')).toBe(`${PROFIL_PLATZHALTER}/x`);
		expect(profilVerbergen('file:///Volumes/Platte/Fotos')).toBe(
			`file:///Volumes/${VOLUME_PLATZHALTER}/Fotos`
		);
		expect(profilVerbergen('file://localhost/Users/beispiel/x')).toBe(
			`file://localhost${PROFIL_PLATZHALTER}/x`
		);
		expect(profilVerbergen('FILE://LOCALHOST/home/beispiel')).toBe(
			`FILE://LOCALHOST${PROFIL_PLATZHALTER}`
		);
	});
	it('Datenträger und root ohne Rücksicht auf Groß und Klein', () => {
		expect(profilVerbergen('FILE:///Volumes/Platte/Fotos')).toBe(
			`FILE:///Volumes/${VOLUME_PLATZHALTER}/Fotos`
		);
		expect(profilVerbergen('file://LOCALHOST/Volumes/Platte/x')).toBe(
			`file://LOCALHOST/Volumes/${VOLUME_PLATZHALTER}/x`
		);
		expect(profilVerbergen('file:///volumes/Platte')).toBe(`file:///volumes/${VOLUME_PLATZHALTER}`);
		expect(profilVerbergen('FILE:///ROOT/x')).toBe(`${PROFIL_PLATZHALTER}/x`);
		expect(profilVerbergen('file://LOCALHOST/root/x')).toBe(
			`file://LOCALHOST${PROFIL_PLATZHALTER}/x`
		);
	});
	it('Schrägstriche wie in JSON (\\/)', () => {
		expect(profilVerbergen('"\\/Users\\/beispiel\\/x"')).toBe(`"${PROFIL_PLATZHALTER}\\/x"`);
		expect(profilVerbergen('"\\/home\\/beispiel"')).toBe(`"${PROFIL_PLATZHALTER}"`);
		// Gegenprobe: eine Domäne davor bleibt eine Adresse.
		expect(profilVerbergen('example.org\\/home\\/beispiel')).toBe('example.org\\/home\\/beispiel');
		// `file:` mit Schrägstrichen wie in JSON, auch mit localhost.
		expect(profilVerbergen('{"cwd":"file:\\/\\/\\/Users\\/beispiel2\\/x"}')).toBe(
			`{"cwd":"${PROFIL_PLATZHALTER}\\/x"}`
		);
		expect(profilVerbergen('"file:\\/\\/localhost\\/home\\/beispiel\\/x"')).toBe(
			`"file:\\/\\/localhost${PROFIL_PLATZHALTER}\\/x"`
		);
	});
	it('Adressen im Netz, Git-Remotes und andere Pfade bleiben', () => {
		for (const p of [
			'https://example.org/users/beispiel',
			'https://example.org/home/beispiel/x',
			'ssh://host/Users/beispiel/repo.git',
			'git@host:/Users/beispiel/repo.git',
			'host.example:/home/beispiel/repo.git',
			'konto@host-1.example:/home/beispiel/repo.git',
			'ssh://konto@host:22/home/beispiel/repo.git',
			'https://example.org/ab:/Users/beispiel',
			'ssh://host/x:/home/beispiel',
			'http://users/beispiel',
			'/Users',
			'/home/',
			'/rootfs/x',
			'/var/rootless',
			'/usr/local/bin/git',
			'/projects/beispiel',
			'/Volumes/'
		]) {
			expect(profilVerbergen(p), p).toBe(p);
		}
	});

	it('ein kodiertes Zeichen vor dem Laufwerk macht es nicht zum Listentrenner', () => {
		expect(profilVerbergen('%2FC:/Users/beispiel zweiter')).toBe(`%2F${PROFIL_PLATZHALTER}`);
		expect(profilVerbergen('%5CD:/Users/beispiel')).toBe(`%5C${PROFIL_PLATZHALTER}`);
		expect(profilVerbergen('%252FC:/Users/beispiel')).toBe(`%252F${PROFIL_PLATZHALTER}`);
	});
});

describe('profilVerbergen', () => {
	it('ersetzt das Profil, behält den Rest', () => {
		expect(profilVerbergen('C:\\Users\\beispiel\\Downloads\\x.zip')).toBe(
			`${PROFIL_PLATZHALTER}\\Downloads\\x.zip`
		);
		expect(profilVerbergen('c:/users/Beispiel Konto/Projekte')).toBe(
			`${PROFIL_PLATZHALTER}/Projekte`
		);
		expect(profilVerbergen('D:\\Benutzer\\beispiel')).toBe(PROFIL_PLATZHALTER);
		expect(profilVerbergen('file:///C:/Users/beispiel/x.pdf')).toBe(`${PROFIL_PLATZHALTER}/x.pdf`);
	});
	it('mehrere Stellen in einem Befehl, Anführungszeichen bleiben', () => {
		expect(
			profilVerbergen('"C:\\Users\\beispiel\\AppData\\a.exe" --ort C:\\Users\\beispiel\\b')
		).toBe(`"${PROFIL_PLATZHALTER}\\AppData\\a.exe" --ort ${PROFIL_PLATZHALTER}\\b`);
	});
	it('vor einem Laufwerk mit Schrägstrich steht kein Wort', () => {
		// Gegenprobe: Ziele in der scp-Form haben `/` hinter dem Doppelpunkt und bleiben.
		for (const p of ['host:/Users/beispiel/x', 'git@host:/users/beispiel/repo.git']) {
			expect(profilVerbergen(p), p).toBe(p);
		}
		expect(profilVerbergen('nach C:\\Users\\beispiel\\x')).toBe(`nach ${PROFIL_PLATZHALTER}\\x`);
		expect(profilVerbergen('"C:/Users/beispiel"')).toBe(`"${PROFIL_PLATZHALTER}"`);
	});
	it('ein Laufwerk mit Rückstrich gilt an jeder Stelle', () => {
		expect(profilVerbergen('Fehler:\\nC:\\\\Users\\\\beispiel\\\\x')).toBe(
			`Fehler:\\n${PROFIL_PLATZHALTER}\\\\x`
		);
		expect(profilVerbergen('x\\tC:\\Users\\beispiel')).toBe(`x\\t${PROFIL_PLATZHALTER}`);
		expect(profilVerbergen('\\u0022C:\\\\Users\\\\beispiel')).toBe(`\\u0022${PROFIL_PLATZHALTER}`);
		expect(profilVerbergen('\\x22C:\\\\Users\\\\beispiel')).toBe(`\\x22${PROFIL_PLATZHALTER}`);
		expect(profilVerbergen('\\042C:\\Users\\beispiel')).toBe(`\\042${PROFIL_PLATZHALTER}`);
		expect(profilVerbergen('\x1b[34mC:\\Users\\beispiel\\x')).toBe(
			`\x1b[34m${PROFIL_PLATZHALTER}\\x`
		);
		expect(profilVerbergen('abC:\\Users\\beispiel')).toBe(`ab${PROFIL_PLATZHALTER}`);
	});
	it('ein Laufwerk mit Schrägstrich beginnt auch nach Escape, kodiertem Zeichen, Farbcode oder Ziffer', () => {
		expect(profilVerbergen('%22C:/Users/beispiel/x')).toBe(`%22${PROFIL_PLATZHALTER}/x`);
		expect(profilVerbergen('%2522C:/Users/beispiel/x')).toBe(`%2522${PROFIL_PLATZHALTER}/x`);
		expect(profilVerbergen('%20C:/Users/beispiel')).toBe(`%20${PROFIL_PLATZHALTER}`);
		expect(profilVerbergen('Fehler:\\nC:/Users/beispiel/x')).toBe(
			`Fehler:\\n${PROFIL_PLATZHALTER}/x`
		);
		expect(profilVerbergen('\\u00e4C:/Users/beispiel')).toBe(`\\u00e4${PROFIL_PLATZHALTER}`);
		expect(profilVerbergen('\\xffC:/Users/beispiel')).toBe(`\\xff${PROFIL_PLATZHALTER}`);
		expect(profilVerbergen('\\042C:/Users/beispiel')).toBe(`\\042${PROFIL_PLATZHALTER}`);
		expect(profilVerbergen('\x1b[1;34mC:/Users/beispiel')).toBe(`\x1b[1;34m${PROFIL_PLATZHALTER}`);
		expect(profilVerbergen('2026C:/Users/beispiel/x')).toBe(`2026${PROFIL_PLATZHALTER}/x`);
	});
	it('ein Schalter direkt vor dem Pfad', () => {
		expect(profilVerbergen('cc -IC:/Users/beispiel/include -LC:/Users/beispiel/lib')).toBe(
			`cc -I${PROFIL_PLATZHALTER}/include -L${PROFIL_PLATZHALTER}/lib`
		);
		expect(profilVerbergen('cc -I/Users/beispiel/include -isystem/home/beispiel/x')).toBe(
			`cc -I${PROFIL_PLATZHALTER}/include -isystem${PROFIL_PLATZHALTER}/x`
		);
		expect(profilVerbergen('--prefix/Users/beispiel')).toBe(`--prefix${PROFIL_PLATZHALTER}`);
		// Gegenprobe: ein Rechnername mit Bindestrich ist kein Schalter.
		for (const p of ['konto@mein-host:/Users/beispiel/x', 'a-b/Users/beispiel']) {
			expect(profilVerbergen(p), p).toBe(p);
		}
	});
	it('ein langer Text ohne Leerraum hält die Oberfläche nicht auf', () => {
		const n = 100_000;
		const proben = [
			'file:'.repeat(n / 5),
			'C:\\Users\\'.repeat(n / 9),
			'/a:'.repeat(n / 3),
			'a'.repeat(n) + ':/Users/beispiel',
			'/a'.repeat(n / 2) + ':/Users/beispiel',
			'/Users/' + 'a '.repeat(n / 2),
			'$a:'.repeat(n / 3) + '/Users/beispiel',
			'\\u0022'.repeat(n / 6) + 'C:/Users/beispiel',
			'\x1b[' + '1;'.repeat(n / 2) + 'mC:/Users/beispiel',
			'-'.repeat(n),
			'-I'.repeat(n / 2) + '/Users/beispiel'
		];
		for (const p of proben) {
			const beginn = performance.now();
			profilVerbergen(p);
			const dauer = performance.now() - beginn;
			expect(dauer, `${p.slice(0, 12)}… ${Math.round(dauer)} ms`).toBeLessThan(200);
		}
	});
	it('andere Pfade bleiben, wie sie sind', () => {
		for (const p of [
			'C:\\ProgramData\\Dashboard\\dienst.toml',
			'C:\\Users',
			'D:/Ablage/Projekte',
			'kein Pfad'
		]) {
			expect(profilVerbergen(p)).toBe(p);
		}
	});
});

describe('LinksListe', () => {
	it('lokaler Pfad: Text mit Hinweis, ohne Link und ohne Kontonamen', () => {
		const { container } = render(LinksListe, {
			links: [
				{ name: 'Export', url: 'C:\\Users\\beispiel\\Downloads\\export.zip' },
				{ name: 'Ablage', url: 'file:///D:/Ablage' },
				{ name: 'Netz', url: 'https://example.org/seite' }
			]
		});
		const links = [...container.querySelectorAll('a')];
		expect(links.map((a) => a.textContent)).toEqual(['Netz']);
		expect(links[0]?.getAttribute('href')).toBe('https://example.org/seite');
		expect(screen.getByText('Export').closest('a')).toBeNull();
		expect(container.textContent).not.toContain('beispiel');
		expect(container.textContent).toContain(`${PROFIL_PLATZHALTER}\\Downloads\\export.zip`);
		expect(screen.getAllByText(SATZ_LOKALER_PFAD)).toHaveLength(2);
	});
});
