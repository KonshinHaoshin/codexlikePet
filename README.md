# SakiPet

一个基于 Tauri 2 + Rust + Vite/TypeScript 的透明桌面宠物。它默认长时间
idle，偶尔散步，也支持拖拽、局部视线跟随、多个宠物实例和本地配置保存。

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

- 双击任意桌宠；
- 菜单栏的 SakiPet 图标 → 管理宠物；
- macOS 顶部菜单栏的 SakiPet → 管理宠物。

管理页支持：

- 同时显示不同种类的宠物，每种宠物最多一个实例；
- 单独显示、隐藏和移除实例；
- 停用/启用宠物资源；
- 为每种宠物分别调整大小、透明度、行走速度、散步、安静模式、位置锁定、点击穿透和暂停状态；
- 导入、校验和删除宠物资源。

配置保存于 Tauri 的应用配置目录，宠物导入文件保存于应用数据目录下的
`pets/`，不会写回仓库。

## 导入宠物包

管理页的“导入宠物包”接受 `.zip` 文件。压缩包内需要包含一个 `pet.json`
和对应的 V2 spritesheet，例如：

```text
my-pet.zip
└── my-pet/
    ├── pet.json
    └── spritesheet.webp
```

`pet.json` 必须使用 `spriteVersionNumber: 2`，spritesheet 必须是 8×11 网格、
每格 192×208 像素，即 1536×2288 像素。`id` 只能使用字母、数字、短横线和
下划线，且需要与 `pet.json` 内的 `id` 一致。

## 资源目录

内置宠物放在 `public/pets/<id>/`，列表在 `public/pets/index.json`。每个内置
宠物都需要包含 `pet.json` 和 `spritesheet.webp`。
