import { Maximize, Minimize } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip';

/** 顶栏全屏切换：跟随 fullscreenchange 事件同步图标，浏览器不支持时隐藏 */
export function FullscreenToggle() {
  const [isFullscreen, setIsFullscreen] = useState(
    () => document.fullscreenElement !== null,
  );

  useEffect(() => {
    function onChange() {
      setIsFullscreen(document.fullscreenElement !== null);
    }
    document.addEventListener('fullscreenchange', onChange);
    return () => document.removeEventListener('fullscreenchange', onChange);
  }, []);

  // document.fullscreenEnabled 变化（如 iframe 权限）时跟随
  const [supported, setSupported] = useState(() => document.fullscreenEnabled);
  useEffect(() => {
    setSupported(document.fullscreenEnabled);
  }, []);

  if (!supported) return null;

  function toggle() {
    if (document.fullscreenElement) {
      document.exitFullscreen().catch(() => {});
    } else {
      document.documentElement.requestFullscreen().catch(() => {});
    }
  }

  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <Button
            variant="ghost"
            size="icon"
            onClick={toggle}
            aria-label={isFullscreen ? '退出全屏' : '进入全屏'}
          >
            {isFullscreen ? (
              <Minimize className="h-4 w-4" />
            ) : (
              <Maximize className="h-4 w-4" />
            )}
          </Button>
        }
      />
      <TooltipContent>{isFullscreen ? '退出全屏' : '进入全屏'}</TooltipContent>
    </Tooltip>
  );
}
