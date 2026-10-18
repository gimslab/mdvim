// Spike 1: markdown <-> projected (visible) text.
// Model: lines of { prefix, chars:[{c,b}], raw, dirty }.
// Unchanged lines serialize from `raw` (byte-preserving); touched lines are regenerated.

const PREFIX_RE = /^(\s*(?:[-*+]|\d+\.) |#{1,6} )/;
const BOLD_RE = /\*\*(.+?)\*\*/g;

function parseLine(raw) {
  const m = PREFIX_RE.exec(raw);
  const prefix = m ? m[1] : '';
  const body = raw.slice(prefix.length);
  const chars = [];
  let last = 0;
  for (const bm of body.matchAll(BOLD_RE)) {
    for (const c of Array.from(body.slice(last, bm.index))) chars.push({ c, b: false });
    for (const c of Array.from(bm[1])) chars.push({ c, b: true });
    last = bm.index + bm[0].length;
  }
  for (const c of Array.from(body.slice(last))) chars.push({ c, b: false });
  return { prefix, chars, raw, dirty: false };
}

export function parse(text) {
  return text.split('\n').map(parseLine);
}

export function project(model) {
  return model.map((l) => l.chars.map((x) => x.c).join(''));
}

function serializeLine(l) {
  if (!l.dirty) return l.raw;
  let out = l.prefix;
  let i = 0;
  const cs = l.chars;
  while (i < cs.length) {
    let j = i;
    while (j < cs.length && cs[j].b === cs[i].b) j++;
    const s = cs.slice(i, j).map((x) => x.c).join('');
    if (cs[i].b) {
      const m = /^(\s*)(.*?)(\s*)$/s.exec(s);
      out += m[2] ? `${m[1]}**${m[2]}**${m[3]}` : s;
    } else out += s;
    i = j;
  }
  return out;
}

export function serialize(model) {
  return model.map(serializeLine).join('\n');
}

// Flat index (over code points, '\n' counted) -> [line, col]
function locate(model, idx) {
  let rest = idx;
  for (let li = 0; li < model.length; li++) {
    const n = model[li].chars.length;
    if (rest <= n) return [li, rest];
    rest -= n + 1;
  }
  throw new Error('index out of range');
}

// Apply the new projected lines coming back from Neovim to the model.
export function applyProjected(model, newLines) {
  const oldFlat = Array.from(project(model).join('\n'));
  const newFlat = Array.from(newLines.join('\n'));
  let p = 0;
  while (p < oldFlat.length && p < newFlat.length && oldFlat[p] === newFlat[p]) p++;
  let s = 0;
  while (
    s < oldFlat.length - p && s < newFlat.length - p &&
    oldFlat[oldFlat.length - 1 - s] === newFlat[newFlat.length - 1 - s]
  ) s++;
  if (p === oldFlat.length && p === newFlat.length) return model;

  const a = p;
  const b = oldFlat.length - s;
  const ins = newFlat.slice(p, newFlat.length - s);
  const [sl, sc] = locate(model, a);
  const [el, ec] = locate(model, b);

  const left = model[sl].chars.slice(0, sc);
  const right = model[el].chars.slice(ec);

  // Style rule for inserted text: replacing -> style of first replaced char;
  // pure insertion -> style of char to the left (same line), else plain.
  let bold = false;
  const firstDeleted = locateFirstDeletedStyle(model, sl, sc, el, ec);
  if (firstDeleted !== null) bold = firstDeleted;
  else if (left.length) bold = left[left.length - 1].b;

  const segs = [[]];
  for (const c of ins) {
    if (c === '\n') segs.push([]);
    else segs[segs.length - 1].push({ c, b: bold });
  }
  const k = segs.length - 1;
  const mk = (prefix, chars) => ({ prefix, chars, raw: '', dirty: true });
  const result = [];
  if (k === 0) {
    const prefix = left.length ? model[sl].prefix : model[el].prefix;
    result.push(mk(prefix, [...left, ...segs[0], ...right]));
  } else {
    result.push(mk(left.length ? model[sl].prefix : '', [...left, ...segs[0]]));
    for (let i = 1; i < k; i++) result.push(mk('', segs[i]));
    result.push(mk(model[el].prefix, [...segs[k], ...right]));
  }
  const out = [...model.slice(0, sl), ...result, ...model.slice(el + 1)];
  return out;
}

function locateFirstDeletedStyle(model, sl, sc, el, ec) {
  for (let li = sl; li <= el; li++) {
    const from = li === sl ? sc : 0;
    const to = li === el ? ec : model[li].chars.length;
    if (from < to) return model[li].chars[from].b;
  }
  return null;
}
