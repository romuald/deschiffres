use std::hash::{Hash, Hasher};
use std::convert::From;
use std::collections::HashSet;
use itertools::Itertools;


enum Operation {
    Addition,
    Multiplication,
    Substraction,
    Divison,
}


struct Number<'a> {
    value: i32,
    parent: Option<(Operation, &'a Number<'a>, &'a Number<'a>)>,
}

fn example() {
    let a = Number::from(5);
    let b = Number::from(1);
    let c = Number::from(10);

    let d = Number { value: a.value+b.value, parent: Some((Operation::Addition, &a, &b))};
    let e = Number { value: d.value+c.value, parent: Some((Operation::Addition, &d, &c))};

    println!("e? {e}")
}

impl std::fmt::Display for Number<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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

impl Hash for Number<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl From<i32> for Number<'_> {
    fn from(item: i32) -> Self {
        Number { value: item, parent: None }
    }
}

fn operate(operation: Operation, a: i32, b: i32, elements: &HashSet<i32>, results: &mut HashSet<i32>) {
    let value = match operation {
        Operation::Addition => Some(a + b),
        Operation::Multiplication => Some(a * b),
        Operation::Substraction => {
            if a - b > 0 {
                Some(a - b)
            } else {
                None
            }
        },
        Operation::Divison => {
            if a % b == 0 {
                Some(a / b)
            } else {
                None
            }
        }
    };

    if let Some(value) = value {
        // Create a new "stack", by removing the elements we used and adding the result
        // then re-call the combine method recursivelly
        let mut subelements = elements.clone();
        
        results.insert(value);

        subelements.insert(value);
        subelements.remove(&a);
        subelements.remove(&b);

        combine(&subelements, results);
    }
}

fn to_tuple(x: Vec<i32>) -> (i32, i32) {
    if let [a, b] = x[0..2] {
        (a, b)
    } else {
        /* Unreachable */
        (0, 0)
    }
}

fn combine(elements: &HashSet<i32>, results: &mut HashSet<i32>) {
    let combinaisons = elements.clone().into_iter().combinations(2).into_iter().map(to_tuple);

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

    example();

    let todo = vec![5, 25, 2];
    let mut elements: HashSet<i32> = HashSet::new();
    let mut results: HashSet<i32> = HashSet::new();
    elements.extend(todo.iter());
    combine(&elements, &mut results);

    println!("Base: {todo:?}");
    println!("Stack: {results:?}");

}
