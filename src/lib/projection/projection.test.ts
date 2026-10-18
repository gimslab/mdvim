import { describe, it, expect } from 'vitest';
import {
  parseMarkdown,
  projectLines,
  serializeMarkdown,
  applyProjected,
  reparseCompletedLines,
} from './index';

describe('mdvim projection engine', () => {
  it('preserves untouched markdown bytes identically', () => {
    const src = '# 안녕하세요\n\n- 항목 **굵게** 테스트\n1. 번호 목록';
    const model = parseMarkdown(src);
    expect(serializeMarkdown(model)).toBe(src);
    expect(projectLines(model)).toEqual([
      '안녕하세요',
      '',
      '항목 굵게 테스트',
      '번호 목록',
    ]);
  });

  it('immediately renders "# Hi" when typed step-by-step and pressing Enter', () => {
    const initial = [
      '# mdvim 에디터',
      '',
      '렌더링 뷰에서 **굵은 글씨**와 Vim 모션을 바로 사용할 수 있습니다.',
      '- 첫 번째 목록 항목',
      '- [ ] 할 일 체크박스 항목',
      '',
      '빈 줄에서 `# 제목` 또는 `**강조**`를 입력하고 Enter나 Esc를 눌러 보세요!',
    ].join('\n');

    let model = parseMarkdown(initial);
    let lines = projectLines(model);

    // User presses 'o' on the last line (line 6) -> opens line 7
    lines = [...lines, ''];
    model = applyProjected(model, lines);
    let out = reparseCompletedLines(model, 6, 'n', 7, 0, 'i');
    model = out.model;

    // User types '#', ' ', 'H', 'i' on line 7
    for (const text of ['#', '# ', '# H', '# Hi']) {
      lines = [...projectLines(model).slice(0, 7), text];
      model = applyProjected(model, lines);
      out = reparseCompletedLines(model, 7, 'i', 7, text.length, 'i');
      model = out.model;
    }

    // User presses Enter -> line 7 is '# Hi', line 8 is ''
    lines = [...projectLines(model).slice(0, 7), '# Hi', ''];
    model = applyProjected(model, lines);
    out = reparseCompletedLines(model, 7, 'i', 8, 0, 'i');
    model = out.model;

    expect(out.bufferChanged).toBe(true);
    expect(model[7].prefix).toBe('# ');
    expect(projectLines(model)[7]).toBe('Hi');
  });

  it('continues list prefix automatically when pressing Enter on a list item', () => {
    let model = parseMarkdown('- 첫 항목');
    model = applyProjected(model, ['첫 항목', '']);
    expect(serializeMarkdown(model)).toBe('- 첫 항목\n- ');
  });

  it('immediately renders "# Hi" when typed on middle empty line (line 5) and pressing Enter', () => {
    const initial = [
      '# mdvim 에디터',
      '',
      '렌더링 뷰에서 **굵은 글씨**와 Vim 모션을 바로 사용할 수 있습니다.',
      '- 첫 번째 목록 항목',
      '- [ ] 할 일 체크박스 항목',
      '',
      '빈 줄에서 `# 제목` 또는 `**강조**`를 입력하고 Enter나 Esc를 눌러 보세요!',
    ].join('\n');

    let model = parseMarkdown(initial);
    let lines = projectLines(model);

    // User moves to line 5 (the empty line before the last line) and types '# Hi'
    for (const text of ['#', '# ', '# H', '# Hi']) {
      lines = [...projectLines(model)];
      lines[5] = text;
      model = applyProjected(model, lines);
      const out = reparseCompletedLines(model, 5, 'i', 5, text.length, 'i');
      model = out.model;
    }

    // Now user presses Enter at the end of line 5 -> line 5 is '# Hi', a new empty line 6 is inserted before old line 6
    lines = [...projectLines(model)];
    lines.splice(6, 0, '');
    model = applyProjected(model, lines);
    const out = reparseCompletedLines(model, 5, 'i', 6, 0, 'i');
    model = out.model;

    expect(out.bufferChanged).toBe(true);
    expect(model[5].prefix).toBe('# ');
    expect(projectLines(model)[5]).toBe('Hi');
  });

  it('immediately renders horizontal rule "---" as a bar when pressing Enter', () => {
    let model = parseMarkdown('첫 줄\n');
    // User types '-', '--', '---' on line 1
    for (const t of ['-', '--', '---']) {
      model = applyProjected(model, ['첫 줄', t]);
      const out = reparseCompletedLines(model, 1, 'i', 1, t.length, 'i');
      model = out.model;
    }
    // User presses Enter on line 1 -> line 1 is '---', line 2 is ''
    model = applyProjected(model, ['첫 줄', '---', '']);
    const out = reparseCompletedLines(model, 1, 'i', 2, 0, 'i');
    model = out.model;

    expect(out.bufferChanged).toBe(true);
    expect(model[1].prefix).toBe('---');
    expect(projectLines(model)).toEqual(['첫 줄', '', '']);
    expect(serializeMarkdown(model)).toBe('첫 줄\n---\n');
  });

  it('follows CommonMark rule: 0-3 leading spaces render HR/heading, 4+ spaces or tabs do NOT', () => {
    const m0 = parseMarkdown('---');
    const m3 = parseMarkdown('   ---');
    const m4 = parseMarkdown('    ---');
    const mTab = parseMarkdown('\t---');
    const h3 = parseMarkdown('   # Title');
    const h4 = parseMarkdown('    # Title');

    expect(m0[0].prefix).toBe('---');
    expect(m3[0].prefix).toBe('   ---');
    expect(m4[0].prefix).toBe('');
    expect(projectLines(m4)).toEqual(['    ---']);
    expect(mTab[0].prefix).toBe('');
    expect(h3[0].prefix).toBe('   # ');
    expect(h4[0].prefix).toBe('');
    expect(projectLines(h4)).toEqual(['    # Title']);
  });
});
