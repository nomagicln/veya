# 实施计划：Veya MVP

## 概述

基于 Tauri 2 + Rust + React 架构，按模块递增实现 Veya MVP 的全部功能。从基础设施（错误处理、数据库、加密存储）开始，逐步构建核心业务模块（划词、截图、播客），最后完成前端交互层和集成联调。每个任务构建在前一步之上，确保无孤立代码。

## 任务

- [x] 1. 项目脚手架与基础设施
  - [x] 1.1 初始化 Tauri 2 + React 项目
    - 使用 `create-tauri-app` 创建项目，选择 React + TypeScript 模板
    - 配置 `tauri.conf.json`：设置应用名称、窗口配置（主窗口隐藏、悬浮窗 `always_on_top` + `decorations: false` + `transparent: true`）
    - 添加 Cargo 依赖：`serde`, `serde_json`, `thiserror`, `tokio`, `reqwest`, `rusqlite`, `tauri-plugin-stronghold`, `whatlang`, `proptest`
    - 添加前端依赖：`react-i18next`, `i18next`, `zustand`, `fast-check`
    - _需求: 全部_

  - [x] 1.2 实现 VeyaError 枚举与 RetryPolicy
    - 在 `src-tauri/src/error.rs` 中定义 `VeyaError` 枚举（InvalidApiKey、InsufficientBalance、NetworkTimeout、ModelUnavailable、OcrFailed、TtsFailed、StorageError、PermissionDenied），实现 `thiserror::Error`、`Serialize`
    - 实现 `is_retryable()` 方法：NetworkTimeout、ModelUnavailable、TtsFailed 返回 true，其余返回 false
    - 在 `src-tauri/src/retry.rs` 中实现 `RetryPolicy` 结构体及指数退避 `execute` 方法
    - _需求: 8.1, 8.2_

  - [x] 1.3 编写 RetryPolicy 属性测试（Property 16: 重试策略执行）
    - **Property 16: 重试策略执行**
    - 使用 `proptest` 生成随机重试次数 N (1..10)，验证持续失败时总调用次数恰好为 N+1
    - **验证: 需求 8.1**

  - [x] 1.4 编写 VeyaError 错误分类属性测试（Property 17: 错误类型分类）
    - **Property 17: 错误类型分类**
    - 使用 `proptest` 生成各类 API 错误变体，验证所有重试失败后返回的错误包含对应的具体错误类型标识
    - **验证: 需求 8.2**

- [x] 2. 数据存储层
  - [x] 2.1 搭建 SQLite 数据库与迁移
    - 在 `src-tauri/src/db.rs` 中初始化 SQLite 连接（使用 `app_data_dir()/veya.db`）
    - 创建迁移脚本，建立 `query_records`、`podcast_records`、`word_frequency`、`api_configs`、`settings` 五张表
    - 实现 `Database` 结构体，封装连接池和基础 CRUD 方法
    - _需求: 6.1, 6.2, 6.3, 8.3_

  - [x] 2.2 集成 Stronghold 加密存储
    - 配置 `tauri-plugin-stronghold`，创建 vault `veya-keys`
    - 实现 `StrongholdStore` 结构体：`store_api_key(config_id, key)`、`get_api_key(config_id)` 、`delete_api_key(config_id)`
    - 确保 SQLite 中 `api_configs.api_key_ref` 仅存储 Stronghold 引用，不存储明文
    - _需求: 5.5_

  - [-] 2.3 编写 API Key 加密存储往返属性测试（Property 11: API Key 加密存储往返）
    - **Property 11: API Key 加密存储往返**
    - 使用 `proptest` 生成随机 API Key 字符串，验证存储后 SQLite 中仅含引用、从 Stronghold 读取返回原始值
    - **验证: 需求 5.5**

- [ ] 3. 检查点 - 基础设施验证
  - 确保所有测试通过，如有问题请向用户确认。

- [ ] 4. API Config 与 Settings 模块
  - [ ] 4.1 实现 API Config 模块
    - 在 `src-tauri/src/api_config.rs` 中定义 `ApiConfig`、`ApiProvider`、`ModelType` 结构体/枚举
    - 实现 Tauri Command：`get_api_configs`、`save_api_config`、`test_api_connection`
    - `save_api_config` 内部将 API Key 存入 Stronghold，元数据存入 SQLite
    - 支持 OpenAI、Anthropic、ElevenLabs、Ollama、Custom 五种 provider
    - _需求: 5.1, 5.2, 5.3, 5.4, 5.5_

  - [ ]* 4.2 编写 API 配置独立性与持久化属性测试（Property 10: API 配置独立性与持久化）
    - **Property 10: API 配置独立性与持久化**
    - 使用 `proptest` 生成不同 model_type 的配置，验证独立存储和检索、保存后读取字段一致
    - **验证: 需求 5.1, 5.2**

  - [ ] 4.3 实现 Settings 模块
    - 在 `src-tauri/src/settings.rs` 中定义 `AppSettings` 结构体
    - 实现 Tauri Command：`get_settings`、`update_settings`
    - 设置项存储在 SQLite `settings` 表中，key-value 格式
    - 包含：`ai_completion_enabled`、`cache_max_size_mb`、`cache_auto_clean_days`、`retry_count`、`shortcut_capture`、`locale`
    - _需求: 7.1, 7.2, 7.3, 7.4, 7.5, 7.6_

  - [ ]* 4.4 编写设置往返一致性属性测试（Property 15: 设置往返一致性）
    - **Property 15: 设置往返一致性**
    - 使用 `proptest` 生成随机有效设置值，验证保存后读取返回完全相同的值
    - **验证: 需求 7.5, 7.6, 9.2**

- [ ] 5. 统一 AI 客户端
  - [ ] 5.1 实现 LlmClient
    - 在 `src-tauri/src/llm_client.rs` 中实现 `LlmClient` 结构体
    - 实现 `stream_chat` 方法：构造 OpenAI 兼容请求，解析 SSE 流，通过 Tauri Event 逐块推送
    - 实现 `chat` 方法：非流式请求，返回完整响应
    - 支持 OpenAI、Anthropic（转换请求格式）、Ollama（OpenAI 兼容）三种文本模型
    - 集成 `RetryPolicy`，对可重试错误自动重试
    - _需求: 5.2, 5.3, 8.1_

  - [ ] 5.2 实现 TtsClient
    - 在 `src-tauri/src/tts_client.rs` 中实现 `TtsClient` 结构体
    - 实现 `synthesize` 方法：发送文本到 TTS 服务，返回音频字节数据
    - 按语言代码路由到对应的 TTS 服务配置
    - 集成 `RetryPolicy`
    - _需求: 3.7, 5.2_

  - [ ]* 5.3 编写 TTS 语言路由属性测试（Property 8: TTS 语言路由）
    - **Property 8: TTS 语言路由**
    - 使用 `proptest` 生成语言代码和多个 TTS 配置，验证请求路由到正确的语言服务地址
    - **验证: 需求 3.7**

- [ ] 6. Text Insight 模块（划词解析）
  - [ ] 6.1 实现 Accessibility API 监听与文本获取
    - 在 `src-tauri/src/text_insight.rs` 中实现 `TextInsightListener`
    - macOS：使用 `accessibility-sys` 监听 `AXSelectedTextChanged`
    - Windows：使用 `windows-rs` UI Automation API
    - 获取划选文本后通过 `on_text_selected` 触发分析流程
    - _需求: 1.1_

  - [ ] 6.2 实现文本分析与流式输出
    - 实现 `analyze_text` Tauri Command
    - 使用 `whatlang` crate 检测文本语言
    - 构造结构化分析 prompt，通过 `LlmClient.stream_chat` 流式调用
    - 通过 `veya://text-insight/stream-chunk` Event 推送 `TextInsightChunk`（start → delta × N → done）
    - 输出六个结构化字段：original、word_by_word、structure、translation、colloquial、simplified
    - _需求: 1.1, 1.2, 1.3, 1.4, 1.5_

  - [ ]* 6.3 编写语言检测属性测试（Property 1: 语言检测准确性）
    - **Property 1: 语言检测准确性**
    - 使用 `proptest` 生成非空文本，验证语言检测函数返回有效语言代码
    - **验证: 需求 1.1**

  - [ ]* 6.4 编写结构化输出完整性属性测试（Property 2: 结构化输出完整性）
    - **Property 2: 结构化输出完整性**
    - 使用 `fast-check` 生成模拟分析结果，验证输出包含全部六个必需字段且均为非空字符串
    - **验证: 需求 1.3**

- [ ] 7. Vision Capture 模块（截图识别）
  - [ ] 7.1 实现截图与区域框选
    - 在 `src-tauri/src/vision_capture.rs` 中实现 `start_capture` Tauri Command
    - macOS：使用 `CGWindowListCreateImage` API 截取屏幕
    - Windows：使用 `Graphics.Capture` API
    - 前端创建全屏透明窗口，用户拖拽选区后将 `CaptureRegion` 坐标传回 Rust
    - _需求: 2.1_

  - [ ] 7.2 实现 Native OCR 与 AI 补全流水线
    - 实现 `process_capture` Tauri Command
    - macOS OCR：调用 Vision Framework `VNRecognizeTextRequest`
    - Windows OCR：调用 `Windows.Media.Ocr` API
    - 根据 `ai_completion` 参数决定是否将 OCR 结果发送至 AI 模型补全
    - 通过 `veya://vision-capture/stream-chunk` Event 推送结果，AI 推测内容标记 `is_ai_inferred: true`
    - _需求: 2.2, 2.3, 2.4, 2.5, 2.6_

  - [ ]* 7.3 编写 AI 补全条件行为属性测试（Property 3: AI 补全流水线条件行为）
    - **Property 3: AI 补全流水线条件行为**
    - 使用 `proptest` 生成 OCR 结果和 AI_Completion 开关状态，验证流水线条件分支正确
    - **验证: 需求 2.3, 2.4**

  - [ ]* 7.4 编写 AI 推测内容标记属性测试（Property 4: AI 推测内容标记）
    - **Property 4: AI 推测内容标记**
    - 使用 `fast-check` 生成包含 AI 补全的识别结果，验证 `aiInferredRanges` 标记准确且不与原始内容重叠
    - **验证: 需求 2.5**

- [ ] 8. 检查点 - 核心模块验证
  - 确保所有测试通过，如有问题请向用户确认。

- [ ] 9. Cast Engine 模块（播客生成）
  - [ ] 9.1 实现播客生成流水线
    - 在 `src-tauri/src/cast_engine.rs` 中实现 `generate_podcast` Tauri Command
    - 接收 `PodcastInput`（content + source）和 `PodcastOptions`（speed + mode + target_language）
    - 流水线：构造口语讲解稿 prompt → LLM 生成讲解稿 → 分段 → TTS 合成 → 合并为 MP3
    - 通过 `veya://cast-engine/progress` Event 推送进度（script_generating → script_done → tts_progress → done）
    - 默认将音频存储到 `app_cache_dir()/audio/temp/`
    - _需求: 3.1, 3.2, 3.3, 3.4_

  - [ ] 9.2 实现音频存储管理
    - 实现 `save_podcast` Tauri Command：将临时音频复制到 `app_data_dir()/audio/saved/`
    - 实现 `cleanup_temp_audio` Tauri Command：清理临时缓存目录
    - 在 Tauri `on_exit` hook 中调用临时目录清理
    - 实现缓存清理策略：按最大空间和最大天数清理持久化音频
    - _需求: 3.5, 3.6, 7.3_

  - [ ]* 9.3 编写播客生成流水线属性测试（Property 5: 播客生成输入接受与流水线）
    - **Property 5: 播客生成输入接受与流水线**
    - 使用 `proptest` 生成三种来源的有效输入，验证流水线按顺序产生所有阶段输出
    - **验证: 需求 3.1, 3.2**

  - [ ]* 9.4 编写播客输出格式属性测试（Property 6: 播客输出选项与格式）
    - **Property 6: 播客输出选项与格式**
    - 使用 `proptest` 生成所有速度模式和播客模式组合，验证生成有效 MP3 文件且大小 > 0
    - **验证: 需求 3.3, 3.4**

  - [ ]* 9.5 编写音频存储生命周期属性测试（Property 7: 音频存储生命周期）
    - **Property 7: 音频存储生命周期**
    - 使用 `proptest` 验证：默认存储在临时目录、保存后存在于持久目录、清理后临时文件删除而持久文件不受影响
    - **验证: 需求 3.5, 3.6**

  - [ ]* 9.6 编写缓存清理策略属性测试（Property 14: 缓存清理策略）
    - **Property 14: 缓存清理策略**
    - 使用 `proptest` 生成随机缓存配置（最大空间 M、最大天数 D），验证清理后目录大小 ≤ M 且无超期文件
    - **验证: 需求 7.3**

- [ ] 10. Learning Record 模块
  - [ ] 10.1 实现学习记录 CRUD 与词频统计
    - 在 `src-tauri/src/learning_record.rs` 中实现全部 Tauri Command
    - `save_query_record`：保存查询记录到 `query_records` 表
    - `save_podcast_record`：保存播客记录到 `podcast_records` 表
    - `get_query_history` / `get_podcast_history`：分页查询历史
    - `get_frequent_words`：从 `word_frequency` 表查询常用词，按 count 降序
    - 在保存查询记录时自动更新 `word_frequency` 表（分词后逐词计数）
    - _需求: 6.1, 6.2, 6.3, 6.4_

  - [ ]* 10.2 编写学习记录持久化属性测试（Property 12: 学习记录自动持久化）
    - **Property 12: 学习记录自动持久化**
    - 使用 `proptest` 生成查询/播客记录，验证保存后数据库中存在对应记录且包含所有必需字段
    - **验证: 需求 6.1, 6.2**

  - [ ]* 10.3 编写词频统计属性测试（Property 13: 词频统计准确性）
    - **Property 13: 词频统计准确性**
    - 使用 `proptest` 生成查询序列，验证常用词列表中每个词的频次等于该词在所有查询中出现的总次数
    - **验证: 需求 6.3**

- [ ] 11. 检查点 - 后端模块完整性验证
  - 确保所有测试通过，如有问题请向用户确认。

- [ ] 12. i18n 国际化
  - [ ] 12.1 搭建 i18n 基础设施
    - 配置 `react-i18next`：初始化 i18n 实例，设置默认语言为 `zh-CN`
    - 创建 `src/locales/zh-CN.json` 和 `src/locales/en-US.json` 翻译文件
    - 按模块组织翻译 key（如 `textInsight.original`、`settings.aiCompletion`、`castEngine.generate`）
    - 实现 `I18nProvider` 包裹应用根组件
    - 语言切换通过 `i18n.changeLanguage()` 即时生效
    - _需求: 9.1, 9.2, 9.3_

- [ ] 13. 前端状态管理与悬浮窗
  - [ ] 13.1 搭建全局状态管理
    - 使用 Zustand 创建 `useAppStore`，定义 `AppState` 接口
    - 包含：`floatingWindow`（visible、pinned、position、currentContent、audioState）、`settings`、`locale`
    - 实现状态更新 actions：`showWindow`、`hideWindow`、`togglePin`、`updateContent`、`updateSettings`
    - _需求: 4.1, 4.2, 4.3_

  - [ ] 13.2 实现悬浮窗核心组件
    - 实现 `FloatingWindow` 根组件：监听 Tauri Event，管理窗口显示/隐藏
    - 实现 `StreamContent` 组件：渲染六个结构化字段，支持流式逐步显示
    - 实现 `ActionBar` 组件：PodcastButton、PinButton、CopyButton
    - 实现 `AudioPlayer` 组件：PlayPauseButton、ProgressBar、SaveButton
    - 所有文本使用 `useTranslation()` hook 实现 i18n
    - _需求: 1.2, 1.3, 2.5, 2.6, 4.1, 4.4, 4.5_

  - [ ] 13.3 实现悬浮窗 Pin/隐藏行为
    - 监听窗口 `blur` 事件：未 Pin 时调用 `hide_floating_window` Command
    - Pin 按钮点击调用 `toggle_pin` Command，切换 `always_on_top` 状态
    - 窗口位置：划词场景跟随鼠标光标，截图场景居中显示
    - _需求: 4.2, 4.3_

  - [ ]* 13.4 编写悬浮窗状态机属性测试（Property 9: 悬浮窗 Pin/隐藏状态机）
    - **Property 9: 悬浮窗 Pin/隐藏状态机**
    - 使用 `fast-check` 生成随机 pin 状态，验证 blur 事件后的可见性行为正确
    - **验证: 需求 4.2, 4.3**

- [ ] 14. 设置页与学习记录页
  - [ ] 14.1 实现设置页面
    - 创建 `SettingsPage` 组件
    - 包含：AI 补全开关、TTS 服务配置、缓存清理策略、模型 API 配置入口、全局快捷键配置、界面语言切换
    - 调用 `get_settings` / `update_settings` Command 读写设置
    - 语言切换时同步调用 `i18n.changeLanguage()` 和 `update_settings`
    - _需求: 7.1, 7.2, 7.3, 7.4, 7.5, 7.6, 7.7_

  - [ ] 14.2 实现 API 配置界面
    - 创建 API 配置组件：列表展示、新增/编辑/删除配置
    - 支持选择 provider（OpenAI、Anthropic、ElevenLabs、Ollama、Custom）
    - 支持配置 base_url、model_name、API Key
    - TTS 配置支持按语言绑定
    - 提供连接测试按钮，调用 `test_api_connection` Command
    - _需求: 5.1, 5.2, 5.3, 5.4_

  - [ ] 14.3 实现学习记录页面
    - 创建 `LearningPage` 组件
    - 查询历史列表：分页展示，显示输入内容、来源、时间
    - 播客历史列表：分页展示，显示输入内容、音频路径、时间
    - 常用词列表：按频次排序展示
    - _需求: 6.3, 6.4_

- [ ] 15. 全局集成与联调
  - [ ] 15.1 注册 Tauri Command 与 Event 监听
    - 在 `main.rs` 中注册所有 Tauri Command
    - 配置全局快捷键（截图触发）
    - 配置系统托盘菜单（打开设置、退出）
    - 在 `on_exit` hook 中调用 `cleanup_temp_audio`
    - 启动 `TextInsightListener` 监听划词事件
    - _需求: 全部_

  - [ ] 15.2 连接前后端数据流
    - 前端监听 `veya://text-insight/stream-chunk` Event，驱动 StreamContent 渲染
    - 前端监听 `veya://vision-capture/stream-chunk` Event，驱动截图结果渲染（含 AI 推测标记）
    - 前端监听 `veya://cast-engine/progress` Event，驱动播客生成进度和音频播放
    - 错误 Event 触发前端错误提示展示
    - 划词/截图完成后自动调用 `save_query_record` 保存学习记录
    - _需求: 1.2, 2.6, 3.2, 4.1, 6.1, 8.2_

- [ ] 16. 最终检查点 - 全部验证
  - 确保所有测试通过，如有问题请向用户确认。

## 备注

- 标记 `*` 的任务为可选任务，可跳过以加速 MVP 交付
- 每个任务引用了具体的需求编号，确保可追溯性
- 检查点任务用于阶段性验证，确保增量正确性
- 属性测试验证通用正确性属性，单元测试验证具体示例和边界情况
- Rust 端属性测试使用 `proptest`，React 前端属性测试使用 `fast-check`
