# nav-extras：CATIA 弦按导航的模块化接入与上游合并指南

> **文档日期：2026-11-10**（功能实现于 2026-11-09，模块化重构于 2026-11-10）
> 配套文档：`docs/design/catia-navigation.md`（方案评审与实现记录）
> 面向读者：后续接手本仓库的工程师或 AI 助手。按本文操作即可在同步上游代码后完整恢复 CATIA 导航功能。

---

## 1. 这是什么

SolidCraft 的导航预设注册表（`NAV_PRESETS`）是"单键 + 修饰键"模型，表达不了 CATIA V5 的
**双键弦按换挡**：按住中键拖动 = 平移；按住中键再按住左/右键 = 旋转；**中键不放、松开侧键** =
缩放（再按侧键又变回旋转）；单击中键 = 把所指点移到视图中心；双击中键 = Fit。

本功能拆成两层：

| 层 | 位置 | 职责 | 依赖 |
|---|---|---|---|
| **纯逻辑（梯子）** | `crates/nav-extras`（crate 名 `solidcraft-nav-extras`） | 弦按规格、换挡决策、点击位移阈值 | **零依赖**，无 UI 类型 |
| **适配层（钩子）** | `crates/ui-egui/src/nav_extras.rs` | 把梯子接到相机、偏好、右键菜单、图例、场景测试 | `SolveApp` 等 crate 内部 |

**设计原则**：`pub(crate)` 的 `SolveApp` 内部状态出不了 ui-egui crate，所以凡是碰相机/偏好
的代码必须留在 ui-egui 内的 `nav_extras` 模块里；`crates/nav-extras` 只放可以被布尔值完全
决定、不需要窗口就能测试的判定逻辑。**上游合并时只有"钩子表"（§4）里列的文件可能冲突，
其余全部是新增文件（零冲突）。**

## 2. `solidcraft-nav-extras` 公开 API（纯逻辑层）

```rust
// crates/nav-extras/src/lib.rs，零依赖，编译和测试都在 1 秒内

pub enum SideButton { Left, Middle, Right }        // 鼠标键的镜像（不导入 ui-egui 的枚举）

pub enum Gear { Anchor, Held, Tapped }             // 本帧拖拽该做什么

pub struct ChordSpec {
    pub anchor: SideButton,                        // 必须一直按住的锚键（CATIA：中键）
    pub sides: &'static [SideButton],              // 换挡的侧键（CATIA：左/右键）
    pub recentre: bool,                            // 锚键裸单击是否重设视图中心
}

impl ChordSpec {
    pub fn is_side(&self, button: SideButton) -> bool;
    pub fn gear(&self, side_down: bool, latched: bool) -> Gear;
    // 决策表：侧键按着 -> Held；侧键松开但闩锁在 -> Tapped；否则 -> Anchor。
    // 闩锁（latched）= 自锚键按下以来侧键曾被按过；宿主在锚键松开时清零。
}

pub const CATIA: ChordSpec;                        // CATIA V5 的弦按规格
pub const CLICK_WOBBLE: f32;                       // 判定"单击"的最大位移（6pt，与 egui 一致）
```

单元测试在 `crates/nav-extras/src/lib.rs` 的 `tests` 模块，覆盖梯子全状态行走、CATIA 规格、
无 recentre 变体、阈值范围。**跑法（秒级）**：

```bash
cargo test -p solidcraft-nav-extras
```

## 3. ui-egui 内 `nav_extras` 模块 API（适配层）

文件：`crates/ui-egui/src/nav_extras.rs`（在 `lib.rs` 以 `mod nav_extras;` 挂载）。

| 函数 | 签名 | 用途 / 调用点 |
|---|---|---|
| `NavChord` | `{ spec: ChordSpec, held: NavMode, tapped: NavMode }` | 弦按本体；`spec.gear()` 出 `Gear`，这里把 `Gear` 翻译成相机 `NavMode` |
| `catia_chord()` | `pub const fn() -> NavChord` | CATIA 预设的弦按常量（`held: Orbit, tapped: Zoom, recentre: true`） |
| `chord_anchor` | `(app: &SolveApp) -> Option<NavChord>` | 当前预设的弦按（无则 `None`） |
| `side_down` | `(chord, primary_down, middle_down, secondary_down: bool) -> bool` | 本帧是否有弦按侧键按着 |
| `drag_mode` | `(app: &mut SolveApp, button: NavButton, chord, side_down, mods) -> Option<NavMode>` | **核心**：本帧拖拽模式。有弦按且是锚键 → 梯子决策；否则走原 `nav_drag_keeps_mode` 逻辑 |
| `chord_reset` | `(app: &mut SolveApp)` | 锚键松开时清闩锁（下次拖拽从平移开始） |
| `right_menu_blocked` | `(app: &SolveApp, ui: &egui::Ui) -> bool` | 锚键按住期间抑制视图右键菜单（否则旋转被菜单打断） |
| `middle_click_recentre` | `(app: &mut SolveApp, ui: &egui::Ui, inside: bool, rect: Rect)` | 单击中键重设视图中心（自记按下位置 + `CLICK_WOBBLE` 判定，规避 egui 把合成拖拽释放帧误判为单击） |
| `legend_rows` | `(app: &SolveApp, gestures: &mut Vec<(Vec<String>, String)>)` | 偏好页图例前插两行（锚键+侧键 按住→旋转 / 单击→缩放） |
| `synthetic_chord_press` / `_release` | `(app, name: Option<&str>, tap: bool, pos, modifiers)` | 场景测试 `ui.drag` 的 `chord` / `chord_tap` 参数合成两键事件 |

适配层自带测试（`nav_extras::tests`）：CATIA 预设与常量一致性、梯子经 `NavMode` 的四态、
单键预设不受影响。**注意：这些测试在 ui-egui 的测试二进制里，链接很慢**（见 §6）。

## 4. 钩子表（上游合并时唯一需要核对的地方）

上游文件中的全部残留改动。每处 1–3 行 + 注释；`git merge upstream/main` 后如报冲突，按下表
把钩子行**原样保留**即可：

| # | 文件 | 位置 | 钩子内容 | 作用 |
|---|---|---|---|---|
| 1 | `crates/ui-egui/src/lib.rs` | 模块声明区（`pub mod mouse_gesture;` 后） | `mod nav_extras;` | 挂载适配层 |
| 2 | `crates/ui-egui/src/prefs.rs` | 原 `bind` 常量函数之后 | `pub use crate::nav_extras::NavChord;`（带 3 行注释） | 向旧路径 `crate::prefs::NavChord` 保持兼容 |
| 3 | `crates/ui-egui/src/prefs.rs` | `NavPreset` 结构体 | 字段 `pub chord: Option<NavChord>,` + 注释 | 预设携带弦按 |
| 4 | `crates/ui-egui/src/prefs.rs` | `impl NavPreset` | 方法 `chord_mode(...)`（5 行） | 预设级梯子入口（测试用） |
| 5 | `crates/ui-egui/src/prefs.rs` | `NAV_PRESETS` 数组，`creo` 与 `nx` 之间 | 整个 `id: "catia"` 预设项（约 15 行，`chord: Some(crate::nav_extras::catia_chord())`） | 注册 CATIA 预设 |
| 6 | `crates/ui-egui/src/navigation.rs` | `handle()` 内 `button_mode` 计算处 | 弦按块（约 15 行，调 `chord_anchor`/`side_down`/`drag_mode`/`chord_reset`） | 每帧换挡 |
| 7 | `crates/ui-egui/src/navigation.rs` | `middle_double_click(...)` 调用后 | `crate::nav_extras::middle_click_recentre(app, ui, inside, rect);` | 单击重设中心 |
| 8 | `crates/ui-egui/src/viewport.rs` | `ViewportState` 结构体 | 字段 `chord_latched: bool` + `chord_press: Option<Pos2>`（带注释） | 换挡闩锁与按下位置 |
| 9 | `crates/ui-egui/src/viewport.rs` | `show()` 内右键菜单分支 | `let chord_right = crate::nav_extras::right_menu_blocked(app, ui);` | 右键菜单守卫 |
| 10 | `crates/ui-egui/src/prefs/navigation.rs` | `mapping_rows()` 内 | `crate::nav_extras::legend_rows(app, &mut gestures);` | 图例两行 |
| 11 | `crates/ui-egui/src/control.rs` | `ui.drag` 分支 | `synthetic_chord_press(...)` / `synthetic_chord_release(...)` 两处调用 | 场景测试合成弦按 |
| 12 | `crates/ui-egui/src/view_home_tests.rs` | `middle_drags_and_single_clicks_do_not_go_home` | 跳过带 `recentre` 的预设（3 行） | 旧断言与 CATIA 单击语义冲突 |
| 13 | 根 `Cargo.toml` | `[workspace.dependencies]` | `solidcraft-nav-extras = { path = "crates/nav-extras" }` | 注册 workspace 依赖 |
| 14 | `crates/ui-egui/Cargo.toml` | `[dependencies]` | `solidcraft-nav-extras = { workspace = true }` | 一行引用 |

**完全新增、永不冲突的文件**：

- `crates/nav-extras/`（Cargo.toml + lib.rs，含单元测试）
- `crates/ui-egui/src/nav_extras.rs`（适配层 + 测试）
- `crates/ui-egui/tests/scenarios/catia_navigation_bindings.json`（端到端场景）
- `assets/locales/pending/catia-navigation.json`（en/zh/zh-TW 文案）
- `docs/design/catia-navigation.md`、本文档

另有 7 个语言包（`assets/locales/*.json`）追加了 `nav.catia.*` 与 `nav.chord.*` 键、
`NOTICE`/`README.md`/`docs/book/src/guide/ui-tour.md` 追加了 CATIA 商标声明与手册段落——
均为追加式改动，冲突概率低，冲突时保留双方即可。

## 5. 上游同步操作流程（给后续 AI 的直接指令）

```bash
cd /Users/honey/WorkBuddy/solidcraft
git remote -v                      # 应有 origin（用户 fork）与 upstream（tangency-labs/solidcraft）
                                   # 若无 upstream：git remote add upstream https://github.com/tangency-labs/solidcraft.git
git fetch upstream                 # 走系统代理：env -u HTTP_PROXY -u HTTPS_PROXY git -c http.proxy=http://127.0.0.1:7897 -c https.proxy=http://127.0.0.1:7897 fetch upstream --tags
git merge upstream/main            # 或 rebase；推荐 merge（保留功能史）
```

冲突处理规则：

1. 只会（且只应）出现在 §4 表中的 14 个文件里；
2. 合并原则：**上游的新逻辑 + 本侧的钩子行共存**。例如 `navigation.rs` 的 `button_mode` 若被
   上游重写，把钩子块重新套在新实现外层（`drag_mode` 已封装原逻辑，通常只需保持调用行）；
3. `NAV_PRESETS` 数组冲突：保留上游新增的预设项 + 本侧的 catia 项；
4. 若上游改了 `NavButton`/`NavMode`/`SolveApp` 相关签名，需同步修改 `nav_extras.rs` 的对应
   函数（这是唯一可能需要"适配"而非"保留"的地方）；
5. 合并后必须跑 §6 的验证清单，全绿才算完成。

**给新预设的贡献者**（例如以后加 NX 真弦按）：在 `crates/nav-extras` 定义新的
`ChordSpec` 常量 → 在 `nav_extras.rs` 加对应 `pub const fn`（给 `held`/`tapped` 选
`NavMode`）→ 在 `NAV_PRESETS` 加预设项引用它 → 场景测试加一组断言。不需要改任何其他钩子。

## 6. 验证清单（合并/改动后必跑）

```bash
# ① 秒级：梯子逻辑
cargo test -p solidcraft-nav-extras

# ② 编译检查（先跑这个，快）
cargo check -p solidcraft-ui-egui

# ③ 端到端场景（平移/重设中心/弦按旋转/侧键单击缩放/预设隔离，5 组断言）
cargo test -p solidcraft-ui-egui --test scenarios catia_navigation_bindings

# ④ 回归（慢，见下方构建注意）
CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 cargo test -j 4 -p solidcraft-ui-egui --lib -- --test-threads=2
```

**构建注意（2026-11-09/10 实测的坑，务必遵守）**：

- ui-egui 的测试二进制默认 profile 链接约 **43 分钟**；`CARGO_PROFILE_TEST_DEBUG=0` 后约
  20 分钟、二进制从 ~700MB 降到 369MB。**不要**在默认 profile 下跑它；
- 中途被 kill（SIGKILL/OOM）会留下损坏产物，表现为链接期大量 `_anon.*.llvm.*` 未定义符号
  → `cargo clean -p solidcraft-ui-egui` 后重跑；
- 并发用 `-j 4`（默认并行会 OOM）；测试线程 `--test-threads=2`；
- 已知与本功能无关的环境性失败：GPU 相关测试在沙箱里报
  `HostUnavailable: Wait timed out`（约 11 个），在真机应通过；
- 语言包：新增英文 i18n 键必须同步 ar/he/ja/vi/zh（`i18n.rs` 的 `COMPLETE` 名单会强制检查）。

## 7. 变更历史

| 日期 | 内容 |
|---|---|
| 2026-11-09 | CATIA 导航按方案 B 落地（弦按底座直接改在 ui-egui 内），场景与全量回归通过 |
| 2026-11-10 | 模块化重构：纯逻辑抽到 `crates/nav-extras`，上游文件足迹从 ~256 行收缩为钩子表 14 处（多为单行）；本指南成文 |
