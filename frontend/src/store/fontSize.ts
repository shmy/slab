import { createStore } from '@tanstack/react-store';

const STORAGE_KEY = 'font-size.mode';

export type FontSizeMode = 'small' | 'default' | 'large' | 'xlarge';

export const FONT_SIZE_OPTIONS: {
  mode: FontSizeMode;
  label: string;
  px: number;
}[] = [
  { mode: 'small', label: '小', px: 14 },
  { mode: 'default', label: '默认', px: 16 },
  { mode: 'large', label: '大', px: 18 },
  { mode: 'xlarge', label: '特大', px: 20 },
];

function readMode(): FontSizeMode {
  const raw = localStorage.getItem(STORAGE_KEY);
  return FONT_SIZE_OPTIONS.some((o) => o.mode === raw)
    ? (raw as FontSizeMode)
    : 'default';
}

/** 调整根字号：Tailwind 的 rem 单位（字体 + 间距）全站联动缩放 */
function apply(mode: FontSizeMode) {
  const px = FONT_SIZE_OPTIONS.find((o) => o.mode === mode)?.px ?? 16;
  document.documentElement.style.fontSize = `${px}px`;
}

// 模块加载即应用，避免字体大小闪烁
const initial = readMode();
apply(initial);

export const fontSizeStore = createStore<{ mode: FontSizeMode }>({
  mode: initial,
});

export function setFontSize(mode: FontSizeMode) {
  try {
    localStorage.setItem(STORAGE_KEY, mode);
  } catch {
    // 隐身模式 / 隐私浏览下存储受限，忽略（会话内仍生效）
  }
  apply(mode);
  fontSizeStore.setState((s) => ({ ...s, mode }));
}
