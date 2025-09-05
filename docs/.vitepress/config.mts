import { defineConfig } from 'vitepress'
import { footnote } from "@mdit/plugin-footnote";

// https://vitepress.dev/reference/site-config
export default defineConfig({
  title: "Frontier",
  description: "Ethereum and EVM compatibility layer for Polkadot",
  base: '/frontier',
  cleanUrls: true,

  themeConfig: {
    docsDir: 'docs',

    nav: [
      { text: 'Home', link: '/' },
      { text: 'Overview', link: '/overview' },
      { text: 'API', link: 'https://polkadot-evm.github.io/frontier/rustdocs/pallet_evm/' }
    ],

    sidebar: [
      {
        text: 'Overview',
        items: [
          { text: 'Overview', link: '/overview' },
          { text: 'Accounts', link: '/accounts' }
        ]
      },
      {
        text: 'Guides',
        items: [
          { text: 'Optimization', link: '/optimization' },
          { text: 'Development workflow', link: '/development-workflow' },
        ]
      }
    ],

    socialLinks: [
      { icon: 'github', link: 'https://github.com/polkadot-evm/frontier' }
    ],

    footer: {
      message: '<a href="https://bitarray.dev/#legal-notice">Legal notice</a>',
      copyright: 'Copyright © 2018-present, Frontier developers'
    },
  },

  markdown: {
    toc: { level: [1, 2] },
    config: (md) => {
      md.use(footnote);
    },
  }
})
