-- P2a: 采购 + 质检 + 库存升级

-- 采购订单
CREATE TABLE purchase_orders (
    id BIGINT PRIMARY KEY,
    code VARCHAR(64) UNIQUE NOT NULL,
    supplier_id BIGINT NOT NULL REFERENCES suppliers(id),
    status SMALLINT NOT NULL DEFAULT 0, -- ApprovalStatus
    order_date DATE NOT NULL DEFAULT CURRENT_DATE,
    expected_delivery_date DATE,
    currency VARCHAR(3) NOT NULL DEFAULT 'CNY',
    total_amount BIGINT NOT NULL DEFAULT 0,
    payment_terms VARCHAR(64),
    remark TEXT,
    created_by BIGINT REFERENCES accounts(id),
    approved_by BIGINT,
    approved_at TIMESTAMPTZ,
    rejected_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER set_updated_at_purchase_orders BEFORE UPDATE ON purchase_orders
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();
CREATE INDEX idx_purchase_orders_supplier ON purchase_orders(supplier_id);
CREATE INDEX idx_purchase_orders_status ON purchase_orders(status);

-- 采购订单行
CREATE TABLE purchase_order_lines (
    id BIGINT PRIMARY KEY,
    order_id BIGINT NOT NULL REFERENCES purchase_orders(id),
    line_no SMALLINT NOT NULL DEFAULT 0,
    item_id BIGINT NOT NULL REFERENCES items(id),
    quantity BIGINT NOT NULL,
    unit VARCHAR(16) NOT NULL,
    unit_price BIGINT NOT NULL DEFAULT 0,
    line_total BIGINT NOT NULL DEFAULT 0,
    received_qty BIGINT NOT NULL DEFAULT 0,
    returned_qty BIGINT NOT NULL DEFAULT 0,
    closed BOOLEAN NOT NULL DEFAULT FALSE,
    remark TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER set_updated_at_purchase_order_lines BEFORE UPDATE ON purchase_order_lines
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();
CREATE INDEX idx_purchase_order_lines_order ON purchase_order_lines(order_id);

-- 采购收货
CREATE TABLE purchase_receipts (
    id BIGINT PRIMARY KEY,
    code VARCHAR(64) UNIQUE NOT NULL,
    order_id BIGINT NOT NULL REFERENCES purchase_orders(id),
    supplier_id BIGINT NOT NULL REFERENCES suppliers(id),
    receipt_date DATE NOT NULL DEFAULT CURRENT_DATE,
    status SMALLINT NOT NULL DEFAULT 0,
    remark TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER set_updated_at_purchase_receipts BEFORE UPDATE ON purchase_receipts
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();
CREATE INDEX idx_purchase_receipts_order ON purchase_receipts(order_id);

-- 采购收货行
CREATE TABLE purchase_receipt_lines (
    id BIGINT PRIMARY KEY,
    receipt_id BIGINT NOT NULL REFERENCES purchase_receipts(id),
    order_line_id BIGINT NOT NULL REFERENCES purchase_order_lines(id),
    item_id BIGINT NOT NULL REFERENCES items(id),
    warehouse_id BIGINT NOT NULL REFERENCES warehouses(id),
    quantity BIGINT NOT NULL,
    batch_number VARCHAR(64),
    unit_cost BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_purchase_receipt_lines_receipt ON purchase_receipt_lines(receipt_id);

-- 采购退货
CREATE TABLE purchase_returns (
    id BIGINT PRIMARY KEY,
    code VARCHAR(64) UNIQUE NOT NULL,
    order_id BIGINT NOT NULL REFERENCES purchase_orders(id),
    supplier_id BIGINT NOT NULL REFERENCES suppliers(id),
    return_date DATE NOT NULL DEFAULT CURRENT_DATE,
    status SMALLINT NOT NULL DEFAULT 0, -- ApprovalStatus
    reason TEXT,
    remark TEXT,
    approved_by BIGINT,
    approved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER set_updated_at_purchase_returns BEFORE UPDATE ON purchase_returns
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();
CREATE INDEX idx_purchase_returns_order ON purchase_returns(order_id);

-- 采购退货行
CREATE TABLE purchase_return_lines (
    id BIGINT PRIMARY KEY,
    return_id BIGINT NOT NULL REFERENCES purchase_returns(id),
    receipt_line_id BIGINT NOT NULL REFERENCES purchase_receipt_lines(id),
    item_id BIGINT NOT NULL REFERENCES items(id),
    quantity BIGINT NOT NULL,
    reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_purchase_return_lines_return ON purchase_return_lines(return_id);

-- 采购发票
CREATE TABLE purchase_invoices (
    id BIGINT PRIMARY KEY,
    code VARCHAR(64) UNIQUE NOT NULL,
    order_id BIGINT NOT NULL REFERENCES purchase_orders(id),
    supplier_id BIGINT NOT NULL REFERENCES suppliers(id),
    invoice_number VARCHAR(64),
    invoice_date DATE,
    amount BIGINT NOT NULL DEFAULT 0,
    tax_amount BIGINT NOT NULL DEFAULT 0,
    total_amount BIGINT NOT NULL DEFAULT 0,
    status SMALLINT NOT NULL DEFAULT 0,
    remark TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER set_updated_at_purchase_invoices BEFORE UPDATE ON purchase_invoices
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();
CREATE INDEX idx_purchase_invoices_order ON purchase_invoices(order_id);

-- 检验模板
CREATE TABLE inspection_templates (
    id BIGINT PRIMARY KEY,
    code VARCHAR(64) UNIQUE NOT NULL,
    name VARCHAR(128) NOT NULL,
    category SMALLINT NOT NULL, -- 1=IQC 2=IPQC 3=OQC
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER set_updated_at_inspection_templates BEFORE UPDATE ON inspection_templates
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();

-- 检验模板项
CREATE TABLE inspection_template_items (
    id BIGINT PRIMARY KEY,
    template_id BIGINT NOT NULL REFERENCES inspection_templates(id),
    name VARCHAR(128) NOT NULL,
    specification VARCHAR(255),
    tolerance_upper VARCHAR(64),
    tolerance_lower VARCHAR(64),
    method VARCHAR(255),
    is_required BOOLEAN NOT NULL DEFAULT TRUE,
    sort_order SMALLINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_inspection_template_items_template ON inspection_template_items(template_id);

-- 检验单
CREATE TABLE inspection_orders (
    id BIGINT PRIMARY KEY,
    code VARCHAR(64) UNIQUE NOT NULL,
    template_id BIGINT REFERENCES inspection_templates(id),
    source_type VARCHAR(32) NOT NULL, -- purchase_receipt / production / warehouse
    source_id BIGINT NOT NULL,
    item_id BIGINT NOT NULL REFERENCES items(id),
    lot_qty BIGINT NOT NULL DEFAULT 0,
    sample_qty BIGINT NOT NULL DEFAULT 0,
    inspector VARCHAR(64),
    result SMALLINT, -- NULL=待检 1=pass 2=fail 3=conditional
    status SMALLINT NOT NULL DEFAULT 0,
    inspected_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER set_updated_at_inspection_orders BEFORE UPDATE ON inspection_orders
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();
CREATE INDEX idx_inspection_orders_source ON inspection_orders(source_type, source_id);

-- 检验结果
CREATE TABLE inspection_results (
    id BIGINT PRIMARY KEY,
    inspection_id BIGINT NOT NULL REFERENCES inspection_orders(id),
    template_item_id BIGINT NOT NULL REFERENCES inspection_template_items(id),
    result SMALLINT NOT NULL, -- 1=pass 2=fail
    actual_value VARCHAR(255),
    remark TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_inspection_results_inspection ON inspection_results(inspection_id);

-- 不合格处理
CREATE TABLE non_conformances (
    id BIGINT PRIMARY KEY,
    code VARCHAR(64) UNIQUE NOT NULL,
    inspection_id BIGINT REFERENCES inspection_orders(id),
    item_id BIGINT NOT NULL REFERENCES items(id),
    quantity BIGINT NOT NULL DEFAULT 0,
    severity SMALLINT NOT NULL, -- 1=critical 2=major 3=minor
    disposition SMALLINT, -- 1=return 2=rework 3=accept
    status SMALLINT NOT NULL DEFAULT 0, -- ApprovalStatus
    remark TEXT,
    approved_by BIGINT,
    approved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER set_updated_at_non_conformances BEFORE UPDATE ON non_conformances
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();
CREATE INDEX idx_non_conformances_inspection ON non_conformances(inspection_id);

-- 库存流水
CREATE TABLE inventory_transactions (
    id BIGINT PRIMARY KEY,
    item_id BIGINT NOT NULL REFERENCES items(id),
    warehouse_id BIGINT NOT NULL REFERENCES warehouses(id),
    transaction_type SMALLINT NOT NULL, -- 1=receipt 2=issue 3=transfer_in 4=transfer_out 5=adjustment_in 6=adjustment_out
    quantity BIGINT NOT NULL,
    batch_number VARCHAR(64),
    reference_type VARCHAR(32) NOT NULL,
    reference_id BIGINT NOT NULL,
    before_quantity BIGINT NOT NULL DEFAULT 0,
    after_quantity BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_inventory_transactions_item ON inventory_transactions(item_id, warehouse_id);
CREATE INDEX idx_inventory_transactions_ref ON inventory_transactions(reference_type, reference_id);

-- 盘点单
CREATE TABLE inventory_checks (
    id BIGINT PRIMARY KEY,
    code VARCHAR(64) UNIQUE NOT NULL,
    warehouse_id BIGINT NOT NULL REFERENCES warehouses(id),
    status SMALLINT NOT NULL DEFAULT 0, -- ApprovalStatus
    plan_date DATE NOT NULL,
    actual_date DATE,
    remark TEXT,
    approved_by BIGINT,
    approved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER set_updated_at_inventory_checks BEFORE UPDATE ON inventory_checks
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();

-- 盘点行
CREATE TABLE inventory_check_items (
    id BIGINT PRIMARY KEY,
    check_id BIGINT NOT NULL REFERENCES inventory_checks(id),
    item_id BIGINT NOT NULL REFERENCES items(id),
    book_qty BIGINT NOT NULL DEFAULT 0,
    actual_qty BIGINT NOT NULL DEFAULT 0,
    diff_qty BIGINT NOT NULL DEFAULT 0,
    remark TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_inventory_check_items_check ON inventory_check_items(check_id);

-- 库存调拨
CREATE TABLE stock_transfers (
    id BIGINT PRIMARY KEY,
    code VARCHAR(64) UNIQUE NOT NULL,
    from_warehouse_id BIGINT NOT NULL REFERENCES warehouses(id),
    to_warehouse_id BIGINT NOT NULL REFERENCES warehouses(id),
    status SMALLINT NOT NULL DEFAULT 0, -- ApprovalStatus
    transfer_date DATE NOT NULL,
    remark TEXT,
    approved_by BIGINT,
    approved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER set_updated_at_stock_transfers BEFORE UPDATE ON stock_transfers
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();

-- 调拨行
CREATE TABLE stock_transfer_items (
    id BIGINT PRIMARY KEY,
    transfer_id BIGINT NOT NULL REFERENCES stock_transfers(id),
    item_id BIGINT NOT NULL REFERENCES items(id),
    quantity BIGINT NOT NULL,
    batch_number VARCHAR(64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_stock_transfer_items_transfer ON stock_transfer_items(transfer_id);

-- 序列
CREATE SEQUENCE seq_purchase_order START 1;
CREATE SEQUENCE seq_purchase_receipt START 1;
CREATE SEQUENCE seq_purchase_return START 1;
CREATE SEQUENCE seq_purchase_invoice START 1;
CREATE SEQUENCE seq_inspection_order START 1;
CREATE SEQUENCE seq_non_conformance START 1;
CREATE SEQUENCE seq_inventory_check START 1;
CREATE SEQUENCE seq_stock_transfer START 1;
