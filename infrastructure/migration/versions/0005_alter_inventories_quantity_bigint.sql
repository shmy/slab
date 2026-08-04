-- P2a: 将 inventories.quantity/locked_qty 改为 BIGINT（与 inventory_transactions 统一）
ALTER TABLE inventories
    ALTER COLUMN quantity TYPE BIGINT USING CAST(quantity * 1000 AS BIGINT),
    ALTER COLUMN locked_qty TYPE BIGINT USING CAST(locked_qty * 1000 AS BIGINT);
