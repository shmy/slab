# P3 生产执行 E2E
# 覆盖: product(BOM+模具) / production(工单+领料+报工+完工入库)

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

# 分类
POST {{base_url}}/api/v1/item-categories
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"name": "原材料"}
```

HTTP 200
[Captures]
raw_cat_id: jsonpath "$.id"

POST {{base_url}}/api/v1/item-categories
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"name": "成品"}
```

HTTP 200
[Captures]
fg_cat_id: jsonpath "$.id"

# 物料
POST {{base_url}}/api/v1/items
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"name": "E2E-ABS", "category_id": "{{raw_cat_id}}", "item_type": 1, "base_unit": "kg"}
```

HTTP 200
[Captures]
raw_id: jsonpath "$.id"

POST {{base_url}}/api/v1/items
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"name": "E2E-螺丝", "category_id": "{{raw_cat_id}}", "item_type": 1, "base_unit": "pcs"}
```

HTTP 200
[Captures]
screw_id: jsonpath "$.id"

POST {{base_url}}/api/v1/items
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"name": "E2E-玩具车", "category_id": "{{fg_cat_id}}", "item_type": 5, "base_unit": "pcs"}
```

HTTP 200
[Captures]
fg_id: jsonpath "$.id"

# 仓库
POST {{base_url}}/api/v1/warehouses
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"name": "E2E-原料仓", "type": 1}
```

HTTP 200
[Captures]
raw_wh: jsonpath "$.id"

POST {{base_url}}/api/v1/warehouses
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"name": "E2E-线边仓", "type": 2}
```

HTTP 200
[Captures]
line_wh: jsonpath "$.id"

POST {{base_url}}/api/v1/warehouses
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"name": "E2E-成品仓", "type": 3}
```

HTTP 200
[Captures]
fg_wh: jsonpath "$.id"

# 初始化原料库存
POST {{base_url}}/api/v1/inventories/initial
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"item_id": "{{raw_id}}", "warehouse_id": "{{raw_wh}}", "quantity": 5000}
```

HTTP 200

POST {{base_url}}/api/v1/inventories/initial
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"item_id": "{{screw_id}}", "warehouse_id": "{{raw_wh}}", "quantity": 10000}
```

HTTP 200

###############################################################################
# 产品: BOM + 模具
###############################################################################

# 创建 BOM
POST {{base_url}}/api/v1/boms
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"name": "E2E-玩具车BOM", "item_id": "{{fg_id}}", "items": [{"item_id": "{{raw_id}}", "quantity": 2, "unit": "kg"}, {"item_id": "{{screw_id}}", "quantity": 8, "unit": "pcs"}]}
```

HTTP 200
[Captures]
bom_id: jsonpath "$.id"
[Asserts]
jsonpath "$.code" matches /^BOM-\d{8}-\d{6}$/

# 查看 BOM
GET {{base_url}}/api/v1/boms/{{bom_id}}
Authorization: Bearer {{admin_token}}

HTTP 200
# 创建模具
POST {{base_url}}/api/v1/molds
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"name": "E2E-车壳模具", "item_id": "{{fg_id}}", "cavity_count": 4, "life_expectancy": 100000}
```

HTTP 200
[Captures]
mold_id: jsonpath "$.id"
[Asserts]
jsonpath "$.code" matches /^MOLD-\d{8}-\d{6}$/

# 查看模具
GET {{base_url}}/api/v1/molds/{{mold_id}}
Authorization: Bearer {{admin_token}}

HTTP 200

###############################################################################
# 生产: 工单 → 下达 → 领料 → 报工 → 完工入库
###############################################################################

# 创建工单
POST {{base_url}}/api/v1/work-orders
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"bom_id": "{{bom_id}}", "item_id": "{{fg_id}}", "planned_qty": 100, "operations": [{"name": "注塑", "sequence": 10}, {"name": "组装", "sequence": 20}]}
```

HTTP 200
[Captures]
wo_id: jsonpath "$.id"
[Asserts]
jsonpath "$.code" matches /^MO-\d{8}-\d{6}$/

# 查看工单（含物料+工序）
GET {{base_url}}/api/v1/work-orders/{{wo_id}}
Authorization: Bearer {{admin_token}}

HTTP 200
[Captures]
wo_mat_id: jsonpath "$.materials[0].id"
wo_op_id: jsonpath "$.operations[0].id"

# 下达工单
POST {{base_url}}/api/v1/work-orders/{{wo_id}}/release
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$.success" == true

# 领料
POST {{base_url}}/api/v1/work-orders/{{wo_id}}/pick
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"items": [{"material_id": "{{wo_mat_id}}", "item_id": "{{raw_id}}", "warehouse_id": "{{raw_wh}}", "quantity": 200}]}
```

HTTP 200
[Asserts]
jsonpath "$.picked" == true

# 工序报工
POST {{base_url}}/api/v1/work-orders/{{wo_id}}/operations/{{wo_op_id}}/report
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"completed_qty": 100, "scrap_qty": 2}
```

HTTP 200
[Asserts]
jsonpath "$.success" == true

# 完工
POST {{base_url}}/api/v1/work-orders/{{wo_id}}/complete
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$.success" == true

# 完工入库
POST {{base_url}}/api/v1/production-receipts
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{"work_order_id": "{{wo_id}}", "warehouse_id": "{{fg_wh}}", "quantity": 98, "batch_number": "B20260724-001"}
```

HTTP 200
[Asserts]
jsonpath "$.code" matches /^PR-\d{8}-\d{6}$/

# 验证库存流水
GET {{base_url}}/api/v1/inventory-transactions?warehouse_id={{raw_wh}}
Authorization: Bearer {{admin_token}}

HTTP 200
