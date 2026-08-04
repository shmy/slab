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
}
