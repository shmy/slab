// 客户主档 CRUD（契约见 openapi.json customer tag；后端扁平 JSON + cursor 分页）

import { authRequest } from './api';
import type { components } from './api-schema';

export type CustomerItem = components['schemas']['SearchCustomerItem'];
export type CustomerDetail = components['schemas']['GetCustomerResponse'];
type CreateCustomerRequest = components['schemas']['CreateCustomerRequest'];
type UpdateCustomerRequest = components['schemas']['UpdateCustomerRequest'];

export interface CustomerPage {
  items: CustomerItem[];
  nextCursor: string | null;
}

/** 列表：q 多字段模糊 + filters（PostgREST 风格：字段 → `op.value`，多参数天然 AND）+ cursor 分页（limit 1–100）。⚠️ wire 格式：serde flatten 在 serde_urlencoded 下展开为顶层键 `limit`/`next_cursor`（openapi 的 `paging` object 仅是 utoipa 呈现，实测嵌套 `paging[...]` 会被静默忽略） */
export function apiSearchCustomers(options: {
  q?: string;
  filters?: Record<string, string>;
  limit?: number;
  nextCursor?: string | null;
}): Promise<CustomerPage> {
  return authRequest<
    components['schemas']['JsonResponse_CursorPagingResult_SearchCustomerItem']
  >({
    method: 'GET',
    url: '/customers',
    params: {
      limit: options.limit ?? 20,
      next_cursor: options.nextCursor ?? undefined,
      q: options.q,
      ...options.filters,
    },
  }).then((res) => ({ items: res.items, nextCursor: res.next_cursor ?? null }));
}

export function apiGetCustomer(id: string): Promise<CustomerDetail> {
  return authRequest<CustomerDetail>({
    method: 'GET',
    url: `/customers/${id}`,
  });
}

export function apiCreateCustomer(
  body: CreateCustomerRequest,
): Promise<components['schemas']['CreateCustomerResponse']> {
  return authRequest({ method: 'POST', url: '/customers', data: body });
}

export function apiUpdateCustomer(
  id: string,
  body: UpdateCustomerRequest,
): Promise<components['schemas']['UpdateCustomerResponse']> {
  return authRequest({ method: 'PATCH', url: `/customers/${id}`, data: body });
}

export function apiDeleteCustomer(
  id: string,
): Promise<components['schemas']['DeleteCustomerResponse']> {
  return authRequest({ method: 'DELETE', url: `/customers/${id}` });
}
