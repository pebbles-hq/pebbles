//! `render-check` binary: argument handling, `--list`, exit codes (§6.4).

use std::process::ExitCode;

use render_check::{args, case::Case, corpus};

fn expect_label(c: &Case) -> String {
    match c.expect {
        render_check::case::Expect::Pass => "pass".to_string(),
        render_check::case::Expect::Xfail { until, .. } => format!("xfail({until})"),
    }
}

fn main() -> ExitCode {
    let args = match args::parse(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("render-check: {e}\n{}", args::USAGE);
            return ExitCode::from(2);
        }
    };
    if args.help {
        println!("{}", args::USAGE);
        return ExitCode::SUCCESS;
    }
    if args.list {
        for c in corpus::all().iter().filter(|c| args.selects(&c.name)) {
            let (w, h) = c.logical;
            println!(
                "{}\t{:?}\t{:?}\t{w}x{h}@{}\t{}\t{}",
                c.name,
                c.category,
                c.slice,
                c.scale,
                expect_label(c),
                c.in_refs
            );
        }
        return ExitCode::SUCCESS;
    }
    eprintln!("render-check: renderers not wired yet (V13/V14)");
    ExitCode::from(2)
}
