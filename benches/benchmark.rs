use criterion::{criterion_group, criterion_main, Criterion};

use color_eyre::Result;
use csv::{ReaderBuilder, Trim, WriterBuilder};
use lib::clients::AsyncClients;
use tokio::runtime::Runtime;

use std::path::PathBuf;

use lib::{IncomingTransaction, SyncClients};

fn run_single_threaded() -> Result<()> {
    let mut reader = ReaderBuilder::new()
        .trim(Trim::All)
        .from_path(PathBuf::from("./test_assets/huge/spec.csv"))?;
    let mut clients: SyncClients = Default::default();
    let iter = reader.deserialize::<IncomingTransaction>();
    clients.process(iter)?;
    let mut writer = WriterBuilder::new().from_path("/dev/null")?.into_inner()?;
    clients.output(&mut writer)?;
    Ok(())
}

async fn run_async() -> Result<()> {
    let mut reader = ReaderBuilder::new()
        .trim(Trim::All)
        .from_path(PathBuf::from("./test_assets/huge/spec.csv"))?;
    let mut clients: AsyncClients = Default::default();
    let iter = reader.deserialize::<IncomingTransaction>();
    clients.process(iter)?;
    let mut writer = WriterBuilder::new().from_path("/dev/null")?.into_inner()?;
    clients.output(&mut writer).await?;
    Ok(())
}

pub fn benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("trx");
    group.sample_size(10);
    group.bench_function("single_threaded", |b| {
        b.iter(|| {
            run_single_threaded().ok();
        })
    });
    group.bench_function("async_actor", |b| {
        b.to_async(Runtime::new().unwrap()).iter(run_async)
    });
    group.finish()
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
