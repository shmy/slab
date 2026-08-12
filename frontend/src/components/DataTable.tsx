// 业务表格：VirtualTable 的薄封装——固定 features（排序 + 固定列），
// 业务方只提供数据 + 列配置数组，无需接触 @tanstack/react-table 的 features/columnHelper。
// 能力：虚拟滚动、sticky 表头、滚动阴影、斑马纹、排序、无限滚动（onLoadMore）。
// React Compiler 豁免：useTable 的 render-phase store 与自动 memo 化不兼容（详见 docs/architecture.md §5.8）
'use no memo';
import {
  columnPinningFeature,
  columnSizingFeature,
  createColumnHelper,
  createSortedRowModel,
  type RowData,
  rowSortingFeature,
  tableFeatures,
} from '@tanstack/react-table';
import { type ReactNode, useMemo } from 'react';
import { VirtualTable } from '@/components/VirtualTable';

export interface DataColumn<T> {
  /** 列 key：数据字段名，或自定义渲染列的标识（如 'actions'） */
  key: string;
  header: string;
  /** 列宽（px），默认 120 */
  width?: number;
  align?: 'left' | 'center' | 'right';
  /** 吸收剩余空间撑满容器（一列即可） */
  grow?: boolean;
  /** 固定列（横向滚动时贴边） */
  pinned?: 'start' | 'end';
  /** 自定义渲染；缺省显示 row[key] 的字符串值 */
  render?: (row: T) => ReactNode;
}

interface DataTableProps<T extends RowData> {
  data: T[];
  columns: DataColumn<T>[];
  /** 稳定行 ID（虚拟滚动 key），如 (row) => row.id */
  getRowId: (row: T) => string;
  /** 无限滚动：接近底部时回调，不传则禁用 */
  onLoadMore?: () => void;
  loadingMore?: boolean;
}

// 只注册用到的 feature：排序 + 固定列（pinning 前置依赖 sizing）
const features = tableFeatures({
  columnSizingFeature,
  columnPinningFeature,
  rowSortingFeature,
  sortedRowModel: createSortedRowModel(),
  columnMeta: {} as { align?: 'left' | 'center' | 'right' },
});

/**
 * 业务表格：data + 列配置 → 虚拟滚动表格。
 * 用法示例见 `routes/_app/customers/index.tsx`。
 */
export function DataTable<T extends RowData>({
  data,
  columns: columnDefs,
  getRowId,
  onLoadMore,
  loadingMore,
}: DataTableProps<T>) {
  const columnHelper = useMemo(
    () => createColumnHelper<typeof features, T>(),
    [],
  );

  // 列配置数组 → tanstack 列定义（render 存在时覆盖默认值渲染）
  const columns = useMemo(
    () =>
      columnHelper.columns(
        columnDefs.map((col) =>
          columnHelper.accessor(
            // 动态 string key 在泛型组件中用函数式 accessor 表达
            (row: T) => (row as Record<string, unknown>)[col.key] as unknown,
            {
              id: col.key,
              header: col.header,
              size: col.width ?? 120,
              enablePinning: col.pinned !== undefined,
              meta: { align: col.align },
              cell: (info) =>
                col.render
                  ? col.render(info.row.original)
                  : String(info.getValue()),
            },
          ),
        ),
      ),
    [columnHelper, columnDefs],
  );

  const growColumnId = columnDefs.find((c) => c.grow)?.key;

  // pinned 列 → tanstack initialState（固定列状态由这里声明）
  const initialState = useMemo(
    () => ({
      columnPinning: {
        // v9 的 ColumnPinningState 字段是 start/end
        start: columnDefs.filter((c) => c.pinned === 'start').map((c) => c.key),
        end: columnDefs.filter((c) => c.pinned === 'end').map((c) => c.key),
      },
    }),
    [columnDefs],
  );

  return (
    <VirtualTable
      features={features}
      columns={columns}
      data={data}
      initialState={initialState}
      growColumnId={growColumnId}
      getRowId={getRowId}
      onLoadMore={onLoadMore}
      loadingMore={loadingMore}
    />
  );
}
