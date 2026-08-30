# SakiPet

一个基于 Tauri 2 + Rust + Vite/TypeScript 的透明桌面宠物。它默认长时间
idle，偶尔散步或斜向上爬，也支持拖拽、局部视线跟随、多个宠物实例和本地配置保存。

## 开发

```bash
COPYFILE_DISABLE=1 npm install
COPYFILE_DISABLE=1 npm run tauri dev
```

快速检查：

```bash
npm run build
cd src-tauri && COPYFILE_DISABLE=1 cargo check
```

由于仓库位于 ExFAT 卷，Rust 的构建目录由
`src-tauri/.cargo/config.toml` 重定向到本机缓存目录，避免 macOS 的
AppleDouble `._*` 文件干扰 Tauri 构建。

## 宠物管理

启动后可以通过以下入口打开管理页：

- 桌宠图标的托盘菜单 → 管理宠物；
- macOS Dock 图标右键菜单 → 管理宠物；
- 菜单栏的 SakiPet 图标 → 管理宠物；
- macOS 顶部菜单栏的 SakiPet → 管理宠物。

双击桌宠会显示它的下一句台词，并触发一次互动动作。

管理页支持：

- 同时显示不同种类的宠物，每种宠物最多一个实例；
- 单独显示、隐藏和移除实例；
- 停用/启用宠物资源；
- 为每种宠物分别调整大小、透明度、行走速度、散步、安静模式、位置锁定、点击穿透、全屏显示和暂停状态；
- 导入、校验和删除宠物资源。

配置保存于 Tauri 的应用配置目录，宠物导入文件保存于应用数据目录下的
`pets/`，不会写回仓库。

### Windows 全屏与虚拟桌面

Windows 下开启“全屏显示”后，宠物窗口会使用 `HWND_TOPMOST`，因此可以覆盖普通窗口和无边框全屏窗口；独占全屏游戏关闭桌面合成时，系统本身不允许普通桌面窗口覆盖，宠物也不会强行绕过这个限制。

如果系统支持 Explorer 的虚拟桌面固定接口，宠物还会自动固定到所有虚拟桌面；接口不可用时会退化为当前虚拟桌面显示，不影响启动和普通使用。

macOS 下开启“全屏显示”后，宠物窗口会提升为非激活的 `NSPanel`，加入所有
Space 和其他应用的全屏 Space，并使用 `screenSaver` 窗口层级显示。应用保持普通的
`Regular` 激活策略，因此 Dock 图标和顶部菜单栏的“管理宠物”入口始终可用。如果视频或应用使用
独占显示输出，系统仍可能不允许普通桌面窗口覆盖。

## 导入宠物包

管理页的“导入宠物包”接受 `.zip` 文件。压缩包内需要包含一个 `pet.json`
和对应的 V2 spritesheet，例如：

```text
my-pet.zip
└── my-pet/
    ├── pet.json
    ├── character.json  # 可选
    └── spritesheet.webp
```

可以额外放入 `character.json` 配置宠物在不同情况下显示的台词：

```json
{
  "version": 1,
  "doubleClick": [
    "你好呀！",
    "今天也一起玩吧。"
  ],
  "click": [
    "怎么啦？",
    "我在这里哦。"
  ],
  "rightClick": [
    "轻一点嘛。"
  ],
  "walk": [
    "我去附近转转。",
    "散步时间到了！"
  ],
  "drag": [
    "要带我去哪里呀？",
    "我来啦！"
  ],
  "idle": [
    "这里待着也很舒服。",
    "要不要陪我说说话？"
  ]
}
```

字段说明：

- `version`：格式版本，目前必须为 `1`。
- `doubleClick`：字符串数组。双击宠物时按顺序轮换显示。
- `click`：字符串数组。单击宠物时按顺序轮换显示。
- `rightClick`：字符串数组。右键点击宠物时按顺序轮换显示。
- `walk`：字符串数组。宠物开始自主行走时按顺序轮换显示。
- `drag`：字符串数组。拖拽宠物移动时按顺序轮换显示。
- `idle`：字符串数组。宠物长时间没有互动时偶尔显示。

每个数组都可以设置为空数组 `[]`，表示关闭对应情况下的台词。未填写的字段会使用默认台词。

格式限制：

- `character.json` 最大 32 KB；
- 每个台词数组最多 32 条台词；
- 单条台词最多 240 个字符；
- 空字符串会被忽略；
- 缺少该文件、格式错误或没有有效台词时，会使用默认台词。

`pet.json` 必须使用 `spriteVersionNumber: 2`，spritesheet 必须是 8×11 网格、
每格 192×208 像素，即 1536×2288 像素。`id` 只能使用字母、数字、短横线和
下划线，且需要与 `pet.json` 内的 `id` 一致。

## 资源目录

内置宠物放在 `public/pets/<id>/`，列表在 `public/pets/index.json`。每个内置
宠物都需要包含 `pet.json` 和 `spritesheet.webp`，可选包含 `character.json`。
