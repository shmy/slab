import { createFileRoute } from '@tanstack/react-router';
import { Activity, Inbox, UserCheck, Users } from 'lucide-react';
import { cn } from '@/lib/utils';

export const Route = createFileRoute('/_app/')({
  // keep-alive：切走时保留页面状态，切回不重建
  staticData: { keepAlive: true },
  component: Dashboard,
});

const stats = [
  {
    label: '总用户数',
    value: '1,234',
    icon: Users,
    trend: '+12.4% 较上月',
    iconClass: 'bg-nord8/15 text-nord10',
  },
  {
    label: '活跃用户',
    value: '890',
    icon: UserCheck,
    trend: '72.1% 占比',
    iconClass: 'bg-nord14/15 text-nord14',
  },
  {
    label: '今日登录',
    value: '56',
    icon: Activity,
    trend: '+8 较昨日',
    iconClass: 'bg-nord13/15 text-nord12',
  },
  {
    label: '待处理工单',
    value: '12',
    icon: Inbox,
    trend: '3 条加急',
    iconClass: 'bg-nord11/15 text-nord11',
  },
];

function Dashboard() {
  return (
    <div>
      <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-4">
        {stats.map((stat) => (
          <div
            key={stat.label}
            className="rounded-xl border border-line bg-surface p-4"
          >
            <div
              className={cn(
                'flex h-10 w-10 items-center justify-center rounded-lg',
                stat.iconClass,
              )}
            >
              <stat.icon className="h-5 w-5" />
            </div>
            <div className="mt-3 text-2xl font-semibold tabular-nums">
              {stat.value}
            </div>
            <div className="text-sm text-ink-soft">{stat.label}</div>
            <div className="mt-1 text-xs text-ink-soft">{stat.trend}</div>
          </div>
        ))}
      </div>
      <p className="mt-4 text-sm text-ink-soft">
        欢迎回来！这里是仪表盘占位内容。
      </p>
    </div>
  );
}
