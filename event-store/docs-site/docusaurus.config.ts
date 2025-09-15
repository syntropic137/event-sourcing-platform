import { themes as prismThemes } from 'prism-react-renderer';
import type { Config } from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

const config: Config = {
  title: 'EventStore (Rust) Docs',
  tagline: 'Durable streams, optimistic concurrency, projections',
  url: 'http://localhost',
  baseUrl: '/',
  onBrokenLinks: 'throw',
  onBrokenMarkdownLinks: 'warn',
  favicon: 'img/favicon.ico',
  organizationName: 'neural',
  projectName: 'rust-event-store',
  i18n: { defaultLocale: 'en', locales: ['en'] },
  presets: [
    [
      'classic',
      {
        docs: {
          sidebarPath: './sidebars.ts',
          routeBasePath: '/',
          // Using symlinks to import docs from parent docs directory
          editUrl: 'https://github.com/NeuralEmpowerment/node-experiment-lab/edit/main/experiments/005-rust-event-store/docs',
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
      title: '🗄️ EventStore',
      logo: {
        alt: 'EventStore Logo',
        src: 'img/logo.svg',
      },
      items: [
        {
          type: 'docSidebar',
          sidebarId: 'tutorialSidebar',
          position: 'left',
          label: '📚 Docs',
        },
        {
          href: 'https://github.com/NeuralEmpowerment/node-experiment-lab',
          label: '💻 GitHub',
          position: 'right',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: '📖 Documentation',
          items: [
            { label: 'Getting Started', to: '/concepts/ubiquitous-language' },
            { label: 'SDKs', to: '/overview/sdk-overview' },
            { label: 'API Reference', to: '/api-reference' },
          ],
        },
        {
          title: '🔧 Development',
          items: [
            { label: 'Architecture', to: '/adrs/client-proposed-optimistic-concurrency' },
            { label: 'Implementation', to: '/implementation/concurrency-and-consistency' },
            { label: 'GitHub', href: 'https://github.com/NeuralEmpowerment/node-experiment-lab' },
          ],
        },
      ],
      copyright: `© ${new Date().getFullYear()} EventStore (Rust) | Built with ❤️ using Docusaurus`,
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
    zoom: {
      selector: '.markdown img',
      config: {
        background: {
          light: 'rgb(255, 255, 255)',
          dark: 'rgb(50, 50, 50)',
        },
      },
    },
  },
};

export default config;
