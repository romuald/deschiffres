# deschiffres
A solver for the French game "des chiffres et des lettres", "Le compte est bon"

Given a list of numbers and a target "resolution" number, it will try to find the best way to combine those numbers using basic math to get to the target

## Compile

You can simply build the binary using `cargo build` or `cargo build -r`

Alternatively a `Makefile` is provided because I'm lazy


## Execution

The executable is currently pretty straightforward (= dumb), it only accepts numbers as arguments.
Any number larger than 100 is considered to be the target number

Example:
```
% ./target/release/deschiffres 5 25 2 50 100 10 281

Problem: find 281 with [5, 25, 2, 50, 100, 10]
Found 11864 possible combinations
Found an exact match:
100 / 50 = 2
10 + 2 = 12
25 - 2 = 23
23 * 12 = 276
5 + 276 = 281
```

The compute is pretty fast for the "standard" 6 numbers (<100ms a M1)
It theoretically works with any number of numbers, but bear in mind that the memory growth is somewhat exponential (probably)


## Web version

A web version is still in development, you can compile it with `make wasm`, and then browse the `html` directory.
Keep in mind this needs to be run on a webserver because it's using Javascript modules. You can run a dumb server with `python3 -m http.server --directory html`

**Warning** you will need the [wasm-pack](https://rustwasm.github.io/wasm-pack/) tool for rust: `cargo install wasm-pack`