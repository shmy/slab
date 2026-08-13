import { createStore } from '@tanstack/react-store';

const STORAGE_KEY = 'theme.mode';

export type ThemeMode = 'light' | 'dark' | 'system';

function readMode(): ThemeMode {
  const raw = localStorage.getItem(STORAGE_KEY);
  return raw === 'light' || raw === 'dark' || raw === 'system' ? raw : 'system';
}

const darkQuery = () => window.matchMedia('(prefers-color-scheme: dark)');

/** 将模式写到 <html data-theme>：system 也解析为具体主题。
 *  Tailwind 的 dark: variant 只认 data-theme 属性（不认媒体查询），
 *  若不解析，跟随系统时 CSS 变量已变深色而 dark:* 类全部失效，两者出现出入。 */
function apply(mode: ThemeMode) {
  const root = document.documentElement;
  root.dataset.theme =
    mode === 'system' ? (darkQuery().matches ? 'dark' : 'light') : mode;
}

// 模块加载即应用，避免主题闪烁
const initial = readMode();
apply(initial);

export const themeStore = createStore<{ mode: ThemeMode }>({ mode: initial });

// system 模式监听系统主题变化，实时切换（JS 负责解析，index.css 的 media 分支仅作无 JS 兜底）
let mq: MediaQueryList | null = null;

function onSystemChange() {
  apply('system');
}

function syncSystem(mode: ThemeMode) {
  mq?.removeEventListener('change', onSystemChange);
  mq = null;
  if (mode === 'system') {
    mq = darkQuery();
    mq.addEventListener('change', onSystemChange);
  }
}

syncSystem(initial);

export function setTheme(mode: ThemeMode) {
  try {
    localStorage.setItem(STORAGE_KEY, mode);
  } catch {
    // 隐身模式 / 隐私浏览下存储受限，忽略（会话内仍生效）
  }
  apply(mode);
  syncSystem(mode);
  themeStore.setState((s) => ({ ...s, mode }));
}
