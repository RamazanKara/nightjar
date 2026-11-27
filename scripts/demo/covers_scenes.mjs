// Procedural 7.0 console poster scenes for DemoConsole.LIBRARY titles 7..36 (600x900 2:3).
import { Art, poly, puff, flag } from './covers.mjs';
export const extraScenes = {};
// 7. Strategy — hex map at dusk.
extraScenes['strategy'] = () => {
  const a = new Art(600, 900, 47);
  a.bg(a.lin([[0, '#081620'], [0.5, '#11343c'], [1, '#2b2a1c']]));
  const R = 44; const dx = R * 1.732; const dy = R * 1.5; const cols = ['#14566a', '#1f6a4f', '#2f7f55', '#8a9a55', '#c9a45a'];
  for (let row = 0; row < 11; row++) for (let col = -1; col < 9; col++) {
    const x = col * dx + (row % 2 ? dx / 2 : 0); const y = 240 + row * dy;
    a.path(poly([0, 1, 2, 3, 4, 5].map((i) => [x + R * Math.sin((i * Math.PI) / 3), y + R * Math.cos((i * Math.PI) / 3)])), cols[Math.floor(a.r() * a.r() * 5)], 1, 'stroke="#081620" stroke-width="3"');
  }
  a.glow(300, 500, 330, '#ffc765', 0.5); a.line('M150,640 C230,560 300,560 400,420', '#ffc765', 7, 1, 'stroke-dasharray="4 20"');
  a.path(poly([[400, 420], [370, 430], [392, 456]]), '#ffc765');
  flag(a, 130, 660, '#58b7ff'); flag(a, 196, 700, '#58b7ff'); flag(a, 90, 720, '#58b7ff');
  flag(a, 420, 410, '#ff4a5e'); flag(a, 484, 370, '#ff4a5e'); flag(a, 500, 450, '#ff4a5e');
  a.bg(a.lin([[0.14, '#081620', 1], [0.32, '#081620', 0]]));
  a.text(['STRATEGY'], { x: 300, y: 128, size: 70, anchor: 'middle', track: 4, heavy: 1.4, fill: '#fff4dc' });
  return a.finish({ vignette: 0.3 });
};
// 8. Music Studio — vinyl grooves and wave ribbons.
extraScenes['music-studio'] = () => {
  const a = new Art(600, 900, 23);
  a.bg(a.lin([[0, '#1a0a3d'], [0.5, '#4a1272'], [1, '#d42c7f']], 0, 0, 0.6, 1));
  a.glow(150, 660, 430, '#ff5ca8', 0.7); a.glow(520, 190, 340, '#7d4dff', 0.8);
  const cx = 420; const cy = 290;
  a.circle(cx, cy, 256, a.rad(cx, cy, 256, [[0, '#120726'], [0.82, '#1a0b34'], [1, '#2d1353']]));
  for (let r = 92; r < 252; r += 6) a.add(`<circle cx="${cx}" cy="${cy}" r="${r}" fill="none" stroke="#fff" stroke-width=".8" opacity="${0.03 + (r % 24 === 0 ? 0.07 : 0.02)}"/>`);
  a.path(`M${cx},${cy} L${cx + 244},${cy - 156} A290,290 0 0 1 ${cx + 274},${cy - 68} Z`, '#ffd1ec', 0.16);
  a.path(`M${cx},${cy} L${cx - 244},${cy + 156} A290,290 0 0 1 ${cx - 274},${cy + 68} Z`, '#ffd1ec', 0.12);
  a.circle(cx, cy, 82, a.lin([[0.2, '#ffbf5e'], [0.5, '#ff5ca8']], 0, 0, 0, 1)); a.circle(cx, cy, 11, '#120726');
  for (let i = 0; i < 32; i++) {
    const t = i / 31; const base = 490 + t * 320; let d = '';
    for (let k = 0; k <= 100; k++) {
      const x = (k / 100) * 620 - 10; const u = x / 600; const env = Math.exp(-((u - 0.42) ** 2) / 0.05);
      const y = base - env * (124 * (1 - t * 0.55)) * (0.65 + 0.35 * Math.sin(u * 19 + t * 5)) - Math.sin(u * 6 + t * 2.2) * 15;
      d += (k ? ' L' : 'M') + x.toFixed(1) + ',' + y.toFixed(1);
    }
    a.path(d + ' L610,900 L-10,900 Z', '#150730', 0.26);
    a.line(d, a.lin([[0, '#ff7ab8'], [0.5, '#ffd27a'], [1, '#8f7dff']], 0, 0, 1, 0), 1.7, 0.4 + t * 0.6);
  }
  a.scrimBottom('#100521', 0.62, 0.66);
  a.text(['Music', 'Studio'], { x: 56, y: 748, size: 88, lead: 0.9, heavy: 1.2, track: -2, fill: '#fff5fb' });
  return a.finish({ vignette: 0.28 });
};
// 9. Ocean Explorer — light shafts, reef, submersible.
extraScenes['ocean-explorer'] = () => {
  const a = new Art(600, 900, 77);
  a.bg(a.lin([[0, '#c2faee'], [0.14, '#58d8d4'], [0.36, '#118ea6'], [0.66, '#07476f'], [1, '#021426']])); a.glow(300, -40, 520, '#f4fff6', 0.9);
  for (let i = 0; i < 9; i++) {
    const x = 30 + i * 66 + a.r() * 30; const wd = 18 + a.r() * 46; const lean = 90 + a.r() * 80;
    a.path(`M${x},-10 L${x + wd},-10 L${x + wd + lean},820 L${x + lean - wd * 0.8},820 Z`, a.lin([[0, '#eafff7', 0.34], [0.72, '#c8fff0', 0.02], [0.9, '#c8fff0', 0]]));
  }
  let fish = '';
  for (let i = 0; i < 46; i++) { const t = a.r(); const x = 90 + t * 300 + a.r() * 70; const y = 400 + Math.sin(t * 6) * 44 + a.r() * 60; const s = 5 + a.r() * 5; fish += `M${x},${y} q${s},${-s * 0.55} ${s * 2.2},0 q${-s * 1.2},${s * 0.55} ${-s * 2.2},0 Z M${x},${y} l${-s * 0.8},${-s * 0.42} l0,${s * 0.84} Z `; }
  a.path(fish, '#032f4a', 0.82);
  const sx = 386; const sy = 556; const beamId = a.id();
  a.defs.push(`<linearGradient id="${beamId}" gradientUnits="userSpaceOnUse" x1="${sx - 60}" y1="0" x2="${sx - 360}" y2="0"><stop offset="0" stop-color="#fff2b5" stop-opacity=".78"/><stop offset="1" stop-color="#fff2b5" stop-opacity="0"/></linearGradient>`);
  a.path(`M${sx - 60},${sy + 4} L${sx - 360},${sy - 54} L${sx - 360},${sy + 150} Z`, `url(#${beamId})`);
  a.add(`<ellipse cx="${sx}" cy="${sy}" rx="66" ry="34" fill="#f2a93b"/><path d="M${sx - 66},${sy} A66,34 0 0 0 ${sx + 66},${sy} Z" fill="#b76a14"/>`);
  a.rect(sx - 20, sy - 50, 40, 22, '#e8962a', 1, 7); a.rect(sx + 62, sy - 18, 24, 36, '#b76a14', 1, 6);
  a.circle(sx - 38, sy - 4, 13, '#063049'); a.circle(sx - 38, sy - 4, 8.5, '#bff5ea');
  a.circle(sx - 2, sy - 4, 9, '#063049'); a.circle(sx - 2, sy - 4, 5.5, '#bff5ea'); a.circle(sx - 66, sy + 2, 6, '#fff2b5');
  for (let i = 0; i < 16; i++) a.add(`<circle cx="${sx + 88 + a.r() * 40}" cy="${sy - 30 - i * 19 - a.r() * 8}" r="${2 + a.r() * 5}" fill="none" stroke="#d6fff4" stroke-width="1.3" opacity="${0.75 - i * 0.03}"/>`);
  a.path(a.ridge(748, 70, 0.62), '#053a58'); a.path(a.ridge(806, 70, 0.66), '#03233c');
  for (let i = 0; i < 12; i++) {
    const x = 24 + a.r() * 552; const ht = 120 + a.r() * 170; const wob = 16 + a.r() * 22; const ph = a.r() * 6; let d = `M${x},900`;
    for (let k = 1; k <= 8; k++) d += ` L${(x + Math.sin(ph + k * 0.9) * wob * (k / 8)).toFixed(1)},${(900 - (ht * k) / 8).toFixed(1)}`;
    a.line(d, '#021a2d', 5 + a.r() * 5, 1);
  }
  a.path(a.ridge(868, 34, 0.6), '#01101f');
  a.text(['OCEAN'], { x: 300, y: 126, size: 92, anchor: 'middle', track: 6, heavy: 1, fill: '#032840', shadow: null });
  a.text(['EXPLORER'], { x: 300, y: 174, size: 32, anchor: 'middle', track: 13, fill: '#032840', shadow: null });
  return a.finish({ vignette: 0.25 });
};
// 10. Skybound — sailplane above sunset clouds.
extraScenes['skybound'] = () => {
  const a = new Art(600, 900, 28);
  a.bg(a.lin([[0, '#0f3278'], [0.4, '#3f7fd0'], [0.66, '#f5b7b0'], [1, '#ffdca8']]));
  a.glow(150, 560, 340, '#fff3cf', 0.95); a.circle(150, 560, 58, '#fffbe8'); a.line('M-20,640 C150,590 300,470 410,350', '#ffffff', 5, 0.75);
  const x = 414; const y = 346;
  a.path(poly([[x + 74, y - 58], [x + 10, y + 20], [x - 20, y + 12], [x - 2, y - 14]]), '#f4f7ff');
  a.path(poly([[x - 90, y - 44], [x + 112, y + 2], [x + 30, y + 12], [x - 10, y - 4]]), '#ffffff');
  a.path(poly([[x + 112, y + 2], [x + 30, y + 12], [x - 90, y - 44], [x, y + 30]]), '#a9bfe8', 0.8);
  a.path(poly([[x - 26, y + 8], [x - 58, y - 28], [x - 44, y + 14]]), '#ff6a5c');
  for (const [cx, cy, s, c] of [[420, 520, 1.4, '#ffe4d6'], [10, 640, 1.8, '#ffeadb'], [320, 650, 2, '#fff2e6'], [-30, 740, 2.6, '#fff8ef'], [260, 770, 3, '#ffffff']]) puff(a, cx, cy, s, c, 1);
  a.text(['Skybound'], { x: 300, y: 142, size: 84, anchor: 'middle', heavy: 1.2, track: -1.5, fill: '#ffffff' });
  return a.finish({ vignette: 0.16, grain: 0.08 });
};
// 11. Ashfall — volcano and lava fissures.
extraScenes['ashfall'] = () => {
  const a = new Art(600, 900, 54);
  a.bg(a.lin([[0, '#0d0c10'], [0.36, '#2a1c1e'], [0.58, '#6c2a1a'], [0.7, '#c9471a'], [1, '#c9471a']]));
  for (let i = 0; i < 26; i++) a.glow(300 + (a.r() - 0.5) * 320 * (1 - i / 40), 430 - i * 20 + a.r() * 40, 80 + i * 8, i < 8 ? '#a1401e' : '#3b3136', 0.5);
  a.glow(300, 460, 280, '#ff7a2a', 0.9);
  a.path('M-20,900 L150,660 L255,470 L285,452 L320,456 L350,478 L465,680 L620,900 Z', a.lin([[0.5, '#2e1414'], [1, '#0a0507']]));
  const lava = (d, w) => { a.line(d, '#ff6a1a', w + 6, 0.7, a.blur(5)); a.line(d, '#ffb347', w); a.line(d, '#fff3c4', w * 0.35); };
  lava('M300,456 C296,520 322,560 305,620 C292,668 266,700 280,770 C290,820 264,860 268,906', 7);
  lava('M308,620 C344,650 354,700 384,740 C410,776 420,840 446,906', 4);
  lava('M284,700 C244,736 226,790 196,830 C174,860 168,884 162,906', 3.4);
  a.path(a.ridge(856, 44, 0.6), '#060305');
  for (let i = 0; i < 120; i++) a.circle(a.r() * 600, a.r() * 900, 0.6 + a.r() ** 3 * 2.6, a.r() > 0.82 ? '#ffb347' : '#b8b2ad', 0.2 + a.r() * 0.6);
  a.text(['ASHFALL'], { x: 300, y: 132, size: 82, anchor: 'middle', track: 4, heavy: 1.8, fill: '#ece4da' });
  return a.finish({ vignette: 0.35 });
};
// 12. Brightwater — sunlit lagoon (bright stress case).
extraScenes['brightwater'] = () => {
  const a = new Art(600, 900, 16);
  a.bg(a.lin([[0, '#fffbe8'], [0.34, '#fff3c0'], [0.48, '#bdf0e4'], [0.72, '#35c6c8'], [1, '#0b7f9e']]));
  a.glow(150, 190, 300, '#ffffff', 1); a.circle(150, 190, 76, '#fff');
  a.path(a.hill(430, [[22, 1.2, 1], [10, 3, 2.5]]), '#8ad6a6'); a.path(a.hill(452, [[12, 1.6, 4], [6, 4.2, 1]]), '#4cb68f');
  a.rect(0, 462, 600, 440, a.lin([[0.51, '#a4efe2'], [0.7, '#37c8c9'], [1, '#0b7f9e']]));
  for (let i = 0; i < 46; i++) { const y = 480 + a.r() ** 1.5 * 400; const w = 24 + ((y - 470) / 420) * 120; a.rect(a.r() * 640 - 30, y, w, 2 + ((y - 470) / 420) * 4, '#fff', 0.5, 2); }
  const x = 372; const y = 620;
  a.path(`M${x - 92},${y} L${x + 92},${y} L${x + 62},${y + 42} L${x - 62},${y + 42} Z`, '#f2a93b');
  a.path(`M${x - 4},${y - 8} L${x - 4},${y - 240} L${x - 112},${y - 8} Z`, '#ffffff');
  a.path(`M${x + 6},${y - 8} L${x + 6},${y - 196} L${x + 88},${y - 8} Z`, '#ff6a5c');
  a.path(`M${x - 70},${y + 46} Q${x},${y + 66} ${x + 70},${y + 46} L${x + 70},${y + 90} L${x - 70},${y + 90} Z`, '#0b7f9e', 0.35);
  a.text(['Bright', 'water'], { x: 544, y: 112, size: 86, lead: 0.9, heavy: 1.1, track: -2, fill: '#074860', anchor: 'end', shadow: null });
  return a.finish({ vignette: 0, grain: 0.05 });
};
// 13. Cinder Road — rider on an ember highway.
extraScenes['cinder-road'] = () => {
  const a = new Art(600, 900, 61);
  a.bg(a.lin([[0, '#100a0c'], [0.3, '#35151a'], [0.5, '#a33a18'], [0.6, '#ff8d3a'], [1, '#ff8d3a']])); a.glow(300, 520, 330, '#ffb55c', 0.9);
  a.path(a.ridge(510, 60, 0.6), '#4a1a14'); a.path(a.ridge(540, 34, 0.55), '#1c0b0d');
  a.path('M0,548 L600,548 L600,900 L0,900 Z', a.lin([[0.6, '#1c0b0d'], [1, '#050304']]));
  a.path(poly([[290, 548], [310, 548], [560, 900], [40, 900]]), a.lin([[0.6, '#5a2a22'], [1, '#17090b']]));
  for (let i = 0; i < 9; i++) { const t = (i / 9) ** 1.8; const y = 552 + t * 340; const hw = 1 + t * 7; a.path(poly([[300 - hw, y], [300 + hw, y], [300 + hw * 1.3, y + 8 + t * 40], [300 - hw * 1.3, y + 8 + t * 40]]), '#ffcf8f', 0.9); }
  const k = '#050304';
  a.path('M270,740 C268,690 278,664 300,658 C322,664 332,690 330,740 L322,784 L278,784 Z', k);
  a.circle(300, 642, 22, k); a.rect(290, 780, 20, 44, k, 1, 6);
  a.add(`<rect x="280" y="736" width="40" height="10" rx="4" fill="#ff2b3a" ${a.blur(6)}/>`); a.rect(284, 738, 32, 6, '#ffb0a8', 1, 3);
  for (let i = 0; i < 110; i++) a.circle(a.r() * 600, a.r() * 760, 0.7 + a.r() ** 2 * 2.6, a.r() > 0.4 ? '#ffb347' : '#ff6a2a', 0.35 + a.r() * 0.6);
  a.text(['CINDER', 'ROAD'], { x: 300, y: 128, size: 92, lead: 0.92, anchor: 'middle', skew: -6, heavy: 1.8, fill: '#fff0dc' });
  return a.finish({ vignette: 0.32 });
};
// 14. Deep Field — aurora over a radio telescope.
extraScenes['deep-field'] = () => {
  const a = new Art(600, 900, 71);
  a.bg(a.lin([[0, '#040818'], [0.6, '#0b1a3a'], [1, '#143a4a']])); a.stars(320, 720, '#e6f2ff', 1.5);
  const f = a.blur(16);
  const curtain = (y0, amp, ph, col, h, op) => {
    for (let i = 0; i < 70; i++) {
      const x = (i / 70) * 640 - 20; const y = y0 + Math.sin(i * 0.17 + ph) * amp + Math.sin(i * 0.05 + ph * 2) * amp * 1.4;
      const hh = h * (0.6 + 0.4 * Math.sin(i * 0.5 + ph));
      a.add(`<rect x="${x}" y="${y - hh}" width="11" height="${hh}" fill="${a.lin([[(y - hh) / 900, col, 0], [y / 900, col, op]])}" ${f}/>`);
    }
  };
  curtain(430, 44, 0.4, '#6d4dff', 300, 0.5); curtain(490, 40, 2.1, '#37e0a8', 320, 0.72); curtain(540, 34, 3.8, '#9df2c8', 170, 0.45);
  a.path(a.ridge(712, 60, 0.55), '#0a2233'); a.path(a.ridge(772, 60, 0.55), '#051421');
  const cx = 300; const cy = 690; const k = '#02070f';
  a.add(`<g transform="rotate(-28 ${cx} ${cy})"><path d="M${cx - 130},${cy - 34} Q${cx},${cy + 92} ${cx + 130},${cy - 34} Q${cx},${cy + 22} ${cx - 130},${cy - 34} Z" fill="${k}"/><path d="M${cx},${cy + 32} L${cx},${cy - 92}" stroke="${k}" stroke-width="7"/><circle cx="${cx}" cy="${cy - 96}" r="8" fill="#f2a93b"/></g>`);
  a.path(`M${cx - 18},${cy + 40} L${cx - 62},${cy + 180} L${cx + 62},${cy + 180} L${cx + 18},${cy + 40} Z`, k); a.path(a.ridge(862, 26, 0.5), k);
  a.text(['DEEP', 'FIELD'], { x: 56, y: 118, size: 70, lead: 1.02, track: 10, weight: 600, fill: '#eafff6' });
  return a.finish({ vignette: 0.25 });
};
// 15. Ember Keep — citadel on a crag.
extraScenes['ember-keep'] = () => {
  const a = new Art(600, 900, 66);
  a.bg(a.lin([[0, '#1a0608'], [0.3, '#5c1210'], [0.56, '#d4421a'], [0.74, '#ffa63d'], [1, '#ffa63d']]));
  a.glow(300, 470, 430, '#ffc05c', 0.9); a.circle(300, 440, 150, a.rad(300, 440, 150, [[0, '#fff2c4'], [0.8, '#ffc765'], [1, '#ff9440']]));
  a.path(a.ridge(640, 80, 0.56), '#8c2614'); a.path(a.ridge(700, 80, 0.56), '#4f130e');
  const k = '#16060a';
  a.path('M60,900 L120,720 L170,690 L210,620 L270,596 L340,600 L400,640 L440,700 L520,760 L600,900 Z', k);
  const tower = (x, top, w) => { a.rect(x - w / 2, top, w, 700 - top, k); const m = w / 5; for (let i = 0; i < 3; i++) a.rect(x - w / 2 + i * 2 * m, top - m * 1.2, m, m * 1.2, k); };
  tower(226, 470, 48); tower(366, 500, 44); tower(296, 396, 62); a.rect(226, 548, 150, 120, k);
  for (let i = 0; i < 8; i++) a.rect(230 + i * 19, 536, 11, 14, k);
  a.path('M296,330 L296,396 M296,332 L340,346 L296,360', 'none', 1, `stroke="${k}" stroke-width="4"`); a.path('M296,333 L342,347 L296,361 Z', k);
  for (const [x, y, w, h] of [[292, 444, 9, 22], [222, 508, 7, 18], [362, 534, 7, 18], [260, 594, 7, 16], [330, 594, 7, 16]]) { a.rect(x, y, w, h, '#ffb347'); a.glow(x + w / 2, y + h / 2, 24, '#ff8a2a', 0.8); }
  a.path(a.ridge(830, 40, 0.5), '#080305');
  for (let i = 0; i < 90; i++) a.circle(a.r() * 600, 240 + a.r() * 660, 0.8 + a.r() ** 2 * 3, a.r() > 0.5 ? '#ffd27a' : '#ff6a2a', 0.5 + a.r() * 0.5);
  a.text(['EMBER KEEP'], { x: 300, y: 118, size: 56, anchor: 'middle', track: 4, heavy: 1.4, fill: '#fff0d6' }); a.rect(170, 142, 260, 2, '#ffc765', 0.8);
  return a.finish({ vignette: 0.3 });
};
// 16. Frostline — crystalline glacier facets (bright stress case).
extraScenes['frostline'] = () => {
  const a = new Art(600, 900, 88); const cols = 8; const rows = 12; const pts = [];
  for (let y = 0; y <= rows; y++) { pts[y] = []; for (let x = 0; x <= cols; x++) pts[y][x] = [(x / cols) * 600 + (x % cols ? (a.r() - 0.5) * 50 : 0), (y / rows) * 900 + (y % rows ? (a.r() - 0.5) * 50 : 0)]; }
  const mix = (c1, c2, t) => { const h = (c, i) => parseInt(c.slice(1 + i * 2, 3 + i * 2), 16); return `rgb(${[0, 1, 2].map((i) => Math.round(h(c1, i) + (h(c2, i) - h(c1, i)) * t)).join(',')})`; };
  for (let y = 0; y < rows; y++) for (let x = 0; x < cols; x++) {
    const [p, q, r, s] = [pts[y][x], pts[y][x + 1], pts[y + 1][x + 1], pts[y + 1][x]];
    for (const tri of [[p, q, r], [p, r, s]]) {
      const cy = (tri[0][1] + tri[1][1] + tri[2][1]) / 2700; const cx = (tri[0][0] + tri[1][0] + tri[2][0]) / 1800;
      const light = Math.max(0, Math.min(1, cy * 1.15 + (cx - 0.5) * 0.28 + (a.r() - 0.5) * 0.22));
      const c = light < 0.5 ? mix('#ffffff', '#bfe9ff', light * 2) : mix('#bfe9ff', '#2d7ec4', (light - 0.5) * 2);
      a.path(`M${tri[0]} L${tri[1]} L${tri[2]} Z`, c, 1, `stroke="${c}" stroke-width=".6"`);
    }
  }
  a.glow(180, 140, 240, '#ffffff', 0.85); a.path(a.ridge(790, 60, 0.45), '#114a86', 0.92); a.path(a.ridge(840, 40, 0.4), '#0a2c5c');
  a.text(['Frost', 'line'], { x: 56, y: 136, size: 102, lead: 0.88, heavy: 1.4, track: -2.5, fill: '#082952', shadow: null });
  return a.finish({ vignette: 0, grain: 0.05 });
};
// 17. Glass Harbor — mirrored waterfront towers.
extraScenes['glass-harbor'] = () => {
  const a = new Art(600, 900, 72);
  a.bg(a.lin([[0, '#051a2c'], [0.42, '#0d5a78'], [0.6, '#7fe4dc'], [1, '#04202f']])); a.glow(320, 520, 280, '#c8fff0', 0.85);
  for (let x = -10; x < 610;) {
    const w = 44 + a.r() * 60; const h = 90 + a.r() * 190; const c = a.lin([[(540 - h) / 900, '#bff3ee', 0.95], [0.6, '#135a78', 0.9]]);
    a.rect(x, 540 - h, w, h, c);
    for (let y = 540 - h + 10; y < 530; y += 18) a.rect(x + 3, y, w - 6, 2, '#04202f', 0.3);
    a.rect(x, 544, w, h * 0.8, c, 0.28); x += w + 3;
  }
  a.rect(0, 538, 600, 3, '#eafffa', 0.9);
  for (let i = 0; i < 44; i++) a.rect(a.r() * 620 - 20, 548 + a.r() ** 1.4 * 330, 30 + a.r() * 90, 1.6, '#d6fff6', 0.34);
  a.path(poly([[230, 640], [370, 640], [346, 668], [254, 668]]), '#04121d'); a.rect(262, 616, 76, 24, '#04121d', 1, 3);
  for (let i = 0; i < 5; i++) a.rect(270 + i * 13, 623, 8, 8, '#ffc765');
  a.text(['Glass', 'Harbor'], { x: 56, y: 126, size: 92, lead: 0.92, heavy: 1.2, track: -2, fill: '#f0fffb' });
  return a.finish({ vignette: 0.24 });
};
// 18. Hollow Peak — lone peak and lit cavern.
extraScenes['hollow-peak'] = () => {
  const a = new Art(600, 900, 83);
  a.bg(a.lin([[0, '#0a0a26'], [0.45, '#2a2468'], [0.7, '#6f4ea6'], [1, '#1a1440']])); a.stars(190, 420, '#e8e6ff', 1.5);
  a.circle(120, 330, 46, '#f1ecff'); a.circle(138, 322, 42, '#2a2468');
  a.path(poly([[-40, 900], [300, 260], [640, 900]]), a.lin([[0.3, '#463c86'], [0.9, '#120e2e']]));
  a.path(poly([[300, 260], [640, 900], [420, 900], [340, 520]]), '#0e0a24', 0.6);
  a.path(poly([[300, 260], [372, 396], [340, 372], [316, 416], [292, 370], [262, 410], [234, 384]]), '#f1f0ff');
  a.glow(300, 690, 170, '#ffb347', 0.95);
  a.path('M258,780 C252,700 276,646 300,646 C324,646 348,700 342,780 Z', '#ffcf7a');
  a.path('M280,780 C276,724 290,690 300,690 C310,690 324,724 320,780 Z', '#fff3d0');
  a.path(a.ridge(800, 40, 0.55), '#07051a');
  a.text(['HOLLOW', 'PEAK'], { x: 300, y: 112, size: 78, lead: 0.98, anchor: 'middle', track: 6, weight: 600, fill: '#f4f1ff' });
  return a.finish({ vignette: 0.3 });
};
// 19. Iron Tide — ironclad in heavy seas.
extraScenes['iron-tide'] = () => {
  const a = new Art(600, 900, 38);
  a.bg(a.lin([[0, '#1c232b'], [0.35, '#46505a'], [0.52, '#d6743a'], [0.62, '#ffb169'], [1, '#ffb169']])); a.glow(430, 520, 250, '#ffd9a0', 0.8);
  for (let i = 0; i < 12; i++) puff(a, a.r() * 640 - 60, 210 + a.r() * 190, 1.4 + a.r() * 1.6, '#1c232b', 0.4);
  const k = '#11161c';
  a.path(poly([[70, 590], [530, 590], [490, 660], [120, 660]]), k); a.path(poly([[160, 590], [160, 550], [420, 550], [440, 590]]), k);
  a.rect(240, 480, 90, 72, k); a.rect(262, 420, 22, 62, k); a.rect(300, 440, 18, 42, k); a.rect(370, 524, 110, 9, k); a.rect(100, 540, 84, 8, k);
  for (let i = 0; i < 12; i++) puff(a, 262 + i * 18 + a.r() * 16, 408 - i * 15, 0.3 + i * 0.1, '#20262e', 0.72);
  for (let i = 0; i < 6; i++) a.circle(182 + i * 42, 572, 4.5, '#ffc765');
  [[640, '#31414f', 16], [684, '#222f3b', 22], [744, '#141e28', 28], [812, '#0a1219', 34]].forEach(([b, c, amp], i) => a.path(a.hill(b, [[amp, 3 + i, i * 1.3], [amp * 0.4, 8 + i, i * 2.1]]), c));
  a.text(['IRON', 'TIDE'], { x: 56, y: 128, size: 102, lead: 0.92, heavy: 2.2, track: 2, fill: '#f6efe6' });
  return a.finish({ vignette: 0.34 });
};
// 20. Jade Circuit — traces converging on a silicon core.
extraScenes['jade-circuit'] = () => {
  const a = new Art(600, 900, 90);
  a.bg(a.lin([[0, '#02140f'], [0.6, '#053226'], [1, '#02140f']])); a.glow(300, 500, 380, '#25d898', 0.5);
  for (let i = 0; i < 46; i++) {
    let x = 20 + (i % 23) * 25; let y = i < 23 ? 880 : 240; const dir = i < 23 ? -1 : 1; let d = `M${x},${y}`; const stop = 500 + dir * (70 + a.r() * 40);
    while ((stop - y) * dir > 0) {
      y += dir * (28 + a.r() * 40); if ((stop - y) * dir < 0) y = stop;
      d += ` L${x},${y}`; const nx = x + (300 - x) * (0.2 + a.r() * 0.3); y += dir * Math.abs(nx - x) * 0.6; x = nx; d += ` L${x.toFixed(1)},${y.toFixed(1)}`;
    }
    a.line(d, i % 5 ? '#29c98f' : '#a7ffd9', i % 5 ? 2.4 : 3.4, 0.8); a.circle(x, y, 5, '#a7ffd9');
  }
  a.rect(190, 390, 220, 220, '#02140f', 1, 18); a.add('<rect x="190" y="390" width="220" height="220" rx="18" fill="none" stroke="#a7ffd9" stroke-width="4"/>');
  a.glow(300, 500, 90, '#a7ffd9', 0.9); a.path(poly([[300, 442], [346, 500], [300, 558], [254, 500]]), '#eafff6');
  a.text(['JADE', 'CIRCUIT'], { x: 300, y: 112, size: 76, lead: 0.98, anchor: 'middle', track: 5, heavy: 1.2, fill: '#eafff6' });
  return a.finish({ vignette: 0.3 });
};
// 21. Kite Season — sunlit meadow and high kites.
extraScenes['kite-season'] = () => {
  const a = new Art(600, 900, 14);
  a.bg(a.lin([[0, '#49b4ea'], [0.55, '#c6ecf4'], [0.75, '#fff3cd'], [1, '#fff3cd']])); a.glow(480, 300, 240, '#fffbe2', 0.95);
  for (const [x, y, s] of [[40, 340, 0.8], [360, 500, 0.7], [90, 560, 0.6]]) puff(a, x, y, s, '#fff', 0.9);
  const kite = (x, y, s, c1, c2, tx, ty) => {
    a.line(`M${x},${y + 64 * s} Q${x + (tx - x) * 0.3},${y + 220 * s} ${tx},${ty}`, '#26485c', 1.8, 0.7);
    a.path(poly([[x, y - 70 * s], [x + 50 * s, y], [x, y + 64 * s]]), c1); a.path(poly([[x, y - 70 * s], [x - 50 * s, y], [x, y + 64 * s]]), c2);
    for (let i = 1; i < 4; i++) a.path(poly([[x + 10 * s, y + (64 + i * 28) * s], [x, y + (76 + i * 28) * s], [x - 10 * s, y + (64 + i * 28) * s], [x, y + (52 + i * 28) * s]]), c1);
  };
  kite(170, 410, 1.5, '#ff5c6a', '#ffc24a', 300, 690); kite(430, 340, 1.1, '#6a4dff', '#3fd0c8', 310, 690); kite(320, 530, 0.7, '#ff8a3a', '#ffe36b', 305, 690);
  a.path(a.hill(742, [[36, 0.8, 1.4], [10, 2.4, 0.4]]), '#7ec85c'); a.path(a.hill(796, [[26, 1.1, 3.4], [8, 3, 2]]), '#4ca34c');
  a.rect(298, 660, 12, 40, '#23324a', 1, 5); a.circle(304, 650, 12, '#23324a');
  a.text(['Kite', 'Season'], { x: 56, y: 122, size: 90, lead: 0.92, heavy: 1.2, track: -2, fill: '#082f4c', shadow: null });
  return a.finish({ vignette: 0, grain: 0.05 });
};
// 22. Lantern Row — warm lantern alley.
extraScenes['lantern-row'] = () => {
  const a = new Art(600, 900, 26);
  a.bg(a.lin([[0, '#140614'], [0.5, '#4a0f22'], [0.75, '#a32d1c'], [1, '#150608']])); a.glow(300, 520, 300, '#ff9e45', 0.75);
  a.path(poly([[0, 170], [240, 430], [240, 650], [0, 900]]), '#120410'); a.path(poly([[600, 170], [360, 430], [360, 650], [600, 900]]), '#0d030c');
  a.path(poly([[0, 900], [240, 650], [360, 650], [600, 900]]), a.lin([[0.72, '#6b2420'], [1, '#120508']]));
  [[-1, 1], [1, 0.72], [-1, 0.5], [1, 0.34], [-1, 0.22], [1, 0.14]].forEach(([side, s], i) => {
    const x = 300 + side * (28 + s * 210); const y = 500 - s * 200;
    a.line(`M${x - side * s * 150},${y - s * 100} Q${x},${y - s * 30} ${x + side * s * 70},${y - s * 90}`, '#120410', 1.5 + s * 2);
    a.glow(x, y + s * 40, 130 * s + 20, '#ff9e45', 0.75);
    a.add(`<ellipse cx="${x}" cy="${y + s * 40}" rx="${44 * s}" ry="${56 * s}" fill="${i % 2 ? '#ffb347' : '#ff5c3a'}"/><ellipse cx="${x}" cy="${y + s * 40}" rx="${18 * s}" ry="${50 * s}" fill="#ffe2a0" opacity=".65"/>`);
    a.rect(x - 16 * s, y - 20 * s, 32 * s, 8 * s, '#2a0a12'); a.rect(x - 16 * s, y + 92 * s, 32 * s, 8 * s, '#2a0a12');
  });
  a.path('M286,690 C284,650 290,630 300,628 C310,630 316,650 314,690 L312,740 L288,740 Z', '#0a0308'); a.circle(300, 616, 11, '#0a0308');
  a.text(['Lantern', 'Row'], { x: 300, y: 112, size: 92, lead: 0.92, anchor: 'middle', heavy: 1.2, track: -2, fill: '#fff0d6' });
  return a.finish({ vignette: 0.32 });
};
// 23. Moss and Stone — standing monoliths in forest mist.
extraScenes['moss-and-stone'] = () => {
  const a = new Art(600, 900, 57);
  a.bg(a.lin([[0, '#0d2a22'], [0.4, '#3a7a5c'], [0.6, '#cfe8b8'], [1, '#0d2a22']])); a.glow(300, 470, 300, '#f6ffd8', 0.9);
  for (const [b, c, amp] of [[470, '#5f9a70', 70], [520, '#3a7a5c', 60]]) {
    a.path(a.ridge(b, amp, 0.6), c); const rd = a.lastRidge; let d = '';
    for (let i = 0; i < 64; i++) { const p = rd[Math.floor(a.r() * rd.length)]; const h = 26 + a.r() * 48; d += `M${p[0]},${p[1] - h} L${p[0] + h * 0.22},${p[1] + 4} L${p[0] - h * 0.22},${p[1] + 4} Z `; }
    a.path(d, c);
  }
  a.path(a.hill(640, [[22, 0.9, 1], [8, 2.6, 2]]), '#1f5a3c');
  for (const [x, top, w] of [[120, 480, 92], [238, 420, 110], [372, 450, 100], [490, 500, 88]]) {
    const b = 720 + (a.r() - 0.5) * 30;
    a.path(poly([[x - w / 2, b], [x - w * 0.42, top + 26], [x - w * 0.1, top], [x + w * 0.36, top + 14], [x + w / 2, b]]), a.lin([[top / 900, '#8d9a92'], [b / 900, '#3a4840']], (x - w) / 600, 0, (x + w) / 600, 0));
    a.path(poly([[x - w / 2, b], [x - w * 0.42, top + 26], [x - w * 0.1, top], [x - w * 0.04, top + 40], [x - w * 0.24, b - 70], [x, b]]), '#5fae4f', 0.92);
  }
  a.path(a.hill(742, [[14, 1.1, 3.2], [8, 3.2, 1.2]]), '#0e3a24');
  a.text(['Moss and', 'Stone'], { x: 56, y: 118, size: 84, lead: 0.96, heavy: 1.2, track: -1.5, fill: '#f6ffe8' });
  return a.finish({ vignette: 0.28 });
};
// 24. Night Courier — rain-slick neon alley.
extraScenes['night-courier'] = () => {
  const a = new Art(600, 900, 33);
  a.bg(a.lin([[0, '#07081c'], [0.5, '#1a1050'], [0.74, '#7a1f8c'], [1, '#04050f']]));
  a.glow(300, 620, 360, '#ff3fa4', 0.75); a.glow(300, 380, 300, '#27d7ff', 0.28);
  for (const [x, w, top, col, winC] of [[190, 110, 220, '#141245', '#6df0ff'], [300, 120, 270, '#191456', '#ff7ac6'], [-10, 170, 210, '#0b0b2b', '#27d7ff'], [440, 180, 200, '#0c0b30', '#ff5ca8']]) {
    a.rect(x, top, w, 900 - top, col);
    for (let wy = top + 16; wy < 730; wy += 22) for (let wx = x + 10; wx < x + w - 14; wx += 18) if (a.r() > 0.55) a.rect(wx, wy, 9, 13, a.r() > 0.75 ? '#ffc765' : winC, 0.25 + a.r() * 0.7);
  }
  a.rect(150, 300, 54, 170, '#ff2f96', 1, 6); a.add(`<rect x="146" y="296" width="62" height="178" rx="8" fill="#ff2f96" ${a.blur(12)}/>`); a.rect(150, 300, 54, 170, '#ff5cb0', 1, 6);
  a.rect(410, 390, 66, 130, '#23d8ff', 1, 6); a.add(`<rect x="406" y="386" width="74" height="138" rx="8" fill="#23d8ff" ${a.blur(12)}/>`); a.rect(410, 390, 66, 130, '#7ceaff', 1, 6);
  a.rect(0, 730, 600, 170, a.lin([[730 / 900, '#2a0f55'], [1, '#04050f']]));
  a.rect(150, 736, 54, 150, '#ff2f96', 0.42); a.rect(410, 736, 66, 130, '#23d8ff', 0.36); a.glow(300, 738, 240, '#ff7ac6', 0.42);
  const x = 300; const y = 720; const k = '#04040e';
  a.add(`<circle cx="${x - 54}" cy="${y}" r="28" fill="none" stroke="${k}" stroke-width="7"/><circle cx="${x + 56}" cy="${y}" r="28" fill="none" stroke="${k}" stroke-width="7"/>`);
  a.path(`M${x - 54},${y} L${x - 8},${y - 44} L${x + 40},${y - 44} L${x + 56},${y} L${x + 30},${y - 66} L${x + 14},${y - 66}`, 'none', 1, `stroke="${k}" stroke-width="7" stroke-linecap="round" stroke-linejoin="round"`);
  a.path(`M${x - 22},${y - 44} L${x - 30},${y - 104} Q${x - 26},${y - 124} ${x},${y - 106} L${x + 26},${y - 76} L${x + 20},${y - 66} L${x - 8},${y - 88} L${x},${y - 44} Z`, k);
  a.circle(x - 16, y - 132, 15, k); a.rect(x - 62, y - 118, 42, 42, '#f2a93b', 1, 6);
  let rain = '';
  for (let i = 0; i < 180; i++) { const rx = a.r() * 660 - 30; const ry = a.r() * 860; const l = 12 + a.r() * 26; rain += `M${rx},${ry} L${rx - l * 0.22},${ry + l} `; }
  a.line(rain, '#cfe6ff', 1.1, 0.34); a.scrimTop('#07081c', 0.55, 0.26);
  a.text(['NIGHT', 'COURIER'], { x: 58, y: 112, size: 76, lead: 0.94, skew: -7, heavy: 1.6, track: 0, fill: '#fff' });
  return a.finish({ vignette: 0.3 });
};
// 25. Orbit Garden — biodome above an ocean world.
extraScenes['orbit-garden'] = () => {
  const a = new Art(600, 900, 44);
  a.bg(a.lin([[0, '#040818'], [0.7, '#0c1c44'], [1, '#123a58']])); a.stars(300, 760, '#e6f2ff', 1.5);
  a.circle(300, 1250, 620, a.rad(300, 1250, 620, [[0.85, '#1e6aa8'], [0.97, '#58c6e8'], [1, '#c6f4ff']]));
  const cx = 300; const cy = 610; const R = 210; a.glow(cx, cy - 70, 300, '#6df0a0', 0.55);
  const id = a.id(); a.defs.push(`<clipPath id="${id}"><path d="M${cx - R},${cy} A${R},${R} 0 0 1 ${cx + R},${cy} Z"/></clipPath>`);
  let plants = `<rect x="${cx - R}" y="${cy - 40}" width="${R * 2}" height="40" fill="#1a5a34"/>`;
  for (let i = 0; i < 26; i++) { const x = cx - R + a.r() * R * 2; const h = 40 + a.r() * 130; const c = ['#2f9a58', '#49c06a', '#8ae07a', '#ff8fb0'][Math.floor(a.r() * 4)]; plants += `<path d="M${x},${cy} Q${x + 6},${cy - h * 0.6} ${x},${cy - h}" stroke="#1a5a34" stroke-width="4" fill="none"/><circle cx="${x}" cy="${cy - h}" r="${12 + a.r() * 22}" fill="${c}"/>`; }
  a.add(`<g clip-path="url(#${id})"><path d="M${cx - R},${cy} A${R},${R} 0 0 1 ${cx + R},${cy} Z" fill="#0d2c34"/>${plants}</g>`);
  a.add(`<path d="M${cx - R},${cy} A${R},${R} 0 0 1 ${cx + R},${cy}" fill="none" stroke="#d6fff4" stroke-width="4" opacity=".85"/>`);
  for (const q of [-0.5, 0, 0.5]) a.add(`<path d="M${cx + q * R},${cy} Q${cx + q * R * 0.6},${cy - R * 0.9} ${cx},${cy - R}" fill="none" stroke="#d6fff4" stroke-width="1.5" opacity=".4"/>`);
  a.add(`<path d="M${cx - 170},${cy - 150} A${R - 18},${R - 18} 0 0 1 ${cx - 60},${cy - 192}" fill="none" stroke="#fff" stroke-width="7" stroke-linecap="round" opacity=".5"/>`);
  a.rect(cx - R - 20, cy, R * 2 + 40, 22, '#c9d8e6', 1, 6); a.rect(cx - 90, cy + 22, 180, 34, '#6c7f96', 1, 8);
  a.path(poly([[cx - R - 20, cy + 10], [cx - R - 130, cy - 6], [cx - R - 130, cy + 32]]), '#f2a93b');
  a.path(poly([[cx + R + 20, cy + 10], [cx + R + 130, cy - 6], [cx + R + 130, cy + 32]]), '#f2a93b');
  a.text(['Orbit', 'Garden'], { x: 56, y: 122, size: 92, lead: 0.92, heavy: 1.2, track: -2, fill: '#eafff4' });
  return a.finish({ vignette: 0.22 });
};
// 26. Paper Fleet — origami boats on faceted waves.
extraScenes['paper-fleet'] = () => {
  const a = new Art(600, 900, 67);
  a.bg(a.lin([[0, '#f6ead0'], [0.4, '#f2cf9e'], [0.5, '#6fb7cf'], [1, '#17557c']])); a.glow(450, 290, 200, '#fff8e2', 1); a.circle(450, 290, 50, '#fff');
  for (let row = 0; row < 8; row++) {
    const y = 440 + row * 60; const w = 62 + row * 12; const c = ['#7fc4d8', '#5fa9c6', '#3f8eb2', '#2b7aa0'][row % 4]; const c2 = ['#a4dbe6', '#7fc4d8', '#5fa9c6', '#3f8eb2'][row % 4];
    for (let x = -w + (row % 2) * (w / 2); x < 620; x += w) { a.path(poly([[x, y + 60], [x + w / 2, y], [x + w / 2, y + 60]]), c); a.path(poly([[x + w, y + 60], [x + w / 2, y], [x + w / 2, y + 60]]), c2); }
  }
  for (const [x, y, s, c] of [[150, 500, 0.5, '#fff4dc'], [460, 520, 0.6, '#ffd9a0'], [300, 600, 1.5, '#ff6a5c'], [110, 720, 0.8, '#fff4dc'], [490, 730, 0.9, '#fff4dc']]) {
    a.path(poly([[x - 90 * s, y], [x + 90 * s, y], [x + 50 * s, y + 52 * s], [x - 50 * s, y + 52 * s]]), c);
    a.path(poly([[x - 90 * s, y], [x, y + 26 * s], [x + 90 * s, y], [x + 50 * s, y + 52 * s], [x - 50 * s, y + 52 * s]]), '#0e2c44', 0.14);
    a.path(poly([[x - 46 * s, y], [x, y - 92 * s], [x, y + 26 * s]]), '#fffdf6'); a.path(poly([[x + 46 * s, y], [x, y - 92 * s], [x, y + 26 * s]]), '#d8cdb4');
  }
  a.text(['Paper', 'Fleet'], { x: 56, y: 122, size: 94, lead: 0.92, heavy: 1.2, track: -2.5, fill: '#123550', shadow: null });
  return a.finish({ vignette: 0, grain: 0.06 });
};
// 27. Quiet Engine — turbine rotor in obsidian shadow.
extraScenes['quiet-engine'] = () => {
  const a = new Art(600, 900, 11);
  a.bg(a.lin([[0, '#0c0f14'], [0.7, '#1b212b'], [1, '#0a0c10']])); const cx = 300; const cy = 510; a.glow(cx, cy, 340, '#f2a93b', 0.4);
  for (const [r, sw, op] of [[258, 26, 0.9], [226, 2, 0.5], [196, 2, 0.4]]) a.add(`<circle cx="${cx}" cy="${cy}" r="${r}" fill="none" stroke="#3d4654" stroke-width="${sw}" opacity="${op}"/>`);
  for (let i = 0; i < 24; i++) { const t = (i / 24) * Math.PI * 2; a.circle(cx + Math.cos(t) * 258, cy + Math.sin(t) * 258, 5, '#7c8798'); }
  for (let i = 0; i < 18; i++) {
    const t = (i / 18) * Math.PI * 2;
    a.path(poly([[cx + Math.cos(t) * 52, cy + Math.sin(t) * 52], [cx + Math.cos(t + 0.5) * 236, cy + Math.sin(t + 0.5) * 236], [cx + Math.cos(t + 0.18) * 236, cy + Math.sin(t + 0.18) * 236]]), i % 2 ? '#39424f' : '#2b333e', 1, 'stroke="#0c0f14" stroke-width="2"');
  }
  a.circle(cx, cy, 58, a.rad(cx, cy, 58, [[0, '#fff0c2'], [0.55, '#f2a93b'], [1, '#7a4a10']])); a.circle(cx, cy, 15, '#0c0f14');
  a.text(['QUIET', 'ENGINE'], { x: 56, y: 112, size: 68, lead: 1.02, track: 8, weight: 600, fill: '#f4efe6' }); a.rect(56, 194, 72, 3, '#f2a93b');
  return a.finish({ vignette: 0.3 });
};
// 28. Red Mesa — desert buttes at dusk.
extraScenes['red-mesa'] = () => {
  const a = new Art(600, 900, 52);
  a.bg(a.lin([[0, '#1c2b59'], [0.34, '#c8688a'], [0.5, '#ff9f5c'], [0.58, '#ffd29a'], [1, '#ffd29a']])); a.glow(180, 430, 260, '#fff0c0', 0.95); a.circle(180, 430, 58, '#fff6d6');
  for (const [x, w, top, base, c, c2] of [[470, 170, 400, 600, '#a83a24', '#6a2018'], [250, 120, 470, 600, '#8c2c20', '#561a16']]) {
    a.path(poly([[x - w / 2 - 44, base], [x - w / 2 + 12, top + 40], [x - w / 2 + 4, top], [x + w / 2 - 4, top], [x + w / 2 - 12, top + 40], [x + w / 2 + 44, base]]), c);
    a.path(poly([[x + w * 0.1, top], [x + w / 2 - 4, top], [x + w / 2 - 12, top + 40], [x + w / 2 + 44, base], [x + w * 0.2, base]]), c2);
    for (let i = 1; i < 6; i++) a.rect(x - w / 2 + 2 - i * 5, top + i * ((base - top) / 6), w + i * 10 - 4, 3, '#3a0f12', 0.18);
  }
  a.path(a.hill(596, [[8, 1.2, 1], [3, 4, 2]]), '#c9552c'); a.path(a.hill(660, [[14, 0.9, 2.2], [5, 3, 1]]), '#8f3320'); a.path(a.hill(750, [[18, 0.7, 4.2], [6, 2.6, 0.5]]), '#4f1814');
  const k = '#16070a';
  a.rect(94, 640, 18, 130, k, 1, 8); a.rect(60, 684, 14, 44, k, 1, 7); a.rect(60, 716, 40, 13, k, 1, 6); a.rect(132, 664, 14, 44, k, 1, 7); a.rect(106, 696, 40, 13, k, 1, 6);
  a.text(['RED', 'MESA'], { x: 56, y: 130, size: 102, lead: 0.92, heavy: 2.4, track: 2, fill: '#fff4e2' });
  return a.finish({ vignette: 0.24 });
};
// 29. Salt Flats Rally — high-key desert sprint.
extraScenes['salt-flats-rally'] = () => {
  const a = new Art(600, 900, 21);
  a.bg(a.lin([[0, '#185494'], [0.26, '#2d7dc8'], [0.45, '#bfe6f5'], [0.52, '#ffffff'], [1, '#d8e6ee']]));
  a.path(a.ridge(444, 22, 0.6), '#7f98b8'); a.rect(0, 452, 600, 450, a.lin([[0.5, '#fbfdff'], [1, '#c6d6e2']]));
  let d = '';
  for (let i = 0; i < 90; i++) { const y = 470 + a.r() ** 1.5 * 430; const s = 12 + ((y - 460) / 440) * 60; const x = a.r() * 620; d += `M${x},${y} l${s},${s * 0.12} l${s * 0.5},${-s * 0.18} `; }
  a.line(d, '#94aabc', 1.5, 0.55);
  for (let i = 0; i < 26; i++) { const t = i / 26; a.circle(420 + t * 230 + a.r() * 30, 640 - t * 160 + (a.r() - 0.5) * (30 + t * 120), 26 + t * 70, '#efe3cf', 0.42); }
  const x = 250; const y = 650;
  a.add(`<ellipse cx="${x}" cy="${y + 54}" rx="170" ry="11" fill="#2a3a4a" opacity=".32"/>`);
  a.path(`M${x - 158},${y + 32} C${x - 160},${y - 2} ${x - 124},${y - 8} ${x - 84},${y - 12} L${x - 44},${y - 58} C${x},${y - 72} ${x + 54},${y - 66} ${x + 80},${y - 18} C${x + 120},${y - 10} ${x + 156},${y - 6} ${x + 160},${y + 32} Z`, '#e4392e');
  a.path(`M${x - 66},${y - 14} L${x - 34},${y - 50} C${x - 8},${y - 58} ${x + 18},${y - 58} ${x + 18},${y - 14} Z M${x + 30},${y - 14} L${x + 30},${y - 56} C${x + 46},${y - 54} ${x + 56},${y - 40} ${x + 66},${y - 14} Z`, '#142436');
  a.rect(x - 158, y + 8, 318, 9, '#fff6ea'); a.circle(x, y + 12, 26, '#fff6ea'); a.circle(x, y + 12, 22, '#142436');
  for (const wx of [x - 96, x + 104]) { a.circle(wx, y + 32, 28, '#0b131c'); a.circle(wx, y + 32, 12, '#d8e6ee'); }
  a.text(['SALT FLATS', 'RALLY'], { x: 64, y: 112, size: 68, lead: 1.0, skew: -7, heavy: 1.6, track: -0.5, fill: '#ffffff' });
  return a.finish({ vignette: 0, grain: 0.05 });
};
// 30. Tidebreaker — lighthouse under a breaking wave.
extraScenes['tidebreaker'] = () => {
  const a = new Art(600, 900, 96);
  a.bg(a.lin([[0, '#061c2c'], [0.5, '#135a72'], [1, '#02101a']])); a.glow(160, 390, 260, '#fff2b5', 0.55);
  a.path(poly([[160, 360], [640, 240], [640, 420]]), a.lin([[0, '#fff2b5', 0.55], [1, '#fff2b5', 0]], 0.26, 0, 1, 0));
  a.path(poly([[136, 640], [146, 372], [174, 372], [184, 640]]), '#f4f0e6');
  for (const y of [430, 520, 600]) a.path(poly([[140 - (y - 430) * 0.02, y], [180 + (y - 430) * 0.02, y], [181 + (y - 430) * 0.02, y + 36], [139 - (y - 430) * 0.02, y + 36]]), '#e4392e');
  a.rect(140, 346, 40, 26, '#fff2b5'); a.rect(134, 340, 52, 7, '#0a1e2a'); a.path(poly([[140, 340], [160, 316], [180, 340]]), '#0a1e2a');
  a.path(poly([[60, 900], [100, 640], [220, 630], [280, 900]]), '#050f16');
  a.path('M640,900 L640,190 C520,180 400,230 360,330 C340,380 380,420 420,400 C390,450 330,450 300,420 C250,560 180,740 80,900 Z', a.lin([[0.22, '#7fe0dc'], [0.5, '#1a7a94'], [1, '#04202f']], 0.9, 0, 0.3, 1));
  a.path('M640,190 C520,180 400,230 360,330 C340,380 380,420 420,400 C380,410 356,384 376,340 C410,262 520,226 640,226 Z', '#f0fffb');
  let foam = '';
  for (let i = 0; i < 70; i++) { const t = a.r(); foam += `<circle cx="${(330 + t * 90 + a.r() * 40).toFixed(1)}" cy="${(330 + t * 60 + a.r() * 110).toFixed(1)}" r="${(1 + a.r() * 5).toFixed(1)}" fill="#f0fffb" opacity="${(0.3 + a.r() * 0.6).toFixed(2)}"/>`; }
  a.add(foam); a.path(a.hill(832, [[20, 2, 1], [8, 5, 2]]), '#020b12');
  a.text(['TIDEBREAKER'], { x: 300, y: 118, size: 58, anchor: 'middle', heavy: 1.6, track: 2, fill: '#f0fffb' });
  return a.finish({ vignette: 0.28 });
};
// 31. Under Ice — diver beneath a fractured ice ceiling.
extraScenes['under-ice'] = () => {
  const a = new Art(600, 900, 35);
  a.bg(a.lin([[0, '#def8ff'], [0.26, '#6ecdea'], [0.55, '#14609a'], [1, '#02132c']]));
  for (let i = 0; i < 16; i++) { const x = i * 42 - 10; a.path(poly([[x, 180], [x + 44, 180], [x + 26 + a.r() * 16, 220 + a.r() * 90], [x + 8, 200 + a.r() * 50]]), '#eafcff', 0.9); }
  let cracks = '';
  for (let i = 0; i < 14; i++) { let x = a.r() * 600; let y = 190; cracks += `M${x},${y}`; for (let k = 0; k < 4; k++) { x += (a.r() - 0.5) * 80; y += 20 + a.r() * 26; cracks += ` L${x.toFixed(1)},${y.toFixed(1)}`; } }
  a.line(cracks, '#2b84b0', 2, 0.55);
  for (let i = 0; i < 6; i++) { const x = 60 + i * 96 + a.r() * 30; a.path(poly([[x, 260], [x + 40, 260], [x + 150, 800], [x - 60, 800]]), a.lin([[0.28, '#eafcff', 0.3], [0.86, '#eafcff', 0]])); }
  const x = 300; const y = 590; const k = '#03192e';
  a.path(poly([[x + 30, y - 12], [x + 300, y - 130], [x + 300, y + 110]]), a.lin([[0, '#fff2b5', 0.85], [1, '#fff2b5', 0]], 0.55, 0, 1, 0));
  a.add(`<ellipse cx="${x - 30}" cy="${y}" rx="76" ry="24" fill="${k}" transform="rotate(-12 ${x - 30} ${y})"/>`);
  a.circle(x + 44, y - 18, 21, k); a.circle(x + 50, y - 19, 10, '#ffc765'); a.rect(x - 86, y - 40, 74, 18, '#f2a93b', 1, 9);
  a.path(poly([[x - 92, y + 12], [x - 186, y + 20], [x - 202, y + 52], [x - 92, y + 32]]), k);
  a.circle(x + 58, y - 56, 7, '#d6fff4', 0.8); a.circle(x + 70, y - 88, 5, '#d6fff4', 0.7); a.circle(x + 60, y - 120, 3.5, '#d6fff4', 0.6);
  a.path(a.ridge(800, 50, 0.6), '#01091a');
  a.text(['Under', 'Ice'], { x: 56, y: 94, size: 74, lead: 0.96, heavy: 1.2, track: -1, fill: '#062c4d', shadow: null });
  return a.finish({ vignette: 0.18, grain: 0.08 });
};
// 32. Vector Bloom — geometric crystalline blossom.
extraScenes['vector-bloom'] = () => {
  const a = new Art(600, 900, 77);
  a.bg(a.lin([[0, '#100826'], [1, '#2a0c44']])); const cx = 300; const cy = 500; a.glow(cx, cy, 340, '#ff4fa8', 0.5);
  for (const [n, r0, r1, c1, c2] of [[12, 50, 260, '#ff4fa8', '#7d4dff'], [12, 40, 190, '#ffb14a', '#ff4fa8'], [8, 20, 110, '#fff0c2', '#ffb14a']]) {
    for (let i = 0; i < n; i++) {
      const t = (i / n) * Math.PI * 2 + (n === 12 && r1 === 190 ? Math.PI / 12 : 0); const s = (Math.PI / n) * 0.85;
      const p = (ang, r) => [cx + Math.cos(ang) * r, cy + Math.sin(ang) * r];
      a.path(poly([p(t, r0), p(t + s, r1 * 0.72), p(t, r1)]), c1, 0.92, 'stroke="#100826" stroke-width="1.5"');
      a.path(poly([p(t, r0), p(t - s, r1 * 0.72), p(t, r1)]), c2, 0.92, 'stroke="#100826" stroke-width="1.5"');
    }
  }
  a.circle(cx, cy, 16, '#100826'); a.line(`M${cx},${cy + 260} L${cx},900 M${cx},${cy + 330} L${cx + 80},${cy + 270} M${cx},${cy + 370} L${cx - 90},${cy + 310}`, '#37e0a8', 5);
  a.text(['VECTOR', 'BLOOM'], { x: 300, y: 112, size: 74, lead: 1, anchor: 'middle', track: 8, weight: 600, fill: '#fdf2ff' });
  return a.finish({ vignette: 0.26 });
};
// 33. Wild Meridian — savanna under armillary meridian lines.
extraScenes['wild-meridian'] = () => {
  const a = new Art(600, 900, 18);
  a.bg(a.lin([[0, '#5a2a68'], [0.32, '#e8604c'], [0.54, '#ffc15e'], [0.66, '#ffe4a0'], [1, '#ffe4a0']]));
  const cx = 300; const cy = 500; const R = 170; a.glow(cx, cy, 340, '#fff0c0', 0.8); a.circle(cx, cy, R, '#fff3c9', 0.95);
  for (const q of [-0.66, -0.33, 0, 0.33, 0.66]) a.add(`<ellipse cx="${cx}" cy="${cy}" rx="${Math.abs(q) * R + 1}" ry="${R}" fill="none" stroke="#b5502e" stroke-width="2" opacity=".5"/>`);
  for (const q of [-0.5, 0, 0.5]) { const w = Math.sqrt(1 - q * q) * R; a.rect(cx - w, cy + q * R, w * 2, 2, '#b5502e', 0.5); }
  a.path(a.hill(590, [[10, 1.1, 2], [4, 3, 1]]), '#c9742e'); a.path(a.hill(650, [[14, 0.8, 4], [5, 3, 0.4]]), '#8c4a22'); a.path(a.hill(752, [[16, 0.9, 0.8], [6, 2.4, 2]]), '#3a1e14');
  const k = '#1c0d0a';
  a.path('M430,660 C434,610 428,580 420,556 L432,556 C440,576 446,596 448,612 C456,588 470,568 486,556 L494,562 C474,590 458,620 452,660 Z', k);
  a.add(`<ellipse cx="456" cy="540" rx="100" ry="24" fill="${k}"/><ellipse cx="392" cy="552" rx="46" ry="13" fill="${k}"/><ellipse cx="520" cy="552" rx="46" ry="13" fill="${k}"/>`);
  for (const [x, y, s] of [[110, 662, 1], [176, 668, 0.86], [232, 672, 0.74], [278, 676, 0.62]]) a.path(`M${x - 30 * s},${y} L${x - 26 * s},${y - 34 * s} L${x + 20 * s},${y - 36 * s} L${x + 34 * s},${y - 76 * s} L${x + 44 * s},${y - 76 * s} L${x + 42 * s},${y - 30 * s} L${x + 30 * s},${y} L${x + 22 * s},${y} L${x + 20 * s},${y - 22 * s} L${x - 16 * s},${y - 22 * s} L${x - 22 * s},${y} Z`, k);
  a.text(['Wild', 'Meridian'], { x: 56, y: 122, size: 92, lead: 0.92, heavy: 1.2, track: -2, fill: '#fff6e4' });
  return a.finish({ vignette: 0.22 });
};
// 34. Xeno Farm — alien moons and bioluminescent crop rows.
extraScenes['xeno-farm'] = () => {
  const a = new Art(600, 900, 64);
  a.bg(a.lin([[0, '#0a1140'], [0.36, '#3a1a78'], [0.56, '#c85aa0'], [0.68, '#ffb59a'], [1, '#ffb59a']])); a.stars(110, 330, '#fff', 1.4);
  a.circle(450, 300, 84, a.lin([[0.24, '#ffe9c8'], [0.42, '#e8a36a']], 0, 0, 0.3, 0.1)); a.circle(170, 370, 36, '#b8f2e0');
  a.path(a.hill(600, [[14, 1, 1.2], [6, 3.4, 2]]), '#5c2a7a'); a.path(a.hill(650, [[16, 0.8, 3.2], [5, 2.8, 0.4]]), '#3a1a5c');
  const x = 400; const y = 636; const k = '#160a2c';
  a.rect(x - 60, y - 76, 120, 76, k); a.path(poly([[x - 72, y - 76], [x, y - 124], [x + 72, y - 76]]), k);
  a.rect(x + 66, y - 170, 44, 170, k); a.add(`<path d="M${x + 66},${y - 170} A22,22 0 0 1 ${x + 110},${y - 170} Z" fill="${k}"/>`);
  a.rect(x - 16, y - 40, 32, 40, '#7df2c8'); a.glow(x, y - 20, 90, '#7df2c8', 0.65);
  for (let row = 0; row < 4; row++) {
    const yy = 680 + row * row * 12 + row * 22; const s = 0.6 + row * 0.34;
    a.path(a.hill(yy + 26 * s, [[3, 2, row], [2, 5, row * 2]]), ['#2a1250', '#201040', '#180a30', '#100620'][row]);
    for (let i = 0; i < 9 - row; i++) {
      const px = 8 + (i + (row % 2) * 0.5) * (600 / (8.4 - row)); const h = 40 * s + a.r() * 18 * s;
      a.line(`M${px},${yy + 26 * s} Q${px + 10 * s},${yy} ${px - 4 * s},${yy - h * 0.5}`, '#37e0a8', 3.4 * s);
      a.glow(px - 4 * s, yy - h * 0.6, 18 * s, '#7df2c8', 0.8); a.add(`<ellipse cx="${px - 4 * s}" cy="${yy - h * 0.6}" rx="${8 * s}" ry="${13 * s}" fill="#d0ffe8"/>`);
    }
  }
  a.text(['XENO', 'FARM'], { x: 56, y: 122, size: 98, lead: 0.92, heavy: 2, track: 2, fill: '#eafff6' });
  return a.finish({ vignette: 0.24 });
};
// 35. Yard Sale Hero — homemade comic superhero.
extraScenes['yard-sale-hero'] = () => {
  const a = new Art(600, 900, 29); a.bg('#ffc531'); const cx = 300; const cy = 480;
  for (let i = 0; i < 24; i += 2) { const t = (i / 24) * Math.PI * 2; const t2 = ((i + 1) / 24) * Math.PI * 2; a.path(poly([[cx, cy], [cx + Math.cos(t) * 900, cy + Math.sin(t) * 900], [cx + Math.cos(t2) * 900, cy + Math.sin(t2) * 900]]), '#ff8f2e'); }
  a.path(a.hill(680, [[10, 1.2, 1], [4, 3, 2]]), '#5eb54a'); const k = '#0e4a5c';
  a.path(poly([[272, 380], [150, 560], [176, 590], [282, 520]]), '#7a3fd0');
  a.path('M258,380 C262,362 338,362 342,380 L332,490 L268,490 Z', k);
  a.path(poly([[268, 490], [332, 490], [340, 640], [308, 640], [300, 530], [292, 640], [260, 640]]), '#13667a');
  a.path(poly([[258, 382], [226, 450], [270, 474], [274, 450], [256, 442], [276, 400]]), k);
  a.path(poly([[342, 382], [374, 450], [330, 474], [326, 450], [344, 442], [324, 400]]), k);
  a.path(poly([[282, 420], [306, 406], [322, 434], [298, 448]]), '#fff1c4');
  a.circle(290, 423, 4, k); a.circle(300, 340, 30, '#ffd9b0');
  a.path('M270,336 C270,300 330,300 330,336 L322,332 L278,332 Z', '#7a3fd0');
  a.rect(250, 636, 50, 14, '#7a3fd0', 1, 6); a.rect(304, 636, 50, 14, '#7a3fd0', 1, 6);
  a.rect(48, 690, 504, 18, '#8a5a2a', 1, 4); a.rect(72, 708, 16, 92, '#5a3a1c'); a.rect(512, 708, 16, 92, '#5a3a1c');
  a.rect(78, 630, 90, 60, '#f4e6c8'); a.path(poly([[78, 630], [168, 630], [148, 612], [98, 612]]), '#d8c49c');
  a.circle(232, 668, 22, '#3fb0d0'); a.rect(402, 642, 44, 48, '#fff', 1, 4); a.rect(452, 660, 64, 30, '#e4392e', 1, 4);
  a.text(['YARD SALE', 'HERO'], { x: 300, y: 112, size: 70, lead: 1.02, anchor: 'middle', skew: -5, heavy: 2, fill: '#142258', shadow: '0 3px 0 rgba(255,250,224,0.85)' });
  return a.finish({ vignette: 0, grain: 0.06 });
};
// 36. Zero Gravity Golf — lunar green under Earthrise.
extraScenes['zero-gravity-golf'] = () => {
  const a = new Art(600, 900, 42);
  a.bg(a.lin([[0, '#02030c'], [1, '#0e1236']])); a.stars(320, 800, '#fff', 1.7);
  a.circle(452, 390, 92, a.lin([[0.34, '#58c6e8'], [0.52, '#1a5a9a']], 0, 0, 0.7, 0.6));
  a.path('M404,340 C430,360 450,340 470,350 C490,380 460,400 440,396 C420,392 396,372 404,340 Z', '#63c77a');
  a.glow(452, 390, 160, '#58c6e8', 0.34); a.path(a.hill(660, [[20, 0.7, 2.2], [6, 2.6, 1]]), a.lin([[0.7, '#a7acbc'], [0.95, '#3d4256']]));
  for (const [x, y, rx] of [[120, 690, 60], [420, 712, 88], [260, 770, 40], [530, 780, 50]]) a.add(`<ellipse cx="${x}" cy="${y}" rx="${rx}" ry="${rx * 0.22}" fill="#3d4256" opacity=".75"/>`);
  a.add('<ellipse cx="430" cy="660" rx="17" ry="5" fill="#02030c"/>');
  a.rect(429, 556, 3, 104, '#eef0f6'); a.path(poly([[432, 556], [484, 572], [432, 588]]), '#ff4a5e');
  a.line('M130,716 C190,330 340,420 416,650', '#ffffff', 3, 0.8, 'stroke-dasharray="1 14"');
  a.circle(258, 440, 13, '#ffffff'); a.circle(254, 436, 3, '#c9cede');
  const k = '#eef0f6';
  a.path('M96,716 L108,640 C112,620 146,620 150,640 L158,716 L134,716 L128,680 L120,716 Z', k);
  a.circle(128, 608, 25, k); a.path('M114,604 C118,592 140,592 144,604 C140,614 118,614 114,604 Z', '#f2a93b');
  a.line('M146,640 L196,670 L176,726', '#3d4256', 4);
  a.text(['ZERO', 'GRAVITY', 'GOLF'], { x: 56, y: 102, size: 72, lead: 0.96, heavy: 1.4, track: 1.5, fill: '#ffffff' });
  return a.finish({ vignette: 0.2, grain: 0.1 });
};
