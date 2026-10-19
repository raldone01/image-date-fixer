use std::process::Command;

/// Linux only: asks the kernel to send `SIGTERM` to the child when the *thread*
/// that spawned it exits.
///
/// # Caveats
///
/// - `PR_SET_PDEATHSIG` is tied to the spawning **thread**, not the process.
///   Call `spawn` from a thread that lives as long as the spawned command is needed.
/// - Unix does not kill children when the parent dies; they are reparented
///   (to init or a subreaper) and keep running. This function prevents that
///   leak on Linux.
/// - On other Unixes (macOS, BSDs) and on Windows this function does nothing.
///   Use crates such as [`process-wrap`](https://crates.io/crates/process-wrap).
/// - The setting is not inherited by grandchildren, is cleared when the child
///   execs a setuid/setgid or file-capability binary, and `SIGTERM` can be
///   caught or ignored by the child.
/// - If the spawning *thread* has already exited by the time the child arms
///   `PR_SET_PDEATHSIG` (the kernel would never deliver the signal), spawning
///   fails with `ECANCELED` ("Operation canceled").
pub fn tie_command_to_thread(command: &mut Command) {
  #[cfg(target_os = "linux")]
  {
    use std::os::unix::process::CommandExt as _;

    // Captured in the spawning thread, before the fork. Inside `pre_exec` we
    // are in the child, where `gettid()` would return the child's tid. These
    // are plain integers, so the closure below stays allocation-free and
    // captures no shared state.
    let parent_tgid = nix::unistd::getpid().as_raw();
    let parent_tid = nix::unistd::gettid().as_raw();

    let pre_exec = move || {
      // Tie the child to the parent: request SIGTERM when the parent thread dies.
      nix::sys::prctl::set_pdeathsig(nix::sys::signal::SIGTERM).map_err(std::io::Error::from)?;

      // The spawning *thread* may have exited between fork and prctl, in which
      // case the kernel never queued our SIGTERM. Signal 0 delivers nothing and
      // only checks that the exact thread (tgid + tid) still exists; it fails
      // with ESRCH if the thread is gone. We report `ECANCELED` instead so the
      // error cannot be mistaken for a missing executable.
      #[expect(unsafe_code)]
      // SAFETY: thin wrapper over the `tgkill` syscall with plain integer
      // arguments; async-signal-safe.
      let rc = unsafe { nix::libc::tgkill(parent_tgid, parent_tid, 0) };
      if rc == -1 {
        return Err(std::io::Error::from_raw_os_error(nix::libc::ECANCELED));
      }
      Ok(())
    };

    #[expect(unsafe_code)]
    // SAFETY: `CommandExt::pre_exec` requires the closure to be sound to run in
    // the forked child, i.e. after `fork()`, before `exec()`. At that point the
    // process may have been forked from a multi-threaded program, so any state
    // guarded by a mutex/allocation in another thread is frozen in an
    // arbitrary state. The closure must therefore be async-signal-safe: no
    // allocations, no locks, no calls into logic that may touch inconsistent
    // global state.
    //
    // The closure installed here satisfies that requirement:
    //   * it captures only two `Copy` integers (tgid and tid), with no shared
    //     state, no heap references and no locks;
    //   * it performs only async-signal-safe raw syscalls (`prctl`, `tgkill`);
    //   * on failure it converts `Errno` into `io::Error`, which for OS errors
    //     stores the errno inline and does not allocate;
    //   * it cannot panic: the error paths return `Err` rather than unwinding.
    //
    // Since the closure meets `pre_exec`'s invariants, calling
    // `Command::pre_exec` on it is sound.
    unsafe {
      command.pre_exec(pre_exec);
    }
  }

  #[cfg(not(target_os = "linux"))]
  let _ = command;
}
