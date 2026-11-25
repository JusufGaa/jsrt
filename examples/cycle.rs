use jsrt::{init, worker, Worker};

fn main() {
    init!();
    let mut worker = worker!();

    // Build a graph with a self-cycle (`a.self === a`), cross references
    // (`a.b === b`, `b.a === a`) and the same object referenced by
    // multiple fields (`list[0] === a`, `list[1] === b`).
    let result = worker.eval(
        r#"var a = { name: 'a' };
           var b = { name: 'b' };
           a.self = a;
           a.b = b;
           b.a = a;
           var list = [a, b];
           a.self.b.name"#,
    );
    println!("a.self.b.name = {}", result);
    let result = worker.eval("a.b.a === a && list[0] === a && list[1] === b");
    println!("identity intact = {}", result);
    let result = worker.eval("a.self.self.name");
    println!("a.self.self.name = {}", result);
    worker.save();

    // Drop everything, then bring the snapshot back.
    // worker.eval("a = null; b = null; list = null;");
    worker.restore();
    let result = worker.eval("a.name");
    println!("after restore a.name = {}", result);
    let result = worker.eval("b.name");
    println!("after restore b.name = {}", result);
    let result = worker.eval("a.self.self.name");
    println!("after restore a.self.self.name = {}", result);
    let result = worker.eval("a.b.a === a && list[0] === a && list[1] === b");
    println!("after restore identity intact = {}", result);
    let result = worker.eval("list[0] === a && list[1] === b");
    println!("after restore identity intact = {}", result);
    let result = worker.eval("list[1].a.b.name");
    println!("after restore list[1].a.b.name = {}", result);
}
