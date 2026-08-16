# Phase 1 主数据 E2E（物料/客户/供应商/仓库/库存）
#
# 依赖：管理员已存在（e2e_admin_phone / e2e_admin_password）
# 响应格式为扁平 JSON，无 data 包装。

###############################################################################
# 管理员登录
POST {{base_url}}/api/v1/identity/login
Content-Type: application/json
```json
{
  "phone": "{{e2e_admin_phone}}",
  "password": "{{e2e_admin_password}}"
}
```

HTTP 200
[Captures]
admin_token: jsonpath "$.access_token"
[Asserts]
jsonpath "$.token_type" == "Bearer"

###############################################################################
# 创建物料分类
POST {{base_url}}/api/v1/item-categories
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{
  "name": "塑料原料"
}
```

HTTP 200
[Captures]
category_id: jsonpath "$.id"
[Asserts]
jsonpath "$.id" matches /^\d+$/

###############################################################################
# 创建子分类
POST {{base_url}}/api/v1/item-categories
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{
  "name": "ABS",
  "parent_id": "{{category_id}}"
}
```

HTTP 200
[Captures]
sub_category_id: jsonpath "$.id"

###############################################################################
# 获取分类树
GET {{base_url}}/api/v1/item-categories/tree
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$[0].name" exists

###############################################################################
# 更新分类
PATCH {{base_url}}/api/v1/item-categories/{{category_id}}
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{
  "name": "塑料原料（主类）"
}
```

HTTP 200
[Asserts]
jsonpath "$.updated" == true

###############################################################################
# 创建物料（原料）
POST {{base_url}}/api/v1/items
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{
  "name": "ABS 塑料米",
  "category_id": "{{category_id}}",
  "item_type": 1,
  "base_unit": "kg"
}
```

HTTP 200
[Captures]
item_id: jsonpath "$.id"
item_code: jsonpath "$.code"
[Asserts]
jsonpath "$.code" matches /^RAW-\d{6}$/

###############################################################################
# 创建物料（产成品）
POST {{base_url}}/api/v1/items
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{
  "name": "遥控赛车（红色）",
  "category_id": "{{category_id}}",
  "item_type": 5,
  "base_unit": "pcs"
}
```

HTTP 200
[Captures]
product_id: jsonpath "$.id"
product_code: jsonpath "$.code"
[Asserts]
jsonpath "$.code" matches /^PRD-\d{6}$/

###############################################################################
# 获取物料详情
GET {{base_url}}/api/v1/items/{{item_id}}
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$.name" == "ABS 塑料米"
jsonpath "$.code" == "{{item_code}}"

###############################################################################
# 搜索物料（按关键字）
GET {{base_url}}/api/v1/items?q=ABS
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$.items[0].name" == "ABS 塑料米"

###############################################################################
# 搜索物料（按类型）
GET {{base_url}}/api/v1/items?item_type=5
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$.items[0].code" matches /^PRD-\d{6}$/

###############################################################################
# 更新物料
PATCH {{base_url}}/api/v1/items/{{item_id}}
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{
  "name": "ABS 塑料米（高抗冲）",
  "spec": "HIPS-1234"
}
```

HTTP 200
[Asserts]
jsonpath "$.updated" == true

###############################################################################
# 删除物料（软删）
DELETE {{base_url}}/api/v1/items/{{item_id}}
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$.deleted" == true

###############################################################################
# 添加单位换算
POST {{base_url}}/api/v1/items/{{item_id}}/units
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{
  "unit": "g",
  "rate": 1000
}
```

HTTP 200
[Captures]
unit_id: jsonpath "$.id"

###############################################################################
# 查询单位换算
GET {{base_url}}/api/v1/items/{{item_id}}/units
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$[0].unit" == "g"
jsonpath "$[0].rate" == 1000

###############################################################################
# 添加成本记录
POST {{base_url}}/api/v1/items/{{item_id}}/costs
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{
  "cost_type": 1,
  "unit_cost": 850
}
```

HTTP 200
[Asserts]
jsonpath "$.id" matches /^\d+$/

###############################################################################
# 查询成本记录
GET {{base_url}}/api/v1/items/{{item_id}}/costs
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$[0].cost_type" == 1
jsonpath "$[0].unit_cost" == 850

###############################################################################
# 删除分类（清理测试数据）
DELETE {{base_url}}/api/v1/item-categories/{{sub_category_id}}
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$.deleted" == true

###############################################################################
# 创建客户
POST {{base_url}}/api/v1/customers
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{
  "name": "上海玩具贸易有限公司",
  "contact_person": "张经理",
  "phone": "13900001111"
}
```

HTTP 200
[Captures]
customer_id: jsonpath "$.id"
customer_code: jsonpath "$.code"
[Asserts]
jsonpath "$.code" matches /^C-\d{6}$/

###############################################################################
# 搜索客户（filter=name 包含 上海）
GET {{base_url}}/api/v1/customers?filter=name=ilike=*上海*
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$.items[0].name" contains "上海"

###############################################################################
# 获取客户详情
GET {{base_url}}/api/v1/customers/{{customer_id}}
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$.name" == "上海玩具贸易有限公司"
jsonpath "$.code" matches /^C-\d{6}$/

###############################################################################
# 更新客户
PATCH {{base_url}}/api/v1/customers/{{customer_id}}
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{
  "contact_person": "李经理"
}
```

HTTP 200
[Asserts]
jsonpath "$.updated" == true

###############################################################################
# 创建供应商
POST {{base_url}}/api/v1/suppliers
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{
  "name": "东莞塑胶原料有限公司",
  "contact_person": "王先生",
  "phone": "13900002222"
}
```

HTTP 200
[Captures]
supplier_id: jsonpath "$.id"
supplier_code: jsonpath "$.code"
[Asserts]
jsonpath "$.code" matches /^S-\d{6}$/

###############################################################################
# 获取供应商详情
GET {{base_url}}/api/v1/suppliers/{{supplier_id}}
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$.name" == "东莞塑胶原料有限公司"
jsonpath "$.code" matches /^S-\d{6}$/

###############################################################################
# 更新供应商
PATCH {{base_url}}/api/v1/suppliers/{{supplier_id}}
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{
  "contact_person": "陈先生"
}
```

HTTP 200
[Asserts]
jsonpath "$.updated" == true

###############################################################################
# 删除供应商
DELETE {{base_url}}/api/v1/suppliers/{{supplier_id}}
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$.deleted" == true

###############################################################################
# 搜索供应商
GET {{base_url}}/api/v1/suppliers?q=东莞
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$.items[0].name" contains "东莞"

###############################################################################
# 删除客户（清理测试数据）
DELETE {{base_url}}/api/v1/customers/{{customer_id}}
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$.deleted" == true

###############################################################################
# 创建仓库
POST {{base_url}}/api/v1/warehouses
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{
  "name": "原料一号仓",
  "type": 1
}
```

HTTP 200
[Captures]
warehouse_id: jsonpath "$.id"
warehouse_code: jsonpath "$.code"
[Asserts]
jsonpath "$.code" matches /^WH-\d{3}$/

###############################################################################
# 查询仓库列表
GET {{base_url}}/api/v1/warehouses
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$[0].name" == "原料一号仓"

###############################################################################
# 更新仓库
PATCH {{base_url}}/api/v1/warehouses/{{warehouse_id}}
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{
  "name": "原料仓（主）"
}
```

HTTP 200
[Asserts]
jsonpath "$.updated" == true

###############################################################################
# 删除仓库
DELETE {{base_url}}/api/v1/warehouses/{{warehouse_id}}
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$.deleted" == true

###############################################################################
# 创建另一个仓库（给库存测试用）
POST {{base_url}}/api/v1/warehouses
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{
  "name": "成品仓",
  "type": 3
}
```

HTTP 200
[Captures]
warehouse_id: jsonpath "$.id"
[Asserts]
jsonpath "$.code" matches /^WH-\d{3}$/

###############################################################################
# 期初库存录入
POST {{base_url}}/api/v1/inventories/initial
Authorization: Bearer {{admin_token}}
Content-Type: application/json
```json
{
  "item_id": "{{item_id}}",
  "warehouse_id": "{{warehouse_id}}",
  "quantity": 500
}
```

HTTP 200
[Asserts]
jsonpath "$.id" matches /^\d+$/

###############################################################################
# 库存台账查询
GET {{base_url}}/api/v1/inventories?warehouse_id={{warehouse_id}}
Authorization: Bearer {{admin_token}}

HTTP 200
[Asserts]
jsonpath "$.items[0].warehouse_id" == {{warehouse_id}}
