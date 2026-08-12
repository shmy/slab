import { createStore } from '@tanstack/react-store';

export interface TabItem {
  to: string;
  label: string;
}

// 打开的页面标签（会话级，不持久化；刷新后回到当前页单个标签）
export const tabsStore = createStore<{ tabs: TabItem[] }>({ tabs: [] });

export function addTab(tab: TabItem) {
  tabsStore.setState((s) =>
    s.tabs.some((t) => t.to === tab.to) ? s : { tabs: [...s.tabs, tab] },
  );
}

export function removeTab(to: string) {
  tabsStore.setState((s) => ({ tabs: s.tabs.filter((t) => t.to !== to) }));
}
