use criterion::{black_box, criterion_group, criterion_main, Criterion};

use color_eyre::Result;
use csv::{ReaderBuilder, Trim, WriterBuilder};
use tokio::runtime::Runtime;

use std::path::PathBuf;
use std::time::Duration;

use lib::clients::actor_like::Clients as ActorLikeClients;
use lib::clients::stream_like::Clients as StreamLikeClients;
use lib::clients::synchronous::Clients as SynchronousClients;
use lib::transaction::IncomingTransaction;
use lib::{AsyncClients, SyncClients};

fn run_sync(mut clients: impl SyncClients) -> Result<()> {
    let mut reader = ReaderBuilder::new()
        .trim(Trim::All)
        .from_path(PathBuf::from("./test_assets/larger/spec.csv"))?;
    let iter = reader.deserialize::<IncomingTransaction>();
    clients.process(iter)?;
    let mut writer = WriterBuilder::new().from_path("/dev/null")?.into_inner()?;
    clients.output(&mut writer)?;
    Ok(())
}

async fn run_async(mut clients: impl AsyncClients + Send + Sync) -> Result<()> {
    let mut reader = ReaderBuilder::new()
        .trim(Trim::All)
        .from_path(PathBuf::from("./test_assets/larger/spec.csv"))?;
    let iter = reader.deserialize::<IncomingTransaction>();
    clients.process(iter).await?;
    let mut writer = WriterBuilder::new().from_path("/dev/null")?.into_inner()?;
    clients.output(&mut writer).await?;
    Ok(())
}

pub fn benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("trx");
    group.sample_size(20);
    group.measurement_time(Duration::from_secs(30));
    group.bench_function("single_threaded", |b| {
        b.iter(|| {
            black_box(run_sync(SynchronousClients::default()).ok());
        })
    });
    group.bench_function("multi_threaded", |b| {
        b.iter(|| {
            black_box(run_sync(StreamLikeClients::default()).ok());
        })
    });
    group.bench_function("async_actor", |b| {
        b.to_async(Runtime::new().unwrap())
            .iter(|| black_box(run_async(ActorLikeClients::default())))
    });
    group.finish()
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
