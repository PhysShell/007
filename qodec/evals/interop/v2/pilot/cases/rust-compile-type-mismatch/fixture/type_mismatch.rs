// First-party pilot fixture (Scope N1). Real rustc diagnostics.
fn add_one(n: i64) -> i64 {
    n + 1
}

fn main() {
    let count: u32 = 10;
    let total = add_one(count);
    println!("{}", total);
}
