/**
 * Metrics Utilities
 *
 * Simple metrics interface with in-memory implementation and Prometheus export.
 */

import {
  Counter,
  Gauge,
  Histogram,
  MetricLabels,
  MetricsRegistry,
  Timer,
} from '../types';

// ============================================================================
// IN-MEMORY METRIC IMPLEMENTATIONS
// ============================================================================

/**
 * In-memory counter implementation.
 */
class InMemoryCounter implements Counter {
  private values: Map<string, number> = new Map();

  constructor(
    public readonly name: string,
    public readonly help: string,
    public readonly labelNames: string[]
  ) {}

  private labelsToKey(labels: MetricLabels = {}): string {
    if (this.labelNames.length === 0) {
      return '';
    }
    return this.labelNames.map((name) => `${name}="${labels[name] ?? ''}"`).join(',');
  }

  inc(labels: MetricLabels = {}): void {
    this.add(1, labels);
  }

  add(value: number, labels: MetricLabels = {}): void {
    if (value < 0) {
      throw new Error('Counter can only be incremented');
    }
    const key = this.labelsToKey(labels);
    const current = this.values.get(key) ?? 0;
    this.values.set(key, current + value);
  }

  getValues(): Map<string, number> {
    return new Map(this.values);
  }

  reset(): void {
    this.values.clear();
  }
}

/**
 * In-memory gauge implementation.
 */
class InMemoryGauge implements Gauge {
  private values: Map<string, number> = new Map();

  constructor(
    public readonly name: string,
    public readonly help: string,
    public readonly labelNames: string[]
  ) {}

  private labelsToKey(labels: MetricLabels = {}): string {
    if (this.labelNames.length === 0) {
      return '';
    }
    return this.labelNames.map((name) => `${name}="${labels[name] ?? ''}"`).join(',');
  }

  set(value: number, labels: MetricLabels = {}): void {
    const key = this.labelsToKey(labels);
    this.values.set(key, value);
  }

  inc(labels: MetricLabels = {}): void {
    this.add(1, labels);
  }

  dec(labels: MetricLabels = {}): void {
    this.add(-1, labels);
  }

  add(value: number, labels: MetricLabels = {}): void {
    const key = this.labelsToKey(labels);
    const current = this.values.get(key) ?? 0;
    this.values.set(key, current + value);
  }

  getValues(): Map<string, number> {
    return new Map(this.values);
  }

  reset(): void {
    this.values.clear();
  }
}

/**
 * In-memory histogram implementation.
 */
class InMemoryHistogram implements Histogram {
  private readonly bucketValues: Map<string, Map<number, number>> = new Map();
  private readonly sums: Map<string, number> = new Map();
  private readonly counts: Map<string, number> = new Map();

  constructor(
    public readonly name: string,
    public readonly help: string,
    public readonly buckets: number[],
    public readonly labelNames: string[]
  ) {
    // Sort buckets ascending
    this.buckets = [...buckets].sort((a, b) => a - b);
  }

  private labelsToKey(labels: MetricLabels = {}): string {
    if (this.labelNames.length === 0) {
      return '';
    }
    return this.labelNames.map((name) => `${name}="${labels[name] ?? ''}"`).join(',');
  }

  observe(value: number, labels: MetricLabels = {}): void {
    const key = this.labelsToKey(labels);

    // Update sum and count
    this.sums.set(key, (this.sums.get(key) ?? 0) + value);
    this.counts.set(key, (this.counts.get(key) ?? 0) + 1);

    // Update buckets
    let bucketMap = this.bucketValues.get(key);
    if (!bucketMap) {
      bucketMap = new Map();
      // Initialize all buckets to 0
      for (const bucket of this.buckets) {
        bucketMap.set(bucket, 0);
      }
      this.bucketValues.set(key, bucketMap);
    }

    // Increment all buckets >= value
    for (const bucket of this.buckets) {
      if (value <= bucket) {
        bucketMap.set(bucket, (bucketMap.get(bucket) ?? 0) + 1);
      }
    }
  }

  startTimer(labels: MetricLabels = {}): () => number {
    const start = process.hrtime.bigint();
    return () => {
      const end = process.hrtime.bigint();
      const durationNs = Number(end - start);
      const durationSec = durationNs / 1e9;
      this.observe(durationSec, labels);
      return durationSec;
    };
  }

  getBuckets(): Map<string, Map<number, number>> {
    return new Map(
      Array.from(this.bucketValues.entries()).map(([k, v]) => [k, new Map(v)])
    );
  }

  getSums(): Map<string, number> {
    return new Map(this.sums);
  }

  getCounts(): Map<string, number> {
    return new Map(this.counts);
  }

  reset(): void {
    this.bucketValues.clear();
    this.sums.clear();
    this.counts.clear();
  }
}

// ============================================================================
// METRICS REGISTRY
// ============================================================================

/**
 * Default histogram buckets (in seconds) for latency metrics.
 */
export const DEFAULT_LATENCY_BUCKETS = [
  0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1, 2.5, 5, 10,
];

/**
 * In-memory metrics registry.
 *
 * Provides simple metric collection with Prometheus-format export.
 */
export class InMemoryMetricsRegistry implements MetricsRegistry {
  private counters: Map<string, InMemoryCounter> = new Map();
  private gauges: Map<string, InMemoryGauge> = new Map();
  private histograms: Map<string, InMemoryHistogram> = new Map();

  counter(name: string, help: string, labelNames: string[] = []): Counter {
    let counter = this.counters.get(name);
    if (!counter) {
      counter = new InMemoryCounter(name, help, labelNames);
      this.counters.set(name, counter);
    }
    return counter;
  }

  gauge(name: string, help: string, labelNames: string[] = []): Gauge {
    let gauge = this.gauges.get(name);
    if (!gauge) {
      gauge = new InMemoryGauge(name, help, labelNames);
      this.gauges.set(name, gauge);
    }
    return gauge;
  }

  histogram(
    name: string,
    help: string,
    buckets: number[] = DEFAULT_LATENCY_BUCKETS,
    labelNames: string[] = []
  ): Histogram {
    let histogram = this.histograms.get(name);
    if (!histogram) {
      histogram = new InMemoryHistogram(name, help, buckets, labelNames);
      this.histograms.set(name, histogram);
    }
    return histogram;
  }

  /**
   * Get all metrics in Prometheus text format.
   */
  async getMetrics(): Promise<string> {
    const lines: string[] = [];

    // Export counters
    for (const counter of this.counters.values()) {
      lines.push(`# HELP ${counter.name} ${counter.help}`);
      lines.push(`# TYPE ${counter.name} counter`);

      for (const [labels, value] of counter.getValues()) {
        const labelStr = labels ? `{${labels}}` : '';
        lines.push(`${counter.name}${labelStr} ${value}`);
      }
      lines.push('');
    }

    // Export gauges
    for (const gauge of this.gauges.values()) {
      lines.push(`# HELP ${gauge.name} ${gauge.help}`);
      lines.push(`# TYPE ${gauge.name} gauge`);

      for (const [labels, value] of gauge.getValues()) {
        const labelStr = labels ? `{${labels}}` : '';
        lines.push(`${gauge.name}${labelStr} ${value}`);
      }
      lines.push('');
    }

    // Export histograms
    for (const histogram of this.histograms.values()) {
      lines.push(`# HELP ${histogram.name} ${histogram.help}`);
      lines.push(`# TYPE ${histogram.name} histogram`);

      const buckets = histogram.getBuckets();
      const sums = histogram.getSums();
      const counts = histogram.getCounts();

      for (const [labels, bucketMap] of buckets) {
        const baseLabels = labels ? `${labels},` : '';

        // Export buckets
        for (const bucket of histogram.buckets) {
          const bucketValue = bucketMap.get(bucket) ?? 0;
          lines.push(`${histogram.name}_bucket{${baseLabels}le="${bucket}"} ${bucketValue}`);
        }
        // +Inf bucket
        const infValue = counts.get(labels) ?? 0;
        lines.push(`${histogram.name}_bucket{${baseLabels}le="+Inf"} ${infValue}`);

        // Sum and count
        const sumLabels = labels ? `{${labels}}` : '';
        lines.push(`${histogram.name}_sum${sumLabels} ${sums.get(labels) ?? 0}`);
        lines.push(`${histogram.name}_count${sumLabels} ${counts.get(labels) ?? 0}`);
      }
      lines.push('');
    }

    return lines.join('\n');
  }

  /**
   * Reset all metrics.
   */
  resetMetrics(): void {
    for (const counter of this.counters.values()) {
      counter.reset();
    }
    for (const gauge of this.gauges.values()) {
      gauge.reset();
    }
    for (const histogram of this.histograms.values()) {
      histogram.reset();
    }
  }

  /**
   * Clear all metric definitions.
   */
  clear(): void {
    this.counters.clear();
    this.gauges.clear();
    this.histograms.clear();
  }
}

// ============================================================================
// GLOBAL REGISTRY
// ============================================================================

let globalRegistry: MetricsRegistry = new InMemoryMetricsRegistry();

/**
 * Get the global metrics registry.
 */
export function getGlobalRegistry(): MetricsRegistry {
  return globalRegistry;
}

/**
 * Set the global metrics registry.
 *
 * Use this to integrate with external metrics systems like Prometheus.
 */
export function setGlobalRegistry(registry: MetricsRegistry): void {
  globalRegistry = registry;
}

/**
 * Reset to default in-memory registry.
 */
export function resetGlobalRegistry(): void {
  globalRegistry = new InMemoryMetricsRegistry();
}

// ============================================================================
// CONVENIENCE FUNCTIONS
// ============================================================================

/**
 * Create or get a counter from the global registry.
 */
export function counter(
  name: string,
  help: string,
  labelNames?: string[]
): Counter {
  return globalRegistry.counter(name, help, labelNames);
}

/**
 * Create or get a gauge from the global registry.
 */
export function gauge(name: string, help: string, labelNames?: string[]): Gauge {
  return globalRegistry.gauge(name, help, labelNames);
}

/**
 * Create or get a histogram from the global registry.
 */
export function histogram(
  name: string,
  help: string,
  buckets?: number[],
  labelNames?: string[]
): Histogram {
  return globalRegistry.histogram(name, help, buckets, labelNames);
}

/**
 * Get all metrics in Prometheus format.
 */
export function getMetrics(): Promise<string> {
  return globalRegistry.getMetrics();
}

/**
 * Reset all metrics values (but keep definitions).
 */
export function resetMetrics(): void {
  globalRegistry.resetMetrics();
}

// ============================================================================
// TIMER HELPER
// ============================================================================

/**
 * Start a timer for measuring duration.
 *
 * @example
 * ```typescript
 * const timer = startTimer();
 * // ... do work ...
 * const durationSeconds = timer.end();
 * ```
 */
export function startTimer(): Timer {
  const start = process.hrtime.bigint();
  return {
    end(): number {
      const end = process.hrtime.bigint();
      const durationNs = Number(end - start);
      return durationNs / 1e9;
    },
  };
}

/**
 * Start a timer that automatically records to a histogram.
 *
 * @example
 * ```typescript
 * const end = startHistogramTimer(
 *   'es_command_duration_seconds',
 *   'Command execution duration',
 *   { command_type: 'PlaceOrder' }
 * );
 *
 * // ... do work ...
 *
 * const duration = end(); // Records to histogram and returns duration
 * ```
 */
export function startHistogramTimer(
  name: string,
  help: string,
  labels?: MetricLabels,
  buckets?: number[]
): () => number {
  const hist = histogram(name, help, buckets, labels ? Object.keys(labels) : []);
  return hist.startTimer(labels);
}
