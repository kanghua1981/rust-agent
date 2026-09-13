# pdf-reader

PDF 文本提取插件。内建的 `read_pdf` 工具已从框架里移除（它是 444 行 Rust，
逻辑本质是「检测 converter → 跑 → 后处理」，属于「碰巧编译进二进制的插件」），
这个插件是它的替代品。

## 安装

```bash
cp -r sample/pdf-reader .agent/plugins/pdf-reader
```

## 依赖

| converter | 何时用 | 安装 |
|---|---|---|
| `marker_single` | 有公式的论文，输出 Markdown + LaTeX | `pip install marker-pdf` |
| `pdftotext` | 普通文本 PDF | `apt install poppler-utils` |

两个都没有时工具会明确报错，而不是返回空结果。

## 为什么要移出来

PDF 足够通用，所以内建也说得过去；但它同时也完全符合「用 skill + 插件就够」
的形状。移出来之后，想换 converter（比如接一个远端 OCR 服务）不需要改 Rust。
