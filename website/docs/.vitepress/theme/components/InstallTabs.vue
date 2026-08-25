<script setup lang="ts">
import { computed, ref } from 'vue'
import { useData } from 'vitepress'

const props = withDefaults(
  defineProps<{ compact?: boolean }>(),
  { compact: false },
)

const { lang } = useData()
const zh = computed(() => lang.value.startsWith('zh'))
const tab = ref<'win' | 'unix' | 'src'>('win')
const copied = ref(false)

const commands = {
  win: 'irm https://raw.githubusercontent.com/cyber-tao/cc-uax/master/install.ps1 | iex',
  unix: 'curl -fsSL https://raw.githubusercontent.com/cyber-tao/cc-uax/master/install.sh | bash',
  src: 'cargo build -p cc-uax-cli --release --locked',
} as const

const labels = computed(() =>
  zh.value
    ? { win: 'Windows', unix: 'macOS / Linux', src: '源码构建', copy: '复制', copied: '已复制' }
    : { win: 'Windows', unix: 'macOS / Linux', src: 'From source', copy: 'Copy', copied: 'Copied' },
)

async function copy() {
  try {
    await navigator.clipboard.writeText(commands[tab.value])
    copied.value = true
    window.setTimeout(() => {
      copied.value = false
    }, 1600)
  } catch {
    copied.value = false
  }
}
</script>

<template>
  <div class="install" :class="{ compact: props.compact }">
    <div class="tabs" role="tablist">
      <button
        v-for="key in ['win', 'unix', 'src'] as const"
        :key="key"
        type="button"
        role="tab"
        :aria-selected="tab === key"
        :class="{ active: tab === key }"
        @click="tab = key"
      >
        {{ labels[key] }}
      </button>
    </div>
    <div class="row">
      <code>{{ commands[tab] }}</code>
      <button type="button" class="copy" @click="copy">
        {{ copied ? labels.copied : labels.copy }}
      </button>
    </div>
  </div>
</template>

<style scoped>
.install {
  width: 100%;
  border: 1px solid var(--vp-c-border);
  border-radius: 14px;
  background: color-mix(in srgb, var(--vp-c-bg-elv) 88%, transparent);
  overflow: hidden;
}

.tabs {
  display: flex;
  gap: 4px;
  padding: 8px 8px 0;
}

.tabs button {
  appearance: none;
  border: 0;
  background: transparent;
  color: var(--vp-c-text-2);
  font: inherit;
  font-size: 13px;
  font-weight: 500;
  padding: 8px 12px;
  border-radius: 9px 9px 0 0;
  cursor: pointer;
}

.tabs button.active {
  color: var(--vp-c-text-1);
  background: color-mix(in srgb, var(--vp-c-brand-1) 12%, transparent);
}

.row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 14px 14px;
}

.row code {
  flex: 1;
  min-width: 0;
  font-family: var(--vp-font-family-mono);
  font-size: 13px;
  line-height: 1.45;
  color: var(--uax-mint);
  overflow-x: auto;
  white-space: nowrap;
}

.copy {
  flex-shrink: 0;
  appearance: none;
  border: 1px solid var(--vp-c-border);
  background: var(--vp-c-bg);
  color: var(--vp-c-text-1);
  font: inherit;
  font-size: 12px;
  font-weight: 600;
  padding: 6px 10px;
  border-radius: 8px;
  cursor: pointer;
}

.copy:hover {
  border-color: var(--vp-c-brand-1);
}

.compact .row code {
  font-size: 12px;
}
</style>
