use alloy_primitives::Address;
use alloy_primitives::{keccak256, Signature, U256};
use alloy_rlp::{Encodable, RlpDecodable, RlpEncodable};
use alloy_sol_types::sol;
use alloy_sol_types::SolValue;
use eyre::Result;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use hashbrown::HashMap;
use rs_merkle::{Hasher, MerkleTree};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::{cell::RefCell, ops::Deref};

pub const MAX_SIZE: usize = 9800;
pub const MAX_VALUE: u8 = 15;

sol! {
    struct PublicValuesStruct {
        bytes32 initialStateRoot;
        bytes32 finalStateRoot;
        bytes32 transaction_commit;
    }

    struct AccountCommit {
        address account;
        uint256 nonce;
        string data;
        address[] contributors;
    }
}

#[derive(Debug, Clone)]
struct Leaf {
    hash: [u8; 32],
    account: Address,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Input {
    pub transactions: Vec<SignedTransaction>,
    pub db: InMemoryDB,
}

#[derive(Debug, Clone, Serialize, Deserialize, RlpEncodable, RlpDecodable)]
pub struct Data {
    pub index: usize,
    pub count: usize,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, RlpEncodable, RlpDecodable)]
pub struct Transaction {
    pub to: Address,
    pub version: u8,
    pub data: Vec<Data>,
    pub nonce: u64,
    pub extra: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, RlpEncodable, RlpDecodable)]
pub struct SignedTransaction {
    pub tx: Transaction,
    pub r: U256,
    pub s: U256,
    pub odd_y_parity: bool,
}

pub struct CanvasProcessor<D> {
    pub db: D,
}

impl<D: AccountDB> CanvasProcessor<&D> {
    pub fn apply_transaction(&mut self, input: &SignedTransaction) -> Result<()> {
        let tx = input.tx.clone();

        let from_address = recover_address_from_tx(input)?;
        let to_address = tx.to;

        let mut from_account = self.db.get_account(&from_address)?;
        let mut to_account = self.db.get_account(&to_address)?;

        if tx.nonce < from_account.nonce {
            return Err(eyre::eyre!(format!(
                "Invalid nonce for {:?}, current nonce is: {:?}",
                from_address, from_account.nonce
            )));
        }

        from_account.nonce += 1;

        let mut data_chars: Vec<char> = to_account.data.chars().collect();

        for data in tx.data.clone() {
            let index = data.index;
            match data.count {
                0 => {
                    data_chars.splice(index..index, data.value.chars());
                }
                _ => {
                    data_chars.drain(index..index + data.count);
                }
            }
        }

        to_account.data = data_chars.into_iter().collect();

        if !to_account.contributors.contains(&from_address) {
            to_account.contributors.push(from_address);
        }

        self.db.set_account(&from_address, &from_account)?;
        self.db.set_account(&to_address, &to_account)?;

        Ok(())
    }
}

impl CanvasProcessor<&InMemoryDB> {
    pub fn generate_state_root(&self) -> eyre::Result<[u8; 32]> {
        let accounts = self.db.accounts.borrow();

        if accounts.len() < 1 {
            return Ok([0; 32]);
        }

        let (tree, _) = self.get_merkle_tree()?;

        let root = tree.root().expect("Could not get merkle root");
        Ok(root)
    }

    pub fn generate_proof(&self, address: &Address) -> eyre::Result<Vec<[u8; 32]>> {
        let (tree, leaves) = self.get_merkle_tree()?;
        let idx = leaves.iter().position(|l| l.account == *address);

        if idx.is_none() {
            return Err(eyre::eyre!("Address not found"));
        }

        let proof = tree.proof(&[idx.unwrap()]);
        Ok(proof.proof_hashes().to_vec())
    }

    pub fn generate_transaction_commit(
        &self,
        transactions: &Vec<SignedTransaction>,
    ) -> eyre::Result<[u8; 32]> {
        let mut transactions_encoded = Vec::<u8>::new();
        transactions.encode(&mut transactions_encoded);

        let mut zlib = ZlibEncoder::new(Vec::new(), Compression::default());
        zlib.write_all(&transactions_encoded)?;
        let transactions_compressed = zlib.finish()?;

        Ok(keccak256(transactions_compressed).into())
    }

    fn get_merkle_tree(&self) -> eyre::Result<(MerkleTree<Keccak256Algorithm>, Vec<Leaf>)> {
        let accounts = self.db.accounts.borrow();

        let mut leaves: Vec<Leaf> = Vec::new();
        accounts.iter().for_each(|(k, v)| {
            let commit = AccountCommit {
                account: *k,
                nonce: U256::from(v.nonce),
                data: v.data.clone(),
                contributors: v.contributors.clone(),
            };
            let hash = keccak256(commit.abi_encode());
            leaves.push(Leaf {
                hash: hash.into(),
                account: *k,
            });
        });

        leaves.sort_by(|a, b| a.hash.cmp(&b.hash));
        let hashes: Vec<[u8; 32]> = leaves.clone().into_iter().map(|l| l.hash).collect();

        let tree: MerkleTree<Keccak256Algorithm> = MerkleTree::from_leaves(&hashes);

        Ok((tree, leaves))
    }
}

#[derive(Clone)]
pub struct Keccak256Algorithm {}

impl Hasher for Keccak256Algorithm {
    type Hash = [u8; 32];

    fn hash(data: &[u8]) -> Self::Hash {
        keccak256(data).into()
    }

    fn concat_and_hash(left: &Self::Hash, right: Option<&Self::Hash>) -> Self::Hash {
        if right.is_none() {
            return *left;
        }

        let a: [u8; 32] = *left;
        let b: [u8; 32] = *right.unwrap();

        let mut sorted = [a, b];
        sorted.sort();

        let concatenated = sorted.concat();

        keccak256(concatenated).into()
    }
}

pub fn recover_address_from_tx(input: &SignedTransaction) -> eyre::Result<Address> {
    let signature = Signature::from_rs_and_parity(input.r, input.s, input.odd_y_parity)?;

    let mut encoded = Vec::<u8>::new();
    input.tx.encode(&mut encoded);

    Ok(signature.recover_address_from_msg(keccak256(encoded))?)
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Account {
    pub nonce: u64,
    pub data: String,
    pub contributors: Vec<Address>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InMemoryDB {
    pub accounts: RefCell<HashMap<Address, Account>>,
}

impl Default for InMemoryDB {
    fn default() -> Self {
        Self {
            accounts: RefCell::new(HashMap::new()),
        }
    }
}

pub trait AccountDB {
    fn get_account(&self, address: &Address) -> eyre::Result<Account>;
    fn set_account(&self, address: &Address, account: &Account) -> eyre::Result<()>;
}

impl AccountDB for InMemoryDB {
    fn get_account(&self, address: &Address) -> eyre::Result<Account> {
        if let Some(account) = self.accounts.borrow().get(address) {
            Ok(account.clone())
        } else {
            Ok(Account::default())
        }
    }

    fn set_account(&self, address: &Address, account: &Account) -> eyre::Result<()> {
        self.accounts.borrow_mut().insert(*address, account.clone());
        Ok(())
    }
}

impl InMemoryDB {
    pub fn snapshot_accounts(&self) -> eyre::Result<Vec<u8>> {
        Ok(bincode::serialize(&self.accounts.borrow().deref())?)
    }

    pub fn from_snapshot(snapshot: &[u8]) -> eyre::Result<InMemoryDB> {
        let db: InMemoryDB = bincode::deserialize(snapshot)?;
        Ok(db)
    }
}
