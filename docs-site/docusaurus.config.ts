import {themes as prismThemes} from 'prism-react-renderer';
import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

const config: Config = {
  title: 'Event Sourcing Platform Docs',
  tagline: 'Docs for concepts, development, SDKs, and architecture',
  url: 'http://localhost',
  baseUrl: '/',
  favicon: 'img/logo.svg',
  organizationName: 'NeuralEmpowerment',
  projectName: 'event-sourcing-platform',
  onBrokenLinks: 'throw',
  onBrokenMarkdownLinks: 'warn',
  i18n: {defaultLocale: 'en', locales: ['en']},
  markdown: {mermaid: true},
  themes: ['@docusaurus/theme-mermaid'],
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
        theme: {customCss: require.resolve('./src/css/custom.css')},
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
      logo: {alt: 'Docs Logo', src: 'img/logo.svg'},
      items: [
        {to: '/', label: 'Overview', position: 'left'},
        {to: '/event-store/index', label: 'Event Store', position: 'left'},
        {to: '/development/fast-testing', label: 'Development', position: 'left'},
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
            {label: 'Overview', to: '/'},
            {label: 'Event Store', to: '/event-store/index'},
            {label: 'Development', to: '/development/fast-testing'},
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
