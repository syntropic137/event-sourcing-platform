/// Common test utilities for Postgres backend tests
/// Simple, lightweight helpers that don't need to be part of the main crate

/// Get database URL for testing - fast dev infrastructure or skip
pub async fn get_test_database_url() -> String {
    // Try fast dev infrastructure first (check environment variables)
    if let Ok(url) = std::env::var("TEST_DATABASE_URL") {
        println!("🚀 Using TEST_DATABASE_URL: {}", url);
        return url;
    }
    
    if let Ok(url) = std::env::var("DATABASE_URL") {
        println!("🚀 Using DATABASE_URL: {}", url);
        return url;
    }

    // If no fast infrastructure, fall back to testcontainers
    println!("🐳 No fast dev infrastructure found, using testcontainers");
    use testcontainers::runners::AsyncRunner;
    use testcontainers_modules::postgres::Postgres as PgImage;
    
    let container = PgImage::default().start().await.expect("start postgres");
    let port = container.get_host_port_ipv4(5432).await.expect("port");
    format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres")
}

