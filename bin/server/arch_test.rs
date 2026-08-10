/// 架构边界测试：解析 workspace Cargo.toml 验证依赖方向规则。
///
/// 零外部依赖，无需数据库。
#[cfg(test)]
mod arch_test {
    use std::collections::HashSet;
    use std::path::PathBuf;

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    /// 目录名（features/ 下的直接子目录）。
    fn feature_dir_names() -> Vec<String> {
        let mut names = Vec::new();
        let features_dir = workspace_root().join("features");
        for entry in std::fs::read_dir(&features_dir).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                names.push(entry.file_name().to_string_lossy().to_string());
            }
        }
        names.sort();
        names
    }

    /// contract crate 名（以 `_contract` 结尾）及其依赖。
    fn contract_crates() -> Vec<(String, HashSet<String>)> {
        feature_dir_names()
            .into_iter()
            .filter(|n| n.ends_with("_contract"))
            .map(|n| {
                let deps = parse_cargo_deps(&workspace_root().join("features").join(&n));
                (n, deps)
            })
            .collect()
    }

    /// feature runtime crate 名（不以 `_contract` 结尾且不是 shared_contract）及其依赖。
    fn feature_runtime_crates() -> Vec<(String, HashSet<String>)> {
        feature_dir_names()
            .into_iter()
            .filter(|n| !n.ends_with("_contract"))
            .map(|n| {
                let deps = parse_cargo_deps(&workspace_root().join("features").join(&n));
                (n, deps)
            })
            .collect()
    }

    /// 解析 Cargo.toml 中的 `[dependencies]` 节，提取包名集合。
    fn parse_cargo_deps<P: Into<PathBuf>>(crate_dir: P) -> HashSet<String> {
        let cargo_toml = crate_dir.into().join("Cargo.toml");
        let content = std::fs::read_to_string(&cargo_toml).unwrap();
        let mut deps = HashSet::new();

        let doc: toml::Value = toml::from_str(&content).unwrap();
        if let Some(dep_table) = doc.get("dependencies").and_then(|v| v.as_table()) {
            for (name, _) in dep_table {
                deps.insert(name.to_owned());
            }
        }
        deps
    }

    /// infrastructure/ 下的直接子目录（crate 名 + 依赖）。
    fn infra_crates() -> Vec<(String, HashSet<String>)> {
        let infra_dir = workspace_root().join("infrastructure");
        let mut names = Vec::new();
        for entry in std::fs::read_dir(&infra_dir).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                names.push((name.clone(), parse_cargo_deps(&infra_dir.join(&name))));
            }
        }
        names.sort_by(|a, b| a.0.cmp(&b.0));
        names
    }

    /// contract crate 不得依赖任何 feature runtime crate。
    #[test]
    fn contract_crates_should_not_depend_on_feature_runtime_crates() {
        let runtime_names: HashSet<String> = feature_runtime_crates()
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        for (contract, deps) in contract_crates() {
            let violations: Vec<_> = deps.intersection(&runtime_names).collect();
            assert!(
                violations.is_empty(),
                "contract `{contract}` depends on runtime crate(s): {violations:?}"
            );
        }
    }

    /// contract crate 不得依赖其他 contract crate（shared_contract 除外）。
    #[test]
    fn contract_crates_should_not_depend_on_other_contract_crates() {
        let contract_names: HashSet<String> = contract_crates()
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        for (contract, deps) in contract_crates() {
            let violations: Vec<_> = deps
                .iter()
                .filter(|d| *d != "shared_contract" && contract_names.contains(*d))
                .collect();
            assert!(
                violations.is_empty(),
                "contract `{contract}` depends on contract crate(s): {violations:?}"
            );
        }
    }

    /// contract crate 不得依赖 infrastructure/*（领域内核不承载技术实现）。
    #[test]
    fn contract_crates_should_not_depend_on_infrastructure() {
        let infra_names: HashSet<String> = infra_crates().into_iter().map(|(n, _)| n).collect();
        for (contract, deps) in contract_crates() {
            let violations: Vec<_> = deps.intersection(&infra_names).collect();
            assert!(
                violations.is_empty(),
                "contract `{contract}` depends on infrastructure crate(s): {violations:?}"
            );
        }
    }

    /// infrastructure/* 不得依赖 feature runtime crate（深度模块可依赖 *_contract，但不得依赖切片）。
    #[test]
    fn infra_crates_should_not_depend_on_feature_runtime_crates() {
        let runtime_names: HashSet<String> = feature_runtime_crates()
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        for (name, deps) in infra_crates() {
            let violations: Vec<_> = deps.intersection(&runtime_names).collect();
            assert!(
                violations.is_empty(),
                "infrastructure `{name}` depends on feature runtime crate(s): {violations:?}"
            );
        }
    }

    /// `{domain}_contract::port` 只允许只读查询：禁止写 SQL（INSERT / UPDATE / DELETE）。
    /// 启发式：先剔除 `FOR UPDATE`，再匹配写关键字，避免误报锁定读。
    #[test]
    fn contract_ports_should_not_contain_write_sql() {
        let features_dir = workspace_root().join("features");
        for entry in std::fs::read_dir(&features_dir).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.ends_with("_contract") {
                continue;
            }
            let port_file = features_dir.join(&name).join("port.rs");
            if !port_file.exists() {
                continue;
            }
            let content = std::fs::read_to_string(&port_file).unwrap();
            let sanitized = content.replace("FOR UPDATE", "");
            for (label, needle) in [
                ("INSERT INTO", "INSERT INTO"),
                ("DELETE FROM", "DELETE FROM"),
                ("UPDATE", "UPDATE "),
            ] {
                assert!(
                    !sanitized.contains(needle),
                    "contract `{name}` port.rs contains write SQL `{label}`: {port_file:?}"
                );
            }
        }
    }

    /// feature runtime crate 不得依赖其他 feature runtime crate（只允许依赖 contract）。
    #[test]
    fn feature_crates_should_not_depend_on_other_feature_runtime_crates() {
        let all_runtime: Vec<(String, HashSet<String>)> = feature_runtime_crates();
        let runtime_names: HashSet<_> = all_runtime.iter().map(|(n, _)| n.clone()).collect();
        for (crate_name, deps) in &all_runtime {
            let violations: Vec<_> = deps
                .iter()
                .filter(|d| *d != crate_name && runtime_names.contains(*d))
                .collect();
            assert!(
                violations.is_empty(),
                "feature `{crate_name}` depends on runtime crate(s): {violations:?}"
            );
        }
    }

    /// contract 和 feature crate 的目录名必须与 Cargo.toml 中的 `[package].name` 一致。
    #[test]
    fn contract_crate_package_names_match_directories() {
        let workspace_root = workspace_root();
        for dir_name in feature_dir_names() {
            let cargo_path = workspace_root
                .join("features")
                .join(&dir_name)
                .join("Cargo.toml");
            let content = std::fs::read_to_string(&cargo_path).unwrap();
            let doc: toml::Value = toml::from_str(&content).unwrap();
            let pkg_name = doc["package"]["name"].as_str().unwrap();
            assert_eq!(
                pkg_name, dir_name,
                "directory `{dir_name}` does not match package name `{pkg_name}`"
            );
        }
    }

    /// 写端点必须接入变更历史（引用 `audit_contract::AuditService`）。
    /// 按动作词识别写端点文件；`EXEMPT` 为设计豁免（blob / 会话写，无资源变更）；
    /// `AUDIT_TODO` 为欠账白名单——未接线写端点，接入后从列表移除。
    #[test]
    fn write_endpoints_must_wire_audit_service() {
        const WRITE_VERBS: &[&str] = &[
            "create",
            "update",
            "delete",
            "submit",
            "approve",
            "reject",
            "release",
            "complete",
            "initial",
            "report",
            "pick",
            "update_password",
            "reset_password",
        ];
        // 设计豁免：blob 写无 DB 资源行；会话写按 ADR-0001 不记请求级审计。
        const EXEMPT: &[&str] = &[
            "file_upload_image",
            "account_login",
            "account_logout",
            "account_refresh_token",
        ];
        // 欠账白名单：全部写端点已接入变更历史（2026-08），列表清空。
        // 新增写端点默认必须引用 AuditService，否则本规则红。
        const AUDIT_TODO: &[&str] = &[];

        let features_dir = workspace_root().join("features");
        for entry in std::fs::read_dir(&features_dir).unwrap() {
            let entry = entry.unwrap();
            let domain = entry.file_name().to_string_lossy().to_string();
            if !entry.file_type().unwrap().is_dir() || domain.ends_with("_contract") {
                continue;
            }
            let endpoint_dir = features_dir.join(&domain).join("endpoint");
            if !endpoint_dir.exists() {
                continue;
            }
            for file in std::fs::read_dir(&endpoint_dir).unwrap() {
                let file = file.unwrap();
                let stem = file.file_name().to_string_lossy().to_string();
                if !stem.ends_with(".rs") {
                    continue;
                }
                let stem = stem.trim_end_matches(".rs");
                if !WRITE_VERBS.iter().any(|v| stem.ends_with(v)) || EXEMPT.contains(&stem) {
                    continue;
                }
                let content = std::fs::read_to_string(file.path()).unwrap();
                if content.contains("AuditService") {
                    assert!(
                        !AUDIT_TODO.contains(&stem),
                        "`{stem}` is wired to AuditService but still listed in AUDIT_TODO — \
                         remove it from the whitelist"
                    );
                } else {
                    assert!(
                        AUDIT_TODO.contains(&stem),
                        "write endpoint `{stem}` does not wire `AuditService` and is not in \
                         AUDIT_TODO - wire change history (AuditService::record_*) or add it to \
                         the whitelist"
                    );
                }
            }
        }
    }

    /// 检测行中是否存在 `status != <数字>` 或 `status == <数字>` 模式。
    /// 枚举常量（字母开头）不匹配；SQL 单等号 `status = $1` 不在检测范围。
    fn status_compares_with_magic_number(line: &str) -> bool {
        for op in ["!=", "=="] {
            let needle = format!("status {op} ");
            if let Some(pos) = line.find(&needle) {
                let rest = line[pos + needle.len()..].trim_start();
                if rest.starts_with('-') || rest.chars().next().is_some_and(|c| c.is_ascii_digit())
                {
                    return true;
                }
            }
        }
        false
    }

    /// 端点不得用裸数字比较 status 字段（必须用领域状态枚举常量）。
    ///
    /// 启发式：扫描 `endpoint/*.rs` 业务代码行（排除注释与 `assert_eq!`/`assert_ne!` 测试断言），
    /// 检测 `<...>.status != <数字>` / `<...>.status == <数字>` 模式。
    /// 枚举常量比较（`status != PurchaseOrderStatus::Approved as i16`）以字母开头，不误报。
    /// SQL 的 `status = $1`（单等号）不在检测范围。
    #[test]
    fn endpoints_should_not_compare_status_with_magic_number() {
        // 欠账白名单：尚未枚举化的域（P0-2/3/4 待处理），完成后从列表移除。
        const STATUS_TODO: &[&str] = &[
            "sales_delivery_create",
            "inspection_order_complete",
            "bom_release",
        ];
        let features_dir = workspace_root().join("features");
        for entry in std::fs::read_dir(&features_dir).unwrap() {
            let entry = entry.unwrap();
            let domain = entry.file_name().to_string_lossy().to_string();
            if !entry.file_type().unwrap().is_dir() || domain.ends_with("_contract") {
                continue;
            }
            let endpoint_dir = features_dir.join(&domain).join("endpoint");
            if !endpoint_dir.exists() {
                continue;
            }
            for file in std::fs::read_dir(&endpoint_dir).unwrap() {
                let file = file.unwrap();
                let stem = file
                    .file_name()
                    .to_string_lossy()
                    .trim_end_matches(".rs")
                    .to_string();
                if STATUS_TODO.contains(&stem.as_str()) {
                    continue;
                }
                let path = file.path();
                let content = std::fs::read_to_string(&path).unwrap();
                for (lineno, line) in content.lines().enumerate() {
                    let trimmed = line.trim();
                    if trimmed.starts_with("//")
                        || trimmed.contains("assert_eq!")
                        || trimmed.contains("assert_ne!")
                    {
                        continue;
                    }
                    assert!(
                        !status_compares_with_magic_number(line),
                        "magic-number status comparison in {}:{lineno}: {line:?} — \
                         use a domain status enum constant (e.g. \
                         `PurchaseOrderStatus::Approved as i16`) instead of a bare integer",
                        path.display()
                    );
                }
            }
        }
    }
}
