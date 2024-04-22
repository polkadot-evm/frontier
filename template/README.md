# Frontier Node Template

A [FRAME](https://docs.substrate.io/v3/runtime/frame)-based [Substrate](https://substrate.io) template node with the Ethereum RPC support, ready for hacking.

## Generation & Upstream

This template was originally forked from the [Substrate Node Template](https://github.com/substrate-developer-hub/substrate-node-template). You can find more information on features of this template there, and more detailed usage on the [Substrate Developer Hub Tutorials](https://docs.substrate.io/tutorials/v3/) that use this heavily.

## Build & Run

To build the chain, execute the following commands from the project root:

```sh
$ cargo build --release
```

To execute the chain, run:

```sh
$ ./target/release/frontier-template-node --dev
```

The node also supports to use manual seal (to produce block manually through RPC). This is also used by the `ts-tests`:

```sh
$ ./target/release/frontier-template-node --dev --sealing=manual
```

The output shows the following logs:

```sh
2024-03-06 10:39:57 Frontier Node    
2024-03-06 10:39:57 ✌️  version 0.0.0-f301825c59d    
2024-03-06 10:39:57 ❤️  by Parity Technologies <admin@parity.io>, 2021-2024    
2024-03-06 10:39:57 📋 Chain specification: Development    
2024-03-06 10:39:57 🏷  Node name: lacking-carriage-4724    
2024-03-06 10:39:57 👤 Role: AUTHORITY    
2024-03-06 10:39:57 💾 Database: RocksDb at /tmp/substrateLf8j5A/chains/dev/db/full    
2024-03-06 10:39:57 🔨 Initializing Genesis block/state (state: 0x6360…7a03, header-hash: 0x9392…cf75)    
2024-03-06 10:39:57 👴 Loading GRANDPA authority set from genesis on what appears to be first startup.    
2024-03-06 10:39:58 Using default protocol ID "sup" because none is configured in the chain specs    
2024-03-06 10:39:58 🏷  Local node identity is: 12D3KooWMVR2r9mktbshMB1FBVU4Pf8eQrnBChUp6AxQYeokysWy    
2024-03-06 10:39:58 💻 Operating system: linux    
2024-03-06 10:39:58 💻 CPU architecture: x86_64    
2024-03-06 10:39:58 💻 Target environment: gnu    
2024-03-06 10:39:58 💻 CPU: AMD Ryzen 7 5700G with Radeon Graphics    
2024-03-06 10:39:58 💻 CPU cores: 8    
2024-03-06 10:39:58 💻 Memory: 63578MB    
2024-03-06 10:39:58 💻 Kernel: 6.5.0-18-generic    
2024-03-06 10:39:58 💻 Linux distribution: Ubuntu 22.04.4 LTS    
2024-03-06 10:39:58 💻 Virtual machine: no    
2024-03-06 10:39:58 📦 Highest known block at #0    
2024-03-06 10:39:58 〽️ Prometheus exporter started at 127.0.0.1:9615    
2024-03-06 10:39:58 Running JSON-RPC server: addr=0.0.0.0:9944, allowed origins=["*"]    
2024-03-06 10:40:00 🙌 Starting consensus session on top of parent 0x939219b0f84644d7a2862f4544af20d571d80250871f7469f634ec52c122cf75    
2024-03-06 10:40:00 🎁 Prepared block for proposing at 1 (0 ms) [hash: 0x148fc7d360aa7f4ad32992e4a6e5e1a140c98b0d13e8da854becc4741e28e2b8; parent_hash: 0x9392…cf75; extrinsics (2): [0x0f84…abb2, 0x549b…7957]    
2024-03-06 10:40:00 🔖 Pre-sealed block for proposal at 1. Hash now 0x782478b32ea46f5607eef9b902ec7d8fc44ebc9ffa8d7be2534028bf8d0c0fce, previously 0x148fc7d360aa7f4ad32992e4a6e5e1a140c98b0d13e8da854becc4741e28e2b8.    
2024-03-06 10:40:00 ✨ Imported #1 (0x7824…0fce)    
2024-03-06 10:40:03 💤 Idle (0 peers), best: #1 (0x7824…0fce), finalized #0 (0x9392…cf75), ⬇ 0 ⬆ 0    
2024-03-06 10:40:06 🙌 Starting consensus session on top of parent 0x782478b32ea46f5607eef9b902ec7d8fc44ebc9ffa8d7be2534028bf8d0c0fce    
2024-03-06 10:40:06 🎁 Prepared block for proposing at 2 (1 ms) [hash: 0xea2b3b5472272a2fc3ab066b6f228aaeba4e209e28bd108308267eb5494b6d94; parent_hash: 0x7824…0fce; extrinsics (2): [0x4d0c…c9c0, 0x549b…7957]    
2024-03-06 10:40:06 🔖 Pre-sealed block for proposal at 2. Hash now 0x9dde0a816c6a21b7761edc930c7527a07208b55998ed0cf65ddbc0a6c06570b3, previously 0xea2b3b5472272a2fc3ab066b6f228aaeba4e209e28bd108308267eb5494b6d94.    
2024-03-06 10:40:06 ✨ Imported #2 (0x9dde…70b3)    
2024-03-06 10:40:08 💤 Idle (0 peers), best: #2 (0x9dde…70b3), finalized #0 (0x9392…cf75), ⬇ 0 ⬆ 0    
2024-03-06 10:40:09 📪 No longer listening on /ip6/fe80::b483:25ff:fe16:5d02/tcp/30333    
2024-03-06 10:40:12 🙌 Starting consensus session on top of parent 0x9dde0a816c6a21b7761edc930c7527a07208b55998ed0cf65ddbc0a6c06570b3    
2024-03-06 10:40:12 🎁 Prepared block for proposing at 3 (0 ms) [hash: 0x1de6d69e3953bb29284a7d5b664a675920db81df3e8a1d828a8facf9ac3c8a21; parent_hash: 0x9dde…70b3; extrinsics (2): [0x4e6e…6257, 0x549b…7957]    
2024-03-06 10:40:12 🔖 Pre-sealed block for proposal at 3. Hash now 0x96af2e23277b4127396d565eccc3c88857c327cb6d360d3ebe3f689f42667fe5, previously 0x1de6d69e3953bb29284a7d5b664a675920db81df3e8a1d828a8facf9ac3c8a21.    
2024-03-06 10:40:12 ✨ Imported #3 (0x96af…7fe5)    
2024-03-06 10:40:13 💤 Idle (0 peers), best: #3 (0x96af…7fe5), finalized #1 (0x7824…0fce), ⬇ 0 ⬆ 0    
2024-03-06 10:40:18 🙌 Starting consensus session on top of parent 0x96af2e23277b4127396d565eccc3c88857c327cb6d360d3ebe3f689f42667fe5    
2024-03-06 10:40:18 🎁 Prepared block for proposing at 4 (0 ms) [hash: 0x0df414ecaab38bcf28e57b3225d9d665f8b29edc557a6d235918067f1fa91a43; parent_hash: 0x96af…7fe5; extrinsics (2): [0x51a6…7b15, 0x549b…7957]    
2024-03-06 10:40:18 🔖 Pre-sealed block for proposal at 4. Hash now 0xf293992d51d1a6943a2ddc37d465ae56e7783fe4d1c704f724910d423e0195d6, previously 0x0df414ecaab38bcf28e57b3225d9d665f8b29edc557a6d235918067f1fa91a43.    
2024-03-06 10:40:18 ✨ Imported #4 (0xf293…95d6)    
2024-03-06 10:40:18 💤 Idle (0 peers), best: #4 (0xf293…95d6), finalized #1 (0x7824…0fce), ⬇ 0 ⬆ 0    
2024-03-06 10:40:21 📪 No longer listening on /ip6/fe80::6065:e5ff:fe84:2a0/tcp/30333    
2024-03-06 10:40:23 💤 Idle (0 peers), best: #4 (0xf293…95d6), finalized #2 (0x9dde…70b3), ⬇ 0 ⬆ 0 
```

## Usage

The default port for the template node is set to `http://127.0.0.1:9944`. Once the node is operational, you can conduct your own tests, including connecting to Ethereum wallets or interacting with smart contracts. Additionally, there are several predefined accounts with test tokens available for immediate use.

- Alith:
    * Public Address: 0xf24FF3a9CF04c71Dbc94D0b566f7A27B94566cac
    * Private Key: 0x5fb92d6e98884f76de468fa3f6278f8807c48bebc13595d45af5bdc4da702133
- Baltathar:
    * Public Address: 0x3Cd0A705a2DC65e5b1E1205896BaA2be8A07c6e0
    * Private Key: 0x8075991ce870b93a8870eca0c0f91913d12f47948ca0fd25b49c6fa7cdbeee8b
- Charleth:
    * Public Address: 0x798d4Ba9baf0064Ec19eB4F0a1a45785ae9D6DFc
    * Private Key: 0x0b6e18cafb6ed99687ec547bd28139cafdd2bffe70e6b688025de6b445aa5c5b
- Dorothy:
    * Public Address: 0x773539d4Ac0e786233D90A233654ccEE26a613D9
    * Private Key: 0x39539ab1876910bbf3a223d84a29e28f1cb4e2e456503e7e91ed39b2e7223d68
- Ethan:
    * Public Address: 0xFf64d3F6efE2317EE2807d223a0Bdc4c0c49dfDB
    * Private Key: 0x7dce9bc8babb68fec1409be38c8e1a52650206a7ed90ff956ae8a6d15eeaaef4
- Faith:
    * Public Address: 0xC0F0f4ab324C46e55D02D0033343B4Be8A55532d
    * Private Key: 0xb9d2ea9a615f3165812e8d44de0d24da9bbd164b65c4f0573e1ce2c8dbd9c8df

