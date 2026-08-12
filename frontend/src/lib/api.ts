// 后端 HTTP 客户端（xior）：Bearer 附加 → 401 单飞刷新重试 → 失败强制登出。
// 后端响应为扁平 JSON（无 data 包装），错误为 RFC 9457 Problem Details。

import xior, { type XiorRequestConfig } from 'xior';
import type { components } from './api-schema';
import { clearAuth, loadTokens, saveTokens } from './token';

// 契约类型来自 openapi.json（后端 utoipa 生成），重新生成见 scripts/fetch-openapi.mjs
type LoginResponse = components['schemas']['LoginResponse'];
type AccountResponse = components['schemas']['GetAccountResponse'];

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

let refreshPromise: Promise<boolean> | null = null;

/** 单飞刷新：并发 401 只触发一次 refresh，其余请求等待同一结果 */
function refreshTokensOnce(): Promise<boolean> {
  refreshPromise ??= (async () => {
    const tokens = loadTokens();
    if (!tokens?.refreshToken) return false;
    try {
      const result = await client.post<LoginResponse, LoginResponse>(
        '/identity/refresh',
        {
          refresh_token: tokens.refreshToken,
        },
      );
      saveTokens({
        accessToken: result.access_token,
        refreshToken: result.refresh_token,
        expiresAt: Date.now() + result.expires_in * 1000,
      });
      return true;
    } catch {
      return false;
    }
  })().finally(() => {
    refreshPromise = null;
  });
  return refreshPromise;
}

/** 刷新失败 / 重试仍 401 → 清理会话并回登录页（整页跳转，保证守卫重新执行） */
function forceLogout() {
  clearAuth();
  const redirect = encodeURIComponent(
    window.location.pathname + window.location.search,
  );
  window.location.assign(`/login?redirect=${redirect}`);
}

type RequestConfig = XiorRequestConfig & { _retried?: boolean };

/** 统一请求：附 Bearer → 401 自动刷新重试一次 → 仍失败强制登出 */
async function authRequest<T>(config: RequestConfig): Promise<T> {
  const tokens = loadTokens();
  const headers = { ...(config.headers ?? {}) };
  if (tokens?.accessToken) {
    headers.Authorization = `Bearer ${tokens.accessToken}`;
  }
  try {
    return await client.request<T, T>({ ...config, headers });
  } catch (error) {
    const apiError = toApiError(error);
    const url = config.url ?? '';
    const isAuthPath = AUTH_PATHS.some((p) => url.includes(p));
    if (apiError.status !== 401 || isAuthPath) throw apiError;
    if (config._retried) {
      forceLogout();
      throw apiError;
    }
    const ok = await refreshTokensOnce();
    if (!ok) {
      forceLogout();
      throw apiError;
    }
    return authRequest<T>({ ...config, _retried: true });
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

export function apiGetAccount(id: string): Promise<AccountResponse> {
  return authRequest<AccountResponse>({
    method: 'GET',
    url: `/accounts/${id}`,
  });
}
