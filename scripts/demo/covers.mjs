// Procedural 7.0 console covers, drawn 100% by code (SVG + Figtree), zero AI and zero external image inputs.
// Usage:
//   node covers.mjs                 -> 36 Nightjar 2:3 Q92 JPEGs (app/src/debug/assets/demo/art/1..36.jpg)
//   node covers.mjs <slug>...       -> specific Nightjar cover(s)
//   node covers.mjs --host          -> 14 Rubylight 2:3 WebPs (docs/media/covers/*.webp) + home-landscape.webp
//   node covers.mjs --all           -> both Nightjar and Rubylight cover sets + contact sheets
import { existsSync, mkdirSync, rmSync, writeFileSync, readFileSync, readdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { resolve, dirname, delimiter } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { extraScenes } from './covers_scenes.mjs';
import { hostScenes, homeLandscape } from './covers_host.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const client = resolve(here, '../..');
const repoRoot = resolve(client, '..');

export function rng(seed) {
  let s = seed >>> 0;
  return () => {
    s = (s + 0x6d2b79f5) >>> 0;
    let t = s;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export const poly = (pts) => 'M' + pts.map((p) => `${p[0].toFixed(1)},${p[1].toFixed(1)}`).join(' L') + ' Z';

export const puff = (a, x, y, s, c, op = 0.92) => {
  for (const [dx, dy, r] of [[0, 0, 34], [38, -14, 42], [80, 0, 32], [40, 10, 40]]) {
    a.circle(x + dx * s, y + dy * s, r * s, c, op);
  }
};

export const flag = (a, x, y, c, k = '#0a0d12') => {
  a.rect(x - 2, y - 70, 4, 70, k);
  a.path(poly([[x + 2, y - 70], [x + 44, y - 56], [x + 2, y - 42]]), c);
};

export class Art {
  constructor(w = 600, h = 900, seed = 1) {
    this.w = w; this.h = h; this.defs = []; this.body = []; this.n = 0; this.r = rng(seed);
  }
  id() { return 'i' + this.n++; }
  stops(stops) {
    return stops.map(([o, c, a]) => `<stop offset="${o}" stop-color="${c}"${a === undefined ? '' : ` stop-opacity="${a}"`}/>`).join('');
  }
  lin(stops, x1 = 0, y1 = 0, x2 = 0, y2 = 1) {
    const id = this.id();
    this.defs.push(`<linearGradient id="${id}" gradientUnits="userSpaceOnUse" x1="${x1 * this.w}" y1="${y1 * this.h}" x2="${x2 * this.w}" y2="${y2 * this.h}">${this.stops(stops)}</linearGradient>`);
    return `url(#${id})`;
  }
  rad(cx, cy, r, stops) {
    const id = this.id();
    this.defs.push(`<radialGradient id="${id}" gradientUnits="userSpaceOnUse" cx="${cx}" cy="${cy}" r="${r}">${this.stops(stops)}</radialGradient>`);
    return `url(#${id})`;
  }
  add(s) { this.body.push(s); return this; }
  bg(fill) { return this.add(`<rect width="${this.w}" height="${this.h}" fill="${fill}"/>`); }
  rect(x, y, w, h, fill, op = 1, rx = 0) { return this.add(`<rect x="${x}" y="${y}" width="${w}" height="${h}" rx="${rx}" fill="${fill}" opacity="${op}"/>`); }
  circle(x, y, r, fill, op = 1) { return this.add(`<circle cx="${x}" cy="${y}" r="${r}" fill="${fill}" opacity="${op}"/>`); }
  glow(x, y, r, c, op = 1) { return this.circle(x, y, r, this.rad(x, y, r, [[0, c, 1], [0.35, c, 0.45], [1, c, 0]]), op); }
  path(d, fill, op = 1, extra = '') { return this.add(`<path d="${d}" fill="${fill}" opacity="${op}" ${extra}/>`); }
  line(d, stroke, width, op = 1, extra = '') { return this.add(`<path d="${d}" fill="none" stroke="${stroke}" stroke-width="${width}" stroke-linecap="round" stroke-linejoin="round" opacity="${op}" ${extra}/>`); }
  blur(std) {
    const id = this.id();
    this.defs.push(`<filter id="${id}" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="${std}"/></filter>`);
    return `filter="url(#${id})"`;
  }
  scrimTop(c = '#05070d', op = 0.46, end = 0.3) {
    return this.rect(0, 0, this.w, this.h * end, this.lin([[0, c, op], [end * 0.65, c, op * 0.45], [end, c, 0]]));
  }
  scrimBottom(c = '#05070d', op = 0.56, start = 0.68) {
    return this.rect(0, this.h * start, this.w, this.h * (1 - start), this.lin([[start, c, 0], [start + (1 - start) * 0.45, c, op * 0.6], [1, c, op]]));
  }
  ridge(base, amp, rough = 0.55, step = 7, r = this.r) {
    const n = 1 << step; const ys = new Array(n + 1).fill(0);
    ys[0] = (r() - 0.5) * amp; ys[n] = (r() - 0.5) * amp;
    let scale = amp;
    for (let size = n; size > 1; size >>= 1) {
      for (let i = size >> 1; i < n; i += size) ys[i] = (ys[i - (size >> 1)] + ys[i + (size >> 1)]) / 2 + (r() - 0.5) * scale;
      scale *= rough;
    }
    const pts = ys.map((y, i) => [(i / n) * this.w, base + y]);
    this.lastRidge = pts;
    return `M0,${this.h} ` + pts.map((p) => `L${p[0].toFixed(1)},${p[1].toFixed(1)}`).join(' ') + ` L${this.w},${this.h} Z`;
  }
  hill(base, parts) {
    const pts = [];
    for (let i = 0; i <= 80; i++) {
      const x = (i / 80) * this.w; let y = base;
      for (const [a, f, p] of parts) y += a * Math.sin((x / this.w) * f * Math.PI * 2 + p);
      pts.push([x, y]);
    }
    this.lastRidge = pts;
    return `M0,${this.h} ` + pts.map((p) => `L${p[0].toFixed(1)},${p[1].toFixed(1)}`).join(' ') + ` L${this.w},${this.h} Z`;
  }
  stars(n, maxY, c = '#fff', big = 1.6) {
    for (let i = 0; i < n; i++) this.circle((this.r() * this.w).toFixed(1), (this.r() * maxY).toFixed(1), (0.4 + this.r() ** 3 * big).toFixed(2), c, (0.25 + this.r() * 0.75).toFixed(2));
    return this;
  }
  text(lines, { x = 54, y = 116, size = 80, lead = 0.95, fill = '#fff', anchor = 'start', track = 0, weight = 600, skew = 0, heavy = 0, op = 1, shadow = '0 4px 18px rgba(0,0,0,0.42)' } = {}) {
    let o = '';
    lines.forEach((t, i) => {
      const yy = y + i * size * lead;
      const tf = skew ? ` transform="translate(${x} ${yy}) skewX(${skew})"` : ` x="${x}" y="${yy}"`;
      const st = heavy ? ` stroke="${fill}" stroke-width="${heavy}" stroke-linejoin="round" paint-order="stroke"` : '';
      const sh = shadow ? ` style="filter:drop-shadow(${shadow})"` : '';
      const esc = t.replace(/&/g, '&amp;');
      o += `<text data-title="1"${tf} font-family="Figtree" font-weight="${weight}" font-size="${size}" fill="${fill}" text-anchor="${anchor}" letter-spacing="${track}" opacity="${op}"${st}${sh}>${esc}</text>`;
    });
    return this.add(o);
  }
  finish({ grain = 0.12, vignette = 0.28 } = {}) {
    if (vignette) this.bg(this.rad(this.w / 2, this.h * 0.45, Math.max(this.w, this.h) * 0.78, [[0.55, '#000', 0], [1, '#000', vignette]]));
    if (grain) {
      const id = this.id();
      this.defs.push(`<filter id="${id}" x="0" y="0" width="100%" height="100%"><feTurbulence type="fractalNoise" baseFrequency="0.85" numOctaves="2" seed="7"/><feColorMatrix values="0 0 0 0 1  0 0 0 0 1  0 0 0 0 1  0 0 0 1.6 -0.55"/></filter>`);
      this.add(`<rect width="${this.w}" height="${this.h}" filter="url(#${id})" opacity="${grain}" style="mix-blend-mode:soft-light"/>`);
    }
    return `<svg xmlns="http://www.w3.org/2000/svg" width="${this.w}" height="${this.h}" viewBox="0 0 ${this.w} ${this.h}"><defs>${this.defs.join('')}</defs>${this.body.join('')}</svg>`;
  }
}

export const coreScenes = {};

// 1. Desktop — 7.0 Obsidian Cinema workstation stage (zero moon, zero orbit rings).
coreScenes['desktop'] = () => {
  const a = new Art(600, 900, 3);
  a.bg(a.lin([[0, '#090a0d'], [0.58, '#10131c'], [1, '#13161f']]));
  a.glow(430, 290, 340, '#59b0ff', 0.22); a.glow(240, 480, 360, '#f5b041', 0.24);
  for (let i = 0; i < 9; i++) a.line(`M48,${620 + i * 32} L552,${620 + i * 32}`, '#ffffff', 1, 0.03 + i * 0.005);
  const win = (x, y, w, h, bg, rim, accent, bars) => {
    a.add(`<rect x="${x}" y="${y + 14}" width="${w}" height="${h}" rx="18" fill="#000" opacity=".48" ${a.blur(16)}/>`);
    a.rect(x, y, w, h, bg, 0.96, 16);
    a.add(`<rect x="${x + 0.75}" y="${y + 0.75}" width="${w - 1.5}" height="${h - 1.5}" rx="15.2" fill="none" stroke="${rim}" stroke-opacity=".34" stroke-width="1.5"/>`);
    a.rect(x + 14, y + 1, w - 28, 1.2, '#ffffff', 0.22); a.rect(x, y + 36, w, 1, '#ffffff', 0.07);
    for (let i = 0; i < 3; i++) a.circle(x + 22 + i * 15, y + 19, 4, i === 0 ? accent : '#9aa3b5', i === 0 ? 0.9 : 0.35);
    a.rect(x + w - 76, y + 13, 56, 12, accent, 0.22, 6);
    bars.forEach(([bx, by, bw, bh, bc, bo]) => a.rect(x + bx, y + by, bw, bh, bc, bo, 5));
  };
  win(64, 184, 376, 232, '#11141d', '#59b0ff', '#59b0ff', [
    [22, 56, 148, 12, '#f5f7fa', 0.75], [22, 78, 240, 9, '#9aa3b5', 0.35],
    [22, 108, 160, 88, '#181c28', 0.95], [194, 108, 160, 88, '#181c28', 0.95],
    [36, 168, 132, 4, '#59b0ff', 0.85], [208, 152, 112, 4, '#45e0a0', 0.75],
  ]);
  win(148, 350, 388, 254, '#151924', '#f5b041', '#f5b041', [
    [24, 58, 182, 14, '#f5f7fa', 0.9], [24, 82, 276, 9, '#c7cdd9', 0.42],
    [24, 112, 340, 62, '#0d1017', 0.95], [24, 188, 210, 40, '#1d2230', 0.9], [244, 188, 120, 40, '#f5b041', 0.2],
  ]);
  a.line('M186,502 C226,482 256,512 296,486 C336,460 374,498 418,476 C452,462 478,478 500,468', '#59b0ff', 2.6, 0.92);
  a.line('M186,512 C232,504 268,494 312,506 C356,518 398,488 446,496 C472,500 488,492 500,490', '#f5b041', 2.2, 0.88);
  a.rect(76, 548, 208, 96, '#181c27', 0.95, 14);
  a.add('<rect x="76.75" y="548.75" width="206.5" height="94.5" rx="13.2" fill="none" stroke="#ffffff" stroke-opacity=".14" stroke-width="1.5"/>');
  a.circle(106, 580, 7, '#45e0a0', 0.95); a.rect(124, 574, 118, 11, '#f5f7fa', 0.82, 5);
  a.rect(98, 604, 162, 8, '#59b0ff', 0.45, 4); a.rect(98, 620, 116, 8, '#f5b041', 0.65, 4);
  a.rect(56, 730, 52, 4, '#f5b041', 0.95, 2);
  a.text(['Desktop'], { x: 56, y: 814, size: 78, weight: 600, heavy: 0.8, track: -1.5, fill: '#f5f7fa' });
  return a.finish({ vignette: 0.24, grain: 0.08 });
};
// 2. Racing Game — dusk highway, tail-light ribbons.
coreScenes['racing-game'] = () => {
  const w = 600; const h = 900; const a = new Art(w, h, 12);
  const hz = h * 0.56; const hzF = hz / h; const vx = w * 0.56; const u = 600;
  a.bg(a.lin([[0, '#120826'], [hzF * 0.42, '#4b1160'], [hzF * 0.72, '#c5275c'], [hzF * 0.9, '#ff6d3a'], [hzF, '#ffc56b'], [1, '#ffc56b']]));
  a.stars(90, hz * 0.5, '#ffe9f4', 1.4); a.glow(vx, hz, u * 0.92, '#ffb36b', 0.8);
  a.circle(vx, hz - u * 0.13, u * 0.2, a.rad(vx, hz - u * 0.13, u * 0.2, [[0, '#fff6d6'], [0.72, '#ffd98a'], [1, '#ff9d4f']]));
  a.path(a.ridge(hz - h * 0.035, h * 0.075, 0.56), '#6d1a5e', 0.9); a.path(a.ridge(hz - h * 0.008, h * 0.05, 0.6), '#2e0e3d');
  const steps = 60; const L = []; const R = []; const C = [];
  const curve = (t) => vx + (w * 0.5 - vx) * t + Math.sin(t * Math.PI) * w * 0.05 * (1 - t);
  for (let i = 0; i <= steps; i++) {
    const t = i / steps; const y = hz + (h - hz) * t ** 1.5 + 0.5; const half = w * 0.012 + w * 0.86 * t ** 1.5;
    L.push([curve(t) - half, y]); R.push([curve(t) + half, y]); C.push([curve(t), y, half]);
  }
  const strip = (l, r) => 'M' + l.map((p) => `${p[0].toFixed(1)},${p[1].toFixed(1)}`).join(' L') + ' L' + [...r].reverse().map((p) => `${p[0].toFixed(1)},${p[1].toFixed(1)}`).join(' L') + ' Z';
  a.path(`M0,${hz} L${w},${hz} L${w},${h} L0,${h} Z`, a.lin([[hzF, '#1b0a24'], [1, '#07050c']]));
  a.path(strip(L, R), a.lin([[hzF, '#6a2a4e'], [hzF + (1 - hzF) * 0.25, '#2a1230'], [1, '#100a16']]));
  a.path(strip(C.map((p) => [p[0] - p[2] * 0.42, p[1]]), C.map((p) => [p[0] + p[2] * 0.42, p[1]])), a.lin([[hzF, '#ffb36b', 0.75], [hzF + (1 - hzF) * 0.6, '#ff6d3a', 0.06], [1, '#ff6d3a', 0]]));
  const ribbon = (off, wid, color, op, from = 0) => {
    const l = []; const r = [];
    for (const p of C.slice(from)) { const cx = p[0] + p[2] * off; const hw = Math.max(0.6, p[2] * wid); l.push([cx - hw, p[1]]); r.push([cx + hw, p[1]]); }
    return [strip(l, r), color, op];
  };
  const trails = [
    ribbon(-0.97, 0.012, '#ffe1b8', 0.95), ribbon(0.97, 0.012, '#ffe1b8', 0.95),
    ribbon(-0.62, 0.016, '#ff2b4e', 1), ribbon(-0.57, 0.009, '#ff8a6e', 1),
    ribbon(0.56, 0.018, '#ffffff', 0.95), ribbon(0.62, 0.009, '#ffd9a0', 0.9),
    ribbon(-0.28, 0.012, '#ff3b5c', 0.9, 6), ribbon(0.27, 0.01, '#fff1d0', 0.8, 10),
  ];
  const gl = a.blur(u * 0.016);
  for (const [d, c, op] of trails) a.path(d, c, op * 0.85, gl);
  for (const [d, c, op] of trails) a.path(d, c, op);
  for (let i = 8; i < steps - 2; i += 6) {
    const s = C[i]; const e = C[Math.min(i + 3, steps)];
    a.path(`M${s[0] - s[2] * 0.012},${s[1]} L${s[0] + s[2] * 0.012},${s[1]} L${e[0] + e[2] * 0.012},${e[1]} L${e[0] - e[2] * 0.012},${e[1]} Z`, '#ffe9c7', 0.9);
  }
  const cs = u * 0.46; const cx = w * 0.5; const cy = h * 0.84;
  const X = (v) => (cx + v * cs).toFixed(1); const Y = (v) => (cy + v * cs).toFixed(1);
  a.add(`<ellipse cx="${cx}" cy="${Y(0.42)}" rx="${cs * 0.72}" ry="${cs * 0.08}" fill="#000" opacity=".75" ${a.blur(cs * 0.03)}/>`);
  a.rect(X(-0.54), Y(0.24), cs * 0.2, cs * 0.18, '#060409', 1, cs * 0.03); a.rect(X(0.34), Y(0.24), cs * 0.2, cs * 0.18, '#060409', 1, cs * 0.03);
  a.path(`M${X(-0.6)},${Y(0.3)} C${X(-0.62)},${Y(0.1)} ${X(-0.6)},${Y(0.0)} ${X(-0.5)},${Y(-0.05)} L${X(-0.32)},${Y(-0.3)} C${X(-0.14)},${Y(-0.36)} ${X(0.14)},${Y(-0.36)} ${X(0.32)},${Y(-0.3)} L${X(0.5)},${Y(-0.05)} C${X(0.6)},${Y(0.0)} ${X(0.62)},${Y(0.1)} ${X(0.6)},${Y(0.3)} Z`, a.lin([[(cy - 0.36 * cs) / h, '#3a1a42'], [(cy + 0.3 * cs) / h, '#0b0710']]));
  a.path(`M${X(-0.42)},${Y(-0.06)} L${X(-0.27)},${Y(-0.26)} C${X(-0.1)},${Y(-0.3)} ${X(0.1)},${Y(-0.3)} ${X(0.27)},${Y(-0.26)} L${X(0.42)},${Y(-0.06)} Z`, a.lin([[(cy - 0.3 * cs) / h, '#ff9a5a', 0.9], [(cy - 0.06 * cs) / h, '#3a1533', 1]]));
  a.rect(X(-0.52), Y(0.03), cs * 1.04, cs * 0.05, '#ff1f42', 1, cs * 0.02);
  a.add(`<rect x="${X(-0.56)}" y="${Y(0.01)}" width="${cs * 1.12}" height="${cs * 0.09}" fill="#ff2b4e" ${a.blur(cs * 0.035)}/>`);
  a.rect(X(-0.5), Y(0.042), cs, cs * 0.018, '#ffd0c8', 1); a.rect(X(-0.16), Y(0.16), cs * 0.32, cs * 0.07, '#17101c', 1, cs * 0.012);
  a.scrimTop('#120826', 0.42, 0.3);
  a.text(['RACING', 'GAME'], { x: 64, y: 146, size: 92, lead: 0.92, skew: -9, heavy: 1.8, track: -1, fill: '#fff6ea' });
  return a.finish();
};
// 3. Open World — full 2:3 layered ridges at dawn.
coreScenes['open-world'] = () => {
  const a = new Art(600, 900, 41);
  a.bg(a.lin([[0, '#09243b'], [0.28, '#1b6a86'], [0.46, '#6fb5a8'], [0.58, '#f5c98a'], [0.7, '#ff9f5c'], [1, '#ff9f5c']]));
  a.glow(380, 450, 290, '#fff0c0', 0.95); a.circle(380, 450, 56, '#fff8e0');
  for (const [b, amp, c] of [[520, 68, '#d98c6a'], [576, 84, '#a4716f'], [632, 84, '#5f6a78'], [696, 76, '#2c5365'], [754, 68, '#12394a']]) {
    a.path(a.ridge(b, amp, 0.58), c); a.bg(a.lin([[(b + 30) / 900, '#ffd9a0', 0.1], [(b + 120) / 900, '#ffd9a0', 0]]));
  }
  a.path(a.ridge(814, 42, 0.5), '#051b26');
  const ridge = a.lastRidge; let d = '';
  for (let i = 0; i < 72; i++) {
    const x = a.r() * 600; const p = ridge[Math.round((x / 600) * (ridge.length - 1))]; const ht = 28 + a.r() * 48; const wd = ht * 0.2;
    d += `M${x},${p[1] - ht} L${x + wd},${p[1] + 6} L${x - wd},${p[1] + 6} Z `;
  }
  a.path(d, '#051b26');
  let fl = '';
  for (const [x, y, s] of [[336, 418, 1], [362, 396, 0.8], [310, 398, 0.7], [388, 430, 0.62], [286, 426, 0.55]]) fl += `M${x - 14 * s},${y - 3 * s} Q${x - 6 * s},${y - 8 * s} ${x},${y} Q${x + 6 * s},${y - 8 * s} ${x + 14 * s},${y - 3 * s} `;
  a.line(fl, '#0c2f4c', 2.6);
  a.text(['Open', 'World'], { x: 300, y: 128, size: 88, lead: 0.92, heavy: 1.2, track: -2, fill: '#fffaf0', anchor: 'middle' });
  return a.finish({ vignette: 0.22 });
};
// 4. Space Sim — ringed planet and ship.
coreScenes['space-sim'] = () => {
  const a = new Art(600, 900, 5);
  a.bg(a.lin([[0, '#03040d'], [0.6, '#0a0d2c'], [1, '#160b37']]));
  a.glow(120, 240, 420, '#3f2fb5', 0.5); a.glow(540, 700, 420, '#c2389a', 0.32); a.stars(420, 900, '#dfe6ff', 1.8);
  const cx = 360; const cy = 500; const R = 230;
  const ring = (arc, col, baseOp) => `<g transform="rotate(-20 ${cx} ${cy})">` + [[R * 1.72, 2], [R * 1.6, 11], [R * 1.46, 18], [R * 1.33, 5]].map(([rx, sw], i) => `<path d="M${cx - rx},${cy} A${rx},${rx * 0.2} 0 0 ${arc} ${cx + rx},${cy}" fill="none" stroke="${col}" stroke-width="${sw}" opacity="${baseOp + i * 0.11}"/>`).join('') + '</g>';
  a.add(ring(1, '#e9c9a1', 0.35));
  const id = a.id(); a.defs.push(`<clipPath id="${id}"><circle cx="${cx}" cy="${cy}" r="${R}"/></clipPath>`);
  a.circle(cx, cy, R, a.lin([[0.36, '#f6c690'], [0.48, '#d9774f'], [0.6, '#7a2c54'], [0.76, '#22103a']], 0, 0, 0.9, 0.6));
  let bands = '';
  for (let i = -8; i < 9; i++) bands += `<rect x="${cx - R - 40}" y="${cy + i * 28}" width="${R * 2 + 80}" height="${5 + a.r() * 12}" fill="${i % 2 ? '#ffe6bd' : '#3c1036'}" opacity="${0.08 + a.r() * 0.14}" transform="rotate(-20 ${cx} ${cy})"/>`;
  a.add(`<g clip-path="url(#${id})">${bands}<circle cx="${cx + 110}" cy="${cy + 96}" r="${R * 1.05}" fill="${a.rad(cx + 110, cy + 96, R * 1.05, [[0.25, '#04030c', 0.92], [1, '#04030c', 0]])}"/></g>`);
  a.add(`<circle cx="${cx}" cy="${cy}" r="${R}" fill="none" stroke="#ffd9a8" stroke-width="1.5" opacity=".35"/>` + ring(0, '#f6ddb9', 0.45));
  const sx = 142; const sy = 738;
  a.path(`M${sx},${sy} L${sx - 150},${sy + 74} L${sx - 150},${sy + 92} Z`, a.lin([[0, '#7fe0ff', 0.9], [1, '#7fe0ff', 0]], sx / 600, 0, (sx - 150) / 600, 0));
  a.glow(sx - 4, sy + 4, 30, '#9ae8ff', 0.95);
  a.path(`M${sx - 8},${sy - 8} L${sx + 66},${sy - 44} L${sx + 38},${sy - 6} L${sx + 8},${sy + 16} Z`, '#f4efe6');
  a.path(`M${sx + 66},${sy - 44} L${sx + 38},${sy - 6} L${sx + 8},${sy + 16} L${sx + 26},${sy - 12} Z`, '#8d93b5');
  a.text(['SPACE SIM'], { x: 300, y: 122, size: 54, anchor: 'middle', track: 10, weight: 600, fill: '#f4efe6' });
  a.rect(258, 148, 84, 2.5, '#f5b041');
  return a.finish({ vignette: 0.2 });
};
// 5. Puzzle Night — isometric blocks.
coreScenes['puzzle-night'] = () => {
  const a = new Art(600, 900, 9);
  a.bg(a.lin([[0, '#14123f'], [0.55, '#2a1f6d'], [1, '#6a3aa6']])); a.glow(300, 560, 480, '#7a64ff', 0.6); a.stars(70, 340, '#fff', 1.2);
  const cube = (cx, cy, s, top, left, right) => {
    const dx = s * 0.866; const dy = s * 0.5;
    a.path(`M${cx},${cy - s} L${cx + dx},${cy - dy} L${cx},${cy} L${cx - dx},${cy - dy} Z`, top);
    a.path(`M${cx - dx},${cy - dy} L${cx},${cy} L${cx},${cy + s} L${cx - dx},${cy + dy} Z`, left);
    a.path(`M${cx + dx},${cy - dy} L${cx},${cy} L${cx},${cy + s} L${cx + dx},${cy + dy} Z`, right);
  };
  const S = 64; const dx = S * 0.866; const dy = S * 0.5;
  const pal = [['#b5a8ff', '#5240c2', '#2e2285'], ['#8d7cf2', '#3f2fa8', '#231a6e'], ['#d5ccff', '#6853d8', '#3a2c9c']];
  const heights = [[3, 2, 3, 1], [2, 3, 1, 2], [1, 2, 2, 0], [2, 1, 0, 1]];
  for (let gx = 0; gx < 4; gx++) for (let gy = 0; gy < 4; gy++) {
    for (let k = 0; k <= heights[gx][gy]; k++) { const p = pal[(gx + gy + k) % 3]; cube(300 + (gx - gy) * dx, 520 + (gx + gy) * dy - k * S, S, p[0], p[1], p[2]); }
  }
  const gx = 300 - dx; const gy = 520 + 5 * dy - 3 * S;
  a.glow(gx, gy, 230, '#ffc865', 0.9); cube(gx, gy, S, '#fff0c2', '#f2a93b', '#c97c18');
  a.text(['Puzzle', 'Night'], { x: 56, y: 130, size: 90, lead: 0.92, heavy: 1.2, track: -2, fill: '#f6f1ff' });
  return a.finish({ vignette: 0.28 });
};
// 6. Retro Arcade — crisp 5x7 pixel display well inside 48px safe margins.
const FONT = {
  R: ['11110', '10001', '10001', '11110', '10100', '10010', '10001'], E: ['11111', '10000', '10000', '11110', '10000', '10000', '11111'],
  T: ['11111', '00100', '00100', '00100', '00100', '00100', '00100'], O: ['01110', '10001', '10001', '10001', '10001', '10001', '01110'],
  A: ['01110', '10001', '10001', '11111', '10001', '10001', '10001'], C: ['01110', '10001', '10000', '10000', '10000', '10001', '01110'],
  D: ['11110', '10001', '10001', '10001', '10001', '10001', '11110'],
};
coreScenes['retro-arcade'] = () => {
  const a = new Art(600, 900, 31); const p = 10;
  a.bg(a.lin([[0, '#14062e'], [0.55, '#3b0c5e'], [1, '#c01e6f']]));
  const word = (t, x, y, px, c, sh) => {
    let g = `<g id="px-${t}" data-title="1">`;
    [...t].forEach((ch, i) => FONT[ch].forEach((row, ry) => [...row].forEach((b, rx) => {
      if (b === '1') { const X = x + (rx + i * 6) * px; const Y = y + ry * px; g += `<rect x="${X + px * 0.45}" y="${Y + px * 0.45}" width="${px}" height="${px}" fill="${sh}"/><rect x="${X}" y="${Y}" width="${px}" height="${px}" fill="${c}"/>`; }
    })));
    a.add(g + '</g>');
  };
  for (let i = 0; i < 60; i++) { const s = a.r() > 0.82 ? p : p / 2; a.rect(Math.floor(a.r() * 60) * p, Math.floor(a.r() * 44) * p, s, s, a.r() > 0.5 ? '#ffe36b' : '#7ef2ff', 0.8); }
  const stick = ['00000rrr00000', '0000rhrrr0000', '0000rrrrr0000', '00000rrr00000', '000000w000000', '000000w000000', '000000w000000', '00000ccc00000', '000ccccccc000', '00ccccccccc00'];
  const ip = 30; const ix = 300 - (13 * ip) / 2; const iy = 342;
  const sc = { r: '#ff5c8a', h: '#ffd5e2', w: '#fff6ea', c: a.lin([[iy / 900, '#7ef2ff'], [(iy + 10 * ip) / 900, '#37e0a8']]) };
  a.glow(300, iy + 130, 320, '#ff3fa4', 0.75);
  stick.forEach((row, ry) => [...row].forEach((b, rx) => { if (b !== '0') a.rect(ix + rx * ip, iy + ry * ip, ip, ip, sc[b]); }));
  a.rect(56, 692, 488, 18, '#fff6ea', 0.88); a.rect(56, 710, 488, 128, '#24093f');
  for (const [bx, c] of [[162, '#ffe36b'], [300, '#7ef2ff'], [438, '#37e0a8']]) for (const [i, j] of [[1, 0], [0, 1], [1, 1], [2, 1], [1, 2]]) a.rect(bx - 33 + i * 22, 738 + j * 22, 22, 22, c);
  word('RETRO', Math.round(300 - (5 * 6 * 13 - 13) / 2), 84, 13, '#ffe36b', '#d6186d');
  word('ARCADE', Math.round(300 - (6 * 6 * 11 - 11) / 2), 198, 11, '#fff6ea', '#7a1fb0');
  for (let y = 0; y < 900; y += 4) a.rect(0, y, 600, 1.4, '#000', 0.14);
  return a.finish({ grain: 0.06, vignette: 0.36 });
};

export const scenes = { ...coreScenes, ...extraScenes };

async function loadChromium() {
  const roots = [
    here,
    resolve(repoRoot, 'rust/web'),
    resolve(client, 'design/prototype'),
    ...(process.env.NODE_PATH ? process.env.NODE_PATH.split(delimiter).filter(Boolean) : []),
  ];
  for (const base of roots) {
    const pkg = resolve(base, 'package.json');
    const reqBase = existsSync(pkg) ? pkg : resolve(base, '_noop.js');
    const req = createRequire(reqBase);
    for (const mod of ['playwright-core', 'playwright']) {
      try { return req(mod).chromium; } catch { /* next candidate */ }
    }
  }
  throw new Error('Playwright not found. Run npm ci in client/scripts/demo or set NODE_PATH.');
}

function findBrowserExecutable(chromium) {
  const envExe = process.env.CHROME_PATH || process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH;
  if (envExe && existsSync(envExe)) return envExe;
  try {
    const def = chromium.executablePath();
    if (def && existsSync(def)) return def;
  } catch { /* fallback below */ }
  if (process.platform === 'win32') {
    const msPlaywright = resolve(process.env.LOCALAPPDATA || '', 'ms-playwright');
    if (existsSync(msPlaywright)) {
      const dirs = readdirSync(msPlaywright).filter((d) => d.startsWith('chromium-')).sort().reverse();
      for (const d of dirs) {
        const exe = resolve(msPlaywright, d, 'chrome-win64/chrome.exe');
        if (existsSync(exe)) return exe;
      }
      const headless = readdirSync(msPlaywright).filter((d) => d.startsWith('chromium_headless_shell-')).sort().reverse();
      for (const d of headless) {
        const exe = resolve(msPlaywright, d, 'chrome-headless-shell-win64/chrome-headless-shell.exe');
        if (existsSync(exe)) return exe;
      }
    }
    for (const sys of [
      'C:/Program Files/Google/Chrome/Application/chrome.exe',
      'C:/Program Files (x86)/Google/Chrome/Application/chrome.exe',
      'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe',
    ]) {
      if (existsSync(sys)) return sys;
    }
  }
  return null;
}

async function launchBrowser(chromium) {
  const args = ['--disable-gpu', '--disable-lcd-text', '--force-color-profile=srgb'];
  const executablePath = findBrowserExecutable(chromium);
  if (executablePath) return chromium.launch({ executablePath, args });
  try { return await chromium.launch({ channel: 'chrome', args }); } catch { return chromium.launch({ args }); }
}

const QUALITY = 92;
const SAFE_X = 48;
const slug = (title) => title.toLowerCase().replace(/ /g, '-');

function library() {
  const source = readFileSync(resolve(client, 'app/src/debug/java/com/limelight/demo/DemoConsole.java'), 'utf8');
  const block = source.match(/LIBRARY = \{([\s\S]*?)\n\s*\};/);
  if (!block) throw new Error('DemoConsole.LIBRARY not found');
  const slugs = [...block[1].matchAll(/\{"([^"]+)",/g)].map((m) => slug(m[1]));
  const missing = slugs.filter((s) => !scenes[s]);
  const extra = Object.keys(scenes).filter((s) => !slugs.includes(s));
  if (missing.length || extra.length) throw new Error(`Covers and DemoConsole.LIBRARY differ: no scene for [${missing}], no game for [${extra}]`);
  return slugs;
}

function pngToWebp(pngPath, webpPath, quality = 90) {
  try {
    execFileSync('python', ['-c', 'import sys; from PIL import Image; Image.open(sys.argv[1]).save(sys.argv[2], "WEBP", quality=int(sys.argv[3]), method=6)', pngPath, webpPath, String(quality)], { stdio: 'pipe' });
    return;
  } catch { /* fallback to ffmpeg */ }
  execFileSync('ffmpeg', ['-y', '-hide_banner', '-loglevel', 'error', '-i', pngPath, '-c:v', 'libwebp', '-lossless', '0', '-q:v', String(quality), '-compression_level', '6', webpPath], { stdio: 'pipe' });
}

async function assertTitleMargins(page, name, width) {
  const violations = await page.evaluate((safe) => {
    const out = [];
    for (const el of document.querySelectorAll('[data-title]')) {
      const r = el.getBoundingClientRect();
      if (r.width === 0) continue;
      if (r.left < safe - 0.5 || r.right > window.innerWidth - safe + 0.5) {
        out.push(`${el.textContent || el.id}: [${r.left.toFixed(1)}, ${r.right.toFixed(1)}]`);
      }
    }
    return out;
  }, SAFE_X);
  if (violations.length) {
    throw new Error(`Title safe-margin violation (< ${SAFE_X}px or > ${width - SAFE_X}px) in "${name}": ${violations.join('; ')}`);
  }
}

async function renderContactSheet(browser, tmp, items, outPath, cols = 9, tileW = 196, tileH = 294) {
  const rows = Math.ceil(items.length / cols);
  const sheetW = cols * tileW + (cols + 1) * 8;
  const sheetH = rows * tileH + (rows + 1) * 8;
  const sheet = await browser.newPage({ viewport: { width: sheetW, height: sheetH }, deviceScaleFactor: 1 });
  writeFileSync(tmp, `<body style="margin:0;background:#090a0d;display:grid;grid-template-columns:repeat(${cols},${tileW}px);grid-auto-rows:${tileH}px;gap:8px;padding:8px">${items.map((p) => `<img src="${pathToFileURL(p).href}" style="width:${tileW}px;height:${tileH}px;object-fit:cover;border-radius:10px">`).join('')}</body>`);
  await sheet.goto(pathToFileURL(tmp).href);
  await sheet.waitForLoadState('networkidle');
  await sheet.screenshot({ path: outPath });
  await sheet.close();
}

async function main() {
  const rawArgs = process.argv.slice(2);
  const doAll = rawArgs.includes('--all');
  const hostOnly = rawArgs.includes('--host') && !doAll;
  const doHost = doAll || rawArgs.includes('--host');
  const only = rawArgs.filter((a) => !a.startsWith('--'));

  const slugs = library();
  const unknown = only.filter((s) => !slugs.includes(s));
  if (unknown.length) throw new Error(`No such cover: ${unknown}`);

  const art = resolve(client, 'app/src/debug/assets/demo/art');
  const work = resolve(client, 'out/demo/assets');
  mkdirSync(work, { recursive: true });
  const font = (weight) => `@font-face{font-family:Figtree;font-weight:${weight};src:url(${pathToFileURL(resolve(client, `protocol/desktop/assets/Figtree-${weight}.ttf`)).href})}`;
  const css = `<style>${font(400)}${font(600)}html,body{margin:0;background:#090a0d}svg{display:block}</style>`;

  const chromium = await loadChromium();
  const browser = await launchBrowser(chromium);
  const tmp = resolve(work, '_cover.html');
  const tmpPng = resolve(work, '_cover.png');
  try {
    if (!hostOnly) {
      if (!only.length) rmSync(art, { recursive: true, force: true });
      mkdirSync(art, { recursive: true });
      const page = await browser.newPage({ viewport: { width: 600, height: 900 }, deviceScaleFactor: 1 });
      for (const [i, name] of slugs.entries()) {
        if (only.length && !only.includes(name)) continue;
        writeFileSync(tmp, css + scenes[name]());
        await page.goto(pathToFileURL(tmp).href);
        await page.evaluate(() => document.fonts.ready);
        await assertTitleMargins(page, name, 600);
        await page.screenshot({ path: resolve(art, `${i + 1}.jpg`), type: 'jpeg', quality: QUALITY });
      }
      await page.close();
      const files = readdirSync(art).filter((f) => f.endsWith('.jpg')).sort((x, y) => parseInt(x, 10) - parseInt(y, 10)).map((f) => resolve(art, f));
      await renderContactSheet(browser, tmp, files, resolve(work, 'covers-contact.png'), 9, 180, 270);
    }

    if (doHost) {
      const hostDir = resolve(repoRoot, 'docs/media/covers');
      mkdirSync(hostDir, { recursive: true });
      const page = await browser.newPage({ viewport: { width: 600, height: 900 }, deviceScaleFactor: 1 });
      const hostFiles = [];
      for (const [name, fn] of Object.entries(hostScenes)) {
        writeFileSync(tmp, css + fn());
        await page.goto(pathToFileURL(tmp).href);
        await page.evaluate(() => document.fonts.ready);
        await assertTitleMargins(page, `host:${name}`, 600);
        await page.screenshot({ path: tmpPng, type: 'png' });
        const dest = resolve(hostDir, `${name}.webp`);
        pngToWebp(tmpPng, dest, 90);
        hostFiles.push(dest);
      }
      await page.close();
      await renderContactSheet(browser, tmp, hostFiles, resolve(work, 'covers-host-contact.png'), 7, 200, 300);

      const landPage = await browser.newPage({ viewport: { width: 1600, height: 900 }, deviceScaleFactor: 1 });
      writeFileSync(tmp, css + homeLandscape());
      await landPage.goto(pathToFileURL(tmp).href);
      await landPage.evaluate(() => document.fonts.ready);
      await landPage.screenshot({ path: tmpPng, type: 'png' });
      await landPage.close();
      pngToWebp(tmpPng, resolve(repoRoot, 'rust/web/src/assets/home-landscape.webp'), 90);
    }
  } finally {
    rmSync(tmp, { force: true });
    rmSync(tmpPng, { force: true });
    await browser.close();
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((e) => { console.error(e); process.exit(1); });
}
