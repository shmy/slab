# P5 成本核算 + 报表 + MRP E2E

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
# 准备: 物料分类 + 物料
POST {{base_url}}/api/v1/item-categories
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"name": "原料"}
```
HTTP 200
[Captures]
cat_id: jsonpath "$.id"

POST {{base_url}}/api/v1/items
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"name": "塑料", "category_id": "{{cat_id}}", "item_type": 1, "base_unit": "kg"}
```
HTTP 200
[Captures]
raw_item_id: jsonpath "$.id"

POST {{base_url}}/api/v1/items
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"name": "玩具车", "category_id": "{{cat_id}}", "item_type": 5, "base_unit": "pcs"}
```
HTTP 200
[Captures]
fg_item_id: jsonpath "$.id"

###############################################################################
# 准备: 仓库 + 供应商 + PO（为了触发加权重算）
POST {{base_url}}/api/v1/warehouses
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"code": "P5-WH01", "name": "P5仓", "type": 1}
```
HTTP 200
[Captures]
wh_id: jsonpath "$.id"

POST {{base_url}}/api/v1/suppliers
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"code": "P5-S001", "name": "P5供应商", "phone": "13900009901", "is_active": true}
```
HTTP 200
[Captures]
supplier_id: jsonpath "$.id"

###############################################################################
# 准备: 创建采购订单 → 审批 → 收货（触发加权平均）
POST {{base_url}}/api/v1/purchase-orders
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"supplier_id": "{{supplier_id}}", "currency": "CNY", "lines": [{"item_id": "{{raw_item_id}}", "quantity": 100, "unit": "kg", "unit_price": 50, "line_total": 5000}]}
```
HTTP 200
[Captures]
po_id: jsonpath "$.id"

POST {{base_url}}/api/v1/purchase-orders/{{po_id}}/submit
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{}
```
HTTP 200

POST {{base_url}}/api/v1/purchase-orders/{{po_id}}/approve
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{}
```
HTTP 200

POST {{base_url}}/api/v1/purchase-orders/{{po_id}}/approve
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{}
```
HTTP 200

# 取订单行 ID（收货需要行级引用）
GET {{base_url}}/api/v1/purchase-orders/{{po_id}}
Authorization: Bearer {{admin_token}}

HTTP 200
[Captures]
order_line_id: jsonpath "$.lines[0].id"

POST {{base_url}}/api/v1/purchase-receipts
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"order_id": "{{po_id}}", "lines": [{"order_line_id": "{{order_line_id}}", "item_id": "{{raw_item_id}}", "warehouse_id": "{{wh_id}}", "quantity": 100, "unit_cost": 5000}]}
```
HTTP 200
[Captures]
receipt_id: jsonpath "$.id"

###############################################################################
# 1. 验证加权均价（100kg × 5000 分/kg = 均价 5000 分）
GET {{base_url}}/api/v1/items/{{raw_item_id}}/weighted-cost
Authorization: Bearer {{admin_token}}
HTTP 200
[Asserts]
jsonpath "$.unit_cost" == 5000
jsonpath "$.currency" == "CNY"

###############################################################################
# 2. 创建 BOM + 工单 → 领料 → 查工单物料成本
POST {{base_url}}/api/v1/boms
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"code": "P5-BOM001", "name": "玩具车BOM", "item_id": "{{fg_item_id}}", "total_qty": 1, "items": [{"item_id": "{{raw_item_id}}", "quantity": 2, "unit": "kg"}]}
```
HTTP 200
[Captures]
bom_id: jsonpath "$.id"

# 发布 BOM（MRP 只纳入已发布的 BOM）
POST {{base_url}}/api/v1/boms/{{bom_id}}/release
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{}
```
HTTP 200

POST {{base_url}}/api/v1/work-orders
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"bom_id": "{{bom_id}}", "item_id": "{{fg_item_id}}", "planned_qty": 10, "due_date": "2026-08-01", "operations": [{"name": "注塑", "sequence": 10}]}
```
HTTP 200
[Captures]
wo_id: jsonpath "$.id"

# 下达工单（草稿态不可领料）
POST {{base_url}}/api/v1/work-orders/{{wo_id}}/release
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{}
```
HTTP 200

# 取工单物料 ID
GET {{base_url}}/api/v1/work-orders/{{wo_id}}
Authorization: Bearer {{admin_token}}

HTTP 200
[Captures]
material_id: jsonpath "$.materials[0].id"

POST {{base_url}}/api/v1/work-orders/{{wo_id}}/pick
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"items": [{"material_id": "{{material_id}}", "item_id": "{{raw_item_id}}", "warehouse_id": "{{wh_id}}", "quantity": 20}]}
```
HTTP 200

GET {{base_url}}/api/v1/production/work-orders/{{wo_id}}/material-cost
Authorization: Bearer {{admin_token}}
HTTP 200
[Asserts]
jsonpath "$.total_material_cost" == 100000
jsonpath "$.lines[0].unit_cost" == 5000
jsonpath "$.lines[0].picked_qty" == 20
jsonpath "$.lines[0].line_cost" == 100000



###############################################################################
# 3. MRP: 创建销售订单（玩具车 50 个 = 100 kg 塑料需求）
# 先建客户
POST {{base_url}}/api/v1/customers
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"name": "P5客户", "contact_person": "李经理", "phone": "13900009902"}
```
HTTP 200
[Captures]
cust_id: jsonpath "$.id"

POST {{base_url}}/api/v1/sales-orders
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"customer_id": "{{cust_id}}", "currency": "CNY", "lines": [{"item_id": "{{fg_item_id}}", "quantity": 50, "unit": "pcs", "unit_price": 1000, "line_total": 50000}]}
```
HTTP 200
[Captures]
so_id: jsonpath "$.id"

GET {{base_url}}/api/v1/planning/mrp
Authorization: Bearer {{admin_token}}
HTTP 200
[Asserts]
jsonpath "$[?(@.item_name == '塑料')].gross_demand" == 100
jsonpath "$[?(@.item_name == '塑料')].net_demand" >= 0

###############################################################################
# 4. 创建销售/采购发票 → 查收支表 + 余额
POST {{base_url}}/api/v1/sales-invoices
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"order_id": "{{so_id}}", "amount": 8000, "tax_amount": 2000}
```
HTTP 200
[Captures]
sales_inv_id: jsonpath "$.id"

POST {{base_url}}/api/v1/purchase-invoices
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"order_id": "{{po_id}}", "amount": 4000, "tax_amount": 1000}
```
HTTP 200
[Captures]
purchase_inv_id: jsonpath "$.id"

GET {{base_url}}/api/v1/finance/income-statement?year=2026
Authorization: Bearer {{admin_token}}
HTTP 200
[Asserts]
jsonpath "$.total_income" >= 10000
jsonpath "$.total_expense" >= 5000
jsonpath "$.gross_profit" exists

GET {{base_url}}/api/v1/finance/balances
Authorization: Bearer {{admin_token}}
HTTP 200
[Asserts]
jsonpath "$.ar_balance" >= 0
jsonpath "$.ap_balance" >= 0
