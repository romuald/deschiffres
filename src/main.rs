use std::hash::{Hash, Hasher};
use std::convert::From;
use std::collections::HashMap;
use std::time::Instant;
use itertools::Itertools;

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

static ZERO: Number = Number { value:0, parent:None };

impl Clone for Number {
    fn clone(&self) -> Self {
        Self { value: self.value, parent: self.parent.clone() }
    }
}


impl Eq for Number {}

#[allow(dead_code)]
fn example() {
    let a = Number::from(5);
    let b = Number::from(1);
    let c = Number::from(10);

    let d = Number { value: a.value+b.value, parent: Some((Operation::Addition, Box::new(a), Box::new(b)))};
    let e = Number { value: d.value+c.value, parent: Some((Operation::Addition, Box::new(d), Box::new(c)))};

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

impl  std::fmt::Debug for Number {
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
        Number { value: item, parent: None }
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

fn operate(operation: Operation, a: &Number, b: &Number, elements: &[Number], results: &mut ResultSet) {
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
        },
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

        // Create a new "stack", by removing the elements we used and adding the result
        // then re-call the combine method recursivelly
        let mut subelements = elements.to_owned();
        
        if let Some(current) = results.get(&value.value) {
            if current.len() > value.len() {
                results.insert(value.value, value.clone());
            }
        } else {
            results.insert(value.value, value.clone());
        }

        subelements.push(value);
        remove_from_vec(&mut subelements, a);
        remove_from_vec(&mut subelements, b);

        if subelements.len() > 1 {
            combine(&subelements, results);
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

fn combine(elements: &[Number], results: &mut ResultSet) {
    let combinaisons = elements.iter().combinations(2).into_iter().map(to_tuple);

    for (a, b) in combinaisons {
        operate(Operation::Addition, a, b, elements, results);
        operate(Operation::Multiplication, a, b, elements, results);
        operate(Operation::Substraction, a, b, elements, results);
        operate(Operation::Substraction, b, a, elements, results);
        operate(Operation::Divison, a, b, elements, results);
        operate(Operation::Divison, b, a, elements, results);
    }
}

fn main() {

    //example();

    let todo = vec![5, 25, 2, 50, 100, 10];
    let todo: Vec<Number> = todo.iter().map(|&x| Number::from(x)).collect();

    let mut elements: Vec<Number> = Vec::new();
    let mut results: ResultSet = HashMap::new();

    elements.extend(todo.clone());

    let start = Instant::now();
    combine(&elements, &mut results);
    let end = Instant::now();
    
    println!("Base: {todo:?}");
    println!("Stack: {:?}", results.len());
    println!("Computed in {:?}", end - start);
    let find_me = 281;

    if let Some(found) = results.get_mut(&find_me) {
        println!("Found {} times, with len {}", found, found.len());
    }
}
