// TanStack Form 文本字段控件：label + Input + 错误展示（FieldError）一体，
// 消除各表单重复的字段 JSX 样板（customers/profile 共用）。
// 用法：<form.Field name="xxx" validators={{...}}>{(field) => <TextField field={field} label="..." />}</form.Field>
// 跨字段校验 / 自定义控件（如密码可见性切换）保留原样写法。
import type { InputHTMLAttributes } from 'react';
import { FieldError } from '@/components/FieldError';
import { Input } from '@/components/ui/input';

// FieldApi 的结构子集（handleBlur/handleChange 签名对 string 字段成立；meta 供 FieldError 展示）
interface TextFieldField {
  state: {
    value: string;
    meta: { isTouched: boolean; errors: unknown[] };
  };
  handleBlur: () => void;
  handleChange: (value: string) => void;
}

interface TextFieldProps {
  field: TextFieldField;
  /** 与 label 关联的 Input id */
  id: string;
  label: string;
  required?: boolean;
  placeholder?: string;
  type?: InputHTMLAttributes<HTMLInputElement>['type'];
  inputMode?: InputHTMLAttributes<HTMLInputElement>['inputMode'];
  autoComplete?: string;
}

export function TextField({
  field,
  id,
  label,
  required,
  ...inputProps
}: TextFieldProps) {
  return (
    <label htmlFor={id} className="block">
      <span className="text-sm text-ink-soft">
        {label}
        {required && <span className="text-destructive"> *</span>}
      </span>
      <Input
        id={id}
        value={field.state.value}
        onBlur={field.handleBlur}
        onChange={(e) => field.handleChange(e.target.value)}
        className="mt-1"
        {...inputProps}
      />
      <FieldError field={field} />
    </label>
  );
}
