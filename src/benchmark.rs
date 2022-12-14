use std::thread::available_parallelism;
use std::time::Instant;

use deschiffres::all_combinations;

const LOOPS: usize = 30;

fn main() {
    let spec = [5, 25, 2, 50, 100, 10];

    let ncores = match available_parallelism() {
        Ok(x) => std::cmp::max(1, x.get()),
        Err(_) => 1,
    };

    for w in 0..ncores {
        let start = Instant::now();
        for _ in 0..LOOPS {
            all_combinations(&spec, w);
        }
        let end = Instant::now();
        println!("max={w} workers, solved in {:?}", end - start);
    }
}
