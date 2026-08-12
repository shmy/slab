import { createStore } from '@tanstack/react-store';
import { apiGetAccount, apiLogin } from '../lib/api';
import {
  type AuthUser,
  clearAllLocalStorage,
  decodeJwtPayload,
  loadTokens,
  loadUser,
  saveTokens,
  saveUser,
} from '../lib/token';

interface AuthState {
  user: AuthUser | null;
}

// 模块加载时从 localStorage 恢复（用户缓存与令牌同写同清）
export const authStore = createStore<AuthState>({
  user: loadUser(),
});

/**
 * 登录：POST /identity/login 拿令牌 → 解码 JWT sub（账号 id）→ GET /accounts/{id} 拿用户信息。
 * 成功后才写本地缓存（令牌 + 用户同一次写入，避免半状态）。
 */
export async function login(phone: string, password: string): Promise<void> {
  const result = await apiLogin(phone, password);
  const claims = decodeJwtPayload(result.access_token);
  const id = claims?.sub ?? '';

  let user: AuthUser;
  try {
    const account = await apiGetAccount(id);
    user = {
      id: account.id,
      name: account.name,
      phone: account.phone,
      privileged: account.privileged,
    };
  } catch {
    // 拉取用户信息失败不阻塞登录：先用 sub + 手机号兜底（会话内头像/角色信息不完整，下次登录正常拉取）
    user = { id, name: phone, phone, privileged: false };
  }

  saveTokens({
    accessToken: result.access_token,
    refreshToken: result.refresh_token,
    expiresAt: Date.now() + result.expires_in * 1000,
  });
  saveUser(user);
  authStore.setState(() => ({ user }));
}

/**
 * 登出：sendBeacon 发 POST 吊销（fire-and-forget，不等待响应、失败不阻塞）。
 * sendBeacon 无法设置 Authorization 头（且实测无论有无 body 都发 POST）→ 令牌走 `?access_token=`
 * （后端中间件支持 header 优先、query 回退，方法不限）。
 * 随后直接清空整个 localStorage（含主题/字号/侧边栏偏好），store 置空。
 */
export function logout(): void {
  const accessToken = loadTokens()?.accessToken;
  if (accessToken) {
    // JWT 是 base64url 字符集，天然 URL 安全，无需 encodeURIComponent
    const url = `/api/v1/identity/logout?access_token=${accessToken}`;
    navigator.sendBeacon(url);
  }
  clearAllLocalStorage();
  authStore.setState(() => ({ user: null }));
}
