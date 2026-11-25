use jsrt::{init, worker, Worker};

fn main() {
    init!();
    let mut worker = worker!();
    let result = worker.eval("var a = 1; a + 1");
    println!("a + 1 = {}", result);
    let result = worker.eval("a");
    println!("a = {}", result);
    worker.save();
    let result = worker.eval("var b = 3; ({\"b\": b})");
    println!("Object {{b}} = {}", result);
    worker.restore();
    let a = worker.eval("a");
    let b = worker.eval("b");
    println!("after restore a = {}, b = {}", a, b);
}