use csv::{ReaderBuilder, Trim};
use paste::paste;
use pretty_assertions::assert_eq;

use lib::engines::{ActorLikeEngine, BasicEngine, StreamLikeEngine};
use lib::transaction::IncomingTransaction;
use lib::{AsyncEngine, SyncEngine};

macro_rules! test_sync {
    ($dir:literal, $engine:ty) => {

        paste ! {
            #[test]
            fn [<run_ $dir _ $engine:snake _test>]() -> color_eyre::Result<()> {
                let mut reader = ReaderBuilder::new()
                    .trim(Trim::All)
                    .flexible(true)
                    .from_path(std::path::PathBuf::from(&format!("./test_assets/{}/spec.csv", $dir)))?;

                let mut engine: $engine = Default::default();
                let iter = reader.deserialize::<IncomingTransaction>();
                engine.process(iter)?;

                let mut result = vec![];
                engine.output(&mut result)?;

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
    ($dir:literal, $engine:ty) => {

        paste ! {
            #[tokio::test]
            async fn [<run_ $dir _ $engine:snake _test>]() -> color_eyre::Result<()> {
                let mut reader = ReaderBuilder::new()
                    .trim(Trim::All)
                    .flexible(true)
                    .from_path(std::path::PathBuf::from(&format!("./test_assets/{}/spec.csv", $dir)))?;

                let mut engine: $engine = Default::default();
                let iter = reader.deserialize::<IncomingTransaction>();
                engine.process(iter).await?;

                let mut result = vec![];
                engine.output(&mut result).await?;

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

test_sync! { "simple", BasicEngine }
test_sync! { "single_client", BasicEngine }
test_sync! { "larger", BasicEngine }
test_sync! { "beyond_4_dp", BasicEngine }

test_sync! { "simple", StreamLikeEngine }
test_sync! { "single_client", StreamLikeEngine }
test_sync! { "larger", StreamLikeEngine }
test_sync! { "beyond_4_dp", StreamLikeEngine }

test_async! { "simple", ActorLikeEngine }
test_async! { "single_client", ActorLikeEngine }
test_async! { "larger", ActorLikeEngine }
test_async! { "beyond_4_dp", ActorLikeEngine }
