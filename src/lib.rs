use crossbeam_channel::{unbounded, Receiver, Sender};
use crossbeam_utils::thread::scope;
use itertools::Itertools;
use std::collections::{HashMap, HashSet};
use std::convert::From;
use std::sync::{Arc, Mutex};
use std::thread::available_parallelism;
use std::time::Duration;

// XXX for some reason on Apple Silicon performance degrades with multiple workers
// need to test on a multicore x86
const MAX_WORKERS: usize = 1;

cfg_if::cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        #[global_allocator]
        static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;
    }
}

#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;
#[cfg(feature = "wasm")]
use serde::{Serialize, Deserialize};
#[cfg(feature = "wasm")]
mod console_log;

type ResultSet = HashMap<i32, Number>;
type SeenType = Arc<Mutex<HashSet<Vec<i32>>>>;


#[derive(Copy, Clone)]
#[cfg_attr(feature = "wasm", derive(Serialize, Deserialize))]
pub enum Operation {
    Addition,
    Multiplication,
    Subtraction,
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

// Basic Number representation, with an optional parent which is 2 other numbers and an operation

#[cfg_attr(feature = "wasm", derive(Serialize, Deserialize))]
pub struct Number {
    pub value: i32,
    pub parent: Option<Box<(Operation, Number, Number)>>,
}

impl Number {
    // The length of a number is how many operations lead to it
    fn len(&self) -> usize {
        match &self.parent {
            None => 0,
            Some(bop) => 1 + bop.1.len() + bop.2.len(),
        }
    }
}

impl Clone for Number {
    fn clone(&self) -> Self {
        Self {
            value: self.value,
            parent: self.parent.clone(),
        }
    }
}

// Recursively display the number and its parent if any
impl std::fmt::Display for Number {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self.parent {
            None => write!(f, "{}", self.value),
            Some(bop) => {
                write!(f, "{} ({} {} {})", self.value, bop.1, bop.0, bop.2)
            }
        }
    }
}

// Only show the value
impl std::fmt::Debug for Number {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl From<i32> for Number {
    fn from(item: i32) -> Self {
        Number {
            value: item,
            parent: None,
        }
    }
}

// Remove a single matching element from a vector of numbers
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
        Operation::Addition => Some(aa + bb), // VERY unlikely overflow
        Operation::Multiplication => {
            // Unlikely overflow
            match (aa as i64 * bb as i64).try_into() {
                Ok(x) => Some(x),
                Err(_) => None,
            }
        }
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
        let value = Number {
            value,
            parent: Some(Box::new((operation, a.clone(), b.clone()))),
        };
        rtx.send(value.clone()).unwrap();

        if elements.len() > 2 {
            let mut subelements = elements.to_owned();

            remove_from_vec(&mut subelements, a);
            remove_from_vec(&mut subelements, b);

            subelements.push(value);

            tx.send(subelements).unwrap();
        }
    }
}

// Receive from the result channel, and set the elements of the result map
// If an duplicate result was is seen, use the shortest Number (least number of operations)
fn result_worker(rtx: Receiver<Number>, results: Arc<Mutex<ResultSet>>, blocking: bool) {
    loop {
        let value = if blocking {
            match rtx.recv() {
                Ok(x) => x,
                Err(_) => break,
            }
        } else {
            match rtx.try_recv() {
                Ok(x) => x,
                Err(_) => break,
            }
        };

        {
            let mut results = results.lock().unwrap();

            if let Some(current) = results.get(&value.value) {
                if current.len() > value.len() {
                    results.insert(value.value, value.clone());
                }
            } else {
                results.insert(value.value, value.clone());
            }
        }
    }
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
    seen: SeenType,
) {
    loop {
        let elements = match rx.recv_timeout(Duration::from_millis(5)) {
            Ok(x) => x,
            Err(_) => break,
        };

        {
            // Do not combine again if this set of elements was already seen
            let mut set = seen.lock().unwrap();
            let mut values: Vec<i32> = elements.iter().map(|x| x.value).collect();
            values.sort();

            // HashSet.insert returns true if element was already present
            if !set.insert(values) {
                continue;
            }
        }

        combine(tx.clone(), &elements, result_tx.clone());
    }
}

// An attempt at displaying a number and the combinations that lead to it
pub fn display_number(show: Number) {
    fn _recurse_display(n: Number, display: &mut Vec<String>) {
        if n.parent.is_none() {
            return;
        }
        //let space = std::iter::repeat(" ").take(n.len()-1).collect::<String>();

        let (op, parent_a, parent_b) = *n.parent.unwrap();
        let fmt = format!("{} {} {} = {}", parent_a.value, op, parent_b.value, n.value);

        display.insert(0, fmt);
        _recurse_display(parent_a, display);
        _recurse_display(parent_b, display);
    }

    let mut display = vec![];
    _recurse_display(show, &mut display);

    println!("{}", display.join("\n"));
}

fn js_worker( tx: Sender<Vec<Number>>,
    rx: Receiver<Vec<Number>>,
    result_tx: Sender<Number>,
    result_rx: Receiver<Number>,
    seen: SeenType,
    results: Arc<Mutex<ResultSet>>) {
        loop {
            result_worker(result_rx.clone(), results.clone(), false);

            let elements = match rx.try_recv() {
                Ok(x) => x,
                Err(_) => break,
            };

            {
                // Do not combine again if this set of elements was already seen
                let mut set = seen.lock().unwrap();
                let mut values: Vec<i32> = elements.iter().map(|x| x.value).collect();
                values.sort();

                // HashSet.insert returns true if element was already present
                if !set.insert(values) {
                    continue;
                }
            }

            combine(tx.clone(), &elements, result_tx.clone());
        }
    }//try_recv


// Main algorithm, find all combinations for a given list of integers
// Use workers + channels for multithreading
fn all_combinations(base_numbers: &[i32]) -> ResultSet {
    let ncores = match available_parallelism() {
        Ok(x) => std::cmp::max(2, x.get()),
        Err(_) => 1,
    };

    let n_workers= std::cmp::min(ncores - 1, MAX_WORKERS);

    // All possible results
    let results: Arc<Mutex<ResultSet>> = Arc::new(Mutex::new(HashMap::new()));

    // The set of list of elements we've already seen (avoid re-computing twice)
    let seen: SeenType = Arc::new(Mutex::new(HashSet::new()));

    let (combine_tx, combine_rx) = unbounded();
    let (result_tx, result_rx) = unbounded();
    // Initial list of numbers
    let initial = base_numbers.iter().map(|x| Number::from(*x)).collect();
    combine_tx.send(initial).unwrap();

    if cfg!(target_arch = "wasm32") {
        js_worker(combine_tx, combine_rx, result_tx, result_rx, seen, results.clone());

        return results.lock().unwrap().to_owned();
    }

    scope(|s| {
        let mut workers = Vec::new();
        for _ in 0..n_workers {
            let vtx = combine_tx.clone();
            let vrx = combine_rx.clone();
            let res_tx = result_tx.clone();
            let seen = seen.clone();

            let worker = s.spawn(|_| combination_worker(vtx, vrx, res_tx, seen));
            workers.push(worker);
        }

        {
            // seems to be no gain from parallelism here
            let rx = result_rx.clone();
            let worker = s.spawn(|_| result_worker(rx, results.clone(), true));
            workers.push(worker)
        }
        drop(combine_tx);
        drop(result_tx);

        for worker in workers {
            worker.join().unwrap();
        }

        results.lock().unwrap().to_owned()
    }).unwrap()
}



pub fn solve(base_numbers: &[i32], to_find: i32, approximation: i32) -> Option<Number> {
    let results = all_combinations(base_numbers);
    // println!("Found {} possible combinations", results.len());

    for i in 0..approximation + 1
    {
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

#[cfg(feature = "wasm")]
#[wasm_bindgen]
pub fn add(a: i32, b: i32) -> JsValue {

    let a = Number::from(a);
    let b = Number::from(b);

    let ret = Number {value: a.value + b.value, parent: Some(Box::new((Operation::Addition, a, b)))};

    serde_wasm_bindgen::to_value(&ret).unwrap()
}



#[cfg(test)]
mod test {
    use crate::*;

    #[test]
    fn test_combinations() {
        let numbers = vec![5, 25, 2, 50, 10];

        let combinations = all_combinations(&numbers);

        assert_eq!(combinations.len(), 1085);
        assert!(combinations.contains_key(&280));
    }

}