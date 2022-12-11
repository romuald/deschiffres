use crossbeam_channel::{unbounded, Receiver, Sender};
use itertools::Itertools;
use std::collections::HashMap;
use std::convert::From;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::time::{Duration};

type ResultSet = HashMap<i32, Number>;

#[derive(Copy, Clone)]
enum Operation {
    Addition,
    Multiplication,
    Substraction,
    Divison,
}

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

static ZERO: Number = Number {
    value: 0,
    parent: None,
};

impl Clone for Number {
    fn clone(&self) -> Self {
        Self {
            value: self.value,
            parent: self.parent.clone(),
        }
    }
}

impl Eq for Number {}

#[allow(dead_code)]
fn example() {
    let a = Number::from(5);
    let b = Number::from(1);
    let c = Number::from(10);

    let d = Number {
        value: a.value + b.value,
        parent: Some((Operation::Addition, Box::new(a), Box::new(b))),
    };
    let e = Number {
        value: d.value + c.value,
        parent: Some((Operation::Addition, Box::new(d), Box::new(c))),
    };

    println!("e? {e}")
}

impl std::fmt::Display for Number {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match &self.parent {
            None => write!(f, "{}", self.value),
            Some((op, a, b)) => {
                let symbol = match op {
                    Operation::Addition => "+",
                    Operation::Multiplication => "*",
                    Operation::Substraction => "-",
                    Operation::Divison => "/",
                };
                write!(f, "{} ({} {} {})", self.value, a, symbol, b)
            }
        }
    }
}

impl std::fmt::Debug for Number {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.value)
    }
}

impl Hash for Number {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}
impl std::cmp::PartialEq for Number {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
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
        if elt == to_remove {
            vec.remove(i);
            break;
        }
    }
}

fn operate(
    tx: &Sender<Vec<Number>>,
    operation: Operation,
    a: &Number,
    b: &Number,
    elements: &[Number],
    tx2: &Sender<Number>,
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
        tx2.send(value.clone()).unwrap();

        let mut subelements = elements.to_owned();
        subelements.push(value);
        remove_from_vec(&mut subelements, a);
        remove_from_vec(&mut subelements, b);

        if subelements.len() > 1 {
            tx.send(subelements).unwrap();
        }
    }
}

fn more_results(rx: Receiver<Number>, results: Arc<Mutex<ResultSet>>) {
    loop {
        let value = rx.recv_timeout(Duration::from_millis(5));
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

fn combine(tx: Sender<Vec<Number>>, elements: &[Number], tx2: Sender<Number>) {
    let combinaisons = elements.iter().combinations(2).into_iter().map(to_tuple);

    for (a, b) in combinaisons {
        operate(&tx, Operation::Addition, a, b, elements, &tx2);
        operate(&tx, Operation::Multiplication, a, b, elements, &tx2);
        operate(&tx, Operation::Substraction, a, b, elements, &tx2);
        operate(&tx, Operation::Substraction, b, a, elements, &tx2);
        operate(&tx, Operation::Divison, a, b, elements, &tx2);
        operate(&tx, Operation::Divison, b, a, elements, &tx2);
    }
}

fn handler(tx: Sender<Vec<Number>>, rx: Receiver<Vec<Number>>, tx2: Sender<Number>) {
    loop {
        let tx = tx.clone();
        let value = rx.recv_timeout(Duration::from_millis(5));
        let elements = match value {
            Ok(x) => x,
            Err(_) => break,
        };

        combine(tx, &elements, tx2.clone());
    }
}

// fn spawn_worker(scope: &Scope, tx: Sender<Vec<Number>>, rx: Receiver<Vec<Number>>, tx2: Sender<Number>) -> ScopedJoinHandle<()> {
//     scope.spawn(|_| {
//         handler(tx, rx, res_tx)
//     })
// }
fn main() {
    use std::thread::available_parallelism;
    let ncores = match available_parallelism() {
        Ok(x) => x.get(),
        Err(_) => 4,
    };

    println!("{ncores} cores");

    let (combine_tx, combine_rx) = unbounded();
    let (result_tx, result_rx) = unbounded();

    let todo = vec![5, 25, 2, 50, 100, 10];
    let todo: Vec<Number> = todo.iter().map(|&x| Number::from(x)).collect();

    let mut elements: Vec<Number> = Vec::new();
    let results: Arc<Mutex<ResultSet>> = Arc::new(Mutex::new(HashMap::new()));

    elements.extend(todo);

    combine_tx.send(elements.clone()).unwrap();
    crossbeam_utils::thread::scope(|s| {
        let mut workers = Vec::new();
        for _ in 0..ncores-1 {
            let vtx = combine_tx.clone();
            let vrx = combine_rx.clone();
            let res_tx = result_tx.clone();

            let worker = s.spawn(|_| handler(vtx, vrx, res_tx));
            workers.push(worker);
        }

        { // seems to be no gain from parallelism here
            let rx = result_rx.clone();
            let worker = s.spawn(|_| more_results(rx, results.clone()));
            workers.push(worker)
        }

        for worker in workers {
            worker.join().unwrap();
        }

        {
            let mut results = results.lock().unwrap();
            println!("realy? {:?}", results.len());

            let find_me = 280;

            if let Some(found) = results.get_mut(&find_me) {
                println!("Found {} times, with len {}", found, found.len());
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
