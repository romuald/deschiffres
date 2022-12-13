use crossbeam_channel::{unbounded, Receiver, Sender};
use crossbeam_utils::thread::scope;
use itertools::Itertools;
use std::collections::{HashMap, HashSet};
use std::convert::From;
use std::process::exit;
use std::sync::{Arc, Mutex};
use std::thread::available_parallelism;
use std::time::Duration;

type ResultSet = HashMap<i32, Number>;
type SeenType = Arc<Mutex<HashSet<Vec<i32>>>>;

#[derive(Copy, Clone)]
enum Operation {
    Addition,
    Multiplication,
    Substraction,
    Divison,
}
impl std::fmt::Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match &self {
                Operation::Addition => "+",
                Operation::Multiplication => "*",
                Operation::Substraction => "-",
                Operation::Divison => "/",
            }
        )
    }
}

// Basic Number representation, with an optional parent which is 2 other numbers and an operation
struct Number {
    value: i32,
    parent: Option<(Operation, Box<Number>, Box<Number>)>,
}

impl Number {
    fn len(&self) -> usize {
        match &self.parent {
            None => 0,
            Some((_, a, b)) => 1 + a.len() + b.len(),
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
            Some((op, a, b)) => {
                write!(f, "{} ({} {} {})", self.value, a, op, b)
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

fn remove_from_vec(vec: &mut Vec<Number>, to_remove: &Number) {
    for (i, elt) in vec.iter().enumerate() {
        if elt.value == to_remove.value {
            vec.remove(i);
            return;
        }
    }
    panic!("Not removed??")
}

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
        Operation::Addition => Some(aa + bb), // VERY unlikelly overflow
        Operation::Multiplication => {
            // Unlikelly overflow
            match (aa as i64 * bb as i64).try_into() {
                Ok(x) => Some(x),
                Err(_) => None,
            }
        }
        Operation::Substraction => {
            if aa - bb > 0 {
                Some(aa - bb)
            } else {
                None
            }
        }
        Operation::Divison => {
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
            parent: Some((operation, Box::new(a.clone()), Box::new(b.clone()))),
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
// If a result was already seen, use the shortest one
fn result_worker(rtx: Receiver<Number>, results: Arc<Mutex<ResultSet>>) {
    loop {
        let value = rtx.recv();
        let value = match value {
            Ok(x) => x,
            Err(_) => break,
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
            operate(&tx, Operation::Substraction, a, b, elements, &rtx);
            operate(&tx, Operation::Substraction, b, a, elements, &rtx);
            operate(&tx, Operation::Divison, a, b, elements, &rtx);
            operate(&tx, Operation::Divison, b, a, elements, &rtx);
        }
    }
}

// Listen the combinaison channel for new lists of Numbers, and combine them
// (that will probably generate more combinaison events)
fn combinaison_worker(
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
            let mut set = seen.lock().unwrap();
            let mut values: Vec<i32> = elements.iter().map(|x| x.value).collect();
            values.sort();
            if set.contains(&values) {
                continue;
            } else {
                set.insert(values);
            }
        }

        combine(tx.clone(), &elements, result_tx.clone());
    }
}

#[allow(dead_code)]
fn example() {
    let q = Number {
        value: 15,
        parent: Some((
            Operation::Multiplication,
            Box::new(Number::from(3)),
            Box::new(Number::from(5)),
        )),
    };

    let show = Number {
        value: 18,
        parent: Some((Operation::Addition, Box::new(q), Box::new(Number::from(3)))),
    };

    display_number(show)
}

fn display_number(show: Number) {
    fn _recurse_display(n: Number, display: &mut Vec<String>) {
        if n.parent.is_none() {
            return;
        }
        //let space = std::iter::repeat(" ").take(n.len()-1).collect::<String>();

        let (op, parent_a, parent_b) = n.parent.unwrap();
        let fmt = format!("{} {} {} = {}", parent_a.value, op, parent_b.value, n.value);

        display.insert(0, fmt);
        _recurse_display(*parent_a, display);
        _recurse_display(*parent_b, display);
    }

    let mut display = vec![];
    _recurse_display(show, &mut display);
    println!("{}", display.join("\n"));
}

fn all_combinaisons(base_numbers: &[i32]) -> ResultSet {
    let ncores = match available_parallelism() {
        Ok(x) => std::cmp::max(2, x.get()),
        Err(_) => 4,
    };

    let results: Arc<Mutex<ResultSet>> = Arc::new(Mutex::new(HashMap::new()));
    let seen: SeenType = Arc::new(Mutex::new(HashSet::new()));

    let (combine_tx, combine_rx) = unbounded();
    let (result_tx, result_rx) = unbounded();

    let initial = base_numbers.iter().map(|x| Number::from(*x)).collect();
    combine_tx.send(initial).unwrap();

    scope(|s| {
        let mut workers = Vec::new();
        for _ in 0..ncores - 1 {
            let vtx = combine_tx.clone();
            let vrx = combine_rx.clone();
            let res_tx = result_tx.clone();
            let seen = seen.clone();

            let worker = s.spawn(|_| combinaison_worker(vtx, vrx, res_tx, seen));
            workers.push(worker);
        }

        {
            // seems to be no gain from parallelism here
            let rx = result_rx.clone();
            let worker = s.spawn(|_| result_worker(rx, results.clone()));
            workers.push(worker)
        }
        drop(combine_tx);
        drop(result_tx);

        for worker in workers {
            worker.join().unwrap();
        }

        results.lock().unwrap().to_owned()
    })
    .unwrap()
}

fn parse_args() -> (Vec<i32>, i32) {
    let args = std::env::args().skip(1);
    let mut numbers: Vec<i32> = vec![];

    // XXX map + filter negative
    let mut find_me = -1;
    for argument in args {
        let number = match argument.parse() {
            Ok(n) => n,
            Err(_) => continue,
        };
        if number > 100 {
            find_me = number;
            continue;
            //XXX if find_me != -1 {}
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

    (numbers, find_me)
}

fn main() {
    let (spec, to_find) = parse_args();

    let approximation = 0; // Possibly try to find an approximate match up to n  (int)
    let results = all_combinaisons(&spec);

    println!("Problem: find {to_find} with {spec:?}");
    println!("Found {} possible combinaisons", results.len());

    let mut found = false;
    'outer: for i in 0..approximation + 1 {
        for sign in [-1, 1].iter() {
            let value = to_find + i * sign;
            if let Some(result) = results.get(&value) {
                let what = if result.value == to_find {
                    "exact"
                } else {
                    "approximate"
                };
                println!("Found an {what} match:");
                display_number(result.to_owned());
                found = true;
                break 'outer;
            }
        }
    }

    if !found {
        println!("Did not find a match")
    }
}
