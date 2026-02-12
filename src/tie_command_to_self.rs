#![expect(
  unsafe_code,
  reason = "This module contains unsafe code by design, but it's properly encapsulated and justified. The unsafe blocks are necessary to interact with low-level OS APIs to ensure child processes are killed when the parent dies. The public API is safe to use."
)]
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

  // Windows: Assign Parent to a Job Object that kills children on close
  #[cfg(windows)]
  {
    use std::sync::Once;
    static INIT: Once = Once::new();

    // We only initialize the Job Object once. All children created
    // by this process afterwards will inherit this behavior.
    INIT.call_once(|| {
      unsafe {
        // Ignore errors: if we are already in a restrictive Job (e.g., some CI providers),
        // this might fail. We can't stop the program, so we just attempt best-effort.
        let _ = setup_job_object();
      }
    });

    // Suppress unused variable warning for Windows
    let _ = command;
  }

  Ok(())
}

#[cfg(windows)]
unsafe fn setup_job_object() -> Result<(), i32> {
  use std::{mem, ptr};
  use windows_sys::Win32::{
    Foundation::CloseHandle,
    System::{
      JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
      },
      Threading::GetCurrentProcess,
    },
  };

  // 1. Create a generic Job Object
  let job = CreateJobObjectW(ptr::null(), ptr::null());
  if job == std::ptr::null_mut() {
    return Err(0);
  }

  // 2. Configure the Job to kill all processes when the handle is closed
  let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = mem::zeroed();
  info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;

  let r = SetInformationJobObject(
    job,
    JobObjectExtendedLimitInformation,
    &info as *const _ as *const _,
    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
  );

  if r == 0 {
    CloseHandle(job);
    return Err(0);
  }

  // 3. Assign the current process (Parent) to this Job
  let r = AssignProcessToJobObject(job, GetCurrentProcess());
  if r == 0 {
    CloseHandle(job);
    return Err(0);
  }

  // 4. LEAK the handle.
  // We intentionally do NOT call CloseHandle(job).
  // The handle needs to stay open for the lifetime of the parent process.
  // When the parent process dies, the OS closes this handle, triggering the kill.
  Ok(())
}
