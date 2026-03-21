# Rust Agent Web UI 安装指南

## 两种版本选择

### 1. 简化版（当前默认）
- 最小依赖，快速启动
- 基础聊天界面
- 演示核心功能
- 适合快速体验

### 2. 完整版
- 所有功能完整
- 文件浏览器
- 工具监控面板
- 代码高亮
- 更好的 UI 体验

## 安装步骤

### 简化版（已配置）
```bash
cd web-ui
./start-simple.sh
```

### 升级到完整版
```bash
cd web-ui

# 备份当前配置
cp package.json package.json.simple

# 安装完整依赖
npm install --save \
  zustand \
  react-markdown \
  prismjs \
  react-syntax-highlighter \
  lucide-react \
  autoprefixer \
  postcss \
  tailwindcss \
  @typescript-eslint/eslint-plugin \
  @typescript-eslint/parser \
  eslint \
  eslint-plugin-react-hooks \
  eslint-plugin-react-refresh

# 恢复完整版 package.json
# 需要从项目仓库获取完整版 package.json
# 或手动添加上述依赖到 package.json

npm install
npm run dev
```

## 故障排除

### Bus error (core dumped)
**原因**：内存不足或依赖冲突
**解决方案**：
1. 使用简化版（当前已解决）
2. 增加系统交换空间
3. 清理 npm 缓存：`npm cache clean --force`
4. 删除 node_modules 重新安装：`rm -rf node_modules && npm install`

### 依赖安装缓慢
**解决方案**：
```bash
# 使用淘宝镜像
npm config set registry https://registry.npmmirror.com

# 或使用 cnpm
npm install -g cnpm --registry=https://registry.npmmirror.com
cnpm install
```

### 端口被占用
**解决方案**：
```bash
# 修改 vite 配置端口
# 在 vite.config.ts 中修改 server.port
npm run dev -- --port 3001
```

## 完整功能清单

### 已实现（简化版）
- ✅ 基础聊天界面
- ✅ 连接状态管理
- ✅ 消息发送/接收
- ✅ 响应式布局

### 待实现（需要完整版）
- ⏳ 文件浏览器
- ⏳ 工具调用监控
- ⏳ 代码语法高亮
- ⏳ 设置面板
- ⏳ WebSocket 实时通信
- ⏳ 工作目录管理

## 开发建议

### 逐步升级
1. 先使用简化版确保基础功能正常
2. 逐步添加需要的功能模块
3. 测试每个新增依赖的兼容性

### 性能优化
- 使用代码分割（Code Splitting）
- 实现虚拟滚动（大量消息时）
- 优化 WebSocket 消息处理
- 使用 React.memo 减少重渲染

### 安全考虑
- 生产环境使用 HTTPS/WSS
- 实现身份验证
- 限制文件访问权限
- 添加请求频率限制

## 获取帮助

1. **查看日志**：浏览器开发者工具 Console 面板
2. **检查网络**：Network 面板查看 WebSocket 连接
3. **验证配置**：确保 Rust Agent 服务器正常运行
4. **查阅文档**：参考项目 README 和技能文档

## 贡献指南

欢迎贡献代码！建议流程：
1. Fork 项目仓库
2. 创建功能分支
3. 实现功能并测试
4. 提交 Pull Request
5. 更新相关文档