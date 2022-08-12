use otel_tests::__reexport::opentelemetry;
use std::time::Instant;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    otel_tests::init_tracing("sqlite-db-bench");
    let conn = sqlite_db::connect("/home/restioson/Downloads/maker.sqlite".parse()?, true).await?;

    let cfds = conn.load_open_cfd_ids().await?;

    for cfd in &cfds {
        let start = Instant::now();
        let _ = conn.load_open_cfd::<model::Cfd>(*cfd, ()).await?;
        println!(
            "Avg from db: {:2}ms/cfd {}",
            (start.elapsed().as_micros() as u128) as f64 / 1000.0,
            cfd
        );
    }

    let start = Instant::now();
    for cfd in &cfds {
        let _ = conn.load_open_cfd::<model::Cfd>(*cfd, ()).await?;
    }
    println!(
        "Avg from cache: {:2}ms/cfd",
        (start.elapsed().as_micros() / cfds.len() as u128) as f64 / 1000.0
    );

    opentelemetry::global::force_flush_tracer_provider();
    opentelemetry::global::shutdown_tracer_provider();

    Ok(())
}
