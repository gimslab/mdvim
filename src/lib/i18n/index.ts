export type Locale = 'ko' | 'en';

export interface Messages {
  renderedView: string;
  codeView: string;
  newTab: string;
  openFile: string;
  saveFile: string;
  unsaved: string;
  saved: string;
  waitingKey: string;
  composing: string;
  normalMode: string;
  insertMode: string;
  visualMode: string;
  commandMode: string;
  lineCol: (ln: number, col: number) => string;
}

export const dictionaries: Record<Locale, Messages> = {
  ko: {
    renderedView: '렌더링 뷰',
    codeView: '코드 뷰',
    newTab: '+ 새 탭',
    openFile: '열기',
    saveFile: '저장',
    unsaved: '수정됨',
    saved: '저장됨',
    waitingKey: '키 입력 대기 중',
    composing: '조합 중',
    normalMode: 'NORMAL',
    insertMode: 'INSERT',
    visualMode: 'VISUAL',
    commandMode: 'COMMAND',
    lineCol: (ln, col) => `줄 ${ln}, 열 ${col}`,
  },
  en: {
    renderedView: 'Rendered View',
    codeView: 'Code View',
    newTab: '+ New Tab',
    openFile: 'Open',
    saveFile: 'Save',
    unsaved: 'Modified',
    saved: 'Saved',
    waitingKey: 'Waiting key...',
    composing: 'Composing',
    normalMode: 'NORMAL',
    insertMode: 'INSERT',
    visualMode: 'VISUAL',
    commandMode: 'COMMAND',
    lineCol: (ln, col) => `Ln ${ln}, Col ${col}`,
  },
};

export function t(locale: Locale): Messages {
  return dictionaries[locale] ?? dictionaries.ko;
}
