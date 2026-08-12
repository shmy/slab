// 可复制文本：hover（移动端常显）显示复制按钮，点击复制到剪贴板，icon 切换 ✓ 反馈
import { Check, Copy } from 'lucide-react';
import { useState } from 'react';
import { cn } from '@/lib/utils';

export function CopyableText({
  value,
  className,
}: {
  value: string;
  className?: string;
}) {
  const [copied, setCopied] = useState(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // 剪贴板不可用（非安全上下文等）时静默失败
    }
  }

  return (
    <button
      type="button"
      onClick={() => void copy()}
      title="点击复制"
      aria-label={`复制 ${value}`}
      className={cn('group inline-flex items-center gap-1.5', className)}
    >
      {value}
      {copied ? (
        <Check className="size-3.5 shrink-0 text-nord14" />
      ) : (
        // 移动端常显；md+ 桌面 hover 才显示（避免无 hover 设备不可见）
        <Copy className="size-3.5 shrink-0 text-ink-soft opacity-100 transition-opacity md:opacity-0 md:group-hover:opacity-100" />
      )}
    </button>
  );
}
