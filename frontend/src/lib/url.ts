// Query 序列化：URLSearchParams 严格编码（encodeURIComponent 一切非 unreserved，
// 业界默认行为，与 GitHub/axios 等一致）。RSQL filters 的可读性由 URL 语法本身承载，
// 编码层面不做任何特殊处理——零兼容性争议。

/** 参数对象 → query string（严格编码；undefined/null 跳过；对象/数组 JSON） */
export function querySerialize(params: Record<string, unknown>): string {
  const usp = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined || value === null) continue;
    if (typeof value === 'string') {
      usp.append(key, value);
    } else if (typeof value === 'number' || typeof value === 'boolean') {
      usp.append(key, String(value));
    } else {
      usp.append(key, JSON.stringify(value));
    }
  }
  return usp.toString();
}
