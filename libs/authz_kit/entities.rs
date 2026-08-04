use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::LazyLock;

use cedar_policy::{Entity, EntityId, EntityTypeName, EntityUid, RestrictedExpression};

// ── 实体类型常量 ──────────────────────────────────────

pub const USER_TYPE: &str = "User";
pub const ROLE_TYPE: &str = "Role";
pub const ACTION_TYPE: &str = "Action";
pub const RESOURCE_TYPE: &str = "Resource";

/// 内置 superuser 角色名 —— privileged 用户自动获得此角色
pub const SUPERUSER_ROLE: &str = "__superuser";

// ── 预解析的 EntityTypeName ───────────────────────────

#[allow(clippy::expect_used, reason = "static literal never fails to parse")]
static USER_ETYPE: LazyLock<EntityTypeName> = LazyLock::new(|| {
    EntityTypeName::from_str("User").expect("User must be a valid EntityTypeName")
});

#[allow(clippy::expect_used, reason = "static literal never fails to parse")]
static ROLE_ETYPE: LazyLock<EntityTypeName> = LazyLock::new(|| {
    EntityTypeName::from_str("Role").expect("Role must be a valid EntityTypeName")
});

#[allow(clippy::expect_used, reason = "static literal never fails to parse")]
static ACTION_ETYPE: LazyLock<EntityTypeName> = LazyLock::new(|| {
    EntityTypeName::from_str("Action").expect("Action must be a valid EntityTypeName")
});

#[allow(clippy::expect_used, reason = "static literal never fails to parse")]
static RESOURCE_ETYPE: LazyLock<EntityTypeName> = LazyLock::new(|| {
    EntityTypeName::from_str("Resource").expect("Resource must be a valid EntityTypeName")
});

// ── Entity builder ────────────────────────────────────

pub struct EntityBuilder {
    ty: EntityTypeName,
    id: EntityId,
    attrs: HashMap<String, RestrictedExpression>,
    parents: HashSet<EntityUid>,
}

impl EntityBuilder {
    pub fn new(type_name: &'static LazyLock<EntityTypeName>, id: &str) -> Self {
        Self {
            ty: (**type_name).clone(),
            id: EntityId::new(id),
            attrs: HashMap::new(),
            parents: HashSet::new(),
        }
    }

    pub fn parent(mut self, uid: &EntityUid) -> Self {
        self.parents.insert(uid.clone());
        self
    }

    pub fn attr(mut self, key: &str, value: RestrictedExpression) -> Self {
        self.attrs.insert(key.to_owned(), value);
        self
    }

    #[allow(
        clippy::expect_used,
        reason = "Entity::new only fails with invalid attributes, which we don't pass"
    )]
    pub fn build(self) -> Entity {
        if self.attrs.is_empty() {
            Entity::new_no_attrs(
                EntityUid::from_type_name_and_id(self.ty, self.id),
                self.parents,
            )
        } else {
            Entity::new(
                EntityUid::from_type_name_and_id(self.ty, self.id),
                self.attrs,
                self.parents,
            )
            .expect("entity with attributes must be valid")
        }
    }
}

// ── UID helpers（EntityId::new 不会失败，无需 Result） ──

pub fn user_uid(id: &str) -> EntityUid {
    EntityUid::from_type_name_and_id((*USER_ETYPE).clone(), EntityId::new(id))
}

pub fn role_uid(name: &str) -> EntityUid {
    EntityUid::from_type_name_and_id((*ROLE_ETYPE).clone(), EntityId::new(name))
}

pub fn action_uid(name: &str) -> EntityUid {
    EntityUid::from_type_name_and_id((*ACTION_ETYPE).clone(), EntityId::new(name))
}

pub fn resource_uid(id: &str) -> EntityUid {
    EntityUid::from_type_name_and_id((*RESOURCE_ETYPE).clone(), EntityId::new(id))
}

// ── 实体构造（无 attributes，不会失败） ──────────────────

pub fn role_entity(name: &str) -> Entity {
    EntityBuilder::new(&ROLE_ETYPE, name).build()
}

pub fn user_entity(id: &str, roles: &[&str], privileged: bool) -> Entity {
    let mut builder = EntityBuilder::new(&USER_ETYPE, id);
    for role_name in roles {
        builder = builder.parent(&role_uid(role_name));
    }
    if privileged {
        builder = builder.parent(&role_uid(SUPERUSER_ROLE));
    }
    builder.build()
}

pub fn resource_entity(id: &str) -> Entity {
    EntityBuilder::new(&RESOURCE_ETYPE, id).build()
}

pub fn action_entity(name: &str) -> Entity {
    EntityBuilder::new(&ACTION_ETYPE, name).build()
}
