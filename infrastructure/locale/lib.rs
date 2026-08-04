pub mod middleware;

use std::{collections::HashMap, sync::LazyLock};

use fluent_bundle::{FluentResource, concurrent::FluentBundle as ConcurrentFluentBundle};
use unic_langid::LanguageIdentifier;

pub const DEFAULT_LOCALE: &str = "en-US";

type Bundle = ConcurrentFluentBundle<FluentResource>;

static BUNDLES: LazyLock<HashMap<String, Bundle>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    register(
        &mut map,
        "en-US",
        &[
            include_str!("locales/en-US/shared.ftl"),
            include_str!("locales/en-US/account.ftl"),
            include_str!("locales/en-US/audit.ftl"),
            include_str!("locales/en-US/file.ftl"),
            include_str!("locales/en-US/item.ftl"),
            include_str!("locales/en-US/customer.ftl"),
            include_str!("locales/en-US/supplier.ftl"),
            include_str!("locales/en-US/warehouse.ftl"),
            include_str!("locales/en-US/finance.ftl"),
            include_str!("locales/en-US/sales.ftl"),
            include_str!("locales/en-US/purchase.ftl"),
            include_str!("locales/en-US/quality.ftl"),
            include_str!("locales/en-US/production.ftl"),
            include_str!("locales/en-US/product.ftl"),
        ],
    );
    register(
        &mut map,
        "zh-CN",
        &[
            include_str!("locales/zh-CN/shared.ftl"),
            include_str!("locales/zh-CN/account.ftl"),
            include_str!("locales/zh-CN/audit.ftl"),
            include_str!("locales/zh-CN/file.ftl"),
            include_str!("locales/zh-CN/item.ftl"),
            include_str!("locales/zh-CN/customer.ftl"),
            include_str!("locales/zh-CN/supplier.ftl"),
            include_str!("locales/zh-CN/warehouse.ftl"),
            include_str!("locales/zh-CN/finance.ftl"),
            include_str!("locales/zh-CN/sales.ftl"),
            include_str!("locales/zh-CN/purchase.ftl"),
            include_str!("locales/zh-CN/quality.ftl"),
            include_str!("locales/zh-CN/production.ftl"),
            include_str!("locales/zh-CN/product.ftl"),
        ],
    );
    map
});

#[allow(
    clippy::expect_used,
    reason = "locale registration is infallible with pre-validated inputs"
)]
fn register(map: &mut HashMap<String, Bundle>, locale: &str, ftls: &[&str]) {
    let langid: LanguageIdentifier = locale.parse().expect("invalid locale identifier");
    let mut bundle = ConcurrentFluentBundle::new_concurrent(vec![langid]);
    for ftl in ftls {
        let resource = FluentResource::try_new(ftl.to_string()).expect("failed to parse FTL");
        bundle
            .add_resource(resource)
            .expect("failed to add FTL resource");
    }
    map.insert(locale.to_string(), bundle);
}

pub fn translate(locale: &str, key: &str) -> String {
    if let Some(text) = try_translate(locale, key) {
        return text;
    }
    if locale != DEFAULT_LOCALE
        && let Some(text) = try_translate(DEFAULT_LOCALE, key)
    {
        return text;
    }
    key.to_string()
}

fn try_translate(locale: &str, key: &str) -> Option<String> {
    let bundle = BUNDLES.get(locale)?;
    let msg = bundle.get_message(key)?;
    let pattern = msg.value()?;
    let mut errors = vec![];
    let value = bundle.format_pattern(pattern, None, &mut errors);
    Some(value.into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;
    use std::collections::HashSet;
    use std::fs;
    use std::path::Path;

    #[test]
    fn test_translate_known_key_zh() {
        assert_eq!(translate("zh-CN", "ok"), "操作成功");
    }

    #[test]
    fn test_translate_known_key_en() {
        assert_eq!(translate("en-US", "ok"), "OK");
    }

    #[test]
    fn test_translate_fallback_to_default() {
        let result = translate("ja", "ok");
        assert_eq!(result, "OK");
    }

    #[test]
    fn test_translate_unknown_key() {
        let result = translate("en-US", "no_such_key");
        assert_eq!(result, "no_such_key");
    }

    #[test]
    fn test_all_l10n_keys_used_in_code_exist_in_bundles() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
        // 捕获所有 #[error(...)] 消息（含参数化/句子风格），结构性校验：
        // - 纯 snake_case → 必须同时存在于 en-US / zh-CN bundle
        // - 参数化（含 `{`）→ 只允许出现在内部库（libs/image_kit、libs/authz_kit），
        //   它们永远 500，不进 locale；出现在域/基础设施代码中即测试失败
        // - 其他任何形态（句子风格）→ 测试失败：错误消息必须是 key 或显式豁免
        let thiserror_re =
            Regex::new(r#"#\[error\("([^"]*)"\)\]"#).expect("compile thiserror regex");
        let web_error_re = Regex::new(r#"WebError::L10n\("([a-zA-Z0-9_-]+)"\.to_string\(\)\)"#)
            .expect("compile web error regex");
        let mut keys = HashSet::new();
        let mut violations = Vec::new();

        collect_rs_files(&root, &mut |path| {
            let content = fs::read_to_string(path).expect("read source");
            for cap in thiserror_re.captures_iter(&content) {
                let msg = &cap[1];
                if msg
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                {
                    keys.insert(msg.to_string());
                } else if msg.contains('{') {
                    let p = path.display().to_string();
                    if !p.contains("libs/image_kit") && !p.contains("libs/authz_kit") {
                        violations.push(format!(
                            "参数化 #[error] 只允许在内部库（image_kit/authz_kit）：{p}: {msg}"
                        ));
                    }
                } else {
                    violations.push(format!(
                        "#[error] 消息必须是 snake_case key 或参数化（内部库）：{}: {msg}",
                        path.display()
                    ));
                }
            }
            for cap in web_error_re.captures_iter(&content) {
                keys.insert(cap[1].to_string());
            }
        });

        assert!(
            violations.is_empty(),
            "错误消息格式违规：\n{}",
            violations.join("\n")
        );

        for key in &keys {
            assert!(
                try_translate("en-US", key).is_some(),
                "missing key `{key}` in en-US locales"
            );
            assert!(
                try_translate("zh-CN", key).is_some(),
                "missing key `{key}` in zh-CN locales"
            );
        }
    }

    fn collect_rs_files(root: &Path, on_file: &mut impl FnMut(&Path)) {
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let entries = match fs::read_dir(&dir) {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n == "target" || n == ".git")
                        .unwrap_or(false)
                    {
                        continue;
                    }
                    stack.push(path);
                } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                    on_file(&path);
                }
            }
        }
    }
}
