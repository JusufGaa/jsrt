use jsrt::preprocessor::stage1::Stage1;
use jsrt::preprocessor::stage2::Stage2;
use jsrt::preprocessor::stage3::Stage3;
use jsrt::{init, worker, Worker};

fn main() {
    init!();
    let mut worker = worker!();

    let code = r#"
        let a = globalThis;
        const add = (...args) => {
            let result = args[0];
            for (let i = args.length - 1; i > 0 ; --i) {
                result += args[i];
            }
            return result;
        };

        function Add(a, b) {
            return add(a - 0, b - 0);
        }

        const f3 = (() => {
            let a = 3;
            return () => a
        })();

        () => {};
         
        a.foo = 1;
        a = {bar: 2};

        Add(foo, a.bar)
    "#;
    let stage1 = Stage1::try_from(code).unwrap();
    let stage2 = Stage2::from(stage1);
    println!("{:?}", stage2);
    let stage3 = Stage3::from(stage2);
    worker.load(stage3);
    let result = worker.run();
    println!("Add(1, 2) = {}", result);
}
