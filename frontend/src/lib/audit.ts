// 审计日志查询（契约见 openapi.json audit tag；按实体查询，wire 顶层平铺键）

import { authRequest } from './api';
import type { components } from './api-schema';

type AuditLogItem = components['schemas']['AuditLogItem'];

export type { AuditLogItem };
export interface AuditLogPage {
  items: AuditLogItem[];
  nextCursor: string | null;
}

/** 按实体查变更历史：entity（如 customer/account）+ entity_id 必填；cursor 分页 */
export function apiSearchAuditLogs(options: {
  entity: string;
  entityId: string;
  limit?: number;
  nextCursor?: string | null;
}): Promise<AuditLogPage> {
  return authRequest<
    components['schemas']['JsonResponse_CursorPagingResult_AuditLogItem']
  >({
    method: 'GET',
    url: '/audit-logs',
    params: {
      entity: options.entity,
      entity_id: options.entityId,
      limit: options.limit ?? 10,
      cursor: options.nextCursor ?? undefined,
    },
  }).then((res) => ({ items: res.items, nextCursor: res.next_cursor ?? null }));
}
