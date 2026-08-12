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
