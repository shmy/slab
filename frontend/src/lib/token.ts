// 令牌与用户缓存的本地存储（纯模块，无 UI / store 依赖；api 层与 auth store 共用）。

// 供 auth store 监听跨标签页变更（storage 事件）
export const TOKENS_KEY = 'auth.tokens';
export const USER_KEY = 'auth.user';

/** storage 事件里判断是否与本会话相关（跨标签页同步用） */
export function isAuthStorageKey(key: string | null): boolean {
  return key === TOKENS_KEY || key === USER_KEY;
}

export interface StoredTokens {
  accessToken: string;
  refreshToken: string;
}

export interface AuthUser {
  id: string;
  name: string;
  phone: string;
  privileged: boolean;
}

/** profile 响应 → 本地用户模型（登录/加载/刷新三处共用） */
export function toAuthUser(profile: {
  id: string;
  name: string;
  phone: string;
  privileged: boolean;
}): AuthUser {
  return {
    id: profile.id,
    name: profile.name,
    phone: profile.phone,
    privileged: profile.privileged,
  };
}

function read<T>(key: string): T | null {
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return null;
    return JSON.parse(raw) as T;
  } catch {
    return null;
  }
}

function write(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // 隐身模式 / 隐私浏览下存储受限，忽略（会话内状态仍可用）
  }
}

export function loadTokens(): StoredTokens | null {
  return read<StoredTokens>(TOKENS_KEY);
}

export function saveTokens(tokens: StoredTokens) {
  write(TOKENS_KEY, tokens);
}

export function loadUser(): AuthUser | null {
  return read<AuthUser>(USER_KEY);
}

export function saveUser(user: AuthUser) {
  write(USER_KEY, user);
}

export function clearAuth() {
  try {
    localStorage.removeItem(TOKENS_KEY);
    localStorage.removeItem(USER_KEY);
  } catch {
    // 同上
  }
}

/** 登出时全量清空本地存储（含主题/字号/侧边栏等偏好） */
export function clearAllLocalStorage() {
  try {
    localStorage.clear();
  } catch {
    // 隐私模式下存储受限，忽略
  }
}
