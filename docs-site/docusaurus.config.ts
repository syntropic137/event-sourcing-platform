import { themes as prismThemes } from 'prism-react-renderer';
import type { Config } from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

// GitHub Pages serves project sites at /<repo-name>/, so baseUrl must match.
// For local development, you can override these with environment variables:
//   BASE_URL=/ pnpm --filter docs-site start
// This allows local dev to serve at http://localhost:3000/ instead of /event-sourcing-platform/
const config: Config = {
  title: 'Event Sourcing Platform Docs',
  tagline: 'Docs for concepts, development, SDKs, and architecture',
  url: process.env.SITE_URL || 'https://neuralempowerment.github.io',
  baseUrl: process.env.BASE_URL || '/event-sourcing-platform/',
  favicon: 'img/logo.svg',
  organizationName: 'NeuralEmpowerment',
  projectName: 'event-sourcing-platform',
  onBrokenLinks: 'throw',
  i18n: { defaultLocale: 'en', locales: ['en'] },
  markdown: {
    mermaid: true,
    hooks: {
      onBrokenMarkdownLinks: 'warn',
    },
  },
  themes: [
    '@docusaurus/theme-mermaid',
    [
      require.resolve('@easyops-cn/docusaurus-search-local'),
      {
        hashed: true,
        language: ['en'],
        highlightSearchTermsOnTargetPage: true,
        explicitSearchResultPath: true,
        indexDocs: true,
        indexBlog: false,
        indexPages: false,
        docsRouteBasePath: '/',
        searchResultLimits: 10,
        searchResultContextMaxLength: 50,
      },
    ],
  ],
  presets: [
    [
      'classic',
      {
        docs: {
          path: 'docs',
          routeBasePath: '/',
          sidebarPath: './sidebars.ts',
          editUrl: 'https://github.com/NeuralEmpowerment/event-sourcing-platform/edit/main/docs-site/docs',
          showLastUpdateAuthor: true,
          showLastUpdateTime: true,
        },
        blog: false,
        theme: { customCss: require.resolve('./src/css/custom.css') },
      } satisfies Preset.Options,
    ],
  ],
  themeConfig: {
    image: 'img/social-card.jpg',
    colorMode: {
      defaultMode: 'dark',
      disableSwitch: false,
      respectPrefersColorScheme: true,
    },
    navbar: {
      title: '📚 Docs',
      logo: { alt: 'Docs Logo', src: 'img/logo.svg' },
      items: [
        { to: '/overview/intro', label: 'Overview', position: 'left' },
        { to: '/event-store/index', label: 'Event Store', position: 'left' },
        { to: '/vsa/index', label: 'VSA Manager', position: 'left' },
        { to: '/development/fast-testing', label: 'Development', position: 'left' },
        {
          href: 'https://github.com/NeuralEmpowerment/event-sourcing-platform',
          label: 'GitHub',
          position: 'right',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Documentation',
          items: [
            { label: 'Overview', to: '/overview/intro' },
            { label: 'Event Store', to: '/event-store/index' },
            { label: 'VSA Manager', to: '/vsa/index' },
            { label: 'Development', to: '/development/fast-testing' },
          ],
        },
      ],
      copyright: `© ${new Date().getFullYear()} Event Sourcing Platform | Built with Docusaurus`,
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.vsDark,
    },
    docs: {
      sidebar: {
        hideable: true,
        autoCollapseCategories: true,
      },
    },
    tableOfContents: {
      minHeadingLevel: 2,
      maxHeadingLevel: 5,
    },
  },
};

export default config;
