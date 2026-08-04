-- P3: 生产执行（BOM + 工单 + 模具）

-- BOM
CREATE TABLE boms (
    id BIGINT PRIMARY KEY,
    code VARCHAR(64) UNIQUE NOT NULL,
    name VARCHAR(128) NOT NULL,
    item_id BIGINT NOT NULL REFERENCES items(id),
    version INT NOT NULL DEFAULT 1,
    status SMALLINT NOT NULL DEFAULT 0, -- 0=draft 1=released 2=obsolete
    total_qty BIGINT NOT NULL DEFAULT 1,
    remark TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER set_updated_at_boms BEFORE UPDATE ON boms
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();

-- BOM 物料明细
CREATE TABLE bom_items (
    id BIGINT PRIMARY KEY,
    bom_id BIGINT NOT NULL REFERENCES boms(id),
    item_id BIGINT NOT NULL REFERENCES items(id),
    quantity BIGINT NOT NULL,
    unit VARCHAR(16) NOT NULL,
    wastage_rate BIGINT DEFAULT 0, -- 万分比: 5% = 500
    parent_item_id BIGINT REFERENCES bom_items(id),
    sort_order SMALLINT NOT NULL DEFAULT 0,
    remark TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_bom_items_bom ON bom_items(bom_id);

-- 模具台账
CREATE TABLE molds (
    id BIGINT PRIMARY KEY,
    code VARCHAR(64) UNIQUE NOT NULL,
    name VARCHAR(128) NOT NULL,
    item_id BIGINT NOT NULL REFERENCES items(id),
    cavity_count INT NOT NULL DEFAULT 1,
    life_expectancy BIGINT, -- 预计寿命（模次）
    life_used BIGINT DEFAULT 0, -- 已用模次
    status SMALLINT NOT NULL DEFAULT 0, -- 0=active 1=maintenance 2=retired
    maintenance_cycle INT, -- 保养周期（模次）
    remark TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER set_updated_at_molds BEFORE UPDATE ON molds
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();

-- 模具保养记录
CREATE TABLE mold_maintenance (
    id BIGINT PRIMARY KEY,
    mold_id BIGINT NOT NULL REFERENCES molds(id),
    type SMALLINT NOT NULL, -- 0=regular 1=repair
    description TEXT,
    cost BIGINT DEFAULT 0,
    maintained_at DATE NOT NULL DEFAULT CURRENT_DATE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_mold_maintenance_mold ON mold_maintenance(mold_id);

-- 生产工单
CREATE TABLE work_orders (
    id BIGINT PRIMARY KEY,
    code VARCHAR(64) UNIQUE NOT NULL,
    bom_id BIGINT NOT NULL REFERENCES boms(id),
    item_id BIGINT NOT NULL REFERENCES items(id),
    planned_qty BIGINT NOT NULL,
    completed_qty BIGINT DEFAULT 0,
    scrap_qty BIGINT DEFAULT 0,
    status SMALLINT NOT NULL DEFAULT 0, -- 0=draft 1=released 2=in_progress 3=completed 4=closed
    due_date DATE,
    remark TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER set_updated_at_work_orders BEFORE UPDATE ON work_orders
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();
CREATE INDEX idx_work_orders_item ON work_orders(item_id);
CREATE INDEX idx_work_orders_status ON work_orders(status);

-- 工单物料需求（BOM 展开结果）
CREATE TABLE work_order_materials (
    id BIGINT PRIMARY KEY,
    work_order_id BIGINT NOT NULL REFERENCES work_orders(id),
    item_id BIGINT NOT NULL REFERENCES items(id),
    required_qty BIGINT NOT NULL,
    picked_qty BIGINT DEFAULT 0,
    warehouse_id BIGINT REFERENCES warehouses(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_wo_materials_order ON work_order_materials(work_order_id);

-- 工单工序
CREATE TABLE work_order_operations (
    id BIGINT PRIMARY KEY,
    work_order_id BIGINT NOT NULL REFERENCES work_orders(id),
    name VARCHAR(64) NOT NULL,
    sequence SMALLINT NOT NULL DEFAULT 0,
    planned_qty BIGINT NOT NULL DEFAULT 0,
    completed_qty BIGINT DEFAULT 0,
    scrap_qty BIGINT DEFAULT 0,
    status SMALLINT NOT NULL DEFAULT 0, -- 0=pending 1=in_progress 2=completed
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER set_updated_at_work_order_operations BEFORE UPDATE ON work_order_operations
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();
CREATE INDEX idx_wo_operations_order ON work_order_operations(work_order_id);

-- 完工入库
CREATE TABLE production_receipts (
    id BIGINT PRIMARY KEY,
    code VARCHAR(64) UNIQUE NOT NULL,
    work_order_id BIGINT NOT NULL REFERENCES work_orders(id),
    item_id BIGINT NOT NULL REFERENCES items(id),
    warehouse_id BIGINT NOT NULL REFERENCES warehouses(id),
    quantity BIGINT NOT NULL,
    batch_number VARCHAR(64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_production_receipts_order ON production_receipts(work_order_id);

-- 废品登记
CREATE TABLE scrap_records (
    id BIGINT PRIMARY KEY,
    code VARCHAR(64) UNIQUE NOT NULL,
    work_order_id BIGINT REFERENCES work_orders(id),
    operation_id BIGINT REFERENCES work_order_operations(id),
    item_id BIGINT NOT NULL REFERENCES items(id),
    quantity BIGINT NOT NULL,
    reason TEXT,
    severity SMALLINT DEFAULT 0, -- 0=minor 1=major 2=fatal
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_scrap_records_order ON scrap_records(work_order_id);

-- 序列
CREATE SEQUENCE seq_bom START 1;
CREATE SEQUENCE seq_work_order START 1;
CREATE SEQUENCE seq_production_receipt START 1;
