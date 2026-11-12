export interface ScrollMetrics {
  /** Current scrollTop of the scrollable editor pane */
  scrollTop: number;
  /** Total scrollable content height of the editor pane */
  scrollHeight: number;
  /** Visible inner height of the editor pane */
  clientHeight: number;
  /** Top edge of the editor pane in viewport coordinates (getBoundingClientRect().top) */
  paneTop: number;
  /** Top edge of the cursor/active-line element in viewport coordinates (getBoundingClientRect().top) */
  targetTop: number;
  /** Bottom edge of the cursor/active-line element in viewport coordinates (getBoundingClientRect().bottom) */
  targetBottom: number;
  /** Current 0-based cursor row index */
  cursorRow: number;
  /** Total number of lines in the document */
  totalLines: number;
  /** Desired top/bottom scroll margin in pixels (default: 28) */
  scrollMargin?: number;
}

export const DEFAULT_SCROLL_MARGIN_PX = 28;

/**
 * Computes the next `scrollTop` value for `.editor-pane` so that the cursor target
 * stays within the visible viewport with a comfortable top/bottom margin (`scrolloff`).
 *
 * Returns the unchanged `scrollTop` if the cursor is already comfortably visible.
 */
export function computeNextScrollTop(m: ScrollMetrics): number {
  if (m.clientHeight <= 0 || m.scrollHeight <= m.clientHeight) {
    return 0;
  }

  const maxScrollTop = Math.max(0, m.scrollHeight - m.clientHeight);
  const desiredMargin = m.scrollMargin ?? DEFAULT_SCROLL_MARGIN_PX;
  const targetHeight = Math.max(0, m.targetBottom - m.targetTop);

  // Prevent margin overlap if viewport height is very small relative to cursor line height
  const effectiveMargin = Math.min(
    desiredMargin,
    Math.max(0, Math.floor((m.clientHeight - targetHeight) / 2))
  );

  const visibleTop = m.paneTop + effectiveMargin;
  const visibleBottom = m.paneTop + m.clientHeight - effectiveMargin;

  let nextScrollTop = m.scrollTop;

  if (m.targetTop < visibleTop) {
    const delta = visibleTop - m.targetTop;
    nextScrollTop = m.scrollTop - delta;
    // Snap to 0 when on the first line and near the top so top padding is fully visible
    if (m.cursorRow === 0 && nextScrollTop <= desiredMargin) {
      nextScrollTop = 0;
    }
  } else if (m.targetBottom > visibleBottom) {
    const delta = m.targetBottom - visibleBottom;
    nextScrollTop = m.scrollTop + delta;
    // Snap to maxScrollTop when on the last line and near the bottom so bottom padding is fully visible
    if (m.cursorRow >= m.totalLines - 1 && maxScrollTop - nextScrollTop <= desiredMargin) {
      nextScrollTop = maxScrollTop;
    }
  } else {
    return m.scrollTop;
  }

  return Math.max(0, Math.min(maxScrollTop, Math.round(nextScrollTop)));
}

/**
 * Measures the active cursor element (or active line fallback) inside `editorPaneEl`
 * and updates `editorPaneEl.scrollTop` immediately if the cursor is outside the
 * comfortable viewport bounds.
 */
export function scrollEditorCursorIntoView(
  editorPaneEl: HTMLElement | undefined | null,
  cursorRow: number,
  totalLines: number,
  scrollMargin = DEFAULT_SCROLL_MARGIN_PX
): void {
  if (!editorPaneEl) return;

  const cursorEl =
    editorPaneEl.querySelector<HTMLElement>('.cursor-block, .cursor-bar') ??
    editorPaneEl.querySelector<HTMLElement>('.line.active-line');

  if (!cursorEl) return;

  const paneRect = editorPaneEl.getBoundingClientRect();
  const targetRect = cursorEl.getBoundingClientRect();

  const nextScrollTop = computeNextScrollTop({
    scrollTop: editorPaneEl.scrollTop,
    scrollHeight: editorPaneEl.scrollHeight,
    clientHeight: editorPaneEl.clientHeight,
    paneTop: paneRect.top,
    targetTop: targetRect.top,
    targetBottom: targetRect.bottom,
    cursorRow,
    totalLines,
    scrollMargin,
  });

  if (nextScrollTop !== editorPaneEl.scrollTop) {
    editorPaneEl.scrollTop = nextScrollTop;
  }
}
