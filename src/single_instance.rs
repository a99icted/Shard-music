//! Per-session Windows instance guard, acquired before opening SQLite or starting audio.
use anyhow::{Context, Result, bail};
use std::{
    cell::Cell,
    ptr,
    time::{Duration, Instant},
};
use windows_sys::{
    Win32::{
        Foundation::{
            CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, LPARAM, LRESULT, WPARAM,
        },
        System::{LibraryLoader::GetModuleHandleW, Threading::CreateMutexW},
        UI::WindowsAndMessaging::*,
    },
    core::w,
};

const ACTIVATE: u32 = WM_APP + 83;
const MUTEX: windows_sys::core::PCWSTR = w!("Local\\Shard-5CBCE71D-68EF-4E41-9565-401586E1F370");
const CLASS: windows_sys::core::PCWSTR = w!("ShardSingleInstance-5CBCE71D");

struct State {
    main: Cell<HWND>,
    requested: Cell<bool>,
}
pub struct Instance {
    mutex: HANDLE,
    window: HWND,
    state: Box<State>,
}

impl Instance {
    pub fn acquire() -> Result<Option<Self>> {
        Self::acquire_named(MUTEX, CLASS)
    }

    fn acquire_named(
        name: windows_sys::core::PCWSTR,
        class: windows_sys::core::PCWSTR,
    ) -> Result<Option<Self>> {
        unsafe {
            let mutex = CreateMutexW(ptr::null(), 0, name);
            if mutex.is_null() {
                return Err(std::io::Error::last_os_error())
                    .context("Не удалось проверить запуск Shard");
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(mutex);
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    let existing = FindWindowW(class, ptr::null());
                    if !existing.is_null() {
                        let mut pid = 0;
                        GetWindowThreadProcessId(existing, &mut pid);
                        AllowSetForegroundWindow(pid);
                        if PostMessageW(existing, ACTIVATE, 0, 0) != 0 {
                            return Ok(None);
                        }
                    }
                    if Instant::now() >= deadline {
                        bail!(
                            "Shard уже запущен, но его окно пока не отвечает. Дождитесь запуска или завершите прежний Shard через трей."
                        );
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
            }
            let state = Box::new(State {
                main: Cell::new(ptr::null_mut()),
                requested: Cell::new(false),
            });
            let module = GetModuleHandleW(ptr::null());
            let window_class = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: module,
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassW(&window_class);
            let window = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                class,
                w!("Shard instance"),
                0,
                0,
                0,
                0,
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                module,
                ptr::null(),
            );
            if window.is_null() {
                let error = std::io::Error::last_os_error();
                CloseHandle(mutex);
                return Err(error).context("Не удалось создать канал активации Shard");
            }
            SetWindowLongPtrW(window, GWLP_USERDATA, (&*state as *const State) as isize);
            Ok(Some(Self {
                mutex,
                window,
                state,
            }))
        }
    }

    pub fn bind(&self, main: HWND) {
        self.state.main.set(main);
        if self.state.requested.replace(false) {
            self.state.show();
        }
    }
}
impl State {
    fn show(&self) {
        let main = self.main.get();
        if main.is_null() {
            self.requested.set(true);
            return;
        }
        unsafe {
            ShowWindow(
                main,
                if IsIconic(main) != 0 {
                    SW_RESTORE
                } else {
                    SW_SHOW
                },
            );
            SetForegroundWindow(main);
        }
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            SetWindowLongPtrW(self.window, GWLP_USERDATA, 0);
            DestroyWindow(self.window);
            CloseHandle(self.mutex);
        }
    }
}
unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if message == ACTIVATE {
            let state = GetWindowLongPtrW(window, GWLP_USERDATA) as *const State;
            if let Some(state) = state.as_ref() {
                state.show();
            }
            return 0;
        }
        DefWindowProcW(window, message, wparam, lparam)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_is_rejected_and_closing_owner_releases_guard() -> Result<()> {
        let suffix = uuid::Uuid::new_v4();
        let name: Vec<u16> = format!("Local\\Shard-test-{suffix}\0")
            .encode_utf16()
            .collect();
        let class: Vec<u16> = format!("Shard-test-{suffix}\0").encode_utf16().collect();
        let owner = Instance::acquire_named(name.as_ptr(), class.as_ptr())?.unwrap();
        assert!(Instance::acquire_named(name.as_ptr(), class.as_ptr())?.is_none());
        let mut message = MSG::default();
        unsafe {
            while PeekMessageW(&mut message, owner.window, 0, 0, PM_REMOVE) != 0 {
                DispatchMessageW(&message);
            }
        }
        assert!(owner.state.requested.get());
        // A queued activation received while initializing is remembered until the main window exists.
        drop(owner);
        assert!(Instance::acquire_named(name.as_ptr(), class.as_ptr())?.is_some());
        Ok(())
    }

    #[test]
    fn child_probe() {
        let Ok(suffix) = std::env::var("SHARD_INSTANCE_TEST_SUFFIX") else {
            return;
        };
        let name: Vec<u16> = format!("Local\\Shard-process-test-{suffix}\0")
            .encode_utf16()
            .collect();
        let class: Vec<u16> = format!("Shard-process-test-{suffix}\0")
            .encode_utf16()
            .collect();
        assert!(
            Instance::acquire_named(name.as_ptr(), class.as_ptr())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn five_parallel_launches_activate_the_owner_without_creating_another_instance() -> Result<()> {
        use std::process::{Command, Stdio};
        let suffix = uuid::Uuid::new_v4().to_string();
        let name: Vec<u16> = format!("Local\\Shard-process-test-{suffix}\0")
            .encode_utf16()
            .collect();
        let class: Vec<u16> = format!("Shard-process-test-{suffix}\0")
            .encode_utf16()
            .collect();
        let owner = Instance::acquire_named(name.as_ptr(), class.as_ptr())?.unwrap();
        let mut children = Vec::new();
        for _ in 0..5 {
            children.push(
                Command::new(std::env::current_exe()?)
                    .args(["--exact", "single_instance::tests::child_probe"])
                    .env("SHARD_INSTANCE_TEST_SUFFIX", &suffix)
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()?,
            );
        }
        let results: Vec<_> = children.iter_mut().map(|child| child.wait()).collect();
        for result in results {
            assert!(
                result?.success(),
                "Duplicate process did not hand off activation"
            );
        }
        let mut message = MSG::default();
        let mut activations = 0;
        unsafe {
            while PeekMessageW(&mut message, owner.window, 0, 0, PM_REMOVE) != 0 {
                if message.message == ACTIVATE {
                    activations += 1;
                }
                DispatchMessageW(&message);
            }
        }
        assert_eq!(activations, 5);
        assert!(owner.state.requested.get());
        Ok(())
    }
}
