# e_agent_gui - Rust Agent Tauri 桌面应用

这是一个基于 Tauri 的 Rust Agent 桌面应用，重用现有的 web-ui 前端代码。

## 特性

- 🚀 **代码共享**: 通过软链接重用 web-ui 的前端代码
- 🖥️ **桌面应用**: 使用 Tauri 构建原生桌面应用
- 📁 **文件系统访问**: 完整的文件系统权限
- 🔧 **开发友好**: 支持热重载和开发工具

## 项目结构

```
tauri-app/
├── src/ -> ../web-ui/src/          # 前端源码（软链接）
├── index.html                      # HTML 入口（本地副本，与 web-ui 同步）
├── src-tauri/                      # Rust 后端
│   ├── src/main.rs                 # Rust 入口点
│   ├── Cargo.toml                  # Rust 依赖
│   └── tauri.conf.json             # Tauri 配置
├── package.json                    # 前端依赖
├── vite.config.ts                  # 构建配置
├── tsconfig.json                   # TypeScript 配置
├── build.sh                        # 构建脚本
└── dev.sh                          # 开发脚本
```

## 快速开始

### 前提条件

- Node.js 18+ 和 npm
- Rust 工具链 (rustc, cargo)
- Tauri 系统依赖 (请参考 [Tauri 文档](https://tauri.app/v1/guides/getting-started/prerequisites))

### 安装依赖

```bash
cd tauri-app
npm install
```

### 开发模式

```bash
./dev.sh
```

或者手动运行：

```bash
npm run tauri dev
```

开发服务器将在 http://localhost:9581 启动，Tauri 应用会自动打开。

### 构建应用

```bash
./build.sh
```

或者手动运行：

```bash
npm run tauri build
```

构建产物位于 `src-tauri/target/release/` 目录，包括：
- `e_agent_gui` - 可执行文件
- `bundle/deb/` - Debian 包
- `bundle/rpm/` - RPM 包

## 技术细节

### 软链接策略

项目使用软链接共享前端源码：
- `tauri-app/src/` → `../web-ui/src/`

`index.html` 是本地副本（而非软链接），因为 Vite 构建工具对软链接的 index.html 支持有限。
当 web-ui 的 index.html 更新时，需要手动同步：

```bash
cp ../web-ui/index.html index.html
```

这意味着对 web-ui/src 的修改会立即反映在 Tauri 应用中。

### 文件系统权限

Tauri 应用配置了完整的文件系统访问权限，通过 `tauri-plugin-fs` 插件实现：
- 读写文件
- 创建/删除目录
- 列出目录内容
- 运行系统命令

### Rust 后端 API

后端提供了以下 Tauri 命令：
- `greet(name: string)` - 测试 API
- `get_home_dir()` - 获取用户主目录
- `read_file(path: string)` - 读取文件
- `write_file(path: string, content: string)` - 写入文件
- `list_dir(path: string)` - 列出目录内容
- `run_command(command: string, args: string[], cwd?: string)` - 运行命令

### 版本信息

- Tauri: 2.x
- @tauri-apps/api: 2.x
- @tauri-apps/cli: 2.x

## 环境检测

前端代码已经包含 Tauri 环境检测：

```typescript
// 检测是否在 Tauri 环境中运行
export const isDesktopApp = (): boolean => {
  return typeof window !== 'undefined' && '__TAURI__' in window;
};
```

## 故障排除

### 软链接问题

如果软链接不工作，可以手动创建：

```bash
cd tauri-app
rm -rf src
ln -sf ../web-ui/src src
```

### 构建错误

1. **Rust 依赖问题**:
   ```bash
   cd tauri-app/src-tauri
   cargo update
   ```

2. **Node.js 依赖问题**:
   ```bash
   cd tauri-app
   rm -rf node_modules package-lock.json
   npm install
   ```

3. **版本不匹配**:
   确保 Tauri Rust crate 和 NPM 包版本一致：
   ```bash
   npm install @tauri-apps/api@latest @tauri-apps/cli@latest
   ```

### 平台特定问题

- **Windows**: 可能需要管理员权限创建软链接
- **macOS**: 可能需要启用开发者模式，图标需要 .icns 格式
- **Linux**: 确保安装了 Tauri 的系统依赖（webkit2gtk 等）

### AppImage 构建失败

如果 AppImage 构建失败，是因为需要安装 linuxdeploy：
```bash
# 安装 linuxdeploy（可选）
wget https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage
chmod +x linuxdeploy-x86_64.AppImage
sudo mv linuxdeploy-x86_64.AppImage /usr/local/bin/linuxdeploy
```

## 许可证

MIT
