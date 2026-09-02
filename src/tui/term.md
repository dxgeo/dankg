# TUI term

Raw terminal mode, the alternate screen, and a size query. This is the
one corner of this crate that talks to the platform C library directly,
rather than through anything `std` wraps, because `std` has no termios
binding and [decision 1](../../architecture.md#decision-1-dependency-policy)
rules out reaching for the `libc` crate to get one. `std` already links
the system libc on macOS and Linux, so declaring the handful of
functions and structs this needs (`tcgetattr`/`tcsetattr`/`cfmakeraw`,
`ioctl` with `TIOCGWINSZ`) costs nothing in `Cargo.toml`.

```rust name=module_doc path=tui/term.rs
//! Raw terminal mode, the alternate screen, and a size query.
//!
//! `std` has no termios binding, and decision 1 rules out the `libc`
//! crate along with every other one, so this talks to the platform C
//! library directly: `tcgetattr`/`tcsetattr`/`cfmakeraw` for raw mode,
//! `ioctl` with `TIOCGWINSZ` for size. Both are declared, not linked
//! from a crate. `std` already pulls in the system libc on macOS and
//! Linux, so no `Cargo.toml` change is needed to call into it.
//!
//! `Termios`'s field layout is not portable: macOS/BSD and Linux glibc
//! disagree on field width and count, so the struct is `cfg`-gated per
//! platform. `Winsize` and the ioctl request number are also
//! platform-specific. Only macOS has been run against a real terminal
//! so far. The Linux path is written from the documented struct layout
//! and constant, but is unverified.

use std::io::{self, Write};
use std::mem::MaybeUninit;

const STDIN_FILENO: i32 = 0;
const TCSANOW: i32 = 0;
```

`Termios`'s own field widths and count disagree between macOS/BSD and
Linux glibc, so the struct itself (not just a constant inside it) is
`cfg`-gated per platform. Everything past this point is written
against whichever one compiled.

```rust name=platform path=tui/term.rs
#[cfg(target_os = "macos")]
mod platform {
    pub const NCCS: usize = 20;
    pub const TIOCGWINSZ: u64 = 0x4008_7468;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct Termios {
        pub c_iflag: u64,
        pub c_oflag: u64,
        pub c_cflag: u64,
        pub c_lflag: u64,
        pub c_cc: [u8; NCCS],
        pub c_ispeed: u64,
        pub c_ospeed: u64,
    }
}

#[cfg(target_os = "linux")]
mod platform {
    pub const NCCS: usize = 32;
    pub const TIOCGWINSZ: u64 = 0x5413;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct Termios {
        pub c_iflag: u32,
        pub c_oflag: u32,
        pub c_cflag: u32,
        pub c_lflag: u32,
        pub c_line: u8,
        pub c_cc: [u8; NCCS],
        pub c_ispeed: u32,
        pub c_ospeed: u32,
    }
}

use platform::{Termios, TIOCGWINSZ};

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Winsize {
    ws_row: u16,
    ws_col: u16,
    ws_xpixel: u16,
    ws_ypixel: u16,
}

unsafe extern "C" {
    fn tcgetattr(fd: i32, termios_p: *mut Termios) -> i32;
    fn tcsetattr(fd: i32, optional_actions: i32, termios_p: *const Termios) -> i32;
    fn cfmakeraw(termios_p: *mut Termios);
    fn ioctl(fd: i32, request: u64, argp: *mut Winsize) -> i32;
    fn isatty(fd: i32) -> i32;
}
```

```rust name=size_and_is_tty path=tui/term.rs
/// Terminal rows and columns, via `TIOCGWINSZ`. Fails with the OS
/// error (typically `ENOTTY`) when stdin is not a real terminal: a pipe
/// or the non-interactive shell this was developed under, for instance.
pub fn size() -> io::Result<(u16, u16)> {
    let mut ws = Winsize::default();
    let rc = unsafe { ioctl(STDIN_FILENO, TIOCGWINSZ, &mut ws) };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok((ws.ws_row, ws.ws_col))
}

/// True when stdin is an interactive terminal. The TUI has no business
/// starting otherwise.
pub fn is_tty() -> bool {
    unsafe { isatty(STDIN_FILENO) == 1 }
}
```

`RawMode` is the guard everything else in the TUI milestone is built
inside of. Nothing should touch stdin/stdout in raw mode without one
of these alive. Its `Drop` restores both raw mode and the normal
screen buffer even during a panic, since `Drop::drop` still runs while
unwinding.

```rust name=raw_mode path=tui/term.rs
/// Raw mode plus the alternate screen, restored on drop. This
/// includes on panic, since `Drop::drop` still runs during unwinding.
/// This is the guard everything else in the milestone is built inside
/// of: nothing should touch stdin/stdout in raw mode without one of
/// these alive.
pub struct RawMode {
    original: Termios,
}

impl RawMode {
    pub fn enter() -> io::Result<RawMode> {
        let mut original = MaybeUninit::<Termios>::uninit();
        if unsafe { tcgetattr(STDIN_FILENO, original.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let original = unsafe { original.assume_init() };

        let mut raw = original;
        unsafe { cfmakeraw(&mut raw) };
        if unsafe { tcsetattr(STDIN_FILENO, TCSANOW, &raw) } != 0 {
            return Err(io::Error::last_os_error());
        }

        // `?1049h` swaps to the alternate screen buffer, so nothing drawn
        // here scrolls into the reader's real history.
        print!("\x1b[?1049h");
        io::stdout().flush()?;

        Ok(RawMode { original })
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        // Best-effort: a guard's own drop cannot propagate an error, and a
        // failure here would otherwise mean the terminal is stuck raw with
        // no way to report it.
        print!("\x1b[?1049l");
        let _ = io::stdout().flush();
        unsafe { tcsetattr(STDIN_FILENO, TCSANOW, &self.original) };
    }
}
```
