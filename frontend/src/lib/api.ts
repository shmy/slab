// 后端 HTTP 客户端（xior）：Bearer 附加 → 401 单飞刷新重试 → 失败强制登出。
// 后端响应为扁平 JSON（无 data 包装），错误为 RFC 9457 Problem Details。

import xior, { type XiorRequestConfig } from 'xior';
import type { components } from './api-schema';
import { clearAuth, loadTokens, saveTokens } from './token.ts';
import { querySerialize } from './url.ts';

// 契约类型来自 openapi.json（后端 utoipa 生成），重新生成见 scripts/fetch-openapi.mjs
type LoginResponse = components['schemas']['LoginResponse'];
type AccountResponse = components['schemas']['GetAccountResponse'];
type UpdatePasswordResponse = components['schemas']['UpdatePasswordResponse'];
type ResetPasswordResponse = components['schemas']['ResetPasswordResponse'];

/** 后端 Problem Details 或网络层错误（status 0 = 网络/未知错误） */
export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  readonly title?: string;
  readonly detail?: string;
  readonly traceId?: string;

  constructor(
    status: number,
    code: string,
    message: string,
    options?: { title?: string; detail?: string; traceId?: string },
  ) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
    this.code = code;
    this.title = options?.title;
    this.detail = options?.detail;
    this.traceId = options?.traceId;
  }
}

const client = xior.create({
  baseURL: '/api/v1',
  timeout: 15_000,
  headers: {
    'Content-Type': 'application/json',
    // 后端 Fluent locale 中间件：错误 detail 按语言渲染
    'Accept-Language': 'zh-CN',
  },
  // 严格 query 编码（URLSearchParams 默认）
  paramsSerializer: (params) => querySerialize(params),
});

// 响应统一解包为 data（后端扁平 JSON）
client.interceptors.response.use((response) => response.data);

/** 登录/刷新路径的 401 不做自动刷新（否则会循环） */
const AUTH_PATHS = ['/identity/login', '/identity/refresh'];

function toApiError(error: unknown): ApiError {
  if (error instanceof ApiError) return error;
  if (error instanceof TypeError) {
    return new ApiError(
      0,
      'network_error',
      '网络连接失败，请检查后端服务是否启动',
    );
  }
  if (error instanceof Error) {
    const err = error as { response?: { status?: number; data?: unknown } };
    const status = err.response?.status;
    if (typeof status === 'number') {
      const data = (err.response?.data ?? {}) as Record<string, unknown>;
      const title = typeof data.title === 'string' ? data.title : undefined;
      const detail = typeof data.detail === 'string' ? data.detail : undefined;
      const code = typeof data.error_code === 'string' ? data.error_code : '';
      const traceId =
        typeof data.trace_id === 'string' ? data.trace_id : undefined;
      return new ApiError(
        status,
        code,
        detail ?? title ?? `请求失败（HTTP ${status}）`,
        {
          title,
          detail,
          traceId,
        },
      );
    }
    return new ApiError(0, 'unknown_error', error.message);
  }
  return new ApiError(0, 'unknown_error', '未知错误');
}

const REFRESH_STATE_KEY = 'auth.refresh.state';
const REFRESH_LOCK_NAME = 'slab-auth-refresh';
const REFRESH_CHANNEL_NAME = 'slab-auth-refresh';
const REFRESH_WAIT_TIMEOUT_MS = 20_000;
// 给 BroadcastChannel 消息传播留出窗口，再进行确定性 owner 选举。
const REFRESH_CLAIM_WINDOW_MS = 500;
const TAB_ID = `${Date.now()}-${Math.random().toString(36).slice(2)}`;

type RefreshState = {
  status: 'refreshing' | 'success' | 'failure';
  accessToken: string;
  owner: string;
  timestamp: number;
};

type RefreshMessage = {
  type: 'intent' | 'state';
  accessToken: string;
  owner: string;
  timestamp: number;
  status?: RefreshState['status'];
};

let refreshPromise: Promise<boolean> | null = null;
let logoutRedirectStarted = false;
const refreshIntents = new Map<string, Set<string>>();
const refreshChannel =
  typeof BroadcastChannel === 'function'
    ? new BroadcastChannel(REFRESH_CHANNEL_NAME)
    : null;

refreshChannel?.addEventListener(
  'message',
  (event: MessageEvent<RefreshMessage>) => {
    const message = event.data;
    if (message?.type !== 'intent' || !message.accessToken) return;
    const owners = refreshIntents.get(message.accessToken) ?? new Set<string>();
    owners.add(message.owner);
    refreshIntents.set(message.accessToken, owners);
  },
);

function sleep(milliseconds: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

function readRefreshState<T>(key: string): T | null {
  try {
    const value = localStorage.getItem(key);
    return value ? (JSON.parse(value) as T) : null;
  } catch {
    return null;
  }
}

function writeRefreshState(key: string, value: unknown): boolean {
  try {
    localStorage.setItem(key, JSON.stringify(value));
    return true;
  } catch {
    return false;
  }
}

function publishRefreshState(
  status: RefreshState['status'],
  accessToken: string,
): void {
  const state = {
    status,
    accessToken,
    owner: TAB_ID,
    timestamp: Date.now(),
  } satisfies RefreshState;
  writeRefreshState(REFRESH_STATE_KEY, state);
  refreshChannel?.postMessage({
    type: 'state',
    ...state,
  } satisfies RefreshMessage);
}

function currentAccessTokenChanged(accessToken: string | undefined): boolean {
  return Boolean(accessToken && loadTokens()?.accessToken !== accessToken);
}

function isFreshRefreshState(
  state: RefreshState | null,
  accessToken: string | undefined,
): state is RefreshState {
  return Boolean(
    state &&
      state.accessToken === accessToken &&
      Date.now() - state.timestamp < REFRESH_WAIT_TIMEOUT_MS,
  );
}

function registerRefreshIntent(accessToken: string): void {
  const owners = refreshIntents.get(accessToken) ?? new Set<string>();
  owners.add(TAB_ID);
  refreshIntents.set(accessToken, owners);
  refreshChannel?.postMessage({
    type: 'intent',
    accessToken,
    owner: TAB_ID,
    timestamp: Date.now(),
  });
}

function electedRefreshOwner(accessToken: string): string {
  return [...(refreshIntents.get(accessToken) ?? new Set([TAB_ID]))].sort()[0];
}

async function performRefresh(accessToken: string): Promise<boolean> {
  const tokens = loadTokens();
  if (currentAccessTokenChanged(accessToken)) return true;
  if (!tokens?.refreshToken) {
    publishRefreshState('failure', accessToken);
    return false;
  }

  const refreshToken = tokens.refreshToken;
  try {
    // 这里必须绕过 authRequest，否则 refresh 失败会再次进入刷新流程。
    const result = await client.post<LoginResponse, LoginResponse>(
      '/identity/refresh',
      { refresh_token: refreshToken },
    );

    // 登出或另一标签页登录后，不允许旧 refresh 请求覆盖新会话。
    const latest = loadTokens();
    if (!latest || latest.refreshToken !== refreshToken) {
      return Boolean(latest?.accessToken);
    }

    saveTokens({
      accessToken: result.access_token,
      refreshToken: result.refresh_token,
    });
    publishRefreshState('success', accessToken);
    return true;
  } catch {
    publishRefreshState('failure', accessToken);
    return false;
  }
}

async function runRefreshUnderLock(
  accessToken: string | undefined,
  fallbackAccessToken: string,
): Promise<boolean> {
  if (currentAccessTokenChanged(accessToken)) return true;
  const state = readRefreshState<RefreshState>(REFRESH_STATE_KEY);
  if (isFreshRefreshState(state, accessToken) && state.status === 'failure') {
    return false;
  }
  publishRefreshState('refreshing', accessToken ?? fallbackAccessToken);
  return performRefresh(accessToken ?? fallbackAccessToken);
}

async function waitForRefreshResult(accessToken: string): Promise<boolean> {
  const deadline = Date.now() + REFRESH_WAIT_TIMEOUT_MS;
  while (Date.now() < deadline) {
    if (currentAccessTokenChanged(accessToken)) return true;
    const state = readRefreshState<RefreshState>(REFRESH_STATE_KEY);
    if (isFreshRefreshState(state, accessToken)) {
      if (state.status === 'failure') return false;
      if (state.status === 'success')
        return currentAccessTokenChanged(accessToken);
    }
    await sleep(50);
  }
  return currentAccessTokenChanged(accessToken);
}

/**
 * 跨标签页协调刷新：优先使用 Web Locks；不支持时使用 BroadcastChannel
 * 选举唯一刷新者，localStorage 只传递状态，不充当非原子的互斥锁。
 */
async function refreshAcrossTabs(
  accessToken: string | undefined,
): Promise<boolean> {
  const before = loadTokens();
  if (!before?.refreshToken) return false;
  if (currentAccessTokenChanged(accessToken)) return true;

  const run = () => runRefreshUnderLock(accessToken, before.accessToken);

  if (typeof navigator !== 'undefined' && navigator.locks) {
    return navigator.locks.request(REFRESH_LOCK_NAME, run);
  }

  const token = accessToken ?? before.accessToken;
  const state = readRefreshState<RefreshState>(REFRESH_STATE_KEY);
  if (isFreshRefreshState(state, token) && state.status === 'refreshing') {
    return waitForRefreshResult(token);
  }

  registerRefreshIntent(token);
  await sleep(REFRESH_CLAIM_WINDOW_MS);
  if (currentAccessTokenChanged(accessToken)) return true;

  const activeState = readRefreshState<RefreshState>(REFRESH_STATE_KEY);
  if (isFreshRefreshState(activeState, token)) {
    if (activeState.status === 'failure') return false;
    return waitForRefreshResult(token);
  }

  // 所有同时发起请求的标签页都能看到 intent，并得到相同的确定性 owner。
  if (electedRefreshOwner(token) !== TAB_ID) {
    return waitForRefreshResult(token);
  }

  publishRefreshState('refreshing', token);
  return performRefresh(token);
}

/** 单飞刷新：当前标签页 + 跨标签页均只触发一次 refresh。 */
function refreshTokensOnce(accessToken: string | undefined): Promise<boolean> {
  refreshPromise ??= refreshAcrossTabs(accessToken).finally(() => {
    refreshPromise = null;
  });
  return refreshPromise;
}

/** 刷新失败 / 重试仍 401 → 清理会话并回登录页（整页跳转，保证守卫重新执行） */
function forceLogout(expectedAccessToken?: string) {
  const current = loadTokens();
  // 另一请求/标签页已经建立新会话时，旧请求不能注销新会话。
  if (
    expectedAccessToken &&
    current?.accessToken &&
    current.accessToken !== expectedAccessToken
  ) {
    return;
  }

  clearAuth();
  if (window.location.pathname.startsWith('/login')) {
    // 已在登录页（如主动登出后 in-flight 请求 401）：只清状态，避免整页刷新丢表单
    return;
  }
  if (logoutRedirectStarted) return;
  logoutRedirectStarted = true;
  const redirect = encodeURIComponent(
    window.location.pathname + window.location.search,
  );
  window.location.assign(`/login?redirect=${redirect}`);
}

type RequestConfig = XiorRequestConfig & {
  _retried?: boolean;
  /** 本次请求实际发送的 access token，仅供 authRequest 内部使用。 */
  _accessToken?: string;
};

/** 统一请求：附 Bearer → 401 自动刷新重试一次 → 仍失败强制登出。 */
export async function authRequest<T>(config: RequestConfig): Promise<T> {
  const { _retried, _accessToken, ...requestConfig } = config;
  const tokens = loadTokens();
  const sentAccessToken = _accessToken ?? tokens?.accessToken;
  const headers = { ...(requestConfig.headers ?? {}) };
  if (tokens?.accessToken) {
    headers.Authorization = `Bearer ${tokens.accessToken}`;
  }
  try {
    return await client.request<T, T>({ ...requestConfig, headers });
  } catch (error) {
    const apiError = toApiError(error);
    const url = requestConfig.url ?? '';
    const isAuthPath = AUTH_PATHS.some((p) => url.includes(p));
    if (apiError.status !== 401 || isAuthPath) throw apiError;
    if (_retried) {
      forceLogout(sentAccessToken);
      throw apiError;
    }

    // 401 到达时可能已经有别的请求刷新完毕；此时禁止再次消费 refresh token。
    const latest = loadTokens();
    const latestAccessToken = latest?.accessToken;
    if (
      sentAccessToken &&
      latestAccessToken &&
      latestAccessToken !== sentAccessToken
    ) {
      return authRequest<T>({
        ...requestConfig,
        _retried: true,
        _accessToken: latestAccessToken,
      });
    }

    const ok = await refreshTokensOnce(sentAccessToken);
    const refreshed = loadTokens();
    if (
      sentAccessToken &&
      refreshed?.accessToken &&
      refreshed.accessToken !== sentAccessToken
    ) {
      return authRequest<T>({
        ...requestConfig,
        _retried: true,
        _accessToken: refreshed.accessToken,
      });
    }
    if (!ok || !refreshed?.accessToken) {
      forceLogout(sentAccessToken);
      throw apiError;
    }
    return authRequest<T>({
      ...requestConfig,
      _retried: true,
      _accessToken: refreshed.accessToken,
    });
  }
}

export function apiLogin(
  phone: string,
  password: string,
): Promise<LoginResponse> {
  return authRequest<LoginResponse>({
    method: 'POST',
    url: '/identity/login',
    data: { phone, password },
  });
}

export function apiGetCurrentProfile(): Promise<AccountResponse> {
  return authRequest<AccountResponse>({
    method: 'GET',
    url: '/profile/current',
  });
}

/** 改自己的密码：需旧密码（PATCH /identity/password，后端不吊销令牌） */
export function apiUpdateMyPassword(
  oldPassword: string,
  newPassword: string,
): Promise<UpdatePasswordResponse> {
  return authRequest<UpdatePasswordResponse>({
    method: 'PATCH',
    url: '/identity/password',
    data: { old_password: oldPassword, new_password: newPassword },
  });
}

/** 管理员重置他人密码：只需新密码（PATCH /accounts/password/{id}） */
export function apiResetAccountPassword(
  id: string,
  newPassword: string,
): Promise<ResetPasswordResponse> {
  return authRequest<ResetPasswordResponse>({
    method: 'PATCH',
    url: `/accounts/password/${id}`,
    data: { new_password: newPassword },
  });
}
