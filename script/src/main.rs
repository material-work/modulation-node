use std::io::Read;

use alloy::{
    primitives::b256,
    providers::{Provider, ProviderBuilder},
    sol,
    sol_types::SolCall,
};
use alloy_rlp::Decodable;
use flate2::read::ZlibDecoder;
use program::{CanvasProcessor, InMemoryDB, SignedTransaction};

sol!(
    /// @notice Verifies the submission of a batch of txs with a zk proof.
    /// @param _publicValuesBytes The zk proof of a state transition.
    /// @param _proofBytes The encoded public values.
    /// @param _transactionData The transaction data included in the batch.
    function submitBatchWithProof(
        bytes calldata _publicValuesBytes,
        bytes calldata _proofBytes,
        bytes calldata _transactionData
    ) public;
);

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let rpc_url = "https://eth.merkle.io".parse()?;
    let provider = ProviderBuilder::new().on_http(rpc_url);
    let mut processor = CanvasProcessor {
        db: &InMemoryDB::default(),
    };

    let txs = [
        b256!("efe792bb5130db405b2d7feb683a6bb4d1ec002e88843cd478dcfd5105d1d964"),
        b256!("5624feb01173396f3c26169ac4bc4122525a7a90f38c3851bebb9becf73d1ab8"),
    ];

    for tx in txs {
        let res = provider.get_transaction_by_hash(tx).await?.unwrap();
        let decoded: submitBatchWithProofCall =
            submitBatchWithProofCall::abi_decode(&res.input, true)?;
        let rollup_tx_data = decoded._transactionData;

        let mut d = ZlibDecoder::new(rollup_tx_data.as_ref());
        let mut bytes = Vec::<u8>::new();
        d.read_to_end(&mut bytes).unwrap();

        let decoded_txs = Vec::<SignedTransaction>::decode(&mut bytes.as_slice())?;

        for rollup_tx in decoded_txs {
            processor.apply_transaction(&rollup_tx)?;
        }
    }

    let final_state_root = processor
        .generate_state_root()
        .expect("Failed to generate final state root");

    println!("Final state root: 0x{}", hex::encode(final_state_root));

    Ok(())
}
