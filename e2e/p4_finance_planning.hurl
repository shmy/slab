# P4 财务 + 计划层 E2E
# 覆盖: finance(收付款+账龄) / planning(再订货预警+采购建议)

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
# 准备: 创建物料分类
POST {{base_url}}/api/v1/item-categories
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"name": "原材料"}
```

HTTP 200
[Captures]
cat_id: jsonpath "$.id"

###############################################################################
# 准备: 创建物料
POST {{base_url}}/api/v1/items
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"name": "钢材", "category_id": "{{cat_id}}", "item_type": 1, "base_unit": "kg"}
```

HTTP 200
[Captures]
item_id: jsonpath "$.id"

# 创建采购类物料
POST {{base_url}}/api/v1/items
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"name": "树脂", "category_id": "{{cat_id}}", "item_type": 3, "base_unit": "kg"}
```

HTTP 200
[Captures]
pur_item_id: jsonpath "$.id"

###############################################################################
# 准备: 创建仓库
POST {{base_url}}/api/v1/warehouses
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"code": "P4-WH01", "name": "P4主仓", "type": 1}
```

HTTP 200
[Captures]
wh_id: jsonpath "$.id"

###############################################################################
# 准备: 初始库存（钢材入库 200kg）
POST {{base_url}}/api/v1/inventories/initial
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"warehouse_id": "{{wh_id}}", "item_id": "{{item_id}}", "quantity": 200}
```

HTTP 200

###############################################################################
# 准备: 创建客户
POST {{base_url}}/api/v1/customers
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"code": "P4-C001", "name": "测试客户", "phone": "13900009001", "is_active": true}
```

HTTP 200
[Captures]
customer_id: jsonpath "$.id"

###############################################################################
# 准备: 创建供应商
POST {{base_url}}/api/v1/suppliers
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"code": "P4-S001", "name": "测试供应商", "phone": "13900009002", "is_active": true}
```

HTTP 200
[Captures]
supplier_id: jsonpath "$.id"

###############################################################################
# 准备: 创建销售订单（用于销售发票）
POST {{base_url}}/api/v1/sales-orders
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"customer_id": "{{customer_id}}", "currency": "CNY", "lines": [{"item_id": "{{item_id}}", "quantity": 10, "unit": "kg", "unit_price": 500, "line_total": 5000}]}
```

HTTP 200
[Captures]
sales_order_id: jsonpath "$.id"

###############################################################################
# 准备: 创建采购订单（用于采购发票 + 在途采购量）
POST {{base_url}}/api/v1/purchase-orders
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"supplier_id": "{{supplier_id}}", "currency": "CNY", "lines": [{"item_id": "{{pur_item_id}}", "quantity": 100, "unit": "kg", "unit_price": 20, "line_total": 2000}]}
```

HTTP 200
[Captures]
po_id: jsonpath "$.id"

###############################################################################
# 准备: 提交并审批采购订单（使其进入 approved 状态，从而被计入在途采购）
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
# 第二次审批使状态达 Approved(3)
POST {{base_url}}/api/v1/purchase-orders/{{po_id}}/approve
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{}
```

HTTP 200

###############################################################################
# 准备: 创建销售发票（用于收款测试 + 账龄测试）
POST {{base_url}}/api/v1/sales-invoices
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"order_id": "{{sales_order_id}}", "amount": 4000, "tax_amount": 1000}
```

HTTP 200
[Captures]
sales_inv_id: jsonpath "$.id"

###############################################################################
# 准备: 创建采购发票（用于付款测试 + 账龄测试）
POST {{base_url}}/api/v1/purchase-invoices
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"order_id": "{{po_id}}", "amount": 1600, "tax_amount": 400}
```

HTTP 200
[Captures]
purchase_inv_id: jsonpath "$.id"

###############################################################################
# 1. 创建收款 → 关联销售发票 → 验证发票已付金额更新
POST {{base_url}}/api/v1/payments
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"payment_type": 1, "invoice_type": "sales_invoice", "invoice_id": "{{sales_inv_id}}", "amount": 3000, "payment_method": "bank_transfer"}
```

HTTP 200
[Asserts]
jsonpath "$.code" matches /^PAY-\d{8}-\d{6}$/
[Captures]
payment_id: jsonpath "$.id"

###############################################################################
# 2. 查询付款单详情
GET {{base_url}}/api/v1/payments/{{payment_id}}
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$.amount" == 3000
jsonpath "$.invoice_type" == "sales_invoice"
jsonpath "$.payment_type" == 1

###############################################################################
# 3. 搜索付款记录
GET {{base_url}}/api/v1/payments
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$.items[0].code" matches /^PAY-\d{8}-\d{6}$/
jsonpath "$.items[0].amount" == 3000

# 按类型筛选
GET {{base_url}}/api/v1/payments?payment_type=1
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$.items[0].payment_type" == 1

###############################################################################
# 4. 创建付款 → 关联采购发票
POST {{base_url}}/api/v1/payments
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"payment_type": 2, "invoice_type": "purchase_invoice", "invoice_id": "{{purchase_inv_id}}", "amount": 2000, "payment_method": "cash"}
```

HTTP 200
[Asserts]
jsonpath "$.code" matches /^PAY-\d{8}-\d{6}$/

###############################################################################
# 5. 账龄分析
GET {{base_url}}/api/v1/finance/aging
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
# AR: 销售发票 5000 - 已收 3000 = 2000 未收
jsonpath "$.ar[0].amount" == 2000
# AP: 采购发票 2000 - 已付 2000 = 0 未付（全部结清，应为空）
jsonpath "$.ap" count == 0

###############################################################################
# 6. 设置物料再订货点 → 触发预警
# 钢材库存 200kg，设 reorder_point=500，应触发预警
# 先更新物料的再订货点
PATCH {{base_url}}/api/v1/items/{{item_id}}
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"reorder_point": 500}
```

HTTP 200

# 查询再订货预警
GET {{base_url}}/api/v1/planning/reorder-alerts
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$[?(@.item_name == '钢材')].shortage" == 300

###############################################################################
# 7. 采购建议清单
# 树脂：安全库存尚未设置，先设置
PATCH {{base_url}}/api/v1/items/{{pur_item_id}}
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"safety_stock": 500}
```

HTTP 200

# 查询采购建议
GET {{base_url}}/api/v1/planning/purchase-suggestions
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
# 树脂库存 0（未入库），安全库存 500，在途 100（采购单100未收货），建议=400
jsonpath "$[?(@.item_name == '树脂')].suggested_qty" exists
jsonpath "$[?(@.item_name == '树脂')].in_transit_qty" == 100
