use jsrt::{init, worker, Worker};

fn main() {
    init!();
    let mut worker = worker!();
    let result = worker.eval("'Hello' + ', ' + 'World!'");
    println!("{}", result);
}
