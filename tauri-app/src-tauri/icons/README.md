# 图标文件

Tauri 应用需要图标文件。请将以下尺寸的 PNG 图标放入此目录：

- 32x32.png
- 128x128.png
- 128x128@2x.png (256x256)
- icon.icns (macOS)
- icon.ico (Windows)

或者，你可以使用 Tauri 的图标生成工具：

```bash
cd tauri-app
npm run tauri icon -- ./path/to/your/icon.png
```

这将自动生成所有需要的图标尺寸。