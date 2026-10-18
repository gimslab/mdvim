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

  it('immediately renders "# 안녕하세요" when pressing Enter to leave the line', () => {
    let model = parseMarkdown('');
    // User types "# 안녕하세요" and hits Enter in rendered mode
    model = applyProjected(model, ['# 안녕하세요', '']);
    const outcome = reparseCompletedLines(model, 0, 'i', 1, 0, 'i');
    expect(outcome.bufferChanged).toBe(true);
    expect(outcome.model[0].prefix).toBe('# ');
    expect(projectLines(outcome.model)).toEqual(['안녕하세요', '']);
    expect(serializeMarkdown(outcome.model)).toBe('# 안녕하세요\n');
  });

  it('continues list prefix automatically when pressing Enter on a list item', () => {
    let model = parseMarkdown('- 첫 항목');
    model = applyProjected(model, ['첫 항목', '']);
    expect(serializeMarkdown(model)).toBe('- 첫 항목\n- ');
  });
});
