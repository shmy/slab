-- P4: 财务管控 + 计划层

-- 付款/收款记录
CREATE TABLE payments (
    id BIGINT PRIMARY KEY,
    code VARCHAR(64) UNIQUE NOT NULL,
    payment_type SMALLINT NOT NULL, -- 1=AR(收款) 2=AP(付款)
    invoice_type VARCHAR(32) NOT NULL, -- 'purchase_invoice' / 'sales_invoice'
    invoice_id BIGINT NOT NULL,
    amount BIGINT NOT NULL, -- 分
    payment_date DATE NOT NULL DEFAULT CURRENT_DATE,
    payment_method VARCHAR(32),
    remark TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE TRIGGER set_updated_at_payments BEFORE UPDATE ON payments
    FOR EACH ROW EXECUTE PROCEDURE fn_set_updated_at();
CREATE INDEX idx_payments_invoice ON payments(invoice_type, invoice_id);
CREATE INDEX idx_payments_date ON payments(payment_date);

-- 发票已付金额
ALTER TABLE sales_invoices ADD COLUMN paid_amount BIGINT NOT NULL DEFAULT 0;
ALTER TABLE purchase_invoices ADD COLUMN paid_amount BIGINT NOT NULL DEFAULT 0;

-- 物料再订货点 / 安全库存
ALTER TABLE items ADD COLUMN reorder_point BIGINT NOT NULL DEFAULT 0;
ALTER TABLE items ADD COLUMN safety_stock BIGINT NOT NULL DEFAULT 0;

CREATE SEQUENCE seq_payment START 1;
