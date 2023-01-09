use crossbeam_channel::{unbounded, Receiver, Sender};
use crossbeam_utils::thread::scope as cross_scope;
use itertools::Itertools;
use std::collections::{HashMap, HashSet};
use std::thread::available_parallelism;
use std::time::Duration;

// This only affects the `solve` method (not the benchmarks)
const MAX_WORKERS: usize = 0;

cfg_if::cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        #[global_allocator]
        static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;
    }
}

#[cfg(feature = "wasm")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;
#[cfg(feature = "wasm")]
mod console_log;

type ResultSet = HashMap<i32, Number>;

#[derive(Clone, Copy)]
#[cfg_attr(feature = "wasm", derive(Serialize, Deserialize))]
// A materialized operation (a + b) without the result
pub struct MOperation(pub Operation, pub i32, pub i32);

#[derive(Clone)]
#[cfg_attr(feature = "wasm", derive(Serialize, Deserialize))]
// Number with the operations that lead to it
pub struct Number {
    pub value: i32,
    pub operations: Vec<MOperation>,
}

impl Number {
    fn from_int(n: i32) -> Self {
        Self {
            value: n,
            operations: vec![],
        }
    }

    fn from(value: i32, op: Operation, a: &Number, b: &Number) -> Self {
        let operations = [
            &[MOperation(op, a.value, b.value)][..],
            &a.operations,
            &b.operations,
        ]
        .concat();

        Self { value, operations }
    }

    // The length of a number is how many operations lead to it
    fn len(&self) -> usize {
        self.operations.len()
    }

    // A text representation of the calculus that lead to this Number
    pub fn as_text(self) -> String {
        let mut output = vec![];
        for op in self.operations.iter().rev() {
            let val = match op.0 {
                Operation::Addition => op.1 + op.2,
                Operation::Multiplication => op.1 * op.2,
                Operation::Subtraction => op.1 - op.2,
                Operation::Division => op.1 / op.2,
            };
            let fmt = format!("{} {} {} = {}", op.1, op.0, op.2, val);
            output.push(fmt);
        }

        output.join("\n")
    }
}

// Only show the value
impl std::fmt::Debug for Number {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

#[derive(Copy, Clone)]
#[cfg_attr(feature = "wasm", derive(Serialize, Deserialize))]
pub enum Operation {
    #[cfg_attr(feature = "wasm", serde(rename = "+"))]
    Addition,
    #[cfg_attr(feature = "wasm", serde(rename = "*"))]
    Multiplication,
    #[cfg_attr(feature = "wasm", serde(rename = "-"))]
    Subtraction,
    #[cfg_attr(feature = "wasm", serde(rename = "/"))]
    Division,
}
impl std::fmt::Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match &self {
                Operation::Addition => "+",
                Operation::Multiplication => "*",
                Operation::Subtraction => "-",
                Operation::Division => "/",
            }
        )
    }
}

// Remove a single matching element from a vector of numbers
//#[inline]
fn remove_from_vec(vec: &mut Vec<Number>, to_remove: &Number) {
    for (i, elt) in vec.iter().enumerate() {
        if elt.value == to_remove.value {
            vec.remove(i);
            return;
        }
    }

    panic!("{to_remove:?} was not present in elements")
}

// Compute a single operation on 2 numbers (of a given list of numbers)
// This operation may fail (eg: number less than 0, non-integer division, …)
// In case it succeed, remove those 2 from the list and append the result
// then send this new element list to the "operation" channel
fn operate(
    tx: &Sender<Vec<Number>>,
    operation: Operation,
    a: &Number,
    b: &Number,
    elements: &[Number],
    rtx: &Sender<Number>,
) {
    let aa = a.value;
    let bb = b.value;

    let value = match operation {
        Operation::Addition => i32::checked_add(aa, bb),
        Operation::Multiplication => i32::checked_mul(aa, bb),
        Operation::Subtraction => {
            if aa - bb > 0 {
                Some(aa - bb)
            } else {
                None
            }
        }
        Operation::Division => {
            if bb > 0 && aa % bb == 0 {
                Some(aa / bb)
            } else {
                None
            }
        }
    };

    if let Some(value) = value {
        let value = Number::from(value, operation, a, b);
        rtx.send(value.clone()).unwrap();

        if elements.len() > 2 {
            let mut subelements = elements.to_owned();

            remove_from_vec(&mut subelements, a);
            remove_from_vec(&mut subelements, b);

            subelements.push(value);
            subelements.sort_by(|a, b| a.value.partial_cmp(&b.value).unwrap());

            tx.send(subelements).unwrap();
        }
    }
}

// Receive from the result channel, and set the elements of the result map
// If an duplicate result was is seen, use the shortest Number (least number of operations)
fn result_worker(rtx: Receiver<Number>) -> ResultSet {
    let mut results: ResultSet = HashMap::new();

    while let Ok(value) = rtx.recv() {
        if let Some(current) = results.get(&value.value) {
            if current.len() > value.len() {
                results.insert(value.value, value.clone());
            }
        } else {
            results.insert(value.value, value.clone());
        }
    }
    results
}

// Given a list of Number, try to combinate every possible pair of them
// Then append those results to the combine channel
fn combine(tx: Sender<Vec<Number>>, elements: &[Number], rtx: Sender<Number>) {
    for pair in elements.iter().combinations(2) {
        if let [a, b] = pair[..] {
            operate(&tx, Operation::Addition, a, b, elements, &rtx);
            operate(&tx, Operation::Multiplication, a, b, elements, &rtx);
            operate(&tx, Operation::Subtraction, a, b, elements, &rtx);
            operate(&tx, Operation::Subtraction, b, a, elements, &rtx);
            operate(&tx, Operation::Division, a, b, elements, &rtx);
            operate(&tx, Operation::Division, b, a, elements, &rtx);
        }
    }
}

// Listen the combination channel for new lists of Numbers, and combine them
// (that will probably generate more combination events)
fn combination_worker(
    tx: Sender<Vec<Number>>,
    rx: Receiver<Vec<Number>>,
    result_tx: Sender<Number>,
) {
    while let Ok(elements) = rx.recv_timeout(Duration::from_millis(2)) {
        combine(tx.clone(), &elements, result_tx.clone());
    }
}

// Single thread/worker that recieve the combinaisons
// and only forwards them if they weren't already seen
fn combine_sieve(rx: Receiver<Vec<Number>>, tx: Sender<Vec<Number>>) {
    let mut seen = HashSet::with_capacity(500);

    while let Ok(elements) = rx.recv_timeout(Duration::from_millis(2)) {
        // Map elements to integers
        let values: Vec<i32> = elements.iter().map(|x| x.value).collect();

        // HashSet.insert returns true if element was NOT present
        if seen.insert(values) {
            tx.send(elements).unwrap()
        }
    }
}

#[inline]
fn results_append(rx: &Receiver<Number>, results: &mut HashMap<i32, Number>) {
    while let Ok(value) = rx.try_recv() {
        if let Some(current) = results.get(&value.value) {
            if current.len() > value.len() {
                results.insert(value.value, value.clone());
            }
        } else {
            results.insert(value.value, value.clone());
        }
    }
}

fn threadless_worker(
    tx: Sender<Vec<Number>>,
    rx: Receiver<Vec<Number>>,
    result_tx: Sender<Number>,
    result_rx: Receiver<Number>,
) -> ResultSet {
    let mut seen = HashSet::with_capacity(500);
    let mut results: HashMap<i32, Number> = HashMap::with_capacity(500);

    loop {
        results_append(&result_rx, &mut results);

        let elements = match rx.try_recv() {
            Ok(x) => x,
            Err(_) => break,
        };

        // Do not combine again if this set of elements was already seen
        let mut values: Vec<i32> = elements.iter().map(|x| x.value).collect();
        values.sort();

        // HashSet.insert returns true if element was already present
        if !seen.insert(values) {
            continue;
        }

        combine(tx.clone(), &elements, result_tx.clone());
    }
    results
}

// Main algorithm, find all combinations for a given list of integers
// Use workers + channels for multithreading
pub fn all_combinations(base_numbers: &[i32], max_workers: usize) -> ResultSet {
    let ncores = match available_parallelism() {
        Ok(x) => x.get(),
        Err(_) => 1,
    };

    let nworkers = match ncores {
        0 | 1 | 2 => 1,
        n => std::cmp::min(n - 2, max_workers),
    };

    let (combine_tx, combine_rx) = unbounded();
    let (sieve_tx, sieve_rx) = unbounded();
    let (result_tx, result_rx) = unbounded();

    // Initial list of numbers
    let initial = base_numbers.iter().map(|x| Number::from_int(*x)).collect();
    combine_tx.send(initial).unwrap();

    if cfg!(target_arch = "wasm32") || nworkers < 2 {
        return threadless_worker(combine_tx, combine_rx, result_tx, result_rx);
    }

    // WARNING: the current implementation is bugged
    // Since the sieve / combien threads are feeding each other,
    // there is no way of reliably know when they are both finished (that is still performant)
    // In some cases the workers are too slow to fill the channels and the worker exits early
    cross_scope(|scope| {
        let mut workers = Vec::new();

        // Combinaison workers (ncores - 2)
        for _ in 0..nworkers {
            let result_tx = result_tx.clone();

            // Sent new combinaisons to the sieve
            let tx = sieve_tx.clone();
            let rx = combine_rx.clone();

            let worker = scope.spawn(|_| combination_worker(tx, rx, result_tx));
            workers.push(worker);
        }
        drop(result_tx);

        // Sieve worker
        {
            let sieve_rx = sieve_rx.clone();
            let combine_tx = combine_tx.clone();
            let worker = scope.spawn(|_| combine_sieve(sieve_rx, combine_tx));
            workers.push(worker)
        }

        // No need? Workers should have finished by the time result_worker is done
        //for worker in workers {
        //    worker.join().unwrap();
        //}

        result_worker(result_rx)
    })
    .unwrap()
}

pub fn solve(base_numbers: &[i32], to_find: i32, approximation: i32) -> Option<Number> {
    let results = all_combinations(base_numbers, MAX_WORKERS);
    // println!("Found {} possible combinations", results.len());

    for i in 0..approximation + 1 {
        if let Some(result) = results.get(&(to_find + i)) {
            return Some(result.to_owned());
        } else if let Some(result) = results.get(&(to_find - i)) {
            return Some(result.to_owned());
        }
    }

    None
}

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn solve_js(base_numbers: &[i32], to_find: i32, approximation: i32) -> JsValue {
    std::panic::set_hook(Box::new(console_error_panic_hook::hook));

    let solved = solve(base_numbers, to_find, approximation);

    serde_wasm_bindgen::to_value(&solved).unwrap()
}

#[cfg(test)]
mod test {
    use crate::*;

    #[test]
    fn test_combinations_multi() {
        let numbers = vec![5, 25, 2, 50, 10];

        let combinations = all_combinations(&numbers, 4);

        assert_eq!(combinations.len(), 1085);
        assert!(combinations.contains_key(&280));
    }

    #[test]
    fn test_combinations_single() {
        let numbers = vec![5, 25, 2, 50, 10];

        let combinations = all_combinations(&numbers, 0);

        assert_eq!(combinations.len(), 1085);
        assert!(combinations.contains_key(&280));
    }
}
