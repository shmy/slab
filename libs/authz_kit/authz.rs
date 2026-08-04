//! 基于 Cedar 的 RBAC 授权系统。
//!
//! 一条龙：配置角色权限 → 编译策略 → 授权判断。
//!
//! # 示例
//!
//! ```
//! use authz_kit::{Authz, Permission};
//!
//! let mut authz = Authz::new();
//! authz.set_role_permissions("admin", vec![
//!     Permission { resource_type: None, action: Some("read".into()) },
//! ]);
//! assert!(authz.is_authorized("alice", &["admin"], "doc-1", "read", false).unwrap());
//! ```

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use cedar_policy::{Authorizer, Context, Decision, Entities, Entity, PolicySet, Request};
use serde_json::json;

use crate::entities;
use crate::error::AuthzError;

/// 一条权限：对某个 resource_type 执行某个 action
///
/// `None` 表示通配所有。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Permission {
    pub resource_type: Option<String>,
    pub action: Option<String>,
}

/// 授权系统入口
///
/// 内部缓存编译后的 PolicySet（RwLock 保证线程安全），角色权限变更时自动失效。
#[derive(Debug, Default)]
pub struct Authz {
    role_permissions: HashMap<String, Vec<Permission>>,
    known_actions: HashSet<String>,
    cached_policies: RwLock<Option<Arc<PolicySet>>>,
}

impl Authz {
    pub fn new() -> Self {
        Self::default()
    }

    // ── 管理接口 ──────────────────────────────────

    pub fn register_action(&mut self, action: &str) {
        self.known_actions.insert(action.to_owned());
    }

    /// 创建或更新角色权限（使编译缓存失效）
    pub fn set_role_permissions(&mut self, role_name: &str, permissions: Vec<Permission>) {
        for p in &permissions {
            if let Some(action) = &p.action {
                self.known_actions.insert(action.clone());
            }
        }
        self.role_permissions
            .insert(role_name.to_owned(), permissions);
        self.invalidate_cache();
    }

    /// 删除角色（使编译缓存失效）
    pub fn remove_role(&mut self, role_name: &str) {
        self.role_permissions.remove(role_name);
        self.invalidate_cache();
    }

    /// 清空所有角色权限（使编译缓存失效）
    pub fn clear(&mut self) {
        self.role_permissions.clear();
        self.known_actions.clear();
        self.invalidate_cache();
    }

    fn invalidate_cache(&self) {
        let _ = self.cached_policies.write().map(|mut guard| *guard = None);
    }

    // ── 授权判断（对外主接口） ──────────────────────

    /// 判断用户能否对资源执行某个动作
    ///
    /// `privileged` 为 true 时用户自动获得 superuser 权限。
    pub fn is_authorized(
        &self,
        user_id: &str,
        user_roles: &[&str],
        resource_id: &str,
        action: &str,
        privileged: bool,
    ) -> Result<bool, AuthzError> {
        Ok(self.evaluate(user_id, user_roles, resource_id, action, privileged)? == Decision::Allow)
    }

    /// 完整评估，返回 Decision
    pub fn evaluate(
        &self,
        user_id: &str,
        user_roles: &[&str],
        resource_id: &str,
        action: &str,
        privileged: bool,
    ) -> Result<Decision, AuthzError> {
        let policies = self.get_or_compile_policies()?;
        let entities = self.build_entities(user_id, user_roles, resource_id, privileged)?;

        let request = Request::new(
            entities::user_uid(user_id),
            entities::action_uid(action),
            entities::resource_uid(resource_id),
            Context::empty(),
            None,
        )
        .map_err(|e| AuthzError::Request(e.to_string()))?;

        Ok(Authorizer::new()
            .is_authorized(&request, &policies, &entities)
            .decision())
    }

    // ── 策略编译（带缓存） ──────────────────────────

    fn get_or_compile_policies(&self) -> Result<Arc<PolicySet>, AuthzError> {
        // 快速路径：读锁命中
        if let Some(ps) = self
            .cached_policies
            .read()
            .ok()
            .and_then(|g| g.as_ref().cloned())
        {
            return Ok(Arc::clone(&ps));
        }
        // 慢速路径：写锁编译
        let mut guard = self
            .cached_policies
            .write()
            .map_err(|_| AuthzError::Request("rwlock poisoned".into()))?;
        // 双重检查：可能已被其他线程填充
        if guard.is_none() {
            *guard = Some(Arc::new(self.compile_policy_set_uncached()?));
        }
        Ok(Arc::clone(guard.as_ref().ok_or_else(|| {
            AuthzError::Request("policy not compiled after write".into())
        })?))
    }

    fn compile_policy_set_uncached(&self) -> Result<PolicySet, AuthzError> {
        let mut static_policies = serde_json::Map::new();

        // 内置 superuser
        static_policies.insert(
            "__builtin_superuser".to_owned(),
            json!({
                "effect": "permit",
                "principal": { "op": "in", "entity": { "type": "Role", "id": "__superuser" } },
                "action":    { "op": "All" },
                "resource":  { "op": "All" },
                "conditions": []
            }),
        );

        for (role_name, permissions) in &self.role_permissions {
            for perm in permissions {
                let policy_id = match &perm.action {
                    None => format!("{role_name}__all"),
                    Some(action) => format!("{role_name}__{action}"),
                };

                static_policies.insert(
                    policy_id,
                    json!({
                        "effect": "permit",
                        "principal": { "op": "in", "entity": { "type": "Role", "id": role_name } },
                        "action": build_action_clause(perm),
                        "resource": build_resource_clause(perm),
                        "conditions": []
                    }),
                );
            }
        }

        PolicySet::from_json_value(json!({
            "staticPolicies": static_policies,
            "templates": {},
            "templateLinks": []
        }))
        .map_err(|e| AuthzError::Policy(Box::new(e)))
    }

    // ── Entities 构造 ─────────────────────────────

    fn build_base_entities(&self) -> Vec<Entity> {
        let mut entities = vec![entities::role_entity(entities::SUPERUSER_ROLE)];

        for name in self.role_permissions.keys() {
            entities.push(entities::role_entity(name));
        }
        for name in &self.known_actions {
            entities.push(entities::action_entity(name));
        }

        entities
    }

    fn build_entities(
        &self,
        user_id: &str,
        user_roles: &[&str],
        resource_id: &str,
        privileged: bool,
    ) -> Result<Entities, AuthzError> {
        Entities::from_entities(
            self.build_base_entities()
                .into_iter()
                .chain([entities::user_entity(user_id, user_roles, privileged)])
                .chain([entities::resource_entity(resource_id)]),
            None,
        )
        .map_err(|e| AuthzError::EntitySet(e.to_string()))
    }
}

// 编译期断言：Authz 可安全用于 Arc<AppCtx>
const _: () = {
    fn assert_send_sync<T: Send + Sync>() {}
    let _ = assert_send_sync::<Authz>;
};

// ── 策略子句构建 ──────────────────────────────────────

fn build_action_clause(perm: &Permission) -> serde_json::Value {
    match &perm.action {
        None => json!({ "op": "All" }),
        Some(action) => json!({ "op": "==", "entity": { "type": "Action", "id": action } }),
    }
}

fn build_resource_clause(perm: &Permission) -> serde_json::Value {
    match &perm.resource_type {
        None => json!({ "op": "All" }),
        Some(rt) => json!({ "op": "is", "entity_type": rt }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_perm(action: &str) -> Permission {
        Permission {
            resource_type: None,
            action: Some(action.to_owned()),
        }
    }

    fn perm_all() -> Permission {
        Permission {
            resource_type: None,
            action: None,
        }
    }

    // ── 集成测试 ──────────────────────────────────────

    #[test]
    fn dynamic_rbac_full_flow() {
        let mut authz = Authz::new();
        authz.set_role_permissions(
            "order_admin",
            vec![
                make_perm("order:read"),
                make_perm("order:write"),
                make_perm("order:delete"),
            ],
        );
        authz.set_role_permissions("order_viewer", vec![make_perm("order:read")]);

        assert!(
            authz
                .is_authorized(
                    "zhangsan",
                    &["order_admin"],
                    "order-001",
                    "order:read",
                    false
                )
                .unwrap()
        );
        assert!(
            authz
                .is_authorized(
                    "zhangsan",
                    &["order_admin"],
                    "order-001",
                    "order:delete",
                    false
                )
                .unwrap()
        );

        assert!(
            authz
                .is_authorized("lisi", &["order_viewer"], "order-001", "order:read", false)
                .unwrap()
        );
        assert!(
            !authz
                .is_authorized("lisi", &["order_viewer"], "order-001", "order:write", false)
                .unwrap()
        );

        assert!(
            !authz
                .is_authorized("wangwu", &[], "order-001", "order:read", false)
                .unwrap()
        );
    }

    #[test]
    fn permission_change_takes_effect() {
        let mut authz = Authz::new();
        authz.set_role_permissions("viewer", vec![make_perm("report:read")]);

        assert!(
            !authz
                .is_authorized("alice", &["viewer"], "rpt-1", "report:write", false)
                .unwrap()
        );

        authz.set_role_permissions(
            "viewer",
            vec![make_perm("report:read"), make_perm("report:write")],
        );
        assert!(
            authz
                .is_authorized("alice", &["viewer"], "rpt-1", "report:write", false)
                .unwrap()
        );
    }

    #[test]
    fn user_role_change_takes_effect() {
        let mut authz = Authz::new();
        authz.set_role_permissions("editor", vec![make_perm("doc:write")]);

        assert!(
            !authz
                .is_authorized("bob", &[], "doc-1", "doc:write", false)
                .unwrap()
        );
        assert!(
            authz
                .is_authorized("bob", &["editor"], "doc-1", "doc:write", false)
                .unwrap()
        );
    }

    #[test]
    fn wildcard_action() {
        let mut authz = Authz::new();
        authz.set_role_permissions("superadmin", vec![perm_all()]);

        assert!(
            authz
                .is_authorized("root", &["superadmin"], "any", "anything", false)
                .unwrap()
        );
    }

    #[test]
    fn privileged_user_bypasses_all() {
        let mut authz = Authz::new();
        authz.set_role_permissions("viewer", vec![make_perm("report:read")]);

        assert!(
            !authz
                .is_authorized("bob", &[], "rpt-1", "report:read", false)
                .unwrap()
        );

        assert!(
            authz
                .is_authorized("root", &[], "rpt-1", "report:read", true)
                .unwrap()
        );
        assert!(
            authz
                .is_authorized("root", &[], "rpt-1", "report:write", true)
                .unwrap()
        );
        assert!(
            authz
                .is_authorized("root", &[], "rpt-1", "report:delete", true)
                .unwrap()
        );
    }

    #[test]
    fn roundtrip_json_to_cedar() {
        let mut authz = Authz::new();
        authz.set_role_permissions(
            "order_admin",
            vec![
                make_perm("order:read"),
                make_perm("order:write"),
                make_perm("order:delete"),
            ],
        );

        let policies = authz.get_or_compile_policies().unwrap();
        let cedar = policies.to_cedar().unwrap();
        println!("=== Cedar 语法 ===\n{cedar}");

        assert!(cedar.contains("permit"));
        assert!(cedar.contains("Role::\"order_admin\""));
        assert!(cedar.contains("Action::\"order:read\""));
        assert!(cedar.contains("Action::\"order:write\""));
        assert!(cedar.contains("Action::\"order:delete\""));

        let json_val = (*policies).clone().to_json().unwrap();
        let _back = PolicySet::from_json_value(json_val).unwrap();
    }

    #[test]
    fn evaluate_returns_decision() {
        let mut authz = Authz::new();
        authz.set_role_permissions("admin", vec![make_perm("order:read")]);

        assert_eq!(
            authz
                .evaluate("alice", &["admin"], "order-1", "order:read", false)
                .unwrap(),
            Decision::Allow
        );
        assert_eq!(
            authz
                .evaluate("bob", &[], "order-1", "order:read", false)
                .unwrap(),
            Decision::Deny
        );
    }

    #[test]
    fn cache_is_used() {
        let mut authz = Authz::new();
        authz.set_role_permissions("admin", vec![make_perm("read")]);

        // 第一次：缓存为空，会编译
        assert!(authz.cached_policies.read().unwrap().is_none());
        let r1 = authz.is_authorized("alice", &["admin"], "doc", "read", false);
        assert!(r1.unwrap());
        assert!(authz.cached_policies.read().unwrap().is_some());

        // 第二次：缓存命中，不再编译
        let r2 = authz.is_authorized("alice", &["admin"], "doc", "read", false);
        assert!(r2.unwrap());

        // 修改权限后缓存失效
        authz.set_role_permissions("viewer", vec![make_perm("read")]);
        assert!(authz.cached_policies.read().unwrap().is_none());
    }

    // ── 纯 Cedar API 演示 ──

    const DEMO_POLICIES: &str = r#"
permit(principal in Role::"__superuser", action, resource);
permit(principal in Role::"admin", action, resource);
permit(principal in Role::"editor", action == Action::"read", resource);
permit(principal in Role::"editor", action == Action::"write", resource);
permit(principal in Role::"viewer", action == Action::"read", resource);
"#;

    fn static_entities() -> Vec<Entity> {
        vec![
            entities::role_entity(entities::SUPERUSER_ROLE),
            entities::role_entity("admin"),
            entities::role_entity("editor"),
            entities::role_entity("viewer"),
            entities::action_entity("read"),
            entities::action_entity("write"),
            entities::action_entity("delete"),
        ]
    }

    fn make_entities(user: &str, roles: &[&str], resource: &str, privileged: bool) -> Entities {
        Entities::from_entities(
            static_entities()
                .into_iter()
                .chain([entities::user_entity(user, roles, privileged)])
                .chain([entities::resource_entity(resource)]),
            None,
        )
        .unwrap()
    }

    fn check(
        policies: &PolicySet,
        entities: &Entities,
        user: &str,
        action: &str,
        resource: &str,
    ) -> bool {
        let request = Request::new(
            entities::user_uid(user),
            entities::action_uid(action),
            entities::resource_uid(resource),
            Context::empty(),
            None,
        )
        .unwrap();
        Authorizer::new()
            .is_authorized(&request, policies, entities)
            .decision()
            == Decision::Allow
    }

    #[test]
    fn static_admin_can_do_anything() {
        let p: PolicySet = DEMO_POLICIES.parse().unwrap();
        let e = make_entities("alice", &["admin"], "doc-1", false);
        assert!(check(&p, &e, "alice", "delete", "doc-1"));
    }

    #[test]
    fn static_viewer_read_only() {
        let p: PolicySet = DEMO_POLICIES.parse().unwrap();
        let e = make_entities("bob", &["viewer"], "doc-1", false);
        assert!(check(&p, &e, "bob", "read", "doc-1"));
        assert!(!check(&p, &e, "bob", "write", "doc-1"));
    }

    #[test]
    fn static_editor_read_write_no_delete() {
        let p: PolicySet = DEMO_POLICIES.parse().unwrap();
        let e = make_entities("carol", &["editor"], "doc-1", false);
        assert!(check(&p, &e, "carol", "read", "doc-1"));
        assert!(check(&p, &e, "carol", "write", "doc-1"));
        assert!(!check(&p, &e, "carol", "delete", "doc-1"));
    }

    #[test]
    fn static_no_role_denied() {
        let p: PolicySet = DEMO_POLICIES.parse().unwrap();
        let e = make_entities("guest", &[], "doc-1", false);
        assert!(!check(&p, &e, "guest", "read", "doc-1"));
    }

    #[test]
    fn static_multi_role_most_permissive() {
        let p: PolicySet = DEMO_POLICIES.parse().unwrap();
        let e = make_entities("dave", &["viewer", "editor"], "doc-1", false);
        assert!(check(&p, &e, "dave", "write", "doc-1"));
    }

    #[test]
    fn static_privileged_bypasses_all() {
        let p: PolicySet = DEMO_POLICIES.parse().unwrap();
        let e = make_entities("root", &[], "any", true);
        assert!(check(&p, &e, "root", "read", "any"));
        assert!(check(&p, &e, "root", "write", "any"));
        assert!(check(&p, &e, "root", "delete", "any"));
    }
}
