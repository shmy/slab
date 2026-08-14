import assert from 'node:assert/strict';
import { test } from 'node:test';

type StoredValue = string | null;

class MemoryStorage {
  private readonly values = new Map<string, string>();

  getItem(key: string): StoredValue {
    return this.values.get(key) ?? null;
  }

  setItem(key: string, value: string): void {
    this.values.set(key, value);
  }

  removeItem(key: string): void {
    this.values.delete(key);
  }

  clear(): void {
    this.values.clear();
  }
}

const storage = new MemoryStorage();
const redirects: string[] = [];

Object.defineProperty(globalThis, 'localStorage', {
  configurable: true,
  value: storage,
});
Object.defineProperty(globalThis, 'window', {
  configurable: true,
  value: {
    location: {
      pathname: '/customers',
      search: '',
      assign: (url: string) => redirects.push(url),
    },
  },
});
Object.defineProperty(globalThis, 'navigator', {
  configurable: true,
  value: {},
});
Object.defineProperty(globalThis, 'BroadcastChannel', {
  configurable: true,
  value: undefined,
});

const apiModule = import('./api.ts');

function setTokens(accessToken = 'access-old', refreshToken = 'refresh-old') {
  storage.setItem('auth.tokens', JSON.stringify({ accessToken, refreshToken }));
}

function response(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

function installFetch(
  handler: (
    url: string,
    authorization: string | undefined,
  ) => Promise<Response>,
): void {
  globalThis.fetch = (async (input, init) => {
    const url = String(input);
    const headers = new Headers(init?.headers);
    return handler(url, headers.get('Authorization') ?? undefined);
  }) as typeof fetch;
}

function reset() {
  storage.clear();
  redirects.length = 0;
  setTokens();
}

test('并发 401 只刷新一次，并让每个原请求最多重试一次', async () => {
  const { authRequest } = await apiModule;
  reset();
  let resourceCalls = 0;
  let refreshCalls = 0;

  installFetch(async (url, authorization) => {
    if (url.endsWith('/identity/refresh')) {
      refreshCalls += 1;
      assert.equal(authorization, undefined);
      return response({
        access_token: 'access-new',
        refresh_token: 'refresh-new',
        token_type: 'Bearer',
        expires_in: 900,
      });
    }
    resourceCalls += 1;
    if (resourceCalls <= 2) return response({ error_code: 'expired' }, 401);
    assert.equal(authorization, 'Bearer access-new');
    return response({ ok: true });
  });

  const results = await Promise.all([
    authRequest<{ ok: boolean }>({ method: 'GET', url: '/resource' }),
    authRequest<{ ok: boolean }>({ method: 'GET', url: '/resource' }),
  ]);

  assert.deepEqual(results, [{ ok: true }, { ok: true }]);
  assert.equal(refreshCalls, 1);
  assert.equal(resourceCalls, 4);
  assert.deepEqual(redirects, []);
});

test('延迟到达的旧 token 401 不会再次消费 refresh token', async () => {
  const { authRequest } = await apiModule;
  reset();
  let resourceCalls = 0;
  let refreshCalls = 0;

  installFetch(async (url, authorization) => {
    if (url.endsWith('/identity/refresh')) {
      refreshCalls += 1;
      return response({
        access_token: 'access-new',
        refresh_token: 'refresh-new',
        token_type: 'Bearer',
        expires_in: 900,
      });
    }
    resourceCalls += 1;
    if (resourceCalls === 1) return response({ error_code: 'expired' }, 401);
    if (resourceCalls === 2) {
      await new Promise((resolve) => setTimeout(resolve, 30));
      return response({ error_code: 'expired' }, 401);
    }
    assert.equal(authorization, 'Bearer access-new');
    return response({ ok: true });
  });

  const results = await Promise.all([
    authRequest<{ ok: boolean }>({ method: 'GET', url: '/resource' }),
    authRequest<{ ok: boolean }>({ method: 'GET', url: '/resource' }),
  ]);

  assert.deepEqual(results, [{ ok: true }, { ok: true }]);
  assert.equal(refreshCalls, 1);
  assert.equal(resourceCalls, 4);
  assert.deepEqual(redirects, []);
});
