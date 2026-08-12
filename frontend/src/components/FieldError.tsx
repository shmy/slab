// 表单字段错误展示：触碰过才显示，Set 去重（change+submit 双事件源会重复入列）

function errorText(error: unknown): string {
  if (typeof error === 'string') return error;
  if (typeof error === 'object' && error !== null && 'message' in error) {
    return String(error.message);
  }
  return String(error);
}

interface FieldLike {
  state: { meta: { isTouched: boolean; errors: unknown[] } };
}

export function FieldError({ field }: { field: FieldLike }) {
  if (!field.state.meta.isTouched || field.state.meta.errors.length === 0) {
    return null;
  }
  return (
    <p className="mt-1 text-sm text-nord11">
      {[...new Set(field.state.meta.errors.map(errorText))].join('、')}
    </p>
  );
}
