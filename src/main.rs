use crossbeam_channel::{unbounded, Receiver, Sender};
use crossbeam_utils::thread::scope;
use itertools::Itertools;
use std::collections::HashMap;
use std::convert::From;
use std::sync::{Arc, Mutex};
use std::thread::available_parallelism;
use std::time::Duration;

type ResultSet = HashMap<i32, Number>;

#[derive(Copy, Clone)]
enum Operation {
    Addition,
    Multiplication,
    Substraction,
    Divison,
}
impl std::fmt::Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", match &self {
            Operation::Addition => "+",
            Operation::Multiplication => "*",
            Operation::Substraction => "-",
            Operation::Divison => "/",
        })
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

static ZERO: Number = Number {value: 0, parent: None};

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
        Operation::Addition => Some(aa + bb),
        Operation::Multiplication => Some(aa * bb),
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

        let mut subelements = elements.to_owned();

        remove_from_vec(&mut subelements, a);
        remove_from_vec(&mut subelements, b);

        subelements.push(value);

        if subelements.len() > 1 {
            tx.send(subelements).unwrap();
        }
    }
}

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

fn to_tuple(x: Vec<&Number>) -> (&Number, &Number) {
    if let Some(a) = x.first() {
        if let Some(b) = x.get(1) {
            return (a, b);
        }
    }

    (&ZERO, &ZERO)
}

fn combine(tx: Sender<Vec<Number>>, elements: &[Number], rtx: Sender<Number>) {
    let combinaisons = elements.iter().combinations(2).into_iter().map(to_tuple);

    for (a, b) in combinaisons {
        operate(&tx, Operation::Addition, a, b, elements, &rtx);
        operate(&tx, Operation::Multiplication, a, b, elements, &rtx);
        operate(&tx, Operation::Substraction, a, b, elements, &rtx);
        operate(&tx, Operation::Substraction, b, a, elements, &rtx);
        operate(&tx, Operation::Divison, a, b, elements, &rtx);
        operate(&tx, Operation::Divison, b, a, elements, &rtx);
    }
}

fn combinaison_worker(tx: Sender<Vec<Number>>, rx: Receiver<Vec<Number>>, result_tx: Sender<Number>) {
    loop {
        let elements = match rx.recv_timeout(Duration::from_millis(5)) {
            Ok(x) => x,
            Err(_) => break,
        };

        combine(tx.clone(), &elements, result_tx.clone());
    }
}

#[allow(dead_code)]
fn example() {

    let q = Number {
        value: 15,
        parent: Some((Operation::Multiplication, Box::new(Number::from(3)), Box::new(Number::from(5))))
    };

    let show = Number {
        value: 18,
        parent: Some((Operation::Addition, Box::new(q), Box::new(Number::from(3))))
    };

    display_number(show)
}


fn display_number(show: Number) {
    fn _recurse_display(n: Number, display: &mut Vec<String>) {
        if n.parent.is_none() {
            return
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

fn main() {
    let ncores = match available_parallelism() {
        Ok(x) => std::cmp::max(2, x.get()),
        Err(_) => 4,
    };

    let (combine_tx, combine_rx) = unbounded();
    let (result_tx, result_rx) = unbounded();

    let spec = vec![5, 25, 2, 50, 100, 10];
    let todo: Vec<Number> = spec.iter().map(|&x| Number::from(x)).collect();

    let mut elements: Vec<Number> = Vec::new();
    let results: Arc<Mutex<ResultSet>> = Arc::new(Mutex::new(HashMap::new()));

    elements.extend(todo);

    combine_tx.send(elements.clone()).unwrap();

    scope(|s| {
        let mut workers = Vec::new();
        for _ in 0..ncores - 1 {
            let vtx = combine_tx.clone();
            let vrx = combine_rx.clone();
            let res_tx = result_tx.clone();

            let worker = s.spawn(|_| combinaison_worker(vtx, vrx, res_tx));
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

        {
            let find_me = 280;
            let mut results = results.lock().unwrap();

            println!("Problem find {find_me} with {:?}", spec);
            println!("Found {} possible combinaison", results.len());

            if let Some(found) = results.get_mut(&find_me) {
                println!("Found a solution with {} operations:", found.len());
                display_number(found.clone());
            } else {
                println!("Did not find a solution")
            }
        }
    })
    .unwrap();

    /*
    let start = Instant::now();
    //combine(&elements, &mut results);
    let end = Instant::now();

    println!("Base: {todo:?}");
    println!("Stack: {:?}", results.len());
    println!("Computed in {:?}", end - start);
    let find_me = 281;

    if let Some(found) = results.get_mut(&find_me) {
        println!("Found {} times, with len {}", found, found.len());
    }
    */
}
