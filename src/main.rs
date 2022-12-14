use std::{process::exit, time::Instant};

use deschiffres::solve;

fn parse_args() -> (Vec<i32>, i32) {
    let args = std::env::args().skip(1);
    let mut numbers: Vec<i32> = vec![];

    let mut find_me = -1;
    for argument in args {
        let number = match argument.parse() {
            Ok(n) => n,
            Err(_) => continue,
        };
        if number > 100 {
            find_me = number;
            continue;
        }

        if number < 1 {
            continue;
        }

        numbers.push(number);
    }

    if find_me == -1 {
        eprintln!("Nothing to find (no number greater than 100)");
        exit(1);
    }

    if numbers.len() < 2 {
        eprintln!("There should be at least 2 numbers, don't you think?");
        exit(1);
    }

    (numbers, find_me)
}

fn main() {
    let (spec, to_find) = parse_args();

    let approximation = 0; // Possibly try to find an approximate match up to n (int)
    println!("Problem: find {to_find} with {spec:?}");

    let start = Instant::now();
    let result = solve(&spec, to_find, approximation);
    let end = Instant::now();
    println!("Solved in {:?}", end - start);

    if let Some(result) = result {
        if result.value == to_find {
            println!("Found an exact match:");
        } else {
            println!("Found an approximate match:");
        }
        println!("{}", result.as_text());
    } else {
        println!("Did not find a match");
    }
}
