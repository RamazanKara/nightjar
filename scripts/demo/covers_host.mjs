// Procedural 7.0 Rubylight host console covers (14 x 600x900) and Obsidian Studio dusk horizon (1600x900).
import { Art, poly, puff } from './covers.mjs';

export const hostScenes = {};

// 1. Ashen Crown — obsidian citadel & fractured ruby-gold crown in caldera dusk.
hostScenes['ashen-crown'] = () => {
  const a = new Art(600, 900, 101);
  a.bg(a.lin([[0, '#090a0d'], [0.34, '#240b15'], [0.62, '#8b1635'], [0.82, '#e25b2d'], [1, '#f5b041']]));
  a.glow(300, 480, 360, '#ce1e49', 0.65); a.glow(300, 560, 260, '#f5b041', 0.85);
  a.add('<circle cx="300" cy="390" r="118" fill="none" stroke="#ffc866" stroke-width="2.2" opacity=".55"/>');
  a.circle(300, 390, 94, '#13080e', 0.72);
  a.path(poly([[234, 412], [248, 352], [276, 384], [300, 326], [324, 384], [352, 352], [366, 412]]), a.lin([[0.35, '#ffe7ad'], [0.46, '#f5b041']]));
  a.path(poly([[294, 300], [300, 282], [306, 300], [300, 314]]), '#fff3cf');
  a.path(a.ridge(610, 74, 0.56), '#4a1122'); a.path(a.ridge(675, 64, 0.56), '#290a14');
  const k = '#11060b';
  a.path(poly([[156, 900], [190, 590], [224, 540], [268, 462], [300, 424], [332, 462], [376, 540], [410, 590], [444, 900]]), k);
  for (const [sx, top, w] of [[214, 488, 22], [260, 438, 28], [300, 392, 34], [340, 438, 28], [386, 488, 22]]) a.path(poly([[sx - w / 2, 620], [sx, top], [sx + w / 2, 620]]), k);
  a.rect(40, 628, 170, 14, k);
  for (let i = 0; i < 4; i++) a.path(`M${52 + i * 38},740 L${52 + i * 38},642 A14,18 0 0 1 ${80 + i * 38},642 L${80 + i * 38},740 Z`, k);
  for (const [wx, wy] of [[297, 468], [258, 512], [338, 512], [297, 536]]) { a.rect(wx, wy, 6, 20, '#ffc866'); a.glow(wx + 3, wy + 10, 22, '#ff7043', 0.8); }
  a.path(a.ridge(805, 52, 0.54), '#090508');
  for (let i = 0; i < 95; i++) a.circle(a.r() * 600, 260 + a.r() * 640, 0.8 + a.r() ** 2 * 2.8, a.r() > 0.45 ? '#ffc866' : '#ff5268', 0.4 + a.r() * 0.55);
  a.scrimTop('#090a0d', 0.52, 0.28);
  a.text(['ASHEN', 'CROWN'], { x: 300, y: 112, size: 76, lead: 0.96, anchor: 'middle', track: 6, heavy: 1.4, fill: '#fff4e4' });
  return a.finish({ vignette: 0.32 });
};

// 2. Chrome Syndicate — cyber-noir vault monoliths & laser security grid.
hostScenes['chrome-syndicate'] = () => {
  const a = new Art(600, 900, 108);
  a.bg(a.lin([[0, '#07090f'], [0.55, '#10192b'], [1, '#090b12']]));
  a.glow(300, 490, 340, '#59b0ff', 0.42); a.glow(300, 670, 280, '#ce1e49', 0.45);
  for (const [x, w, top, c] of [[28, 124, 240, '#0d1320'], [166, 98, 300, '#121b2d'], [340, 104, 280, '#121b2d'], [458, 118, 220, '#0d1320']]) {
    a.rect(x, top, w, 900 - top, c); a.rect(x + w - 3, top, 2, 900 - top, '#59b0ff', 0.35);
  }
  a.path(poly([[300, 300], [412, 470], [300, 640], [188, 470]]), a.lin([[0.33, '#eef6ff'], [0.52, '#59b0ff'], [0.72, '#18243a']]));
  a.path(poly([[300, 300], [412, 470], [300, 494]]), '#ffffff', 0.65); a.path(poly([[300, 494], [412, 470], [300, 640]]), '#0f1829', 0.82);
  for (const [x1, y1, x2, y2] of [[0, 560, 600, 410], [0, 430, 600, 610], [80, 900, 300, 494], [520, 900, 300, 494]]) {
    a.line(`M${x1},${y1} L${x2},${y2}`, '#ff335f', 5, 0.35, a.blur(4)); a.line(`M${x1},${y1} L${x2},${y2}`, '#ff7596', 1.8, 0.92);
  }
  a.scrimTop('#07090f', 0.55, 0.28);
  a.text(['CHROME', 'SYNDICATE'], { x: 56, y: 114, size: 66, lead: 0.98, track: 3, heavy: 1.4, fill: '#f5f7fa' }); a.rect(56, 200, 88, 3, '#ce1e49');
  return a.finish({ vignette: 0.3 });
};

// 3. Dust Runners — ochre canyon skimmers & sunlit contrails.
hostScenes['dust-runners'] = () => {
  const a = new Art(600, 900, 115);
  a.bg(a.lin([[0, '#1c0f0c'], [0.34, '#6e2816'], [0.62, '#de7730'], [1, '#fbd084']]));
  a.glow(380, 430, 300, '#ffe8a8', 0.9); a.circle(380, 420, 68, '#fff7dc');
  a.path(poly([[0, 900], [0, 330], [168, 410], [218, 630], [150, 900]]), '#3a160f');
  a.path(poly([[600, 900], [600, 290], [456, 380], [412, 650], [468, 900]]), '#280e0a');
  a.path(a.hill(730, [[22, 1.1, 0.8], [8, 3.2, 1.5]]), '#592014'); a.path(a.hill(808, [[18, 1.4, 2.4], [6, 4, 0.5]]), '#1f0b08');
  for (let i = 0; i < 18; i++) { const t = i / 18; a.circle(160 + t * 220, 640 - t * 90 + Math.sin(i) * 14, 18 + (1 - t) * 44, '#ffdca0', 0.28); }
  for (const [x, y, s] of [[330, 540, 1.15], [236, 606, 0.78]]) {
    a.line(`M${x - 130 * s},${y + 38 * s} Q${x - 50 * s},${y + 18 * s} ${x},${y}`, '#fff3c4', 4 * s, 0.85);
    a.path(poly([[x + 68 * s, y - 16 * s], [x - 24 * s, y - 6 * s], [x - 42 * s, y + 16 * s], [x + 18 * s, y + 10 * s]]), '#140807');
    a.glow(x - 28 * s, y + 6 * s, 22 * s, '#59b0ff', 0.9);
  }
  a.text(['DUST', 'RUNNERS'], { x: 56, y: 120, size: 80, lead: 0.94, skew: -7, heavy: 1.8, track: 1, fill: '#fff4e0' });
  return a.finish({ vignette: 0.26 });
};

// 4. Frostline — high-key alpine glacier & expedition beacon.
hostScenes['frostline'] = () => {
  const a = new Art(600, 900, 122);
  a.bg(a.lin([[0, '#0b2545'], [0.35, '#2b70b0'], [0.68, '#b8e8ff'], [1, '#eef9ff']])); a.glow(430, 320, 260, '#ffffff', 0.85);
  a.path(poly([[-30, 900], [180, 350], [340, 560], [470, 290], [630, 900]]), a.lin([[0.32, '#ffffff'], [0.78, '#5ca8dc']]));
  a.path(poly([[180, 350], [340, 560], [260, 900], [-30, 900]]), '#1d568c', 0.85);
  a.path(poly([[470, 290], [630, 900], [430, 900], [390, 560]]), '#154270', 0.88);
  a.path(a.ridge(770, 48, 0.48), '#0b294a');
  a.glow(296, 728, 38, '#f5b041', 0.95); a.rect(292, 730, 8, 24, '#07182e', 1, 3); a.circle(296, 724, 5, '#ffd584');
  a.text(['FROST', 'LINE'], { x: 56, y: 124, size: 88, lead: 0.92, heavy: 1.6, track: 2, fill: '#f2fbff' });
  return a.finish({ vignette: 0.18 });
};

// 5. Hollow Ward — gothic sanctuary arch, wrought-iron tracery & emerald ward sigil.
hostScenes['hollow-ward'] = () => {
  const a = new Art(600, 900, 129);
  a.bg(a.lin([[0, '#070a12'], [0.55, '#132438'], [1, '#080c14']]));
  a.glow(300, 510, 320, '#45e0a0', 0.52); a.glow(300, 510, 155, '#b8ffe0', 0.78);
  for (const x of [210, 255, 345, 390]) a.line(`M${x},240 L${x},820`, '#1d3247', 2.4, 0.7);
  a.path('M0,0 L600,0 L600,900 L456,900 L456,470 Q456,266 300,202 Q144,266 144,470 L144,900 L0,900 Z', '#090d16');
  a.add('<path d="M144,900 L144,470 Q144,266 300,202 Q456,266 456,470 L456,900" fill="none" stroke="#2f4660" stroke-width="5"/>');
  a.add('<path d="M168,900 L168,476 Q168,292 300,232 Q432,292 432,476 L432,900" fill="none" stroke="#45e0a0" stroke-opacity=".28" stroke-width="2"/>');
  a.line('M300,202 L300,446', '#35506b', 3);
  a.add('<circle cx="300" cy="500" r="88" fill="none" stroke="#45e0a0" stroke-width="2.6" opacity=".78"/><circle cx="300" cy="500" r="56" fill="none" stroke="#c8ffea" stroke-width="1.8" opacity=".88"/>');
  a.path(poly([[300, 448], [330, 500], [300, 552], [270, 500]]), '#d8fff0');
  a.path(poly([[264, 780], [286, 650], [314, 650], [336, 780]]), '#c8ffea', 0.22);
  for (let i = 0; i < 6; i++) puff(a, 110 + i * 64, 770 + (i % 2) * 28, 1.3, '#1a2d3d', 0.48);
  a.path(a.ridge(842, 30, 0.5), '#05080e');
  a.text(['HOLLOW', 'WARD'], { x: 300, y: 108, size: 72, lead: 0.98, anchor: 'middle', track: 7, weight: 600, fill: '#eafaf4' });
  return a.finish({ vignette: 0.34 });
};

// 6. Iron Meridian — steel viaduct truss, industrial skyline & strider searchlight.
hostScenes['iron-meridian'] = () => {
  const a = new Art(600, 900, 136);
  a.bg(a.lin([[0, '#0b0e14'], [0.46, '#233042'], [0.74, '#c86b2d'], [1, '#f5b041']]));
  a.glow(410, 560, 290, '#ffc866', 0.78);
  for (const [sx, w, h] of [[84, 18, 120], [114, 14, 96], [478, 22, 135]]) a.rect(sx, 630 - h, w, h, '#18202c');
  a.path(a.ridge(640, 66, 0.55), '#1c2532');
  const k = '#0e1218';
  a.rect(0, 446, 600, 18, k); a.rect(0, 492, 600, 10, k);
  for (let x = -20; x < 620; x += 48) a.line(`M${x},454 L${x + 48},496 M${x + 48},454 L${x},496`, k, 4);
  for (const x of [110, 180, 250, 350, 420, 490]) a.line(`M${x},496 L${x},690`, k, 3, 0.55);
  a.add(`<path d="M40,900 A260,390 0 0 1 560,900" fill="none" stroke="${k}" stroke-width="22"/>`);
  a.path(poly([[266, 404], [600, 520], [600, 650]]), a.lin([[0, '#fff2c2', 0.65], [1, '#fff2c2', 0]], 0.44, 0, 1, 0));
  a.rect(224, 376, 56, 42, k, 1, 8); a.line('M236,418 L218,450 M264,418 L280,450', k, 7); a.circle(270, 396, 6, '#fff4cc');
  a.path(a.ridge(810, 44, 0.52), '#080a0f');
  a.text(['IRON', 'MERIDIAN'], { x: 56, y: 120, size: 76, lead: 0.96, heavy: 1.8, track: 3, fill: '#f5f7fa' });
  return a.finish({ vignette: 0.3 });
};

// 7. Lumen Grove — bioluminescent canopy & spirit pool.
hostScenes['lumen-grove'] = () => {
  const a = new Art(600, 900, 143);
  a.bg(a.lin([[0, '#041016'], [0.48, '#0b2d36'], [0.76, '#155c58'], [1, '#061a20']]));
  a.glow(300, 610, 310, '#45e0a0', 0.62); a.glow(300, 430, 240, '#59b0ff', 0.4);
  a.add(`<ellipse cx="300" cy="740" rx="230" ry="52" fill="${a.rad(300, 740, 230, [[0, '#b8ffee', 0.9], [0.6, '#35c998', 0.55], [1, '#0b2d36', 0]])}"/>`);
  a.path('M0,0 L160,0 C110,180 96,420 128,900 L0,900 Z', '#041217');
  a.path('M600,0 L440,0 C490,180 504,420 472,900 L600,900 Z', '#041217');
  a.path('M110,260 C210,210 390,210 490,270 C390,248 220,248 110,296 Z', '#041217');
  for (const [lx, ly, r, c] of [[204, 330, 14, '#7effd4'], [266, 294, 18, '#59b0ff'], [338, 306, 16, '#7effd4'], [402, 342, 13, '#ffc866']]) {
    a.line(`M${lx},250 L${lx},${ly}`, '#134e4a', 2); a.glow(lx, ly, r * 3.2, c, 0.75); a.circle(lx, ly, r, '#eafff8');
  }
  for (let i = 0; i < 65; i++) a.circle(90 + a.r() * 420, 280 + a.r() * 540, 1.2 + a.r() * 3, a.r() > 0.35 ? '#8affdc' : '#7cc8ff', 0.4 + a.r() * 0.55);
  a.text(['Lumen', 'Grove'], { x: 56, y: 124, size: 92, lead: 0.92, heavy: 1.2, track: -2, fill: '#eafff7' });
  return a.finish({ vignette: 0.28 });
};

// 8. Orbital Siege — planetary curvature & orbital ring defense.
hostScenes['orbital-siege'] = () => {
  const a = new Art(600, 900, 150);
  a.bg(a.lin([[0, '#04060d'], [0.62, '#0c152e'], [1, '#1a1026']])); a.stars(280, 700, '#dce8ff', 1.6);
  a.glow(300, 760, 420, '#ff6242', 0.65);
  a.circle(300, 1180, 560, a.rad(300, 1180, 560, [[0.82, '#141e38'], [0.95, '#ce1e49'], [1, '#ffc866']]));
  a.add('<ellipse cx="300" cy="540" rx="260" ry="56" fill="none" stroke="#59b0ff" stroke-width="3" opacity=".55" transform="rotate(-14 300 540)"/>');
  for (const [x1, y1, x2, y2, c] of [[190, 390, 340, 650, '#59b0ff'], [420, 350, 270, 640, '#ff7596'], [310, 430, 390, 660, '#ffc866']]) {
    a.line(`M${x1},${y1} L${x2},${y2}`, c, 5, 0.35, a.blur(4)); a.line(`M${x1},${y1} L${x2},${y2}`, '#ffffff', 1.8, 0.9);
  }
  a.path(poly([[130, 412], [296, 356], [268, 398], [166, 428]]), '#e6eefc');
  a.path(poly([[370, 366], [488, 330], [464, 362], [392, 380]]), '#c8d6f0');
  a.text(['ORBITAL', 'SIEGE'], { x: 300, y: 116, size: 78, lead: 0.96, anchor: 'middle', track: 6, heavy: 1.5, fill: '#f5f7fa' });
  return a.finish({ vignette: 0.24 });
};

// 9. Pip & Bloom — botanical terrace & geometric seed-sprite.
hostScenes['pip-and-bloom'] = () => {
  const a = new Art(600, 900, 157);
  a.bg(a.lin([[0, '#123832'], [0.46, '#2b7a62'], [0.8, '#a6e3a8'], [1, '#fef0c2']])); a.glow(440, 280, 260, '#fff7d6', 0.85);
  a.path(a.hill(620, [[26, 1.1, 0.6], [10, 2.8, 1.4]]), '#1f5e4b'); a.path(a.hill(730, [[22, 1.3, 2.2], [8, 3.4, 0.8]]), '#134235');
  for (const [cx, cy, s, c1, c2] of [[160, 650, 1.1, '#ff7596', '#ce1e49'], [430, 590, 1.25, '#ffc866', '#f5923e']]) {
    a.line(`M${cx},${cy} L${cx},900`, '#0d2e25', 8 * s);
    for (const ang of [-0.7, -0.35, 0, 0.35, 0.7]) {
      const dx = Math.sin(ang) * 74 * s; const dy = -Math.cos(ang) * 68 * s;
      a.path(`M${cx},${cy} Q${cx + dx * 0.9},${cy + dy * 0.3} ${cx + dx},${cy + dy} Q${cx + dx * 0.2},${cy + dy * 0.9} ${cx},${cy} Z`, Math.abs(ang) > 0.4 ? c2 : c1);
    }
    a.circle(cx, cy - 12 * s, 14 * s, '#ffe896');
  }
  a.glow(296, 486, 66, '#fff6c4', 0.8);
  a.path('M272,500 C272,462 320,462 320,500 C320,524 272,524 272,500 Z', '#fffdf2');
  a.path('M296,466 Q316,434 334,448 Q314,468 296,466 Z', '#45e0a0');
  a.circle(288, 496, 4, '#123832'); a.circle(306, 496, 4, '#123832');
  a.text(['Pip &', 'Bloom'], { x: 56, y: 124, size: 92, lead: 0.92, heavy: 1.4, track: -2, fill: '#fffdf2' });
  return a.finish({ vignette: 0.18 });
};

// 10. Redline Avenue — midnight expressway overpass & ruby light trails.
hostScenes['redline-avenue'] = () => {
  const a = new Art(600, 900, 164);
  a.bg(a.lin([[0, '#080910'], [0.48, '#1a1026'], [0.78, '#6e1230'], [1, '#090a10']])); a.glow(340, 520, 320, '#ce1e49', 0.65);
  for (const [x, w, h] of [[42, 74, 380], [132, 92, 320], [390, 86, 300], [492, 78, 350]]) {
    a.rect(x, 540 - h, w, h, '#101320');
    for (let y = 540 - h + 18; y < 520; y += 24) a.rect(x + 12, y, w - 24, 3, '#f5b041', 0.25);
  }
  a.path('M0,540 L600,540 L600,900 L0,900 Z', '#090a10');
  a.path('M250,540 L370,540 L620,900 L-20,900 Z', a.lin([[0.6, '#261528'], [1, '#0c0d14']]));
  for (const [xTop, xBot, wBot, c] of [[276, 60, 26, '#ce1e49'], [290, 128, 16, '#ff7596'], [334, 456, 22, '#f5b041'], [348, 522, 14, '#fff2c6']]) {
    a.path(poly([[xTop, 540], [xTop + 4, 540], [xBot + wBot, 900], [xBot, 900]]), c, 0.9, a.blur(3));
    a.path(poly([[xTop, 540], [xTop + 3, 540], [xBot + wBot * 0.6, 900], [xBot, 900]]), c);
  }
  for (let i = 0; i < 6; i++) a.circle(230 + i * 28, 470 + i * 12, 5, '#ffc866', 0.85);
  a.text(['REDLINE', 'AVENUE'], { x: 64, y: 122, size: 80, lead: 0.94, skew: -8, heavy: 1.8, track: 1, fill: '#f5f7fa' });
  return a.finish({ vignette: 0.3 });
};

// 11. Skyline Strikers — rooftop twilight arena, floodlights & blazing comet arc.
hostScenes['skyline-strikers'] = () => {
  const a = new Art(600, 900, 171);
  a.bg(a.lin([[0, '#08152e'], [0.45, '#1c4d8c'], [0.76, '#f07b3f'], [1, '#ffd07b']])); a.glow(300, 560, 320, '#ffc866', 0.78);
  for (const [x, w, top] of [[18, 90, 560], [122, 76, 605], [408, 82, 595], [504, 84, 548]]) {
    a.rect(x, top, w, 900 - top, '#0c1b33');
    for (let wy = top + 16; wy < 820; wy += 24) a.rect(x + 12, wy, w - 24, 4, '#59b0ff', 0.22);
  }
  for (const [fx, dir] of [[156, 1], [444, -1]]) {
    a.line(`M${fx},630 L${fx - dir * 18},520`, '#0c1b33', 5);
    a.glow(fx - dir * 18, 520, 42, '#e0f4ff', 0.75); a.circle(fx - dir * 18, 520, 7, '#ffffff');
  }
  a.path(poly([[70, 760], [200, 630], [400, 630], [530, 760]]), '#0e2444');
  a.add('<path d="M70,760 L200,630 L400,630 L530,760 Z" fill="none" stroke="#59b0ff" stroke-width="3"/><ellipse cx="300" cy="695" rx="74" ry="22" fill="none" stroke="#59b0ff" stroke-width="2.2" opacity=".8"/>');
  a.line('M124,690 Q270,320 442,438', '#ffc866', 9, 0.45, a.blur(5)); a.line('M124,690 Q270,320 442,438', '#fff4cc', 4, 0.95);
  a.glow(442, 438, 68, '#ff9d3b', 0.95); a.circle(442, 438, 22, '#ffffff');
  a.text(['SKYLINE', 'STRIKERS'], { x: 300, y: 118, size: 74, lead: 0.96, anchor: 'middle', skew: -6, heavy: 1.6, track: 2, fill: '#ffffff' });
  return a.finish({ vignette: 0.22 });
};

// 12. Starfall Vanguard — meteor shower & spearhead cruiser.
hostScenes['starfall-vanguard'] = () => {
  const a = new Art(600, 900, 178);
  a.bg(a.lin([[0, '#060712'], [0.5, '#19133d'], [0.85, '#591b54'], [1, '#1a0d26']])); a.stars(260, 760, '#e6ecff', 1.6); a.glow(360, 380, 320, '#b79bff', 0.42);
  for (const [x, y, l, c] of [[150, 260, 150, '#7ee8fa'], [290, 220, 210, '#ffc866'], [450, 280, 170, '#7ee8fa'], [220, 390, 140, '#ff7596'], [490, 410, 130, '#ffc866']]) {
    a.line(`M${x},${y} L${x - l * 0.7},${y + l}`, c, 3.2, 0.85); a.circle(x - l * 0.7, y + l, 4, '#ffffff');
  }
  a.path(poly([[300, 340], [374, 590], [318, 560], [300, 612], [282, 560], [226, 590]]), a.lin([[0.38, '#f5f7fa'], [0.68, '#7a86ad']]));
  a.path(poly([[300, 340], [300, 612], [282, 560], [226, 590]]), '#c7d2ee');
  a.path(poly([[286, 588], [300, 740], [314, 588]]), a.lin([[0.65, '#7ee8fa', 0.95], [0.82, '#7ee8fa', 0]]));
  a.glow(300, 604, 72, '#59b0ff', 0.85); a.path(a.ridge(816, 48, 0.55), '#090718');
  a.text(['STARFALL', 'VANGUARD'], { x: 300, y: 114, size: 66, lead: 0.98, anchor: 'middle', track: 4, heavy: 1.4, fill: '#f5f7fa' });
  return a.finish({ vignette: 0.26 });
};

// 13. Tidebound — basalt sea arch & golden-hour expedition cutter.
hostScenes['tidebound'] = () => {
  const a = new Art(600, 900, 185);
  a.bg(a.lin([[0, '#071f30'], [0.44, '#145a73'], [0.66, '#f2bd74'], [1, '#082738']]));
  a.glow(310, 520, 290, '#ffe8b0', 0.9); a.circle(310, 505, 58, '#fff7dc');
  a.rect(0, 560, 600, 340, a.lin([[0.62, '#227a8c'], [1, '#061c29']]));
  a.path('M-20,590 L70,290 L220,250 L410,290 L480,590 L386,590 C376,410 214,410 204,590 Z', '#0b2636');
  a.path(poly([[490, 590], [532, 390], [584, 590]]), '#0e3144');
  for (let i = 0; i < 36; i++) { const y = 575 + i * 8; a.rect(240 + (a.r() - 0.5) * 90, y, 36 + i * 2.2, 2.2, '#ffe8b0', 0.45, 1); }
  const sx = 298; const sy = 618;
  a.path(poly([[sx - 48, sy], [sx + 52, sy], [sx + 34, sy + 22], [sx - 34, sy + 22]]), '#071520');
  a.path(poly([[sx - 4, sy - 4], [sx - 4, sy - 128], [sx - 56, sy - 6]]), '#fff8e8');
  a.path(poly([[sx + 4, sy - 4], [sx + 4, sy - 102], [sx + 46, sy - 6]]), '#f5b041');
  a.text(['TIDEBOUND'], { x: 300, y: 124, size: 68, anchor: 'middle', track: 5, heavy: 1.6, fill: '#f4fbff' });
  return a.finish({ vignette: 0.26 });
};

// 14. Turbo Critters — 4-player neon kart boost circuit.
hostScenes['turbo-critters'] = () => {
  const a = new Art(600, 900, 192);
  a.bg(a.lin([[0, '#160c36'], [0.48, '#461873'], [0.82, '#e04268'], [1, '#ff9848']])); a.glow(300, 540, 340, '#ff7596', 0.65); a.stars(90, 360, '#fff4d8', 1.5);
  a.path(poly([[220, 460], [380, 460], [620, 900], [-20, 900]]), a.lin([[0.52, '#2d1554'], [1, '#120829']]));
  a.line('M220,460 L-20,900 M380,460 L620,900', '#ffc866', 6);
  a.path(poly([[252, 620], [300, 578], [348, 620], [330, 636], [300, 608], [270, 636]]), '#45e0a0');
  for (const [x, y, s, bodyC, helmC] of [[216, 540, 0.72, '#59b0ff', '#dff2ff'], [386, 552, 0.76, '#45e0a0', '#dcfff0'], [428, 690, 0.96, '#ff7596', '#ffe0ea'], [252, 720, 1.18, '#ffc866', '#fff5d6']]) {
    a.glow(x, y + 10 * s, 38 * s, bodyC, 0.6); a.rect(x - 38 * s, y, 76 * s, 24 * s, bodyC, 1, 10 * s);
    a.circle(x - 24 * s, y + 24 * s, 11 * s, '#0b0818'); a.circle(x + 24 * s, y + 24 * s, 11 * s, '#0b0818'); a.circle(x, y - 12 * s, 18 * s, helmC);
    a.path(poly([[x - 14 * s, y - 24 * s], [x - 22 * s, y - 42 * s], [x - 4 * s, y - 28 * s]]), helmC);
    a.path(poly([[x + 14 * s, y - 24 * s], [x + 22 * s, y - 42 * s], [x + 4 * s, y - 28 * s]]), helmC);
  }
  a.text(['TURBO', 'CRITTERS'], { x: 300, y: 120, size: 78, lead: 0.94, anchor: 'middle', skew: -6, heavy: 2, track: 1, fill: '#fff8eb' });
  return a.finish({ vignette: 0.22 });
};

// Wide 1600x900 procedural 7.0 Obsidian Studio dusk horizon for Overview.svelte (zero text).
export function homeLandscape() {
  const w = 1600; const h = 900; const a = new Art(w, h, 707);
  a.bg(a.lin([[0, '#08090d'], [0.42, '#0e121c'], [0.76, '#1c1428'], [1, '#2b1322']], 0, 0, 1, 0.85));
  a.glow(1210, 460, 650, '#ce1e49', 0.44); a.glow(1250, 520, 410, '#f5b041', 0.52); a.glow(920, 320, 480, '#59b0ff', 0.16);
  for (let i = 0; i < 150; i++) a.circle((220 + a.r() * 1360).toFixed(1), (24 + a.r() * 390).toFixed(1), (0.5 + a.r() ** 2 * 1.4).toFixed(2), '#e8eefc', (0.14 + a.r() * 0.44).toFixed(2));
  // Subtle diagonal dusk light shafts behind the horizon peaks.
  for (const [x1, x2, op] of [[1210, 920, 0.09], [1240, 1080, 0.12], [1260, 1380, 0.1]]) {
    a.path(poly([[1240, 510], [x1, 120], [x2, 120]]), a.lin([[0.15, '#ffc866', 0], [0.56, '#ce1e49', op]]));
  }
  a.circle(1240, 498, 76, a.rad(1240, 498, 76, [[0, '#fff4d4', 0.94], [0.65, '#f5b041', 0.74], [1, '#ce1e49', 0]]));
  // Distant alpine ridge, grounded studio monolith spire, and foreground dusk ridge.
  a.path(a.ridge(492, 105, 0.54, 8), a.lin([[0, '#101420'], [0.6, '#231a30'], [1, '#441a2d']], 0, 0, 1, 0), 0.92);
  a.path(poly([[1076, 640], [1108, 404], [1124, 404], [1156, 640]]), '#191324');
  a.path(poly([[1113, 404], [1116, 352], [1119, 404]]), '#f5b041', 0.9); a.rect(1114, 432, 4, 28, '#ffc866', 0.9);
  a.path(a.ridge(568, 86, 0.56, 8), a.lin([[0, '#0c0f18'], [0.64, '#181422'], [1, '#2a1320']], 0, 0, 1, 0), 0.98);
  // Calm obsidian mirror fjord from y=628..900 with warm ruby & amber dusk shimmer.
  a.rect(0, 628, w, 272, a.lin([[0.7, '#12131e'], [0.86, '#0b0d14'], [1, '#08090d']]));
  a.glow(1240, 650, 360, '#ce1e49', 0.3); a.glow(1240, 642, 220, '#f5b041', 0.35);
  for (let i = 0; i < 44; i++) {
    const t = i / 44; const ry = 636 + t ** 1.35 * 232; const rw = 44 + (1 - Math.abs(t - 0.35)) * 190 + a.r() * 70;
    const rx = 1240 - rw / 2 + (a.r() - 0.5) * (90 + t * 240);
    a.rect(rx.toFixed(1), ry.toFixed(1), rw.toFixed(1), (1.2 + t * 2.2).toFixed(1), i % 3 === 0 ? '#ffc866' : '#ff7596', (0.36 - t * 0.25).toFixed(2), 1);
  }
  a.path(a.ridge(792, 58, 0.52, 8), '#07080c');
  a.rect(0, 0, 760, h, a.lin([[0, '#090a0d', 0.72], [0.32, '#090a0d', 0.35], [0.48, '#090a0d', 0]], 0, 0, 1, 0));
  return a.finish({ grain: 0.08, vignette: 0.26 });
}
