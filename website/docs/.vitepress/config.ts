import { defineConfig } from 'vitepress'

const repo = 'https://github.com/cyber-tao/cc-uax'
const site = 'https://cyber-tao.github.io/cc-uax/'

const guideEn = [
  { text: 'Install', link: '/guide/install' },
  { text: 'CLI', link: '/guide/cli' },
  { text: 'Tutorials', link: '/guide/tutorials' },
  { text: 'Reports', link: '/guide/reports' },
  { text: 'Architecture', link: '/guide/architecture' },
  { text: 'Agent skill', link: '/guide/skill' },
  { text: 'Scope and limits', link: '/guide/limits' },
]

const guideZh = [
  { text: '安装', link: '/zh/guide/install' },
  { text: 'CLI', link: '/zh/guide/cli' },
  { text: '教程', link: '/zh/guide/tutorials' },
  { text: '报告', link: '/zh/guide/reports' },
  { text: '架构', link: '/zh/guide/architecture' },
  { text: 'Agent Skill', link: '/zh/guide/skill' },
  { text: '范围与限制', link: '/zh/guide/limits' },
]

export default defineConfig({
  base: '/cc-uax/',
  title: 'cc-uax',
  titleTemplate: ':title | cc-uax',
  description:
    'Structured analysis of Unreal Engine 5 editor assets for Claude Code, Codex, and other engineering agents.',
  lastUpdated: true,
  cleanUrls: false,
  ignoreDeadLinks: [/^https?:\/\/localhost/],
  appearance: 'dark',
  sitemap: {
    hostname: site,
  },
  head: [
    ['link', { rel: 'icon', type: 'image/svg+xml', href: '/cc-uax/favicon.svg' }],
    ['meta', { name: 'theme-color', content: '#07090f' }],
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:title', content: 'cc-uax' }],
    [
      'meta',
      {
        property: 'og:description',
        content:
          'Structured analysis of versioned UE5.0–5.8 editor packages — without loading Unreal Editor.',
      },
    ],
    ['meta', { property: 'og:url', content: site }],
    ['meta', { name: 'twitter:card', content: 'summary' }],
  ],
  themeConfig: {
    logo: { src: '/logo.svg', alt: 'cc-uax' },
    socialLinks: [{ icon: 'github', link: repo }],
    search: {
      provider: 'local',
      options: {
        locales: {
          zh: {
            translations: {
              button: { buttonText: '搜索', buttonAriaLabel: '搜索' },
              modal: {
                displayDetails: '显示详细列表',
                resetButtonTitle: '重置搜索',
                backButtonTitle: '关闭搜索',
                noResultsText: '没有结果',
                footer: {
                  selectText: '选择',
                  selectKeyAriaLabel: '输入',
                  navigateText: '导航',
                  navigateUpKeyAriaLabel: '上箭头',
                  navigateDownKeyAriaLabel: '下箭头',
                  closeText: '关闭',
                  closeKeyAriaLabel: 'esc',
                },
              },
            },
          },
        },
      },
    },
  },
  locales: {
    root: {
      label: 'English',
      lang: 'en-US',
      description:
        'Structured analysis of Unreal Engine 5 editor assets for Claude Code, Codex, and other engineering agents.',
      themeConfig: {
        siteTitle: 'cc-uax',
        nav: [
          { text: 'Guide', link: '/guide/install' },
          { text: 'CLI', link: '/guide/cli' },
          { text: 'Tutorials', link: '/guide/tutorials' },
          { text: 'GitHub', link: repo },
        ],
        sidebar: [{ text: 'Guide', items: guideEn }],
        outline: { label: 'On this page', level: [2, 3] },
        editLink: {
          pattern: `${repo}/edit/master/website/docs/:path`,
          text: 'Edit this page on GitHub',
        },
        lastUpdated: { text: 'Updated' },
        docFooter: { prev: 'Previous', next: 'Next' },
        footer: {
          message: 'Released under the MIT License.',
          copyright: 'UE5.0–5.8 editor packages · evidence, not guesses',
        },
      },
    },
    zh: {
      label: '简体中文',
      lang: 'zh-CN',
      description:
        '面向 Claude Code、Codex 等工程 Agent 的 Unreal Engine 5 编辑器资产结构化分析工具。',
      themeConfig: {
        siteTitle: 'cc-uax',
        nav: [
          { text: '指南', link: '/zh/guide/install' },
          { text: 'CLI', link: '/zh/guide/cli' },
          { text: '教程', link: '/zh/guide/tutorials' },
          { text: 'GitHub', link: repo },
        ],
        sidebar: [{ text: '指南', items: guideZh }],
        outline: { label: '本页目录', level: [2, 3] },
        editLink: {
          pattern: `${repo}/edit/master/website/docs/:path`,
          text: '在 GitHub 上编辑此页',
        },
        lastUpdated: { text: '更新于' },
        docFooter: { prev: '上一页', next: '下一页' },
        darkModeSwitchLabel: '外观',
        lightModeSwitchTitle: '切换到浅色',
        darkModeSwitchTitle: '切换到深色',
        sidebarMenuLabel: '菜单',
        returnToTopLabel: '回到顶部',
        langMenuLabel: '切换语言',
        footer: {
          message: '以 MIT 许可证发布。',
          copyright: 'UE5.0–5.8 编辑器包 · 证据，而不是猜测',
        },
      },
      markdown: {
        container: {
          tipLabel: '提示',
          warningLabel: '注意',
          dangerLabel: '警告',
          infoLabel: '说明',
          detailsLabel: '详情',
        },
        codeCopyButton: {
          tooltipText: '复制代码',
          copiedText: '已复制',
        },
      },
    },
  },
})
