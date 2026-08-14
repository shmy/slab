import { createStore } from '@tanstack/react-store';
import { apiGetCurrentProfile, apiLogin } from '../lib/api';
import {
  type AuthUser,
  clearAllLocalStorage,
  clearAuth,
  isAuthStorageKey,
  loadTokens,
  loadUser,
  saveTokens,
  saveUser,
  toAuthUser,
  USER_KEY,
} from '../lib/token';

interface AuthState {
  user: AuthUser | null;
}

// 模块加载时从 localStorage 恢复（用户缓存与令牌同写同清）
export const authStore = createStore<AuthState>({
  user: loadUser(),
});

/**
 * 页面加载时有令牌 → 主动拉最新用户信息（改名/权限即时生效；401 由 api 层自动刷新，
 * 刷新失败则强制登出回登录页）。无令牌时零开销返回。
 */
function hydrateUser() {
  const tokens = loadTokens();
  if (!tokens?.accessToken) return;
  void apiGetCurrentProfile()
    .then((profile) => {
      const user = toAuthUser(profile);
      saveUser(user);
      authStore.setState(() => ({ user }));
    })
    .catch(() => {
      // 401 已在 api 层处理（刷新→重试→失败强制登出）；其余错误（如断网）保持现状
    });
}
hydrateUser();

// 跨标签页同步：另一标签页登录/登出/刷新令牌后，本页同步状态（storage 事件仅跨标签页触发）
window.addEventListener('storage', (event) => {
  if (!isAuthStorageKey(event.key)) return;
  if (!loadTokens()) {
    // 令牌被清空（他页登出/失效）→ 本页登出
    authStore.setState(() => ({ user: null }));
  } else if (event.key === USER_KEY) {
    // 用户缓存被更新（他页登录/刷新）→ 同步 store
    authStore.setState(() => ({ user: loadUser() }));
  }
});

/**
 * 登录：POST /identity/login 拿令牌 → 存令牌 → GET /profile/current 自省（从 Bearer 取账号，
 * 不解码 JWT）→ 存用户。profile 拉取失败则登录失败（清令牌，不留半状态）。
 */
export async function login(phone: string, password: string): Promise<void> {
  const result = await apiLogin(phone, password);
  // 先存令牌：profile 自省请求需要 Bearer
  saveTokens({
    accessToken: result.access_token,
    refreshToken: result.refresh_token,
  });

  try {
    const profile = await apiGetCurrentProfile();
    const user = toAuthUser(profile);
    saveUser(user);
    authStore.setState(() => ({ user }));
  } catch (error) {
    // profile 拉取失败 → 登录失败：清令牌，不留半状态
    clearAuth();
    throw error;
  }
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
    if (typeof navigator.sendBeacon === 'function') {
      navigator.sendBeacon(url);
    }
  }
  clearAllLocalStorage();
  authStore.setState(() => ({ user: null }));
}
