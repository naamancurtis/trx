use csv::{ReaderBuilder, Trim};
use paste::paste;
use pretty_assertions::assert_eq;

use lib::clients::SyncClients;
use lib::IncomingTransaction;

macro_rules! test_dir {
    ($dir:literal) => {

        paste ! {
            #[test]
            fn [<run_ $dir _test>]() -> color_eyre::Result<()> {
                let mut reader = ReaderBuilder::new().trim(Trim::All)
                    .from_path(std::path::PathBuf::from(&format!("./test_assets/{}/spec.csv", $dir)))?;

                let mut clients: SyncClients = Default::default();
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
                    assert_eq!(res, exp);
                }

                Ok(())
            }
        }
    };
}

test_dir! { "simple" }
test_dir! { "single_client" }
test_dir! { "larger" }
