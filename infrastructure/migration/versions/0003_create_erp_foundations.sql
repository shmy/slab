-- 物料分类（树形）
CREATE TABLE item_categories (
    id BIGINT PRIMARY KEY,
    name VARCHAR(64) NOT NULL,
    parent_id BIGINT REFERENCES item_categories(id),
    sort_order INT NOT NULL DEFAULT 0,
    is_active BOOLEAN DEFAULT TRUE NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER set_updated_at_item_categories BEFORE UPDATE ON item_categories
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();

-- 物料主数据
CREATE TABLE items (
    id BIGINT PRIMARY KEY,
    code VARCHAR(64) UNIQUE NOT NULL,
    name VARCHAR(128) NOT NULL,
    category_id BIGINT REFERENCES item_categories(id),
    item_type SMALLINT NOT NULL,
    base_unit VARCHAR(16) NOT NULL,
    parent_item_id BIGINT REFERENCES items(id),
    spec VARCHAR(255),
    is_active BOOLEAN DEFAULT TRUE NOT NULL,
    version BIGINT NOT NULL DEFAULT 1,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER set_updated_at_items BEFORE UPDATE ON items
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();

-- 单位换算
CREATE TABLE item_units (
    id BIGINT PRIMARY KEY,
    item_id BIGINT NOT NULL REFERENCES items(id),
    unit VARCHAR(16) NOT NULL,
    rate BIGINT NOT NULL,
    UNIQUE(item_id, unit)
);

-- 物料成本
CREATE TABLE item_costs (
    id BIGINT PRIMARY KEY,
    item_id BIGINT NOT NULL REFERENCES items(id),
    cost_type SMALLINT NOT NULL,
    unit_cost BIGINT NOT NULL,
    currency VARCHAR(3) DEFAULT 'CNY',
    effective_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    is_current BOOLEAN DEFAULT FALSE NOT NULL,
    remark VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_item_costs_current ON item_costs(item_id, cost_type, is_current)
    WHERE is_current = TRUE;

-- 客户
CREATE TABLE customers (
    id BIGINT PRIMARY KEY,
    code VARCHAR(64) UNIQUE NOT NULL,
    name VARCHAR(128) NOT NULL,
    contact_person VARCHAR(64),
    phone VARCHAR(20),
    address TEXT,
    payment_terms VARCHAR(64),
    is_active BOOLEAN DEFAULT TRUE NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER set_updated_at_customers BEFORE UPDATE ON customers
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();

-- 供应商
CREATE TABLE suppliers (
    id BIGINT PRIMARY KEY,
    code VARCHAR(64) UNIQUE NOT NULL,
    name VARCHAR(128) NOT NULL,
    contact_person VARCHAR(64),
    phone VARCHAR(20),
    address TEXT,
    payment_terms VARCHAR(64),
    is_active BOOLEAN DEFAULT TRUE NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER set_updated_at_suppliers BEFORE UPDATE ON suppliers
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();

-- 仓库
CREATE TABLE warehouses (
    id BIGINT PRIMARY KEY,
    code VARCHAR(32) UNIQUE NOT NULL,
    name VARCHAR(128) NOT NULL,
    type SMALLINT NOT NULL, -- 1=原料仓 2=半成品仓 3=成品仓 4=包材仓 5=消耗品仓
    is_active BOOLEAN DEFAULT TRUE NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER set_updated_at_warehouses BEFORE UPDATE ON warehouses
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();

-- 库存台账
CREATE TABLE inventories (
    id BIGINT PRIMARY KEY,
    item_id BIGINT NOT NULL REFERENCES items(id),
    warehouse_id BIGINT NOT NULL REFERENCES warehouses(id),
    quantity DECIMAL(12,3) NOT NULL DEFAULT 0,
    locked_qty DECIMAL(12,3) NOT NULL DEFAULT 0,
    version BIGINT NOT NULL DEFAULT 1,
    UNIQUE(item_id, warehouse_id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER set_updated_at_inventories BEFORE UPDATE ON inventories
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();

-- 编码序列
CREATE SEQUENCE seq_item_raw START 1;
CREATE SEQUENCE seq_item_pur START 1;
CREATE SEQUENCE seq_item_mft START 1;
CREATE SEQUENCE seq_item_sub START 1;
CREATE SEQUENCE seq_item_prd START 1;
CREATE SEQUENCE seq_item_pkg START 1;
CREATE SEQUENCE seq_item_con START 1;
CREATE SEQUENCE seq_customer START 1;
CREATE SEQUENCE seq_supplier START 1;
CREATE SEQUENCE seq_warehouse START 1;
