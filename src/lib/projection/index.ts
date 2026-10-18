export interface StyledChar {
  c: string;
  b: boolean; // bold (**text**)
  i?: boolean; // italic (*text*)
}

export interface ProjectedLine {
  prefix: string;
  chars: StyledChar[];
  raw: string;
  dirty: boolean;
}

const PREFIX_RE = /^(\s*(?:[-*+] \[[ xX]\] |[-*+] |\d+\. |>+ )|#{1,6} )/;
const BOLD_RE = /\*\*(.+?)\*\*/g;

export function parseLine(raw: string): ProjectedLine {
  const m = PREFIX_RE.exec(raw);
  const prefix = m ? m[1] : '';
  const body = raw.slice(prefix.length);
  const chars: StyledChar[] = [];
  let last = 0;

  for (const bm of body.matchAll(BOLD_RE)) {
    const idx = bm.index ?? 0;
    for (const c of Array.from(body.slice(last, idx))) {
      chars.push({ c, b: false });
    }
    for (const c of Array.from(bm[1])) {
      chars.push({ c, b: true });
    }
    last = idx + bm[0].length;
  }
  for (const c of Array.from(body.slice(last))) {
    chars.push({ c, b: false });
  }
  return { prefix, chars, raw, dirty: false };
}

export function parseMarkdown(text: string): ProjectedLine[] {
  return text.split('\n').map(parseLine);
}

export function projectLines(model: ProjectedLine[]): string[] {
  return model.map((l) => l.chars.map((x) => x.c).join(''));
}

export function serializeLine(l: ProjectedLine): string {
  if (!l.dirty) return l.raw;
  let out = l.prefix;
  let idx = 0;
  const cs = l.chars;
  while (idx < cs.length) {
    let j = idx;
    while (j < cs.length && cs[j].b === cs[idx].b) j++;
    const s = cs
      .slice(idx, j)
      .map((x) => x.c)
      .join('');
    if (cs[idx].b) {
      const m = /^(\s*)(.*?)(\s*)$/s.exec(s);
      out += m && m[2] ? `${m[1]}**${m[2]}**${m[3]}` : s;
    } else {
      out += s;
    }
    idx = j;
  }
  return out;
}

export function serializeMarkdown(model: ProjectedLine[]): string {
  return model.map(serializeLine).join('\n');
}

function locate(model: ProjectedLine[], idx: number): [number, number] {
  let rest = idx;
  for (let li = 0; li < model.length; li++) {
    const n = model[li].chars.length;
    if (rest <= n) return [li, rest];
    rest -= n + 1;
  }
  return [model.length - 1, model[model.length - 1]?.chars.length ?? 0];
}

function locateFirstDeletedStyle(
  model: ProjectedLine[],
  sl: number,
  sc: number,
  el: number,
  ec: number
): boolean | null {
  for (let li = sl; li <= el; li++) {
    const from = li === sl ? sc : 0;
    const to = li === el ? ec : model[li].chars.length;
    if (from < to) return model[li].chars[from].b;
  }
  return null;
}

function nextListPrefix(prefix: string): string {
  if (/^\s*[-*+] \[[ xX]\] $/.test(prefix)) {
    return prefix.replace(/\[[xX]\]/, '[ ]');
  }
  const numMatch = /^(\s*)(\d+)(\. )$/.exec(prefix);
  if (numMatch) {
    return `${numMatch[1]}${ Number(numMatch[2]) + 1 }${numMatch[3]}`;
  }
  if (/^\s*(?:[-*+] |>+ )$/.test(prefix)) {
    return prefix;
  }
  return '';
}

export function applyProjected(model: ProjectedLine[], newLines: string[]): ProjectedLine[] {
  const oldFlat = Array.from(projectLines(model).join('\n'));
  const newFlat = Array.from(newLines.join('\n'));
  let p = 0;
  while (p < oldFlat.length && p < newFlat.length && oldFlat[p] === newFlat[p]) p++;
  let s = 0;
  while (
    s < oldFlat.length - p &&
    s < newFlat.length - p &&
    oldFlat[oldFlat.length - 1 - s] === newFlat[newFlat.length - 1 - s]
  ) {
    s++;
  }
  if (p === oldFlat.length && p === newFlat.length) return model;

  const a = p;
  const b = oldFlat.length - s;
  const ins = newFlat.slice(p, newFlat.length - s);
  const [sl, sc] = locate(model, a);
  const [el, ec] = locate(model, b);

  const left = model[sl].chars.slice(0, sc);
  const right = model[el].chars.slice(ec);

  let bold = false;
  const firstDeleted = locateFirstDeletedStyle(model, sl, sc, el, ec);
  if (firstDeleted !== null) bold = firstDeleted;
  else if (left.length) bold = left[left.length - 1].b;

  const segs: StyledChar[][] = [[]];
  for (const c of ins) {
    if (c === '\n') segs.push([]);
    else segs[segs.length - 1].push({ c, b: bold });
  }
  const k = segs.length - 1;
  const mk = (prefix: string, chars: StyledChar[]): ProjectedLine => ({
    prefix,
    chars,
    raw: '',
    dirty: true,
  });

  const result: ProjectedLine[] = [];
  if (k === 0) {
    const prefix = left.length ? model[sl].prefix : model[el].prefix;
    result.push(mk(prefix, [...left, ...segs[0], ...right]));
  } else {
    // Enter on an empty list/quote item clears the prefix (per SPEC.md 4.2)
    if (
      k === 1 &&
      left.length === 0 &&
      right.length === 0 &&
      segs[0].length === 0 &&
      segs[1].length === 0 &&
      nextListPrefix(model[sl].prefix) !== ''
    ) {
      result.push(mk('', []));
    } else {
      const firstPrefix = left.length || right.length === 0 ? model[sl].prefix : '';
      const contPrefix = nextListPrefix(model[sl].prefix);
      result.push(mk(firstPrefix, [...left, ...segs[0]]));
      for (let i = 1; i < k; i++) {
        result.push(mk(contPrefix, segs[i]));
      }
      result.push(mk(contPrefix || (left.length ? '' : model[el].prefix), [...segs[k], ...right]));
    }
  }
  return [...model.slice(0, sl), ...result, ...model.slice(el + 1)];
}

export interface ReparseOutcome {
  model: ProjectedLine[];
  bufferChanged: boolean;
  newCursorCol: number;
}

/**
 * Re-parses dirty lines when the user leaves a line (Enter, j/k) or exits Insert mode (Esc)
 * so typed markdown syntax (# Heading, - list, **bold**) renders immediately without switching to Code View.
 */
export function reparseCompletedLines(
  model: ProjectedLine[],
  prevRow: number,
  prevMode: string,
  newRow: number,
  newCol: number,
  newMode: string
): ReparseOutcome {
  const leftLine = newRow !== prevRow;
  const exitedInsert = prevMode.startsWith('i') && !newMode.startsWith('i');
  if (!leftLine && !exitedInsert) {
    return { model, bufferChanged: false, newCursorCol: newCol };
  }

  let bufferChanged = false;
  let newCursorCol = newCol;
  const nextModel = [...model];

  for (let i = 0; i < nextModel.length; i++) {
    const isCurrentInsertLine = newMode.startsWith('i') && i === newRow;
    if (isCurrentInsertLine || !nextModel[i].dirty) continue;

    const rawLine = serializeLine(nextModel[i]);
    const reparsed = parseLine(rawLine);
    const oldVisible = nextModel[i].chars.map((x) => x.c).join('');
    const newVisible = reparsed.chars.map((x) => x.c).join('');

    const styleChanged =
      reparsed.prefix !== nextModel[i].prefix ||
      oldVisible !== newVisible ||
      reparsed.chars.some(
        (ch, idx) => !nextModel[i].chars[idx] || ch.b !== nextModel[i].chars[idx].b
      );

    if (styleChanged) {
      if (i === newRow) {
        const removedChars = Array.from(oldVisible).length - Array.from(newVisible).length;
        newCursorCol = Math.max(0, newCursorCol - Math.max(0, removedChars));
      }
      nextModel[i] = reparsed;
      if (oldVisible !== newVisible) {
        bufferChanged = true;
      }
    }
  }

  return { model: nextModel, bufferChanged, newCursorCol };
}
