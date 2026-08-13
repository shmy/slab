/* biome-ignore-all lint/a11y/useSemanticElements: 虚拟滚动行必须绝对定位，table 元素会因 display 转换（table-row→block）导致列错位，故用 div+role 保持语义 */
/* biome-ignore-all lint/a11y/useFocusableInteractive: 虚拟表格的 row/columnheader 仅作屏幕阅读器语义，不参与键盘导航 */
/* biome-ignore-all lint/suspicious/noExplicitAny: v9 泛型 feature API 无法在通用组件中表达，any 为刻意取舍（渲染按 TableColumnApi 契约，调用方获得完整类型推断） */
// React Compiler 豁免：useTable 的 render-phase store 与编译器自动 memo 化不兼容（详见 docs/architecture.md §5.8）
'use no memo';
import {
  type ColumnDef,
  type ReactTable,
  type RowData,
  type SortingState,
  type TableFeatures,
  type TableOptions,
  useTable,
} from '@tanstack/react-table';
import { useVirtualizer } from '@tanstack/react-virtual';
import {
  type CSSProperties,
  type ReactNode,
  useEffect,
  useRef,
  useState,
} from 'react';
import { cn } from '@/lib/utils';

export interface VirtualTableProps<TData extends RowData> {
  features: TableFeatures;
  columns: readonly ColumnDef<any, TData, any>[];
  data: TData[];
  /** 传给 useTable 的初始状态（如 columnPinning） */
  initialState?: TableOptions<any, TData>['initialState'];
  /** 排序点击循环是否可移除（默认 false：asc ↔ desc 循环） */
  enableSortingRemoval?: boolean;
  /** 受控排序状态（服务端排序：由 URL order 驱动；传则表头点击走 onSortingChange 而非内部状态） */
  sorting?: SortingState;
  /** 排序状态变化回调（服务端排序时接到 URL） */
  onSortingChange?: TableOptions<any, TData>['onSortingChange'];
  /** 吸收剩余空间撑满容器的列 id（flexGrow），如 'email' */
  growColumnId?: string;
  /** 稳定行 ID（行选择等依赖），如 (row) => String(row.id) */
  getRowId?: (row: TData) => string;
  /** 无限滚动：接近底部时回调（如追加数据），不传则禁用 */
  onLoadMore?: () => void;
  loadingMore?: boolean;
  /** 表格容器高度：默认 flex-1 min-h-0 充满父容器剩余空间；
   *  传 Tailwind 高度类（如 'h-[60vh]'）则使用固定高度 */
  height?: string;
  /** 表格上方的工具栏，可通过 table 操作全局过滤等 */
  toolbar?: (table: ReactTable<any, TData, any>) => ReactNode;
}

interface TableRowApi {
  /** 行是否被选中（调用方注册 rowSelectionFeature 后存在） */
  getIsSelected?: () => boolean;
}

// 泛型场景下列的 feature API 不可见，声明组件实际使用的契约
// （调用方需注册 rowSortingFeature / columnSizingFeature / columnPinningFeature）
interface TableColumnApi {
  id: string;
  columnDef: { meta?: { align?: 'left' | 'center' | 'right' } };
  getCanSort(): boolean;
  getIsSorted(): 'asc' | 'desc' | false;
  getToggleSortingHandler(): ((event: unknown) => void) | undefined;
  getIsPinned(): 'start' | 'end' | false;
  getStart(position: 'start' | 'center' | 'end'): number;
  getAfter(position: 'start' | 'center' | 'end'): number;
  getSize(): number;
}

// 固定列 = feature 算偏移 + renderer 自己贴 sticky CSS（技能：renderer-owned sticky）
// 背景不用 inline style：否则 group-hover 类压不过 inline，固定列 hover 会失效；
// 背景统一放 className（斑马纹 + group-hover），滚动时依旧不透明
function pinnedStyle(column: TableColumnApi): CSSProperties {
  return {
    position: column.getIsPinned() ? 'sticky' : 'relative',
    insetInlineStart:
      column.getIsPinned() === 'start'
        ? `${column.getStart('start')}px`
        : undefined,
    insetInlineEnd:
      column.getIsPinned() === 'end'
        ? `${column.getAfter('end')}px`
        : undefined,
    width: `${column.getSize()}px`,
    // flex 子项禁止收缩：列宽始终等于 getSize()，与表头严格对齐
    flexShrink: 0,
    zIndex: column.getIsPinned() ? 2 : 1,
  };
}

// 滚动阴影：start 固定列右侧、end 固定列左侧，滚到边缘时消失
function shadowStyle(
  column: TableColumnApi,
  showStart: boolean,
  showEnd: boolean,
): CSSProperties | undefined {
  if (column.getIsPinned() === 'start' && showStart) {
    return { boxShadow: '4px 0 8px -4px rgb(0 0 0 / 0.15)' };
  }
  if (column.getIsPinned() === 'end' && showEnd) {
    return { boxShadow: '-4px 0 8px -4px rgb(0 0 0 / 0.15)' };
  }
  return undefined;
}

// 单元格样式：固定定位 + 阴影 + 弹性列（表头/行共用）
function cellStyle(
  col: TableColumnApi,
  showStartShadow: boolean,
  showEndShadow: boolean,
  grow: boolean,
): CSSProperties {
  return {
    ...pinnedStyle(col),
    ...shadowStyle(col, showStartShadow, showEndShadow),
    flexGrow: grow ? 1 : undefined,
  };
}

/**
 * 虚拟滚动表格：固定列（start/end + 滚动阴影）、排序表头、
 * 斑马纹 + 行 hover、无限滚动。宽度撑满容器（推荐配合 growColumnId）。
 */
export function VirtualTable<TData extends RowData>({
  features,
  columns,
  data,
  initialState,
  enableSortingRemoval = false,
  sorting,
  onSortingChange,
  growColumnId,
  getRowId,
  onLoadMore,
  loadingMore,
  height,
  toolbar,
}: VirtualTableProps<TData>) {
  const [showStartShadow, setShowStartShadow] = useState(false);
  const [showEndShadow, setShowEndShadow] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const table = useTable<any, TData, any>({
    features,
    columns,
    data,
    enableSortingRemoval,
    initialState,
    getRowId,
    // 服务端排序：受控 sorting（URL 驱动）；不传则表格内部状态（客户端排序）
    ...(sorting !== undefined ? { state: { sorting }, onSortingChange } : {}),
  });

  const rows = table.getRowModel().rows;

  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 40,
    getItemKey: (index) => rows[index].id as string | number,
    overscan: 8,
  });

  // 用 ref 保存最新闭包，滚动监听只绑定一次
  const onLoadMoreRef = useRef(onLoadMore);
  onLoadMoreRef.current = onLoadMore;
  // loadingMore 也要最新值：请求进行中忽略后续触发（防竞态/重复请求）
  const loadingMoreRef = useRef(loadingMore);
  loadingMoreRef.current = loadingMore;

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const onScroll = () => {
      // 门控：没有回调，或上一批还在请求中 → 忽略
      if (
        onLoadMoreRef.current &&
        !loadingMoreRef.current &&
        el.scrollTop + el.clientHeight >= el.scrollHeight - 60
      ) {
        onLoadMoreRef.current();
      }
      // 阴影随滚动位置更新（只在边界状态变化时 setState）
      const atStart = el.scrollLeft <= 0;
      const atEnd = el.scrollLeft + el.clientWidth >= el.scrollWidth - 1;
      setShowStartShadow((prev) => (prev === !atStart ? prev : !atStart));
      setShowEndShadow((prev) => (prev === !atEnd ? prev : !atEnd));
    };
    el.addEventListener('scroll', onScroll, { passive: true });
    // 初始校正一次（未滚动时窄屏的 end 阴影应为 true）
    onScroll();
    return () => el.removeEventListener('scroll', onScroll);
  }, []);

  return (
    // 根容器参与父级 flex 链条：页面根 → 组件根 → 表格容器逐层 flex-1
    <div className="flex min-h-0 flex-1 flex-col">
      {toolbar?.(table)}
      <div
        ref={scrollRef}
        className={cn(
          'mt-4 overflow-auto rounded-2xl bg-surface shadow-md vt-card',
          height ?? 'flex-1 min-h-0',
        )}
      >
        <div
          role="table"
          // 宽度撑满容器：配合 growColumnId 列 flexGrow 吸收剩余空间。
          // end 固定列安全：宽屏时内容被撑到视口右缘，前一列右边界 = 视口-末列宽 = sticky 位置；
          // 窄屏时无剩余空间 flexGrow 不生效，列宽=模型宽，滚动时同样精确贴齐
          style={{ width: '100%' }}
        >
          {/* 表头（sticky 垂直固定，固定列 sticky 水平固定）
              背景在单元格上：行盒宽度=视口宽，横向滚动时行级背景覆盖不到右侧内容 */}
          <div
            role="row"
            className="sticky top-0 z-10 flex border-b border-line"
          >
            {table.getHeaderGroups()[0].headers.map((header) => {
              // 泛型下 feature API 不可见，按 TableColumnApi 契约使用
              const col = header.column as unknown as TableColumnApi;
              const align = col.columnDef.meta?.align;
              return (
                <div
                  key={header.id}
                  role="columnheader"
                  style={{
                    ...cellStyle(
                      col,
                      showStartShadow,
                      showEndShadow,
                      col.id === growColumnId,
                    ),
                    // 单元格统一 flex 垂直居中；水平方向按 meta.align（默认 left）
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent:
                      align === 'center'
                        ? 'center'
                        : align === 'right'
                          ? 'flex-end'
                          : undefined,
                  }}
                  className="border-r border-line bg-stripe px-3 py-2.5 text-left text-sm font-semibold text-ink-soft"
                >
                  {col.getCanSort() ? (
                    <button
                      type="button"
                      onClick={col.getToggleSortingHandler()}
                      // 始终 flex：justifyContent 才能生效（不可排序列点击无动作，由 handler 为空保证）
                      className="flex w-full items-center gap-1 cursor-pointer select-none"
                      style={{
                        // 表头按钮 flex 对齐：居中列按钮内容居中，默认两端分布（标题 + 排序箭头）
                        justifyContent:
                          align === 'center'
                            ? 'center'
                            : align === 'right'
                              ? 'flex-end'
                              : 'space-between',
                      }}
                    >
                      <table.FlexRender header={header} />
                      {col.getIsSorted() === 'asc'
                        ? ' ↑'
                        : col.getIsSorted() === 'desc'
                          ? ' ↓'
                          : ''}
                    </button>
                  ) : (
                    // 不可排序列（选择列/操作列）：直接渲染，避免 button 包裹 checkbox 等交互元素
                    <table.FlexRender header={header} />
                  )}
                </div>
              );
            })}
          </div>
          {/* 虚拟行（absolute 定位 + translateY，flex 行内单元格等宽） */}
          <div
            role="rowgroup"
            style={{
              height: virtualizer.getTotalSize(),
              position: 'relative',
            }}
          >
            {virtualizer.getVirtualItems().map((item) => {
              const row = rows[item.index];
              const selected = (
                row as unknown as TableRowApi
              ).getIsSelected?.();
              return (
                <div
                  key={row.id}
                  role="row"
                  data-index={item.index}
                  ref={virtualizer.measureElement}
                  style={{
                    position: 'absolute',
                    transform: `translateY(${item.start}px)`,
                    width: '100%',
                    display: 'flex',
                  }}
                  className="group"
                >
                  {row.getAllCells().map((cell) => {
                    const col = cell.column as unknown as TableColumnApi;
                    const align = col.columnDef.meta?.align;
                    return (
                      <div
                        key={cell.id}
                        role="cell"
                        style={{
                          ...cellStyle(
                            col,
                            showStartShadow,
                            showEndShadow,
                            col.id === growColumnId,
                          ),
                          // 单元格统一 flex 垂直居中；水平方向按 meta.align（默认 left）
                          display: 'flex',
                          alignItems: 'center',
                          justifyContent:
                            align === 'center'
                              ? 'center'
                              : align === 'right'
                                ? 'flex-end'
                                : undefined,
                        }}
                        className={cn(
                          selected
                            ? 'bg-accent-soft/10'
                            : item.index % 2
                              ? 'bg-stripe'
                              : 'bg-surface',
                          'group-hover:bg-stripe-hover border-b border-r border-line px-2 py-2 text-sm',
                        )}
                      >
                        <table.FlexRender cell={cell} />
                      </div>
                    );
                  })}
                </div>
              );
            })}
          </div>
        </div>
        {loadingMore && (
          <div className="py-3 text-center text-xs text-ink-soft">
            加载中...
          </div>
        )}
      </div>
    </div>
  );
}
