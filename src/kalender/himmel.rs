// SPDX-License-Identifier: Apache-2.0
// Copyright 2023 The Unicode Consortium (ICU4X Project Developers)
// Herkunft: Kiste calendrical_calculations 0.2.4 (https://github.com/unicode-org/icu4x),
// Dateien src/astronomy.rs und src/helpers.rs (poly, binary_search, invert_angular); dort
// nach Reingold & Dershowitz, Calendrical Calculations (4. Auflage 2018, Lisp-Code unter
// Apache-2.0) und Meeus, Astronomical Algorithms.
// Gegenüber der Quelle geändert: nur Teil 1 (Neumond, Mondphase, Mond- und Sonnenlänge,
// Nutation, Aberration, ΔT); Zeitpunkte als f64 (Tage seit dem 1.1. des Jahres 1, UT)
// statt `Moment`; die ausgeschriebenen Reihen als Tabellen mit einer Schleife; ΔT nur ab
// 1800 (davor die Näherung der Quelle für ferne Jahre); ΔT 2051–2150 mit −0,5628·(2150−J)
// wie bei NASA (Espenak/Meeus) statt +0,5628, sonst springt ΔT 2050/51 um knapp zwei
// Minuten; neu `phase_ab` und `sonnenlaenge_ab` nach dem Lisp-Code (lunar-phase-at-or-after,
// solar-longitude-after); deutsche Namen.
// Teil 2 (Stufe 3): Schiefe der Ekliptik, Deklination, Zeitgleichung, Refraktion,
// `sine_offset`, `approx_moment_of_depression`, `moment_of_depression`, `local_from_apparent`,
// `universal_from_local`; statt `Location` Breite und Länge als Zahlen (Höhe nur in der
// Refraktion); `moment_of_depression` als Schleife, die bei 30 Sekunden (30/86400 Tagen)
// abbricht statt bei `30.0` Tagen (dort endet die Suche immer nach dem ersten Schritt), mit
// höchstens 10 Schritten, ohne Zusammenlaufen `None`; Deklination vorzeichenrichtig (−90…90 statt
// 0…360, gleichwertig in tan und cos); Höhe nur in `refraktion` (für den Abgleich, sonst 0);
// `early` als `morgens`.
// Der Text der Lizenz steht in LICENSES/Apache-2.0.txt.

//! Rechenkern für Sonne und Mond (Kalender, Stufe 2). Ein Zeitpunkt ist ein
//! Tag mit Bruchteil in Weltzeit (UT), gezählt wie die Tage der Kiste: Tag 1
//! ist der 1.1. des Jahres 1 (gregorianisch), Mitternacht = ganze Zahl.

use calendrical_calculations::gregorian;

/// Mittlere Länge eines Mondmonats (Neumond bis Neumond) in Tagen.
pub const SYNODISCHER_MONAT: f64 = 29.530588861;
/// Mittleres tropisches Jahr in Tagen.
const TROPISCHES_JAHR: f64 = 365.242189;
/// Mittag am 1.1.2000.
const J2000: f64 = 730120.5;
/// Der erste Neumond der Zeitrechnung (11.1. des Jahres 1).
const NEUMOND_NULL: f64 = 11.458922815770109;

/// Polynom mit den Beiwerten in aufsteigender Potenz.
fn poly(x: f64, beiwerte: &[f64]) -> f64 {
    beiwerte.iter().rev().fold(0.0, |a, c| a * x + c)
}

fn sin_grad(g: f64) -> f64 {
    g.to_radians().sin()
}

// Wie in der Quelle: für n ≥ 0 der Quotient selbst, nicht abgerundet.
fn div_euclid_f64(n: f64, d: f64) -> f64 {
    let (a, b) = (n / d, n % d);
    if n >= 0.0 || b == 0.0 { a } else { a - 1.0 }
}

/// ΔT: Unterschied von dynamischer Zeit und Weltzeit in Tagen.
fn delta_t(t: f64) -> f64 {
    let jahr = t / 365.2425;
    let j = (if jahr > 0.0 { jahr + 1.0 } else { jahr }) as i32;
    let mitte = gregorian::fixed_from_gregorian(j, 7, 1).to_i64_date() as f64;
    let c = (mitte - 693596.0) / 36525.0;
    let y2000 = f64::from(j - 2000);
    match j {
        // Minus wie bei NASA (Espenak/Meeus); die Quelle hat hier ein Plus.
        2051..=2150 => (-20.0 + 32.0 * (f64::from((j - 1820) * (j - 1820)) / 10000.0) - 0.5628 * f64::from(2150 - j)) / 86400.0,
        2006..=2050 => poly(y2000, &[62.92, 0.32217, 0.005589]) / 86400.0,
        1987..=2005 => poly(y2000, &[63.86, 0.3345, -0.060374, 0.0017275, 0.000651814, 0.00002373599]) / 86400.0,
        1900..=1986 => poly(c, &[-0.00002, 0.000297, 0.025184, -0.181133, 0.553040, -0.861938, 0.677066, -0.212591]),
        1800..=1899 => poly(c, &[-0.000009, 0.003844, 0.083563, 0.865736, 4.867575, 15.845535, 31.332267, 38.291999, 28.316289, 11.636204, 2.043794]),
        _ => {
            let y1820 = f64::from(j - 1820) / 100.0;
            (-20.0 + 32.0 * y1820 * y1820) / 86400.0
        }
    }
}

fn weltzeit_aus_dynamisch(t: f64) -> f64 {
    t - delta_t(t)
}

/// Julianische Jahrhunderte (dynamische Zeit) seit J2000.
fn jahrhunderte(t: f64) -> f64 {
    (t + delta_t(t) - J2000) / 36525.0
}

const NEUMOND_V: [f64; 24] = [
    -0.40720, 0.17241, 0.01608, 0.01039, 0.00739, -0.00514, 0.00208, -0.00111, -0.00057, 0.00056, -0.00042, 0.00042, 0.00038, -0.00024, -0.00007, 0.00004,
    0.00004, 0.00003, 0.00003, -0.00003, 0.00003, -0.00002, -0.00002, 0.00002,
];
const NEUMOND_E: [i32; 24] = [0, 1, 0, 0, 1, 1, 2, 0, 0, 1, 0, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
const NEUMOND_X: [f64; 24] = [0.0, 1.0, 0.0, 0.0, -1.0, 1.0, 2.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, -1.0, 2.0, 0.0, 3.0, 1.0, 0.0, 1.0, -1.0, -1.0, 1.0, 0.0];
const NEUMOND_Y: [f64; 24] = [1.0, 0.0, 2.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 2.0, 3.0, 0.0, 0.0, 2.0, 1.0, 2.0, 0.0, 1.0, 2.0, 1.0, 1.0, 1.0, 3.0, 4.0];
const NEUMOND_Z: [f64; 24] = [0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, -2.0, 2.0, 0.0, 0.0, 2.0, -2.0, 0.0, 0.0, -2.0, 0.0, -2.0, 2.0, 2.0, 2.0, -2.0, 0.0, 0.0];
const NEUMOND_I: [f64; 13] = [251.88, 251.83, 349.42, 84.66, 141.74, 207.14, 154.84, 34.52, 207.19, 291.34, 161.72, 239.56, 331.55];
const NEUMOND_J: [f64; 13] =
    [0.016321, 26.651886, 36.412478, 18.206239, 53.303771, 2.453732, 7.306860, 27.261239, 0.121824, 1.844379, 24.198154, 25.513099, 3.592518];
const NEUMOND_L: [f64; 13] = [0.000165, 0.000164, 0.000126, 0.000110, 0.000062, 0.000060, 0.000056, 0.000047, 0.000042, 0.000040, 0.000037, 0.000035, 0.000023];

/// Zeitpunkt des n-ten Neumonds (gezählt ab [`NEUMOND_NULL`]), nach Meeus.
pub fn neumond(n: i32) -> f64 {
    let k = f64::from(n) - 24724.0;
    let c = k / 1236.85;
    let ungefaehr = J2000 + poly(c, &[5.09766, SYNODISCHER_MONAT * 1236.85, 0.00015437, -0.00000015, 0.00000000073]);
    let e = poly(c, &[1.0, -0.002516, -0.0000074]);
    let sonne = poly(c, &[2.5534, 1236.85 * 29.10535670, -0.0000014, -0.00000011]);
    let mond = poly(c, &[201.5643, 385.81693528 * 1236.85, 0.0107582, 0.00001238, -0.000000058]);
    let breite = poly(c, &[160.7108, 390.67050284 * 1236.85, -0.0016118, -0.00000227, 0.000000011]);
    let knoten = poly(c, &[124.7746, -1.56375588 * 1236.85, 0.0020672, 0.00000215]);
    let reihe: f64 = (0..24).map(|i| NEUMOND_V[i] * e.powi(NEUMOND_E[i]) * sin_grad(NEUMOND_X[i] * sonne + NEUMOND_Y[i] * mond + NEUMOND_Z[i] * breite)).sum();
    let korrektur = -0.00017 * sin_grad(knoten) + reihe;
    let extra = 0.000325 * sin_grad(poly(c, &[299.77, 132.8475848, -0.009173]));
    let zusatz: f64 = (0..13).map(|i| NEUMOND_L[i] * sin_grad(NEUMOND_I[i] + NEUMOND_J[i] * k)).sum();
    weltzeit_aus_dynamisch(ungefaehr + korrektur + extra + zusatz)
}

const MONDLAENGE_V: [f64; 59] = [
    6288774.0, 1274027.0, 658314.0, 213618.0, -185116.0, -114332.0, 58793.0, 57066.0, 53322.0, 45758.0, -40923.0, -34720.0, -30383.0, 15327.0, -12528.0,
    10980.0, 10675.0, 10034.0, 8548.0, -7888.0, -6766.0, -5163.0, 4987.0, 4036.0, 3994.0, 3861.0, 3665.0, -2689.0, -2602.0, 2390.0, -2348.0, 2236.0, -2120.0,
    -2069.0, 2048.0, -1773.0, -1595.0, 1215.0, -1110.0, -892.0, -810.0, 759.0, -713.0, -700.0, 691.0, 596.0, 549.0, 537.0, 520.0, -487.0, -399.0, -381.0,
    351.0, -340.0, 330.0, 327.0, -323.0, 299.0, 294.0,
];
const MONDLAENGE_W: [f64; 59] = [
    0.0, 2.0, 2.0, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0, 2.0, 0.0, 1.0, 0.0, 2.0, 0.0, 0.0, 4.0, 0.0, 4.0, 2.0, 2.0, 1.0, 1.0, 2.0, 2.0, 4.0, 2.0, 0.0, 2.0, 2.0, 1.0,
    2.0, 0.0, 0.0, 2.0, 2.0, 2.0, 4.0, 0.0, 3.0, 2.0, 4.0, 0.0, 2.0, 2.0, 2.0, 4.0, 0.0, 4.0, 1.0, 2.0, 0.0, 1.0, 3.0, 4.0, 2.0, 0.0, 1.0, 2.0,
];
const MONDLAENGE_X: [f64; 59] = [
    0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, -1.0, 0.0, -1.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, -1.0, 0.0, 0.0, 0.0, 1.0, 0.0, -1.0,
    0.0, -2.0, 1.0, 2.0, -2.0, 0.0, 0.0, -1.0, 0.0, 0.0, 1.0, -1.0, 2.0, 2.0, 1.0, -1.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.0, -1.0, 2.0, 1.0, 0.0,
];
const MONDLAENGE_Y: [f64; 59] = [
    1.0, -1.0, 0.0, 2.0, 0.0, 0.0, -2.0, -1.0, 1.0, 0.0, -1.0, 0.0, 1.0, 0.0, 1.0, 1.0, -1.0, 3.0, -2.0, -1.0, 0.0, -1.0, 0.0, 1.0, 2.0, 0.0, -3.0, -2.0, -1.0,
    -2.0, 1.0, 0.0, 2.0, 0.0, -1.0, 1.0, 0.0, -1.0, 2.0, -1.0, 1.0, -2.0, -1.0, -1.0, -2.0, 0.0, 1.0, 4.0, 0.0, -2.0, 0.0, 2.0, 1.0, -2.0, -3.0, 2.0, 1.0,
    -1.0, 3.0,
];
const MONDLAENGE_Z: [f64; 59] = [
    0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -2.0, 2.0, -2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 0.0,
    0.0, 0.0, 0.0, 0.0, 0.0, -2.0, 2.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, -2.0, 0.0, 0.0, 0.0, 0.0, -2.0, -2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
];

fn nutation(c: f64) -> f64 {
    let a = poly(c, &[124.90, -1934.134, 0.002063]);
    let b = poly(c, &[201.11, 72001.5377, 0.00057]);
    -0.004778 * sin_grad(a) - 0.0003667 * sin_grad(b)
}

fn aberration(c: f64) -> f64 {
    0.0000974 * (177.63 + 35999.01848 * c).to_radians().cos() - 0.005575
}

/// Ekliptikale Länge des Mondes in Grad, `c` in julianischen Jahrhunderten.
fn mondlaenge(c: f64) -> f64 {
    let l = (218.3164477 + c * (481267.88123421 - 0.0015786 * c + c * c / 538841.0 - c * c * c / 65194000.0)).rem_euclid(360.0);
    let d = (297.85019021 + 445267.1114034 * c - 0.0018819 * c * c + c * c * c / 545868.0 - c * c * c * c / 113065000.0).rem_euclid(360.0);
    let ms = (357.5291092 + 35999.0502909 * c - 0.0001536 * c * c + c * c * c / 24490000.0).rem_euclid(360.0);
    let ml = (134.9633964 + 477198.8675055 * c + 0.0087414 * c * c + c * c * c / 69699.0 - c * c * c * c / 14712000.0).rem_euclid(360.0);
    let f = (93.2720950 + 483202.0175233 * c - 0.0036539 * c * c - c * c * c / 3526000.0 + c * c * c * c / 863310000.0).rem_euclid(360.0);
    let e = poly(c, &[1.0, -0.002516, -0.0000074]);
    let reihe: f64 = (0..59)
        .map(|i| {
            let x = MONDLAENGE_X[i];
            MONDLAENGE_V[i] * e.powi(x.abs() as i32) * sin_grad(MONDLAENGE_W[i] * d + x * ms + MONDLAENGE_Y[i] * ml + MONDLAENGE_Z[i] * f)
        })
        .sum();
    let venus = 3958.0 / 1000000.0 * sin_grad(119.75 + c * 131.849);
    let jupiter = 318.0 / 1000000.0 * sin_grad(53.09 + c * 479264.29);
    let erdform = 1962.0 / 1000000.0 * sin_grad(l - f);
    (l + reihe / 1000000.0 + venus + jupiter + erdform + nutation(c)).rem_euclid(360.0)
}

const SONNENLAENGE_X: [f64; 49] = [
    403406.0, 195207.0, 119433.0, 112392.0, 3891.0, 2819.0, 1721.0, 660.0, 350.0, 334.0, 314.0, 268.0, 242.0, 234.0, 158.0, 132.0, 129.0, 114.0, 99.0, 93.0,
    86.0, 78.0, 72.0, 68.0, 64.0, 46.0, 38.0, 37.0, 32.0, 29.0, 28.0, 27.0, 27.0, 25.0, 24.0, 21.0, 21.0, 20.0, 18.0, 17.0, 14.0, 13.0, 13.0, 13.0, 12.0, 10.0,
    10.0, 10.0, 10.0,
];
const SONNENLAENGE_Y: [f64; 49] = [
    270.54861, 340.19128, 63.91854, 331.26220, 317.843, 86.631, 240.052, 310.26, 247.23, 260.87, 297.82, 343.14, 166.79, 81.53, 3.50, 132.75, 182.95, 162.03,
    29.8, 266.4, 249.2, 157.6, 257.8, 185.1, 69.9, 8.0, 197.1, 250.4, 65.3, 162.7, 341.5, 291.6, 98.5, 146.7, 110.0, 5.2, 342.6, 230.9, 256.1, 45.3, 242.9,
    115.2, 151.8, 285.3, 53.3, 126.6, 205.7, 85.9, 146.1,
];
const SONNENLAENGE_Z: [f64; 49] = [
    0.9287892,
    35999.1376958,
    35999.4089666,
    35998.7287385,
    71998.20261,
    71998.4403,
    36000.35726,
    71997.4812,
    32964.4678,
    -19.4410,
    445267.1117,
    45036.8840,
    3.1008,
    22518.4434,
    -19.9739,
    65928.9345,
    9038.0293,
    3034.7684,
    33718.148,
    3034.448,
    -2280.773,
    29929.992,
    31556.493,
    149.588,
    9037.750,
    107997.405,
    -4444.176,
    151.771,
    67555.316,
    31556.080,
    -4561.540,
    107996.706,
    1221.655,
    62894.167,
    31437.369,
    14578.298,
    -31931.757,
    34777.243,
    1221.999,
    62894.511,
    -4442.039,
    107997.909,
    119.066,
    16859.071,
    -4.578,
    26895.292,
    -39.127,
    12297.536,
    90073.778,
];

/// Scheinbare ekliptikale Länge der Sonne in Grad, `c` in julianischen Jahrhunderten
/// (nach Bretagnon & Simon 1986).
fn sonnenlaenge_c(c: f64) -> f64 {
    let reihe: f64 = (0..49).map(|i| SONNENLAENGE_X[i] * sin_grad(SONNENLAENGE_Y[i] + SONNENLAENGE_Z[i] * c)).sum();
    let lambda = reihe * 0.000005729577951308232 + 282.7771834 + 36000.76953744 * c;
    (lambda + aberration(c) + nutation(c)).rem_euclid(360.0)
}

/// Länge der Sonne in Grad zum Zeitpunkt `t` (0 = Frühlings-Gleiche, 90, 180, 270).
pub fn sonnenlaenge(t: f64) -> f64 {
    sonnenlaenge_c(jahrhunderte(t))
}

/// Mondphase in Grad zum Zeitpunkt `t`: 0 Neumond, 90 erstes Viertel, 180 Vollmond, 270 letztes Viertel.
pub fn mondphase(t: f64) -> f64 {
    let c = jahrhunderte(t);
    let n = div_euclid_f64(t - NEUMOND_NULL, SYNODISCHER_MONAT).round() as i32;
    let a = (mondlaenge(c) - sonnenlaenge_c(c)).rem_euclid(360.0);
    let b = 360.0 * ((t - neumond(n)) / SYNODISCHER_MONAT).rem_euclid(1.0);
    if (a - b).abs() > 180.0 { b } else { a }
}

/// Nummer des ersten Neumonds zum Zeitpunkt `t` oder danach.
pub fn neumond_ab(t: f64) -> i32 {
    let phi = mondphase(t);
    let mut n = (div_euclid_f64(t - NEUMOND_NULL, SYNODISCHER_MONAT) - phi / 360.0).round() as i32;
    for _ in 0..31 {
        if neumond(n) >= t {
            break;
        }
        n += 1;
    }
    n
}

// Halbiert [l, h], bis es kürzer als `genau` ist; `links(x)`: Das Ziel liegt links von x.
fn halbieren(mut l: f64, mut h: f64, links: impl Fn(f64) -> bool, genau: f64) -> f64 {
    loop {
        let mitte = l + (h - l) / 2.0;
        (l, h) = if links(mitte) { (l, mitte) } else { (mitte, h) };
        if h - l < genau {
            return mitte;
        }
    }
}

/// Der Zeitpunkt in [a, b], an dem der Winkel `f` den Wert `y` erreicht (Genauigkeit 1e-5 Tage, unter einer Sekunde).
fn umkehren(f: impl Fn(f64) -> f64, y: f64, a: f64, b: f64) -> f64 {
    halbieren(a, b, |x| (f(x) - y).rem_euclid(360.0) < 180.0, 1e-5)
}

/// Der erste Zeitpunkt ab `t`, an dem die Mondphase `phase` Grad erreicht.
pub fn phase_ab(phase: f64, t: f64) -> f64 {
    let tau = t + SYNODISCHER_MONAT / 360.0 * (phase - mondphase(t)).rem_euclid(360.0);
    umkehren(mondphase, phase, t.max(tau - 2.0), tau + 2.0)
}

/// Der erste Zeitpunkt ab `t`, an dem die Sonne die Länge `lambda` Grad erreicht.
pub fn sonnenlaenge_ab(lambda: f64, t: f64) -> f64 {
    let tau = t + TROPISCHES_JAHR / 360.0 * (lambda - sonnenlaenge(t)).rem_euclid(360.0);
    umkehren(sonnenlaenge, lambda, t.max(tau - 5.0), tau + 5.0)
}

// ----- Teil 2: Sonnenstand am Ort -----

/// Schiefe der Ekliptik in Grad (Neigung der Erdachse, etwa 23,44°).
fn schiefe(t: f64) -> f64 {
    23.0 + 26.0 / 60.0 + 21.448 / 3600.0 + poly(jahrhunderte(t), &[0.0, -46.8150 / 3600.0, -0.00059 / 3600.0, 0.001813 / 3600.0])
}

/// Deklination in Grad (−90 bis 90) eines Punkts mit ekliptikaler Breite `beta` und Länge `lambda`.
pub fn deklination(t: f64, beta: f64, lambda: f64) -> f64 {
    let e = schiefe(t).to_radians();
    (beta.to_radians().sin() * e.cos() + beta.to_radians().cos() * e.sin() * lambda.to_radians().sin()).asin().to_degrees()
}

/// Zeitgleichung in Tagen: wahre minus mittlere Sonnenzeit (höchstens ein halber Tag).
fn zeitgleichung(t: f64) -> f64 {
    let c = jahrhunderte(t);
    let lambda = poly(c, &[280.46645, 36000.76983, 0.0003032]);
    let anomalie = poly(c, &[357.52910, 35999.05030, -0.0001559, -0.00000048]);
    let exz = poly(c, &[0.016708617, -0.000042037, -0.0000001236]);
    let y = (schiefe(t) / 2.0).to_radians().tan().powi(2);
    let g = (y * sin_grad(2.0 * lambda) - 2.0 * exz * sin_grad(anomalie) + 4.0 * exz * y * sin_grad(anomalie) * (2.0 * lambda).to_radians().cos()
        - 0.5 * y * y * sin_grad(4.0 * lambda)
        - 1.25 * exz * exz * sin_grad(2.0 * anomalie))
        / (2.0 * std::f64::consts::PI);
    g.signum() * g.abs().min(0.5)
}

/// Refraktion samt Kimmtiefe in Grad für eine Höhe in Metern (auf Meereshöhe 34′).
pub(crate) fn refraktion(hoehe: f64) -> f64 {
    let h = hoehe.max(0.0);
    let r = 6.372e6;
    34.0 / 60.0 + (r / (r + h)).acos().to_degrees() + 19.0 / 3600.0 * h.sqrt()
}

/// Weltzeit aus mittlerer Ortszeit der Länge `laenge` (Grad, Osten positiv).
pub fn welt_aus_lokal(t: f64, laenge: f64) -> f64 {
    t - laenge / 360.0
}

/// Mittlere Ortszeit aus wahrer (scheinbarer) Sonnenzeit.
pub fn lokal_aus_scheinbar(t: f64, laenge: f64) -> f64 {
    t - zeitgleichung(welt_aus_lokal(t, laenge))
}

/// Sinus des Abstands zwischen der Sonne zur Ortszeit `t` und dem Stand mit der
/// Tiefe `alpha` unter dem Horizont. Über 1: Die Sonne bleibt darüber; unter −1: darunter.
pub fn sinus_abstand(t: f64, breite: f64, laenge: f64, alpha: f64) -> f64 {
    let u = welt_aus_lokal(t, laenge);
    let delta = deklination(u, 0.0, sonnenlaenge(u)).to_radians();
    let phi = breite.to_radians();
    phi.tan() * delta.tan() + alpha.to_radians().sin() / (delta.cos() * phi.cos())
}

/// Ungefährer Zeitpunkt (mittlere Ortszeit) nahe `t`, an dem die Sonne die Tiefe
/// `alpha` erreicht, morgens oder abends; `None`, wenn sie das an diesem Tag nicht tut.
fn tiefe_ungefaehr(t: f64, breite: f64, laenge: f64, alpha: f64, morgens: bool) -> Option<f64> {
    let tag = t.floor();
    let ersatz = if alpha >= 0.0 { if morgens { tag } else { tag + 1.0 } } else { tag + 0.5 };
    let mut wert = sinus_abstand(t, breite, laenge, alpha);
    if wert.abs() > 1.0 {
        wert = sinus_abstand(ersatz, breite, laenge, alpha);
    }
    if wert.abs() > 1.0 {
        return None;
    }
    let versatz = (wert.asin().to_degrees().rem_euclid(360.0) / 360.0 + 0.5).rem_euclid(1.0) - 0.5;
    Some(lokal_aus_scheinbar(tag + if morgens { 0.25 - versatz } else { 0.75 + versatz }, laenge))
}

/// Größter Abstand zweier Schritte, bei dem die Suche nach der Tiefe endet: 30 Sekunden.
pub const TIEFE_GENAU: f64 = 30.0 / 86400.0;

/// Der Zeitpunkt (mittlere Ortszeit) nahe `t`, an dem die Sonne die Tiefe `alpha`
/// erreicht (negativ: über dem Horizont), morgens oder abends; `None`, wenn sie das
/// an diesem Tag nicht tut. `genau`: Abbruch, sobald zwei Schritte so nah beieinander liegen.
pub fn tiefe_mit(t: f64, breite: f64, laenge: f64, alpha: f64, morgens: bool, genau: f64) -> Option<f64> {
    let mut alt = t;
    for _ in 0..10 {
        let neu = tiefe_ungefaehr(alt, breite, laenge, alpha, morgens)?;
        if (alt - neu).abs() < genau {
            return Some(neu);
        }
        alt = neu;
    }
    // Ohne Zusammenlaufen (nahe der Polargrenze) gibt es keine Zeit statt einer geratenen.
    None
}

/// [`tiefe_mit`] mit der Genauigkeit [`TIEFE_GENAU`].
pub fn tiefe(t: f64, breite: f64, laenge: f64, alpha: f64, morgens: bool) -> Option<f64> {
    tiefe_mit(t, breite, laenge, alpha, morgens, TIEFE_GENAU)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Vergleichswerte aus den Tests der Quelle (Lisp-Code von Reingold & Dershowitz),
    // nur die Tage 1800–2050, für die ΔT hier gleich gerechnet wird (ab 2051 berichtigtes Vorzeichen).
    const TAGE: [f64; 8] = [694799.0, 704424.0, 708842.0, 709409.0, 709580.0, 727274.0, 728714.0, 744313.0];

    fn nah(ist: f64, soll: f64) {
        // Absolut: 1e-6 Tage (unter 0,1 s) bei Zeitpunkten, 1e-6 Grad bei Längen.
        assert!((ist - soll).abs() <= 1e-6, "ist {ist}, soll {soll}");
    }

    #[test]
    fn gleich_wie_die_quelle() {
        let sonne = [
            28.25199345351575,
            151.7806330331332,
            185.94586701843946,
            28.55560762159439,
            193.3478921554779,
            357.15125499424175,
            336.1706924761211,
            228.18487947607719,
        ];
        let mond = [
            287.9877436469169,
            25.626707154435444,
            290.28830064619893,
            189.91314245171338,
            284.93173002623826,
            152.3390442635215,
            51.66226507971774,
            26.68206023138705,
        ];
        let neu = [
            694807.5633711396,
            704433.4911827276,
            708863.5970001582,
            709424.4049294397,
            709602.0826867367,
            727291.2094001573,
            728737.4476913146,
            744329.5739998783,
        ];
        for (i, tag) in TAGE.into_iter().enumerate() {
            nah(sonnenlaenge(tag + 0.5), sonne[i]);
            nah(mondlaenge(jahrhunderte(tag)), mond[i]);
            nah(neumond(neumond_ab(tag)), neu[i]);
        }
        // Der 0. Neumond liegt am 11.1. des Jahres 1.
    }

    /// ΔT schließt 2050/51 stetig an (mit dem Plus der Quelle sprang es von 93 auf 206 s).
    #[test]
    fn delta_t_stetig() {
        let mitte = |j: i32| gregorian::fixed_from_gregorian(j, 7, 1).to_i64_date() as f64;
        let sekunden = |j: i32| delta_t(mitte(j)) * 86400.0;
        // Je Jahr wächst ΔT hier um etwa 2 s; ein Sprung wäre über 100 s.
        assert!((sekunden(2051) - sekunden(2050)).abs() < 3.0, "{} {}", sekunden(2050), sekunden(2051));
        assert!((sekunden(2050) - 93.0).abs() < 1.0 && (sekunden(2150) - (-20.0 + 32.0 * 3.3 * 3.3)).abs() < 1.0, "{} {}", sekunden(2050), sekunden(2150));
        for j in 2051..2150 {
            assert!(sekunden(j + 1) > sekunden(j), "steigt {j}");
        }
    }

    /// Sonnenuntergang in Jerusalem (Höhe 740 m, Weltzeit + 2 h) wie im Test `check_sunset`
    /// der Quelle; die Werte stammen aus dem Lisp-Code, der bei 30 Sekunden abbricht.
    fn untergang_jerusalem(tag: f64, genau: f64) -> f64 {
        let (breite, laenge) = (31.78, 35.24);
        let lokal = tiefe_mit(tag + 0.75, breite, laenge, refraktion(740.0) + 16.0 / 60.0, false, genau).unwrap();
        welt_aus_lokal(lokal, laenge) + 2.0 / 24.0
    }

    const UNTERGANG: [f64; 8] =
        [694799.7602310368, 704424.7619096127, 708842.730647343, 709409.7603906896, 709580.7240122546, 727274.745361792, 728714.734750938, 744313.699821144];

    #[test]
    fn untergang_wie_die_quelle() {
        for (tag, soll) in TAGE.into_iter().zip(UNTERGANG) {
            let ist = untergang_jerusalem(tag, TIEFE_GENAU);
            assert!((ist - soll).abs() * 86400.0 < 1.0, "{tag}: {} s", (ist - soll) * 86400.0);
        }
        // Schiefe am Tag 727274 aus `check_obliquity`.
        assert!((schiefe(727274.0) - 23.440304562352033).abs() < 1e-9);
    }

    /// Mit dem Abbruch der Quelle (`30.0` Tage) endet die Suche nach dem ersten
    /// Schritt; das weicht hier messbar ab, an hohen Breiten um Minuten.
    #[test]
    fn abbruch_nach_30_sekunden() {
        let mut groesste = 0.0_f64;
        for tag in TAGE {
            groesste = groesste.max((untergang_jerusalem(tag, 30.0) - untergang_jerusalem(tag, TIEFE_GENAU)).abs() * 86400.0);
        }
        assert!(groesste > 1.0, "Jerusalem: {groesste} s");
        // Breite 60–68° Nord über ein Jahr: Aufgang und Untergang, größte Abweichung.
        let alpha = refraktion(0.0) + 16.0 / 60.0;
        let anfang = gregorian::fixed_from_gregorian(2027, 1, 1).to_i64_date() as f64;
        let mut hoch = 0.0_f64;
        for breite in [60.0, 63.0, 66.0, 68.0] {
            for d in 0..365 {
                let tag = anfang + f64::from(d);
                for (start, morgens) in [(0.25, true), (0.75, false)] {
                    if let (Some(g), Some(f)) = (tiefe_mit(tag + start, breite, 19.0, alpha, morgens, 30.0), tiefe(tag + start, breite, 19.0, alpha, morgens)) {
                        hoch = hoch.max((g - f).abs() * 1440.0);
                    }
                }
            }
        }
        // Gemessen: Jerusalem 1,6 s, hohe Breiten 7,7 min.
        assert!(hoch > 5.0, "hohe Breiten: {hoch} min");
    }

    #[test]
    fn null_neumond_am_11_januar() {
        assert_eq!(neumond(0) as i32, 11);
    }

    #[test]
    fn phasen_und_laengen_treffen_ihr_ziel() {
        // Der Mond wandert etwa 0,5 Grad je Stunde gegen die Sonne: 0,01 Grad sind gut eine Minute.
        for tag in TAGE {
            for ziel in [0.0, 90.0, 180.0, 270.0] {
                let p = phase_ab(ziel, tag);
                assert!(p >= tag && p < tag + SYNODISCHER_MONAT + 1.0);
                let d = (mondphase(p) - ziel + 180.0).rem_euclid(360.0) - 180.0;
                assert!(d.abs() < 0.01, "Phase {ziel} ab {}: {d}", tag);
                let s = sonnenlaenge_ab(ziel, tag);
                assert!(s >= tag && s < tag + TROPISCHES_JAHR + 1.0);
                let d = (sonnenlaenge(s) - ziel + 180.0).rem_euclid(360.0) - 180.0;
                assert!(d.abs() < 1e-3, "Sonne {ziel} ab {}: {d}", tag);
            }
        }
    }
}
