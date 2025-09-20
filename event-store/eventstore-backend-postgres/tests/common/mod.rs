/// Common test utilities for Postgres backend tests
/// Simple, lightweight helpers that don't need to be part of the main crate
///
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
    use std::time::Duration;
    use testcontainers::runners::AsyncRunner;
    use testcontainers_modules::postgres::Postgres as PgImage;

    // Configure testcontainers for CI environment
    let postgres_image = PgImage::default()
        .with_db_name("postgres")
        .with_user("postgres")
        .with_password("postgres");

    println!("🐳 Starting PostgreSQL testcontainer...");
    let container = postgres_image.start().await.expect("start postgres");

    println!("🐳 Getting container port...");
    let port = container.get_host_port_ipv4(5432).await.expect("port");

    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    println!("🐳 PostgreSQL testcontainer ready at: {url}");

    // Wait for PostgreSQL to be fully ready by attempting a connection
    println!("🐳 Waiting for PostgreSQL to be ready...");
    for attempt in 1..=15 {
        match sqlx::PgPool::connect(&url).await {
            Ok(pool) => {
                println!("🐳 PostgreSQL connection successful on attempt {attempt}");

                // Also test that we can create multiple connections (like the actual store will)
                match sqlx::postgres::PgPoolOptions::new()
                    .max_connections(3)
                    .acquire_timeout(Duration::from_secs(10))
                    .connect(&url)
                    .await
                {
                    Ok(test_pool) => {
                        println!("🐳 PostgreSQL pool test successful");
                        test_pool.close().await;
                        pool.close().await;
                        break;
                    }
                    Err(e) if attempt < 15 => {
                        println!("🐳 Pool test attempt {attempt} failed: {e}, retrying...");
                        pool.close().await;
                        tokio::time::sleep(Duration::from_millis(2000)).await;
                        continue;
                    }
                    Err(e) => {
                        pool.close().await;
                        panic!("🐳 Failed to create connection pool after 15 attempts: {e}");
                    }
                }
            }
            Err(e) if attempt < 15 => {
                println!("🐳 Connection attempt {attempt} failed: {e}, retrying...");
                tokio::time::sleep(Duration::from_millis(2000)).await;
            }
            Err(e) => {
                panic!("🐳 Failed to connect to PostgreSQL after 15 attempts: {e}");
            }
        }
    }

    url
}
