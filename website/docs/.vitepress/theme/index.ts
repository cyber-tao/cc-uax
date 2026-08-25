import type { Theme } from 'vitepress'
import DefaultTheme from 'vitepress/theme'
import HomePage from './components/HomePage.vue'
import InstallTabs from './components/InstallTabs.vue'
import './custom.css'

export default {
  extends: DefaultTheme,
  enhanceApp({ app }) {
    app.component('HomePage', HomePage)
    app.component('InstallTabs', InstallTabs)
  },
} satisfies Theme
