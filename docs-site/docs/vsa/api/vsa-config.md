---
sidebar_position: 1
---

# VSA Configuration API

TypeScript types and interfaces for `vsa.yaml` configuration.

## VsaConfig

Root configuration object.

```typescript
interface VsaConfig {
  version: number;
  language: 'typescript' | 'python' | 'rust';
  root: string;
  framework?: FrameworkConfig;
  bounded_contexts?: BoundedContext[];
  integration_events?: IntegrationEventsConfig;
  validation?: ValidationConfig;
}
```

## FrameworkConfig

Framework integration settings.

```typescript
interface FrameworkConfig {
  name: string;
  aggregate_class: string;
  aggregate_import: string;
}
```

## BoundedContext

Bounded context definition.

```typescript
interface BoundedContext {
  name: string;
  description?: string;
  publishes?: string[];
  subscribes?: string[];
  validation?: ValidationConfig;
}
```

## IntegrationEventsConfig

Integration events configuration.

```typescript
interface IntegrationEventsConfig {
  path: string;
  events?: Record<string, IntegrationEventConfig>;
}

interface IntegrationEventConfig {
  publisher: string;
  subscribers: string[];
  description?: string;
  version?: number;
}
```

## ValidationConfig

Validation rules.

```typescript
interface ValidationConfig {
  require_tests?: boolean;
  require_handler?: boolean;
  require_aggregate?: boolean;
  exclude?: string[];
}
```

## Example Usage

```typescript
import { VsaConfig } from 'vsa-core';

const config: VsaConfig = {
  version: 1,
  language: 'typescript',
  root: 'src/contexts',
  
  bounded_contexts: [
    {
      name: 'orders',
      publishes: ['OrderPlaced'],
      subscribes: ['ProductAdded']
    }
  ],
  
  validation: {
    require_tests: true,
    require_handler: true
  }
};
```

---

More API documentation coming soon!

