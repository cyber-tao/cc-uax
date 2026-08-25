<script setup lang="ts">
import { computed } from 'vue'
import { useData, withBase } from 'vitepress'
import InstallTabs from './InstallTabs.vue'

const { lang, site } = useData()
const zh = computed(() => lang.value.startsWith('zh'))
const prefix = computed(() => (zh.value ? '/zh' : ''))

function href(path: string) {
  const suffix = site.value.cleanUrls ? '' : '.html'
  return withBase(`${prefix.value}${path}${suffix}`)
}

const t = computed(() =>
  zh.value
    ? {
        eyebrow: 'UE5.0–5.8 · Rust 2024 · MIT',
        title: '不启动编辑器，读懂 Unreal 资产',
        lede: 'cc-uax 把有版本、未 Cook 的 UE5 编辑器包变成带类型和证据的报告。给 Claude Code、Codex 和其他工程 Agent 蓝图执行流、序列化属性、引用邻接和项目级索引——而不是猜测。',
        install: '安装',
        docs: '阅读指南',
        github: 'GitHub',
        whyTitle: '为什么需要它',
        whyBody:
          'Unreal 项目的大量逻辑在二进制 .uasset / .umap 里。源码导向的 Agent 能读 C++ 和配置，却看不到蓝图、PCG、StateTree 或 World Partition 外部包。cc-uax 在不加载 Unreal Editor 的情况下补上这一层证据。',
        scope:
          '范围：有版本信息、未 Cook 的 UE5.0–5.8 编辑器包（FileVersionUE5 1000–1018）。范围之外的包会被拒绝而不是猜着解析。',
        featuresTitle: '它提供什么',
        features: [
          {
            k: '01',
            title: '强类型包分析',
            body: '包元数据、import/export、带标签属性、对象引用、诊断和字节覆盖率。',
          },
          {
            k: '02',
            title: '按图隔离的逻辑',
            body: 'K2/EdGraph 节点始终归属具体图。不会把不同 EventGraph 里的同名节点拼成虚假链路。',
          },
          {
            k: '03',
            title: '专用适配器',
            body: 'RigVM / ControlRig、StateTree、PCG 和 Niagara 编辑器图——只在序列化证据充分时展开。',
          },
          {
            k: '04',
            title: '项目级索引',
            body: '一次扫描建立资产清单、前向/反向邻接，以及 World Partition 外部包归属闭包。',
          },
          {
            k: '05',
            title: '显式不确定性',
            body: '每份报告都有 schema、status、coverage 和 diagnostics。known_opaque 是限制，不是成功。',
          },
          {
            k: '06',
            title: 'Agent Skill',
            body: '随附 skill 要求 Agent 先收集项目证据，再描述玩法或资源使用。',
          },
        ],
        flowTitle: '两个明确的工作流',
        assetTitle: '分析单个资产',
        assetBody: '按 view 选择最小必要证据：身份、图、属性、引用，或完整报告。',
        projectTitle: '分析整个项目',
        projectBody: '一次扫描，再按 --focus 下钻。默认挂载 /Game 和每个插件 Content。',
        archTitle: '单向依赖',
        archBody: '三个 crate，职责分开。core 从不依赖项目扫描或 CLI 呈现。',
        ctaTitle: '先扫项目，再下钻资产',
        ctaBody: '需要引用或可达性时，不要对每个蓝图单独跑一遍 asset。',
        visualFile: 'BP_Player.uasset',
        visualMeta: 'UE 5.4 · FileVersionUE5 1012',
        visualStatus: 'complete',
        visualView: 'logic',
        visualHint: '证据完整时可以为 complete；缺口必须具名。',
      }
    : {
        eyebrow: 'UE5.0–5.8 · Rust 2024 · MIT',
        title: 'Read Unreal assets without the Editor',
        lede: 'cc-uax turns versioned, uncooked UE5 editor packages into typed, evidence-bearing reports. Blueprint flow, serialized properties, reference adjacency, and a project index — for Claude Code, Codex, and other engineering agents.',
        install: 'Install',
        docs: 'Read the guide',
        github: 'GitHub',
        whyTitle: 'Why it exists',
        whyBody:
          'Most of an Unreal project lives in binary .uasset and .umap packages. Source-oriented agents can read C++ and config, but not Blueprint execution, PCG graphs, StateTrees, or World Partition packages. cc-uax supplies that evidence without loading Unreal Editor.',
        scope:
          'Scope: versioned, uncooked UE5.0–5.8 editor packages (FileVersionUE5 1000–1018). Anything outside that range is rejected rather than guessed at.',
        featuresTitle: 'What it provides',
        features: [
          {
            k: '01',
            title: 'Typed package analysis',
            body: 'Metadata, imports/exports, tagged properties, object references, diagnostics, and byte coverage.',
          },
          {
            k: '02',
            title: 'Graph-aware logic',
            body: 'K2/EdGraph nodes stay in their owning graph. Display names are labels, never identity.',
          },
          {
            k: '03',
            title: 'Specialized adapters',
            body: 'RigVM / ControlRig, StateTree, PCG, and Niagara editor graphs — only where serialized evidence supports them.',
          },
          {
            k: '04',
            title: 'Project indexing',
            body: 'One scan builds inventory, forward/reverse adjacency, and World Partition ownership closure.',
          },
          {
            k: '05',
            title: 'Explicit uncertainty',
            body: 'Every report carries a schema, status, coverage, and diagnostics. known_opaque is a limitation, not success.',
          },
          {
            k: '06',
            title: 'Agent skill',
            body: 'The bundled skill makes Claude Code and Codex gather project evidence before describing gameplay.',
          },
        ],
        flowTitle: 'Two explicit workflows',
        assetTitle: 'Analyze one asset',
        assetBody: 'Pick the smallest --view that answers the question: summary, logic, properties, references, or full.',
        projectTitle: 'Analyze a project',
        projectBody: 'Scan once, then drill in with --focus. Default mounts are /Game plus every plugin Content root.',
        archTitle: 'One-way dependencies',
        archBody: 'Three crates, separated on purpose. Core never owns filesystem scanning or JSON presentation.',
        ctaTitle: 'Scan the project first, then drill in',
        ctaBody: 'Do not run asset once per Blueprint when you need references or reachability.',
        visualFile: 'BP_Player.uasset',
        visualMeta: 'UE 5.4 · FileVersionUE5 1012',
        visualStatus: 'complete',
        visualView: 'logic',
        visualHint: 'A report may be complete when evidence is complete. Gaps must be named.',
      },
)
</script>

<template>
  <div class="landing">
    <div class="glow" aria-hidden="true" />

    <section class="hero">
      <div class="copy">
        <p class="eyebrow">{{ t.eyebrow }}</p>
        <h1>{{ t.title }}</h1>
        <p class="lede">{{ t.lede }}</p>
        <div class="actions">
          <a class="btn primary" :href="href('/guide/install')">{{ t.install }}</a>
          <a class="btn ghost" :href="href('/guide/cli')">{{ t.docs }}</a>
          <a class="btn ghost" href="https://github.com/cyber-tao/cc-uax" target="_blank" rel="noreferrer">
            {{ t.github }}
          </a>
        </div>
        <InstallTabs compact />
      </div>

      <aside class="visual" aria-hidden="true">
        <div class="panel">
          <header>
            <span class="dot" />
            <span class="dot gold" />
            <span class="dot blue" />
            <strong>{{ t.visualFile }}</strong>
          </header>
          <p class="meta">{{ t.visualMeta }}</p>
          <dl>
            <div>
              <dt>status</dt>
              <dd class="ok">{{ t.visualStatus }}</dd>
            </div>
            <div>
              <dt>view</dt>
              <dd>{{ t.visualView }}</dd>
            </div>
            <div>
              <dt>nodes</dt>
              <dd>128</dd>
            </div>
            <div>
              <dt>edges</dt>
              <dd>214</dd>
            </div>
          </dl>
          <svg class="graph" viewBox="0 0 360 150" fill="none">
            <path class="wire exec" d="M78 38h52" />
            <path class="wire data" d="M78 58h52" />
            <path class="wire exec" d="M186 38h52" />
            <path class="wire data" d="M186 78c20 0 20 40 52 40" />
            <rect x="16" y="22" width="62" height="52" rx="8" class="node" />
            <rect x="130" y="22" width="56" height="72" rx="8" class="node" />
            <rect x="238" y="18" width="70" height="44" rx="8" class="node mint" />
            <rect x="238" y="96" width="70" height="36" rx="8" class="node gold" />
            <text x="47" y="52" text-anchor="middle">BeginPlay</text>
            <text x="158" y="54" text-anchor="middle">Branch</text>
            <text x="273" y="44" text-anchor="middle">Set Speed</text>
            <text x="273" y="118" text-anchor="middle">Print</text>
          </svg>
          <p class="hint">{{ t.visualHint }}</p>
        </div>
      </aside>
    </section>

    <section class="why">
      <div>
        <h2>{{ t.whyTitle }}</h2>
        <p>{{ t.whyBody }}</p>
      </div>
      <blockquote>{{ t.scope }}</blockquote>
    </section>

    <section>
      <h2>{{ t.featuresTitle }}</h2>
      <div class="grid">
        <article v-for="item in t.features" :key="item.k">
          <span>{{ item.k }}</span>
          <h3>{{ item.title }}</h3>
          <p>{{ item.body }}</p>
        </article>
      </div>
    </section>

    <section>
      <h2>{{ t.flowTitle }}</h2>
      <div class="flows">
        <a class="flow" :href="href('/guide/cli')">
          <code>cc-uax asset</code>
          <h3>{{ t.assetTitle }}</h3>
          <p>{{ t.assetBody }}</p>
        </a>
        <a class="flow" :href="href('/guide/tutorials')">
          <code>cc-uax project</code>
          <h3>{{ t.projectTitle }}</h3>
          <p>{{ t.projectBody }}</p>
        </a>
      </div>
    </section>

    <section class="arch">
      <div>
        <h2>{{ t.archTitle }}</h2>
        <p>{{ t.archBody }}</p>
        <a class="more" :href="href('/guide/architecture')">{{ zh ? '查看架构' : 'See architecture' }}</a>
      </div>
      <ol>
        <li>
          <strong>cc-uax-cli</strong>
          <span>{{ zh ? '命令、view / focus、JSON 渲染' : 'Commands, views / focus, JSON rendering' }}</span>
        </li>
        <li>
          <strong>cc-uax-project</strong>
          <span>{{ zh ? '发现、清单、邻接、归属、缓存' : 'Discovery, inventory, adjacency, ownership, cache' }}</span>
        </li>
        <li>
          <strong>cc-uax-core</strong>
          <span>{{ zh ? '绑定字节的解析、图、coverage' : 'Byte-bound parsing, graphs, coverage' }}</span>
        </li>
      </ol>
    </section>

    <section class="cta">
      <h2>{{ t.ctaTitle }}</h2>
      <p>{{ t.ctaBody }}</p>
      <div class="actions">
        <a class="btn primary" :href="href('/guide/install')">{{ t.install }}</a>
        <a class="btn ghost" :href="href('/guide/tutorials')">{{ zh ? '使用教程' : 'Tutorials' }}</a>
      </div>
    </section>
  </div>
</template>

<style scoped>
.landing {
  position: relative;
  max-width: 1120px;
  margin: 0 auto;
  padding: 96px 24px 80px;
}

.glow {
  pointer-events: none;
  position: absolute;
  inset: -40px auto auto 10%;
  width: min(720px, 80vw);
  height: 420px;
  background: radial-gradient(circle at 30% 30%, rgba(62, 224, 178, 0.16), transparent 58%),
    radial-gradient(circle at 80% 20%, rgba(122, 162, 255, 0.14), transparent 50%);
  filter: blur(8px);
}

section {
  position: relative;
  margin-bottom: 88px;
}

h2 {
  margin: 0 0 20px;
  font-size: 28px;
  letter-spacing: -0.04em;
  line-height: 1.2;
}

.hero {
  display: grid;
  grid-template-columns: minmax(0, 1.05fr) minmax(280px, 0.95fr);
  gap: 48px;
  align-items: center;
}

.eyebrow {
  margin: 0 0 14px;
  color: var(--uax-mint);
  font-family: var(--vp-font-family-mono);
  font-size: 12px;
  letter-spacing: 0.08em;
  text-transform: uppercase;
}

h1 {
  margin: 0 0 18px;
  font-size: clamp(36px, 5vw, 56px);
  line-height: 1.05;
  letter-spacing: -0.045em;
}

.lede,
.why p,
.arch p,
.cta p,
.grid p,
.flow p {
  color: var(--vp-c-text-2);
  line-height: 1.65;
}

.lede {
  margin: 0 0 28px;
  font-size: 18px;
}

.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  margin-bottom: 22px;
}

.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-height: 42px;
  padding: 0 16px;
  border-radius: 999px;
  font-weight: 600;
  font-size: 14px;
  text-decoration: none;
  border: 1px solid transparent;
}

.btn.primary {
  background: var(--vp-c-brand-1);
  color: #062018;
}

.btn.ghost {
  border-color: var(--vp-c-border);
  color: var(--vp-c-text-1);
}

.btn.ghost:hover,
.more:hover {
  border-color: var(--vp-c-brand-1);
  color: var(--vp-c-brand-1);
}

.panel {
  border: 1px solid #243049;
  border-radius: 20px;
  background: linear-gradient(180deg, #101628, #0b1020);
  padding: 18px 18px 16px;
  box-shadow: 0 24px 80px rgba(0, 0, 0, 0.35);
  color: #eef2f8;
}

.panel header {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
}

.dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: var(--uax-mint);
}

.dot.gold {
  background: var(--uax-gold);
}

.dot.blue {
  background: var(--uax-blue);
}

.panel strong {
  margin-left: 6px;
  font-family: var(--vp-font-family-mono);
  font-size: 13px;
}

.meta,
.hint {
  margin: 0;
  color: #9aa6c2;
  font-size: 12px;
}

.hint {
  margin-top: 8px;
}

dl {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px 16px;
  margin: 16px 0 8px;
}

dt {
  color: #6d7893;
  font-size: 11px;
  text-transform: uppercase;
  letter-spacing: 0.06em;
}

dd {
  margin: 2px 0 0;
  font-family: var(--vp-font-family-mono);
  font-size: 15px;
}

dd.ok {
  color: var(--uax-mint);
}

.graph {
  width: 100%;
  margin-top: 8px;
}

.graph .node {
  fill: #151c30;
  stroke: #31405f;
}

.graph .node.mint {
  stroke: #3ee0b2;
}

.graph .node.gold {
  stroke: #f0b429;
}

.graph text {
  fill: #c9d3e8;
  font-size: 11px;
  font-family: var(--vp-font-family-mono);
}

.graph .wire {
  stroke-width: 2;
  stroke-linecap: round;
}

.graph .exec {
  stroke: #eef2f8;
}

.graph .data {
  stroke: #7aa2ff;
}

.why {
  display: grid;
  grid-template-columns: 1.2fr 0.8fr;
  gap: 28px;
  align-items: start;
}

.why blockquote {
  margin: 0;
  padding: 18px 20px;
  border: 1px solid var(--vp-c-border);
  border-left: 3px solid var(--vp-c-brand-1);
  border-radius: 14px;
  background: var(--vp-c-bg-soft);
  color: var(--vp-c-text-2);
  font-size: 14px;
  line-height: 1.6;
}

.grid {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 14px;
}

.grid article {
  padding: 18px 18px 16px;
  border: 1px solid var(--vp-c-border);
  border-radius: 16px;
  background: var(--vp-c-bg-soft);
}

.grid span {
  color: var(--uax-mint);
  font-family: var(--vp-font-family-mono);
  font-size: 12px;
}

.grid h3,
.flow h3 {
  margin: 8px 0 8px;
  font-size: 18px;
  letter-spacing: -0.03em;
}

.grid p,
.flow p {
  margin: 0;
  font-size: 14px;
}

.flows {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 14px;
}

.flow {
  display: block;
  padding: 22px;
  border: 1px solid var(--vp-c-border);
  border-radius: 16px;
  text-decoration: none;
  color: inherit;
  background: var(--vp-c-bg-soft);
}

.flow:hover {
  border-color: var(--vp-c-brand-1);
}

.flow code {
  color: var(--uax-mint);
  font-size: 13px;
}

.arch {
  display: grid;
  grid-template-columns: 0.85fr 1.15fr;
  gap: 28px;
  align-items: center;
}

.arch ol {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  gap: 10px;
}

.arch li {
  display: grid;
  gap: 4px;
  padding: 14px 16px;
  border: 1px solid var(--vp-c-border);
  border-radius: 14px;
  background: var(--vp-c-bg-soft);
}

.arch li strong {
  font-family: var(--vp-font-family-mono);
  font-size: 14px;
}

.arch li span {
  color: var(--vp-c-text-2);
  font-size: 13px;
}

.more {
  display: inline-block;
  margin-top: 14px;
  color: var(--vp-c-brand-1);
  font-weight: 600;
  text-decoration: none;
}

.cta {
  text-align: center;
  padding: 36px 20px 8px;
}

.cta .actions {
  justify-content: center;
  margin-bottom: 0;
}

@media (max-width: 920px) {
  .hero,
  .why,
  .arch,
  .grid,
  .flows {
    grid-template-columns: 1fr;
  }

  .landing {
    padding-top: 72px;
  }
}
</style>
