use jsrt::preprocessor::stage1::Stage1;
use jsrt::{init, worker, Worker};

fn main() {
    init!();
    let mut worker = worker!();

    let code = r#"
        const add = (...args) => {
            let result = args[0];
            for (let i = 1; i < args.length; i++) {
                result += args[i];
            }
            return result;
        };

        function Add(a, b) {
            return add(a - 0, b - 0);
        }

        const f3 = (() => {
            let a = 3;
            () => a
        })();

        () => {};

        Add(1, 2)
    "#;
    let stage1 = Stage1::try_from(code).unwrap();
    println!("{}", stage1.debug());
    worker.load(stage1);
    let result = worker.run();
    println!("Add(1, 2) = {}", result);
}
