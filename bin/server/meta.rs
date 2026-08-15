//! 筛选协议元数据端点：`GET /api/v1/meta/filter-schemas`。
//!
//! 事实源 = `libs/filter_kit` 操作符矩阵（[`filter_kit::OPERATOR_MATRIX`]）+ 各域端点声明的
//! `FILTER_SCHEMA` 白名单（见 [`FILTER_SCHEMAS`]）。前端 `pnpm gen:api` 拉取本端点 →
//! 生成 `src/lib/filter-schema.ts`，操作符矩阵 / 字段白名单不再双端手抄。
//!
//! 新增可筛实体：域内声明并导出 `FILTER_SCHEMA` → 此处加一行 → 前端补 label 映射。

use axum::Json;
use filter_kit::FilterSchema;
use serde_json::{Map, Value, json};

/// 已接入筛选的实体（实体名 → FilterSchema 白名单）。
pub(crate) const FILTER_SCHEMAS: &[(&str, FilterSchema)] = &[("customer", customer::FILTER_SCHEMA)];

/// 输出形状（与前端 `scripts/fetch-openapi.mjs` 的生成逻辑对应，改动需两端同步）：
/// ```json
/// {
///   "operatorMatrix": { "text": ["eq","neq","ilike"], "date": [...], "int": [...] },
///   "opPrefixes": ["ilike.","neq.","gte.","lte.","gt.","lt.","eq."],
///   "entities": { "customer": { "fields": [ { "name":"code","type":"text" }, ... ] } }
/// }
/// ```
pub(crate) fn handler() -> Json<Value> {
    let mut operator_matrix = Map::new();
    for (kind, ops) in filter_kit::OPERATOR_MATRIX {
        operator_matrix.insert(
            kind.to_string(),
            json!(ops.iter().map(|op| op.as_str()).collect::<Vec<_>>()),
        );
    }
    let op_prefixes: Vec<&str> = filter_kit::op_prefixes().iter().map(|(p, _)| *p).collect();

    let mut entities = Map::new();
    for (name, schema) in FILTER_SCHEMAS {
        let fields: Vec<Value> = schema
            .allowed_fields()
            .into_iter()
            .map(|f| json!({ "name": f, "type": schema.field_kind(f) }))
            .collect();
        entities.insert(name.to_string(), json!({ "fields": fields }));
    }

    Json(json!({
        "operatorMatrix": operator_matrix,
        "opPrefixes": op_prefixes,
        "entities": entities,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handler_output_shape_matches_frontend_contract() {
        let Json(value) = crate::meta::handler(); // 矩阵三键齐全且 ilike 只在 text
        let matrix = value["operatorMatrix"].as_object().expect("object");
        assert_eq!(matrix.len(), 3);
        assert!(matrix["text"].as_array().unwrap().contains(&json!("ilike")));
        assert!(!matrix["date"].as_array().unwrap().contains(&json!("ilike")));
        assert!(!matrix["int"].as_array().unwrap().contains(&json!("ilike")));
        // 前缀从长到短
        let prefixes = value["opPrefixes"].as_array().unwrap();
        assert_eq!(prefixes.first().unwrap(), &json!("ilike."));
        // 实体字段白名单
        let entities = value["entities"].as_object().expect("entities");
        assert!(entities.contains_key("customer"));
        let fields = entities["customer"]["fields"].as_array().unwrap();
        assert_eq!(fields.len(), 5);
        assert!(fields.contains(&json!({ "name": "code", "type": "text" })));
        assert!(fields.contains(&json!({ "name": "contact_person", "type": "text" })));
        assert!(fields.contains(&json!({ "name": "created_at", "type": "date" })));
    }
}
