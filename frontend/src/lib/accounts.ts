// 账号（用户）管理 CRUD（契约见 openapi.json account tag；wire 顶层平铺键）

import { authRequest } from './api';
import type { components } from './api-schema';

export type AccountItem = components['schemas']['SearchAccountItem'];
export type AccountDetail = components['schemas']['GetAccountResponse'];
type CreateAccountRequest = components['schemas']['CreateAccountRequest'];
type UpdateAccountRequest = components['schemas']['UpdateAccountRequest'];

export interface AccountPage {
  items: AccountItem[];
  nextCursor: string | null;
}

/** 列表：q 模糊搜索（姓名/手机号）+ cursor 分页 */
export function apiSearchAccounts(options: {
  q?: string;
  limit?: number;
  nextCursor?: string | null;
}): Promise<AccountPage> {
  return authRequest<
    components['schemas']['JsonResponse_CursorPagingResult_SearchAccountItem']
  >({
    method: 'GET',
    url: '/accounts',
    params: {
      limit: options.limit ?? 20,
      cursor: options.nextCursor ?? undefined,
      q: options.q,
    },
  }).then((res) => ({ items: res.items, nextCursor: res.next_cursor ?? null }));
}

export function apiGetAccount(id: string): Promise<AccountDetail> {
  return authRequest<AccountDetail>({ method: 'GET', url: `/accounts/${id}` });
}

export function apiCreateAccount(
  body: CreateAccountRequest,
): Promise<components['schemas']['CreateAccountResponse']> {
  return authRequest({ method: 'POST', url: '/accounts', data: body });
}

export function apiUpdateAccount(
  id: string,
  body: UpdateAccountRequest,
): Promise<components['schemas']['UpdateAccountResponse']> {
  return authRequest({ method: 'PATCH', url: `/accounts/${id}`, data: body });
}

export function apiDeleteAccount(
  id: string,
): Promise<components['schemas']['DeleteAccountResponse']> {
  return authRequest({ method: 'DELETE', url: `/accounts/${id}` });
}
