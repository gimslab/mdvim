export interface StyledChar {
  c: string;
  b: boolean; // bold (**text**)
  i: boolean; // italic (*text*)
  code: boolean; // inline code (`text`)
}

export interface ProjectedLine {
  prefix: string;
  chars: StyledChar[];
  raw: string;
  dirty: boolean;
}

// CommonMark 4.1 Thematic breaks: 0-3 leading spaces, 3+ matching -, *, or _ (with optional spaces/tabs between/after), 4+ leading spaces or leading tab is NOT a thematic break.
export const HR_RE = /^ {0,3}(?:-(?:[ \t]*-){2,}|\*(?:[ \t]*\*){2,}|_(?:[ \t]*_){2,})[ \t]*$/;
// CommonMark 4.2 ATX headings & 5.1 Block quotes allow 0-3 leading spaces; lists allow indentation for sublists.
const PREFIX_RE = /^(?:( {0,3}(?:#{1,6} |>+ ))|([ \t]*(?:[-*+] \[[ xX]\] |[-*+] |\d+\. )))/;
const INLINE_TOKEN_RE = /(\*\*(.+?)\*\*|`([^`]+)`|\*([^*]+)\*)/g;

export function isHorizontalRulePrefix(prefix: string): boolean {
  return HR_RE.test(prefix);
}

export function parseLine(raw: string): ProjectedLine {
  if (HR_RE.test(raw)) {
    return { prefix: raw, chars: [], raw, dirty: false };
  }
  const m = PREFIX_RE.exec(raw);
  const prefix = m ? (m[1] ?? m[2] ?? '') : '';
  const body = raw.slice(prefix.length);
  const chars: StyledChar[] = [];
  let last = 0;

  for (const tm of body.matchAll(INLINE_TOKEN_RE)) {
    const idx = tm.index ?? 0;
    for (const c of Array.from(body.slice(last, idx))) {
      chars.push({ c, b: false, i: false, code: false });
    }
    if (tm[2] !== undefined) {
      for (const c of Array.from(tm[2])) {
        chars.push({ c, b: true, i: false, code: false });
      }
    } else if (tm[3] !== undefined) {
      for (const c of Array.from(tm[3])) {
        chars.push({ c, b: false, i: false, code: true });
      }
    } else if (tm[4] !== undefined) {
      for (const c of Array.from(tm[4])) {
        chars.push({ c, b: false, i: true, code: false });
      }
    }
    last = idx + tm[0].length;
  }
  for (const c of Array.from(body.slice(last))) {
    chars.push({ c, b: false, i: false, code: false });
  }
  return { prefix, chars, raw, dirty: false };
}

export function parseMarkdown(text: string): ProjectedLine[] {
  return text.split('\n').map(parseLine);
}

export function projectLines(model: ProjectedLine[]): string[] {
  return model.map((l) => l.chars.map((x) => x.c).join(''));
}

function sameStyle(a: StyledChar, b: StyledChar): boolean {
  return a.b === b.b && a.i === b.i && a.code === b.code;
}

export function serializeLine(l: ProjectedLine): string {
  if (!l.dirty) return l.raw;
  if (isHorizontalRulePrefix(l.prefix)) {
    if (l.chars.length === 0) return l.prefix;
    // If user typed characters onto an HR line, drop the HR prefix and serialize the characters
  }
  let out = isHorizontalRulePrefix(l.prefix) ? '' : l.prefix;
  let idx = 0;
  const cs = l.chars;
  while (idx < cs.length) {
    let j = idx;
    while (j < cs.length && sameStyle(cs[j], cs[idx])) j++;
    const s = cs
      .slice(idx, j)
      .map((x) => x.c)
      .join('');
    if (cs[idx].code) {
      out += `\`${s}\``;
    } else if (cs[idx].b) {
      const m = /^(\s*)(.*?)(\s*)$/s.exec(s);
      out += m && m[2] ? `${m[1]}**${m[2]}**${m[3]}` : s;
    } else if (cs[idx].i) {
      const m = /^(\s*)(.*?)(\s*)$/s.exec(s);
      out += m && m[2] ? `${m[1]}*${m[2]}*${m[3]}` : s;
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

/**
 * Extracts raw/rendered markdown for a given yanked range.
 * If regtype is 'V' (linewise), returns whole lines including markdown prefixes and inline formatting.
 * If regtype is 'v' (characterwise), extracts inline markdown formatted segment.
 */
export function extractMarkdownRange(
  model: ProjectedLine[],
  startRow: number,
  startColChar: number,
  endRow: number,
  endColChar: number,
  regtype: string
): string[] {
  if (model.length === 0) return [];
  const sRow = Math.max(0, Math.min(startRow, model.length - 1));
  const eRow = Math.max(sRow, Math.min(endRow, model.length - 1));

  if (regtype === 'V') {
    const lines: string[] = [];
    for (let r = sRow; r <= eRow; r++) {
      lines.push(serializeLine(model[r]));
    }
    return lines;
  }

  // Characterwise yank
  if (sRow === eRow) {
    const line = model[sRow];
    const chars = line.chars.slice(startColChar, endColChar + 1);
    const subLine: ProjectedLine = {
      prefix: '',
      chars,
      raw: '',
      dirty: true,
    };
    return [serializeLine(subLine)];
  }

  const lines: string[] = [];
  for (let r = sRow; r <= eRow; r++) {
    const line = model[r];
    if (r === sRow) {
      const chars = line.chars.slice(startColChar);
      lines.push(serializeLine({ prefix: '', chars, raw: '', dirty: true }));
    } else if (r === eRow) {
      const chars = line.chars.slice(0, endColChar + 1);
      lines.push(serializeLine({ prefix: line.prefix, chars, raw: '', dirty: true }));
    } else {
      lines.push(serializeLine(line));
    }
  }
  return lines;
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
): Pick<StyledChar, 'b' | 'i' | 'code'> | null {
  for (let li = sl; li <= el; li++) {
    const from = li === sl ? sc : 0;
    const to = li === el ? ec : model[li].chars.length;
    if (from < to) {
      const ch = model[li].chars[from];
      return { b: ch.b, i: ch.i, code: ch.code };
    }
  }
  return null;
}

function nextListPrefix(prefix: string): string {
  if (/^\s*[-*+] \[[ xX]\] $/.test(prefix)) {
    return prefix.replace(/\[[xX]\]/, '[ ]');
  }
  const numMatch = /^(\s*)(\d+)(\. )$/.exec(prefix);
  if (numMatch) {
    return `${numMatch[1]}${Number(numMatch[2]) + 1}${numMatch[3]}`;
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

  let inherited = { b: false, i: false, code: false };
  const firstDeleted = locateFirstDeletedStyle(model, sl, sc, el, ec);
  if (firstDeleted !== null) {
    inherited = firstDeleted;
  } else if (left.length) {
    const prev = left[left.length - 1];
    inherited = { b: prev.b, i: prev.i, code: prev.code };
  }

  const segs: StyledChar[][] = [[]];
  for (const c of ins) {
    if (c === '\n') segs.push([]);
    else segs[segs.length - 1].push({ c, ...inherited });
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
      result.push(
        mk(contPrefix || (left.length ? '' : model[el].prefix), [...segs[k], ...right])
      );
    }
  }
  return [...model.slice(0, sl), ...result, ...model.slice(el + 1)];
}

export interface ReparseOutcome {
  model: ProjectedLine[];
  bufferChanged: boolean;
  newCursorCol: number;
}

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
        (ch, idx) =>
          !nextModel[i].chars[idx] ||
          !sameStyle(ch, nextModel[i].chars[idx])
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
