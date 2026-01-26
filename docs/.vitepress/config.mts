import { defineConfig } from 'vitepress'

// https://vitepress.dev/reference/site-config
export default defineConfig({
  title: "Mocktioneer",
  description: "Deterministic OpenRTB banner bidder for edge platforms",
  base: "/mocktioneer/",
  themeConfig: {
    // https://vitepress.dev/reference/default-theme-config
    nav: [
      { text: 'Home', link: '/' },
      { text: 'Guide', link: '/guide/getting-started' },
      { text: 'API Reference', link: '/api/' },
      { text: 'Integrations', link: '/integrations/' }
    ],

    sidebar: [
      {
        text: 'Introduction',
        items: [
          { text: 'What is Mocktioneer?', link: '/guide/what-is-mocktioneer' },
          { text: 'Getting Started', link: '/guide/getting-started' }
        ]
      },
      {
        text: 'Configuration',
        items: [
          { text: 'edgezero.toml', link: '/guide/configuration' },
          { text: 'Architecture', link: '/guide/architecture' }
        ]
      },
      {
        text: 'Adapters',
        items: [
          { text: 'Overview', link: '/guide/adapters/' },
          { text: 'Axum (Native)', link: '/guide/adapters/axum' },
          { text: 'Fastly Compute', link: '/guide/adapters/fastly' },
          { text: 'Cloudflare Workers', link: '/guide/adapters/cloudflare' }
        ]
      },
      {
        text: 'API Reference',
        items: [
          { text: 'Overview', link: '/api/' },
          { text: 'OpenRTB Auction', link: '/api/openrtb-auction' },
          { text: 'APS TAM Bid', link: '/api/aps-bid' },
          { text: 'Creatives & Assets', link: '/api/creatives' },
          { text: 'Tracking', link: '/api/tracking' },
          { text: 'Mediation', link: '/api/mediation' },
          { text: 'APS Win Notification', link: '/api/aps-win' }
        ]
      },
      {
        text: 'Integrations',
        items: [
          { text: 'Overview', link: '/integrations/' },
          { text: 'Prebid.js', link: '/integrations/prebidjs' },
          { text: 'Prebid Server', link: '/integrations/prebid-server' }
        ]
      }
    ],

    socialLinks: [
      { icon: 'github', link: 'https://github.com/stackpop/mocktioneer' }
    ],

    footer: {
      message: 'Built with EdgeZero',
      copyright: 'Copyright 2024-present Stackpop'
    },

    search: {
      provider: 'local'
    }
  }
})
