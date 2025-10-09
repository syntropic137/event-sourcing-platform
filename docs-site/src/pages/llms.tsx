import React from 'react';
import Layout from '@theme/Layout';

export default function LLMApiDocs(): React.ReactElement {
  return (
    <Layout
      title="LLM API Documentation"
      description="Plain text API documentation optimized for LLM consumption">
      <main style={{ padding: '2rem', maxWidth: '1200px', margin: '0 auto' }}>
        <h1>LLM-Friendly API Documentation</h1>
        <p>
          This endpoint provides all API documentation in a plain text format optimized for Large Language Model consumption.
        </p>
        
        <div style={{ marginTop: '2rem' }}>
          <h2>Available Formats</h2>
          <ul>
            <li>
              <a href="/api-docs-llm.txt" download>
                <strong>Download Plain Text API Docs</strong>
              </a> - Complete API reference in plain text format
            </li>
          </ul>
        </div>

        <div style={{ marginTop: '2rem', padding: '1rem', backgroundColor: '#f5f5f5', borderRadius: '8px' }}>
          <h3>What's Included</h3>
          <ul>
            <li><strong>Event Store SDK API</strong> - Low-level event store client APIs (TypeScript, Python, Rust)</li>
            <li><strong>Event Sourcing SDK API</strong> - High-level aggregate, repository, and projection APIs</li>
            <li><strong>Core Concepts</strong> - Event sourcing patterns and best practices</li>
            <li><strong>Code Examples</strong> - Real-world usage examples from the platform</li>
          </ul>
        </div>

        <div style={{ marginTop: '2rem' }}>
          <h3>Usage with LLMs</h3>
          <p>
            This documentation is formatted for easy consumption by AI assistants and language models:
          </p>
          <ul>
            <li>Clear section markers for easy parsing</li>
            <li>Consistent formatting throughout</li>
            <li>Complete type signatures and parameter descriptions</li>
            <li>Practical code examples with context</li>
            <li>No HTML or complex markdown formatting</li>
          </ul>
        </div>

        <div style={{ marginTop: '2rem', padding: '1rem', backgroundColor: '#e3f2fd', borderRadius: '8px' }}>
          <h3>🚀 Coming Soon: MCP Server</h3>
          <p>
            We're building a Model Context Protocol (MCP) server that will provide:
          </p>
          <ul>
            <li>Interactive documentation queries</li>
            <li>Context-aware code examples</li>
            <li>Real-time API search</li>
            <li>Integration with IDE extensions</li>
          </ul>
        </div>

        <div style={{ marginTop: '2rem' }}>
          <h3>Feedback</h3>
          <p>
            Have suggestions for improving LLM-friendly documentation? 
            <a href="https://github.com/neurale/event-sourcing-platform/issues" style={{ marginLeft: '0.5rem' }}>
              Open an issue on GitHub
            </a>
          </p>
        </div>
      </main>
    </Layout>
  );
}
