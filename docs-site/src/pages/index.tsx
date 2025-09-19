import React from 'react';
import Layout from '@theme/Layout';
import Link from '@docusaurus/Link';

export default function Home(): JSX.Element {
  return (
    <Layout title="Event Sourcing Platform" description="Documentation hub for the Event Store and future services">
      <main className="hero">
        <h1 className="hero__title">Event Sourcing Platform Docs</h1>
        <p className="hero__subtitle">Concepts, implementation details, and runbooks for the Event Store platform.</p>
        <div className="hero__cta">
          <Link className="button button--primary button--lg" to="/overview/intro">
            Docs Overview
          </Link>
          <Link className="button button--secondary button--lg" to="/event-store/index">
            Event Store Guide
          </Link>
          <Link className="button button--secondary button--lg" to="/development/fast-testing">
            Development Guides
          </Link>
        </div>
      </main>
    </Layout>
  );
}
