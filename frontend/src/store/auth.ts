import { createStore } from '@tanstack/react-store';

const STORAGE_KEY = 'auth.user';

export interface AuthUser {
  username: string;
}

function readUser(): AuthUser | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    return JSON.parse(raw) as AuthUser;
  } catch {
    return null;
  }
}

export const authStore = createStore<{ user: AuthUser | null }>({
  user: readUser(),
});

export function login(username: string) {
  const user: AuthUser = { username };
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(user));
  } catch {
    // 隐身模式 / 隐私浏览下存储受限，忽略（会话内状态仍可用）
  }
  authStore.setState((s) => ({ ...s, user }));
}

export function logout() {
  try {
    localStorage.removeItem(STORAGE_KEY);
  } catch {
    // 同上
  }
  authStore.setState((s) => ({ ...s, user: null }));
}
