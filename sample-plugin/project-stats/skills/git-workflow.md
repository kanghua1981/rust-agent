# Git 工作流技能

本技能说明如何配合 `git_log` 工具高效分析项目历史。

## 可用工具

### git_log
查询 Git 提交历史。

**常用场景：**

```
# 查看最近 10 条提交
git_log(limit=10)

# 仅看某作者的提交
git_log(author="alice@example.com", limit=20)

# 查看某文件的变更历史
git_log(path="src/main.rs", limit=15)

# 查看某日期后的提交
git_log(since="2024-01-01", limit=50)
```

### word_count
统计代码规模。

```
# 统计整个项目（自动排除 target/node_modules）
word_count(path=".")

# 只统计 Rust 源码
word_count(path="src", ext="rs")

# 统计单个文件
word_count(path="src/main.rs")
```

## 常见分析任务

### 最近一周谁改动最多？
1. 用 `git_log(since="7 days ago", limit=200)` 获取提交列表
2. 按 `author` 字段分组统计

### 某功能模块规模多大？
1. 用 `word_count(path="src/feature_x", ext="rs")`
2. 和其他模块对比

### 新人上手时应关注哪些文件？
1. 用 `git_log(limit=100)` 看近期活跃提交
2. 统计哪些路径出现频次最高
