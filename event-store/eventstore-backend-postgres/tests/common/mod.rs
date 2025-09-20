/// Common test utilities for Postgres backend tests
/// Simple, lightweight helpers that don't need to be part of the main crate

/// Get database URL for testing - fast dev infrastructure or skip
pub async fn get_test_database_url() -> String {
    // Try fast dev infrastructure first (check environment variables)
    if let Ok(url) = std::env::var("TEST_DATABASE_URL") {
        println!("🚀 Using TEST_DATABASE_URL: {url}");
        return url;
    }

    if let Ok(url) = std::env::var("DATABASE_URL") {
        println!("🚀 Using DATABASE_URL: {url}");
        return url;
    }

    // If no fast infrastructure, fall back to testcontainers
    println!("🐳 No fast dev infrastructure found, using testcontainers");
    use testcontainers::runners::AsyncRunner;
    use testcontainers_modules::postgres::Postgres as PgImage;
    use std::time::Duration;

    // Configure testcontainers for CI environment
    let postgres_image = PgImage::default()
        .with_db_name("postgres")
        .with_username("postgres")
        .with_password("postgres");

    println!("🐳 Starting PostgreSQL testcontainer...");
    let container = postgres_image.start().await.expect("start postgres");
    
    println!("🐳 Getting container port...");
    let port = container.get_host_port_ipv4(5432).await.expect("port");
    
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    println!("🐳 PostgreSQL testcontainer ready at: {url}");
    
    // Wait for PostgreSQL to be fully ready by attempting a connection
    println!("🐳 Waiting for PostgreSQL to be ready...");
    for attempt in 1..=10 {
        match sqlx::PgPool::connect(&url).await {
            Ok(pool) => {
                println!("🐳 PostgreSQL connection successful on attempt {attempt}");
                pool.close().await;
                break;
            }
            Err(e) if attempt < 10 => {
                println!("🐳 Connection attempt {attempt} failed: {e}, retrying...");
                tokio::time::sleep(Duration::from_millis(1000)).await;
            }
            Err(e) => {
                panic!("🐳 Failed to connect to PostgreSQL after 10 attempts: {e}");
            }
        }
    }
    
    url
}
