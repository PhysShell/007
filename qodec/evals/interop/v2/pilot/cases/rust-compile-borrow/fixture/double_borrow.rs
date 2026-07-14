// First-party pilot fixture (Scope N1). Real rustc diagnostics.
fn main() {
    let mut buffer = vec![1, 2, 3];
    let first = &mut buffer;
    let second = &mut buffer;
    first.push(4);
    second.push(5);
}
