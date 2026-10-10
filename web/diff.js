// Line diffs for the Changes pane and the Edit tool: Myers' O(ND) algorithm,
// grouped into hunks with context.

import { esc } from './markdown.js';

const MAX_EDIT = 4000;

/** [{t:' '|'+'|'-', a, b, text}] where a/b are 1-based old/new line numbers. */
export function diffLines(before, after) {
  const a = before === '' ? [] : before.replace(/\n$/, '').split('\n');
  const b = after === '' ? [] : after.replace(/\n$/, '').split('\n');
  const n = a.length, m = b.length, max = n + m;
  const v = new Int32Array(2 * max + 2);
  const trace = [];
  let found = false;
  for (let d = 0; d <= Math.min(max, MAX_EDIT) && !found; d++) {
    trace.push(v.slice());
    for (let k = -d; k <= d; k += 2) {
      let x = k === -d || (k !== d && v[max + k - 1] < v[max + k + 1]) ? v[max + k + 1] : v[max + k - 1] + 1;
      let y = x - k;
      while (x < n && y < m && a[x] === b[y]) { x++; y++; }
      v[max + k] = x;
      if (x >= n && y >= m) { found = true; break; }
    }
  }
  if (!found) {
    // Too different to align: everything out, everything in.
    return [...a.map((text, i) => ({ t: '-', a: i + 1, text })), ...b.map((text, i) => ({ t: '+', b: i + 1, text }))];
  }
  const ops = [];
  let x = n, y = m;
  for (let d = trace.length - 1; d >= 0; d--) {
    const vv = trace[d];
    const k = x - y;
    const prevK = k === -d || (k !== d && vv[max + k - 1] < vv[max + k + 1]) ? k + 1 : k - 1;
    const px = vv[max + prevK], py = px - prevK;
    while (x > px && y > py) { ops.push({ t: ' ', a: x, b: y, text: a[x - 1] }); x--; y--; }
    if (d > 0) {
      if (x === px) { ops.push({ t: '+', b: y, text: b[y - 1] }); y--; }
      else { ops.push({ t: '-', a: x, text: a[x - 1] }); x--; }
    }
  }
  return ops.reverse();
}

export function counts(ops) {
  let add = 0, del = 0;
  for (const o of ops) { if (o.t === '+') add++; else if (o.t === '-') del++; }
  return { add, del };
}

/** HTML for a unified diff with `context` lines around each change. */
export function renderDiff(ops, context = 3) {
  const keep = new Uint8Array(ops.length);
  ops.forEach((o, i) => {
    if (o.t !== ' ') for (let j = Math.max(0, i - context); j <= Math.min(ops.length - 1, i + context); j++) keep[j] = 1;
  });
  const out = [];
  let skipped = 0;
  ops.forEach((o, i) => {
    if (!keep[i]) { skipped++; return; }
    if (skipped) { out.push(`<div class="hunk">${skipped} unchanged line${skipped === 1 ? '' : 's'}</div>`); skipped = 0; }
    const cls = o.t === '+' ? 'a' : o.t === '-' ? 'd' : '';
    const num = o.t === '+' ? o.b : o.a;
    out.push(`<div class="ln ${cls}"><span class="num">${num ?? ''}</span><span class="sign">${o.t === ' ' ? '' : o.t}</span><span class="code">${esc(o.text) || ' '}</span></div>`);
  });
  if (skipped) out.push(`<div class="hunk">${skipped} unchanged line${skipped === 1 ? '' : 's'}</div>`);
  return `<div class="diff">${out.join('')}</div>`;
}
