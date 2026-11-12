import { describe, it, expect } from 'vitest';
import {
  computeNextScrollTop,
  scrollEditorCursorIntoView,
  DEFAULT_SCROLL_MARGIN_PX,
} from './index';

describe('computeNextScrollTop', () => {
  const baseMetrics = {
    scrollTop: 200,
    scrollHeight: 2000,
    clientHeight: 500,
    paneTop: 40, // e.g. topbar height is 40px, so pane visible area is y: 40..540
    targetTop: 250,
    targetBottom: 275,
    cursorRow: 10,
    totalLines: 80,
    scrollMargin: DEFAULT_SCROLL_MARGIN_PX, // 28px -> comfortable zone is y: 68..512
  };

  it('returns 0 when document fits entirely within clientHeight', () => {
    expect(
      computeNextScrollTop({
        ...baseMetrics,
        scrollTop: 0,
        scrollHeight: 400,
        clientHeight: 500,
      })
    ).toBe(0);
  });

  it('does not change scrollTop when cursor is comfortably inside viewport', () => {
    expect(computeNextScrollTop(baseMetrics)).toBe(200);
  });

  it('scrolls down by minimal delta when cursor moves below bottom margin', () => {
    // visibleBottom is 40 + 500 - 28 = 512.
    // Suppose cursor bottom is at 532 (20px below visibleBottom)
    const next = computeNextScrollTop({
      ...baseMetrics,
      targetTop: 507,
      targetBottom: 532,
    });
    expect(next).toBe(220);
  });

  it('scrolls up by minimal delta when cursor moves above top margin', () => {
    // visibleTop is 40 + 28 = 68.
    // Suppose cursor top is at 50 (18px above visibleTop)
    const next = computeNextScrollTop({
      ...baseMetrics,
      targetTop: 50,
      targetBottom: 75,
    });
    expect(next).toBe(182);
  });

  it('snaps to 0 when cursor reaches the first line (row 0) near the top', () => {
    // scrollTop is 30, cursor moves to row 0 with targetTop = 55 (13px above visibleTop 68)
    // Raw nextScrollTop would be 30 - 13 = 17, which is <= 28 margin, so it snaps to 0
    const next = computeNextScrollTop({
      ...baseMetrics,
      scrollTop: 30,
      cursorRow: 0,
      targetTop: 55,
      targetBottom: 80,
    });
    expect(next).toBe(0);
  });

  it('snaps to maxScrollTop when cursor reaches the last line near the bottom', () => {
    // maxScrollTop = 2000 - 500 = 1500.
    // scrollTop is 1470, cursor moves to last row (79) with targetBottom = 522 (10px below 512)
    // Raw nextScrollTop would be 1480, within 28px of 1500, so it snaps to 1500
    const next = computeNextScrollTop({
      ...baseMetrics,
      scrollTop: 1470,
      cursorRow: 79,
      totalLines: 80,
      targetTop: 497,
      targetBottom: 522,
    });
    expect(next).toBe(1500);
  });

  it('handles large jumps (e.g. G or gg) and clamps within [0, maxScrollTop]', () => {
    // Jumping to bottom (G) from top (scrollTop = 0), target is far below at y = 1800
    const jumpBottom = computeNextScrollTop({
      ...baseMetrics,
      scrollTop: 0,
      cursorRow: 79,
      totalLines: 80,
      targetTop: 1775,
      targetBottom: 1800,
    });
    expect(jumpBottom).toBe(1288);

    // Jumping to top (gg) from bottom (scrollTop = 1500), target is far above at y = -1400
    const jumpTop = computeNextScrollTop({
      ...baseMetrics,
      scrollTop: 1500,
      cursorRow: 0,
      totalLines: 80,
      targetTop: -1430,
      targetBottom: -1405,
    });
    expect(jumpTop).toBe(0);
  });
});

describe('scrollEditorCursorIntoView', () => {
  it('queries .cursor-block/.cursor-bar and updates editorPaneEl.scrollTop when below viewport', () => {
    const mockCursorEl = {
      getBoundingClientRect: () => ({ top: 530, bottom: 555 }),
    } as unknown as HTMLElement;

    const mockPaneEl = {
      scrollTop: 100,
      scrollHeight: 1600,
      clientHeight: 500,
      getBoundingClientRect: () => ({ top: 40, bottom: 540 }),
      querySelector: (selector: string) =>
        selector.includes('.cursor-block') ? mockCursorEl : null,
    } as unknown as HTMLElement;

    scrollEditorCursorIntoView(mockPaneEl, 25, 100);
    // visibleBottom = 40 + 500 - 28 = 512. targetBottom = 555 -> delta = +43 -> 100 + 43 = 143
    expect(mockPaneEl.scrollTop).toBe(143);
  });

  it('falls back to .line.active-line when no cursor span is present (e.g. horizontal rule line)', () => {
    const mockLineEl = {
      getBoundingClientRect: () => ({ top: 48, bottom: 72 }),
    } as unknown as HTMLElement;

    const mockPaneEl = {
      scrollTop: 300,
      scrollHeight: 1600,
      clientHeight: 500,
      getBoundingClientRect: () => ({ top: 40, bottom: 540 }),
      querySelector: (selector: string) =>
        selector === '.line.active-line' ? mockLineEl : null,
    } as unknown as HTMLElement;

    scrollEditorCursorIntoView(mockPaneEl, 12, 100);
    // visibleTop = 40 + 28 = 68. targetTop = 48 -> delta = -20 -> 300 - 20 = 280
    expect(mockPaneEl.scrollTop).toBe(280);
  });
});

