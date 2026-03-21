# Web UI 部署指南

> 这是一个前端应用，不是 npm 库。推荐使用静态文件部署，无需发布到 npm。

## 📦 快速部署（推荐）

### 方案 1：内嵌到 Rust 服务器（零配置）

**最简单的方式**是将 Web UI 直接嵌入到 Rust Agent 服务器：

```bash
# 1. 构建前端
cd web-ui
npm run build

# 2. Rust 服务器会自动从 dist/ 目录提供静态文件
cd ..
./target/release/agent --mode server --port 9527
```

访问 `http://localhost:9527` 即可使用。

---

### 方案 2：独立静态文件服务器

#### 使用 Python（开发/测试）

```bash
cd web-ui/dist
python3 -m http.server 3000
```

访问 `http://localhost:3000`

#### 使用 Node.js serve

```bash
npm install -g serve
cd web-ui/dist
serve -s . -p 3000
```

#### 使用 Nginx（生产环境）

创建 `/etc/nginx/sites-available/rust-agent-ui`:

```nginx
server {
    listen 80;
    server_name your-domain.com;
    root /path/to/rust_agent/web-ui/dist;
    index index.html;

    location / {
        try_files $uri $uri/ /index.html;
    }

    # 代理到 Rust Agent WebSocket 服务器
    location /ws {
        proxy_pass http://localhost:9527;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
        proxy_set_header Host $host;
    }
}
```

#### 使用 Caddy（自动 HTTPS）

创建 `Caddyfile`:

```
your-domain.com {
    root * /path/to/rust_agent/web-ui/dist
    file_server
    try_files {path} /index.html

    reverse_proxy /ws localhost:9527 {
        header_up Upgrade {http.request.header.Upgrade}
        header_up Connection {http.request.header.Connection}
    }
}
```

---

## 📤 分发方式

### 1. **静态文件压缩包**（推荐）

```bash
# 构建 + 打包
cd web-ui
npm run build
tar -czf rust-agent-ui-v0.1.0.tar.gz dist/

# 或使用 zip
zip -r rust-agent-ui-v0.1.0.zip dist/
```

**用户使用**：
```bash
tar -xzf rust-agent-ui-v0.1.0.tar.gz
cd dist
python3 -m http.server 3000
```

### 2. **Docker 容器**

创建 `web-ui/Dockerfile`:

```dockerfile
FROM nginx:alpine

# 复制构建产物
COPY dist /usr/share/nginx/html

# 自定义 nginx 配置（可选）
COPY nginx.conf /etc/nginx/conf.d/default.conf

EXPOSE 80
CMD ["nginx", "-g", "daemon off;"]
```

构建和运行：
```bash
docker build -t rust-agent-ui .
docker run -d -p 3000:80 rust-agent-ui
```

### 3. **GitHub Releases**

```bash
# 构建
npm run build

# 创建 release 并上传 dist/ 目录
gh release create v0.1.0 \
  --title "Web UI v0.1.0" \
  --notes "Release notes..." \
  dist/
```

### 4. **CDN 部署**（推荐用于公开服务）

上传到以下平台，自动获得全球加速：

- **Vercel**: `vercel deploy web-ui` （免费，自动 HTTPS）
- **Netlify**: 拖拽 `dist/` 文件夹
- **Cloudflare Pages**: 连接 Git 仓库，自动构建
- **GitHub Pages**: 
  ```bash
  npm run build
  npx gh-pages -d dist
  ```

---

## 🚀 自动部署脚本

我已为你创建了 `deploy.sh`，支持多种部署方式：

```bash
# 查看帮助
./deploy.sh --help

# 构建并打包
./deploy.sh --package

# 构建 Docker 镜像
./deploy.sh --docker

# 部署到服务器（需配置 SSH）
./deploy.sh --remote user@server:/var/www/
```

---

## ⚙️ 配置说明

### 修改默认服务器地址

编辑 `src/stores/agentStore.ts`:

```typescript
const initialState = {
  serverUrl: 'ws://your-server:9527',  // 修改这里
  // ...
}
```

### 构建时环境变量

创建 `.env.production`:

```bash
VITE_WS_URL=wss://your-domain.com/ws
VITE_API_TIMEOUT=30000
```

在代码中使用：
```typescript
const serverUrl = import.meta.env.VITE_WS_URL || 'ws://localhost:9527';
```

---

## 📋 部署检查清单

- [ ] 运行 `npm run build` 成功
- [ ] 检查 `dist/` 目录生成
- [ ] 测试静态文件服务器可访问
- [ ] WebSocket 连接配置正确
- [ ] CORS 配置（如果前后端分离）
- [ ] HTTPS 证书配置（生产环境）
- [ ] 防火墙端口开放

---

## 🔧 故障排查

### WebSocket 连接失败

1. 检查服务器地址配置
2. 确认 Rust Agent 服务器运行 `--mode server`
3. 检查防火墙/安全组规则
4. HTTPS 页面需要 WSS 协议

### 页面空白

1. 检查浏览器控制台错误
2. 确认 `index.html` 和 JS 文件路径正确
3. 清除浏览器缓存

### 生产环境优化

```bash
# 启用 gzip 压缩
npm run build -- --mode production

# 分析包体积
npm install -D rollup-plugin-visualizer
npm run build -- --mode analyze
```

---

## 📚 更多信息

- [Vite 部署文档](https://vitejs.dev/guide/static-deploy.html)
- [React 部署指南](https://reactjs.org/docs/deployment.html)
- 项目主仓库：`/media/kanghua/disk/src/tools/rust_agent`
