---
title: 范围与限制
description: cc-uax 的支持范围、拒绝策略和当前具名缺口。
---

# 范围与限制

序列化判断对照 UE5.0–5.8 源码核对。接受窗口是 `FileVersionUE5` 1000–1018。两端都在 `PackageFileSummary::parse` 里按超出范围拒绝，而不是事后推断。

## 支持范围内

有版本信息、未 Cook 的 UE5.0–5.8 编辑器包（`FileVersionUE5` 1000–1018）。证据完整时可以为 `status=complete`。

真实项目已经覆盖过 `FileVersionUE5` 1002–1004、1006–1009 和 1012–1018。1000、1001、1005、1010 和 1011 尚未在真实资产中见到：它们按对照源码核对过的版本门控解码，但还没有真实包加以确认。

## 超出范围

会被拒绝而不是猜着解析：

- UE4 及更早（`FileVersionUE5` 低于 1000）
- 高于 1018（本解析器未见过的布局）
- cooked 包，包括带 `PKG_Cooked` 或 `PKG_UnversionedProperties` 标志的包
- 无版本包
- UE3、大端，以及包级压缩

`cc-uax asset` 对超范围包以退出码 `1` 输出 error 文档。`cc-uax project` 把它记为 `unsupported` 证据，进程仍以 `0` 退出。

UE4 格式的包（`FileVersionUE5` = 0）仍是 `unsupported`，但项目扫描会读取它的 linker 引用表（name、import、soft package reference）。它提供引用边和可达性，并带有 `file_version_ue4`；它的属性和图不会被分析。

## 当前限制

- RigVM 编译字节码（`rig_vm_bytecode`）、压缩 RigHierarchy（`rig_hierarchy`）的源码级还原
- 编译后的 Niagara VM/GPU payload（`niagara_compiled`，具名 capability，不是匿名尾巴）
- 旧版（低于 1012）属性 tag 中的 set/map 元素 struct：tag 不记录 struct 名，除非该属性的声明能从 UE 源码得知（cc-uax 携带的引擎声明，限定在声明它们的类型内），否则保持 opaque
- 布局依赖 UE 反射或类注册表的 payload
- 无法由序列化图、属性、配置或引用证明的运行时行为
- 尚未核对 UE5.0–5.8 序列化契约的插件原生格式

编译后的 Blueprint 字节码已经不在这个清单里：`UStruct`、`UFunction` 和 `UClass` 按结构化字段解码，Kismet 指令流也会反汇编，因此 Blueprint 的函数、变量以及编译代码触及的目标都是可报告的证据。linker 表仍然装不下的，是以字符串形式填进图 pin 的资源路径，`reference_evidence` 会按资产逐个度量这部分残差，而不是留一句没有边界的免责声明。

证据不完整时，下游结论必须保留 `partial`、`unsupported`、diagnostics 和 capability 限制。

同一个 `FileVersionUE5` 不保证同一种布局：UE5.7 和 UE5.8 共享 `1018` 但仍有分叉。这些格式由 custom version、必要时再加上引擎版本来门控。

## 验证

序列化判断对照 UE5.0–5.8 源码核对，并用外部真实编辑器资产验证。真实语料的验收门禁由独立于 workspace crate 维护的 harness 定义，它不作为 workspace 成员提交，也独立于普通 workspace 测试。外部资产和机器相关路径保留在本地，仓库不提交它们。

## 许可

[MIT](https://github.com/cyber-tao/cc-uax/blob/master/LICENSE)
