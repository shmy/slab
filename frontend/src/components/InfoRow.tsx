// 信息行：label（灰字）+ value（加粗），用于只读详情卡（profile / 客户详情）
import type { ReactNode } from 'react';
import { cn } from '@/lib/utils';

export function InfoRow({
  label,
  value,
  valueClassName,
}: {
  label: string;
  value: ReactNode;
  valueClassName?: string;
}) {
  return (
    <div className="flex items-center justify-between py-3.5">
      <dt className="text-sm text-ink-soft">{label}</dt>
      <dd className={cn('text-sm font-medium text-ink', valueClassName)}>
        {value ?? '—'}
      </dd>
    </div>
  );
}
