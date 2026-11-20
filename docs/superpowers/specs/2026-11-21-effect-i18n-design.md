# 效果系统多语言（v0.2.15）设计

日期：2026-11-21　状态：已批准（用户当轮确认"可以实施"）

## 目标

中文界面下，效果内容层（调整面板参数、滤镜/转场/效果库的名称与描述）
不再显示英文。做法是在 effect.toml 数据文件中直接携带翻译（方案 B），
接受与上游 Concat 的 effect.toml 格式分歧——这是 YumoCut 的主动决策。

## Schema

effect.toml 的 `[effect]` 与 `[[param]]` 增加可选 `i18n` 子表，键为语言代码：

```toml
[effect]
id = "concat.adjust"
name = "Adjust"                      # 英文原文，必有，回退值
description = "The manual colour panel..."

[effect.i18n."zh-Hans"]
name = "调整"
description = "每段画面自带的手动调色面板：曝光、色调、平衡、锐度与边缘。"

[[param]]
key = "temperature"
group = "Color"
label = "Temperature"

[param.i18n."zh-Hans"]
group = "色彩"
label = "色温"
```

- 翻译字段全部可选；缺失回退英文。enum 的选项文案（labels）同样支持。
- 加新语言零代码改动（语言代码即子表键）。
- manifest.rs 是 deny_unknown_fields 严格 schema，Rust 结构必须同步加字段。

## 代码改动

1. `concat-effects/src/manifest.rs`：Effect 与 Param 结构加
   `i18n: HashMap<String, EffectI18n / ParamI18n>`（serde default），加解析测试。
2. `concat` crate：新 helper 按当前语言取值、英文回退
   （效果渲染用 `i18n::current()`）。
3. `studio/keyframes.rs`：调整/关键帧面板的 label、group 走 helper。
4. `panes/media_bin.rs` 与效果库 UI：name、description 走 helper。

## 翻译范围

- 全部 125 个内置效果包（`concat-effects/packages/*`）：name、description、
  param 的 label/group/enum labels。
- `yumo.volume` 插件：加翻译，version v3 → v4（走插件更新通道）。
- 风格名按通行中文译法（如 Lofi → 低保真，Vaporwave → 蒸汽波）。

## 版本与发布

- 主程序 0.2.15；manifest.json schema 不变，更新机制不动。
- 链路：CI 全绿 → tag v0.2.15 → Release → 下载校验 → 发布
  （GitHub 最新版被替换为汉化版；不改 v0.2.14 已发布资产）。
- 收尾做实机验收：0.2.14 → 0.2.15 更新 + 调整面板汉化核验。

## 决策记录

- 子表 + 语言键（非每语言平铺字段）：扩展语言不改代码，diff 集中。
- 用户明确接受与上游 effect.toml 格式的冲突（"跟上游冲突就对了"）。
- 翻译写入由脚本批量完成（扫描 → 译表 → 写回），保证格式一致。
