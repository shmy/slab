'use client';

import { useSelector } from '@tanstack/react-store';
import {
  CheckCircle2,
  Info,
  Loader2,
  OctagonX,
  TriangleAlert,
} from 'lucide-react';
import type * as React from 'react';
import { Toaster as Sonner, type ToasterProps } from 'sonner';
import { themeStore } from '@/store/theme';

const Toaster = ({ ...props }: ToasterProps) => {
  // 与项目主题联动（themeStore 的 mode 即 light/dark/system，与 sonner 语义一致）
  const mode = useSelector(themeStore, (s) => s.mode);

  return (
    <Sonner
      theme={mode}
      className="toaster group"
      icons={{
        success: <CheckCircle2 className="size-4" />,
        info: <Info className="size-4" />,
        warning: <TriangleAlert className="size-4" />,
        error: <OctagonX className="size-4" />,
        loading: <Loader2 className="size-4 animate-spin" />,
      }}
      style={
        {
          '--normal-bg': 'var(--popover)',
          '--normal-text': 'var(--popover-foreground)',
          '--normal-border': 'var(--border)',
          '--border-radius': 'var(--radius)',
        } as React.CSSProperties
      }
      toastOptions={{
        classNames: {
          toast: 'cn-toast',
        },
      }}
      {...props}
    />
  );
};

export { Toaster };
