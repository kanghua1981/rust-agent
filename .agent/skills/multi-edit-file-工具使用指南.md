---
name: Multi Edit File 工具使用指南
description: 如何正确使用 multi_edit_file 工具，避免常见错误和失败
---

# Multi Edit File 工具使用指南

# Multi Edit File 工具使用指南

## 工具概述
`multi_edit_file` 工具允许你在单个操作中对文件应用多个查找-替换编辑。这对于批量修改代码或配置文件非常有用，可以减少 LLM 往返次数。

## 工作原理
1. 编辑按顺序应用（从上到下）
2. 每个 `old_string` 必须在应用时在文件中**精确匹配一次**
3. 如果 `old_string` 未找到或找到多次，该编辑将失败
4. 编辑失败不会阻止后续编辑的执行

## 常见失败原因

### 1. 编辑之间的依赖关系
**问题**：第一个编辑的 `new_string` 包含第二个编辑的 `old_string`。
**示例**：
```json
{
  "edits": [
    {"old_string": "def", "new_string": "def def"},
    {"old_string": "def", "new_string": "xyz"}  // 失败！现在有2个"def"
  ]
}
```
**解决方案**：
- 确保编辑之间没有重叠
- 或者使用不同的匹配字符串

### 2. 字符串不精确匹配
**问题**：`old_string` 与文件内容不完全匹配（空格、制表符、换行符）。
**示例**：文件中有 `"hello world"`，但尝试匹配 `"hello  world"`（两个空格）。
**解决方案**：
- 使用 `read_file` 工具查看确切内容
- 复制粘贴确切的字符串

### 3. 编辑顺序错误
**问题**：需要先删除的行被后续编辑引用。
**解决方案**：
- 仔细规划编辑顺序
- 先进行删除，再进行添加/修改

## 最佳实践

### 1. 先读取文件
```json
{
  "tool": "read_file",
  "path": "目标文件路径"
}
```

### 2. 验证编辑
在应用编辑前，检查：
- 每个 `old_string` 是否只出现一次
- 编辑之间是否有冲突
- 编辑顺序是否合理

### 3. 使用更具体的匹配
避免使用短字符串，使用更具体的上下文：
```json
// 不好
{"old_string": "config", "new_string": "settings"}

// 好
{"old_string": "const config = {", "new_string": "const settings = {"}
```

### 4. 分步进行
如果编辑复杂，考虑：
1. 先应用不冲突的编辑
2. 检查结果
3. 应用剩余的编辑

## 调试技巧

### 1. 检查错误信息
如果编辑失败，错误信息会显示：
- 哪个编辑失败了（索引）
- 失败原因（未找到、找到多次等）

### 2. 使用单个编辑测试
对于复杂修改，先用 `edit_file` 测试单个编辑。

### 3. 查看中间状态
如果多个编辑失败，可能是之前的编辑改变了文件状态。考虑：
1. 应用部分编辑
2. 查看文件
3. 调整剩余编辑

## 示例

### 成功示例
```json
{
  "path": "src/main.rs",
  "edits": [
    {
      "old_string": "fn old_function() {\n    println!(\"old\");\n}",
      "new_string": "fn new_function() {\n    println!(\"new\");\n}"
    },
    {
      "old_string": "    old_function();",
      "new_string": "    new_function();"
    }
  ]
}
```

### 失败示例及修复
**问题**：编辑冲突
```json
{
  "edits": [
    {"old_string": "foo", "new_string": "foo bar"},
    {"old_string": "bar", "new_string": "baz"}  // 冲突！
  ]
}
```

**修复**：调整顺序或使用不同匹配
```json
{
  "edits": [
    {"old_string": "bar", "new_string": "baz"},
    {"old_string": "foo", "new_string": "foo bar"}
  ]
}
```

## 替代方案
如果 `multi_edit_file` 持续失败：
1. 使用多个 `edit_file` 调用
2. 使用 `write_file` 完全重写文件
3. 使用 `run_command` 调用 sed/awk 进行批量编辑

记住：`multi_edit_file` 是一个强大的工具，但需要仔细规划。当不确定时，分步进行更安全。
