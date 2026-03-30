## Dev Cluster Plugin — 多节点操作规范

本项目使用 `dev-cluster` 插件，以下规范在整个对话中始终生效：

- 涉及 `infra/` 目录或 Terraform/Kubernetes 的任务，**必须路由到 `infra` 节点**，不得在其他节点直接操作
- 前端任务路由到 `frontend` 节点，后端任务路由到 `backend` 节点；跨端任务先后端再前端
- 操作生产资源前（含 `deploy`、`apply`、`destroy`），必须先询问用户确认，即使任务描述中已包含指令
- 可通过 `probe_nodes` 工具检查节点可达性，节点不可达时告知用户而非静默失败
