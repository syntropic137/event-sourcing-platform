import type { SidebarsConfig } from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  docs: [
    // Overview
    'README',

    // Architectural Decisions
    {
      type: 'category',
      label: 'Architecture',
      collapsed: false,
      items: [
        'adrs/client-proposed-optimistic-concurrency',
        'adrs/aggregate-vs-stream-terminology',
      ],
    },

    // Core Concepts
    {
      type: 'category',
      label: 'Core Concepts',
      collapsed: false,
      items: [
        'concepts/ubiquitous-language',
        'concepts/event-model',
        'concepts/aggregate-pattern',
        'concepts/event-store-vs-event-bus',
      ],
    },

    // Implementation
    {
      type: 'category',
      label: 'Implementation',
      collapsed: false,
      items: [
        'implementation/concurrency-and-consistency',
        'implementation/axon-alignment',
        'implementation/sql-enforcement',
        'implementation/sdk-design',
        'implementation/initial-plan_proto-and-clients',
      ],
    },

    // SDKs
    {
      type: 'category',
      label: 'SDKs',
      collapsed: false,
      items: [
        'overview/sdk-overview',
        'typescript/typescript-sdk',
        'python/python-sdk',
        'rust/rust-sdk',
        'api-reference',
      ],
    },

    // Development Resources
    {
      type: 'category',
      label: 'Resources',
      collapsed: true,
      items: [
        'development/rust',
      ],
    },
  ],
};

export default {
  tutorialSidebar: sidebars,
};
