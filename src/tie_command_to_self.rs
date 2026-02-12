use std::process::Command;

pub fn tie_command_to_self(command: &mut Command) -> anyhow::Result<()> {
  // Unix: Request SIGTERM if parent dies
  #[cfg(unix)]
  #[expect(
    unsafe_code,
    reason = "This is necessary to ensure that the exiftool process doesn't become a zombie if the main process crashes. The unsafe block is required to call pre_exec, which is a safe API, but we need to call an unsafe function (libc::prctl) inside it."
  )]
  unsafe {
    use std::os::unix::process::CommandExt as _;

    command.pre_exec(|| {
      // PR_SET_PDEATHSIG = 1
      let r = libc::prctl(1, libc::SIGTERM);
      if r != 0 {
        return Err(std::io::Error::last_os_error());
      }
      Ok(())
    });
  }

  Ok(())
}
