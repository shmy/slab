// 令牌与用户缓存的本地存储（纯模块，无 UI / store 依赖；api 层与 auth store 共用）。

const TOKENS_KEY = 'auth.tokens';
const USER_KEY = 'auth.user';

export interface StoredTokens {
  accessToken: string;
  refreshToken: string;
  /** access token 过期时刻（epoch ms） */
  expiresAt: number;
}

export interface AuthUser {
  id: string;
  name: string;
  phone: string;
  privileged: boolean;
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

/** 解码 JWT payload（不验签，仅取 sub / exp 等公开 claims） */
export function decodeJwtPayload(
  token: string,
): { sub: string; exp: number } | null {
  try {
    const payload = token.split('.')[1];
    if (!payload) return null;
    const normalized = payload.replace(/-/g, '+').replace(/_/g, '/');
    const data = JSON.parse(atob(normalized)) as {
      sub?: unknown;
      exp?: unknown;
    };
    if (typeof data.sub !== 'string' || typeof data.exp !== 'number')
      return null;
    return { sub: data.sub, exp: data.exp };
  } catch {
    return null;
  }
}
