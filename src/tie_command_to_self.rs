use std::process::Command;

/// In contrast to windows, Unix doesn't automatically kill child processes when the parent process dies.
/// To ensure that child processes don't become zombies if the main process crashes,
/// prctl is used to request that the child process receives a SIGTERM signal when the parent process dies.
pub fn tie_command_to_self(command: &mut Command) {
  // Unix: Request SIGTERM if parent dies
  #[cfg(unix)]
  #[expect(
    unsafe_code,
    reason = "This is necessary to ensure that the exiftool process doesn't become a zombie if the main process crashes. The unsafe block is required to call pre_exec, which is a safe API, but we need to call an unsafe function (libc::prctl) inside it."
  )]
  unsafe {
    use std::os::unix::process::CommandExt as _;

    command.pre_exec(|| {
      let r = libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
      if r < 0 {
        return Err(std::io::Error::last_os_error());
      }
      Ok(())
    });
  }
}
