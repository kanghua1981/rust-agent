## Project Stats Plugin — 上下文规范

本项目使用 `project-stats` 插件，以下规范在整个对话中始终生效：

- 分析 Git 历史时，优先使用 `git_log` 工具获取结构化数据，不要直接执行裸 git 命令
- 统计代码规模时使用 `word_count` 工具，排除 `target/`、`node_modules/`、`.git/` 目录
- 所有统计结果以 JSON 格式展示，便于后续处理
