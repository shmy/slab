import { type ClassValue, clsx } from 'clsx';
import { twMerge } from 'tailwind-merge';

/** 合并 Tailwind 类名：条件类 + 冲突解决（shadcn 标准工具） */
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

/** 手机号脱敏：138****8888（对齐后端 PhoneNumber::masked 语义） */
export function maskPhone(phone: string): string {
  if (!/^\d{11}$/.test(phone)) return phone;
  return `${phone.slice(0, 3)}****${phone.slice(7)}`;
}

/** ISO 时间 → 本地可读格式（如 2026/8/12 16:22） */
export function formatDateTime(iso: string): string {
  return new Date(iso).toLocaleString('zh-CN', {
    hour12: false,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
  });
}
