## Setup

install [SP1 SDK](https://docs.succinct.xyz/getting-started/install.html)
and [Docker](https://www.docker.com/)

clone this repo:

```bash
git clone https://github.com/material-work/modulation-node.git
```

## Verify program

```bash
cd program
cargo prove build --docker
cargo prove vkey --elf elf/riscv32im-succinct-zkvm-elf
```

Ensure the returned Verification Key Hash matches the value `vKey` on the contract.

## Verfy state

```bash
cd script
cargo run
```

Ensure the state root matches the value of `stateRoot` on the contract.
