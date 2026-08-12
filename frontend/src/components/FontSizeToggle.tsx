import { useSelector } from '@tanstack/react-store';
import { ALargeSmall, Check } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import {
  FONT_SIZE_OPTIONS,
  fontSizeStore,
  setFontSize,
} from '@/store/fontSize';

export function FontSizeToggle() {
  const mode = useSelector(fontSizeStore, (s) => s.mode);

  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button variant="ghost" size="icon" aria-label="调整字体大小">
            <ALargeSmall className="h-4 w-4" />
          </Button>
        }
      />
      <DropdownMenuContent align="end" className="w-36">
        {FONT_SIZE_OPTIONS.map((option) => (
          <DropdownMenuItem
            key={option.mode}
            onClick={() => setFontSize(option.mode)}
          >
            <span className="flex-1">{option.label}</span>
            <span className="text-xs tabular-nums text-muted-foreground">
              {option.px}px
            </span>
            {mode === option.mode && <Check className="h-4 w-4 text-primary" />}
          </DropdownMenuItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
