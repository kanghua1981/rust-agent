---
name: Read Ebook Files
description: How to extract text from ebook files (MOBI, EPUB, AZW3, FB2, DOCX, etc.) using external tools
---

# Read Ebook Files

# 读取电子书文件

## 概述
本技能指导如何使用外部工具提取电子书文件（MOBI、EPUB、AZW3、FB2、DOCX等）的文本内容。由于电子书读取功能已从内置工具中移除，现在需要通过 `run_command` 工具使用外部程序来实现。

## 支持的格式
- **主要格式**: MOBI, EPUB, AZW, AZW3, AZW4, KFX
- **其他格式**: FB2, LIT, LRF, PDB, RB, SNB, TCR
- **文档格式**: DOCX, RTF, ODT, HTMLZ, TXTZ
- **图像格式**: CBZ, CBR, DJVU

## 方法一：使用 Calibre 的 ebook-convert（推荐）

### 安装 Calibre
```bash
# Ubuntu/Debian
sudo apt install calibre

# macOS
brew install calibre

# Windows
# 从 https://calibre-ebook.com/download 下载安装
```

### 使用 ebook-convert 提取文本
```bash
# 基本用法
ebook-convert input.epub output.txt

# 使用 run_command 工具
{
  "command": "ebook-convert /path/to/book.epub /tmp/output.txt && cat /tmp/output.txt",
  "working_dir": "/current/directory"
}

# 带选项的示例
{
  "command": "ebook-convert /path/to/book.mobi /tmp/book.txt --enable-heuristics --max-line-length=80",
  "working_dir": "/current/directory"
}
```

## 方法二：使用 Pandoc

### 安装 Pandoc
```bash
# Ubuntu/Debian
sudo apt install pandoc

# macOS
brew install pandoc

# Windows
# 从 https://pandoc.org/installing.html 下载
```

### 使用 Pandoc 提取文本
```bash
# 基本用法（支持 EPUB, DOCX, ODT, RTF, FB2）
pandoc input.epub -t plain --wrap=auto

# 使用 run_command 工具
{
  "command": "pandoc /path/to/book.epub -t plain --wrap=auto",
  "working_dir": "/current/directory"
}
```

## 方法三：使用 Python 脚本

### 安装 Python 依赖
```bash
pip install ebooklib beautifulsoup4
```

### 创建 Python 脚本
```python
#!/usr/bin/env python3
import sys
from ebooklib import epub

def extract_epub_text(epub_path):
    book = epub.read_epub(epub_path)
    text = ""
    for item in book.get_items():
        if item.get_type() == ebooklib.ITEM_DOCUMENT:
            text += item.get_content().decode('utf-8')
    return text

if __name__ == "__main__":
    if len(sys.argv) < 2:
        print("Usage: python extract_ebook.py <epub_file>")
        sys.exit(1)
    
    text = extract_epub_text(sys.argv[1])
    print(text)
```

### 使用 run_command 调用 Python 脚本
```bash
{
  "command": "python3 /path/to/extract_ebook.py /path/to/book.epub",
  "working_dir": "/current/directory"
}
```

## 完整示例：提取电子书内容

### 步骤 1：检查可用工具
```bash
{
  "command": "which ebook-convert pandoc python3",
  "working_dir": "/current/directory"
}
```

### 步骤 2：提取文本（使用最佳可用工具）
```bash
# 如果 ebook-convert 可用
{
  "command": "TMPFILE=$(mktemp) && ebook-convert /path/to/book.epub $TMPFILE --enable-heuristics && cat $TMPFILE && rm $TMPFILE",
  "working_dir": "/current/directory"
}

# 如果只有 pandoc 可用
{
  "command": "pandoc /path/to/book.epub -t plain --wrap=auto | head -c 50000",
  "working_dir": "/current/directory"
}
```

### 步骤 3：处理大文件（分页读取）
```bash
{
  "command": "TMPFILE=$(mktemp) && ebook-convert /path/to/large_book.mobi $TMPFILE && head -c 100000 $TMPFILE && echo '\\n... (truncated)' && rm $TMPFILE",
  "working_dir": "/current/directory"
}
```

## 故障排除

### 常见问题
1. **"command not found: ebook-convert"**
   - 安装 Calibre：`sudo apt install calibre`

2. **"Unsupported format"**
   - 检查文件扩展名是否在支持列表中
   - 尝试使用 `file` 命令检查实际文件类型：`file book.epub`

3. **权限问题**
   - 确保对文件有读取权限
   - 使用绝对路径而不是相对路径

4. **内存不足**
   - 对于大文件，使用 `head -c` 限制输出大小
   - 考虑分块处理

### 验证安装
```bash
{
  "command": "ebook-convert --version 2>/dev/null || echo 'Calibre not installed'",
  "working_dir": "/current/directory"
}
```

## 最佳实践

1. **总是检查工具可用性**：在执行前检查 `ebook-convert` 或 `pandoc` 是否已安装
2. **使用临时文件**：避免污染工作目录
3. **限制输出大小**：对于大文件，限制返回的字符数（如 50000 字符）
4. **提供清晰的错误信息**：如果工具不可用，给出明确的安装说明
5. **记录操作**：在 agent memory 中记录读取了哪些文件

## 替代方案
如果以上方法都不可用，可以考虑：
1. 使用在线转换服务
2. 手动打开电子书并复制文本
3. 使用其他电子书阅读器软件

## 注意事项
- PDF 文件应使用 `read_pdf` 工具而不是电子书工具
- 某些 DRM 保护的电子书可能无法提取文本
- 图像格式（CBZ、CBR、DJVU）主要包含图片，文本提取效果有限
