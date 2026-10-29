//! Crates of crates.io on 9front: regex, serde with derive, serde_json.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct Point {
    x: i32,
    name: String,
}

fn main() {
    let re = regex::Regex::new(r"(\w+)!(\d+)").unwrap();
    let c = re.captures("tcp!564").unwrap();
    assert_eq!((&c[1], &c[2]), ("tcp", "564"));
    let p = Point { x: 9, name: "front".into() };
    let s = serde_json::to_string(&p).unwrap();
    assert_eq!(serde_json::from_str::<Point>(&s).unwrap(), p);
    println!("crates: {s}");
    println!("crates: ok");
}
