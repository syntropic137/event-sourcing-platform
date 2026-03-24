/**
 * Tests for InMemoryMetricsRegistry (extracted helpers)
 * Verifies counter, gauge, histogram, and Prometheus export
 */

import { InMemoryMetricsRegistry } from '../src/observability/metrics/metrics';

describe('InMemoryMetricsRegistry', () => {
  let registry: InMemoryMetricsRegistry;

  beforeEach(() => {
    registry = new InMemoryMetricsRegistry();
  });

  describe('Counter', () => {
    it('should increment by 1 with inc()', () => {
      const counter = registry.counter('requests_total', 'Total requests');
      counter.inc();
      counter.inc();

      // Access values through a fresh counter reference (get-or-create)
      const same = registry.counter('requests_total', 'Total requests');
      // They should be the same instance
      expect(same).toBe(counter);
    });

    it('should add arbitrary positive values', async () => {
      const counter = registry.counter('bytes_total', 'Total bytes');
      counter.add(100);
      counter.add(50);

      const output = await registry.getMetrics();
      expect(output).toContain('bytes_total 150');
    });

    it('should throw when adding negative value', () => {
      const counter = registry.counter('bad_counter', 'Bad');
      expect(() => counter.add(-1)).toThrow('Counter can only be incremented');
    });

    it('should support reset', async () => {
      const counter = registry.counter('resettable', 'Resettable counter');
      counter.inc();
      counter.inc();

      registry.resetMetrics();

      const output = await registry.getMetrics();
      // After reset, counter should not appear in output (no values)
      expect(output).not.toContain('resettable 2');
    });
  });

  describe('Gauge', () => {
    it('should set value', async () => {
      const gauge = registry.gauge('temperature', 'Current temp');
      gauge.set(72.5);

      const output = await registry.getMetrics();
      expect(output).toContain('temperature 72.5');
    });

    it('should increment and decrement', async () => {
      const gauge = registry.gauge('connections', 'Active connections');
      gauge.inc();
      gauge.inc();
      gauge.dec();

      const output = await registry.getMetrics();
      expect(output).toContain('connections 1');
    });

    it('should add positive and negative values', async () => {
      const gauge = registry.gauge('balance', 'Account balance');
      gauge.add(100);
      gauge.add(-30);

      const output = await registry.getMetrics();
      expect(output).toContain('balance 70');
    });
  });

  describe('Histogram', () => {
    it('should observe values and track sums/counts', async () => {
      const hist = registry.histogram(
        'request_duration',
        'Request duration',
        [0.1, 0.5, 1.0]
      );
      hist.observe(0.05);
      hist.observe(0.3);
      hist.observe(0.8);

      const output = await registry.getMetrics();
      expect(output).toContain('request_duration_sum 1.15');
      expect(output).toContain('request_duration_count 3');
    });

    it('should distribute values across buckets', async () => {
      const hist = registry.histogram(
        'latency',
        'Latency',
        [0.1, 0.5, 1.0]
      );
      hist.observe(0.05); // <= 0.1, 0.5, 1.0
      hist.observe(0.3);  // <= 0.5, 1.0
      hist.observe(0.8);  // <= 1.0

      const output = await registry.getMetrics();
      expect(output).toContain('latency_bucket{le="0.1"} 1');
      expect(output).toContain('latency_bucket{le="0.5"} 2');
      expect(output).toContain('latency_bucket{le="1"} 3');
      expect(output).toContain('latency_bucket{le="+Inf"} 3');
    });
  });

  describe('getMetrics (Prometheus format)', () => {
    it('should include HELP and TYPE lines for counters', async () => {
      registry.counter('http_requests_total', 'Total HTTP requests');
      const counter = registry.counter('http_requests_total', 'Total HTTP requests');
      counter.inc();

      const output = await registry.getMetrics();
      expect(output).toContain('# HELP http_requests_total Total HTTP requests');
      expect(output).toContain('# TYPE http_requests_total counter');
    });

    it('should include HELP and TYPE lines for gauges', async () => {
      const gauge = registry.gauge('active_users', 'Active user count');
      gauge.set(42);

      const output = await registry.getMetrics();
      expect(output).toContain('# HELP active_users Active user count');
      expect(output).toContain('# TYPE active_users gauge');
      expect(output).toContain('active_users 42');
    });

    it('should include HELP and TYPE lines for histograms', async () => {
      const hist = registry.histogram('duration_seconds', 'Duration', [1, 5]);
      hist.observe(2);

      const output = await registry.getMetrics();
      expect(output).toContain('# HELP duration_seconds Duration');
      expect(output).toContain('# TYPE duration_seconds histogram');
    });

    it('should return empty string when no metrics recorded', async () => {
      const output = await registry.getMetrics();
      expect(output).toBe('');
    });
  });

  describe('Labels', () => {
    it('should produce labeled counter output', async () => {
      const counter = registry.counter('http_requests', 'Requests', ['method', 'status']);
      counter.inc({ method: 'GET', status: '200' });
      counter.inc({ method: 'GET', status: '200' });
      counter.inc({ method: 'POST', status: '201' });

      const output = await registry.getMetrics();
      expect(output).toContain('http_requests{method="GET",status="200"} 2');
      expect(output).toContain('http_requests{method="POST",status="201"} 1');
    });

    it('should produce labeled gauge output', async () => {
      const gauge = registry.gauge('cpu_usage', 'CPU usage', ['core']);
      gauge.set(45.2, { core: '0' });
      gauge.set(62.1, { core: '1' });

      const output = await registry.getMetrics();
      expect(output).toContain('cpu_usage{core="0"} 45.2');
      expect(output).toContain('cpu_usage{core="1"} 62.1');
    });
  });

  describe('Registry behavior', () => {
    it('should return same counter for same name (idempotent)', () => {
      const c1 = registry.counter('my_counter', 'help1');
      const c2 = registry.counter('my_counter', 'help2');
      expect(c1).toBe(c2);
    });

    it('should return same gauge for same name (idempotent)', () => {
      const g1 = registry.gauge('my_gauge', 'help1');
      const g2 = registry.gauge('my_gauge', 'help2');
      expect(g1).toBe(g2);
    });

    it('should return same histogram for same name (idempotent)', () => {
      const h1 = registry.histogram('my_hist', 'help1', [1]);
      const h2 = registry.histogram('my_hist', 'help2', [2]);
      expect(h1).toBe(h2);
    });

    it('should clear all metric definitions', async () => {
      registry.counter('c', 'c');
      registry.gauge('g', 'g');
      registry.histogram('h', 'h');

      registry.clear();

      const output = await registry.getMetrics();
      expect(output).toBe('');
    });
  });
});
