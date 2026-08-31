//! Manual spike for `dankg::tui::term`. Not a test -- raw mode needs a real
//! terminal, and this sandbox's shell does not have one (`tty` reports "not
//! a tty"), so this has to be run and watched by a person.
//!
//! Run with `cargo run --example term_spike`. Expect: the current terminal
//! size printed, then the screen clears (alternate screen) and every key
//! you press is echoed as its raw bytes -- arrow keys should show up as
//! three-byte escape sequences (`1B 5B 41` for up, etc.), not as line-
//! buffered characters, and typed characters should not echo to the
//! screen on their own. Press `q` to quit; the screen should return to
//! exactly what was there before, proving the guard's restore-on-drop path
//! runs on a normal exit. Press Ctrl-C partway through instead to check the
//! same thing on a panic/interrupt path.

use dankg::tui::term;
use std::io::Read;

fn main() {
    if !term::is_tty() {
        eprintln!("term_spike: stdin is not a terminal; run this interactively, not piped.");
        std::process::exit(1);
    }

    match term::size() {
        Ok((rows, cols)) => println!("size: {rows} rows x {cols} cols"),
        Err(e) => {
            eprintln!("size() failed: {e}");
            std::process::exit(1);
        }
    }
    println!("entering raw mode + alternate screen; press keys, 'q' to quit");
    std::thread::sleep(std::time::Duration::from_millis(800));

    let _raw = match term::RawMode::enter() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("RawMode::enter() failed: {e}");
            std::process::exit(1);
        }
    };

    let mut stdin = std::io::stdin();
    let mut byte = [0u8; 1];
    loop {
        if stdin.read(&mut byte).unwrap_or(0) == 0 {
            break;
        }
        if byte[0] == b'q' {
            break;
        }
        print!("\r\nbyte: {:#04x}", byte[0]);
        use std::io::Write;
        let _ = std::io::stdout().flush();
    }
    // `_raw` drops here, restoring the terminal before main returns.
}
