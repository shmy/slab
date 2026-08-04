# P2 采购+质检+销售 全链路 E2E
# 依赖: P1 物料/客户/供应商/仓库 已就绪
# 覆盖: purchase(PO+收货+退货+发票) / quality(模板+IQC+NC) / sales(SO+发货+发票)

###############################################################################
# 登录
POST {{base_url}}/api/v1/identity/login
Content-Type: application/json
```json
{"phone": "{{e2e_admin_phone}}", "password": "{{e2e_admin_password}}"}
```

HTTP 200
[Captures]
admin_token: jsonpath "$.access_token"

###############################################################################
# 准备主数据
###############################################################################

# 物料分类
POST {{base_url}}/api/v1/item-categories
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"name": "原材料"}
```

HTTP 200
[Captures]
raw_category_id: jsonpath "$.id"

# 物料 - 原料
POST {{base_url}}/api/v1/items
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"name": "E2E-ABS颗粒", "category_id": "{{raw_category_id}}", "item_type": 1, "base_unit": "kg"}
```

HTTP 200
[Captures]
raw_item_id: jsonpath "$.id"
[Asserts]
jsonpath "$.code" matches /^RAW-\d{6}$/

# 物料 - 成品
POST {{base_url}}/api/v1/items
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"name": "E2E-玩具熊", "category_id": "{{raw_category_id}}", "item_type": 5, "base_unit": "pcs"}
```

HTTP 200
[Captures]
fg_item_id: jsonpath "$.id"

# 供应商
POST {{base_url}}/api/v1/suppliers
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"name": "E2E-供应商A"}
```

HTTP 200
[Captures]
supplier_id: jsonpath "$.id"
[Asserts]
jsonpath "$.code" matches /^S-\d{6}$/

# 客户
POST {{base_url}}/api/v1/customers
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"name": "E2E-客户A"}
```

HTTP 200
[Captures]
customer_id: jsonpath "$.id"
[Asserts]
jsonpath "$.code" matches /^C-\d{6}$/

# 仓库 - 原料仓
POST {{base_url}}/api/v1/warehouses
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"name": "E2E-原料仓", "type": 1}
```

HTTP 200
[Captures]
raw_wh_id: jsonpath "$.id"

# 仓库 - 成品仓
POST {{base_url}}/api/v1/warehouses
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"name": "E2E-成品仓", "type": 3}
```

HTTP 200
[Captures]
fg_wh_id: jsonpath "$.id"

###############################################################################
# P2a: 采购订单 → 审批 → 收货 → IQC → 退货
###############################################################################

# 创建采购订单
POST {{base_url}}/api/v1/purchase-orders
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{
  "supplier_id": "{{supplier_id}}",
  "lines": [{"item_id": "{{raw_item_id}}", "quantity": 1000, "unit": "kg", "unit_price": 28}]
}
```

HTTP 200
[Captures]
po_id: jsonpath "$.id"
[Asserts]
jsonpath "$.code" matches /^PO-\d{8}-\d{6}$/

# 验证 PO 详情
GET {{base_url}}/api/v1/purchase-orders/{{po_id}}
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$.order.status" == 0

# 提交审批
POST {{base_url}}/api/v1/purchase-orders/{{po_id}}/submit
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$.submitted" == true

# 主管审批
POST {{base_url}}/api/v1/purchase-orders/{{po_id}}/approve
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$.approved" == true
jsonpath "$.status" == 2

# 经理审批
POST {{base_url}}/api/v1/purchase-orders/{{po_id}}/approve
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$.approved" == true
jsonpath "$.status" == 3

# 获取 order line id
GET {{base_url}}/api/v1/purchase-orders/{{po_id}}
Authorization: Bearer {{admin_token}}

HTTP 200
[Captures]
po_line_id: jsonpath "$.lines[0].id"

# 采购收货
POST {{base_url}}/api/v1/purchase-receipts
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{
  "order_id": "{{po_id}}",
  "lines": [{"order_line_id": "{{po_line_id}}", "item_id": "{{raw_item_id}}", "warehouse_id": "{{raw_wh_id}}", "quantity": 500, "unit_cost": 28}]
}
```

HTTP 200
[Captures]
receipt_id: jsonpath "$.id"
[Asserts]
jsonpath "$.code" matches /^RCV-\d{8}-\d{6}$/

# 验证收货详情
GET {{base_url}}/api/v1/purchase-receipts/{{receipt_id}}
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$.receipt.status" == 1

# 验证库存流水
GET {{base_url}}/api/v1/inventory-transactions?warehouse_id={{raw_wh_id}}
Authorization: Bearer {{admin_token}}

HTTP 200

# 创建 IQC 检验模板
POST {{base_url}}/api/v1/inspection-templates
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"name": "E2E-塑料米IQC", "category": 1, "items": [{"name": "外观", "is_required": true}, {"name": "尺寸", "is_required": false}]}
```

HTTP 200
[Captures]
template_id: jsonpath "$.id"
[Asserts]
jsonpath "$.code" matches /^IQC-\d{6}$/

# 查看模板详情
GET {{base_url}}/api/v1/inspection-templates/{{template_id}}
Authorization: Bearer {{admin_token}}

HTTP 200
[Captures]
tmpl_item_id: jsonpath "$.items[0].id"

# 创建检验单（关联收货）
POST {{base_url}}/api/v1/inspection-orders
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"template_id": "{{template_id}}", "source_type": "purchase_receipt", "source_id": {{receipt_id}}, "item_id": "{{raw_item_id}}", "lot_qty": 500, "inspector": "QC01"}
```

HTTP 200
[Captures]
inspection_id: jsonpath "$.id"

# 查看检验单
GET {{base_url}}/api/v1/inspection-orders/{{inspection_id}}
Authorization: Bearer {{admin_token}}

HTTP 200

# 检验判定通过
POST {{base_url}}/api/v1/inspection-orders/{{inspection_id}}/complete
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"results": [{"template_item_id": "{{tmpl_item_id}}", "result": 1, "actual_value": "OK"}]}
```

HTTP 200

# 创建不合格处理
POST {{base_url}}/api/v1/non-conformances
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"inspection_id": "{{inspection_id}}", "item_id": "{{raw_item_id}}", "quantity": 10, "severity": 2, "disposition": 1}
```

HTTP 200
[Captures]
nc_id: jsonpath "$.id"
[Asserts]
jsonpath "$.code" matches /^NC-\d{8}-\d{6}$/

# 查看不合格处理
GET {{base_url}}/api/v1/non-conformances/{{nc_id}}
Authorization: Bearer {{admin_token}}

HTTP 200

# 采购发票
POST {{base_url}}/api/v1/purchase-invoices
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"order_id": "{{po_id}}", "amount": 14000, "tax_amount": 1820}
```

HTTP 200
[Captures]
inv_id: jsonpath "$.id"
[Asserts]
jsonpath "$.code" matches /^INV-\d{8}-\d{6}$/

# 查看发票（捕获总额，结清时全额支付）
GET {{base_url}}/api/v1/purchase-invoices/{{inv_id}}
Authorization: Bearer {{admin_token}}

HTTP 200
[Captures]
purchase_inv_total: jsonpath "$.invoice.total_amount"

# 采购退货
POST {{base_url}}/api/v1/purchase-returns
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"order_id": "{{po_id}}", "lines": []}
```

HTTP 200
[Captures]
return_id: jsonpath "$.id"

# 查看退货
GET {{base_url}}/api/v1/purchase-returns/{{return_id}}
Authorization: Bearer {{admin_token}}

HTTP 200

# 搜索采购订单
GET {{base_url}}/api/v1/purchase-orders?supplier_id={{supplier_id}}
Authorization: Bearer {{admin_token}}

HTTP 200
###############################################################################
# P2b: 销售订单 → 审批 → 发货 → 发票
###############################################################################

# 初始化成品库存
POST {{base_url}}/api/v1/inventories/initial
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"item_id": "{{fg_item_id}}", "warehouse_id": "{{fg_wh_id}}", "quantity": 5000}
```

HTTP 200

# 创建销售订单
POST {{base_url}}/api/v1/sales-orders
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"customer_id": "{{customer_id}}", "lines": [{"item_id": "{{fg_item_id}}", "quantity": 100, "unit": "pcs", "unit_price": 50}]}
```

HTTP 200
[Captures]
so_id: jsonpath "$.id"

# 查看 SO
GET {{base_url}}/api/v1/sales-orders/{{so_id}}
Authorization: Bearer {{admin_token}}

HTTP 200
[Captures]
so_line_id: jsonpath "$.lines[0].id"

# 提交审批
POST {{base_url}}/api/v1/sales-orders/{{so_id}}/submit
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$.success" == true

# 主管审批
POST {{base_url}}/api/v1/sales-orders/{{so_id}}/approve
Authorization: Bearer {{admin_token}}

HTTP 200

# 经理审批
POST {{base_url}}/api/v1/sales-orders/{{so_id}}/approve
Authorization: Bearer {{admin_token}}

HTTP 200

# 销售发货
POST {{base_url}}/api/v1/sales-deliveries
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"order_id": "{{so_id}}", "lines": [{"order_line_id": "{{so_line_id}}", "item_id": "{{fg_item_id}}", "warehouse_id": "{{fg_wh_id}}", "quantity": 50}]}
```

HTTP 200
[Asserts]
jsonpath "$.code" matches /^DLV-\d{8}-\d{6}$/

# 销售发票
POST {{base_url}}/api/v1/sales-invoices
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"order_id": "{{so_id}}", "amount": 2500, "tax_amount": 325}
```

HTTP 200
[Captures]
sales_inv_id: jsonpath "$.id"
[Asserts]
jsonpath "$.code" matches /^SINV-\d{8}-\d{6}$/

# 结清本文件产生的发票（p4 账龄断言假设全库仅剩自己的未付数据）
# 销售发票无 GET 端点，金额与上方创建请求 amount+tax_amount 保持一致，改动需同步
POST {{base_url}}/api/v1/payments
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"payment_type": 2, "invoice_type": "purchase_invoice", "invoice_id": "{{inv_id}}", "amount": {{purchase_inv_total}}, "payment_method": "bank_transfer"}
```

HTTP 200

POST {{base_url}}/api/v1/payments
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"payment_type": 1, "invoice_type": "sales_invoice", "invoice_id": "{{sales_inv_id}}", "amount": 2825, "payment_method": "bank_transfer"}
```

HTTP 200
