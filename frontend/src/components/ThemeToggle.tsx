import { useSelector } from '@tanstack/react-store';
import { Check, Monitor, Moon, Sun } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { setTheme, type ThemeMode, themeStore } from '@/store/theme';

const options: { mode: ThemeMode; label: string; icon: typeof Sun }[] = [
  { mode: 'light', label: '浅色', icon: Sun },
  { mode: 'dark', label: '深色', icon: Moon },
  { mode: 'system', label: '跟随系统', icon: Monitor },
];

export function ThemeToggle() {
  const mode = useSelector(themeStore, (s) => s.mode);
  const current = options.find((o) => o.mode === mode) ?? options[2];

  return (
    <Tooltip>
      <DropdownMenu>
        <DropdownMenuTrigger
          render={
            <TooltipTrigger
              render={
                <Button variant="ghost" size="icon" aria-label="切换主题">
                  <current.icon className="h-4 w-4" />
                </Button>
              }
            />
          }
        />
        <DropdownMenuContent align="end" className="w-32">
          {options.map((option) => (
            <DropdownMenuItem
              key={option.mode}
              onClick={() => setTheme(option.mode)}
            >
              <option.icon className="h-4 w-4" />
              <span className="flex-1">{option.label}</span>
              {mode === option.mode && <Check />}
            </DropdownMenuItem>
          ))}
        </DropdownMenuContent>
      </DropdownMenu>
      <TooltipContent>切换主题</TooltipContent>
    </Tooltip>
  );
}
