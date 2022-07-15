use criterion::{black_box, criterion_group, criterion_main, Criterion};

use color_eyre::Result;
use csv::{ReaderBuilder, Trim, WriterBuilder};
use lib::{Clients, IncomingTransaction};

use std::path::PathBuf;

fn run_single_threaded() -> Result<()> {
    let mut reader = ReaderBuilder::new()
        .trim(Trim::All)
        .from_path(PathBuf::from("./test_assets/huge/spec.csv"))?;
    let mut clients: Clients = Default::default();
    let iter = reader.deserialize::<IncomingTransaction>();
    clients.process(iter)?;
    let mut writer = WriterBuilder::new().from_path("/dev/null")?.into_inner()?;
    clients.output(&mut writer)?;
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
    group.finish()
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
