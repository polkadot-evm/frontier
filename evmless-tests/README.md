1. Install [mocha](https://mochajs.org/):
```
$ npm install --global mocha
```

2. Build the EVMless Frontier Node Template:
```
$ cargo build --release
$ ./target/release/frontier-template-node --dev
```

3. Run the tests:
```
$ cd evmless-tests
$ npm install
$ mocha evmless-erc20.test.js --timeout 10000
```
