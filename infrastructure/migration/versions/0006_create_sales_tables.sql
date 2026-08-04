-- P2b: 销售出库

-- 销售订单
CREATE TABLE sales_orders (
    id BIGINT PRIMARY KEY,
    code VARCHAR(64) UNIQUE NOT NULL,
    customer_id BIGINT NOT NULL REFERENCES customers(id),
    status SMALLINT NOT NULL DEFAULT 0,
    order_date DATE NOT NULL DEFAULT CURRENT_DATE,
    currency VARCHAR(3) NOT NULL DEFAULT 'CNY',
    total_amount BIGINT NOT NULL DEFAULT 0,
    remark TEXT,
    created_by BIGINT REFERENCES accounts(id),
    approved_by BIGINT,
    approved_at TIMESTAMPTZ,
    rejected_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER set_updated_at_sales_orders BEFORE UPDATE ON sales_orders
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();
CREATE INDEX idx_sales_orders_customer ON sales_orders(customer_id);
CREATE INDEX idx_sales_orders_status ON sales_orders(status);

-- 销售订单行
CREATE TABLE sales_order_lines (
    id BIGINT PRIMARY KEY,
    order_id BIGINT NOT NULL REFERENCES sales_orders(id),
    line_no SMALLINT NOT NULL DEFAULT 0,
    item_id BIGINT NOT NULL REFERENCES items(id),
    quantity BIGINT NOT NULL,
    unit VARCHAR(16) NOT NULL,
    unit_price BIGINT NOT NULL DEFAULT 0,
    line_total BIGINT NOT NULL DEFAULT 0,
    delivered_qty BIGINT NOT NULL DEFAULT 0,
    returned_qty BIGINT NOT NULL DEFAULT 0,
    closed BOOLEAN NOT NULL DEFAULT FALSE,
    remark TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER set_updated_at_sales_order_lines BEFORE UPDATE ON sales_order_lines
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();
CREATE INDEX idx_sales_order_lines_order ON sales_order_lines(order_id);

-- 销售发货
CREATE TABLE sales_deliveries (
    id BIGINT PRIMARY KEY,
    code VARCHAR(64) UNIQUE NOT NULL,
    order_id BIGINT NOT NULL REFERENCES sales_orders(id),
    customer_id BIGINT NOT NULL REFERENCES customers(id),
    delivery_date DATE NOT NULL DEFAULT CURRENT_DATE,
    status SMALLINT NOT NULL DEFAULT 0,
    remark TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER set_updated_at_sales_deliveries BEFORE UPDATE ON sales_deliveries
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();
CREATE INDEX idx_sales_deliveries_order ON sales_deliveries(order_id);

-- 销售发货行
CREATE TABLE sales_delivery_lines (
    id BIGINT PRIMARY KEY,
    delivery_id BIGINT NOT NULL REFERENCES sales_deliveries(id),
    order_line_id BIGINT NOT NULL REFERENCES sales_order_lines(id),
    item_id BIGINT NOT NULL REFERENCES items(id),
    warehouse_id BIGINT NOT NULL REFERENCES warehouses(id),
    quantity BIGINT NOT NULL,
    batch_number VARCHAR(64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_sales_delivery_lines_delivery ON sales_delivery_lines(delivery_id);

-- 销售退货
CREATE TABLE sales_returns (
    id BIGINT PRIMARY KEY,
    code VARCHAR(64) UNIQUE NOT NULL,
    order_id BIGINT NOT NULL REFERENCES sales_orders(id),
    customer_id BIGINT NOT NULL REFERENCES customers(id),
    return_date DATE NOT NULL DEFAULT CURRENT_DATE,
    status SMALLINT NOT NULL DEFAULT 0,
    reason TEXT,
    remark TEXT,
    approved_by BIGINT,
    approved_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER set_updated_at_sales_returns BEFORE UPDATE ON sales_returns
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();
CREATE INDEX idx_sales_returns_order ON sales_returns(order_id);

-- 销售退货行
CREATE TABLE sales_return_lines (
    id BIGINT PRIMARY KEY,
    return_id BIGINT NOT NULL REFERENCES sales_returns(id),
    delivery_line_id BIGINT NOT NULL REFERENCES sales_delivery_lines(id),
    item_id BIGINT NOT NULL REFERENCES items(id),
    quantity BIGINT NOT NULL,
    reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_sales_return_lines_return ON sales_return_lines(return_id);

-- 销售发票
CREATE TABLE sales_invoices (
    id BIGINT PRIMARY KEY,
    code VARCHAR(64) UNIQUE NOT NULL,
    order_id BIGINT NOT NULL REFERENCES sales_orders(id),
    customer_id BIGINT NOT NULL REFERENCES customers(id),
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
CREATE TRIGGER set_updated_at_sales_invoices BEFORE UPDATE ON sales_invoices
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();
CREATE INDEX idx_sales_invoices_order ON sales_invoices(order_id);

-- 序列
CREATE SEQUENCE seq_sales_order START 1;
CREATE SEQUENCE seq_sales_delivery START 1;
CREATE SEQUENCE seq_sales_return START 1;
CREATE SEQUENCE seq_sales_invoice START 1;
