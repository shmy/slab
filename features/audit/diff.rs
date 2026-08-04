//! 读时字段级 diff：由 before / after JSONB 快照计算变更明细（git diff 风格展示的输入）。

use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;

/// 一条字段级变更。
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ChangeField {
    /// 字段路径，如 `status`、`address.city`（数组整体对比）
    pub field: String,
    /// 变更前值（新增字段为 null）
    pub before: Value,
    /// 变更后值（删除字段为 null）
    pub after: Value,
}

/// 计算两个 JSON 快照的字段级差异。
///
/// - 创建（before 为 `None`）：after 所有叶子字段记 `before: null`
/// - 删除（after 为 `None`）：before 所有叶子字段记 `after: null`
/// - 更新：比较同名路径；对象递归展开（`.` 路径）；数组整体对比；标量直接比较
pub fn json_diff(before: Option<&Value>, after: Option<&Value>) -> Vec<ChangeField> {
    match (before, after) {
        (None, Some(after)) => collect("", after)
            .into_iter()
            .map(|(field, value)| ChangeField {
                field,
                before: Value::Null,
                after: value,
            })
            .collect(),
        (Some(before), None) => collect("", before)
            .into_iter()
            .map(|(field, value)| ChangeField {
                field,
                before: value,
                after: Value::Null,
            })
            .collect(),
        (Some(before), Some(after)) => {
            let mut out = Vec::new();
            diff_value("", before, after, &mut out);
            out
        }
        (None, None) => Vec::new(),
    }
}

/// 展平快照为叶子路径（对象递归展开，数组 / 标量作为整体），键排序保证确定性。
fn collect(path: &str, value: &Value) -> Vec<(String, Value)> {
    let Value::Object(map) = value else {
        return vec![(path.to_string(), value.clone())];
    };
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort();
    let mut result = Vec::with_capacity(map.len());
    for key in keys {
        let child_path = if path.is_empty() {
            key.clone()
        } else {
            format!("{path}.{key}")
        };
        result.extend(collect(&child_path, &map[key]));
    }
    result
}

fn diff_value(path: &str, before: &Value, after: &Value, out: &mut Vec<ChangeField>) {
    match (before, after) {
        (before, after) if before == after => {}
        (Value::Object(before_map), Value::Object(after_map)) => {
            let mut keys: Vec<&String> = before_map.keys().chain(after_map.keys()).collect();
            keys.sort();
            keys.dedup();
            for key in keys {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                diff_value(
                    &child_path,
                    before_map.get(key).unwrap_or(&Value::Null),
                    after_map.get(key).unwrap_or(&Value::Null),
                    out,
                );
            }
        }
        _ => out.push(ChangeField {
            field: path.to_string(),
            before: before.clone(),
            after: after.clone(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_create_records_all_fields_with_null_before() {
        let after = json!({"name": "Tom", "age": 30});
        let fields = json_diff(None, Some(&after));
        assert_eq!(fields.len(), 2);
        // 键排序：age 先于 name
        assert_eq!(fields[0].field, "age");
        assert_eq!(fields[0].before, Value::Null);
        assert_eq!(fields[0].after, json!(30));
        assert_eq!(fields[1].field, "name");
        assert_eq!(fields[1].after, json!("Tom"));
    }

    #[test]
    fn test_delete_records_all_fields_with_null_after() {
        let before = json!({"name": "Tom"});
        let fields = json_diff(Some(&before), None);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field, "name");
        assert_eq!(fields[0].before, json!("Tom"));
        assert_eq!(fields[0].after, Value::Null);
    }

    #[test]
    fn test_update_skips_unchanged_fields() {
        let before = json!({"name": "Tom", "age": 30});
        let after = json!({"name": "Tom", "age": 31});
        let fields = json_diff(Some(&before), Some(&after));
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field, "age");
        assert_eq!(fields[0].before, json!(30));
        assert_eq!(fields[0].after, json!(31));
    }

    #[test]
    fn test_update_field_added_and_removed() {
        let before = json!({"name": "Tom", "old": 1});
        let after = json!({"name": "Tom", "new": 2});
        let fields = json_diff(Some(&before), Some(&after));
        // 键排序：new 先于 old（新增/删除各一条）
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].field, "new");
        assert_eq!(fields[0].before, Value::Null);
        assert_eq!(fields[0].after, json!(2));
        assert_eq!(fields[1].field, "old");
        assert_eq!(fields[1].before, json!(1));
        assert_eq!(fields[1].after, Value::Null);
    }

    #[test]
    fn test_nested_object_expands_with_path() {
        let before = json!({"address": {"city": "Shanghai", "zip": "200000"}});
        let after = json!({"address": {"city": "Hangzhou", "zip": "200000"}});
        let fields = json_diff(Some(&before), Some(&after));
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field, "address.city");
        assert_eq!(fields[0].before, json!("Shanghai"));
        assert_eq!(fields[0].after, json!("Hangzhou"));
    }

    #[test]
    fn test_array_changed_records_whole_value() {
        let before = json!({"lines": [{"qty": 1}, {"qty": 2}]});
        let after = json!({"lines": [{"qty": 1}, {"qty": 3}]});
        let fields = json_diff(Some(&before), Some(&after));
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field, "lines");
        assert_eq!(fields[0].before, json!([{"qty": 1}, {"qty": 2}]));
        assert_eq!(fields[0].after, json!([{"qty": 1}, {"qty": 3}]));
    }

    #[test]
    fn test_empty_views() {
        assert!(json_diff(None, None).is_empty());
        assert!(json_diff(Some(&json!({})), Some(&json!({}))).is_empty());
    }
}
