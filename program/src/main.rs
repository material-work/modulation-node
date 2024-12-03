#![no_main]
sp1_zkvm::entrypoint!(main);
use program::{CanvasProcessor, Input, PublicValuesStruct};

use alloy_sol_types::SolValue;

pub fn main() {
    let input = sp1_zkvm::io::read::<Input>();

    let mut canvas = CanvasProcessor { db: &input.db };

    let initial_state_root = canvas
        .generate_state_root()
        .expect("Failed to generate inital state root");

    let transaction_commit = canvas
        .generate_transaction_commit(&input.transactions)
        .expect("Failed to generate transaction commit");

    for tx in input.transactions {
        canvas
            .apply_transaction(&tx)
            .expect("Failed to apply transaction");
    }

    let final_state_root = canvas
        .generate_state_root()
        .expect("Failed to generate final state root");

    let public_values = PublicValuesStruct {
        initialStateRoot: initial_state_root.into(),
        finalStateRoot: final_state_root.into(),
        transaction_commit: transaction_commit.into(),
    };

    sp1_zkvm::io::commit_slice(public_values.abi_encode().as_slice());
}
