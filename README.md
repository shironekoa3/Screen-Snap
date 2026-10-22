# Screen Snap

Windows 桌面端的定时屏幕快照记录工具：按间隔自动截屏，异步 OCR 识别截图中的文字，支持按日期浏览和关键词检索。

## 技术栈

- **外壳**：Tauri 2（Rust）
- **前端**：Vue 3 + TypeScript + Pinia + Vue Router + Tailwind CSS
- **截屏**：`screenshots` crate
- **OCR**：Windows.Media.Ocr（系统自带 WinRT 引擎，无模型文件）
- **存储**：图片落盘 + SQLite（`rusqlite` bundled）存 OCR 文本

## 开发

```bash
npm install
npm run tauri dev
```

打包：

```bash
npm run tauri build
```

## 数据存放位置（便携模式）

程序的所有运行时数据都与 **exe 同目录**，不写 AppData：

```
<exe 所在目录>/
├─ screen-snapshot.exe
├─ ocr.db                 # OCR 文本索引
├─ settings.json          # 截图间隔 / 格式 / 质量
├─ startup-error.log      # 仅在启动失败时生成
└─ screenshots/
   ├─ 2026-10-22/
   │  ├─ 18-46-35-412.png   # 原图（毫秒级时间戳）
   │  └─ ...
   └─ .thumbs/              # 列表用缩略图缓存，可随时删除
      └─ 2026-10-22/
         └─ 18-46-35-412.jpg
```

> ⚠️ 因为数据跟着程序走，**不要对 exe 所在目录做清理/覆盖安装**，`cargo clean` 或删除 `target/` 会连同截图一起删掉。需要迁移时整个目录一起拷贝即可。

## 目录结构

```
src/
├─ views/           控制台 / 历史记录 / 文字搜索
├─ stores/          capture / settings / search（Pinia）
├─ composables/     useImageCache：缩略图与原图缓存
└─ components/      Sidebar

src-tauri/src/
├─ main.rs          Tauri 命令、定时循环、托盘
├─ screenshot.rs    截屏、编码、缩略图
├─ ocr.rs           WinRT OCR
└─ db.rs            SQLite
```

## 实现要点

- 截图与 OCR 全程在阻塞线程池执行，不占 tokio worker
- OCR 用信号量串行，避免短间隔下任务堆积
- 列表与预览只传 640px 缩略图（几十 KB），原图仅在灯箱按需读取
- 文件读写命令只接受 `screenshots/` 之下的路径
- 搜索与按日期浏览合并在「历史记录」页：搜索框留空即浏览
- 开机自启写入 `HKCU\...\CurrentVersion\Run`（无需管理员权限），
  由启动项拉起时带 `--autostart` 参数，窗口只进托盘不弹出

## 常见问题

### 构建报 `LNK1120`、大量「无法解析的外部符号」

不是代码问题，是 `target/debug/incremental` 里的增量编译缓存损坏了。典型特征是报错里
serde、tokio、exr、url 等一堆互不相关的 crate 同时缺符号，且符号名带 `.llvm.xxxx` 后缀。
常见诱因：编译过程中崩溃、被强制中断，或者**两个 cargo 同时构建同一个 target**
（最容易踩的是：`npm run tauri dev` 在后台 watch，同时又手动跑了一次 `cargo build`）。

修复（**不会动截图数据**）：

```bash
cd src-tauri
rm -rf target/debug/incremental
rm -rf target/debug/deps/screen_snapshot-* target/debug/.fingerprint/screen-snapshot-*
cargo build
```

> ⚠️ 不要用 `cargo clean`：它会删掉整个 `target/`，包括 `target/release/screenshots`
> 里你的全部截图。只删 `incremental` 目录就够了。

