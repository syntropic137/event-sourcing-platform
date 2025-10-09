use std::time::Duration;
use testcontainers::runners::AsyncRunner;
use testcontainers::ContainerAsync;
use testcontainers_modules::postgres::Postgres as PgImage;
use tokio::sync::OnceCell;

/// Shared PostgreSQL testcontainer that lives for the entire test suite
/// This prevents creating multiple containers and avoids resource exhaustion
static SHARED_CONTAINER: OnceCell<SharedContainer> = OnceCell::const_new();

struct SharedContainer {
    url: String,
    _container: ContainerAsync<PgImage>,
}

/// Get database URL for testing - fast dev infrastructure or shared testcontainer
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

    // Use shared testcontainer (create once, reuse for all tests)
    let shared = SHARED_CONTAINER
        .get_or_init(|| async {
            println!("🐳 No fast dev infrastructure found, creating shared testcontainer");

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
            let start_time = std::time::Instant::now();

            for attempt in 1..=15 {
                match sqlx::PgPool::connect(&url).await {
                    Ok(pool) => {
                        println!("🐳 PostgreSQL connection successful on attempt {attempt}");

                        // Also test that we can create multiple connections (like the actual store will)
                        // Match the production test configuration: 8 connections, 120s timeout
                        match sqlx::postgres::PgPoolOptions::new()
                            .max_connections(8)
                            .acquire_timeout(Duration::from_secs(30))
                            .connect(&url)
                            .await
                        {
                            Ok(test_pool) => {
                                let elapsed = start_time.elapsed();
                                println!(
                                    "🐳 PostgreSQL pool test successful (ready in {elapsed:?})"
                                );
                                println!("🐳 Pool stats: max_connections=8, acquire_timeout=30s");
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
                                panic!(
                                    "🐳 Failed to create connection pool after 15 attempts: {e}"
                                );
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

            let total_elapsed = start_time.elapsed();
            println!("🐳 Shared testcontainer fully initialized in {total_elapsed:?}");
            println!("🐳 Container will be reused by all tests in this suite");

            SharedContainer {
                url,
                _container: container,
            }
        })
        .await;

    println!("🐳 Using shared testcontainer: {}", shared.url);
    shared.url.clone()
}
