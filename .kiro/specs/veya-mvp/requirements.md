# 需求文档

## 简介

Veya 是一个 AI 驱动的语言理解与听说强化系统级桌面工具。用户可通过划词、截图、播客生成等方式，实现"理解 → 听懂 → 说出"的语言学习闭环。MVP 版本聚焦于核心的划词解析、截图识别、播客生成、悬浮窗交互、多模型 API 配置、轻量学习记录和系统设置功能。

技术栈：Tauri 2 + Rust 后端 + React 前端，数据存储使用 SQLite + Tauri Stronghold。

## 术语表

- **Veya**: AI 驱动的语言理解与听说强化桌面工具
- **Text_Insight（划词解析模块）**: 通过系统 Accessibility API 获取用户划选文本并进行结构化语言分析的模块
- **Vision_Capture（截图识别模块）**: 通过全局快捷键触发截图、框选区域并进行 OCR 识别与可选 AI 补全的模块
- **Cast_Engine（播客生成模块）**: 将文本内容转化为口语讲解稿并生成音频文件的模块
- **Floating_Window（悬浮窗）**: Veya 的主交互界面，用于展示分析结果、播放音频和触发操作
- **API_Config（API 配置系统）**: 管理文本模型、视觉模型和语音模型 API 连接的配置系统
- **Learning_Record（学习记录模块）**: 记录查询历史、播客历史和常用词的轻量级模块
- **Settings（系统设置模块）**: 管理用户偏好和系统配置的模块
- **Native_OCR**: 系统原生 OCR 能力（macOS Vision Framework / Windows OCR API）
- **AI_Completion（AI 智能补全）**: 使用 AI 模型对 OCR 截断或模糊文本进行推断补全的功能
- **TTS_Service（TTS 服务）**: 文本转语音服务，支持按语言种类配置不同的服务地址
- **Stronghold**: Tauri 提供的加密存储机制，用于安全存储 API Key 等敏感信息

## 需求

### 需求 1：划词解析

**用户故事：** 作为语言学习者，我希望在任意应用中划词后自动获得结构化语言分析，以便深入理解词汇和句子结构。

#### 验收标准

1. WHEN 用户在任意应用中划选文本, THE Text_Insight SHALL 通过系统 Accessibility API 获取划选文本并自动检测文本语言
2. WHEN Text_Insight 获取到划选文本, THE Floating_Window SHALL 自动弹出并以流式方式输出结构化分析结果
3. THE Text_Insight SHALL 输出以下结构化内容：原文、逐词解释、句子结构分析、精准翻译、更口语版本、更简单版本
4. WHEN 使用本地模型时, THE Text_Insight SHALL 在 500ms 内呈现首字符输出
5. WHEN 使用远程 API 时, THE Text_Insight SHALL 在 1.5 秒内呈现首字符输出

### 需求 2：截图识别

**用户故事：** 作为语言学习者，我希望通过截图识别图片中的文字并获得结构化解释，以便理解图片中的语言内容。

#### 验收标准

1. WHEN 用户按下全局快捷键, THE Vision_Capture SHALL 进入截图模式并允许用户框选屏幕区域
2. WHEN 用户完成区域框选, THE Vision_Capture SHALL 调用 Native_OCR 识别框选区域内的文字，识别耗时少于 1 秒
3. WHILE AI_Completion 开关处于启用状态, WHEN Native_OCR 完成识别, THE Vision_Capture SHALL 将 OCR 结果发送至 AI 模型进行截断和模糊文本的推断补全
4. WHILE AI_Completion 开关处于关闭状态, THE Vision_Capture SHALL 仅使用 Native_OCR 的识别结果生成结构化输出
5. WHEN AI_Completion 产生补全内容, THE Floating_Window SHALL 以视觉标记区分 AI 推测内容与原始 OCR 识别内容
6. WHEN Vision_Capture 完成文字识别, THE Floating_Window SHALL 自动弹出并以流式方式展示结构化解释结果

### 需求 3：播客生成

**用户故事：** 作为语言学习者，我希望将文本内容转化为口语化的讲解音频，以便通过听觉方式强化语言学习。

#### 验收标准

1. WHEN 用户在 Floating_Window 中触发播客生成, THE Cast_Engine SHALL 接收输入内容（划词结果、截图内容或用户自定义文本）
2. THE Cast_Engine SHALL 按以下流程处理输入内容：内容理解、转化为口语讲解稿、分段结构化、生成音频文件
3. THE Cast_Engine SHALL 支持以下输出选项：慢速模式、正常语速、双语对照、单语沉浸模式
4. THE Cast_Engine SHALL 以 MP3 格式输出生成的音频文件
5. THE Cast_Engine SHALL 将生成的播客音频默认存储为临时缓存，在软件关闭时自动删除临时缓存音频
6. WHEN 用户主动选择"保存"操作, THE Cast_Engine SHALL 将音频文件持久化到本地存储
7. THE Cast_Engine SHALL 支持按语言种类配置不同的 TTS_Service 地址（例如：英语使用 ElevenLabs，中文使用其他 TTS 服务）

### 需求 4：悬浮窗交互

**用户故事：** 作为语言学习者，我希望通过轻量级悬浮窗快速查看分析结果并执行操作，以便不中断当前工作流。

#### 验收标准

1. WHEN 划词或截图操作完成, THE Floating_Window SHALL 自动弹出并展示对应的分析结果
2. WHEN Floating_Window 未被 Pin 住且失去焦点, THE Floating_Window SHALL 自动隐藏
3. WHEN 用户点击 Pin 按钮, THE Floating_Window SHALL 切换为常驻屏幕模式，保持可见直到用户取消 Pin
4. THE Floating_Window SHALL 提供播客生成触发按钮，允许用户直接在悬浮窗内发起播客生成
5. THE Floating_Window SHALL 提供音频播放控件，允许用户在悬浮窗内播放和保存生成的播客音频

### 需求 5：多模型 API 配置

**用户故事：** 作为用户，我希望灵活配置不同的 AI 模型 API，以便根据需求选择最合适的模型服务或使用本地模型离线工作。

#### 验收标准

1. THE API_Config SHALL 支持独立配置文本模型 API、视觉模型 API 和语音模型 API（语音模型按语言种类独立配置）
2. THE API_Config SHALL 支持 OpenAI 兼容接口、自定义 Base URL 配置
3. THE API_Config SHALL 默认兼容 OpenAI、Anthropic、ElevenLabs 和 Ollama 服务
4. WHEN 用户配置本地模型（如 Ollama）, THE Veya SHALL 支持完全离线使用，划词解析、截图识别（Native_OCR + 本地 AI 补全）和播客生成均可正常工作
5. THE API_Config SHALL 使用 Stronghold 加密存储所有 API Key

### 需求 6：学习记录

**用户故事：** 作为语言学习者，我希望查看历史查询和播客记录，以便回顾和巩固学习内容。

#### 验收标准

1. WHEN 用户完成一次划词解析或截图识别, THE Learning_Record SHALL 自动记录该次查询的输入内容和分析结果
2. WHEN 用户保存一个播客音频, THE Learning_Record SHALL 记录该播客的生成时间、输入内容和音频文件路径
3. THE Learning_Record SHALL 统计并展示用户的常用词列表
4. THE Learning_Record SHALL 提供查询历史和播客历史的浏览界面

### 需求 7：系统设置

**用户故事：** 作为用户，我希望自定义 Veya 的各项配置，以便根据个人偏好调整工具行为。

#### 验收标准

1. THE Settings SHALL 提供 AI_Completion 开关，控制截图识别场景中是否启用 AI 智能补全
2. THE Settings SHALL 提供 TTS_Service 地址配置，支持按语言种类独立配置
3. THE Settings SHALL 提供缓存清理策略配置，包括保存音频的最大占用空间和自动清理超过指定天数的保存音频
4. THE Settings SHALL 提供模型 API 配置入口，链接至 API_Config 配置界面
5. THE Settings SHALL 提供全局快捷键配置功能
6. THE Settings SHALL 提供界面语言切换功能，默认支持中文和英文
7. THE Settings SHALL 在架构上支持扩展其他界面语言

### 需求 8：错误处理与稳定性

**用户故事：** 作为用户，我希望在 API 调用失败时获得清晰的错误信息和自动重试，以便不影响使用体验。

#### 验收标准

1. WHEN API 调用失败, THE Veya SHALL 自动重试，重试次数由用户在 Settings 中配置
2. IF API 调用在所有重试后仍然失败, THEN THE Veya SHALL 向用户展示具体的错误信息（包括：Key 无效、余额不足、网络超时、模型不可用等错误类型）
3. THE Veya SHALL 将所有用户数据默认存储在本地，不上传至任何远程服务器

### 需求 9：国际化

**用户故事：** 作为不同语言背景的用户，我希望使用母语界面操作 Veya，以便降低使用门槛。

#### 验收标准

1. THE Veya SHALL 默认提供中文和英文两种界面语言
2. WHEN 用户在 Settings 中切换界面语言, THE Veya SHALL 立即应用新的界面语言，无需重启应用
3. THE Veya SHALL 采用可扩展的国际化架构，支持后续添加其他语言而无需修改核心代码
