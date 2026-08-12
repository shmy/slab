// 表单校验 schema（与后端 value object 规则对齐）
import { z } from 'zod';

/** 密码：与后端 `identity_contract::Password` 一致——trim 后 4–64 位 */
export const passwordSchema = z
  .string()
  .trim()
  .min(4, '密码至少 4 位')
  .max(64, '密码最多 64 位');
