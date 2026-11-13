//! Own only this download's process tree, including Node and FFmpeg spawned by yt-dlp.
use anyhow::{Context, Result};
use std::process::{Child, Command};

pub(crate) struct DownloadProcess {
    pub child: Child,
    #[cfg(windows)]
    job: windows::Job,
}

impl DownloadProcess {
    pub fn spawn(command: &mut Command) -> Result<Self> {
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, CREATE_SUSPENDED};
            let job = windows::Job::new()?;
            // Attach before executing any downloader code, so no child can escape ownership.
            command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
            let mut child = command.spawn().context("Не удалось запустить загрузчик")?;
            if let Err(error) = job.attach_and_resume(&child) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
            Ok(Self { child, job })
        }
        #[cfg(not(windows))]
        Ok(Self {
            child: command.spawn().context("Не удалось запустить загрузчик")?,
        })
    }
    pub fn stop(&mut self) {
        #[cfg(windows)]
        self.job.terminate();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for DownloadProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(windows)]
mod windows {
    use anyhow::{Context, Result, bail};
    use std::{mem::size_of, os::windows::io::AsRawHandle, process::Child, ptr};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First,
                Thread32Next,
            },
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject, TerminateJobObject,
            },
            Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME},
        },
    };

    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    pub(super) struct Job(Handle);
    impl Job {
        pub fn new() -> Result<Self> {
            // Unnamed, non-inheritable handle; the OS closes it even if Shard crashes.
            let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
            if handle.is_null() {
                return Err(std::io::Error::last_os_error())
                    .context("Не удалось создать группу загрузки");
            }
            let job = Self(Handle(handle));
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if unsafe {
                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            } == 0
            {
                return Err(std::io::Error::last_os_error())
                    .context("Не удалось настроить завершение загрузчика");
            }
            Ok(job)
        }
        pub fn attach_and_resume(&self, child: &Child) -> Result<()> {
            if unsafe { AssignProcessToJobObject(self.0.0, child.as_raw_handle().cast()) } == 0 {
                return Err(std::io::Error::last_os_error())
                    .context("Не удалось привязать загрузчик к приложению");
            }
            let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
            if snapshot == INVALID_HANDLE_VALUE {
                return Err(std::io::Error::last_os_error())
                    .context("Не удалось найти поток загрузчика");
            }
            let snapshot = Handle(snapshot);
            let mut entry = THREADENTRY32 {
                dwSize: size_of::<THREADENTRY32>() as u32,
                ..Default::default()
            };
            let mut found = unsafe { Thread32First(snapshot.0, &mut entry) } != 0;
            while found {
                if entry.th32OwnerProcessID == child.id() {
                    let thread =
                        unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
                    if thread.is_null() {
                        return Err(std::io::Error::last_os_error())
                            .context("Не удалось открыть поток загрузчика");
                    }
                    let thread = Handle(thread);
                    if unsafe { ResumeThread(thread.0) } == u32::MAX {
                        return Err(std::io::Error::last_os_error())
                            .context("Не удалось запустить поток загрузчика");
                    }
                    return Ok(());
                }
                found = unsafe { Thread32Next(snapshot.0, &mut entry) } != 0;
            }
            bail!("Поток загрузчика не найден")
        }
        pub fn terminate(&self) {
            unsafe {
                TerminateJobObject(self.0.0, 1);
            }
        }
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::{
        fs,
        process::Stdio,
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };

    // Re-enter only this test in a child process to simulate a downloader and its runtime.
    #[test]
    #[allow(clippy::zombie_processes)] // Deliberately orphan a runtime; the download Job must terminate it.
    fn process_helper() {
        let Ok(role) = std::env::var("SHARD_PROCESS_TEST_ROLE") else {
            return;
        };
        if role == "runtime" {
            std::thread::sleep(Duration::from_secs(30));
            return;
        }
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "process_tree::tests::process_helper",
                "--nocapture",
            ])
            .env("SHARD_PROCESS_TEST_ROLE", "runtime")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        fs::write(
            std::env::var_os("SHARD_PROCESS_TEST_PID").unwrap(),
            child.id().to_string(),
        )
        .unwrap();
        if role == "wait" {
            let _ = child.wait();
        }
    }

    fn check_runtime_cleanup(parent_exits: bool, explicit_stop: bool) {
        let root = tempfile::tempdir().unwrap();
        let pid_file = root.path().join("runtime.pid");
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "process_tree::tests::process_helper",
                "--nocapture",
            ])
            .env(
                "SHARD_PROCESS_TEST_ROLE",
                if parent_exits { "exit" } else { "wait" },
            )
            .env("SHARD_PROCESS_TEST_PID", &pid_file)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let mut process = DownloadProcess::spawn(&mut command).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let pid = loop {
            if let Some(pid) = fs::read_to_string(&pid_file)
                .ok()
                .and_then(|s| s.parse::<u32>().ok())
            {
                break pid;
            }
            assert!(Instant::now() < deadline, "Runtime was not started");
            std::thread::sleep(Duration::from_millis(20));
        };
        let runtime = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        assert!(!runtime.is_null());
        assert_eq!(unsafe { WaitForSingleObject(runtime, 0) }, WAIT_TIMEOUT);
        if parent_exits {
            assert!(process.child.wait().unwrap().success());
        }
        if explicit_stop {
            process.stop();
        }
        drop(process);
        let status = unsafe { WaitForSingleObject(runtime, 3000) };
        unsafe {
            CloseHandle(runtime);
        }
        assert_eq!(status, WAIT_OBJECT_0, "Runtime survived its download");
    }

    #[test]
    fn runtime_is_closed_after_downloader_exits_successfully() {
        check_runtime_cleanup(true, false);
    }
    #[test]
    fn runtime_is_closed_when_download_is_cancelled() {
        check_runtime_cleanup(false, true);
    }
    #[test]
    fn runtime_is_closed_on_early_return() {
        check_runtime_cleanup(false, false);
    }
}
