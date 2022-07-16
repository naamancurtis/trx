use csv::{ReaderBuilder, Trim};
use paste::paste;
use pretty_assertions::assert_eq;

use lib::clients::actor_like::Clients as ActorLikeClients;
use lib::clients::stream_like::Clients as StreamLikeClients;
use lib::clients::synchronous::Clients as SynchronousClients;
use lib::transaction::IncomingTransaction;
use lib::{AsyncClients, SyncClients};

macro_rules! test_sync {
    ($dir:literal, $client:ty) => {

        paste ! {
            #[test]
            fn [<run_ $dir _ $client:snake _test>]() -> color_eyre::Result<()> {
                let mut reader = ReaderBuilder::new()
                    .trim(Trim::All)
                    .flexible(true)
                    .from_path(std::path::PathBuf::from(&format!("./test_assets/{}/spec.csv", $dir)))?;

                let mut clients: $client = Default::default();
                let iter = reader.deserialize::<IncomingTransaction>();
                clients.process(iter)?;

                let mut result = vec![];
                clients.output(&mut result)?;

                let mut results = ReaderBuilder::new().trim(Trim::All)
                    .from_reader(&*result)
                    .records()
                    .filter_map(|r| r.ok())
                    .collect::<Vec<_>>();

                let mut expected = ReaderBuilder::new().trim(Trim::All)
                    .from_path(std::path::PathBuf::from(&format!("./test_assets/{}/expected.csv", $dir)))?
                    .records()
                    .filter_map(|r| r.ok())
                    .collect::<Vec<_>>();
                results.sort_by_key(|k| k[0].to_string());
                expected.sort_by_key(|k| k[0].to_string());

                for (res, exp) in results.into_iter().zip(expected.into_iter()) {
                    assert_eq!(res, exp, "expected lhs(result) to equal rhs(expected)");
                }

                Ok(())
            }
        }
    };
}

macro_rules! test_async {
    ($dir:literal, $client:ty) => {

        paste ! {
            #[tokio::test]
            async fn [<run_ $dir _ $client:snake _test>]() -> color_eyre::Result<()> {
                let mut reader = ReaderBuilder::new()
                    .trim(Trim::All)
                    .flexible(true)
                    .from_path(std::path::PathBuf::from(&format!("./test_assets/{}/spec.csv", $dir)))?;

                let mut clients: $client = Default::default();
                let iter = reader.deserialize::<IncomingTransaction>();
                clients.process(iter).await?;

                let mut result = vec![];
                clients.output(&mut result).await?;

                let mut results = ReaderBuilder::new().trim(Trim::All)
                    .from_reader(&*result)
                    .records()
                    .filter_map(|r| r.ok())
                    .collect::<Vec<_>>();

                let mut expected = ReaderBuilder::new().trim(Trim::All)
                    .from_path(std::path::PathBuf::from(&format!("./test_assets/{}/expected.csv", $dir)))?
                    .records()
                    .filter_map(|r| r.ok())
                    .collect::<Vec<_>>();
                results.sort_by_key(|k| k[0].to_string());
                expected.sort_by_key(|k| k[0].to_string());

                for (res, exp) in results.into_iter().zip(expected.into_iter()) {
                    assert_eq!(res, exp, "expected lhs(result) to equal rhs(expected)");
                }

                Ok(())
            }
        }
    };
}

test_sync! { "simple", SynchronousClients }
test_sync! { "single_client", SynchronousClients }
test_sync! { "larger", SynchronousClients }

test_sync! { "simple", StreamLikeClients }
test_sync! { "single_client", StreamLikeClients }
test_sync! { "larger", StreamLikeClients }

test_async! { "simple", ActorLikeClients }
test_async! { "single_client", ActorLikeClients }
test_async! { "larger", ActorLikeClients }
